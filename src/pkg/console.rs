//! Console control events, used to ask the wrapped process to stop
//! gracefully with a Ctrl+C, like WinSW does.
//!
//! A service has no console, so the wrapped (console) process gets a console
//! of its own when it is started. To stop it, wsw temporarily attaches to
//! that console, generates a Ctrl+C for every process attached to it and
//! detaches again. wsw itself must survive the event it generates: it
//! ignores Ctrl+C (`SetConsoleCtrlHandler(NULL, TRUE)`) from right after
//! attaching until the wrapped process exited, then restores the normal
//! handling, since the attribute would be inherited by the processes started
//! later, which would then ignore Ctrl+C themselves. A handler routine that
//! swallows Ctrl+C is registered at startup too, as a second line of defence.
//!
//! When wsw runs in the foreground (`wsw run` from a terminal) the wrapped
//! process shares the terminal and receives Ctrl+C directly; the handler then
//! only asks the supervisor to stop.

use std::io;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Console::{
    AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, FreeConsole,
    GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
};
use windows_sys::core::BOOL;

use crate::pkg::stop_signal::StopSignal;

/// Triggered by Ctrl+C when running in the foreground.
static FOREGROUND_STOP: OnceLock<StopSignal> = OnceLock::new();
/// Triggered once the foreground supervisor has stopped.
static FOREGROUND_DONE: OnceLock<StopSignal> = OnceLock::new();

/// Windows terminates the process as soon as the handler returns from a
/// close event, and at most 5 seconds after it was raised.
const CLOSE_GRACE: Duration = Duration::from_millis(4500);

unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
            if let Some(stop) = FOREGROUND_STOP.get() {
                stop.trigger();
                // The console window is being closed: give the supervisor the
                // time to stop the process in an orderly way
                if ctrl_type == CTRL_CLOSE_EVENT
                    && let Some(done) = FOREGROUND_DONE.get()
                {
                    done.wait_timeout(CLOSE_GRACE);
                }
            }
            1
        }
        _ => 0,
    }
}

/// Tells a pending console close that the foreground supervisor stopped.
pub fn foreground_stopped() {
    if let Some(done) = FOREGROUND_DONE.get() {
        done.trigger();
    }
}

/// Registers the console control handler. `foreground_stop` is the signal
/// to trigger on Ctrl+C when running in a terminal, None as a service.
pub fn install_ctrl_handler(foreground_stop: Option<StopSignal>) -> io::Result<()> {
    if let Some(stop) = foreground_stop {
        let _ = FOREGROUND_STOP.set(stop);
        let _ = FOREGROUND_DONE.set(StopSignal::new());
    }
    // Safety: plain FFI calls, the handler is a function that lives for the
    // whole process.
    unsafe {
        // wsw may have been started with Ctrl+C ignored (e.g. by a launcher
        // using CREATE_NEW_PROCESS_GROUP): the attribute is inherited by the
        // wrapped process, which would then ignore the graceful stop
        SetConsoleCtrlHandler(None, 0);
        if SetConsoleCtrlHandler(Some(ctrl_handler), 1) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Keeps wsw ignoring Ctrl+C while a Ctrl+C it generated is being
/// delivered. Dropping it restores the normal handling.
pub struct IgnoreCtrlC {
    sent_at: Instant,
}

/// The Ctrl+C is delivered asynchronously, by a thread the console creates
/// in every attached process: wsw keeps ignoring it at least this long.
const DELIVERY_GRACE: Duration = Duration::from_secs(1);

impl Drop for IgnoreCtrlC {
    fn drop(&mut self) {
        let elapsed = self.sent_at.elapsed();
        if elapsed < DELIVERY_GRACE {
            std::thread::sleep(DELIVERY_GRACE - elapsed);
        }
        // Safety: plain FFI call. The "ignore Ctrl+C" attribute is inherited
        // by the processes started later: it must not outlive the stop.
        unsafe {
            SetConsoleCtrlHandler(None, 0);
        }
    }
}

/// Sends Ctrl+C to the console of the process `pid`, reaching every
/// process attached to it. Must not be used while wsw has a console of its
/// own (foreground mode).
///
/// wsw is attached to that console while the event is generated, so it
/// receives the Ctrl+C too: it ignores it (`SetConsoleCtrlHandler(NULL,
/// TRUE)`, set after attaching, as WinSW does) until the returned guard is
/// dropped. Keep the guard until the wrapped process exited or the stop
/// timeout expired, and drop it before starting any other process.
pub fn send_ctrl_c(pid: u32) -> io::Result<IgnoreCtrlC> {
    // Safety: plain FFI calls without pointers.
    unsafe {
        // A process can be attached to a single console at a time
        FreeConsole();
        if AttachConsole(pid) == 0 {
            return Err(io::Error::other(format!(
                "cannot attach to the console of process {}: {}",
                pid,
                io::Error::last_os_error()
            )));
        }
        let guard = IgnoreCtrlC {
            sent_at: Instant::now(),
        };
        SetConsoleCtrlHandler(None, 1);
        let sent = GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0);
        let error = io::Error::last_os_error();
        FreeConsole();
        if sent == 0 {
            return Err(io::Error::other(format!(
                "cannot send Ctrl+C to process {}: {}",
                pid, error
            )));
        }
        Ok(guard)
    }
}
