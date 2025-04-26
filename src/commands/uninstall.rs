use crate::pkg::{
    logs::setup_logging,
    service::{get_service_name, uninstall_service},
};

pub fn handle(name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    let res = uninstall_service(&svc_name);
    if res.is_ok() {
        tracing::info!("Service '{}' uninstalled successfully.", svc_name);
    } else {
        tracing::error!(
            "Failed to uninstall service '{}': {}",
            name,
            res.unwrap_err()
        );
    }
}
