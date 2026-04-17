use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use chrono::Utc;

#[cfg(not(target_os = "android"))]
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

use onyx_core::{
    config::{AppConfig, WorkspaceConfig, WorkspaceMode},
    google_tasks,
    models::{Task, TaskList, TaskStatus},
    repository::TaskRepository,
    sync::{self, SyncMode, SyncResult as CoreSyncResult},
    webdav,
};

// ── Google OAuth constants ───────────────────────────────────────────
// Replace these placeholder values with real credentials from Google Cloud Console.
// Desktop: "Desktop app" OAuth client type. Android: "Android" OAuth client type.
// Neither value is a security secret in the traditional sense — both can be extracted
// from the binary — but keep them out of public source control where possible.

/// Placeholder: replace with your "Desktop app" client ID from Google Cloud Console.
#[cfg(not(target_os = "android"))]
const GOOGLE_CLIENT_ID: &str = "REPLACE_WITH_DESKTOP_CLIENT_ID.apps.googleusercontent.com";

/// Desktop app client secret (required for token exchange even though not truly secret).
#[cfg(not(target_os = "android"))]
const GOOGLE_CLIENT_SECRET: &str = "REPLACE_WITH_DESKTOP_CLIENT_SECRET";

/// Placeholder: replace with your "Android" client ID from Google Cloud Console.
#[cfg(target_os = "android")]
const GOOGLE_CLIENT_ID: &str = "REPLACE_WITH_ANDROID_CLIENT_ID.apps.googleusercontent.com";
use tauri_plugin_credentials::Credentials;

#[cfg(not(target_os = "android"))]
/// Active file watcher stored globally so it lives for the app lifetime.
static WATCHER: Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>> =
    Mutex::new(None);

#[cfg(not(target_os = "android"))]
/// Shared mute timestamp — set before writes, checked by the watcher.
static LAST_WRITE: Mutex<Option<Instant>> = Mutex::new(None);

/// Shared application state behind a mutex.
struct AppState {
    config: AppConfig,
    config_path: PathBuf,
    app_data_dir: PathBuf,
    repo: Option<TaskRepository>,
}

/// Lock the AppState mutex, converting poisoned locks into an error string.
fn lock_state(state: &Mutex<AppState>) -> Result<std::sync::MutexGuard<'_, AppState>, String> {
    state.lock().map_err(|e| format!("State lock poisoned: {}", e))
}

impl AppState {
    /// Persist config to disk, converting errors to String for Tauri commands.
    fn save_config(&self) -> Result<(), String> {
        self.config.save_to_file(&self.config_path).map_err(|e| e.to_string())
    }
}

/// Validate that a workspace path is a reasonable directory and not a system path.
fn validate_workspace_path(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    // Reject obviously dangerous paths
    let normalized = p.to_string_lossy();
    if normalized.is_empty() {
        return Err("Workspace path cannot be empty".into());
    }
    // Reject paths that are system root directories
    #[cfg(unix)]
    {
        let forbidden = ["/", "/etc", "/usr", "/bin", "/sbin", "/var", "/proc", "/sys", "/dev"];
        let canonical = normalized.trim_end_matches('/');
        if forbidden.contains(&canonical) {
            return Err(format!("Cannot use system directory as workspace: {}", path));
        }
    }
    #[cfg(windows)]
    {
        let upper = normalized.to_uppercase();
        if upper.len() <= 3 && (upper.ends_with(":\\") || upper.ends_with(":")) {
            return Err(format!("Cannot use drive root as workspace: {}", path));
        }
    }
    Ok(())
}

/// Serializable sync result for the frontend.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct SyncResult {
    uploaded: u32,
    downloaded: u32,
    deleted_local: u32,
    deleted_remote: u32,
    conflicts: u32,
    errors: Vec<String>,
}

impl From<CoreSyncResult> for SyncResult {
    fn from(r: CoreSyncResult) -> Self {
        Self {
            uploaded: r.uploaded,
            downloaded: r.downloaded,
            deleted_local: r.deleted_local,
            deleted_remote: r.deleted_remote,
            conflicts: r.conflicts,
            errors: r.errors,
        }
    }
}

/// Suppress file watcher events for the next second (call before writes).
#[cfg(not(target_os = "android"))]
fn mute_watcher(_state: &mut AppState) {
    if let Ok(mut t) = LAST_WRITE.lock() {
        *t = Some(Instant::now());
    }
}

