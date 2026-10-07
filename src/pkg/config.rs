use std::str::FromStr;
use std::time::Duration;

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;

use crate::pkg::SERVICE_DESCRIPTION_PREFIX;
use crate::pkg::account::{self, AccountKind};
use crate::pkg::restart::{self, RestartConfig, RestartPolicy, ScmAction};
use crate::pkg::{cmdline, env};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
    Minutely,
    Hourly,
    Daily,
    Never,
}

impl std::fmt::Display for LogRotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogRotation::Minutely => "minutely",
            LogRotation::Hourly => "hourly",
            LogRotation::Daily => "daily",
            LogRotation::Never => "never",
        };
        f.write_str(s)
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

/// When the SCM starts the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum StartType {
    /// Started at boot
    Auto,
    /// Started shortly after boot, once the other auto start services are running
    DelayedAuto,
    /// Started on demand only
    Manual,
    /// Cannot be started
    Disabled,
}

const DEFAULT_STOP_TIMEOUT: u64 = 15;
const DEFAULT_SCM_FAILURE_RESET: u64 = 24 * 60 * 60;
/// Seconds added to the stop timeout for the preshutdown timeout
const PRESHUTDOWN_MARGIN: u64 = 15;

/// Every option of a wsw service.
///
/// The same structure is used for the command line flags, the TOML
/// configuration file and the configuration persisted in the registry, so
/// every field is optional: configurations are layered with [`merge`] and
/// defaults are applied by [`resolve`].
///
/// [`merge`]: ServiceConfig::merge
/// [`resolve`]: ServiceConfig::resolve
#[derive(Args, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ServiceConfig {
    /// Name of the service [default: wsw]
    #[arg(long, short)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Display name of the service [default: wsw-<name>]
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// Description of the service, shown by the Services console
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Command line to run as a service, executed through 'cmd.exe /C'.
    /// Prefer --exe: it avoids the cmd.exe quoting rules, allows a graceful
    /// stop and reports the real exit code of the process
    #[arg(long, short, conflicts_with = "exe")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,

    /// Executable to run as a service, started directly. Its arguments
    /// follow '--': wsw install --name x --exe C:\app\app.exe -- --port 80
    #[arg(long, value_name = "PATH")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,

    /// Arguments of --exe
    #[arg(last = true, value_name = "ARGS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// Service working directory.
    /// If not specified, the directory of the executable will be used
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Environment variable of the wrapped process, repeatable.
    /// Values can reference other variables: --env "PATH=C:\Ruby\bin;%PATH%"
    #[arg(long = "env", value_name = "KEY=VALUE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,

    /// File with the environment variables of the wrapped process, one
    /// KEY=VALUE per line ('#' comments allowed). It is read every time the
    /// service starts; --env entries take precedence
    #[arg(long, value_name = "PATH")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,

    /// Seconds to wait for the wrapped process to exit after asking it to
    /// stop (Ctrl+C or --stop-cmd), before killing its whole process tree
    /// [default: 15]
    #[arg(long, value_name = "SECS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timeout: Option<u64>,

    /// Command line run (through 'cmd.exe /C') to ask the wrapped process to
    /// stop, instead of sending it Ctrl+C
    #[arg(long, value_name = "CMDLINE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_cmd: Option<String>,

    /// When to restart the wrapped process after it exits on its own:
    /// always, on-failure (non-zero exit code) or never [default: always].
    /// When it is not restarted the service stops, reporting the exit code
    /// of the process to the SCM
    #[arg(long, value_enum, value_name = "POLICY")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartPolicy>,

    /// Seconds to wait before restarting the wrapped process. Alone it is a
    /// fixed delay; with --restart-max-delay it doubles at every
    /// consecutive failure [default: 1, doubling up to 60]
    #[arg(long, value_name = "SECS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_delay: Option<u64>,

    /// Maximum restart delay in seconds, enables the exponential backoff
    #[arg(long, value_name = "SECS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_max_delay: Option<u64>,

    /// Seconds of stable run after which the restart delay and the failure
    /// count start over [default: 60]
    #[arg(long, value_name = "SECS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_after: Option<u64>,

    /// Consecutive restarts before giving up: the service then stops with
    /// a failure exit code, so that the SCM recovery actions can kick in
    /// [default: 0, unlimited]
    #[arg(long, value_name = "COUNT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_restarts: Option<u32>,

    /// If set, wrapped application logs will not be captured.
    /// This means that following call to the "logs" subcommand will not
    /// display any output regarding the wrapped app. This is useful in scenarios
    /// where logs are fully managed from the wrapped application already.
    #[arg(long, short, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_logs: Option<bool>,

    /// Set the log rotation policy: daily, hourly, minutely, never [default: daily]
    #[arg(long, short)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_rotation: Option<LogRotation>,

    /// How many log files to keep [default: 30]
    /// This is only used if the log rotation policy is set to something other than "never"
    #[arg(long, short)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_log_files: Option<usize>,

    /// When the service is started [default: auto]
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_type: Option<StartType>,

    /// Directory of the log files [default: %ProgramData%\wsw\logs]
    #[arg(long, value_name = "PATH")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_dir: Option<String>,

    /// Also rotate a log file when it grows over this size, in megabytes.
    /// --max-log-files still applies
    #[arg(long, value_name = "MB")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_max_size: Option<u64>,

    /// Write the stderr of the wrapped process to a separate log file
    /// (<name>.err.log), shown by 'wsw logs --stderr'
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_split: Option<bool>,

    /// Service that must be running before this one starts, repeatable
    #[arg(long, value_name = "SERVICE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,

    /// Native recovery actions of the SCM, applied when the service fails
    /// (it crashes or stops with a non-zero exit code): comma separated
    /// 'restart:<secs>' or 'none' for the first, second and subsequent
    /// failures, e.g. 'restart:10,restart:60,none'
    #[arg(long, value_name = "ACTIONS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_failure_actions: Option<String>,

    /// Seconds without failures after which the SCM resets the failure
    /// count of --scm-failure-actions [default: 86400]
    #[arg(long, value_name = "SECS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scm_failure_reset: Option<u64>,

    /// Run the service using the specified account (default: LocalSystem).
    /// Built-in accounts need no password: LocalSystem, LocalService,
    /// NetworkService (optionally prefixed by 'NT AUTHORITY\'), virtual
    /// accounts ('NT SERVICE\<name>') and gMSA ('DOMAIN\name$').
    /// If the user is local put it in the format .\username
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,

    /// Password of --account-name, required for regular user accounts only
    #[arg(long)]
    #[serde(skip_serializing)]
    pub account_password: Option<String>,

    /// Grant the "Log on as a service" right to --account-name. Only needed
    /// by regular users and gMSA, built-in accounts already have it
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_logon_right: Option<bool>,

    /// Grant the service account "Modify" permissions on a directory and
    /// everything inside it, repeatable
    #[arg(long, value_name = "PATH")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_dir: Option<Vec<String>>,
}

