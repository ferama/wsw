use windows_service::service::ServiceState;

use crate::pkg::{
    logs::setup_logging,
    service::{get_service_name, start_service, stop_service, wait_service_status},
};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match stop_service(&svc_name) {
        Ok(_) => {
            tracing::info!("Service '{}' stopped successfully.", svc_name);
            match wait_service_status(
                &svc_name,
                ServiceState::Stopped,
                std::time::Duration::from_secs(10),
            ) {
                Ok(_) => tracing::info!("Service '{}' is now stopped.", svc_name),
                Err(e) => tracing::error!("Failed to wait for service '{}': {}", svc_name, e),
            }
            match start_service(&svc_name) {
                Ok(_) => tracing::info!("Service '{}' started successfully.", svc_name),
                Err(e) => tracing::error!("Failed to start service '{}': {}", svc_name, e),
            }
        }
        Err(e) => tracing::error!("Failed to stop service '{}': {}", svc_name, e),
    }
}
