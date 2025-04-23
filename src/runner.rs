use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tracing::info;

use crate::logs::LogWriter;

pub fn run_command(cmd: &str) -> Result<Child, std::io::Error> {
    let mut parts = cmd.split_whitespace();
    if let Some(exe) = parts.next() {
        let parent_dir = Path::new(exe).parent().unwrap();
        info!("Executable directory: {:?}", parent_dir);
        let exe_args: Vec<&str> = parts.collect();
        let command = Command::new(exe)
            .args(&exe_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // .current_dir(parent_dir)
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
