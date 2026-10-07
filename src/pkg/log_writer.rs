use chrono::Local;
use encoding_rs::WINDOWS_1252;
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

/// Lines longer than this are split, so that a child that never writes a
/// newline cannot make wsw buffer an unbounded amount of memory.
const MAX_LINE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamEncoding {
    /// Not enough data seen yet to tell
    Unknown,
    /// UTF-8, or a legacy single byte code page as a fallback
    Bytes,
    /// UTF-16LE, as written by some Windows tools
    Utf16Le,
}

/// Splits a byte stream into lines, buffering incomplete lines across
/// writes. Lines are decoded only once complete, so multi-byte characters
/// split between two reads are never mangled: a `\n` byte cannot appear
/// inside a multi-byte UTF-8 sequence, and UTF-16 is split on whole code
/// units.
pub struct LineBuffer {
    buf: Vec<u8>,
    encoding: StreamEncoding,
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LineBuffer {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            encoding: StreamEncoding::Unknown,
        }
    }

    /// Feeds a chunk of the stream, returning the lines it completed.
    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(data);
        if self.encoding == StreamEncoding::Unknown {
            self.detect_encoding();
            if self.encoding == StreamEncoding::Unknown {
                return Vec::new();
            }
        }

        let mut lines = Vec::new();
        while let Some(end) = self.next_line_end() {
            let raw: Vec<u8> = self.buf.drain(..end).collect();
            self.emit(&raw, &mut lines);
        }
        while self.buf.len() > MAX_LINE_BYTES {
            let cut = self.safe_cut(MAX_LINE_BYTES);
            let raw: Vec<u8> = self.buf.drain(..cut).collect();
            self.emit(&raw, &mut lines);
        }
        lines
    }

    /// Returns the last, unterminated line, if any.
    pub fn finish(&mut self) -> Option<String> {
        if self.encoding == StreamEncoding::Unknown {
            self.encoding = StreamEncoding::Bytes;
        }
        let raw = std::mem::take(&mut self.buf);
        let mut lines = Vec::new();
        self.emit(&raw, &mut lines);
        lines.pop()
    }

    fn detect_encoding(&mut self) {
        const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
        const UTF16_BOM: &[u8] = &[0xFF, 0xFE];
        let b = &self.buf;
        if b.len() < UTF8_BOM.len() && (UTF8_BOM.starts_with(b) || UTF16_BOM.starts_with(b)) {
            // Could still be the beginning of a BOM: wait for more data
            return;
        }
        if b.starts_with(UTF16_BOM) {
            self.buf.drain(..2);
            self.encoding = StreamEncoding::Utf16Le;
        } else if b.starts_with(UTF8_BOM) {
            self.buf.drain(..3);
            self.encoding = StreamEncoding::Bytes;
        } else if b.len() >= 4 && b[0] != 0 && b[1] == 0 && b[3] == 0 {
            // ASCII text encoded as UTF-16LE: every other byte is zero
            self.encoding = StreamEncoding::Utf16Le;
        } else if (b.len() >= 2 && b[1] != 0) || b.len() >= 4 {
            self.encoding = StreamEncoding::Bytes;
        }
    }

    /// Index just past the next line terminator, if a full line is buffered.
    fn next_line_end(&self) -> Option<usize> {
        match self.encoding {
            StreamEncoding::Utf16Le => self
                .buf
                .chunks_exact(2)
                .position(|unit| unit == [b'\n', 0])
                .map(|i| i * 2 + 2),
            _ => self.buf.iter().position(|&b| b == b'\n').map(|i| i + 1),
        }
    }

    /// Largest split point not greater than `max` that does not cut a
    /// character in half.
    fn safe_cut(&self, max: usize) -> usize {
        match self.encoding {
            StreamEncoding::Utf16Le => {
                let mut cut = max - max % 2;
                let high_surrogate = |i: usize| (0xD8..=0xDB).contains(&self.buf[i + 1]);
                if cut >= 2 && high_surrogate(cut - 2) {
                    cut -= 2;
                }
                cut
            }
            _ => match std::str::from_utf8(&self.buf[..max]) {
                // Only an incomplete trailing sequence moves the cut back
                Err(e) if e.error_len().is_none() && e.valid_up_to() > 0 => e.valid_up_to(),
                _ => max,
            },
        }
    }

    fn emit(&self, raw: &[u8], lines: &mut Vec<String>) {
        let text = match self.encoding {
            StreamEncoding::Utf16Le => {
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
            }
            _ => match std::str::from_utf8(raw) {
                Ok(s) => s.to_string(),
                // Not UTF-8: assume the legacy ANSI code page
                Err(_) => WINDOWS_1252.decode(raw).0.into_owned(),
            },
        };
        let line = text.trim_end_matches(['\n', '\r']);
        if !line.is_empty() {
            lines.push(line.to_string());
        }
    }
}

