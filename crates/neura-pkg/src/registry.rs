use serde::{Serialize, Deserialize};
use neura_storage::db::Database;
use chrono::Utc;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum PkgError {
    #[error("Package not found: {0}")]
    NotFound(String),
    #[error("Already installed: {0}")]
    AlreadyInstalled(String),
    #[error("Database error: {0}")]
    Db(String),
}

pub type PkgResult<T> = Result<T, PkgError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub installed_at: String,
    pub signature: Option<String>,
}

pub struct PackageRegistry {
    db: Database,
}

impl PackageRegistry {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn install(&self, name: &str, version: &str) -> PkgResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT INTO packages (id, name, version, installed_at) VALUES (?1, ?2, ?3, ?4)",
            &[&id, &name, &version, &now],
        ).await.map_err(|e| PkgError::Db(e.to_string()))?;
        info!("Installed package: {} v{}", name, version);
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> PkgResult<()> {
        let count = self.db.execute(
            "DELETE FROM packages WHERE name = ?1",
            &[&name],
        ).await.map_err(|e| PkgError::Db(e.to_string()))?;
        if count == 0 {
            return Err(PkgError::NotFound(name.to_string()));
        }
        info!("Removed package: {}", name);
        Ok(())
    }

    pub async fn is_installed(&self, name: &str) -> PkgResult<bool> {
        let conn = self.db.lock().await;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM packages WHERE name = ?1",
            [name],
            |row| row.get(0),
        ).map_err(|e| PkgError::Db(e.to_string()))?;
        Ok(count > 0)
    }
}
