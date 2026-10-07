use crate::{
    cli::ServiceConfig,
    pkg::{
        account::{AccountKind, resolve_password, scm_account_name},
        env,
        service::install_service,
    },
};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

pub fn handle(config: ServiceConfig) {
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

    let (account_name, account_password) = match config.account_name.clone() {
        Some(account) => {
            if config.account_password.is_some()
                && !AccountKind::classify(&account).requires_password()
            {
                eprintln!("Ignoring --account-password: '{account}' has no password.");
            }
            match resolve_password(&account, config.account_password.clone()) {
                Ok(password) => (scm_account_name(&account), password),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        None => (None, None),
    };

    match install_service(&config, account_name, account_password) {
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
