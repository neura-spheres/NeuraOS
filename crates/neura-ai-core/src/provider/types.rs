use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    FunctionCall { name: String, args: Value },
    FunctionResponse { name: String, response: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: Vec<MessageContent>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: vec![MessageContent::Text(text.into())],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: vec![MessageContent::Text(text.into())],
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: vec![MessageContent::Text(text.into())],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub messages: Vec<ChatMessage>,
    pub system_prompt: Option<String>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl GenerateRequest {
    pub fn simple(prompt: &str) -> Self {
        Self {
            messages: vec![ChatMessage::user(prompt)],
            system_prompt: None,
            tools: Vec::new(),
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct GenerateResponse {
    pub content: Vec<MessageContent>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

impl GenerateResponse {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}
