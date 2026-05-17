//! Tracing setup: JSON file output with daily rotation, optional console mirror.
//!
//! Call [`init`] once at app startup, holding onto the returned `WorkerGuard`
//! until shutdown. Dropping the guard flushes pending log lines, which is
//! critical for the final `app.shutdown.complete` record.

// `SensitiveString` lives in the top-level `redact` module. Logging re-exports
// it for backwards compatibility.
pub use crate::redact::SensitiveString;

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::{SubscriberInitExt, TryInitError};
use tracing_subscriber::EnvFilter;

pub const LOG_FILE_PREFIX: &str = "codexbar";

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("could not initialize file appender at {path}: {source}")]
    Appender {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("tracing subscriber initialization failed: {0}")]
    Subscriber(String),
}

/// Initialize the global tracing subscriber.
///
/// Writes JSON lines to `<logs_dir>/codexbar.YYYY-MM-DD.log` with daily
/// rotation. If `RUST_LOG` is set, a colored console layer mirrors the
/// matching subset to stderr; otherwise the file is the only sink.
///
/// The returned `WorkerGuard` must be held until shutdown. Drop it before
/// exiting so the final lines are flushed.
pub fn init(logs_dir: &Path) -> Result<WorkerGuard, LoggingError> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(7)
        .build(logs_dir)
        .map_err(|source| LoggingError::Appender {
            path: logs_dir.to_path_buf(),
            source: std::io::Error::other(source.to_string()),
        })?;

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let redacting = crate::redact::tracing_layer::RedactingMakeWriter::new(non_blocking);

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_writer(redacting);

    let console_layer = std::env::var_os("RUST_LOG").map(|_| {
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_writer(std::io::stderr)
    });

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);
    let to_logging_error =
        |e: TryInitError| -> LoggingError { LoggingError::Subscriber(e.to_string()) };
    if let Some(console) = console_layer {
        registry
            .with(console)
            .try_init()
            .map_err(to_logging_error)?;
    } else {
        registry.try_init().map_err(to_logging_error)?;
    }

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_log_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = init(dir.path()).expect("init");
        tracing::info!(target: "codexbar::logging::tests", "hello");
        drop(guard);

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .collect();
        assert!(
            !entries.is_empty(),
            "expected at least one rotated log file"
        );
    }
}
