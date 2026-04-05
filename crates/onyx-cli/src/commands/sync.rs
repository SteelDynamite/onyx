use anyhow::{Context, Result};
use colored::Colorize;
use onyx_core::sync::{SyncMode, sync_workspace, get_sync_status};
use onyx_core::webdav::{WebDavClient, store_credentials, load_credentials};
use onyx_core::config::AppConfig;
use crate::output;
use super::{load_config, save_config};

/// Resolve a workspace name to (id, config). Falls back to current workspace if name is None.
fn resolve_workspace(config: &AppConfig, name: Option<&str>) -> Result<(String, onyx_core::config::WorkspaceConfig)> {
    if let Some(name) = name {
        let (id, ws) = config.find_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", name))?;
        Ok((id.clone(), ws.clone()))
    } else {
        let (id, ws) = config.get_current_workspace()
            .context("No workspace set. Use 'onyx init' to create one.")?;
        Ok((id.clone(), ws.clone()))
    }
}

/// Run sync setup: prompt for URL, username, password, test connection, store credentials.
pub fn setup(workspace_name: Option<String>) -> Result<()> {
    let mut config = load_config()?;
    let (id, workspace) = resolve_workspace(&config, workspace_name.as_deref())?;

    // Prompt for WebDAV URL
    output::header(&format!("WebDAV sync setup for workspace \"{}\"", workspace.name.green()));
    output::blank();

    let url = prompt("WebDAV URL: ")?;
    if url.is_empty() {
        output::error("URL cannot be empty");
        return Ok(());
    }

    let username = prompt("Username: ")?;
    let password = rpassword::read_password_from_tty(Some("Password: "))
        .context("Failed to read password")?;

    // Test connection
    output::blank();
    output::info("Testing connection...");

    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;
    let client = WebDavClient::new(&url, &username, &password)
        .context("Invalid WebDAV URL")?;

    match rt.block_on(client.test_connection()) {
        Ok(()) => {
            output::success("Connection successful!");
        }
        Err(e) => {
            output::error(&format!("Connection failed: {}", e));
            return Ok(());
        }
    }

    // Store credentials in keychain
    let domain = extract_domain(&url);
    match store_credentials(&domain, &username, &password) {
        Ok(()) => output::info("Credentials stored in system keychain"),
        Err(e) => {
            output::warning(&format!(
                "Could not store in keychain ({}). Set ONYX_WEBDAV_USER and ONYX_WEBDAV_PASS env vars instead.",
                e
            ));
        }
    }

    // Update workspace config with WebDAV URL
    if let Some(ws) = config.workspaces.get_mut(&id) {
        ws.webdav_url = Some(url);
    }
    save_config(&config)?;

    output::success("Sync setup complete. Run 'onyx sync' to sync.");
    Ok(())
}

/// Execute a sync operation.
pub fn execute(mode: SyncMode, workspace_name: Option<String>) -> Result<()> {
    let config = load_config()?;
    let (_id, workspace) = resolve_workspace(&config, workspace_name.as_deref())?;

    let url = workspace.webdav_url.as_ref()
        .ok_or_else(|| anyhow::anyhow!(
            "No WebDAV URL configured for workspace '{}'. Run 'onyx sync --setup' first.", workspace.name
        ))?;

    let domain = extract_domain(url);
    let (username, password) = load_credentials(&domain)
        .context("Failed to load credentials")?;

    let mode_str = match mode {
        SyncMode::Full => "Syncing",
        SyncMode::Push => "Pushing",
        SyncMode::Pull => "Pulling",
    };
    output::info(&format!("{} workspace \"{}\"...", mode_str, workspace.name.green()));

    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;
    let result = rt.block_on(sync_workspace(
        &workspace.path,
        url,
        &username,
        &password,
        mode,
        Some(Box::new(|msg: &str| { println!("{}", msg); })),
    )).context("Sync failed")?;

    // Print summary
    let mut parts = Vec::new();
    if result.uploaded > 0 { parts.push(format!("{} uploaded", result.uploaded)); }
    if result.downloaded > 0 { parts.push(format!("{} downloaded", result.downloaded)); }
    if result.deleted_local > 0 { parts.push(format!("{} deleted locally", result.deleted_local)); }
    if result.deleted_remote > 0 { parts.push(format!("{} deleted remotely", result.deleted_remote)); }
    if result.conflicts > 0 { parts.push(format!("{} conflicts", result.conflicts)); }

    if parts.is_empty() {
        output::success("Already in sync, nothing to do.");
    } else {
        let summary = parts.join(", ");
        if result.errors.is_empty() {
            output::success(&format!("Sync complete: {}", summary));
        } else {
            output::warning(&format!("Sync complete with errors: {}", summary));
            for err in &result.errors {
                output::error(err);
            }
        }
    }

    Ok(())
}

/// Show sync status for a workspace.
pub fn status(workspace_name: Option<String>, all: bool) -> Result<()> {
    let config = load_config()?;

    if all {
        // Show status for all workspaces that have sync configured
        let mut found_any = false;
        let mut workspaces: Vec<_> = config.workspaces.values().collect();
        workspaces.sort_by(|a, b| a.name.cmp(&b.name));
        for ws in workspaces {
            if ws.webdav_url.is_some() {
                found_any = true;
                print_workspace_status(&ws.name, &ws.path, ws.webdav_url.as_deref())?;
                output::blank();
            }
        }
        if !found_any {
            output::info("No workspaces have sync configured. Run 'onyx sync --setup' to set up.");
        }
        return Ok(());
    }

    let (_id, workspace) = resolve_workspace(&config, workspace_name.as_deref())?;
    print_workspace_status(&workspace.name, &workspace.path, workspace.webdav_url.as_deref())?;
    Ok(())
}

fn print_workspace_status(name: &str, path: &std::path::Path, webdav_url: Option<&str>) -> Result<()> {
    output::header(&format!("Workspace: {}", name.green()));

    if let Some(url) = webdav_url {
        output::detail("WebDAV URL", url);
    } else {
        output::detail("WebDAV", &"not configured".dimmed().to_string());
        return Ok(());
    }

    let info = get_sync_status(path)?;

    if let Some(last) = info.last_sync {
        output::detail("Last sync", &last.format("%Y-%m-%d %H:%M:%S UTC").to_string());
    } else {
        output::detail("Last sync", &"never".dimmed().to_string());
    }

    output::detail("Tracked files", &info.tracked_files.to_string());
    output::detail("Pending changes", &info.pending_changes.to_string());
    if info.queued_operations > 0 {
        output::detail("Queued operations", &format!("{}", info.queued_operations).yellow().to_string());
    }

    Ok(())
}

/// Extract host from a URL for credential storage.
fn extract_domain(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host_port = if let Some(at_pos) = authority.rfind('@') {
        &authority[at_pos + 1..]
    } else {
        authority
    };
    host_port.split(':').next().unwrap_or(host_port).to_string()
}

/// Prompt the user for text input.
fn prompt(message: &str) -> Result<String> {
    use std::io::Write;
    print!("{}", message);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
