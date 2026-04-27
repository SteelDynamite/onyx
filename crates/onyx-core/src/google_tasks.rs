//! Google Tasks API client and one-way pull sync.
//!
//! Workspaces of mode `GoogleTasks` are read-only: remote always wins. The sync
//! fetches all task lists and tasks from the Google Tasks REST API and writes them
//! to the local `FileSystemStorage` format, overwriting stale local state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::{Task, TaskStatus};
use crate::storage::{ListMetadata, RootMetadata, atomic_write};

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Fixed UUID v5 namespace for deterministic Google ID → Onyx UUID conversion.
/// Changing this value would invalidate all existing synced task IDs.
const GT_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1,
    0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Convert a Google Tasks opaque ID to a stable Onyx UUID using UUID v5.
/// The same Google ID always produces the same UUID, enabling stable local files
/// across sync cycles without needing an explicit ID mapping file.
pub fn gt_id_to_uuid(google_id: &str) -> Uuid {
    Uuid::new_v5(&GT_NAMESPACE, google_id.as_bytes())
}

// ── API response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GtListsResponse {
    #[serde(default)]
    items: Vec<GtTaskList>,
}

#[derive(Debug, Deserialize)]
struct GtTaskList {
    id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
struct GtTasksResponse {
    #[serde(default)]
    items: Vec<GtTask>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GtTask {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    notes: String,
    /// "needsAction" or "completed"
    #[serde(default)]
    status: String,
    /// RFC 3339 timestamp; time component is always T00:00:00.000Z (date-only).
    due: Option<String>,
    /// Parent task Google ID (absent for top-level tasks).
    parent: Option<String>,
    /// Opaque position string used for ordering within a list.
    #[serde(default)]
    position: String,
}

// ── Client ───────────────────────────────────────────────────────────

/// Thin wrapper around `reqwest::Client` that adds a Bearer auth header to every
/// request and handles pagination for list endpoints.
pub struct GoogleTasksClient {
    client: Client,
    access_token: String,
}

impl GoogleTasksClient {
    pub fn new(access_token: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| Error::WebDav(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self { client, access_token })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self.client
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(Error::Credential("Google access token expired or invalid".to_string()));
        }
        if !status.is_success() {
            return Err(Error::WebDav(format!("Google Tasks API error: HTTP {}", status)));
        }

        resp.json().await.map_err(|e| Error::WebDav(format!("Failed to parse Google API response: {}", e)))
    }

    /// Returns all task lists for the authenticated user.
    async fn list_task_lists(&self) -> Result<Vec<GtTaskList>> {
        let resp: GtListsResponse = self
            .get("https://tasks.googleapis.com/tasks/v1/users/@me/lists")
            .await?;
        Ok(resp.items)
    }

    /// Returns all tasks in a task list, following pagination automatically.
    async fn list_tasks(&self, list_id: &str) -> Result<Vec<GtTask>> {
        let mut all_tasks = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let url = match &page_token {
                Some(token) => format!(
                    "https://tasks.googleapis.com/tasks/v1/lists/{}/tasks\
                     ?showCompleted=true&showHidden=true&maxResults=100&pageToken={}",
                    list_id, token
                ),
                None => format!(
                    "https://tasks.googleapis.com/tasks/v1/lists/{}/tasks\
                     ?showCompleted=true&showHidden=true&maxResults=100",
                    list_id
                ),
            };

            let resp: GtTasksResponse = self.get(&url).await?;
            all_tasks.extend(resp.items);
            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        Ok(all_tasks)
    }
}

// ── Token refresh ────────────────────────────────────────────────────

/// Exchange a refresh token for a new access token.
/// `client_secret` is `None` for Android (no secret required for Android OAuth clients).
pub async fn refresh_access_token(
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<String> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| Error::WebDav(format!("Failed to build HTTP client: {}", e)))?;

    let mut params = vec![
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Credential(format!("Token refresh failed: {}", body)));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    let token_resp: TokenResponse = resp.json().await
        .map_err(|e| Error::WebDav(format!("Failed to parse token response: {}", e)))?;
    Ok(token_resp.access_token)
}

// ── Sync ─────────────────────────────────────────────────────────────

/// Result of a Google Tasks one-way pull sync.
pub struct GoogleSyncResult {
    pub downloaded: u32,
    pub errors: Vec<String>,
}

