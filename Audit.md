# Audit Log

## 2026-04-29

Found and fixed 3 issues:

1. **Code quality: duplicated atomic-write in `OfflineQueue::save`** (sync.rs:332) — the function maintained its own copy of the temp-file + rename + cleanup-on-failure dance even though `storage::atomic_write` is `pub(crate)` and was already shared by `AppConfig::save_to_file` (fixed 04-25) and `google_tasks.rs`. Replaced the inline implementation with a call to `crate::storage::atomic_write`.
2. **Code quality: duplicated atomic-write in `SyncState::save`** (sync.rs:534) — same pattern as `OfflineQueue::save`. Replaced with a call to `atomic_write`, completing the consolidation of every per-call atomic write into a single shared helper.
3. **Code quality: redundant clone in `start_watcher`** (tauri/lib.rs:1206) — `start_watcher(handle: tauri::AppHandle, ...)` took `handle` by value, then immediately did `let handle = handle.clone();` before moving it into the file-watcher closure. The parameter was unused outside the closure, so the intermediate clone was pure waste. Removed it.

## 2026-04-27

Found and fixed 3 issues:

1. **Perf: needless clone of upload payload** (sync.rs:733) — the `SyncAction::Upload` arm read the file into `data`, computed `compute_checksum(&data)`, then called `client.put_file(path, data.clone())`. The clone existed only because the next statement needed `data.len()` for the sync-state record. Captured `data.len() as u64` into `len` first, moved `data` into `put_file`, and used `len` afterwards — one full byte copy avoided per uploaded file.
2. **Bug: Google Tasks sync silently drops metadata-write failures** (google_tasks.rs:361, 377) — both `.listdata.json` and `.onyx-workspace.json` were written via `if let Ok(meta_content) = serde_json::to_string_pretty(...) { let _ = atomic_write(...); }`, so a serialization or atomic-write error returned `Ok(GoogleSyncResult { downloaded: N, errors: [] })` even though list/workspace ordering was never persisted. Both writes now push their errors into the `errors` vec already returned in `GoogleSyncResult`.
3. **Code quality: unreachable dead-error path in storage dedup** (storage.rs:447) — the dedup loop computed `Option<Task>` from each `by_id` group and then `ok_or_else(|| Error::InvalidData("Empty dedup entries for task"))?`. `by_id` is only populated by `entry(uuid).or_default().push(entry)`, so every group has ≥1 element and the `None` branch is unreachable. Replaced the `Option`+`?` with direct `expect` calls (one per branch) that document the non-empty invariant; the loop now yields `Task` directly.

## 2026-04-25

Found and fixed 3 issues:

1. **Perf: O(n²) deletion-detection in `get_sync_status`** (sync.rs:918) — for every path tracked in `sync_state.files`, the loop scanned `local_files` linearly via `.any(|f| f.path == *path)` to decide whether to count it as a deleted-locally pending change. The earlier "modified or new" loop already used the inverse direction with `sync_state.files.get(...)` (O(1)), so the second loop was the inconsistent one. Built a `HashSet<&str>` of local paths once and used `contains` for the membership check.
2. **Perf: cascade delete walks all_tasks per frontier pop** (tauri/lib.rs:460) — `delete_task`'s descendant BFS scanned the full task list on every parent popped from the frontier, making the work O(n × depth). Built a `parent_id -> [child_id]` `HashMap` once, then the BFS visits each descendant in O(1) amortised, dropping total cost to O(n).
3. **Code quality: duplicate atomic-write in `AppConfig::save_to_file`** (config.rs:114) — the function had its own copy of the temp-file + rename + cleanup-on-failure dance even though `storage::atomic_write` is `pub(crate)` and was already shared by `google_tasks.rs`. Replaced the inline implementation with a call to `crate::storage::atomic_write` so the crate has one canonical atomic write path.

## 2026-04-24

Found and fixed 3 issues:

1. **Bug: orphan base entries never cleaned from sync state** (sync.rs) — when a file was deleted both locally and remotely, `compute_sync_actions` emitted no action (the `(None, None, Some(_))` arm), so the base entry in `.syncstate.json` persisted forever. On each subsequent sync the same no-op case fired and the state file grew. Added `prune_orphan_bases` pass in `sync_workspace_inner` that drops base entries not present in either scan.
2. **Code quality: redundant is_some_and on already-matched Option** (sync.rs:208) — the `(None, Some(_), Some(b))` arm re-checked `remote` via `remote.is_some_and(|r| ...)` even though the pattern had just proven `remote` is `Some(_)`. Bound the inner value with `Some(r)` in the pattern and used `r` directly.
3. **Code quality: single-caller sanitize_filename wrapper** (storage.rs) — `FileSystemStorage::sanitize_filename` was a one-line forwarder to `crate::sanitize_filename` with one call site. Inlined the crate call and removed the method.

## 2026-04-20

Found and fixed 4 issues:

1. **Dead code in conflict recovery** (sync.rs:756) — `parts[1] != ".listdata.json"` was unreachable because the branch is already gated on `parts[1].ends_with(".md")`, which `.listdata.json` cannot satisfy. Removed the redundant check.
2. **O(n²) cascade delete** (tauri/lib.rs) — descendant traversal in `delete_task` used `Vec::contains` inside the inner loop, making it quadratic in the number of tasks per list. Swapped the visited set to `HashSet`; `HashSet::insert` folds the contains+push into one call.
3. **Silent cascade failure in toggle_task** (tauri/lib.rs) — subtask `update_task` errors were discarded with `let _ = ...`, leaving subtasks stuck at the old status with no UI feedback. Propagate the error so the frontend can surface it.
4. **Duplicated UUID-parse boilerplate** (tauri/lib.rs) — 17 commands repeated `Uuid::parse_str(&x).map_err(|e| e.to_string())?`. Extracted a `parse_uuid` helper so callers read as `let id = parse_uuid(&list_id)?;`.

## 2026-04-15

Found and fixed 4 issues:

1. **Bug: debouncedSave shared timer loses edits** (TaskDetailView.svelte) - When user edits both title and description within 400ms, only the last-edited field was saved. Fixed by always saving both fields in the debounced callback.
2. **Code duplication: atomic_write_bytes** (google_tasks.rs) - Identical copy of `atomic_write` from storage.rs. Removed duplicate and reused the shared `pub(crate)` function.
3. **Bug: silent success on missing workspace** (lib.rs) - Four Tauri commands (`set_webdav_config`, `set_workspace_theme`, `set_sync_interval`, `set_sync_interval_unfocused`) silently succeeded when given a nonexistent workspace ID. Fixed to return an error.
4. **Bug: failing test due to wrong frontmatter field name** (storage.rs) - `test_parse_frontmatter_with_optional_fields` used `due:` instead of `date:` in frontmatter YAML, causing the assertion on `fm.date.is_some()` to fail.
