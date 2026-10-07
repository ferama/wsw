//! Configuration stored by the SCM: applying it at install/update time and
//! querying it back.

use std::ffi::OsString;
use std::io;
use std::iter::once;
use std::ptr;
use std::time::Duration;

use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceDependency,
    ServiceFailureActions, ServiceFailureResetPeriod, ServiceStartType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfigW, QueryServiceConfig2W, SERVICE_CONFIG,
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DESCRIPTIONW, SERVICE_NO_CHANGE,
};

use crate::pkg::account::{AccountKind, scm_account_name};
use crate::pkg::config::{ScmSettings, ServiceConfig, StartType};
use crate::pkg::registry;
use crate::pkg::restart::ScmAction;
use crate::pkg::security;

pub fn scm_start_type(start_type: StartType) -> ServiceStartType {
    match start_type {
        StartType::Auto | StartType::DelayedAuto => ServiceStartType::AutoStart,
        StartType::Manual => ServiceStartType::OnDemand,
        StartType::Disabled => ServiceStartType::Disabled,
    }
}

fn start_type_from_scm(start_type: ServiceStartType, delayed: bool) -> Option<StartType> {
    match start_type {
        ServiceStartType::AutoStart if delayed => Some(StartType::DelayedAuto),
        ServiceStartType::AutoStart => Some(StartType::Auto),
        ServiceStartType::OnDemand => Some(StartType::Manual),
        ServiceStartType::Disabled => Some(StartType::Disabled),
        _ => None,
    }
}

/// Applies the settings that are not part of CreateService/ChangeServiceConfig.
/// `account` is the account the service runs as (None for LocalSystem), the
/// one the rights are granted to.
pub fn configure_service(
    service: &Service,
    scm: &ScmSettings,
    account: Option<&str>,
) -> windows_service::Result<()> {
    service.set_delayed_auto_start(scm.start_type == StartType::DelayedAuto)?;
    service.set_preshutdown_timeout(scm.preshutdown_timeout)?;
    if let Some(description) = &scm.description {
        service.set_description(description)?;
    }

    if let Some(actions) = &scm.failure_actions {
        let actions: Vec<ServiceAction> = actions
            .iter()
            .map(|action| match action {
                ScmAction::Restart(delay) => ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: *delay,
                },
                ScmAction::None => ServiceAction {
                    action_type: ServiceActionType::None,
                    delay: Duration::default(),
                },
            })
            .collect();
        let enabled = !actions.is_empty();
        service.update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(scm.failure_reset),
            reboot_msg: None,
            command: None,
            actions: Some(actions),
        })?;
        // Also apply them when the service stops with a non-zero exit code,
        // not only when its process crashes
        service.set_failure_actions_on_non_crash_failures(enabled)?;
    }

    if scm.grant_logon_right
        && let Some(account) = account
    {
        security::grant_service_logon_right(account).map_err(windows_service::Error::Winapi)?;
    }
    // Virtual accounts (NT SERVICE\<name>) only exist once the service has been created
    for dir in &scm.grant_dirs {
        security::grant_directory_access(dir, account).map_err(windows_service::Error::Winapi)?;
    }
    if let Some(dir) = &scm.log_dir {
        std::fs::create_dir_all(dir).map_err(windows_service::Error::Winapi)?;
        // LocalSystem can write anywhere already
        if account.is_some() {
            security::grant_directory_access(dir, account)
                .map_err(windows_service::Error::Winapi)?;
        }
    }
    Ok(())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

/// A change of the configuration stored by ChangeServiceConfigW. Unlike the
/// `windows-service` wrapper, an empty dependency list clears them.
struct ConfigChange<'a> {
    image_path: &'a str,
    start_type: ServiceStartType,
    dependencies: &'a [String],
    display_name: &'a str,
    /// Account and password, None to leave them unchanged
    account: Option<(&'a str, Option<&'a str>)>,
}

