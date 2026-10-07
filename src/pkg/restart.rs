use std::time::Duration;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// When the wrapped process is restarted after it exits on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// Always restart it
    Always,
    /// Restart it only if it exits with a non-zero exit code
    OnFailure,
    /// Never restart it: the service stops with the process
    Never,
}

impl RestartPolicy {
    /// `exit_code` is None if the process could not be started at all.
    pub fn should_restart(self, exit_code: Option<i32>) -> bool {
        match self {
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => exit_code != Some(0),
            RestartPolicy::Never => false,
        }
    }
}

/// Restart settings, with defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RestartConfig {
    pub policy: RestartPolicy,
    pub delay: Duration,
    pub max_delay: Duration,
    pub reset_after: Duration,
    /// Consecutive restarts before giving up, 0 for unlimited
    pub max_restarts: u32,
}

impl RestartConfig {
    pub const DEFAULT_DELAY: u64 = 1;
    pub const DEFAULT_MAX_DELAY: u64 = 60;
    pub const DEFAULT_RESET_AFTER: u64 = 60;

    /// Without options the delay starts at 1s and doubles up to 60s. Setting
    /// only the delay makes it fixed; setting the max delay too enables the
    /// exponential backoff between the two.
    pub fn new(
        policy: Option<RestartPolicy>,
        delay: Option<u64>,
        max_delay: Option<u64>,
        reset_after: Option<u64>,
        max_restarts: Option<u32>,
    ) -> Self {
        let max_delay = match (delay, max_delay) {
            (_, Some(max)) => max,
            (Some(delay), None) => delay,
            (None, None) => Self::DEFAULT_MAX_DELAY,
        };
        Self {
            policy: policy.unwrap_or(RestartPolicy::Always),
            delay: Duration::from_secs(delay.unwrap_or(Self::DEFAULT_DELAY)),
            max_delay: Duration::from_secs(max_delay),
            reset_after: Duration::from_secs(reset_after.unwrap_or(Self::DEFAULT_RESET_AFTER)),
            max_restarts: max_restarts.unwrap_or(0),
        }
    }

    pub fn backoff(&self) -> Backoff {
        Backoff::new(self.delay, self.max_delay, self.reset_after)
    }

    /// Whether `failures` consecutive failures exceed the allowed restarts.
    pub fn gives_up(&self, failures: u32) -> bool {
        self.max_restarts > 0 && failures > self.max_restarts
    }
}

/// A recovery action of the SCM (`--scm-failure-actions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScmAction {
    /// Restart the service after the delay
    Restart(Duration),
    /// Take no action
    None,
}

/// Parses a comma separated list of SCM recovery actions, applied to the
/// first, second and subsequent failures: `restart:<secs>` or `none`, e.g.
/// `restart:10,restart:60,none`. An empty string clears the actions.
pub fn parse_scm_actions(spec: &str) -> Result<Vec<ScmAction>, String> {
    if spec.trim().is_empty() {
        return Ok(Vec::new());
    }
    spec.split(',')
        .map(|action| {
            let action = action.trim();
            match action.split_once(':') {
                _ if action.eq_ignore_ascii_case("none") => Ok(ScmAction::None),
                Some((kind, delay)) if kind.trim().eq_ignore_ascii_case("restart") => delay
                    .trim()
                    .parse::<u64>()
                    .map(|secs| ScmAction::Restart(Duration::from_secs(secs)))
                    .map_err(|_| format!("invalid delay in SCM failure action '{action}'")),
                _ => Err(format!(
                    "invalid SCM failure action '{action}': expected restart:<secs> or none"
                )),
            }
        })
        .collect()
}

/// Exponential backoff between restarts of the wrapped process.
///
/// The delay starts at `base` and doubles after every consecutive failure,
/// up to `max`. If the process ran for at least `reset_after` before
/// exiting, it is considered stable and the failure counter starts over.
#[derive(Debug, Clone)]
pub struct Backoff {
    base: Duration,
    max: Duration,
    reset_after: Duration,
    failures: u32,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration, reset_after: Duration) -> Self {
        Self {
            base,
            // A max lower than the base means a fixed delay
            max: max.max(base),
            reset_after,
            failures: 0,
        }
    }

    /// Records an exit of the wrapped process after `uptime` and returns how
    /// long to wait before the next start.
    pub fn next_delay(&mut self, uptime: Duration) -> Duration {
        if uptime >= self.reset_after {
            self.failures = 0;
        }
        self.failures = self.failures.saturating_add(1);
        let exponent = (self.failures - 1).min(31);
        self.base
            .checked_mul(1u32 << exponent)
            .map_or(self.max, |delay| delay.min(self.max))
    }

    /// Number of consecutive exits since the process was last stable.
    pub fn consecutive_failures(&self) -> u32 {
        self.failures
    }
}

