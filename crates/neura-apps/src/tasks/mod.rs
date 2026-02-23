use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    pub done: bool,
    pub priority: Priority,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Low => write!(f, "LOW"),
            Priority::Medium => write!(f, "MED"),
            Priority::High => write!(f, "HIGH"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    List,
    AddTask,
}

pub struct TasksApp {
    vfs: Arc<Vfs>,
    username: String,
    tasks: Vec<Task>,
    selected: usize,
    mode: Mode,
    input_buffer: String,
    show_done: bool,
    initialized: bool,
}

impl TasksApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        Self {
            vfs,
            username: username.to_string(),
            tasks: Vec::new(),
            selected: 0,
            mode: Mode::List,
            input_buffer: String::new(),
            show_done: true,
            initialized: false,
        }
    }

    fn tasks_file(&self) -> String {
        format!("/home/{}/tasks.task", self.username)
    }

    fn save_tasks(&self) {
        let vfs = self.vfs.clone();
        let path = self.tasks_file();
        let tasks = self.tasks.clone();
        let username = self.username.clone();
        tokio::spawn(async move {
            if let Ok(data) = serde_json::to_vec_pretty(&tasks) {
                let _ = vfs.write_file(&path, data, &username).await;
            }
        });
    }

    fn visible_tasks(&self) -> Vec<(usize, &Task)> {
        self.tasks.iter().enumerate()
            .filter(|(_, t)| self.show_done || !t.done)
            .collect()
    }
}

impl App for TasksApp {
    fn id(&self) -> &str { "tasks" }
    fn name(&self) -> &str { "NeuraTasks" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.initialized = true;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            Mode::List => {
                match key.code {
                    KeyCode::Char('a') => {
                        self.mode = Mode::AddTask;
                        self.input_buffer.clear();
                        true
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        let visible = self.visible_tasks();
                        if let Some(&(real_idx, _)) = visible.get(self.selected) {
                            self.tasks[real_idx].done = !self.tasks[real_idx].done;
                            self.save_tasks();
                        }
                        true
                    }
                    KeyCode::Char('d') => {
                        let visible = self.visible_tasks();
                        if let Some(&(real_idx, _)) = visible.get(self.selected) {
                            self.tasks.remove(real_idx);
                            if self.selected > 0 && self.selected >= self.visible_tasks().len() {
                                self.selected = self.visible_tasks().len().saturating_sub(1);
                            }
                            self.save_tasks();
                        }
                        true
                    }
                    KeyCode::Char('p') => {
                        let visible = self.visible_tasks();
                        if let Some(&(real_idx, _)) = visible.get(self.selected) {
                            self.tasks[real_idx].priority = match self.tasks[real_idx].priority {
                                Priority::Low => Priority::Medium,
                                Priority::Medium => Priority::High,
                                Priority::High => Priority::Low,
                            };
                            self.save_tasks();
                        }
                        true
                    }
                    KeyCode::Char('h') => {
                        self.show_done = !self.show_done;
                        self.selected = 0;
                        true
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected > 0 { self.selected -= 1; }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let count = self.visible_tasks().len();
                        if self.selected + 1 < count { self.selected += 1; }
                        true
                    }
                    KeyCode::Esc => false,
                    _ => true,
                }
            }
            Mode::AddTask => {
                match key.code {
                    KeyCode::Enter => {
                        if !self.input_buffer.is_empty() {
                            self.tasks.push(Task {
                                title: self.input_buffer.clone(),
                                done: false,
                                priority: Priority::Medium,
                                created_at: Utc::now().to_rfc3339(),
                            });
                            self.save_tasks();
                        }
                        self.mode = Mode::List;
                        self.selected = self.visible_tasks().len().saturating_sub(1);
                        true
                    }
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                        true
                    }
                    _ => true,
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        // Header with stats
        let total = self.tasks.len();
        let done = self.tasks.iter().filter(|t| t.done).count();
        let header = Paragraph::new(format!(" Tasks: {} total, {} done, {} remaining", total, done, total - done))
            .style(Style::default().fg(ORANGE))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER))
                .title(" NeuraTasks ").title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
        frame.render_widget(header, chunks[0]);

        // Task list or add input
        match self.mode {
            Mode::List => {
                let visible = self.visible_tasks();
                let items: Vec<ListItem> = visible.iter().enumerate().map(|(vi, &(_, task))| {
                    let checkbox = if task.done { "[x]" } else { "[ ]" };
                    let prio = match task.priority {
                        Priority::High => ("!", RED),
                        Priority::Medium => ("~", ORANGE),
                        Priority::Low => (" ", MUTED),
                    };
                    let style = if vi == self.selected {
                        Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
                    } else if task.done {
                        Style::default().fg(DIM)
                    } else {
                        Style::default().fg(TEXT)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!(" {} ", checkbox), style),
                        Span::styled(format!("[{}] ", prio.0), Style::default().fg(prio.1)),
                        Span::styled(&task.title, style),
                    ]))
                }).collect();

                let list = List::new(items)
                    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)));
                frame.render_widget(list, chunks[1]);
            }
            Mode::AddTask => {
                let input = Paragraph::new(format!("  New task: {}", self.input_buffer))
                    .style(Style::default().fg(GREEN))
                    .block(Block::default().borders(Borders::ALL)
                        .border_style(Style::default().fg(PRIMARY))
                        .title(" Add Task (Enter to save, Esc to cancel) "));
                frame.render_widget(input, chunks[1]);
            }
        }

        // Help bar
        let help = Paragraph::new(" [a]dd  [Space]toggle  [d]elete  [p]riority  [h]ide done  [Esc]back")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[2]);
    }

    fn on_close(&mut self) {
        self.save_tasks();
    }

    fn save_state(&self) -> Option<Value> {
        serde_json::to_value(&self.tasks).ok()
    }

    fn load_state(&mut self, state: Value) {
        if let Ok(tasks) = serde_json::from_value(state) {
            self.tasks = tasks;
        }
    }

    fn ai_tools(&self) -> Vec<Value> {
        vec![serde_json::json!({
            "name": "add_task",
            "description": "Add a todo task",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "priority": { "type": "string", "enum": ["Low", "Medium", "High"] }
                },
                "required": ["title"]
            }
        })]
    }

    fn handle_ai_tool(&mut self, tool_name: &str, args: Value) -> Option<Value> {
        match tool_name {
            "add_task" => {
                let title = args.get("title")?.as_str()?.to_string();
                let priority = args.get("priority")
                    .and_then(|p| p.as_str())
                    .map(|p| match p {
                        "High" => Priority::High,
                        "Low" => Priority::Low,
                        _ => Priority::Medium,
                    })
                    .unwrap_or(Priority::Medium);
                self.tasks.push(Task {
                    title: title.clone(),
                    done: false,
                    priority,
                    created_at: Utc::now().to_rfc3339(),
                });
                self.save_tasks();
                Some(serde_json::json!({"status": "added", "title": title}))
            }
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
