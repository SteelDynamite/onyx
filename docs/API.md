# Onyx Core - API Documentation

## Overview

The `onyx-core` library provides a complete backend for managing tasks in a local-first manner. Tasks are stored as markdown files with YAML frontmatter, compatible with Obsidian and other markdown editors.

## Core Concepts

### Data Models

#### Task

Represents an individual task.

```rust
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub date: Option<DateTime<Utc>>,
    pub has_time: bool,            // Whether date includes a specific time
    pub version: u64,              // Increments (saturating) on every write; used for sync dedup
    pub parent_id: Option<Uuid>,
}

pub enum TaskStatus {
    Backlog,     // Not yet completed
    Completed,   // Done
}
```

**Creating a Task:**

```rust
use onyx_core::Task;

// Simple task
let task = Task::new("Buy groceries".to_string());

// Task with description and date
let task = Task::new("Review PR #123".to_string())
    .with_description("Check the authentication changes".to_string())
    .with_date(chrono::Utc::now() + chrono::Duration::days(2));
```

#### TaskList

Represents a collection of tasks.

```rust
pub struct TaskList {
    pub id: Uuid,
    pub title: String,
    pub tasks: Vec<Task>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub group_by_date: bool,
}
```

### Configuration

#### AppConfig

Global application configuration supporting multiple workspaces. Workspaces are keyed by UUID string.

```rust
pub struct AppConfig {
    pub workspaces: HashMap<String, WorkspaceConfig>,  // UUID keys
    pub current_workspace: Option<String>,              // UUID of active workspace
}
```

**Location:**
- Windows: `%APPDATA%/onyx/config.json`
- Linux: `~/.config/onyx/config.json`
- macOS: `~/Library/Application Support/onyx/config.json`

**Usage:**

```rust
use onyx_core::AppConfig;

// Load config
let config_path = AppConfig::get_config_path();
let mut config = AppConfig::load_from_file(&config_path)?;

// Add workspace (returns generated UUID)
let id = config.add_workspace(
    WorkspaceConfig::new("personal".to_string(), PathBuf::from("/home/user/tasks"))
);

// Set current workspace by ID
config.set_current_workspace(id)?;

// Find workspace by display name
if let Some((id, ws)) = config.find_by_name("personal") {
    println!("Found: {} at {:?}", id, ws.path);
}

// Save config
config.save_to_file(&config_path)?;
```

#### WorkspaceConfig

Configuration for a single workspace.

```rust
pub enum WorkspaceMode {
    Local,
    Webdav,
    GoogleTasks,
}

pub struct WorkspaceConfig {
    pub name: String,                                  // Display name
    pub path: PathBuf,
    pub mode: WorkspaceMode,                           // Local, Webdav, or GoogleTasks
    pub webdav_url: Option<String>,
    pub webdav_path: Option<String>,                   // User-selected remote folder path
    pub google_account: Option<String>,                // Email/display name (GoogleTasks workspaces)
    pub last_sync: Option<DateTime<Utc>>,
    pub theme: Option<String>,
    pub sync_interval_secs: Option<u64>,               // Auto-sync polling interval (focused)
    pub sync_interval_unfocused_secs: Option<u64>,     // Auto-sync interval when unfocused
}
```

## TaskRepository API

The main interface for interacting with tasks and lists.

### Initialization

```rust
use onyx_core::TaskRepository;
use std::path::PathBuf;

// Open existing repository
let repo = TaskRepository::new(PathBuf::from("/path/to/tasks"))?;

// Initialize new repository
let repo = TaskRepository::init(PathBuf::from("/path/to/tasks"))?;
```

### Task Operations

#### Create Task

```rust
let task = Task::new("My task".to_string());
let created_task = repo.create_task(list_id, task)?;
```

#### Get Task

```rust
let task = repo.get_task(list_id, task_id)?;
```

#### Update Task

```rust
let mut task = repo.get_task(list_id, task_id)?;
task.title = "Updated title".to_string();
task.complete();
repo.update_task(list_id, task)?;
```

#### Delete Task

```rust
repo.delete_task(list_id, task_id)?;
```

#### List Tasks

```rust
let tasks = repo.list_tasks(list_id)?;
```

### List Operations

#### Create List

```rust
let list = repo.create_list("My List".to_string())?;
```

#### Get Lists

```rust
let lists = repo.get_lists()?;
```

#### Get Specific List

```rust
let list = repo.get_list(list_id)?;
```

#### Delete List

```rust
repo.delete_list(list_id)?;
```

#### Rename List

```rust
repo.rename_list(list_id, "New Name".to_string())?;
```

#### Move Task Between Lists

```rust
// Atomically moves a task from one list to another.
// If the delete-from-source step fails, the copy in the destination is rolled back.
repo.move_task(from_list_id, to_list_id, task_id)?;
```

### Task Ordering

#### Reorder Task

```rust
// Move task to position 0 (first)
repo.reorder_task(list_id, task_id, 0)?;
```

