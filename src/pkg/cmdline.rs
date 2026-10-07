//! Windows command line quoting and splitting, following the rules of
//! `CommandLineToArgvW` and of the Microsoft C runtime.

/// Quotes an argument so that `CommandLineToArgvW` parses it back unchanged.
///
/// Arguments without spaces, tabs or quotes are returned as they are.
/// Otherwise the argument is wrapped in quotes, quotes are escaped with a
/// backslash and the backslashes preceding a quote (or the closing quote)
/// are doubled.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            c => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// Joins arguments into a command line.
pub fn join<'a>(args: impl IntoIterator<Item = &'a str>) -> String {
    args.into_iter()
        .map(quote_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits a command line into arguments like `CommandLineToArgvW`.
// Only used by the tests until the ImagePath of legacy services is parsed
#[allow(dead_code)]
///
/// The first argument (the program name) follows simpler rules: it extends
/// up to the next whitespace, or up to the closing quote if it starts with
/// one, and backslashes are never special.
pub fn split(cmdline: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = cmdline.chars().peekable();

    // Program name
    let mut program = String::new();
    if chars.peek() == Some(&'"') {
        chars.next();
        for c in chars.by_ref() {
            if c == '"' {
                break;
            }
            program.push(c);
        }
    } else {
        while let Some(&c) = chars.peek() {
            if c == ' ' || c == '\t' {
                break;
            }
            program.push(c);
            chars.next();
        }
    }
    if !cmdline.is_empty() {
        args.push(program);
    }

    loop {
        while matches!(chars.peek(), Some(' ' | '\t')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        let mut arg = String::new();
        let mut in_quotes = false;
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' if !in_quotes => break,
                '\\' => {
                    let mut backslashes = 0;
                    while chars.peek() == Some(&'\\') {
                        chars.next();
                        backslashes += 1;
                    }
                    if chars.peek() == Some(&'"') {
                        arg.extend(std::iter::repeat_n('\\', backslashes / 2));
                        if backslashes % 2 == 1 {
                            arg.push('"');
                            chars.next();
                        }
                    } else {
                        arg.extend(std::iter::repeat_n('\\', backslashes));
                    }
                }
                '"' => {
                    chars.next();
                    if in_quotes && chars.peek() == Some(&'"') {
                        // "" inside quotes is a literal quote
                        arg.push('"');
                        chars.next();
                    } else {
                        in_quotes = !in_quotes;
                    }
                }
                c => {
                    arg.push(c);
                    chars.next();
                }
            }
        }
        args.push(arg);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_arguments_are_not_quoted() {
        assert_eq!(quote_arg("exec"), "exec");
        assert_eq!(
            quote_arg(r"C:\Redmine\ruby\bin\bundle"),
            r"C:\Redmine\ruby\bin\bundle"
        );
        assert_eq!(quote_arg("tcp://0.0.0.0:3000"), "tcp://0.0.0.0:3000");
    }

    #[test]
    fn quotes_spaces_quotes_and_trailing_backslashes() {
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(
            quote_arg(r"C:\Program Files\app"),
            r#""C:\Program Files\app""#
        );
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote_arg(r"C:\with space\"), r#""C:\with space\\""#);
        assert_eq!(quote_arg(r#"a\"b"#), r#""a\\\"b""#);
    }

    #[test]
    fn split_follows_command_line_to_argv_rules() {
        assert_eq!(
            split(r#""C:\Program Files\wsw\wsw.exe" run --name "my app""#),
            vec![r"C:\Program Files\wsw\wsw.exe", "run", "--name", "my app"]
        );
        assert_eq!(
            split(r#"C:\wsw.exe a\\\"b "c\\" d\e"#),
            vec![r"C:\wsw.exe", r#"a\"b"#, r"c\", r"d\e"]
        );
        assert_eq!(split(r#"p "" x"#), vec!["p", "", "x"]);
        assert_eq!(split(r#"p "a""b""#), vec!["p", r#"a"b"#]);
        assert_eq!(split("p   a\tb  "), vec!["p", "a", "b"]);
        assert!(split("").is_empty());
    }

    #[test]
    fn quoting_round_trips() {
        let args = [
            r"C:\Redmine\ruby\bin\ruby.exe",
            r"C:\Redmine\ruby\bin\bundle",
            "exec",
            "puma",
            "-b",
            "tcp://0.0.0.0:3000",
            "",
            "with space",
            r"trailing\",
            r"trailing space\ ",
            r#"quote " inside"#,
            r#"\"already\" escaped"#,
            r"\\server\share\",
            "%PATH%",
            "tab\there",
        ];
        assert_eq!(split(&join(args)), args);
    }
}
