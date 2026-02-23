use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use thiserror::Error;
use tracing::info;

use neura_storage::db::Database;
use crate::roles::Role;
use crate::password;

#[derive(Error, Debug)]
pub enum AccountError {
    #[error("User not found: {0}")]
    NotFound(String),
    #[error("Username already exists: {0}")]
    AlreadyExists(String),
    #[error("Database error: {0}")]
    Db(String),
    #[error("Password error: {0}")]
    Password(#[from] password::PasswordError),
}

pub type AccountResult<T> = Result<T, AccountError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persistent user store backed by SQLite.
pub struct UserStore {
    db: Database,
}

impl UserStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create a new user account.
    pub async fn create_user(&self, username: &str, plaintext_password: &str, role: Role) -> AccountResult<User> {
        // Check if username exists
        if self.find_by_username(username).await?.is_some() {
            return Err(AccountError::AlreadyExists(username.to_string()));
        }

        let id = Uuid::new_v4().to_string();
        let hash = password::hash_password(plaintext_password)?;
        let now = Utc::now();
        let role_str = role.to_string();

        self.db.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[&id, &username, &hash, &role_str, &now.to_rfc3339(), &now.to_rfc3339()],
        ).await.map_err(|e| AccountError::Db(e.to_string()))?;

        // Create user home directory
        let home = neura_storage::paths::user_home(username);
        neura_kernel::syscall::FsHost::create_dir(&home)
            .map_err(|e| AccountError::Db(e.to_string()))?;

        info!("Created user: {} (role: {})", username, role);

        Ok(User {
            id,
            username: username.to_string(),
            password_hash: hash,
            role,
            created_at: now,
            updated_at: now,
        })
    }

    /// Find a user by username.
    pub async fn find_by_username(&self, username: &str) -> AccountResult<Option<User>> {
        let conn = self.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, password_hash, role, created_at, updated_at FROM users WHERE username = ?1"
        ).map_err(|e| AccountError::Db(e.to_string()))?;

        let mut rows = stmt.query([username]).map_err(|e| AccountError::Db(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| AccountError::Db(e.to_string()))? {
            let role_str: String = row.get(3).map_err(|e| AccountError::Db(e.to_string()))?;
            let role: Role = role_str.parse().map_err(|e: String| AccountError::Db(e))?;
            let created_str: String = row.get(4).map_err(|e| AccountError::Db(e.to_string()))?;
            let updated_str: String = row.get(5).map_err(|e| AccountError::Db(e.to_string()))?;

            Ok(Some(User {
                id: row.get(0).map_err(|e| AccountError::Db(e.to_string()))?,
                username: row.get(1).map_err(|e| AccountError::Db(e.to_string()))?,
                password_hash: row.get(2).map_err(|e| AccountError::Db(e.to_string()))?,
                role,
                created_at: DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }))
        } else {
            Ok(None)
        }
    }

    /// Find a user by ID.
    pub async fn find_by_id(&self, id: &str) -> AccountResult<Option<User>> {
        let conn = self.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, password_hash, role, created_at, updated_at FROM users WHERE id = ?1"
        ).map_err(|e| AccountError::Db(e.to_string()))?;

        let mut rows = stmt.query([id]).map_err(|e| AccountError::Db(e.to_string()))?;

        if let Some(row) = rows.next().map_err(|e| AccountError::Db(e.to_string()))? {
            let role_str: String = row.get(3).map_err(|e| AccountError::Db(e.to_string()))?;
            let role: Role = role_str.parse().map_err(|e: String| AccountError::Db(e))?;
            let created_str: String = row.get(4).map_err(|e| AccountError::Db(e.to_string()))?;
            let updated_str: String = row.get(5).map_err(|e| AccountError::Db(e.to_string()))?;

            Ok(Some(User {
                id: row.get(0).map_err(|e| AccountError::Db(e.to_string()))?,
                username: row.get(1).map_err(|e| AccountError::Db(e.to_string()))?,
                password_hash: row.get(2).map_err(|e| AccountError::Db(e.to_string()))?,
                role,
                created_at: DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }))
        } else {
            Ok(None)
        }
    }

    /// List all users.
    pub async fn list_users(&self) -> AccountResult<Vec<User>> {
        let conn = self.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, username, password_hash, role, created_at, updated_at FROM users ORDER BY username"
        ).map_err(|e| AccountError::Db(e.to_string()))?;

        let mut users = Vec::new();
        let mut rows = stmt.query([]).map_err(|e| AccountError::Db(e.to_string()))?;

        while let Some(row) = rows.next().map_err(|e| AccountError::Db(e.to_string()))? {
            let role_str: String = row.get(3).map_err(|e| AccountError::Db(e.to_string()))?;
            let role: Role = role_str.parse().map_err(|e: String| AccountError::Db(e))?;
            let created_str: String = row.get(4).map_err(|e| AccountError::Db(e.to_string()))?;
            let updated_str: String = row.get(5).map_err(|e| AccountError::Db(e.to_string()))?;

            users.push(User {
                id: row.get(0).map_err(|e| AccountError::Db(e.to_string()))?,
                username: row.get(1).map_err(|e| AccountError::Db(e.to_string()))?,
                password_hash: row.get(2).map_err(|e| AccountError::Db(e.to_string()))?,
                role,
                created_at: DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            });
        }

        Ok(users)
    }

    /// Returns true if at least one user account exists.
    pub async fn has_any_users(&self) -> AccountResult<bool> {
        let conn = self.db.lock().await;
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM users")
            .map_err(|e| AccountError::Db(e.to_string()))?;
        let count: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| AccountError::Db(e.to_string()))?;
        Ok(count > 0)
    }

    /// Count users with admin or root privileges.
    pub async fn count_admins(&self) -> AccountResult<usize> {
        let conn = self.db.lock().await;
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM users WHERE role IN ('root', 'admin')")
            .map_err(|e| AccountError::Db(e.to_string()))?;
        let count: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| AccountError::Db(e.to_string()))?;
        Ok(count as usize)
    }

    /// Update a user's password.
    pub async fn change_password(&self, username: &str, new_plaintext: &str) -> AccountResult<()> {
        let hash = password::hash_password(new_plaintext)?;
        let now = Utc::now();
        let rows = self.db.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE username = ?3",
            &[&hash, &now.to_rfc3339(), &username],
        ).await.map_err(|e| AccountError::Db(e.to_string()))?;
        if rows == 0 {
            return Err(AccountError::NotFound(username.to_string()));
        }
        info!("Password changed for user: {}", username);
        Ok(())
    }

    /// Delete a user by username.
    pub async fn delete_user(&self, username: &str) -> AccountResult<()> {
        let result = self.db.execute(
            "DELETE FROM users WHERE username = ?1",
            &[&username],
        ).await.map_err(|e| AccountError::Db(e.to_string()))?;

        if result == 0 {
            return Err(AccountError::NotFound(username.to_string()));
        }

        info!("Deleted user: {}", username);
        Ok(())
    }
}