#### Get Task Order

```rust
let order = repo.get_task_order(list_id)?;
// Returns: Vec<Uuid> - ordered list of task IDs
```

### Grouping

#### Enable/Disable Group by Date

```rust
// Enable grouping
repo.set_group_by_date(list_id, true)?;

// Disable grouping
repo.set_group_by_date(list_id, false)?;

// Check current setting
let is_grouped = repo.get_group_by_date(list_id)?;
```

## File Format

### Task Files

Tasks are stored as `.md` files with YAML frontmatter:

```markdown
---
id: 550e8400-e29b-41d4-a716-446655440000
status: backlog
version: 3
date: 2026-11-15T14:00:00Z
has_time: true
parent: 550e8400-e29b-41d4-a716-446655440001
---

Task description and notes go here in **markdown** format.

- Can include lists
- Rich formatting
- Links, etc.
```

The filename (without `.md`) becomes the task title.

### List Metadata

Each list folder contains a `.listdata.json` file:

```json
{
  "id": "list-uuid-1",
  "created_at": "2026-10-26T10:00:00Z",
  "updated_at": "2026-10-27T14:30:00Z",
  "group_by_date": false,
  "task_order": [
    "task-uuid-1",
    "task-uuid-2",
    "task-uuid-3"
  ]
}
```

### Root Metadata

The root folder contains a `.onyx-workspace.json` file:

```json
{
  "version": 1,
  "list_order": ["list-uuid-1", "list-uuid-2"],
  "last_opened_list": "list-uuid-1"
}
```

## WebDAV & Sync

The sync module provides bi-directional WebDAV synchronization with three-way diff, offline queuing, and platform keychain credential storage.

### Sync Functions

Sync functions live in the `onyx_core::sync` module as standalone functions (not on `TaskRepository`).

#### Sync a Workspace

```rust
use onyx_core::sync::{sync_workspace, SyncMode};
use std::path::Path;

// Full bi-directional sync
let result = sync_workspace(
    Path::new("/home/user/tasks"),
    "https://nextcloud.example.com/remote.php/dav/files/user/Tasks",
    "username",
    "password",
    SyncMode::Full,
    None, // optional progress callback
).await?;

// Push-only or pull-only
sync_workspace(path, url, user, pass, SyncMode::Push, None).await?;
sync_workspace(path, url, user, pass, SyncMode::Pull, None).await?;
```

#### Check Sync Status

```rust
use onyx_core::sync::get_sync_status;

let status = get_sync_status(Path::new("/home/user/tasks"))?;
// Returns SyncStatusInfo with last sync time, pending changes, etc.
```

### Credential Storage

Credentials are stored in the platform keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service).

**Core library** (`onyx-core::webdav`): The username is stored under service key `com.onyx.webdav.<domain>` and the password under `com.onyx.webdav.<domain>::<username>` — the `::` separator scopes the password per-username and prevents collisions when usernames contain dots. On first load, credentials stored in the legacy unscoped format (password stored without the username suffix) are automatically migrated to the scoped format.

```rust
use onyx_core::webdav::{store_credentials, load_credentials, delete_credentials};
use zeroize::Zeroizing;

// Store credentials
store_credentials("nextcloud.example.com", "username", "password")?;

// Load credentials — returns Zeroizing<String> wrappers that wipe memory on drop
let (username, password): (Zeroizing<String>, Zeroizing<String>) =
    load_credentials("nextcloud.example.com")?;

// Delete credentials
delete_credentials("nextcloud.example.com")?;
```

**Tauri GUI**: Uses `tauri-plugin-credentials` instead of direct keyring calls. This plugin provides cross-platform support: EncryptedSharedPreferences (Android Keystore) on Android, keyring crate on desktop. Plugin crate at `apps/tauri/tauri-plugin-credentials/`.

### WebDAV Client

```rust
use onyx_core::webdav::WebDavClient;

let client = WebDavClient::new(
    "https://nextcloud.example.com/remote.php/dav/files/user/Tasks",
    "username",
    "password",
)?;  // Returns Result — rejects non-HTTPS URLs

// Test connection
client.test_connection().await?;

// List remote files
let files = client.list_files("/").await?;

// Upload/download
client.put_file("My Tasks/task.md", content).await?;
let data = client.get_file("My Tasks/task.md").await?;

// Directory operations
client.ensure_dir("My Tasks").await?;
client.delete_file("old-task.md").await?;
```

### Sync Strategy

