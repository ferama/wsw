use tracing_appender::non_blocking::WorkerGuard;

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::info;
use tracing_appender::non_blocking::NonBlocking;

use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::{Registry, layer::SubscriberExt};

use crate::pkg::config::LogConfig;
use crate::pkg::log_writer::LocalTimer;
use crate::pkg::rolling::RollingFile;

pub const SERVICE_LOG_PREFIX: &str = "|SVC-LOG| ";

pub fn get_log_dir() -> PathBuf {
    match env::var("PROGRAMDATA") {
        Ok(path) => {
            let log_path = PathBuf::from(path).join("wsw").join("logs");
            std::fs::create_dir_all(&log_path).unwrap_or_else(|_| {
                // logs is not ready here, so use eprintln! and not error!
                eprintln!("Failed to create log directory: {:?}", log_path);
            });
            log_path
        }
        Err(_) => {
            eprintln!("Failed to get PROGRAMDATA environment variable.");
            let log_path: PathBuf = match env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("logs")))
            {
                Some(path) => path,
                None => {
                    eprintln!("Failed to get current executable path.");
                    PathBuf::from("logs")
                }
            };
            log_path
        }
    }
}

/// Log directory of a service: the configured one or the default.
pub fn log_dir(custom: Option<&str>) -> PathBuf {
    match custom {
        Some(dir) => {
            let log_path = PathBuf::from(dir);
            if let Err(e) = std::fs::create_dir_all(&log_path) {
                // logs is not ready here, so use eprintln! and not error!
                eprintln!("Failed to create log directory {:?}: {}", log_path, e);
            }
            log_path
        }
        None => get_log_dir(),
    }
}

pub fn get_log_filename_prefix(name: &str) -> String {
    format!("{}.log", name)
}

/// Prefix of the files holding the stderr of the wrapped process when the
/// output is split (--log-split).
pub fn get_err_log_filename_prefix(name: &str) -> String {
    format!("{}.err.log", name)
}

/// Keeps the background log writers alive: logs are flushed on drop.
pub struct LogGuards(#[allow(dead_code)] Vec<WorkerGuard>);

/// Where the stderr lines of the wrapped process go when the output is
/// split, None when they go to the main log.
static STDERR_LOG: OnceLock<NonBlocking> = OnceLock::new();

/// Writes a line of the wrapped process stderr to its own log file, if the
/// output is split. Returns false if the line must go to the main log.
pub fn write_stderr_line(line: &str) -> bool {
    match STDERR_LOG.get() {
        Some(writer) => {
            let mut writer = writer.clone();
            let _ = writeln!(
                writer,
                "{} {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                line
            );
            true
        }
        None => false,
    }
}

fn file_writer(
    dir: &Path,
    prefix: &str,
    config: &LogConfig,
) -> std::io::Result<(NonBlocking, WorkerGuard)> {
    // tracing-appender cannot rotate by size: use our own writer when needed
    if config.max_size.is_some() || config.split {
        let file = RollingFile::new(
            dir,
            prefix,
            config.rotation,
            config.max_size,
            config.max_files,
        )?;
        return Ok(tracing_appender::non_blocking(file));
    }
    let file = rolling::Builder::new()
        .filename_prefix(prefix)
        .rotation(config.rotation.into())
        .max_log_files(config.max_files)
        .build(dir)
        .map_err(std::io::Error::other)?;
    Ok(tracing_appender::non_blocking(file))
}

/// Sets up console and file logging. Failures never abort the process: if
/// the log file cannot be created wsw keeps running with console logging only.
pub fn setup_logging(name: &str, config: &LogConfig) -> LogGuards {
    let log_path = log_dir(config.dir.as_deref());
    let mut guards = Vec::new();

    let file_layer = match file_writer(&log_path, &get_log_filename_prefix(name), config) {
        Ok((non_blocking_file, guard)) => {
            guards.push(guard);
            let file_layer = fmt::layer()
                .with_writer(non_blocking_file)
                .with_target(false)
                .with_timer(LocalTimer)
                .with_ansi(false); // Disable ANSI escape codes
            Some(file_layer)
        }
        Err(e) => {
            eprintln!("Failed to create log file in {:?}: {}", log_path, e);
            None
        }
    };

    if config.split {
        match file_writer(&log_path, &get_err_log_filename_prefix(name), config) {
            Ok((writer, guard)) => {
                guards.push(guard);
                let _ = STDERR_LOG.set(writer);
            }
            Err(e) => eprintln!("Failed to create stderr log file in {:?}: {}", log_path, e),
        }
    }

    // Console layer (stderr by default, can also write to stdout)
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr) // change to stdout if preferred
        .with_target(false)
        .with_timer(LocalTimer);

    // Set up subscriber with both layers
    let subscriber = Registry::default()
        .with(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into()))
        .with(console_layer)
        .with(file_layer);

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("Failed to set up logging: {}", e);
    }

    info!("Log path: {:?}", log_path);

    LogGuards(guards)
}
