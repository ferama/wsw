use std::path::PathBuf;

use crate::{cli::ServiceConfig, pkg::scm::update_service};
use windows_service::Error;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};

pub fn handle(config_file: Option<PathBuf>, config: ServiceConfig) {
    let changes = match config.with_file(config_file.as_deref()) {
        Ok(changes) => changes,
        Err(e) => {
            eprintln!("Invalid configuration: {e}");
            std::process::exit(1);
        }
    };
    let name = changes.service_name();

    match update_service(&name, &changes) {
        Ok(_) => {
            println!("Service '{}' updated successfully.", name);
            println!(
                "Restart it to apply the changes: wsw restart --name {}",
                name
            );
        }
        Err(Error::Winapi(e)) => {
            match e.raw_os_error() {
                Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                    eprintln!("Service '{}' is not installed.", name);
                }
                Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                    eprintln!("Access denied — run as Administrator or add the privilege.");
                }
                _ => eprintln!("Failed to update the service '{}': {}", name, e),
            }
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to update the service '{}': {}", name, e);
            std::process::exit(1);
        }
    }
}
