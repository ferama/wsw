use chrono::Local;
use tracing_appender::non_blocking::WorkerGuard;

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use tracing::{error, info};

use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{Registry, layer::SubscriberExt};

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

pub fn setup_logging(name: &str) -> WorkerGuard {
    let log_path: PathBuf = match env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("logs")))
    {
        Some(path) => path,
        None => {
            error!("Failed to get current executable path.");
            PathBuf::from("logs")
        }
    };
    // let file_appender = rolling::daily("logs", name);
    let file_appender = rolling::daily(log_path, name);
    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender); // Set up logging here if needed

    // Console layer (stderr by default, can also write to stdout)
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr) // change to stdout if preferred
        .with_target(false)
        .with_timer(LocalTimer);

    // File layer
    let file_layer = fmt::layer()
        .with_writer(non_blocking_file)
        .with_target(false)
        .with_timer(LocalTimer);

    // Set up subscriber with both layers
    let subscriber = Registry::default()
        .with(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with(console_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set up logging");

    guard
}

pub struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                // Optionally trim trailing newlines or chunk it better
                // info!(target: "LogWriter", "{}", s);
                for line in s.lines() {
                    info!("{}", line);
                }
            }
            Err(_) => {
                info!("<non-utf8 data>");
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
