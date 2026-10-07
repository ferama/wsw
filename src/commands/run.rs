use windows_service::{define_windows_service, service_dispatcher};

use crate::{
    cli::LogRotation,
    pkg::{
        logs::setup_logging,
        runner::run_command,
        service::{RunOptions, service_main, set_run_options},
    },
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
    let _guard = setup_logging(name, log_rotation, max_log_files);
    set_run_options(RunOptions {
        name: name.to_string(),
        cmd: cmd.to_string(),
        working_dir: working_dir.clone(),
        disable_logs,
    });
    if let Err(_e) = service_dispatcher::start(name, ffi_service_main) {
        // Not started by the SCM: run the command once in the foreground and
        // exit with its exit code
        let exit_code = match run_command(cmd, working_dir, disable_logs) {
            Ok(mut child) => match child.child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(e) => {
                    tracing::error!("Failed to wait for child process: {}", e);
                    1
                }
            },
            Err(e) => {
                tracing::error!("Failed to run cmd: {:?}", e);
                1
            }
        };
        drop(_guard);
        std::process::exit(exit_code);
    }
}
