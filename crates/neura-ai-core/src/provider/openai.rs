use std::future::Future;
use std::pin::Pin;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use super::{AiProvider, AiError, AiResult};
use super::types::*;

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    display_name: String,
}

impl OpenAiProvider {
    fn new_with(api_key: String, model: String, base_url: String, display_name: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        Self { client, api_key, model, base_url, display_name }
    }

    pub fn openai(api_key: String, model: String) -> Self {
        Self::new_with(api_key, model, "https://api.openai.com/v1".to_string(), "OpenAI".to_string())
    }

    pub fn deepseek(api_key: String, model: String) -> Self {
        Self::new_with(api_key, model, "https://api.deepseek.com/v1".to_string(), "DeepSeek".to_string())
    }

    pub fn custom(api_key: String, model: String, base_url: String) -> Self {
        Self::new_with(api_key, model, base_url, "Custom".to_string())
    }

    fn convert_messages(request: &GenerateRequest) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();
        if let Some(ref sys) = request.system_prompt {
            messages.push(json!({"role": "system", "content": sys}));
        }
        for msg in &request.messages {
            let role = match msg.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "system",
            };
            for c in &msg.content {
                match c {
                    MessageContent::Text(t) => {
                        messages.push(json!({"role": role, "content": t}));
                    }
                    MessageContent::FunctionCall { name, args } => {
                        messages.push(json!({
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": format!("call_{}", name),
                                "type": "function",
                                "function": {"name": name, "arguments": args.to_string()}
                            }]
                        }));
                    }
                    MessageContent::FunctionResponse { name, response } => {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": format!("call_{}", name),
                            "content": response.to_string()
                        }));
                    }
                }
            }
        }
        messages
    }

    fn convert_tools(tools: &[ToolDefinition]) -> Option<serde_json::Value> {
        if tools.is_empty() {
            return None;
        }
        let tool_defs: Vec<serde_json::Value> = tools.iter().map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        }).collect();
        Some(json!(tool_defs))
    }
}

impl AiProvider for OpenAiProvider {
    fn provider_name(&self) -> &str { &self.display_name }
    fn model_name(&self) -> &str { &self.model }

    fn generate<'a>(
        &'a self,
        request: GenerateRequest,
    ) -> Pin<Box<dyn Future<Output = AiResult<GenerateResponse>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!("{}/chat/completions", self.base_url);
            let messages = Self::convert_messages(&request);

            let mut body = json!({
                "model": self.model,
                "messages": messages,
                "temperature": request.temperature,
                "max_tokens": request.max_tokens,
            });

            if let Some(tools) = Self::convert_tools(&request.tools) {
                body["tools"] = tools;
            }

            debug!("Sending request to {} API", self.display_name);
            let mut req = self.client.post(&url).json(&body);
            if !self.api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.api_key));
            }
            let resp = req.send().await.map_err(|e| AiError::Http(e.to_string()))?;

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

            let mut content = Vec::new();
            if let Some(choices) = raw.get("choices").and_then(|c| c.as_array()) {
                if let Some(choice) = choices.first() {
                    if let Some(msg) = choice.get("message") {
                        if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                            content.push(MessageContent::Text(text.to_string()));
                        }
                        if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                            for tc in tool_calls {
                                if let Some(func) = tc.get("function") {
                                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                    let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                                    let args = serde_json::from_str(args_str).unwrap_or(json!({}));
                                    content.push(MessageContent::FunctionCall { name, args });
                                }
                            }
                        }
                    }
                }
            }

            let usage = raw.get("usage").map(|u| TokenUsage {
                prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            });

            Ok(GenerateResponse {
                content,
                usage,
                finish_reason: raw.get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|f| f.as_str())
                    .map(String::from),
            })
        })
    }
}
