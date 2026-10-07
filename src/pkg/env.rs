//! Environment variables of the wrapped process (`--env`, `--env-file`).
//!
//! Variables are persisted unexpanded with the rest of the service
//! configuration and resolved when the service starts: values may reference
//! other variables with the `%VAR%` syntax, like WinSW does, e.g.
//! `PATH=C:\Ruby\bin;%PATH%`. They are applied in order, env file first, so a
//! value can reference a variable defined before it. As on Windows, names
//! are case insensitive.
//!
//! The SCM native `Environment` registry value is deliberately not used: the
//! SCM does not expand it, so `PATH=...;%PATH%` would replace the system PATH
//! instead of extending it.

/// Parses a `KEY=VALUE` assignment. The value may contain `=` and be empty.
pub fn parse_assignment(entry: &str) -> Result<(String, String), String> {
    let Some((key, value)) = entry.split_once('=') else {
        return Err(format!(
            "invalid environment variable '{entry}': expected KEY=VALUE"
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(format!(
            "invalid environment variable '{entry}': empty name"
        ));
    }
    Ok((key.to_string(), value.to_string()))
}

/// Parses the content of an env file: one `KEY=VALUE` per line, empty lines
/// and lines starting with `#` are ignored, values may be wrapped in single
/// or double quotes.
pub fn parse_env_file(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut vars = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) =
            parse_assignment(line).map_err(|e| format!("line {}: {}", index + 1, e))?;
        vars.push((key, unquote(value.trim()).to_string()));
    }
    Ok(vars)
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

/// Expands `%NAME%` references like `ExpandEnvironmentStrings` does:
/// undefined variables and lone `%` are left untouched.
pub fn expand(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(expanded) => {
                        result.push_str(&expanded);
                        rest = &after[end + 1..];
                    }
                    None => {
                        // Keep the reference, the closing % may open another one
                        result.push('%');
                        result.push_str(name);
                        rest = &after[end..];
                    }
                }
            }
            _ => {
                result.push('%');
                rest = after;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Resolves `vars` in order on top of the `base` environment, expanding each
/// value against the variables defined so far. Returns the variables to set
/// on the child process.
pub fn resolve(base: &[(String, String)], vars: &[(String, String)]) -> Vec<(String, String)> {
    let mut current: Vec<(String, String)> = base.to_vec();
    let mut resolved: Vec<(String, String)> = Vec::new();
    for (key, value) in vars {
        let expanded = expand(value, |name| {
            current
                .iter()
                .rev()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        });
        current.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        current.push((key.clone(), expanded.clone()));
        resolved.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        resolved.push((key.clone(), expanded));
    }
    resolved
}

/// Loads the variables of the wrapped process: the env file first, then the
/// single `--env` entries, resolved against the current process environment.
pub fn load(env_file: Option<&str>, entries: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut vars = Vec::new();
    if let Some(path) = env_file {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read env file '{path}': {e}"))?;
        vars.extend(parse_env_file(&text).map_err(|e| format!("env file '{path}', {e}"))?);
    }
    for entry in entries {
        vars.push(parse_assignment(entry)?);
    }
    let base: Vec<(String, String)> = std::env::vars().collect();
    Ok(resolve(&base, &vars))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_assignments() {
        assert_eq!(
            parse_assignment("RAILS_ENV=production"),
            Ok(("RAILS_ENV".into(), "production".into()))
        );
        assert_eq!(
            parse_assignment("OPTS=-a=1 -b=2"),
            Ok(("OPTS".into(), "-a=1 -b=2".into()))
        );
        assert_eq!(parse_assignment("EMPTY="), Ok(("EMPTY".into(), "".into())));
        assert!(parse_assignment("NOVALUE").is_err());
        assert!(parse_assignment("=value").is_err());
    }

    #[test]
    fn parses_env_files() {
        let text = "# comment\n\nRAILS_ENV=production\r\n  PORT = 3000\nQUOTED=\"a b\"\nSINGLE='x=1'\n#OFF=1\n";
        assert_eq!(
            parse_env_file(text),
            Ok(pairs(&[
                ("RAILS_ENV", "production"),
                ("PORT", "3000"),
                ("QUOTED", "a b"),
                ("SINGLE", "x=1"),
            ]))
        );
    }

    #[test]
    fn env_file_errors_report_the_line() {
        let err = parse_env_file("A=1\nbroken\n").unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
    }

    #[test]
    fn expands_variables() {
        let lookup = |name: &str| match name.to_ascii_uppercase().as_str() {
            "PATH" => Some(r"C:\Windows".to_string()),
            "HOME" => Some(r"C:\Users\x".to_string()),
            _ => None,
        };
        assert_eq!(
            expand(r"C:\Ruby\bin;%PATH%", lookup),
            r"C:\Ruby\bin;C:\Windows"
        );
        assert_eq!(expand("%home%-%Path%", lookup), r"C:\Users\x-C:\Windows");
        assert_eq!(expand("%UNDEFINED%", lookup), "%UNDEFINED%");
        assert_eq!(expand("100% sure", lookup), "100% sure");
        assert_eq!(expand("%%", lookup), "%%");
        assert_eq!(expand("50%%PATH%", lookup), r"50%C:\Windows");
        assert_eq!(expand("%UNDEFINED%PATH%", lookup), r"%UNDEFINEDC:\Windows");
        assert_eq!(expand("trailing %", lookup), "trailing %");
    }

    #[test]
    fn resolves_in_order_against_the_base_environment() {
        let base = pairs(&[("Path", r"C:\Windows"), ("OTHER", "x")]);
        let vars = pairs(&[
            ("RUBY", r"C:\Ruby"),
            ("PATH", r"%RUBY%\bin;%PATH%"),
            ("RAILS_ENV", "production"),
            ("ruby", r"D:\Ruby"),
        ]);
        assert_eq!(
            resolve(&base, &vars),
            pairs(&[
                ("PATH", r"C:\Ruby\bin;C:\Windows"),
                ("RAILS_ENV", "production"),
                ("ruby", r"D:\Ruby"),
            ])
        );
    }

    #[test]
    fn self_reference_without_base_is_kept() {
        let vars = pairs(&[("NEW", "a;%NEW%")]);
        assert_eq!(resolve(&[], &vars), pairs(&[("NEW", "a;%NEW%")]));
    }
}
