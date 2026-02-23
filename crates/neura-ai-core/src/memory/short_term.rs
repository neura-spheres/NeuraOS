use std::collections::HashMap;

/// In-memory session-scoped key-value store.
pub struct ShortTermMemory {
    data: HashMap<String, String>,
    history: Vec<String>,
    max_history: usize,
}

impl ShortTermMemory {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            history: Vec::new(),
            max_history: 100,
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.history.push(format!("{}: {}", key, value));
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    pub fn recent_entries(&self, n: usize) -> Vec<String> {
        self.history.iter().rev().take(n).cloned().collect()
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.history.clear();
    }
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new()
    }
}
