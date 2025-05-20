use windows::Win32::Foundation::CloseHandle;
use windows_service::{define_windows_service, service_dispatcher};

use crate::{
    cli::LogRotation,
    pkg::{logs::setup_logging, runner::run_command, service::service_main},
};

pub fn handle(
    cmd: &str,
    working_dir: Option<String>,
    name: &str,
    disable_logs: bool,
    log_rotation: LogRotation,
    max_log_files: usize,
) {
    define_windows_service!(ffi_service_main, service_main);
    let _guard = setup_logging(&name, log_rotation, max_log_files);
    if let Err(_e) = service_dispatcher::start(name, ffi_service_main) {
        if let Ok(mut child) = run_command(&cmd, working_dir, disable_logs) {
            if let Err(e) = child.1.wait() {
                tracing::error!("Failed to wait for child process: {}", e);
            }
            unsafe {
                if let Err(e) = CloseHandle(std::mem::transmute(child.0)) {
                    tracing::error!("Failed to close handle: {:?}", e);
                }
            }
        }
    }
}
