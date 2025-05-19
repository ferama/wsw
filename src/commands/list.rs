use prettytable::{Table, row};

use crate::pkg::service::{SERVICE_NAME_PREFIX, list_services_with_status};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle() {
    match list_services_with_status() {
        Ok(services) => {
            if services.is_empty() {
                println!("No services found.");
            } else {
                let mut table = Table::new();
                table.add_row(row!["Service Name", "Status"]);

                for service in services {
                    let mut name = service.0.to_string();
                    if name != SERVICE_NAME_PREFIX {
                        name = name[4..].to_string();
                    } else {
                        name = "[default]".to_string();
                    }
                    table.add_row(row![name, service.1]);
                }

                table.printstd();
            }
        }
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to list services: {:?}", e);
            }
        },
        Err(e) => {
            eprintln!("Failed to list services: {:?}", e);
        }
    }
}
