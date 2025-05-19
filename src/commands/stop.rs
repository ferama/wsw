use windows_service::service::ServiceState;

use crate::pkg::{
    logs::setup_logging,
    service::{get_service_name, stop_service, wait_for_service_status},
};
use windows_service::Error;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_NOT_ACTIVE,
};

pub fn handle_stop_error(e: Error, name: &str) {
    match e {
        Error::Winapi(ref winapi_err) => match winapi_err.raw_os_error() {
            Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                tracing::error!("Service '{name}' is not installed.");
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                tracing::error!("Access denied — run as Administrator or add the privilege.");
            }
            Some(code) if code as u32 == ERROR_SERVICE_NOT_ACTIVE => {
                tracing::error!("Service '{name}' is alredy stopped.");
            }
            _ => {
                tracing::error!("Failed to stop service '{}': {:?}", name, e);
            }
        },
        _ => {
            tracing::error!("Failed to stop service '{}': {:?}", name, e);
        }
    }
}

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match stop_service(&svc_name) {
        Ok(_) => {
            match wait_for_service_status(
                &svc_name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => tracing::info!("Service '{}' is now stopped.", svc_name),
                Err(e) => tracing::error!("Failed to wait for service '{}': {}", svc_name, e),
            }
        }
        Err(e) => handle_stop_error(e, name),
    }
}
