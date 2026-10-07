use regex::Regex;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
};
use tracing::info;
use which::which;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

use crate::pkg::log_writer::LogWriter;

/// Owned Job Object handle. Closing it kills every process assigned to it
/// because the job is created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
pub struct Job(HANDLE);

impl Drop for Job {
    fn drop(&mut self) {
        // Safety: the handle is owned by this struct and closed only once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn create_job_object() -> io::Result<Job> {
    // Safety: plain FFI call, the returned handle is checked before use.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        return Err(io::Error::other(format!(
            "CreateJobObjectW failed: {}",
            io::Error::last_os_error()
        )));
    }
    let job = Job(handle);

    // Set the Job Object to kill all processes on close
    // Safety: JOBOBJECT_EXTENDED_LIMIT_INFORMATION is a plain C struct, all zeroes is valid.
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    // Safety: `info` outlives the call and the size matches the information class.
    let set_result = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if set_result == 0 {
        return Err(io::Error::other(format!(
            "Failed to set information on Job Object: {}",
            io::Error::last_os_error()
        )));
    }

    Ok(job)
}

fn assign_to_job(job: &Job, child: &Child) -> io::Result<()> {
    // Safety: both handles are valid for the duration of the call.
    let assign_result = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) };
    if assign_result == 0 {
        return Err(io::Error::other(format!(
            "Failed to assign process to Job Object: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// A running child process together with the Job Object that owns its
/// whole process tree.
pub struct ChildProcess {
    pub child: Child,
    // Dropping the job kills the whole process tree
    job: Option<Job>,
}

impl ChildProcess {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Kills the whole process tree and reaps the child.
    pub fn kill_tree(&mut self) {
        self.job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

fn find_working_dir(cmdline: &str, working_dir: Option<String>) -> PathBuf {
    let mut cmd_working_dir: PathBuf = Path::new(".").to_path_buf();

    // Check if the working directory is provided and not empty
    if let Some(dir) = working_dir {
        cmd_working_dir = PathBuf::from(dir);
        if cmd_working_dir != Path::new("") {
            return cmd_working_dir;
        }
    }

    // Attempt to find the working directory from the command line
    // Split the command line into parts and get the first part as the executable name
    if let Some(exe) = extract_executable(cmdline) {
        if let Some(parent) = Path::new(&exe).parent() {
            cmd_working_dir = Path::new(parent).to_path_buf();
        }

        if cmd_working_dir == Path::new("")
            && let Ok(path) = which(exe)
            && let Some(parent) = path.parent()
        {
            cmd_working_dir = Path::new(parent).to_path_buf();
        }
    }

    cmd_working_dir
}

pub fn run_command(
    cmdline: &str,
    working_dir: Option<String>,
    disable_logs: bool,
) -> io::Result<ChildProcess> {
    // detect the more appropriate working directory for the command line
    let cmd_working_dir = find_working_dir(cmdline, working_dir);
    info!("Command: {:?}", cmdline);
    info!("Working directory: {:?}", cmd_working_dir);

    // Create a Job Object
    // The Job Object is used to manage the process and its children
    // and to ensure that all processes are terminated when the Job Object is closed
    // or when the process exits. Windows does not supports child processes
    // that are not part of the Job Object. It's not like Linux where you can fork a child process
    // and it will be a child of the parent process. In Windows, the child process is not a child of the parent process
    // unless the parent process is a Job Object. So we need to create a Job Object and assign the process to it.
    let job = create_job_object()?;

    // When logs are disabled nobody would drain the pipes and the child
    // would block as soon as they are full, so discard the output instead.
    let output = || {
        if disable_logs {
            Stdio::null()
        } else {
            Stdio::piped()
        }
    };

    let mut child = Command::new("cmd.exe")
        .arg("/C")
        .arg(cmdline)
        .stdin(Stdio::null())
        .stdout(output())
        .stderr(output())
        .current_dir(cmd_working_dir)
        .spawn()?;

    if let Err(e) = assign_to_job(&job, &child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }

    if let Some(mut stdout) = child.stdout.take() {
        let mut stdout_logger = LogWriter;
        thread::spawn(move || {
            let _ = std::io::copy(&mut stdout, &mut stdout_logger);
        });
    }
    if let Some(mut stderr) = child.stderr.take() {
        let mut stderr_logger = LogWriter;
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut stderr_logger);
        });
    }

    Ok(ChildProcess {
        child,
        job: Some(job),
    })
}

fn extract_executable(command: &str) -> Option<String> {
    // Regex to capture quoted or unquoted executable paths at the beginning.
    // For unquoted paths, we look for patterns ending with .exe (common executable extension)
    // followed by a space and arguments, or end of string.
    let re = Regex::new(r#"^(?:"([^"]+)"|([^"]*\.exe))(?:\s|$)"#).ok()?;

    let caps = re.captures(command)?;
    // Choose the matching capture group: either quoted (1) or unquoted (2)
    caps.get(1)
        .or_else(|| caps.get(2))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_executable_with_quoted_path() {
        let command = r#""C:\Program Files\SomeApp\app.exe" --arg1 --arg2"#;
        let result = extract_executable(command);
        assert_eq!(
            result,
            Some(String::from(r#"C:\Program Files\SomeApp\app.exe"#))
        );
    }

    #[test]
    fn test_extract_executable_with_unquoted_path() {
        let command = r#"C:\SomeApp\app.exe --arg1 --arg2"#;
        let result = extract_executable(command);
        assert_eq!(result, Some(String::from(r#"C:\SomeApp\app.exe"#)));
    }

    #[test]
    fn test_extract_executable_with_no_arguments() {
        let command = r#"C:\SomeApp\app.exe"#;
        let result = extract_executable(command);
        assert_eq!(result, Some(String::from(r#"C:\SomeApp\app.exe"#)));
    }

    #[test]
    fn test_executable_with_spaces() {
        let cmdline = r#"C:\Program Files\app.exe --arg1"#;
        let result = extract_executable(cmdline);
        assert_eq!(result, Some(String::from(r#"C:\Program Files\app.exe"#)));
    }

    #[test]
    fn test_extract_executable_with_empty_string() {
        let command = r#""#;
        let result = extract_executable(command);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_working_dir_with_provided_working_dir() {
        let cmdline = r#"C:\SomeApp\app.exe --arg1"#;
        let working_dir = Some(String::from(r#"C:\CustomDir"#));
        let result = find_working_dir(cmdline, working_dir);
        assert_eq!(result, PathBuf::from(r#"C:\CustomDir"#));
    }

    #[test]
    fn test_find_working_dir_with_executable_path() {
        let cmdline = r#"C:\SomeApp\app.exe --arg1"#;
        let result = find_working_dir(cmdline, None);
        assert_eq!(result, PathBuf::from(r#"C:\SomeApp"#));
    }

    #[test]
    fn test_find_working_dir_with_empty_command() {
        let cmdline = r#""#;
        let result = find_working_dir(cmdline, None);
        assert_eq!(result, PathBuf::from(r#"."#));
    }
}
