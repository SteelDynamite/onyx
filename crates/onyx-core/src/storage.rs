use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{Error, Result};
use crate::models::{Task, TaskList, TaskStatus};

/// Maximum allowed length for task titles.
const MAX_TITLE_LENGTH: usize = 500;
/// Maximum allowed length for task descriptions.
const MAX_DESCRIPTION_LENGTH: usize = 1_000_000; // 1 MB
/// Maximum allowed length for list names.
const MAX_LIST_NAME_LENGTH: usize = 255;
/// Maximum allowed size for YAML frontmatter (64 KB) to prevent DoS via crafted files.
const MAX_FRONTMATTER_LENGTH: usize = 64 * 1024;
/// Workspace root metadata filename.
const WORKSPACE_METADATA_FILE: &str = ".onyx-workspace.json";
/// Per-list metadata filename.
const LIST_METADATA_FILE: &str = ".listdata.json";
/// Task file extension.
const TASK_FILE_EXT: &str = "md";
/// Default version for tasks without a version field (legacy files).
const DEFAULT_TASK_VERSION: u64 = 1;

/// Write data to a temporary file then atomically rename to the target path.
/// Prevents corruption from partial writes on crash. Cleans up temp file on
/// rename failure to prevent accumulation.
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, content)?;
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

/// Metadata stored in root .onyx-workspace.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootMetadata {
    pub version: u32,
    pub list_order: Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_opened_list: Option<Uuid>,
}

impl Default for RootMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            list_order: Vec::new(),
            last_opened_list: None,
        }
    }
}

/// Metadata stored in each list's .listdata.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMetadata {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub group_by_date: bool,
    pub task_order: Vec<Uuid>,
}

impl ListMetadata {
    pub fn new(id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id,
            created_at: now,
            updated_at: now,
            group_by_date: false,
            task_order: Vec::new(),
        }
    }
}

fn is_false(v: &bool) -> bool { !v }
fn default_version() -> u64 { DEFAULT_TASK_VERSION }

/// Frontmatter for task markdown files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFrontmatter {
    pub id: Uuid,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_time: bool,
    #[serde(default = "default_version")]
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Uuid>,
}

impl From<&Task> for TaskFrontmatter {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id,
            status: task.status,
            due: task.date,
            has_time: task.has_time,
            version: task.version,
            parent: task.parent_id,
        }
    }
}

pub trait Storage {
    fn read_task(&self, list_id: Uuid, task_id: Uuid) -> Result<Task>;
    fn write_task(&mut self, list_id: Uuid, task: &Task) -> Result<()>;
    fn delete_task(&mut self, list_id: Uuid, task_id: Uuid) -> Result<()>;
    fn list_tasks(&self, list_id: Uuid) -> Result<Vec<Task>>;

    fn create_list(&mut self, name: String) -> Result<TaskList>;
    fn get_lists(&self) -> Result<Vec<TaskList>>;
    fn delete_list(&mut self, list_id: Uuid) -> Result<()>;

    fn read_root_metadata(&self) -> Result<RootMetadata>;
    fn write_root_metadata(&mut self, metadata: &RootMetadata) -> Result<()>;

    fn rename_list(&mut self, list_id: Uuid, new_name: String) -> Result<()>;

    fn read_list_metadata(&self, list_id: Uuid) -> Result<ListMetadata>;
    fn write_list_metadata(&mut self, metadata: &ListMetadata) -> Result<()>;
}

#[derive(Debug)]
pub struct FileSystemStorage {
    root_path: PathBuf,
}

impl FileSystemStorage {
    pub fn new(root_path: PathBuf) -> Result<Self> {
        if !root_path.exists() {
            return Err(Error::NotFound(format!("Path does not exist: {:?}", root_path)));
        }
        let storage = Self { root_path };
        storage.cleanup_stale_tmp_files();
        Ok(storage)
    }