/// Overwrites the fields of `$base` with the ones set in `$over`.
macro_rules! merge_fields {
    ($base:ident, $over:ident; $($field:ident),* $(,)?) => {
        ServiceConfig {
            $($field: $over.$field.or($base.$field),)*
        }
    };
}

impl ServiceConfig {
    /// Returns a configuration where every option set in `over` takes
    /// precedence over the one in `self`.
    pub fn merge(self, over: ServiceConfig) -> ServiceConfig {
        let mut base = self;
        // The command is replaced as a whole: --cmd and --exe are exclusive
        if over.cmd.is_some() || over.exe.is_some() {
            base.cmd = None;
            base.exe = None;
            base.args = None;
        }
        merge_fields!(base, over;
            name,
            display_name,
            description,
            cmd,
            exe,
            args,
            working_dir,
            env,
            env_file,
            stop_timeout,
            stop_cmd,
            restart,
            restart_delay,
            restart_max_delay,
            reset_after,
            max_restarts,
            disable_logs,
            log_rotation,
            max_log_files,
            log_dir,
            log_max_size,
            log_split,
            start_type,
            depends_on,
            scm_failure_actions,
            scm_failure_reset,
            account_name,
            account_password,
            grant_logon_right,
            grant_dir,
        )
    }

    /// Only the options read by the service process at run time. This is
    /// what gets persisted in the registry: the options stored by the SCM
    /// itself (account, start type...) are queried from the SCM instead, so
    /// that they cannot get out of sync.
    pub fn runtime(&self) -> ServiceConfig {
        ServiceConfig {
            display_name: None,
            description: None,
            start_type: None,
            depends_on: None,
            scm_failure_actions: None,
            scm_failure_reset: None,
            account_name: None,
            account_password: None,
            grant_logon_right: None,
            grant_dir: None,
            ..self.clone()
        }
    }

