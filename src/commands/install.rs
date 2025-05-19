use crate::pkg::logs::setup_logging;
use crate::pkg::service::{get_service_name, install_service};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle(cmd: &str, working_dir: Option<String>, name: &str, disable_logs: bool) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match install_service(&svc_name, working_dir, &cmd, disable_logs) {
        Ok(_) => tracing::info!("Service '{}' installed successfully.", svc_name),
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to install the service '{}': {:?}", svc_name, e);
            }
        },
        Err(e) => tracing::error!("Failed to install service '{}': {}", svc_name, e),
    }
}
