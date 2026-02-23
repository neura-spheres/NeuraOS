use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use neura_app_framework::app_trait::App;

use neura_app_framework::palette::*;

#[derive(Debug, Clone)]
struct OutputLine {
    text: String,
    is_error: bool,
    is_prompt: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum ExecState {
    Idle,
    Running,
}

pub struct TerminalApp {
    output: Vec<OutputLine>,
    input: String,
    input_cursor: usize,
    scroll: usize,
    history: Vec<String>,
    history_idx: usize,
    cwd: String,
    exec_state: ExecState,
    pending_command: Option<String>,
}

impl TerminalApp {
    pub fn new() -> Self {
        let mut app = Self {
            output: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            scroll: 0,
            history: Vec::new(),
            history_idx: 0,
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            exec_state: ExecState::Idle,
            pending_command: None,
        };
        app.push_output("NeuraTerminal - System Shell", false);
        app.push_output(&format!("Platform: {} ({})", std::env::consts::OS, std::env::consts::ARCH), false);
        app.push_output("Type commands to execute them on the host system.", false);
        app.push_output("Ctrl+C to interrupt | Ctrl+L to clear | Esc to close", false);
        app.push_output("", false);
        app
    }

    fn push_output(&mut self, text: &str, is_error: bool) {
        for line in text.split('\n') {
            self.output.push(OutputLine {
                text: line.to_string(),
                is_error,
                is_prompt: false,
            });
        }
        // Auto-scroll to bottom
        self.scroll = usize::MAX;
    }

    fn push_prompt(&mut self, prompt: &str, cmd: &str) {
        self.output.push(OutputLine {
            text: format!("{} $ {}", prompt, cmd),
            is_error: false,
            is_prompt: true,
        });
        self.scroll = usize::MAX;
    }

    pub fn needs_exec(&self) -> bool {
        self.pending_command.is_some() && self.exec_state == ExecState::Idle
    }

    pub async fn async_exec(&mut self) {
        let cmd = match self.pending_command.take() {
            Some(c) => c,
            None => return,
        };

        self.exec_state = ExecState::Running;

        let result = execute_command(&cmd, &self.cwd).await;

        match result {
            Ok((stdout, stderr, new_cwd)) => {
                if let Some(dir) = new_cwd {
                    self.cwd = dir;
                }
                if !stdout.is_empty() {
                    self.push_output(&stdout, false);
                }
                if !stderr.is_empty() {
                    self.push_output(&stderr, true);
                }
            }
            Err(e) => {
                self.push_output(&format!("Error: {}", e), true);
            }
        }

        self.exec_state = ExecState::Idle;
    }

    fn prompt(&self) -> String {
        // Shorten home directory display
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let display = if !home.is_empty() && self.cwd.starts_with(&home) {
            format!("~{}", &self.cwd[home.len()..])
        } else {
            self.cwd.clone()
        };
        // Just last 2 components for brevity
        let parts: Vec<&str> = display.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
        let short = if parts.len() > 2 {
            format!(".../{}/{}", parts[parts.len()-2], parts[parts.len()-1])
        } else {
            display
        };
        short
    }
}

async fn execute_command(cmd: &str, cwd: &str) -> Result<(String, String, Option<String>), String> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return Ok((String::new(), String::new(), None));
    }

    // Handle cd specially
    if cmd.starts_with("cd ") || cmd == "cd" {
        let target = cmd.trim_start_matches("cd").trim();
        let new_dir = if target.is_empty() {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string())
        } else if target.starts_with('~') {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            target.replacen('~', &home, 1)
        } else if std::path::Path::new(target).is_absolute() {
            target.to_string()
        } else {
            format!("{}/{}", cwd, target)
        };

        let path = std::path::Path::new(&new_dir);
        if path.exists() && path.is_dir() {
            let canonical = std::fs::canonicalize(path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(new_dir);
            return Ok((String::new(), String::new(), Some(canonical)));
        } else {
            return Ok((String::new(), format!("cd: no such directory: {}", new_dir), None));
        }
    }

    // Build the command
    let (shell, shell_flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let output = tokio::process::Command::new(shell)
        .arg(shell_flag)
        .arg(cmd)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Clean up carriage returns on Windows
    let stdout = stdout.replace('\r', "");
    let stderr = stderr.replace('\r', "");

    Ok((stdout, stderr, None))
}

