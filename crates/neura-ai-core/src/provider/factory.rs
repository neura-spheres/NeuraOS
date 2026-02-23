use std::sync::Arc;

use super::{AiProvider, AiError, AiResult};
use super::types::ProviderConfig;
use super::gemini::GeminiProvider;
use super::openai::OpenAiProvider;
use super::ollama::OllamaProvider;

pub fn create_provider(config: ProviderConfig) -> AiResult<Arc<dyn AiProvider>> {
    match config.provider.as_str() {
        "gemini" => {
            if config.api_key.is_empty() {
                return Err(AiError::NoApiKey);
            }
            Ok(Arc::new(GeminiProvider::new(config.api_key, config.model)))
        }
        "openai" => {
            if config.api_key.is_empty() {
                return Err(AiError::NoApiKey);
            }
            Ok(Arc::new(OpenAiProvider::openai(config.api_key, config.model)))
        }
        "deepseek" => {
            if config.api_key.is_empty() {
                return Err(AiError::NoApiKey);
            }
            Ok(Arc::new(OpenAiProvider::deepseek(config.api_key, config.model)))
        }
        "ollama" => {
            let base_url = if config.base_url.is_empty() { None } else { Some(config.base_url) };
            Ok(Arc::new(OllamaProvider::new(config.model, base_url)))
        }
        "custom" => {
            if config.base_url.is_empty() {
                return Err(AiError::UnsupportedProvider(
                    "Custom provider requires a base_url".to_string(),
                ));
            }
            Ok(Arc::new(OpenAiProvider::custom(config.api_key, config.model, config.base_url)))
        }
        other => Err(AiError::UnsupportedProvider(other.to_string())),
    }
}
