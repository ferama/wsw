use encoding_rs::{Encoding, WINDOWS_1252};
use std::io::{self, Write};
use std::os::windows::io::AsRawHandle;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tracing::{error, info};
use which::which;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::core::PCWSTR;

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

pub const SERVICE_LOG_PREFIX: &str = "|SVC-LOG| ";

pub struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let decoded = try_decode(buf);

        match decoded {
            Some(text) => {
                for line in text.lines() {
                    if !line.is_empty() {
                        info!("{}{}", SERVICE_LOG_PREFIX, line);
                    }
                }
            }
            None => {
                error!("{}<unreadable data: {:?}>", SERVICE_LOG_PREFIX, buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn try_decode(buf: &[u8]) -> Option<String> {
    // 1. Try UTF-8
    if let Ok(s) = std::str::from_utf8(buf) {
        return Some(s.to_string());
    }

    // 2. Try UTF-16LE (only if even length)
    if buf.len() % 2 == 0 {
        let utf16: Vec<u16> = buf
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if let Ok(s) = String::from_utf16(&utf16) {
            return Some(s);
        }
    }

    // 3. Try Windows-1252
    let (s_win1252, _, had_errors) = WINDOWS_1252.decode(buf);
    if !had_errors {
        return Some(s_win1252.into_owned());
    }

    // 4. Try CP437 (OEM US)
    if let Some(cp437) = Encoding::for_label(b"ibm437") {
        let (s, _, had_errors) = cp437.decode(buf);
        if !had_errors {
            return Some(s.into_owned());
        }
    }

    // 5. Give up
    None
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
            // unsafe {
            // WaitForSingleObject(process_handle, INFINITE);
            // if let Err(e) = CloseHandle(std::mem::transmute(job)) {
            //     return Err(io::Error::new(
            //         io::ErrorKind::Other,
            //         format!("Failed to set information on Job Object: {}", e),
            //     ));
            // }
            // };
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
