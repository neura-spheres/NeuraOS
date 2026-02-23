use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

/// Unread message indicator (same as ORANGE in the palette).
const UNREAD: ratatui::style::Color = ORANGE;

// ── Provider database (SMTP + IMAP) ──────────────────────────────────────────

struct ProviderInfo {
    name:          &'static str,
    smtp_host:     &'static str,
    smtp_port:     u16,
    smtp_starttls: bool,
    imap_host:     &'static str,
    imap_port:     u16,
    note:          &'static str,
}

fn detect_provider(email: &str) -> Option<ProviderInfo> {
    let domain = email.split('@').nth(1)?.to_lowercase();
    match domain.as_str() {
        "gmail.com" | "googlemail.com" => Some(ProviderInfo {
            name: "Gmail",
            smtp_host: "smtp.gmail.com",        smtp_port: 587, smtp_starttls: true,
            imap_host: "imap.gmail.com",        imap_port: 993,
            note: "Use an App Password — myaccount.google.com/apppasswords",
        }),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => Some(ProviderInfo {
            name: "Outlook / Hotmail",
            smtp_host: "smtp-mail.outlook.com",  smtp_port: 587, smtp_starttls: true,
            imap_host: "outlook.office365.com",  imap_port: 993,
            note: "",
        }),
        "yahoo.com" | "ymail.com" => Some(ProviderInfo {
            name: "Yahoo Mail",
            smtp_host: "smtp.mail.yahoo.com",    smtp_port: 587, smtp_starttls: true,
            imap_host: "imap.mail.yahoo.com",    imap_port: 993,
            note: "Use an App Password from Yahoo account security settings",
        }),
        "icloud.com" | "me.com" | "mac.com" => Some(ProviderInfo {
            name: "iCloud Mail",
            smtp_host: "smtp.mail.me.com",       smtp_port: 587, smtp_starttls: true,
            imap_host: "imap.mail.me.com",       imap_port: 993,
            note: "Use an App-Specific Password from appleid.apple.com",
        }),
        "protonmail.com" | "proton.me" | "pm.me" => Some(ProviderInfo {
            name: "Proton Mail",
            smtp_host: "127.0.0.1",              smtp_port: 1025, smtp_starttls: false,
            imap_host: "127.0.0.1",              imap_port: 1143,
            note: "Requires Proton Mail Bridge running locally",
        }),
        "zoho.com" | "zohomail.com" => Some(ProviderInfo {
            name: "Zoho Mail",
            smtp_host: "smtp.zoho.com",          smtp_port: 587, smtp_starttls: true,
            imap_host: "imap.zoho.com",          imap_port: 993,
            note: "",
        }),
        "fastmail.com" | "fastmail.fm" | "fastmail.net" => Some(ProviderInfo {
            name: "Fastmail",
            smtp_host: "smtp.fastmail.com",      smtp_port: 587, smtp_starttls: true,
            imap_host: "imap.fastmail.com",      imap_port: 993,
            note: "",
        }),
        _ => None,
    }
}

fn guess_hosts(email: &str) -> (String, String) {
    let domain = email.split('@').nth(1).unwrap_or("").to_lowercase();
    (format!("smtp.{}", domain), format!("imap.{}", domain))
}

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct AccountInfo {
    display_name:  String,
    email:         String,
    password:      String,
    account_type:  String,
    provider_name: String,
    smtp_host:     String,
    smtp_port:     u16,
    smtp_starttls: bool,
    imap_host:     String,
    imap_port:     u16,
}

impl AccountInfo {
    fn is_configured(&self) -> bool { !self.email.is_empty() && !self.smtp_host.is_empty() }
    fn has_imap(&self)     -> bool { !self.imap_host.is_empty() && self.imap_port > 0 }
    fn provider_str(&self) -> &str { if self.provider_name.is_empty() { "Custom" } else { &self.provider_name } }
}

/// A message fetched from the IMAP server.
#[derive(Debug, Clone)]
struct InboxEmail {
    uid:      u32,
    _seq:      u32,
    from:     String,   // display string "Name <addr>" or just "addr"
    subject:  String,
    date_raw: String,   // raw Date header value
    date_fmt: String,   // formatted for display
    is_read:  bool,
    body:     Option<String>,   // None until loaded
    preview:  String,           // first 120 chars of body (if available)
}

/// A message sent locally via SMTP.
#[derive(Debug, Clone)]
struct SentEmail {
    id:      String,
    to:      String,
    subject: String,
    body:    String,
    sent_at: String,
}

#[derive(Debug, Clone, PartialEq)]
enum FolderTab { Inbox, Sent }

impl FolderTab {
    fn label(&self) -> &'static str { match self { Self::Inbox => "Inbox", Self::Sent => "Sent" } }
}

#[derive(Debug, Clone, PartialEq)]
enum ActivePane { Folders, MessageList, Preview }

#[derive(Debug, Clone, PartialEq)]
enum View { Main, Compose, Setup, Progress(String), Done(String), Error(String) }

#[derive(Debug, Clone, PartialEq)]
enum ComposeField { To, Subject, Body }

#[derive(Debug, Clone, PartialEq)]
enum SetupField { AccountType, Name, Email, Password, CustomSmtp, CustomImap, CustomPort }

// ── MailApp struct ────────────────────────────────────────────────────────────

pub struct MailApp {
    vfs: Arc<Vfs>,
    username: String,
    account: AccountInfo,

    // Inbox (IMAP)
    inbox: Vec<InboxEmail>,
    inbox_sel: usize,
    inbox_scroll: usize,
    inbox_loading: bool,   // true while async_fetch_inbox is in flight

    // Sent (local)
    sent: Vec<SentEmail>,
    sent_sel: usize,
    sent_scroll: usize,

    // 3-pane state
    folder: FolderTab,
    pane: ActivePane,
    preview_scroll: usize,

    // View
    view: View,

    // Compose
    compose_to: String, compose_subject: String, compose_body: String,
    compose_cursor: usize, compose_field: ComposeField, compose_scroll: usize,

    // Setup wizard
    setup_type: String,   // "Personal" or "Business"
    setup_name: String,   setup_email: String,  setup_password: String,
    setup_smtp: String,   setup_imap: String,   setup_port: String,
    setup_field: SetupField, setup_cursor: usize,

    // Async flags (read by main.rs)
    needs_load_flag: bool,
    pub inbox_needs_fetch: bool,
    pub pending_body_uid: Option<u32>,
    pub pending_delete_uid: Option<u32>,
    pub status_msg: String,
}

