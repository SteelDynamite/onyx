use anyhow::{Context, Result};
use crate::output;
use crate::commands::get_repository;

pub fn enable(list_name: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    let list = lists.iter()
        .find(|l| l.title == list_name)
        .ok_or_else(|| anyhow::anyhow!("List '{}' not found", list_name))?;

    repo.set_group_by_date(list.id, true)
        .context("Failed to enable grouping")?;

    output::success(&format!("Enabled group-by-date for list \"{}\"", list_name));

    Ok(())
}

pub fn disable(list_name: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    let list = lists.iter()
        .find(|l| l.title == list_name)
        .ok_or_else(|| anyhow::anyhow!("List '{}' not found", list_name))?;

    repo.set_group_by_date(list.id, false)
        .context("Failed to disable grouping")?;

    output::success(&format!("Disabled group-by-date for list \"{}\"", list_name));

    Ok(())
}
