use std::sync::Arc;
use chrono::{Utc, FixedOffset};
use serde_json::Value;
use neura_storage::vfs::NodeType;
use neura_ai_core::{AgentEngine, ToolRegistry, Tool, ToolParam, MemoryManager};
use neura_ai_core::provider::types::ChatMessage;
use neura_ai_core::agent::tool_registry::ToolError;
use neura_users::roles::Role;
use neura_app_framework::consts::{OS_NAME, OS_VERSION, SHELL_DISPLAY};
use crate::context::ShellContext;

pub struct Builtins;

impl Builtins {
    /// Returns the list of all builtin command names.
    pub fn command_names() -> &'static [&'static str] {
        &[
            "echo", "whoami", "hostname", "date", "pwd", "help", "clear",
            "version", "exit", "logout", "shutdown",
            "ls", "cd", "mkdir", "touch", "cat", "rm", "stat", "write", "tree",
            "sysinfo", "apps", "open", "neofetch", "ai",
            // User management
            "users", "userlist", "useradd", "userdel", "passwd",
        ]
    }

    pub async fn try_execute(program: &str, args: &[String], ctx: &mut ShellContext) -> Option<String> {
        match program {
            "echo" => Some(args.join(" ")),
            "whoami" => Some(format!("{} ({})", ctx.username, ctx.role)),
            "hostname" => Some(ctx.hostname.clone()),
            "date" => {
                let offset_secs = ctx.timezone_offset_mins * 60;
                let tz = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap());
                let now = Utc::now().with_timezone(&tz);
                let sign = if ctx.timezone_offset_mins >= 0 { "+" } else { "-" };
                let abs = ctx.timezone_offset_mins.abs();
                let utc_tag = format!("UTC{}{:02}:{:02}", sign, abs / 60, abs % 60);
                Some(format!("{} {} ({})", now.format("%Y-%m-%d %H:%M:%S"), utc_tag, ctx.timezone_label))
            }
            "pwd" => Some(ctx.cwd.clone()),
            "help" => Some(Self::help_text()),
            "clear" => Some("__CLEAR__".to_string()),
            "version" => Some(format!("{} v{}", OS_NAME, OS_VERSION)),
            "exit" | "logout" | "shutdown" => Some("__EXIT__".to_string()),

            // Filesystem commands
            "ls" => Some(Self::cmd_ls(args, ctx).await),
            "cd" => Some(Self::cmd_cd(args, ctx).await),
            "mkdir" => Some(Self::cmd_mkdir(args, ctx).await),
            "touch" => Some(Self::cmd_touch(args, ctx).await),
            "cat" => Some(Self::cmd_cat(args, ctx).await),
            "rm" => Some(Self::cmd_rm(args, ctx).await),
            "stat" => Some(Self::cmd_stat(args, ctx).await),
            "write" => Some(Self::cmd_write(args, ctx).await),
            "tree" => Some(Self::cmd_tree(args, ctx).await),

            // System commands
            "sysinfo" => Some(Self::cmd_sysinfo(ctx)),
            "apps" => Some(Self::cmd_apps()),
            "open" => Some(Self::cmd_open(args)),
            "neofetch" => Some(Self::cmd_neofetch(ctx)),

            // User management
            "users" | "userlist" => Some(Self::cmd_userlist(ctx).await),
            "useradd" => Some(Self::cmd_useradd(args, ctx).await),
            "userdel" => Some(Self::cmd_userdel(args, ctx).await),
            "passwd" => Some(Self::cmd_passwd(args, ctx).await),

            // AI Agent
            "ai" => {
                if args.is_empty() {
                    Some("Usage: ai <prompt>\n  The AI agent can use tools to manage files, notes, tasks, contacts, and more.\n  Example: ai make a summary of my tasks\n  Example: ai create a note called Meeting Notes with the agenda\n  Tip: Set your API key in 'open settings' > AI > Api Key".to_string())
                } else {
                    Some(Self::cmd_ai(args, ctx).await)
                }
            }

            _ => None,
        }
    }

    async fn cmd_ls(args: &[String], ctx: &ShellContext) -> String {
        let path = if args.is_empty() {
            ctx.cwd.clone()
        } else {
            ctx.resolve_path(&args[0])
        };
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        match ctx.vfs.list_dir(&path).await {
            Ok(entries) => {
                if entries.is_empty() {
                    "(empty)".to_string()
                } else {
                    let mut sorted = entries;
                    sorted.sort();
                    let mut lines = Vec::new();
                    for name in &sorted {
                        let full = if path == "/" {
                            format!("/{}", name)
                        } else {
                            format!("{}/{}", path, name)
                        };
                        let suffix = if matches!(ctx.vfs.stat(&full).await, Ok(ref info) if matches!(info.node_type, NodeType::Directory)) {
                            "/"
                        } else {
                            ""
                        };
                        lines.push(format!("  {}{}", name, suffix));
                    }
                    lines.join("\n")
                }
            }
            Err(e) => format!("ls: {}", e),
        }
    }

    async fn cmd_cd(args: &[String], ctx: &mut ShellContext) -> String {
        let target = if args.is_empty() {
            format!("/home/{}", ctx.username)
        } else {
            ctx.resolve_path(&args[0])
        };
        if let Err(e) = ctx.check_path_access(&target) {
            return e;
        }
        if ctx.vfs.exists(&target).await {
            let is_dir = matches!(ctx.vfs.stat(&target).await, Ok(ref info) if matches!(info.node_type, NodeType::Directory));
            if is_dir {
                ctx.cwd = target.clone();
                // Auto-list the new directory so the user sees what's there instantly.
                Self::cmd_ls(&[], ctx).await
            } else {
                format!("cd: not a directory: {}", target)
            }
        } else {
            format!("cd: no such directory: {}", target)
        }
    }

    async fn cmd_mkdir(args: &[String], ctx: &ShellContext) -> String {
        if args.is_empty() {
            return "Usage: mkdir <directory>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        match ctx.vfs.mkdir(&path, &ctx.username).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(&path);
                format!("created: {}/", name)
            }
            Err(e) => format!("mkdir: {}", e),
        }
    }

    async fn cmd_touch(args: &[String], ctx: &ShellContext) -> String {
        if args.is_empty() {
            return "Usage: touch <file>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        if ctx.vfs.exists(&path).await {
            return String::new();
        }
        match ctx.vfs.write_file(&path, Vec::new(), &ctx.username).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(&path);
                format!("created: {}", name)
            }
            Err(e) => format!("touch: {}", e),
        }
    }

    async fn cmd_cat(args: &[String], ctx: &ShellContext) -> String {
        if args.is_empty() {
            return "Usage: cat <file>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        match ctx.vfs.read_file(&path).await {
            Ok(data) => String::from_utf8_lossy(&data).to_string(),
            Err(e) => format!("cat: {}", e),
        }
    }

    async fn cmd_write(args: &[String], ctx: &ShellContext) -> String {
        if args.len() < 2 {
            return "Usage: write <file> <content>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        let content = args[1..].join(" ");
        match ctx.vfs.write_file(&path, content.into_bytes(), &ctx.username).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(&path);
                format!("written: {}", name)
            }
            Err(e) => format!("write: {}", e),
        }
    }

    async fn cmd_rm(args: &[String], ctx: &ShellContext) -> String {
        if args.is_empty() {
            return "Usage: rm <file|dir>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        match ctx.vfs.remove(&path).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(&path);
                format!("removed: {}", name)
            }
            Err(e) => format!("rm: {}", e),
        }
    }

    async fn cmd_stat(args: &[String], ctx: &ShellContext) -> String {
        if args.is_empty() {
            return "Usage: stat <path>".to_string();
        }
        let path = ctx.resolve_path(&args[0]);
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        match ctx.vfs.stat(&path).await {
            Ok(info) => {
                let type_str = match info.node_type {
                    NodeType::File => "file",
                    NodeType::Directory => "directory",
                    NodeType::Symlink(_) => "symlink",
                };
                format!(
                    "  Path: {}\n  Type: {}\n  Owner: {}\n  Size: {} bytes\n  Perms: {:o}\n  Created: {}\n  Modified: {}",
                    info.path, type_str, info.owner, info.size,
                    info.permissions.to_octal(), info.created_at, info.modified_at
                )
            }
            Err(e) => format!("stat: {}", e),
        }
    }

    async fn cmd_tree(args: &[String], ctx: &ShellContext) -> String {
        let path = if args.is_empty() {
            ctx.cwd.clone()
        } else {
            ctx.resolve_path(&args[0])
        };
        if let Err(e) = ctx.check_path_access(&path) {
            return e;
        }
        let mut output = vec![path.clone()];
        Self::tree_recursive(&ctx.vfs, &path, "", &mut output, 0, 3).await;
        output.join("\n")
    }

    async fn tree_recursive(vfs: &neura_storage::vfs::Vfs, path: &str, prefix: &str, output: &mut Vec<String>, depth: usize, max_depth: usize) {
        if depth >= max_depth {
            return;
        }
        if let Ok(entries) = vfs.list_dir(path).await {
            let mut sorted = entries;
            sorted.sort();
            let count = sorted.len();
            for (i, name) in sorted.iter().enumerate() {
                let is_last = i == count - 1;
                let connector = if is_last { "\u{2514}\u{2500}\u{2500} " } else { "\u{251c}\u{2500}\u{2500} " };
                let child_prefix = if is_last { "    " } else { "\u{2502}   " };
                output.push(format!("{}{}{}", prefix, connector, name));
                let child_path = if path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", path, name)
                };
                Box::pin(Self::tree_recursive(vfs, &child_path, &format!("{}{}", prefix, child_prefix), output, depth + 1, max_depth)).await;
            }
        }
    }

    // ── User management commands ──────────────────────────────────────────────

    async fn cmd_userlist(ctx: &ShellContext) -> String {
        if !ctx.role.can_manage_users() {
            return "Permission denied: only administrators can list users.".to_string();
        }
        let store = match ctx.user_store.as_ref() {
            Some(arc) => arc.clone(),
            None => return "Error: user store not available.".to_string(),
        };
        match store.list_users().await {
            Ok(users) => {
                if users.is_empty() {
                    return "  (no users)".to_string();
                }
                let mut lines = vec![
                    format!("  {:<20} {:<10} Created", "Username", "Role"),
                    format!("  {:-<20} {:-<10} {:-<16}", "", "", ""),
                ];
                for u in &users {
                    let marker = if u.username == ctx.username { " *" } else { "" };
                    lines.push(format!(
                        "  {:<20} {:<10} {}{}",
                        u.username,
                        u.role.to_string(),
                        u.created_at.format("%Y-%m-%d %H:%M"),
                        marker,
                    ));
                }
                lines.join("\n")
            }
            Err(e) => format!("userlist: {}", e),
        }
    }

    async fn cmd_useradd(args: &[String], ctx: &ShellContext) -> String {
        if !ctx.role.can_manage_users() {
            return "Permission denied: only administrators can add users.".to_string();
        }
        if args.len() < 2 {
            return "Usage: useradd <username> <password> [admin|user|guest]".to_string();
        }
        let username = &args[0];
        let password = &args[1];
        let role_str = args.get(2).map(|s| s.as_str()).unwrap_or("guest");

        if username.len() < 2 || username.len() > 32 {
            return "useradd: username must be 2–32 characters.".to_string();
        }
        if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return "useradd: username may only contain letters, digits, '_', or '-'.".to_string();
        }
        if password.len() < 6 {
            return "useradd: password must be at least 6 characters.".to_string();
        }

        let role: Role = match role_str.to_lowercase().as_str() {
            "admin" => {
                if !matches!(ctx.role, Role::Root) {
                    return "useradd: only root can create administrator accounts.".to_string();
                }
                Role::Admin
            }
            "user"  => Role::User,
            "guest" => Role::Guest,
            other   => return format!("useradd: unknown role '{}'. Use: admin, user, or guest.", other),
        };

        let store = match ctx.user_store.as_ref() {
            Some(arc) => arc.clone(),
            None => return "Error: user store not available.".to_string(),
        };
        match store.create_user(username, password, role.clone()).await {
            Ok(_) => {
                // Create home directory in VFS
                let home = format!("/home/{}", username);
                if !ctx.vfs.exists(&home).await {
                    let _ = ctx.vfs.mkdir(&home, username).await;
                }
                format!("User '{}' created with role '{}'.", username, role)
            }
            Err(e) => format!("useradd: {}", e),
        }
    }

    async fn cmd_userdel(args: &[String], ctx: &ShellContext) -> String {
        if !ctx.role.can_manage_users() {
            return "Permission denied: only administrators can delete users.".to_string();
        }
        if args.is_empty() {
            return "Usage: userdel <username>".to_string();
        }
        let target = &args[0];
        if target == &ctx.username {
            return "userdel: cannot delete your own account.".to_string();
        }
        if target.as_str() == "root" {
            return "userdel: cannot delete the root account.".to_string();
        }

        let store = match ctx.user_store.as_ref() {
            Some(arc) => arc.clone(),
            None => return "Error: user store not available.".to_string(),
        };

        // Protect the last administrator
        if let Ok(Some(user)) = store.find_by_username(target).await {
            if user.role.is_privileged() {
                if let Ok(n) = store.count_admins().await {
                    if n <= 1 {
                        return "userdel: cannot delete the last administrator account.".to_string();
                    }
                }
            }
        }

        match store.delete_user(target).await {
            Ok(()) => format!("User '{}' deleted.", target),
            Err(e)  => format!("userdel: {}", e),
        }
    }

    async fn cmd_passwd(args: &[String], ctx: &ShellContext) -> String {
        // Admin:  passwd <username> <new_password>
        // Others: passwd <new_password>  (own account only)
        let (target, new_pass) = if ctx.role.can_manage_users() && args.len() >= 2 {
            (args[0].clone(), args[1].clone())
        } else if !args.is_empty() {
            (ctx.username.clone(), args[0].clone())
        } else {
            return if ctx.role.can_manage_users() {
                "Usage: passwd [<username>] <new_password>".to_string()
            } else {
                "Usage: passwd <new_password>".to_string()
            };
        };

        if !ctx.role.can_manage_users() && target != ctx.username {
            return "Permission denied: you can only change your own password.".to_string();
        }
        if new_pass.len() < 6 {
            return "passwd: new password must be at least 6 characters.".to_string();
        }

        let store = match ctx.user_store.as_ref() {
            Some(arc) => arc.clone(),
            None => return "Error: user store not available.".to_string(),
        };
        match store.change_password(&target, &new_pass).await {
            Ok(()) => format!("Password changed for '{}'.", target),
            Err(e)  => format!("passwd: {}", e),
        }
    }

    fn cmd_sysinfo(ctx: &ShellContext) -> String {
        let ai_info = match &ctx.ai_client {
            Some(client) => format!("{} ({})", client.provider_name(), client.model_name()),
            None => "Not configured".to_string(),
        };
        [
            "  NeuraOS System Information",
            &format!("  Version: {}", OS_VERSION),
            "  Runtime: Tokio async",
            "  Database: SQLite (WAL mode)",
            &format!("  AI: {}", ai_info),
            "  TUI: ratatui + crossterm",
            &format!("  Platform: {}", std::env::consts::OS),
            &format!("  Arch: {}", std::env::consts::ARCH),
        ].join("\n")
    }

    fn cmd_apps() -> String {
        [
            "  Available Applications:",
            "",
            "  Core Apps:",
            "  notes      - NeuraNotes (quick notes)",
            "  tasks      - NeuraTasks (todo manager)",
            "  files      - NeuraFiles (file browser)",
            "  monitor    - NeuraMonitor (task manager)",
            "  settings   - NeuraSettings (system config)",
            "  contacts   - NeuraContacts (address book)",
            "  logs       - NeuraLogs (log viewer)",
            "  calendar   - NeuraCalendar (calendar)",
            "  calc       - NeuraCalc (calculator)",
            "  clock      - NeuraClock (clock + timers)",
            "  sysinfo    - NeuraSystemInfo",
            "",
            "  Productivity:",
            "  chat       - NeuraChat (AI assistant)",
            "  dev        - NeuraDev (code editor)",
            "  mail       - NeuraMail (email client)",
            "",
            "  System & Network:",
            "  weather    - NeuraWeather (live weather)",
            "  terminal   - NeuraTerminal (shell)",
            "  browser    - NeuraBrowse (web browser)",
            "  ssh        - NeuraSSH (secure shell)",
            "  ftp        - NeuraFTP (file transfer)",
            "",
            "  Data & Storage:",
            "  db         - NeuraDB (database browser)",
            "  backup     - NeuraBackup (backup manager)",
            "  sync       - NeuraSync (cloud sync)",
            "  media      - NeuraMedia (media manager)",
            "  store      - NeuraStore (app store)",
            "",
            "  Usage: open <app-name>",
        ].join("\n")
    }

    fn cmd_open(args: &[String]) -> String {
        if args.is_empty() {
            return "Usage: open <app-name>  (type 'apps' for list)".to_string();
        }
        format!("__OPEN_APP__:{}", args[0])
    }

    fn cmd_neofetch(ctx: &ShellContext) -> String {
        let os_info = format!("{}@{}", ctx.username, ctx.hostname);
        let sep = "-".repeat(os_info.len());
        let ai_info = match &ctx.ai_client {
            Some(client) => format!("{} ({})", client.provider_name(), client.model_name()),
            None => "Not configured".to_string(),
        };
        [
            "",
            "  \u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}",
            "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}",
            "  \u{2588}\u{2588}\u{2554}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}",
            "  \u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551}",
            "  \u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}",
            "  \u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}",
            "",
            &format!("  {}", os_info),
            &format!("  {}", sep),
            &format!("  OS: {} v{}", OS_NAME, OS_VERSION),
            &format!("  Host: {} ({})", std::env::consts::OS, std::env::consts::ARCH),
            &format!("  Shell: {}", SHELL_DISPLAY),
            "  DE: NeuraDesktop (TUI)",
            &format!("  AI: {}", ai_info),
            "  DB: SQLite (WAL)",
            "  Theme: Tokyo Night",
            "",
        ].join("\n")
    }

    // ── AI Agent Command ──────────────────────────────────────────────────────

    async fn cmd_ai(args: &[String], ctx: &mut ShellContext) -> String {
        let prompt = args.join(" ");

        let client = match &ctx.ai_client {
            Some(c) => c.clone(),
            None => {
                return "Error: No AI API key configured.\n  Set it via: open settings > AI > Api Key\n  Or set GEMINI_API_KEY / OPENAI_API_KEY / DEEPSEEK_API_KEY in environment.".to_string();
            }
        };

        let provider_name = client.provider_name().to_string();
        let vfs = ctx.vfs.clone();
        let username = ctx.username.clone();
        let cwd = ctx.cwd.clone();
        let hostname = ctx.hostname.clone();
        let ai_info = format!("{} ({})", client.provider_name(), client.model_name());
        let history = ctx.ai_history.clone();

        // ── Build Tool Registry ──
        let mut tools = ToolRegistry::new();

        // list_files
        {
            let vfs = vfs.clone();
            tools.register(Tool {
                name: "list_files".to_string(),
                description: "List files and directories at a given VFS path. Use this to explore the filesystem.".to_string(),
                parameters: vec![
                    ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "Directory path to list (e.g. /home/root, /home/root/notes)".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    Box::pin(async move {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/").to_string();
                        match vfs.list_dir(&path).await {
                            Ok(entries) => Ok(serde_json::json!({ "path": path, "entries": entries })),
                            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                        }
                    })
                }),
            });
        }

        // read_file
        {
            let vfs = vfs.clone();
            tools.register(Tool {
                name: "read_file".to_string(),
                description: "Read the text contents of a file from the VFS filesystem.".to_string(),
                parameters: vec![
                    ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "Absolute VFS file path to read".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    Box::pin(async move {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        match vfs.read_file(&path).await {
                            Ok(data) => {
                                let content = String::from_utf8_lossy(&data).to_string();
                                Ok(serde_json::json!({ "path": path, "content": content, "size": data.len() }))
                            }
                            Err(e) => Err(ToolError::ExecutionFailed(format!("Cannot read '{}': {}", path, e))),
                        }
                    })
                }),
            });
        }

        // write_file
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "write_file".to_string(),
                description: "Write text content to a file in the VFS. Creates the file if it does not exist. Overwrites existing content.".to_string(),
                parameters: vec![
                    ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "Absolute VFS file path to write".to_string(), required: true },
                    ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "Text content to write to the file".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        // Ensure parent directory exists
                        if let Some(parent) = path.rfind('/') {
                            let dir = &path[..parent];
                            if !dir.is_empty() {
                                let _ = vfs.mkdir(dir, &username).await;
                            }
                        }
                        match vfs.write_file(&path, content.into_bytes(), &username).await {
                            Ok(()) => Ok(serde_json::json!({ "status": "written", "path": path })),
                            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                        }
                    })
                }),
            });
        }

        // create_directory
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "create_directory".to_string(),
                description: "Create a new directory in the VFS filesystem.".to_string(),
                parameters: vec![
                    ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "Directory path to create".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        match vfs.mkdir(&path, &username).await {
                            Ok(()) => Ok(serde_json::json!({ "status": "created", "path": path })),
                            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                        }
                    })
                }),
            });
        }

        // delete_file
        {
            let vfs = vfs.clone();
            tools.register(Tool {
                name: "delete_file".to_string(),
                description: "Delete a file or directory from the VFS filesystem.".to_string(),
                parameters: vec![
                    ToolParam { name: "path".to_string(), param_type: "string".to_string(), description: "Absolute VFS path to delete".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    Box::pin(async move {
                        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        match vfs.remove(&path).await {
                            Ok(()) => Ok(serde_json::json!({ "status": "deleted", "path": path })),
                            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                        }
                    })
                }),
            });
        }

        // list_notes
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "list_notes".to_string(),
                description: "List all notes stored in NeuraNotes. Returns note titles and metadata.".to_string(),
                parameters: vec![],
                handler: Box::new(move |_args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let notes_dir = format!("/home/{}/notes", username);
                        match vfs.list_dir(&notes_dir).await {
                            Ok(entries) => {
                                let mut notes = Vec::new();
                                for name in &entries {
                                    let path = format!("{}/{}", notes_dir, name);
                                    if let Ok(data) = vfs.read_file(&path).await {
                                        if let Ok(note) = serde_json::from_slice::<serde_json::Value>(&data) {
                                            notes.push(serde_json::json!({
                                                "title": note.get("title"),
                                                "modified_at": note.get("modified_at"),
                                                "preview": note.get("content").and_then(|c| c.as_str()).unwrap_or("").chars().take(80).collect::<String>(),
                                            }));
                                        }
                                    }
                                }
                                Ok(serde_json::json!({ "count": notes.len(), "notes": notes }))
                            }
                            Err(_) => Ok(serde_json::json!({ "count": 0, "notes": [] })),
                        }
                    })
                }),
            });
        }

        // create_note
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "create_note".to_string(),
                description: "Create a new note in NeuraNotes with a title and content.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Note title".to_string(), required: true },
                    ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "Note content text".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
                        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let now = Utc::now().to_rfc3339();
                        let note = serde_json::json!({
                            "title": title,
                            "content": content,
                            "created_at": now,
                            "modified_at": now,
                        });
                        let notes_dir = format!("/home/{}/notes", username);
                        let _ = vfs.mkdir(&notes_dir, &username).await;
                        let filename = title.replace(' ', "_").to_lowercase();
                        let path = format!("{}/{}.notes", notes_dir, filename);
                        if let Ok(data) = serde_json::to_vec_pretty(&note) {
                            match vfs.write_file(&path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "created", "title": title, "path": path })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // read_note
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "read_note".to_string(),
                description: "Read the full content of a specific note by its title.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "The exact title of the note to read".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let filename = title.replace(' ', "_").to_lowercase();
                        let path = format!("/home/{}/notes/{}.notes", username, filename);
                        match vfs.read_file(&path).await {
                            Ok(data) => {
                                serde_json::from_slice::<serde_json::Value>(&data)
                                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
                            }
                            Err(e) => Err(ToolError::ExecutionFailed(format!("Note '{}' not found: {}", title, e))),
                        }
                    })
                }),
            });
        }

        // update_note
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "update_note".to_string(),
                description: "Update the content of an existing note by title.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "The title of the note to update".to_string(), required: true },
                    ToolParam { name: "content".to_string(), param_type: "string".to_string(), description: "New content for the note".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let filename = title.replace(' ', "_").to_lowercase();
                        let path = format!("/home/{}/notes/{}.notes", username, filename);
                        let now = Utc::now().to_rfc3339();
                        // Read existing to preserve created_at
                        let mut note: serde_json::Value = match vfs.read_file(&path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or(serde_json::json!({})),
                            Err(_) => serde_json::json!({ "title": title, "created_at": now }),
                        };
                        note["content"] = serde_json::json!(content);
                        note["modified_at"] = serde_json::json!(now);
                        if let Ok(data) = serde_json::to_vec_pretty(&note) {
                            match vfs.write_file(&path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "updated", "title": title })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // delete_note
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "delete_note".to_string(),
                description: "Delete a note by its title.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Title of the note to delete".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let filename = title.replace(' ', "_").to_lowercase();
                        let path = format!("/home/{}/notes/{}.notes", username, filename);
                        match vfs.remove(&path).await {
                            Ok(()) => Ok(serde_json::json!({ "status": "deleted", "title": title })),
                            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                        }
                    })
                }),
            });
        }

        // list_tasks
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "list_tasks".to_string(),
                description: "List all tasks in NeuraTasks including their priority and completion status.".to_string(),
                parameters: vec![
                    ToolParam { name: "filter".to_string(), param_type: "string".to_string(), description: "Optional filter: 'pending', 'done', or 'all' (default: all)".to_string(), required: false },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("all").to_string();
                        let tasks_path = format!("/home/{}/tasks.task", username);
                        let tasks: Vec<serde_json::Value> = match vfs.read_file(&tasks_path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                            Err(_) => Vec::new(),
                        };
                        let filtered: Vec<&serde_json::Value> = tasks.iter().filter(|t| {
                            match filter.as_str() {
                                "pending" => t.get("done").and_then(|v| v.as_bool()) != Some(true),
                                "done" => t.get("done").and_then(|v| v.as_bool()) == Some(true),
                                _ => true,
                            }
                        }).collect();
                        Ok(serde_json::json!({ "count": filtered.len(), "tasks": filtered }))
                    })
                }),
            });
        }

        // create_task
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "create_task".to_string(),
                description: "Create a new task in NeuraTasks.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Task title or description".to_string(), required: true },
                    ToolParam { name: "priority".to_string(), param_type: "string".to_string(), description: "Priority level: Low, Medium, or High (default: Medium)".to_string(), required: false },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("Task").to_string();
                        let priority = args.get("priority").and_then(|v| v.as_str()).unwrap_or("Medium").to_string();
                        let now = Utc::now().to_rfc3339();
                        let tasks_path = format!("/home/{}/tasks.task", username);
                        let mut tasks: Vec<serde_json::Value> = match vfs.read_file(&tasks_path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                            Err(_) => Vec::new(),
                        };
                        tasks.push(serde_json::json!({
                            "title": title,
                            "done": false,
                            "priority": priority,
                            "created_at": now,
                        }));
                        if let Ok(data) = serde_json::to_vec_pretty(&tasks) {
                            match vfs.write_file(&tasks_path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "created", "title": title, "priority": priority })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // complete_task
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "complete_task".to_string(),
                description: "Mark a task as completed/done by its title.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Title of the task to mark as done".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let tasks_path = format!("/home/{}/tasks.task", username);
                        let mut tasks: Vec<serde_json::Value> = match vfs.read_file(&tasks_path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                            Err(_) => return Err(ToolError::ExecutionFailed("No tasks found".to_string())),
                        };
                        let mut found = false;
                        for task in &mut tasks {
                            if task.get("title").and_then(|v| v.as_str()) == Some(&title) {
                                task["done"] = serde_json::json!(true);
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            return Err(ToolError::ExecutionFailed(format!("Task '{}' not found", title)));
                        }
                        if let Ok(data) = serde_json::to_vec_pretty(&tasks) {
                            match vfs.write_file(&tasks_path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "completed", "title": title })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // delete_task
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "delete_task".to_string(),
                description: "Delete a task by its title.".to_string(),
                parameters: vec![
                    ToolParam { name: "title".to_string(), param_type: "string".to_string(), description: "Title of the task to delete".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let tasks_path = format!("/home/{}/tasks.task", username);
                        let mut tasks: Vec<serde_json::Value> = match vfs.read_file(&tasks_path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                            Err(_) => return Err(ToolError::ExecutionFailed("No tasks found".to_string())),
                        };
                        let before = tasks.len();
                        tasks.retain(|t| t.get("title").and_then(|v| v.as_str()) != Some(&title));
                        if tasks.len() == before {
                            return Err(ToolError::ExecutionFailed(format!("Task '{}' not found", title)));
                        }
                        if let Ok(data) = serde_json::to_vec_pretty(&tasks) {
                            match vfs.write_file(&tasks_path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "deleted", "title": title })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // list_contacts
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "list_contacts".to_string(),
                description: "List all contacts in NeuraContacts address book.".to_string(),
                parameters: vec![],
                handler: Box::new(move |_args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let contacts_path = format!("/home/{}/contacts.json", username);
                        match vfs.read_file(&contacts_path).await {
                            Ok(data) => {
                                let contacts: serde_json::Value = serde_json::from_slice(&data)
                                    .unwrap_or(serde_json::json!([]));
                                let count = contacts.as_array().map(|a| a.len()).unwrap_or(0);
                                Ok(serde_json::json!({ "count": count, "contacts": contacts }))
                            }
                            Err(_) => Ok(serde_json::json!({ "count": 0, "contacts": [] })),
                        }
                    })
                }),
            });
        }

        // add_contact
        {
            let vfs = vfs.clone();
            let username = username.clone();
            tools.register(Tool {
                name: "add_contact".to_string(),
                description: "Add a new contact to NeuraContacts.".to_string(),
                parameters: vec![
                    ToolParam { name: "name".to_string(), param_type: "string".to_string(), description: "Contact full name".to_string(), required: true },
                    ToolParam { name: "email".to_string(), param_type: "string".to_string(), description: "Contact email address".to_string(), required: false },
                    ToolParam { name: "phone".to_string(), param_type: "string".to_string(), description: "Contact phone number".to_string(), required: false },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    let username = username.clone();
                    Box::pin(async move {
                        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let email = args.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let phone = args.get("phone").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let now = Utc::now().to_rfc3339();
                        let contacts_path = format!("/home/{}/contacts.json", username);
                        let mut contacts: Vec<serde_json::Value> = match vfs.read_file(&contacts_path).await {
                            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                            Err(_) => Vec::new(),
                        };
                        contacts.push(serde_json::json!({
                            "name": name, "email": email, "phone": phone, "created_at": now,
                        }));
                        if let Ok(data) = serde_json::to_vec_pretty(&contacts) {
                            match vfs.write_file(&contacts_path, data, &username).await {
                                Ok(()) => Ok(serde_json::json!({ "status": "added", "name": name })),
                                Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
                            }
                        } else {
                            Err(ToolError::ExecutionFailed("Serialization failed".to_string()))
                        }
                    })
                }),
            });
        }

        // get_current_time
        {
            tools.register(Tool {
                name: "get_current_time".to_string(),
                description: "Get the current date and time in UTC.".to_string(),
                parameters: vec![],
                handler: Box::new(move |_args: Value| {
                    Box::pin(async move {
                        let now = Utc::now();
                        Ok(serde_json::json!({
                            "utc": now.to_rfc3339(),
                            "date": now.format("%Y-%m-%d").to_string(),
                            "time": now.format("%H:%M:%S").to_string(),
                            "formatted": now.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                            "timestamp": now.timestamp(),
                        }))
                    })
                }),
            });
        }

        // calculate
        {
            tools.register(Tool {
                name: "calculate".to_string(),
                description: "Evaluate a mathematical expression and return the numeric result. Supports +, -, *, /, parentheses.".to_string(),
                parameters: vec![
                    ToolParam { name: "expression".to_string(), param_type: "string".to_string(), description: "Math expression to evaluate, e.g. '(100 - 20) / 4' or '2^10'".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    Box::pin(async move {
                        let expr = args.get("expression").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        match eval_math(&expr) {
                            Some(result) => Ok(serde_json::json!({ "expression": expr, "result": result })),
                            None => Err(ToolError::ExecutionFailed(format!("Cannot evaluate expression: {}", expr))),
                        }
                    })
                }),
            });
        }

        // get_system_info
        {
            let username2 = username.clone();
            let cwd2 = cwd.clone();
            let hostname2 = hostname.clone();
            let ai_info2 = ai_info.clone();
            tools.register(Tool {
                name: "get_system_info".to_string(),
                description: "Get NeuraOS system information: OS version, user, hostname, platform, AI provider.".to_string(),
                parameters: vec![],
                handler: Box::new(move |_args: Value| {
                    let username = username2.clone();
                    let cwd = cwd2.clone();
                    let hostname = hostname2.clone();
                    let ai_info = ai_info2.clone();
                    Box::pin(async move {
                        Ok(serde_json::json!({
                            "os": OS_NAME,
                            "version": OS_VERSION,
                            "user": username,
                            "hostname": hostname,
                            "cwd": cwd,
                            "platform": std::env::consts::OS,
                            "arch": std::env::consts::ARCH,
                            "ai_provider": ai_info,
                        }))
                    })
                }),
            });
        }

        // search_files
        {
            let vfs = vfs.clone();
            tools.register(Tool {
                name: "search_files".to_string(),
                description: "Search for files by name pattern under a given directory path.".to_string(),
                parameters: vec![
                    ToolParam { name: "directory".to_string(), param_type: "string".to_string(), description: "Directory to search in (e.g. /home/root)".to_string(), required: true },
                    ToolParam { name: "pattern".to_string(), param_type: "string".to_string(), description: "Filename pattern to search for (case-insensitive substring match)".to_string(), required: true },
                ],
                handler: Box::new(move |args: Value| {
                    let vfs = vfs.clone();
                    Box::pin(async move {
                        let dir = args.get("directory").and_then(|v| v.as_str()).unwrap_or("/").to_string();
                        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let mut results = Vec::new();
                        search_recursive(&vfs, &dir, &pattern, &mut results, 0, 4).await;
                        Ok(serde_json::json!({ "pattern": pattern, "found": results.len(), "paths": results }))
                    })
                }),
            });
        }

        // ── Build Memory ──
        let memory = if let Some(db) = &ctx.db {
            let lt = neura_ai_core::memory::long_term::LongTermMemory::new(db.clone(), "shell_agent".to_string());
            MemoryManager::new(lt)
        } else {
            MemoryManager::new_ephemeral()
        };

        // ── Build System Prompt ──
        let system_prompt = format!(
            "You are NeuraOS AI Agent — an intelligent, fully agentic assistant embedded in NeuraOS terminal OS.\n\
             You have tools to manage files, notes, tasks, contacts, calculations, and system info.\n\
             Current context: user='{}', cwd='{}', hostname='{}'\n\
             \n\
             IMPORTANT AGENT RULES:\n\
             1. When asked to DO something (create a file, add a task, make a note), USE YOUR TOOLS. Don't just explain.\n\
             2. For complex requests, chain multiple tool calls: e.g. 'make a docs file summarizing my todos' requires: list_tasks → write_file.\n\
             3. VFS paths: user files are at /home/{}/... Use /home/{}/documents/ for docs, /home/{}/notes/ for notes.\n\
             4. After using tools, briefly confirm what you did. Keep responses concise and terminal-friendly.\n\
             5. If a tool call fails, try an alternative approach or explain why it cannot be done.\n\
             6. You can chain as many tool calls as needed to fully complete the user's request.",
            username, cwd, hostname, username, username, username
        );

        // ── Run Agent ──
        let engine = AgentEngine::new(client.clone(), tools, memory, system_prompt)
            .with_history(history)
            .with_max_steps(15);

        match engine.run(&prompt).await {
            Ok(text) => {
                if !text.is_empty() {
                    // Update conversation history
                    ctx.ai_history.push(ChatMessage::user(prompt));
                    ctx.ai_history.push(ChatMessage::assistant(text.clone()));

                    // Keep history manageable (last 20 messages)
                    if ctx.ai_history.len() > 20 {
                        let remove_count = ctx.ai_history.len() - 20;
                        ctx.ai_history.drain(0..remove_count);
                    }
                    
                    ctx.save_ai_history().await;

                    format!("[AI/{}] {}", provider_name, text)
                } else {
                    format!("[AI/{}] (empty response)", provider_name)
                }
            }
            Err(e) => format!("[AI/{}] Error: {}", provider_name, e),
        }
    }

    fn help_text() -> String {
        [
            "  NeuraOS Shell Commands:",
            "",
            "  Filesystem:",
            "    ls [path]          List directory contents",
            "    cd <path>          Change directory",
            "    mkdir <dir>        Create directory",
            "    touch <file>       Create empty file",
            "    cat <file>         Display file contents",
            "    write <file> <txt> Write text to file",
            "    rm <path>          Remove file/directory",
            "    stat <path>        Show file metadata",
            "    tree [path]        Show directory tree",
            "    pwd                Print working directory",
            "",
            "  System:",
            "    sysinfo            System information",
            "    neofetch           System info (fancy)",
            "    apps               List available apps",
            "    open <app>         Open an application",
            "    version            Show version",
            "",
            "  User management (admin only):",
            "    userlist           List all user accounts",
            "    useradd <u> <p> [role]  Add user (roles: admin, user, guest)",
            "    userdel <username> Delete a user account",
            "    passwd [user] <pw> Change password",
            "",
            "  AI Agent (fully agentic with tools):",
            "    ai <prompt>        Ask the AI agent to do things",
            "    Examples:",
            "      ai list my tasks",
            "      ai create a note called Meeting with the agenda",
            "      ai make a docs file summarizing my todo list",
            "      ai what files do I have in /home/root?",
            "      ai add a contact named Alice with email alice@example.com",
            "      ai calculate 2^10 + (50 / 5)",
            "",
            "  Other:",
            "    echo <text>        Print text",
            "    whoami             Show current user",
            "    hostname           Show hostname",
            "    date               Show current date/time",
            "    clear              Clear screen",
            "    help               Show this help",
            "    exit               Shutdown NeuraOS",
            "",
            "  Keyboard: Ctrl+P palette | Ctrl+H help | Ctrl+D exit",
        ].join("\n")
    }
}

