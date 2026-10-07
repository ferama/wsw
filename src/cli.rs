use clap::{Parser, Subcommand};

use crate::pkg::SERVICE_DESCRIPTION_PREFIX;
pub use crate::pkg::config::{LogRotation, ServiceConfig};

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
    /// Show the logs of the Windows service managed from 'wsw'
    #[command()]
    Logs {
        /// Name of the service to show logs for
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
        /// Follow the log file and show new lines as they are added
        #[arg(long, short, default_value_t = false)]
        follow: bool,
        /// Show all log lines including the ones from the wsw service wrapper itself
        /// This is useful for debugging the service itself
        #[arg(long, default_value_t = false)]
        full: bool,
    },
    /// Show the status of the Windows services managed from 'wsw'
    #[command(visible_alias = "ls")]
    List,
    /// Start a service
    #[command()]
    Start {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
    },
    /// Stop a service
    #[command()]
    Stop {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
    },
    /// Print a service status
    #[command()]
    Status {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
    },
    /// Restart a service
    #[command()]
    Restart {
        /// Name of the service to start
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
    },
    /// Install and start the Windows service
    #[command(visible_alias = "i")]
    Install {
        #[command(flatten)]
        config: ServiceConfig,
    },
    /// Stop and uninstall the Windows service
    #[command(visible_alias = "u")]
    Uninstall {
        /// Name of the service to uninstall
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
    },
    /// Run in service mode (called by the system or for debugging)
    /// This command is not intended to be called directly from the command line
    #[command(hide = true)]
    Run {
        #[command(flatten)]
        config: ServiceConfig,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