impl App for TerminalApp {
    fn id(&self) -> &str { "terminal" }
    fn name(&self) -> &str { "NeuraTerminal" }

    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.exec_state == ExecState::Running {
            if key.code == KeyCode::Esc { return false; }
            return true;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return false,
            KeyCode::Enter => {
                let cmd = self.input.trim().to_string();
                self.push_prompt(&self.prompt(), &cmd);
                if !cmd.is_empty() {
                    self.history.push(cmd.clone());
                    self.history_idx = self.history.len();
                    if cmd == "clear" {
                        self.output.clear();
                    } else {
                        self.pending_command = Some(cmd);
                    }
                }
                self.input.clear();
                self.input_cursor = 0;
                true
            }
            KeyCode::Char('l') if ctrl => {
                self.output.clear();
                true
            }
            KeyCode::Char(c) if !ctrl => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += 1;
                true
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                    self.input.remove(self.input_cursor);
                }
                true
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input.len() {
                    self.input.remove(self.input_cursor);
                }
                true
            }
            KeyCode::Left => { if self.input_cursor > 0 { self.input_cursor -= 1; } true }
            KeyCode::Right => { if self.input_cursor < self.input.len() { self.input_cursor += 1; } true }
            KeyCode::Home => { self.input_cursor = 0; true }
            KeyCode::End => { self.input_cursor = self.input.len(); true }
            KeyCode::Up => {
                if !self.history.is_empty() && self.history_idx > 0 {
                    self.history_idx -= 1;
                    self.input = self.history[self.history_idx].clone();
                    self.input_cursor = self.input.len();
                }
                true
            }
            KeyCode::Down => {
                if self.history_idx < self.history.len().saturating_sub(1) {
                    self.history_idx += 1;
                    self.input = self.history[self.history_idx].clone();
                    self.input_cursor = self.input.len();
                } else {
                    self.history_idx = self.history.len();
                    self.input.clear();
                    self.input_cursor = 0;
                }
                true
            }
            KeyCode::PageUp => { self.scroll = self.scroll.saturating_sub(10); true }
            KeyCode::PageDown => { self.scroll = self.scroll.saturating_add(10); true }
            _ => true,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // title
                Constraint::Min(5),     // output
                Constraint::Length(3),  // input
                Constraint::Length(1),  // help
            ])
            .split(area);

        // ── Title ──
        let cwd_display = self.prompt();
        let title_text = format!(" NeuraTerminal  |  {}  |  {} ", cwd_display,
            if self.exec_state == ExecState::Running { "Running..." } else { "Ready" });
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(BORDER)));
        frame.render_widget(title, chunks[0]);

        // ── Output ──
        let out_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Output ")
            .title_style(Style::default().fg(DIM));
        let out_inner = out_block.inner(chunks[1]);
        frame.render_widget(out_block, chunks[1]);

        let visible = out_inner.height as usize;
        let total = self.output.len();
        let max_scroll = total.saturating_sub(visible);
        let scroll_off = if self.scroll == usize::MAX { max_scroll } else { self.scroll.min(max_scroll) };

        let lines: Vec<Line> = self.output.iter()
            .skip(scroll_off)
            .take(visible)
            .map(|ol| {
                if ol.is_prompt {
                    let parts: Vec<&str> = ol.text.splitn(2, " $ ").collect();
                    if parts.len() == 2 {
                        Line::from(vec![
                            Span::styled(parts[0], Style::default().fg(PROMPT).add_modifier(Modifier::BOLD)),
                            Span::styled(" $ ", Style::default().fg(DIM)),
                            Span::styled(parts[1], Style::default().fg(GREEN)),
                        ])
                    } else {
                        Line::from(vec![Span::styled(&ol.text, Style::default().fg(PROMPT))])
                    }
                } else if ol.is_error {
                    Line::from(vec![Span::styled(&ol.text, Style::default().fg(RED))])
                } else {
                    Line::from(vec![Span::styled(&ol.text, Style::default().fg(TEXT))])
                }
            })
            .collect();

        let out_para = Paragraph::new(lines).style(Style::default().bg(BG));
        frame.render_widget(out_para, out_inner);

        // ── Input ──
        let is_running = self.exec_state == ExecState::Running;
        let prompt_str = format!("{} $ ", self.prompt());
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if is_running { DIM } else { GREEN }))
            .title(format!(" {} ", if is_running { "Executing..." } else { "Input" }))
            .title_style(Style::default().fg(if is_running { ORANGE } else { GREEN }));
        let input_inner = input_block.inner(chunks[2]);
        frame.render_widget(input_block, chunks[2]);

        let input_line = Line::from(vec![
            Span::styled(&prompt_str, Style::default().fg(PROMPT).add_modifier(Modifier::BOLD)),
            Span::styled(&self.input, Style::default().fg(TEXT)),
        ]);
        frame.render_widget(Paragraph::new(input_line), input_inner);

        if !is_running {
            let cursor_x = input_inner.x + prompt_str.len() as u16 + self.input_cursor as u16;
            if cursor_x < input_inner.x + input_inner.width {
                frame.set_cursor_position((cursor_x, input_inner.y));
            }
        }

        // ── Help ──
        let help = Paragraph::new("  [Enter] execute  [↑↓] history  [Ctrl+L] clear  [PgUp/Dn] scroll  [Esc] close")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[3]);
    }

    fn on_pause(&mut self) {}
    fn on_resume(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        let hist: Vec<Value> = self.history.iter().map(|h| Value::String(h.clone())).collect();
        Some(serde_json::json!({ "history": hist, "cwd": self.cwd }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(hist) = state.get("history").and_then(|v| v.as_array()) {
            self.history = hist.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            self.history_idx = self.history.len();
        }
        if let Some(cwd) = state.get("cwd").and_then(|v| v.as_str()) {
            self.cwd = cwd.to_string();
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
