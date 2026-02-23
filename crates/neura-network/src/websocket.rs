/// WebSocket client abstraction.
/// Will be implemented when real-time features (NeuraChat, NeuraSync) require it.
pub struct WsClient;

impl WsClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WsClient {
    fn default() -> Self {
        Self::new()
    }
}
