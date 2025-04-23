use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use clap::Parser;
use windows_service::define_windows_service;
use windows_service::service_dispatcher;

mod cli;
mod manager;
mod runner;
mod service;

use cli::*;
use manager::*;
use runner::runner;
use service::*;

define_windows_service!(ffi_service_main, service_main);

fn main() {
    let cli = Cli::parse();
    // If parsing fails, clap will print the error and exit
    match cli.command {
        Some(Commands::Install { cmd }) => {
            if let Some(cmd) = cmd {
                install_service(SERVICE_NAME, cmd);
            } else {
                eprintln!("--cmd is required with install");
            }
        }
        Some(Commands::Uninstall) => {
            let res = uninstall_service(SERVICE_NAME);
            if res.is_ok() {
                println!("Service uninstalled successfully.");
            } else {
                eprintln!("Failed to uninstall service: {}", res.unwrap_err());
            }
        }
        Some(Commands::Run { cmd }) => {
            if let Err(e) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
                eprintln!("Failed to start service: {}", e);
            }
            let watcher = Arc::new(AtomicBool::new(true));
            if let Some(cmd) = cmd {
                runner(cmd, watcher);
            } else {
                eprintln!("--cmd is required with run");
            }
        }
        None => {
            eprintln!("No command provided. Use --help to see usage.");
        }
    }
}
