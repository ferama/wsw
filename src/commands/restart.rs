use windows_service::service::ServiceState;

use crate::{
    commands::start::handle_start_error,
    pkg::service::{start_service, stop_service, wait_for_service_status},
};

use super::stop::handle_stop_error;

pub fn handle(name: &str) {
    match stop_service(name) {
        Ok(_) => {
            match wait_for_service_status(
                name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => println!("Service '{}' is now stopped.", name),
                Err(e) => {
                    eprintln!("Failed to wait for service '{}': {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        // A stopped service is simply started
        Err(e) => {
            if !handle_stop_error(e, name) {
                std::process::exit(1);
            }
        }
    }
    match start_service(name) {
        Ok(_) => match wait_for_service_status(
            name,
            ServiceState::Running,
            std::time::Duration::from_secs(10),
        ) {
            Ok(_) => println!("Service '{}' is now running.", name),
            Err(e) => {
                eprintln!("Failed to wait for service '{}': {}", name, e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            handle_start_error(e, name);
            std::process::exit(1);
        }
    }
}
