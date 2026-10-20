//! The commands that send or read a body, which the ones of `action` (a GET with numbers) can't do:
//! `torchio scenario <file>` and `torchio payload get|post`. They take `--url` and `--token` like the
//! others and speak plain HTTP only.
use super::action::answer;
use super::{USAGE, http};
use serde_json::Value;
use std::io::Read;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const MB: f64 = 1024.0 * 1024.0;

struct Options {
    url: String,
    token: Option<String>,
    wait: bool,
    help: bool,
    words: Vec<String>,
}

/// What follows the name of the command: words, and the options of every command (`--url`, `--token`,
/// and `--wait` for a scenario)
fn parse(args: &[String], env_token: Option<String>, allow_wait: bool) -> Result<Options, String> {
    let mut options =
        Options { url: format!("http://127.0.0.1:{}", http::port()), token: env_token, wait: false, help: false, words: Vec::new() };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |what: &str| inline.clone().or_else(|| args.next().cloned()).ok_or(format!("Option '{what}' argument missing"));
        match name {
            "-h" | "--help" => options.help = true,
            "--url" => options.url = value("--url <url>")?,
            "--token" => options.token = Some(value("--token <token>")?),
            "--wait" if allow_wait => options.wait = true,
            // `-` is stdin, and the only word that starts with a dash
            _ if name.starts_with('-') && name != "-" => return Err(format!("Unknown option '{name}'")),
            _ => options.words.push(arg.clone()),
        }
    }
    Ok(options)
}

fn env_token() -> Option<String> {
    std::env::var("TOKEN").ok().filter(|token| !token.is_empty())
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// The text of a JSON answer written out, or the text as it came
fn pretty(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| body.trim_end().to_string())
}

/// The scenario in `source`: a file, or `-` for what is piped in
fn read_scenario(source: &str) -> Result<String, String> {
    if source == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| format!("Cannot read the scenario from stdin: {e}"))?;
        return Ok(text);
    }
    std::fs::read_to_string(source).map_err(|e| format!("Cannot read {source}: {e}"))
}

/// `torchio scenario <file|-> [--wait]`, `torchio scenario status`, `torchio scenario stop`
pub fn scenario(args: &[String]) -> ExitCode {
    let options = match parse(args, env_token(), true) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if options.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let [word] = options.words.as_slice() else {
        return fail("Usage: torchio scenario <file|-> [--wait] | status | stop");
    };
    let url = match http::endpoint_url(&options.url, "/scenario") {
        Ok(url) => url,
        Err(message) => return fail(&message),
    };
    let token = options.token.as_deref();
    let timeout = Duration::from_secs(30);

    let result = match word.as_str() {
        "status" => http::get_with_token(&url, token, timeout),
        "stop" => http::send("DELETE", &url, token, "", timeout),
        source => match read_scenario(source) {
            Ok(body) => http::send("POST", &url, token, &body, timeout),
            Err(message) => return fail(&message),
        },
    };
    match answer(result, &options.url) {
        Ok(body) => {
            println!("{}", pretty(&body));
            if options.wait && !matches!(word.as_str(), "status" | "stop") {
                return wait(&url, token);
            }
            ExitCode::SUCCESS
        }
        Err(message) => fail(&message),
    }
}

/// Follows a scenario that was just started until it is over: exit 0 if it finished, 1 otherwise, so
/// a pipeline can run it and go on
fn wait(url: &str, token: Option<&str>) -> ExitCode {
    let mut seen = 0;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let state = match http::get_with_token(url, token, Duration::from_secs(10)) {
            Ok((200, body)) => serde_json::from_str::<Value>(&body).unwrap_or(Value::Null),
            Ok((status, body)) => return fail(&format!("HTTP {status}: {}", body.trim_end())),
            // the server is frozen by a block, or restarting: keep asking
            Err(_) => continue,
        };
        let step = state["step"].as_u64().unwrap_or(0);
        match state["state"].as_str() {
            Some("running") => {
                if step != seen {
                    seen = step;
                    eprintln!("step {step} of {}", state["steps"]);
                }
            }
            Some("finished") => {
                eprintln!("The scenario finished");
                return ExitCode::SUCCESS;
            }
            Some("failed") => return fail(&format!("The scenario failed: {}", state["error"].as_str().unwrap_or("no reason given"))),
            Some(other) => return fail(&format!("The scenario ended: {other}")),
            None => return fail("The server did not answer with a scenario"),
        }
    }
}

/// `torchio payload get [mb]` and `torchio payload post <file|->`
pub fn payload(args: &[String]) -> ExitCode {
    let options = match parse(args, env_token(), false) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if options.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let url = match http::endpoint_url(&options.url, "/payload") {
        Ok(url) => url,
        Err(message) => return fail(&message),
    };
    let token = options.token.as_deref();
    let result = match options.words.as_slice() {
        [word, rest @ ..] if word == "get" && rest.len() <= 1 => download(&url, rest.first().map(String::as_str), token),
        [word, source] if word == "post" => upload(&url, source, token),
        _ => return fail("Usage: torchio payload get [mb] | post <file|->"),
    };
    match result {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(message) => fail(&message),
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(3600))).http_status_as_error(false).build().into()
}