#[cfg(target_os = "android")]
fn mute_watcher(_state: &mut AppState) {}

/// Helper: get or open a TaskRepository for the current workspace.
/// Safe against double-init because it runs under the AppState Mutex and uses
/// get_or_insert to atomically check-and-set.
fn ensure_repo(state: &mut AppState) -> Result<(), String> {
    if state.repo.is_some() {
        return Ok(());
    }
    let (_name, ws) = state
        .config
        .get_current_workspace()
        .map_err(|e| e.to_string())?;
    let path = ws.path.clone();
    // Use a separate variable to avoid borrow issues — the Mutex ensures
    // no concurrent access, so TOCTOU is not possible here.
    let repo = TaskRepository::new(path).map_err(|e| e.to_string())?;
    state.repo = Some(repo);
    Ok(())
}

/// Get an immutable reference to the repo, returning an error if not initialized.
fn repo_ref(state: &AppState) -> Result<&TaskRepository, String> {
    state.repo.as_ref().ok_or_else(|| "Repository not initialized".to_string())
}

/// Get a mutable reference to the repo, returning an error if not initialized.
fn repo_mut(state: &mut AppState) -> Result<&mut TaskRepository, String> {
    state.repo.as_mut().ok_or_else(|| "Repository not initialized".to_string())
}

// ── Config commands ──────────────────────────────────────────────────

#[tauri::command]
fn get_config(state: State<'_, Mutex<AppState>>) -> Result<AppConfig, String> {
    let s = lock_state(&state)?;
    Ok(s.config.clone())
}

#[tauri::command]
fn save_config(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let s = lock_state(&state)?;
    s.save_config()
}

#[tauri::command]
fn add_workspace(
    name: String,
    path: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    validate_workspace_path(&path)?;
    // Ensure the path exists and is a valid workspace before persisting the
    // config. Without this, calling add_workspace directly on a missing
    // directory would save the workspace but every subsequent ensure_repo
    // call would fail with "Path does not exist".
    TaskRepository::init(PathBuf::from(&path))
        .map(|_| ())
        .map_err(|e| e.to_string())?;
    let mut s = lock_state(&state)?;
    let ws = WorkspaceConfig::new(name, PathBuf::from(&path));
    let id = s.config.add_workspace(ws);
    s.config
        .set_current_workspace(id)
        .map_err(|e| e.to_string())?;
    s.repo = None;
    s.save_config()
}

#[tauri::command]
fn set_current_workspace(
    id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    s.config
        .set_current_workspace(id)
        .map_err(|e| e.to_string())?;
    s.repo = None;
    s.save_config()
}

#[tauri::command]
fn remove_workspace(
    id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    s.config.remove_workspace(&id);
    s.repo = None;
    s.save_config()
}

#[tauri::command]
async fn rename_workspace(
    id: String,
    new_name: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    // Extract workspace info while holding the lock briefly
    let (mode, old_path, webdav_url, webdav_path) = {
        let s = lock_state(&state)?;
        let ws = s.config.workspaces.get(&id).ok_or("Workspace not found")?;
        (
            ws.mode.clone(),
            ws.path.clone(),
            ws.webdav_url.clone(),
            ws.webdav_path.clone(),
        )
    };

    match mode {
        WorkspaceMode::Local => {
            // Rename the local folder
            let parent = old_path.parent().ok_or("Workspace has no parent directory")?;
            let new_path = parent.join(&new_name);
            if new_path != old_path {
                if new_path.exists() {
                    return Err(format!("A folder named '{}' already exists at that location", new_name));
                }
                std::fs::rename(&old_path, &new_path).map_err(|e| format!("Failed to rename folder: {}", e))?;
            }
            let mut s = lock_state(&state)?;
            s.config.rename_workspace(&id, new_name).map_err(|e| e.to_string())?;
            if let Some(ws) = s.config.workspaces.get_mut(&id) {
                ws.path = new_path;
            }
            s.repo = None;
            s.save_config()?;
        }
        WorkspaceMode::Webdav => {
            // Rename the remote folder via WebDAV MOVE
            let base_url = webdav_url.as_deref().ok_or("No WebDAV URL configured")?;
            let remote_path = webdav_path.as_deref().unwrap_or("");

            let domain = base_url
                .split("://").nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or("").to_string();
            let creds = app_handle.state::<Credentials<tauri::Wry>>();
            let (username, password) = creds.load(&domain)?;

            let client = webdav::WebDavClient::new(base_url, &username, &password)
                .map_err(|e| e.to_string())?;

            // Compute new remote path by replacing the last segment
            let new_remote_path = if remote_path.is_empty() || remote_path == "/" {
                new_name.clone()
            } else if let Some(parent) = remote_path.trim_end_matches('/').rsplit_once('/') {
                format!("{}/{}", parent.0, new_name)
            } else {
                new_name.clone()
            };

            if new_remote_path != remote_path {
                client.move_resource(remote_path, &new_remote_path).await.map_err(|e| e.to_string())?;
            }

            let mut s = lock_state(&state)?;
            s.config.rename_workspace(&id, new_name).map_err(|e| e.to_string())?;
            if let Some(ws) = s.config.workspaces.get_mut(&id) {
                ws.webdav_path = Some(new_remote_path);
            }
            s.repo = None;
            s.save_config()?;
        }
        WorkspaceMode::GoogleTasks => {
            // Google Tasks workspaces: local cache path is app-managed; only update display name.
            let mut s = lock_state(&state)?;
            s.config.rename_workspace(&id, new_name).map_err(|e| e.to_string())?;
            s.save_config()?;
        }
    }

    Ok(())
}

