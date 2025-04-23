use clap::Parser;
use std::{
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tracing::{error, info};
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
};

use windows_service::service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use std::ffi::OsString;

use crate::{
    cli::{Cli, Commands},
    runner::run_command,
};

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
pub const SERVICE_NAME_PREFIX: &str = "WSW";

pub fn service_main(_args: Vec<OsString>) {
    let cli = Cli::parse();
    let cmd_arg;
    let svc_name_arg;
    match cli.command {
        Some(Commands::Run { cmd, name }) => {
            if let Some(cmd) = cmd {
                cmd_arg = cmd.clone();
                svc_name_arg = name.clone();
                info!("#### within service_main(): '{:?}', '{:?}'", cmd, name);
            } else {
                panic!("--cmd is required with run");
            }
        }
        _ => {
            panic!("Service main called without --cmd argument");
        }
    }

    let running = Arc::new(AtomicBool::new(true));
    let stop_flag = running.clone();

    // TODO: get service name from args
    let event_handler =
        service_control_handler::register(svc_name_arg, move |control_event| match control_event {
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

    let g_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let g_child_bg = Arc::clone(&g_child);

    thread::spawn(move || {
        loop {
            if let Ok(child) = run_command(&cmd_arg) {
                info!("Child process started with PID: {}", child.id());

                {
                    let mut lock = g_child_bg.lock().unwrap();
                    *lock = Some(child);
                }

                let maybe_child = g_child_bg.lock().unwrap().take();

                if let Some(mut child) = maybe_child {
                    if let Err(e) = child.wait() {
                        error!("Failed to wait for child process: {}", e);
                    }
                }

                // {
                //     let mut lock = g_child_bg.lock().unwrap();
                //     *lock = None;
                // }

                error!("Child process exited. Restarting...");
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
    // Main loop
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(1));
    }
    // TODO: process does not exit when service is stopped

    let mut g_child = g_child.lock().unwrap();
    info!("=== child: {:?}", g_child);
    if let Some(mut child) = g_child.take() {
        info!("Stopping child process with PID: {}", child.id());
        child.kill().expect("Failed to kill child process");
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

pub fn install_service(name: &str, service_cmd: String) {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .expect("Failed to connect to service manager");

    let executable_path = ::std::env::current_exe().unwrap();
    let cmd_line = format!(
        "\"{} run --cmd {}\"",
        executable_path.display(),
        service_cmd
    );
    info!("CmdLine: '{}'", cmd_line);

    let service_info = ServiceInfo {
        name: OsString::from(name),
        display_name: OsString::from("Windows Service Wrapper"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable_path.into(),
        launch_arguments: vec![
            OsString::from("run"),
            OsString::from("--cmd"),
            OsString::from(service_cmd),
        ],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = service_manager
        .create_service(&service_info, ServiceAccess::START)
        .expect("Failed to create service");

    service
        .start::<std::ffi::OsString>(&[])
        .expect("Failed to start service");
    info!("Service installed and started successfully");
}

pub fn uninstall_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(
        name,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    )?;

    // Stop the service if it's running
    let status = service.query_status()?;
    if status.current_state != ServiceState::Stopped {
        service.stop()?;

        // Wait for the service to stop
        let timeout = std::time::Duration::from_secs(10);
        let start = std::time::Instant::now();
        loop {
            let status = service.query_status()?;
            if status.current_state == ServiceState::Stopped {
                break;
            }
            if start.elapsed() > timeout {
                eprintln!("Timeout waiting for service to stop");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    // Now delete it
    service.delete()?;
    info!("Service '{}' uninstalled successfully.", name);
    Ok(())
}
