use std::io;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, MouseEventKind, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::style::Modifier;
use tracing::info;

use neura_kernel::fs_ops;
use neura_logging::init_logging;
use neura_storage::db::{Database, MigrationRunner};
use neura_storage::paths;
use neura_storage::vfs::Vfs;
use neura_users::auth::AuthService;
use neura_users::account::UserStore;
use neura_users::roles::Role;
use neura_shell::{ShellParser, ShellExecutor, ShellContext};
use neura_shell::builtins::Builtins;
use neura_desktop::renderer::{Desktop, DesktopMode, HomeFocus};
use neura_desktop::input::{InputHandler, InputAction};
use neura_app_framework::app_trait::App;
use neura_app_framework::palette;
use neura_app_framework::consts::{OS_NAME, OS_VERSION};
use neura_apps::notes::NotesApp;
use neura_apps::tasks::TasksApp;
use neura_apps::files::FilesApp;
use neura_apps::settings::{SettingsApp, AppTheme, ResetOption};
use neura_apps::calc::CalcApp;
use neura_apps::clock::ClockApp;
use neura_apps::monitor::{TaskManagerApp, OpenAppsTracker, AppEntry};
use neura_apps::calendar::CalendarApp;
use neura_apps::sysinfo_app::SysInfoApp;
use neura_apps::logs::LogsApp;
use neura_apps::contacts::ContactsApp;
use neura_apps::chat::ChatApp;
use neura_apps::dev::DevApp;
use neura_apps::weather::WeatherApp;
use neura_apps::terminal_app::TerminalApp;
use neura_apps::media::MediaApp;
use neura_apps::backup::BackupApp;
use neura_apps::browser::BrowserApp;
use neura_apps::mail::MailApp;
use neura_apps::placeholder::create_placeholder;
use neura_apps::agent_tools::{build_os_tools, MediaCmdSlot, NowPlayingSlot, NowPlayingSnapshot, OsActionQueue, OsAction};
use neura_apps::ollama_setup::OllamaSetupApp;
use neura_ai_core::provider::{create_provider, types::ProviderConfig};
use neura_ai_core::{ToolRegistry, OllamaManager};

