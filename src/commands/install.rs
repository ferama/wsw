use crate::pkg::logs::setup_logging;
use crate::pkg::service::{get_service_name, install_service};

pub fn handle(cmd: &str, working_dir: Option<String>, name: &str) {
    let svc_name = get_service_name(&name);
    let _guard = setup_logging(&svc_name);

    install_service(&svc_name, working_dir, &cmd);
    tracing::info!("Service '{}' installed successfully.", svc_name);
}
