use std::time::Duration;

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