    pub fn from_toml(text: &str) -> Result<ServiceConfig, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|e| e.to_string())
    }

    pub fn service_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| SERVICE_DESCRIPTION_PREFIX.to_string())
    }

    /// Validates the configuration and applies the defaults.
    pub fn resolve(&self) -> Result<RunConfig, String> {
        let command = match (&self.cmd, &self.exe) {
            (Some(_), Some(_)) => return Err("--cmd and --exe cannot be used together".to_string()),
            (Some(cmd), None) if !cmd.trim().is_empty() => {
                if self.args.is_some() {
                    return Err("arguments after '--' require --exe".to_string());
                }
                CommandSpec::Shell(cmd.clone())
            }
            (None, Some(exe)) if !exe.trim().is_empty() => CommandSpec::Exec {
                exe: exe.clone(),
                args: self.args.clone().unwrap_or_default(),
            },
            _ => return Err("a command is required: use --exe or --cmd".to_string()),
        };

        // An empty value clears a list option
        let env: Vec<String> = non_empty(self.env.clone());
        for entry in &env {
            env::parse_assignment(entry)?;
        }

        Ok(RunConfig {
            name: self.service_name(),
            command,
            working_dir: self.working_dir.clone().filter(|dir| !dir.is_empty()),
            env,
            env_file: self.env_file.clone().filter(|path| !path.is_empty()),
            stop_timeout: Duration::from_secs(self.stop_timeout.unwrap_or(DEFAULT_STOP_TIMEOUT)),
            stop_cmd: self.stop_cmd.clone().filter(|cmd| !cmd.trim().is_empty()),
            restart: RestartConfig::new(
                self.restart,
                self.restart_delay,
                self.restart_max_delay,
                self.reset_after,
                self.max_restarts,
            ),
            logs: LogConfig {
                disabled: self.disable_logs.unwrap_or(false),
                rotation: self.log_rotation.unwrap_or(LogRotation::Daily),
                max_files: self.max_log_files.unwrap_or(30),
                dir: self.log_dir.clone().filter(|dir| !dir.is_empty()),
                max_size: self
                    .log_max_size
                    .filter(|mb| *mb > 0)
                    .map(|mb| mb * 1024 * 1024),
                split: self.log_split.unwrap_or(false),
            },
        })
    }
}

fn non_empty(list: Option<Vec<String>>) -> Vec<String> {
    list.unwrap_or_default()
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect()
}

/// Display name of services installed without --display-name. The `wsw`
/// prefix is also how services installed by older versions are recognized.
pub fn default_display_name(name: &str) -> String {
    if name == SERVICE_DESCRIPTION_PREFIX {
        SERVICE_DESCRIPTION_PREFIX.to_string()
    } else {
        format!("{}-{}", SERVICE_DESCRIPTION_PREFIX, name)
    }
}