#[tokio::main]
async fn main() -> Result<()> {
    fs_ops::ensure_base_dirs()
        .map_err(|e| anyhow::anyhow!("Failed to create base dirs: {}", e))?;

    init_logging(&paths::logs_dir(), "info");
    info!("{} v{} starting...", OS_NAME, OS_VERSION);

    let db = Database::open(&paths::db_path())
        .map_err(|e| anyhow::anyhow!("Failed to open database: {}", e))?;

    let migrations = MigrationRunner::new();
    migrations.run(&db).await
        .map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;
    info!("Database initialized");

    let vfs_path = neura_kernel::fs_ops::neura_home().join("data").join("vfs.json");
    let vfs = Vfs::with_persistence(&vfs_path)
        .unwrap_or_else(|_| Vfs::new());
    if let Err(e) = vfs.bootstrap_defaults().await {
        tracing::warn!("VFS bootstrap defaults failed: {}", e);
    }
    info!("VFS ready");

    let vfs = Arc::new(vfs);

    let user_store_arc = Arc::new(UserStore::new(db.clone()));
    let auth = AuthService::new(UserStore::new(db.clone()));
    info!("User system ready");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (username, role) = match run_auth_screen(&mut terminal, &auth).await? {
        Some(pair) => pair,
        None => {
            let _ = disable_raw_mode();
            let _ = execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen);
            let _ = terminal.show_cursor();
            return Ok(());
        }
    };
    info!("NeuraOS boot — user: {} ({})", username, role);

    let mut shell_ctx = ShellContext::new(vfs.clone(), &username, "neuraos");
    shell_ctx.role = role.clone();
    shell_ctx.user_store = Some(user_store_arc);
    shell_ctx.db = Some(db.clone());
    shell_ctx.load_ai_history().await;

    let home_dir = format!("/home/{}", username);
    if !vfs.exists(&home_dir).await {
        if let Err(e) = vfs.mkdir(&home_dir, &username).await {
            tracing::warn!("Failed to create home directory: {}", e);
        }
    }
    let mut apps: HashMap<String, Box<dyn App>> = HashMap::new();

    // Shared app-lifecycle tracker — TaskManagerApp holds a clone and reads it;
    // main.rs updates it whenever an app opens, closes, or is killed.
    let tracker: OpenAppsTracker = Arc::new(Mutex::new(HashMap::new()));

    // Core apps (fully implemented with VFS persistence)
    apps.insert("notes".to_string(), Box::new(NotesApp::new(vfs.clone(), &username)));
    apps.insert("tasks".to_string(), Box::new(TasksApp::new(vfs.clone(), &username)));
    apps.insert("files".to_string(), Box::new(FilesApp::new(vfs.clone(), &username, role.is_privileged())));
    apps.insert("settings".to_string(), Box::new(SettingsApp::new(vfs.clone(), &username)));
    apps.insert("contacts".to_string(), Box::new(ContactsApp::new(vfs.clone(), &username)));
    apps.insert("logs".to_string(), Box::new(LogsApp::new(vfs.clone(), &username)));

    // Standalone apps (no VFS dependency)
    apps.insert("calc".to_string(), Box::new(CalcApp::new()));
    apps.insert("clock".to_string(), Box::new(ClockApp::new()));
    apps.insert("monitor".to_string(), Box::new(TaskManagerApp::new(tracker.clone())));
    apps.insert("calendar".to_string(), Box::new(CalendarApp::new(vfs.clone(), &username)));
    apps.insert("sysinfo".to_string(), Box::new(SysInfoApp::new()));

    // New fully-functional apps
    apps.insert("chat".to_string(), Box::new(ChatApp::new(&username)));
    apps.insert("dev".to_string(), Box::new(DevApp::new(vfs.clone(), &username)));
    apps.insert("weather".to_string(), Box::new(WeatherApp::new()));
    apps.insert("terminal".to_string(), Box::new(TerminalApp::new()));
    apps.insert("media".to_string(), Box::new(MediaApp::new(vfs.clone(), &username)));
    apps.insert("backup".to_string(), Box::new(BackupApp::new(vfs.clone(), &username)));
    apps.insert("browser".to_string(), Box::new(BrowserApp::new(vfs.clone(), &username)));
    apps.insert("mail".to_string(), Box::new(MailApp::new(vfs.clone(), &username)));

    // Remaining placeholder apps
    for id in &["ssh", "ftp", "db", "sync", "store"] {
        apps.insert(id.to_string(), Box::new(create_placeholder(id)));
    }

    // Populate the tracker with every registered app (all start Idle).
    {
        let mut t = tracker.lock().unwrap();
        for app in apps.values() {
            t.insert(app.id().to_string(), AppEntry::new(app.id(), app.name()));
        }
    }

    // Initialize all apps
    for app in apps.values_mut() {
        if let Err(e) = app.init() {
            tracing::warn!("App '{}' init: {}", app.id(), e);
        }
    }

    // Load persisted app state from VFS
    load_app_states(&vfs, &home_dir, &mut apps).await;
    info!("Applications initialized");

    // These are Arc<Mutex<_>> values shared between the AI tool closures and the app that owns the resource.
    let media_cmd_slot: MediaCmdSlot = Arc::new(Mutex::new(None));
    let now_playing_slot: NowPlayingSlot = Arc::new(Mutex::new(NowPlayingSnapshot::default()));
    let os_action_queue: OsActionQueue = Arc::new(Mutex::new(Vec::new()));

    // Wire media slots into MediaApp so AI agent commands reach the audio backend each tick.
    if let Some(media_app) = apps.get_mut("media") {
        if let Some(media) = media_app.as_any_mut().downcast_mut::<MediaApp>() {
            media.set_agent_cmd_slot(media_cmd_slot.clone());
            media.set_now_playing_slot(now_playing_slot.clone());
        }
    }

    let mut ai_client_opt = None;
    let mut user_system_prompt = String::new();
    let mut ai_temperature = 0.7f32;
    let mut ai_max_tokens = 4096u32;
    // Stored so the hot-reload block can reconstruct the full agent prompt
    // when the user edits ai.system_prompt in Settings.
    let mut agent_base_prompt = String::new();

    if let Some(settings_app) = apps.get("settings") {
        if let Some(state) = settings_app.save_state() {
            let get_val = |key: &str| -> String {
                state.get("values")
                    .and_then(|v| v.get(key))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            let provider = get_val("ai.provider");
            let provider_name = if provider.is_empty() { "gemini".to_string() } else { provider };
            let model = get_val("ai.model");
            
            // Determine default model based on provider
            let default_model = match provider_name.as_str() {
                "ollama" => "llama3.2:3b",
                "openai" => "gpt-4o-mini",
                "deepseek" => "deepseek-chat",
                _ => "gemini-2.5-flash-lite",
            };
            let model = if model.is_empty() { default_model.to_string() } else { model };
            
            let mut api_key = get_val("ai.api_key");
            let base_url = get_val("ai.base_url");

            // Auto-setup Ollama if selected as provider
            if provider_name == "ollama" && !OllamaManager::is_installed() {
                info!("Ollama not detected, launching setup wizard...");
                
                // Create Ollama setup app
                let mut ollama_setup = OllamaSetupApp::new(&username);
                
                // Run the setup wizard
                match run_ollama_setup(&mut terminal, &mut ollama_setup).await {
                    Ok(Some(selected_model)) => {
                        info!("Ollama setup completed with model: {}", selected_model);
                        // Update settings with the selected model
                        if let Some(settings_app) = apps.get_mut("settings") {
                            if let Some(settings) = settings_app.as_any_mut().downcast_mut::<SettingsApp>() {
                                settings.set_pref("ai.model", &selected_model);
                            }
                        }
                    }
                    Ok(None) => {
                        info!("Ollama setup cancelled by user");
                    }
                    Err(e) => {
                        tracing::warn!("Ollama setup failed: {}", e);
                    }
                }
            }

            // Environment variable fallback per provider
            if api_key.is_empty() {
                api_key = match provider_name.as_str() {
                    "gemini" => std::env::var("GEMINI_API_KEY").unwrap_or_default(),
                    "openai" | "custom" => std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                    "deepseek" => std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
                    _ => String::new(),
                };
            }

            let config = ProviderConfig {
                provider: provider_name.clone(),
                model,
                api_key,
                base_url,
            };

            match create_provider(config) {
                Ok(client) => {
                    shell_ctx.set_ai_client(client.clone());
                    ai_client_opt = Some(client);
                    info!("AI client initialized: {}", provider_name);
                }
                Err(neura_ai_core::AiError::NoApiKey) => {
                    info!("No AI API key configured - ai command will prompt user");
                }
                Err(e) => {
                    tracing::warn!("Failed to create AI provider: {}", e);
                }
            }

            // Wire AI settings
            if let Ok(temp) = get_val("ai.temperature").parse::<f32>() {
                shell_ctx.ai_temperature = temp;
                ai_temperature = temp;
            }
            if let Ok(tokens) = get_val("ai.max_tokens").parse::<u32>() {
                shell_ctx.ai_max_tokens = tokens;
                ai_max_tokens = tokens;
            }
            if let Ok(hist) = get_val("shell.history_size").parse::<usize>() {
                shell_ctx.max_history = hist;
            }
            // Capture user-configured system prompt prefix for the AI agent
            user_system_prompt = get_val("ai.system_prompt");
        }
    }

    // Wire AI client + OS tool registry into ChatApp
    if let Some(ref client) = ai_client_opt {
        if let Some(chat_app) = apps.get_mut("chat") {
            if let Some(chat) = chat_app.as_any_mut().downcast_mut::<ChatApp>() {
                chat.set_ai_client(client.clone());
                chat.set_ai_params(ai_temperature, ai_max_tokens);

                // Build the OS tool registry and enable full agentic mode
                let os_tools = build_os_tools(
                    vfs.clone(),
                    username.clone(),
                    media_cmd_slot.clone(),
                    now_playing_slot.clone(),
                    os_action_queue.clone(),
                );
                let mut registry = ToolRegistry::new();
                for tool in os_tools {
                    registry.register(tool);
                }

                let base_prompt = format!(
"You are NeuraChat — the SUPER SMART AI brain of NeuraOS. You have DIRECT, IMMEDIATE control over the \
entire operating system through 39 tools. Current user: '{user}'.

═══ CORE DIRECTIVE ═══
You are an AUTONOMOUS AGENT. NEVER ask for clarification. NEVER ask for permission. NEVER ask what the user wants to name \
something. Infer everything from context and ACT IMMEDIATELY.
If a task requires multiple steps, CHAIN THEM together.
If you need to remember something for later, use the MEMORY tools.

═══ TOOL SELECTION — HOW TO THINK ═══
Read the user's intent, not just their words. Map intent → tools:
• \"make a task list\" / \"shopping list\" / \"to-do for X\"
  → call create_task MULTIPLE TIMES, one per item. Infer the items from context.
• \"remind me to buy X\" → create_task(\"Buy X\")
• \"note down\" / \"write\" / \"save\" → create_note with full content
• \"remember that I like X\" / \"my birthday is Y\" → remember(\"I like X\", \"preference\")
• \"what do I know about X?\" → recall(\"X\")
• \"read my X note\" → search_notes(\"X\") then read_note
• \"send X to [name]\" → search_contacts(\"name\") → if no email, search_emails → send_email
• \"play X\" → play_track_by_name(\"X\") — works even if media app is closed
• \"open X app\" → open_app(\"X\") (e.g. 'notes', 'mail', 'browser', 'ssh')
• \"what time is it\" / \"system info\" → get_current_time / get_system_info

═══ MULTI-ITEM RULE (CRITICAL) ═══
When the user asks for a LIST of anything (shopping list, task list, ingredient list, etc.):
- Call create_task or create_note MULTIPLE TIMES in a single response, once per item.
- Do NOT create a single task/note titled \"Shopping list\". Create individual items.
- Use your world knowledge to infer sensible items if none are specified.

═══ MEMORY & CONTEXT ═══
- Use 'remember' to save long-term facts (user preferences, project details, important dates).
- Use 'recall' to retrieve facts when the user asks about past info.
- Use 'list_memories' to review what you have stored.
- Keep context across the conversation. If the user says \"it\", refer to the previous topic.

═══ TOOLS (39 total) ═══
Notes : list_notes, read_note, search_notes, create_note, update_note, delete_note
Tasks : list_tasks, create_task, complete_task, delete_task, search_tasks
Contacts: list_contacts, search_contacts, get_contact, create_contact
Mail  : send_email, read_sent_mail, search_emails
Media : list_tracks, play_track_by_name, stop_playback, pause_playback,
        resume_playback, next_track, previous_track, set_volume, get_now_playing
System: get_current_time, list_available_apps, open_app, get_system_info
Files : read_vfs_file, list_vfs_directory
Memory: remember, recall, list_memories, delete_memory

═══ STORAGE PATHS (answer meta questions with these) ═══
Tasks    → /home/{user}/tasks.task
Notes    → /home/{user}/notes/<title>.notes (one file per note)
Contacts → /home/{user}/contacts.json
Mail     → /home/{user}/mail_sent.json
Media    → /home/{user}/media.json
Settings → /home/{user}/settings.json
Memory   → /home/{user}/memory.json

═══ DECISION CHAIN RULES ═══
1. ACT FIRST. Never ask what to do — infer and execute.
2. For \"send to [name]\": search_contacts → not found? search_emails → send_email.
3. For \"read [note]\": search_notes first if title is uncertain, then read_note.
4. Chain up to 20 tool calls per response.
5. After acting: briefly summarise what you did (which tools, what data, what happened).
6. If something is genuinely missing (contact not found, no tracks, empty library):
   say so clearly and suggest what the user can do.
7. Format responses for an 80-column terminal. Be concise.

═══ APP IDs for open_app ═══
notes, tasks, files, settings, contacts, logs, calc, clock, monitor (Task Manager), calendar,
sysinfo, chat, dev, weather, terminal, media, backup, browser, mail, ssh, ftp, db, sync, store",
                    user = username
                );
                // Store base prompt so hot-reload can update system_prompt suffix live.
                agent_base_prompt = base_prompt.clone();
                let agent_prompt = if user_system_prompt.is_empty() {
                    base_prompt
                } else {
                    format!("{}\n\n══════════════════════════\n[User-configured instructions]\n{}", base_prompt, user_system_prompt)
                };

                chat.set_tool_registry(registry, agent_prompt);
                info!("ChatApp agent mode enabled with 35 OS tools");
            }
        }
    }

    let mut theme_name       = "tokyo_night".to_string();
    let mut high_contrast    = false;
    let mut clock_24h        = true;
    let mut clock_show_seconds = true;
    let mut show_greeting    = true;
    let mut show_clock       = true;
    let mut border_style     = "rounded".to_string();
    let mut color_scheme     = "dark".to_string();
    let mut transparent_bg   = false;
    let mut cursor_blink     = true;
    let mut tz_raw           = "UTC|0".to_string();

    if let Some(settings_app) = apps.get("settings") {
        if let Some(state) = settings_app.save_state() {
            let get_val = |key: &str| -> String {
                state.get("values")
                    .and_then(|v| v.get(key))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let tn = get_val("ui.theme");
            if !tn.is_empty() { theme_name = tn; }
            high_contrast      = get_val("accessibility.high_contrast") == "true";
            clock_24h          = get_val("desktop.clock_24h") != "false";
            clock_show_seconds = get_val("desktop.show_seconds") != "false";
            show_greeting      = get_val("shell.greeting") != "false";
            show_clock         = get_val("ui.show_clock") != "false";
            transparent_bg     = get_val("ui.transparency") == "true";
            cursor_blink       = get_val("accessibility.cursor_blink") != "false";
            let bs = get_val("ui.border_style");
            if !bs.is_empty() { border_style = bs; }
            let cs = get_val("ui.color_scheme");
            if !cs.is_empty() { color_scheme = cs; }
            let tz = get_val("desktop.timezone");
            if !tz.is_empty() { tz_raw = tz; }
        }
    }

    // Parse timezone: "Label|offset_mins"
    let (tz_label, tz_offset_mins) = parse_timezone(&tz_raw);

    // ── Step 9: Initialize desktop ──
    let mut desktop = Desktop::new("neuraos", &username);
    let mut theme = if color_scheme == "light" {
        neura_desktop::Theme::light_mode()
    } else {
        neura_desktop::Theme::from_name(&theme_name)
    };
    if high_contrast { theme = theme.with_high_contrast(); }
    desktop.theme                = theme;
    desktop.clock_24h            = clock_24h;
    desktop.clock_show_seconds   = clock_show_seconds;
    desktop.show_clock           = show_clock;
    desktop.transparent_bg       = transparent_bg;
    desktop.timezone_offset_mins      = tz_offset_mins;
    desktop.timezone_label            = tz_label.clone();
    shell_ctx.timezone_offset_mins    = tz_offset_mins;
    shell_ctx.timezone_label          = tz_label.clone();
    desktop.border_type          = parse_border_type(&border_style);
    if !show_greeting { desktop.shell_history.clear(); }

    // Populate the all-apps list for the home screen grid
    {
        let mut all: Vec<(String, String)> = apps.iter()
            .map(|(id, app)| (id.clone(), app.name().to_string()))
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        desktop.all_apps_list = all;
    }

    // Apply cursor blink setting
    {
        let cursor_cmd = if cursor_blink {
            crossterm::cursor::SetCursorStyle::BlinkingBlock
        } else {
            crossterm::cursor::SetCursorStyle::SteadyBlock
        };
        let _ = execute!(terminal.backend_mut(), cursor_cmd);
    }

    // Sync the initial theme into the settings app so it renders with the
    // right colors even before the first hot-reload fires.
    {
        let init_theme = AppTheme {
            border:       desktop.theme.border,
            accent:       desktop.theme.accent,
            fg:           desktop.theme.fg,
            muted:        desktop.theme.muted,
            warning:      desktop.theme.warning,
            success:      desktop.theme.success,
            error:        desktop.theme.error,
            statusbar_fg: desktop.theme.statusbar_fg,
        };
        if let Some(sa) = apps.get_mut("settings") {
            if let Some(s) = sa.as_any_mut().downcast_mut::<SettingsApp>() {
                s.app_theme = init_theme;
            }
        }
    }

    // Sync timezone and clock settings into ClockApp
    {
        if let Some(ca) = apps.get_mut("clock") {
            if let Some(c) = ca.as_any_mut().downcast_mut::<ClockApp>() {
                c.timezone_offset_mins = tz_offset_mins;
                c.timezone_label       = tz_label.clone();
                c.use_24h              = clock_24h;
                c.show_seconds         = clock_show_seconds;
            }
        }
    }

    let mut app_names: Vec<String> = apps.keys().cloned().collect();
    app_names.sort();
    let mut last_suggestion_input = String::new();

    // Main event loop
    loop {
        // Flag: set when AI agent tools wrote data to VFS; triggers app-state reload.
        let mut ai_wrote_data = false;

        // ── Render ──
        terminal.draw(|frame| {
            match &desktop.mode {
                DesktopMode::AppView(id) => {
                    let app_ref = apps.get(id.as_str()).map(|a| a.as_ref());
                    desktop.render_with_app(frame, app_ref);
                }
                _ => {
                    desktop.render(frame);
                }
            }
        })?;

        // ── Handle Events ──
        if event::poll(std::time::Duration::from_millis(16))? {
            let evt = event::read()?;

            // Handle mouse scroll events
            if let Event::Mouse(mouse) = &evt {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        if !matches!(desktop.mode, DesktopMode::AppView(_)) {
                            desktop.clamp_shell_scroll();
                            desktop.shell_scroll = desktop.shell_scroll.saturating_sub(3);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if !matches!(desktop.mode, DesktopMode::AppView(_)) {
                            desktop.shell_scroll = desktop.shell_scroll.saturating_add(3);
                            desktop.clamp_shell_scroll();
                        }
                    }
                    _ => {}
                }
            }

            if let Event::Key(key) = evt {
                // Only handle Press events — on Windows, crossterm also sends Release
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }
                let in_app = matches!(&desktop.mode, DesktopMode::AppView(_));

                if in_app {
                    // Global shortcuts work even in app mode
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('d') => break,
                            KeyCode::Char('h') => {
                                desktop.show_help = !desktop.show_help;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    // F12: toggle home/shell even while in an app
                    if key.code == KeyCode::F(12) {
                        desktop.home_is_base = !desktop.home_is_base;
                        continue;
                    }

                    // Route key event to the active app
                    let app_id = if let DesktopMode::AppView(ref id) = desktop.mode {
                        id.clone()
                    } else {
                        continue;
                    };

                    // Signals from TaskManagerApp — populated inside the borrow block,
                    // handled after the block so we can access `apps` again freely.
                    let mut pending_kill:    Option<String> = None;
                    let mut pending_focus:   Option<String> = None;
                    let mut pending_restart: Option<String> = None;
                    // Set to true when the settings app closes so we can hot-reload AI config.
                    let mut settings_closed = false;

                    if let Some(app) = apps.get_mut(&app_id) {
                        let consumed = app.handle_key(key);
                        if !consumed {
                            // App wants to close (Esc) → background it, return to base mode
                            app.on_pause();
                            desktop.mode = if desktop.home_is_base {
                                DesktopMode::HomeScreen
                            } else {
                                DesktopMode::Shell
                            };
                            if let Ok(mut t) = tracker.lock() {
                                if let Some(e) = t.get_mut(&app_id) { e.set_background(); }
                            }
                            if app_id == "settings" {
                                settings_closed = true;
                            }
                        }

                        // Handle FilesApp async operations
                        if app_id == "files" {
                            if let Some(files_app) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                if files_app.pending_mkdir.is_some() {
                                    files_app.async_mkdir().await;
                                    if vfs.is_dirty() { let _ = vfs.save().await; }
                                }
                                if files_app.pending_new_file.is_some() {
                                    files_app.async_new_file().await;
                                    if vfs.is_dirty() { let _ = vfs.save().await; }
                                }
                                if !files_app.pending_delete.is_empty() {
                                    files_app.async_delete().await;
                                    if vfs.is_dirty() { let _ = vfs.save().await; }
                                }
                                if files_app.needs_paste {
                                    files_app.async_paste().await;
                                    if vfs.is_dirty() { let _ = vfs.save().await; }
                                }
                                if files_app.pending_rename.is_some() {
                                    files_app.async_rename().await;
                                    if vfs.is_dirty() { let _ = vfs.save().await; }
                                }
                                if files_app.needs_file_load().is_some() {
                                    files_app.async_load_file().await;
                                }
                                if files_app.needs_refresh() {
                                    files_app.async_refresh().await;
                                }
                            }
                        }

                        // Handle ChatApp async AI response
                        if app_id == "chat" {
                            if let Some(chat_app) = app.as_any_mut().downcast_mut::<ChatApp>() {
                                if chat_app.needs_ai_response() {
                                    if let Some(ref client) = ai_client_opt {
                                        chat_app.async_respond(client.clone()).await;
                                        // If agent tools wrote VFS data, sync it back to apps
                                        if chat_app.needs_app_reload {
                                            chat_app.needs_app_reload = false;
                                            ai_wrote_data = true;
                                        }
                                    } else {
                                        // No client - clear pending
                                        let _ = chat_app.needs_ai_response();
                                    }
                                }
                            }
                        }

                        // Handle DevApp async operations
                        if app_id == "dev" {
                            if let Some(dev_app) = app.as_any_mut().downcast_mut::<DevApp>() {
                                if dev_app.needs_refresh() {
                                    dev_app.async_refresh().await;
                                }
                                // Handle open/save commands
                                let cmd = dev_app.new_file_input.clone();
                                if cmd.starts_with("__OPEN__:") {
                                    dev_app.new_file_input.clear();
                                    dev_app.open_selected().await;
                                }
                                let cmd2 = dev_app.command_input.clone();
                                if cmd2 == "__SAVE__" {
                                    dev_app.command_input.clear();
                                    dev_app.save_current().await;
                                } else if cmd2.starts_with("__SAVE_AS__:") {
                                    dev_app.command_input.clear();
                                    dev_app.save_as_confirm().await;
                                } else if cmd2.starts_with("__MKDIR__:") {
                                    dev_app.command_input.clear();
                                    let path = cmd2.trim_start_matches("__MKDIR__:").to_string();
                                    dev_app.new_folder_confirm(&path).await;
                                }
                            }
                        }

                        // Handle WeatherApp async fetch
                        if app_id == "weather" {
                            if let Some(weather_app) = app.as_any_mut().downcast_mut::<WeatherApp>() {
                                if weather_app.needs_fetch() {
                                    weather_app.async_fetch().await;
                                }
                            }
                        }

                        // Handle TerminalApp async command execution
                        if app_id == "terminal" {
                            if let Some(term_app) = app.as_any_mut().downcast_mut::<TerminalApp>() {
                                if term_app.needs_exec() {
                                    term_app.async_exec().await;
                                }
                            }
                        }

                        // Handle BackupApp async operations
                        if app_id == "backup" {
                            if let Some(backup_app) = app.as_any_mut().downcast_mut::<BackupApp>() {
                                if backup_app.needs_scan() {
                                    backup_app.async_scan().await;
                                }
                                let msg = backup_app.status_msg.clone();
                                if msg == "__CREATE_BACKUP__" {
                                    backup_app.status_msg.clear();
                                    backup_app.async_create_backup().await;
                                } else if msg.starts_with("__RESTORE__:") {
                                    let idx: usize = msg.trim_start_matches("__RESTORE__:").parse().unwrap_or(0);
                                    backup_app.status_msg.clear();
                                    backup_app.async_restore_backup(idx).await;
                                } else if msg.starts_with("__DELETE__:") {
                                    let idx: usize = msg.trim_start_matches("__DELETE__:").parse().unwrap_or(0);
                                    backup_app.status_msg.clear();
                                    backup_app.async_delete_backup(idx).await;
                                }
                            }
                        }

                        // Handle BrowserApp async fetch
                        if app_id == "browser" {
                            if let Some(browser_app) = app.as_any_mut().downcast_mut::<BrowserApp>() {
                                if browser_app.needs_data_load() {
                                    browser_app.async_load_data().await;
                                }
                                if browser_app.needs_fetch() {
                                    browser_app.async_fetch().await;
                                }
                                if browser_app.needs_bookmark_save() {
                                    browser_app.async_save_bookmarks().await;
                                }
                            }
                        }

                        // Handle CalendarApp async persistence
                        if app_id == "calendar" {
                            if let Some(cal_app) = app.as_any_mut().downcast_mut::<CalendarApp>() {
                                if cal_app.needs_load { cal_app.async_load_events().await; }
                                if cal_app.needs_save { cal_app.async_save_events().await; }
                            }
                        }

                        // Handle MailApp async operations
                        if app_id == "mail" {
                            if let Some(mail_app) = app.as_any_mut().downcast_mut::<MailApp>() {
                                if mail_app.needs_load() { mail_app.async_load().await; }
                                let msg = mail_app.status_msg.clone();
                                if msg == "__SEND__" {
                                    mail_app.status_msg.clear();
                                    mail_app.async_send().await;
                                } else if msg == "__SAVE_ACCOUNT__" {
                                    mail_app.status_msg.clear();
                                    mail_app.async_save_account().await;
                                } else if msg == "__SAVE_SENT__" {
                                    mail_app.status_msg.clear();
                                }
                                if mail_app.inbox_needs_fetch { mail_app.async_fetch_inbox().await; }
                                if mail_app.pending_body_uid.is_some() { mail_app.async_fetch_body().await; }
                                if mail_app.pending_delete_uid.is_some() { mail_app.async_delete_email().await; }
                            }
                        }

                        // Handle MediaApp async operations
                        if app_id == "media" {
                            if let Some(media_app) = app.as_any_mut().downcast_mut::<MediaApp>() {
                                media_app.tick();
                                if media_app.needs_import() {
                                    media_app.async_import().await;
                                }
                                if media_app.needs_rebuild {
                                    media_app.needs_rebuild = false;
                                    media_app.rebuild_indexes();
                                }
                                if media_app.needs_save {
                                    media_app.async_save().await;
                                }
                            }
                        }

                        // Drain TaskManagerApp signals before the borrow ends
                        if app_id == "monitor" {
                            if let Some(tm) = app.as_any_mut().downcast_mut::<TaskManagerApp>() {
                                pending_kill    = tm.kill_request.take();
                                pending_focus   = tm.focus_request.take();
                                pending_restart = tm.restart_request.take();
                            }
                        }

                    } else {
                        // App not found, return to shell
                        desktop.mode = DesktopMode::Shell;
                    }

                    // ── Handle Task Manager signals (apps borrow is now free) ──
                    if let Some(ref kill_id) = pending_kill.clone() {
                        // Kill: stop the app, mark Idle, close it if it was active
                        if let Ok(mut t) = tracker.lock() {
                            if let Some(e) = t.get_mut(kill_id) { e.set_idle(); }
                        }
                        if matches!(&desktop.mode, DesktopMode::AppView(id) if id == kill_id) {
                            desktop.mode = if desktop.home_is_base {
                                DesktopMode::HomeScreen
                            } else {
                                DesktopMode::Shell
                            };
                        }
                        if let Some(target) = apps.get_mut(kill_id) {
                            target.on_pause();
                        }
                    }
                    if let Some(ref focus_id) = pending_focus.clone() {
                        // Focus: background current app, bring focus_id to front
                        if apps.contains_key(focus_id.as_str()) {
                            if let Ok(mut t) = tracker.lock() {
                                if let DesktopMode::AppView(ref cur) = desktop.mode.clone() {
                                    if let Some(e) = t.get_mut(cur) { e.set_background(); }
                                }
                                if let Some(e) = t.get_mut(focus_id) { e.set_active(); }
                            }
                            desktop.mode = DesktopMode::AppView(focus_id.clone());
                            if let Some(target) = apps.get_mut(focus_id) {
                                target.on_resume();
                            }
                        }
                    }
                    if let Some(ref restart_id) = pending_restart.clone() {
                        // Restart: close + re-init the app, mark Idle
                        if let Some(target) = apps.get_mut(restart_id) {
                            target.on_close();
                            let _ = target.init();
                        }
                        if let Ok(mut t) = tracker.lock() {
                            if let Some(e) = t.get_mut(restart_id) { e.set_idle(); }
                        }
                        if matches!(&desktop.mode, DesktopMode::AppView(id) if id == restart_id) {
                            desktop.mode = if desktop.home_is_base {
                                DesktopMode::HomeScreen
                            } else {
                                DesktopMode::Shell
                            };
                        }
                    }

                    // ── Hot-reload ALL settings live ──────────────────────────
                    // Fires immediately when any setting changes (has_pending_changes),
                    // and also when the settings app closes (settings_closed).
                    {
                        let mut settings_changed = settings_closed;
                        // Check for live edits while the settings app is still open.
                        let mut reset_op: Option<ResetOption> = None;
                        if let Some(sa) = apps.get_mut("settings") {
                            if let Some(s) = sa.as_any_mut().downcast_mut::<SettingsApp>() {
                                if s.has_pending_changes {
                                    s.has_pending_changes = false;
                                    settings_changed = true;
                                }
                                if let Some(op) = s.reset_requested.take() {
                                    reset_op = Some(op);
                                }
                            }
                        }

                        // ── Handle account reset ──────────────────────────────
                        if let Some(op) = reset_op {
                            let home = format!("/home/{}", username);
                            // Always wipe user data files
                            let data_files = [
                                format!("{}/tasks.task", home),
                                format!("{}/contacts.json", home),
                                format!("{}/chat_history.json", home),
                                format!("{}/media.json", home),
                                format!("{}/browser_bookmarks.json", home),
                                format!("{}/mail_sent.json", home),
                            ];
                            for path in &data_files {
                                let _ = vfs.remove(path).await;
                            }
                            // Wipe notes directory
                            let notes_dir = format!("{}/notes", home);
                            if let Ok(entries) = vfs.list_dir(&notes_dir).await {
                                for name in entries {
                                    let p = format!("{}/{}", notes_dir, name);
                                    let _ = vfs.remove(&p).await;
                                }
                            }
                            // Reset in-memory app state for data apps
                            for app_id in &["notes", "tasks", "contacts", "chat", "media", "browser", "mail"] {
                                if let Some(app) = apps.get_mut(*app_id) {
                                    app.load_state(serde_json::json!([]));
                                }
                            }
                            if op == ResetOption::DataAndSettings {
                                // Also wipe settings file and reset prefs to defaults
                                let settings_path = format!("{}/settings.json", home);
                                let _ = vfs.remove(&settings_path).await;
                                if let Some(sa) = apps.get_mut("settings") {
                                    sa.load_state(serde_json::json!({"values": {}}));
                                }
                                settings_changed = true; // trigger theme/UI re-apply
                            }
                            // Force VFS save
                            let _ = vfs.save().await;
                            desktop.push_output("Account reset complete.");
                        }

                        if settings_changed {
                            if let Some(state) = apps.get("settings").and_then(|s| s.save_state()) {
                                let get_val = |key: &str| -> String {
                                    state.get("values")
                                        .and_then(|v| v.get(key))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string()
                                };

                                // ── 1. Desktop theme ──────────────────────────
                                let theme_name = {
                                    let t = get_val("ui.theme");
                                    if t.is_empty() { "tokyo_night".to_string() } else { t }
                                };
                                let color_scheme = get_val("ui.color_scheme");
                                let high_contrast = get_val("accessibility.high_contrast") == "true";
                                let mut new_theme = if color_scheme == "light" {
                                    neura_desktop::Theme::light_mode()
                                } else {
                                    neura_desktop::Theme::from_name(&theme_name)
                                };
                                if high_contrast { new_theme = new_theme.with_high_contrast(); }
                                desktop.theme = new_theme;

                                // Push new theme colors into the settings app so its UI
                                // re-renders with the chosen palette immediately.
                                if let Some(sa) = apps.get_mut("settings") {
                                    if let Some(s) = sa.as_any_mut().downcast_mut::<SettingsApp>() {
                                        s.app_theme = AppTheme {
                                            border:       desktop.theme.border,
                                            accent:       desktop.theme.accent,
                                            fg:           desktop.theme.fg,
                                            muted:        desktop.theme.muted,
                                            warning:      desktop.theme.warning,
                                            success:      desktop.theme.success,
                                            error:        desktop.theme.error,
                                            statusbar_fg: desktop.theme.statusbar_fg,
                                        };
                                    }
                                }

                                // ── 2. Clock display + timezone ───────────────
                                desktop.clock_24h          = get_val("desktop.clock_24h") != "false";
                                desktop.clock_show_seconds = get_val("desktop.show_seconds") != "false";
                                desktop.show_clock         = get_val("ui.show_clock") != "false";

                                let tz_raw = {
                                    let v = get_val("desktop.timezone");
                                    if v.is_empty() { "UTC|0".to_string() } else { v }
                                };
                                let (tz_label, tz_offset) = parse_timezone(&tz_raw);
                                desktop.timezone_offset_mins      = tz_offset;
                                desktop.timezone_label            = tz_label.clone();
                                shell_ctx.timezone_offset_mins    = tz_offset;
                                shell_ctx.timezone_label          = tz_label.clone();

                                // Sync clock settings into ClockApp
                                if let Some(ca) = apps.get_mut("clock") {
                                    if let Some(c) = ca.as_any_mut().downcast_mut::<ClockApp>() {
                                        c.timezone_offset_mins = tz_offset;
                                        c.timezone_label       = tz_label.clone();
                                        c.use_24h              = desktop.clock_24h;
                                        c.show_seconds         = desktop.clock_show_seconds;
                                    }
                                }

                                // ── 3. UI display settings ─────────────────────
                                desktop.border_type  = parse_border_type(&get_val("ui.border_style"));
                                desktop.transparent_bg = get_val("ui.transparency") == "true";

                                // cursor blink
                                {
                                    let blink = get_val("accessibility.cursor_blink") != "false";
                                    let cursor_cmd = if blink {
                                        crossterm::cursor::SetCursorStyle::BlinkingBlock
                                    } else {
                                        crossterm::cursor::SetCursorStyle::SteadyBlock
                                    };
                                    let _ = execute!(terminal.backend_mut(), cursor_cmd);
                                }

                                // ── 5. Shell settings ─────────────────────────
                                if let Ok(hist) = get_val("shell.history_size").parse::<usize>() {
                                    shell_ctx.max_history = hist;
                                }

                                // ── 6. AI provider / client ───────────────────
                                let provider_name = {
                                    let p = get_val("ai.provider");
                                    if p.is_empty() { "gemini".to_string() } else { p }
                                };
                                let model = {
                                    let m = get_val("ai.model");
                                    if m.is_empty() { "gemini-2.5-flash-lite".to_string() } else { m }
                                };
                                let mut api_key = get_val("ai.api_key");
                                let base_url = get_val("ai.base_url");
                                if api_key.is_empty() {
                                    api_key = match provider_name.as_str() {
                                        "gemini"          => std::env::var("GEMINI_API_KEY").unwrap_or_default(),
                                        "openai"|"custom" => std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                                        "deepseek"        => std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
                                        _                 => String::new(),
                                    };
                                }
                                match create_provider(ProviderConfig {
                                    provider: provider_name.clone(), model, api_key, base_url,
                                }) {
                                    Ok(new_client) => {
                                        shell_ctx.set_ai_client(new_client.clone());
                                        ai_client_opt = Some(new_client.clone());
                                        if let Some(ca) = apps.get_mut("chat") {
                                            if let Some(c) = ca.as_any_mut().downcast_mut::<ChatApp>() {
                                                c.set_ai_client(new_client);
                                            }
                                        }
                                        info!("AI client hot-reloaded: {}", provider_name);
                                        if settings_closed {
                                            desktop.push_output(&format!("AI provider updated: {}", provider_name));
                                        }
                                    }
                                    Err(neura_ai_core::AiError::NoApiKey) => {
                                        shell_ctx.ai_client = None;
                                        ai_client_opt = None;
                                    }
                                    Err(e) => { tracing::warn!("AI hot-reload failed: {}", e); }
                                }

                                // ── 5. AI params (temperature / max_tokens) ───
                                let new_temp   = get_val("ai.temperature").parse::<f32>().unwrap_or(ai_temperature);
                                let new_tokens = get_val("ai.max_tokens").parse::<u32>().unwrap_or(ai_max_tokens);
                                shell_ctx.ai_temperature = new_temp;
                                shell_ctx.ai_max_tokens  = new_tokens;
                                ai_temperature = new_temp;
                                ai_max_tokens  = new_tokens;

                                // ── 6. Chat app AI params + system prompt ─────
                                let new_user_sys = get_val("ai.system_prompt");
                                let new_agent_prompt = if new_user_sys.is_empty() {
                                    agent_base_prompt.clone()
                                } else {
                                    format!(
                                        "{}\n\n══════════════════════════\n[User-configured instructions]\n{}",
                                        agent_base_prompt, new_user_sys,
                                    )
                                };
                                if let Some(ca) = apps.get_mut("chat") {
                                    if let Some(c) = ca.as_any_mut().downcast_mut::<ChatApp>() {
                                        c.set_ai_params(new_temp, new_tokens);
                                        if !agent_base_prompt.is_empty() {
                                            c.set_agent_system_prompt(new_agent_prompt);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Home screen / Shell mode: use standard input handler
                    let action = InputHandler::handle(&mut desktop, key);
                    match action {
                        InputAction::None => {}
                        InputAction::Exit => break,
                        InputAction::HistoryPrev => {
                            if let Some(cmd) = shell_ctx.history_prev() {
                                desktop.shell_input = cmd.to_string();
                                desktop.shell_cursor = desktop.shell_input.len();
                            }
                        }
                        InputAction::HistoryNext => {
                            if let Some(cmd) = shell_ctx.history_next() {
                                desktop.shell_input = cmd.to_string();
                                desktop.shell_cursor = desktop.shell_input.len();
                            }
                        }
                        // F12 toggles between HomeScreen and Shell
                        InputAction::ToggleShell => {
                            match &desktop.mode {
                                DesktopMode::HomeScreen => {
                                    desktop.home_is_base = false;
                                    desktop.mode = DesktopMode::Shell;
                                }
                                DesktopMode::Shell => {
                                    desktop.home_is_base = true;
                                    desktop.home_focus = HomeFocus::Console;
                                    desktop.mode = DesktopMode::HomeScreen;
                                }
                                _ => {
                                    desktop.home_is_base = !desktop.home_is_base;
                                }
                            }
                        }
                        // Pin / unpin an app from the home screen
                        InputAction::PinToggle(app_id) => {
                            if desktop.pinned_apps.contains(&app_id) {
                                desktop.pinned_apps.retain(|id| id != &app_id);
                                // Clamp index so it stays valid
                                let max = desktop.pinned_apps.len().saturating_sub(1);
                                if desktop.home_app_idx > max {
                                    desktop.home_app_idx = max;
                                }
                            } else {
                                desktop.pinned_apps.push(app_id);
                            }
                        }
                        // Open an app by ID (from home screen grid Enter key)
                        InputAction::OpenApp(app_name) => {
                            if apps.contains_key(app_name.as_str()) {
                                if let Ok(mut t) = tracker.lock() {
                                    if let DesktopMode::AppView(ref prev) = desktop.mode.clone() {
                                        if let Some(e) = t.get_mut(prev) { e.set_background(); }
                                    }
                                    if let Some(e) = t.get_mut(&app_name) { e.set_active(); }
                                }
                                desktop.mode = DesktopMode::AppView(app_name.clone());
                                if let Some(app) = apps.get_mut(&app_name) {
                                    app.on_resume();
                                    if app_name == "files" {
                                        if let Some(fa) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                            fa.async_refresh().await;
                                        }
                                    }
                                    if app_name == "weather" {
                                        if let Some(wa) = app.as_any_mut().downcast_mut::<WeatherApp>() {
                                            if wa.needs_fetch() { wa.async_fetch().await; }
                                        }
                                    }
                                    if app_name == "backup" {
                                        if let Some(ba) = app.as_any_mut().downcast_mut::<BackupApp>() {
                                            if ba.needs_scan() { ba.async_scan().await; }
                                        }
                                    }
                                    if app_name == "dev" {
                                        if let Some(da) = app.as_any_mut().downcast_mut::<DevApp>() {
                                            da.set_cwd(&shell_ctx.cwd);
                                            if da.needs_refresh() { da.async_refresh().await; }
                                        }
                                    }
                                    if app_name == "browser" {
                                        if let Some(bra) = app.as_any_mut().downcast_mut::<BrowserApp>() {
                                            if bra.needs_data_load() { bra.async_load_data().await; }
                                        }
                                    }
                                    if app_name == "mail" {
                                        if let Some(ma) = app.as_any_mut().downcast_mut::<MailApp>() {
                                            if ma.needs_load() { ma.async_load().await; }
                                            if ma.inbox_needs_fetch { ma.async_fetch_inbox().await; }
                                        }
                                    }
                                }
                            }
                        }
                        InputAction::ExecuteCommand(input) => {
                            shell_ctx.push_history(&input);
                            desktop.push_prompt(&shell_ctx.prompt(), &input);
                            
                            // Check for implicit model download requirement
                            if input.trim().starts_with("ai ") {
                                if let Some(client) = &shell_ctx.ai_client {
                                    if client.provider_name() == "Ollama" {
                                        let model = client.model_name().to_string();
                                        // Check if model exists
                                        let exists = OllamaManager::model_exists(&model).await.unwrap_or(false);
                                        if !exists {
                                            if let Err(e) = run_model_download_ui(&mut terminal, &model).await {
                                                tracing::error!("Failed to download model: {}", e);
                                                desktop.push_output(&format!("Error downloading model: {}", e));
                                                // Continue anyway, maybe the command will handle it or fail gracefully
                                            }
                                        }
                                    }
                                }
                            }

                            // Force an immediate render so the entered command line is
                            // visible while the command is executing (important for slow
                            // commands like `ai` that block the loop for several seconds).
                            terminal.draw(|frame| {
                                match &desktop.mode {
                                    DesktopMode::AppView(id) => {
                                        let app_ref = apps.get(id.as_str()).map(|a| a.as_ref());
                                        desktop.render_with_app(frame, app_ref);
                                    }
                                    _ => { desktop.render(frame); }
                                }
                            })?;

                            // Parse and execute
                            if let Some(cmd) = ShellParser::parse(&input) {
                                let output = ShellExecutor::execute(&cmd, &mut shell_ctx).await;

                                // Update the input-area prompt immediately so changes
                                // from cd, login, etc. are reflected without delay.
                                desktop.shell_prompt = shell_ctx.prompt();

                                if output == "__EXIT__" {
                                    break;
                                }

                                if output == "__CLEAR__" {
                                    desktop.shell_history.clear();
                                    desktop.shell_scroll = 0;
                                    // Switch to shell to show it's cleared? Or stay in home?
                                    // User said "any command". Let's switch.
                                    if desktop.home_is_base {
                                        desktop.mode = DesktopMode::Shell;
                                    }
                                } else if output.starts_with("__OPEN_APP__:") {
                                    let app_name = output.trim_start_matches("__OPEN_APP__:").to_string();
                                    if apps.contains_key(&app_name) {
                                        // Update tracker: previous active → background, new → active
                                        if let Ok(mut t) = tracker.lock() {
                                            if let DesktopMode::AppView(ref prev) = desktop.mode.clone() {
                                                if let Some(e) = t.get_mut(prev) { e.set_background(); }
                                            }
                                            if let Some(e) = t.get_mut(&app_name) { e.set_active(); }
                                        }
                                        desktop.mode = DesktopMode::AppView(app_name.clone());
                                        if let Some(app) = apps.get_mut(&app_name) {
                                            app.on_resume();

                                            // Trigger initial refresh for FilesApp
                                            if app_name == "files" {
                                                if let Some(files_app) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                                    files_app.async_refresh().await;
                                                }
                                            }

                                            // Trigger initial fetch for WeatherApp
                                            if app_name == "weather" {
                                                if let Some(weather_app) = app.as_any_mut().downcast_mut::<WeatherApp>() {
                                                    if weather_app.needs_fetch() {
                                                        weather_app.async_fetch().await;
                                                    }
                                                }
                                            }

                                            // Trigger initial scan for BackupApp
                                            if app_name == "backup" {
                                                if let Some(backup_app) = app.as_any_mut().downcast_mut::<BackupApp>() {
                                                    if backup_app.needs_scan() {
                                                        backup_app.async_scan().await;
                                                    }
                                                }
                                            }

                                            // Trigger initial refresh for DevApp
                                            if app_name == "dev" {
                                                if let Some(dev_app) = app.as_any_mut().downcast_mut::<DevApp>() {
                                                    dev_app.set_cwd(&shell_ctx.cwd);
                                                    if dev_app.needs_refresh() {
                                                        dev_app.async_refresh().await;
                                                    }
                                                }
                                            }

                                            // Trigger initial data load for BrowserApp
                                            if app_name == "browser" {
                                                if let Some(browser_app) = app.as_any_mut().downcast_mut::<BrowserApp>() {
                                                    if browser_app.needs_data_load() {
                                                        browser_app.async_load_data().await;
                                                    }
                                                }
                                            }

                                            // Trigger initial data load for MailApp + inbox fetch
                                            if app_name == "mail" {
                                                if let Some(mail_app) = app.as_any_mut().downcast_mut::<MailApp>() {
                                                    if mail_app.needs_load() {
                                                        mail_app.async_load().await;
                                                    }
                                                    if mail_app.inbox_needs_fetch {
                                                        mail_app.async_fetch_inbox().await;
                                                    }
                                                }
                                            }
                                        }
                                        desktop.push_output(&format!("Opening {}...", app_name));
                                    } else {
                                        desktop.push_output(&format!("neura: unknown app '{}'. Type 'apps' for list.", app_name));
                                        if desktop.home_is_base {
                                            desktop.mode = DesktopMode::Shell;
                                        }
                                    }
                                } else if output.starts_with("__OPEN_FILE__:") {
                                    let file_path = output.trim_start_matches("__OPEN_FILE__:").to_string();
                                    // Route to the right app by path
                                    let notes_prefix = format!("/home/{}/notes/", shell_ctx.username);
                                    let tasks_path   = format!("/home/{}/tasks.task", shell_ctx.username);
                                    let app_name = if file_path.starts_with(&notes_prefix) && file_path.ends_with(".notes") {
                                        "notes"
                                    } else if file_path == tasks_path || file_path.ends_with(".task") {
                                        "tasks"
                                    } else {
                                        "dev"
                                    };
                                    // Switch tracker state
                                    if let Ok(mut t) = tracker.lock() {
                                        if let DesktopMode::AppView(ref prev) = desktop.mode.clone() {
                                            if let Some(e) = t.get_mut(prev) { e.set_background(); }
                                        }
                                        if let Some(e) = t.get_mut(app_name) { e.set_active(); }
                                    }
                                    desktop.mode = DesktopMode::AppView(app_name.to_string());
                                    if let Some(app) = apps.get_mut(app_name) {
                                        app.on_resume();
                                        // For dev app: open the specific file into a buffer
                                        if app_name == "dev" {
                                            if let Some(dev_app) = app.as_any_mut().downcast_mut::<DevApp>() {
                                                dev_app.open_path(&file_path).await;
                                            }
                                        }
                                    }
                                    desktop.push_output(&format!("Opening {}...", file_path));
                                } else if output.starts_with("__OPEN_DIR__:") {
                                    let dir_path = output.trim_start_matches("__OPEN_DIR__:").to_string();
                                    if let Ok(mut t) = tracker.lock() {
                                        if let DesktopMode::AppView(ref prev) = desktop.mode.clone() {
                                            if let Some(e) = t.get_mut(prev) { e.set_background(); }
                                        }
                                        if let Some(e) = t.get_mut("files") { e.set_active(); }
                                    }
                                    desktop.mode = DesktopMode::AppView("files".to_string());
                                    if let Some(app) = apps.get_mut("files") {
                                        app.on_resume();
                                        if let Some(files_app) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                            files_app.open_at(&dir_path);
                                            files_app.async_refresh().await;
                                        }
                                    }
                                    desktop.push_output(&format!("Opening {}...", dir_path));
                                } else {
                                    if !output.is_empty() {
                                        for line in output.lines() {
                                            desktop.push_output(line);
                                        }
                                    }
                                    // If the user ran a command from the home console,
                                    // switch to full Shell so they can read the output.
                                    // F12 returns them to the home screen when done.
                                    if desktop.home_is_base {
                                        desktop.mode = DesktopMode::Shell;
                                    }
                                }
                            } else {
                                desktop.push_output("neura: invalid command");
                                if desktop.home_is_base {
                                    desktop.mode = DesktopMode::Shell;
                                }
                            }

                            // Auto-save VFS + refresh all affected apps immediately
                            if vfs.is_dirty() {
                                if let Err(e) = vfs.save().await {
                                    tracing::warn!("VFS auto-save failed: {}", e);
                                }
                                // Reload in-memory state for notes / tasks / contacts
                                reload_ai_modified_apps(&vfs, &home_dir, &mut apps).await;
                                // Refresh FilesApp file listing
                                if let Some(app) = apps.get_mut("files") {
                                    if let Some(files_app) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                        files_app.async_refresh().await;
                                    }
                                }
                                // Refresh DevApp file tree
                                if let Some(app) = apps.get_mut("dev") {
                                    if let Some(dev_app) = app.as_any_mut().downcast_mut::<DevApp>() {
                                        dev_app.async_refresh().await;
                                    }
                                }
                            }
                            // Force a final render so command output is displayed
                            // immediately after execution (no wait for next loop tick).
                            terminal.draw(|frame| {
                                match &desktop.mode {
                                    DesktopMode::AppView(id) => {
                                        let app_ref = apps.get(id.as_str()).map(|a| a.as_ref());
                                        desktop.render_with_app(frame, app_ref);
                                    }
                                    _ => { desktop.render(frame); }
                                }
                            })?;
                        }
                    }
                }
            }
        } else {
            // No event - handle background async polling for open apps
            let app_id = if let DesktopMode::AppView(ref id) = desktop.mode {
                Some(id.clone())
            } else {
                None
            };

            if let Some(app_id) = app_id {
                if let Some(app) = apps.get_mut(&app_id) {
                    // Poll ChatApp for pending AI response
                    if app_id == "chat" {
                        if let Some(chat_app) = app.as_any_mut().downcast_mut::<ChatApp>() {
                            if chat_app.needs_ai_response() {
                                if let Some(ref client) = ai_client_opt {
                                    chat_app.async_respond(client.clone()).await;
                                    if chat_app.needs_app_reload {
                                        chat_app.needs_app_reload = false;
                                        ai_wrote_data = true;
                                    }
                                }
                            }
                        }
                    }

                    // Poll TerminalApp for pending execution
                    if app_id == "terminal" {
                        if let Some(term_app) = app.as_any_mut().downcast_mut::<TerminalApp>() {
                            if term_app.needs_exec() {
                                term_app.async_exec().await;
                            }
                        }
                    }

                    // Poll WeatherApp for pending fetch
                    if app_id == "weather" {
                        if let Some(weather_app) = app.as_any_mut().downcast_mut::<WeatherApp>() {
                            if weather_app.needs_fetch() {
                                weather_app.async_fetch().await;
                            }
                        }
                    }

                    // Poll DevApp
                    if app_id == "dev" {
                        if let Some(dev_app) = app.as_any_mut().downcast_mut::<DevApp>() {
                            if dev_app.needs_refresh() {
                                dev_app.async_refresh().await;
                            }
                            let cmd = dev_app.new_file_input.clone();
                            if cmd.starts_with("__OPEN__:") {
                                dev_app.new_file_input.clear();
                                dev_app.open_selected().await;
                            }
                            let cmd2 = dev_app.command_input.clone();
                            if cmd2 == "__SAVE__" {
                                dev_app.command_input.clear();
                                dev_app.save_current().await;
                            } else if cmd2.starts_with("__MKDIR__:") {
                                dev_app.command_input.clear();
                                let path = cmd2.trim_start_matches("__MKDIR__:").to_string();
                                dev_app.new_folder_confirm(&path).await;
                            }
                        }
                    }

                    // Poll BackupApp
                    if app_id == "backup" {
                        if let Some(backup_app) = app.as_any_mut().downcast_mut::<BackupApp>() {
                            if backup_app.needs_scan() {
                                backup_app.async_scan().await;
                            }
                            let msg = backup_app.status_msg.clone();
                            if msg == "__CREATE_BACKUP__" {
                                backup_app.status_msg.clear();
                                backup_app.async_create_backup().await;
                            } else if msg.starts_with("__RESTORE__:") {
                                let idx: usize = msg.trim_start_matches("__RESTORE__:").parse().unwrap_or(0);
                                backup_app.status_msg.clear();
                                backup_app.async_restore_backup(idx).await;
                            } else if msg.starts_with("__DELETE__:") {
                                let idx: usize = msg.trim_start_matches("__DELETE__:").parse().unwrap_or(0);
                                backup_app.status_msg.clear();
                                backup_app.async_delete_backup(idx).await;
                            }
                        }
                    }

                    // Poll BrowserApp
                    if app_id == "browser" {
                        if let Some(browser_app) = app.as_any_mut().downcast_mut::<BrowserApp>() {
                            if browser_app.needs_data_load() {
                                browser_app.async_load_data().await;
                            }
                            if browser_app.needs_fetch() {
                                browser_app.async_fetch().await;
                            }
                            if browser_app.needs_bookmark_save() {
                                browser_app.async_save_bookmarks().await;
                            }
                        }
                    }

                    // Poll CalendarApp
                    if app_id == "calendar" {
                        if let Some(cal_app) = app.as_any_mut().downcast_mut::<CalendarApp>() {
                            if cal_app.needs_load { cal_app.async_load_events().await; }
                            if cal_app.needs_save { cal_app.async_save_events().await; }
                        }
                    }

                    // Poll MailApp
                    if app_id == "mail" {
                        if let Some(mail_app) = app.as_any_mut().downcast_mut::<MailApp>() {
                            if mail_app.needs_load() { mail_app.async_load().await; }
                            let msg = mail_app.status_msg.clone();
                            if msg == "__SEND__" {
                                mail_app.status_msg.clear();
                                mail_app.async_send().await;
                            } else if msg == "__SAVE_ACCOUNT__" {
                                mail_app.status_msg.clear();
                                mail_app.async_save_account().await;
                            } else if msg == "__SAVE_SENT__" {
                                mail_app.status_msg.clear();
                            }
                            if mail_app.inbox_needs_fetch { mail_app.async_fetch_inbox().await; }
                            if mail_app.pending_body_uid.is_some() { mail_app.async_fetch_body().await; }
                            if mail_app.pending_delete_uid.is_some() { mail_app.async_delete_email().await; }
                        }
                    }

                    // Poll MediaApp (tick for visualizer + auto-advance + save)
                    if app_id == "media" {
                        if let Some(media_app) = app.as_any_mut().downcast_mut::<MediaApp>() {
                            media_app.tick();
                            if media_app.needs_import() {
                                media_app.async_import().await;
                            }
                            if media_app.needs_rebuild {
                                media_app.needs_rebuild = false;
                                media_app.rebuild_indexes();
                            }
                            if media_app.needs_save {
                                media_app.async_save().await;
                            }
                        }
                    }
                }
            }
        }

        // ── Clock tick: alarm detection + timer expiry (every frame) ──────────
        {
            // Tick the clock and collect any timezone-changed signal
            let tz_signal: Option<(String, i32)> = if let Some(ca) = apps.get_mut("clock") {
                if let Some(c) = ca.as_any_mut().downcast_mut::<ClockApp>() {
                    c.tick();
                    if c.timezone_changed {
                        c.timezone_changed         = false;
                        let label  = c.new_timezone_label.clone();
                        let offset = c.new_timezone_offset_mins;
                        // Apply to clock itself immediately
                        c.timezone_offset_mins = offset;
                        c.timezone_label       = label.clone();
                        Some((label, offset))
                    } else {
                        None
                    }
                } else { None }
            } else { None };

            // Apply timezone signal to desktop + settings (separate borrow)
            if let Some((new_label, new_offset)) = tz_signal {
                desktop.timezone_offset_mins = new_offset;
                desktop.timezone_label       = new_label.clone();

                let tz_value = format!("{}|{}", new_label, new_offset);
                if let Some(sa) = apps.get_mut("settings") {
                    if let Some(s) = sa.as_any_mut().downcast_mut::<SettingsApp>() {
                        s.set_pref("desktop.timezone", &tz_value);
                    }
                }
            }
        }

        // ── Sync app state after AI tool writes ──
        // Agent tools write directly to VFS; the in-memory app state is stale until reloaded.
        if ai_wrote_data {
            reload_ai_modified_apps(&vfs, &home_dir, &mut apps).await;
        }

        // ── Process OS actions queued by the AI agent ──
        // (tools push to os_action_queue; we drain here each frame)
        {
            let actions: Vec<OsAction> = if let Ok(mut q) = os_action_queue.lock() {
                std::mem::take(&mut *q)
            } else {
                Vec::new()
            };
            for action in actions {
                match action {
                    OsAction::OpenApp(app_name) => {
                        if apps.contains_key(app_name.as_str()) {
                            if let Ok(mut t) = tracker.lock() {
                                if let DesktopMode::AppView(ref prev) = desktop.mode.clone() {
                                    if let Some(e) = t.get_mut(prev) { e.set_background(); }
                                }
                                if let Some(e) = t.get_mut(&app_name) { e.set_active(); }
                            }
                            desktop.mode = DesktopMode::AppView(app_name.clone());
                            if let Some(app) = apps.get_mut(&app_name) {
                                app.on_resume();
                                if app_name == "files" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<FilesApp>() {
                                        a.async_refresh().await;
                                    }
                                }
                                if app_name == "weather" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<WeatherApp>() {
                                        if a.needs_fetch() { a.async_fetch().await; }
                                    }
                                }
                                if app_name == "mail" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<MailApp>() {
                                        if a.needs_load() { a.async_load().await; }
                                    }
                                }
                                if app_name == "backup" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<BackupApp>() {
                                        if a.needs_scan() { a.async_scan().await; }
                                    }
                                }
                                if app_name == "browser" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<BrowserApp>() {
                                        if a.needs_data_load() { a.async_load_data().await; }
                                    }
                                }
                                if app_name == "dev" {
                                    if let Some(a) = app.as_any_mut().downcast_mut::<DevApp>() {
                                        if a.needs_refresh() { a.async_refresh().await; }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Update autocomplete suggestions when input changes
        let allow_suggestions = desktop.mode == DesktopMode::Shell || 
            (desktop.mode == DesktopMode::HomeScreen && desktop.home_focus == HomeFocus::Console);

        if !allow_suggestions {
            if !desktop.suggestions.is_empty() {
                desktop.suggestions.clear();
                desktop.suggestion_selected = 0;
            }
        } else if desktop.shell_input != last_suggestion_input {
            last_suggestion_input = desktop.shell_input.clone();
            if !desktop.shell_input.is_empty() {
                desktop.suggestions = compute_suggestions(&desktop.shell_input, &shell_ctx, &app_names).await;
                desktop.suggestion_selected = 0;
            } else {
                desktop.suggestions.clear();
                desktop.suggestion_selected = 0;
            }
        }
    }

    // ── Cleanup: Save all app state ──
    for app in apps.values_mut() {
        app.on_close();
    }
    if let Err(e) = vfs.save().await {
        eprintln!("Warning: VFS shutdown save failed: {}", e);
        tracing::warn!("VFS shutdown save failed: {}", e);
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    info!("NeuraOS shutdown complete");
    println!("NeuraOS shut down. Goodbye.");
    Ok(())
}

/// Load persisted state from VFS into each app.
async fn load_app_states(vfs: &Vfs, home_dir: &str, apps: &mut HashMap<String, Box<dyn App>>) {
    // Load notes (stored as individual JSON files in ~/notes/)
    let notes_dir = format!("{}/notes", home_dir);
    if let Ok(entries) = vfs.list_dir(&notes_dir).await {
        let mut notes: Vec<serde_json::Value> = Vec::new();
        let mut sorted = entries;
        sorted.sort();
        for name in sorted {
            let path = format!("{}/{}", notes_dir, name);
            if let Ok(data) = vfs.read_file(&path).await {
                if let Ok(note) = serde_json::from_slice::<serde_json::Value>(&data) {
                    notes.push(note);
                }
            }
        }
        if !notes.is_empty() {
            if let Some(app) = apps.get_mut("notes") {
                app.load_state(serde_json::Value::Array(notes));
            }
        }
    }

    // Load tasks (stored as a JSON array in ~/tasks.task)
    let tasks_path = format!("{}/tasks.task", home_dir);
    if let Ok(data) = vfs.read_file(&tasks_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("tasks") {
                app.load_state(state);
            }
        }
    }

    // Load settings (stored as Preferences JSON in ~/settings.json)
    let settings_path = format!("{}/settings.json", home_dir);
    if let Ok(data) = vfs.read_file(&settings_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("settings") {
                app.load_state(state);
            }
        }
    }

    // Load contacts (stored as JSON array in ~/contacts.json)
    let contacts_path = format!("{}/contacts.json", home_dir);
    if let Ok(data) = vfs.read_file(&contacts_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("contacts") {
                app.load_state(state);
            }
        }
    }

    // Load chat history
    let chat_path = format!("{}/chat_history.json", home_dir);
    if let Ok(data) = vfs.read_file(&chat_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("chat") {
                app.load_state(state);
            }
        }
    }

    // Load media library
    let media_path = format!("{}/media.json", home_dir);
    if let Ok(data) = vfs.read_file(&media_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("media") {
                app.load_state(state);
            }
        }
    }

    // Load browser bookmarks + history
    let browser_path = format!("{}/browser_bookmarks.json", home_dir);
    if let Ok(data) = vfs.read_file(&browser_path).await {
        if let Ok(bms) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("browser") {
                app.load_state(serde_json::json!({ "bookmarks": bms }));
            }
        }
    }

    // Load mail sent messages
    let mail_sent_path = format!("{}/mail_sent.json", home_dir);
    if let Ok(data) = vfs.read_file(&mail_sent_path).await {
        if let Ok(sent) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("mail") {
                app.load_state(serde_json::json!({ "sent": sent }));
            }
        }
    }
}

