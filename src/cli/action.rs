//! The commands that start a test on a running server, one for each endpoint that changes its
//! state: `torchio cpu 80 --hold 60`, `torchio memory 512`, `torchio ready off 30`…
//!
//! The first values after the name are the parameters of the endpoint in the order of `/help`, and
//! any of them can also be given as `--name value`. The limits are the server's: a value it does not
//! accept comes back as its `400`.
use super::{USAGE, http};
use std::process::ExitCode;
use std::time::Duration;

/// A command, the endpoint it calls and the parameters it takes, in order
struct Action {
    name: &'static str,
    about: &'static str,
    path: &'static str,
    params: &'static [&'static str],
}

const ACTIONS: &[Action] = &[
    Action { name: "stress", about: "Keep a core busy with a calculation", path: "/stress", params: &["number"] },
    Action { name: "cpu", about: "Keep every worker busy for a share of the time", path: "/cpu", params: &["percent", "hold", "workers"] },
    Action { name: "delay", about: "Make the server answer after a delay", path: "/delay", params: &["ms"] },
    Action { name: "status", about: "Make the server answer with an HTTP status", path: "/status/{code}", params: &["code"] },
    Action { name: "flaky", about: "Make the server fail a share of the requests", path: "/flaky", params: &["rate", "code", "ms"] },
    Action { name: "memory", about: "Allocate memory and hold it", path: "/memory", params: &["mb", "hold"] },
    Action { name: "leak", about: "Grow memory by itself up to a ceiling", path: "/leak", params: &["rate", "mb", "hold"] },
    Action { name: "disk", about: "Write a file and keep it", path: "/disk", params: &["mb", "hold"] },
    Action { name: "fd", about: "Open file descriptors and hold them", path: "/fd", params: &["count", "hold"] },
    Action {
        name: "connections",
        about: "Open TCP connections to the server and hold them",
        path: "/connections",
        params: &["count", "hold"],
    },
    Action { name: "log", about: "Write lines to the log at a rate", path: "/log", params: &["lines", "rate"] },
    Action { name: "crash", about: "End the server process", path: "/crash", params: &["code", "after"] },
    // `ready on` and `ready off [seconds]`: the word picks the endpoint
    Action {
        name: "ready",
        about: "Make the readiness probe fail (off) or pass (on)",
        path: "/ready/{state}",
        params: &["state", "seconds"],
    },
];

/// What the shell completions say about a command
pub struct Command {
    pub name: &'static str,
    pub about: &'static str,
    /// The parameters that can be given by name, as `--hold`
    pub params: Vec<&'static str>,
}

/// The commands that start a test, for the completions
pub fn commands() -> Vec<Command> {
    // the word after `ready` is positional, it is not an option
    ACTIONS
        .iter()
        .map(|action| Command {
            name: action.name,
            about: action.about,
            params: action.params.iter().copied().filter(|param| *param != "state").collect(),
        })
        .collect()
}

/// The names of the commands that start a test
pub fn names() -> Vec<&'static str> {
    ACTIONS.iter().map(|action| action.name).collect()
}

/// How a command is written, like `cpu [percent] [hold]`
pub fn usage(name: &str) -> Option<String> {
    let action = ACTIONS.iter().find(|action| action.name == name)?;
    Some(match name {
        "ready" => "ready on | off [seconds]".to_string(),
        "status" => "status <code>".to_string(),
        _ => action.params.iter().fold(name.to_string(), |text, param| format!("{text} [{param}]")),
    })
}

pub fn is_action(name: &str) -> bool {
    ACTIONS.iter().any(|action| action.name == name)
}

struct Request {
    url: String,
    token: Option<String>,
    values: Vec<(&'static str, String)>,
    help: bool,
}

fn parse(action: &Action, args: &[String], env_token: Option<String>) -> Result<Request, String> {
    let mut request = Request { url: format!("http://127.0.0.1:{}", http::port()), token: env_token, values: Vec::new(), help: false };
    let mut positional = action.params.iter();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        if arg == "-h" || arg == "--help" {
            request.help = true;
            continue;
        }
        let Some(option) = name.strip_prefix("--") else {
            let param = positional.next().ok_or(format!("Unexpected argument '{arg}'"))?;
            set(&mut request.values, param, arg.clone())?;
            continue;
        };
        let mut value = |what: &str| inline.clone().or_else(|| args.next().cloned()).ok_or(format!("Option '{what}' argument missing"));
        match option {
            "url" => request.url = value("--url <url>")?,
            "token" => request.token = Some(value("--token <token>")?),
            other => {
                let param = action.params.iter().find(|param| **param == other).ok_or(format!("Unknown option '{name}'"))?;
                set(&mut request.values, param, value(&format!("--{other} <value>"))?)?;
            }
        }
    }
    Ok(request)
}

