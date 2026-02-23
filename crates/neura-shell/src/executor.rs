use crate::parser::ParsedCommand;
use crate::builtins::Builtins;
use crate::context::ShellContext;
use neura_storage::vfs::NodeType;
use neura_app_framework::consts::app_id;

/// Executes parsed shell commands.
pub struct ShellExecutor;

impl ShellExecutor {
    pub async fn execute(cmd: &ParsedCommand, ctx: &mut ShellContext) -> String {
        // Try builtins first
        if let Some(output) = Builtins::try_execute(&cmd.program, &cmd.args, ctx).await {
            return output;
        }

        // Bare app name → open the app
        let prog_lower = cmd.program.to_lowercase();
        if app_id::ALL.contains(&prog_lower.as_str()) {
            return format!("__OPEN_APP__:{}", prog_lower);
        }

        // Looks like a file/directory path → try to resolve and open
        let resolved = ctx.resolve_path(&cmd.program);
        if ctx.vfs.exists(&resolved).await {
            let is_dir = matches!(ctx.vfs.stat(&resolved).await, Ok(ref info) if matches!(info.node_type, NodeType::Directory));
            if is_dir {
                return format!("__OPEN_DIR__:{}", resolved);
            } else {
                return format!("__OPEN_FILE__:{}", resolved);
            }
        }

        // Fall back to unknown command
        format!("neura: command not found: '{}'\n  Tip: type 'help' for commands, 'apps' to list apps, or 'open <app>' to launch an app.", cmd.program)
    }
}
