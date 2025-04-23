use std::ffi::OsString;

use windows_service::service::{
    ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

pub fn install_service(name: &str, service_cmd: String) {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .expect("Failed to connect to service manager");

    let executable_path = ::std::env::current_exe().unwrap();
    println!("Executable path: {}", executable_path.display());
    println!("Run CMD: {}", service_cmd);

    let service_info = ServiceInfo {
        name: OsString::from(name),
        display_name: OsString::from("Windows Service Wrapper"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable_path.clone(),
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
    println!("Service installed and started successfully");
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
    println!("Service '{}' uninstalled successfully.", name);
    Ok(())
}