fn set(values: &mut Vec<(&'static str, String)>, param: &&'static str, value: String) -> Result<(), String> {
    if values.iter().any(|(name, _)| name == param) {
        return Err(format!("'{param}' given twice"));
    }
    values.push((param, value));
    Ok(())
}

/// The path and the query string of the request: parameters that are part of the path go in it
fn target(action: &Action, values: &[(&'static str, String)]) -> Result<String, String> {
    let get = |name: &str| values.iter().find(|(key, _)| *key == name).map(|(_, value)| value.as_str());
    if action.name == "ready" {
        let state = get("state").ok_or("Usage: torchio ready on | off [seconds]")?;
        return match (state, get("seconds")) {
            ("on", None) => Ok("/ready/on".into()),
            ("off", seconds) => Ok(match seconds {
                Some(seconds) => format!("/ready/off?{}", serde_urlencoded::to_string([("seconds", seconds)]).map_err(|e| e.to_string())?),
                None => "/ready/off".into(),
            }),
            _ => Err("Usage: torchio ready on | off [seconds]".into()),
        };
    }
    let mut path = action.path.to_string();
    let mut query = Vec::new();
    for (name, value) in values {
        let placeholder = format!("{{{name}}}");
        if path.contains(&placeholder) {
            path = path.replace(&placeholder, &serde_urlencoded::to_string([("", value.as_str())]).map_err(|e| e.to_string())?[1..]);
        } else {
            query.push((*name, value.as_str()));
        }
    }
    if path.contains('{') {
        return Err(format!("Usage: torchio {} <{}>", action.name, action.params[0]));
    }
    if query.is_empty() {
        return Ok(path);
    }
    Ok(format!("{path}?{}", serde_urlencoded::to_string(query).map_err(|e| e.to_string())?))
}

/// How long to wait for the answer: `/stress` and `/delay` answer when they are done
fn timeout(action: &Action) -> Duration {
    match action.name {
        "stress" | "delay" | "flaky" => Duration::from_secs(600),
        _ => Duration::from_secs(30),
    }
}

/// Runs the command in `line` (an action and its values, as typed after `torchio`) against the
/// server at `url` and returns what the server answered, or why it did not. For the interactive view.
pub fn perform(line: &str, url: &str, token: Option<&str>) -> Result<String, String> {
    let mut words = line.split_whitespace();
    let name = words.next().ok_or("Type a command, like cpu 80")?;
    let action = ACTIONS.iter().find(|action| action.name == name).ok_or(format!("Unknown command '{name}'"))?;
    let mut args: Vec<String> = words.map(String::from).collect();
    args.extend(["--url".into(), url.into()]);
    let request = parse(action, &args, token.map(String::from))?;
    let target = target(action, &request.values)?;
    answer(http::get_with_token(&http::endpoint_url(&request.url, &target)?, request.token.as_deref(), timeout(action)), &request.url)
}

/// What the server answered as a message: its body, or what went wrong
fn answer(result: Result<(u16, String), String>, url: &str) -> Result<String, String> {
    match result {
        Ok((status, body)) if status < 400 => Ok(body.trim_end().to_string()),
        Ok((401, _)) => Err("The server asks for a token: use --token <token> or TOKEN".into()),
        Ok((status, body)) => Err(format!("HTTP {status}: {}", body.trim_end())),
        Err(message) => Err(format!("Cannot reach {url}: {message}")),
    }
}

pub fn run(name: &str, args: &[String]) -> ExitCode {
    let Some(action) = ACTIONS.iter().find(|action| action.name == name) else { return ExitCode::FAILURE };
    let env_token = std::env::var("TOKEN").ok().filter(|token| !token.is_empty());
    let request = match parse(action, args, env_token) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            println!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if request.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let url = match target(action, &request.values).and_then(|target| http::endpoint_url(&request.url, &target)) {
        Ok(url) => url,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    match answer(http::get_with_token(&url, request.token.as_deref(), timeout(action)), &request.url) {
        Ok(body) => {
            println!("{body}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn action(name: &str) -> &'static Action {
        ACTIONS.iter().find(|action| action.name == name).unwrap()
    }

    fn url_of(name: &str, list: &[&str]) -> Result<String, String> {
        let request = parse(action(name), &args(list), None)?;
        target(action(name), &request.values)
    }

    #[test]
    fn the_first_values_are_the_parameters_in_order() {
        assert_eq!(url_of("cpu", &["80", "60"]).unwrap(), "/cpu?percent=80&hold=60");
        assert_eq!(url_of("memory", &["512"]).unwrap(), "/memory?mb=512");
        assert_eq!(url_of("memory", &[]).unwrap(), "/memory");
    }

    #[test]
    fn a_parameter_may_be_given_by_name() {
        assert_eq!(url_of("memory", &["--hold", "5", "--mb=100"]).unwrap(), "/memory?hold=5&mb=100");
        assert_eq!(url_of("leak", &["1", "--hold", "9"]).unwrap(), "/leak?rate=1&hold=9");
    }

    #[test]
    fn flaky_takes_the_rate_the_code_and_the_delay() {
        assert_eq!(url_of("flaky", &["30", "502"]).unwrap(), "/flaky?rate=30&code=502");
        assert_eq!(usage("flaky").unwrap(), "flaky [rate] [code] [ms]");
    }

    #[test]
    fn the_code_of_status_goes_in_the_path() {
        assert_eq!(url_of("status", &["503"]).unwrap(), "/status/503");
        assert!(url_of("status", &[]).is_err());
    }

    #[test]
    fn ready_is_on_or_off_with_seconds() {
        assert_eq!(url_of("ready", &["on"]).unwrap(), "/ready/on");
        assert_eq!(url_of("ready", &["off"]).unwrap(), "/ready/off");
        assert_eq!(url_of("ready", &["off", "30"]).unwrap(), "/ready/off?seconds=30");
        assert!(url_of("ready", &[]).is_err());
        assert!(url_of("ready", &["maybe"]).is_err());
        assert!(url_of("ready", &["on", "5"]).is_err());
    }

    #[test]
    fn values_are_escaped_and_mistakes_are_errors() {
        assert_eq!(url_of("delay", &["a b&c"]).unwrap(), "/delay?ms=a+b%26c");
        assert!(url_of("delay", &["1", "2"]).is_err());
        assert!(url_of("delay", &["--nope", "1"]).is_err());
        assert!(url_of("delay", &["1", "--ms", "2"]).is_err());
        assert!(url_of("delay", &["--ms"]).is_err());
    }

    #[test]
    fn url_and_token_are_options_and_the_token_may_come_from_the_environment() {
        let request = parse(action("cpu"), &args(&["--url", "http://h:1", "--token=t", "50"]), Some("env".into())).unwrap();
        assert_eq!((request.url.as_str(), request.token.as_deref()), ("http://h:1", Some("t")));
        let request = parse(action("cpu"), &args(&["50"]), Some("env".into())).unwrap();
        assert_eq!(request.token.as_deref(), Some("env"));
    }

    #[test]
    fn a_line_is_an_action_and_its_values() {
        assert_eq!(perform("", "http://h:1", None), Err("Type a command, like cpu 80".into()));
        assert_eq!(perform("nope 1", "http://h:1", None), Err("Unknown command 'nope'".into()));
        assert_eq!(perform("cpu 1 2 3 4", "http://h:1", None), Err("Unexpected argument '4'".into()));
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        assert!(perform("cpu 50", &url, None).unwrap_err().starts_with(&format!("Cannot reach {url}: ")));
    }

    #[test]
    fn usage_shows_the_parameters_of_a_command() {
        assert_eq!(usage("cpu").as_deref(), Some("cpu [percent] [hold] [workers]"));
        assert_eq!(usage("status").as_deref(), Some("status <code>"));
        assert_eq!(usage("ready").as_deref(), Some("ready on | off [seconds]"));
        assert_eq!(usage("watch"), None);
    }

    #[test]
    fn every_action_is_known() {
        for name in ["stress", "cpu", "delay", "status", "memory", "leak", "disk", "fd", "log", "crash", "ready"] {
            assert!(is_action(name), "{name}");
        }
        assert!(!is_action("watch"));
    }
}
