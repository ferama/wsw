/// Kind of account a service can run as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    /// `LocalSystem` (`NT AUTHORITY\SYSTEM`)
    LocalSystem,
    /// `NT AUTHORITY\LocalService`
    LocalService,
    /// `NT AUTHORITY\NetworkService`
    NetworkService,
    /// Virtual account, `NT SERVICE\<service name>`
    Virtual,
    /// Group managed service account, `DOMAIN\name$`
    ManagedService,
    /// Regular local or domain user
    User,
}

impl AccountKind {
    pub fn classify(account: &str) -> Self {
        let account = account.trim();
        let (domain, user) = match account.rsplit_once('\\') {
            Some((domain, user)) => (Some(domain.to_ascii_lowercase()), user),
            None => (None, account),
        };
        let user_lc = user.to_ascii_lowercase();
        let domain = domain.as_deref();

        match (domain, user_lc.as_str()) {
            (None | Some("."), "localsystem") => AccountKind::LocalSystem,
            (Some("nt authority"), "system") => AccountKind::LocalSystem,
            (None | Some("nt authority"), "localservice" | "local service") => {
                AccountKind::LocalService
            }
            (None | Some("nt authority"), "networkservice" | "network service") => {
                AccountKind::NetworkService
            }
            (Some("nt service"), name) if !name.is_empty() => AccountKind::Virtual,
            (_, name) if name.len() > 1 && name.ends_with('$') => AccountKind::ManagedService,
            _ => AccountKind::User,
        }
    }

    /// Built-in, virtual and managed accounts have no password: Windows
    /// manages their credentials.
    pub fn requires_password(self) -> bool {
        self == AccountKind::User
    }

    /// Whether the account holds the "Log on as a service" right already:
    /// built-in accounts always do, virtual accounts get it from the SCM.
    pub fn has_implicit_logon_right(self) -> bool {
        !matches!(self, AccountKind::User | AccountKind::ManagedService)
    }
}

/// Returns the account name in the form the SCM expects, or None for
/// LocalSystem (the SCM default).
pub fn scm_account_name(account: &str) -> Option<String> {
    match AccountKind::classify(account) {
        AccountKind::LocalSystem => None,
        AccountKind::LocalService => Some(r"NT AUTHORITY\LocalService".to_string()),
        AccountKind::NetworkService => Some(r"NT AUTHORITY\NetworkService".to_string()),
        _ => Some(account.trim().to_string()),
    }
}

/// Name to resolve the account SID with. LookupAccountName knows the
/// built-in accounts only by their LSA names (`NT AUTHORITY\NETWORK SERVICE`,
/// with the space, not the `NetworkService` spelling the SCM accepts) and
/// does not understand the `.\user` notation for local users.
/// `account` is None for LocalSystem.
pub fn lookup_account_name(account: Option<&str>) -> String {
    let Some(account) = account else {
        return r"NT AUTHORITY\SYSTEM".to_string();
    };
    match AccountKind::classify(account) {
        AccountKind::LocalSystem => r"NT AUTHORITY\SYSTEM".to_string(),
        AccountKind::LocalService => r"NT AUTHORITY\LOCAL SERVICE".to_string(),
        AccountKind::NetworkService => r"NT AUTHORITY\NETWORK SERVICE".to_string(),
        _ => {
            let account = account.trim();
            match account.strip_prefix(r".\") {
                // Qualified with the computer name, so that the lookup does
                // not query the domain (which may be unreachable)
                Some(user) => match std::env::var("COMPUTERNAME") {
                    Ok(computer) if !computer.is_empty() => format!(r"{}\{}", computer, user),
                    _ => user.to_string(),
                },
                None => account.to_string(),
            }
        }
    }
}

/// Binary SID of the built-in accounts, which have well-known identifiers
/// and need no lookup at all: S-1-5-18 (LocalSystem), S-1-5-19
/// (LocalService) and S-1-5-20 (NetworkService). Looking them up by name
/// may involve the domain, failing when it cannot be reached. `account` is
/// None for LocalSystem.
pub fn builtin_sid(account: Option<&str>) -> Option<Vec<u8>> {
    let rid: u32 = match account.map(AccountKind::classify) {
        None | Some(AccountKind::LocalSystem) => 18,
        Some(AccountKind::LocalService) => 19,
        Some(AccountKind::NetworkService) => 20,
        _ => return None,
    };
    // Revision 1, one sub-authority, NT authority (5, big endian), then the
    // sub-authority (little endian)
    let mut sid = vec![1, 1, 0, 0, 0, 0, 0, 5];
    sid.extend_from_slice(&rid.to_le_bytes());
    Some(sid)
}

/// Password to hand to ChangeServiceConfig when switching to `account`:
/// empty for the built-in accounts, NULL (None) for virtual accounts and
/// gMSA, as the documentation requires, the given one for users.
pub fn change_config_password(account: &str, password: Option<String>) -> Option<String> {
    match AccountKind::classify(account) {
        AccountKind::LocalSystem | AccountKind::LocalService | AccountKind::NetworkService => {
            Some(String::new())
        }
        AccountKind::Virtual | AccountKind::ManagedService => None,
        AccountKind::User => password,
    }
}

