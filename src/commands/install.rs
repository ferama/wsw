use std::path::PathBuf;

use crate::{
    cli::ServiceConfig,
    pkg::config::StartType,
    pkg::{account::AccountKind, env, service::install_service},
};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle(config_file: Option<PathBuf>, config: ServiceConfig) {
    let config = match config.with_file(config_file.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Invalid configuration: {e}");
            std::process::exit(1);
        }
    };
    let name = config.service_name();
    let run_config = match config.resolve() {
        Ok(run_config) => run_config,
        Err(e) => {
            eprintln!("Invalid configuration: {e}");
            std::process::exit(1);
        }
    };
    // The env file is read when the service starts, it may not exist yet
    if let Some(path) = &run_config.env_file
        && let Err(e) = env::load(Some(path), &[])
    {
        eprintln!("Warning: {e}");
    }

    let scm = match config.scm() {
        Ok(scm) => scm,
        Err(e) => {
            eprintln!("Invalid configuration: {e}");
            std::process::exit(1);
        }
    };
    if let Some(account) = &config.account_name
        && config.account_password.is_some()
        && !AccountKind::classify(account).requires_password()
    {
        eprintln!("Ignoring --account-password: '{account}' has no password.");
    }

    match install_service(&config, &scm) {
        Ok(_) if scm.start_type == StartType::Disabled => {
            println!("Service '{}' installed successfully (disabled).", name)
        }
        Ok(_) => println!("Service '{}' installed successfully.", name),
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to install the service '{}': {:?}", name, e);
            }
        },
        Err(e) => eprintln!("Failed to install service '{}': {}", name, e),
    }
}
