use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub entry_point: String,
}

pub struct PluginRegistry {
    plugins: HashMap<String, PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, manifest: PluginManifest) {
        self.plugins.insert(manifest.id.clone(), manifest);
    }

    pub fn get(&self, id: &str) -> Option<&PluginManifest> {
        self.plugins.get(id)
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.values().collect()
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        self.plugins.remove(id).is_some()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
