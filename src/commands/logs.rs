use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
};

use crate::pkg::{
    logs::{self, get_log_filename_prefix},
    service::get_service_name,
};

pub fn handle(name: &str, follow: bool) {
    let svc_name = get_service_name(&name);
    let _guard = logs::setup_logging(&svc_name);

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
                            .map(|f| f.starts_with(get_log_filename_prefix(name).as_str()))
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
                        println!("{}", line);
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
                            println!("{}", line);
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
