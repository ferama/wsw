use windows_service::service::ServiceState;

use crate::pkg::service::{get_service_name, start_service, wait_for_service_status};
use windows_service::Error;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_DOES_NOT_EXIST,
};

pub fn handle_start_error(e: Error, name: &str) {
    match e {
        Error::Winapi(ref winapi_err) => match winapi_err.raw_os_error() {
            Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                eprintln!("Service '{name}' is not installed.");
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            Some(code) if code as u32 == ERROR_SERVICE_ALREADY_RUNNING => {
                eprintln!("Service '{name}' is alredy running.");
            }
            _ => {
                eprintln!("Failed to start service '{}': {:?}", name, e);
            }
        },
        _ => {
            eprintln!("Failed to start service '{}': {:?}", name, e);
        }
    }
}

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);

    match start_service(&svc_name) {
        Ok(_) => {
            match wait_for_service_status(
                &svc_name,
                ServiceState::Running,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => println!("Service '{}' is now running.", svc_name),
                Err(e) => eprintln!("Failed to wait for service '{}': {}", svc_name, e),
            }
        }
        Err(e) => handle_start_error(e, name),
    }
}
