use crate::{
    cli::LogRotation,
    pkg::{
        account::{AccountKind, resolve_password, scm_account_name},
        service::install_service,
    },
};
use windows_service::Error;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

#[allow(clippy::too_many_arguments)]
pub fn handle(
    cmd: &str,
    working_dir: Option<String>,
    name: &str,
    disable_logs: bool,
    log_rotation: LogRotation,
    max_log_files: usize,
    account_name: Option<String>,
    account_password: Option<String>,
) {
    let (account_name, account_password) = match account_name {
        Some(account) => {
            if account_password.is_some() && !AccountKind::classify(&account).requires_password() {
                eprintln!("Ignoring --account-password: '{account}' has no password.");
            }
            match resolve_password(&account, account_password) {
                Ok(password) => (scm_account_name(&account), password),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        None => (None, None),
    };

    match install_service(
        name,
        working_dir,
        cmd,
        disable_logs,
        log_rotation,
        max_log_files,
        account_name,
        account_password,
    ) {
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
