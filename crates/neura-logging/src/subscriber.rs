use std::path::Path;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_appender::rolling;

/// Initialize the global tracing subscriber.
///
/// Logs are written ONLY to a daily-rotating file so that no tracing output
/// leaks to stdout/stderr and corrupts the ratatui TUI.  To read logs while
/// running, tail the file at `{neura_home}/logs/neuraos.log`.
pub fn init_logging(log_dir: &Path, log_level: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            EnvFilter::new(format!("neura={},warn", log_level))
        });

    // File appender: daily rotation, non-blocking so it never stalls the UI.
    std::fs::create_dir_all(log_dir).ok();
    let file_appender = rolling::daily(log_dir, "neuraos.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so the background writer thread stays alive forever.
    std::mem::forget(guard);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
        )
        .init();
}
