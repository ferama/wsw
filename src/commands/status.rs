use regex::Regex;
use windows_service::Error;
use windows_service::service::ServiceState;

use crate::pkg::service::{get_service_command_line, get_service_name, get_service_status};
use prettytable::{Table, row};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);

    match get_service_status(&svc_name) {
        Ok(status) => {
            let mut table = Table::new();
            table.add_row(row!["Service Name", &svc_name]);
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

            if let Ok(commandline) = get_service_command_line(&svc_name) {
                let re = Regex::new(r#"--cmd\s+"([^"]+)""#).unwrap();
                if let Some(caps) = re.captures(&commandline) {
                    let cmd = &caps[1];
                    table.add_row(row!["Cmdline", format!("{}", cmd)]);
                }
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
                eprintln!("Service '{}' is not installed.", svc_name);
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to get service status '{}': {:?}", svc_name, e);
            }
        },
        Err(e) => {
            eprintln!("Failed to get service status '{}': {:?}", svc_name, e);
        }
    }
}
