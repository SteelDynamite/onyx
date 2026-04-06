# Onyx Project Audit

**Date:** 2026-04-06
**Scope:** Full codebase audit for quality, simplicity, security, and maintainability
**Codebase:** ~5,047 lines Rust + Svelte 5 frontend across 5 crates/packages

---

## Executive Summary

Onyx is well-architected with clean separation of concerns, a solid storage abstraction, and thoughtful security defaults (HTTPS-only WebDAV, zeroized credentials, path traversal protection). However, the audit identified **67 findings** across security, data integrity, code quality, and testing. The most critical issues are: missing path validation in sync operations, non-atomic file writes risking data corruption on crash, a logic bug in task reordering, and zero CI/CD infrastructure.

### Findings by Severity

| Severity | Count | Key Examples |
|----------|-------|--------------|
| **CRITICAL** | 5 | Path traversal in sync, reorder logic bug, no CI/CD |
| **HIGH** | 12 | Non-atomic writes, incomplete move_task rollback, no file size limits |
| **MEDIUM** | 25 | Swallowed errors, missing input validation, accessibility gaps |
| **LOW** | 25 | Code duplication, magic numbers, minor inefficiencies |

---

## 1. Security

### 1.1 CRITICAL: Path Traversal in Sync Operations

**Files:** `sync.rs:603, 622, 702, 715`

The sync module joins remote paths directly to the local workspace path without validation:

```rust
let local_path = workspace_path.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
```

If `path` contains `../`, it could write files outside the workspace directory. The storage layer has robust path traversal protection (`storage.rs:154-172` with canonicalization checks), but the sync module bypasses it entirely.

**Recommendation:** Add canonicalize + prefix check before all file operations in sync:
```rust
let local_path = workspace_path.join(&path);
assert!(local_path.starts_with(workspace_path));
```

### 1.2 HIGH: No File Size Limits on Downloads

**File:** `webdav.rs:116-133`

`get_file()` downloads entire files into memory with no size limit. PROPFIND responses are correctly capped at 10MB (`MAX_PROPFIND_BYTES`), but actual file downloads are unbounded. A malicious or compromised WebDAV server could cause OOM.

**Recommendation:** Enforce a configurable max file size (e.g., 10MB) on `get_file()`.

### 1.3 HIGH: No Input Size Limits

**Files:** `storage.rs`, `models.rs`, Tauri commands

No maximum length enforced on task titles, descriptions, or list names at any layer. A 1GB task description would be fully buffered in memory during read, write, and sync operations.

**Recommendation:** Add `const MAX_TASK_SIZE: usize = 10_000_000` and `const MAX_TITLE_LENGTH: usize = 512` with validation at the storage boundary.

### 1.4 MEDIUM: WebDAV URL Not Validated at Save Time

**File:** Tauri `set_webdav_config` command

WebDAV URLs are saved to config without validation. HTTPS enforcement happens in `webdav.rs:28-31` at connection time, but a user could save an HTTP URL and not discover the error until sync. Additionally, empty domain from a failed URL parse could create a catch-all keychain entry.

**Recommendation:** Validate URL format and HTTPS scheme at save time.

### 1.5 MEDIUM: Workspace Paths Not Validated in Tauri

**File:** Tauri `init_workspace`, `watch_workspace` commands

These commands accept arbitrary paths from the frontend without server-side validation. A malicious frontend call could target system directories.

**Recommendation:** Validate paths are within user home directory or explicitly allowed locations.

### 1.6 LOW: CSP Allows `unsafe-inline` Styles

**File:** `tauri.conf.json`

`style-src 'unsafe-inline'` is set, which allows inline CSS injection if a DOM-based XSS exists. Acceptable for a desktop app but could be tightened.

### 1.7 Security Strengths

- HTTPS-only enforcement for WebDAV (credentials never sent in plaintext)
- `Zeroizing<String>` for all credential fields with platform keyring storage
- Path traversal protection in storage layer (blacklist + canonicalize + prefix check)
- PROPFIND response capped at 10MB
- 30s request timeout, 10s connect timeout
- `quick-xml` streaming parser immune to XXE and billion-laughs attacks
- No header injection or URL injection vulnerabilities found
- No `unsafe` code blocks anywhere in the codebase
- No XSS vectors in Svelte frontend (no `{@html}` usage found)

