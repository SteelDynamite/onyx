use anyhow::{Context, Result};
use colored::*;
use onyx_core::{Task, TaskStatus};
use crate::output;
use crate::commands::get_repository;

fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        output::item("No tasks");
        return;
    }
    for task in tasks {
        let checkbox = if task.status == TaskStatus::Completed { "[✓]".green() } else { "[ ]".normal() };
        let due_str = task.date.map(|d| format!(" ({})", d.format("%Y-%m-%d")).yellow().to_string()).unwrap_or_default();
        output::item(&format!("{} {}{} {}", checkbox, task.title, due_str, task.id.to_string().dimmed()));
    }
}

pub fn create(name: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    repo.create_list(name.clone())
        .context("Failed to create list")?;

    output::success(&format!("Created list \"{}\"", name));

    Ok(())
}

pub fn show(list_name: Option<String>, workspace: Option<String>) -> Result<()> {
    let (repo, _workspace_name) = get_repository(workspace)?;

    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    if lists.is_empty() {
        output::info("No lists found. Create one with 'onyx list create <name>'");
        return Ok(());
    }

    // If a specific list is requested, show only that one
    if let Some(name) = list_name {
        let list = lists.iter()
            .find(|l| l.title == name)
            .ok_or_else(|| anyhow::anyhow!("List '{}' not found", name))?;

        output::header(&format!("{} ({})", list.title, format!("{} tasks", list.tasks.len()).dimmed()));
        print_tasks(&list.tasks);
    } else {
        // Show all lists
        for list in &lists {
            output::header(&format!("{} ({})", list.title, format!("{} tasks", list.tasks.len()).dimmed()));
            print_tasks(&list.tasks);
            output::blank();
        }
    }

    Ok(())
}

pub fn delete(name: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    let list = lists.iter()
        .find(|l| l.title == name)
        .ok_or_else(|| anyhow::anyhow!("List '{}' not found", name))?;

    // Confirm
    output::warning(&format!("This will delete list \"{}\" and all its tasks", name));
    print!("Continue? (y/n): ");
    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "y" {
        output::info("Cancelled");
        return Ok(());
    }

    repo.delete_list(list.id)
        .context("Failed to delete list")?;

    output::success(&format!("Deleted list \"{}\"", name));

    Ok(())
}