    /// Remove leftover .tmp files from interrupted atomic writes.
    fn cleanup_stale_tmp_files(&self) {
        let cleanup_dir = |dir: &Path| {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        };
        // Clean root-level .tmp files
        cleanup_dir(&self.root_path);
        // Clean .tmp files inside list directories
        if let Ok(entries) = fs::read_dir(&self.root_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    cleanup_dir(&entry.path());
                }
            }
        }
    }

    pub fn init(root_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root_path)?;

        let storage = Self { root_path };

        // Create default metadata if it doesn't exist
        if !storage.metadata_path().exists() {
            storage.write_root_metadata_internal(&RootMetadata::default())?;
        }

        Ok(storage)
    }

    fn metadata_path(&self) -> PathBuf {
        self.root_path.join(WORKSPACE_METADATA_FILE)
    }

    fn list_dir_path(&self, list_id: Uuid) -> Result<PathBuf> {
        // Find the directory with this list ID
        let entries = fs::read_dir(&self.root_path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let listdata_path = path.join(LIST_METADATA_FILE);
                if listdata_path.exists() {
                    let content = fs::read_to_string(&listdata_path)?;
                    let list_metadata: ListMetadata = serde_json::from_str(&content)?;
                    if list_metadata.id == list_id {
                        return Ok(path);
                    }
                }
            }
        }

        Err(Error::ListNotFound(list_id.to_string()))
    }

    fn list_dir_path_by_name(&self, name: &str) -> Result<PathBuf> {
        // Reject names containing path separators or traversal components
        if name.contains('/') || name.contains('\\') || name == ".." || name.starts_with("../") || name.starts_with("..\\") {
            return Err(Error::InvalidData("Invalid list name: path traversal not allowed".to_string()));
        }
        let path = self.root_path.join(name);
        // Verify resolved path stays within root.
        // Always build canonical_path from canonical_root + filename to avoid TOCTOU
        // races and symlink escapes (canonicalize resolves symlinks, so a symlink
        // pointing outside the workspace would be caught).
        let canonical_root = self.root_path.canonicalize()
            .map_err(Error::Io)?;
        let canonical_path = if path.exists() {
            let resolved = path.canonicalize().map_err(Error::Io)?;
            // Re-check after canonicalization to catch symlinks pointing outside
            if !resolved.starts_with(&canonical_root) {
                return Err(Error::InvalidData("Invalid list name: path escapes workspace".to_string()));
            }
            resolved
        } else {
            // Parent must exist and be canonicalizable (it's root_path)
            canonical_root.join(path.file_name().unwrap_or_default())
        };
        if !canonical_path.starts_with(&canonical_root) {
            return Err(Error::InvalidData("Invalid list name: path escapes workspace".to_string()));
        }
        Ok(path)
    }

    fn sanitize_filename(name: &str) -> String {
        let sanitized: String = name.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                '\0'..='\x1f' => '_',
                _ => c,
            })
            .collect::<String>()
            .trim_matches(|c: char| c == '.' || c == ' ')
            .to_string();
        // Reject Windows reserved device names (CON, PRN, AUX, NUL, COM0-9, LPT0-9)
        let stem = sanitized.split('.').next().unwrap_or("").to_uppercase();
        let is_reserved = matches!(stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL"
            | "COM0" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT0" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
        );
        if is_reserved {
            format!("_{}", sanitized)
        } else {
            sanitized
        }
    }

    fn task_file_path(&self, list_dir: &Path, task: &Task) -> PathBuf {
        let safe_title = Self::sanitize_filename(&task.title);
        let filename = if safe_title.is_empty() {
            task.id.to_string()
        } else {
            safe_title
        };
        list_dir.join(format!("{}.{}", filename, TASK_FILE_EXT))
    }

    fn parse_markdown_with_frontmatter(&self, content: &str) -> Result<(TaskFrontmatter, String)> {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || lines[0] != "---" {
            return Err(Error::InvalidData("Missing frontmatter delimiter".to_string()));
        }

        // Find closing ---
        let end_idx = lines[1..]
            .iter()
            .position(|&line| line == "---")
            .ok_or_else(|| Error::InvalidData("Missing closing frontmatter delimiter".to_string()))?;

        let frontmatter_lines = &lines[1..=end_idx];
        let frontmatter_str = frontmatter_lines.join("\n");
        if frontmatter_str.len() > MAX_FRONTMATTER_LENGTH {
            return Err(Error::InvalidData(format!(
                "Frontmatter too large ({} bytes, max {})",
                frontmatter_str.len(), MAX_FRONTMATTER_LENGTH
            )));
        }
        let frontmatter: TaskFrontmatter = serde_yaml::from_str(&frontmatter_str)?;

        let description = if end_idx + 2 < lines.len() {
            lines[end_idx + 2..].join("\n")
        } else {
            String::new()
        };

        Ok((frontmatter, description.trim().to_string()))
    }

    fn write_markdown_with_frontmatter(&self, task: &Task) -> Result<String> {
        let mut frontmatter = TaskFrontmatter::from(task);
        frontmatter.version = task.version.saturating_add(1);
        let yaml = serde_yaml::to_string(&frontmatter)?;

        let mut content = String::new();
        content.push_str("---\n");
        content.push_str(&yaml);
        content.push_str("---\n\n");
        content.push_str(&task.description);

        Ok(content)
    }

    fn read_root_metadata_internal(&self) -> Result<RootMetadata> {
        let path = self.metadata_path();
        if !path.exists() {
            return Ok(RootMetadata::default());
        }
        let content = fs::read_to_string(&path)?;
        let metadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    fn write_root_metadata_internal(&self, metadata: &RootMetadata) -> Result<()> {
        let path = self.metadata_path();
        let content = serde_json::to_string_pretty(&metadata)?;
        atomic_write(&path, content.as_bytes())?;
        Ok(())
    }
}

