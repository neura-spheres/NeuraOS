use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_ai_core::provider::{AiProvider, types::*};
use neura_ai_core::ToolRegistry;
use neura_app_framework::palette::*;

#[derive(Debug, Clone, PartialEq)]
pub enum ChatRole { User, Assistant, System, Tool }

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

pub struct ChatApp {
    messages: Vec<ChatMessage>,
    input: String,
    input_cursor: usize,
    scroll: usize,
    is_thinking: bool,
    pending_prompt: Option<String>,
    ai_client: Option<Arc<dyn AiProvider>>,
    ai_temperature: f32,
    ai_max_tokens: u32,
    username: String,
    /// OS tool registry — when set, enables full agentic multi-step behaviour.
    tool_registry: Option<Arc<ToolRegistry>>,
    /// System prompt used when tool_registry is active.
    agent_system_prompt: String,
    /// How many tool steps the agent took on last call (for display).
    last_tool_steps: u32,
    /// Set to true after agent loop runs; main.rs reloads app states and resets this.
    pub needs_app_reload: bool,
}

impl ChatApp {
    pub fn new(username: &str) -> Self {
        let welcome = ChatMessage {
            role: ChatRole::System,
            content: "Welcome to NeuraChat! I'm your AI assistant with full NeuraOS access.\n\
                      I can read/write your notes, manage tasks, find contacts, send emails, browse your music library, and more.\n\
                      Try: \"Read my shopping note and send it to Frans\" or \"Create a task to buy milk\"".to_string(),
        };
        Self {
            messages: vec![welcome],
            input: String::new(),
            input_cursor: 0,
            scroll: 0,
            is_thinking: false,
            pending_prompt: None,
            ai_client: None,
            ai_temperature: 0.7,
            ai_max_tokens: 4096,
            username: username.to_string(),
            tool_registry: None,
            agent_system_prompt: String::new(),
            last_tool_steps: 0,
            needs_app_reload: false,
        }
    }

    /// Set the AI provider client.
    pub fn set_ai_client(&mut self, client: Arc<dyn AiProvider>) {
        self.ai_client = Some(client);
    }

    pub fn set_ai_params(&mut self, temperature: f32, max_tokens: u32) {
        self.ai_temperature = temperature;
        self.ai_max_tokens = max_tokens;
    }

    /// Wire in the OS tool registry + agent system prompt.
    /// Once set, every user message runs through the full agentic loop.
    pub fn set_tool_registry(&mut self, registry: ToolRegistry, system_prompt: String) {
        self.tool_registry = Some(Arc::new(registry));
        self.agent_system_prompt = system_prompt;
    }

    /// Hot-reload the agent system prompt without rebuilding the tool registry.
    /// Called when `ai.system_prompt` changes in settings.
    pub fn set_agent_system_prompt(&mut self, prompt: String) {
        self.agent_system_prompt = prompt;
    }

    /// Returns true when there is a prompt waiting for a response.
    pub fn needs_ai_response(&self) -> bool {
        self.pending_prompt.is_some() && !self.is_thinking
    }

    // ── Core agentic loop ────────────────────────────────────────────────────

    /// Called by the main event loop.
    pub async fn async_respond(&mut self, client: Arc<dyn AiProvider>) {
        let prompt = match self.pending_prompt.take() {
            Some(p) => p,
            None => return,
        };
        self.is_thinking = true;

        if let Some(ref registry) = self.tool_registry.clone() {
            self.run_agent_loop(client, registry.clone(), prompt).await;
            // Signal main.rs to reload TasksApp / NotesApp / ContactsApp from VFS
            // because tool calls may have written new data that apps don't know about yet.
            if self.last_tool_steps > 0 {
                self.needs_app_reload = true;
            }
        } else {
            self.run_simple_chat(client, prompt).await;
        }

        self.is_thinking = false;
        self.scroll = usize::MAX; // scroll to bottom
    }

