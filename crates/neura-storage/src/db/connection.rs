use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use rusqlite::Connection;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Query error: {0}")]
    Query(String),
}

pub type DbResult<T> = Result<T, DbError>;

/// Thread-safe SQLite database wrapper.
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> DbResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        info!("Database opened at {}", path.display());
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Get a lock on the underlying connection to execute queries.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.conn.lock().await
    }

    /// Execute a statement that doesn't return rows.
    pub async fn execute(&self, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> DbResult<usize> {
        let conn = self.conn.lock().await;
        let count = conn.execute(sql, params)?;
        Ok(count)
    }

    /// Execute a batch of SQL statements.
    pub async fn execute_batch(&self, sql: &str) -> DbResult<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(sql)?;
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}
