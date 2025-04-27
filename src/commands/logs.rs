use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
};

use crate::pkg::{
    logs::{self, SERVICE_LOG_PREFIX, get_log_filename_prefix},
    service::get_service_name,
};

pub fn handle(name: &str, follow: bool, full: bool) {
    let svc_name = get_service_name(&name);

    let log_dir = logs::get_log_dir();
    let res = fs::read_dir(log_dir.clone());
    match res {
        Ok(content) => {
            let mut log_files: Vec<PathBuf> = content
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .map(|f| f.starts_with(get_log_filename_prefix(&svc_name).as_str()))
                            .unwrap_or(false)
                })
                .collect();

            if log_files.is_empty() {
                eprintln!("No log files found in {}", log_dir.display());
                return;
            }
            log_files.sort_by_key(|path| {
                fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });
            let latest_log = log_files.pop().unwrap();
            let file = File::open(&latest_log)
                .unwrap_or_else(|_| panic!("Failed to open log file: {:?}", latest_log));

            if !follow {
                let reader = BufReader::new(file.try_clone().unwrap());
                for line_result in reader.lines() {
                    if let Ok(line) = line_result {
                        if full {
                            println!("{}", line);
                        } else {
                            if let Some(message) = extract_message(&line) {
                                println!("{}", message);
                            }
                        }
                    } else {
                        eprintln!("Failed to read line from log file: {:?}", latest_log);
                    }
                }
            } else {
                loop {
                    let reader = BufReader::new(file.try_clone().unwrap());
                    let mut lines = reader.lines().peekable();
                    while lines.peek().is_some() {
                        if let Some(Ok(line)) = lines.next() {
                            if full {
                                println!("{}", line);
                            } else {
                                if let Some(message) = extract_message(&line) {
                                    println!("{}", message);
                                }
                            }
                        } else {
                            eprintln!("Failed to read line from log file: {:?}", latest_log);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read log directory: {}", e);
            return;
        }
    }
}

/// Extracts the log message from a log line formatted as:
/// `2025-04-27 00:59:24  LEVEL  MESSAGE`
fn extract_message(line: &str) -> Option<&str> {
    // Skip timestamp (first 19 characters) + 2 spaces
    if line.len() < 21 {
        return None;
    }
    let after_timestamp = &line[21..];

    // Find the next space after LEVEL
    if let Some(space_idx) = after_timestamp.find(' ') {
        let message_start = 21 + space_idx + 1;
        if message_start < line.len() {
            let message = &line[message_start..];
            if message.trim_start().starts_with(SERVICE_LOG_PREFIX) {
                Some(message.trim_start()[SERVICE_LOG_PREFIX.len()..].trim())
            } else {
                // If the message doesn't start with the service log prefix, return None
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}
