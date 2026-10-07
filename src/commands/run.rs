use std::path::PathBuf;
use std::time::Duration;

use windows_service::{define_windows_service, service_dispatcher};

use crate::{
    cli::ServiceConfig,
    pkg::{
        config::{LogConfig, RunConfig},
        console,
        logs::setup_logging,
        registry,
        restart::service_specific_code,
        service::{service_main, set_run_config},
        stop_signal::StopSignal,
        supervisor::{Outcome, StatusSink, supervise},
    },
};

/// Builds the configuration of the service process.
///
/// Services installed by previous versions of wsw carry their whole
/// configuration on the command line (`run --cmd ...`). Newer ones only
/// carry their name: the configuration is read from the registry, and
/// command line options still take precedence (useful for debugging).
fn load_config(cli_config: ServiceConfig) -> Result<RunConfig, String> {
    if cli_config.cmd.is_some() || cli_config.exe.is_some() {
        return cli_config.resolve();
    }
    let name = cli_config.service_name();
    match registry::read_config(&name) {
        Ok(Some(stored)) => stored.merge(cli_config).resolve(),
        Ok(None) => Err(format!(
            "no configuration found for service '{}': use --exe or --cmd",
            name
        )),
        Err(e) => Err(format!(
            "failed to read the configuration of service '{}': {}",
            name, e
        )),
    }
}

pub fn handle(config_file: Option<PathBuf>, cli_config: ServiceConfig) {
    define_windows_service!(ffi_service_main, service_main);

    let config = cli_config.with_file(config_file.as_deref());
    let name = match &config {
        Ok(config) => config.service_name(),
        Err(_) => ServiceConfig::default().service_name(),
    };
    let config = config.and_then(load_config);
    let logs = match &config {
        Ok(config) => config.logs.clone(),
        Err(_) => LogConfig::default(),
    };
    let _guard = setup_logging(&name, &logs);

    set_run_config(config.clone());
    if let Err(_e) = service_dispatcher::start(&name, ffi_service_main) {
        // Not started by the SCM: supervise the command in the foreground,
        // with the same behaviour as the service, until Ctrl+C
        let exit_code = match config {
            Ok(config) => run_foreground(&config),
            Err(e) => {
                tracing::error!("Invalid configuration: {}", e);
                1
            }
        };
        drop(_guard);
        std::process::exit(exit_code);
    }
}

struct ForegroundStatus;

impl StatusSink for ForegroundStatus {
    fn running(&mut self) {}
    fn stopping(&mut self, _wait_hint: Duration) {}
}

fn run_foreground(config: &RunConfig) -> i32 {
    let stop = StopSignal::new();
    if let Err(e) = console::install_ctrl_handler(Some(stop.clone())) {
        tracing::error!("Failed to install the console control handler: {}", e);
    }
    match supervise(config, &stop, &mut ForegroundStatus, true) {
        Outcome::Stopped => 0,
        Outcome::Exited(code) => code.unwrap_or(1),
        Outcome::GaveUp(code) => service_specific_code(code) as i32,
    }
}
