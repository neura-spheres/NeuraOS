use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use chrono::Utc;

use crate::task::{TaskId, TaskInfo, TaskStatus, TaskHandle};

pub struct TaskScheduler {
    tasks: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    _max_concurrent: usize,
}

impl TaskScheduler {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            _max_concurrent: max_concurrent,
        }
    }

    /// Spawn a new async task.
    pub async fn spawn<F>(&self, name: &str, owner: &str, fut: F) -> TaskId
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut info = TaskInfo::new(name, owner);
        info.status = TaskStatus::Running;
        info.started_at = Some(Utc::now());
        let id = info.id.clone();

        let join_handle = tokio::spawn(fut);

        let handle = TaskHandle {
            info,
            join_handle: Some(join_handle),
        };

        self.tasks.write().await.insert(id.clone(), handle);
        info!("Task spawned: {} ({})", name, id);
        id
    }

    /// Get info about a task.
    pub async fn get_task_info(&self, id: &str) -> Option<TaskInfo> {
        let tasks = self.tasks.read().await;
        tasks.get(id).map(|h| h.info.clone())
    }

    /// List all tasks.
    pub async fn list_tasks(&self) -> Vec<TaskInfo> {
        let tasks = self.tasks.read().await;
        tasks.values().map(|h| h.info.clone()).collect()
    }

    /// List tasks for a specific user.
    pub async fn list_user_tasks(&self, owner: &str) -> Vec<TaskInfo> {
        let tasks = self.tasks.read().await;
        tasks.values()
            .filter(|h| h.info.owner == owner)
            .map(|h| h.info.clone())
            .collect()
    }

    /// Cancel a task.
    pub async fn cancel(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(handle) = tasks.get_mut(id) {
            handle.abort();
            handle.info.status = TaskStatus::Cancelled;
            handle.info.finished_at = Some(Utc::now());
            info!("Task cancelled: {}", id);
            true
        } else {
            warn!("Task not found for cancel: {}", id);
            false
        }
    }

    /// Clean up finished tasks.
    pub async fn cleanup_finished(&self) {
        let mut tasks = self.tasks.write().await;
        let finished: Vec<TaskId> = tasks.iter()
            .filter(|(_, h)| h.is_finished() && matches!(h.info.status, TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Cancelled))
            .map(|(id, _)| id.clone())
            .collect();

        for id in finished {
            tasks.remove(&id);
        }
    }

    /// Number of currently running tasks.
    pub async fn running_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.values()
            .filter(|h| matches!(h.info.status, TaskStatus::Running))
            .count()
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(64)
    }
}