impl ServiceConfig {
    /// Validates the options stored by the SCM and applies the defaults.
    pub fn scm(&self) -> Result<ScmSettings, String> {
        let (account_name, account_password, needs_logon_right) = match &self.account_name {
            Some(account) if !account.trim().is_empty() => (
                account::scm_account_name(account),
                account::resolve_password(account, self.account_password.clone())?,
                !AccountKind::classify(account).has_implicit_logon_right(),
            ),
            _ => (None, None, false),
        };
        // During a system shutdown the SCM waits for the service at most for
        // the preshutdown timeout: give it the time to stop gracefully
        let stop_timeout = self.stop_timeout.unwrap_or(DEFAULT_STOP_TIMEOUT);
        let preshutdown_timeout = Duration::from_secs(stop_timeout + PRESHUTDOWN_MARGIN);

        let failure_actions = match &self.scm_failure_actions {
            Some(spec) => Some(restart::parse_scm_actions(spec)?),
            None => None,
        };

        let display_name = match &self.display_name {
            Some(display_name) if !display_name.trim().is_empty() => display_name.clone(),
            _ => default_display_name(&self.service_name()),
        };

        Ok(ScmSettings {
            display_name,
            description: self.description.clone(),
            failure_actions,
            failure_reset: Duration::from_secs(
                self.scm_failure_reset.unwrap_or(DEFAULT_SCM_FAILURE_RESET),
            ),
            preshutdown_timeout,
            start_type: self.start_type.unwrap_or(StartType::Auto),
            depends_on: non_empty(self.depends_on.clone()),
            account_name,
            account_password,
            grant_logon_right: self.grant_logon_right.unwrap_or(false) && needs_logon_right,
            grant_dirs: non_empty(self.grant_dir.clone()),
            log_dir: self.log_dir.clone().filter(|dir| !dir.is_empty()),
        })
    }
}

/// Options stored by the SCM, validated.
#[derive(Debug, Clone, PartialEq)]
pub struct ScmSettings {
    pub display_name: String,
    /// None to leave the description untouched
    pub description: Option<String>,
    /// None to leave the recovery actions untouched, empty to clear them
    pub failure_actions: Option<Vec<ScmAction>>,
    pub failure_reset: Duration,
    pub preshutdown_timeout: Duration,
    pub start_type: StartType,
    pub depends_on: Vec<String>,
    /// None for LocalSystem
    pub account_name: Option<String>,
    pub account_password: Option<String>,
    /// Grant SeServiceLogonRight, only set when the account needs it
    pub grant_logon_right: bool,
    pub grant_dirs: Vec<String>,
    /// Custom log directory: created at install time and made writable by
    /// the service account
    pub log_dir: Option<String>,
}

/// How the wrapped process is launched.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandSpec {
    /// A command line interpreted by `cmd.exe /C`
    Shell(String),
    /// An executable started directly
    Exec { exe: String, args: Vec<String> },
}

impl std::fmt::Display for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandSpec::Shell(cmdline) => write!(f, "cmd.exe /C {}", cmdline),
            CommandSpec::Exec { exe, args } => f.write_str(&cmdline::join(
                std::iter::once(exe.as_str()).chain(args.iter().map(String::as_str)),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogConfig {
    pub disabled: bool,
    pub rotation: LogRotation,
    pub max_files: usize,
    pub dir: Option<String>,
    /// Bytes
    pub max_size: Option<u64>,
    pub split: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            disabled: false,
            rotation: LogRotation::Daily,
            max_files: 30,
            dir: None,
            max_size: None,
            split: false,
        }
    }
}

