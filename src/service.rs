use std::{
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
};

use std::ffi::OsString;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
pub const SERVICE_NAME: &str = "WSW";

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

pub fn service_main(args: Vec<OsString>) {
    let running = Arc::new(AtomicBool::new(true));
    let stop_flag = running.clone();
    let watcher = running.clone();

    let event_handler =
        service_control_handler::register(SERVICE_NAME, move |control_event| match control_event {
            ServiceControl::Stop => {
                stop_flag.store(false, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .unwrap();

    event_handler
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .unwrap();

    let args = args
        .into_iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<String>>();
    if let Some(pos) = args.iter().position(|a| a == "--cmd") {
        if args.len() > pos + 1 {
            let full_cmd = args[pos + 1].clone();
            // Spawn a thread to run the child process
            thread::spawn(move || {
                runner(full_cmd, watcher);
            });
        }
    }

    // Main loop
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(1));
    }
    // Update status before exiting
    event_handler
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .expect("set service stopped");
}
