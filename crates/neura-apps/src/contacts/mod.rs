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

/// A single contact entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub created_at: String,
}

/// The current interaction mode of the contacts app.
#[derive(Debug, Clone, PartialEq)]
enum Mode {
    List,
    View,
    AddName,
    AddEmail,
    AddPhone,
}

/// Which field is currently being entered during the Add flow.
#[derive(Debug, Clone, Default)]
struct AddBuffer {
    name: String,
    email: String,
    phone: String,
}

pub struct ContactsApp {
    vfs: Arc<Vfs>,
    username: String,
    contacts: Vec<Contact>,
    selected: usize,
    mode: Mode,
    add_buf: AddBuffer,
    initialized: bool,
}

impl ContactsApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        Self {
            vfs,
            username: username.to_string(),
            contacts: Vec::new(),
            selected: 0,
            mode: Mode::List,
            add_buf: AddBuffer::default(),
            initialized: false,
        }
    }

    /// VFS path where contacts are persisted.
    fn contacts_path(&self) -> String {
        format!("/home/{}/contacts.json", self.username)
    }

    /// Persist the full contact list to the VFS asynchronously.
    fn save_contacts(&self) {
        let vfs = self.vfs.clone();
        let path = self.contacts_path();
        let contacts = self.contacts.clone();
        let username = self.username.clone();
        tokio::spawn(async move {
            if let Ok(data) = serde_json::to_vec_pretty(&contacts) {
                let _ = vfs.write_file(&path, data, &username).await;
            }
        });
    }

    /// Add a contact programmatically (used by AI tool handler as well).
    fn add_contact(&mut self, name: String, email: String, phone: String) {
        let now = Utc::now().to_rfc3339();
        self.contacts.push(Contact {
            name,
            email,
            phone,
            created_at: now,
        });
        self.save_contacts();
    }
}

impl App for ContactsApp {
    fn id(&self) -> &str { "contacts" }
    fn name(&self) -> &str { "NeuraContacts" }