/// Validates an account/password pair, returning the password to hand to
/// the SCM. Passwords given for accounts that have none are dropped.
pub fn resolve_password(account: &str, password: Option<String>) -> Result<Option<String>, String> {
    let kind = AccountKind::classify(account);
    match (kind.requires_password(), password) {
        (true, None) => Err(format!(
            "--account-password is required for the user account '{}'",
            account
        )),
        (true, password) => Ok(password),
        (false, _) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_builtin_accounts() {
        use AccountKind::*;
        let cases = [
            ("LocalSystem", LocalSystem),
            (r".\LocalSystem", LocalSystem),
            (r"NT AUTHORITY\SYSTEM", LocalSystem),
            ("LocalService", LocalService),
            (r"NT AUTHORITY\LocalService", LocalService),
            (r"nt authority\local service", LocalService),
            ("NetworkService", NetworkService),
            (r"NT AUTHORITY\NetworkService", NetworkService),
            (r"NT AUTHORITY\Network Service", NetworkService),
            (r"NT SERVICE\Redmine", Virtual),
            (r"CONTOSO\svc-redmine$", ManagedService),
            ("svc-redmine$", ManagedService),
            (r".\marco", User),
            (r"CONTOSO\marco", User),
            ("marco", User),
            (r"NT SERVICE\", User),
            ("$", User),
        ];
        for (account, expected) in cases {
            assert_eq!(AccountKind::classify(account), expected, "{account}");
        }
    }

    #[test]
    fn only_users_need_a_password() {
        assert!(AccountKind::User.requires_password());
        assert!(!AccountKind::NetworkService.requires_password());
        assert!(!AccountKind::ManagedService.requires_password());
        assert!(!AccountKind::Virtual.requires_password());
    }

    #[test]
    fn logon_right_is_needed_by_users_and_gmsa_only() {
        assert!(!AccountKind::User.has_implicit_logon_right());
        assert!(!AccountKind::ManagedService.has_implicit_logon_right());
        assert!(AccountKind::NetworkService.has_implicit_logon_right());
        assert!(AccountKind::Virtual.has_implicit_logon_right());
    }

    #[test]
    fn scm_account_names() {
        assert_eq!(scm_account_name("LocalSystem"), None);
        assert_eq!(
            scm_account_name("NetworkService").as_deref(),
            Some(r"NT AUTHORITY\NetworkService")
        );
        assert_eq!(
            scm_account_name(r"NT AUTHORITY\Local Service").as_deref(),
            Some(r"NT AUTHORITY\LocalService")
        );
        assert_eq!(
            scm_account_name(r"NT SERVICE\Redmine").as_deref(),
            Some(r"NT SERVICE\Redmine")
        );
        assert_eq!(scm_account_name(r".\marco").as_deref(), Some(r".\marco"));
    }

    #[test]
    fn lookup_names() {
        assert_eq!(lookup_account_name(None), r"NT AUTHORITY\SYSTEM");
        assert_eq!(
            lookup_account_name(Some("LocalSystem")),
            r"NT AUTHORITY\SYSTEM"
        );
        assert_eq!(
            lookup_account_name(Some(r"NT AUTHORITY\NetworkService")),
            r"NT AUTHORITY\NETWORK SERVICE"
        );
        assert_eq!(
            lookup_account_name(Some("LocalService")),
            r"NT AUTHORITY\LOCAL SERVICE"
        );
        let local = lookup_account_name(Some(r".\marco"));
        assert!(local == "marco" || local.ends_with(r"\marco"), "{local}");
        assert_eq!(
            lookup_account_name(Some(r"NT SERVICE\Redmine")),
            r"NT SERVICE\Redmine"
        );
    }

    #[test]
    fn builtin_sids() {
        let sid = |rid: u8| vec![1, 1, 0, 0, 0, 0, 0, 5, rid, 0, 0, 0];
        assert_eq!(builtin_sid(None), Some(sid(18)));
        assert_eq!(builtin_sid(Some("LocalSystem")), Some(sid(18)));
        assert_eq!(builtin_sid(Some(r"NT AUTHORITY\SYSTEM")), Some(sid(18)));
        assert_eq!(builtin_sid(Some("LocalService")), Some(sid(19)));
        assert_eq!(
            builtin_sid(Some(r"NT AUTHORITY\NetworkService")),
            Some(sid(20))
        );
        assert_eq!(
            builtin_sid(Some(r"NT AUTHORITY\Network Service")),
            Some(sid(20))
        );
        assert_eq!(builtin_sid(Some(r"NT SERVICE\Redmine")), None);
        assert_eq!(builtin_sid(Some(r".\marco")), None);
    }

    #[test]
    fn change_config_passwords() {
        assert_eq!(
            change_config_password("NetworkService", None),
            Some(String::new())
        );
        assert_eq!(
            change_config_password("LocalSystem", None),
            Some(String::new())
        );
        assert_eq!(change_config_password(r"NT SERVICE\x", None), None);
        assert_eq!(change_config_password(r"CONTOSO\gmsa$", None), None);
        assert_eq!(
            change_config_password(r".\marco", Some("pw".into())),
            Some("pw".into())
        );
    }

    #[test]
    fn password_resolution() {
        assert!(resolve_password(r".\marco", None).is_err());
        assert_eq!(
            resolve_password(r".\marco", Some("pw".into())),
            Ok(Some("pw".into()))
        );
        assert_eq!(resolve_password("NetworkService", None), Ok(None));
        assert_eq!(
            resolve_password("NetworkService", Some("pw".into())),
            Ok(None)
        );
        assert_eq!(resolve_password(r"CONTOSO\gmsa$", None), Ok(None));
    }
}
