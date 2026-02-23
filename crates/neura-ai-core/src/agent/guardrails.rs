
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GuardrailError {
    #[error("Blocked: {0}")]
    Blocked(String),
    #[error("Token limit exceeded")]
    TokenLimitExceeded,
    #[error("Step limit exceeded")]
    StepLimitExceeded,
}

/// Check if an AI response or tool call should be blocked.
pub struct Guardrails {
    pub max_tokens_per_request: u64,
    pub max_steps_per_session: u32,
    pub blocked_patterns: Vec<String>,
}

impl Guardrails {
    pub fn new() -> Self {
        Self {
            max_tokens_per_request: 100_000,
            max_steps_per_session: 50,
            blocked_patterns: Vec::new(),
        }
    }

    pub fn check_output(&self, text: &str) -> Result<(), GuardrailError> {
        for pattern in &self.blocked_patterns {
            if text.contains(pattern.as_str()) {
                return Err(GuardrailError::Blocked(
                    format!("Output contains blocked pattern: {}", pattern)
                ));
            }
        }
        Ok(())
    }

    pub fn check_token_usage(&self, used: u64) -> Result<(), GuardrailError> {
        if self.max_tokens_per_request > 0 && used > self.max_tokens_per_request {
            return Err(GuardrailError::TokenLimitExceeded);
        }
        Ok(())
    }

    pub fn check_step_count(&self, steps: u32) -> Result<(), GuardrailError> {
        if self.max_steps_per_session > 0 && steps > self.max_steps_per_session {
            return Err(GuardrailError::StepLimitExceeded);
        }
        Ok(())
    }
}

impl Default for Guardrails {
    fn default() -> Self {
        Self::new()
    }
}