impl MailApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        Self {
            vfs, username: username.to_string(),
            account: AccountInfo { smtp_port: 587, smtp_starttls: true, imap_port: 993, ..Default::default() },
            inbox: Vec::new(), inbox_sel: 0, inbox_scroll: 0, inbox_loading: false,
            sent: Vec::new(),  sent_sel: 0,  sent_scroll: 0,
            folder: FolderTab::Inbox, pane: ActivePane::MessageList, preview_scroll: 0,
            view: View::Main,
            compose_to: String::new(), compose_subject: String::new(),
            compose_body: String::new(), compose_cursor: 0,
            compose_field: ComposeField::To, compose_scroll: 0,
            setup_type: "Personal".into(), setup_name: String::new(),
            setup_email: String::new(),    setup_password: String::new(),
            setup_smtp: String::new(),     setup_imap: String::new(),
            setup_port: "587".into(),      setup_field: SetupField::Name, setup_cursor: 0,
            needs_load_flag: true, inbox_needs_fetch: false,
            pending_body_uid: None, pending_delete_uid: None,
            status_msg: String::new(),
        }
    }

    // ── Async flags ───────────────────────────────────────────────────────────

    pub fn needs_load(&self) -> bool { self.needs_load_flag }

    pub async fn async_load(&mut self) {
        self.needs_load_flag = false;
        let cfg = format!("/home/{}/mail_account.json", self.username);
        if let Ok(data) = self.vfs.read_file(&cfg).await {
            if let Ok(v) = serde_json::from_slice::<Value>(&data) {
                let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let n = |k: &str, d: u64| v.get(k).and_then(|x| x.as_u64()).unwrap_or(d) as u16;
                let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d);
                self.account.display_name  = s("display_name");
                self.account.email         = s("email");
                self.account.password      = s("password");
                self.account.account_type  = s("account_type");
                self.account.provider_name = s("provider_name");
                self.account.smtp_host     = s("smtp_host");
                self.account.smtp_port     = n("smtp_port", 587);
                self.account.smtp_starttls = b("smtp_starttls", true);
                self.account.imap_host     = s("imap_host");
                self.account.imap_port     = n("imap_port", 993);
                // Trigger inbox fetch if we have IMAP
                if self.account.has_imap() { self.inbox_needs_fetch = true; }
            }
        }
        let sent_path = format!("/home/{}/mail_sent.json", self.username);
        if let Ok(data) = self.vfs.read_file(&sent_path).await {
            if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&data) {
                self.sent = arr.iter().filter_map(|m| {
                    let g = |k: &str| -> Option<String> { Some(m.get(k)?.as_str()?.to_string()) };
                    Some(SentEmail { id: g("id")?, to: g("to")?, subject: g("subject")?,
                        body: g("body")?, sent_at: g("sent_at")? })
                }).collect();
            }
        }
    }

    pub async fn async_save_account(&mut self) {
        self.status_msg.clear();
        let email = self.setup_email.trim().to_string();
        let (smtp_host, smtp_port, smtp_starttls, imap_host, imap_port, provider) =
            if let Some(p) = detect_provider(&email) {
                (p.smtp_host.to_string(), p.smtp_port, p.smtp_starttls,
                 p.imap_host.to_string(), p.imap_port, p.name.to_string())
            } else {
                let smtp = if self.setup_smtp.trim().is_empty() { guess_hosts(&email).0 } else { self.setup_smtp.trim().to_string() };
                let imap = if self.setup_imap.trim().is_empty() { guess_hosts(&email).1 } else { self.setup_imap.trim().to_string() };
                let port: u16 = self.setup_port.parse().unwrap_or(587);
                (smtp, port, true, imap, 993, "Custom".to_string())
            };
        self.account.display_name  = self.setup_name.trim().to_string();
        self.account.email         = email;
        self.account.password      = self.setup_password.clone();
        self.account.account_type  = self.setup_type.clone();
        self.account.provider_name = provider;
        self.account.smtp_host     = smtp_host;
        self.account.smtp_port     = smtp_port;
        self.account.smtp_starttls = smtp_starttls;
        self.account.imap_host     = imap_host;
        self.account.imap_port     = imap_port;
        let v = serde_json::json!({
            "display_name": self.account.display_name, "email": self.account.email,
            "password": self.account.password,         "account_type": self.account.account_type,
            "provider_name": self.account.provider_name,
            "smtp_host": self.account.smtp_host, "smtp_port": self.account.smtp_port,
            "smtp_starttls": self.account.smtp_starttls,
            "imap_host": self.account.imap_host, "imap_port": self.account.imap_port,
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&v) {
            let path = format!("/home/{}/mail_account.json", self.username);
            let _ = self.vfs.write_file(&path, bytes, &self.username).await;
        }
        if self.account.has_imap() { self.inbox_needs_fetch = true; }
        self.view = View::Done(format!("Account saved! ({})", self.account.email));
    }

    pub async fn async_send(&mut self) {
        self.status_msg.clear();
        let to      = self.compose_to.trim().to_string();
        let subject = self.compose_subject.trim().to_string();
        let body    = self.compose_body.clone();
        if to.is_empty() || subject.is_empty() {
            self.view = View::Error("To and Subject cannot be empty.".to_string()); return;
        }
        if !self.account.is_configured() {
            self.view = View::Error("No account. Press [a] to set one up.".to_string()); return;
        }
        let from_str = if self.account.display_name.is_empty() {
            self.account.email.clone()
        } else {
            format!("{} <{}>", self.account.display_name, self.account.email)
        };
        let email = match build_lettre_message(&from_str, &to, &subject, &body) {
            Ok(e) => e,
            Err(e) => { self.view = View::Error(format!("Build error: {}", e)); return; }
        };
        match send_via_smtp(email, &self.account.smtp_host, self.account.smtp_port,
            &self.account.email, &self.account.password, self.account.smtp_starttls).await
        {
            Ok(()) => {
                let msg = SentEmail {
                    id: uuid_simple(), to, subject, body, sent_at: Utc::now().to_rfc3339()
                };
                self.sent.push(msg);
                self.save_sent().await;
                self.compose_to.clear(); self.compose_subject.clear();
                self.compose_body.clear(); self.compose_cursor = 0;
                self.compose_field = ComposeField::To;
                self.view = View::Done("Email sent!".to_string());
            }
            Err(e) => { self.view = View::Error(format!("Send failed: {}", e)); }
        }
    }

    /// Fetch inbox from IMAP in a blocking thread.
    pub async fn async_fetch_inbox(&mut self) {
        self.inbox_needs_fetch = false;
        if !self.account.has_imap() { return; }
        self.inbox_loading = true;
        let host = self.account.imap_host.clone();
        let port = self.account.imap_port;
        let user = self.account.email.clone();
        let pass = self.account.password.clone();
        match tokio::task::spawn_blocking(move || fetch_imap_blocking(host, port, user, pass)).await {
            Ok(Ok(emails)) => {
                self.inbox = emails;
                if self.inbox_sel >= self.inbox.len() { self.inbox_sel = 0; }
            }
            Ok(Err(e)) => {
                self.view = View::Error(format!("IMAP: {}", e));
            }
            Err(_) => {}
        }
        self.inbox_loading = false;
    }

    /// Fetch full body for a message by UID.
    pub async fn async_fetch_body(&mut self) {
        let uid = match self.pending_body_uid.take() { Some(u) => u, None => return };
        if !self.account.has_imap() { return; }
        let host = self.account.imap_host.clone();
        let port = self.account.imap_port;
        let user = self.account.email.clone();
        let pass = self.account.password.clone();
        match tokio::task::spawn_blocking(move || fetch_imap_body_blocking(host, port, user, pass, uid)).await {
            Ok(Ok(body)) => {
                if let Some(msg) = self.inbox.iter_mut().find(|m| m.uid == uid) {
                    msg.preview = body[..body.len().min(200)].to_string();
                    msg.body    = Some(body);
                    msg.is_read = true;
                }
            }
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    /// Delete a message from IMAP by UID.
    pub async fn async_delete_email(&mut self) {
        let uid = match self.pending_delete_uid.take() { Some(u) => u, None => return };
        // Remove from local list immediately
        self.inbox.retain(|m| m.uid != uid);
        if self.inbox_sel >= self.inbox.len() && self.inbox_sel > 0 { self.inbox_sel -= 1; }
        // Fire-and-forget the IMAP delete
        if !self.account.has_imap() { return; }
        let host = self.account.imap_host.clone();
        let port = self.account.imap_port;
        let user = self.account.email.clone();
        let pass = self.account.password.clone();
        tokio::task::spawn_blocking(move || imap_delete_blocking(host, port, user, pass, uid)).await.ok();
    }

    async fn save_sent(&self) {
        let arr: Vec<Value> = self.sent.iter().map(|m| serde_json::json!({
            "id": m.id, "to": m.to, "subject": m.subject, "body": m.body, "sent_at": m.sent_at,
        })).collect();
        if let Ok(bytes) = serde_json::to_vec_pretty(&Value::Array(arr)) {
            let path = format!("/home/{}/mail_sent.json", self.username);
            let _ = self.vfs.write_file(&path, bytes, &self.username).await;
        }
    }

    // ── Setup helpers ─────────────────────────────────────────────────────────

    fn open_setup(&mut self) {
        self.setup_name     = self.account.display_name.clone();
        self.setup_email    = self.account.email.clone();
        self.setup_password = self.account.password.clone();
        self.setup_type     = if self.account.account_type.is_empty() { "Personal".into() } else { self.account.account_type.clone() };
        self.setup_field    = SetupField::Name;
        self.setup_cursor   = self.setup_name.len();
        self.setup_smtp     = if self.account.provider_name.is_empty() || self.account.provider_name == "Custom" { self.account.smtp_host.clone() } else { String::new() };
        self.setup_imap     = if self.account.provider_name.is_empty() || self.account.provider_name == "Custom" { self.account.imap_host.clone() } else { String::new() };
        self.setup_port     = self.account.smtp_port.to_string();
        self.view = View::Setup;
    }

    fn setup_needs_custom(&self) -> bool {
        detect_provider(&self.setup_email).is_none() && self.setup_email.contains('@')
    }

    fn setup_cursor_len(&self) -> usize {
        match self.setup_field {
            SetupField::Name        => self.setup_name.len(),
            SetupField::Email       => self.setup_email.len(),
            SetupField::Password    => self.setup_password.len(),
            SetupField::CustomSmtp  => self.setup_smtp.len(),
            SetupField::CustomImap  => self.setup_imap.len(),
            SetupField::CustomPort  => self.setup_port.len(),
            SetupField::AccountType => 0,
        }
    }

    fn setup_insert(&mut self, c: char) {
        if self.setup_field == SetupField::AccountType { return; }
        let cur = self.setup_cursor;
        match self.setup_field {
            SetupField::Name       => { self.setup_name.insert(cur, c); }
            SetupField::Email      => { self.setup_email.insert(cur, c); }
            SetupField::Password   => { self.setup_password.insert(cur, c); }
            SetupField::CustomSmtp => { self.setup_smtp.insert(cur, c); }
            SetupField::CustomImap => { self.setup_imap.insert(cur, c); }
            SetupField::CustomPort => { self.setup_port.insert(cur, c); }
            SetupField::AccountType => {}
        }
        self.setup_cursor += 1;
    }

    fn setup_backspace(&mut self) {
        if self.setup_field == SetupField::AccountType || self.setup_cursor == 0 { return; }
        self.setup_cursor -= 1;
        let cur = self.setup_cursor;
        match self.setup_field {
            SetupField::Name       => { self.setup_name.remove(cur); }
            SetupField::Email      => { self.setup_email.remove(cur); }
            SetupField::Password   => { self.setup_password.remove(cur); }
            SetupField::CustomSmtp => { self.setup_smtp.remove(cur); }
            SetupField::CustomImap => { self.setup_imap.remove(cur); }
            SetupField::CustomPort => { self.setup_port.remove(cur); }
            SetupField::AccountType => {}
        }
    }

    fn setup_next(&mut self) {
        let custom = self.setup_needs_custom();
        self.setup_field = match (&self.setup_field, custom) {
            (SetupField::AccountType, _)     => SetupField::Name,
            (SetupField::Name,        _)     => SetupField::Email,
            (SetupField::Email,       _)     => SetupField::Password,
            (SetupField::Password,    false) => SetupField::AccountType,
            (SetupField::Password,    true)  => SetupField::CustomSmtp,
            (SetupField::CustomSmtp,  _)     => SetupField::CustomImap,
            (SetupField::CustomImap,  _)     => SetupField::CustomPort,
            (SetupField::CustomPort,  _)     => SetupField::AccountType,
        };
        self.setup_cursor = self.setup_cursor_len();
    }

    fn setup_prev(&mut self) {
        let custom = self.setup_needs_custom();
        self.setup_field = match (&self.setup_field, custom) {
            (SetupField::AccountType, false) => SetupField::Password,
            (SetupField::AccountType, true)  => SetupField::CustomPort,
            (SetupField::Name,        _)     => SetupField::AccountType,
            (SetupField::Email,       _)     => SetupField::Name,
            (SetupField::Password,    _)     => SetupField::Email,
            (SetupField::CustomSmtp,  _)     => SetupField::Password,
            (SetupField::CustomImap,  _)     => SetupField::CustomSmtp,
            (SetupField::CustomPort,  _)     => SetupField::CustomImap,
        };
        self.setup_cursor = self.setup_cursor_len();
    }

    // ── Helpers for 3-pane ────────────────────────────────────────────────────

    fn current_message_count(&self) -> usize {
        match self.folder { FolderTab::Inbox => self.inbox.len(), FolderTab::Sent => self.sent.len() }
    }

    fn current_sel(&self) -> usize {
        match self.folder { FolderTab::Inbox => self.inbox_sel, FolderTab::Sent => self.sent_sel }
    }

    fn current_sel_mut(&mut self) -> &mut usize {
        match self.folder { FolderTab::Inbox => &mut self.inbox_sel, FolderTab::Sent => &mut self.sent_sel }
    }

    fn current_scroll_mut(&mut self) -> &mut usize {
        match self.folder { FolderTab::Inbox => &mut self.inbox_scroll, FolderTab::Sent => &mut self.sent_scroll }
    }

    fn inbox_unread_count(&self) -> usize { self.inbox.iter().filter(|m| !m.is_read).count() }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for MailApp {
    fn id(&self) -> &str { "mail" }
    fn name(&self) -> &str { "NeuraMail" }
    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.view.clone() {
            View::Main        => self.handle_main_key(key),
            View::Compose     => self.handle_compose_key(key),
            View::Setup       => self.handle_setup_key(key),
            View::Progress(_) => { if key.code == KeyCode::Esc { return false; } true }
            View::Done(_) | View::Error(_) => {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => { self.view = View::Main; }
                    _ => {}
                }
                true
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match &self.view {
            View::Main          => self.render_main(frame, area),
            View::Compose       => self.render_compose(frame, area),
            View::Setup         => self.render_setup(frame, area),
            View::Progress(msg) => render_overlay(frame, area, CYAN,  "Please Wait", msg),
            View::Done(msg)     => render_overlay(frame, area, GREEN, "Done",        msg),
            View::Error(msg)    => render_overlay(frame, area, RED,   "Error",       msg),
        }
    }

    fn on_resume(&mut self) { self.needs_load_flag = true; }
    fn on_pause(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        let sent: Vec<Value> = self.sent.iter().map(|m| serde_json::json!({
            "id": m.id, "to": m.to, "subject": m.subject, "body": m.body, "sent_at": m.sent_at,
        })).collect();
        Some(serde_json::json!({ "sent": sent }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(arr) = state.get("sent").and_then(|v| v.as_array()) {
            self.sent = arr.iter().filter_map(|m| {
                let g = |k: &str| -> Option<String> { Some(m.get(k)?.as_str()?.to_string()) };
                Some(SentEmail { id: g("id")?, to: g("to")?, subject: g("subject")?,
                    body: g("body")?, sent_at: g("sent_at")? })
            }).collect();
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Key handlers ──────────────────────────────────────────────────────────────

impl MailApp {
    fn handle_main_key(&mut self, key: KeyEvent) -> bool {
        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('r') => { self.inbox_needs_fetch = true; return true; }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => return false,
            // F5 = refresh
            KeyCode::F(5) => { self.inbox_needs_fetch = true; }
            // [a] account setup
            KeyCode::Char('a') => { self.open_setup(); }
            // [n] compose
            KeyCode::Char('n') => {
                if !self.account.is_configured() { self.open_setup(); }
                else { self.start_compose(None); }
            }
            // [r] reply
            KeyCode::Char('r') => {
                if self.account.is_configured() { self.start_reply(); }
            }
            // [d] delete
            KeyCode::Char('d') => { self.delete_selected(); }
            // [Tab] cycle pane
            KeyCode::Tab => {
                self.pane = match self.pane {
                    ActivePane::Folders     => ActivePane::MessageList,
                    ActivePane::MessageList => ActivePane::Preview,
                    ActivePane::Preview     => ActivePane::Folders,
                };
            }
            // [1/2] folder selection
            KeyCode::Char('1') => { self.folder = FolderTab::Inbox; self.pane = ActivePane::MessageList; }
            KeyCode::Char('2') => { self.folder = FolderTab::Sent;  self.pane = ActivePane::MessageList; }
            // Navigation depends on active pane
            KeyCode::Up | KeyCode::Char('k') => self.nav_up(),
            KeyCode::Down | KeyCode::Char('j') => self.nav_down(),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.nav_enter(),
            KeyCode::Char('h') | KeyCode::Left => { self.pane = ActivePane::Folders; }
            // Mark unread
            KeyCode::Char('u') => { self.mark_unread_selected(); }
            // u = mark unread, m = mark read
            KeyCode::Char('m') => { self.mark_read_selected(); }
            _ => {}
        }
        true
    }

    fn nav_up(&mut self) {
        match self.pane {
            ActivePane::Folders => {
                self.folder = FolderTab::Inbox;
            }
            ActivePane::MessageList => {
                let sel = self.current_sel_mut();
                if *sel > 0 { *sel -= 1; }
                let sel_val = self.current_sel();
                let scroll = self.current_scroll_mut();
                if sel_val < *scroll { *scroll = sel_val; }
            }
            ActivePane::Preview => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }
        }
    }

    fn nav_down(&mut self) {
        match self.pane {
            ActivePane::Folders => {
                self.folder = FolderTab::Sent;
            }
            ActivePane::MessageList => {
                let count = self.current_message_count();
                let sel = self.current_sel_mut();
                if count > 0 && *sel + 1 < count { *sel += 1; }
            }
            ActivePane::Preview => {
                self.preview_scroll = self.preview_scroll.saturating_add(1);
            }
        }
    }

    fn nav_enter(&mut self) {
        match self.pane {
            ActivePane::Folders => {
                self.pane = ActivePane::MessageList;
            }
            ActivePane::MessageList => {
                self.preview_scroll = 0;
                self.pane = ActivePane::Preview;
                // Trigger body load for IMAP messages
                if self.folder == FolderTab::Inbox {
                    if let Some(msg) = self.inbox.get(self.inbox_sel) {
                        if msg.body.is_none() {
                            self.pending_body_uid = Some(msg.uid);
                        } else {
                            // Mark as read locally
                            if let Some(m) = self.inbox.get_mut(self.inbox_sel) { m.is_read = true; }
                        }
                    }
                }
            }
            ActivePane::Preview => {}
        }
    }

    fn delete_selected(&mut self) {
        match self.folder {
            FolderTab::Inbox => {
                if let Some(msg) = self.inbox.get(self.inbox_sel) {
                    self.pending_delete_uid = Some(msg.uid);
                }
            }
            FolderTab::Sent => {
                if self.sent_sel < self.sent.len() {
                    self.sent.remove(self.sent_sel);
                    if self.sent_sel > 0 && self.sent_sel >= self.sent.len() { self.sent_sel -= 1; }
                    self.status_msg = "__SAVE_SENT__".to_string();
                }
            }
        }
    }

    fn mark_read_selected(&mut self) {
        if self.folder == FolderTab::Inbox {
            if let Some(m) = self.inbox.get_mut(self.inbox_sel) { m.is_read = true; }
        }
    }

    fn mark_unread_selected(&mut self) {
        if self.folder == FolderTab::Inbox {
            if let Some(m) = self.inbox.get_mut(self.inbox_sel) { m.is_read = false; }
        }
    }

    fn start_compose(&mut self, reply_to: Option<&InboxEmail>) {
        if let Some(orig) = reply_to {
            self.compose_to      = extract_reply_to(&orig.from);
            self.compose_subject = if orig.subject.starts_with("Re:") {
                orig.subject.clone()
            } else {
                format!("Re: {}", orig.subject)
            };
            self.compose_body = format!("\n\n---\nOn {} {} wrote:\n{}", orig.date_fmt, orig.from,
                orig.body.as_deref().unwrap_or("").lines().map(|l| format!("> {}", l)).collect::<Vec<_>>().join("\n"));
        } else {
            self.compose_to.clear(); self.compose_subject.clear(); self.compose_body.clear();
        }
        self.compose_cursor = self.compose_to.len();
        self.compose_field  = if self.compose_to.is_empty() { ComposeField::To } else { ComposeField::Body };
        self.compose_scroll = 0;
        self.view = View::Compose;
    }

    fn start_reply(&mut self) {
        if self.folder == FolderTab::Inbox {
            if let Some(msg) = self.inbox.get(self.inbox_sel).cloned() {
                self.start_compose(Some(&msg));
            }
        }
    }

    fn handle_compose_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    self.view = View::Progress("Sending…".into());
                    self.status_msg = "__SEND__".to_string();
                    return true;
                }
                KeyCode::Char('c') | KeyCode::Char('q') => { self.view = View::Main; return true; }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => { self.view = View::Main; }
            KeyCode::Tab => {
                self.compose_field = match self.compose_field {
                    ComposeField::To => ComposeField::Subject, ComposeField::Subject => ComposeField::Body, ComposeField::Body => ComposeField::To,
                };
                self.compose_cursor = match &self.compose_field {
                    ComposeField::To => self.compose_to.len(), ComposeField::Subject => self.compose_subject.len(), ComposeField::Body => self.compose_body.len(),
                };
            }
            KeyCode::Char(c) => {
                let cur = self.compose_cursor;
                match self.compose_field {
                    ComposeField::To      => { self.compose_to.insert(cur, c); }
                    ComposeField::Subject => { self.compose_subject.insert(cur, c); }
                    ComposeField::Body    => { self.compose_body.insert(cur, c); }
                }
                self.compose_cursor += 1;
            }
            KeyCode::Enter if self.compose_field == ComposeField::Body => {
                self.compose_body.insert(self.compose_cursor, '\n'); self.compose_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.compose_cursor > 0 {
                    self.compose_cursor -= 1;
                    let cur = self.compose_cursor;
                    match self.compose_field {
                        ComposeField::To      => { self.compose_to.remove(cur); }
                        ComposeField::Subject => { self.compose_subject.remove(cur); }
                        ComposeField::Body    => { self.compose_body.remove(cur); }
                    }
                }
            }
            KeyCode::Delete => {
                let len = match self.compose_field { ComposeField::To => self.compose_to.len(), ComposeField::Subject => self.compose_subject.len(), ComposeField::Body => self.compose_body.len() };
                if self.compose_cursor < len {
                    let cur = self.compose_cursor;
                    match self.compose_field { ComposeField::To => { self.compose_to.remove(cur); }, ComposeField::Subject => { self.compose_subject.remove(cur); }, ComposeField::Body => { self.compose_body.remove(cur); } }
                }
            }
            KeyCode::Left  => { if self.compose_cursor > 0 { self.compose_cursor -= 1; } }
            KeyCode::Right => {
                let len = match self.compose_field { ComposeField::To => self.compose_to.len(), ComposeField::Subject => self.compose_subject.len(), ComposeField::Body => self.compose_body.len() };
                if self.compose_cursor < len { self.compose_cursor += 1; }
            }
            KeyCode::Up   if self.compose_field == ComposeField::Body => { self.compose_scroll = self.compose_scroll.saturating_sub(1); }
            KeyCode::Down if self.compose_field == ComposeField::Body => { self.compose_scroll += 1; }
            _ => {}
        }
        true
    }

    fn handle_setup_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            if self.setup_email.trim().is_empty() {
                self.view = View::Error("Email address is required.".into()); return true;
            }
            if self.setup_password.is_empty() {
                self.view = View::Error("Password is required.".into()); return true;
            }
            self.status_msg = "__SAVE_ACCOUNT__".to_string();
            self.view = View::Progress("Saving account…".into());
            return true;
        }
        match key.code {
            KeyCode::Esc        => { self.view = View::Main; }
            KeyCode::Tab | KeyCode::Down    => { self.setup_next(); }
            KeyCode::BackTab | KeyCode::Up  => { self.setup_prev(); }
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if self.setup_field == SetupField::AccountType => {
                self.setup_type = if self.setup_type == "Personal" { "Business".into() } else { "Personal".into() };
            }
            KeyCode::Char(c) => { self.setup_insert(c); }
            KeyCode::Backspace => { self.setup_backspace(); }
            KeyCode::Delete => {
                let len = self.setup_cursor_len(); let cur = self.setup_cursor;
                if cur < len {
                    match self.setup_field {
                        SetupField::Name       => { self.setup_name.remove(cur); }
                        SetupField::Email      => { self.setup_email.remove(cur); }
                        SetupField::Password   => { self.setup_password.remove(cur); }
                        SetupField::CustomSmtp => { self.setup_smtp.remove(cur); }
                        SetupField::CustomImap => { self.setup_imap.remove(cur); }
                        SetupField::CustomPort => { self.setup_port.remove(cur); }
                        SetupField::AccountType => {}
                    }
                }
            }
            KeyCode::Left  => { if self.setup_cursor > 0 { self.setup_cursor -= 1; } }
            KeyCode::Right => { let l = self.setup_cursor_len(); if self.setup_cursor < l { self.setup_cursor += 1; } }
            KeyCode::Home  => { self.setup_cursor = 0; }
            KeyCode::End   => { self.setup_cursor = self.setup_cursor_len(); }
            _ => {}
        }
        true
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

impl MailApp {
    fn render_main(&self, frame: &mut Frame, area: Rect) {
        // Layout: header | content | statusbar
        let outer = Layout::vertical([
            Constraint::Length(4), Constraint::Min(6), Constraint::Length(1),
        ]).split(area);

        // Content: folders | messages
        let content = Layout::horizontal([
            Constraint::Length(22), Constraint::Min(20),
        ]).split(outer[1]);

        // Right side: message list | preview
        let right = Layout::vertical([
            Constraint::Percentage(42), Constraint::Percentage(58),
        ]).split(content[1]);

        self.render_header(frame, outer[0]);
        self.render_folders(frame, content[0]);
        self.render_message_list(frame, right[0]);
        self.render_preview(frame, right[1]);
        self.render_statusbar(frame, outer[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let (email_str, provider_str, type_str, sync_str) = if self.account.is_configured() {
            (
                self.account.email.clone(),
                format!("  {}", self.account.provider_str()),
                format!("  [{}]", self.account.account_type),
                if self.inbox_loading || self.inbox_needs_fetch { "  ↻ syncing…" } else { "" },
            )
        } else {
            ("No account — press [a] to set up".to_string(), String::new(), String::new(), "")
        };

        let unread = self.inbox_unread_count();
        let total_inbox = self.inbox.len();
        let total_sent  = self.sent.len();

        let lines = vec![
            Line::from(vec![
                Span::styled(" NeuraMail ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled("│", Style::default().fg(BORDER)),
                Span::styled(format!(" {} ", email_str), Style::default().fg(TEXT)),
                Span::styled(provider_str, Style::default().fg(CYAN)),
                Span::styled(type_str, Style::default().fg(DIM)),
                Span::styled(sync_str, Style::default().fg(ORANGE)),
            ]),
            Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("Inbox: {} messages", total_inbox), Style::default().fg(MUTED)),
                if unread > 0 {
                    Span::styled(format!("  ({} unread)", unread), Style::default().fg(UNREAD).add_modifier(Modifier::BOLD))
                } else { Span::raw("") },
                Span::styled(format!("   Sent: {}", total_sent), Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::raw(" "),
                Span::styled("[n] compose  [r] reply  [d] delete  [F5/Ctrl+R] refresh  [a] account", Style::default().fg(DIM)),
            ]),
        ];

        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" NeuraMail ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_folders(&self, frame: &mut Frame, area: Rect) {
        let active = self.pane == ActivePane::Folders;
        let border_color = if active { PRIMARY } else { BORDER };
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" Folders ")
            .title_style(Style::default().fg(if active { PRIMARY } else { MUTED }));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let folders = [
            (FolderTab::Inbox, self.inbox_unread_count(), self.inbox.len()),
            (FolderTab::Sent,  0, self.sent.len()),
        ];

        let mut y = inner.y;
        for (tab, unread, total) in &folders {
            if y >= inner.y + inner.height { break; }
            let is_active = &self.folder == tab;
            let prefix = if is_active { "▸ " } else { "  " };
            let sty = if is_active && active {
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            let badge = if *unread > 0 {
                format!(" [{}]", unread)
            } else if *total > 0 {
                format!("  {}", total)
            } else {
                String::new()
            };
            let badge_sty = if *unread > 0 { Style::default().fg(UNREAD).add_modifier(Modifier::BOLD) }
                            else { Style::default().fg(DIM) };
            let line = Line::from(vec![
                Span::styled(format!("{}{}", prefix, tab.label()), sty),
                Span::styled(badge, badge_sty),
            ]);
            frame.render_widget(Paragraph::new(vec![line]), Rect { x: inner.x, y, width: inner.width, height: 1 });
            y += 1;
        }

        // IMAP status
        if !self.account.has_imap() && self.account.is_configured() && inner.y + 3 < inner.y + inner.height {
            let note_y = inner.y + 3;
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("  IMAP off", Style::default().fg(DIM)))),
                Rect { x: inner.x, y: note_y, width: inner.width, height: 1 },
            );
        }
    }

    fn render_message_list(&self, frame: &mut Frame, area: Rect) {
        let active = self.pane == ActivePane::MessageList;
        let border_color = if active { PRIMARY } else { BORDER };

        let folder_name = self.folder.label();
        let unread = if self.folder == FolderTab::Inbox { self.inbox_unread_count() } else { 0 };
        let count = self.current_message_count();

        let title = if unread > 0 {
            format!(" {} — {} unread ", folder_name, unread)
        } else {
            format!(" {} — {} messages ", folder_name, count)
        };

        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_style(Style::default().fg(if active { PRIMARY } else { MUTED }));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_h = inner.height as usize;
        let sel = self.current_sel();
        let scroll = match self.folder { FolderTab::Inbox => self.inbox_scroll, FolderTab::Sent => self.sent_scroll };

        if count == 0 {
            let msg = match self.folder {
                FolderTab::Inbox => {
                    if self.inbox_loading          { "  Connecting to IMAP…" }
                    else if self.account.has_imap() { "  Empty inbox. Press F5 to sync." }
                    else                            { "  IMAP not configured. Press [a] to set up account." }
                }
                FolderTab::Sent  => "  No sent messages.",
            };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(DIM)),
                inner,
            );
            return;
        }

        let mut rows: Vec<Line> = Vec::new();

        match self.folder {
            FolderTab::Inbox => {
                for (i, msg) in self.inbox.iter().enumerate().skip(scroll).take(visible_h) {
                    let is_sel = i == sel;
                    let bg = if is_sel && active { SEL_BG } else { Color::Reset };
                    let prefix = if is_sel { "▸ " } else { "  " };
                    let dot = if !msg.is_read { "●" } else { " " };
                    let dot_sty = if !msg.is_read { Style::default().fg(UNREAD) } else { Style::default().fg(DIM) };

                    let fw = inner.width as usize;
                    let from_w = (fw / 4).min(28).max(12);
                    let subj_w = fw.saturating_sub(from_w + 12);
                    let from_disp = truncate_str(&short_from(&msg.from), from_w);
                    let subj_disp = truncate_str(&msg.subject, subj_w);
                    let date_disp = &msg.date_fmt;

                    let text_sty = if !msg.is_read {
                        Style::default().fg(TEXT).bg(bg).add_modifier(Modifier::BOLD)
                    } else if is_sel {
                        Style::default().fg(TEXT).bg(bg)
                    } else {
                        Style::default().fg(MUTED)
                    };

                    rows.push(Line::from(vec![
                        Span::styled(prefix, text_sty),
                        Span::styled(dot, dot_sty.bg(bg)),
                        Span::styled(format!(" {:<w$} ", from_disp, w = from_w), text_sty),
                        Span::styled(format!("{:<w$}", subj_disp, w = subj_w.min(fw.saturating_sub(from_w + 14))), text_sty),
                        Span::styled(format!(" {:>8}", date_disp), Style::default().fg(DIM).bg(bg)),
                    ]));
                }
            }
            FolderTab::Sent => {
                for (i, msg) in self.sent.iter().enumerate().skip(scroll).take(visible_h) {
                    let is_sel = i == sel;
                    let bg = if is_sel && active { SEL_BG } else { Color::Reset };
                    let prefix = if is_sel { "▸ " } else { "  " };
                    let fw = inner.width as usize;
                    let to_w   = (fw / 4).min(28).max(12);
                    let subj_w = fw.saturating_sub(to_w + 12);
                    let to_disp   = truncate_str(&msg.to, to_w);
                    let subj_disp = truncate_str(&msg.subject, subj_w);
                    let date_disp = msg.sent_at.get(..10).unwrap_or("");

                    let sty = if is_sel { Style::default().fg(TEXT).bg(bg) } else { Style::default().fg(MUTED) };
                    rows.push(Line::from(vec![
                        Span::styled(prefix, sty),
                        Span::styled(format!(" ➜ {:<w$} ", to_disp, w = to_w), sty),
                        Span::styled(format!("{:<w$}", subj_disp, w = subj_w.min(fw.saturating_sub(to_w + 16))), sty),
                        Span::styled(format!(" {:>10}", date_disp), Style::default().fg(DIM).bg(bg)),
                    ]));
                }
            }
        }

        frame.render_widget(Paragraph::new(rows), inner);
    }

    fn render_preview(&self, frame: &mut Frame, area: Rect) {
        let active = self.pane == ActivePane::Preview;
        let border_color = if active { PRIMARY } else { BORDER };

        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" Preview ")
            .title_style(Style::default().fg(if active { PRIMARY } else { MUTED }));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();

        match self.folder {
            FolderTab::Inbox => {
                if let Some(msg) = self.inbox.get(self.inbox_sel) {
                    lines.push(Line::from(vec![
                        Span::styled("  From:    ", Style::default().fg(MUTED)),
                        Span::styled(msg.from.clone(), Style::default().fg(if !msg.is_read { UNREAD } else { TEXT }).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Subject: ", Style::default().fg(MUTED)),
                        Span::styled(msg.subject.clone(), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Date:    ", Style::default().fg(MUTED)),
                        Span::styled(msg.date_raw.clone(), Style::default().fg(DIM)),
                    ]));
                    lines.push(Line::from(Span::styled(
                        "  ─────────────────────────────────────────────────────────────────────",
                        Style::default().fg(BORDER),
                    )));
                    lines.push(Line::from(""));

                    if let Some(body) = &msg.body {
                        for line in body.lines() {
                            let sty = if line.starts_with('>') { Style::default().fg(DIM) } else { Style::default().fg(TEXT) };
                            lines.push(Line::from(vec![Span::raw("  "), Span::styled(line.to_string(), sty)]));
                        }
                    } else if !msg.preview.is_empty() {
                        for line in msg.preview.lines() {
                            lines.push(Line::from(vec![Span::raw("  "), Span::styled(line.to_string(), Style::default().fg(TEXT))]));
                        }
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled("  [Press Enter to load full message]", Style::default().fg(DIM))));
                    } else {
                        lines.push(Line::from(Span::styled("  [Press Enter to load message]", Style::default().fg(DIM))));
                    }
                } else {
                    lines.push(Line::from(Span::styled("  Select a message to preview.", Style::default().fg(DIM))));
                }
            }
            FolderTab::Sent => {
                if let Some(msg) = self.sent.get(self.sent_sel) {
                    lines.push(Line::from(vec![
                        Span::styled("  To:      ", Style::default().fg(MUTED)),
                        Span::styled(msg.to.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Subject: ", Style::default().fg(MUTED)),
                        Span::styled(msg.subject.clone(), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  Sent:    ", Style::default().fg(MUTED)),
                        Span::styled(msg.sent_at.clone(), Style::default().fg(DIM)),
                    ]));
                    lines.push(Line::from(Span::styled(
                        "  ─────────────────────────────────────────────────────────────────────",
                        Style::default().fg(BORDER),
                    )));
                    lines.push(Line::from(""));
                    for line in msg.body.lines() {
                        lines.push(Line::from(vec![Span::raw("  "), Span::styled(line.to_string(), Style::default().fg(TEXT))]));
                    }
                } else {
                    lines.push(Line::from(Span::styled("  Select a message to preview.", Style::default().fg(DIM))));
                }
            }
        }

        let vis = inner.height as usize;
        let max_scroll = lines.len().saturating_sub(vis);
        let scroll = self.preview_scroll.min(max_scroll);
        let visible: Vec<Line> = lines.into_iter().skip(scroll).take(vis).collect();
        frame.render_widget(Paragraph::new(visible), inner);
    }

    fn render_statusbar(&self, frame: &mut Frame, area: Rect) {
        let help = match self.pane {
            ActivePane::Folders => "  [Tab] → messages  [1] Inbox  [2] Sent  [n] compose  [a] account  [Esc] exit",
            ActivePane::MessageList => "  [Tab] → preview  [j/k] navigate  [Enter] open  [r] reply  [d] delete  [F5] sync",
            ActivePane::Preview => "  [Tab] → folders  [j/k] scroll  [r] reply  [d] delete  [n] compose  [Esc] exit",
        };
        frame.render_widget(
            Paragraph::new(help).style(Style::default().fg(DIM)),
            area,
        );
    }

    fn render_compose(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(" Compose New Email ")
            .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::vertical([
            Constraint::Length(1), Constraint::Length(1), Constraint::Length(1),
            Constraint::Length(1), Constraint::Length(1), Constraint::Min(5), Constraint::Length(1),
        ]).split(inner);

        // From info
        let from_str = if self.account.is_configured() {
            format!("  From: {} <{}>  ({})", self.account.display_name, self.account.email, self.account.provider_str())
        } else { "  ⚠ No account. Press Esc → [a]".to_string() };
        frame.render_widget(Paragraph::new(from_str).style(Style::default().fg(MUTED)), chunks[0]);

        // Separator
        frame.render_widget(Paragraph::new(Span::styled("  ─────────────────────────────────────────────────────────────────", Style::default().fg(BORDER))), chunks[1]);

        let field_row = |label: &str, value: &str, active: bool, _cursor: usize| {
            let sty = if active { Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
            let val_sty = if active { Style::default().fg(TEXT) } else { Style::default().fg(DIM) };
            Line::from(vec![
                Span::styled(format!("  {}: ", label), sty),
                Span::styled(value.to_string(), val_sty),
                if active { Span::styled("│", Style::default().fg(PRIMARY)) } else { Span::raw("") },
            ])
        };

        let to_active  = self.compose_field == ComposeField::To;
        let sub_active = self.compose_field == ComposeField::Subject;
        frame.render_widget(Paragraph::new(vec![field_row("To     ", &self.compose_to, to_active, self.compose_cursor)]), chunks[2]);
        frame.render_widget(Paragraph::new(vec![field_row("Subject", &self.compose_subject, sub_active, self.compose_cursor)]), chunks[3]);
        frame.render_widget(Paragraph::new(Span::styled("  ─────────────────────────────────────────────────────────────────", Style::default().fg(BORDER))), chunks[4]);

        // Body
        {
            let active = self.compose_field == ComposeField::Body;
            let sty = if active { Style::default().fg(TEXT) } else { Style::default().fg(DIM) };
            let vh = chunks[5].height as usize;
            let lines: Vec<&str> = self.compose_body.split('\n').collect();
            let scroll = if active {
                let before = &self.compose_body[..self.compose_cursor.min(self.compose_body.len())];
                before.chars().filter(|&c| c == '\n').count().saturating_sub(vh.saturating_sub(2))
            } else { self.compose_scroll };
            let scroll = scroll.min(lines.len().saturating_sub(vh));
            let dlines: Vec<Line> = lines.iter().skip(scroll).take(vh)
                .map(|l| Line::from(vec![Span::raw("  "), Span::styled(l.to_string(), sty)]))
                .collect();
            frame.render_widget(Paragraph::new(dlines), chunks[5]);

            if active {
                let before = &self.compose_body[..self.compose_cursor.min(self.compose_body.len())];
                let cl = before.chars().filter(|&c| c == '\n').count();
                let ln = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let cy = chunks[5].y + cl.saturating_sub(scroll) as u16;
                let cx = chunks[5].x + 2 + (self.compose_cursor - ln) as u16;
                if cy < chunks[5].y + chunks[5].height { frame.set_cursor_position((cx, cy)); }
            }
        }

        // Cursor for To/Subject
        if to_active {
            let cx = inner.x + 10 + (self.compose_cursor as u16).min(inner.width.saturating_sub(12));
            frame.set_cursor_position((cx, chunks[2].y));
        } else if sub_active {
            let cx = inner.x + 12 + (self.compose_cursor as u16).min(inner.width.saturating_sub(14));
            frame.set_cursor_position((cx, chunks[3].y));
        }

        frame.render_widget(
            Paragraph::new("  [Tab] next field  [Ctrl+S] send  [Esc/Ctrl+C] cancel")
                .style(Style::default().fg(DIM)),
            chunks[6],
        );
    }

    fn render_setup(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(" Add / Edit Email Account ")
            .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let custom   = self.setup_needs_custom();
        let provider = detect_provider(&self.setup_email);
        let has_at   = self.setup_email.contains('@');

        let mut row = inner.y + 1;
        let lw = inner.width;
        let lx = inner.x;

        // ── Account Type ──────────────────────────────────────────────────────
        {
            let active = self.setup_field == SetupField::AccountType;
            let ls = if active { Style::default().fg(TEXT).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
            let ps = if self.setup_type == "Personal" { Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD) } else { Style::default().fg(DIM) };
            let bs = if self.setup_type == "Business" { Style::default().fg(ORANGE).add_modifier(Modifier::BOLD) } else { Style::default().fg(DIM) };
            let hint = if active { Span::styled("  ← Space/←→", Style::default().fg(DIM)) } else { Span::raw("") };
            let l = Line::from(vec![Span::styled("  Account Type: ", ls), Span::styled("[ Personal ]", ps), Span::raw("   "), Span::styled("[ Business ]", bs), hint]);
            frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 2;
        }

        // ── Text fields ───────────────────────────────────────────────────────
        let text_defs: &[(&str, &str, SetupField, bool)] = &[
            ("Your Name    ", &self.setup_name,     SetupField::Name,     false),
            ("Email Address", &self.setup_email,    SetupField::Email,    false),
            ("Password     ", &self.setup_password, SetupField::Password, true),
        ];

        for (label, val, field, masked) in text_defs {
            if row >= lx + lw { break; }
            let active = &self.setup_field == field;
            let display: String = if *masked { "•".repeat(val.len()) } else { val.to_string() };
            let max_w = (lw as usize).saturating_sub(22).max(10);
            let scroll = if self.setup_cursor > max_w { self.setup_cursor - max_w } else { 0 };
            let vis: String = display.chars().skip(scroll).take(max_w).collect();

            let ls = if active { Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
            let vs = if active { Style::default().fg(TEXT) } else { Style::default().fg(DIM) };
            let l = Line::from(vec![
                Span::styled(format!("  {}: ", label), ls),
                Span::styled(format!("[{}]", vis), vs),
            ]);
            frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 });

            if active {
                let cx = lx + 2 + label.len() as u16 + 4 + (self.setup_cursor.saturating_sub(scroll) as u16).min(max_w as u16);
                frame.set_cursor_position((cx, row));
            }
            row += 1;

            // Provider feedback under email
            if *field == SetupField::Email && has_at {
                if let Some(ref p) = provider {
                    let l = Line::from(vec![
                        Span::raw("                    "),
                        Span::styled(format!("✓ {} — auto-configured!", p.name), Style::default().fg(GREEN)),
                    ]);
                    frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 1;
                    if !p.note.is_empty() {
                        let l2 = Line::from(vec![Span::raw("                    "), Span::styled(format!("ℹ  {}", p.note), Style::default().fg(ORANGE))]);
                        frame.render_widget(Paragraph::new(vec![l2]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 1;
                    }
                } else if custom {
                    let (sg, ig) = guess_hosts(&self.setup_email);
                    let l = Line::from(vec![Span::raw("                    "), Span::styled(format!("Custom — fill SMTP/IMAP below (guessed: {})", sg), Style::default().fg(MUTED))]);
                    frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 1;
                    let _ = ig; // used below
                }
            }
            row += 1;
        }

        // ── Custom SMTP/IMAP ──────────────────────────────────────────────────
        if custom && row + 5 < inner.y + inner.height {
            let sep = Line::from(Span::styled("  ─── Server Settings (auto-guessed, edit if wrong) ───────────────────────", Style::default().fg(BORDER)));
            frame.render_widget(Paragraph::new(vec![sep]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 1;

            let custom_defs: &[(&str, &str, SetupField)] = &[
                ("SMTP Host", &self.setup_smtp, SetupField::CustomSmtp),
                ("IMAP Host", &self.setup_imap, SetupField::CustomImap),
                ("SMTP Port", &self.setup_port, SetupField::CustomPort),
            ];

            for (label, val, field) in custom_defs {
                if row >= inner.y + inner.height { break; }
                let active = &self.setup_field == field;
                let placeholder = if val.is_empty() {
                    match field {
                        SetupField::CustomSmtp => guess_hosts(&self.setup_email).0,
                        SetupField::CustomImap => guess_hosts(&self.setup_email).1,
                        _ => String::new(),
                    }
                } else { val.to_string() };
                let ls = if active { Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
                let vs = if active { Style::default().fg(TEXT) } else if val.is_empty() { Style::default().fg(DIM).add_modifier(Modifier::ITALIC) } else { Style::default().fg(DIM) };
                let l = Line::from(vec![
                    Span::styled(format!("  {}: ", label), ls),
                    Span::styled(format!("[{}]", placeholder), vs),
                ]);
                frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 });
                if active { frame.set_cursor_position((lx + label.len() as u16 + 6 + (self.setup_cursor as u16), row)); }
                row += 1;
            }
        }

        // ── Controls ──────────────────────────────────────────────────────────
        if row + 2 < inner.y + inner.height {
            row += 1;
            let sep = Line::from(Span::styled("  ─────────────────────────────────────────────────────────────────────────", Style::default().fg(BORDER)));
            frame.render_widget(Paragraph::new(vec![sep]), Rect { x: lx, y: row, width: lw, height: 1 }); row += 1;
            let l = Line::from(vec![
                Span::styled("  [Tab/↓] next  [Shift+Tab/↑] prev  ", Style::default().fg(DIM)),
                Span::styled("[Ctrl+S] Save Account", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled("  [Esc] cancel", Style::default().fg(DIM)),
            ]);
            frame.render_widget(Paragraph::new(vec![l]), Rect { x: lx, y: row, width: lw, height: 1 });
        }
    }
}

fn render_overlay(frame: &mut Frame, area: Rect, color: Color, title: &str, msg: &str) {
    let block = Block::default().borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let icon = if color == GREEN { "✓" } else if color == RED { "✗" } else { "⟳" };
    let footer = if color == CYAN { "\n\n  Please wait…" } else { "\n\n  [Enter/Esc] back" };
    frame.render_widget(
        Paragraph::new(format!("\n\n\n  {} {}{}", icon, msg, footer))
            .style(Style::default().fg(color)).alignment(Alignment::Center),
        inner,
    );
}

// ── Utility helpers ───────────────────────────────────────────────────────────

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>()) }
}

/// Extract "Name" or "addr" from a From header string.
fn short_from(from: &str) -> String {
    // "Name <addr>" → "Name"
    if let Some(end) = from.find(" <") {
        return from[..end].trim_matches('"').to_string();
    }
    // Just "addr@domain" → "addr@domain" truncated
    from.trim_matches('<').trim_matches('>').to_string()
}

fn extract_reply_to(from: &str) -> String {
    // Extract just the email address from "Name <addr>" or plain addr
    if let Some(start) = from.find('<') {
        if let Some(end) = from.find('>') {
            return from[start + 1..end].to_string();
        }
    }
    from.trim().to_string()
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:032x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos())
}

