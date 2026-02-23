use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub app_id: String,
    pub values: HashMap<String, Value>,
}

impl AppConfig {
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            values: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).and_then(|v| v.as_str().map(String::from))
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }
}