    /// Full agentic loop: AI reasons over tools until it gives a final answer.
    async fn run_agent_loop(
        &mut self,
        client: Arc<dyn AiProvider>,
        registry: Arc<ToolRegistry>,
        prompt: String,
    ) {
        // Build history context (last 20 user/assistant pairs, excluding System/Tool msgs)
        let mut messages: Vec<AIMessage> = Vec::new();
        let start = self.messages.len().saturating_sub(40);
        for msg in &self.messages[start..] {
            match msg.role {
                ChatRole::User => messages.push(AIMessage::user(&msg.content)),
                ChatRole::Assistant => messages.push(AIMessage::assistant(&msg.content)),
                ChatRole::System | ChatRole::Tool => {} // not sent to AI
            }
        }
        messages.push(AIMessage::user(&prompt));

        let tool_defs = registry.to_tool_definitions();
        self.last_tool_steps = 0;

        for step in 0..20u32 {
            let request = GenerateRequest {
                messages: messages.clone(),
                system_prompt: Some(self.agent_system_prompt.clone()),
                tools: tool_defs.clone(),
                temperature: self.ai_temperature,
                max_tokens: self.ai_max_tokens,
            };

            let response = match client.generate(request).await {
                Ok(r) => r,
                Err(e) => {
                    self.push_msg(ChatRole::Assistant, format!("Error: {}", e));
                    return;
                }
            };

            // Collect function calls and text
            let mut function_responses: Vec<MessageContent> = Vec::new();
            let mut text_parts: Vec<String> = Vec::new();
            let mut has_tool_call = false;

            for part in &response.content {
                match part {
                    MessageContent::FunctionCall { name, args } => {
                        has_tool_call = true;
                        self.last_tool_steps = step + 1;

                        // Pretty-print args for display
                        let args_display = if args.as_object().map(|m| m.is_empty()).unwrap_or(true) {
                            String::new()
                        } else {
                            format!("({})", args.to_string())
                        };
                        self.push_msg(ChatRole::Tool, format!("  {} {}", name, args_display));

                        // Execute tool
                        let result = if let Some(tool) = registry.get(name) {
                            match (tool.handler)(args.clone()).await {
                                Ok(val) => val,
                                Err(e) => serde_json::json!({"error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"error": format!("Unknown tool: {}", name)})
                        };

                        function_responses.push(MessageContent::FunctionResponse {
                            name: name.clone(),
                            response: result,
                        });
                    }
                    MessageContent::Text(t) => text_parts.push(t.clone()),
                    _ => {}
                }
            }

            // Append AI turn to loop context
            messages.push(AIMessage {
                role: AIChatRole::Assistant,
                content: response.content.clone(),
            });

            if has_tool_call {
                // Feed tool results back
                messages.push(AIMessage {
                    role: AIChatRole::User,
                    content: function_responses,
                });
                continue;
            }

            // No tool call — final text answer
            let text = text_parts.join("").trim().to_string();
            self.push_msg(ChatRole::Assistant, if text.is_empty() { "(no response)".to_string() } else { text });
            return;
        }

        self.push_msg(ChatRole::Assistant, "Agent reached maximum steps without a final answer.".to_string());
    }

    /// Simple single-turn chat (no tools).
    async fn run_simple_chat(&mut self, client: Arc<dyn AiProvider>, prompt: String) {
        let mut messages: Vec<AIMessage> = Vec::new();
        let start = self.messages.len().saturating_sub(20);
        for msg in &self.messages[start..] {
            match msg.role {
                ChatRole::User => messages.push(AIMessage::user(&msg.content)),
                ChatRole::Assistant => messages.push(AIMessage::assistant(&msg.content)),
                _ => {}
            }
        }
        messages.push(AIMessage::user(&prompt));

        let system = format!(
            "You are NeuraChat, an AI assistant embedded in NeuraOS.\n\
             Current user: '{}'. Answer helpfully, formatted for a terminal.",
            self.username
        );

        let request = GenerateRequest {
            messages,
            system_prompt: Some(system),
            tools: Vec::new(),
            temperature: self.ai_temperature,
            max_tokens: self.ai_max_tokens,
        };

        match client.generate(request).await {
            Ok(resp) => {
                let t = resp.text();
                self.push_msg(ChatRole::Assistant, if t.is_empty() { "(empty response)".to_string() } else { t });
            }
            Err(e) => self.push_msg(ChatRole::Assistant, format!("Error: {}", e)),
        }
    }

    fn push_msg(&mut self, role: ChatRole, content: String) {
        self.messages.push(ChatMessage { role, content });
    }

    // ── Text wrapping ────────────────────────────────────────────────────────

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        if width == 0 { return vec![text.to_string()]; }
        let mut lines = Vec::new();
        for raw_line in text.split('\n') {
            if raw_line.is_empty() { lines.push(String::new()); continue; }
            let words: Vec<&str> = raw_line.split_whitespace().collect();
            if words.is_empty() { lines.push(String::new()); continue; }
            let mut current = String::new();
            for word in &words {
                if current.is_empty() {
                    current.push_str(word);
                } else if current.len() + 1 + word.len() <= width {
                    current.push(' '); current.push_str(word);
                } else {
                    lines.push(current.clone()); current = word.to_string();
                }
            }
            if !current.is_empty() { lines.push(current); }
        }
        lines
    }
}

// Type aliases to avoid name conflict with local ChatMessage / ChatRole
type AIMessage = neura_ai_core::provider::types::ChatMessage;
type AIChatRole = neura_ai_core::provider::types::ChatRole;

impl App for ChatApp {
    fn id(&self) -> &str { "chat" }
    fn name(&self) -> &str { "NeuraChat" }
    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.is_thinking {
            if key.code == KeyCode::Esc { return false; }
            return true;
        }
        match key.code {
            KeyCode::Esc => return false,
            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    let prompt = self.input.trim().to_string();
                    self.input.clear(); self.input_cursor = 0;
                    self.push_msg(ChatRole::User, prompt.clone());
                    if self.ai_client.is_some() {
                        self.pending_prompt = Some(prompt);
                    } else {
                        self.push_msg(ChatRole::System, "No AI configured. Set your API key in Settings > AI.".to_string());
                    }
                    self.scroll = usize::MAX;
                }
                true
            }
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match c { 'l' => { self.messages.clear(); true } _ => true }
            }
            KeyCode::Char(c) => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += 1; true
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 { self.input_cursor -= 1; self.input.remove(self.input_cursor); }
                true
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input.len() { self.input.remove(self.input_cursor); }
                true
            }
            KeyCode::Left  => { if self.input_cursor > 0 { self.input_cursor -= 1; } true }
            KeyCode::Right => { if self.input_cursor < self.input.len() { self.input_cursor += 1; } true }
            KeyCode::Home  => { self.input_cursor = 0; true }
            KeyCode::End   => { self.input_cursor = self.input.len(); true }
            KeyCode::Up | KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(if key.code == KeyCode::PageUp { 10 } else { 3 });
                true
            }
            KeyCode::Down | KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(if key.code == KeyCode::PageDown { 10 } else { 3 });
                true
            }
            _ => true,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),   // title bar
                Constraint::Min(5),      // chat area
                Constraint::Length(3),   // input
                Constraint::Length(1),   // help bar
            ])
            .split(area);

        // ── Title Bar ──
        let agent_mode = self.tool_registry.is_some();
        let ai_name = self.ai_client.as_ref()
            .map(|c| format!("{} ({})", c.provider_name(), c.model_name()))
            .unwrap_or_else(|| "No AI configured".to_string());
        let mode_tag = if agent_mode { " [AGENT] " } else { " " };
        let title_text = format!(" NeuraChat{}| {}  | Ctrl+L: clear ", mode_tag, ai_name);
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(if agent_mode { CYAN } else { PRIMARY }).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(BORDER)));
        frame.render_widget(title, chunks[0]);

        // ── Chat Area ──
        let chat_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Conversation ")
            .title_style(Style::default().fg(PRIMARY));
        let chat_inner = chat_block.inner(chunks[1]);
        frame.render_widget(chat_block, chunks[1]);

        let wrap_width = chat_inner.width as usize;
        let mut all_lines: Vec<Line> = Vec::new();

        for msg in &self.messages {
            match msg.role {
                ChatRole::System => {
                    all_lines.push(Line::from(vec![
                        Span::styled("  [SYS] ", Style::default().fg(DIM)),
                        Span::styled(msg.content.clone(), Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
                    ]));
                }
                ChatRole::Tool => {
                    // Tool call notification line — purple/violet
                    all_lines.push(Line::from(vec![
                        Span::styled("   ⚙ ", Style::default().fg(TOOL_CLR)),
                        Span::styled(msg.content.clone(), Style::default().fg(TOOL_CLR).add_modifier(Modifier::ITALIC)),
                    ]));
                }
                ChatRole::User => {
                    all_lines.push(Line::from(vec![
                        Span::styled("  You ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                        Span::styled("─────────────", Style::default().fg(DIM)),
                    ]));
                    for line in Self::wrap_text(&msg.content, wrap_width.saturating_sub(4)) {
                        all_lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(line, Style::default().fg(TEXT)),
                        ]));
                    }
                }
                ChatRole::Assistant => {
                    all_lines.push(Line::from(vec![
                        Span::styled("  AI  ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                        Span::styled("─────────────", Style::default().fg(DIM)),
                    ]));
                    for line in Self::wrap_text(&msg.content, wrap_width.saturating_sub(4)) {
                        all_lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(line, Style::default().fg(TEXT)),
                        ]));
                    }
                }
            }
        }

        // Thinking indicator
        if self.is_thinking {
            all_lines.push(Line::from(vec![
                Span::styled("  AI  ", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled("thinking...", Style::default().fg(ORANGE).add_modifier(Modifier::ITALIC)),
            ]));
        }

        let total = all_lines.len();
        let visible = chat_inner.height as usize;
        let max_scroll = total.saturating_sub(visible);
        let scroll_offset = if self.scroll == usize::MAX { max_scroll } else { self.scroll.min(max_scroll) };

        let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll_offset).take(visible).collect();
        frame.render_widget(Paragraph::new(visible_lines).style(Style::default().bg(MSG_BG)), chat_inner);

        // Scroll %
        if total > visible {
            let pct = if max_scroll == 0 { 100 } else { (scroll_offset * 100) / max_scroll };
            let scroll_area = Rect {
                x: chunks[1].x + chunks[1].width.saturating_sub(8),
                y: chunks[1].y, width: 7, height: 1,
            };
            frame.render_widget(Paragraph::new(format!(" {}% ", pct)).style(Style::default().fg(MUTED)), scroll_area);
        }

        // ── Input Box ──
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.is_thinking { DIM } else { PRIMARY }))
            .title(if self.is_thinking { " Agent working... " } else { " Message " })
            .title_style(Style::default().fg(if self.is_thinking { ORANGE } else { PRIMARY }));
        let input_inner = input_block.inner(chunks[2]);
        frame.render_widget(input_block, chunks[2]);

        let display_input = if self.is_thinking { "Please wait — agent is running...".to_string() } else { self.input.clone() };
        frame.render_widget(
            Paragraph::new(display_input).style(Style::default().fg(if self.is_thinking { DIM } else { TEXT })),
            input_inner,
        );

        if !self.is_thinking {
            let cx = input_inner.x + self.input_cursor as u16;
            if cx < input_inner.x + input_inner.width {
                frame.set_cursor_position((cx, input_inner.y));
            }
        }

        // ── Help Bar ──
        let mode_help = if self.tool_registry.is_some() { "AGENT MODE — tools: notes,tasks,contacts,mail,media,files" } else { "CHAT MODE" };
        let help_text = format!("  [Enter]send  [Ctrl+L]clear  [↑↓]scroll  [Esc]back  |  {}", mode_help);
        frame.render_widget(Paragraph::new(help_text).style(Style::default().fg(MUTED)), chunks[3]);
    }

    fn on_pause(&mut self) {}
    fn on_resume(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        // Save only User/Assistant messages (skip Tool/System noise)
        let msgs: Vec<Value> = self.messages.iter()
            .filter(|m| matches!(m.role, ChatRole::User | ChatRole::Assistant))
            .map(|m| serde_json::json!({
                "role": match m.role { ChatRole::User => "user", _ => "assistant" },
                "content": m.content,
            }))
            .collect();
        Some(serde_json::json!({ "messages": msgs }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(msgs) = state.get("messages").and_then(|v| v.as_array()) {
            let loaded: Vec<ChatMessage> = msgs.iter().filter_map(|m| {
                let role = match m.get("role")?.as_str()? {
                    "user" => ChatRole::User,
                    "assistant" => ChatRole::Assistant,
                    _ => ChatRole::System,
                };
                let content = m.get("content")?.as_str()?.to_string();
                Some(ChatMessage { role, content })
            }).collect();
            if !loaded.is_empty() {
                // Keep welcome, append history
                self.messages.extend(loaded);
            }
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
