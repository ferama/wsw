use clap::{Parser, Subcommand, command};

use crate::service::SERVICE_NAME_PREFIX;

#[derive(Parser)]
#[command(
    name = "WSW",
    about = "Tiny tool to wrap any executable into a Windows service",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install and start the Windows service
    Install {
        /// Path and args for the executable to run as a service
        #[arg(long)]
        cmd: Option<String>,
        /// Name of the service to install
        #[arg(long, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Stop and uninstall the Windows service
    Uninstall {
        /// Name of the service to uninstall
        #[arg(long, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Run in service mode (called by the system)
    Run {
        /// Path and args for the executable to run
        #[arg(long)]
        cmd: Option<String>,
        /// Name of the service to run
        #[arg(long, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
}
