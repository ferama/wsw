use windows_service::{define_windows_service, service::ServiceType, service_dispatcher};

use clap::{Parser, Subcommand};

mod svc_main;
mod svc_man;
use crate::svc_main::*;
use crate::svc_man::*;

define_windows_service!(ffi_service_main, service_main);

#[derive(Parser)]
#[command(
    name = "WSW",
    about = "Tiny tool to wrap any executable into a Windows service"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install and start the Windows service
    Install {
        /// Path and args for the executable to run as a service
        #[arg(long)]
        cmd: Option<String>,
    },
    /// Stop and uninstall the Windows service
    Uninstall,
    /// Run in service mode (called by the system)
    Run {
        /// Path and args for the executable to run
        #[arg(long)]
        cmd: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install { cmd }) => {
            if let Some(cmd) = cmd {
                install_service(SERVICE_NAME, cmd);
            } else {
                eprintln!("--cmd is required with install");
            }
        }
        Some(Commands::Uninstall) => {
            uninstall_service(SERVICE_NAME);
        }
        Some(Commands::Run { cmd }) => {
            if let Err(e) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
                eprintln!("Failed to start service: {}", e);
            }
        }
        None => {
            eprintln!("No command provided. Use --help to see usage.");
        }
    }
}