async fn run_model_download_ui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model_name: &str,
) -> Result<()> {
    use tokio::sync::mpsc;
    use ratatui::widgets::{Gauge, Paragraph, Clear};
    use ratatui::layout::Alignment;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let (comp_tx, mut comp_rx) = mpsc::unbounded_channel();
    let model_name_clone = model_name.to_string();

    // Spawn download task
    tokio::spawn(async move {
        let cb = move |msg: String| {
            let _ = tx.send(msg);
        };
        let result = OllamaManager::install_model(&model_name_clone, Some(cb)).await;
        let _ = comp_tx.send(result);
    });

    let mut last_msg = "Starting download...".to_string();
    let mut progress = 0.0;

    loop {
        // Poll for progress updates
        while let Ok(msg) = rx.try_recv() {
            last_msg = msg.clone();
            // Try to parse percentage from string like "pulling ... 45%"
            if let Some(idx) = msg.rfind('%') {
                // Look for space before number
                let substr = &msg[..idx];
                if let Some(space_idx) = substr.rfind(' ') {
                    if let Ok(p) = substr[space_idx+1..].parse::<f64>() {
                        progress = p / 100.0;
                    }
                }
            }
        }

        // Check for completion
        if let Ok(result) = comp_rx.try_recv() {
            return result;
        }

        // Draw UI
        terminal.draw(|f| {
            let area = f.area();
            
            // Calculate modal area
            let percent_x = 60;
            let percent_y = 20;
            let popup_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage((100 - percent_y) / 2),
                    Constraint::Percentage(percent_y),
                    Constraint::Percentage((100 - percent_y) / 2),
                ])
                .split(area);

            let modal_area = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage((100 - percent_x) / 2),
                    Constraint::Percentage(percent_x),
                    Constraint::Percentage((100 - percent_x) / 2),
                ])
                .split(popup_layout[1])[1];

            f.render_widget(Clear, modal_area);

            let block = Block::default()
                .title(format!(" Downloading Model: {} ", model_name))
                .borders(Borders::ALL)
                .style(Style::default().bg(palette::PANEL).fg(palette::FG));
            f.render_widget(block.clone(), modal_area);

            let inner = block.inner(modal_area);
            
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ])
                .split(inner);

            f.render_widget(
                Paragraph::new("Please wait while the model is being downloaded...").alignment(Alignment::Center),
                chunks[0]
            );

            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title("Progress"))
                .gauge_style(Style::default().fg(palette::ACCENT))
                .ratio(progress);
            f.render_widget(gauge, chunks[1]);

            f.render_widget(
                Paragraph::new(last_msg.clone()).alignment(Alignment::Center).style(Style::default().fg(palette::MUTED)),
                chunks[2]
            );
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
             // Consume events to prevent buffer buildup, but don't act on them except maybe resize
             let _ = event::read()?;
        }
    }
}

