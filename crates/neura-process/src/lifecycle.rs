use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEvent {
    Starting,
    Started,
    Pausing,
    Paused,
    Resuming,
    Resumed,
    Stopping,
    Stopped,
    Crashed(String),
}

pub trait ProcessLifecycle {
    fn on_start(&mut self) -> Result<(), String>;
    fn on_stop(&mut self) -> Result<(), String>;
    fn on_pause(&mut self) -> Result<(), String> { Ok(()) }
    fn on_resume(&mut self) -> Result<(), String> { Ok(()) }
}