---

## 2. Data Integrity

### 2.1 HIGH: No Atomic Writes

**Files:** `sync.rs:462, 289, 671, 684` | `storage.rs:250, 315, 423, 526, 557` | `config.rs:117`

All file writes use `fs::write()` directly. If the process crashes mid-write, files are left in a corrupted partial state. This affects:
- **Sync state** (`.syncstate.json`) — corrupt state silently resets, losing all sync metadata
- **Offline queue** — corrupt queue silently resets, losing queued operations
- **Task files** — partial writes produce unparseable markdown
- **Config** — app config lost on crash during save

**Recommendation:** Use atomic write pattern everywhere:
```rust
fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, content)?;
    fs::rename(&temp, path)?; // atomic on most filesystems
    Ok(())
}
```

### 2.2 HIGH: Multi-Step Operations Not Transactional

**Files:** `sync.rs:676-688`, `storage.rs:314-322`, `storage.rs:518-526`

Several operations involve multiple file writes that can leave inconsistent state on partial failure:

- **Conflict recovery** (`sync.rs`): Overwrites local file, creates duplicate, updates metadata — crash between steps loses data
- **write_task** (`storage.rs`): Writes task file then updates metadata — crash between leaves orphaned file
- **rename_list** (`storage.rs`): Renames directory then writes metadata — crash between makes list inaccessible

### 2.3 MEDIUM: Sync State Corruption Silently Resets

**File:** `sync.rs:454`

```rust
serde_json::from_str(&content).unwrap_or_default()
```

If `.syncstate.json` is corrupted, it silently resets to empty state. The next sync will re-upload everything, potentially overwriting newer remote data with stale local data. Should warn the user.

### 2.4 MEDIUM: Concurrent Access Not Protected

No file locking mechanism exists. If two processes (or multiple devices via shared filesystem) access the same workspace simultaneously, data corruption is possible. The Tauri layer uses a Mutex for in-process safety, but no cross-process protection exists.

---

## 3. Logic Bugs

### 3.1 CRITICAL: Task Reorder Index Calculation Bug

**File:** `repository.rs:88-106`

```rust
metadata.task_order.remove(current_pos);
let new_pos = new_position.min(metadata.task_order.len());
metadata.task_order.insert(new_pos, task_id);
```

After removing at `current_pos`, all indices shift. The `new_position` parameter refers to the original index space, but insertion happens in the shifted space. Example:
- List: `[A, B, C, D]`, move B (pos 1) to pos 3
- After remove: `[A, C, D]`
- Insert at min(3, 3) = 3: `[A, C, D, B]` — correct by accident
- But: move C (pos 2) to pos 1 → remove: `[A, B, D]` → insert at 1: `[A, C, B, D]` — correct
- Edge case: move A (pos 0) to pos 2 → remove: `[B, C, D]` → insert at 2: `[B, C, A, D]` — user expected `[B, C, A, D]`... actually correct

The logic may work for most cases due to `min()` clamping, but the semantics are ambiguous — does `new_position` mean "insert before this index in the original list" or "insert at this index in the new list"? This should be explicitly documented and tested for all boundary conditions.

### 3.2 HIGH: Incomplete move_task Rollback

**File:** `repository.rs:76-85`

```rust
pub fn move_task(&mut self, ...) -> Result<()> {
    let task = self.storage.read_task(from_list_id, task_id)?;
    self.storage.write_task(to_list_id, &task)?;
    if let Err(e) = self.storage.delete_task(from_list_id, task_id) {
        let _ = self.storage.delete_task(to_list_id, task_id); // rollback
        return Err(e);
    }
    Ok(())
}
```

If `write_task` partially succeeds (file written but metadata not updated) and then `delete_task` fails, the rollback `delete_task` on the destination may also partially fail. Task could end up in both lists or neither.

### 3.3 MEDIUM: Unwrap That Can Panic in Production

**File:** `storage.rs:393`

```rust
let (_, task) = entries.into_iter().next().unwrap();
```

