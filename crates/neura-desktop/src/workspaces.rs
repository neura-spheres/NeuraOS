use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub window_ids: Vec<String>,
}

pub struct WorkspaceManager {
    workspaces: HashMap<String, Workspace>,
    active: String,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        let mut workspaces = HashMap::new();
        workspaces.insert("main".to_string(), Workspace {
            name: "main".to_string(),
            window_ids: Vec::new(),
        });
        Self {
            workspaces,
            active: "main".to_string(),
        }
    }

    pub fn create(&mut self, name: &str) {
        self.workspaces.insert(name.to_string(), Workspace {
            name: name.to_string(),
            window_ids: Vec::new(),
        });
    }

    pub fn switch(&mut self, name: &str) -> bool {
        if self.workspaces.contains_key(name) {
            self.active = name.to_string();
            true
        } else {
            false
        }
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn list(&self) -> Vec<&str> {
        self.workspaces.keys().map(|s| s.as_str()).collect()
    }

    pub fn add_window_to_active(&mut self, window_id: &str) {
        if let Some(ws) = self.workspaces.get_mut(&self.active) {
            ws.window_ids.push(window_id.to_string());
        }
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}
