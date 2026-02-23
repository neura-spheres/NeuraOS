use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::BTreeMap;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Color, Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette;
use neura_storage::vfs::Vfs;

/// Live color palette fed by main.rs whenever the user switches themes.
/// Defaults to Tokyo Night so the app looks right on first launch even
/// before the first hot-reload fires.
#[derive(Clone)]
pub struct AppTheme {
    pub border:       Color,
    pub accent:       Color,
    pub fg:           Color,
    pub muted:        Color,
    pub warning:      Color,
    pub success:      Color,
    pub error:        Color,
    pub statusbar_fg: Color,
}

impl Default for AppTheme {
    fn default() -> Self {
        Self {
            border:       palette::BORDER,
            accent:       palette::PRIMARY,
            fg:           palette::TEXT,
            muted:        palette::STATUSBAR_MUTED,
            warning:      palette::ORANGE,
            success:      palette::GREEN,
            error:        palette::RED,
            statusbar_fg: palette::MUTED,
        }
    }
}

/// All user preferences stored as a flat key-value map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub values: BTreeMap<String, String>,
}

impl Default for Preferences {
    fn default() -> Self {
        let mut values = BTreeMap::new();

        // ── AI Settings ──
        values.insert("ai.provider".into(), "gemini".into());
        values.insert("ai.model".into(), "gemini-2.5-flash-lite".into());
        values.insert("ai.api_key".into(), String::new());
        values.insert("ai.max_tokens".into(), "8192".into());
        values.insert("ai.temperature".into(), "0.7".into());
        values.insert("ai.rate_limit_rpm".into(), "60".into());
        values.insert("ai.streaming".into(), "true".into());
        values.insert("ai.auto_suggest".into(), "false".into());
        values.insert("ai.base_url".into(), String::new());
        values.insert("ai.system_prompt".into(), String::new());
        values.insert("ai.response_format".into(), "plain".into());
        values.insert("ai.reset_memory".into(), "false".into());

        // ── Appearance ──
        values.insert("ui.theme".into(), "tokyo_night".into());
        values.insert("ui.show_statusbar".into(), "true".into());
        values.insert("ui.show_clock".into(), "true".into());
        values.insert("ui.show_notifications".into(), "true".into());
        values.insert("ui.border_style".into(), "rounded".into());
        values.insert("ui.color_scheme".into(), "dark".into());
        values.insert("ui.transparency".into(), "false".into());
        values.insert("ui.font_size".into(), "default".into());
        values.insert("ui.accent_color".into(), "#7aa2f7".into());
        values.insert("ui.dim_inactive".into(), "false".into());

        // ── Shell Settings ──
        values.insert("shell.prompt_style".into(), "default".into());
        values.insert("shell.history_size".into(), "1000".into());
        values.insert("shell.auto_complete".into(), "true".into());
        values.insert("shell.syntax_highlight".into(), "true".into());
        values.insert("shell.bell".into(), "false".into());
        values.insert("shell.vi_mode".into(), "false".into());
        values.insert("shell.default_editor".into(), "neura-dev".into());
        values.insert("shell.greeting".into(), "true".into());

        // ── Desktop ──
        values.insert("desktop.default_workspace".into(), "main".into());
        values.insert("desktop.workspace_count".into(), "4".into());
        values.insert("desktop.session_restore".into(), "true".into());
        values.insert("desktop.auto_save_interval".into(), "30".into());
        values.insert("desktop.startup_app".into(), "none".into());
        values.insert("desktop.window_animation".into(), "false".into());
        values.insert("desktop.clock_24h".into(), "true".into());
        values.insert("desktop.show_seconds".into(), "true".into());
        values.insert("desktop.timezone".into(), "UTC|0".into());

        // ── Storage ──
        values.insert("storage.max_cache_mb".into(), "512".into());
        values.insert("storage.auto_backup".into(), "true".into());
        values.insert("storage.backup_interval_hours".into(), "24".into());
        values.insert("storage.vfs_auto_save".into(), "true".into());
        values.insert("storage.journal_max_entries".into(), "10000".into());
        values.insert("storage.compress_backups".into(), "false".into());

        // ── Security ──
        values.insert("security.session_timeout_hours".into(), "24".into());
        values.insert("security.require_password_on_wake".into(), "false".into());
        values.insert("security.sandbox_apps".into(), "true".into());
        values.insert("security.allow_network_apps".into(), "true".into());
        values.insert("security.signed_packages_only".into(), "false".into());

        // ── Notifications ──
        values.insert("notifications.enabled".into(), "true".into());
        values.insert("notifications.sound".into(), "false".into());
        values.insert("notifications.position".into(), "top_right".into());
        values.insert("notifications.duration_secs".into(), "5".into());

        // ── Accessibility ──
        values.insert("accessibility.high_contrast".into(), "false".into());
        values.insert("accessibility.screen_reader".into(), "false".into());
        values.insert("accessibility.large_text".into(), "false".into());
        values.insert("accessibility.reduce_motion".into(), "false".into());
        values.insert("accessibility.cursor_blink".into(), "true".into());

        // ── Network ──
        values.insert("network.proxy".into(), String::new());
        values.insert("network.timeout_secs".into(), "30".into());
        values.insert("network.auto_sync".into(), "false".into());
        values.insert("network.sync_interval_mins".into(), "15".into());

        Self { values }
    }
}

