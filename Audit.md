# Audit Log

## 2026-04-15

Found and fixed 4 issues:

1. **Bug: debouncedSave shared timer loses edits** (TaskDetailView.svelte) - When user edits both title and description within 400ms, only the last-edited field was saved. Fixed by always saving both fields in the debounced callback.
2. **Code duplication: atomic_write_bytes** (google_tasks.rs) - Identical copy of `atomic_write` from storage.rs. Removed duplicate and reused the shared `pub(crate)` function.
3. **Bug: silent success on missing workspace** (lib.rs) - Four Tauri commands (`set_webdav_config`, `set_workspace_theme`, `set_sync_interval`, `set_sync_interval_unfocused`) silently succeeded when given a nonexistent workspace ID. Fixed to return an error.
4. **Bug: failing test due to wrong frontmatter field name** (storage.rs) - `test_parse_frontmatter_with_optional_fields` used `due:` instead of `date:` in frontmatter YAML, causing the assertion on `fm.date.is_some()` to fail.
