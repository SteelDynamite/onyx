use std::path::PathBuf;
use uuid::Uuid;
use crate::error::{Error, Result};
use crate::models::{Task, TaskList};
use crate::storage::{FileSystemStorage, Storage};

pub struct TaskRepository {
    storage: Box<dyn Storage + Send + Sync>,
}

impl TaskRepository {
    pub fn new(tasks_folder: PathBuf) -> Result<Self> {
        let storage = FileSystemStorage::new(tasks_folder)?;
        Ok(Self {
            storage: Box::new(storage),
        })
    }

    pub fn init(tasks_folder: PathBuf) -> Result<Self> {
        let storage = FileSystemStorage::init(tasks_folder)?;
        Ok(Self {
            storage: Box::new(storage),
        })
    }

    // Task operations
    pub fn create_task(&mut self, list_id: Uuid, mut task: Task) -> Result<Task> {
        self.storage.write_task(list_id, &task)?;
        task.version += 1;
        Ok(task)
    }

    pub fn get_task(&self, list_id: Uuid, task_id: Uuid) -> Result<Task> {
        self.storage.read_task(list_id, task_id)
    }

    pub fn update_task(&mut self, list_id: Uuid, task: Task) -> Result<()> {
        // Verify task exists first
        let _ = self.storage.read_task(list_id, task.id)?;
        self.storage.write_task(list_id, &task)?;
        Ok(())
    }

    pub fn delete_task(&mut self, list_id: Uuid, task_id: Uuid) -> Result<()> {
        self.storage.delete_task(list_id, task_id)
    }

    pub fn list_tasks(&self, list_id: Uuid) -> Result<Vec<Task>> {
        self.storage.list_tasks(list_id)
    }

    // List operations
    pub fn create_list(&mut self, name: String) -> Result<TaskList> {
        self.storage.create_list(name)
    }

    pub fn get_lists(&self) -> Result<Vec<TaskList>> {
        self.storage.get_lists()
    }

    pub fn get_list(&self, list_id: Uuid) -> Result<TaskList> {
        let lists = self.get_lists()?;
        lists.into_iter()
            .find(|list| list.id == list_id)
            .ok_or_else(|| Error::ListNotFound(list_id.to_string()))
    }

    pub fn delete_list(&mut self, list_id: Uuid) -> Result<()> {
        self.storage.delete_list(list_id)
    }

    pub fn rename_list(&mut self, list_id: Uuid, new_name: String) -> Result<()> {
        self.storage.rename_list(list_id, new_name)
    }

    pub fn move_task(&mut self, from_list_id: Uuid, to_list_id: Uuid, task_id: Uuid) -> Result<()> {
        let task = self.storage.read_task(from_list_id, task_id)?;
        self.storage.write_task(to_list_id, &task)?;
        // If delete from source fails, roll back by removing the copy from destination
        if let Err(e) = self.storage.delete_task(from_list_id, task_id) {
            if let Err(rollback_err) = self.storage.delete_task(to_list_id, task_id) {
                // Rollback failed — task now exists in both lists.
                // Return an error describing the inconsistent state.
                return Err(Error::InvalidData(format!(
                    "move_task failed and rollback also failed: original error: {}, rollback error: {}. Task {} may exist in both lists.",
                    e, rollback_err, task_id
                )));
            }
            return Err(e);
        }
        Ok(())
    }

    // Task ordering
    pub fn reorder_task(&mut self, list_id: Uuid, task_id: Uuid, new_position: usize) -> Result<()> {
        let mut metadata = self.storage.read_list_metadata(list_id)?;

        // Find current position
        let current_pos = metadata.task_order.iter().position(|&id| id == task_id)
            .ok_or_else(|| Error::TaskNotFound(task_id.to_string()))?;

        // Remove from current position
        metadata.task_order.remove(current_pos);

        // Insert at new position
        let new_pos = new_position.min(metadata.task_order.len());
        metadata.task_order.insert(new_pos, task_id);

        metadata.updated_at = chrono::Utc::now();
        self.storage.write_list_metadata(&metadata)?;

        Ok(())
    }

    pub fn get_task_order(&self, list_id: Uuid) -> Result<Vec<Uuid>> {
        let metadata = self.storage.read_list_metadata(list_id)?;
        Ok(metadata.task_order)
    }

    // Grouping preference
    pub fn set_group_by_due_date(&mut self, list_id: Uuid, enabled: bool) -> Result<()> {
        let mut metadata = self.storage.read_list_metadata(list_id)?;
        metadata.group_by_due_date = enabled;
        metadata.updated_at = chrono::Utc::now();
        self.storage.write_list_metadata(&metadata)?;
        Ok(())
    }

