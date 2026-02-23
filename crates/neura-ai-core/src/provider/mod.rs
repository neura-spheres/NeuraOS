pub mod types;
pub mod gemini;
pub mod openai;
pub mod ollama;
pub mod factory;

use std::future::Future;
use std::pin::Pin;
use thiserror::Error;

pub use types::*;
pub use factory::create_provider;

#[derive(Error, Debug)]
pub enum AiError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("No API key configured")]
    NoApiKey,
    #[error("Rate limited")]
    RateLimited,
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Setup error: {0}")]
    Setup(String),
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
}

pub type AiResult<T> = Result<T, AiError>;

/// Provider-agnostic AI interface.
pub trait AiProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn generate<'a>(
        &'a self,
        request: GenerateRequest,
    ) -> Pin<Box<dyn Future<Output = AiResult<GenerateResponse>> + Send + 'a>>;
}
