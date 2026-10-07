use std::time::{Duration, Instant};

use tracing::{error, info};

use crate::pkg::config::RunConfig;
use crate::pkg::env;
use crate::pkg::restart::Backoff;
use crate::pkg::runner::run_command;
use crate::pkg::stop_signal::StopSignal;

/// How long the SCM should wait for the service to leave the stop pending state.
pub const STOP_WAIT_HINT: Duration = Duration::from_secs(10);
/// How often the child process is checked for exit.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Restart backoff: first delay, maximum delay and the uptime after which
/// the child is considered stable and the backoff starts over.
const RESTART_DELAY: Duration = Duration::from_secs(1);
const RESTART_MAX_DELAY: Duration = Duration::from_secs(60);
const RESTART_RESET_AFTER: Duration = Duration::from_secs(60);

/// Receives the state changes of the supervisor (reported to the SCM when
/// running as a service).
pub trait StatusSink {
    /// The wrapped process had its first chance to start.
    fn running(&mut self);
    /// Stopping is in progress and may take up to `wait_hint` more.
    fn stopping(&mut self, wait_hint: Duration);
}

/// Why the supervisor returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Stopped on request
    Stopped,
    /// Terminated on its own because of the wrapped process. Holds the exit
    /// code of the last run, None if it could not be started.
    Exited(Option<i32>),
}

/// Runs the wrapped process, restarting it when it exits, until `stop` is
/// triggered.
pub fn supervise(config: &RunConfig, stop: &StopSignal, status: &mut dyn StatusSink) -> Outcome {
    // Resolved once, when the service starts
    let env = match env::load(config.env_file.as_deref(), &config.env) {
        Ok(env) => env,
        Err(e) => {
            error!("Invalid environment: {}", e);
            return Outcome::Exited(None);
        }
    };
    if !env.is_empty() {
        // Values may hold secrets: only log the names
        let names: Vec<&str> = env.iter().map(|(key, _)| key.as_str()).collect();
        info!("Environment variables: {}", names.join(", "));
    }

    let mut backoff = Backoff::new(RESTART_DELAY, RESTART_MAX_DELAY, RESTART_RESET_AFTER);
    let mut first_start = true;
    loop {
        if stop.is_triggered() {
            return Outcome::Stopped;
        }
        let started_at = Instant::now();
        let process = run_command(config, &env);

        // Only report Running once the child had its chance to start. A start
        // failure is not fatal: the loop keeps retrying.
        if first_start {
            first_start = false;
            status.running();
        }

        match process {
            Err(e) => {
                error!("Failed to start command: {}", e);
            }
            Ok(mut process) => {
                info!("Child process started with PID: {}", process.id());

                // Wait for the child to exit or for a stop request
                while !stop.wait_timeout(POLL_INTERVAL) {
                    match process.child.try_wait() {
                        Ok(Some(exit_status)) => {
                            error!("Child exited with status: {}", exit_status);
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            info!("Failed to check child status: {}", e);
                            break;
                        }
                    }
                }

                if stop.is_triggered() {
                    status.stopping(STOP_WAIT_HINT);
                    info!("Stopping child process with PID: {}", process.id());
                }
                process.kill_tree();
            }
        }

        if !stop.is_triggered() {
            let delay = backoff.next_delay(started_at.elapsed());
            info!(
                "Restarting in {:?} (consecutive failures: {})",
                delay,
                backoff.consecutive_failures()
            );
            stop.wait_timeout(delay);
        }
    }
}