- **Three-way diff**: Compares local state, remote state, and last-known baseline to determine actions (upload, download, delete local/remote, conflict)
- **Conflict resolution**: Checksum-based — downloads remote file and compares SHA-256 checksums. Identical content is a false conflict (skipped). When different, remote wins and the local version is recovered as a duplicate task with a new UUID and `[RECOVERED FROM CONFLICT]` prefix, inserted adjacent to the original in `.listdata.json`
- **Offline queue**: Pending operations are queued and replayed when connectivity returns
- **Sync state**: Stored in `.syncstate.json` within the workspace directory
- **Auto-sync**: Periodic polling (configurable `sync_interval_secs`), debounced file-change trigger (5s), window-focus trigger (30s stale threshold)
- **Response size cap**: PROPFIND responses and file downloads are limited to 10 MB (checked via `Content-Length` header and actual body size) to prevent memory exhaustion from malicious servers
- **Path traversal protection**: Sync paths are validated to reject `..` components and backslashes anywhere in the path before any file system operation
- **Concurrent sync lock**: File-based `.sync.lock` prevents overlapping sync operations on the same workspace. Stale locks older than 5 minutes are automatically cleaned up
- **Atomic writes**: Sync state (`.syncstate.json`) and offline queue (`.syncqueue.json`) use atomic write pattern (temp file + rename, with cleanup on failure) to prevent corruption on crash
- **Delete ordering**: Delete operations update metadata before removing files, so a crash between steps leaves an orphaned file (recoverable) rather than an orphaned metadata entry
- **Syncable files**: Only processes files at expected depths — `.onyx-workspace.json` at root (depth 1), `.listdata.json` and `*.md` inside list directories (depth 2)

## Error Handling

All operations return `Result<T, Error>` where `Error` is:

```rust
pub enum Error {
    Io(io::Error),
    Serialization(String),
    NotFound(String),
    InvalidData(String),
    WorkspaceNotFound(String),
    ListNotFound(String),
    TaskNotFound(String),
    WebDav(String),
    Sync(String),
    Credential(String),
}
```

## Input Validation & Safety

### Size Limits

The storage layer enforces the following limits:

| Input | Max Length | Error |
|-------|-----------|-------|
| Task title | 500 characters | `InvalidData` |
| Task description | 1,000,000 bytes (1 MB) | `InvalidData` |
| List name | 255 characters | `InvalidData` |
| WebDAV file download | 10 MB | `WebDav` |
| PROPFIND response | 10 MB | `WebDav` |
| YAML frontmatter | 65,536 bytes (64 KB) | `InvalidData` |

### Atomic Writes

All metadata and state files use an atomic write pattern (write to `.tmp` then rename) to prevent data corruption if the process crashes mid-write. If the rename step fails, the `.tmp` file is cleaned up to prevent accumulation. Affected files:

- `.onyx-workspace.json` (root metadata)
- `.listdata.json` (list metadata)
- `config.json` (app config)
- `.syncstate.json` (sync state)
- `.syncqueue.json` (offline queue)

### Path Safety

- **List names**: Rejected if they contain `/`, `\`, or `..` components. Canonicalized and verified to stay within workspace root.
- **Sync paths**: Validated to reject `..` components and backslashes anywhere in the path before any file system operation.
- **Workspace paths** (Tauri): Rejected if they point to system directories (`/etc`, `/usr`, `/bin`, etc.).
- **Filenames**: Sanitized to replace `/ \ : * ? " < > |` and control characters with `_`.

## Example: Complete Workflow

```rust
use onyx_core::{TaskRepository, Task, AppConfig, WorkspaceConfig};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize repository
    let path = PathBuf::from("/home/user/tasks");
    let mut repo = TaskRepository::init(path.clone())?;

    // Create a list
    let list = repo.create_list("My Tasks".to_string())?;

    // Create tasks
    let task1 = Task::new("Buy groceries".to_string());
    let task1 = repo.create_task(list.id, task1)?;

    let task2 = Task::new("Call dentist".to_string())
        .with_date(chrono::Utc::now() + chrono::Duration::days(1));
    let task2 = repo.create_task(list.id, task2)?;

    // List all tasks
    let tasks = repo.list_tasks(list.id)?;
    for task in tasks {
        println!("- [{}] {}",
            if task.status == TaskStatus::Completed { "✓" } else { " " },
            task.title
        );
    }

    // Complete a task
    let mut task = repo.get_task(list.id, task1.id)?;
    task.complete();
    repo.update_task(list.id, task)?;

    // Configure workspace
    let mut config = AppConfig::new();
    let ws_id = config.add_workspace(WorkspaceConfig::new("personal".to_string(), path));
    config.set_current_workspace(ws_id)?;
    config.save_to_file(&AppConfig::get_config_path())?;

    Ok(())
}
```

## Testing

The core library includes comprehensive tests. Run them with:

```bash
cargo test -p onyx-core
```

Key test areas:
- Task CRUD operations
- List management
- Task ordering
- Markdown parsing
- Metadata persistence
- Error handling

## Thread Safety

The `Storage` trait requires `Send + Sync`, and `TaskRepository` wraps `Box<dyn Storage + Send + Sync>`, so repository instances can be shared across threads behind a `Mutex`. The Tauri GUI uses `Mutex<AppState>` for this purpose.

For concurrent access:

1. Wrap `TaskRepository` in `Mutex` or `RwLock` (the Tauri app does this)
2. Or create separate repository instances per thread (file system handles locking)
