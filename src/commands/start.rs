use crate::pkg::{
    logs::setup_logging,
    service::{get_service_name, start_service},
};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match start_service(&svc_name) {
        Ok(_) => tracing::info!("Service '{}' started successfully.", svc_name),
        Err(e) => tracing::error!("Failed to start service '{}': {}", svc_name, e),
    }
}
