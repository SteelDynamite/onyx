use std::collections::HashMap;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use uuid::Uuid;
use crate::error::{Error, Result};
use crate::storage::{ListMetadata, TaskFrontmatter};
use crate::webdav::WebDavClient;

// --- Sync State ---

/// Persisted sync state for a workspace, stored as `.syncstate.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    pub last_sync: Option<DateTime<Utc>>,
    pub files: HashMap<String, SyncFileEntry>,
}

/// Entry tracking the last-synced state of a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFileEntry {
    pub checksum: String,
    pub modified_at: Option<String>,
    pub size: u64,
}

// --- Sync Actions ---

/// An action to take during sync, computed from the three-way diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Upload { path: String },
    Download { path: String },
    DeleteLocal { path: String },
    DeleteRemote { path: String },
    Conflict { path: String },
}

impl SyncAction {
    pub fn path(&self) -> &str {
        match self {
            SyncAction::Upload { path }
            | SyncAction::Download { path }
            | SyncAction::DeleteLocal { path }
            | SyncAction::DeleteRemote { path }
            | SyncAction::Conflict { path } => path,
        }
    }
}

/// Result summary of a sync operation.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub uploaded: u32,
    pub downloaded: u32,
    pub deleted_local: u32,
    pub deleted_remote: u32,
    pub conflicts: u32,
    pub errors: Vec<String>,
}

/// Sync direction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Push,
    Pull,
    Full,
}

// --- Local / Remote file info for diffing ---

/// Snapshot of a local file's state.
#[derive(Debug, Clone)]
pub struct LocalFileInfo {
    pub path: String,
    pub checksum: String,
    pub modified_at: Option<String>,
    pub size: u64,
}

/// Snapshot of a remote file's state (from PROPFIND).
#[derive(Debug, Clone)]
pub struct RemoteFileSnapshot {
    pub path: String,
    pub last_modified: Option<String>,
    pub size: u64,
}

// --- Three-way diff ---

/// Compute sync actions by comparing local files, remote files, and the last-synced base state.
///
/// Three-way diff logic:
/// | Local vs Base | Remote vs Base | Action                                      |
/// |---------------|----------------|---------------------------------------------|
/// | unchanged     | unchanged      | skip                                        |
/// | added         | absent         | upload                                      |
/// | absent        | added          | download                                    |
/// | modified      | unchanged      | upload                                      |
/// | unchanged     | modified       | download                                    |
/// | deleted       | unchanged      | delete remote                               |
/// | unchanged     | deleted        | delete local                                |
/// | modified      | modified       | last-write-wins (compare timestamps)        |
/// | deleted       | modified       | download (remote wins)                      |
/// | modified      | deleted        | upload (local wins)                         |
/// | added         | added          | last-write-wins                             |
pub fn compute_sync_actions(
    local_files: &[LocalFileInfo],
    remote_files: &[RemoteFileSnapshot],
    sync_state: &SyncState,
) -> Vec<SyncAction> {
    let local_map: HashMap<&str, &LocalFileInfo> = local_files.iter().map(|f| (f.path.as_str(), f)).collect();
    let remote_map: HashMap<&str, &RemoteFileSnapshot> = remote_files.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut all_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for f in local_files { all_paths.insert(&f.path); }
    for f in remote_files { all_paths.insert(&f.path); }
    for p in sync_state.files.keys() { all_paths.insert(p); }

    let mut actions = Vec::new();

    for path in all_paths {
        let local = local_map.get(path);
        let remote = remote_map.get(path);
        let base = sync_state.files.get(path);

        match (local, remote, base) {
            // Both present, base known: check for changes
            (Some(l), Some(r), Some(b)) => {
                let local_changed = l.checksum != b.checksum;
                // Compare remote vs base using parsed timestamps to avoid format mismatches
                let remote_changed = r.size != b.size || !timestamps_equal(r.last_modified.as_deref(), b.modified_at.as_deref());

                match (local_changed, remote_changed) {
                    (false, false) => {} // Skip, unchanged
                    (true, false) => actions.push(SyncAction::Upload { path: path.to_string() }),
                    (false, true) => actions.push(SyncAction::Download { path: path.to_string() }),
                    (true, true) => {
                        actions.push(SyncAction::Conflict { path: path.to_string() });
                    }
                }
            }

            // Local only, no base: added locally
            (Some(_), None, None) => {
                actions.push(SyncAction::Upload { path: path.to_string() });
            }

            // Remote only, no base: added remotely
            (None, Some(_), None) => {
                actions.push(SyncAction::Download { path: path.to_string() });
            }

            // Both present, no base (both added): conflict
            (Some(_), Some(_), None) => {
                actions.push(SyncAction::Conflict { path: path.to_string() });
            }

            // Local present, remote gone, base known: remote was deleted
            (Some(l), None, Some(b)) => {
                let local_changed = l.checksum != b.checksum;
                if local_changed {
                    // modified locally + deleted remotely -> upload (local wins)
                    actions.push(SyncAction::Upload { path: path.to_string() });
                } else {
                    // unchanged locally + deleted remotely -> delete local
                    actions.push(SyncAction::DeleteLocal { path: path.to_string() });
                }
            }

            // Remote present, local gone, base known: local was deleted
            (None, Some(_), Some(b)) => {
                let remote_changed = remote.is_some_and(|r| r.size != b.size || !timestamps_equal(r.last_modified.as_deref(), b.modified_at.as_deref()));
                if remote_changed {
                    // deleted locally + modified remotely -> download (remote wins)
                    actions.push(SyncAction::Download { path: path.to_string() });
                } else {
                    // deleted locally, remote unchanged -> delete remote
                    actions.push(SyncAction::DeleteRemote { path: path.to_string() });
                }
            }

            // Both gone, base known: both deleted, skip (clean up base)
            (None, None, Some(_)) => {}

            // Local gone, remote gone, no base: nothing to do
            (None, None, None) => {}

        }
    }

    // Sort actions for deterministic output
    actions.sort_by(|a, b| a.path().cmp(b.path()));
    actions
}

