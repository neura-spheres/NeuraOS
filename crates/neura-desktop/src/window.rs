use std::collections::HashMap;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

pub type WindowId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowLayout {
    FullScreen,
    SplitVertical,
    SplitHorizontal,
    Floating { x: u16, y: u16, width: u16, height: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub app_id: String,
    pub layout: WindowLayout,
    pub visible: bool,
    pub focused: bool,
    pub z_order: u32,
}

pub struct WindowManager {
    windows: HashMap<WindowId, Window>,
    focus_stack: Vec<WindowId>,
    next_z: u32,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            focus_stack: Vec::new(),
            next_z: 0,
        }
    }

    pub fn open_window(&mut self, title: &str, app_id: &str, layout: WindowLayout) -> WindowId {
        let id = Uuid::new_v4().to_string();
        let window = Window {
            id: id.clone(),
            title: title.to_string(),
            app_id: app_id.to_string(),
            layout,
            visible: true,
            focused: true,
            z_order: self.next_z,
        };
        self.next_z += 1;

        // Unfocus previous
        if let Some(prev_id) = self.focus_stack.last() {
            if let Some(prev) = self.windows.get_mut(prev_id) {
                prev.focused = false;
            }
        }

        self.windows.insert(id.clone(), window);
        self.focus_stack.push(id.clone());
        id
    }

    pub fn close_window(&mut self, id: &str) {
        self.windows.remove(id);
        self.focus_stack.retain(|w| w != id);
        // Re-focus top of stack
        if let Some(top) = self.focus_stack.last() {
            if let Some(win) = self.windows.get_mut(top) {
                win.focused = true;
            }
        }
    }

    pub fn focus_window(&mut self, id: &str) {
        // Unfocus all
        for win in self.windows.values_mut() {
            win.focused = false;
        }
        if let Some(win) = self.windows.get_mut(id) {
            win.focused = true;
            win.z_order = self.next_z;
            self.next_z += 1;
        }
        self.focus_stack.retain(|w| w != id);
        self.focus_stack.push(id.to_string());
    }

    pub fn get_focused(&self) -> Option<&Window> {
        self.focus_stack.last()
            .and_then(|id| self.windows.get(id))
    }

    pub fn list_windows(&self) -> Vec<&Window> {
        let mut windows: Vec<&Window> = self.windows.values().collect();
        windows.sort_by_key(|w| w.z_order);
        windows
    }

    pub fn get_window(&self, id: &str) -> Option<&Window> {
        self.windows.get(id)
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