/// Service specific exit code reported to the SCM when the service
/// terminates on its own because of the wrapped process. A non-zero code is
/// what lets the SCM recovery actions kick in, so a child that exited with
/// 0 or could not be started at all (`None`) is reported as 1.
pub fn service_specific_code(child_exit_code: Option<i32>) -> u32 {
    match child_exit_code {
        Some(code) if code != 0 => code as u32,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_policies() {
        use RestartPolicy::*;
        assert!(Always.should_restart(Some(0)));
        assert!(Always.should_restart(None));
        assert!(!OnFailure.should_restart(Some(0)));
        assert!(OnFailure.should_restart(Some(1)));
        assert!(OnFailure.should_restart(None));
        assert!(!Never.should_restart(Some(1)));
    }

    #[test]
    fn restart_config_defaults() {
        let config = RestartConfig::new(None, None, None, None, None);
        assert_eq!(config.policy, RestartPolicy::Always);
        let mut backoff = config.backoff();
        let delays: Vec<u64> = (0..8)
            .map(|_| backoff.next_delay(Duration::ZERO).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 32, 60, 60]);
        assert!(!config.gives_up(1000));
    }

    #[test]
    fn restart_delay_alone_is_fixed() {
        let config = RestartConfig::new(Some(RestartPolicy::OnFailure), Some(10), None, None, None);
        let mut backoff = config.backoff();
        assert_eq!(backoff.next_delay(Duration::ZERO), Duration::from_secs(10));
        assert_eq!(backoff.next_delay(Duration::ZERO), Duration::from_secs(10));
    }

    #[test]
    fn restart_delay_with_max_is_exponential() {
        let config = RestartConfig::new(None, Some(5), Some(30), Some(120), Some(3));
        let mut backoff = config.backoff();
        let delays: Vec<u64> = (0..4)
            .map(|_| backoff.next_delay(Duration::ZERO).as_secs())
            .collect();
        assert_eq!(delays, vec![5, 10, 20, 30]);
        assert_eq!(config.reset_after, Duration::from_secs(120));
        assert!(!config.gives_up(3));
        assert!(config.gives_up(4));
    }

    #[test]
    fn parses_scm_actions() {
        assert_eq!(
            parse_scm_actions("restart:10, restart:60,NONE"),
            Ok(vec![
                ScmAction::Restart(Duration::from_secs(10)),
                ScmAction::Restart(Duration::from_secs(60)),
                ScmAction::None,
            ])
        );
        assert_eq!(parse_scm_actions(""), Ok(vec![]));
        assert!(parse_scm_actions("restart").is_err());
        assert!(parse_scm_actions("restart:x").is_err());
        assert!(parse_scm_actions("reboot:10").is_err());
    }

    #[test]
    fn service_specific_code_is_never_zero() {
        assert_eq!(service_specific_code(Some(3)), 3);
        assert_eq!(service_specific_code(Some(0)), 1);
        assert_eq!(service_specific_code(None), 1);
        // NTSTATUS crash codes are reported as is
        assert_eq!(
            service_specific_code(Some(0xC000_0005_u32 as i32)),
            0xC000_0005
        );
    }

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn delay_doubles_up_to_max() {
        let mut backoff = Backoff::new(S(1), S(10), S(60));
        let delays: Vec<u64> = (0..6)
            .map(|_| backoff.next_delay(Duration::ZERO).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 10, 10]);
        assert_eq!(backoff.consecutive_failures(), 6);
    }

    #[test]
    fn stable_run_resets_the_counter() {
        let mut backoff = Backoff::new(S(1), S(60), S(30));
        backoff.next_delay(S(1));
        backoff.next_delay(S(1));
        assert_eq!(backoff.next_delay(S(1)), S(4));
        // Ran long enough: start over
        assert_eq!(backoff.next_delay(S(30)), S(1));
        assert_eq!(backoff.consecutive_failures(), 1);
    }

    #[test]
    fn max_lower_than_base_means_fixed_delay() {
        let mut backoff = Backoff::new(S(5), S(1), S(60));
        assert_eq!(backoff.next_delay(Duration::ZERO), S(5));
        assert_eq!(backoff.next_delay(Duration::ZERO), S(5));
    }

    #[test]
    fn zero_base_never_waits() {
        let mut backoff = Backoff::new(Duration::ZERO, S(60), S(60));
        assert_eq!(backoff.next_delay(Duration::ZERO), Duration::ZERO);
        assert_eq!(backoff.next_delay(Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn many_failures_do_not_overflow() {
        let mut backoff = Backoff::new(S(1), S(3600), S(60));
        for _ in 0..100 {
            assert!(backoff.next_delay(Duration::ZERO) <= S(3600));
        }
        assert_eq!(backoff.next_delay(Duration::ZERO), S(3600));
    }
}
