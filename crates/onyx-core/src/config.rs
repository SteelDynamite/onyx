use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceMode {
    Local,
    Webdav,
}

impl Default for WorkspaceMode {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub mode: WorkspaceMode,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub webdav_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub webdav_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sync_interval_secs: Option<u64>,
}

impl WorkspaceConfig {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self { name, path, mode: WorkspaceMode::Local, webdav_url: None, webdav_path: None, last_sync: None, theme: None, sync_interval_secs: None }
    }
}

/// Workspaces keyed by UUID string.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub workspaces: HashMap<String, WorkspaceConfig>,
    pub current_workspace: Option<String>,
}

impl AppConfig {
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
            current_workspace: None,
        }
    }

    pub fn add_workspace(&mut self, config: WorkspaceConfig) -> String {
        let id = Uuid::new_v4().to_string();
        self.workspaces.insert(id.clone(), config);
        id
    }

    pub fn remove_workspace(&mut self, id: &str) -> Option<WorkspaceConfig> {
        if self.current_workspace.as_deref() == Some(id) {
            self.current_workspace = None;
        }
        self.workspaces.remove(id)
    }

    pub fn rename_workspace(&mut self, id: &str, new_name: String) -> Result<()> {
        let ws = self.workspaces.get_mut(id)
            .ok_or_else(|| Error::InvalidData(format!("Workspace '{}' not found", id)))?;
        ws.name = new_name;
        Ok(())
    }

    pub fn get_workspace(&self, id: &str) -> Option<&WorkspaceConfig> {
        self.workspaces.get(id)
    }

    pub fn get_current_workspace(&self) -> Result<(&String, &WorkspaceConfig)> {
        let id = self.current_workspace.as_ref()
            .ok_or_else(|| Error::WorkspaceNotFound("No current workspace set".to_string()))?;
        let config = self.workspaces.get(id)
            .ok_or_else(|| Error::WorkspaceNotFound(id.clone()))?;
        Ok((id, config))
    }

    pub fn set_current_workspace(&mut self, id: String) -> Result<()> {
        if !self.workspaces.contains_key(&id) {
            return Err(Error::WorkspaceNotFound(id));
        }
        self.current_workspace = Some(id);
        Ok(())
    }

    /// Find a workspace by display name. Returns (id, config) of the first match.
    pub fn find_by_name(&self, name: &str) -> Option<(&String, &WorkspaceConfig)> {
        self.workspaces.iter().find(|(_, ws)| ws.name == name)
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        let config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self)?;
        // Atomic write: write to temp file then rename to prevent corruption on crash
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, &content)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn get_config_path() -> PathBuf {
        directories::ProjectDirs::from("", "", "onyx")
            .map(|dirs| dirs.config_dir().join("config.json"))
            .unwrap_or_else(|| PathBuf::from("onyx-config.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_current_workspace_none_set() {
        let config = AppConfig::new();
        let result = config.get_current_workspace();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::WorkspaceNotFound(_)));
    }

    #[test]
    fn test_get_current_workspace_id_points_to_removed_workspace() {
        let mut config = AppConfig::new();
        let id = config.add_workspace(WorkspaceConfig::new("test".into(), PathBuf::from("/tmp")));
        config.current_workspace = Some(id.clone());
        config.workspaces.remove(&id);

        let result = config.get_current_workspace();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::WorkspaceNotFound(_)));
    }

    #[test]
    fn test_set_current_workspace_nonexistent() {
        let mut config = AppConfig::new();
        let result = config.set_current_workspace("ghost".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::WorkspaceNotFound(_)));
    }

    #[test]
    fn test_set_current_workspace_valid() {
        let mut config = AppConfig::new();
        let id = config.add_workspace(WorkspaceConfig::new("real".into(), PathBuf::from("/tmp")));
        assert!(config.set_current_workspace(id.clone()).is_ok());
        assert_eq!(config.current_workspace.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn test_remove_current_workspace_clears_current() {
        let mut config = AppConfig::new();
        let id = config.add_workspace(WorkspaceConfig::new("ws".into(), PathBuf::from("/tmp")));
        config.set_current_workspace(id.clone()).unwrap();

        config.remove_workspace(&id);
        assert!(config.current_workspace.is_none());
        assert!(config.get_workspace(&id).is_none());
    }

    #[test]
    fn test_remove_noncurrent_workspace_keeps_current() {
        let mut config = AppConfig::new();
        let id_a = config.add_workspace(WorkspaceConfig::new("a".into(), PathBuf::from("/a")));
        let id_b = config.add_workspace(WorkspaceConfig::new("b".into(), PathBuf::from("/b")));
        config.set_current_workspace(id_a.clone()).unwrap();

        config.remove_workspace(&id_b);
        assert_eq!(config.current_workspace.as_deref(), Some(id_a.as_str()));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::new();
        let id1 = config.add_workspace(WorkspaceConfig::new("ws1".into(), PathBuf::from("/path/one")));
        let _id2 = config.add_workspace(WorkspaceConfig::new("ws2".into(), PathBuf::from("/path/two")));
        config.set_current_workspace(id1.clone()).unwrap();
        config.save_to_file(&config_path).unwrap();

        let loaded = AppConfig::load_from_file(&config_path).unwrap();
        assert_eq!(loaded.current_workspace.as_deref(), Some(id1.as_str()));
        assert_eq!(loaded.workspaces.len(), 2);
        assert_eq!(loaded.get_workspace(&id1).unwrap().path, PathBuf::from("/path/one"));
        assert_eq!(loaded.get_workspace(&id1).unwrap().name, "ws1");
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let config = AppConfig::load_from_file(&PathBuf::from("/nonexistent/config.json")).unwrap();
        assert!(config.workspaces.is_empty());
        assert!(config.current_workspace.is_none());
    }

    #[test]
    fn test_load_corrupt_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        std::fs::write(&config_path, "not valid json {{{").unwrap();

        let result = AppConfig::load_from_file(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nested").join("dir").join("config.json");

        let config = AppConfig::new();
        assert!(config.save_to_file(&config_path).is_ok());
        assert!(config_path.exists());
    }

    #[test]
    fn test_duplicate_names_allowed() {
        let mut config = AppConfig::new();
        let id1 = config.add_workspace(WorkspaceConfig::new("Onyx".into(), PathBuf::from("/a")));
        let id2 = config.add_workspace(WorkspaceConfig::new("Onyx".into(), PathBuf::from("/b")));

        assert_ne!(id1, id2);
        assert_eq!(config.workspaces.len(), 2);
        assert_eq!(config.get_workspace(&id1).unwrap().name, "Onyx");
        assert_eq!(config.get_workspace(&id2).unwrap().name, "Onyx");
    }

    #[test]
    fn test_find_by_name() {
        let mut config = AppConfig::new();
        let id = config.add_workspace(WorkspaceConfig::new("Tasks".into(), PathBuf::from("/tasks")));

        let found = config.find_by_name("Tasks");
        assert!(found.is_some());
        assert_eq!(found.unwrap().0, &id);

        assert!(config.find_by_name("Nonexistent").is_none());
    }

    #[test]
    fn test_rename_workspace() {
        let mut config = AppConfig::new();
        let id = config.add_workspace(WorkspaceConfig::new("Old".into(), PathBuf::from("/tmp")));
        config.rename_workspace(&id, "New".into()).unwrap();
        assert_eq!(config.get_workspace(&id).unwrap().name, "New");
    }

    #[test]
    fn test_workspace_config_with_webdav_fields_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::new();
        let mut ws = WorkspaceConfig::new("synced".into(), PathBuf::from("/tasks"));
        ws.webdav_url = Some("https://dav.example.com/tasks".to_string());
        ws.last_sync = Some(chrono::Utc::now());
        let id = config.add_workspace(ws);
        config.save_to_file(&config_path).unwrap();

        let loaded = AppConfig::load_from_file(&config_path).unwrap();
        let ws = loaded.get_workspace(&id).unwrap();
        assert_eq!(ws.webdav_url.as_deref(), Some("https://dav.example.com/tasks"));
        assert!(ws.last_sync.is_some());
    }
}
