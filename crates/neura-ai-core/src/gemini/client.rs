use reqwest::Client;
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use thiserror::Error;
use tracing::debug;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Error, Debug)]
pub enum GeminiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },
    #[error("No API key configured")]
    NoApiKey,
    #[error("Rate limited")]
    RateLimited,
    #[error("Parse error: {0}")]
    Parse(String),
}

pub type GeminiResult<T> = Result<T, GeminiError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: Some(text.into()), function_call: None, function_response: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub candidates: Vec<Candidate>,
    #[serde(default)]
    pub usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub content: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    #[serde(default)]
    pub prompt_token_count: u32,
    #[serde(default)]
    pub candidates_token_count: u32,
    #[serde(default)]
    pub total_token_count: u32,
}

pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
    total_tokens_used: AtomicU64,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            api_key,
            model,
            total_tokens_used: AtomicU64::new(0),
        }
    }

    pub fn from_env(model: &str) -> GeminiResult<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| GeminiError::NoApiKey)?;
        Ok(Self::new(api_key, model.to_string()))
    }

    /// Send a generate content request.
    pub async fn generate(
        &self,
        messages: &[Message],
        system_instruction: Option<&str>,
        tools: Option<&Value>,
        temperature: f32,
        max_tokens: u32,
    ) -> GeminiResult<GenerateResponse> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let mut body = json!({
            "contents": messages,
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": max_tokens,
            }
        });

        if let Some(instruction) = system_instruction {
            body["systemInstruction"] = json!({
                "parts": [{"text": instruction}]
            });
        }

        if let Some(tools_def) = tools {
            body["tools"] = tools_def.clone();
        }

        debug!("Sending request to Gemini API");
        let resp = self.client.post(&url)
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status == 429 {
            return Err(GeminiError::RateLimited);
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(GeminiError::Api { status, message: text });
        }

        let response: GenerateResponse = resp.json().await
            .map_err(|e| GeminiError::Parse(e.to_string()))?;

        // Track token usage
        if let Some(ref usage) = response.usage_metadata {
            self.total_tokens_used.fetch_add(usage.total_token_count as u64, Ordering::Relaxed);
        }

        Ok(response)
    }

    pub fn total_tokens_used(&self) -> u64 {
        self.total_tokens_used.load(Ordering::Relaxed)
    }
}