/// Compare two timestamps for equality by parsing both, tolerating format differences.
fn timestamps_equal(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            // Try string equality first (fast path)
            if a == b { return true; }
            // Parse both and compare as DateTime
            match (parse_timestamp(a), parse_timestamp(b)) {
                (Some(ta), Some(tb)) => ta == tb,
                _ => false,
            }
        }
        _ => false,
    }
}

/// Parse a timestamp string (ISO 8601 or HTTP date format).
fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 / RFC 3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try RFC 2822
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try HTTP date format: "Mon, 01 Jan 2026 00:00:00 GMT"
    // Strip the day-of-week prefix and GMT suffix, parse the core date
    if s.ends_with("GMT") {
        let trimmed = s.trim_end_matches("GMT").trim();
        // After stripping "Mon, " prefix: "01 Jan 2026 00:00:00"
        if let Some(comma_pos) = trimmed.find(", ") {
            let date_part = &trimmed[comma_pos + 2..];
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(date_part, "%d %b %Y %H:%M:%S") {
                return Some(dt.and_utc());
            }
        }
    }
    None
}

// --- Offline Queue ---

/// Persisted offline operation queue, stored as `.syncqueue.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OfflineQueue {
    pub operations: Vec<QueuedOperation>,
}

/// A queued sync operation that failed to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedOperation {
    pub action_type: String,
    pub path: String,
    pub queued_at: DateTime<Utc>,
}

impl OfflineQueue {
    pub fn load(workspace_path: &Path) -> Self {
        let queue_path = workspace_path.join(".syncqueue.json");
        if !queue_path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&queue_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(queue) => queue,
                Err(e) => {
                    eprintln!("Warning: corrupt sync queue, backing up and resetting: {}", e);
                    let backup = workspace_path.join(".syncqueue.json.bak");
                    let _ = std::fs::copy(&queue_path, &backup);
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read sync queue: {}", e);
                Self::default()
            }
        }
    }

    pub fn save(&self, workspace_path: &Path) -> Result<()> {
        let queue_path = workspace_path.join(".syncqueue.json");
        if self.operations.is_empty() {
            // Clean up empty queue file
            if queue_path.exists() {
                let _ = std::fs::remove_file(&queue_path);
            }
            return Ok(());
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&queue_path, content)?;
        Ok(())
    }

    /// Merge queued operations with fresh actions, deduplicating by path.
    /// Fresh actions take precedence over stale queued ones.
    pub fn merge_with_actions(&self, fresh_actions: Vec<SyncAction>) -> Vec<SyncAction> {
        let mut result_map: HashMap<String, SyncAction> = HashMap::new();

        // Add queued operations first (lower priority)
        for op in &self.operations {
            if let Some(action) = queued_op_to_action(op) {
                result_map.insert(op.path.clone(), action);
            }
        }

        // Fresh actions override queued ones
        for action in fresh_actions {
            result_map.insert(action.path().to_string(), action);
        }

        let mut actions: Vec<SyncAction> = result_map.into_values().collect();
        actions.sort_by(|a, b| a.path().cmp(b.path()));
        actions
    }
}

