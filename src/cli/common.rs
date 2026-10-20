//! What the commands that talk to a server have in common when they read their arguments and end:
//! `--name=value` or `--name value`, the token of the environment, and how a wrong command and a
//! request for help are answered.
use super::USAGE;
use std::process::ExitCode;
use std::slice::Iter;

/// `--name=value` as the name and the value; any other argument as it is, with no value
pub fn split(arg: &str) -> (&str, Option<String>) {
    match arg.split_once('=') {
        Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
        _ => (arg, None),
    }
}

/// The value of the option `what` (written like `--url <url>`): the one after the `=`, or the next argument
pub fn value(inline: Option<String>, rest: &mut Iter<String>, what: &str) -> Result<String, String> {
    inline.or_else(|| rest.next().cloned()).ok_or(format!("Option '{what}' argument missing"))
}

/// `TOKEN`, when it is set and not empty
pub fn env_token() -> Option<String> {
    std::env::var("TOKEN").ok().filter(|token| !token.is_empty())
}

/// Says why a command failed, on the standard error, and ends with a failure
pub fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// A command that was written wrong: the reason, then how to write it, on the standard error
pub fn usage_error(message: &str) -> ExitCode {
    eprintln!("{message}");
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}

/// The usage that was asked for, on the standard output
pub fn help() -> ExitCode {
    println!("{USAGE}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_option_may_carry_its_value_after_an_equals() {
        assert_eq!(split("--url=http://h:1"), ("--url", Some("http://h:1".to_string())));
        assert_eq!(split("--url"), ("--url", None));
        assert_eq!(split("a=b"), ("a=b", None), "only an option is split");
    }

    #[test]
    fn the_value_is_the_inline_one_or_the_next_argument() {
        let rest = ["next".to_string()];
        assert_eq!(value(Some("inline".into()), &mut rest.iter(), "--x <v>"), Ok("inline".to_string()));
        assert_eq!(value(None, &mut rest.iter(), "--x <v>"), Ok("next".to_string()));
        assert_eq!(value(None, &mut [].iter(), "--x <v>"), Err("Option '--x <v>' argument missing".to_string()));
    }
}
