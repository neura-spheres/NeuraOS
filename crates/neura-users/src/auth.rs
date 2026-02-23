use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use thiserror::Error;
use tracing::{info, warn};

use crate::account::UserStore;
use crate::password;
use crate::roles::Role;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Session expired")]
    SessionExpired,
    #[error("Session not found")]
    SessionNotFound,
    #[error("Account error: {0}")]
    Account(#[from] crate::account::AccountError),
    #[error("Insufficient permissions")]
    InsufficientPermissions,
}

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Authentication service handling login, logout, session management.
pub struct AuthService {
    user_store: UserStore,
    sessions: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Session>>>,
    session_duration_hours: i64,
}

impl AuthService {
    pub fn new(user_store: UserStore) -> Self {
        Self {
            user_store,
            sessions: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            session_duration_hours: 24,
        }
    }

    /// Authenticate a user and create a session.
    pub async fn login(&self, username: &str, plaintext_password: &str) -> AuthResult<Session> {
        let user = self.user_store.find_by_username(username).await?
            .ok_or(AuthError::InvalidCredentials)?;

        let valid = password::verify_password(plaintext_password, &user.password_hash)
            .map_err(|_| AuthError::InvalidCredentials)?;

        if !valid {
            warn!("Failed login attempt for user: {}", username);
            return Err(AuthError::InvalidCredentials);
        }

        let now = Utc::now();
        let session = Session {
            id: Uuid::new_v4().to_string(),
            user_id: user.id.clone(),
            username: user.username.clone(),
            role: user.role.clone(),
            created_at: now,
            expires_at: now + Duration::hours(self.session_duration_hours),
        };

        self.sessions.write().await.insert(session.id.clone(), session.clone());
        info!("User logged in: {}", username);
        Ok(session)
    }

    /// Validate a session token.
    pub async fn validate_session(&self, session_id: &str) -> AuthResult<Session> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(session_id)
            .ok_or(AuthError::SessionNotFound)?;

        if session.is_expired() {
            drop(sessions);
            self.sessions.write().await.remove(session_id);
            return Err(AuthError::SessionExpired);
        }

        Ok(session.clone())
    }

    /// Logout (destroy session).
    pub async fn logout(&self, session_id: &str) -> AuthResult<()> {
        let removed = self.sessions.write().await.remove(session_id);
        if let Some(session) = removed {
            info!("User logged out: {}", session.username);
        }
        Ok(())
    }

    pub async fn ensure_root_user(&self, default_password: &str) -> AuthResult<()> {
        if self.user_store.find_by_username("root").await?.is_none() {
            self.user_store.create_user("root", default_password, Role::Root).await?;
            info!("Created default root user");
        }
        Ok(())
    }

    pub async fn has_any_users(&self) -> AuthResult<bool> {
        Ok(self.user_store.has_any_users().await?)
    }

    pub fn user_store(&self) -> &UserStore {
        &self.user_store
    }
}
