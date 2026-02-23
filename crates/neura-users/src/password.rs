use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PasswordError {
    #[error("Password hashing failed: {0}")]
    HashError(String),
    #[error("Password verification failed")]
    VerifyError,
}

pub type PasswordResult<T> = Result<T, PasswordError>;

/// Hash a plaintext password using Argon2id.
pub fn hash_password(password: &str) -> PasswordResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashError(e.to_string()))?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored hash.
pub fn verify_password(password: &str, hash: &str) -> PasswordResult<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| PasswordError::HashError(e.to_string()))?;
    let argon2 = Argon2::default();
    match argon2.verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}
