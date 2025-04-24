use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tracing::info;
use which::which;

use crate::logs::LogWriter;

fn find_working_dir(exe: &str, working_dir: Option<String>) -> PathBuf {
    let mut svc_working_dir: PathBuf = Path::new(".").to_path_buf();
    if let Some(dir) = working_dir {
        svc_working_dir = PathBuf::from(dir);
    } else {
        if let Some(parent) = Path::new(exe).parent() {
            svc_working_dir = Path::new(parent).to_path_buf();
        }
    }

    if svc_working_dir == Path::new("") {
        match which(exe) {
            Ok(path) => {
                if let Some(parent) = path.parent() {
                    svc_working_dir = Path::new(parent).to_path_buf();
                }
            }
            Err(_) => {}
        }
    }

    svc_working_dir
}

pub fn run_command(cmd: &str, working_dir: Option<String>) -> Result<Child, std::io::Error> {
    let mut parts = cmd.split_whitespace();
    if let Some(exe) = parts.next() {
        let svc_working_dir = find_working_dir(exe, working_dir);
        info!("Command: {:?}", cmd);
        info!("Executable directory: {:?}", svc_working_dir);
        let exe_args: Vec<&str> = parts.collect();
        let command = Command::new(exe)
            .args(&exe_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(svc_working_dir)
            .spawn()
            .map(|mut child| {
                if let Some(stdout) = child.stdout.take() {
                    if let Some(stderr) = child.stderr.take() {
                        let logger = LogWriter;

                        let stdout = Arc::new(Mutex::new(stdout));
                        let stderr = Arc::new(Mutex::new(stderr));
                        let logger = Arc::new(Mutex::new(logger));

                        let stdout_clone = Arc::clone(&stdout);
                        let logger_clone = Arc::clone(&logger);
                        thread::spawn(move || {
                            let _ = std::io::copy(
                                &mut *stdout_clone.lock().unwrap(),
                                &mut *logger_clone.lock().unwrap(),
                            );
                        });

                        let stderr_clone = Arc::clone(&stderr);
                        let logger_clone = Arc::clone(&logger);
                        thread::spawn(move || {
                            let _ = std::io::copy(
                                &mut *stderr_clone.lock().unwrap(),
                                &mut *logger_clone.lock().unwrap(),
                            );
                        });
                    }
                }
                child
            });
        return command;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("Command not found: {}", cmd),
    ))
}