fn queued_op_to_action(op: &QueuedOperation) -> Option<SyncAction> {
    let path = op.path.clone();
    match op.action_type.as_str() {
        "upload" => Some(SyncAction::Upload { path }),
        "download" => Some(SyncAction::Download { path }),
        "delete_local" => Some(SyncAction::DeleteLocal { path }),
        "delete_remote" => Some(SyncAction::DeleteRemote { path }),
        "conflict" => Some(SyncAction::Conflict { path }),
        _ => None,
    }
}

fn action_to_queued_op(action: &SyncAction) -> QueuedOperation {
    let (action_type, path) = match action {
        SyncAction::Upload { path } => ("upload", path),
        SyncAction::Download { path } => ("download", path),
        SyncAction::DeleteLocal { path } => ("delete_local", path),
        SyncAction::DeleteRemote { path } => ("delete_remote", path),
        SyncAction::Conflict { path } => ("conflict", path),
    };
    QueuedOperation {
        action_type: action_type.to_string(),
        path: path.clone(),
        queued_at: Utc::now(),
    }
}

// --- File Scanning ---

/// Compute SHA-256 checksum of file contents.
pub fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Check if a file is syncable: *.md files and metadata files at expected depths.
fn is_syncable(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let filename = parts.last().copied().unwrap_or(path);
    // .onyx-workspace.json only at workspace root (depth 1)
    if filename == ".onyx-workspace.json" {
        return parts.len() == 1;
    }
    // .listdata.json only inside a list directory (depth 2)
    if filename == ".listdata.json" {
        return parts.len() == 2;
    }
    // .md files inside a list directory (depth 2)
    if filename.ends_with(".md") {
        return parts.len() == 2;
    }
    false
}

/// Scan local workspace files and compute checksums.
pub fn scan_local_files(workspace_path: &Path) -> Result<Vec<LocalFileInfo>> {
    let mut files = Vec::new();
    scan_dir_recursive(workspace_path, workspace_path, &mut files)?;
    Ok(files)
}

fn scan_dir_recursive(root: &Path, dir: &Path, files: &mut Vec<LocalFileInfo>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root)
            .map_err(|e| Error::Sync(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");

        // Skip sync state/queue files
        if relative == ".syncstate.json" || relative == ".syncqueue.json" {
            continue;
        }

        if path.is_dir() {
            scan_dir_recursive(root, &path, files)?;
        } else if is_syncable(&relative) {
            let data = std::fs::read(&path)?;
            let metadata = std::fs::metadata(&path)?;
            let modified = metadata.modified().ok()
                .map(|t| {
                    let dt: DateTime<Utc> = t.into();
                    dt.to_rfc3339()
                });

            files.push(LocalFileInfo {
                path: relative,
                checksum: compute_checksum(&data),
                modified_at: modified,
                size: data.len() as u64,
            });
        }
    }
    Ok(())
}

/// Convert PROPFIND results into RemoteFileSnapshot list, recursing into directories.
fn scan_remote_files<'a>(client: &'a WebDavClient, base_path: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RemoteFileSnapshot>>> + Send + 'a>> {
    let base_path = base_path.to_string();
    Box::pin(async move {
        let mut result = Vec::new();
        let entries = client.list_files(&base_path).await?;

        for entry in entries {
            let full_path = if base_path.is_empty() {
                entry.path.clone()
            } else {
                format!("{}/{}", base_path.trim_end_matches('/'), entry.path)
            };

            if entry.is_dir {
                let sub_entries = scan_remote_files(client, &full_path).await?;
                result.extend(sub_entries);
            } else if is_syncable(&full_path) {
                result.push(RemoteFileSnapshot {
                    path: full_path,
                    last_modified: entry.last_modified,
                    size: entry.content_length,
                });
            }
        }

        Ok(result)
    })
}

// --- Sync State I/O ---

impl SyncState {
    pub fn load(workspace_path: &Path) -> Self {
        let state_path = workspace_path.join(".syncstate.json");
        if !state_path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&state_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, workspace_path: &Path) -> Result<()> {
        let state_path = workspace_path.join(".syncstate.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&state_path, content)?;
        Ok(())
    }

    /// Update the sync state for a single file after a successful sync action.
    pub fn record_file(&mut self, path: &str, checksum: &str, modified_at: Option<&str>, size: u64) {
        self.files.insert(path.to_string(), SyncFileEntry {
            checksum: checksum.to_string(),
            modified_at: modified_at.map(|s| s.to_string()),
            size,
        });
    }

    /// Remove a file entry from sync state (after deletion).
    pub fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
    }
}

// --- Sync Executor ---