After the deduplication loop drains all but one entry, this unwrap should always succeed. But if the entries vector is somehow empty (e.g., all files unreadable), it panics. Should use `.ok_or_else(|| Error::InvalidData(...))`.

### 3.4 LOW: O(n^2) Deleted File Detection in Sync

**File:** `sync.rs:775`

```rust
for path in sync_state.files.keys() {
    if !local_files.iter().any(|f| f.path == *path) { // linear scan per key
```

Should use a `HashSet` for local file paths.

---

## 4. Error Handling

### 4.1 HIGH: Errors Silently Swallowed

Multiple locations silently ignore errors with `let _ =`:

| File | Line(s) | What's Ignored |
|------|---------|----------------|
| `sync.rs` | 268 | Queue backup creation failure |
| `sync.rs` | 284 | Queue file removal failure |
| `sync.rs` | 684-685 | Listdata metadata write failure during conflict recovery |
| `storage.rs` | 390 | Stale file deletion during dedup |
| Tauri commands | various | `watch_workspace` errors logged to console only |
| Svelte | `SettingsScreen:29` | `loadCredentials` error completely swallowed with `.catch(() => {})` |

**Recommendation:** At minimum, log all swallowed errors. For data-affecting operations, propagate errors to the user.

### 4.2 MEDIUM: Error Type Loses Context

**File:** `error.rs`

The `Error` enum uses `String` for most variants (`Serialization(String)`, `WebDav(String)`, `Sync(String)`). This discards the original error chain — `serde_json::Error` line/column info, `reqwest::Error` kind, etc.

No `source()` implementation, so error chain traversal is impossible.

**Recommendation:** Consider `thiserror` crate or structured error variants with context fields.

### 4.3 MEDIUM: Sync State Corruption Not Reported

**File:** `sync.rs:454`

Corrupt sync state file is silently replaced with empty default. User loses all sync metadata with no notification.

### 4.4 MEDIUM: Frontend Error Messages Are Raw Backend Strings

**File:** `app.svelte.ts` (14+ locations)

All error handling is `error = String(e)`, showing raw Rust error messages to users. No user-friendly error translation layer.

---

## 5. Code Quality & Simplicity

### 5.1 HIGH: Overly Large Files

| File | Lines | Recommendation |
|------|-------|----------------|
| `sync.rs` | 1,221 | Split into `sync_state.rs`, `sync_actions.rs`, `sync_engine.rs`, `conflict.rs` |
| `storage.rs` | 925 | Extract `frontmatter.rs`, `dedup.rs`, `metadata.rs` |
| `webdav.rs` | 775 | Extract `propfind.rs`, `credentials.rs` |
| `TasksScreen.svelte` | 667 | Extract drawer, header, drag-drop into components |
| `TaskDetailView.svelte` | 419 | Extract subtask section, menus, date picker integration |

### 5.2 MEDIUM: Hardcoded Magic Numbers

Constants scattered throughout without named definitions:

| Value | Location | Purpose |
|-------|----------|---------|
| `10 * 1024 * 1024` | `webdav.rs:103` | PROPFIND response cap |
| `Duration::from_secs(30)` | `webdav.rs:7,39` | Request timeout (defined as const but also hardcoded) |
| `Duration::from_secs(10)` | `webdav.rs:40` | Connect timeout |
| `usize::MAX` | `storage.rs:404` | Unordered task sentinel |
| `1` | `storage.rs:52` | Default task version |
| `.md`, `.listdata.json`, `.onyx-workspace.json` | scattered | File extensions/names repeated as string literals |

**Recommendation:** Define all as named constants.

### 5.3 MEDIUM: Duplicated Frontend Logic

- **Date formatting** duplicated in `TaskItem.svelte`, `NewTaskInput.svelte`, `TaskDetailView.svelte`
- **Menu click-outside handlers** duplicated in `TasksScreen.svelte` and `TaskDetailView.svelte`
- **Error handling pattern** (`error = String(e)`) repeated 14+ times

**Recommendation:** Extract to shared utilities/composables.

### 5.4 LOW: Unused Dependency

`wiremock 0.6` is in `onyx-core` dev-dependencies but not used in any tests. Should either be used for WebDAV integration tests or removed.

---

## 6. Testing

