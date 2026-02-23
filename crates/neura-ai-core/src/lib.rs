pub mod gemini;
pub mod agent;
pub mod memory;
pub mod provider;
pub mod ollama_manager;

pub use gemini::client::GeminiClient;
pub use agent::engine::AgentEngine;
pub use agent::tool_registry::{Tool, ToolRegistry, ToolParam, ToolError, ToolResult};
pub use memory::MemoryManager;
pub use ollama_manager::{OllamaManager, OllamaModel, OllamaAvailableModel};

pub use provider::{AiProvider, AiError, AiResult, create_provider};
pub use provider::types::{
    ChatMessage, ChatRole, MessageContent, GenerateRequest, GenerateResponse,
    ToolDefinition, TokenUsage, ProviderConfig,
};
