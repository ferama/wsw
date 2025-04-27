use clap::Parser;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tracing::{error, info};
use windows::{Win32::System::Services::*, core::PWSTR};
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

use crate::cli::{Cli, Commands};

use super::runner::run_command;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
pub const SERVICE_NAME_PREFIX: &str = "wsw";

pub fn get_service_name(name: &str) -> String {
    if name == SERVICE_NAME_PREFIX {
        SERVICE_NAME_PREFIX.to_string()
    } else {
        format!("{}-{}", SERVICE_NAME_PREFIX, name)
    }
}

pub fn service_main(_args: Vec<OsString>) {
    let cli = Cli::parse();
    let cmd_arg;
    let svc_name_arg;
    let working_dir_arg: Option<String>;
    match cli.command {
        Some(Commands::Run {
            cmd,
            working_dir,
            name,
        }) => {
            cmd_arg = cmd.clone();
            svc_name_arg = get_service_name(name.as_str());
            working_dir_arg = working_dir;
        }
        _ => {
            panic!("Service main called without --cmd argument");
        }
    }

    let running = Arc::new(AtomicBool::new(true));
    let stop_flag = running.clone();

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

    let running_bg = Arc::clone(&running);

    while running_bg.load(Ordering::SeqCst) {
        if let Ok(mut child) = run_command(&cmd_arg, working_dir_arg.clone()) {
            info!("Child process started with PID: {}", child.id());

            // Poll for shutdown
            while running_bg.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(1));
                let exited = {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            error!("Child exited with status: {}", status);
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            info!("Failed to check child status: {}", e);
                            true
                        }
                    }
                };
                if exited {
                    break;
                }
            }

            let _ = child.kill();
            thread::sleep(Duration::from_secs(1));
        }
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

pub fn install_service(
    name: &str,
    working_dir: Option<String>,
    service_cmd: &str,
) -> windows_service::Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let executable_path = std::env::current_exe().unwrap();

    let mut launch_arguments = vec![
        OsString::from("run"),
        OsString::from("--cmd"),
        OsString::from(service_cmd),
        OsString::from("--name"),
        OsString::from(name),
    ];

    if let Some(dir) = working_dir {
        launch_arguments.push(OsString::from("--working-dir"));
        launch_arguments.push(OsString::from(dir));
    }

    let service_info = ServiceInfo {
        name: OsString::from(name),
        display_name: OsString::from("Windows Service Wrapper"),
        service_type: SERVICE_TYPE,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable_path.into(),
        launch_arguments,
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = service_manager.create_service(&service_info, ServiceAccess::START)?;

    service.start::<std::ffi::OsString>(&[])?;
    Ok(())
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

    service.stop()?;
    match wait_for_service_status(
        &name,
        ServiceState::Stopped,
        std::time::Duration::from_secs(10),
    ) {
        Ok(_) => tracing::info!("Service '{}' is now stopped.", name),
        Err(e) => tracing::error!("Failed to wait for service '{}': {}", name, e),
    }

    // Now delete it
    service.delete()?;
    Ok(())
}

pub fn start_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::START | ServiceAccess::QUERY_STATUS)?;

    // Start the service
    service.start::<std::ffi::OsString>(&[])?;
    Ok(())
}

pub fn get_service_status(name: &str) -> windows_service::Result<ServiceStatus> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_STATUS)?;

    // Query the service status
    let status = service.query_status()?;
    Ok(status)
}

pub fn wait_for_service_status(
    name: &str,
    target_state: ServiceState,
    timeout: Duration,
) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_STATUS)?;

    // Wait for the service to reach the target state
    loop {
        let status = service.query_status()?;
        let start = std::time::Instant::now();
        if start.elapsed() > timeout {
            tracing::error!("Timeout waiting for service status to change");
            return Err(windows_service::Error::Winapi(io::Error::new(
                io::ErrorKind::TimedOut,
                "operation timed out",
            )));
        }
        if status.current_state == target_state {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Ok(())
}

pub fn stop_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(
        name,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
    )?;

    // Stop the service
    service.stop()?;
    Ok(())
}

pub fn list_services_with_status() -> Vec<(String, String)> {
    let mut service_list = Vec::new();

    unsafe {
        let scm_handle_res = OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE);
        if scm_handle_res.is_err() {
            error!(
                "Failed to open service control manager: {:?}",
                scm_handle_res.err()
            );
            return service_list;
        }
        let scm_handle = scm_handle_res.unwrap();
        if scm_handle.0.is_null() {
            panic!("Failed to open service control manager");
        }

        let mut bytes_needed = 0u32;
        let mut services_returned = 0u32;
        let mut resume_handle: u32 = 0;

        // First call to get buffer size
        let _ = EnumServicesStatusExW(
            scm_handle,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut bytes_needed,
            &mut services_returned,
            Some(&mut resume_handle),
            None,
        );

        let mut buffer: Vec<u8> = vec![0; bytes_needed as usize];

        let _ = EnumServicesStatusExW(
            scm_handle,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(&mut buffer),
            &mut bytes_needed,
            &mut services_returned,
            Some(&mut resume_handle),
            None,
        );

        let services = std::slice::from_raw_parts(
            buffer.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW,
            services_returned as usize,
        );

        for svc in services {
            let name = widestring_to_string(svc.lpServiceName);
            // info!("Service Name: {}", name);
            if name.starts_with(SERVICE_NAME_PREFIX) {
                let status = match svc.ServiceStatusProcess.dwCurrentState {
                    SERVICE_RUNNING => "Running".to_string(),
                    SERVICE_STOPPED => "Stopped".to_string(),
                    SERVICE_START_PENDING => "Start Pending".to_string(),
                    SERVICE_STOP_PENDING => "Stop Pending".to_string(),
                    SERVICE_CONTINUE_PENDING => "Continue Pending".to_string(),
                    SERVICE_PAUSE_PENDING => "Pause Pending".to_string(),
                    SERVICE_PAUSED => "Paused".to_string(),
                    _ => "Unknown".to_string(),
                };
                service_list.push((name, status));
            }
        }

        let _ = CloseServiceHandle(scm_handle);
    }
    return service_list;
}

fn widestring_to_string(ptr: PWSTR) -> String {
    unsafe {
        if ptr.0.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *ptr.0.offset(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr.0, len as usize);
        String::from_utf16_lossy(slice)
    }
}
