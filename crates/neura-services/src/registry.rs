use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use chrono::Utc;

use crate::daemon::{ServiceDaemon, ServiceStatus};

/// Registry of all system services/daemons.
pub struct ServiceRegistry {
    services: Arc<RwLock<HashMap<String, ServiceDaemon>>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new service.
    pub async fn register(&self, daemon: ServiceDaemon) {
        info!("Registered service: {}", daemon.name);
        self.services.write().await.insert(daemon.name.clone(), daemon);
    }

    /// Start a service by name.
    pub async fn start(&self, name: &str) -> Result<(), String> {
        let mut services = self.services.write().await;
        let daemon = services.get_mut(name)
            .ok_or_else(|| format!("Service not found: {}", name))?;

        // Check dependencies
        let deps = daemon.depends_on.clone();
        for dep in &deps {
            if let Some(dep_svc) = services.get(dep.as_str()) {
                if !dep_svc.is_running() {
                    return Err(format!("Dependency '{}' is not running", dep));
                }
            } else {
                return Err(format!("Dependency '{}' not registered", dep));
            }
        }

        let daemon = services.get_mut(name).unwrap();
        daemon.status = ServiceStatus::Running;
        daemon.started_at = Some(Utc::now());
        info!("Started service: {}", name);
        Ok(())
    }

    /// Stop a service by name.
    pub async fn stop(&self, name: &str) -> Result<(), String> {
        let mut services = self.services.write().await;
        let daemon = services.get_mut(name)
            .ok_or_else(|| format!("Service not found: {}", name))?;

        daemon.status = ServiceStatus::Stopped;
        info!("Stopped service: {}", name);
        Ok(())
    }

    /// Get the status of a service.
    pub async fn status(&self, name: &str) -> Option<ServiceStatus> {
        let services = self.services.read().await;
        services.get(name).map(|d| d.status.clone())
    }

    /// List all registered services.
    pub async fn list(&self) -> Vec<ServiceDaemon> {
        let services = self.services.read().await;
        services.values().cloned().collect()
    }

    /// Start all services respecting dependency order.
    pub async fn start_all(&self) -> Vec<String> {
        let services = self.services.read().await;
        let names: Vec<String> = services.keys().cloned().collect();
        drop(services);

        let mut errors = Vec::new();
        // Simple topological attempt: try starting each, retry failed ones
        let mut remaining = names;
        let mut max_attempts = remaining.len() * 2;
        while !remaining.is_empty() && max_attempts > 0 {
            let mut still_remaining = Vec::new();
            for name in &remaining {
                if let Err(e) = self.start(name).await {
                    still_remaining.push(name.clone());
                    if max_attempts <= 1 {
                        errors.push(format!("{}: {}", name, e));
                    }
                }
            }
            remaining = still_remaining;
            max_attempts -= 1;
        }
        errors
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
