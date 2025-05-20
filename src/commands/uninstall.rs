use crate::pkg::service::{get_service_name, uninstall_service};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);

    match uninstall_service(&svc_name) {
        Ok(_) => println!("Service '{}' uninstalled successfully.", svc_name),
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to uninstall the service '{}': {:?}", svc_name, e);
            }
        },
        Err(e) => tracing::error!("Failed to uninstall service '{}': {}", svc_name, e),
    }
}