/// Callback type for sync progress reporting.
pub type ProgressCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Execute a full sync between a local workspace and a remote WebDAV server.
pub async fn sync_workspace(
    workspace_path: &Path,
    webdav_url: &str,
    username: &str,
    password: &str,
    mode: SyncMode,
    on_progress: Option<ProgressCallback>,
) -> Result<SyncResult> {
    // Wrap entire sync in a hard timeout — reqwest's built-in timeout
    // doesn't reliably fire on Windows native TLS when the server is unreachable.
    match tokio::time::timeout(
        crate::webdav::REQUEST_TIMEOUT * 2,
        sync_workspace_inner(workspace_path, webdav_url, username, password, mode, on_progress),
    ).await {
        Ok(result) => result,
        Err(_) => Err(Error::WebDav("Sync timed out — server may be unreachable".into())),
    }
}

async fn sync_workspace_inner(
    workspace_path: &Path,
    webdav_url: &str,
    username: &str,
    password: &str,
    mode: SyncMode,
    on_progress: Option<ProgressCallback>,
) -> Result<SyncResult> {
    let client = WebDavClient::new(webdav_url, username, password)?;
    let mut sync_state = SyncState::load(workspace_path);
    let queue = OfflineQueue::load(workspace_path);
    let mut result = SyncResult::default();

    let report = |msg: &str| {
        if let Some(ref cb) = on_progress {
            cb(msg);
        }
    };

    client.test_connection().await?;

    // Scan local files
    let local_files = scan_local_files(workspace_path)?;

    // Scan remote files
    let remote_files = match scan_remote_files(&client, "").await {
        Ok(files) => files,
        Err(e) => {
            // Network error during scan: save what we can and return
            result.errors.push(format!("Failed to scan remote: {}", e));
            return Ok(result);
        }
    };

    // Compute actions from three-way diff
    let fresh_actions = compute_sync_actions(&local_files, &remote_files, &sync_state);

    // Merge with offline queue
    let all_actions = queue.merge_with_actions(fresh_actions);

    // Filter by sync mode (conflicts always run in any mode since they need both sides)
    let actions: Vec<SyncAction> = all_actions.into_iter().filter(|a| match mode {
        SyncMode::Full => true,
        SyncMode::Push => matches!(a, SyncAction::Upload { .. } | SyncAction::DeleteRemote { .. } | SyncAction::Conflict { .. }),
        SyncMode::Pull => matches!(a, SyncAction::Download { .. } | SyncAction::DeleteLocal { .. } | SyncAction::Conflict { .. }),
    }).collect();

    // Execute actions, collecting failures for the queue
    let mut failed_actions = Vec::new();

    // Build remote timestamp lookup for recording accurate download times
    let remote_meta: HashMap<&str, &RemoteFileSnapshot> = remote_files.iter().map(|f| (f.path.as_str(), f)).collect();

    for action in &actions {
        match execute_action(&client, workspace_path, action, &mut sync_state, &remote_meta, &report).await {
            Ok(()) => {
                match action {
                    SyncAction::Upload { .. } => result.uploaded += 1,
                    SyncAction::Download { .. } => result.downloaded += 1,
                    SyncAction::DeleteLocal { .. } => result.deleted_local += 1,
                    SyncAction::DeleteRemote { .. } => result.deleted_remote += 1,
                    SyncAction::Conflict { .. } => result.conflicts += 1,
                }
            }
            Err(e) => {
                let msg = format!("Failed {}: {}", action.path(), e);
                report(&format!("  ! {}", msg));
                result.errors.push(msg);
                failed_actions.push(action.clone());
            }
        }
    }

    // Save queue with remaining failed actions
    let new_queue = OfflineQueue {
        operations: failed_actions.iter().map(action_to_queued_op).collect(),
    };
    new_queue.save(workspace_path)?;

    // Update sync state timestamp
    sync_state.last_sync = Some(Utc::now());
    sync_state.save(workspace_path)?;

    Ok(result)
}

