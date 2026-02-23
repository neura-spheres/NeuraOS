use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SigningError {
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    #[error("Signing error: {0}")]
    SignError(String),
}

pub type SignResult<T> = Result<T, SigningError>;

/// Generate a new Ed25519 signing keypair.
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}
pub fn sign(data: &[u8], key: &SigningKey) -> Vec<u8> {
    let signature = key.sign(data);
    signature.to_bytes().to_vec()
}
pub fn verify(data: &[u8], signature_bytes: &[u8], key: &VerifyingKey) -> SignResult<()> {
    let signature = Signature::from_slice(signature_bytes)
        .map_err(|e| SigningError::InvalidKey(e.to_string()))?;
    key.verify(data, &signature)
        .map_err(|_| SigningError::VerificationFailed)
}
