use super::connection::{Database, DbResult};
use tracing::info;

pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub struct MigrationRunner {
    migrations: Vec<Migration>,
}

impl MigrationRunner {
    pub fn new() -> Self {
        Self {
            migrations: vec![
                Migration {
                    version: 1,
                    name: "initial_schema",
                    sql: "
                        CREATE TABLE IF NOT EXISTS users (
                            id TEXT PRIMARY KEY,
                            username TEXT UNIQUE NOT NULL,
                            password_hash TEXT NOT NULL,
                            role TEXT NOT NULL DEFAULT 'user',
                            created_at TEXT NOT NULL,
                            updated_at TEXT NOT NULL
                        );

                        CREATE TABLE IF NOT EXISTS ai_memory (
                            id TEXT PRIMARY KEY,
                            user_id TEXT NOT NULL,
                            memory_type TEXT NOT NULL,
                            key TEXT NOT NULL,
                            value TEXT NOT NULL,
                            created_at TEXT NOT NULL,
                            expires_at TEXT,
                            FOREIGN KEY (user_id) REFERENCES users(id)
                        );

                        CREATE TABLE IF NOT EXISTS app_state (
                            id TEXT PRIMARY KEY,
                            app_id TEXT NOT NULL,
                            user_id TEXT NOT NULL,
                            state_json TEXT NOT NULL,
                            updated_at TEXT NOT NULL,
                            FOREIGN KEY (user_id) REFERENCES users(id)
                        );

                        CREATE TABLE IF NOT EXISTS packages (
                            id TEXT PRIMARY KEY,
                            name TEXT UNIQUE NOT NULL,
                            version TEXT NOT NULL,
                            installed_at TEXT NOT NULL,
                            signature TEXT
                        );

                        CREATE TABLE IF NOT EXISTS schema_version (
                            version INTEGER PRIMARY KEY,
                            applied_at TEXT NOT NULL
                        );
                    ",
                },
            ],
        }
    }

    pub async fn run(&self, db: &Database) -> DbResult<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );"
        ).await?;

        let current_version = {
            let conn = db.lock().await;
            conn.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get::<_, u32>(0),
            ).unwrap_or(0)
        };

        for migration in &self.migrations {
            if migration.version > current_version {
                info!("Running migration v{}: {}", migration.version, migration.name);
                db.execute_batch(migration.sql).await?;
                db.execute(
                    "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
                    &[&migration.version],
                ).await?;
            }
        }

        Ok(())
    }
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self::new()
    }
}
