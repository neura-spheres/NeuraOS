use serde_json::Value;
use crate::agent::tool_registry::{ToolRegistry, ToolResult};
use tracing::{info, error};

/// Execute a tool call within the sandbox.
pub async fn execute_tool(
    registry: &ToolRegistry,
    tool_name: &str,
    args: Value,
) -> ToolResult {
    let tool = registry.get(tool_name)
        .ok_or_else(|| crate::agent::tool_registry::ToolError::NotFound(tool_name.to_string()))?;

    info!("Executing tool: {}", tool_name);
    let result = (tool.handler)(args).await;

    match &result {
        Ok(_) => info!("Tool {} completed successfully", tool_name),
        Err(e) => error!("Tool {} failed: {}", tool_name, e),
    }

    result
}