/// Reload only the apps that AI agent tools can mutate (tasks, notes, contacts).
/// Called after every AI response that made at least one tool call.
/// This syncs VFS writes (done by tools) back into the apps' in-memory state.
async fn reload_ai_modified_apps(vfs: &Vfs, home_dir: &str, apps: &mut HashMap<String, Box<dyn App>>) {
    // Tasks — flat JSON array at ~/tasks.task
    let tasks_path = format!("{}/tasks.task", home_dir);
    if let Ok(data) = vfs.read_file(&tasks_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("tasks") {
                app.load_state(state);
            }
        }
    }

    // Notes — individual JSON files in ~/notes/
    let notes_dir = format!("{}/notes", home_dir);
    if let Ok(mut entries) = vfs.list_dir(&notes_dir).await {
        entries.sort();
        let mut notes: Vec<serde_json::Value> = Vec::new();
        for name in entries {
            let path = format!("{}/{}", notes_dir, name);
            if let Ok(data) = vfs.read_file(&path).await {
                if let Ok(note) = serde_json::from_slice::<serde_json::Value>(&data) {
                    notes.push(note);
                }
            }
        }
        if let Some(app) = apps.get_mut("notes") {
            app.load_state(serde_json::Value::Array(notes));
        }
    }

    // Contacts — JSON array at ~/contacts.json
    let contacts_path = format!("{}/contacts.json", home_dir);
    if let Ok(data) = vfs.read_file(&contacts_path).await {
        if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&data) {
            if let Some(app) = apps.get_mut("contacts") {
                app.load_state(state);
            }
        }
    }
}

