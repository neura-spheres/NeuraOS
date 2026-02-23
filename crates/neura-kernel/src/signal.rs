use tokio::sync::broadcast;
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemSignal {
    Shutdown,
    Restart,
    Suspend,
    Resume,
    UserLogout,
    Custom(String),
}

/// Global signal bus for system-wide events.
#[derive(Clone)]
pub struct SignalBus {
    sender: broadcast::Sender<SystemSignal>,
}

impl SignalBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn emit(&self, signal: SystemSignal) {
        info!(?signal, "System signal emitted");
        let _ = self.sender.send(signal);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemSignal> {
        self.sender.subscribe()
    }
}

impl Default for SignalBus {
    fn default() -> Self {
        Self::new(64)
    }
}
