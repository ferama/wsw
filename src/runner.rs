use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tracing::{error, info};

use crate::logs::LogWriter;

pub fn runner(cmd: String, watcher: Arc<AtomicBool>) {
    info!("Starting runner with command: '{}'", cmd);
    let mut parts = cmd.split_whitespace();
    if let Some(exe) = parts.next() {
        let parent_dir = Path::new(exe).parent().unwrap();
        info!("Executable directory: {:?}", parent_dir);
        let exe_args: Vec<&str> = parts.collect();

        loop {
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

            match command {
                Ok(child) => {
                    info!("Child process started with PID: {}", child.id());
                    let process_closed = Arc::new(AtomicBool::new(false));
                    let process_closed_clone = Arc::clone(&process_closed);
                    let child = Arc::new(Mutex::new(child));
                    let child_clone = Arc::clone(&child);
                    let watcher_clone = Arc::clone(&watcher);
                    let watcher_thread = thread::spawn(move || {
                        loop {
                            if !watcher_clone.load(Ordering::SeqCst) {
                                let mut child = child_clone.lock().unwrap();
                                let _ = child.kill().map_err(|e| {
                                    error!("Failed to kill child process: {}", e);
                                });
                                break;
                            }
                            if process_closed_clone.load(Ordering::SeqCst) {
                                break;
                            }
                            thread::sleep(Duration::from_secs(1));
                        }
                    });
                    let mut child = child.lock().unwrap();
                    if let Err(e) = child.wait() {
                        error!("Failed to wait for child process: {}", e);
                    }
                    error!("Child process exited. Restarting...");
                    process_closed.store(true, Ordering::SeqCst);
                    watcher_thread.join().unwrap();
                    thread::sleep(Duration::from_secs(2));
                }
                Err(e) => {
                    error!("Failed to start child process: {}", e);
                    thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }
}
