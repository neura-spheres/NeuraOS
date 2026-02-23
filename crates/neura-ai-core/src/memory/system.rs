use std::collections::HashSet;

/// Tracks system-level state visible to the AI agent.
pub struct SystemMemory {
    running_apps: HashSet<String>,
    active_files: Vec<String>,
    user_preferences: std::collections::HashMap<String, String>,
}

impl SystemMemory {
    pub fn new() -> Self {
        Self {
            running_apps: HashSet::new(),
            active_files: Vec::new(),
            user_preferences: std::collections::HashMap::new(),
        }
    }

    pub fn register_app(&mut self, app_name: &str) {
        self.running_apps.insert(app_name.to_string());
    }

    pub fn unregister_app(&mut self, app_name: &str) {
        self.running_apps.remove(app_name);
    }

    pub fn running_apps(&self) -> Vec<String> {
        self.running_apps.iter().cloned().collect()
    }

    pub fn set_active_file(&mut self, path: &str) {
        if !self.active_files.contains(&path.to_string()) {
            self.active_files.push(path.to_string());
        }
    }

    pub fn active_files(&self) -> &[String] {
        &self.active_files
    }

    pub fn set_preference(&mut self, key: &str, value: &str) {
        self.user_preferences.insert(key.to_string(), value.to_string());
    }

    pub fn get_preference(&self, key: &str) -> Option<&String> {
        self.user_preferences.get(key)
    }
}

impl Default for SystemMemory {
    fn default() -> Self {
        Self::new()
    }
}
