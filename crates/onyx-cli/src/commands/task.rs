use anyhow::{Context, Result};
use onyx_core::Task;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::output;
use crate::commands::get_repository;
use onyx_core::TaskList;

/// Find a task by ID across all lists, returning the list ID and cloned task.
fn find_task(lists: &[TaskList], task_id: Uuid) -> Option<(Uuid, Task)> {
    for list in lists {
        if let Some(task) = list.tasks.iter().find(|t| t.id == task_id) {
            return Some((list.id, task.clone()));
        }
    }
    None
}

pub fn add(title: String, list_name: Option<String>, date_str: Option<String>, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    // Get lists
    let lists = repo.get_lists()
        .context("Failed to get lists")?;

    if lists.is_empty() {
        anyhow::bail!("No lists found. Create one with 'onyx list create <name>'");
    }

    // Find the target list
    let list = if let Some(name) = list_name {
        lists.iter()
            .find(|l| l.title == name)
            .ok_or_else(|| anyhow::anyhow!("List '{}' not found", name))?
    } else {
        // Use the first list
        &lists[0]
    };

    // Create task
    let mut task = Task::new(title.clone());

    // Parse date if provided
    if let Some(due_str) = date_str {
        let date = parse_date(&due_str)?;
        task.date = Some(date);
    }

    // Save task
    repo.create_task(list.id, task.clone())
        .context("Failed to create task")?;

    let due_info = if let Some(due) = task.date {
        format!("\n  Date: {}", due.format("%Y-%m-%d"))
    } else {
        String::new()
    };

    output::success(&format!("Created task \"{}\" ({}){}", title, task.id, due_info));

    Ok(())
}

pub fn complete(task_id_str: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let task_id = Uuid::parse_str(&task_id_str)
        .context("Invalid task ID")?;

    let lists = repo.get_lists()?;
    let (list_id, mut task) = find_task(&lists, task_id)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id_str))?;

    task.complete();
    repo.update_task(list_id, task.clone())
        .context("Failed to update task")?;

    output::success(&format!("Completed task \"{}\"", task.title));

    Ok(())
}

pub fn delete(task_id_str: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let task_id = Uuid::parse_str(&task_id_str)
        .context("Invalid task ID")?;

    let lists = repo.get_lists()?;
    let (list_id, task) = find_task(&lists, task_id)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id_str))?;

    output::warning(&format!("This will delete task \"{}\"", task.title));
    print!("Continue? (y/n): ");
    use std::io::{self, Write};
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        output::info("Cancelled");
        return Ok(());
    }

    repo.delete_task(list_id, task_id)
        .context("Failed to delete task")?;

    output::success(&format!("Deleted task \"{}\"", task.title));

    Ok(())
}

pub fn edit(task_id_str: String, workspace: Option<String>) -> Result<()> {
    let (mut repo, _workspace_name) = get_repository(workspace)?;

    let task_id = Uuid::parse_str(&task_id_str)
        .context("Invalid task ID")?;

    let lists = repo.get_lists()?;
    let (list_id, task) = find_task(&lists, task_id)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", task_id_str))?;

    // Create temporary file with task content. On Unix, open with 0600 so
    // other local users on a shared system can't read the task body off /tmp
    // while the editor is running.
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("onyx-{}.md", task.id));

    let content = format!("# {}\n\n{}", task.title, task.description);
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&temp_file)
            .with_context(|| format!("Failed to create {}", temp_file.display()))?;
        f.write_all(content.as_bytes())?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
        if cfg!(windows) {
            "notepad".to_string()
        } else {
            "nano".to_string()
        }
    });

    // Open editor
    let status = std::process::Command::new(&editor)
        .arg(&temp_file)
        .status()
        .context(format!("Failed to open editor: {}", editor))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Read updated content
    let updated_content = std::fs::read_to_string(&temp_file)?;

    // Parse the content
    let lines: Vec<&str> = updated_content.lines().collect();
    let (title, description) = if !lines.is_empty() && lines[0].starts_with("# ") {
        let title = lines[0].trim_start_matches("# ").trim().to_string();
        let description = if lines.len() > 2 {
            lines[2..].join("\n").trim().to_string()
        } else {
            String::new()
        };
        (title, description)
    } else {
        (task.title.clone(), updated_content.trim().to_string())
    };

    // Update task
    let mut updated_task = task.clone();
    updated_task.title = title;
    updated_task.description = description;

    repo.update_task(list_id, updated_task.clone())
        .context("Failed to update task")?;

    // Clean up temp file
    std::fs::remove_file(&temp_file).ok();

    output::success(&format!("Updated task \"{}\"", updated_task.title));

    Ok(())
}

fn parse_date(s: &str) -> Result<DateTime<Utc>> {
    // Try parsing as date only (YYYY-MM-DD)
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_datetime = naive_date.and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
        return Ok(DateTime::from_naive_utc_and_offset(naive_datetime, Utc));
    }

    // Try parsing as full datetime (ISO 8601)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    anyhow::bail!("Invalid date format. Use YYYY-MM-DD or ISO 8601 format (YYYY-MM-DDTHH:MM:SS)")
}
