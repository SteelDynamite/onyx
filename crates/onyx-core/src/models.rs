use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Backlog,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub has_time: bool,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
}

impl Task {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            description: String::new(),
            status: TaskStatus::Backlog,
            due_date: None,
            has_time: false,
            version: 0,
            parent_id: None,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn with_due_date(mut self, due_date: DateTime<Utc>) -> Self {
        self.due_date = Some(due_date);
        self
    }

    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn complete(&mut self) {
        self.status = TaskStatus::Completed;
    }

    pub fn uncomplete(&mut self) {
        self.status = TaskStatus::Backlog;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskList {
    pub id: Uuid,
    pub title: String,
    pub tasks: Vec<Task>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub group_by_due_date: bool,
}

impl TaskList {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            tasks: Vec::new(),
            created_at: now,
            updated_at: now,
            group_by_due_date: false,
        }
    }

    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
        self.updated_at = Utc::now();
    }

    pub fn remove_task(&mut self, task_id: Uuid) -> Option<Task> {
        if let Some(pos) = self.tasks.iter().position(|t| t.id == task_id) {
            self.updated_at = Utc::now();
            Some(self.tasks.remove(pos))
        } else {
            None
        }
    }

    pub fn get_task(&self, task_id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    pub fn get_task_mut(&mut self, task_id: Uuid) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == task_id)
    }

    pub fn update_task(&mut self, task: Task) -> bool {
        if let Some(existing) = self.get_task_mut(task.id) {
            *existing = task;
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }
}
