use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::pkg::config::RunConfig;
use crate::pkg::console;
use crate::pkg::env;
use crate::pkg::restart::Backoff;
use crate::pkg::runner::{ChildProcess, run_command, spawn_shell_command};
use crate::pkg::stop_signal::StopSignal;

/// Extra time given to the SCM on top of the stop timeout, to tear down
/// the process tree and report the final state.
pub const STOP_MARGIN: Duration = Duration::from_secs(5);
/// How often the child process is checked for exit.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// How often the exit of a stopping child is checked.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);
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
///
/// `foreground` is true when wsw runs in a terminal: the wrapped process
/// shares it and receives Ctrl+C directly from the user.
pub fn supervise(
    config: &RunConfig,
    stop: &StopSignal,
    status: &mut dyn StatusSink,
    foreground: bool,
) -> Outcome {
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
                    stop_gracefully(&mut process, config, &env, status, foreground);
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

/// Asks the wrapped process to exit, with the stop command or a Ctrl+C, and
/// waits for it up to the stop timeout. The caller kills whatever is left
/// of the process tree afterwards.
fn stop_gracefully(
    process: &mut ChildProcess,
    config: &RunConfig,
    env: &[(String, String)],
    status: &mut dyn StatusSink,
    foreground: bool,
) {
    let pid = process.id();
    let timeout = config.stop_timeout;
    status.stopping(timeout + STOP_MARGIN);
    info!(
        "Stopping child process with PID {} (timeout {:?})",
        pid, timeout
    );

    // Kept alive until the child exits: dropping it kills the stop command
    let mut _stop_process = None;
    if let Some(stop_cmd) = &config.stop_cmd {
        info!("Running stop command: {}", stop_cmd);
        match spawn_shell_command(stop_cmd, config, env) {
            Ok(stop_process) => _stop_process = Some(stop_process),
            Err(e) => error!("Failed to run the stop command: {}", e),
        }
    } else if foreground {
        // The child shares the terminal and got the user's Ctrl+C already
    } else if let Err(e) = console::send_ctrl_c(pid) {
        warn!("{}", e);
    }

    let started = Instant::now();
    let mut last_report = started;
    loop {
        match process.child.try_wait() {
            Ok(Some(exit_status)) => {
                info!(
                    "Child process exited after {:?} with status: {}",
                    started.elapsed(),
                    exit_status
                );
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!("Failed to check child status: {}", e);
                return;
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            warn!(
                "Child process did not exit within {:?}, killing the process tree",
                timeout
            );
            return;
        }
        // Keep the SCM informed that the stop is progressing
        if last_report.elapsed() >= Duration::from_secs(1) {
            last_report = Instant::now();
            status.stopping(timeout - elapsed + STOP_MARGIN);
        }
        std::thread::sleep(STOP_POLL_INTERVAL);
    }
}