fn authorization(token: Option<&str>) -> String {
    token.map(|token| format!("Bearer {token}")).unwrap_or_default()
}

/// Asks for `mb` MB and throws them away as they come, counting. A body shorter than the server
/// said (cut by a proxy, a timeout) is an error.
fn download(url: &str, mb: Option<&str>, token: Option<&str>) -> Result<String, String> {
    let target = match mb {
        Some(mb) => format!("{url}?{}", serde_urlencoded::to_string([("mb", mb)]).map_err(|e| e.to_string())?),
        None => url.to_string(),
    };
    let mut request = agent().get(&target);
    if token.is_some() {
        request = request.header("Authorization", authorization(token));
    }
    let started = Instant::now();
    let mut response = request.call().map_err(|e| format!("Cannot reach the server: {e}"))?;
    let status = response.status().as_u16();
    if status >= 400 {
        let body = response.body_mut().read_to_string().unwrap_or_default();
        return answer(Ok((status, body)), url).map(|_| String::new());
    }
    let announced: Option<u64> =
        response.headers().get("content-length").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut received: u64 = 0;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => received += read as u64,
            Err(e) => return Err(format!("The body was cut short after {received} bytes: {e}")),
        }
    }
    if let Some(announced) = announced.filter(|announced| *announced != received) {
        return Err(format!("The body was cut short: {received} of {announced} bytes"));
    }
    let seconds = started.elapsed().as_secs_f64();
    Ok(format!("Received {:.1} MB in {seconds:.2} s ({:.1} MB/s)", received as f64 / MB, speed(received, seconds)))
}

fn speed(bytes: u64, seconds: f64) -> f64 {
    if seconds > 0.0 { bytes as f64 / MB / seconds } else { 0.0 }
}

/// Sends a file, or what is piped in, and says what the server counted. A server that answers before it
/// has read everything (the limit of the size) closes the connection: that is said, not hidden.
fn upload(url: &str, source: &str, token: Option<&str>) -> Result<String, String> {
    let mut request = agent().post(url);
    if token.is_some() {
        request = request.header("Authorization", authorization(token));
    }
    let started = Instant::now();
    let sent = if source == "-" {
        request.send(std::io::stdin())
    } else {
        let file = std::fs::File::open(source).map_err(|e| format!("Cannot read {source}: {e}"))?;
        request.send(file)
    };
    let mut response = sent.map_err(|e| {
        format!("The connection was closed while the body was being sent: {e}. A limit on the size of the body, here or on the way?")
    })?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let body = answer(Ok((status, body)), url)?;
    let seconds = started.elapsed().as_secs_f64();
    let report: Value = serde_json::from_str(&body).map_err(|_| format!("Not the answer of /payload: {body}"))?;
    let bytes = report["bytes"].as_u64().unwrap_or(0);
    Ok(format!(
        "Sent {:.1} MB in {seconds:.2} s ({:.1} MB/s), the server counted {bytes} bytes in {} s",
        bytes as f64 / MB,
        speed(bytes, seconds),
        report["seconds"]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn words_and_options_are_told_apart() {
        let options = parse(&args(&["file.json", "--url", "http://h:1", "--wait", "--token=t"]), None, true).unwrap();
        assert_eq!(
            (options.words, options.url, options.wait, options.token),
            (vec!["file.json".to_string()], "http://h:1".into(), true, Some("t".into()))
        );
        assert_eq!(parse(&args(&["-"]), None, true).unwrap().words, ["-"], "a dash is stdin, not an option");
        assert_eq!(parse(&args(&["--wait"]), None, false).err().as_deref(), Some("Unknown option '--wait'"));
        assert_eq!(parse(&args(&["--nope"]), None, true).err().as_deref(), Some("Unknown option '--nope'"));
        assert_eq!(parse(&args(&["--url"]), None, true).err().as_deref(), Some("Option '--url <url>' argument missing"));
        assert!(parse(&args(&["-h"]), None, true).unwrap().help);
    }

    #[test]
    fn a_json_answer_is_written_out() {
        assert_eq!(pretty(r#"{"state":"idle"}"#), "{\n  \"state\": \"idle\"\n}");
        assert_eq!(pretty("No scenario is running\n"), "No scenario is running");
    }

    #[test]
    fn the_speed_is_zero_when_no_time_went_by() {
        assert_eq!(speed(10 * 1024 * 1024, 0.0), 0.0);
        assert_eq!(speed(10 * 1024 * 1024, 2.0), 5.0);
    }
}