// ── Math Evaluator ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token { Num(f64), Plus, Minus, Star, Slash, Caret, LParen, RParen }

fn eval_math(expr: &str) -> Option<f64> {
    let tokens = tokenize_math(expr).ok()?;
    let mut pos = 0usize;
    let result = parse_add(&tokens, &mut pos).ok()?;
    if pos < tokens.len() { return None; }
    Some(result)
}

fn tokenize_math(expr: &str) -> Result<Vec<Token>, ()> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => { i += 1; }
            '+' => { tokens.push(Token::Plus); i += 1; }
            '-' => { tokens.push(Token::Minus); i += 1; }
            '*' => { tokens.push(Token::Star); i += 1; }
            '/' => { tokens.push(Token::Slash); i += 1; }
            '^' => { tokens.push(Token::Caret); i += 1; }
            '(' => { tokens.push(Token::LParen); i += 1; }
            ')' => { tokens.push(Token::RParen); i += 1; }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Num(s.parse().map_err(|_| ())?));
            }
            _ => return Err(()),
        }
    }
    Ok(tokens)
}

fn parse_add(tokens: &[Token], pos: &mut usize) -> Result<f64, ()> {
    let mut left = parse_mul(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Plus => { *pos += 1; left += parse_mul(tokens, pos)?; }
            Token::Minus => { *pos += 1; left -= parse_mul(tokens, pos)?; }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_mul(tokens: &[Token], pos: &mut usize) -> Result<f64, ()> {
    let mut left = parse_pow(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Star => { *pos += 1; left *= parse_pow(tokens, pos)?; }
            Token::Slash => {
                *pos += 1;
                let right = parse_pow(tokens, pos)?;
                if right == 0.0 { return Err(()); }
                left /= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_pow(tokens: &[Token], pos: &mut usize) -> Result<f64, ()> {
    let base = parse_unary(tokens, pos)?;
    if *pos < tokens.len() {
        if let Token::Caret = &tokens[*pos] {
            *pos += 1;
            let exp = parse_pow(tokens, pos)?;
            return Ok(base.powf(exp));
        }
    }
    Ok(base)
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<f64, ()> {
    if *pos < tokens.len() {
        if let Token::Minus = &tokens[*pos] {
            *pos += 1;
            return Ok(-parse_primary(tokens, pos)?);
        }
    }
    parse_primary(tokens, pos)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, ()> {
    if *pos >= tokens.len() { return Err(()); }
    match &tokens[*pos] {
        Token::Num(n) => { let v = *n; *pos += 1; Ok(v) }
        Token::LParen => {
            *pos += 1;
            let val = parse_add(tokens, pos)?;
            if *pos < tokens.len() {
                if let Token::RParen = &tokens[*pos] {
                    *pos += 1;
                    return Ok(val);
                }
            }
            Err(())
        }
        _ => Err(()),
    }
}

// ── Recursive file search helper ──────────────────────────────────────────────

async fn search_recursive(
    vfs: &Arc<neura_storage::vfs::Vfs>,
    path: &str,
    pattern: &str,
    results: &mut Vec<String>,
    depth: usize,
    max_depth: usize,
) {
    if depth >= max_depth || results.len() >= 50 { return; }
    if let Ok(entries) = vfs.list_dir(path).await {
        for name in &entries {
            let full = if path == "/" { format!("/{}", name) } else { format!("{}/{}", path, name) };
            if name.to_lowercase().contains(pattern) {
                results.push(full.clone());
            }
            // Recurse into subdirectories
            if depth + 1 < max_depth {
                Box::pin(search_recursive(vfs, &full, pattern, results, depth + 1, max_depth)).await;
            }
        }
    }
}
