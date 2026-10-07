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