    fn init(&mut self) -> anyhow::Result<()> {
        if !self.initialized {
            self.initialized = true;
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            // ── List mode ──
            Mode::List => {
                match key.code {
                    KeyCode::Esc => false, // exit app
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected + 1 < self.contacts.len() {
                            self.selected += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        if !self.contacts.is_empty() {
                            self.mode = Mode::View;
                        }
                        true
                    }
                    KeyCode::Char('a') => {
                        self.add_buf = AddBuffer::default();
                        self.mode = Mode::AddName;
                        true
                    }
                    KeyCode::Char('d') => {
                        if !self.contacts.is_empty() {
                            self.contacts.remove(self.selected);
                            if self.selected > 0 && self.selected >= self.contacts.len() {
                                self.selected = self.contacts.len().saturating_sub(1);
                            }
                            self.save_contacts();
                        }
                        true
                    }
                    _ => true,
                }
            }

            // ── View mode ──
            Mode::View => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = Mode::List;
                        true
                    }
                    _ => true,
                }
            }

            // ── Add: entering name ──
            Mode::AddName => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Enter => {
                        if !self.add_buf.name.is_empty() {
                            self.mode = Mode::AddEmail;
                        }
                        true
                    }
                    KeyCode::Char(c) => {
                        self.add_buf.name.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.add_buf.name.pop();
                        true
                    }
                    _ => true,
                }
            }

            // ── Add: entering email ──
            Mode::AddEmail => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::AddPhone;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.add_buf.email.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.add_buf.email.pop();
                        true
                    }
                    _ => true,
                }
            }

            // ── Add: entering phone ──
            Mode::AddPhone => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Enter => {
                        // Finalise the new contact.
                        let name = self.add_buf.name.clone();
                        let email = self.add_buf.email.clone();
                        let phone = self.add_buf.phone.clone();
                        self.add_contact(name, email, phone);
                        self.selected = self.contacts.len().saturating_sub(1);
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.add_buf.phone.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.add_buf.phone.pop();
                        true
                    }
                    _ => true,
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.mode {
            Mode::List => self.render_list(frame, area),
            Mode::View => self.render_view(frame, area),
            Mode::AddName | Mode::AddEmail | Mode::AddPhone => self.render_add(frame, area),
        }
    }

    fn on_close(&mut self) {
        self.save_contacts();
    }

    fn save_state(&self) -> Option<Value> {
        serde_json::to_value(&self.contacts).ok()
    }

    fn load_state(&mut self, state: Value) {
        if let Ok(contacts) = serde_json::from_value(state) {
            self.contacts = contacts;
        }
    }

    fn ai_tools(&self) -> Vec<Value> {
        vec![serde_json::json!({
            "name": "add_contact",
            "description": "Add a new contact to NeuraContacts",
            "parameters": {
                "type": "object",
                "properties": {
                    "name":  { "type": "string", "description": "Contact name" },
                    "email": { "type": "string", "description": "Email address" },
                    "phone": { "type": "string", "description": "Phone number" }
                },
                "required": ["name", "email", "phone"]
            }
        })]
    }

    fn handle_ai_tool(&mut self, tool_name: &str, args: Value) -> Option<Value> {
        match tool_name {
            "add_contact" => {
                let name = args.get("name")?.as_str()?.to_string();
                let email = args.get("email")?.as_str()?.to_string();
                let phone = args.get("phone")?.as_str()?.to_string();
                self.add_contact(name.clone(), email, phone);
                Some(serde_json::json!({"status": "added", "name": name}))
            }
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ─── Rendering helpers ───────────────────────────────────────────────────────

impl ContactsApp {
    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        // Header
        let header = Paragraph::new(format!(" {} contacts", self.contacts.len()))
            .style(Style::default().fg(ORANGE))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER))
                    .title(" NeuraContacts ")
                    .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            );
        frame.render_widget(header, chunks[0]);

        // Contact list
        if self.contacts.is_empty() {
            let empty = Paragraph::new("  No contacts yet. Press [a] to add one.")
                .style(Style::default().fg(MUTED))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(BORDER)),
                );
            frame.render_widget(empty, chunks[1]);
        } else {
            let items: Vec<ListItem> = self
                .contacts
                .iter()
                .enumerate()
                .map(|(i, contact)| {
                    let style = if i == self.selected {
                        Style::default()
                            .fg(PRIMARY)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TEXT)
                    };
                    let prefix = if i == self.selected { " > " } else { "   " };
                    ListItem::new(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(&contact.name, style),
                        Span::styled(
                            format!("  <{}>", contact.email),
                            Style::default().fg(MUTED),
                        ),
                    ]))
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER)),
            );
            frame.render_widget(list, chunks[1]);
        }

        // Help
        let help =
            Paragraph::new(" [a]dd  [Enter]view  [d]elete  [Up/Down]navigate  [Esc]back")
                .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[2]);
    }

    fn render_view(&self, frame: &mut Frame, area: Rect) {
        let contact = match self.contacts.get(self.selected) {
            Some(c) => c,
            None => {
                let msg = Paragraph::new("  No contact selected.")
                    .style(Style::default().fg(MUTED));
                frame.render_widget(msg, area);
                return;
            }
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(area);

        let detail_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name:     ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(&contact.name, Style::default().fg(TEXT)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Email:    ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(&contact.email, Style::default().fg(GREEN)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Phone:    ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(&contact.phone, Style::default().fg(ORANGE)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Added:    ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(&contact.created_at, Style::default().fg(MUTED)),
            ]),
        ];

        let detail = Paragraph::new(detail_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(format!(" Contact: {} ", contact.name))
                .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
        );
        frame.render_widget(detail, chunks[0]);

        let help = Paragraph::new(" [Esc/q] back to list")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[1]);
    }

    fn render_add(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(area);

        let (step_label, step_num) = match self.mode {
            Mode::AddName => ("Name", 1),
            Mode::AddEmail => ("Email", 2),
            Mode::AddPhone => ("Phone", 3),
            _ => ("", 0),
        };

        let current_input = match self.mode {
            Mode::AddName => &self.add_buf.name,
            Mode::AddEmail => &self.add_buf.email,
            Mode::AddPhone => &self.add_buf.phone,
            _ => &self.add_buf.name,
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Step {}/3: Enter {}", step_num, step_label),
                Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        // Show previously entered fields
        let name_style = if self.mode == Mode::AddName {
            Style::default().fg(GREEN)
        } else {
            Style::default().fg(MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled("  Name:  ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(
                if self.mode == Mode::AddName {
                    format!("{}|", self.add_buf.name)
                } else {
                    self.add_buf.name.clone()
                },
                name_style,
            ),
        ]));

        let email_style = if self.mode == Mode::AddEmail {
            Style::default().fg(GREEN)
        } else {
            Style::default().fg(MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled("  Email: ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(
                if self.mode == Mode::AddEmail {
                    format!("{}|", self.add_buf.email)
                } else {
                    self.add_buf.email.clone()
                },
                email_style,
            ),
        ]));

        let phone_style = if self.mode == Mode::AddPhone {
            Style::default().fg(GREEN)
        } else {
            Style::default().fg(MUTED)
        };
        lines.push(Line::from(vec![
            Span::styled("  Phone: ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(
                if self.mode == Mode::AddPhone {
                    format!("{}|", self.add_buf.phone)
                } else {
                    self.add_buf.phone.clone()
                },
                phone_style,
            ),
        ]));

        // Active input prompt
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  > ", Style::default().fg(GREEN)),
            Span::styled(current_input.as_str(), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().fg(GREEN).add_modifier(Modifier::RAPID_BLINK)),
        ]));

        let form = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PRIMARY))
                .title(" Add Contact ")
                .title_style(
                    Style::default()
                        .fg(ORANGE)
                        .add_modifier(Modifier::BOLD),
                ),
        );
        frame.render_widget(form, chunks[0]);

        let help = Paragraph::new(" [Enter] next step  [Esc] cancel")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[1]);
    }
}
