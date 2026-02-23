use std::future::Future;
use std::pin::Pin;

use super::{AiProvider, AiResult, AiError};
use super::types::*;
use super::openai::OpenAiProvider;

/// Ollama provider using the OpenAI-compatible endpoint.
pub struct OllamaProvider {
    inner: OpenAiProvider,
}

impl OllamaProvider {
    pub fn new(model: String, base_url: Option<String>) -> Self {
        let url = base_url.unwrap_or_else(|| "http://localhost:11434/v1".to_string());
        Self {
            inner: OpenAiProvider::custom(String::new(), model, url),
        }
    }
}

impl AiProvider for OllamaProvider {
    fn provider_name(&self) -> &str { "Ollama" }
    fn model_name(&self) -> &str { self.inner.model_name() }

    fn generate<'a>(
        &'a self,
        request: GenerateRequest,
    ) -> Pin<Box<dyn Future<Output = AiResult<GenerateResponse>> + Send + 'a>> {
        Box::pin(async move {
            // Ensure Ollama is running
            if let Err(e) = crate::OllamaManager::ensure_ollama_running().await {
                return Err(AiError::Setup(format!("Failed to start Ollama: {}", e)));
            }

            // Check if model exists, if not try to pull it
            let model_name = self.model_name().to_string();
            match crate::OllamaManager::model_exists(&model_name).await {
                Ok(exists) => {
                    if !exists {
                        tracing::info!("Model '{}' not found, attempting to pull...", model_name);
                        // Define a callback to log progress
                        let progress_cb = |msg: String| {
                            tracing::debug!("Ollama pull: {}", msg);
                        };
                        
                        if let Err(e) = crate::OllamaManager::install_model(&model_name, Some(progress_cb)).await {
                             return Err(AiError::Setup(format!("Model '{}' not found and failed to install: {}", model_name, e)));
                        }
                        tracing::info!("Model '{}' pulled successfully", model_name);
                    }
                },
                Err(e) => {
                     return Err(AiError::Setup(format!("Failed to check model existence: {}", e)));
                }
            }

            self.inner.generate(request).await
        })
    }
}
