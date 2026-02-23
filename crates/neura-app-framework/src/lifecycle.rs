use std::collections::HashMap;
use tracing::info;

use crate::app_trait::{App, AppId};

pub struct AppLifecycleManager {
    apps: HashMap<AppId, Box<dyn App>>,
}

impl AppLifecycleManager {
    pub fn new() -> Self {
        Self {
            apps: HashMap::new(),
        }
    }

    pub fn register(&mut self, app: Box<dyn App>) {
        let id = app.id().to_string();
        info!("Registered app: {}", id);
        self.apps.insert(id, app);
    }

    pub fn open(&mut self, id: &str) -> anyhow::Result<()> {
        if let Some(app) = self.apps.get_mut(id) {
            app.init()?;
            info!("Opened app: {}", id);
            Ok(())
        } else {
            anyhow::bail!("App not found: {}", id)
        }
    }

    pub fn close(&mut self, id: &str) {
        if let Some(app) = self.apps.get_mut(id) {
            app.on_close();
            info!("Closed app: {}", id);
        }
    }

    pub fn get(&self, id: &str) -> Option<&dyn App> {
        self.apps.get(id).map(|a| a.as_ref())
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Box<dyn App>> {
        self.apps.get_mut(id)
    }

    pub fn list_ids(&self) -> Vec<&str> {
        self.apps.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for AppLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}
