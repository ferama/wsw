//! Log file writer rotating by time and, optionally, by size.
//!
//! File names follow the scheme of `tracing-appender` so that the `logs`
//! command finds them: `<prefix>.<period>` (or just `<prefix>` without time
//! rotation), with a `.<n>` suffix for the files created when the current
//! one exceeds the maximum size. Only the `max_files` most recent files are
//! kept.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::pkg::config::LogRotation;

pub struct RollingFile {
    dir: PathBuf,
    prefix: String,
    rotation: LogRotation,
    max_size: Option<u64>,
    max_files: usize,
    now: Box<dyn Fn() -> DateTime<Local> + Send>,
    /// Name of the current file without the size index
    base: String,
    index: u32,
    file: Option<File>,
    size: u64,
}

impl RollingFile {
    pub fn new(
        dir: &Path,
        prefix: &str,
        rotation: LogRotation,
        max_size: Option<u64>,
        max_files: usize,
    ) -> io::Result<Self> {
        Self::with_clock(
            dir,
            prefix,
            rotation,
            max_size,
            max_files,
            Box::new(Local::now),
        )
    }

    fn with_clock(
        dir: &Path,
        prefix: &str,
        rotation: LogRotation,
        max_size: Option<u64>,
        max_files: usize,
        now: Box<dyn Fn() -> DateTime<Local> + Send>,
    ) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let mut writer = Self {
            dir: dir.to_path_buf(),
            prefix: prefix.to_string(),
            rotation,
            max_size: max_size.filter(|size| *size > 0),
            max_files: max_files.max(1),
            now,
            base: String::new(),
            index: 0,
            file: None,
            size: 0,
        };
        writer.base = writer.current_base();
        writer.index = writer.last_index();
        writer.open()?;
        Ok(writer)
    }

    fn current_base(&self) -> String {
        let format = match self.rotation {
            LogRotation::Minutely => "%Y-%m-%d-%H-%M",
            LogRotation::Hourly => "%Y-%m-%d-%H",
            LogRotation::Daily => "%Y-%m-%d",
            LogRotation::Never => return self.prefix.clone(),
        };
        format!("{}.{}", self.prefix, (self.now)().format(format))
    }

    fn file_name(&self) -> String {
        file_name(&self.base, self.index)
    }

    /// Highest size index already used for the current base, to append to
    /// it after a restart instead of starting over.
    fn last_index(&self) -> u32 {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| parse_index(&self.base, &name))
            .max()
            .unwrap_or(0)
    }

    fn open(&mut self) -> io::Result<()> {
        let path = self.dir.join(self.file_name());
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.size = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.file = Some(file);
        // The file just opened may already be full
        if self.is_full(1) {
            self.index += 1;
            return self.open();
        }
        self.prune();
        Ok(())
    }

    fn is_full(&self, incoming: usize) -> bool {
        match self.max_size {
            Some(max) => self.size > 0 && self.size + incoming as u64 > max,
            None => false,
        }
    }

    /// Deletes the oldest log files, keeping `max_files`.
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, String, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                if !belongs_to(&self.prefix, &name) {
                    return None;
                }
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, name, entry.path()))
            })
            .collect();
        // Newest first; the name breaks ties between files written in the same instant
        files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| natural_cmp(&b.1, &a.1)));
        let current = self.file_name();
        for (_, name, path) in files.into_iter().skip(self.max_files) {
            if name != current {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn file_name(base: &str, index: u32) -> String {
    if index == 0 {
        base.to_string()
    } else {
        format!("{}.{}", base, index)
    }
}

/// Size index of `name` if it is a file of `base`.
fn parse_index(base: &str, name: &str) -> Option<u32> {
    if name == base {
        return Some(0);
    }
    name.strip_prefix(base)?.strip_prefix('.')?.parse().ok()
}

/// Whether `name` is one of the log files with the given prefix: the prefix
/// itself, or followed by a period and/or a size index.
fn belongs_to(prefix: &str, name: &str) -> bool {
    match name.strip_prefix(prefix) {
        Some("") => true,
        Some(rest) => {
            rest.len() > 1
                && rest.starts_with('.')
                && rest[1..]
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
        }
        None => false,
    }
}

/// Compares names so that `x.10` sorts after `x.9`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| -> (String, u32) {
        match s.rsplit_once('.') {
            Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => {
                (head.to_string(), tail.parse().unwrap_or(0))
            }
            _ => (s.to_string(), 0),
        }
    };
    split(a).cmp(&split(b))
}

