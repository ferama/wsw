use std::{io, sync::OnceLock, time::Duration};
use tracing::error;
use windows::{
    Win32::System::Services::*,
    core::{PCWSTR, PWSTR},
};
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
};

use windows_service::service::{
    Service, ServiceAccess, ServiceDependency, ServiceErrorControl, ServiceInfo, ServiceStartType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use std::ffi::OsString;

use super::config::{RunConfig, ScmSettings, ServiceConfig, StartType};
use super::registry;
use super::restart::service_specific_code;
use super::security;
use super::stop_signal::StopSignal;
use super::supervisor::{Outcome, STOP_WAIT_HINT, StatusSink, supervise};
use windows_sys::Win32::Foundation::ERROR_BAD_CONFIGURATION;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
pub use super::SERVICE_DESCRIPTION_PREFIX;

pub fn get_service_desc(name: &str) -> String {
    if name == SERVICE_DESCRIPTION_PREFIX {
        SERVICE_DESCRIPTION_PREFIX.to_string()
    } else {
        format!("{}-{}", SERVICE_DESCRIPTION_PREFIX, name)
    }
}

/// Configuration of the service process. It is resolved by the `run`
/// command before the service dispatcher is started, so that `service_main`
/// never has to parse it again. An invalid configuration is kept as an error
/// so that it can be reported to the SCM.
static RUN_CONFIG: OnceLock<Result<RunConfig, String>> = OnceLock::new();

pub fn set_run_config(config: Result<RunConfig, String>) {
    let _ = RUN_CONFIG.set(config);
}

pub fn service_main(_args: Vec<OsString>) {
    let Some(config) = RUN_CONFIG.get() else {
        error!("Service started without configuration");
        return;
    };
    if let Err(e) = run_service(config) {
        error!("Service failed: {}", e);
    }
}

/// Reports the service state to the SCM, keeping track of the checkpoint
/// counter required by the pending states.
struct StatusReporter {
    handle: ServiceStatusHandle,
    checkpoint: u32,
}

impl StatusReporter {
    fn new(handle: ServiceStatusHandle) -> Self {
        Self {
            handle,
            checkpoint: 0,
        }
    }

    /// Reports a pending state. Every call bumps the checkpoint, telling the
    /// SCM that progress is being made and the wait hint starts over.
    fn pending(&mut self, state: ServiceState, wait_hint: Duration) {
        self.checkpoint += 1;
        self.set(
            state,
            ServiceControlAccept::empty(),
            ServiceExitCode::Win32(0),
            wait_hint,
        );
    }

    fn running(&mut self) {
        self.checkpoint = 0;
        self.set(
            ServiceState::Running,
            ServiceControlAccept::STOP
                | ServiceControlAccept::SHUTDOWN
                | ServiceControlAccept::PRESHUTDOWN,
            ServiceExitCode::Win32(0),
            Duration::default(),
        );
    }

    fn stopped(&mut self, exit_code: ServiceExitCode) {
        self.checkpoint = 0;
        self.set(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            exit_code,
            Duration::default(),
        );
    }

    fn set(
        &self,
        state: ServiceState,
        controls_accepted: ServiceControlAccept,
        exit_code: ServiceExitCode,
        wait_hint: Duration,
    ) {
        let status = ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: state,
            controls_accepted,
            exit_code,
            checkpoint: self.checkpoint,
            wait_hint,
            process_id: None,
        };
        if let Err(e) = self.handle.set_service_status(status) {
            error!("Failed to report service state {:?}: {}", state, e);
        }
    }
}

impl StatusSink for StatusReporter {
    fn running(&mut self) {
        StatusReporter::running(self);
    }

    fn stopping(&mut self, wait_hint: Duration) {
        self.pending(ServiceState::StopPending, wait_hint);
    }
}

/// How long the SCM should wait for the service to leave the start pending state.
const START_WAIT_HINT: Duration = Duration::from_secs(10);

