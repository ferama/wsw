use clap::CommandFactory;
use clap::Parser;
use runner::run_command;
use tracing::{error, info};

use windows_service::define_windows_service;
use windows_service::service_dispatcher;

mod cli;
mod logs;
mod runner;
mod service;

use cli::*;
use logs::*;
use service::*;

fn main() {
    let cli = Cli::parse();
    // If parsing fails, clap will print the error and exit
    match cli.command {
        Some(Commands::Install { cmd, name }) => {
            let svc_name = get_service_name(&name);
            let _guard = setup_logging(&svc_name);

            if let Some(cmd) = cmd {
                install_service(&svc_name, cmd);
                info!("Service '{}' installed successfully.", svc_name);
            } else {
                error!("--cmd is required with install");
            }
        }
        Some(Commands::Uninstall { name }) => {
            let svc_name = get_service_name(&name);
            let _guard = setup_logging(&svc_name);

            let res = uninstall_service(&svc_name);
            if res.is_ok() {
                info!("Service '{}' uninstalled successfully.", svc_name);
            } else {
                error!("Failed to uninstall service: {}", res.unwrap_err());
            }
        }
        Some(Commands::Run { cmd, name }) => {
            let _guard = setup_logging(&name);

            define_windows_service!(ffi_service_main, service_main);

            if let Err(_e) = service_dispatcher::start(name, ffi_service_main) {
                // error!("Failed to start service: {}", e);
                if let Some(cmd) = cmd {
                    if let Ok(mut child) = run_command(&cmd) {
                        if let Err(e) = child.wait() {
                            error!("Failed to wait for child process: {}", e);
                        }
                    }
                } else {
                    error!("--cmd is required with run");
                }
            }
        }
        None => {
            let help = Cli::command().render_help();
            println!("{}", help.ansi());
            std::process::exit(1);
        }
    }
}
