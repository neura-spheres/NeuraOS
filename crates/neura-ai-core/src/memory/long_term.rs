use neura_storage::db::{Database, DbError, DbResult};
use chrono::Utc;
use uuid::Uuid;

/// SQLite-backed persistent memory for AI agent.
pub struct LongTermMemory {
    db: Database,
    user_id: String,
}

impl LongTermMemory {
    pub fn new(db: Database, user_id: String) -> Self {
        Self { db, user_id }
    }

    /// Store a memory entry.
    pub async fn store(&self, key: &str, value: &str, memory_type: &str) -> DbResult<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT INTO ai_memory (id, user_id, memory_type, key, value, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[&id, &self.user_id, &memory_type, &key, &value, &now],
        ).await?;
        Ok(())
    }

    /// Retrieve a memory entry by key.
    pub async fn retrieve(&self, key: &str) -> DbResult<Option<String>> {
        let conn = self.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT value FROM ai_memory WHERE user_id = ?1 AND key = ?2 ORDER BY created_at DESC LIMIT 1"
        ).map_err(|e| DbError::Query(e.to_string()))?;

        let result = stmt.query_row(
            [&self.user_id, key],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("no rows") || msg.contains("QueryReturnedNoRows") {
                    Ok(None)
                } else {
                    Err(DbError::Query(msg))
                }
            }
        }
    }

    /// Search memories by key prefix.
    pub async fn search(&self, key_prefix: &str) -> DbResult<Vec<(String, String)>> {
        let conn = self.db.lock().await;
        let pattern = format!("{}%", key_prefix);
        let mut stmt = conn.prepare(
            "SELECT key, value FROM ai_memory WHERE user_id = ?1 AND key LIKE ?2 ORDER BY created_at DESC"
        ).map_err(|e| DbError::Query(e.to_string()))?;

        let mut results = Vec::new();
        let mut rows = stmt.query([&self.user_id, &pattern])
            .map_err(|e| DbError::Query(e.to_string()))?;

        while let Some(row) = rows.next().map_err(|e| DbError::Query(e.to_string()))? {
            let key: String = row.get(0).map_err(|e| DbError::Query(e.to_string()))?;
            let value: String = row.get(1).map_err(|e| DbError::Query(e.to_string()))?;
            results.push((key, value));
        }

        Ok(results)
    }

    /// Delete expired memory entries.
    pub async fn garbage_collect(&self) -> DbResult<usize> {
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "DELETE FROM ai_memory WHERE user_id = ?1 AND expires_at IS NOT NULL AND expires_at < ?2",
            &[&self.user_id, &now],
        ).await
    }
}