fn run_service(config: &Result<RunConfig, String>) -> windows_service::Result<()> {
    let stop = StopSignal::new();
    let stop_handler = stop.clone();

    let name = match config {
        Ok(config) => config.name.clone(),
        Err(_) => String::new(),
    };
    let event_handler =
        service_control_handler::register(&name, move |control_event| match control_event {
            // On machine shutdown the child gets the same orderly stop as on
            // a regular stop request. Preshutdown is preferred by the SCM
            // because it allows to block the shutdown up to the preshutdown
            // timeout, Shutdown is handled too for completeness.
            ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Preshutdown => {
                stop_handler.trigger();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    let mut status = StatusReporter::new(event_handler);

    let config = match config {
        Ok(config) => config,
        Err(e) => {
            error!("Invalid service configuration: {}", e);
            status.stopped(ServiceExitCode::Win32(ERROR_BAD_CONFIGURATION));
            return Ok(());
        }
    };

    status.pending(ServiceState::StartPending, START_WAIT_HINT);

    let outcome = supervise(config, &stop, &mut status);

    status.pending(ServiceState::StopPending, STOP_WAIT_HINT);
    let exit_code = match outcome {
        Outcome::Stopped => ServiceExitCode::Win32(0),
        Outcome::Exited(child_exit_code) => {
            let code = service_specific_code(child_exit_code);
            error!(
                "Service terminated on its own, reporting exit code {}",
                code
            );
            ServiceExitCode::ServiceSpecific(code)
        }
    };
    status.stopped(exit_code);
    Ok(())
}

/// Installs the service: the ImagePath only carries the service name, the
/// rest of the configuration is persisted in the registry.
pub fn install_service(config: &ServiceConfig, scm: &ScmSettings) -> windows_service::Result<()> {
    let name = config.service_name();
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let executable_path = std::env::current_exe().map_err(windows_service::Error::Winapi)?;

    let launch_arguments = vec![
        OsString::from("run"),
        OsString::from("--name"),
        OsString::from(&name),
    ];

    let service_info = ServiceInfo {
        name: OsString::from(&name),
        display_name: OsString::from(get_service_desc(&name)),
        service_type: SERVICE_TYPE,
        start_type: scm_start_type(scm.start_type),
        error_control: ServiceErrorControl::Normal,
        executable_path,
        launch_arguments,
        dependencies: scm
            .depends_on
            .iter()
            .map(|service| ServiceDependency::Service(OsString::from(service)))
            .collect(),
        account_name: scm.account_name.clone().map(OsString::from),
        account_password: scm.account_password.clone().map(OsString::from),
    };

    let service = service_manager.create_service(
        &service_info,
        ServiceAccess::START | ServiceAccess::DELETE | ServiceAccess::CHANGE_CONFIG,
    )?;

    let configured = registry::write_config(&name, config)
        .map_err(windows_service::Error::Winapi)
        .and_then(|_| configure_service(&service, scm));
    if let Err(e) = configured {
        // Do not leave behind a half configured service
        let _ = service.delete();
        return Err(e);
    }

    if scm.start_type != StartType::Disabled {
        service.start::<std::ffi::OsString>(&[])?;
    }
    Ok(())
}

fn scm_start_type(start_type: StartType) -> ServiceStartType {
    match start_type {
        StartType::Auto | StartType::DelayedAuto => ServiceStartType::AutoStart,
        StartType::Manual => ServiceStartType::OnDemand,
        StartType::Disabled => ServiceStartType::Disabled,
    }
}

/// Applies the settings that are not part of CreateService/ChangeServiceConfig.
fn configure_service(service: &Service, scm: &ScmSettings) -> windows_service::Result<()> {
    service.set_delayed_auto_start(scm.start_type == StartType::DelayedAuto)?;

    let account = scm.account_name.as_deref();
    if scm.grant_logon_right
        && let Some(account) = account
    {
        security::grant_service_logon_right(account).map_err(windows_service::Error::Winapi)?;
    }
    // Virtual accounts (NT SERVICE\<name>) only exist once the service has been created
    for dir in &scm.grant_dirs {
        security::grant_directory_access(dir, account).map_err(windows_service::Error::Winapi)?;
    }
    Ok(())
}

pub fn uninstall_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(
        name,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    )?;

    let _ = service.stop().is_err(); // Ignore error if service is already stopped
    match wait_for_service_status(
        name,
        ServiceState::Stopped,
        std::time::Duration::from_secs(10),
    ) {
        Ok(_) => tracing::info!("Service '{}' is now stopped.", name),
        Err(e) => tracing::error!("Failed to wait for service '{}': {}", name, e),
    }

    // Now delete it
    service.delete()?;
    Ok(())
}

pub fn start_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::START | ServiceAccess::QUERY_STATUS)?;

    // Start the service
    service.start::<std::ffi::OsString>(&[])?;
    Ok(())
}

pub fn get_service_status(name: &str) -> windows_service::Result<ServiceStatus> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_STATUS)?;

    // Query the service status
    let status = service.query_status()?;
    Ok(status)
}

