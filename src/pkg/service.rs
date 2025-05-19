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
use windows::{
    Win32::{Foundation::CloseHandle, System::Services::*},
    core::{PCWSTR, PWSTR},
};
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
    let no_logs: bool;

    match cli.command {
        Some(Commands::Run {
            cmd,
            working_dir,
            name,
            disable_logs,
        }) => {
            cmd_arg = cmd.clone();
            svc_name_arg = get_service_name(name.as_str());
            working_dir_arg = working_dir;
            no_logs = disable_logs;
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
        if let Ok(mut process) = run_command(&cmd_arg, working_dir_arg.clone(), no_logs) {
            info!("Child process started with PID: {}", process.1.id());

            // Poll for shutdown
            while running_bg.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(1));
                let exited = {
                    match process.1.try_wait() {
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

            let _ = process.1.kill();
            unsafe {
                if let Err(e) = CloseHandle(std::mem::transmute(process.0)) {
                    error!("Failed to close handle: {:?}", e);
                }
            }
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
    disable_logs: bool,
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
    if disable_logs {
        launch_arguments.push(OsString::from("--disable-logs"));
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

    let _ = service.stop().is_err(); // Ignore error if service is already stopped
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

pub fn get_service_command_line(name: &str) -> windows_service::Result<String> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_CONFIG)?;

    let service_handle = SC_HANDLE(service.raw_handle());
    // Query the service info
    unsafe {
        // Query needed buffer size first
        let mut needed = 0u32;
        let result = QueryServiceConfigW(service_handle, None, 0, &mut needed);

        if result.is_ok() {
            return Err(windows_service::Error::Winapi(io::Error::new(
                io::ErrorKind::Other,
                "Unexpected result while querying service config",
            )));
        }

        // Allocate buffer
        let mut buffer = vec![0u8; needed as usize];
        let config_ptr =
            buffer.as_mut_ptr() as *mut windows::Win32::System::Services::QUERY_SERVICE_CONFIGW;

        QueryServiceConfigW(service_handle, Some(config_ptr), needed, &mut needed).map_err(
            |e| windows_service::Error::Winapi(std::io::Error::from_raw_os_error(e.code().0)),
        )?;

        let config = &*config_ptr;

        let binary_path = PCWSTR(config.lpBinaryPathName.0)
            .to_string()
            .map_err(|e| windows_service::Error::Winapi(io::Error::new(io::ErrorKind::Other, e)))?;

        Ok(binary_path)
    }
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

pub fn list_services_with_status() -> windows_service::Result<Vec<(String, String)>> {
    let mut service_list = Vec::new();

    unsafe {
        let scm_handle_res = OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE);
        if scm_handle_res.is_err() {
            let win_err = scm_handle_res.unwrap_err();
            let io_err = std::io::Error::from_raw_os_error(win_err.code().0);
            return Err(windows_service::Error::Winapi(io_err));
        }
        let scm_handle = scm_handle_res.unwrap();
        if scm_handle.0.is_null() {
            return Err(windows_service::Error::Winapi(
                std::io::Error::from_raw_os_error(
                    windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32,
                ),
            ));
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
    return Ok(service_list);
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
