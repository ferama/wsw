//! Persistence of the wsw configuration under the service registry key.
//!
//! Services installed by wsw only carry `run --name <name>` in their
//! ImagePath; the runtime configuration is stored as TOML in
//! `HKLM\SYSTEM\CurrentControlSet\Services\<name>\Parameters\Config`, next to
//! a `ManagedBy = wsw` marker. The key is deleted by the SCM together with the
//! service. Storing it here instead of the ImagePath avoids the length and
//! quoting limits of the command line, and the SCM expanding `%VARS%` in the
//! ImagePath when the service starts.

use std::io;
use std::iter::once;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
};

use crate::pkg::config::ServiceConfig;

const CONFIG_VALUE: &str = "Config";
const MANAGED_BY_VALUE: &str = "ManagedBy";
const MANAGED_BY: &str = "wsw";

fn parameters_key(service: &str) -> String {
    format!(r"SYSTEM\CurrentControlSet\Services\{}\Parameters", service)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

fn check(status: WIN32_ERROR) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// An open registry key, closed on drop.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // Safety: the key is owned by this struct and closed only once.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

impl Key {
    fn create(path: &str) -> io::Result<Key> {
        let path = wide(path);
        let mut handle: HKEY = ptr::null_mut();
        // Safety: every pointer is valid for the duration of the call.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                ptr::null(),
                &mut handle,
                ptr::null_mut(),
            )
        };
        check(status)?;
        Ok(Key(handle))
    }

    fn set_string(&self, value: &str, data: &str) -> io::Result<()> {
        let value = wide(value);
        let data = wide(data);
        // Safety: `data` is a nul terminated UTF-16 buffer of the given byte length.
        let status = unsafe {
            RegSetValueExW(
                self.0,
                value.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            )
        };
        check(status)
    }
}

/// Reads a REG_SZ value under HKLM, returning None if it does not exist.
fn get_string(path: &str, value: &str) -> io::Result<Option<String>> {
    let path = wide(path);
    let value = wide(value);
    loop {
        let mut size: u32 = 0;
        // Safety: a null data pointer asks for the required size only.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;

        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        // Safety: `buffer` is `size` bytes long, as reported by the previous call.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr() as *mut _,
                &mut size,
            )
        };
        match status {
            // The value grew in between the two calls: try again
            windows_sys::Win32::Foundation::ERROR_MORE_DATA => continue,
            ERROR_FILE_NOT_FOUND => return Ok(None),
            status => check(status)?,
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        return Ok(Some(String::from_utf16_lossy(&buffer[..len])));
    }
}

/// Persists the runtime configuration of a service and marks it as managed
/// by wsw.
pub fn write_config(service: &str, config: &ServiceConfig) -> io::Result<()> {
    let text = config.runtime().to_toml().map_err(io::Error::other)?;
    let key = Key::create(&parameters_key(service))?;
    key.set_string(CONFIG_VALUE, &text)?;
    key.set_string(MANAGED_BY_VALUE, MANAGED_BY)
}

/// Reads the configuration persisted by [`write_config`], if any. Services
/// installed by older wsw versions have none: their whole configuration is
/// in the ImagePath.
pub fn read_config(service: &str) -> io::Result<Option<ServiceConfig>> {
    match get_string(&parameters_key(service), CONFIG_VALUE)? {
        Some(text) => ServiceConfig::from_toml(&text)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        None => Ok(None),
    }
}

/// Whether the service has been installed by wsw (any version since the
/// marker was introduced).
pub fn is_managed_by_wsw(service: &str) -> bool {
    matches!(
        get_string(&parameters_key(service), MANAGED_BY_VALUE),
        Ok(Some(value)) if value == MANAGED_BY
    )
}