    pub fn get_group_by_due_date(&self, list_id: Uuid) -> Result<bool> {
        let metadata = self.storage.read_list_metadata(list_id)?;
        Ok(metadata.group_by_due_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_repository() {
        let temp_dir = TempDir::new().unwrap();
        let repo = TaskRepository::init(temp_dir.path().to_path_buf());
        assert!(repo.is_ok());
    }

    #[test]
    fn test_create_and_list_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        // Create a list
        let list = repo.create_list("Test List".to_string()).unwrap();

        // Create a task
        let task = Task::new("Test Task".to_string());
        let created_task = repo.create_task(list.id, task).unwrap();

        // List tasks
        let tasks = repo.list_tasks(list.id).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Test Task");
    }

    #[test]
    fn test_update_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list = repo.create_list("Test List".to_string()).unwrap();
        let mut task = Task::new("Original".to_string());
        task = repo.create_task(list.id, task).unwrap();

        task.title = "Updated".to_string();
        repo.update_task(list.id, task.clone()).unwrap();

        let retrieved = repo.get_task(list.id, task.id).unwrap();
        assert_eq!(retrieved.title, "Updated");
    }

    #[test]
    fn test_delete_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list = repo.create_list("Test List".to_string()).unwrap();
        let task = Task::new("To Delete".to_string());
        let task = repo.create_task(list.id, task).unwrap();

        repo.delete_task(list.id, task.id).unwrap();

        let tasks = repo.list_tasks(list.id).unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_reorder_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list = repo.create_list("Test List".to_string()).unwrap();

        let task1 = repo.create_task(list.id, Task::new("Task 1".to_string())).unwrap();
        let task2 = repo.create_task(list.id, Task::new("Task 2".to_string())).unwrap();
        let task3 = repo.create_task(list.id, Task::new("Task 3".to_string())).unwrap();

        // Move task3 to position 0
        repo.reorder_task(list.id, task3.id, 0).unwrap();