/// Writer that forwards the output of the wrapped process to the log, one
/// line at a time.
#[derive(Default)]
pub struct LogWriter {
    lines: LineBuffer,
}

impl LogWriter {
    pub fn new() -> Self {
        Self::default()
    }

    fn log(line: &str) {
        tracing::info!("{}{}", SERVICE_LOG_PREFIX, line);
    }
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for line in self.lines.push(buf) {
            Self::log(&line);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        // The stream ended: log the last line even if it has no terminator
        if let Some(line) = self.lines.finish() {
            Self::log(&line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&[u8]]) -> Vec<String> {
        let mut buffer = LineBuffer::new();
        let mut lines = Vec::new();
        for chunk in chunks {
            lines.extend(buffer.push(chunk));
        }
        lines.extend(buffer.finish());
        lines
    }

    #[test]
    fn joins_lines_split_across_chunks() {
        let lines = feed(&[b"hel", b"lo wor", b"ld\nsecond", b" line\n"]);
        assert_eq!(lines, vec!["hello world", "second line"]);
    }

    #[test]
    fn keeps_utf8_characters_split_across_chunks() {
        // "è" is C3 A8, "€" is E2 82 AC
        let lines = feed(&[b"caff\xC3", b"\xA8 1\xE2\x82", b"\xAC\r\n"]);
        assert_eq!(lines, vec!["caffè 1€"]);
    }

    #[test]
    fn handles_crlf_and_skips_empty_lines() {
        let lines = feed(&[b"a\r\n\r\n", b"b\r", b"\nc"]);
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn emits_trailing_line_without_newline() {
        let mut buffer = LineBuffer::new();
        assert!(buffer.push(b"no newline").is_empty());
        assert_eq!(buffer.finish().as_deref(), Some("no newline"));
        assert_eq!(buffer.finish(), None);
    }

    #[test]
    fn falls_back_to_windows_1252() {
        let lines = feed(&[b"caf\xE9\n"]);
        assert_eq!(lines, vec!["café"]);
    }

    #[test]
    fn strips_utf8_bom() {
        let lines = feed(&[b"\xEF\xBB", b"\xBFhi\n"]);
        assert_eq!(lines, vec!["hi"]);
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn decodes_utf16_split_on_odd_boundaries() {
        let mut data = vec![0xFF, 0xFE];
        data.extend(utf16("ciao è\r\nsecond 😀\r\n"));
        let chunks: Vec<&[u8]> = data.chunks(3).collect();
        assert_eq!(feed(&chunks), vec!["ciao è", "second 😀"]);
    }

    #[test]
    fn detects_utf16_without_bom() {
        let data = utf16("hello\nworld\n");
        let chunks: Vec<&[u8]> = data.chunks(1).collect();
        assert_eq!(feed(&chunks), vec!["hello", "world"]);
    }

    #[test]
    fn splits_overlong_lines_on_character_boundaries() {
        let mut data = vec![b'x'; MAX_LINE_BYTES - 1];
        data.extend("è".as_bytes()); // crosses the limit
        let lines = feed(&[&data, b"tail\n"]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), MAX_LINE_BYTES - 1);
        assert_eq!(lines[1], "ètail");
    }

    #[test]
    fn short_first_chunks_are_not_lost() {
        assert_eq!(feed(&[b"a", b"\n"]), vec!["a"]);
        assert_eq!(feed(&[b"a"]), vec!["a"]);
    }
}
