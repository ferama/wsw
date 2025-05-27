use windows_service::Error;
use windows_service::service::ServiceState;

use crate::pkg::service::{get_service_command_line, get_service_status};
use prettytable::{Table, row};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};

pub fn handle(name: &str) {
    match get_service_status(&name) {
        Ok(status) => {
            let mut table = Table::new();

            table.add_row(row!["Service Name", &name]);
            table.add_row(row!["Status", format!("{:?}", status.current_state)]);

            match status.process_id {
                Some(pid) => {
                    table.add_row(prettytable::Row::new(vec![
                        prettytable::Cell::new("PID"),
                        prettytable::Cell::new(&pid.to_string()),
                    ]));
                }
                None => {
                    table.add_row(prettytable::Row::new(vec![
                        prettytable::Cell::new("PID"),
                        prettytable::Cell::new("Not running"),
                    ]));
                }
            }

            if let Ok(commandline) = get_service_command_line(&name) {
                table.add_row(row!["FullCmd", format!("{}", commandline)]);
            }

            if status.current_state == ServiceState::Stopped {
                table.add_row(row!["Exit Code", format!("{:?}", status.exit_code)]);
            } else {
                table.add_row(row!["Exit Code", "N/A"]);
            }

            table.printstd();
        }
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                eprintln!("Service '{}' is not installed.", name);
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to get service status '{}': {:?}", name, e);
            }
        },
        Err(e) => {
            eprintln!("Failed to get service status '{}': {:?}", name, e);
        }
    }
}