fn change_service_config(service: &Service, change: &ConfigChange) -> windows_service::Result<()> {
    let image_path = wide(change.image_path);
    let display_name = wide(change.display_name);
    // Double nul terminated list
    let mut dependencies: Vec<u16> = change
        .dependencies
        .iter()
        .flat_map(|dependency| wide(dependency))
        .collect();
    dependencies.push(0);
    if change.dependencies.is_empty() {
        dependencies.push(0);
    }
    let account = change.account.map(|(name, _)| wide(name));
    // Accounts without password (built-in, virtual, gMSA) need an empty one
    let password = change
        .account
        .map(|(_, password)| wide(password.unwrap_or("")));

    // Safety: every string is a nul terminated buffer that outlives the call.
    let ok = unsafe {
        ChangeServiceConfigW(
            service.raw_handle(),
            SERVICE_NO_CHANGE,
            change.start_type.to_raw(),
            SERVICE_NO_CHANGE,
            image_path.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            dependencies.as_ptr(),
            account.as_ref().map_or(ptr::null(), |a| a.as_ptr()),
            password.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
            display_name.as_ptr(),
        )
    };
    if ok == 0 {
        return Err(windows_service::Error::Winapi(io::Error::last_os_error()));
    }
    Ok(())
}

/// Reads a SERVICE_CONFIG_* information level. The buffer is made of u64
/// to be suitably aligned for the structure it holds, whose pointers point
/// inside the buffer itself.
fn query_config2(service: &Service, level: SERVICE_CONFIG) -> io::Result<Vec<u64>> {
    let mut needed: u32 = 0;
    // Safety: a zero sized query only returns the needed size.
    unsafe {
        QueryServiceConfig2W(service.raw_handle(), level, ptr::null_mut(), 0, &mut needed);
    }
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // Safety: the buffer is as large as declared.
    let ok = unsafe {
        QueryServiceConfig2W(
            service.raw_handle(),
            level,
            buffer.as_mut_ptr() as *mut u8,
            (buffer.len() * 8) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn query_description(service: &Service) -> Option<String> {
    let buffer = query_config2(service, SERVICE_CONFIG_DESCRIPTION).ok()?;
    // Safety: the buffer holds a SERVICE_DESCRIPTIONW whose string, if any,
    // is nul terminated and lives in the same buffer, still alive here.
    let text = unsafe {
        let description = &*(buffer.as_ptr() as *const SERVICE_DESCRIPTIONW);
        if description.lpDescription.is_null() {
            return None;
        }
        let mut len = 0;
        while *description.lpDescription.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(description.lpDescription, len))
    };
    (!text.is_empty()).then_some(text)
}

fn query_delayed_auto_start(service: &Service) -> bool {
    match query_config2(service, SERVICE_CONFIG_DELAYED_AUTO_START_INFO) {
        // Safety: the buffer holds a SERVICE_DELAYED_AUTO_START_INFO.
        Ok(buffer) => unsafe {
            (*(buffer.as_ptr() as *const SERVICE_DELAYED_AUTO_START_INFO)).fDelayedAutostart != 0
        },
        Err(_) => false,
    }
}

/// Everything known about an installed wsw service.
pub struct InstalledService {
    pub name: String,
    pub scm: windows_service::service::ServiceConfig,
    pub start_type: Option<StartType>,
    pub description: Option<String>,
    pub failure_actions: Option<ServiceFailureActions>,
    /// Path of the wsw executable the service runs
    pub executable: String,
    /// Runtime configuration
    pub runtime: ServiceConfig,
    /// Configured through the ImagePath by a previous version of wsw
    pub legacy: bool,
}

impl InstalledService {
    /// Account the service runs as, None for LocalSystem.
    pub fn account(&self) -> Option<String> {
        let account = self
            .scm
            .account_name
            .as_ref()?
            .to_string_lossy()
            .to_string();
        scm_account_name(&account)
    }

    /// The SCM settings as a configuration, to be layered with changes.
    /// The account is left out: its password cannot be read back.
    pub fn scm_config(&self) -> ServiceConfig {
        let dependencies: Vec<String> = self
            .scm
            .dependencies
            .iter()
            .filter_map(|dependency| match dependency {
                ServiceDependency::Service(name) => Some(name.to_string_lossy().to_string()),
                ServiceDependency::Group(_) => None,
            })
            .collect();
        ServiceConfig {
            name: Some(self.name.clone()),
            display_name: Some(self.scm.display_name.to_string_lossy().to_string()),
            start_type: self.start_type,
            depends_on: Some(dependencies),
            ..Default::default()
        }
    }
}

fn not_a_wsw_service(name: &str) -> windows_service::Error {
    windows_service::Error::Winapi(io::Error::other(format!(
        "'{}' is not a service managed by wsw",
        name
    )))
}

fn open_service(name: &str, access: ServiceAccess) -> windows_service::Result<Service> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    manager.open_service(name, access)
}

fn query(service: &Service, name: &str) -> windows_service::Result<InstalledService> {
    let scm = service.query_config()?;
    let image_path = scm.executable_path.to_string_lossy().to_string();
    let (executable, image_config) =
        ServiceConfig::from_image_path(&image_path).ok_or_else(|| not_a_wsw_service(name))?;

    let (runtime, legacy) = if image_config.cmd.is_some() || image_config.exe.is_some() {
        (image_config, true)
    } else {
        let stored = registry::read_config(name)
            .map_err(windows_service::Error::Winapi)?
            .ok_or_else(|| not_a_wsw_service(name))?;
        (stored, false)
    };

    let delayed = query_delayed_auto_start(service);
    Ok(InstalledService {
        name: name.to_string(),
        start_type: start_type_from_scm(scm.start_type, delayed),
        description: query_description(service),
        failure_actions: service.get_failure_actions().ok(),
        executable,
        runtime,
        legacy,
        scm,
    })
}

pub fn query_installed_service(name: &str) -> windows_service::Result<InstalledService> {
    let service = open_service(name, ServiceAccess::QUERY_CONFIG)?;
    query(&service, name)
}

/// Changes the configuration of an installed service, without reinstalling
/// it. Only the options set in `changes` are modified. Services installed by
/// previous versions are migrated: their configuration moves from the
/// ImagePath to the registry.
pub fn update_service(name: &str, changes: &ServiceConfig) -> windows_service::Result<()> {
    let service = open_service(
        name,
        ServiceAccess::QUERY_CONFIG | ServiceAccess::CHANGE_CONFIG | ServiceAccess::START,
    )?;
    let installed = query(&service, name)?;
    let invalid = |e: String| windows_service::Error::Winapi(io::Error::other(e));

    let mut changes = changes.clone();
    changes.name = Some(name.to_string());

    // Runtime configuration
    let runtime = installed.runtime.clone().merge(changes.runtime());
    runtime.resolve().map_err(invalid)?;

    // SCM configuration: the current one with the changes applied
    let config = installed
        .scm_config()
        .merge(runtime.clone())
        .merge(changes.clone());
    let scm = ServiceConfig {
        // The account is validated only if it changes
        account_name: changes.account_name.clone(),
        ..config
    }
    .scm()
    .map_err(invalid)?;

    let account_change = changes.account_name.as_ref().map(|account| {
        let name = match AccountKind::classify(account) {
            AccountKind::LocalSystem => "LocalSystem".to_string(),
            _ => scm.account_name.clone().unwrap_or_default(),
        };
        (name, scm.account_password.clone())
    });

    let image_path =
        crate::pkg::cmdline::join([installed.executable.as_str(), "run", "--name", name]);
    change_service_config(
        &service,
        &ConfigChange {
            image_path: &image_path,
            start_type: scm_start_type(scm.start_type),
            dependencies: &scm.depends_on,
            display_name: &scm.display_name,
            account: account_change
                .as_ref()
                .map(|(name, password)| (name.as_str(), password.as_deref())),
        },
    )?;
    registry::write_config(name, &runtime).map_err(windows_service::Error::Winapi)?;

    let account = match &account_change {
        Some(_) => scm.account_name.clone(),
        None => installed.account(),
    };
    configure_service(&service, &scm, account.as_deref())
}

/// Dependencies of a service as names, for display.
pub fn dependency_names(dependencies: &[ServiceDependency]) -> Vec<String> {
    dependencies
        .iter()
        .map(|dependency| match dependency {
            ServiceDependency::Service(name) => name.to_string_lossy().to_string(),
            ServiceDependency::Group(name) => {
                let mut group = OsString::from("+");
                group.push(name);
                group.to_string_lossy().to_string()
            }
        })
        .collect()
}
