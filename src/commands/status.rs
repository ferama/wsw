use windows_service::service::ServiceState;

use crate::pkg::service::{get_service_name, get_service_status};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);

    match get_service_status(&svc_name) {
        Ok(status) => {
            println!("==========================");
            println!("Service: {}", &svc_name);
            println!("Status: {:?}", status.current_state);
            match status.process_id {
                Some(pid) => println!("PID: {}", pid),
                None => println!("PID: Not running"),
            }
            if status.current_state == ServiceState::Stopped {
                println!("Exit Code: {:?}", status.exit_code);
            } else {
                println!("Exit Code: N/A");
            }
        }
        Err(e) => {
            eprintln!("Error getting service status: {}", e);
        }
    }
}
