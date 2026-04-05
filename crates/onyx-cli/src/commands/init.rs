use anyhow::{Context, Result};
use onyx_core::{AppConfig, TaskRepository, WorkspaceConfig};
use std::path::PathBuf;
use crate::output;

pub fn execute(path: String, name: String) -> Result<()> {
    let path_buf = PathBuf::from(path);
    let path_buf = if path_buf.is_relative() {
        std::env::current_dir()?.join(path_buf)
    } else {
        path_buf
    };

    // Initialize the repository
    let mut repo = TaskRepository::init(path_buf.clone())
        .context("Failed to initialize tasks folder")?;

    // Create default list if it doesn't exist
    let lists = repo.get_lists().context("Failed to get lists")?;
    if !lists.iter().any(|l| l.title == "My Tasks") {
        repo.create_list("My Tasks".to_string())
            .context("Failed to create default list")?;
    }

    // Load or create config
    let config_path = AppConfig::get_config_path();
    let mut config = AppConfig::load_from_file(&config_path)
        .unwrap_or_else(|_| AppConfig::new());

    // Add workspace
    let id = config.add_workspace(WorkspaceConfig::new(name.clone(), path_buf.clone()));
    config.set_current_workspace(id)?;

    // Save config
    config.save_to_file(&config_path)
        .context("Failed to save config")?;

    output::success(&format!("Initialized workspace \"{}\" at {}", name, path_buf.display()));
    output::success("Created default list \"My Tasks\"");
    output::success(&format!("Set \"{}\" as current workspace", name));

    Ok(())
}
