use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use tokio::task::JoinHandle;

pub type TaskId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: TaskId,
    pub name: String,
    pub owner: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl TaskInfo {
    pub fn new(name: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            owner: owner.into(),
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }
}

/// A handle to a spawned async task.
pub struct TaskHandle {
    pub info: TaskInfo,
    pub join_handle: Option<JoinHandle<()>>,
}

impl TaskHandle {
    pub fn is_finished(&self) -> bool {
        self.join_handle.as_ref().map_or(true, |h| h.is_finished())
    }

    pub fn abort(&self) {
        if let Some(ref handle) = self.join_handle {
            handle.abort();
        }
    }
}
