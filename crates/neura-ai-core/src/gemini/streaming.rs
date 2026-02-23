/// Streaming response handler for Gemini API.
/// Will be fully implemented when real-time TUI streaming is needed.
pub struct StreamHandler;

impl StreamHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StreamHandler {
    fn default() -> Self {
        Self::new()
    }
}
