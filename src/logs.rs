use chrono::Local;
use encoding_rs::{Encoding, WINDOWS_1252};
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
    let log_path = match env::var("PROGRAMDATA") {
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
    };
    let file_appender = rolling::daily(log_path.clone(), name);
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
        .with_timer(LocalTimer)
        .with_ansi(false); // Disable ANSI escape codes

    // Set up subscriber with both layers
    let subscriber = Registry::default()
        .with(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with(console_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set up logging");

    info!("Log path: {:?}", log_path);

    guard
}

pub struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let decoded = try_decode(buf);

        match decoded {
            Some(text) => {
                for line in text.lines() {
                    if !line.trim().is_empty() {
                        info!("{}", line);
                    }
                }
            }
            None => {
                error!("<unreadable data: {:?}>", buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn try_decode(buf: &[u8]) -> Option<String> {
    // 1. Try UTF-8
    if let Ok(s) = std::str::from_utf8(buf) {
        return Some(s.to_string());
    }

    // 2. Try UTF-16LE (only if even length)
    if buf.len() % 2 == 0 {
        let utf16: Vec<u16> = buf
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if let Ok(s) = String::from_utf16(&utf16) {
            return Some(s);
        }
    }

    // 3. Try Windows-1252
    let (s_win1252, _, had_errors) = WINDOWS_1252.decode(buf);
    if !had_errors {
        return Some(s_win1252.into_owned());
    }

    // 4. Try CP437 (OEM US)
    if let Some(cp437) = Encoding::for_label(b"ibm437") {
        let (s, _, had_errors) = cp437.decode(buf);
        if !had_errors {
            return Some(s.into_owned());
        }
    }

    // 5. Give up
    None
}
