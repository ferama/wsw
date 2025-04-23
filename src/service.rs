use std::{
    sync::{
        Arc,
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

use crate::runner::runner;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
pub const SERVICE_NAME: &str = "WSW";

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