/// Compute context-aware autocomplete suggestions based on current input.
async fn compute_suggestions(input: &str, ctx: &ShellContext, app_names: &[String]) -> Vec<String> {
    let mut suggestions: Vec<String> = Vec::new();
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.is_empty() {
        return suggestions;
    }

    let has_trailing_space = input.ends_with(' ');

    // Case 1: Completing command name (first word)
    if parts.len() == 1 && !has_trailing_space {
        let partial = parts[0];
        for cmd in Builtins::command_names() {
            if cmd.starts_with(partial) && *cmd != partial {
                suggestions.push(cmd.to_string());
            }
        }
        // Add matching history entries
        for hist in ctx.command_history.iter().rev() {
            if suggestions.len() >= 8 { break; }
            if hist.starts_with(partial) && hist != partial && !suggestions.contains(hist) {
                suggestions.push(hist.clone());
            }
        }
        suggestions.truncate(8);
        suggestions.retain(|s| s != input);
        return suggestions;
    }

    // Case 2: Completing arguments
    let command = parts[0];
    let completing_first_arg = (parts.len() == 1 && has_trailing_space)
        || (parts.len() == 2 && !has_trailing_space);

    if completing_first_arg {
        let partial_arg = if has_trailing_space { "" } else { parts.get(1).copied().unwrap_or("") };
        let input_prefix = if has_trailing_space {
            input.to_string()
        } else {
            input[..input.len() - partial_arg.len()].to_string()
        };

        match command {
            "open" => {
                for name in app_names {
                    if name.starts_with(partial_arg) {
                        suggestions.push(format!("{}{}", input_prefix, name));
                    }
                    if suggestions.len() >= 8 { break; }
                }
            }
            "cd" | "ls" | "cat" | "rm" | "stat" | "mkdir" | "touch" | "tree" | "write" => {
                // VFS path completion
                let (dir_to_list, name_prefix) = if partial_arg.is_empty() {
                    (ctx.cwd.clone(), String::new())
                } else if partial_arg.ends_with('/') {
                    (ctx.resolve_path(partial_arg), String::new())
                } else {
                    let resolved = ctx.resolve_path(partial_arg);
                    if let Some(last_slash) = resolved.rfind('/') {
                        let dir = if last_slash == 0 { "/".to_string() } else { resolved[..last_slash].to_string() };
                        let name = resolved[last_slash + 1..].to_string();
                        (dir, name)
                    } else {
                        (ctx.cwd.clone(), resolved)
                    }
                };

                if let Ok(entries) = ctx.vfs.list_dir(&dir_to_list).await {
                    let mut filtered: Vec<String> = entries.into_iter()
                        .filter(|e| e.starts_with(&name_prefix))
                        .collect();
                    filtered.sort();
                    for entry in filtered.iter().take(8) {
                        let completed_arg = if partial_arg.is_empty() {
                            entry.clone()
                        } else if partial_arg.ends_with('/') {
                            format!("{}{}", partial_arg, entry)
                        } else if let Some(last_slash) = partial_arg.rfind('/') {
                            format!("{}/{}", &partial_arg[..last_slash], entry)
                        } else {
                            entry.clone()
                        };
                        suggestions.push(format!("{}{}", input_prefix, completed_arg));
                    }
                }
            }
            _ => {}
        }
    }

    // History-based fallback for any case with no suggestions yet
    if suggestions.is_empty() && command != "ai" && command != "echo" {
        for hist in ctx.command_history.iter().rev() {
            if suggestions.len() >= 8 { break; }
            if hist.starts_with(input) && hist != input {
                suggestions.push(hist.clone());
            }
        }
    }

    suggestions.retain(|s| s != input);
    suggestions.truncate(8);
    suggestions
}


