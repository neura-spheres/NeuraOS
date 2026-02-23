use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_ai_core::{OllamaManager, OllamaAvailableModel};
use anyhow::Result;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum OllamaSetupMode {
    Welcome,
    InstallPrompt,
    ModelSelection,
    InstallingOllama,
    InstallingModel,
    Complete,
    Error(String),
}

pub struct OllamaSetupApp {
    mode: OllamaSetupMode,
    available_models: Vec<OllamaAvailableModel>,
    installed_models: Vec<String>,
    selected_model: usize,
    scroll_offset: AtomicUsize,
    _username: String,
    installation_progress: String,
    _auto_detected_model: Option<String>,
    next_action: Option<OllamaSetupAction>,
    progress_rx: Option<mpsc::UnboundedReceiver<String>>,
    completion_rx: Option<mpsc::UnboundedReceiver<Result<()>>>,
}

#[derive(Debug, Clone)]
enum OllamaSetupAction {
    CheckStatus,
    InstallOllama,
    InstallModel(String),
}

impl OllamaSetupApp {
    pub fn new(username: &str) -> Self {
        Self {
            mode: OllamaSetupMode::Welcome,
            available_models: OllamaManager::get_popular_models(),
            installed_models: Vec::new(),
            selected_model: 0,
            scroll_offset: AtomicUsize::new(0),
            _username: username.to_string(),
            installation_progress: String::new(),
            _auto_detected_model: None,
            next_action: None,
            progress_rx: None,
            completion_rx: None,
        }
    }

    /// Check current Ollama status and transition to appropriate mode
    pub async fn check_status(&mut self) {
        self.installation_progress = "Checking Ollama status...".to_string();
        
        if !OllamaManager::is_installed() {
            self.mode = OllamaSetupMode::InstallPrompt;
            return;
        }

        // Try to ensure it's running
        if let Err(e) = OllamaManager::ensure_ollama_running().await {
             self.mode = OllamaSetupMode::Error(format!("Failed to start Ollama: {}", e));
             return;
        }

        // Get installed models
        match OllamaManager::get_installed_models().await {
            Ok(models) => {
                self.installed_models = models;
                self.mode = OllamaSetupMode::ModelSelection;
            }
            Err(e) => {
                self.mode = OllamaSetupMode::Error(format!("Failed to get models: {}", e));
            }
        }
    }

    /// Install Ollama
    pub async fn install_ollama(&mut self) -> Result<()> {
        self.mode = OllamaSetupMode::InstallingOllama;
        self.installation_progress = "Starting Ollama installation...".to_string();
        
        let (comp_tx, comp_rx) = mpsc::unbounded_channel();
        self.completion_rx = Some(comp_rx);
        
        tokio::spawn(async move {
            let result = async {
                OllamaManager::install_ollama().await?;
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                Ok(())
            }.await;
            let _ = comp_tx.send(result);
        });
        
        Ok(())
    }

    /// Install the selected model
    pub async fn install_selected_model(&mut self) -> Result<()> {
        if let Some(model) = self.available_models.get(self.selected_model) {
            let model_name = model.name.clone();
            self.mode = OllamaSetupMode::InstallingModel;
            self.installation_progress = format!("Starting installation of {}...", model_name);
            
            let (prog_tx, prog_rx) = mpsc::unbounded_channel();
            let (comp_tx, comp_rx) = mpsc::unbounded_channel();
            
            self.progress_rx = Some(prog_rx);
            self.completion_rx = Some(comp_rx);
            
            tokio::spawn(async move {
                let cb = move |msg: String| {
                    let _ = prog_tx.send(msg);
                };
                
                let result = async {
                    // Check if model already exists
                    if OllamaManager::model_exists(&model_name).await? {
                        return Ok(());
                    }
                    OllamaManager::install_model(&model_name, Some(cb)).await
                }.await;
                
                let _ = comp_tx.send(result);
            });
        }
        Ok(())
    }

    /// Get the selected model name
    pub fn get_selected_model_name(&self) -> Option<String> {
        self.available_models.get(self.selected_model).map(|m| m.name.clone())
    }

