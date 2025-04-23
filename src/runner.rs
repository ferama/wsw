use std::{
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
        let exe_args: Vec<&str> = parts.collect();
        loop {
            println!("Starting child process: {}", exe);
            match Command::new(exe)
                .args(&exe_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
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
