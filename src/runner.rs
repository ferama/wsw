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

pub fn runner(cmd: String, watcher: Arc<AtomicBool>) {
    let mut parts = cmd.split_whitespace();
    if let Some(exe) = parts.next() {
        if let Some(parent_dir) = Path::new(exe).parent() {
            println!("Executable directory: {:?}", parent_dir);
        }
        let exe_args: Vec<&str> = parts.collect();
        loop {
            println!("bin: '{}', args: '{:?}'", exe, exe_args);
            let command = Command::new(exe)
                .args(&exe_args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // .current_dir(dir)
                .spawn()
                .map(|mut child| {
                    if let Some(stdout) = child.stdout.take() {
                        if let Some(stderr) = child.stderr.take() {
                            let log_file = std::fs::File::create("output.log").unwrap();
                            let stdout = Arc::new(Mutex::new(stdout));
                            let stderr = Arc::new(Mutex::new(stderr));
                            let log_file = Arc::new(Mutex::new(log_file));

                            let stdout_clone = Arc::clone(&stdout);
                            let log_file_clone = Arc::clone(&log_file);
                            thread::spawn(move || {
                                let _ = std::io::copy(
                                    &mut *stdout_clone.lock().unwrap(),
                                    &mut *log_file_clone.lock().unwrap(),
                                );
                            });

                            let stderr_clone = Arc::clone(&stderr);
                            let log_file_clone = Arc::clone(&log_file);
                            thread::spawn(move || {
                                let _ = std::io::copy(
                                    &mut *stderr_clone.lock().unwrap(),
                                    &mut *log_file_clone.lock().unwrap(),
                                );
                            });
                        }
                    }
                    child
                });

            match command {
                Ok(child) => {
                    println!("Child process started with PID: {}", child.id());
                    let child = Arc::new(Mutex::new(child));
                    let child_clone = Arc::clone(&child);
                    let watcher_clone = Arc::clone(&watcher);
                    let watcher_thread = thread::spawn(move || {
                        loop {
                            if !watcher_clone.load(Ordering::SeqCst) {
                                let mut child = child_clone.lock().unwrap();
                                let _ = child.kill().map_err(|e| {
                                    eprintln!("Failed to kill child process: {}", e);
                                });
                                break;
                            }
                            thread::sleep(Duration::from_secs(1));
                        }
                    });
                    let mut child = child.lock().unwrap();
                    if let Err(e) = child.wait() {
                        eprintln!("Failed to wait for child process: {}", e);
                    }
                    watcher_thread.join().unwrap();
                    eprintln!("Child process exited. Restarting...");
                    thread::sleep(Duration::from_secs(2));
                }
                Err(e) => {
                    eprintln!("Failed to start child process: {}", e);
                    thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }
}
