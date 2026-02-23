use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JournalOp {
    Mkdir,
    WriteFile,
    Remove,
    Rename,
    Chmod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: DateTime<Utc>,
    pub op: JournalOp,
    pub path: String,
    pub detail: String,
    pub actor: String,
}

impl JournalEntry {
    pub fn mkdir(path: String, actor: String) -> Self {
        Self {
            timestamp: Utc::now(),
            op: JournalOp::Mkdir,
            path,
            detail: String::new(),
            actor,
        }
    }

    pub fn write_file(path: String, size: usize, actor: String) -> Self {
        Self {
            timestamp: Utc::now(),
            op: JournalOp::WriteFile,
            path,
            detail: format!("{} bytes", size),
            actor,
        }
    }

    pub fn remove(path: String) -> Self {
        Self {
            timestamp: Utc::now(),
            op: JournalOp::Remove,
            path,
            detail: String::new(),
            actor: String::new(),
        }
    }
}

/// Append-only journal for VFS operations.
pub struct Journal {
    entries: Vec<JournalEntry>,
    max_entries: usize,
}

impl Journal {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 10_000,
        }
    }

    pub fn record(&mut self, entry: JournalEntry) {
        self.entries.push(entry);
        // GC: trim old entries when over limit
        if self.entries.len() > self.max_entries {
            let drain_count = self.entries.len() - self.max_entries;
            self.entries.drain(..drain_count);
        }
    }

    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}
