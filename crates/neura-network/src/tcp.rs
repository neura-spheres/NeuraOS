/// TCP client abstraction.
/// Will be implemented when NeuraSSH and NeuraFTP require raw TCP support.
pub struct TcpClient;

impl TcpClient {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TcpClient {
    fn default() -> Self {
        Self::new()
    }
}
