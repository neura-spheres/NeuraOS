pub mod short_term;
pub mod long_term;
pub mod system;

use std::sync::Arc;
use tokio::sync::RwLock;
use short_term::ShortTermMemory;
use long_term::LongTermMemory;
use system::SystemMemory;
use neura_storage::db::Database;

/// Unified memory manager combining all three tiers.
pub struct MemoryManager {
    pub short_term: Arc<RwLock<ShortTermMemory>>,
    pub long_term: Arc<RwLock<LongTermMemory>>,
    pub system: Arc<RwLock<SystemMemory>>,
}

impl MemoryManager {
    /// Create a MemoryManager backed by an ephemeral in-memory SQLite database.
    /// Use this when you don't need persistent long-term memory (e.g. per-command AI calls).
    pub fn new_ephemeral() -> Self {
        let db = Database::open_in_memory().unwrap_or_else(|_| {
            // Fallback: shouldn't fail, but handle gracefully
            Database::open_in_memory().expect("in-memory db unavailable")
        });
        let lt = LongTermMemory::new(db, "ephemeral".to_string());
        Self {
            short_term: Arc::new(RwLock::new(ShortTermMemory::new())),
            long_term: Arc::new(RwLock::new(lt)),
            system: Arc::new(RwLock::new(SystemMemory::new())),
        }
    }

    pub fn new(long_term: LongTermMemory) -> Self {
        Self {
            short_term: Arc::new(RwLock::new(ShortTermMemory::new())),
            long_term: Arc::new(RwLock::new(long_term)),
            system: Arc::new(RwLock::new(SystemMemory::new())),
        }
    }

    /// Store a key-value in short-term memory.
    pub async fn store_short_term(&self, key: &str, value: &str) {
        self.short_term.write().await.set(key, value);
    }

    /// Retrieve from short-term memory.
    pub async fn get_short_term(&self, key: &str) -> Option<String> {
        self.short_term.read().await.get(key)
    }

    /// Get a context summary for AI prompts.
    pub async fn get_context_summary(&self) -> String {
        let st = self.short_term.read().await;
        let sys = self.system.read().await;
        let mut parts = Vec::new();

        let recent = st.recent_entries(5);
        if !recent.is_empty() {
            parts.push(format!("Recent context: {}", recent.join("; ")));
        }

        let running = sys.running_apps();
        if !running.is_empty() {
            parts.push(format!("Running apps: {}", running.join(", ")));
        }

        parts.join("\n")
    }
}
