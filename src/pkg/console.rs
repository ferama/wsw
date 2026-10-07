//! Console control events, used to ask the wrapped process to stop
//! gracefully with a Ctrl+C, like WinSW does.
//!
//! A service has no console, so the wrapped (console) process gets a console
//! of its own when it is started. To stop it, wsw temporarily attaches to
//! that console, generates a Ctrl+C for every process attached to it and
//! detaches again. wsw itself must survive the event it generates: a handler
//! that swallows Ctrl+C and Ctrl+Break is registered once at startup. A
//! handler routine is used instead of `SetConsoleCtrlHandler(NULL, TRUE)`,
//! because the latter would be inherited by the processes started later,
//! which would then ignore Ctrl+C themselves.
//!
//! When wsw runs in the foreground (`wsw run` from a terminal) the wrapped
//! process shares the terminal and receives Ctrl+C directly; the handler then
//! only asks the supervisor to stop.

use std::io;
use std::sync::OnceLock;

use windows_sys::Win32::System::Console::{
    AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, FreeConsole,
    GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
};
use windows_sys::core::BOOL;

use crate::pkg::stop_signal::StopSignal;

/// Triggered by Ctrl+C when running in the foreground.
static FOREGROUND_STOP: OnceLock<StopSignal> = OnceLock::new();

unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
            if let Some(stop) = FOREGROUND_STOP.get() {
                stop.trigger();
            }
            1
        }
        _ => 0,
    }
}

/// Registers the console control handler. `foreground_stop` is the signal
/// to trigger on Ctrl+C when running in a terminal, None as a service.
pub fn install_ctrl_handler(foreground_stop: Option<StopSignal>) -> io::Result<()> {
    if let Some(stop) = foreground_stop {
        let _ = FOREGROUND_STOP.set(stop);
    }
    // Safety: the handler is a plain function that lives for the whole process.
    if unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Sends Ctrl+C to the console of the process `pid`, reaching every
/// process attached to it. Must not be used while wsw has a console of its
/// own (foreground mode).
pub fn send_ctrl_c(pid: u32) -> io::Result<()> {
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
        let sent = GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0);
        let error = io::Error::last_os_error();
        FreeConsole();
        if sent == 0 {
            return Err(io::Error::other(format!(
                "cannot send Ctrl+C to process {}: {}",
                pid, error
            )));
        }
    }
    Ok(())
}
