use anyhow::{Context, Result};
use onyx_core::{TaskRepository, WorkspaceConfig};
use std::path::PathBuf;
use colored::*;
use crate::output;
use crate::commands::{load_config, save_config};

pub fn add(name: String, path: String) -> Result<()> {
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

    // Load config
    let mut config = load_config()?;

    // Add workspace
    let id = config.add_workspace(WorkspaceConfig::new(name.clone(), path_buf.clone()));

    // Save config
    save_config(&config)?;

    output::success(&format!("Added workspace \"{}\" ({}) at {}", name, &id[..8], path_buf.display()));
    output::success("Created default list \"My Tasks\"");

    Ok(())
}

pub fn list() -> Result<()> {
    let config = load_config()?;

    if config.workspaces.is_empty() {
        output::info("No workspaces configured. Use 'onyx init' to create one.");
        return Ok(());
    }

    let current = config.current_workspace.as_deref();

    let mut workspaces: Vec<_> = config.workspaces.iter().collect();
    workspaces.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    for (id, workspace_config) in workspaces {
        let marker = if Some(id.as_str()) == current {
            " (current)".green()
        } else {
            "".normal()
        };
        output::item(&format!("{}: {}{}", workspace_config.name, workspace_config.path.display(), marker));
    }

    Ok(())
}

/// Resolve a workspace name to its ID. Errors if not found or ambiguous.
fn resolve_name(config: &onyx_core::config::AppConfig, name: &str) -> Result<String> {
    let matches: Vec<_> = config.workspaces.iter()
        .filter(|(_, ws)| ws.name == name)
        .collect();
    match matches.len() {
        0 => anyhow::bail!("Workspace '{}' not found", name),
        1 => Ok(matches[0].0.clone()),
        n => anyhow::bail!("Ambiguous: {} workspaces named '{}'. Use the workspace ID instead.", n, name),
    }
}

pub fn switch(name: String) -> Result<()> {
    let mut config = load_config()?;
    let id = resolve_name(&config, &name)?;

    config.set_current_workspace(id)?;
    save_config(&config)?;

    output::success(&format!("Switched to workspace \"{}\"", name));

    Ok(())
}

pub fn remove(name: String) -> Result<()> {
    let mut config = load_config()?;
    let id = resolve_name(&config, &name)?;

    // Confirm
    output::warning("This will delete workspace config (files remain on disk)");
    print!("Continue? (y/n): ");
    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "y" {
        output::info("Cancelled");
        return Ok(());
    }

    config.remove_workspace(&id);
    save_config(&config)?;

    output::success(&format!("Removed workspace \"{}\"", name));

    Ok(())
}

pub fn retarget(name: String, path: String) -> Result<()> {
    let path_buf = PathBuf::from(path);
    let path_buf = if path_buf.is_relative() {
        std::env::current_dir()?.join(path_buf)
    } else {
        path_buf
    };

    let mut config = load_config()?;
    let id = resolve_name(&config, &name)?;

    // Update path
    config.workspaces.get_mut(&id).unwrap().path = path_buf.clone();
    save_config(&config)?;

    output::success(&format!("Workspace \"{}\" now points to {}", name, path_buf.display()));

    Ok(())
}

pub fn migrate(name: String, new_path: String) -> Result<()> {
    let new_path_buf = PathBuf::from(new_path);
    let new_path_buf = if new_path_buf.is_relative() {
        std::env::current_dir()?.join(new_path_buf)
    } else {
        new_path_buf
    };

    let mut config = load_config()?;
    let id = resolve_name(&config, &name)?;

    // Get current workspace config
    let old_path = config.get_workspace(&id)
        .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", name))?
        .path.clone();

    // Confirm
    output::warning(&format!("This will move all files from {} to {}", old_path.display(), new_path_buf.display()));
    print!("Continue? (y/n): ");
    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "y" {
        output::info("Cancelled");
        return Ok(());
    }

    // Validate destination
    if old_path == new_path_buf {
        anyhow::bail!("Source and destination paths are the same");
    }
    if new_path_buf.exists() && new_path_buf.read_dir()?.next().is_some() {
        anyhow::bail!("Destination directory '{}' already contains files", new_path_buf.display());
    }

    // Create destination directory
    std::fs::create_dir_all(&new_path_buf)?;

    // Move files, tracking what was moved for rollback
    output::info("Moving files...");
    let entries: Vec<_> = std::fs::read_dir(&old_path)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut moved: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();

    let move_result: Result<()> = (|| {
        for entry in &entries {
            let file_name = entry.file_name();
            let dest = new_path_buf.join(&file_name);

            if entry.path().is_dir() {
                let mut options = fs_extra::dir::CopyOptions::new();
                options.copy_inside = true;
                fs_extra::dir::move_dir(entry.path(), &new_path_buf, &options)?;
            } else {
                std::fs::rename(entry.path(), &dest)?;
            }
            moved.push((entry.path(), dest));
            output::item(&format!("Moved {}", file_name.to_string_lossy()));
        }
        Ok(())
    })();

    if let Err(e) = move_result {
        output::error(&format!("Migration failed: {}. Rolling back...", e));
        for (src, dest) in moved.into_iter().rev() {
            if dest.exists() {
                if dest.is_dir() {
                    let mut options = fs_extra::dir::CopyOptions::new();
                    options.copy_inside = true;
                    let _ = fs_extra::dir::move_dir(&dest, &old_path, &options);
                } else {
                    let _ = std::fs::rename(&dest, &src);
                }
            }
        }
        anyhow::bail!("Migration failed and was rolled back: {}", e);
    }

    // Remove old directory if empty
    if old_path.exists() && old_path.read_dir()?.next().is_none() {
        std::fs::remove_dir(&old_path)?;
    }

    // Update config
    config.workspaces.get_mut(&id).unwrap().path = new_path_buf.clone();
    save_config(&config)?;

    output::success(&format!("Migrated {} items to {}", moved.len(), new_path_buf.display()));
    output::success(&format!("Workspace \"{}\" now points to {}", name, new_path_buf.display()));

    Ok(())
}
