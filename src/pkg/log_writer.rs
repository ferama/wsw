use chrono::Local;
use encoding_rs::{Encoding, WINDOWS_1252};
use std::io::{self, Write};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

use crate::pkg::logs::SERVICE_LOG_PREFIX;

pub struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

pub struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let decoded = LogWriter::try_decode(buf);

        match decoded {
            Some(text) => {
                for line in text.lines() {
                    if !line.is_empty() {
                        tracing::info!("{}{}", SERVICE_LOG_PREFIX, line);
                    }
                }
            }
            None => {
                tracing::error!("{}<unreadable data: {:?}>", SERVICE_LOG_PREFIX, buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl LogWriter {
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
}