impl Preferences {
    fn settings_path(username: &str) -> String {
        format!("/home/{}/settings.json", username)
    }

    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }

    pub fn delete(&mut self, key: &str) {
        self.values.insert(key.to_string(), String::new());
    }

    pub fn categories(&self) -> Vec<&str> {
        let mut cats: Vec<&str> = self.values.keys()
            .filter_map(|k| k.split('.').next())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        cats.sort();
        cats
    }

    pub fn keys_for_category(&self, category: &str) -> Vec<(&str, &str)> {
        self.values.iter()
            .filter(|(k, _)| k.starts_with(&format!("{}.", category)))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SettingsMode {
    CategoryList,
    SettingsList,
    Editing,
    Selecting,
    /// Step 1 — choose between Data Only vs Data + Settings
    ResetChoose,
    /// Step 2 — type "RESET" to confirm
    ResetConfirm,
}

/// Which reset scope the user chose.
#[derive(Debug, Clone, PartialEq)]
pub enum ResetOption {
    /// Wipe all user data files but keep settings.json
    DataOnly,
    /// Wipe everything including settings
    DataAndSettings,
}

pub struct SettingsApp {
    vfs: Arc<Vfs>,
    username: String,
    prefs: Preferences,
    categories: Vec<String>,
    selected_cat: usize,
    current_keys: Vec<(String, String)>,
    selected_setting: usize,
    settings_scroll: AtomicUsize,
    selector_scroll: AtomicUsize,
    mode: SettingsMode,
    edit_buffer: String,
    edit_key: String,
    select_options: Vec<String>,
    select_index: usize,
    initialized: bool,
    status_msg: String,
    /// Set to true every time a setting is saved. main.rs polls this each
    /// frame and applies changes immediately without waiting for app close.
    pub has_pending_changes: bool,
    /// Live theme colors — updated by main.rs on every hot-reload so the
    /// settings UI reflects the chosen theme in real time.
    pub app_theme: AppTheme,
    // ── Reset Account ─────────────────────────────────────────────────────────
    reset_option_sel: usize,       // 0 = Data Only, 1 = Data + Settings
    reset_option: ResetOption,     // chosen option carried into step 2
    reset_confirm_buf: String,     // user types "RESET" here
    /// Set by the 2-step reset flow; main.rs reads this and performs the wipe.
    pub reset_requested: Option<ResetOption>,
}

impl SettingsApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        let prefs = Preferences::default();
        let categories: Vec<String> = prefs.categories().into_iter().map(String::from).collect();
        Self {
            vfs,
            username: username.to_string(),
            prefs,
            categories,
            selected_cat: 0,
            current_keys: Vec::new(),
            selected_setting: 0,
            settings_scroll: AtomicUsize::new(0),
            selector_scroll: AtomicUsize::new(0),
            mode: SettingsMode::CategoryList,
            edit_buffer: String::new(),
            edit_key: String::new(),
            select_options: Vec::new(),
            select_index: 0,
            initialized: false,
            status_msg: String::new(),
            has_pending_changes: false,
            app_theme: AppTheme::default(),
            reset_option_sel: 0,
            reset_option: ResetOption::DataOnly,
            reset_confirm_buf: String::new(),
            reset_requested: None,
        }
    }

    fn refresh_keys(&mut self) {
        if let Some(cat) = self.categories.get(self.selected_cat) {
            let provider = self.prefs.get("ai.provider");
            self.current_keys = self.prefs.keys_for_category(cat)
                .into_iter()
                .filter(|(k, _)| {
                    // Hide API Key setting if provider is Ollama
                    if k == &"ai.api_key" && provider == "ollama" {
                        return false;
                    }
                    true
                })
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        }
    }

    /// Set a preference key/value from outside the settings app (e.g. from main.rs).
    /// Persists the change and triggers hot-reload.
    pub fn set_pref(&mut self, key: &str, value: &str) {
        self.prefs.set(key, value);
        self.save_prefs();
    }

    fn save_prefs(&mut self) {
        let vfs = self.vfs.clone();
        let path = Preferences::settings_path(&self.username);
        let prefs = self.prefs.clone();
        let username = self.username.clone();
        tokio::spawn(async move {
            match serde_json::to_vec_pretty(&prefs) {
                Ok(data) => {
                    if let Err(e) = vfs.write_file(&path, data, &username).await {
                        tracing::warn!("Failed to save settings: {}", e);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to serialize settings: {}", e);
                }
            }
        });
        self.status_msg = "Settings saved.".to_string();
        // Signal main.rs to hot-reload — fires immediately on every change.
        self.has_pending_changes = true;
    }

    /// Called after a setting value changes to apply side-effects.
    fn on_setting_changed(&mut self, key: &str, value: &str) {
        if key == "ai.provider" {
            let (model, base_url) = match value {
                "gemini" => ("gemini-2.5-flash-lite", ""),
                "openai" => ("gpt-4o-mini", ""),
                "deepseek" => ("deepseek-chat", ""),
                "ollama" => ("llama3.2", "http://localhost:11434/v1"),
                _ => return,
            };
            self.prefs.set("ai.model", model);
            self.prefs.set("ai.base_url", base_url);
        }

        if key == "ai.reset_memory" && value == "true" {
            let vfs = self.vfs.clone();
            let username = self.username.clone();
            tokio::spawn(async move {
                let path = format!("/home/{}/memory.json", username);
                let _ = vfs.remove(&path).await;
            });
            // Reset the toggle back to false immediately
            self.prefs.set("ai.reset_memory", "false");
            self.status_msg = "Memory cleared.".to_string();
            self.has_pending_changes = true;
        }
    }

    fn is_boolean_setting(key: &str) -> bool {
        matches!(key,
            "ai.streaming" | "ai.auto_suggest" | "ai.reset_memory" |
            "ui.show_statusbar" | "ui.show_clock" | "ui.show_notifications" |
            "ui.transparency" | "ui.dim_inactive" |
            "shell.auto_complete" | "shell.syntax_highlight" | "shell.bell" |
            "shell.vi_mode" | "shell.greeting" |
            "desktop.session_restore" | "desktop.window_animation" |
            "desktop.clock_24h" | "desktop.show_seconds" |
            "storage.auto_backup" | "storage.vfs_auto_save" | "storage.compress_backups" |
            "security.require_password_on_wake" | "security.sandbox_apps" |
            "security.allow_network_apps" | "security.signed_packages_only" |
            "notifications.enabled" | "notifications.sound" |
            "accessibility.high_contrast" | "accessibility.screen_reader" |
            "accessibility.large_text" | "accessibility.reduce_motion" |
            "accessibility.cursor_blink" |
            "network.auto_sync"
        )
    }

    fn known_options(&self, key: &str) -> Option<Vec<&'static str>> {
        match key {
            "ai.provider" => Some(vec!["gemini", "openai", "deepseek", "ollama", "custom"]),
            "ai.model" => {
                match self.prefs.get("ai.provider") {
                    "gemini" => Some(vec!["gemini-2.5-flash-lite", "gemini-2.0-flash", "gemini-2.5-pro"]),
                    "openai" => Some(vec!["gpt-4o-mini", "gpt-4o", "gpt-4-turbo", "o3-mini"]),
                    "deepseek" => Some(vec!["deepseek-chat", "deepseek-reasoner"]),
                    "ollama" => Some(vec![
                        // Cloud Models
                        "kimi-k2.5:cloud", "glm-5:cloud", "ministral-3:14b-cloud",
                        "devstral-2:123b-cloud", "glm-4.7:cloud",
                        
                        // Local Models
                        "llama3.2:1b", "llama3.2:3b", "llama3.1:8b", 
                        "mistral:7b", "gemma2:2b", "gemma2:9b", 
                        "gemma3:1b", "gemma3:4b",
                        "ministral-3:14b", "ministral-3:8b", "ministral-3:3b",
                        "qwen2.5:0.5b", "qwen2.5:1.5b", "qwen2.5:7b", "qwen2.5-coder:7b",
                        "phi3.5:3.8b", "deepseek-coder:6.7b", "deepseek-r1:1.5b", "deepseek-r1:7b",
                        "neural-chat:7b", "starling-lm:7b", "orca-mini:3b", "vicuna:7b", "tinyllama:1.1b"
                    ]),
                    _ => None,
                }
            }
            "ai.response_format" => Some(vec!["plain", "markdown"]),
            "ui.theme" => Some(vec!["tokyo_night", "catppuccin", "dracula", "gruvbox", "nord", "solarized", "monokai"]),
            "ui.border_style" => Some(vec!["rounded", "plain", "double", "thick"]),
            "ui.color_scheme" => Some(vec!["dark", "light"]),
            "ui.font_size" => Some(vec!["small", "default", "large"]),
            "shell.prompt_style" => Some(vec!["default", "minimal", "powerline"]),
            "shell.default_editor" => Some(vec!["neura-dev", "vi", "nano"]),
            "notifications.position" => Some(vec!["top_right", "top_left", "bottom_right", "bottom_left"]),
            "desktop.startup_app" => Some(vec!["none", "notes", "tasks", "files", "settings", "calc", "clock", "monitor", "calendar"]),
            "desktop.timezone" => Some(vec![
                "UTC|0",
                "London (GMT/BST)|0",
                "Paris (CET)|60",
                "Berlin (CET)|60",
                "Cairo (EET)|120",
                "Moscow (MSK)|180",
                "Dubai (GST)|240",
                "Karachi (PKT)|300",
                "India (IST)|330",
                "Dhaka (BST)|360",
                "Bangkok (ICT)|420",
                "Jakarta (WIB)|420",
                "Bali / Makassar (WITA)|480",
                "Singapore (SGT)|480",
                "Beijing (CST)|480",
                "Papua / Ambon (WIT)|540",
                "Tokyo (JST)|540",
                "Seoul (KST)|540",
                "Sydney (AEST)|600",
                "Auckland (NZST)|720",
                "São Paulo (BRT)|-180",
                "New York (EST)|-300",
                "Chicago (CST)|-360",
                "Denver (MST)|-420",
                "Los Angeles (PST)|-480",
                "Anchorage (AKST)|-540",
                "Honolulu (HST)|-600",
            ]),
            _ => None,
        }
    }

    fn display_key(key: &str) -> String {
        key.split('.')
            .last()
            .unwrap_or(key)
            .replace('_', " ")
            .split(' ')
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.collect::<String>()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn display_value(key: &str, value: &str) -> String {
        if key == "desktop.timezone" {
            // Strip the "|offset" suffix — show only the label part
            return value.split('|').next().unwrap_or(value).to_string();
        }
        if Self::is_boolean_setting(key) {
            if value == "true" { "[ON]".to_string() } else { "[OFF]".to_string() }
        } else if key.contains("api_key") || key.contains("password") {
            if value.is_empty() {
                "(not set)".to_string()
            } else {
                let len = value.len();
                if len > 8 {
                    format!("{}...{}", &value[..4], &value[len-4..])
                } else {
                    "****".to_string()
                }
            }
        } else if value.is_empty() {
            "(not set)".to_string()
        } else {
            value.to_string()
        }
    }

    fn category_icon(cat: &str) -> &str {
        match cat {
            "ai" => "[AI]",
            "ui" => "[UI]",
            "shell" => "[SH]",
            "desktop" => "[DK]",
            "storage" => "[ST]",
            "security" => "[SC]",
            "notifications" => "[NT]",
            "accessibility" => "[AC]",
            "network" => "[NW]",
            _ => "[--]",
        }
    }

    fn category_description(cat: &str) -> &str {
        match cat {
            "ai" => "AI model, API key, generation settings",
            "ui" => "Theme, colors, display preferences",
            "shell" => "Shell behavior, history, completion",
            "desktop" => "Workspaces, session, startup",
            "storage" => "Cache, backups, VFS settings",
            "security" => "Sessions, sandboxing, packages",
            "notifications" => "Alerts, sounds, positioning",
            "accessibility" => "Contrast, text size, motion",
            "network" => "Proxy, timeouts, sync",
            _ => "",
        }
    }
}

