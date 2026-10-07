use crate::pkg::service::uninstall_service;
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle(name: &str) {
    let result = uninstall_service(name);
    let failed = result.is_err();
    match result {
        Ok(_) => println!("Service '{}' uninstalled successfully.", name),
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to uninstall the service '{}': {:?}", name, e);
            }
        },
        Err(e) => eprintln!("Failed to uninstall service '{}': {}", name, e),
    }
    if failed {
        std::process::exit(1);
    }
}
