use windows_service::{define_windows_service, service_dispatcher};

use crate::{
    cli::ServiceConfig,
    pkg::{
        config::{LogConfig, LogRotation, RunConfig},
        env,
        logs::setup_logging,
        registry,
        runner::run_command,
        service::{service_main, set_run_config},
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
            "no configuration found for service '{}': use --cmd",
            name
        )),
        Err(e) => Err(format!(
            "failed to read the configuration of service '{}': {}",
            name, e
        )),
    }
}

pub fn handle(cli_config: ServiceConfig) {
    define_windows_service!(ffi_service_main, service_main);

    let name = cli_config.service_name();
    let config = load_config(cli_config);
    let logs = match &config {
        Ok(config) => config.logs.clone(),
        Err(_) => LogConfig {
            disabled: false,
            rotation: LogRotation::Daily,
            max_files: 30,
        },
    };
    let _guard = setup_logging(&name, logs.rotation, logs.max_files);

    set_run_config(config.clone());
    if let Err(_e) = service_dispatcher::start(&name, ffi_service_main) {
        // Not started by the SCM: run the command once in the foreground and
        // exit with its exit code
        let exit_code = match config {
            Ok(config) => match env::load(config.env_file.as_deref(), &config.env)
                .map_err(std::io::Error::other)
                .and_then(|env| run_command(&config, &env))
            {
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
            },
            Err(e) => {
                tracing::error!("Invalid configuration: {}", e);
                1
            }
        };
        drop(_guard);
        std::process::exit(exit_code);
    }
}