        let order = repo.get_task_order(list.id).unwrap();
        assert_eq!(order[0], task3.id);
        assert_eq!(order[1], task1.id);
        assert_eq!(order[2], task2.id);
    }

    #[test]
    fn test_group_by_due_date() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list = repo.create_list("Test List".to_string()).unwrap();

        assert!(!repo.get_group_by_due_date(list.id).unwrap());

        repo.set_group_by_due_date(list.id, true).unwrap();
        assert!(repo.get_group_by_due_date(list.id).unwrap());

        repo.set_group_by_due_date(list.id, false).unwrap();
        assert!(!repo.get_group_by_due_date(list.id).unwrap());
    }

    // --- Error path tests ---

    #[test]
    fn test_get_task_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let result = repo.get_task(list.id, Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::TaskNotFound(_)));
    }

    #[test]
    fn test_update_nonexistent_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let task = Task::new("Ghost".to_string());
        let result = repo.update_task(list.id, task);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::TaskNotFound(_)));
    }

    #[test]
    fn test_delete_nonexistent_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let result = repo.delete_task(list.id, Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::TaskNotFound(_)));
    }

    #[test]
    fn test_get_list_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let result = repo.get_list(Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ListNotFound(_)));
    }

    #[test]
    fn test_delete_nonexistent_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let result = repo.delete_list(Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ListNotFound(_)));
    }

    #[test]
    fn test_list_tasks_nonexistent_list() {
        let temp_dir = TempDir::new().unwrap();
        let repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let result = repo.list_tasks(Uuid::new_v4());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ListNotFound(_)));
    }

    #[test]
    fn test_reorder_task_not_in_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();
        repo.create_task(list.id, Task::new("A".to_string())).unwrap();

        let result = repo.reorder_task(list.id, Uuid::new_v4(), 0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::TaskNotFound(_)));
    }

    #[test]
    fn test_reorder_task_position_clamped() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let t1 = repo.create_task(list.id, Task::new("A".to_string())).unwrap();
        let t2 = repo.create_task(list.id, Task::new("B".to_string())).unwrap();

        // Position 999 should clamp to end
        repo.reorder_task(list.id, t1.id, 999).unwrap();
        let order = repo.get_task_order(list.id).unwrap();
        assert_eq!(order[0], t2.id);
        assert_eq!(order[1], t1.id);
    }

    #[test]
    fn test_create_duplicate_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        repo.create_list("Dupes".to_string()).unwrap();

        let result = repo.create_list("Dupes".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_get_lists_empty() {
        let temp_dir = TempDir::new().unwrap();
        let repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let lists = repo.get_lists().unwrap();
        assert!(lists.is_empty());
    }

    #[test]
    fn test_move_task_between_lists() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list_a = repo.create_list("List A".to_string()).unwrap();
        let list_b = repo.create_list("List B".to_string()).unwrap();
        let task = repo.create_task(list_a.id, Task::new("Movable".to_string())).unwrap();

        repo.move_task(list_a.id, list_b.id, task.id).unwrap();

        let tasks_a = repo.list_tasks(list_a.id).unwrap();
        assert_eq!(tasks_a.len(), 0);

        let tasks_b = repo.list_tasks(list_b.id).unwrap();
        assert_eq!(tasks_b.len(), 1);
        assert_eq!(tasks_b[0].title, "Movable");
    }

    #[test]
    fn test_rename_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list = repo.create_list("Old Name".to_string()).unwrap();
        repo.rename_list(list.id, "New Name".to_string()).unwrap();

        let renamed = repo.get_list(list.id).unwrap();
        assert_eq!(renamed.title, "New Name");

        // Old directory should be gone
        assert!(!temp_dir.path().join("Old Name").exists());
        assert!(temp_dir.path().join("New Name").exists());
    }

    #[test]
    fn test_rename_list_duplicate_name() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        repo.create_list("A".to_string()).unwrap();
        let list_b = repo.create_list("B".to_string()).unwrap();

        let result = repo.rename_list(list_b.id, "A".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidData(_)));
    }

    #[test]
    fn test_delete_list_removes_from_root_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list1 = repo.create_list("A".to_string()).unwrap();
        let list2 = repo.create_list("B".to_string()).unwrap();

        repo.delete_list(list1.id).unwrap();

        let lists = repo.get_lists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, list2.id);
    }

    #[test]
    fn test_new_on_nonexistent_path() {
        let result = TaskRepository::new(PathBuf::from("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_err());
    }

    #[test]
    fn test_task_with_description_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let task = Task::new("Has Description".to_string())
            .with_description("Some **markdown** notes".to_string());
        let created = repo.create_task(list.id, task).unwrap();

        let retrieved = repo.get_task(list.id, created.id).unwrap();
        assert_eq!(retrieved.description, "Some **markdown** notes");
    }

    #[test]
    fn test_task_rename_removes_old_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let mut task = repo.create_task(list.id, Task::new("Old Name".to_string())).unwrap();
        task.title = "New Name".to_string();
        repo.update_task(list.id, task.clone()).unwrap();

        // Old file should be gone, new file should exist
        let tasks = repo.list_tasks(list.id).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "New Name");

        // Verify old .md file no longer on disk
        let old_path = temp_dir.path().join("Test").join("Old Name.md");
        assert!(!old_path.exists());
    }

    #[test]
    fn test_create_task_increments_version() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let task = Task::new("V".to_string());
        assert_eq!(task.version, 0);
        let created = repo.create_task(list.id, task).unwrap();
        assert_eq!(created.version, 1);
    }

    #[test]
    fn test_move_task_nonexistent_task() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list_a = repo.create_list("A".to_string()).unwrap();
        let list_b = repo.create_list("B".to_string()).unwrap();

        let result = repo.move_task(list_a.id, list_b.id, Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_move_task_preserves_task_data() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list_a = repo.create_list("A".to_string()).unwrap();
        let list_b = repo.create_list("B".to_string()).unwrap();

        let task = Task::new("Rich Task".to_string())
            .with_description("Important notes".to_string());
        let task = repo.create_task(list_a.id, task).unwrap();
        let task_id = task.id;

        repo.move_task(list_a.id, list_b.id, task_id).unwrap();

        let moved = repo.get_task(list_b.id, task_id).unwrap();
        assert_eq!(moved.title, "Rich Task");
        assert_eq!(moved.description, "Important notes");
    }

    #[test]
    fn test_subtask_creation_and_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let parent = repo.create_task(list.id, Task::new("Parent".to_string())).unwrap();
        let child = Task::new("Child".to_string()).with_parent(parent.id);
        let child = repo.create_task(list.id, child).unwrap();

        let retrieved = repo.get_task(list.id, child.id).unwrap();
        assert_eq!(retrieved.parent_id, Some(parent.id));
    }

    #[test]
    fn test_multiple_lists_independent() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();

        let list_a = repo.create_list("A".to_string()).unwrap();
        let list_b = repo.create_list("B".to_string()).unwrap();

        repo.create_task(list_a.id, Task::new("Task A1".to_string())).unwrap();
        repo.create_task(list_a.id, Task::new("Task A2".to_string())).unwrap();
        repo.create_task(list_b.id, Task::new("Task B1".to_string())).unwrap();

        assert_eq!(repo.list_tasks(list_a.id).unwrap().len(), 2);
        assert_eq!(repo.list_tasks(list_b.id).unwrap().len(), 1);
    }

    #[test]
    fn test_task_order_after_delete() {
        let temp_dir = TempDir::new().unwrap();
        let mut repo = TaskRepository::init(temp_dir.path().to_path_buf()).unwrap();
        let list = repo.create_list("Test".to_string()).unwrap();

        let t1 = repo.create_task(list.id, Task::new("A".to_string())).unwrap();
        let t2 = repo.create_task(list.id, Task::new("B".to_string())).unwrap();
        let t3 = repo.create_task(list.id, Task::new("C".to_string())).unwrap();

        repo.delete_task(list.id, t2.id).unwrap();

        let order = repo.get_task_order(list.id).unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0], t1.id);
        assert_eq!(order[1], t3.id);
    }
}
