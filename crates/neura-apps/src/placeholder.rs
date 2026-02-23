use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_app_framework::consts::{OS_NAME, OS_VERSION};

pub struct PlaceholderApp {
    app_id: String,
    app_name: String,
    description: String,
    icon: String,
}

impl PlaceholderApp {
    pub fn new(id: &str, name: &str, description: &str, icon: &str) -> Self {
        Self {
            app_id: id.to_string(),
            app_name: name.to_string(),
            description: description.to_string(),
            icon: icon.to_string(),
        }
    }
}

impl App for PlaceholderApp {
    fn id(&self) -> &str { &self.app_id }
    fn name(&self) -> &str { &self.app_name }

    fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => false,
            _ => true,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Outer border block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" {} ", self.app_name))
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // We need at least some rows to render the content
        if inner.height < 3 || inner.width < 10 {
            return;
        }

        // Build the centered content lines
        let icon_line = Line::from(vec![
            Span::styled(&self.icon, Style::default().fg(ORANGE).add_modifier(Modifier::BOLD)),
        ]);

        let name_line = Line::from(vec![
            Span::styled(&self.app_name, Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
        ]);

        let desc_line = Line::from(vec![
            Span::styled(&self.description, Style::default().fg(TEXT)),
        ]);

        let spacer = Line::from("");

        let coming_soon_line = Line::from(vec![
            Span::styled("Coming Soon", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        ]);

        // Progress bar: static visual representation
        let bar_width = 30usize;
        let filled = 12usize;
        let empty = bar_width - filled;
        let progress_bar = format!(
            "[{}{}] {}%",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(empty),
            (filled * 100) / bar_width,
        );

        let progress_line = Line::from(vec![
            Span::styled("Under Development  ", Style::default().fg(ORANGE)),
            Span::styled(progress_bar, Style::default().fg(MUTED)),
        ]);

        let version_line = Line::from(vec![
            Span::styled(format!("{} v{}", OS_NAME, OS_VERSION), Style::default().fg(DIM)),
        ]);

        let esc_line = Line::from(vec![
            Span::styled("Press ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
            Span::styled(" to return", Style::default().fg(DIM)),
        ]);

        let text = Text::from(vec![
            spacer.clone(),
            spacer.clone(),
            icon_line,
            spacer.clone(),
            name_line,
            desc_line,
            spacer.clone(),
            spacer.clone(),
            coming_soon_line,
            spacer.clone(),
            progress_line,
            spacer.clone(),
            spacer.clone(),
            version_line,
            spacer.clone(),
            esc_line,
        ]);

        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center);

        // Center vertically within the inner area
        let total_lines = 16u16;
        let vertical_offset = if inner.height > total_lines {
            (inner.height - total_lines) / 2
        } else {
            0
        };

        let centered_area = Rect {
            x: inner.x,
            y: inner.y + vertical_offset,
            width: inner.width,
            height: inner.height.saturating_sub(vertical_offset),
        };

        frame.render_widget(paragraph, centered_area);
    }

    fn on_pause(&mut self) {}
    fn on_resume(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> { None }
    fn load_state(&mut self, _state: Value) {}

    fn ai_tools(&self) -> Vec<Value> { Vec::new() }
    fn handle_ai_tool(&mut self, _tool_name: &str, _args: Value) -> Option<Value> { None }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

/// Create a placeholder app for a known app ID.
///
/// Maps recognized app IDs to their display names, descriptions, and icons.
/// Unknown IDs will produce a generic placeholder.
pub fn create_placeholder(id: &str) -> PlaceholderApp {
    let (name, description, icon) = match id {
        "weather"  => ("NeuraWeather",  "Real-time weather forecasts and conditions",    "[WE]"),
        "dev"      => ("NeuraDev",      "Integrated code editor and development environment", "[DE]"),
        "terminal" => ("NeuraTerminal", "Nested terminal emulator shell",                "[TM]"),
        "mail"     => ("NeuraMail",     "Email client with IMAP/SMTP support",           "[MA]"),
        "chat"     => ("NeuraChat",     "Encrypted messaging and AI chat",               "[CH]"),
        "browser"  => ("NeuraBrowse",   "Terminal web browser",                          "[BR]"),
        "ssh"      => ("NeuraSSH",      "Secure Shell client",                           "[SS]"),
        "ftp"      => ("NeuraFTP",      "File transfer protocol client",                 "[FT]"),
        "db"       => ("NeuraDB",       "Database browser and query tool",               "[DB]"),
        "sync"     => ("NeuraSync",     "Cloud synchronization service",                 "[SY]"),
        "backup"   => ("NeuraBackup",   "Automated backup management",                   "[BK]"),
        "media"    => ("NeuraMedia",    "Media player and manager",                      "[MD]"),
        "store"    => ("NeuraStore",    "Application package store",                     "[ST]"),
        _          => ("Unknown App",   "This application is not yet registered",        "[??]"),
    };

    PlaceholderApp::new(id, name, description, icon)
}
