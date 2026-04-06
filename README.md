# Onyx

A **local-first, cross-platform tasks application** built with Rust. Inspired by Google Tasks, designed for speed and flexibility.

## Core Principles

- **Local-First**: Your data, your folder, your control
- **Fast**: Sub-second startup, instant response
- **Cross-Platform**: Single codebase, all platforms
- **Flexible**: Multiple workspaces for different contexts

## Project Structure

```
onyx/
├── Cargo.toml                    # Workspace definition
├── PLAN.md                       # Detailed project plan
├── README.md                     # This file
├── crates/
│   ├── onyx-core/          # Core library (backend)
│   └── onyx-cli/           # CLI frontend
├── apps/
│   └── tauri/                    # Tauri v2 GUI (Svelte 5 + Tailwind CSS 4)
└── docs/
```

## Project Status

- **Phase 1** (Core + CLI): Complete
- **Phase 2** (WebDAV Sync): Complete — backend, CLI, and GUI all wired
- **Phase 3** (GUI MVP): Complete
- **Phase 4** (Mobile): Tauri Android cfg-gated, needs `tauri android init` + build

### Core Library (`onyx-core`)
- Data models (Task, TaskList, AppConfig, WorkspaceConfig)
- Markdown file I/O with YAML frontmatter
- Local storage with repository pattern
- Multiple workspace support
- Task ordering and grouping
- Subtask hierarchy (parent_id)
- WebDAV sync with three-way diff and offline queue
- Platform keychain credential storage (feature-gated for Android)
- Checksum-based conflict resolution (remote wins, local recovered as duplicate)

### CLI (`onyx-cli`)
- Workspace management (init, add, list, switch, remove, retarget, migrate)
- Task list management (create, show, delete)
- Task operations (add, complete, delete, edit)
- Group-by-due-date toggle
- WebDAV sync (setup, push, pull, status)

### GUI (`apps/tauri/`)
- Tauri v2 + Svelte 5 + Tailwind CSS 4
- Task CRUD with animated transitions
- Drag-and-drop reordering
- Sliding lists drawer, settings popup
- Workspace switcher with add/remove
- Dark mode (GNOME-style neutral grays, cyan-blue accent)
- Due date picker/editor with optional time
- Subtask hierarchy with three-panel slide navigation
- Move tasks between lists
- List rename, group-by-due-date toggle, delete completed tasks
- Keyboard shortcuts (Escape priority chain)
- WebDAV setup flow with credential auto-population
- File watcher (auto-reloads on external changes)
- Auto-sync with configurable interval, status indicators
- Custom confirmation dialogs
- Desktop packaging (Linux: AppImage + .deb; Windows: MSI)

## Development Setup

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Git
- Node.js 18+ (for Tauri GUI)

### Build

```bash
# Clone and build
git clone https://github.com/SteelDynamite/onyx.git
cd onyx
cargo build

# Run tests
cargo test -p onyx-core

# Run CLI
cargo run -p onyx-cli -- --help

# Run Tauri GUI
cd apps/tauri && npm install
npm run tauri dev
```

## Quick Start

### Initialize your first workspace

```bash
# Initialize a new workspace
cargo run -p onyx-cli -- init ~/Documents/Tasks --name personal

# This creates:
# - A workspace named "personal" at ~/Documents/Tasks
# - A default list called "My Tasks"
# - Sets "personal" as the current workspace
```

### Add and manage tasks

```bash
# Add a task
cargo run -p onyx-cli -- add "Buy groceries"

# Add a task with due date
cargo run -p onyx-cli -- add "Review PR #123" --list "Work" --due "2026-11-15"

# List all tasks
cargo run -p onyx-cli -- list show

# Complete a task
cargo run -p onyx-cli -- complete <task-id>

# Edit a task (opens in $EDITOR)
cargo run -p onyx-cli -- edit <task-id>

# Delete a task
cargo run -p onyx-cli -- delete <task-id>
```

### Manage workspaces

```bash
# Add another workspace
cargo run -p onyx-cli -- workspace add shared ~/Dropbox/TeamTasks

# List workspaces
cargo run -p onyx-cli -- workspace list

# Switch workspace
cargo run -p onyx-cli -- workspace switch shared

# Use specific workspace for a command
cargo run -p onyx-cli -- add "Team meeting" --workspace shared
```

### Manage task lists

```bash
# Create a new list
cargo run -p onyx-cli -- list create "Work"

# Show tasks in a specific list
cargo run -p onyx-cli -- list show --list "Work"

# Delete a list
cargo run -p onyx-cli -- list delete "Work"
```

## Data Format

Tasks are stored as markdown files with YAML frontmatter (Obsidian-compatible):

```markdown
---
id: 550e8400-e29b-41d4-a716-446655440000
status: backlog
version: 3
due: 2026-11-15T14:00:00Z
---

Task description and notes go here in **markdown** format.

- Can include lists
- Rich formatting
- Links, etc.
```

## File System Structure

```
~/Documents/Tasks/           # User-selected folder
├── .onyx-workspace.json     # Workspace metadata: list ordering, detection
├── My Tasks/                # Task list folder
│   ├── .listdata.json       # List metadata: task order, id, timestamps
│   ├── Buy groceries.md     # Individual task files
│   └── Call dentist.md
└── Work/
    ├── .listdata.json
    ├── Review PRs.md
    └── Team meeting prep.md
```

## Testing

Run the test suite:

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p onyx-core

# Run tests with output
cargo test -- --nocapture
```

## What's Next?

- **Phase 4**: Mobile support (iOS & Android via Tauri v2 mobile)
- **Phase 5**: GUI advanced features (rich markdown editor, search/filter)
- **Phase 6**: Mobile polish and platform-specific integrations
- **Phase 7**: Google Tasks importer and unique features

See [PLAN.md](PLAN.md) for detailed roadmap.

## License

[GNU General Public License v3.0 (GPL-3.0)](https://www.gnu.org/licenses/gpl-3.0.en.html)

This project is free and open-source software.
