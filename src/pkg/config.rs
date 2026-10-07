use std::str::FromStr;

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;

use crate::pkg::SERVICE_DESCRIPTION_PREFIX;
use crate::pkg::account::{self, AccountKind};
use crate::pkg::env;

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

    /// Command line to run as a service, executed through 'cmd.exe /C'
    #[arg(long, short)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,

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

    /// Service that must be running before this one starts, repeatable
    #[arg(long, value_name = "SERVICE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,

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
        let base = self;
        merge_fields!(base, over;
            name,
            cmd,
            working_dir,
            env,
            env_file,
            disable_logs,
            log_rotation,
            max_log_files,
            start_type,
            depends_on,
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
            start_type: None,
            depends_on: None,
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
        let command = match &self.cmd {
            Some(cmd) if !cmd.trim().is_empty() => CommandSpec::Shell(cmd.clone()),
            _ => return Err("a command is required: use --cmd".to_string()),
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
            logs: LogConfig {
                disabled: self.disable_logs.unwrap_or(false),
                rotation: self.log_rotation.unwrap_or(LogRotation::Daily),
                max_files: self.max_log_files.unwrap_or(30),
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
        Ok(ScmSettings {
            start_type: self.start_type.unwrap_or(StartType::Auto),
            depends_on: non_empty(self.depends_on.clone()),
            account_name,
            account_password,
            grant_logon_right: self.grant_logon_right.unwrap_or(false) && needs_logon_right,
            grant_dirs: non_empty(self.grant_dir.clone()),
        })
    }
}

/// Options stored by the SCM, validated.
#[derive(Debug, Clone, PartialEq)]
pub struct ScmSettings {
    pub start_type: StartType,
    pub depends_on: Vec<String>,
    /// None for LocalSystem
    pub account_name: Option<String>,
    pub account_password: Option<String>,
    /// Grant SeServiceLogonRight, only set when the account needs it
    pub grant_logon_right: bool,
    pub grant_dirs: Vec<String>,
}

/// How the wrapped process is launched.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandSpec {
    /// A command line interpreted by `cmd.exe /C`
    Shell(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogConfig {
    pub disabled: bool,
    pub rotation: LogRotation,
    pub max_files: usize,
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