pub fn get_service_command_line(name: &str) -> windows_service::Result<String> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_CONFIG)?;

    let service_handle = SC_HANDLE(service.raw_handle());
    // Query the service info
    unsafe {
        // Query needed buffer size first
        let mut needed = 0u32;
        let result = QueryServiceConfigW(service_handle, None, 0, &mut needed);

        if result.is_ok() {
            return Err(windows_service::Error::Winapi(io::Error::other(
                "Unexpected result while querying service config",
            )));
        }

        // Allocate buffer
        let mut buffer = vec![0u8; needed as usize];
        let config_ptr =
            buffer.as_mut_ptr() as *mut windows::Win32::System::Services::QUERY_SERVICE_CONFIGW;

        QueryServiceConfigW(service_handle, Some(config_ptr), needed, &mut needed).map_err(
            |e| windows_service::Error::Winapi(std::io::Error::from_raw_os_error(e.code().0)),
        )?;

        let config = &*config_ptr;

        let binary_path = PCWSTR(config.lpBinaryPathName.0)
            .to_string()
            .map_err(|e| windows_service::Error::Winapi(io::Error::other(e)))?;

        Ok(binary_path)
    }
}

pub fn wait_for_service_status(
    name: &str,
    target_state: ServiceState,
    timeout: Duration,
) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(name, ServiceAccess::QUERY_STATUS)?;

    // Wait for the service to reach the target state
    let start = std::time::Instant::now();
    loop {
        let status = service.query_status()?;
        if status.current_state == target_state {
            break;
        }
        if start.elapsed() > timeout {
            tracing::error!("Timeout waiting for service status to change");
            return Err(windows_service::Error::Winapi(io::Error::new(
                io::ErrorKind::TimedOut,
                "operation timed out",
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Ok(())
}

pub fn stop_service(name: &str) -> windows_service::Result<()> {
    // Connect to the SCM
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    // Open the existing service
    let service = manager.open_service(
        name,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
    )?;

    // Stop the service
    service.stop()?;
    Ok(())
}

pub fn list_services_with_status() -> windows_service::Result<Vec<(String, String)>> {
    let mut service_list = Vec::new();

    unsafe {
        let scm_handle = match OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE) {
            Ok(handle) => handle,
            Err(win_err) => {
                let io_err = std::io::Error::from_raw_os_error(win_err.code().0);
                return Err(windows_service::Error::Winapi(io_err));
            }
        };
        if scm_handle.0.is_null() {
            return Err(windows_service::Error::Winapi(
                std::io::Error::from_raw_os_error(
                    windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32,
                ),
            ));
        }

        let mut bytes_needed = 0u32;
        let mut services_returned = 0u32;
        let mut resume_handle: u32 = 0;

        // First call to get buffer size
        let _ = EnumServicesStatusExW(
            scm_handle,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut bytes_needed,
            &mut services_returned,
            Some(&mut resume_handle),
            None,
        );

        let mut buffer: Vec<u8> = vec![0; bytes_needed as usize];

        let _ = EnumServicesStatusExW(
            scm_handle,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(&mut buffer),
            &mut bytes_needed,
            &mut services_returned,
            Some(&mut resume_handle),
            None,
        );

        let services = std::slice::from_raw_parts(
            buffer.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW,
            services_returned as usize,
        );

        for svc in services {
            let name = widestring_to_string(svc.lpServiceName);
            let display_name = widestring_to_string(svc.lpDisplayName);
            // Services installed by older versions are recognized by their
            // display name, newer ones by the marker in the registry
            if display_name.starts_with(SERVICE_DESCRIPTION_PREFIX)
                || registry::is_managed_by_wsw(&name)
            {
                let status = match svc.ServiceStatusProcess.dwCurrentState {
                    SERVICE_RUNNING => "Running".to_string(),
                    SERVICE_STOPPED => "Stopped".to_string(),
                    SERVICE_START_PENDING => "Start Pending".to_string(),
                    SERVICE_STOP_PENDING => "Stop Pending".to_string(),
                    SERVICE_CONTINUE_PENDING => "Continue Pending".to_string(),
                    SERVICE_PAUSE_PENDING => "Pause Pending".to_string(),
                    SERVICE_PAUSED => "Paused".to_string(),
                    _ => "Unknown".to_string(),
                };
                service_list.push((name, status));
            }
        }

        let _ = CloseServiceHandle(scm_handle);
    }
    Ok(service_list)
}

fn widestring_to_string(ptr: PWSTR) -> String {
    unsafe {
        if ptr.0.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *ptr.0.offset(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr.0, len as usize);
        String::from_utf16_lossy(slice)
    }
}