// ── Workspace init ───────────────────────────────────────────────────

#[tauri::command]
fn init_workspace(path: String) -> Result<(), String> {
    validate_workspace_path(&path)?;
    TaskRepository::init(PathBuf::from(path))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ── List commands ────────────────────────────────────────────────────

#[tauri::command]
fn get_lists(state: State<'_, Mutex<AppState>>) -> Result<Vec<TaskList>, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    repo_ref(&s)?
        .get_lists()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_list(
    name: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<TaskList, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    repo_mut(&mut s)?
        .create_list(name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_list(
    list_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .delete_list(id)
        .map_err(|e| e.to_string())
}

// ── Task commands ────────────────────────────────────────────────────

#[tauri::command]
fn list_tasks(
    list_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<Task>, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_ref(&s)?
        .list_tasks(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_task(
    list_id: String,
    title: String,
    description: Option<String>,
    parent_id: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<Task, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut task = Task::new(title);
    if let Some(desc) = description.filter(|d| !d.is_empty()) {
        task.description = desc;
    }
    if let Some(pid) = parent_id {
        let parent_uuid = Uuid::parse_str(&pid).map_err(|e| e.to_string())?;
        task.parent_id = Some(parent_uuid);
    }
    repo_mut(&mut s)?
        .create_task(id, task)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_task(
    list_id: String,
    task: Task,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .update_task(id, task)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_task(
    list_id: String,
    task_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let lid = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    let repo = repo_mut(&mut s)?;
    // Cascade-delete subtasks first
    let all_tasks = repo.list_tasks(lid).map_err(|e| e.to_string())?;
    let child_ids: Vec<Uuid> = all_tasks
        .iter()
        .filter(|t| t.parent_id == Some(tid))
        .map(|t| t.id)
        .collect();
    for child_id in child_ids {
        repo.delete_task(lid, child_id).map_err(|e| format!("Failed to delete subtask {}: {}", child_id, e))?;
    }
    repo.delete_task(lid, tid)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_task(
    list_id: String,
    task_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Task, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let lid = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    let repo = repo_mut(&mut s)?;
    let mut task = repo.get_task(lid, tid).map_err(|e| e.to_string())?;
    match task.status {
        TaskStatus::Backlog => task.complete(),
        TaskStatus::Completed => task.uncomplete(),
    }
    repo.update_task(lid, task.clone())
        .map_err(|e| e.to_string())?;
    // Cascade: complete/uncomplete subtasks to match parent
    let all_tasks = repo.list_tasks(lid).map_err(|e| e.to_string())?;
    for mut child in all_tasks.into_iter().filter(|t| t.parent_id == Some(tid)) {
        if child.status != task.status {
            match task.status {
                TaskStatus::Backlog => child.uncomplete(),
                TaskStatus::Completed => child.complete(),
            }
            let _ = repo.update_task(lid, child);
        }
    }
    Ok(task)
}

#[tauri::command]
fn reorder_task(
    list_id: String,
    task_id: String,
    new_position: usize,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let lid = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .reorder_task(lid, tid, new_position)
        .map_err(|e| e.to_string())
}

// ── Move / rename / grouping ────────────────────────────────────────

#[tauri::command]
fn move_task(
    from_list_id: String,
    to_list_id: String,
    task_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let from = Uuid::parse_str(&from_list_id).map_err(|e| e.to_string())?;
    let to = Uuid::parse_str(&to_list_id).map_err(|e| e.to_string())?;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .move_task(from, to, tid)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_list(
    list_id: String,
    new_name: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .rename_list(id, new_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_group_by_date(
    list_id: String,
    enabled: bool,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    mute_watcher(&mut s);
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_mut(&mut s)?
        .set_group_by_date(id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_group_by_date(
    list_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<bool, String> {
    let mut s = lock_state(&state)?;
    ensure_repo(&mut s)?;
    let id = Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    repo_ref(&s)?
        .get_group_by_date(id)
        .map_err(|e| e.to_string())
}

// ── Sync commands ────────────────────────────────────────────────────

#[tauri::command]
fn set_webdav_config(
    workspace_id: String,
    webdav_url: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    let ws = s.config.workspaces.get_mut(&workspace_id)
        .ok_or_else(|| format!("Workspace '{}' not found", workspace_id))?;
    ws.webdav_url = Some(webdav_url);
    s.save_config()
}

#[tauri::command]
fn set_workspace_theme(
    workspace_id: String,
    theme: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    let ws = s.config.workspaces.get_mut(&workspace_id)
        .ok_or_else(|| format!("Workspace '{}' not found", workspace_id))?;
    ws.theme = theme;
    s.save_config()
}

#[tauri::command]
fn set_sync_interval(
    workspace_id: String,
    interval_secs: Option<u64>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    let ws = s.config.workspaces.get_mut(&workspace_id)
        .ok_or_else(|| format!("Workspace '{}' not found", workspace_id))?;
    ws.sync_interval_secs = interval_secs;
    s.save_config()
}

#[tauri::command]
fn set_sync_interval_unfocused(
    workspace_id: String,
    interval_secs: Option<u64>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    let ws = s.config.workspaces.get_mut(&workspace_id)
        .ok_or_else(|| format!("Workspace '{}' not found", workspace_id))?;
    ws.sync_interval_unfocused_secs = interval_secs;
    s.save_config()
}

/// A remote folder entry returned to the frontend.
#[derive(Debug, Serialize, Deserialize)]
struct RemoteFolderEntry {
    name: String,
    is_workspace: bool,
}

/// Summary of a list inside a remote workspace.
#[derive(Debug, Serialize, Deserialize)]
struct RemoteListInfo {
    name: String,
    task_count: usize,
}

#[tauri::command]
async fn list_remote_folder(
    url: String,
    username: String,
    password: String,
    path: String,
) -> Result<Vec<RemoteFolderEntry>, String> {
    let client = onyx_core::webdav::WebDavClient::new(&url, &username, &password)
        .map_err(|e| e.to_string())?;
    let entries = client.list_files(&path).await.map_err(|e| e.to_string())?;

    let dir_entries: Vec<_> = entries.into_iter().filter(|e| e.is_dir).collect();

    // Check all subfolders for .onyx-workspace.json in parallel
    let sub_paths: Vec<_> = dir_entries.iter().map(|entry| {
        if path.is_empty() { entry.path.clone() }
        else { format!("{}/{}", path.trim_end_matches('/'), entry.path) }
    }).collect();
    let checks: Vec<_> = sub_paths.iter().map(|sp| {
        client.list_files(sp)
    }).collect();
    let results: Vec<_> = futures::future::join_all(checks).await
        .into_iter().map(|r| r.unwrap_or_else(|e| {
            eprintln!("Warning: failed to inspect remote subfolder: {}", e);
            Vec::new()
        })).collect();

    let folders = dir_entries.into_iter().zip(results).map(|(entry, sub_files)| {
        let is_workspace = sub_files.iter().any(|f| !f.is_dir && f.path == ".onyx-workspace.json");
        RemoteFolderEntry { name: entry.path, is_workspace }
    }).collect();

    Ok(folders)
}

#[tauri::command]
async fn inspect_remote_workspace(
    url: String,
    username: String,
    password: String,
    path: String,
) -> Result<Vec<RemoteListInfo>, String> {
    let client = onyx_core::webdav::WebDavClient::new(&url, &username, &password)
        .map_err(|e| e.to_string())?;
    let entries = client.list_files(&path).await.map_err(|e| e.to_string())?;

    let mut lists = Vec::new();
    for entry in entries {
        if !entry.is_dir { continue; }
        let list_path = if path.is_empty() {
            entry.path.clone()
        } else {
            format!("{}/{}", path.trim_end_matches('/'), entry.path)
        };
        let files = client.list_files(&list_path).await.unwrap_or_else(|e| {
            eprintln!("Warning: failed to list remote folder '{}': {}", list_path, e);
            Vec::new()
        });
        let has_listdata = files.iter().any(|f| !f.is_dir && f.path == ".listdata.json");
        if has_listdata {
            let task_count = files.iter().filter(|f| !f.is_dir && f.path.ends_with(".md")).count();
            lists.push(RemoteListInfo {
                name: entry.path,
                task_count,
            });
        }
    }

    Ok(lists)
}

#[tauri::command]
async fn create_remote_workspace(
    url: String,
    username: String,
    password: String,
    path: String,
) -> Result<(), String> {
    let client = onyx_core::webdav::WebDavClient::new(&url, &username, &password)
        .map_err(|e| e.to_string())?;
    if !path.is_empty() {
        client.ensure_dir(&path).await.map_err(|e| e.to_string())?;
    }
    // Upload an empty .onyx-workspace.json
    let metadata = serde_json::json!({
        "version": 1,
        "list_order": [],
        "last_opened_list": null,
    });
    let file_path = if path.is_empty() {
        ".onyx-workspace.json".to_string()
    } else {
        format!("{}/{}", path.trim_end_matches('/'), ".onyx-workspace.json")
    };
    client.put_file(&file_path, serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?.into_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn add_webdav_workspace(
    name: String,
    webdav_url: String,
    webdav_path: String,
    username: String,
    password: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = lock_state(&state)?;
    // Use a UUID-based directory name to avoid filesystem conflicts with duplicate workspace names
    let dir_id = uuid::Uuid::new_v4().to_string();
    let managed_dir = s.app_data_dir.join("workspaces").join(&dir_id);
    std::fs::create_dir_all(&managed_dir).map_err(|e| e.to_string())?;
    TaskRepository::init(managed_dir.clone()).map(|_| ()).map_err(|e| e.to_string())?;

    let mut ws = WorkspaceConfig::new(name, managed_dir);
    ws.mode = WorkspaceMode::Webdav;
    ws.webdav_url = Some(webdav_url.clone());
    ws.webdav_path = Some(webdav_path);

    let id = s.config.add_workspace(ws);
    s.config.set_current_workspace(id).map_err(|e| e.to_string())?;
    s.repo = None;

    // Store credentials keyed by hostname
    let domain = webdav_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .to_string();
    s.save_config()?;
    drop(s);
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    creds.store(&domain, &username, &password)?;
    Ok(())
}

#[tauri::command]
async fn store_credentials(
    domain: String,
    username: String,
    password: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    creds.store(&domain, &username, &password)
}

#[tauri::command]
async fn load_credentials(
    domain: String,
    app_handle: tauri::AppHandle,
) -> Result<(String, String), String> {
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    creds.load(&domain)
}

#[tauri::command]
async fn test_webdav_connection(
    url: String,
    username: String,
    password: String,
) -> Result<(), String> {
    let client = onyx_core::webdav::WebDavClient::new(&url, &username, &password)
        .map_err(|e| e.to_string())?;
    client
        .test_connection()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_workspace(
    workspace_id: String,
    mode: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<SyncResult, String> {
    // Step 1: read config — combine base URL with the user-chosen remote path
    let (workspace_path, webdav_url) = {
        let s = lock_state(&state)?;
        let ws = s.config.workspaces.get(&workspace_id)
            .ok_or("Workspace not found")?;
        let base = ws.webdav_url.clone().ok_or("No WebDAV URL configured")?;
        let full = match &ws.webdav_path {
            Some(p) if !p.is_empty() => format!("{}/{}", base.trim_end_matches('/'), p.trim_matches('/')),
            _ => base,
        };
        (ws.path.clone(), full)
    };

    // Step 2: load credentials
    let domain = webdav_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .to_string();
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    let (username, password) = creds.load(&domain)?;

    let sync_mode = match mode.as_str() {
        "push" => SyncMode::Push,
        "pull" => SyncMode::Pull,
        _ => SyncMode::Full,
    };
    let result = sync::sync_workspace(
        &workspace_path,
        &webdav_url,
        &username,
        &password,
        sync_mode,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    {
        let mut s = lock_state(&state)?;
        // Suppress file watcher events from sync-written files (500ms debounce + margin)
        mute_watcher(&mut s);
        if let Some(ws) = s.config.workspaces.get_mut(&workspace_id) {
            ws.last_sync = Some(Utc::now());
        }
        s.save_config()?;
    }

    Ok(result.into())
}

// ── Google Tasks OAuth + workspace ──────────────────────────────────

/// Returned to the frontend after a successful Google OAuth flow.
#[derive(Debug, Serialize, Deserialize)]
struct GoogleAuthResult {
    access_token: String,
    refresh_token: String,
    /// Display name or email for the connected account.
    account: String,
}

/// Desktop-only: run the PKCE OAuth 2.0 Authorization Code flow using a temporary
/// loopback HTTP server. Opens the system browser, waits for the redirect with the
/// auth code, exchanges it for tokens, and returns them.
///
/// On Android this is a stub — the Kotlin layer handles OAuth via Credential Manager.
#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn start_google_oauth() -> Result<GoogleAuthResult, String> {
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // ── PKCE code verifier + challenge ───────────────────────────────
    // Build 64 random bytes from four UUID v4 values (each contributes 122 bits of
    // randomness). Encode as base64url to produce a valid code verifier.
    let rand_bytes: Vec<u8> = (0..4)
        .flat_map(|_| uuid::Uuid::new_v4().as_bytes().to_vec())
        .collect();
    let verifier = base64url_encode(&rand_bytes);

    let challenge_bytes = Sha256::digest(verifier.as_bytes());
    let challenge = base64url_encode(&challenge_bytes);

    // ── Loopback listener ────────────────────────────────────────────
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind loopback listener: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{}", port);

    // ── Build auth URL ───────────────────────────────────────────────
    let scope = "https://www.googleapis.com/auth/tasks.readonly \
                 https://www.googleapis.com/auth/userinfo.email";
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?client_id={client_id}\
         &redirect_uri={redirect_uri}\
         &response_type=code\
         &scope={scope}\
         &code_challenge={challenge}\
         &code_challenge_method=S256\
         &access_type=offline\
         &prompt=consent",
        client_id = GOOGLE_CLIENT_ID,
        redirect_uri = urlencodeq(&redirect_uri),
        scope = urlencodeq(scope),
        challenge = challenge,
    );

    // Open system browser
    open_browser(&auth_url);

    // ── Accept one connection on the loopback server ─────────────────
    let (mut stream, _) = listener.accept().await
        .map_err(|e| format!("Failed to accept OAuth callback: {}", e))?;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await
        .map_err(|e| format!("Failed to read OAuth callback request: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the request line: "GET /?code=...&state=... HTTP/1.1"
    let code = request.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| {
            path.split('?').nth(1).and_then(|qs| {
                qs.split('&')
                    .find(|p| p.starts_with("code="))
                    .and_then(|p| p.strip_prefix("code="))
                    .map(|s| s.to_string())
            })
        })
        .ok_or_else(|| "OAuth callback did not contain an authorization code".to_string())?;

    // Return a simple success page to the browser
    let html_response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body style='font-family:sans-serif;text-align:center;padding-top:4rem'>\
        <h2>Connected!</h2><p>You can close this tab and return to Onyx.</p>\
        </body></html>";
    let _ = stream.write_all(html_response.as_bytes()).await;

    // ── Exchange code for tokens ─────────────────────────────────────
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let params = [
        ("client_id", GOOGLE_CLIENT_ID),
        ("client_secret", GOOGLE_CLIENT_SECRET),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("grant_type", "authorization_code"),
        ("code_verifier", &verifier),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", body));
    }

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
    }

    let token_resp: TokenResponse = resp.json().await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;
    let refresh_token = token_resp.refresh_token
        .ok_or_else(|| "Google did not return a refresh token — try revoking access and reconnecting".to_string())?;

    // ── Fetch account email ──────────────────────────────────────────
    let userinfo_resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&token_resp.access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch user info: {}", e))?;

    #[derive(serde::Deserialize)]
    struct UserInfo {
        #[serde(default)]
        email: String,
    }

    let account = if userinfo_resp.status().is_success() {
        userinfo_resp.json::<UserInfo>().await
            .map(|u| u.email)
            .unwrap_or_default()
    } else {
        String::new()
    };

    Ok(GoogleAuthResult {
        access_token: token_resp.access_token,
        refresh_token,
        account,
    })
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn start_google_oauth() -> Result<GoogleAuthResult, String> {
    // On Android, OAuth is handled by the Kotlin layer via Credential Manager.
    // This stub exists only so the command is registered on all platforms.
    Err("Android OAuth must be initiated via the native sign-in flow".to_string())
}

/// Create a new Google Tasks workspace: provision a local cache directory,
/// store OAuth credentials, run the initial sync, and make it the active workspace.
#[tauri::command]
async fn add_google_tasks_workspace(
    name: String,
    access_token: String,
    refresh_token: String,
    account: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let managed_dir = {
        let s = lock_state(&state)?;
        let dir_id = uuid::Uuid::new_v4().to_string();
        s.app_data_dir.join("google-tasks").join(&dir_id)
    };

    std::fs::create_dir_all(&managed_dir).map_err(|e| e.to_string())?;

    // Run initial sync before registering the workspace so the user sees content immediately.
    google_tasks::sync_google_tasks(&managed_dir, &access_token)
        .await
        .map_err(|e| e.to_string())?;

    let mut s = lock_state(&state)?;
    let mut ws = WorkspaceConfig::new(name, managed_dir.clone());
    ws.mode = WorkspaceMode::GoogleTasks;
    ws.google_account = if account.is_empty() { None } else { Some(account.clone()) };
    ws.last_sync = Some(Utc::now());

    let id = s.config.add_workspace(ws);
    s.config.set_current_workspace(id.clone()).map_err(|e| e.to_string())?;
    s.repo = None;
    s.save_config()?;
    drop(s);

    // Store refresh token: domain = "google-oauth-{workspace_id}", username = account, password = refresh_token
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    let cred_key = format!("google-oauth-{}", id);
    creds.store(&cred_key, &account, &refresh_token)?;

    Ok(())
}

/// Sync a Google Tasks workspace: refresh the access token, then pull all remote changes.
#[tauri::command]
async fn sync_google_tasks_workspace(
    workspace_id: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<SyncResult, String> {
    let workspace_path = {
        let s = lock_state(&state)?;
        s.config.workspaces.get(&workspace_id)
            .ok_or("Workspace not found")?
            .path
            .clone()
    };

    // Load the stored refresh token.
    let creds = app_handle.state::<Credentials<tauri::Wry>>();
    let cred_key = format!("google-oauth-{}", workspace_id);
    let (_account, refresh_token) = creds.load(&cred_key)?;

    // Refresh to get a fresh access token.
    #[cfg(not(target_os = "android"))]
    let access_token = google_tasks::refresh_access_token(
        GOOGLE_CLIENT_ID,
        Some(GOOGLE_CLIENT_SECRET),
        &refresh_token,
    )
    .await
    .map_err(|e| e.to_string())?;

    #[cfg(target_os = "android")]
    let access_token = google_tasks::refresh_access_token(
        GOOGLE_CLIENT_ID,
        None,
        &refresh_token,
    )
    .await
    .map_err(|e| e.to_string())?;

    let result = google_tasks::sync_google_tasks(&workspace_path, &access_token)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut s = lock_state(&state)?;
        mute_watcher(&mut s);
        if let Some(ws) = s.config.workspaces.get_mut(&workspace_id) {
            ws.last_sync = Some(Utc::now());
        }
        s.save_config()?;
    }

    Ok(SyncResult {
        uploaded: 0,
        downloaded: result.downloaded,
        deleted_local: 0,
        deleted_remote: 0,
        conflicts: 0,
        errors: result.errors,
    })
}

// ── OAuth helpers (desktop only) ─────────────────────────────────────

#[cfg(not(target_os = "android"))]
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdg-open").arg(url).spawn(); }
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(url).spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn(); }
}

/// Percent-encode a string for use in a URL query parameter value.
#[cfg(not(target_os = "android"))]
fn urlencodeq(s: &str) -> String {
    s.bytes().flat_map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'_' | b'.' | b'~' => vec![b as char],
        _ => format!("%{:02X}", b).chars().collect(),
    }).collect()
}

/// Encode bytes as base64url (RFC 4648 §5, no padding).
#[cfg(not(target_os = "android"))]
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((data.len() * 4 + 2) / 3);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 { out.push(CHARS[((n >> 6) & 0x3F) as usize] as char); }
        if chunk.len() > 2 { out.push(CHARS[(n & 0x3F) as usize] as char); }
    }
    out
}

// ── File watcher ────────────────────────────────────────────────────

#[cfg(not(target_os = "android"))]
fn start_watcher(handle: tauri::AppHandle, path: PathBuf) {
    // Stop any existing watcher before starting a new one
    if let Ok(mut w) = WATCHER.lock() {
        *w = None;
    }
    let handle = handle.clone();
    let debouncer = new_debouncer(
        std::time::Duration::from_millis(500),
        move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            let Ok(events) = events else {
                let err = events.unwrap_err();
                eprintln!("File watcher error: {:?}", err);
                let _ = handle.emit("watcher-error", format!("{}", err));
                return;
            };
            // Only care about data file changes
            let has_data_change = events.iter().any(|e| {
                if e.kind != DebouncedEventKind::Any { return false; }
                let p = e.path.to_string_lossy();
                p.ends_with(".md") || p.ends_with(".json")
            });
            if !has_data_change { return; }
            // Skip if we wrote recently (self-change suppression)
            if let Ok(guard) = LAST_WRITE.lock() {
                if let Some(t) = *guard {
                    if t.elapsed() < std::time::Duration::from_secs(1) { return; }
                }
            }
            let _ = handle.emit("fs-changed", ());
        },
    );
    match debouncer {
        Ok(mut d) => {
            if let Err(e) = d.watcher().watch(&path, notify::RecursiveMode::Recursive) {
                eprintln!("Failed to watch path {}: {e}", path.display());
            }
            if let Ok(mut w) = WATCHER.lock() {
                *w = Some(d);
            }
        }
        Err(e) => eprintln!("Failed to start file watcher: {e}"),
    }
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn watch_workspace(path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    start_watcher(app_handle, PathBuf::from(path));
    Ok(())
}

#[cfg(target_os = "android")]
#[tauri::command]
fn watch_workspace(_path: String, _app_handle: tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

// ── App entry ────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_credentials::init())
        .setup(|app| {
            // Resolve app data dir and config path
            let app_data_dir = app.path().app_data_dir()
                .map_err(|e| format!("Failed to get app data dir: {}", e))?;
            let config_path = {
                #[cfg(target_os = "android")]
                { app_data_dir.join("config.json") }
                #[cfg(not(target_os = "android"))]
                { AppConfig::get_config_path() }
            };
            let config = AppConfig::load_from_file(&config_path).unwrap_or_default();
            let workspace_path = config.get_current_workspace().ok().map(|(_, ws)| ws.path.clone());
            app.manage(Mutex::new(AppState { config, config_path, app_data_dir, repo: None }));

            #[cfg(not(target_os = "android"))]
            if let Some(path) = workspace_path {
                let handle = app.handle().clone();
                start_watcher(handle, path);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            add_workspace,
            set_current_workspace,
            remove_workspace,
            rename_workspace,
            init_workspace,
            get_lists,
            create_list,
            delete_list,
            list_tasks,
            create_task,
            update_task,
            delete_task,
            toggle_task,
            reorder_task,
            move_task,
            rename_list,
            set_group_by_date,
            get_group_by_date,
            set_webdav_config,
            set_workspace_theme,
            set_sync_interval,
            set_sync_interval_unfocused,
            add_webdav_workspace,
            list_remote_folder,
            inspect_remote_workspace,
            create_remote_workspace,
            store_credentials,
            load_credentials,
            test_webdav_connection,
            sync_workspace,
            watch_workspace,
            start_google_oauth,
            add_google_tasks_workspace,
            sync_google_tasks_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
