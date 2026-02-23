use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed(String),
    Restarting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
    OnFailureMax(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDaemon {
    pub name: String,
    pub status: ServiceStatus,
    pub restart_policy: RestartPolicy,
    pub depends_on: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub max_restarts: u32,
}

impl ServiceDaemon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ServiceStatus::Stopped,
            restart_policy: RestartPolicy::OnFailure,
            depends_on: Vec::new(),
            started_at: None,
            restart_count: 0,
            max_restarts: 5,
        }
    }

    pub fn with_dependency(mut self, dep: impl Into<String>) -> Self {
        self.depends_on.push(dep.into());
        self
    }

    pub fn with_restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    pub fn should_restart(&self) -> bool {
        match &self.restart_policy {
            RestartPolicy::Never => false,
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => matches!(self.status, ServiceStatus::Failed(_)),
            RestartPolicy::OnFailureMax(max) => {
                matches!(self.status, ServiceStatus::Failed(_)) && self.restart_count < *max
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == ServiceStatus::Running
    }
}