// ── Timezone / border-style helpers ────────────────────────────────────────────

/// Parse "Label|offset_mins" (e.g. "Jakarta (WIB)|420") into (label, offset_mins).
fn parse_timezone(raw: &str) -> (String, i32) {
    if let Some(pos) = raw.rfind('|') {
        let label  = raw[..pos].to_string();
        let offset = raw[pos + 1..].parse::<i32>().unwrap_or(0);
        (label, offset)
    } else {
        (raw.to_string(), 0)
    }
}

/// Map a border-style setting string to a ratatui BorderType.
fn parse_border_type(style: &str) -> ratatui::widgets::BorderType {
    use ratatui::widgets::BorderType;
    match style {
        "plain"  => BorderType::Plain,
        "double" => BorderType::Double,
        "thick"  => BorderType::Thick,
        _        => BorderType::Rounded,
    }
}

// ── Auth screen ────────────────────────────────────────────────────────────────
//
// Single entry point for all authentication flows:
//   Menu  → choose Login or Create Account
//   Login → username + password form
//   Create→ username + password + confirm form
//
// Design follows the Tokyo Night shell/desktop palette.

async fn run_auth_screen(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    auth: &AuthService,
) -> Result<Option<(String, Role)>> {
    // ── Tokyo Night palette (centralised — one edit in palette.rs updates here) ──
    const BG:     Color = palette::BG;
    const PANEL:  Color = palette::PANEL;
    const BORDER: Color = palette::BORDER;
    const ACCENT: Color = palette::PRIMARY;
    const FG:     Color = palette::FG;
    const MUTED:  Color = palette::STATUSBAR_MUTED;
    const OK:     Color = palette::OK;
    const ERR:    Color = palette::ERR;
    const INPUT_ACTIVE: Color = Color::Rgb(60, 60, 80);

    enum Mode { Menu, Login, Create }

    // Detect fresh install — skip menu, go straight to Create
    let has_existing_users = auth.has_any_users().await.unwrap_or(false);
    let mut mode: Mode = if has_existing_users { Mode::Menu } else { Mode::Create };

    // Menu state
    let mut menu_sel: usize = 0; // 0 = Login, 1 = Create

    // Login state
    let mut l_user  = String::new();
    let mut l_pass  = String::new();
    let mut l_field = 0usize;

    // Create state
    let mut c_user    = String::new();
    let mut c_pass    = String::new();
    let mut c_confirm = String::new();
    let mut c_field   = 0usize;

    // Status message
    let mut msg    = String::new();
    let mut msg_ok = false;

    // If fresh install, show a helpful prompt
    if !has_existing_users {
        msg    = "No accounts found. Create your administrator account.".to_string();
        msg_ok = true;
    }

    loop {
        terminal.draw(|f| {
            let size = f.area();

            // Full-screen background
            f.render_widget(
                Block::default().style(Style::default().bg(BG)),
                size,
            );

            // Panel dimensions per mode (Larger for better look)
            let (panel_w, panel_h): (u16, u16) = match mode {
                Mode::Menu   => (70, 20),
                Mode::Login  => (70, 22),
                Mode::Create => (70, 26),
            };
            
            // Center the panel
            let px = (size.width.saturating_sub(panel_w)) / 2;
            let py = (size.height.saturating_sub(panel_h)) / 2;
            let panel = Rect::new(px, py, panel_w, panel_h);

            // Drop shadow
            let shadow = Rect::new(px + 1, py + 1, panel_w, panel_h);
            f.render_widget(Block::default().style(Style::default().bg(Color::Black)), shadow);

            // Main Card
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT))
                    .style(Style::default().bg(PANEL).fg(FG)),
                panel,
            );

            // Inner content layout
            let inner_area = Block::default().borders(Borders::ALL).inner(panel);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Length(1), // Separator
                    Constraint::Min(1),    // Content
                    Constraint::Length(3), // Footer
                ])
                .split(inner_area);

            // 1. Header
            let (title, subtitle) = match mode {
                Mode::Menu   => ("NEURA OS", "Secure System Access"),
                Mode::Login  => ("LOGIN", "Enter Credentials"),
                Mode::Create => ("SETUP", "Create Administrator"),
            };
            
            let header_text = vec![
                Line::from(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(subtitle, Style::default().fg(MUTED))),
            ];
            f.render_widget(Paragraph::new(header_text).alignment(Alignment::Center), chunks[0]);

            // 2. Separator
            f.render_widget(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(MUTED)), Rect::new(inner_area.x, inner_area.y + 2, inner_area.width, 1));

            // 3. Content
            let content_area = chunks[2];
            match mode {
                Mode::Menu => {
                    let items = ["Login", "Create Account"];
                    let start_y = content_area.y + 3;
                    
                    for (i, item) in items.iter().enumerate() {
                        let y = start_y + (i as u16 * 4); // More spacing
                        let is_sel = i == menu_sel;
                        let style = if is_sel {
                            Style::default().bg(ACCENT).fg(BG).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(FG).bg(PANEL).add_modifier(Modifier::BOLD)
                        };
                        
                        // Centered button-like look
                        let btn_width = 32;
                        let btn_x = content_area.x + (content_area.width.saturating_sub(btn_width)) / 2;
                        
                        // Button background
                        f.render_widget(
                            Block::default().style(style).borders(Borders::NONE),
                            Rect::new(btn_x, y, btn_width, 3)
                        );

                        // Button Text
                        let prefix = if is_sel { "▶ " } else { "  " };
                        f.render_widget(
                            Paragraph::new(format!("{}{}", prefix, item))
                                .style(style)
                                .alignment(Alignment::Center),
                            Rect::new(btn_x, y + 1, btn_width, 1)
                        );
                    }
                }
                Mode::Login => {
                    let fields = [("Username", &l_user, false), ("Password", &l_pass, true)];
                    let start_y = content_area.y + 3;

                    for (i, (label, val, mask)) in fields.iter().enumerate() {
                        let y = start_y + (i as u16 * 4);
                        let is_active = i == l_field;
                        
                        // Label
                        f.render_widget(
                            Paragraph::new(format!("{:>10} ", label))
                                .style(Style::default().fg(if is_active { ACCENT } else { MUTED })),
                            Rect::new(content_area.x + 4, y + 1, 12, 1)
                        );

                        // Input Box
                        let input_bg = if is_active { INPUT_ACTIVE } else { Color::Rgb(30, 30, 40) };
                        let input_rect = Rect::new(content_area.x + 18, y, content_area.width.saturating_sub(26), 3);
                        
                        f.render_widget(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(if is_active { ACCENT } else { MUTED }))
                                .style(Style::default().bg(input_bg)),
                            input_rect
                        );

                        let display = if *mask { "●".repeat(val.len()) } else { val.to_string() };
                        let cursor = if is_active { "▌" } else { "" };
                        
                        f.render_widget(
                            Paragraph::new(format!("{}{}", display, cursor))
                                .style(Style::default().fg(FG).bg(input_bg)),
                            Rect::new(input_rect.x + 1, input_rect.y + 1, input_rect.width.saturating_sub(2), 1)
                        );
                    }
                }
                Mode::Create => {
                    let fields = [
                        ("Username", &c_user, false),
                        ("Password", &c_pass, true),
                        ("Confirm", &c_confirm, true)
                    ];
                    let start_y = content_area.y + 2;

                    for (i, (label, val, mask)) in fields.iter().enumerate() {
                        let y = start_y + (i as u16 * 4);
                        let is_active = i == c_field;
                        
                        // Label
                        f.render_widget(
                            Paragraph::new(format!("{:>10} ", label))
                                .style(Style::default().fg(if is_active { ACCENT } else { MUTED })),
                            Rect::new(content_area.x + 4, y + 1, 12, 1)
                        );

                        // Input Box
                        let input_bg = if is_active { INPUT_ACTIVE } else { Color::Rgb(30, 30, 40) };
                        let input_rect = Rect::new(content_area.x + 18, y, content_area.width.saturating_sub(26), 3);
                        
                        f.render_widget(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(if is_active { ACCENT } else { MUTED }))
                                .style(Style::default().bg(input_bg)),
                            input_rect
                        );

                        let display = if *mask { "●".repeat(val.len()) } else { val.to_string() };
                        let cursor = if is_active { "▌" } else { "" };
                        
                        f.render_widget(
                            Paragraph::new(format!("{}{}", display, cursor))
                                .style(Style::default().fg(FG).bg(input_bg)),
                            Rect::new(input_rect.x + 1, input_rect.y + 1, input_rect.width.saturating_sub(2), 1)
                        );
                    }
                }
            }

            // 4. Footer
            if !msg.is_empty() {
                let color = if msg_ok { OK } else { ERR };
                let icon = if msg_ok { "✔" } else { "✘" };
                f.render_widget(
                    Paragraph::new(format!("{} {}", icon, msg))
                        .style(Style::default().fg(color))
                        .alignment(Alignment::Center),
                    chunks[3]
                );
            } else {
                let hint = match mode {
                    Mode::Menu => "↑/↓ Select • Enter Confirm",
                    Mode::Login => "Tab Next • Enter Login • Esc Back",
                    Mode::Create => "Tab Next • Enter Setup • Esc Back",
                };
                f.render_widget(
                    Paragraph::new(hint).style(Style::default().fg(MUTED)).alignment(Alignment::Center),
                    chunks[3]
                );
            }
        })?;

        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }

                // Global exits
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('d') | KeyCode::Char('c') => return Ok(None),
                        _ => {}
                    }
                    continue;
                }

                match mode {
                    // ── Menu events ─────────────────────────────────────────
                    Mode::Menu => match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            menu_sel = if menu_sel == 0 { 1 } else { 0 };
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            menu_sel = if menu_sel == 1 { 0 } else { 1 };
                        }
                        KeyCode::Enter => {
                            msg.clear();
                            if menu_sel == 0 {
                                mode = Mode::Login;
                                l_user.clear(); l_pass.clear(); l_field = 0;
                            } else {
                                mode = Mode::Create;
                                c_user.clear(); c_pass.clear(); c_confirm.clear(); c_field = 0;
                            }
                        }
                        _ => {}
                    },

                    // ── Login events ─────────────────────────────────────────
                    Mode::Login => match key.code {
                        KeyCode::Esc => {
                            mode = Mode::Menu;
                            msg.clear();
                        }
                        KeyCode::Tab => {
                            l_field = (l_field + 1) % 2;
                            msg.clear();
                        }
                        KeyCode::BackTab => {
                            l_field = 1 - l_field;
                            msg.clear();
                        }
                        KeyCode::Backspace => {
                            if l_field == 0 { l_user.pop(); } else { l_pass.pop(); }
                            msg.clear();
                        }
                        KeyCode::Char(c) => {
                            if l_field == 0 { l_user.push(c); } else { l_pass.push(c); }
                            msg.clear();
                        }
                        KeyCode::Enter => {
                            if l_user.is_empty() {
                                msg = "Enter your username.".to_string();
                                msg_ok = false;
                            } else if l_pass.is_empty() {
                                msg = "Enter your password.".to_string();
                                msg_ok = false;
                            } else {
                                match auth.login(&l_user, &l_pass).await {
                                    Ok(session) => {
                                        info!("Login: {} ({})", session.username, session.role);
                                        return Ok(Some((session.username, session.role)));
                                    }
                                    Err(_) => {
                                        msg    = "Invalid username or password.".to_string();
                                        msg_ok = false;
                                        l_pass.clear();
                                    }
                                }
                            }
                        }
                        _ => {}
                    },

                    // ── Create events ────────────────────────────────────────
                    Mode::Create => match key.code {
                        KeyCode::Esc => {
                            // Only allow back if there are existing users to log in to
                            if has_existing_users {
                                mode = Mode::Menu;
                                msg.clear();
                            }
                        }
                        KeyCode::Tab => {
                            c_field = (c_field + 1) % 3;
                            msg.clear();
                        }
                        KeyCode::BackTab => {
                            c_field = if c_field == 0 { 2 } else { c_field - 1 };
                            msg.clear();
                        }
                        KeyCode::Backspace => {
                            match c_field {
                                0 => { c_user.pop(); }
                                1 => { c_pass.pop(); }
                                _ => { c_confirm.pop(); }
                            }
                            msg.clear();
                        }
                        KeyCode::Char(c) => {
                            match c_field {
                                0 => c_user.push(c),
                                1 => c_pass.push(c),
                                _ => c_confirm.push(c),
                            }
                            msg.clear();
                        }
                        KeyCode::Enter => {
                            if c_user.len() < 2 || c_user.len() > 32 {
                                msg    = "Username must be 2–32 characters.".to_string();
                                msg_ok = false;
                            } else if !c_user.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                                msg    = "Letters, digits, _ or - only.".to_string();
                                msg_ok = false;
                            } else if c_pass.len() < 6 {
                                msg    = "Password must be at least 6 characters.".to_string();
                                msg_ok = false;
                            } else if c_pass != c_confirm {
                                msg    = "Passwords do not match.".to_string();
                                msg_ok = false;
                                c_confirm.clear();
                            } else {
                                // First admin gets Root role; any subsequent account gets User
                                let new_role = match auth.user_store().count_admins().await {
                                    Ok(0) => Role::Admin,
                                    _     => Role::User,
                                };
                                match auth.user_store().create_user(&c_user, &c_pass, new_role.clone()).await {
                                    Ok(_) => {
                                        info!("Created account: {} ({})", c_user, new_role);
                                        return Ok(Some((c_user.clone(), new_role)));
                                    }
                                    Err(e) => {
                                        msg    = e.to_string();
                                        msg_ok = false;
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

async fn run_ollama_setup(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ollama_setup: &mut OllamaSetupApp,
) -> Result<Option<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    
    loop {
        // Process any pending async actions
        if ollama_setup.has_pending_action() {
            ollama_setup.process_next_action().await?;
        }
        
        terminal.draw(|f| {
            ollama_setup.render(f, f.area());
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        if let Some(selected_model) = ollama_setup.handle_enter() {
                            return Ok(Some(selected_model));
                        }
                    }
                    KeyCode::Up => {
                        ollama_setup.handle_up();
                    }
                    KeyCode::Down => {
                        ollama_setup.handle_down();
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }
}
