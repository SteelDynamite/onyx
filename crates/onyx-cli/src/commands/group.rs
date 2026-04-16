use anyhow::{Context, Result};
use crate::output;
use crate::commands::get_repository;

fn set_grouping(list_name: String, workspace: Option<String>, enabled: bool) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    let list = lists.iter()
        .find(|l| l.title == list_name)
        .ok_or_else(|| anyhow::anyhow!("List '{}' not found", list_name))?;

    let action = if enabled { "enable" } else { "disable" };
    repo.set_group_by_date(list.id, enabled)
        .context(format!("Failed to {} grouping", action))?;

    let past = if enabled { "Enabled" } else { "Disabled" };
    output::success(&format!("{} group-by-date for list \"{}\"", past, list_name));

    Ok(())
}

pub fn enable(list_name: String, workspace: Option<String>) -> Result<()> {
    set_grouping(list_name, workspace, true)
}

pub fn disable(list_name: String, workspace: Option<String>) -> Result<()> {
    set_grouping(list_name, workspace, false)
}