/// A validated service configuration, with every default applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RunConfig {
    pub name: String,
    pub command: CommandSpec,
    pub working_dir: Option<String>,
    /// Unexpanded KEY=VALUE entries
    pub env: Vec<String>,
    pub env_file: Option<String>,
    pub stop_timeout: Duration,
    pub stop_cmd: Option<String>,
    pub restart: RestartConfig,
    pub logs: LogConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    fn parse_run(args: &[&str]) -> ServiceConfig {
        let mut argv = vec!["wsw", "run"];
        argv.extend_from_slice(args);
        match Cli::try_parse_from(argv).unwrap().command {
            Some(Commands::Run { config }) => config,
            _ => panic!("not a run command"),
        }
    }

    #[test]
    fn legacy_image_path_arguments_still_parse() {
        // Arguments written in the ImagePath by previous versions of wsw
        let config = parse_run(&[
            "--cmd",
            r"C:\app\app.exe --port 80",
            "--name",
            "myapp",
            "--log-rotation",
            "hourly",
            "--max-log-files",
            "5",
            "--working-dir",
            r"C:\app",
            "--disable-logs",
        ]);
        let run = config.resolve().unwrap();
        assert_eq!(run.name, "myapp");
        assert_eq!(
            run.command,
            CommandSpec::Shell(r"C:\app\app.exe --port 80".into())
        );
        assert_eq!(run.working_dir.as_deref(), Some(r"C:\app"));
        assert!(run.logs.disabled);
        assert_eq!(run.logs.rotation, LogRotation::Hourly);
        assert_eq!(run.logs.max_files, 5);
    }

    #[test]
    fn defaults() {
        let run = parse_run(&["--cmd", "app.exe"]).resolve().unwrap();
        assert_eq!(run.name, "wsw");
        assert!(!run.logs.disabled);
        assert_eq!(run.logs.rotation, LogRotation::Daily);
        assert_eq!(run.logs.max_files, 30);
        assert_eq!(run.working_dir, None);
    }

    #[test]
    fn exe_with_trailing_arguments() {
        let config = parse_run(&[
            "--name",
            "Redmine",
            "--exe",
            r"C:\Redmine\ruby\bin\ruby.exe",
            "--",
            r"C:\Redmine\ruby\bin\bundle",
            "exec",
            "puma",
            "-e",
            "production",
            "-b",
            "tcp://0.0.0.0:3000",
        ]);
        let run = config.resolve().unwrap();
        assert_eq!(
            run.command,
            CommandSpec::Exec {
                exe: r"C:\Redmine\ruby\bin\ruby.exe".into(),
                args: [
                    r"C:\Redmine\ruby\bin\bundle",
                    "exec",
                    "puma",
                    "-e",
                    "production",
                    "-b",
                    "tcp://0.0.0.0:3000"
                ]
                .map(String::from)
                .to_vec(),
            }
        );
        assert_eq!(
            run.command.to_string(),
            r"C:\Redmine\ruby\bin\ruby.exe C:\Redmine\ruby\bin\bundle exec puma -e production -b tcp://0.0.0.0:3000"
        );

        // Arguments that look like wsw flags belong to the executable
        let config = parse_run(&["--exe", "app.exe", "--", "--name", "-d"]);
        assert_eq!(config.args, Some(vec!["--name".into(), "-d".into()]));
        assert_eq!(config.name, None);
    }

    #[test]
    fn cmd_and_exe_are_exclusive() {
        let cli = || Cli::try_parse_from(["wsw", "run", "--cmd", "a", "--exe", "b"]);
        assert!(cli().is_err());
        assert!(parse_run(&["--cmd", "a", "--", "x"]).resolve().is_err());

        // A new command replaces the stored one as a whole
        let stored = ServiceConfig {
            exe: Some("old.exe".into()),
            args: Some(vec!["--old".into()]),
            ..Default::default()
        };
        let merged = stored.merge(ServiceConfig {
            cmd: Some("new.bat".into()),
            ..Default::default()
        });
        assert_eq!(merged.exe, None);
        assert_eq!(merged.args, None);
        assert_eq!(
            merged.resolve().unwrap().command,
            CommandSpec::Shell("new.bat".into())
        );
    }

    #[test]
    fn stop_options() {
        let run = parse_run(&["--exe", "a.exe"]).resolve().unwrap();
        assert_eq!(run.stop_timeout, Duration::from_secs(15));
        assert_eq!(run.stop_cmd, None);

        let config = parse_run(&[
            "--exe",
            "a.exe",
            "--stop-timeout",
            "20",
            "--stop-cmd",
            "a.exe --shutdown",
        ]);
        let run = config.resolve().unwrap();
        assert_eq!(run.stop_timeout, Duration::from_secs(20));
        assert_eq!(run.stop_cmd.as_deref(), Some("a.exe --shutdown"));
        assert_eq!(
            config.scm().unwrap().preshutdown_timeout,
            Duration::from_secs(35)
        );
    }

    #[test]
    fn restart_options() {
        let config = parse_run(&[
            "--exe",
            "a.exe",
            "--restart",
            "on-failure",
            "--restart-delay",
            "10",
            "--max-restarts",
            "5",
            "--scm-failure-actions",
            "restart:10,none",
        ]);
        let run = config.resolve().unwrap();
        assert_eq!(run.restart.policy, RestartPolicy::OnFailure);
        assert_eq!(run.restart.delay, Duration::from_secs(10));
        assert_eq!(run.restart.max_delay, Duration::from_secs(10));
        assert_eq!(run.restart.max_restarts, 5);

        let scm = config.scm().unwrap();
        assert_eq!(
            scm.failure_actions,
            Some(vec![
                ScmAction::Restart(Duration::from_secs(10)),
                ScmAction::None
            ])
        );
        assert_eq!(scm.failure_reset, Duration::from_secs(86400));
        // SCM recovery actions are not a runtime option
        assert_eq!(config.runtime().scm_failure_actions, None);

        let bad = parse_run(&["--scm-failure-actions", "reboot:1"]);
        assert!(bad.scm().is_err());
        assert_eq!(parse_run(&[]).scm().unwrap().failure_actions, None);
    }

    #[test]
    fn log_options() {
        let run = parse_run(&[
            "--exe",
            "a.exe",
            "--log-dir",
            r"C:\Redmine\logs",
            "--log-max-size",
            "10",
            "--max-log-files",
            "8",
            "--log-split",
        ])
        .resolve()
        .unwrap();
        assert_eq!(run.logs.dir.as_deref(), Some(r"C:\Redmine\logs"));
        assert_eq!(run.logs.max_size, Some(10 * 1024 * 1024));
        assert_eq!(run.logs.max_files, 8);
        assert!(run.logs.split);

        let defaults = parse_run(&["--exe", "a.exe"]).resolve().unwrap();
        assert_eq!(defaults.logs, LogConfig::default());
    }

    #[test]
    fn display_name_and_description() {
        let config = parse_run(&["--name", "Redmine"]);
        assert_eq!(config.scm().unwrap().display_name, "wsw-Redmine");

        let config = parse_run(&[
            "--name",
            "Redmine",
            "--display-name",
            "Redmine (Puma)",
            "--description",
            "Redmine project management",
        ]);
        let scm = config.scm().unwrap();
        assert_eq!(scm.display_name, "Redmine (Puma)");
        assert_eq!(
            scm.description.as_deref(),
            Some("Redmine project management")
        );
        assert_eq!(config.runtime().display_name, None);
    }

    #[test]
    fn a_command_is_required() {
        assert!(parse_run(&["--name", "x"]).resolve().is_err());
    }

    #[test]
    fn boolean_flags_can_be_turned_off() {
        assert_eq!(parse_run(&["-d"]).disable_logs, Some(true));
        assert_eq!(
            parse_run(&["--disable-logs=false"]).disable_logs,
            Some(false)
        );
        assert_eq!(parse_run(&[]).disable_logs, None);
    }

    #[test]
    fn later_layers_take_precedence() {
        let stored = ServiceConfig {
            cmd: Some("old.exe".into()),
            max_log_files: Some(3),
            disable_logs: Some(true),
            ..Default::default()
        };
        let cli = ServiceConfig {
            cmd: Some("new.exe".into()),
            disable_logs: Some(false),
            ..Default::default()
        };
        let merged = stored.merge(cli);
        assert_eq!(merged.cmd.as_deref(), Some("new.exe"));
        assert_eq!(merged.max_log_files, Some(3));
        assert_eq!(merged.disable_logs, Some(false));
    }

    #[test]
    fn toml_round_trip_never_persists_the_password() {
        let config = ServiceConfig {
            name: Some("app".into()),
            cmd: Some(r#""C:\Program Files\app.exe" --x"#.into()),
            log_rotation: Some(LogRotation::Never),
            account_name: Some(r".\marco".into()),
            account_password: Some("secret".into()),
            ..Default::default()
        };
        let text = config.to_toml().unwrap();
        assert!(!text.contains("secret"));
        let parsed = ServiceConfig::from_toml(&text).unwrap();
        assert_eq!(
            parsed,
            ServiceConfig {
                account_password: None,
                ..config.clone()
            }
        );

        let runtime = config.runtime().to_toml().unwrap();
        assert!(!runtime.contains("marco"));
    }

    #[test]
    fn env_entries_are_validated_and_can_be_cleared() {
        let run = parse_run(&[
            "--cmd",
            "app.exe",
            "--env",
            "RAILS_ENV=production",
            "--env",
            r"PATH=C:\Ruby\bin;%PATH%",
            "--env-file",
            r"C:\app\.env",
        ])
        .resolve()
        .unwrap();
        assert_eq!(
            run.env,
            vec!["RAILS_ENV=production", r"PATH=C:\Ruby\bin;%PATH%"]
        );
        assert_eq!(run.env_file.as_deref(), Some(r"C:\app\.env"));

        assert!(
            parse_run(&["--cmd", "a", "--env", "BROKEN"])
                .resolve()
                .is_err()
        );

        let cleared = parse_run(&["--cmd", "a", "--env", "", "--env-file", ""])
            .resolve()
            .unwrap();
        assert!(cleared.env.is_empty());
        assert_eq!(cleared.env_file, None);
    }

    #[test]
    fn scm_settings() {
        let config = ServiceConfig {
            depends_on: Some(vec!["RedminePostgreSQL".into(), "".into()]),
            account_name: Some("NetworkService".into()),
            ..Default::default()
        };
        let scm = config.scm().unwrap();
        assert_eq!(scm.display_name, "wsw");
        assert_eq!(scm.description, None);
        assert_eq!(scm.start_type, StartType::Auto);
        assert_eq!(scm.depends_on, vec!["RedminePostgreSQL"]);
        assert_eq!(
            scm.account_name.as_deref(),
            Some(r"NT AUTHORITY\NetworkService")
        );
        assert_eq!(scm.account_password, None);
        // SCM options are not persisted with the runtime configuration
        assert_eq!(config.runtime(), ServiceConfig::default());

        let user = ServiceConfig {
            account_name: Some(r".\marco".into()),
            ..Default::default()
        };
        assert!(user.scm().is_err());

        // The logon right is only granted to accounts that need it
        let grant = |account: &str| ServiceConfig {
            account_name: Some(account.into()),
            account_password: Some("pw".into()),
            grant_logon_right: Some(true),
            grant_dir: Some(vec![r"C:\Redmine".into()]),
            ..Default::default()
        };
        let scm = grant(r".\marco").scm().unwrap();
        assert!(scm.grant_logon_right);
        assert_eq!(scm.grant_dirs, vec![r"C:\Redmine"]);
        assert!(!grant("NetworkService").scm().unwrap().grant_logon_right);
        assert!(grant(r"CONTOSO\gmsa$").scm().unwrap().grant_logon_right);
        assert_eq!(grant(r".\marco").runtime(), ServiceConfig::default());
    }

    #[test]
    fn start_types() {
        for (value, expected) in [
            ("auto", StartType::Auto),
            ("delayed-auto", StartType::DelayedAuto),
            ("manual", StartType::Manual),
            ("disabled", StartType::Disabled),
        ] {
            let config = parse_run(&["--start-type", value]);
            assert_eq!(config.start_type, Some(expected));
            let toml = config.to_toml().unwrap();
            assert_eq!(toml.trim(), format!("start-type = \"{value}\""));
        }
    }

    #[test]
    fn toml_rejects_unknown_keys() {
        assert!(ServiceConfig::from_toml("cmd = 'a'\nunknown = 1").is_err());
    }
}