// ── IMAP blocking functions ───────────────────────────────────────────────────

fn fetch_imap_blocking(host: String, port: u16, user: String, pass: String) -> anyhow::Result<Vec<InboxEmail>> {
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(false)
        .build()
        .map_err(|e| anyhow::anyhow!("TLS: {}", e))?;

    let client = imap::connect((host.as_str(), port), host.as_str(), &tls)
        .map_err(|e| anyhow::anyhow!("Connect: {}", e))?;

    let mut session = client.login(user.as_str(), pass.as_str())
        .map_err(|(e, _)| anyhow::anyhow!("Login: {}", e))?;

    let mailbox = session.select("INBOX")
        .map_err(|e| anyhow::anyhow!("Select: {}", e))?;

    let total = mailbox.exists;
    if total == 0 {
        session.logout().ok();
        return Ok(Vec::new());
    }

    let start = if total > 50 { total - 49 } else { 1 };
    let seq   = format!("{}:{}", start, total);

    let messages = session.fetch(
        &seq,
        "(UID FLAGS BODY.PEEK[HEADER])",
    ).map_err(|e| anyhow::anyhow!("Fetch: {}", e))?;

    let mut emails: Vec<InboxEmail> = Vec::new();
    for msg in messages.iter() {
        let uid     = msg.uid.unwrap_or(0);
        let seq_num = msg.message;
        let is_read = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));

        let (from, subject, date_raw) = if let Some(hdr) = msg.header() {
            parse_imap_headers(hdr)
        } else {
            (String::new(), "(no subject)".to_string(), String::new())
        };

        let date_fmt = format_imap_date(&date_raw);

        emails.push(InboxEmail {
            uid, _seq: seq_num, from, subject, date_raw, date_fmt,
            is_read, body: None, preview: String::new(),
        });
    }

    emails.reverse(); // most recent first
    session.logout().ok();
    Ok(emails)
}

