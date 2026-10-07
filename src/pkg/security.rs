//! Rights granted to the service account at install time
//! (`--grant-logon-right`, `--grant-dir`).

use std::io;
use std::iter::once;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};

use crate::pkg::account::{builtin_sid, lookup_account_name};
use windows_sys::Win32::Security::Authentication::Identity::{
    LSA_HANDLE, LSA_OBJECT_ATTRIBUTES, LSA_UNICODE_STRING, LsaAddAccountRights, LsaClose,
    LsaNtStatusToWinError, LsaOpenPolicy, POLICY_CREATE_ACCOUNT, POLICY_LOOKUP_NAMES,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, LookupAccountNameW, PSECURITY_DESCRIPTOR, PSID, SID_NAME_USE,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

/// Resolves an account name (None for LocalSystem) to its SID.
fn account_sid(account: Option<&str>) -> io::Result<Vec<u8>> {
    if let Some(sid) = builtin_sid(account) {
        return Ok(sid);
    }
    let name = lookup_account_name(account);
    let wide_name = wide(&name);
    let mut sid_size: u32 = 0;
    let mut domain_size: u32 = 0;
    let mut sid_use: SID_NAME_USE = 0;
    // Safety: the first call only queries the buffer sizes.
    unsafe {
        LookupAccountNameW(
            ptr::null(),
            wide_name.as_ptr(),
            ptr::null_mut(),
            &mut sid_size,
            ptr::null_mut(),
            &mut domain_size,
            &mut sid_use,
        );
    }
    if sid_size == 0 {
        return Err(io::Error::other(format!(
            "cannot find account '{}': {}",
            name,
            io::Error::last_os_error()
        )));
    }

    let mut sid = vec![0u8; sid_size as usize];
    let mut domain = vec![0u16; domain_size.max(1) as usize];
    // Safety: the buffers have the sizes returned by the previous call.
    let ok = unsafe {
        LookupAccountNameW(
            ptr::null(),
            wide_name.as_ptr(),
            sid.as_mut_ptr() as PSID,
            &mut sid_size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut sid_use,
        )
    };
    if ok == 0 {
        return Err(io::Error::other(format!(
            "cannot find account '{}': {}",
            name,
            io::Error::last_os_error()
        )));
    }
    Ok(sid)
}

/// Grants the "Log on as a service" right (SeServiceLogonRight) to the
/// account, like WinSW <allowservicelogon>.
pub fn grant_service_logon_right(account: &str) -> io::Result<()> {
    let mut sid = account_sid(Some(account))?;

    let attributes = LSA_OBJECT_ATTRIBUTES::default();
    let mut policy: LSA_HANDLE = 0;
    // Safety: `attributes` and `policy` are valid for the duration of the call.
    let status = unsafe {
        LsaOpenPolicy(
            ptr::null(),
            &attributes,
            (POLICY_CREATE_ACCOUNT | POLICY_LOOKUP_NAMES) as u32,
            &mut policy,
        )
    };
    if status != 0 {
        return Err(lsa_error("LsaOpenPolicy", status));
    }

    let mut right: Vec<u16> = "SeServiceLogonRight".encode_utf16().collect();
    let right = LSA_UNICODE_STRING {
        Length: (right.len() * 2) as u16,
        MaximumLength: (right.len() * 2) as u16,
        Buffer: right.as_mut_ptr(),
    };
    // Safety: the policy handle is open, the SID and the string are valid.
    let status = unsafe { LsaAddAccountRights(policy, sid.as_mut_ptr() as PSID, &right, 1) };
    // Safety: the handle has been opened above and is closed only here.
    unsafe {
        LsaClose(policy);
    }
    if status != 0 {
        return Err(lsa_error("LsaAddAccountRights", status));
    }
    Ok(())
}

fn lsa_error(function: &str, status: i32) -> io::Error {
    // Safety: plain conversion of a status code.
    let code = unsafe { LsaNtStatusToWinError(status) };
    io::Error::other(format!(
        "{} failed: {}",
        function,
        io::Error::from_raw_os_error(code as i32)
    ))
}

/// Frees memory allocated by the system with LocalAlloc on drop.
struct LocalMemory(*mut core::ffi::c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // Safety: the pointer has been allocated by the system with LocalAlloc.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

/// Grants the account "Modify" permissions on a directory, inherited by
/// every file and subdirectory (like `icacls <dir> /grant <account>:(OI)(CI)M`).
/// `account` is None for LocalSystem.
pub fn grant_directory_access(path: &str, account: Option<&str>) -> io::Result<()> {
    let mut sid = account_sid(account)?;
    let wide_path = wide(path);

    let mut old_dacl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // Safety: output pointers are valid, the descriptor is freed by LocalMemory.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut old_dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    let _descriptor = LocalMemory(descriptor);
    if status != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "cannot read the permissions of '{}': {}",
            path,
            io::Error::from_raw_os_error(status as i32)
        )));
    }

    let access = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_GENERIC_READ
            | FILE_GENERIC_WRITE
            | FILE_GENERIC_EXECUTE
            | DELETE,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.as_mut_ptr() as *mut u16,
        },
    };
    let mut new_dacl: *mut ACL = ptr::null_mut();
    // Safety: `old_dacl` belongs to `descriptor`, still alive; `new_dacl` is freed by LocalMemory.
    let status = unsafe { SetEntriesInAclW(1, &access, old_dacl, &mut new_dacl) };
    let _new_dacl = LocalMemory(new_dacl as *mut _);
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }

    // Safety: `new_dacl` is a valid ACL built by SetEntriesInAclW.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            new_dacl,
            ptr::null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::other(format!(
            "cannot change the permissions of '{}': {}",
            path,
            io::Error::from_raw_os_error(status as i32)
        )));
    }
    Ok(())
}
