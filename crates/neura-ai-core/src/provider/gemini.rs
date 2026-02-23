use std::future::Future;
use std::pin::Pin;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use super::{AiProvider, AiError, AiResult};
use super::types::*;

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        Self { client, api_key, model }
    }

    fn convert_messages(messages: &[ChatMessage]) -> serde_json::Value {
        let contents: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| m.role != ChatRole::System)
            .map(|msg| {
                let role = match msg.role {
                    ChatRole::User => "user",
                    ChatRole::Assistant => "model",
                    ChatRole::System => "user",
                };
                let parts: Vec<serde_json::Value> = msg.content.iter().map(|c| match c {
                    MessageContent::Text(t) => json!({"text": t}),
                    MessageContent::FunctionCall { name, args } => {
                        json!({"functionCall": {"name": name, "args": args}})
                    }
                    MessageContent::FunctionResponse { name, response } => {
                        json!({"functionResponse": {"name": name, "response": response}})
                    }
                }).collect();
                json!({"role": role, "parts": parts})
            })
            .collect();
        json!(contents)
    }

    fn convert_tools(tools: &[ToolDefinition]) -> Option<serde_json::Value> {
        if tools.is_empty() {
            return None;
        }
        let declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        }).collect();
        Some(json!([{"functionDeclarations": declarations}]))
    }
}

impl AiProvider for GeminiProvider {
    fn provider_name(&self) -> &str { "Gemini" }
    fn model_name(&self) -> &str { &self.model }

    fn generate<'a>(
        &'a self,
        request: GenerateRequest,
    ) -> Pin<Box<dyn Future<Output = AiResult<GenerateResponse>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                self.model, self.api_key
            );

            let contents = Self::convert_messages(&request.messages);
            let mut body = json!({
                "contents": contents,
                "generationConfig": {
                    "temperature": request.temperature,
                    "maxOutputTokens": request.max_tokens,
                }
            });

            if let Some(ref sys) = request.system_prompt {
                body["systemInstruction"] = json!({"parts": [{"text": sys}]});
            }

            if let Some(tools) = Self::convert_tools(&request.tools) {
                body["tools"] = tools;
            }

            debug!("Sending request to Gemini API");
            let resp = self.client.post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| AiError::Http(e.to_string()))?;

            let status = resp.status().as_u16();
            if status == 429 {
                return Err(AiError::RateLimited);
            }
            if !resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(AiError::Api { status, message: text });
            }

            let raw: serde_json::Value = resp.json().await
                .map_err(|e| AiError::Parse(e.to_string()))?;

            // Parse response
            let mut content = Vec::new();
            if let Some(candidates) = raw.get("candidates").and_then(|c| c.as_array()) {
                if let Some(candidate) = candidates.first() {
                    if let Some(parts) = candidate.get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                    {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                content.push(MessageContent::Text(text.to_string()));
                            }
                            if let Some(fc) = part.get("functionCall") {
                                content.push(MessageContent::FunctionCall {
                                    name: fc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                                    args: fc.get("args").cloned().unwrap_or(json!({})),
                                });
                            }
                        }
                    }
                }
            }

            let usage = raw.get("usageMetadata").map(|u| TokenUsage {
                prompt_tokens: u.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                total_tokens: u.get("totalTokenCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            });

            Ok(GenerateResponse {
                content,
                usage,
                finish_reason: raw.get("candidates")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finishReason"))
                    .and_then(|f| f.as_str())
                    .map(String::from),
            })
        })
    }
}
