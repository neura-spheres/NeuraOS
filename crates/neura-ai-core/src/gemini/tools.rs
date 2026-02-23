use serde_json::{json, Value};

/// Build a Gemini-compatible function declaration.
pub fn function_declaration(
    name: &str,
    description: &str,
    parameters: Value,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "parameters": parameters,
    })
}

/// Build a tools array for Gemini API from function declarations.
pub fn build_tools_payload(declarations: Vec<Value>) -> Value {
    json!([{
        "functionDeclarations": declarations
    }])
}

/// Helper to build a parameter schema.
pub fn param_schema(
    param_type: &str,
    properties: Value,
    required: Vec<&str>,
) -> Value {
    json!({
        "type": param_type,
        "properties": properties,
        "required": required,
    })
}
