use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use thiserror::Error;
use crate::provider::types::ToolDefinition;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

pub type ToolResult = Result<Value, ToolError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

/// Definition of a tool that the AI agent can use.
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
    pub handler: Box<dyn Fn(Value) -> Pin<Box<dyn Future<Output = ToolResult> + Send>> + Send + Sync>,
}

impl Tool {
    /// Convert this tool to a Gemini function declaration.
    pub fn to_gemini_declaration(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            properties.insert(param.name.clone(), serde_json::json!({
                "type": param.param_type,
                "description": param.description,
            }));
            if param.required {
                required.push(param.name.clone());
            }
        }

        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
            }
        })
    }
}

/// Registry of all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get all Gemini function declarations.
    pub fn to_gemini_tools(&self) -> Value {
        let declarations: Vec<Value> = self.tools.values()
            .map(|t| t.to_gemini_declaration())
            .collect();
        serde_json::json!([{
            "functionDeclarations": declarations
        }])
    }

    /// Get provider-agnostic tool definitions.
    pub fn to_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for param in &t.parameters {
                properties.insert(param.name.clone(), serde_json::json!({
                    "type": param.param_type,
                    "description": param.description,
                }));
                if param.required {
                    required.push(serde_json::Value::String(param.name.clone()));
                }
            }
            ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }),
            }
        }).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
