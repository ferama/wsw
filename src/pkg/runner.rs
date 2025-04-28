use std::io::{self};
use std::os::windows::io::AsRawHandle;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tracing::info;
use which::which;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::core::PCWSTR;

use crate::pkg::logs_writer::LogWriter;

unsafe extern "system" {
    pub unsafe fn CreateJobObjectW(
        lpJobAttributes: *const SECURITY_ATTRIBUTES,
        lpName: PCWSTR,
    ) -> HANDLE;
}
fn create_job_object() -> Result<HANDLE, std::io::Error> {
    unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
            panic!("CreateJobObjectW failed, error={}", GetLastError());
        }

        // Set the Job Object to kill all processes on close
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let set_result = SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if set_result == 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to set information on Job Object: {}",
                    GetLastError()
                ),
            ));
        }

        Ok(handle)
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
    // TODO: the split_whitespace is not realiable, it will not work for all cases
    let mut parts = cmdline.split_whitespace();
    if let Some(exe) = parts.next() {
        if let Some(parent) = Path::new(exe).parent() {
            cmd_working_dir = Path::new(parent).to_path_buf();
        }

        if cmd_working_dir == Path::new("") {
            match which(exe) {
                Ok(path) => {
                    if let Some(parent) = path.parent() {
                        cmd_working_dir = Path::new(parent).to_path_buf();
                    }
                }
                Err(_) => {}
            }
        }
    }

    cmd_working_dir
}

pub fn run_command(
    cmdline: &str,
    working_dir: Option<String>,
) -> Result<(HANDLE, Child), std::io::Error> {
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

    // Use the job handle to create a new process to ensure
    // properly parsed command line arguments
    let command = Command::new("powershell.exe")
        .arg("-Command")
        .arg(cmdline)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(cmd_working_dir)
        .spawn()
        .map(|mut child| {
            if let Some(stdout) = child.stdout.take() {
                if let Some(stderr) = child.stderr.take() {
                    let logger = LogWriter;

                    let stdout = Arc::new(Mutex::new(stdout));
                    let stderr = Arc::new(Mutex::new(stderr));
                    let logger = Arc::new(Mutex::new(logger));

                    let stdout_clone = Arc::clone(&stdout);
                    let logger_clone = Arc::clone(&logger);
                    thread::spawn(move || {
                        let _ = std::io::copy(
                            &mut *stdout_clone.lock().unwrap(),
                            &mut *logger_clone.lock().unwrap(),
                        );
                    });

                    let stderr_clone = Arc::clone(&stderr);
                    let logger_clone = Arc::clone(&logger);
                    thread::spawn(move || {
                        let _ = std::io::copy(
                            &mut *stderr_clone.lock().unwrap(),
                            &mut *logger_clone.lock().unwrap(),
                        );
                    });
                }
            }
            child
        });

    if let Ok(child) = &command {
        let process_handle = child.as_raw_handle();
        let assign_result = unsafe { AssignProcessToJobObject(job, process_handle) };
        if assign_result == 0 {
            unsafe {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to assign process to Job Object: {}", GetLastError()),
                ));
            }
        }
    }

    unsafe {
        if let Ok(child) = command {
            return Ok((std::mem::transmute(job), child));
        } else {
            return Err(command.unwrap_err());
        }
    }
}
