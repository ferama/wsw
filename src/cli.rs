use std::str::FromStr;

use clap::{Parser, Subcommand, command};
use tracing_appender::rolling::Rotation;

use crate::pkg::service::SERVICE_DESCRIPTION_PREFIX;

#[derive(Debug, Clone)]
pub enum LogRotation {
    Minutely,
    Hourly,
    Daily,
    Never,
}

impl ToString for LogRotation {
    fn to_string(&self) -> String {
        match self {
            LogRotation::Minutely => "minutely".to_string(),
            LogRotation::Hourly => "hourly".to_string(),
            LogRotation::Daily => "daily".to_string(),
            LogRotation::Never => "never".to_string(),
        }
    }
}

impl FromStr for LogRotation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "minutely" => Ok(LogRotation::Minutely),
            "hourly" => Ok(LogRotation::Hourly),
            "daily" => Ok(LogRotation::Daily),
            "never" => Ok(LogRotation::Never),
            _ => Err(format!("Invalid log rotation: {}", s)),
        }
    }
}

impl From<LogRotation> for Rotation {
    fn from(lr: LogRotation) -> Self {
        match lr {
            LogRotation::Minutely => Rotation::MINUTELY,
            LogRotation::Hourly => Rotation::HOURLY,
            LogRotation::Daily => Rotation::DAILY,
            LogRotation::Never => Rotation::NEVER,
        }
    }
}

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
        /// Path and args for the executable to run as a service
        #[arg(long, short)]
        cmd: String,
        /// Service working directory
        /// If not specified, the target directory of the executable (cmd arg) will be used
        #[arg(long)]
        working_dir: Option<String>,
        /// Name of the service to install
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
        /// If set to true, wrapped application logs will not be captured.
        /// This means that following call to the "logs" subcommand will not
        /// display any output regarding the wrapped app. This is useful in scenarios
        /// where logs full managed from the wrapped application already.
        #[arg(long, short, default_value_t = false)]
        disable_logs: bool,

        /// Set the log rotation policy
        /// * daily
        /// * hourly
        /// * minutely
        /// * never
        #[arg(long, short, default_value_t = LogRotation::Daily)]
        log_rotation: LogRotation,

        /// How many log files to keep
        /// This is only used if the log rotation policy is set to something other than "never"
        #[arg(long, short, default_value_t = 30)]
        max_log_files: usize,
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
        /// Path and args for the executable to run
        #[arg(long, short)]
        cmd: String,
        /// Service working directory
        /// If not specified, the target directory of the executable (cmd arg) will be used
        #[arg(long)]
        working_dir: Option<String>,
        /// Name of the service to run
        #[arg(long, short, default_value_t = String::from(SERVICE_DESCRIPTION_PREFIX))]
        name: String,
        /// If set to true, wrapped application logs will not be captured.
        /// This means that following call to the "logs" subcommand will not
        /// display any output regarding the wrapped app. This is useful in scenarios
        /// where logs full managed from the wrapped application already.
        #[arg(long, short, default_value_t = false)]
        disable_logs: bool,
        /// Set the log rotation policy
        /// * daily
        /// * hourly
        /// * minutely
        /// * never
        #[arg(long, short, default_value_t = LogRotation::Daily)]
        log_rotation: LogRotation,

        /// How many log files to keep
        /// This is only used if the log rotation policy is set to something other than "never"
        #[arg(long, short, default_value_t = 30)]
        max_log_files: usize,
    },
}