impl Storage for FileSystemStorage {
    fn read_task(&self, list_id: Uuid, task_id: Uuid) -> Result<Task> {
        let list_dir = self.list_dir_path(list_id)?;

        // Read all task files in the list directory
        let entries = fs::read_dir(&list_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some(TASK_FILE_EXT) {
                let content = fs::read_to_string(&path)?;
                let (frontmatter, description) = self.parse_markdown_with_frontmatter(&content)?;

                if frontmatter.id == task_id {
                    let title = path.file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| Error::InvalidData("Invalid filename".to_string()))?
                        .to_string();

                    return Ok(Task {
                        id: frontmatter.id,
                        title,
                        description,
                        status: frontmatter.status,
                        date: frontmatter.date,
                        has_time: frontmatter.has_time,
                        version: frontmatter.version,
                        parent_id: frontmatter.parent,
                    });
                }
            }
        }

        Err(Error::TaskNotFound(task_id.to_string()))
    }

    fn write_task(&mut self, list_id: Uuid, task: &Task) -> Result<()> {
        if task.title.len() > MAX_TITLE_LENGTH {
            return Err(Error::InvalidData(format!("Task title too long ({} chars, max {})", task.title.len(), MAX_TITLE_LENGTH)));
        }
        if task.description.len() > MAX_DESCRIPTION_LENGTH {
            return Err(Error::InvalidData(format!("Task description too long ({} bytes, max {})", task.description.len(), MAX_DESCRIPTION_LENGTH)));
        }

        let list_dir = self.list_dir_path(list_id)?;
        let task_path = self.task_file_path(&list_dir, task);

        // Remove old file if task was renamed (different filename, same ID)
        for entry in fs::read_dir(&list_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path == task_path { continue; }
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some(TASK_FILE_EXT) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok((fm, _)) = self.parse_markdown_with_frontmatter(&content) {
                        if fm.id == task.id {
                            fs::remove_file(&path)?;
                            break;
                        }
                    }
                }
            }
        }

        let content = self.write_markdown_with_frontmatter(task)?;
        fs::write(&task_path, content)?;

        // Update list metadata to include this task in task_order if not already present
        let mut list_metadata = self.read_list_metadata(list_id)?;
        if !list_metadata.task_order.contains(&task.id) {
            list_metadata.task_order.push(task.id);
            list_metadata.updated_at = Utc::now();
            self.write_list_metadata(&list_metadata)?;
        }

        Ok(())
    }

    fn delete_task(&mut self, list_id: Uuid, task_id: Uuid) -> Result<()> {
        let task = self.read_task(list_id, task_id)?;
        let list_dir = self.list_dir_path(list_id)?;
        let task_path = self.task_file_path(&list_dir, &task);

        // Update metadata first so a crash between steps leaves an orphaned file
        // (recoverable) rather than an orphaned metadata entry (confusing).
        let mut list_metadata = self.read_list_metadata(list_id)?;
        list_metadata.task_order.retain(|&id| id != task_id);
        list_metadata.updated_at = Utc::now();
        self.write_list_metadata(&list_metadata)?;

        fs::remove_file(&task_path)?;

        Ok(())
    }

    fn list_tasks(&self, list_id: Uuid) -> Result<Vec<Task>> {
        let list_dir = self.list_dir_path(list_id)?;
        let list_metadata = self.read_list_metadata(list_id)?;

        let mut file_tasks: Vec<(PathBuf, Task)> = Vec::new();
        let entries = fs::read_dir(&list_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some(TASK_FILE_EXT) {
                let content = fs::read_to_string(&path)?;
                let (frontmatter, description) = self.parse_markdown_with_frontmatter(&content)?;

                let title = path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::InvalidData("Invalid filename".to_string()))?
                    .to_string();

                let task = Task {
                    id: frontmatter.id,
                    title,
                    description,
                    status: frontmatter.status,
                    date: frontmatter.date,
                    has_time: frontmatter.has_time,
                    version: frontmatter.version,
                    parent_id: frontmatter.parent,
                };

                file_tasks.push((path, task));
            }
        }

        // Self-healing dedup: group by UUID, keep highest version, delete stale files.
        // When versions are equal, keep the file with the latest filesystem modification
        // time to avoid non-deterministic selection.
        let mut by_id: HashMap<Uuid, Vec<(PathBuf, Task)>> = HashMap::new();
        for entry in file_tasks {
            by_id.entry(entry.1.id).or_default().push(entry);
        }

        let mut tasks = Vec::new();
        for (_id, mut entries) in by_id {
            if entries.len() > 1 {
                entries.sort_by(|a, b| {
                    // Primary: highest version first
                    let version_cmp = b.1.version.cmp(&a.1.version);
                    if version_cmp != std::cmp::Ordering::Equal {
                        return version_cmp;
                    }
                    // Tiebreaker: most recently modified file first
                    let mtime_a = fs::metadata(&a.0).and_then(|m| m.modified()).ok();
                    let mtime_b = fs::metadata(&b.0).and_then(|m| m.modified()).ok();
                    mtime_b.cmp(&mtime_a)
                });
                for (stale_path, _) in entries.drain(1..) {
                    if let Err(e) = fs::remove_file(&stale_path) {
                        eprintln!("Warning: failed to remove stale duplicate task file {:?}: {}", stale_path, e);
                    }
                }
            }
            let (_, task) = entries.into_iter().next()
                .ok_or_else(|| Error::InvalidData("Empty dedup entries for task".to_string()))?;
            tasks.push(task);
        }

        // Sort by task_order
        let order_map: HashMap<Uuid, usize> = list_metadata.task_order
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();

        tasks.sort_by_key(|task| order_map.get(&task.id).copied().unwrap_or(usize::MAX));

        Ok(tasks)
    }

    fn create_list(&mut self, name: String) -> Result<TaskList> {
        if name.trim().is_empty() {
            return Err(Error::InvalidData("List name cannot be empty".to_string()));
        }
        if name.len() > MAX_LIST_NAME_LENGTH {
            return Err(Error::InvalidData(format!("List name too long ({} chars, max {})", name.len(), MAX_LIST_NAME_LENGTH)));
        }
        let list_dir = self.list_dir_path_by_name(&name)?;

        if list_dir.exists() {
            return Err(Error::InvalidData(format!("List '{}' already exists", name)));
        }

        fs::create_dir_all(&list_dir)?;

        let list_id = Uuid::new_v4();
        let list_metadata = ListMetadata::new(list_id);

        let metadata_path = list_dir.join(".listdata.json");
        let content = serde_json::to_string_pretty(&list_metadata)?;
        atomic_write(&metadata_path, content.as_bytes())?;

        // Add to root metadata
        let mut root_metadata = self.read_root_metadata_internal()?;
        root_metadata.list_order.push(list_id);
        if root_metadata.last_opened_list.is_none() {
            root_metadata.last_opened_list = Some(list_id);
        }
        self.write_root_metadata_internal(&root_metadata)?;

        let task_list = TaskList {
            id: list_id,
            title: name,
            tasks: Vec::new(),
            created_at: list_metadata.created_at,
            updated_at: list_metadata.updated_at,
            group_by_date: list_metadata.group_by_date,
        };

        Ok(task_list)
    }

    fn get_lists(&self) -> Result<Vec<TaskList>> {
        let root_metadata = self.read_root_metadata_internal()?;
        let mut lists = Vec::new();

        let entries = fs::read_dir(&self.root_path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let listdata_path = path.join(LIST_METADATA_FILE);
                if listdata_path.exists() {
                    let content = fs::read_to_string(&listdata_path)?;
                    let list_metadata: ListMetadata = serde_json::from_str(&content)?;

                    let title = path.file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| Error::InvalidData("Invalid directory name".to_string()))?
                        .to_string();

                    let tasks = self.list_tasks(list_metadata.id)?;

                    let task_list = TaskList {
                        id: list_metadata.id,
                        title,
                        tasks,
                        created_at: list_metadata.created_at,
                        updated_at: list_metadata.updated_at,
                        group_by_date: list_metadata.group_by_date,
                    };

                    lists.push(task_list);
                }
            }
        }

        // Sort by list_order
        let order_map: HashMap<Uuid, usize> = root_metadata.list_order
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();

        lists.sort_by_key(|list| order_map.get(&list.id).copied().unwrap_or(usize::MAX));

        Ok(lists)
    }

    fn delete_list(&mut self, list_id: Uuid) -> Result<()> {
        let list_dir = self.list_dir_path(list_id)?;

        // Update root metadata first so a crash between steps leaves an orphaned
        // directory (recoverable) rather than an orphaned metadata entry.
        let mut root_metadata = self.read_root_metadata_internal()?;
        root_metadata.list_order.retain(|&id| id != list_id);
        if root_metadata.last_opened_list == Some(list_id) {
            root_metadata.last_opened_list = root_metadata.list_order.first().copied();
        }
        self.write_root_metadata_internal(&root_metadata)?;

        fs::remove_dir_all(&list_dir)?;

        Ok(())
    }

    fn rename_list(&mut self, list_id: Uuid, new_name: String) -> Result<()> {
        if new_name.trim().is_empty() {
            return Err(Error::InvalidData("List name cannot be empty".to_string()));
        }
        if new_name.len() > MAX_LIST_NAME_LENGTH {
            return Err(Error::InvalidData(format!("List name too long ({} chars, max {})", new_name.len(), MAX_LIST_NAME_LENGTH)));
        }
        let old_dir = self.list_dir_path(list_id)?;
        let new_dir = self.list_dir_path_by_name(&new_name)?;

        if new_dir.exists() {
            return Err(Error::InvalidData(format!("A list named '{}' already exists", new_name)));
        }

        fs::rename(&old_dir, &new_dir)?;

        // Update metadata timestamp
        let metadata_path = new_dir.join(".listdata.json");
        let content = fs::read_to_string(&metadata_path)?;
        let mut metadata: ListMetadata = serde_json::from_str(&content)?;
        metadata.updated_at = Utc::now();
        let json = serde_json::to_string_pretty(&metadata)?;
        atomic_write(&metadata_path, json.as_bytes())?;

        Ok(())
    }

    fn read_root_metadata(&self) -> Result<RootMetadata> {
        self.read_root_metadata_internal()
    }

    fn write_root_metadata(&mut self, metadata: &RootMetadata) -> Result<()> {
        self.write_root_metadata_internal(metadata)
    }

    fn read_list_metadata(&self, list_id: Uuid) -> Result<ListMetadata> {
        let list_dir = self.list_dir_path(list_id)?;
        let metadata_path = list_dir.join(".listdata.json");

        if !metadata_path.exists() {
            return Err(Error::NotFound(format!("List metadata not found: {}", list_id)));
        }

        let content = fs::read_to_string(&metadata_path)?;
        let metadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    fn write_list_metadata(&mut self, metadata: &ListMetadata) -> Result<()> {
        let list_dir = self.list_dir_path(metadata.id)?;
        let metadata_path = list_dir.join(".listdata.json");

        let content = serde_json::to_string_pretty(&metadata)?;
        atomic_write(&metadata_path, content.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Task;
    use tempfile::TempDir;

    fn init_storage(temp_dir: &TempDir) -> FileSystemStorage {
        FileSystemStorage::init(temp_dir.path().to_path_buf()).unwrap()
    }

    // --- Frontmatter parsing ---

    #[test]
    fn test_parse_valid_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\nversion: 3\n---\n\nSome description";
        let (fm, desc) = storage.parse_markdown_with_frontmatter(content).unwrap();
        assert_eq!(fm.id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(fm.status, TaskStatus::Backlog);
        assert_eq!(fm.version, 3);
        assert_eq!(desc, "Some description");
    }

    #[test]
    fn test_parse_frontmatter_no_body() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: completed\nversion: 1\n---";
        let (fm, desc) = storage.parse_markdown_with_frontmatter(content).unwrap();
        assert_eq!(fm.status, TaskStatus::Completed);
        assert!(desc.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_missing_opening_delimiter() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let result = storage.parse_markdown_with_frontmatter("no frontmatter here");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_parse_frontmatter_missing_closing_delimiter() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\n";
        let result = storage.parse_markdown_with_frontmatter(content);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_parse_frontmatter_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let result = storage.parse_markdown_with_frontmatter("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_parse_frontmatter_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\n: : : not valid yaml\n---\n";
        let result = storage.parse_markdown_with_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_frontmatter_with_optional_fields() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\ndue: 2026-06-15T12:00:00Z\nversion: 2\nparent: 660e8400-e29b-41d4-a716-446655440001\n---\n\nNotes";
        let (fm, _) = storage.parse_markdown_with_frontmatter(content).unwrap();
        assert!(fm.date.is_some());
        assert!(fm.parent.is_some());
    }

    // --- Markdown write/read roundtrip ---

    #[test]
    fn test_markdown_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let task = Task::new("Test".to_string())
            .with_description("Line 1\n\nLine 3".to_string());

        let markdown = storage.write_markdown_with_frontmatter(&task).unwrap();
        let (fm, desc) = storage.parse_markdown_with_frontmatter(&markdown).unwrap();

        assert_eq!(fm.id, task.id);
        assert_eq!(fm.status, task.status);
        assert_eq!(desc, "Line 1\n\nLine 3");
    }

    // --- FileSystemStorage init/new ---

    #[test]
    fn test_new_nonexistent_path() {
        let result = FileSystemStorage::new(PathBuf::from("/does/not/exist"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[test]
    fn test_init_creates_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let _storage = init_storage(&temp_dir);
        assert!(temp_dir.path().join(".onyx-workspace.json").exists());
    }

    #[test]
    fn test_init_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        let mut s = FileSystemStorage::init(path.clone()).unwrap();
        let list = s.create_list("Keep Me".to_string()).unwrap();

        // Re-init should not destroy existing data
        let s2 = FileSystemStorage::init(path).unwrap();
        let lists = s2.get_lists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, list.id);
    }

    // --- Root metadata ---

    #[test]
    fn test_root_metadata_defaults_when_missing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        // Delete the metadata file to simulate missing
        fs::remove_file(temp_dir.path().join(".onyx-workspace.json")).unwrap();

        let meta = storage.read_root_metadata().unwrap();
        assert_eq!(meta.version, 1);
        assert!(meta.list_order.is_empty());
    }

    // --- List operations ---

    #[test]
    fn test_create_list_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);

        storage.create_list("Dupes".to_string()).unwrap();
        let result = storage.create_list("Dupes".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_delete_list_cleans_up_root_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);

        let list = storage.create_list("To Delete".to_string()).unwrap();
        let meta_before = storage.read_root_metadata().unwrap();
        assert!(meta_before.list_order.contains(&list.id));

        storage.delete_list(list.id).unwrap();
        let meta_after = storage.read_root_metadata().unwrap();
        assert!(!meta_after.list_order.contains(&list.id));
    }

    #[test]
    fn test_list_dir_path_nonexistent_list() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let result = storage.list_dir_path(Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ListNotFound(_)));
    }

    // --- Task file operations ---

    #[test]
    fn test_write_and_read_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let task = Task::new("Hello".to_string());
        storage.write_task(list.id, &task).unwrap();

        let read_back = storage.read_task(list.id, task.id).unwrap();
        assert_eq!(read_back.title, "Hello");
        assert_eq!(read_back.id, task.id);
    }

    #[test]
    fn test_read_task_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let result = storage.read_task(list.id, Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::TaskNotFound(_)));
    }

    #[test]
    fn test_write_task_adds_to_task_order() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let t1 = Task::new("First".to_string());
        let t2 = Task::new("Second".to_string());
        storage.write_task(list.id, &t1).unwrap();
        storage.write_task(list.id, &t2).unwrap();

        let meta = storage.read_list_metadata(list.id).unwrap();
        assert_eq!(meta.task_order.len(), 2);
        assert_eq!(meta.task_order[0], t1.id);
        assert_eq!(meta.task_order[1], t2.id);
    }

    #[test]
    fn test_write_task_idempotent_order() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let task = Task::new("Once".to_string());
        storage.write_task(list.id, &task).unwrap();
        storage.write_task(list.id, &task).unwrap(); // Write again

        let meta = storage.read_list_metadata(list.id).unwrap();
        assert_eq!(meta.task_order.len(), 1); // Should not duplicate
    }

    #[test]
    fn test_delete_task_removes_from_order() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let task = Task::new("Bye".to_string());
        storage.write_task(list.id, &task).unwrap();
        storage.delete_task(list.id, task.id).unwrap();

        let meta = storage.read_list_metadata(list.id).unwrap();
        assert!(!meta.task_order.contains(&task.id));
    }

    #[test]
    fn test_list_tasks_respects_order() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let t1 = Task::new("Alpha".to_string());
        let t2 = Task::new("Beta".to_string());
        let t3 = Task::new("Gamma".to_string());
        storage.write_task(list.id, &t1).unwrap();
        storage.write_task(list.id, &t2).unwrap();
        storage.write_task(list.id, &t3).unwrap();

        // Rewrite metadata with reversed order
        let mut meta = storage.read_list_metadata(list.id).unwrap();
        meta.task_order = vec![t3.id, t1.id, t2.id];
        storage.write_list_metadata(&meta).unwrap();

        let tasks = storage.list_tasks(list.id).unwrap();
        assert_eq!(tasks[0].id, t3.id);
        assert_eq!(tasks[1].id, t1.id);
        assert_eq!(tasks[2].id, t2.id);
    }

    #[test]
    fn test_list_tasks_empty_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Empty".to_string()).unwrap();

        let tasks = storage.list_tasks(list.id).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_missing_version_defaults_to_1() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\n---\n\nOld task";
        let (fm, _) = storage.parse_markdown_with_frontmatter(content).unwrap();
        assert_eq!(fm.version, 1);
    }

    #[test]
    fn test_missing_has_time_defaults_to_false() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\nversion: 1\n---\n";
        let (fm, _) = storage.parse_markdown_with_frontmatter(content).unwrap();
        assert!(!fm.has_time);
    }

    #[test]
    fn test_version_increments_on_write() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Tasks".to_string()).unwrap();

        let task = Task::new("Versioned".to_string());
        assert_eq!(task.version, 0);

        storage.write_task(list.id, &task).unwrap();
        let read_back = storage.read_task(list.id, task.id).unwrap();
        assert_eq!(read_back.version, 1);

        // Write again — version should increment again
        storage.write_task(list.id, &read_back).unwrap();
        let read_again = storage.read_task(list.id, task.id).unwrap();
        assert_eq!(read_again.version, 2);
    }

    #[test]
    fn test_dedup_keeps_highest_version() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Dedup".to_string()).unwrap();

        let task = Task::new("Original".to_string());
        let task_id = task.id;
        storage.write_task(list.id, &task).unwrap();

        // Simulate a sync duplicate: manually write a second file with the same UUID but lower version
        let list_dir = storage.list_dir_path(list.id).unwrap();
        let stale_content = format!(
            "---\nid: {}\nstatus: backlog\nversion: 1\n---\n\nStale copy",
            task_id
        );
        let stale_path = list_dir.join("Original_old.md");
        fs::write(&stale_path, &stale_content).unwrap();

        let tasks = storage.list_tasks(list.id).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task_id);
        // Both are version 1, so mtime tiebreaker picks the most recent file.
        // Verify only one .md file remains.
        let md_count = fs::read_dir(&list_dir).unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().and_then(|s| s.to_str()) == Some("md"))
            .count();
        assert_eq!(md_count, 1);
    }

    // --- Deduplication mtime tiebreaker ---

    #[test]
    fn test_dedup_equal_versions_uses_mtime_tiebreaker() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("Dedup2".to_string()).unwrap();

        let task_id = uuid::Uuid::new_v4();
        let list_dir = storage.list_dir_path(list.id).unwrap();

        // Create two files with the same UUID and same version (1)
        let content_a = format!(
            "---\nid: {}\nstatus: backlog\nversion: 1\n---\n\nVersion A (older)",
            task_id
        );
        let content_b = format!(
            "---\nid: {}\nstatus: backlog\nversion: 1\n---\n\nVersion B (newer)",
            task_id
        );

        let path_a = list_dir.join("TaskA.md");
        let path_b = list_dir.join("TaskB.md");
        fs::write(&path_a, &content_a).unwrap();
        // Sleep briefly so mtime differs
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&path_b, &content_b).unwrap();

        let tasks = storage.list_tasks(list.id).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task_id);
        // The newer file (B) should win the mtime tiebreaker
        assert_eq!(tasks[0].description, "Version B (newer)");

        // Verify only one .md file remains
        let md_count = fs::read_dir(&list_dir).unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().and_then(|s| s.to_str()) == Some("md"))
            .count();
        assert_eq!(md_count, 1);
    }

    // --- Frontmatter size limit ---

    #[test]
    fn test_parse_frontmatter_rejects_oversized() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        // Create content with frontmatter larger than 64KB
        let huge = "x".repeat(70_000);
        let content = format!(
            "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\nversion: 1\nhuge: {}\n---\n\nBody",
            huge
        );
        let result = storage.parse_markdown_with_frontmatter(&content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too large"), "Error should mention size: {}", err);
    }

    #[test]
    fn test_parse_frontmatter_accepts_normal_size() {
        let temp_dir = TempDir::new().unwrap();
        let storage = init_storage(&temp_dir);

        let content = "---\nid: 550e8400-e29b-41d4-a716-446655440000\nstatus: backlog\nversion: 1\n---\n\nDescription";
        let result = storage.parse_markdown_with_frontmatter(content);
        assert!(result.is_ok());
    }

    // --- Version saturating_add ---

    #[test]
    fn test_version_saturates_at_max() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("MaxVer".to_string()).unwrap();

        let mut task = Task::new("Saturate".to_string());
        task.version = u64::MAX - 1;

        storage.write_task(list.id, &task).unwrap();
        let read_back = storage.read_task(list.id, task.id).unwrap();
        assert_eq!(read_back.version, u64::MAX, "Version should saturate at u64::MAX");

        // Writing again should not panic or wrap
        storage.write_task(list.id, &read_back).unwrap();
        let read_again = storage.read_task(list.id, task.id).unwrap();
        assert_eq!(read_again.version, u64::MAX, "Version should stay at u64::MAX");
    }

    // --- Delete ordering: metadata before file ---

    #[test]
    fn test_delete_task_removes_from_metadata_first() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = init_storage(&temp_dir);
        let list = storage.create_list("DelOrder".to_string()).unwrap();

        let task = Task::new("ToDelete".to_string());
        let task_id = task.id;
        storage.write_task(list.id, &task).unwrap();

        // Verify task is in metadata
        let meta = storage.read_list_metadata(list.id).unwrap();
        assert!(meta.task_order.contains(&task_id));

        // Delete
        storage.delete_task(list.id, task_id).unwrap();

        // Verify metadata no longer contains the task
        let meta_after = storage.read_list_metadata(list.id).unwrap();
        assert!(!meta_after.task_order.contains(&task_id));

        // Verify file is also gone
        let list_dir = storage.list_dir_path(list.id).unwrap();
        let md_count = fs::read_dir(&list_dir).unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().and_then(|s| s.to_str()) == Some("md"))
            .count();
        assert_eq!(md_count, 0);
    }

    // --- Atomic write no leftover tmp ---

    #[test]
    fn test_atomic_write_no_leftover_tmp() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("test.json");
        atomic_write(&target, b"hello").unwrap();

        let tmp_files: Vec<_> = fs::read_dir(temp_dir.path()).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("tmp"))
            .collect();
        assert!(tmp_files.is_empty(), "No .tmp files should remain after atomic_write");
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }
}
