# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Onyx is a local-first, cross-platform task management app built in Rust. Tasks are stored as markdown files with YAML frontmatter in user-selected folders. The GUI uses Tauri v2 (Svelte 5 + Tailwind CSS 4) in `apps/tauri/`.

## Build & Test Commands

```bash
cargo build                        # Build all crates
cargo build -p onyx-cli      # Build CLI only
cargo test                         # Run all tests
cargo test -p onyx-core      # Run core library tests only
cargo run -p onyx-cli -- <args>  # Run CLI with arguments

# Tauri GUI
cd apps/tauri && npm install       # Install frontend dependencies
WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri dev  # Run Tauri in dev mode (Wayland)
npm run tauri build                # Build for production
```

The CLI binary is named `onyx` (from the `onyx-cli` crate).

The Tauri dev server runs on port 1422 (`vite.config.ts` and `tauri.conf.json`).

## Architecture

Two-crate workspace (`resolver = "2"`, edition 2021) plus a Tauri app:

- **onyx-core** — Pure Rust library. Storage trait with `FileSystemStorage` implementation, `TaskRepository` (main API), data models, config, error types. No CLI/UI dependencies. `keyring` feature-gated behind `keyring-storage` (default on) for Android compatibility.
- **onyx-cli** — CLI frontend using clap. Commands are in `src/commands/` (init, workspace, list, task, group, sync). Output formatting in `src/output.rs`.
- **apps/tauri/** — Tauri v2 GUI. Svelte 5 frontend in `src/`, Rust backend in `src-tauri/` with Tauri commands that call into `onyx-core`. `notify` crate feature-gated for Android. `tauri-plugin-credentials/` provides cross-platform credential storage (Android Keystore via EncryptedSharedPreferences, desktop via keyring crate).

### Key patterns

- **Storage trait** (`storage.rs`): Strategy pattern for task persistence. `FileSystemStorage` reads/writes markdown files with YAML frontmatter and JSON metadata files. Atomic writes (temp file + rename, with temp cleanup on failure) for all metadata files. Input validation: task titles max 500 chars, descriptions max 1MB, list names max 255 chars, YAML frontmatter max 64KB. Delete operations update metadata before removing files to prevent orphaned metadata on crash.
- **Repository** (`repository.rs`): `TaskRepository` wraps a `Storage` impl and provides the public API for task/list CRUD, ordering, and grouping. Tests live here.
- **Config** (`config.rs`): `AppConfig` manages workspaces keyed by UUID string. `WorkspaceConfig` stores `name`, `path`, `mode` (Local/Webdav/GoogleTasks), `webdav_url`, `webdav_path` (user-selected remote folder), `google_account` (display name/email for GoogleTasks workspaces), `last_sync` (timestamp of last successful sync), `theme`, `sync_interval_secs` (focused polling interval), and `sync_interval_unfocused_secs` (lower-frequency polling when window loses focus, for mobile battery optimization). `add_workspace` returns a generated UUID. Stored in platform-specific config dirs via the `directories` crate. Atomic writes (temp file + rename) prevent corruption on crash.
- **Sync** (`sync.rs`): Three-way diff sync with offline queue. File-based `.sync.lock` prevents concurrent sync operations (auto-cleaned after 5 minutes if stale). Checksum-based conflict resolution: downloads remote, compares SHA-256 — identical content is a false conflict (skipped); when different, remote wins and local is recovered as a duplicate with a new UUID and `[RECOVERED FROM CONFLICT]` prefix (duplicate file cleaned up if metadata update fails). Auto-sync lifecycle: periodic polling (configurable interval, default 60s), debounced file-change (5s), window-focus (30s stale threshold). Wrapped in `tokio::time::timeout` (60s) to handle unreachable servers on Windows. Path traversal validation rejects `..` components and backslashes anywhere in sync paths. Atomic writes for sync state and queue files (temp cleanup on failure).
- **WebDAV** (`webdav.rs`): reqwest client with rustls-tls, 30s request timeout, 10s connect timeout. Rejects non-HTTPS URLs. `Zeroizing<String>` for credential fields. `move_resource` method for WebDAV MOVE (workspace rename). 10MB cap on both PROPFIND responses and file downloads. Desktop credentials via `keyring` crate (feature-gated); Tauri GUI uses `tauri-plugin-credentials` for cross-platform support (Android Keystore + desktop keychain).
- **Google Tasks** (`google_tasks.rs`): Read-only Google Tasks API client using reqwest with Bearer auth. `gt_id_to_uuid()` converts Google Task IDs to stable UUID v5 values for consistent cross-sync identity. Fetches all task lists and tasks from the Google Tasks REST API and writes them locally via `FileSystemStorage`. Remote always wins (read-only workspace mode). OAuth flow is partially implemented — client ID/secret are placeholders pending real credentials.

### On-disk format

Workspaces are plain folders. Each task list is a subfolder containing `.listdata.json` (metadata/ordering) and one `.md` file per task. The workspace root has `.onyx-workspace.json` for list ordering and workspace detection. Sync only processes files at expected depths: `.onyx-workspace.json` at root, `.listdata.json` and `*.md` inside list directories. Task frontmatter contains `id`, `status`, `version` (u64, increments via `saturating_add` on every write, defaults to 1 for legacy files), and optionally `date`, `has_time`, `parent` (omitted when false/null). `list_tasks` auto-deduplicates by UUID, keeping the highest-version file and deleting stale copies; when versions are equal, filesystem modification time is used as a tiebreaker.

### Tauri GUI structure

The GUI uses Svelte 5 runes mode (`$state`, `$derived`, `$effect`, `$props()`). Key UI patterns:

- **Sliding drawer**: Left panel (lists) slides with main content as one piece via `translateX`. 80cqi wide (80% container query inline size). List items show checkmark for active list and chevron on hover.
- **Three-panel slide**: Main content area is 300% wide with three panels (task list, task detail, subtask detail) that slide via `translateX` using a `taskStack` array. Stack depth 0 = list, 1 = task detail, 2 = subtask detail.
- **Settings modal**: Per-workspace settings opened from workspace kebab menu. Shows WebDAV config (for webdav workspaces), sync controls, and theme selector.
- **Workspace switcher**: Custom drop-up menu in drawer footer (left), kebab menu per workspace (right) with Settings option.
- **Task animations**: Grid-rows `0fr`/`1fr` trick for smooth collapse/expand. Module-level `animateInIds` Set coordinates expand-in after toggle.
- **Inline editing**: Click task to edit, auto-save on blur. `debouncedSave` snapshots task before timer to prevent stale-reference errors on component destroy.
- **Kebab menus**: Tasks and lists use kebab menus with custom `ConfirmDialog` component (not native `confirm()`). "Move to..." is inline in the menu (not a submenu) to avoid overflow.
- **Main panel header**: Hamburger + window controls in top bar; list name (large, bold) + kebab below divider (matching task detail layout). Kebab has Rename, Group by date, Delete completed, Delete list.
- **New task**: FAB button opens bottom toast sheet (outside sliding container for fixed positioning).

### Development phase

Pre-alpha. No users, no released builds, no data to migrate. Breaking changes to on-disk formats, config structure, or sync conventions are free — do not add migration logic.

### Current state (2026-04-15)

- **Phase 1** (Core + CLI): Complete
- **Phase 2** (WebDAV sync): Complete — remote folder browsing, checksum-based conflict resolution, auto-sync lifecycle, per-workspace sync interval
- **Phase 3** (GUI MVP): Complete
- **Phase 4** (Mobile): In progress — Android preliminaries done (file-watcher gating, tauri-plugin-credentials, safe area insets, Android targets configured); needs build verification and iOS setup

### GUI features done

- Task CRUD (create, read, update, delete)
- Task completion/restoration with animated transitions
- Drag-and-drop task reordering
- Inline task editing (auto-save on blur)
- Sliding lists drawer with checkmark selection
- Settings popup overlay
- Workspace switcher drop-up with add/remove
- Per-workspace theme system (System default, Light, Dark, Nord, Dracula, Solarized Dark, Black and Gold, Ink) via CSS `data-theme` attribute
- Completed tasks section with animated show/hide
- Date picker/editor (DateTimePicker in new task + task detail); `has_time: bool` field tracks whether time is set
- Move task between lists (inline list in kebab menu, no submenu)
- List rename (inline input in main panel header via kebab)
- Group-by-date toggle per list (main panel kebab; persists flag but display sorting not yet implemented)
- Delete completed tasks (main panel kebab + subtask kebab, with confirmation dialogs)
- Keyboard shortcuts (Escape priority chain: settings → detail → list menu → drawer → menus)
- Setup screen with 2-step mode selection (Local Folder vs WebDAV Server), window dragging, "Open Existing Folder" option, remote folder browsing
- WebDAV setup flow with connection test, credential storage via tauri-plugin-credentials (Android Keystore + desktop keychain)
- WebDAV sync: user-selected remote folder, 60s hard timeout, checksum-based conflict resolution (remote wins, local recovered as duplicate)
- Auto-sync lifecycle: periodic polling (configurable interval), debounced file-change (5s), window-focus (30s stale threshold); sync status dot (idle/synced/error/offline) and manual sync button in drawer
- Initial sync loading screen for new WebDAV workspaces
- Per-workspace sync interval (configurable in settings)
- Upload/download counts in drawer footer (hidden when zero)
- Transient sync error suppression (connectivity issues update status dot only, no error banner)
- File watcher (notify crate, 500ms debounce, auto-reloads on external changes, emits `watcher-error` event to frontend on failures)
- WorkspaceMode enum (local/webdav/googletasks) with per-workspace config, UUID-keyed workspaces
- Window decorations selector (Custom/None/System) — persisted to localStorage, toggled via settings; Custom renders a Linux-style rounded border with `data-decorations` attribute
- Workspace rename (local folder rename + WebDAV MOVE for remote folders, with confirmation dialog)
- Desktop packaging (Linux: AppImage + .deb; Windows: MSI)
- Tauri desktop-only deps (notify, keyring) feature-gated for Android compilation
- Safe area insets for mobile (CSS variables --safe-top/--safe-bottom, viewport-fit=cover)
- Task deduplication on load (handles sync conflict duplicates)
- Subtask hierarchy: subtask count shown on parent tasks in list, subtask detail via three-panel slide navigation, inline add at top of subtask list (new subtasks prepend), collapsible completed subtasks section, cascade delete (parent deletion removes all subtasks with confirmation warning)
- Custom confirmation dialogs (ConfirmDialog component replaces native confirm())
- Workspace path validation (rejects filesystem root `/` and system directories: `/etc`, `/usr`, `/bin`, `/sbin`, `/var`, `/proc`, `/sys`, `/dev`)
- Task detail auto-cleanup (taskStack clears when viewed task is deleted or list switches)
- Swipe gestures on mobile: swipe left/right on a task to toggle completion (swipe direction depends on current status)
- Accessibility: ARIA labels/roles on interactive components, keyboard handlers, `prefers-reduced-motion` CSS support

### GUI features NOT yet done

- Search/filter tasks
- Desktop packaging for macOS

## Roadmap

See `PLAN.md` for the 7-phase roadmap. Detailed API docs in `docs/API.md`, development practices in `docs/DEVELOPMENT.md`.

## GitButler

If you generate code or modify files, run the gitbutler update branches MCP tool.
