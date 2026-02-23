use ratatui::Frame;
use crossterm::event::KeyEvent;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::any::Any;

pub type AppId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub category: String,
}

pub trait App: Send + Sync {
    /// Unique app identifier.
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Initialize the app. Called once on first open.
    fn init(&mut self) -> anyhow::Result<()>;

    /// Handle a keyboard event. Return true if consumed.
    fn handle_key(&mut self, key: KeyEvent) -> bool;

    /// Render the app into a ratatui frame area.
    fn render(&self, frame: &mut Frame, area: ratatui::layout::Rect);

    /// Called when app is paused (loses focus).
    fn on_pause(&mut self) {}

    /// Called when app is resumed (gains focus).
    fn on_resume(&mut self) {}

    /// Called when app is closed. Persist state here.
    fn on_close(&mut self) {}

    /// Get the app's current state as JSON for persistence.
    fn save_state(&self) -> Option<Value> { None }

    /// Restore state from JSON.
    fn load_state(&mut self, _state: Value) {}

    /// Get AI tool definitions this app exposes.
    fn ai_tools(&self) -> Vec<Value> { Vec::new() }

    /// Handle an AI tool call.
    fn handle_ai_tool(&mut self, _tool_name: &str, _args: Value) -> Option<Value> { None }

    /// Downcast helper.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