fn fetch_imap_body_blocking(host: String, port: u16, user: String, pass: String, uid: u32) -> anyhow::Result<String> {
    let tls = native_tls::TlsConnector::builder().build()
        .map_err(|e| anyhow::anyhow!("TLS: {}", e))?;
    let client = imap::connect((host.as_str(), port), host.as_str(), &tls)
        .map_err(|e| anyhow::anyhow!("Connect: {}", e))?;
    let mut session = client.login(user.as_str(), pass.as_str())
        .map_err(|(e, _)| anyhow::anyhow!("Login: {}", e))?;
    session.select("INBOX").map_err(|e| anyhow::anyhow!("Select: {}", e))?;

    let messages = session.uid_fetch(uid.to_string(), "RFC822")
        .map_err(|e| anyhow::anyhow!("Fetch body: {}", e))?;

    let body_text = if let Some(msg) = messages.iter().next() {
        if let Some(raw) = msg.body() {
            extract_text_from_email(raw)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Mark as seen
    session.uid_store(uid.to_string(), "+FLAGS (\\Seen)").ok();
    session.logout().ok();
    Ok(body_text)
}

fn imap_delete_blocking(host: String, port: u16, user: String, pass: String, uid: u32) -> anyhow::Result<()> {
    let tls = native_tls::TlsConnector::builder().build()?;
    let client = imap::connect((host.as_str(), port), host.as_str(), &tls)?;
    let mut session = client.login(user.as_str(), pass.as_str())
        .map_err(|(e, _)| anyhow::anyhow!("{}", e))?;
    session.select("INBOX")?;
    session.uid_store(uid.to_string(), "+FLAGS (\\Deleted)").ok();
    session.expunge().ok();
    session.logout().ok();
    Ok(())
}

// ── Email parsing helpers ─────────────────────────────────────────────────────

fn parse_imap_headers(raw: &[u8]) -> (String, String, String) {
    // Use mailparse for proper MIME-decoded headers
    if let Ok((headers, _)) = mailparse::parse_headers(raw) {
        let get = |name: &str| -> String {
            headers.iter()
                .find(|h| h.get_key_ref().eq_ignore_ascii_case(name))
                .map(|h| h.get_value())
                .unwrap_or_default()
        };
        let from    = get("from");
        let subject = get("subject");
        let date    = get("date");
        let subject = if subject.is_empty() { "(no subject)".to_string() } else { subject };
        (from, subject, date)
    } else {
        // Fallback: simple line-by-line parse
        let text = String::from_utf8_lossy(raw);
        let mut from = String::new(); let mut subject = String::new(); let mut date = String::new();
        for line in text.lines() {
            let l = line.to_lowercase();
            if l.starts_with("from:") && from.is_empty()    { from    = line[5..].trim().to_string(); }
            if l.starts_with("subject:") && subject.is_empty() { subject = line[8..].trim().to_string(); }
            if l.starts_with("date:") && date.is_empty()    { date    = line[5..].trim().to_string(); }
        }
        let subject = if subject.is_empty() { "(no subject)".to_string() } else { subject };
        (from, subject, date)
    }
}

fn extract_text_from_email(raw: &[u8]) -> String {
    match mailparse::parse_mail(raw) {
        Ok(parsed) => get_text_body(&parsed),
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    }
}

fn get_text_body(mail: &mailparse::ParsedMail) -> String {
    let mime = mail.ctype.mimetype.as_str();
    if mime == "text/plain" {
        return mail.get_body().unwrap_or_default();
    }
    if mime == "text/html" {
        return html_to_plain(&mail.get_body().unwrap_or_default());
    }
    // multipart: prefer text/plain
    for part in &mail.subparts {
        if part.ctype.mimetype == "text/plain" { return part.get_body().unwrap_or_default(); }
    }
    for part in &mail.subparts {
        let t = get_text_body(part);
        if !t.is_empty() { return t; }
    }
    String::new()
}

fn html_to_plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut skip_content = false;
    let mut tag_buf = String::new();

    for ch in html.chars() {
        match ch {
            '<' => { in_tag = true; tag_buf.clear(); }
            '>' => {
                let tag = tag_buf.to_lowercase();
                let tag = tag.trim_start_matches('/').trim();
                skip_content = matches!(tag.split_whitespace().next().unwrap_or(""), "script" | "style" | "head");
                if matches!(tag, "br" | "p" | "div" | "h1" | "h2" | "h3" | "li") {
                    out.push('\n');
                }
                in_tag = false;
            }
            _ if in_tag => { tag_buf.push(ch); }
            _ if !skip_content => { out.push(ch); }
            _ => {}
        }
    }
    // Clean up HTML entities
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

fn format_imap_date(date_raw: &str) -> String {
    // Try to extract just date/time part — e.g. "Mon, 20 Feb 2026 14:30:00 +0000" → "20 Feb 14:30"
    let parts: Vec<&str> = date_raw.split_whitespace().collect();
    if parts.len() >= 5 {
        // parts[0]=Mon, [1]=20 [2]=Feb [3]=2026 [4]=14:30:00
        let day   = parts[1];
        let month = parts[2];
        let time  = parts[4].get(..5).unwrap_or(parts[4]);
        return format!("{} {} {}", day, month, time);
    }
    if date_raw.len() > 16 { date_raw[..16].to_string() } else { date_raw.to_string() }
}

// ── SMTP send via lettre ──────────────────────────────────────────────────────

use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

fn build_lettre_message(from: &str, to: &str, subject: &str, body: &str) -> anyhow::Result<Message> {
    Message::builder()
        .from(from.parse().map_err(|e| anyhow::anyhow!("Invalid from: {}", e))?)
        .to(to.parse().map_err(|e| anyhow::anyhow!("Invalid to: {}", e))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| anyhow::anyhow!("{}", e))
}

async fn send_via_smtp(
    email: Message, host: &str, port: u16,
    username: &str, password: &str, starttls: bool,
) -> anyhow::Result<()> {
    let creds = Credentials::new(username.to_string(), password.to_string());
    let transport: AsyncSmtpTransport<Tokio1Executor> = if starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| anyhow::anyhow!("{}", e))?.port(port).credentials(creds).build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|e| anyhow::anyhow!("{}", e))?.port(port).credentials(creds).build()
    };
    transport.send(email).await.map_err(|e| anyhow::anyhow!("SMTP: {}", e))?;
    Ok(())
}
