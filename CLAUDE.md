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
- **onyx-cli** — CLI frontend using clap. Commands are in `src/commands/` (init, workspace, list, task, group). Output formatting in `src/output.rs`.
- **apps/tauri/** — Tauri v2 GUI. Svelte 5 frontend in `src/`, Rust backend in `src-tauri/` with Tauri commands that call into `onyx-core`. `notify` crate feature-gated for Android.

### Key patterns

- **Storage trait** (`storage.rs`): Strategy pattern for task persistence. `FileSystemStorage` reads/writes markdown files with YAML frontmatter and JSON metadata files.
- **Repository** (`repository.rs`): `TaskRepository` wraps a `Storage` impl and provides the public API for task/list CRUD, ordering, and grouping. Tests live here.
- **Config** (`config.rs`): `AppConfig` manages named workspaces with paths, mode (local/webdav), theme, and WebDAV URL. Stored in platform-specific config dirs via the `directories` crate.
- **Sync** (`sync.rs`): Three-way diff sync with offline queue. Auto-appends `Onyx/` to WebDAV URL. Wrapped in `tokio::time::timeout` (60s) to handle unreachable servers on Windows.
- **WebDAV** (`webdav.rs`): reqwest client with rustls-tls, 30s request timeout, 10s connect timeout. Credentials stored via `keyring` crate (feature-gated). `Zeroizing<String>` for credential fields. Scoped keyring keys (`com.onyx.webdav.<domain>::<username>`); auto-migrates legacy dot-separated format on load. 10MB PROPFIND response cap.

### On-disk format

Workspaces are plain folders. Each task list is a subfolder containing `.listdata.json` (metadata/ordering) and one `.md` file per task. The workspace root has `.metadata.json` for list ordering.

### Tauri GUI structure

The GUI uses Svelte 5 runes mode (`$state`, `$derived`, `$effect`, `$props()`). Key UI patterns:

- **Sliding drawer**: Left panel (lists) slides with main content as one piece via `translateX`. 80vw wide. List items show checkmark for active list and chevron on hover.
- **Three-panel slide**: Main content area is 300% wide with three panels (task list, task detail, subtask detail) that slide via `translateX` using a `taskStack` array. Stack depth 0 = list, 1 = task detail, 2 = subtask detail.
- **Settings modal**: Per-workspace settings opened from workspace kebab menu. Shows WebDAV config (for webdav workspaces), sync controls, and theme selector.
- **Workspace switcher**: Custom drop-up menu in drawer footer (left), kebab menu per workspace (right) with Settings option.
- **Task animations**: Grid-rows `0fr`/`1fr` trick for smooth collapse/expand. Module-level `animateInIds` Set coordinates expand-in after toggle.
- **Inline editing**: Click task to edit, auto-save on blur. `debouncedSave` snapshots task before timer to prevent stale-reference errors on component destroy.
- **Kebab menus**: Tasks and lists use kebab menus with custom `ConfirmDialog` component (not native `confirm()`). "Move to..." is inline in the menu (not a submenu) to avoid overflow.
- **Main panel header**: Hamburger + window controls in top bar; list name (large, bold) + kebab below divider (matching task detail layout). Kebab has Rename, Group by due date, Delete completed, Delete list.
- **New task**: FAB button opens bottom toast sheet (outside sliding container for fixed positioning).

### Current state (2026-04-03)

- **Phase 1** (Core + CLI): Complete
- **Phase 2** (WebDAV sync): Complete — CLI + GUI sync working, auto-creates `Onyx/` subfolder on remote
- **Phase 3** (GUI MVP): Complete
- **Phase 4** (Mobile): Tauri Android cfg-gated, needs `tauri android init` + build

### GUI features done

- Task CRUD (create, read, update, delete)
- Task completion/restoration with animated transitions
- Drag-and-drop task reordering
- Inline task editing (auto-save on blur)
- Sliding lists drawer with checkmark selection
- Settings popup overlay
- Workspace switcher drop-up with add/remove
- Per-workspace theme system (System default, Light, Dark, Nord, Dracula, Solarized Dark) via CSS `data-theme` attribute
- Completed tasks section with animated show/hide
- Due date picker/editor (DateTimePicker in new task + task detail); `has_time: bool` field tracks whether time is set
- Move task between lists (inline list in kebab menu, no submenu)
- List rename (inline input in main panel header via kebab)
- Group-by-due-date toggle per list (main panel kebab)
- Delete completed tasks (main panel kebab + subtask kebab, with confirmation dialogs)
- Keyboard shortcuts (Escape priority chain: settings → detail → list menu → drawer → menus)
- Setup screen with 2-step mode selection (Local Folder vs WebDAV Server), window dragging, "Open Existing Folder" option
- WebDAV setup flow with connection test, credential storage in system keychain
- WebDAV sync: auto-creates `Onyx/` subfolder on remote, 60s hard timeout, sync error display in settings
- File watcher (notify crate, 500ms debounce, auto-reloads on external changes)
- Sync status indicators (last-sync time + upload/download counts chip)
- Push/pull/full sync mode selection (session-only, in settings)
- WorkspaceMode enum (local/webdav) with per-workspace config
- Desktop packaging (Linux: AppImage + .deb; Windows: MSI)
- Tauri desktop-only deps (notify, keyring) feature-gated for Android compilation
- Subtask hierarchy: subtask count shown on parent tasks in list, subtask detail via three-panel slide navigation, inline add at top of subtask list (new subtasks prepend), collapsible completed subtasks section, cascade delete (parent deletion removes all subtasks with confirmation warning)
- Custom confirmation dialogs (ConfirmDialog component replaces native confirm())

### GUI features NOT yet done

- Workspace retarget/migrate
- Search/filter tasks
- Desktop packaging for macOS

## Roadmap

See `PLAN.md` for the 7-phase roadmap. Detailed API docs in `docs/API.md`, development practices in `docs/DEVELOPMENT.md`.

## GitButler

If you generate code or modify files, run the gitbutler update branches MCP tool.
