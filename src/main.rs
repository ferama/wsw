use clap::Parser;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::{error, info};

use windows_service::define_windows_service;
use windows_service::service_dispatcher;

mod cli;
mod logs;
mod runner;
mod service;

use cli::*;
use logs::*;
use runner::runner;
use service::*;

fn main() {
    let cli = Cli::parse();
    // If parsing fails, clap will print the error and exit
    match cli.command {
        Some(Commands::Install { cmd, name }) => {
            let _guard = setup_logging(name.as_str());

            if let Some(cmd) = cmd {
                install_service(&name, cmd);
            } else {
                error!("--cmd is required with install");
            }
        }
        Some(Commands::Uninstall { name }) => {
            let _guard = setup_logging(name.as_str());

            let res = uninstall_service(&name);
            if res.is_ok() {
                info!("Service uninstalled successfully.");
            } else {
                error!("Failed to uninstall service: {}", res.unwrap_err());
            }
        }
        Some(Commands::Run { cmd, name }) => {
            let _guard = setup_logging(name.as_str());
            info!("= Starting service =");

            define_windows_service!(ffi_service_main, service_main);

            if let Err(e) = service_dispatcher::start(name, ffi_service_main) {
                error!("Failed to start service: {}", e);
                let watcher = Arc::new(AtomicBool::new(true));
                if let Some(cmd) = cmd {
                    runner(cmd, watcher);
                } else {
                    error!("--cmd is required with run");
                }
            }
            info!("= Service stopped =");
        }
        None => {
            eprintln!("No command provided. Use --help to see usage.");
        }
    }
}
