use clap::{Parser, Subcommand, command};

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
