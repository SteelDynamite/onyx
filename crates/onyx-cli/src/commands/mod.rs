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
