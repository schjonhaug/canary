//! Tracing subscriber setup.

use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Installs the global tracing subscriber.
///
/// Logs go to stdout. When `CANARY_FILE_LOGGING` is `true` or `1` (local
/// development), they are also written to `logs/backend.log` with daily
/// rotation; the directory is created owner-only because logs can contain
/// wallet names.
pub fn init_tracing() -> Result<()> {
    let env_filter = EnvFilter::from_default_env().add_directive("canary=info".parse()?);
    let stdout_layer = fmt::layer().with_writer(std::io::stdout);

    let file_logging_enabled =
        std::env::var("CANARY_FILE_LOGGING").is_ok_and(|value| value == "true" || value == "1");

    if file_logging_enabled {
        use tracing_appender::rolling::{RollingFileAppender, Rotation};

        let log_dir = std::env::current_dir()?.join("logs");
        create_private_dir(&log_dir)?;
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "backend.log");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(stdout_layer)
            .with(fmt::layer().with_writer(file_appender).with_ansi(false))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(stdout_layer)
            .init();
    }

    Ok(())
}

#[cfg(unix)]
fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    // `mode` only applies to newly created directories; tighten existing ones too.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}
