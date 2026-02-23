use std::sync::Arc;
use tracing::{info, warn, error};

use crate::provider::{AiProvider, AiResult};
use crate::provider::types::*;
use crate::agent::tool_registry::ToolRegistry;
use crate::memory::MemoryManager;

/// The main agent engine: Observe -> Reason -> Plan -> Tool Select -> Execute -> Reflect.
pub struct AgentEngine {
    client: Arc<dyn AiProvider>,
    tools: ToolRegistry,
    memory: MemoryManager,
    system_prompt: String,
    history: Vec<ChatMessage>,
    max_steps: u32,
    temperature: f32,
    max_tokens: u32,
}

impl AgentEngine {
    pub fn new(
        client: Arc<dyn AiProvider>,
        tools: ToolRegistry,
        memory: MemoryManager,
        system_prompt: String,
    ) -> Self {
        Self {
            client,
            tools,
            memory,
            system_prompt,
            history: Vec::new(),
            max_steps: 10,
            temperature: 0.7,
            max_tokens: 8192,
        }
    }

    pub fn with_max_steps(mut self, steps: u32) -> Self {
        self.max_steps = steps;
        self
    }

    pub fn with_history(mut self, history: Vec<ChatMessage>) -> Self {
        self.history = history;
        self
    }

    /// Run the agent loop for a user query.
    pub async fn run(&self, user_input: &str) -> AiResult<String> {
        let mut messages = self.history.clone();

        // Add context from memory
        let context = self.memory.get_context_summary().await;
        let full_input = if context.is_empty() {
            user_input.to_string()
        } else {
            format!("Context:\n{}\n\nUser request: {}", context, user_input)
        };

        messages.push(ChatMessage::user(&full_input));

        let tool_defs = self.tools.to_tool_definitions();

        for step in 0..self.max_steps {
            info!("Agent step {}/{}", step + 1, self.max_steps);

            let request = GenerateRequest {
                messages: messages.clone(),
                system_prompt: Some(self.system_prompt.clone()),
                tools: tool_defs.clone(),
                temperature: self.temperature,
                max_tokens: self.max_tokens,
            };

            let response = self.client.generate(request).await?;

            if response.content.is_empty() {
                warn!("Empty response from provider");
                break;
            }

            // Check for function calls
            let mut has_function_call = false;
            let mut function_responses = Vec::new();
            let mut text_parts = Vec::new();

            for part in &response.content {
                match part {
                    MessageContent::FunctionCall { name, args } => {
                        has_function_call = true;
                        info!("Agent calling tool: {}", name);

                        let result = if let Some(tool) = self.tools.get(name) {
                            match (tool.handler)(args.clone()).await {
                                Ok(val) => val,
                                Err(e) => {
                                    error!("Tool execution failed: {}", e);
                                    serde_json::json!({"error": e.to_string()})
                                }
                            }
                        } else {
                            warn!("Tool not found: {}", name);
                            serde_json::json!({"error": format!("Unknown tool: {}", name)})
                        };

                        function_responses.push(MessageContent::FunctionResponse {
                            name: name.clone(),
                            response: result,
                        });
                    }
                    MessageContent::Text(t) => {
                        text_parts.push(t.clone());
                    }
                    _ => {}
                }
            }

            // Add assistant message to history
            messages.push(ChatMessage {
                role: ChatRole::Assistant,
                content: response.content.clone(),
            });

            if has_function_call {
                messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: function_responses,
                });
                continue;
            }

            // No function call — return text response
            if !text_parts.is_empty() {
                let text = text_parts.join("");
                self.memory.store_short_term("last_response", &text).await;
                return Ok(text);
            }

            break;
        }

        Ok("Agent completed without generating a response.".to_string())
    }
}