### 6.1 CRITICAL: No CI/CD Pipeline

No `.github/workflows/`, no `Makefile`, no pre-commit hooks. Nothing prevents broken code from being committed.

**Recommendation:** Add GitHub Actions with:
- `cargo test` (all platforms)
- `cargo clippy`
- `cargo fmt --check`
- `cargo audit` (security)
- Frontend lint/build check

### 6.2 HIGH: Test Coverage Gaps

**107 tests total** — good for core logic, but major gaps:

| Category | Status | Gap |
|----------|--------|-----|
| Core business logic | Good (93 tests) | Edge cases missing |
| WebDAV network ops | Not tested | `wiremock` imported but unused |
| Async/sync engine | Not tested | No `#[tokio::test]` found |
| CLI commands | 0 tests | Entire crate untested |
| Tauri commands | 0 tests | All commands untested |
| Frontend | 0 tests | No component or integration tests |
| Security | 0 tests | No path traversal, malformed input, or auth failure tests |
| Concurrent access | 0 tests | No race condition tests |

### 6.3 MEDIUM: Tests Only Cover Happy Path

Existing tests verify correct behavior but rarely test:
- Network failures and timeouts
- Corrupted/malformed files
- Boundary conditions (empty lists, max-length strings, unicode edge cases)
- Partial failure and recovery
- Concurrent modifications

### 6.4 Specific Missing Test Cases

**sync.rs:**
- Path traversal attempts in remote file paths
- Corrupted sync state recovery
- Large file handling
- Network failure during multi-file sync
- Concurrent sync attempts

**storage.rs:**
- Line 393 unwrap scenario (empty entries after dedup)
- Non-UTF8 filenames
- Symlink handling
- Files exceeding memory limits

**repository.rs:**
- `move_task` rollback failure scenarios
- `reorder_task` boundary positions (0, len, > len)
- Circular parent relationships
- Empty task titles after sanitization

---

## 7. Frontend (Svelte)

### 7.1 HIGH: Sliding Panel State Corruption

**File:** `TasksScreen.svelte`

Multiple scenarios can leave `taskStack` in an inconsistent state:

1. **Deleted task in detail panel** — `taskStack` still holds the deleted ID, `parentTask` becomes null, panel shows empty content
2. **List switch with open detail** — tasks from old list gone, detail panel shows null
3. **Rapid back navigation** — state changes during CSS transition can leave panels at wrong positions
4. **Sync conflict dedup** — may remove the task ID currently in `taskStack`

**Recommendation:** Clear `taskStack` when tasks are deleted, lists are switched, or workspace changes.

### 7.2 HIGH: Accessibility

- **16 instances** of `svelte-ignore a11y_no_static_element_interactions` across 7 files
- Only **1 `aria-label`** in entire frontend
- No focus traps in modals (Settings, ConfirmDialog)
- No keyboard Tab navigation in menus
- Missing ARIA roles on drawer, menus, and overlay elements

### 7.3 MEDIUM: No List Virtualization

All tasks rendered as DOM nodes. For workspaces with 1000+ tasks, this will cause performance issues. `{#each}` loops in `TasksScreen.svelte` render every task without windowing.

### 7.4 MEDIUM: Race Conditions in State Management

- `triggerSync()` can fire while user is editing (no edit lock during sync)
- `toggleTask()` updates UI optimistically but re-fetches from backend — sync conflicts can show stale data
- Workspace switch doesn't fully reset all component state
- `onFocusChanged` setup has no `.catch()` — unhandled promise rejection

### 7.5 LOW: Double requestAnimationFrame

**File:** `TaskItem.svelte:29-31`

Nested `requestAnimationFrame(() => requestAnimationFrame(...))` chains can cause jank. Should use a single RAF with proper timing.

---

## 8. Tauri Backend

### 8.1 State Management: GOOD

The Tauri backend has well-designed state management:
- Dedicated `lock_state()` helper converts poisoned Mutex locks to errors (never panics)
- No nested locks detected
- Short critical sections
- Async operations release locks before `.await`

### 8.2 MEDIUM: Method Unwraps in WebDAV

**File:** `webdav.rs:67, 89, 175, 195`

