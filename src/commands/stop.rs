use windows_service::service::ServiceState;

use crate::pkg::service::{stop_service, wait_for_service_status};
use windows_service::Error;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_NOT_ACTIVE,
};

pub fn handle_stop_error(e: Error, name: &str) {
    match e {
        Error::Winapi(ref winapi_err) => match winapi_err.raw_os_error() {
            Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                eprintln!("Service '{name}' is not installed.");
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            Some(code) if code as u32 == ERROR_SERVICE_NOT_ACTIVE => {
                eprintln!("Service '{name}' is alredy stopped.");
            }
            _ => {
                eprintln!("Failed to stop service '{}': {:?}", name, e);
            }
        },
        _ => {
            eprintln!("Failed to stop service '{}': {:?}", name, e);
        }
    }
}

pub fn handle(name: &str) {
    match stop_service(&name) {
        Ok(_) => {
            match wait_for_service_status(
                &name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => println!("Service '{}' is now stopped.", name),
                Err(e) => eprintln!("Failed to wait for service '{}': {}", name, e),
            }
        }
        Err(e) => handle_stop_error(e, name),
    }
}
