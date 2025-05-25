use windows_service::service::ServiceState;

use crate::{
    commands::start::handle_start_error,
    pkg::service::{start_service, stop_service, wait_for_service_status},
};

use super::stop::handle_stop_error;

pub fn handle(name: &str) {
    match stop_service(&name) {
        Ok(_) => {
            println!("Service '{}' stopped successfully.", name);
            match wait_for_service_status(
                &name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => println!("Service '{}' is now stopped.", name),
                Err(e) => eprintln!("Failed to wait for service '{}': {}", name, e),
            }
            match start_service(&name) {
                Ok(_) => eprintln!("Service '{}' started successfully.", name),
                Err(e) => handle_start_error(e, name),
            }
        }
        Err(e) => handle_stop_error(e, name),
    }
}
