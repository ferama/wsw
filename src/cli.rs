use clap::{Parser, Subcommand, command};

use crate::pkg::service::SERVICE_NAME_PREFIX;

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
    /// Show the status of the Windows services managed from 'wsw'
    #[command(visible_alias = "ls")]
    List,
    /// Start a service
    #[command()]
    Start {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Stop a service
    #[command()]
    Stop {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Install and start the Windows service
    #[command(visible_alias = "i")]
    Install {
        /// Path and args for the executable to run as a service
        #[arg(long, short)]
        cmd: String,
        /// Service working directory
        /// If not specified, the target directory of the executable (cmd arg) will be used
        #[arg(long)]
        working_dir: Option<String>,
        /// Name of the service to install
        #[arg(long, short, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Stop and uninstall the Windows service
    #[command(visible_alias = "u")]
    Uninstall {
        /// Name of the service to uninstall
        #[arg(long, short, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
    /// Run in service mode (called by the system or for debugging)
    /// This command is not intended to be called directly from the command line
    #[command(hide = true)]
    Run {
        /// Path and args for the executable to run
        #[arg(long, short)]
        cmd: String,
        /// Service working directory
        /// If not specified, the target directory of the executable (cmd arg) will be used
        #[arg(long)]
        working_dir: Option<String>,
        /// Name of the service to run
        #[arg(long, short, default_value_t = String::from(SERVICE_NAME_PREFIX))]
        name: String,
    },
}
