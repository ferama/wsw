use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::pkg::config::RunConfig;
use crate::pkg::console;
use crate::pkg::env;
use crate::pkg::runner::{ChildProcess, run_command, spawn_shell_command};
use crate::pkg::stop_signal::StopSignal;

/// Extra time given to the SCM on top of the stop timeout, to tear down
/// the process tree and report the final state.
pub const STOP_MARGIN: Duration = Duration::from_secs(5);
/// How often the child process is checked for exit.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// How often the exit of a stopping child is checked.
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Receives the state changes of the supervisor (reported to the SCM when
/// running as a service).
pub trait StatusSink {
    /// Starting is in progress and may take up to `wait_hint` more.
    fn starting(&mut self, wait_hint: Duration);
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
    /// The wrapped process exited and the restart policy says not to
    /// restart it. Holds its exit code, None if it could not be started.
    Exited(Option<i32>),
    /// The wrapped process failed more than --max-restarts times in a row.
    /// Holds the exit code of the last run.
    GaveUp(Option<i32>),
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

    if let Some(pre_start) = &config.pre_start
        && let Err(exit_code) = run_hook("pre-start", pre_start, config, &env, status, false)
    {
        error!("The pre-start hook failed, not starting the service");
        return Outcome::Exited(exit_code.filter(|code| *code != 0).or(Some(1)));
    }

    let outcome = supervise_process(config, &env, stop, status, foreground);

    if let Some(post_stop) = &config.post_stop
        && run_hook("post-stop", post_stop, config, &env, status, true).is_err()
    {
        error!("The post-stop hook failed");
    }
    outcome
}

/// Runs a hook synchronously, killing it after the hook timeout. Returns
/// the exit code of a failed hook (None if it could not run or timed out).
fn run_hook(
    kind: &str,
    cmdline: &str,
    config: &RunConfig,
    env: &[(String, String)],
    status: &mut dyn StatusSink,
    stopping: bool,
) -> Result<(), Option<i32>> {
    let timeout = config.hook_timeout;
    let report = |status: &mut dyn StatusSink, remaining: Duration| {
        if stopping {
            status.stopping(remaining + STOP_MARGIN);
        } else {
            status.starting(remaining + STOP_MARGIN);
        }
    };
    report(status, timeout);
    info!("Running {} hook: {}", kind, cmdline);

    let mut process = match spawn_shell_command(cmdline, config, env) {
        Ok(process) => process,
        Err(e) => {
            error!("Failed to run the {} hook: {}", kind, e);
            return Err(None);
        }
    };
    let started = Instant::now();
    let mut last_report = started;
    loop {
        match process.child.try_wait() {
            Ok(Some(exit_status)) if exit_status.success() => {
                info!("The {} hook completed", kind);
                return Ok(());
            }
            Ok(Some(exit_status)) => {
                error!("The {} hook exited with status: {}", kind, exit_status);
                return Err(exit_status.code());
            }
            Ok(None) => {}
            Err(e) => {
                error!("Failed to check the {} hook status: {}", kind, e);
                return Err(None);
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            error!("The {} hook did not complete within {:?}", kind, timeout);
            // Dropping the process kills its whole tree
            return Err(None);
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            last_report = Instant::now();
            report(status, timeout - elapsed);
        }
        std::thread::sleep(STOP_POLL_INTERVAL);
    }
}

/// Runs the wrapped process, restarting it as configured, until `stop` is
/// triggered or the restart policy says to stop.
fn supervise_process(
    config: &RunConfig,
    env: &[(String, String)],
    stop: &StopSignal,
    status: &mut dyn StatusSink,
    foreground: bool,
) -> Outcome {
    let mut backoff = config.restart.backoff();
    let mut first_start = true;
    loop {
        if stop.is_triggered() {
            return Outcome::Stopped;
        }
        let started_at = Instant::now();
        let process = run_command(config, env);

        // Only report Running once the child had its chance to start. A start
        // failure is not fatal: the loop keeps retrying.
        if first_start {
            first_start = false;
            status.running();
        }

        // Exit code of this run, None if the process could not be started
        let exit_code = match process {
            Err(e) => {
                error!("Failed to start command: {}", e);
                None
            }
            Ok(mut process) => {
                info!("Child process started with PID: {}", process.id());

                // Wait for the child to exit or for a stop request
                let mut exit_code = None;
                while !stop.wait_timeout(POLL_INTERVAL) {
                    match process.child.try_wait() {
                        Ok(Some(exit_status)) => {
                            error!("Child exited with status: {}", exit_status);
                            exit_code = exit_status.code();
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
                    stop_gracefully(&mut process, config, env, status, foreground);
                }
                process.kill_tree();
                exit_code
            }
        };

        if stop.is_triggered() {
            return Outcome::Stopped;
        }

        let restart = &config.restart;
        if !restart.policy.should_restart(exit_code) {
            info!(
                "Not restarting the child process (restart policy: {:?})",
                restart.policy
            );
            return Outcome::Exited(exit_code);
        }

        let delay = backoff.next_delay(started_at.elapsed());
        if restart.gives_up(backoff.consecutive_failures()) {
            error!(
                "Child process failed {} times in a row, giving up",
                backoff.consecutive_failures()
            );
            return Outcome::GaveUp(exit_code);
        }
        info!(
            "Restarting in {:?} (consecutive failures: {})",
            delay,
            backoff.consecutive_failures()
        );
        stop.wait_timeout(delay);
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
