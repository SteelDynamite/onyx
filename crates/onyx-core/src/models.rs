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

#[cfg(test)]
mod tests {
    use super::*;

    // --- TaskStatus tests ---

    #[test]
    fn test_task_status_serde_roundtrip() {
        let json = serde_json::to_string(&TaskStatus::Backlog).unwrap();
        assert_eq!(json, "\"backlog\"");
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaskStatus::Backlog);

        let json = serde_json::to_string(&TaskStatus::Completed).unwrap();
        assert_eq!(json, "\"completed\"");
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TaskStatus::Completed);
    }

    #[test]
    fn test_task_status_equality() {
        assert_eq!(TaskStatus::Backlog, TaskStatus::Backlog);
        assert_eq!(TaskStatus::Completed, TaskStatus::Completed);
        assert_ne!(TaskStatus::Backlog, TaskStatus::Completed);
    }

    // --- Task tests ---

    #[test]
    fn test_task_new_defaults() {
        let task = Task::new("My Task".to_string());
        assert_eq!(task.title, "My Task");
        assert_eq!(task.description, "");
        assert_eq!(task.status, TaskStatus::Backlog);
        assert!(task.due_date.is_none());
        assert!(!task.has_time);
        assert_eq!(task.version, 0);
        assert!(task.parent_id.is_none());
    }

    #[test]
    fn test_task_with_description() {
        let task = Task::new("T".to_string())
            .with_description("Some notes".to_string());
        assert_eq!(task.description, "Some notes");
    }

    #[test]
    fn test_task_with_due_date() {
        let dt = Utc::now();
        let task = Task::new("T".to_string()).with_due_date(dt);
        assert_eq!(task.due_date, Some(dt));
    }

    #[test]
    fn test_task_with_parent() {
        let parent_id = Uuid::new_v4();
        let task = Task::new("Sub".to_string()).with_parent(parent_id);
        assert_eq!(task.parent_id, Some(parent_id));
    }

    #[test]
    fn test_task_complete_and_uncomplete() {
        let mut task = Task::new("T".to_string());
        assert_eq!(task.status, TaskStatus::Backlog);

        task.complete();
        assert_eq!(task.status, TaskStatus::Completed);

        task.uncomplete();
        assert_eq!(task.status, TaskStatus::Backlog);
    }

    #[test]
    fn test_task_builder_chaining() {
        let parent_id = Uuid::new_v4();
        let dt = Utc::now();
        let task = Task::new("Chained".to_string())
            .with_description("Desc".to_string())
            .with_due_date(dt)
            .with_parent(parent_id);

        assert_eq!(task.title, "Chained");
        assert_eq!(task.description, "Desc");
        assert_eq!(task.due_date, Some(dt));
        assert_eq!(task.parent_id, Some(parent_id));
    }

    #[test]
    fn test_task_unique_ids() {
        let t1 = Task::new("A".to_string());
        let t2 = Task::new("B".to_string());
        assert_ne!(t1.id, t2.id);
    }

    #[test]
    fn test_task_serde_roundtrip() {
        let parent_id = Uuid::new_v4();
        let task = Task::new("Serde".to_string())
            .with_description("Desc".to_string())
            .with_parent(parent_id);
        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.title, "Serde");
        assert_eq!(parsed.description, "Desc");
        assert_eq!(parsed.parent_id, Some(parent_id));
    }

    #[test]
    fn test_task_serde_skips_none_fields() {
        let task = Task::new("Minimal".to_string());
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("due_date"));
        assert!(!json.contains("parent_id"));
    }

    // --- TaskList tests ---

    #[test]
    fn test_task_list_new_defaults() {
        let list = TaskList::new("My List".to_string());
        assert_eq!(list.title, "My List");
        assert!(list.tasks.is_empty());
        assert!(!list.group_by_due_date);
        assert!(list.created_at <= Utc::now());
        assert!(list.updated_at <= Utc::now());
    }

    #[test]
    fn test_task_list_add_task() {
        let mut list = TaskList::new("L".to_string());
        let before = list.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));

        let task = Task::new("T".to_string());
        let task_id = task.id;
        list.add_task(task);

        assert_eq!(list.tasks.len(), 1);
        assert_eq!(list.tasks[0].id, task_id);
        assert!(list.updated_at >= before);
    }

    #[test]
    fn test_task_list_remove_task() {
        let mut list = TaskList::new("L".to_string());
        let task = Task::new("T".to_string());
        let task_id = task.id;
        list.add_task(task);

        let removed = list.remove_task(task_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, task_id);
        assert!(list.tasks.is_empty());
    }

    #[test]
    fn test_task_list_remove_nonexistent_task() {
        let mut list = TaskList::new("L".to_string());
        let removed = list.remove_task(Uuid::new_v4());
        assert!(removed.is_none());
    }

    #[test]
    fn test_task_list_get_task() {
        let mut list = TaskList::new("L".to_string());
        let task = Task::new("T".to_string());
        let task_id = task.id;
        list.add_task(task);

        assert!(list.get_task(task_id).is_some());
        assert_eq!(list.get_task(task_id).unwrap().title, "T");
        assert!(list.get_task(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_task_list_get_task_mut() {
        let mut list = TaskList::new("L".to_string());
        let task = Task::new("T".to_string());
        let task_id = task.id;
        list.add_task(task);

        let t = list.get_task_mut(task_id).unwrap();
        t.title = "Modified".to_string();

        assert_eq!(list.get_task(task_id).unwrap().title, "Modified");
    }

    #[test]
    fn test_task_list_update_task() {
        let mut list = TaskList::new("L".to_string());
        let task = Task::new("Old".to_string());
        let task_id = task.id;
        list.add_task(task);

        let mut updated = Task::new("New".to_string());
        updated.id = task_id;
        assert!(list.update_task(updated));
        assert_eq!(list.get_task(task_id).unwrap().title, "New");
    }

    #[test]
    fn test_task_list_update_nonexistent_task() {
        let mut list = TaskList::new("L".to_string());
        let task = Task::new("Ghost".to_string());
        assert!(!list.update_task(task));
    }

    #[test]
    fn test_task_list_unique_ids() {
        let l1 = TaskList::new("A".to_string());
        let l2 = TaskList::new("B".to_string());
        assert_ne!(l1.id, l2.id);
    }
}