impl App for SettingsApp {
    fn id(&self) -> &str { "settings" }
    fn name(&self) -> &str { "NeuraSettings" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.initialized = true;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.status_msg.clear();

        match self.mode {
            SettingsMode::CategoryList => {
                // Total items = real categories + 1 "Reset Account" entry at the bottom
                let total = self.categories.len() + 1;
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected_cat > 0 { self.selected_cat -= 1; }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected_cat + 1 < total { self.selected_cat += 1; }
                        true
                    }
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                        if self.selected_cat == self.categories.len() {
                            // "Reset Account" entry selected
                            self.reset_option_sel = 0;
                            self.reset_confirm_buf.clear();
                            self.mode = SettingsMode::ResetChoose;
                        } else {
                            self.refresh_keys();
                            self.selected_setting = 0;
                            self.settings_scroll.store(0, Ordering::Relaxed);
                            self.mode = SettingsMode::SettingsList;
                        }
                        true
                    }
                    KeyCode::Esc => false,
                    _ => true,
                }
            }
            SettingsMode::SettingsList => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected_setting > 0 {
                            self.selected_setting -= 1;
                        }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected_setting + 1 < self.current_keys.len() {
                            self.selected_setting += 1;
                        }
                        true
                    }
                    KeyCode::Enter | KeyCode::Char('e') => {
                        if let Some((key, val)) = self.current_keys.get(self.selected_setting).cloned() {
                            if Self::is_boolean_setting(&key) {
                                // Toggle boolean directly
                                let new_val = if val == "true" { "false" } else { "true" };
                                self.prefs.set(&key, new_val);
                                self.on_setting_changed(&key, new_val);
                                self.save_prefs();
                                self.refresh_keys();
                                self.status_msg = format!("{}: {}", Self::display_key(&key),
                                    if new_val == "true" { "ON" } else { "OFF" });
                            } else if let Some(options) = self.known_options(&key) {
                                // Enter selection mode
                                self.select_options = options.iter().map(|s| s.to_string()).collect();
                                self.select_index = self.select_options.iter()
                                    .position(|o| o == &val)
                                    .unwrap_or(0);
                                self.edit_key = key;
                                // Reset selector scroll
                                self.selector_scroll.store(0, Ordering::Relaxed);
                                self.mode = SettingsMode::Selecting;
                            } else {
                                // Enter free-text editing
                                self.edit_key = key;
                                self.edit_buffer = val;
                                self.mode = SettingsMode::Editing;
                            }
                        }
                        true
                    }
                    KeyCode::Char('d') => {
                        if let Some((key, _)) = self.current_keys.get(self.selected_setting) {
                            let key = key.clone();
                            self.prefs.delete(&key);
                            self.save_prefs();
                            self.refresh_keys();
                            self.status_msg = format!("Cleared: {}", key);
                        }
                        true
                    }
                    KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
                        self.mode = SettingsMode::CategoryList;
                        true
                    }
                    _ => true,
                }
            }
            SettingsMode::Editing => {
                match key.code {
                    KeyCode::Enter => {
                        let edit_key = self.edit_key.clone();
                        let edit_buf = self.edit_buffer.clone();
                        self.prefs.set(&edit_key, &edit_buf);
                        self.on_setting_changed(&edit_key, &edit_buf);
                        self.save_prefs();
                        self.refresh_keys();
                        self.mode = SettingsMode::SettingsList;
                        self.status_msg = format!("Saved: {}", edit_key);
                        true
                    }
                    KeyCode::Esc => {
                        self.mode = SettingsMode::SettingsList;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.edit_buffer.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.edit_buffer.pop();
                        true
                    }
                    _ => true,
                }
            }
            SettingsMode::Selecting => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.select_index > 0 { self.select_index -= 1; }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.select_index + 1 < self.select_options.len() {
                            self.select_index += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        if let Some(value) = self.select_options.get(self.select_index).cloned() {
                            let edit_key = self.edit_key.clone();
                            self.prefs.set(&edit_key, &value);
                            self.on_setting_changed(&edit_key, &value);
                            self.save_prefs();
                            self.refresh_keys();
                            self.mode = SettingsMode::SettingsList;
                            self.status_msg = format!("Set {} = {}", Self::display_key(&edit_key), value);
                        }
                        true
                    }
                    KeyCode::Esc => {
                        self.mode = SettingsMode::SettingsList;
                        true
                    }
                    _ => true,
                }
            }

            // ── Step 1: choose reset scope ────────────────────────────────────
            SettingsMode::ResetChoose => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.reset_option_sel > 0 { self.reset_option_sel -= 1; }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.reset_option_sel < 1 { self.reset_option_sel += 1; }
                        true
                    }
                    KeyCode::Enter => {
                        self.reset_option = if self.reset_option_sel == 0 {
                            ResetOption::DataOnly
                        } else {
                            ResetOption::DataAndSettings
                        };
                        self.reset_confirm_buf.clear();
                        self.mode = SettingsMode::ResetConfirm;
                        true
                    }
                    KeyCode::Esc => {
                        self.mode = SettingsMode::CategoryList;
                        true
                    }
                    _ => true,
                }
            }

            // ── Step 2: type "RESET" to confirm ──────────────────────────────
            SettingsMode::ResetConfirm => {
                match key.code {
                    KeyCode::Enter => {
                        if self.reset_confirm_buf.trim() == "RESET" {
                            self.reset_requested = Some(self.reset_option.clone());
                            self.reset_confirm_buf.clear();
                            self.mode = SettingsMode::CategoryList;
                            self.status_msg = "Resetting account...".to_string();
                        } else {
                            self.status_msg = "Incorrect — type RESET exactly (all caps).".to_string();
                        }
                        true
                    }
                    KeyCode::Esc => {
                        self.reset_confirm_buf.clear();
                        self.mode = SettingsMode::ResetChoose;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.reset_confirm_buf.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.reset_confirm_buf.pop();
                        true
                    }
                    _ => true,
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(2)])
            .split(area);

        match self.mode {
            SettingsMode::CategoryList => {
                self.render_categories(frame, outer_chunks[0]);
            }
            SettingsMode::SettingsList => {
                self.render_settings_list(frame, outer_chunks[0]);
            }
            SettingsMode::Editing => {
                self.render_editor(frame, outer_chunks[0]);
            }
            SettingsMode::Selecting => {
                self.render_selector(frame, outer_chunks[0]);
            }
            SettingsMode::ResetChoose => {
                self.render_reset_choose(frame, outer_chunks[0]);
            }
            SettingsMode::ResetConfirm => {
                self.render_reset_confirm(frame, outer_chunks[0]);
            }
        }

        // Status / help bar
        let help_text = match self.mode {
            SettingsMode::CategoryList => {
                if self.status_msg.is_empty() {
                    " [Enter] open category  [Esc] exit".to_string()
                } else {
                    format!(" {} | [Enter] open  [Esc] exit", self.status_msg)
                }
            }
            SettingsMode::SettingsList => {
                if self.status_msg.is_empty() {
                    " [Enter] edit/toggle  [d] clear value  [Esc] back".to_string()
                } else {
                    format!(" {} | [Enter] edit/toggle  [d] clear  [Esc] back", self.status_msg)
                }
            }
            SettingsMode::Editing => {
                format!(" Editing: {} | [Enter] save  [Esc] cancel", self.edit_key)
            }
            SettingsMode::Selecting => {
                format!(" Select: {} | [Enter] confirm  [Esc] cancel", Self::display_key(&self.edit_key))
            }
            SettingsMode::ResetChoose => {
                " [↑↓] select option  [Enter] continue  [Esc] cancel".to_string()
            }
            SettingsMode::ResetConfirm => {
                if self.status_msg.is_empty() {
                    " Type RESET (all caps) then [Enter] to confirm  [Esc] back".to_string()
                } else {
                    format!(" {}  [Esc] back", self.status_msg)
                }
            }
        };
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(self.app_theme.statusbar_fg));
        frame.render_widget(help, outer_chunks[1]);
    }

    fn on_close(&mut self) {
        self.save_prefs();
    }

    fn save_state(&self) -> Option<Value> {
        serde_json::to_value(&self.prefs).ok()
    }

    fn load_state(&mut self, state: Value) {
        if let Ok(prefs) = serde_json::from_value::<Preferences>(state) {
            let defaults = Preferences::default();
            let mut merged = defaults;
            for (k, v) in prefs.values {
                merged.values.insert(k, v);
            }
            self.prefs = merged;
            self.categories = self.prefs.categories().into_iter().map(String::from).collect();
        }
    }

    fn ai_tools(&self) -> Vec<Value> {
        vec![serde_json::json!({
            "name": "set_preference",
            "description": "Set a NeuraOS preference",
            "parameters": {
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Preference key (e.g. ai.api_key, ui.theme)" },
                    "value": { "type": "string", "description": "Preference value" }
                },
                "required": ["key", "value"]
            }
        })]
    }

    fn handle_ai_tool(&mut self, tool_name: &str, args: Value) -> Option<Value> {
        match tool_name {
            "set_preference" => {
                let key = args.get("key")?.as_str()?.to_string();
                let value = args.get("value")?.as_str()?.to_string();
                self.prefs.set(&key, &value);
                self.on_setting_changed(&key, &value);
                self.save_prefs();
                Some(serde_json::json!({"status": "set", "key": key, "value": value}))
            }
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Render Methods ──

impl SettingsApp {
    fn render_categories(&self, frame: &mut Frame, area: Rect) {
        let reset_idx = self.categories.len(); // index of the Reset Account entry
        let mut items: Vec<ListItem> = self.categories.iter().enumerate().map(|(i, cat)| {
            let icon = Self::category_icon(cat);
            let desc = Self::category_description(cat);
            let style = if i == self.selected_cat {
                Style::default().fg(self.app_theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.app_theme.fg)
            };
            let prefix = if i == self.selected_cat { " > " } else { "   " };
            let cat_display: String = {
                let mut chars = cat.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.collect::<String>()),
                    None => String::new(),
                }
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{} ", icon), Style::default().fg(self.app_theme.warning)),
                Span::styled(format!("{:<18}", cat_display), style),
                Span::styled(desc, Style::default().fg(self.app_theme.muted)),
            ]))
        }).collect();

        // ── Reset Account button at the bottom ──
        let reset_selected = self.selected_cat == reset_idx;
        let reset_prefix = if reset_selected { " > " } else { "   " };
        let reset_style = if reset_selected {
            Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.app_theme.error)
        };
        items.push(ListItem::new(Line::raw(""))); // spacer
        items.push(ListItem::new(Line::from(vec![
            Span::styled(reset_prefix, reset_style),
            Span::styled("[!!] ", Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<18}", "Reset Account"), reset_style.add_modifier(Modifier::BOLD)),
            Span::styled("Wipe user data and/or settings", Style::default().fg(self.app_theme.muted)),
        ])));

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.app_theme.border))
                .title(" NeuraSettings - Preferences ")
                .title_style(Style::default().fg(self.app_theme.accent).add_modifier(Modifier::BOLD)));
        frame.render_widget(list, area);
    }

    fn render_settings_list(&self, frame: &mut Frame, area: Rect) {
        let cat_name = self.categories.get(self.selected_cat).cloned().unwrap_or_default();
        let title = {
            let mut chars = cat_name.chars();
            match chars.next() {
                Some(c) => format!(" {} Settings ", format!("{}{}", c.to_uppercase(), chars.collect::<String>())),
                None => " Settings ".to_string(),
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.app_theme.border))
            .title(title)
            .title_style(Style::default().fg(self.app_theme.accent).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_height = inner.height as usize;
        let mut scroll = self.settings_scroll.load(Ordering::Relaxed);

        // Auto-adjust scroll so selected item is visible
        if self.selected_setting >= scroll + visible_height {
            scroll = self.selected_setting.saturating_sub(visible_height.saturating_sub(1));
        } else if self.selected_setting < scroll {
            scroll = self.selected_setting;
        }
        self.settings_scroll.store(scroll, Ordering::Relaxed);

        let items: Vec<Line> = self.current_keys.iter().enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, (key, value))| {
                let display_key = Self::display_key(key);
                let display_val = Self::display_value(key, value);
                let style = if i == self.selected_setting {
                    Style::default().fg(self.app_theme.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.app_theme.fg)
                };
                let prefix = if i == self.selected_setting { " > " } else { "   " };

                let val_color = if Self::is_boolean_setting(key) {
                    if value == "true" { self.app_theme.success } else { self.app_theme.error }
                } else if value.is_empty() {
                    palette::DIM
                } else if key.contains("api_key") {
                    self.app_theme.success
                } else {
                    self.app_theme.warning
                };

                // Show type indicator
                let type_hint = if Self::is_boolean_setting(key) {
                    ""
                } else if self.known_options(key).is_some() {
                    " [...]"
                } else {
                    ""
                };

                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{:<22}", display_key), style),
                    Span::styled(display_val, Style::default().fg(val_color)),
                    Span::styled(type_hint, Style::default().fg(palette::DIM)),
                ])
            }).collect();

        let paragraph = Paragraph::new(items);
        frame.render_widget(paragraph, inner);

        // Scroll indicators
        let total_items = self.current_keys.len();
        if scroll > 0 {
            frame.render_widget(
                Paragraph::new("▲").alignment(Alignment::Right).style(Style::default().fg(self.app_theme.accent)),
                Rect::new(inner.x, inner.y, inner.width, 1)
            );
        }
        if scroll + visible_height < total_items {
            frame.render_widget(
                Paragraph::new("▼").alignment(Alignment::Right).style(Style::default().fg(self.app_theme.accent)),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1)
            );
        }
    }

    fn render_editor(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(5),
                Constraint::Min(1),
            ])
            .split(area);

        // Key info
        let display_key = Self::display_key(&self.edit_key);
        let info = Paragraph::new(vec![
            Line::from(Span::styled(format!("  Setting: {}", self.edit_key), Style::default().fg(self.app_theme.statusbar_fg))),
            Line::from(Span::styled(format!("  Label:   {}", display_key), Style::default().fg(self.app_theme.fg))),
        ])
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.app_theme.border))
            .title(" Edit Setting ").title_style(Style::default().fg(self.app_theme.warning).add_modifier(Modifier::BOLD)));
        frame.render_widget(info, chunks[0]);

        // Edit input
        let is_secret = self.edit_key.contains("api_key") || self.edit_key.contains("password");
        let hint = if is_secret {
            "  (value will be masked in display)"
        } else {
            ""
        };
        let input = Paragraph::new(vec![
            Line::from(Span::styled(format!("  > {}", self.edit_buffer), Style::default().fg(self.app_theme.success))),
            Line::from(Span::styled(hint, Style::default().fg(palette::DIM))),
        ])
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.app_theme.accent))
            .title(" Value ").title_style(Style::default().fg(self.app_theme.accent)));
        frame.render_widget(input, chunks[1]);

        // Tips
        let tip = self.get_setting_tip(&self.edit_key);
        if !tip.is_empty() {
            let tip_widget = Paragraph::new(format!("  Tip: {}", tip))
                .style(Style::default().fg(self.app_theme.muted))
                .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(self.app_theme.border))
                    .title(" Help "));
            frame.render_widget(tip_widget, chunks[2]);
        }
    }

    fn render_selector(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(4),
            ])
            .split(area);

        // Header showing which setting we're selecting for
        let display_key = Self::display_key(&self.edit_key);
        let current_val = self.prefs.get(&self.edit_key);
        let header = Paragraph::new(format!("  Select value for: {}  (current: {})", display_key, current_val))
            .style(Style::default().fg(self.app_theme.statusbar_fg))
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(self.app_theme.border))
                .title(" Select Option ")
                .title_style(Style::default().fg(self.app_theme.warning).add_modifier(Modifier::BOLD)));
        frame.render_widget(header, chunks[0]);

        // Options list
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(self.app_theme.accent))
            .title(" Options ")
            .title_style(Style::default().fg(self.app_theme.accent));
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);

        let visible_height = inner.height as usize;
        let mut scroll = self.selector_scroll.load(Ordering::Relaxed);

        // Auto-adjust scroll
        if self.select_index >= scroll + visible_height {
            scroll = self.select_index.saturating_sub(visible_height.saturating_sub(1));
        } else if self.select_index < scroll {
            scroll = self.select_index;
        }
        self.selector_scroll.store(scroll, Ordering::Relaxed);

        let items: Vec<Line> = self.select_options.iter().enumerate()
            .skip(scroll)
            .take(visible_height)
            .map(|(i, option)| {
            let is_current = option == current_val;
            let is_selected = i == self.select_index;
            let prefix = if is_selected { " > " } else { "   " };
            let suffix = if is_current { "  (current)" } else { "" };
            let style = if is_selected {
                Style::default().fg(self.app_theme.accent).add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default().fg(self.app_theme.success)
            } else {
                Style::default().fg(self.app_theme.fg)
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(option.to_string(), style),
                Span::styled(suffix, Style::default().fg(self.app_theme.muted)),
            ])
        }).collect();

        let paragraph = Paragraph::new(items);
        frame.render_widget(paragraph, inner);

        // Scroll indicators
        let total_items = self.select_options.len();
        if scroll > 0 {
            frame.render_widget(
                Paragraph::new("▲").alignment(Alignment::Right).style(Style::default().fg(self.app_theme.accent)),
                Rect::new(inner.x, inner.y, inner.width, 1)
            );
        }
        if scroll + visible_height < total_items {
            frame.render_widget(
                Paragraph::new("▼").alignment(Alignment::Right).style(Style::default().fg(self.app_theme.accent)),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1)
            );
        }

        // Tip for this setting
        let tip = self.get_setting_tip(&self.edit_key);
        if !tip.is_empty() {
            let tip_widget = Paragraph::new(format!("  Tip: {}", tip))
                .style(Style::default().fg(self.app_theme.muted))
                .block(Block::default().borders(Borders::ALL)
                    .border_style(Style::default().fg(self.app_theme.border))
                    .title(" Help "));
            frame.render_widget(tip_widget, chunks[2]);
        }
    }

    fn render_reset_choose(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Min(4),
            ])
            .split(area);

        // Warning header
        let warning = Paragraph::new(vec![
            Line::from(Span::styled(
                "  !! WARNING — This action is IRREVERSIBLE !!",
                Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  All selected data will be permanently deleted.",
                Style::default().fg(self.app_theme.fg),
            )),
            Line::from(Span::styled(
                "  There is no undo. Choose carefully.",
                Style::default().fg(self.app_theme.warning),
            )),
        ])
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.app_theme.error))
            .title(" !! Reset Account !! ")
            .title_style(Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)));
        frame.render_widget(warning, chunks[0]);

        // Option list
        let options = [
            (
                "Data Only",
                "Wipe notes, tasks, contacts, chat history,\n  media library and other user files.\n  Your settings and preferences are kept.",
            ),
            (
                "Data + Settings",
                "Wipe EVERYTHING — all user files AND all\n  preferences. System resets to factory defaults.",
            ),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.app_theme.border))
            .title(" Select Reset Type ")
            .title_style(Style::default().fg(self.app_theme.warning).add_modifier(Modifier::BOLD));
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);

        let mut lines: Vec<Line> = Vec::new();
        for (i, (label, desc)) in options.iter().enumerate() {
            let selected = i == self.reset_option_sel;
            let prefix = if selected { " > " } else { "   " };
            let label_style = if selected {
                Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.app_theme.fg)
            };
            let check = if selected { "[x]" } else { "[ ]" };
            lines.push(Line::from(vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{} ", check), label_style),
                Span::styled(label.to_string(), label_style.add_modifier(Modifier::BOLD)),
            ]));
            for desc_line in desc.split('\n') {
                lines.push(Line::from(Span::styled(
                    format!("       {}", desc_line),
                    Style::default().fg(self.app_theme.muted),
                )));
            }
            lines.push(Line::raw(""));
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_reset_confirm(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Length(5),
                Constraint::Min(1),
            ])
            .split(area);

        let scope_label = match self.reset_option {
            ResetOption::DataOnly        => "DATA ONLY",
            ResetOption::DataAndSettings => "DATA + SETTINGS",
        };
        let will_delete: Vec<Line> = {
            let mut lines = vec![
                Line::from(Span::styled(
                    format!("  Performing: {} reset", scope_label),
                    Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
                Line::from(Span::styled("  The following will be permanently deleted:", Style::default().fg(self.app_theme.fg))),
                Line::from(Span::styled("  • All notes (.notes files)", Style::default().fg(self.app_theme.warning))),
                Line::from(Span::styled("  • All tasks (.task files)", Style::default().fg(self.app_theme.warning))),
                Line::from(Span::styled("  • Contacts, chat history, media library", Style::default().fg(self.app_theme.warning))),
                Line::from(Span::styled("  • Browser bookmarks, mail, other data files", Style::default().fg(self.app_theme.warning))),
            ];
            if self.reset_option == ResetOption::DataAndSettings {
                lines.push(Line::from(Span::styled(
                    "  • All preferences and settings",
                    Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD),
                )));
            }
            lines
        };

        let warning = Paragraph::new(will_delete)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.app_theme.error))
                .title(" !! Confirm Reset — Step 2 of 2 !! ")
                .title_style(Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)));
        frame.render_widget(warning, chunks[0]);

        // Confirmation input
        let typed_style = if self.reset_confirm_buf == "RESET" {
            Style::default().fg(self.app_theme.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.app_theme.warning)
        };
        let input = Paragraph::new(vec![
            Line::from(Span::styled("  Type RESET (all caps) to confirm:", Style::default().fg(self.app_theme.fg))),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(self.app_theme.muted)),
                Span::styled(self.reset_confirm_buf.clone(), typed_style),
                Span::styled("_", Style::default().fg(self.app_theme.muted)),
            ]),
        ])
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.app_theme.accent))
            .title(" Confirmation ")
            .title_style(Style::default().fg(self.app_theme.accent)));
        frame.render_widget(input, chunks[1]);

        // Hint
        let hint = Paragraph::new("  Press [Enter] when done typing, or [Esc] to go back.")
            .style(Style::default().fg(self.app_theme.muted));
        frame.render_widget(hint, chunks[2]);
    }

    fn get_setting_tip(&self, key: &str) -> &str {
        match key {
            "ai.provider" => "Select your AI provider. Model and endpoint auto-configure.",
            "ai.api_key" => "Required for Gemini/OpenAI/DeepSeek. Not needed for Ollama.",
            "ai.model" => "Recommended models shown. Auto-set when provider changes.",
            "ai.temperature" => "0.0 = deterministic, 1.0 = creative. Default: 0.7",
            "ai.max_tokens" => "Maximum tokens per response. Range: 256 - 32768",
            "ai.base_url" => "Auto-configured per provider. Only edit for custom setups.",
            "ai.system_prompt" => "Custom prefix added to AI system prompt. Leave empty for default.",
            "ai.response_format" => "Output format for AI responses.",
            "ai.rate_limit_rpm" => "Maximum requests per minute (1-120)",
            "ai.streaming" => "Enable streaming AI responses.",
            "ai.auto_suggest" => "Auto-suggest AI completions while typing.",
            "ui.theme" => "Color theme for the entire UI. Applies immediately in real time.",
            "ui.accent_color" => "Hex color code for accent elements (e.g. #7aa2f7)",
            "ui.border_style" => "Border style for shell and input panels: rounded, plain, double, thick. Applies immediately.",
            "ui.color_scheme" => "Color scheme: 'dark' uses your selected theme; 'light' switches to a light palette. Applies immediately.",
            "ui.font_size" => "Text size preference (informational — set in your terminal emulator).",
            "ui.show_statusbar" => "Show the top status bar.",
            "ui.show_clock" => "Show clock in status bar. Toggle applies immediately.",
            "ui.show_notifications" => "Show notification popups.",
            "ui.transparency" => "Skip background fill on the status bar — lets the terminal background show through.",
            "ui.dim_inactive" => "Dim inactive panes.",
            "shell.prompt_style" => "Shell prompt appearance.",
            "shell.history_size" => "Number of commands to keep in history (100-10000)",
            "shell.auto_complete" => "Enable tab-completion suggestions.",
            "shell.syntax_highlight" => "Highlight shell command syntax.",
            "shell.bell" => "Audible bell on errors.",
            "shell.vi_mode" => "Enable vi-style keybindings in the shell.",
            "shell.default_editor" => "Default text editor for the shell.",
            "shell.greeting" => "Show ASCII logo and welcome on startup.",
            "desktop.default_workspace" => "Name of the default workspace.",
            "desktop.workspace_count" => "Number of virtual workspaces (1-8)",
            "desktop.session_restore" => "Restore previous session on startup.",
            "desktop.auto_save_interval" => "Seconds between auto-saves (10-300)",
            "desktop.startup_app" => "App to launch automatically on startup.",
            "desktop.window_animation" => "Enable window transition animations.",
            "desktop.clock_24h" => "Use 24-hour clock format.",
            "desktop.show_seconds" => "Show seconds in status bar clock.",
            "desktop.timezone" => "Timezone for the status bar clock and NeuraClock. Select from the list — changes apply immediately.",
            "storage.max_cache_mb" => "Maximum cache size in megabytes (64-4096)",
            "storage.auto_backup" => "Automatically back up data.",
            "storage.backup_interval_hours" => "Hours between automatic backups (1-168)",
            "storage.vfs_auto_save" => "Auto-save virtual filesystem changes.",
            "storage.journal_max_entries" => "Maximum journal log entries (1000-100000)",
            "storage.compress_backups" => "Compress backup archives.",
            "security.session_timeout_hours" => "Hours before session expires (1-168)",
            "security.require_password_on_wake" => "Require password after idle timeout.",
            "security.sandbox_apps" => "Run apps in a sandboxed environment.",
            "security.allow_network_apps" => "Allow apps to access the network.",
            "security.signed_packages_only" => "Only install cryptographically signed packages.",
            "notifications.enabled" => "Enable the notification system.",
            "notifications.sound" => "Play sound on notifications.",
            "notifications.position" => "Screen corner for notification popups.",
            "notifications.duration_secs" => "Seconds before notifications auto-dismiss (1-30)",
            "accessibility.high_contrast" => "Boost border and text contrast. Applies immediately.",
            "accessibility.screen_reader" => "Print extra context lines in the shell for screen readers.",
            "accessibility.large_text" => "Add extra spacing inside panels for easier reading.",
            "accessibility.reduce_motion" => "Disable any UI blinking or rapid-update effects.",
            "accessibility.cursor_blink" => "Toggle cursor blinking in the input field. Applies immediately.",
            "network.proxy" => "HTTP proxy URL (e.g. http://proxy:8080). Leave empty for direct.",
            "network.timeout_secs" => "Network request timeout in seconds (5-120)",
            "network.auto_sync" => "Automatically sync data with remote.",
            "network.sync_interval_mins" => "Minutes between auto-sync attempts (1-60)",
            _ => "",
        }
    }
}
