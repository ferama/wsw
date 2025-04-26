use crate::pkg::{
    logs::setup_logging,
    service::{get_service_name, stop_service},
};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match stop_service(&svc_name) {
        Ok(_) => tracing::info!("Service '{}' stopped successfully.", svc_name),
        Err(e) => tracing::error!("Failed to stop service '{}': {}", svc_name, e),
    }
}
