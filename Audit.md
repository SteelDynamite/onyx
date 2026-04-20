# Audit Log

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