/// One-way pull sync: fetch all Google Tasks lists and tasks, write to local storage.
///
/// Remote always wins. Local edits (if any) are silently overwritten. This function
/// never pushes anything to Google.
pub async fn sync_google_tasks(
    workspace_path: &Path,
    access_token: &str,
) -> Result<GoogleSyncResult> {
    let client = GoogleTasksClient::new(access_token.to_string())?;

    std::fs::create_dir_all(workspace_path)?;
    let mut downloaded: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    let gt_lists = client.list_task_lists().await?;

    // Compute the set of UUIDs that correspond to remote lists (for cleanup).
    let remote_list_uuids: HashSet<Uuid> = gt_lists.iter()
        .map(|l| gt_id_to_uuid(&l.id))
        .collect();

    // Remove local list directories that no longer exist remotely.
    if let Ok(entries) = std::fs::read_dir(workspace_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let listdata_path = path.join(".listdata.json");
            if let Ok(content) = std::fs::read_to_string(&listdata_path) {
                if let Ok(meta) = serde_json::from_str::<ListMetadata>(&content) {
                    if !remote_list_uuids.contains(&meta.id) {
                        let _ = std::fs::remove_dir_all(&path);
                    }
                }
            }
        }
    }

    let mut new_list_order: Vec<Uuid> = Vec::new();

    for gt_list in &gt_lists {
        let list_uuid = gt_id_to_uuid(&gt_list.id);
        new_list_order.push(list_uuid);

        let list_dir = match find_or_create_list_dir(workspace_path, list_uuid, &gt_list.title) {
            Ok(d) => d,
            Err(e) => {
                errors.push(format!("Failed to set up list '{}': {}", gt_list.title, e));
                continue;
            }
        };

        let listdata_path = list_dir.join(".listdata.json");
        let mut list_meta: ListMetadata = if listdata_path.exists() {
            std::fs::read_to_string(&listdata_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| ListMetadata::new(list_uuid))
        } else {
            ListMetadata::new(list_uuid)
        };

        let gt_tasks = match client.list_tasks(&gt_list.id).await {
            Ok(tasks) => tasks,
            Err(e) => {
                errors.push(format!("Failed to fetch tasks for list '{}': {}", gt_list.title, e));
                continue;
            }
        };

        // Compute the set of remote task UUIDs so we can remove deleted ones locally.
        let remote_task_uuids: HashSet<Uuid> = gt_tasks.iter()
            .map(|t| gt_id_to_uuid(&t.id))
            .collect();

        // Remove local task files for tasks deleted from Google.
        if let Ok(entries) = std::fs::read_dir(&list_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(task_uuid) = extract_task_uuid(&content) {
                        if !remote_task_uuids.contains(&task_uuid) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Sort by position to preserve Google Tasks ordering.
        let mut sorted_tasks = gt_tasks;
        sorted_tasks.sort_by(|a, b| a.position.cmp(&b.position));

        let mut task_order: Vec<Uuid> = Vec::new();

        for gt_task in &sorted_tasks {
            if gt_task.title.is_empty() { continue; }

            let task_uuid = gt_id_to_uuid(&gt_task.id);
            task_order.push(task_uuid);

            let status = if gt_task.status == "completed" {
                TaskStatus::Completed
            } else {
                TaskStatus::Backlog
            };

            // Google Tasks dates are date-only (time is always T00:00:00Z).
            let date = gt_task.due.as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok());

            let parent_id = gt_task.parent.as_deref().map(gt_id_to_uuid);

            let task = Task {
                id: task_uuid,
                title: gt_task.title.clone(),
                description: gt_task.notes.clone(),
                status,
                date,
                has_time: false,
                version: 1,
                parent_id,
            };

            // File is named after the sanitized title (matching FileSystemStorage convention).
            // If two tasks share a sanitized title, append a short UUID suffix to avoid collision.
            let safe_title = sanitize_name(&task.title);
            let candidate = list_dir.join(format!("{}.md", safe_title));
            let task_path = if candidate.exists() {
                // Check if the existing file already belongs to this task UUID.
                let existing_ok = std::fs::read_to_string(&candidate)
                    .ok()
                    .and_then(|c| extract_task_uuid(&c))
                    .map(|u| u == task_uuid)
                    .unwrap_or(false);
                if existing_ok {
                    candidate
                } else {
                    list_dir.join(format!("{}_{}.md", safe_title, &task_uuid.to_string()[..8]))
                }
            } else {
                candidate
            };

            let content = render_task_markdown(&task);
            if let Err(e) = atomic_write(&task_path, content.as_bytes()) {
                errors.push(format!("Failed to write task '{}': {}", task.title, e));
            } else {
                downloaded += 1;
            }
        }

        list_meta.task_order = task_order;
        list_meta.updated_at = Utc::now();

        match serde_json::to_string_pretty(&list_meta) {
            Ok(meta_content) => {
                if let Err(e) = atomic_write(&listdata_path, meta_content.as_bytes()) {
                    errors.push(format!("Failed to write metadata for list '{}': {}", gt_list.title, e));
                }
            }
            Err(e) => {
                errors.push(format!("Failed to serialize metadata for list '{}': {}", gt_list.title, e));
            }
        }
    }

    // Update workspace root metadata with the new list ordering.
    let root_meta_path = workspace_path.join(".onyx-workspace.json");
    let mut root_meta: RootMetadata = if root_meta_path.exists() {
        std::fs::read_to_string(&root_meta_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        RootMetadata::default()
    };
    root_meta.list_order = new_list_order;
    match serde_json::to_string_pretty(&root_meta) {
        Ok(meta_content) => {
            if let Err(e) = atomic_write(&root_meta_path, meta_content.as_bytes()) {
                errors.push(format!("Failed to write workspace metadata: {}", e));
            }
        }
        Err(e) => {
            errors.push(format!("Failed to serialize workspace metadata: {}", e));
        }
    }

    Ok(GoogleSyncResult { downloaded, errors })
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Find an existing list directory by UUID, or create a new one named after the list title.
fn find_or_create_list_dir(
    workspace_path: &Path,
    list_uuid: Uuid,
    list_title: &str,
) -> std::io::Result<PathBuf> {
    // Look for an existing directory already associated with this list UUID.
    if let Ok(entries) = std::fs::read_dir(workspace_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let listdata_path = path.join(".listdata.json");
            if let Ok(content) = std::fs::read_to_string(&listdata_path) {
                if let Ok(meta) = serde_json::from_str::<ListMetadata>(&content) {
                    if meta.id == list_uuid {
                        return Ok(path);
                    }
                }
            }
        }
    }

    // No existing directory found; create one named after the list.
    let safe_name = sanitize_name(list_title);
    let dir = workspace_path.join(&safe_name);
    // If the name is taken by a different list, append a short UUID suffix.
    let dir = if dir.exists() {
        workspace_path.join(format!("{}_{}", safe_name, &list_uuid.to_string()[..8]))
    } else {
        dir
    };

    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Extract the task UUID from a `.md` file's frontmatter without fully parsing it.
fn extract_task_uuid(content: &str) -> Option<Uuid> {
    let mut lines = content.lines();
    if lines.next()? != "---" { return None; }
    for line in lines {
        if line == "---" { break; }
        if let Some(rest) = line.strip_prefix("id: ") {
            return rest.trim().parse().ok();
        }
    }
    None
}

/// Render an Onyx `Task` as the markdown format expected by `FileSystemStorage`.
/// Version is fixed at 1; it will be incremented by the storage layer on any
/// subsequent write by the user (which is blocked in the UI for Google Tasks workspaces).
fn render_task_markdown(task: &Task) -> String {
    let status_str = match task.status {
        TaskStatus::Backlog => "backlog",
        TaskStatus::Completed => "completed",
    };
    let mut yaml = format!("id: {}\nstatus: {}\nversion: 1\n", task.id, status_str);
    if let Some(due) = task.date {
        yaml.push_str(&format!("date: {}\n", due.to_rfc3339()));
    }
    if let Some(parent) = task.parent_id {
        yaml.push_str(&format!("parent: {}\n", parent));
    }
    format!("---\n{}---\n\n{}", yaml, task.description)
}

/// Sanitize a string for use as a filesystem path component.
fn sanitize_name(name: &str) -> String {
    let s = crate::sanitize_filename(name);
    if s.is_empty() { "Untitled".to_string() } else { s }
}

