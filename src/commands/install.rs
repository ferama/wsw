use crate::pkg::logs::setup_logging;
use crate::pkg::service::{get_service_name, install_service};

pub fn handle(cmd: &str, working_dir: Option<String>, name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    match install_service(&svc_name, working_dir, &cmd) {
        Ok(_) => tracing::info!("Service '{}' installed successfully.", svc_name),
        Err(e) => tracing::error!("Failed to install service '{}': {}", svc_name, e),
    }
}
