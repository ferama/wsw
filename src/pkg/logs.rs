use tracing_appender::non_blocking::WorkerGuard;

use std::env;
use std::path::PathBuf;
use tracing::info;

use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::{Registry, layer::SubscriberExt};

use crate::cli::LogRotation;
use crate::pkg::log_writer::LocalTimer;

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

pub fn get_log_filename_prefix(name: &str) -> String {
    format!("{}.log", name)
}

/// Sets up console and file logging. Failures never abort the process: if
/// the log file cannot be created wsw keeps running with console logging only.
pub fn setup_logging(
    name: &str,
    log_rotation: LogRotation,
    max_log_files: usize,
) -> Option<WorkerGuard> {
    let log_path = get_log_dir();

    let file_appender = rolling::Builder::new()
        .filename_prefix(get_log_filename_prefix(name))
        .rotation(log_rotation.into())
        .max_log_files(max_log_files)
        .build(&log_path);

    let (file_layer, guard) = match file_appender {
        Ok(file_appender) => {
            let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);
            let file_layer = fmt::layer()
                .with_writer(non_blocking_file)
                .with_target(false)
                .with_timer(LocalTimer)
                .with_ansi(false); // Disable ANSI escape codes
            (Some(file_layer), Some(guard))
        }
        Err(e) => {
            eprintln!("Failed to create log file in {:?}: {}", log_path, e);
            (None, None)
        }
    };

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

    guard
}