    /// Check if setup is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.mode, OllamaSetupMode::Complete)
    }

    /// Get recommended model based on system specs
    fn _get_recommended_model(&self) -> usize {
        // Simple heuristic: recommend smaller models for most users
        0 // llama3.2
    }

    /// Handle enter key and return the selected model if setup is complete
    pub fn handle_enter(&mut self) -> Option<String> {
        match &self.mode {
            OllamaSetupMode::Welcome => {
                self.next_action = Some(OllamaSetupAction::CheckStatus);
                None
            }
            OllamaSetupMode::InstallPrompt => {
                self.next_action = Some(OllamaSetupAction::InstallOllama);
                None
            }
            OllamaSetupMode::ModelSelection => {
                if let Some(model) = self.available_models.get(self.selected_model) {
                    self.next_action = Some(OllamaSetupAction::InstallModel(model.name.clone()));
                }
                None
            }
            OllamaSetupMode::Complete => {
                self.get_selected_model_name()
            }
            _ => None,
        }
    }

    /// Handle up arrow key
    pub fn handle_up(&mut self) {
        if matches!(self.mode, OllamaSetupMode::ModelSelection) {
            if self.selected_model > 0 {
                self.selected_model -= 1;
            }
        }
    }

    /// Handle down arrow key
    pub fn handle_down(&mut self) {
        if matches!(self.mode, OllamaSetupMode::ModelSelection) {
            if self.selected_model < self.available_models.len().saturating_sub(1) {
                self.selected_model += 1;
            }
        }
    }

    /// Process the next async action
    pub async fn process_next_action(&mut self) -> Result<()> {
        // Poll for progress
        if let Some(rx) = &mut self.progress_rx {
            while let Ok(msg) = rx.try_recv() {
                // Keep only the last line or append? 
                // For progress bars, replacing is better.
                self.installation_progress = msg;
            }
        }
        
        // Poll for completion
        let mut completed = false;
        if let Some(rx) = &mut self.completion_rx {
            if let Ok(result) = rx.try_recv() {
                completed = true;
                match result {
                    Ok(_) => {
                        if matches!(self.mode, OllamaSetupMode::InstallingOllama) {
                             self.installation_progress = "Ollama installed successfully!".to_string();
                             // Wait a bit or proceed
                             self.check_status().await;
                        } else if matches!(self.mode, OllamaSetupMode::InstallingModel) {
                             if let Some(name) = self.get_selected_model_name() {
                                 self.installation_progress = format!("{} installed successfully!", name);
                             }
                             self.mode = OllamaSetupMode::Complete;
                        }
                    }
                    Err(e) => {
                        self.mode = OllamaSetupMode::Error(e.to_string());
                    }
                }
            }
        }
        
        if completed {
            self.progress_rx = None;
            self.completion_rx = None;
        }

        if let Some(action) = self.next_action.take() {
            match action {
                OllamaSetupAction::CheckStatus => {
                    self.check_status().await;
                }
                OllamaSetupAction::InstallOllama => {
                    if let Err(e) = self.install_ollama().await {
                        self.mode = OllamaSetupMode::Error(e.to_string());
                    }
                }
                OllamaSetupAction::InstallModel(_model_name) => {
                    if let Err(e) = self.install_selected_model().await {
                        self.mode = OllamaSetupMode::Error(e.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if there's a pending action or background task
    pub fn has_pending_action(&self) -> bool {
        self.next_action.is_some() || self.progress_rx.is_some()
    }
}

impl App for OllamaSetupApp {
    fn id(&self) -> &str { "ollama_setup" }
    fn name(&self) -> &str { "Ollama Setup" }
    
    fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            OllamaSetupMode::Welcome => {
                match key.code {
                    KeyCode::Enter => {
                        self.next_action = Some(OllamaSetupAction::CheckStatus);
                        true
                    }
                    KeyCode::Esc => false,
                    _ => true,
                }
            }
            OllamaSetupMode::InstallPrompt => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.next_action = Some(OllamaSetupAction::InstallOllama);
                        true
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => false,
                    _ => true,
                }
            }
            OllamaSetupMode::ModelSelection => {
                match key.code {
                    KeyCode::Up => {
                        if self.selected_model > 0 {
                            self.selected_model -= 1;
                        }
                        true
                    }
                    KeyCode::Down => {
                        if self.selected_model < self.available_models.len().saturating_sub(1) {
                            self.selected_model += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        if let Some(model) = self.available_models.get(self.selected_model) {
                            self.next_action = Some(OllamaSetupAction::InstallModel(model.name.clone()));
                        }
                        true
                    }
                    KeyCode::Esc => false,
                    _ => true,
                }
            }
            OllamaSetupMode::Complete => {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => false,
                    _ => true,
                }
            }
            OllamaSetupMode::Error(_) => {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc => false,
                    _ => true,
                }
            }
            _ => true, // Block input during installation
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // Title
                Constraint::Min(10),   // Main content
                Constraint::Length(3), // Status/Help
            ])
            .split(area);

        // Title
        let title = Paragraph::new("🦙 Ollama Setup Wizard")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(title, chunks[0]);

        // Main content
        let content = match &self.mode {
            OllamaSetupMode::Welcome => {
                let text = format!(
                    "Welcome to Ollama Setup!\n\n\
                    This wizard will help you:\n\
                    • Check if Ollama is installed\n\
                    • Install Ollama if needed\n\
                    • Download and install AI models\n\n\
                    Press Enter to continue..."
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::White))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
            OllamaSetupMode::InstallPrompt => {
                let text = format!(
                    "Ollama is not installed on your system.\n\n\
                    Would you like to install Ollama now?\n\n\
                    This will download and install the latest version of Ollama.\n\n\
                    Press 'Y' to install, 'N' to skip..."
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
            OllamaSetupMode::ModelSelection => {
                let height = chunks[1].height as usize;
                // Header: "Select a model..." + blank line = 2
                // Footer: "Use up/down..." = 1 (rendered in separate paragraph?) No, appended.
                // Indicators: up/down arrows = 2 max
                // Total overhead approx 4-6 lines. Safe bet is 6.
                let visible_lines = height.saturating_sub(6);
                let mut offset_val = self.scroll_offset.load(Ordering::Relaxed);

                // Auto-scroll logic to ensure selection is visible
                if self.selected_model < offset_val {
                    offset_val = self.selected_model;
                } else if self.selected_model >= offset_val + visible_lines {
                    offset_val = self.selected_model.saturating_sub(visible_lines).saturating_add(1);
                }
                self.scroll_offset.store(offset_val, Ordering::Relaxed);

                let start = offset_val;
                let end = (start + visible_lines).min(self.available_models.len());
                
                let mut text = String::from("Select a model to install:\n\n");
                
                // Show scroll indicator top
                if start > 0 {
                    text.push_str("  ↑ more...\n");
                }

                for i in start..end {
                    if let Some(model) = self.available_models.get(i) {
                        let prefix = if i == self.selected_model { "▶ " } else { "  " };
                        let recommended = if model.recommended { " (Recommended)" } else { "" };
                        
                        // Truncate description if too long
                        let desc = if model.description.len() > 50 {
                            format!("{}...", &model.description[..47])
                        } else {
                            model.description.clone()
                        };
                        
                        let line = format!("{}{:<12} - {}{}\n", 
                            prefix, model.name, desc, recommended);
                        text.push_str(&line);
                    }
                }

                // Show scroll indicator bottom
                if end < self.available_models.len() {
                    text.push_str("  ↓ more...\n");
                }
                
                text.push_str("\nUse ↑/↓ to navigate, Enter to install...");
                
                Paragraph::new(text)
                    .style(Style::default().fg(Color::White))
                    // Do NOT wrap, as it breaks line counting for scrolling
                    .wrap(ratatui::widgets::Wrap { trim: false })
            }
            OllamaSetupMode::InstallingOllama => {
                let text = format!(
                    "Installing Ollama...\n\n\
                    {}\n\n\
                    This may take a few minutes...",
                    self.installation_progress
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Blue))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
            OllamaSetupMode::InstallingModel => {
                let text = format!(
                    "Installing model...\n\n\
                    {}\n\n\
                    This may take several minutes depending on model size...",
                    self.installation_progress
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Blue))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
            OllamaSetupMode::Complete => {
                let text = format!(
                    "Setup Complete! 🎉\n\n\
                    {}\n\n\
                    Ollama is ready to use with your selected model.\n\n\
                    Press Enter to finish...",
                    self.installation_progress
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Green))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
            OllamaSetupMode::Error(msg) => {
                let text = format!(
                    "Error Occurred:\n\n\
                    {}\n\n\
                    Press Enter to exit...",
                    msg
                );
                Paragraph::new(text)
                    .style(Style::default().fg(Color::Red))
                    .wrap(ratatui::widgets::Wrap { trim: true })
            }
        };
        
        frame.render_widget(content, chunks[1]);

        // Status/Help line
        let help_text = match &self.mode {
            OllamaSetupMode::Welcome => "Press Enter to continue, Esc to exit",
            OllamaSetupMode::InstallPrompt => "Press 'Y' to install, 'N' to skip",
            OllamaSetupMode::ModelSelection => "↑/↓ to navigate, Enter to install, Esc to exit",
            OllamaSetupMode::InstallingOllama | OllamaSetupMode::InstallingModel => "Installing... please wait",
            OllamaSetupMode::Complete => "Press Enter to finish",
            OllamaSetupMode::Error(_) => "Press Enter to exit",
        };
        
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    fn save_state(&self) -> Option<Value> {
        None
    }

    fn load_state(&mut self, _state: Value) {
        // No state to load
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}