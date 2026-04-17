pub mod init;
pub mod workspace;
pub mod list;
pub mod task;
pub mod group;
pub mod sync;

use onyx_core::{AppConfig, TaskRepository};
use onyx_core::config::WorkspaceConfig;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn get_config_path() -> PathBuf {
    AppConfig::get_config_path()
}

pub fn load_config() -> Result<AppConfig> {
    let path = get_config_path();
    AppConfig::load_from_file(&path).context("Failed to load config")
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = get_config_path();
    config.save_to_file(&path).context("Failed to save config")
}

/// Resolve a user-supplied identifier to (id, WorkspaceConfig). Accepts either
/// the workspace's display name or its UUID. Falls back to the current
/// workspace when `identifier` is `None`.
pub fn resolve_workspace(config: &AppConfig, identifier: Option<&str>) -> Result<(String, WorkspaceConfig)> {
    if let Some(s) = identifier {
        // Try by UUID first (exact match on map key), then fall back to name lookup.
        if let Some(ws) = config.get_workspace(s) {
            return Ok((s.to_string(), ws.clone()));
        }
        let (id, ws) = config.find_by_name(s)
            .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", s))?;
        Ok((id.clone(), ws.clone()))
    } else {
        let (id, ws) = config.get_current_workspace()
            .context("No workspace set. Run 'onyx workspace add <name> <path>' to create one, or 'onyx workspace switch <name>' to select one.")?;
        Ok((id.clone(), ws.clone()))
    }
}

pub fn get_repository(workspace_identifier: Option<String>) -> Result<(TaskRepository, String)> {
    let config = load_config()?;
    let (_id, workspace_config) = resolve_workspace(&config, workspace_identifier.as_deref())?;
    let name = workspace_config.name.clone();

    let repo = TaskRepository::new(workspace_config.path.clone())
        .context(format!("Failed to open workspace '{}'", name))?;

    Ok((repo, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config_with(ws: &[(&str, &str)]) -> (AppConfig, Vec<String>) {
        let mut config = AppConfig::new();
        let ids: Vec<String> = ws.iter()
            .map(|(name, path)| config.add_workspace(WorkspaceConfig::new(name.to_string(), PathBuf::from(path))))
            .collect();
        (config, ids)
    }

    #[test]
    fn resolve_by_name() {
        let (config, _ids) = make_config_with(&[("dev", "/tmp/dev"), ("home", "/tmp/home")]);
        let (id, ws) = resolve_workspace(&config, Some("dev")).unwrap();
        assert_eq!(ws.name, "dev");
        assert!(config.workspaces.contains_key(&id));
    }

    #[test]
    fn resolve_by_uuid() {
        let (config, ids) = make_config_with(&[("dev", "/tmp/dev")]);
        let target = ids[0].clone();
        let (id, ws) = resolve_workspace(&config, Some(&target)).unwrap();
        assert_eq!(id, target);
        assert_eq!(ws.name, "dev");
    }

    #[test]
    fn resolve_unknown_identifier_errors() {
        let (config, _ids) = make_config_with(&[("dev", "/tmp/dev")]);
        let err = resolve_workspace(&config, Some("ghost")).unwrap_err();
        assert!(err.to_string().contains("Workspace 'ghost' not found"));
    }

    #[test]
    fn resolve_falls_back_to_current() {
        let (mut config, ids) = make_config_with(&[("a", "/tmp/a"), ("b", "/tmp/b")]);
        config.set_current_workspace(ids[1].clone()).unwrap();
        let (id, ws) = resolve_workspace(&config, None).unwrap();
        assert_eq!(id, ids[1]);
        assert_eq!(ws.name, "b");
    }

    #[test]
    fn resolve_no_current_gives_actionable_message() {
        let config = AppConfig::new();
        let err = resolve_workspace(&config, None).unwrap_err();
        let msg = err.to_string();
        // The message should point the user at the right sub-commands, not
        // at the obsolete 'onyx init' suggestion.
        assert!(msg.contains("workspace add") || msg.contains("workspace switch"),
            "expected actionable message, got: {msg}");
    }
}
