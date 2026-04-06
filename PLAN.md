# Onyx - Project Plan

## Vision

A **local-first, cross-platform tasks application** inspired by Google Tasks. Built with Rust for high performance and true native support across Windows, Linux, macOS, iOS, and Android.

**Core Principles**:
- **Local-First**: Your data, your folder, your control
- **Fast**: Sub-second startup, instant response
- **Cross-Platform**: Single codebase, all platforms
- **Flexible**: Multiple workspaces for different contexts (personal, shared, work, etc.)

**Data Format**: Tasks stored as markdown files with YAML frontmatter (Obsidian-compatible)
**Storage**: User selects folder location for each workspace (e.g., `~/Documents/Tasks`, `~/Dropbox/TeamTasks`)
**Sync**: Optional per-workspace WebDAV for cross-device synchronization
**Architecture**: Backend/frontend separation with CLI-first development

---

## Resources

- [Tauri Documentation](https://v2.tauri.app/)
- [Svelte Documentation](https://svelte.dev/)
- [Tailwind CSS Documentation](https://tailwindcss.com/)
- [WebDAV RFC 4918](https://datatracker.ietf.org/doc/html/rfc4918)
- [Google Tasks API](https://developers.google.com/tasks) (for importer reference)

---

## Phase 1: Core Library & CLI MVP

**Goal**: Build and validate the backend with a functional CLI

### Why CLI First?
- Test backend thoroughly before GUI complexity
- CLI useful for power users and automation
- Clean API boundaries
- Easy to write comprehensive tests

### Architecture

#### Cargo Workspace Structure
```
onyx/
├── Cargo.toml                    # Workspace definition
├── PLAN.md
├── README.md
├── apps/
│   └── tauri/                    # Tauri GUI (Svelte + Tailwind)
├── crates/
│   ├── onyx-core/          # Core library (backend)
│   └── onyx-cli/           # CLI frontend
└── docs/
```

#### Data Model

Tasks are stored as individual `.md` files with YAML frontmatter:

```markdown
---
id: 550e8400-e29b-41d4-a716-446655440000
status: backlog
version: 3
due: 2026-11-15T14:00:00Z
parent: 550e8400-e29b-41d4-a716-446655440001
---

Task description and notes go here in **markdown** format.

- Can include lists
- Rich formatting
- Links, etc.
```

**TaskStatus values**:
- `backlog` - Task not yet completed
- `completed` - Task is done

**In-Memory Model**:
```rust
Task {
    id: Uuid,
    title: String,              // Derived from filename (without .md extension)
    description: String,              // Markdown content
    status: TaskStatus,         // Backlog or Completed
    due_date: Option<DateTime>,
    has_time: bool,             // Whether due_date includes a specific time
    version: u64,               // Increments (saturating) on every write; used for sync dedup
    parent_id: Option<Uuid>,    // For subtasks
}

enum TaskStatus {
    Backlog,     // Not yet completed
    Completed,   // Done
}

TaskList {
    id: Uuid,
    title: String,              // Derived from folder name
    tasks: Vec<Task>,           // Ordered by task_order, optionally grouped by due date
    created_at: DateTime,
    updated_at: DateTime,
    group_by_due_date: bool,    // If true, group by due date before applying task_order
}

AppConfig {
    workspaces: HashMap<String, WorkspaceConfig>,  // UUID keys
    current_workspace: Option<String>,              // UUID
}

WorkspaceConfig {
    name: String,                      // Display name
    path: PathBuf,
    mode: WorkspaceMode,               // Local or Webdav
    webdav_url: Option<String>,
    webdav_path: Option<String>,       // User-selected remote folder
    theme: Option<String>,
    sync_interval_secs: Option<u64>,
}
```

#### File System Structure

```
~/Documents/Tasks/           # User-selected folder
├── .onyx-workspace.json           # Global: list ordering, last opened list
├── My Tasks/                # Task list folder
│   ├── .listdata.json       # List metadata: task order, id, timestamps
│   ├── Buy groceries.md     # Title: "Buy groceries" (without .md)
│   └── Call dentist.md      # Title: "Call dentist" (without .md)
└── Work/                    # Another task list
    ├── .listdata.json
    ├── Review PRs.md        # Title: "Review PRs" (without .md)
    └── Team meeting prep.md # Title: "Team meeting prep" (without .md)
```

**Note**: Task titles are derived from filenames by removing the `.md` extension.

**`.onyx-workspace.json` (root level)**:
```json
{
  "version": 1,
  "list_order": ["list-uuid-1", "list-uuid-2"],
  "last_opened_list": "list-uuid-1"
}
```

**`.listdata.json` (per list)**:
```json
{
  "id": "list-uuid-1",
  "created_at": "2026-10-26T10:00:00Z",
  "updated_at": "2026-10-27T14:30:00Z",
  "group_by_due_date": false,
  "task_order": [
    "task-uuid-1",
    "task-uuid-2",
    "task-uuid-3"
  ]
}
```

**Task Ordering**:
- Tasks are always ordered according to the `task_order` array (manual ordering)
- When `group_by_due_date` is `true`, tasks are first grouped by their due date, then sorted within each group by `task_order`
- Tasks without due dates appear at the end when grouping is enabled

**App Configuration** (separate from task data, supports multiple workspaces):
- Windows: `%APPDATA%/onyx/config.json`
- Linux: `~/.config/onyx/config.json`
- macOS: `~/Library/Application Support/onyx/config.json`

```json
{
  "workspaces": {
    "a1b2c3d4-...": {
      "name": "personal",
      "path": "/home/user/Documents/Tasks",
      "mode": "local"
    },
    "e5f6g7h8-...": {
      "name": "shared",
      "path": "/home/user/Dropbox/TeamTasks",
      "mode": "webdav",
      "webdav_url": "https://nextcloud.example.com/remote.php/dav/files/user/",
      "webdav_path": "TeamTasks"
    }
  },
  "current_workspace": "a1b2c3d4-..."
}
```

#### Core Library API

```rust
pub struct TaskRepository {
    storage: Box<dyn Storage>,
}

impl TaskRepository {
    pub fn new(tasks_folder: PathBuf) -> Result<Self>;
    pub fn init(tasks_folder: PathBuf) -> Result<Self>;

    // Task operations
    pub fn create_task(&mut self, list_id: Uuid, task: Task) -> Result<Task>;
    pub fn get_task(&self, list_id: Uuid, task_id: Uuid) -> Result<Task>;
    pub fn update_task(&mut self, list_id: Uuid, task: Task) -> Result<()>;
    pub fn delete_task(&mut self, list_id: Uuid, task_id: Uuid) -> Result<()>;
    pub fn list_tasks(&self, list_id: Uuid) -> Result<Vec<Task>>;

    // List operations
    pub fn create_list(&mut self, name: String) -> Result<TaskList>;
    pub fn get_lists(&self) -> Result<Vec<TaskList>>;
    pub fn get_list(&self, list_id: Uuid) -> Result<TaskList>;
    pub fn delete_list(&mut self, id: Uuid) -> Result<()>;

    // Task ordering (modifies .listdata.json)
    pub fn reorder_task(&mut self, list_id: Uuid, task_id: Uuid, new_position: usize) -> Result<()>;
    pub fn get_task_order(&self, list_id: Uuid) -> Result<Vec<Uuid>>;

    // Grouping preference (modifies .listdata.json)
    pub fn set_group_by_due_date(&mut self, list_id: Uuid, enabled: bool) -> Result<()>;
    pub fn get_group_by_due_date(&self, list_id: Uuid) -> Result<bool>;
}

pub trait Storage {
    fn read_task(&self, list_id: Uuid, task_id: Uuid) -> Result<Task>;
    fn write_task(&mut self, list_id: Uuid, task: &Task) -> Result<()>;
    // ... more methods
}
```

### Dependencies

**Workspace Cargo.toml**:
```toml
[workspace]
members = [
    "crates/onyx-core",
    "crates/onyx-cli",
]
exclude = [
    "apps/tauri/src-tauri",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
uuid = { version = "1.0", features = ["serde", "v4"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
tokio = { version = "1.40", features = ["full"] }
```

**onyx-core/Cargo.toml**:
```toml
[package]
name = "onyx-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = "1.0"
serde_yaml = "0.9"        # YAML frontmatter
uuid = { workspace = true }
chrono = { workspace = true }
directories = "5.0"

[dev-dependencies]
tempfile = "3.0"
```

**onyx-cli/Cargo.toml**:
```toml
[package]
name = "onyx-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "onyx"
path = "src/main.rs"

[dependencies]
onyx-core = { path = "../onyx-core" }
clap = { version = "4.5", features = ["derive", "env"] }
colored = "2.0"
anyhow = { workspace = true }
fs_extra = "1.3"
```

### Features

- [x] Cargo workspace setup
- [x] Data models (Task, TaskList, AppConfig, WorkspaceConfig)
- [x] Markdown file I/O with YAML frontmatter parsing
- [x] Local storage implementation
- [x] Repository pattern and public API
- [x] Multiple workspace support
- [x] CLI: `init` command (create named workspace)
- [x] CLI: `workspace add` command (add additional workspaces)
- [x] CLI: `workspace list` command (view all workspaces)
- [x] CLI: `workspace switch` command (change current workspace)
- [x] CLI: `workspace remove` command (delete workspace)
- [x] CLI: `workspace retarget` command (update workspace path without moving files)
- [x] CLI: `workspace migrate` command (move files to new location)
- [x] CLI: `list create` command (create new task lists)
- [x] CLI: `list` command (view tasks)
- [x] CLI: `add` command (create tasks)
- [x] CLI: `complete` command (mark done)
- [x] CLI: `delete` command (remove tasks)
- [x] CLI: `edit` command (modify tasks - CLI only, creates temp file)
- [x] Manual task ordering (always via task_order array)
- [x] CLI: `group` command (toggle group-by-due-date for a list)
- [x] Support for `--workspace` flag on all commands
- [x] Comprehensive unit and integration tests (>80% coverage)

### CLI Usage Examples

```bash
# First run: initialize a workspace (creates named workspace)
$ onyx init ~/Documents/Tasks --name personal
✓ Initialized workspace "personal" at ~/Documents/Tasks
✓ Created default list "My Tasks"
✓ Set "personal" as current workspace

# Add more workspaces (e.g., for shared/collaborative tasks)
$ onyx workspace add shared ~/Dropbox/TeamTasks
✓ Added workspace "shared" at ~/Dropbox/TeamTasks
✓ Created default list "My Tasks"

# List all workspaces
$ onyx workspace list
  personal: ~/Documents/Tasks (current)
  shared: ~/Dropbox/TeamTasks

# Switch between workspaces
$ onyx workspace switch shared
✓ Switched to workspace "shared"

# Create a new task list
$ onyx list create "Work"
✓ Created list "Work"

$ onyx list create "Personal Projects"
✓ Created list "Personal Projects"

# Add tasks (uses current workspace by default)
$ onyx add "Buy groceries"
✓ Created task "Buy groceries" (550e8400-e29b-41d4-a716-446655440000)

$ onyx add "Review PR #123" --list "Work" --due "2026-11-15"
✓ Created task "Review PR #123" (7f3a9c21-b8d2-4e5f-9a1c-3d8e7f6a2b1c)
  Due: 2026-11-15

# Or specify workspace explicitly
$ onyx add "Team meeting" --workspace shared
✓ Created task "Team meeting" in workspace "shared"

# List all tasks (from current workspace)
$ onyx list show
My Tasks (3 tasks)
  [ ] Buy groceries
  [ ] Call dentist
  [✓] Pay bills

Work (2 tasks)
  [ ] Review PR #123 (due: 2026-11-15)
  [ ] Team meeting prep

# List tasks from specific workspace
$ onyx list show --workspace shared
Shared Tasks (2 tasks)
  [ ] Team meeting
  [ ] Quarterly planning

# List tasks in specific list
$ onyx list show --list "Work"
Work (2 tasks)
  [ ] Review PR #123 (due: 2026-11-15)
  [ ] Team meeting prep

# Complete a task
$ onyx complete 550e8400-e29b-41d4-a716-446655440000
✓ Completed task "Buy groceries"

# Edit a task (CLI-only: creates temp file, opens $EDITOR, blocks until editor exits, then parses)
$ onyx edit 7f3a9c21-b8d2-4e5f-9a1c-3d8e7f6a2b1c
# Opens editor with task markdown file
# User edits and saves, then exits editor
✓ Updated task "Review PR #123"

# Delete a task
$ onyx delete 550e8400-e29b-41d4-a716-446655440000
✓ Deleted task "Buy groceries"

# Retarget workspace (files already at new location, just update config)
$ onyx workspace retarget personal ~/new/path/to/Tasks
✓ Workspace "personal" now points to ~/new/path/to/Tasks

# Migrate workspace (move files to new location)
$ onyx workspace migrate personal ~/Dropbox/Tasks
⚠ This will move all files from ~/Documents/Tasks to ~/Dropbox/Tasks
Continue? (y/n): y
Moving files...
  Moved .onyx-workspace.json
  Moved My Tasks/ (15 files)
  Moved Work/ (8 files)
✓ Migrated 23 files to ~/Dropbox/Tasks
✓ Workspace "personal" now points to ~/Dropbox/Tasks

# Remove a workspace
$ onyx workspace remove shared
⚠ Warning: This will delete workspace config (files remain on disk)
Continue? (y/n): y
✓ Removed workspace "shared"

# Toggle grouping by due date (tasks always use manual task_order within groups)
$ onyx group enable --list "Work"
✓ Enabled group-by-due-date for list "Work"

$ onyx group disable --list "Personal"
✓ Disabled group-by-due-date for list "Personal"
```

### Deliverables

- [x] `onyx-core` library with stable API
- [x] Functional CLI that can manage tasks
- [x] Data persists as Obsidian-compatible .md files
- [x] Well-tested backend (>80% coverage)
- [x] Documentation for core library API

### Development Setup

```bash
# Clone and build
git clone <repository-url>
cd onyx
cargo build

# Run tests
cargo test -p onyx-core

# Run CLI
cargo run -p onyx-cli -- init ~/test-tasks --name test
cargo run -p onyx-cli -- add "Test task"
cargo run -p onyx-cli -- list
cargo run -p onyx-cli -- workspace list
```

---

## Phase 2: WebDAV Sync (Backend + CLI)

**Goal**: Enable cross-device synchronization via CLI

### Architecture

#### WebDAV Integration

Add WebDAV support to `onyx-core`:

```rust
// WorkspaceConfig with WebDAV support (UUID-keyed in AppConfig)
WorkspaceConfig {
    name: String,
    path: PathBuf,
    mode: WorkspaceMode,               // Local or Webdav
    webdav_url: Option<String>,
    webdav_path: Option<String>,       // User-selected remote folder
    last_sync: Option<DateTime>,
    theme: Option<String>,
    sync_interval_secs: Option<u64>,
}

// AppConfig remains the same (workspaces + current_workspace)
AppConfig {
    workspaces: HashMap<String, WorkspaceConfig>,
    current_workspace: Option<String>,
}

// Sync functions in onyx_core::sync module (standalone, not on TaskRepository)
pub async fn sync_workspace(
    workspace_path: &Path,
    webdav_url: &str,
    username: &str,
    password: &str,
    mode: SyncMode,       // Full, PushOnly, or PullOnly
) -> Result<SyncResult>;

pub fn get_sync_status(workspace_path: &Path) -> Result<SyncStatusInfo>;

// Credential functions in onyx_core::webdav module
pub fn store_credentials(domain: &str, username: &str, password: &str) -> Result<()>;
pub fn load_credentials(domain: &str) -> Result<(String, String)>;
pub fn delete_credentials(domain: &str) -> Result<()>;
```

#### Sync Strategy
- **Trigger**: Auto-sync lifecycle — periodic polling (configurable interval, default 60s), debounced file-change (5s), window-focus (30s stale threshold)
- **Conflict Resolution**: Checksum-based — downloads remote, compares SHA-256. Identical = false conflict (skipped). Different = remote wins, local recovered as duplicate with new UUID and `[RECOVERED FROM CONFLICT]` prefix
- **Offline Support**: Queue operations when offline, sync when online

#### Authentication

**Primary**: Platform Keychain via `keyring` crate
- Store WebDAV username + password in system keychain
- Key format: `com.onyx.webdav.{server-domain}::{username}` (the `::` separator prevents key collisions when usernames contain dots)
- Works on: Windows (Credential Manager), macOS (Keychain), Linux (Secret Service), iOS/Android (Keystore)

**Fallback**: Not yet implemented (planned: encrypted local storage if keychain unavailable)

### Dependencies

Add to `onyx-core/Cargo.toml`:
```toml
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
keyring = "3.0"
# TODO: Evaluate dav-client or implement custom WebDAV
```

### Features

- [x] WebDAV client implementation in core library
- [x] Credential storage (platform keychain)
- [x] Bi-directional sync (push/pull)
- [x] Conflict resolution (remote wins; local recovered as duplicate)
- [x] Offline queue for pending operations
- [x] CLI: `sync --setup` command
- [x] CLI: `sync --push` command
- [x] CLI: `sync --pull` command
- [x] CLI: `sync --status` command
- [ ] Progress indicators for sync operations

### CLI Usage Examples

```bash
# Setup WebDAV for current workspace
$ onyx sync --setup
WebDAV URL: https://nextcloud.example.com/remote.php/dav/files/username/Tasks
Username: myuser
Password: ********
✓ WebDAV credentials saved to system keychain
✓ Connection verified for workspace "personal"

# Setup WebDAV for specific workspace
$ onyx sync --setup --workspace shared
WebDAV URL: https://nextcloud.example.com/remote.php/dav/files/username/SharedTasks
Username: myuser
Password: ********
✓ WebDAV credentials saved to system keychain
✓ Connection verified for workspace "shared"

# Push local changes to WebDAV server (current workspace)
$ onyx sync --push
Syncing workspace "personal" to https://nextcloud.example.com/...
  Uploading My Tasks/.listdata.json
  Uploading My Tasks/Buy groceries.md
  Uploading Work/Review PR #123.md
✓ Pushed 3 files to WebDAV server

# Pull changes from WebDAV server
$ onyx sync --pull
Syncing workspace "personal" from https://nextcloud.example.com/...
  Downloading Work/Team meeting notes.md
  Downloading Personal/Call mom.md
✓ Pulled 2 files from WebDAV server

# Automatic two-way sync
$ onyx sync
Syncing workspace "personal" with https://nextcloud.example.com/...
  ↑ Uploading My Tasks/New task.md
  ↓ Downloading Work/Updated task.md
  = No changes for 15 files
✓ Sync complete

# Sync specific workspace
$ onyx sync --workspace shared
Syncing workspace "shared" with https://nextcloud.example.com/...
✓ Sync complete (no changes)

# Check sync status for current workspace
$ onyx sync --status
Workspace: personal
WebDAV Server: https://nextcloud.example.com/remote.php/dav/files/username/Tasks
Status: Connected
Last sync: 2026-10-27 14:32:15
Local changes: 2 files modified
Remote changes: 0 files modified

# Check sync status for all workspaces
$ onyx sync --status --all
Workspace: personal
  WebDAV: https://nextcloud.example.com/.../Tasks
  Status: Connected
  Last sync: 2026-10-27 14:32:15

Workspace: shared
  WebDAV: https://nextcloud.example.com/.../SharedTasks
  Status: Connected
  Last sync: 2026-10-27 14:28:42
```

### Deliverables

- [x] Working WebDAV sync in backend
- [x] CLI can sync with remote WebDAV server
- [x] Reliable conflict resolution
- [ ] Tested with Nextcloud, ownCloud

---

## Phase 3: GUI MVP (Desktop)

**Goal**: Build graphical interface on desktop platforms

### Architecture

#### Frontend Framework: Tauri v2 + Svelte 5 + Tailwind CSS 4

**Decision**: Use Tauri v2 with Svelte and Tailwind for the GUI

**Why Tauri?**
- Native Rust backend — direct integration with `onyx-core`
- Svelte 5 for reactive, performant UI with minimal boilerplate
- Tailwind CSS 4 for rapid, consistent styling
- Small binary size (~5-10MB)
- Cross-platform (Windows, Linux, macOS; mobile in Tauri v2)
- Web technologies for UI = rich ecosystem, easy to iterate
- Tauri commands expose core library directly to the frontend

#### GUI Structure

```
apps/tauri/
├── package.json
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/                          # Svelte frontend
│   ├── main.ts
│   ├── app.css
│   ├── App.svelte
│   └── lib/
│       ├── screens/
│       │   ├── TasksScreen.svelte
│       │   ├── SettingsScreen.svelte
│       │   └── SetupScreen.svelte
│       ├── components/
│       │   ├── TaskItem.svelte
│       │   ├── NewTaskInput.svelte
│       │   └── ListSelector.svelte
│       └── stores/
│           └── app.ts
└── src-tauri/                    # Rust backend (Tauri commands)
    ├── Cargo.toml
    ├── tauri.conf.json
    └── src/
        ├── main.rs
        ├── commands.rs           # Tauri command handlers
        └── lib.rs
```

#### First Run Experience
- Show workspace setup dialog on first launch
- User creates first workspace with name and folder location
- User selects where to store tasks (e.g., `~/Documents/Tasks`)
- No default hidden directories
- Remember workspaces in app config

#### Workspace UI Elements
- Workspace selector dropdown in toolbar
- Quick-switch between workspaces
- Visual indicator of current workspace
- Settings panel to manage workspaces (add/remove/configure)

#### App Configuration (Phase 3+)

**AppConfig** with UI preferences (theme is per-workspace):
```rust
AppConfig {
    workspaces: HashMap<String, WorkspaceConfig>,  // UUID keys
    current_workspace: Option<String>,              // UUID
}

WorkspaceConfig {
    name: String,                        // Display name
    path: PathBuf,
    mode: WorkspaceMode,                 // Local or Webdav
    webdav_url: Option<String>,
    webdav_path: Option<String>,         // User-selected remote folder
    last_sync: Option<DateTime>,
    theme: Option<String>,               // Per-workspace theme
    sync_interval_secs: Option<u64>,     // Auto-sync interval
}
```

### Performance Strategy

**Startup Sequence**:
1. Initialize Tauri window + load Svelte app (< 100ms)
2. Load config from disk via Tauri command (< 20ms)
3. Render UI (first paint < 150ms)
4. Load current task list in background
5. Update UI as tasks load
6. Start WebDAV sync in background (if configured)

**Target**: < 300ms cold start on desktop

**Optimizations**:
- Lazy data loading (load visible tasks first)
- Background operations for sync via async Tauri commands
- Efficient file I/O (stream large files)
- Svelte's compiled reactivity for minimal DOM updates

### Features

- [x] Tauri v2 + Svelte 5 + Tailwind CSS 4 framework integration
- [x] Workspace setup dialog on first launch
- [x] Workspace selector (drop-up menu in drawer footer)
- [x] Quick-switch between workspaces
- [x] Basic task list view with pending/completed sections
- [x] Create new tasks (FAB + bottom toast sheet with title/description)
- [x] Edit existing tasks (inline editing, auto-save on blur)
- [x] Delete tasks (kebab menu → delete)
- [x] Mark tasks complete/incomplete with animated transitions
- [x] Drag-and-drop task reordering
- [x] Sliding lists drawer (80vw, left side)
- [x] Settings popup overlay (WebDAV config, dark mode toggle)
- [x] Dark mode (GNOME-style neutral theme, cyan-blue accent)
- [x] Animated completed section show/hide
- [x] Move task between lists (kebab menu → "Move to..." submenu in task detail view)
- [x] Optional time on due dates (`has_time: bool` field on Task with `#[serde(default)]` for backward compat; replaces the hours==0 heuristic)
- [x] Due date picker/editor (DateTimePicker component in both new task toast + task detail view)
- [x] WebDAV setup flow with credentials (settings auto-populates URL/username/password from config + keychain on open)
- [x] List rename (inline input via list kebab menu in drawer)
- [x] Keyboard shortcuts (Escape closes settings → detail → drawer → menus in priority order)
- [x] Sync status indicators (last-sync time + upload/download counts chip in TasksScreen)
- [x] Push/pull sync mode selection (session-only sync direction selector in SettingsScreen)
- [x] Group-by-due-date toggle per list (checkmark toggle in list kebab menu)
- [x] Subtask hierarchy (expand/collapse, inline add, cascade toggle/delete)
- [ ] Search/filter tasks
- [x] Desktop packaging (Linux: AppImage + .deb; Windows: MSI; macOS not yet verified)
- [x] File watcher (notify crate, 500ms debounce, auto-reloads UI on external file changes)

### Deliverables

- [x] Functional desktop GUI app (Linux verified, Windows MSI packaging added)
- [ ] Sub-300ms startup time (not yet measured/optimized)
- [x] Clean, minimal UI
- [ ] Feature parity with CLI

### Build & Release

**Distribution**:
- Linux: AppImage, .deb, .tar.gz
- macOS: DMG
- Windows: MSI, portable .exe

**CI/CD**: GitHub Actions for automated builds

---

## Phase 4: Mobile Basic Support

**Goal**: Get the Tauri GUI building and running on Android and iOS, validate cross-platform architecture

### Why Early Mobile?
- De-risk mobile builds early in development
- Test cross-platform architecture sooner
- Get mobile-specific feedback early
- Can dogfood on mobile while building desktop features

### Hard Constraint: iOS Requires macOS

iOS builds require Xcode, which only runs on macOS. Android builds work fine on Linux. Options for iOS CI:
- **GitHub Actions** `macos-latest` runner (free for public repos, paid minutes for private)
- **Codemagic / Bitrise** — dedicated mobile CI services
- A physical Mac

All Android work can be done locally on Linux. iOS must go through CI or a Mac.

---

### Tauri GUI

Tauri v2 has mobile support but it's newer and less mature.

#### Known Blockers

**`notify` crate doesn't compile for mobile.** The file-watcher subsystem (`notify` + `notify-debouncer-mini` in `Cargo.toml`) does not support Android or iOS targets. The entire file-watcher initialization path must be gated behind `#[cfg(not(mobile))]` before cross-compilation will succeed.

**Desktop-only window config.** `tauri.conf.json` has `decorations: false` and `transparent: true` — these are ignored on mobile but may cause confusion. Mobile uses full-screen native WebViews.

**No mobile init done.** `gen/android/` and `gen/ios/` Gradle/Xcode projects have not been generated yet.

#### Android Prerequisites

1. Android Studio + NDK r26+ installed, `ANDROID_HOME` and `NDK_HOME` set
2. Rust Android targets: `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`

#### Build Commands

```bash
cd apps/tauri

# Android
npm run tauri android init   # first time: generates gen/android/
npm run tauri android dev    # dev build with hot reload
npm run tauri android build  # release APK/AAB

# iOS (macOS CI only)
npm run tauri ios init       # first time: generates gen/ios/
npm run tauri ios dev
npm run tauri ios build
```

#### Features

- [x] Gate file-watcher initialization behind `#[cfg(not(mobile))]`
- [x] Install Android Studio + NDK, configure env vars
- [x] Add Android Rust targets
- [x] `npm run tauri android init` (generates `gen/android/`)
- [x] Confirm `npm run tauri android build` succeeds
- [ ] Basic smoke test: app launches, workspace setup, create a task
- [ ] Set up macOS CI for iOS builds
- [ ] `npm run tauri ios init` (generates `gen/ios/`)
- [ ] Confirm `npm run tauri ios build` succeeds on CI

---

### Mobile Adaptation

**Touch Support**:
- Larger touch targets (44pt minimum)
- Mobile-responsive layouts
- Test on real devices

**File System Access**:
- iOS: App sandbox documents directory + file dialog plugin
- Android: Scoped storage + file dialog plugin

**First Run on Mobile**:
- Show folder picker on first launch
- Suggest: Documents, iCloud Drive (iOS), Google Drive (Android)
- User selects folder, path stored in preferences

### Deliverables

- [ ] Tauri APK builds locally on Linux (Android)
- [ ] Tauri iOS builds on macOS CI
- [ ] Basic task CRUD works on mobile
- [ ] Validates cross-platform architecture

### Distribution

- Android: .apk (direct install / sideloading)
- iOS: .ipa for TestFlight (early access)

**Note**: This phase prioritizes getting mobile building and launching, even with a simple UI. Touch polish comes in Phase 6.

---

## Phase 5: GUI Advanced Features (Desktop + Mobile)

**Goal**: Feature parity with Google Tasks across all platforms

### Features

#### Desktop & Mobile
- [x] Multiple task lists (folders)
- [x] Switch between lists
- [x] Subtasks support
- [x] Due dates with date picker
- [ ] Rich markdown editor for task notes
- [x] Move tasks between lists
- [ ] Change storage folder location in settings
- [ ] Search functionality
- [x] Theme selection (light/dark mode)

#### Desktop-Specific
- [x] Drag & drop reordering
- [x] Keyboard shortcuts
- [ ] Multiple windows (optional)

#### Mobile-Specific
- [x] Swipe gestures (swipe to complete, swipe to delete)
- [ ] Pull-to-refresh
- [ ] Touch-optimized UI elements
- [ ] Larger touch targets

### Deliverables

- [ ] Full-featured task manager on all platforms
- [ ] Polished UX on desktop
- [ ] Touch-optimized UX on mobile
- [ ] Consistent feature set across platforms

---

## Phase 6: Mobile Polish & Platform-Specific Features

**Goal**: Native mobile experience and deep platform integration

### Features

#### iOS-Specific
- [ ] Share extension (share to tasks)
- [ ] iOS widgets (home screen, lock screen)
- [ ] Siri shortcuts
- [ ] Haptic feedback
- [ ] iOS-native gestures
- [ ] App icon badge with task count
- [ ] Quick capture via 3D touch / long press
- [ ] iCloud Drive integration

#### Android-Specific
- [ ] Share target (share to tasks)
- [ ] Android widgets (home screen)
- [ ] Quick settings tile
- [ ] Haptic feedback
- [ ] Material Design guidelines
- [ ] Google Drive integration

#### Both Platforms
- [ ] Background sync on mobile
- [ ] Push notifications for due dates
- [ ] Notification actions (complete from notification)
- [ ] App shortcuts
- [ ] Platform-specific animations

### Deliverables

- [ ] Native-feeling mobile apps
- [ ] Deep platform integration
- [ ] Mobile-specific features

### Distribution

**App Store Distribution**:
- iOS: Apple App Store
- Android: Google Play Store
- Android: F-Droid (FOSS store)

---

## Phase 7: Advanced Features & Imports

**Goal**: Differentiate from Google Tasks, add unique features

### Features

#### Google Tasks Importer
- [ ] **Import from Google Tasks** (via API or export)
- [ ] Migrate tasks, lists, due dates, notes
- [ ] Preserve task hierarchy and order
- [ ] Easy onboarding for Google Tasks users

#### Advanced Task Management
- [ ] **Recurring tasks** (tasks that automatically uncomplete and reschedule)
  - When completed, task automatically returns to backlog
  - Due date updates by specified interval (e.g., +1 day, +1 week, +1 month)
  - Intervals: daily, weekly, monthly, yearly, custom (e.g., "every 3 days")
  - Optional: limit number of repetitions or end date
  - Stored in frontmatter: `recurs: "daily"`, `recurs_until: "2026-01-01"`
- [ ] Task templates (save common tasks)
- [ ] Bulk operations (select multiple, bulk edit)
- [ ] Full-text search across all tasks
- [ ] Filters and smart lists (e.g., "Due this week")
- [ ] Statistics and insights (completion rate, etc.)

#### Integration & Automation
- [ ] Calendar integration (view tasks in calendar)
- [ ] Email to task (send email to create task)
- [ ] Voice input (speech-to-text for tasks)
- [ ] URL schemes / deep links
- [ ] Zapier integration (optional)

#### Collaboration (Optional)
- [ ] Share lists with other users
- [ ] Collaborative editing
- [ ] Comments on tasks
- [ ] Activity log

#### Customization & Polish
- [ ] Custom themes and color schemes
- [ ] Advanced animations (consider Bevy migration)
- [ ] Plugin system for extensions (optional)
- [ ] Custom fonts
- [ ] Export/import (backup/restore to .zip)

### Optional: Bevy Migration

If you want game-like polish after Phase 7:
- Migrate GUI from Tauri/Svelte to Bevy
- Full control over animations and rendering
- Unique, polished look beyond standard apps
- Backend (`onyx-core`) stays identical
- Only rewrite the GUI layer

### Deliverables

- [ ] Polished, delightful UX
- [ ] Unique features not in Google Tasks
- [ ] Easy migration path from Google Tasks
- [ ] Distribution to all app stores

### Final Distribution

**All Platforms**:
- F-Droid (FOSS Android)
- Flathub (Linux Flatpak)
- Google Play Store (Android)
- Apple App Store (iOS and macOS)
- Microsoft Store (Windows)
- Direct downloads (all platforms)

---

## License

[GNU General Public License v3.0 (GPL-3.0)](https://www.gnu.org/licenses/gpl-3.0.en.html)

This project is free and open-source software licensed under GPL v3.

---

**Last Updated**: 2026-04-05
**Document Version**: 4.3
**Status**: Ready to Implement - Milestone-Driven Plan