/// Execute a single sync action.
async fn execute_action(
    client: &WebDavClient,
    workspace_path: &Path,
    action: &SyncAction,
    sync_state: &mut SyncState,
    remote_meta: &HashMap<&str, &RemoteFileSnapshot>,
    report: &(dyn Fn(&str) + Send + Sync),
) -> Result<()> {
    match action {
        SyncAction::Upload { path } => {
            let local_path = workspace_path.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let data = std::fs::read(&local_path)?;
            let checksum = compute_checksum(&data);

            if let Some(parent) = path_parent(path) {
                client.ensure_dir(parent).await?;
            }

            report(&format!("  ^ Uploading {}", path));
            client.put_file(path, data.clone()).await?;

            // Record in sync state using local file metadata
            let modified = std::fs::metadata(&local_path).ok()
                .and_then(|m| m.modified().ok())
                .map(|t| { let dt: DateTime<Utc> = t.into(); dt.to_rfc3339() });
            sync_state.record_file(path, &checksum, modified.as_deref(), data.len() as u64);
        }

        SyncAction::Conflict { path } => {
            let local_path = workspace_path.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let local_data = std::fs::read(&local_path)?;
            let local_checksum = compute_checksum(&local_data);

            let remote_data = client.get_file(path).await?;
            let remote_checksum = compute_checksum(&remote_data);

            // If checksums match, it's a false conflict — both sides made the same edit
            if local_checksum == remote_checksum {
                report(&format!("  = Conflict resolved: identical content for {}", path));
                let modified = std::fs::metadata(&local_path).ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| { let dt: DateTime<Utc> = t.into(); dt.to_rfc3339() });
                sync_state.record_file(path, &local_checksum, modified.as_deref(), local_data.len() as u64);
            } else {
                report(&format!("  ! Conflict: remote wins for {}, recovering local as duplicate", path));

                // Remote wins: overwrite local with remote content
                std::fs::write(&local_path, &remote_data)?;
                let modified = std::fs::metadata(&local_path).ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| { let dt: DateTime<Utc> = t.into(); dt.to_rfc3339() });
                sync_state.record_file(path, &remote_checksum, modified.as_deref(), remote_data.len() as u64);

                // For .md task files inside a list dir, create a duplicate of the local version
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() == 2 && parts[1].ends_with(".md") && parts[1] != ".listdata.json" {
                    let local_content = String::from_utf8_lossy(&local_data);
                    if let Ok((frontmatter, description)) = parse_frontmatter_for_conflict(&local_content) {
                        let original_id = frontmatter.id;
                        let new_id = Uuid::new_v4();
                        let prefixed_desc = if description.is_empty() {
                            "[RECOVERED FROM CONFLICT]".to_string()
                        } else {
                            format!("[RECOVERED FROM CONFLICT]\n{}", description)
                        };

                        let new_frontmatter = TaskFrontmatter {
                            id: new_id,
                            ..frontmatter
                        };
                        let yaml = serde_yaml::to_string(&new_frontmatter)
                            .map_err(|e| Error::Sync(e.to_string()))?;
                        let new_content = format!("---\n{}---\n\n{}", yaml, prefixed_desc);

                        // Write the duplicate file using the new UUID as filename
                        let list_dir = workspace_path.join(parts[0]);
                        let dup_filename = format!("{}.md", new_id);
                        let dup_path = list_dir.join(&dup_filename);
                        std::fs::write(&dup_path, &new_content)?;

                        // Insert new task adjacent to original in .listdata.json
                        let listdata_path = list_dir.join(".listdata.json");
                        if listdata_path.exists() {
                            if let Ok(content) = std::fs::read_to_string(&listdata_path) {
                                if let Ok(mut metadata) = serde_json::from_str::<ListMetadata>(&content) {
                                    let insert_pos = metadata.task_order.iter()
                                        .position(|id| *id == original_id)
                                        .map(|p| p + 1)
                                        .unwrap_or(metadata.task_order.len());
                                    metadata.task_order.insert(insert_pos, new_id);
                                    if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                        let _ = std::fs::write(&listdata_path, json);
                                    }
                                }
                            }
                        }

                        // Don't record the duplicate in sync state — next sync will see it
                        // as "local added, remote absent" and upload it automatically.
                    }
                }
            }
        }

        SyncAction::Download { path } => {
            report(&format!("  v Downloading {}", path));
            let data = client.get_file(path).await?;
            let checksum = compute_checksum(&data);

            let local_path = workspace_path.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = local_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&local_path, &data)?;

            // Record remote's last_modified so next diff won't see a timestamp mismatch
            let modified = remote_meta.get(path.as_str()).and_then(|r| r.last_modified.clone());
            sync_state.record_file(path, &checksum, modified.as_deref(), data.len() as u64);
        }

        SyncAction::DeleteLocal { path } => {
            report(&format!("  x Deleting local {}", path));
            let local_path = workspace_path.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if local_path.exists() {
                std::fs::remove_file(&local_path)?;
            }
            sync_state.remove_file(path);
        }

        SyncAction::DeleteRemote { path } => {
            report(&format!("  x Deleting remote {}", path));
            client.delete_file(path).await?;
            sync_state.remove_file(path);
        }
    }
    Ok(())
}

