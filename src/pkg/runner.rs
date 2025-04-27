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

fn find_working_dir(exe: &str, working_dir: Option<String>) -> PathBuf {
    let mut cmd_working_dir: PathBuf = Path::new(".").to_path_buf();
    if let Some(dir) = working_dir {
        cmd_working_dir = PathBuf::from(dir);
    } else {
        if let Some(parent) = Path::new(exe).parent() {
            cmd_working_dir = Path::new(parent).to_path_buf();
        }
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

    cmd_working_dir
}

pub fn run_command(
    cmdline: &str,
    working_dir: Option<String>,
) -> Result<(HANDLE, Child), std::io::Error> {
    let mut parts = cmdline.split_whitespace();
    if let Some(exe) = parts.next() {
        let cmd_working_dir = find_working_dir(exe, working_dir);
        info!("Command: {:?}", cmdline);
        info!("Working directory: {:?}", cmd_working_dir);

        let job = create_job_object().unwrap();

        // let exe_args: Vec<&str> = parts.collect();
        let command = Command::new("powershell.exe")
            .arg("-Command")
            .arg(cmdline)
            // .args(&exe_args)
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
            info!("Process handle: {:?}", process_handle);
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
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("Command not found: {}", cmdline),
    ))
}
