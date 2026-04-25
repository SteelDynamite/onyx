# Audit Log

## 2026-04-25 (second pass)

Found and fixed 2 issues (distinct from PR #62 which covers `get_sync_status`, `delete_task` cascade BFS, and `AppConfig::save_to_file`):

1. **Code duplication: inline atomic-write in `OfflineQueue::save` and `SyncState::save`** (sync.rs) — both methods open-coded the same temp-file + rename + cleanup-on-failure dance even though `storage::atomic_write` is `pub(crate)` and already imported at the top of the file. Same dedup PR #62 applied to `AppConfig::save_to_file`, but for the two sync I/O paths. Replaced with `atomic_write` calls.
2. **Duplicate constants between storage.rs and sync.rs** — `WORKSPACE_METADATA_FILE` (".onyx-workspace.json") and `LIST_METADATA_FILE` (".listdata.json") were defined identically in both files. `parse_frontmatter_for_conflict` also re-encoded the 64KB frontmatter cap as a magic number (`64 * 1024`) with "65536" baked into the error string, while storage.rs already had `MAX_FRONTMATTER_LENGTH`. Promoted the three storage definitions to `pub(crate)` and imported them in sync.rs so a future tweak (e.g. raising the cap) needs one edit instead of three.

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