impl Write for RollingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let base = self.current_base();
        if base != self.base {
            self.base = base;
            self.index = self.last_index();
            self.open()?;
        } else if self.is_full(buf.len()) {
            self.index += 1;
            self.open()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file not open"))?;
        file.write_all(buf)?;
        self.size += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::{Arc, Mutex};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("wsw-rolling-{}-{}", name, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn files(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort_by(|a, b| natural_cmp(a, b));
            names
        }

        fn read(&self, name: &str) -> String {
            fs::read_to_string(self.0.join(name)).unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn clock(time: Arc<Mutex<DateTime<Local>>>) -> Box<dyn Fn() -> DateTime<Local> + Send> {
        Box::new(move || *time.lock().unwrap())
    }

    fn at(day: u32, hour: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn rotates_by_size() {
        let dir = TempDir::new("size");
        let mut log =
            RollingFile::new(&dir.0, "app.log", LogRotation::Never, Some(10), 10).unwrap();
        for line in ["12345\n", "67890\n", "abcde\n", "fghij\n", "k\n"] {
            log.write_all(line.as_bytes()).unwrap();
        }
        assert_eq!(
            dir.files(),
            vec!["app.log", "app.log.1", "app.log.2", "app.log.3"]
        );
        assert_eq!(dir.read("app.log"), "12345\n");
        assert_eq!(dir.read("app.log.1"), "67890\n");
        assert_eq!(dir.read("app.log.3"), "fghij\nk\n");
    }

    #[test]
    fn a_single_oversized_write_is_not_split() {
        let dir = TempDir::new("oversized");
        let mut log = RollingFile::new(&dir.0, "app.log", LogRotation::Never, Some(4), 10).unwrap();
        log.write_all(b"0123456789\n").unwrap();
        log.write_all(b"x\n").unwrap();
        assert_eq!(dir.read("app.log"), "0123456789\n");
        assert_eq!(dir.read("app.log.1"), "x\n");
    }

    #[test]
    fn rotates_by_time_and_size() {
        let dir = TempDir::new("time");
        let time = Arc::new(Mutex::new(at(7, 10)));
        let mut log = RollingFile::with_clock(
            &dir.0,
            "app.log",
            LogRotation::Daily,
            Some(10),
            10,
            clock(time.clone()),
        )
        .unwrap();
        log.write_all(b"day one 1\n").unwrap();
        log.write_all(b"day one 2\n").unwrap();
        *time.lock().unwrap() = at(8, 0);
        log.write_all(b"day two\n").unwrap();
        assert_eq!(
            dir.files(),
            vec![
                "app.log.2026-10-07",
                "app.log.2026-10-07.1",
                "app.log.2026-10-08"
            ]
        );
        assert_eq!(dir.read("app.log.2026-10-08"), "day two\n");
    }

    #[test]
    fn appends_to_the_last_file_after_a_restart() {
        let dir = TempDir::new("restart");
        {
            let mut log =
                RollingFile::new(&dir.0, "app.log", LogRotation::Never, Some(10), 10).unwrap();
            log.write_all(b"aaaaaaaa\n").unwrap();
            log.write_all(b"bbb\n").unwrap();
        }
        let mut log =
            RollingFile::new(&dir.0, "app.log", LogRotation::Never, Some(10), 10).unwrap();
        log.write_all(b"ccc\n").unwrap();
        assert_eq!(dir.files(), vec!["app.log", "app.log.1"]);
        assert_eq!(dir.read("app.log.1"), "bbb\nccc\n");
    }

    #[test]
    fn keeps_only_max_files() {
        let dir = TempDir::new("prune");
        fs::write(dir.0.join("other.log"), "not ours").unwrap();
        fs::write(dir.0.join("app.log.err"), "not ours either").unwrap();
        let mut log = RollingFile::new(&dir.0, "app.log", LogRotation::Never, Some(2), 3).unwrap();
        for i in 0..6 {
            // Distinct modification times
            std::thread::sleep(std::time::Duration::from_millis(20));
            log.write_all(format!("{i}\n").as_bytes()).unwrap();
        }
        assert_eq!(
            dir.files(),
            vec![
                "app.log.3",
                "app.log.4",
                "app.log.5",
                "app.log.err",
                "other.log"
            ]
        );
        assert_eq!(dir.read("app.log.5"), "5\n");
    }

    #[test]
    fn file_name_helpers() {
        assert_eq!(parse_index("a.log", "a.log"), Some(0));
        assert_eq!(parse_index("a.log", "a.log.12"), Some(12));
        assert_eq!(parse_index("a.log", "a.log.x"), None);
        assert_eq!(parse_index("a.log", "a.log2"), None);
        assert!(belongs_to("a.log", "a.log"));
        assert!(belongs_to("a.log", "a.log.2026-10-07.1"));
        assert!(!belongs_to("a.log", "a.logs"));
        assert!(!belongs_to("a.log", "a.log.err"));
        assert!(!belongs_to("a.log", "a.log."));
    }
}