/// Parse frontmatter and description from a markdown task file for conflict recovery.
fn parse_frontmatter_for_conflict(content: &str) -> Result<(TaskFrontmatter, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0] != "---" {
        return Err(Error::InvalidData("Missing frontmatter delimiter".to_string()));
    }
    let end_idx = lines[1..].iter().position(|&line| line == "---")
        .ok_or_else(|| Error::InvalidData("Missing closing frontmatter delimiter".to_string()))?;
    let frontmatter_str = lines[1..=end_idx].join("\n");
    let frontmatter: TaskFrontmatter = serde_yaml::from_str(&frontmatter_str)
        .map_err(|e| Error::Sync(format!("Failed to parse frontmatter: {}", e)))?;
    let description = if end_idx + 2 < lines.len() {
        lines[end_idx + 2..].join("\n").trim().to_string()
    } else {
        String::new()
    };
    Ok((frontmatter, description))
}

/// Get the parent path of a sync path (e.g., "My Tasks/file.md" -> "My Tasks").
fn path_parent(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

/// Get sync status information for display.
pub fn get_sync_status(workspace_path: &Path) -> Result<SyncStatusInfo> {
    let sync_state = SyncState::load(workspace_path);
    let queue = OfflineQueue::load(workspace_path);
    let local_files = scan_local_files(workspace_path)?;

    // Count pending changes (files changed since last sync)
    let mut pending_changes = 0u32;
    for file in &local_files {
        if let Some(base) = sync_state.files.get(&file.path) {
            if file.checksum != base.checksum {
                pending_changes += 1;
            }
        } else {
            pending_changes += 1; // New file
        }
    }

    // Count files in base that are now missing locally (deleted)
    for path in sync_state.files.keys() {
        if !local_files.iter().any(|f| f.path == *path) {
            pending_changes += 1;
        }
    }

    Ok(SyncStatusInfo {
        last_sync: sync_state.last_sync,
        tracked_files: sync_state.files.len() as u32,
        pending_changes,
        queued_operations: queue.operations.len() as u32,
    })
}

/// Summary of sync status for display.
pub struct SyncStatusInfo {
    pub last_sync: Option<DateTime<Utc>>,
    pub tracked_files: u32,
    pub pending_changes: u32,
    pub queued_operations: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- compute_sync_actions tests ---

    fn make_local(path: &str, checksum: &str) -> LocalFileInfo {
        LocalFileInfo {
            path: path.to_string(),
            checksum: checksum.to_string(),
            modified_at: Some("2026-01-15T12:00:00+00:00".to_string()),
            size: 100,
        }
    }

    fn make_remote(path: &str) -> RemoteFileSnapshot {
        RemoteFileSnapshot {
            path: path.to_string(),
            last_modified: Some("Mon, 01 Jan 2026 00:00:00 GMT".to_string()),
            size: 100,
        }
    }

    fn make_base(checksum: &str) -> SyncFileEntry {
        SyncFileEntry {
            checksum: checksum.to_string(),
            modified_at: Some("Mon, 01 Jan 2026 00:00:00 GMT".to_string()),
            size: 100,
        }
    }

    #[test]
    fn test_unchanged_both_sides() {
        let local = vec![make_local("file.md", "abc123")];
        let remote = vec![make_remote("file.md")];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_local_added_remote_absent() {
        let local = vec![make_local("new.md", "abc123")];
        let remote = vec![];
        let state = SyncState::default();

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Upload { path: "new.md".to_string() });
    }

    #[test]
    fn test_remote_added_local_absent() {
        let local = vec![];
        let remote = vec![make_remote("new.md")];
        let state = SyncState::default();

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Download { path: "new.md".to_string() });
    }

    #[test]
    fn test_local_modified_remote_unchanged() {
        let local = vec![make_local("file.md", "new_checksum")];
        let remote = vec![make_remote("file.md")];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("old_checksum"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Upload { path: "file.md".to_string() });
    }

    #[test]
    fn test_remote_modified_local_unchanged() {
        let local = vec![make_local("file.md", "same_checksum")];
        let mut remote = make_remote("file.md");
        remote.size = 200; // Changed size indicates modification
        let remote = vec![remote];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("same_checksum"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Download { path: "file.md".to_string() });
    }

    #[test]
    fn test_local_deleted_remote_unchanged() {
        let local = vec![];
        let remote = vec![make_remote("file.md")];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::DeleteRemote { path: "file.md".to_string() });
    }

    #[test]
    fn test_remote_deleted_local_unchanged() {
        let local = vec![make_local("file.md", "abc123")];
        let remote = vec![];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        // Local unchanged, remote deleted -> delete local
        assert_eq!(actions[0], SyncAction::DeleteLocal { path: "file.md".to_string() });
    }

    #[test]
    fn test_remote_deleted_local_modified() {
        let local = vec![make_local("file.md", "new_checksum")];
        let remote = vec![];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        // Local modified, remote deleted -> upload (local wins)
        assert_eq!(actions[0], SyncAction::Upload { path: "file.md".to_string() });
    }

    #[test]
    fn test_both_modified_emits_conflict() {
        let local = make_local("file.md", "new_local");
        let mut remote = make_remote("file.md");
        remote.size = 200;

        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("old_base"));

        let actions = compute_sync_actions(&[local], &[remote], &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Conflict { path: "file.md".to_string() });
    }

    #[test]
    fn test_deleted_local_modified_remote() {
        let local = vec![];
        let mut remote = make_remote("file.md");
        remote.size = 200; // Modified
        let remote = vec![remote];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Download { path: "file.md".to_string() });
    }

    #[test]
    fn test_modified_local_deleted_remote() {
        let local = vec![make_local("file.md", "new_checksum")];
        let remote = vec![];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("old_checksum"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Upload { path: "file.md".to_string() });
    }

    #[test]
    fn test_both_added_emits_conflict() {
        let local = make_local("file.md", "local_content");
        let remote = make_remote("file.md");

        let state = SyncState::default(); // No base entry

        let actions = compute_sync_actions(&[local], &[remote], &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], SyncAction::Conflict { path: "file.md".to_string() });
    }

    #[test]
    fn test_both_deleted() {
        let local = vec![];
        let remote = vec![];
        let mut state = SyncState::default();
        state.files.insert("file.md".to_string(), make_base("abc123"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_multiple_files_mixed() {
        let local = vec![
            make_local("keep.md", "same"),
            make_local("modified.md", "new"),
            make_local("new_local.md", "brand_new"),
        ];
        let remote = vec![
            make_remote("keep.md"),
            make_remote("modified.md"),
            make_remote("new_remote.md"),
        ];
        let mut state = SyncState::default();
        state.files.insert("keep.md".to_string(), make_base("same"));
        state.files.insert("modified.md".to_string(), make_base("old"));

        let actions = compute_sync_actions(&local, &remote, &state);
        assert_eq!(actions.len(), 3);
        // modified.md: local modified, remote unchanged -> upload
        assert!(actions.iter().any(|a| matches!(a, SyncAction::Upload { path } if path == "modified.md")));
        // new_local.md: added locally -> upload
        assert!(actions.iter().any(|a| matches!(a, SyncAction::Upload { path } if path == "new_local.md")));
        // new_remote.md: added remotely -> download
        assert!(actions.iter().any(|a| matches!(a, SyncAction::Download { path } if path == "new_remote.md")));
    }

    // --- Sync state persistence ---

    #[test]
    fn test_sync_state_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = SyncState::default();
        state.last_sync = Some(Utc::now());
        state.record_file("test.md", "abc123", Some("2026-01-01T00:00:00Z"), 42);

        state.save(temp_dir.path()).unwrap();
        let loaded = SyncState::load(temp_dir.path());

        assert!(loaded.last_sync.is_some());
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files["test.md"].checksum, "abc123");
        assert_eq!(loaded.files["test.md"].size, 42);
    }

    #[test]
    fn test_sync_state_load_missing() {
        let temp_dir = TempDir::new().unwrap();
        let state = SyncState::load(temp_dir.path());
        assert!(state.last_sync.is_none());
        assert!(state.files.is_empty());
    }

    // --- Offline queue ---

    #[test]
    fn test_queue_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let queue = OfflineQueue {
            operations: vec![QueuedOperation {
                action_type: "upload".to_string(),
                path: "test.md".to_string(),
                queued_at: Utc::now(),
            }],
        };

        queue.save(temp_dir.path()).unwrap();
        let loaded = OfflineQueue::load(temp_dir.path());
        assert_eq!(loaded.operations.len(), 1);
        assert_eq!(loaded.operations[0].path, "test.md");
    }

    #[test]
    fn test_queue_empty_cleans_up_file() {
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join(".syncqueue.json");

        // Write a non-empty queue first
        let queue = OfflineQueue {
            operations: vec![QueuedOperation {
                action_type: "upload".to_string(),
                path: "test.md".to_string(),
                queued_at: Utc::now(),
            }],
        };
        queue.save(temp_dir.path()).unwrap();
        assert!(queue_path.exists());

        // Save empty queue should remove the file
        let empty_queue = OfflineQueue::default();
        empty_queue.save(temp_dir.path()).unwrap();
        assert!(!queue_path.exists());
    }

    #[test]
    fn test_queue_merge_fresh_overrides_stale() {
        let queue = OfflineQueue {
            operations: vec![QueuedOperation {
                action_type: "upload".to_string(),
                path: "file.md".to_string(),
                queued_at: Utc::now(),
            }],
        };

        let fresh = vec![SyncAction::Download { path: "file.md".to_string() }];
        let merged = queue.merge_with_actions(fresh);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], SyncAction::Download { path: "file.md".to_string() });
    }

    #[test]
    fn test_queue_merge_combines_different_paths() {
        let queue = OfflineQueue {
            operations: vec![QueuedOperation {
                action_type: "upload".to_string(),
                path: "a.md".to_string(),
                queued_at: Utc::now(),
            }],
        };

        let fresh = vec![SyncAction::Download { path: "b.md".to_string() }];
        let merged = queue.merge_with_actions(fresh);

        assert_eq!(merged.len(), 2);
    }

    // --- Checksum ---

    #[test]
    fn test_compute_checksum_deterministic() {
        let data = b"hello world";
        let c1 = compute_checksum(data);
        let c2 = compute_checksum(data);
        assert_eq!(c1, c2);
        assert!(!c1.is_empty());
    }

    #[test]
    fn test_compute_checksum_different_data() {
        let c1 = compute_checksum(b"hello");
        let c2 = compute_checksum(b"world");
        assert_ne!(c1, c2);
    }

    // --- File scanning ---

    #[test]
    fn test_is_syncable() {
        // .md files must be inside a list dir (depth 2)
        assert!(is_syncable("My Tasks/Buy groceries.md"));
        assert!(!is_syncable("file.md")); // root-level md not valid
        // .listdata.json inside a list dir (depth 2)
        assert!(is_syncable("My Tasks/.listdata.json"));
        assert!(!is_syncable(".listdata.json")); // root-level not valid
        // .onyx-workspace.json only at root (depth 1)
        assert!(is_syncable(".onyx-workspace.json"));
        assert!(!is_syncable("My Tasks/.onyx-workspace.json")); // nested not valid
        // Non-syncable
        assert!(!is_syncable(".syncstate.json"));
        assert!(!is_syncable("random.txt"));
        assert!(!is_syncable("image.png"));
        assert!(!is_syncable("a/b/c/deep.md")); // too deep
    }

    #[test]
    fn test_scan_local_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create a workspace-like structure
        std::fs::write(root.join(".onyx-workspace.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("My Tasks")).unwrap();
        std::fs::write(root.join("My Tasks").join(".listdata.json"), "{}").unwrap();
        std::fs::write(root.join("My Tasks").join("task1.md"), "# Task 1").unwrap();
        std::fs::write(root.join("My Tasks").join("task2.md"), "# Task 2").unwrap();
        // Non-syncable file should be skipped
        std::fs::write(root.join("My Tasks").join("notes.txt"), "notes").unwrap();
        // Sync state file should be skipped
        std::fs::write(root.join(".syncstate.json"), "{}").unwrap();

        let files = scan_local_files(root).unwrap();
        assert_eq!(files.len(), 4); // .onyx-workspace.json, .listdata.json, task1.md, task2.md
        assert!(files.iter().any(|f| f.path == ".onyx-workspace.json"));
        assert!(files.iter().any(|f| f.path == "My Tasks/.listdata.json"));
        assert!(files.iter().any(|f| f.path == "My Tasks/task1.md"));
        assert!(files.iter().any(|f| f.path == "My Tasks/task2.md"));
        assert!(!files.iter().any(|f| f.path.contains("notes.txt")));
        assert!(!files.iter().any(|f| f.path.contains(".syncstate.json")));
    }

    // --- Sync status ---

    #[test]
    fn test_get_sync_status_no_state() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        std::fs::write(root.join(".onyx-workspace.json"), "{}").unwrap();

        let status = get_sync_status(root).unwrap();
        assert!(status.last_sync.is_none());
        assert_eq!(status.tracked_files, 0);
        assert_eq!(status.pending_changes, 1); // .onyx-workspace.json is new
        assert_eq!(status.queued_operations, 0);
    }

    // --- Timestamp parsing ---

    #[test]
    fn test_parse_timestamp_rfc3339() {
        let result = parse_timestamp("2026-01-15T12:00:00+00:00");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_timestamp_http_date() {
        let result = parse_timestamp("Mon, 01 Jan 2026 00:00:00 GMT");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        let result = parse_timestamp("not a date");
        assert!(result.is_none());
    }

    // --- path_parent ---

    #[test]
    fn test_path_parent() {
        assert_eq!(path_parent("My Tasks/file.md"), Some("My Tasks"));
        assert_eq!(path_parent("file.md"), None);
        assert_eq!(path_parent("a/b/c.md"), Some("a/b"));
    }
}