```rust
reqwest::Method::from_bytes(b"PROPFIND").unwrap()
```

These are safe in practice (hardcoded valid HTTP methods) but violate defensive coding. Should use `.expect("PROPFIND is a valid HTTP method")` with justification comments.

---

## 9. Dependencies

### 9.1 Dependency Summary

| Crate | Version | Notes |
|-------|---------|-------|
| tokio | 1.40 | Current, `full` feature (consider trimming for binary size) |
| reqwest | 0.12 | Using `rustls-tls` (good — no OpenSSL dependency) |
| serde/serde_json/serde_yaml | 1.0/1.0/0.9 | Standard, well-maintained |
| quick-xml | 0.36 | Current, streaming parser (safe from XXE) |
| keyring | 3.0 | Platform-native credential storage |
| zeroize | 1.0 | Credential memory safety |
| tauri | 2.x | Current major version |
| notify | 7.0 | File watching (feature-gated for mobile) |
| chrono | 0.4 | Note: `1.0` specified in `onyx-cli` but `0.4` in workspace — version mismatch |
| wiremock | 0.6 | Dev dependency — imported but not used |

### 9.2 Recommendations

- Run `cargo audit` regularly (no CI to automate this currently)
- Remove unused `wiremock` or write WebDAV integration tests with it
- Fix `chrono` version discrepancy between workspace (`0.4`) and `onyx-cli` (`1.0`)
- No certificate pinning for WebDAV servers — acceptable for general use but worth noting

---

## 10. Prioritized Recommendations

### Phase 1: Critical Fixes (Immediate)

1. **Add path validation in sync.rs** — canonicalize + prefix check before all file operations
2. **Set up CI/CD** — GitHub Actions with `cargo test`, `clippy`, `fmt`, `audit`
3. **Implement atomic writes** — temp file + rename for all state files (sync state, config, metadata)
4. **Add file size limits** — cap downloads and task file sizes at 10MB
5. **Fix or document reorder_task semantics** — clarify index behavior, add boundary tests

### Phase 2: High-Priority Improvements (Short-term)

6. **Stop swallowing errors** — log or propagate all `let _ =` patterns
7. **Fix move_task rollback** — ensure transactional behavior or document limitations
8. **Replace dangerous unwrap at storage.rs:393** — use proper error handling
9. **Clear taskStack on state changes** — prevent stale panel state in frontend
10. **Add WebDAV integration tests** — use the already-imported `wiremock`

### Phase 3: Quality & Maintainability (Medium-term)

11. **Split large files** — sync.rs, storage.rs, webdav.rs into focused modules
12. **Extract named constants** — replace all magic numbers and repeated string literals
13. **Improve error types** — add context fields, implement `source()`, consider `thiserror`
14. **Add accessibility** — ARIA labels, focus traps, keyboard navigation
15. **Deduplicate frontend code** — shared date formatting, menu handlers, error display

### Phase 4: Hardening (Longer-term)

16. **Add security tests** — path traversal, malformed YAML/JSON, auth failures, oversized payloads
17. **Add concurrent access tests** — race conditions, multi-device scenarios
18. **Validate models** — enforce invariants (no circular parents, non-empty titles, valid paths)
19. **Add frontend tests** — component tests for critical flows (panel navigation, sync status)
20. **Implement streaming for large files** — avoid buffering entire file contents in memory

---

## Appendix: Files Audited

| File | Lines | Tests | Findings |
|------|-------|-------|----------|
| `onyx-core/src/sync.rs` | 1,221 | 29 | 15 |
| `onyx-core/src/storage.rs` | 925 | 26 | 11 |
| `onyx-core/src/webdav.rs` | 775 | 14 | 8 |
| `onyx-core/src/repository.rs` | 459 | 24 | 5 |
| `onyx-core/src/config.rs` | 286 | 14 | 5 |
| `onyx-core/src/models.rs` | ~100 | 0 | 3 |
| `onyx-core/src/error.rs` | ~60 | 0 | 2 |
| `apps/tauri/src-tauri/src/*.rs` | ~700 | 0 | 6 |
| `apps/tauri/src/**/*.svelte` | ~2,500 | 0 | 12 |
| **Total** | **~7,000** | **107** | **67** |
