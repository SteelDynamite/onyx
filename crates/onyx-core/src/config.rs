use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
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
}

impl WorkspaceConfig {
    pub fn new(path: PathBuf) -> Self {
        Self { path, mode: WorkspaceMode::Local, webdav_url: None, webdav_path: None, last_sync: None, theme: None }
    }
}

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

    pub fn add_workspace(&mut self, name: String, config: WorkspaceConfig) {
        self.workspaces.insert(name, config);
    }

    pub fn remove_workspace(&mut self, name: &str) -> Option<WorkspaceConfig> {
        if self.current_workspace.as_deref() == Some(name) {
            self.current_workspace = None;
        }
        self.workspaces.remove(name)
    }

    pub fn rename_workspace(&mut self, old_name: &str, new_name: String) -> Result<()> {
        if !self.workspaces.contains_key(old_name) {
            return Err(Error::InvalidData(format!("Workspace '{}' not found", old_name)));
        }
        if self.workspaces.contains_key(&new_name) {
            return Err(Error::InvalidData(format!("Workspace '{}' already exists", new_name)));
        }
        let ws = self.workspaces.remove(old_name).unwrap();
        if self.current_workspace.as_deref() == Some(old_name) {
            self.current_workspace = Some(new_name.clone());
        }
        self.workspaces.insert(new_name, ws);
        Ok(())
    }

    pub fn get_workspace(&self, name: &str) -> Option<&WorkspaceConfig> {
        self.workspaces.get(name)
    }

    pub fn get_current_workspace(&self) -> Result<(&String, &WorkspaceConfig)> {
        let name = self.current_workspace.as_ref()
            .ok_or_else(|| Error::WorkspaceNotFound("No current workspace set".to_string()))?;
        let config = self.workspaces.get(name)
            .ok_or_else(|| Error::WorkspaceNotFound(name.clone()))?;
        Ok((name, config))
    }

    pub fn set_current_workspace(&mut self, name: String) -> Result<()> {
        if !self.workspaces.contains_key(&name) {
            return Err(Error::WorkspaceNotFound(name));
        }
        self.current_workspace = Some(name);
        Ok(())
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
        std::fs::write(path, content)?;
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
    fn test_get_current_workspace_name_points_to_removed_workspace() {
        let mut config = AppConfig::new();
        config.add_workspace("test".to_string(), WorkspaceConfig::new(PathBuf::from("/tmp")));
        config.current_workspace = Some("test".to_string());
        config.workspaces.remove("test");

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
        config.add_workspace("real".to_string(), WorkspaceConfig::new(PathBuf::from("/tmp")));
        assert!(config.set_current_workspace("real".to_string()).is_ok());
        assert_eq!(config.current_workspace.as_deref(), Some("real"));
    }

    #[test]
    fn test_remove_current_workspace_clears_current() {
        let mut config = AppConfig::new();
        config.add_workspace("ws".to_string(), WorkspaceConfig::new(PathBuf::from("/tmp")));
        config.set_current_workspace("ws".to_string()).unwrap();

        config.remove_workspace("ws");
        assert!(config.current_workspace.is_none());
        assert!(config.get_workspace("ws").is_none());
    }

    #[test]
    fn test_remove_noncurrent_workspace_keeps_current() {
        let mut config = AppConfig::new();
        config.add_workspace("a".to_string(), WorkspaceConfig::new(PathBuf::from("/a")));
        config.add_workspace("b".to_string(), WorkspaceConfig::new(PathBuf::from("/b")));
        config.set_current_workspace("a".to_string()).unwrap();

        config.remove_workspace("b");
        assert_eq!(config.current_workspace.as_deref(), Some("a"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::new();
        config.add_workspace("ws1".to_string(), WorkspaceConfig::new(PathBuf::from("/path/one")));
        config.add_workspace("ws2".to_string(), WorkspaceConfig::new(PathBuf::from("/path/two")));
        config.set_current_workspace("ws1".to_string()).unwrap();
        config.save_to_file(&config_path).unwrap();

        let loaded = AppConfig::load_from_file(&config_path).unwrap();
        assert_eq!(loaded.current_workspace.as_deref(), Some("ws1"));
        assert_eq!(loaded.workspaces.len(), 2);
        assert_eq!(loaded.get_workspace("ws1").unwrap().path, PathBuf::from("/path/one"));
        assert_eq!(loaded.get_workspace("ws2").unwrap().path, PathBuf::from("/path/two"));
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
    fn test_add_workspace_overwrites_existing() {
        let mut config = AppConfig::new();
        config.add_workspace("ws".to_string(), WorkspaceConfig::new(PathBuf::from("/old")));
        config.add_workspace("ws".to_string(), WorkspaceConfig::new(PathBuf::from("/new")));

        assert_eq!(config.get_workspace("ws").unwrap().path, PathBuf::from("/new"));
        assert_eq!(config.workspaces.len(), 1);
    }

    #[test]
    fn test_workspace_config_with_webdav_fields_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::new();
        let mut ws = WorkspaceConfig::new(PathBuf::from("/tasks"));
        ws.webdav_url = Some("https://dav.example.com/tasks".to_string());
        ws.last_sync = Some(chrono::Utc::now());
        config.add_workspace("synced".to_string(), ws);
        config.save_to_file(&config_path).unwrap();

        let loaded = AppConfig::load_from_file(&config_path).unwrap();
        let ws = loaded.get_workspace("synced").unwrap();
        assert_eq!(ws.webdav_url.as_deref(), Some("https://dav.example.com/tasks"));
        assert!(ws.last_sync.is_some());
    }

    #[test]
    fn test_backwards_compat_loading_old_format() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write old-format JSON without webdav_url, last_sync, mode, or theme fields
        let old_json = r#"{
            "workspaces": {
                "personal": { "path": "/home/user/tasks" }
            },
            "current_workspace": "personal"
        }"#;
        std::fs::write(&config_path, old_json).unwrap();

        let loaded = AppConfig::load_from_file(&config_path).unwrap();
        let ws = loaded.get_workspace("personal").unwrap();
        assert_eq!(ws.path, PathBuf::from("/home/user/tasks"));
        assert!(ws.webdav_url.is_none());
        assert!(ws.last_sync.is_none());
        assert_eq!(ws.mode, WorkspaceMode::Local);
        assert!(ws.theme.is_none());
    }
}
