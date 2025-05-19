use windows_service::service::ServiceState;

use crate::{
    commands::start::handle_start_error,
    pkg::{
        logs::setup_logging,
        service::{get_service_name, start_service, stop_service, wait_for_service_status},
    },
};

use super::stop::handle_stop_error;

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match stop_service(&svc_name) {
        Ok(_) => {
            tracing::info!("Service '{}' stopped successfully.", svc_name);
            match wait_for_service_status(
                &svc_name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => tracing::info!("Service '{}' is now stopped.", svc_name),
                Err(e) => tracing::error!("Failed to wait for service '{}': {}", svc_name, e),
            }
            match start_service(&svc_name) {
                Ok(_) => tracing::info!("Service '{}' started successfully.", svc_name),
                Err(e) => handle_start_error(e, name),
            }
        }
        Err(e) => handle_stop_error(e, name),
    }
}
