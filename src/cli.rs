use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::pkg::SERVICE_DESCRIPTION_PREFIX;
pub use crate::pkg::config::ServiceConfig;

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
        /// Show the stderr of the wrapped process, for services installed
        /// with --log-split
        #[arg(long, default_value_t = false)]
        stderr: bool,
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
        /// Also show the whole effective configuration of the service
        #[arg(long, short, default_value_t = false)]
        verbose: bool,
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
        /// TOML file with the service options, named like the flags
        /// (e.g. working-dir = 'C:\app'). Command line flags take precedence
        #[arg(long = "config", value_name = "PATH")]
        config_file: Option<PathBuf>,

        #[command(flatten)]
        config: ServiceConfig,
    },
    /// Change the configuration of an installed service, without
    /// reinstalling it. Only the given options change; list options (--env,
    /// --depends-on...) are replaced as a whole and cleared by an empty
    /// value. Restart the service to apply the changes
    #[command(visible_alias = "reconfigure")]
    Update {
        /// TOML file with the options to change, named like the flags.
        /// Command line flags take precedence
        #[arg(long = "config", value_name = "PATH")]
        config_file: Option<PathBuf>,

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
        /// TOML file with the service options
        #[arg(long = "config", value_name = "PATH")]
        config_file: Option<PathBuf>,

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

    fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
        let mut argv = vec!["wsw"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|cli| cli.command.unwrap())
    }

    fn service_config(args: &[&str]) -> ServiceConfig {
        match parse(args).unwrap() {
            Commands::Install { config, .. } | Commands::Run { config, .. } => config,
            _ => panic!("not install/run"),
        }
    }

    /// Every invocation accepted by wsw 0.9.1 must keep its meaning.
    #[test]
    fn v0_9_1_invocations_still_parse() {
        use crate::pkg::config::{CommandSpec, LogRotation};

        // install, long flags
        let config = service_config(&[
            "install",
            "--cmd",
            r"C:\app\app.exe --x",
            "--working-dir",
            r"C:\app",
            "--name",
            "app",
            "--disable-logs",
            "--log-rotation",
            "Hourly",
            "--max-log-files",
            "5",
            "--account-name",
            r".\marco",
            "--account-password",
            "pw",
        ]);
        let run = config.resolve().unwrap();
        assert_eq!(run.name, "app");
        assert_eq!(
            run.command,
            CommandSpec::Shell(r"C:\app\app.exe --x".into())
        );
        assert_eq!(run.working_dir.as_deref(), Some(r"C:\app"));
        assert!(run.logs.disabled);
        assert_eq!(run.logs.rotation, LogRotation::Hourly);
        assert_eq!(run.logs.max_files, 5);
        assert_eq!(config.account_name.as_deref(), Some(r".\marco"));
        assert_eq!(config.account_password.as_deref(), Some("pw"));

        // install, short flags and the 'i' alias, -d followed by other flags
        for args in [
            &[
                "i", "-c", "app.exe", "-d", "-n", "app", "-l", "never", "-m", "3",
            ][..],
            &[
                "install", "-d", "-c", "app.exe", "-n", "app", "-l", "never", "-m", "3",
            ][..],
            &[
                "install", "-c", "app.exe", "-n", "app", "-l", "never", "-m", "3", "-d",
            ][..],
            &["install", "-c=app.exe", "-n=app", "-l=never", "-m=3", "-d"][..],
            &[
                "install",
                "--cmd=app.exe",
                "--name=app",
                "--log-rotation=never",
                "--max-log-files=3",
                "--disable-logs",
            ][..],
        ] {
            let run = service_config(args).resolve().unwrap();
            assert_eq!(run.name, "app", "{args:?}");
            assert_eq!(
                run.command,
                CommandSpec::Shell("app.exe".into()),
                "{args:?}"
            );
            assert!(run.logs.disabled, "{args:?}");
            assert_eq!(run.logs.rotation, LogRotation::Never, "{args:?}");
            assert_eq!(run.logs.max_files, 3, "{args:?}");
        }

        // Grouped short flags
        let run = service_config(&["install", "-dc", "app.exe"])
            .resolve()
            .unwrap();
        assert!(run.logs.disabled);
        assert_eq!(run.command, CommandSpec::Shell("app.exe".into()));

        // Defaults of 0.9.1
        let run = service_config(&["install", "-c", "app.exe"])
            .resolve()
            .unwrap();
        assert_eq!(run.name, "wsw");
        assert!(!run.logs.disabled);
        assert_eq!(run.logs.rotation, LogRotation::Daily);
        assert_eq!(run.logs.max_files, 30);

        // The other commands, with their aliases and short flags
        for args in [
            &["logs"][..],
            &["logs", "-n", "app", "-f", "--full"][..],
            &["list"][..],
            &["ls"][..],
            &["start", "-n", "app"][..],
            &["stop", "--name", "app"][..],
            &["status", "-n", "app"][..],
            &["restart", "-n", "app"][..],
            &["uninstall", "-n", "app"][..],
            &["u", "--name", "app"][..],
        ] {
            assert!(parse(args).is_ok(), "{args:?}");
        }
        match parse(&["logs", "-n", "app", "-f", "--full"]).unwrap() {
            Commands::Logs {
                name,
                follow,
                full,
                stderr,
            } => {
                assert_eq!(name, "app");
                assert!(follow && full && !stderr);
            }
            _ => panic!("not logs"),
        }
    }

    #[test]
    fn update_accepts_service_options() {
        let cli = Cli::try_parse_from([
            "wsw",
            "update",
            "--name",
            "Redmine",
            "--config",
            "redmine.toml",
            "--stop-timeout",
            "30",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Update {
                config_file,
                config,
            }) => {
                assert_eq!(config_file, Some(PathBuf::from("redmine.toml")));
                assert_eq!(config.name.as_deref(), Some("Redmine"));
                assert_eq!(config.stop_timeout, Some(30));
            }
            _ => panic!("not an update command"),
        }
    }
}
