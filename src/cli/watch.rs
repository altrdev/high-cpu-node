//! `torchio watch`: what a server is doing, in the terminal.
use super::info::{self, Info};
use super::view::{History, Style, render};
use super::{USAGE, http};
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

struct Options {
    url: String,
    interval: String,
    once: bool,
    help: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options { url: format!("http://127.0.0.1:{}", http::port()), interval: "1".into(), once: false, help: false };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |what: &str| inline.clone().or_else(|| args.next().cloned()).ok_or(format!("Option '{what}' argument missing"));
        match name {
            "--url" => options.url = value("--url <url>")?,
            "--interval" => options.interval = value("--interval <seconds>")?,
            "--once" => options.once = true,
            "--help" | "-h" => options.help = true,
            other => return Err(format!("Unknown option '{other}'")),
        }
    }
    Ok(options)
}

/// The time between samples: the seconds asked for, 1 if they are not a number, at least 0.2
fn interval(text: &str) -> Duration {
    let seconds = text.trim().parse::<f64>().ok().filter(|n| *n != 0.0 && n.is_finite()).unwrap_or(1.0);
    Duration::from_secs_f64(seconds.max(0.2))
}

/// Restores the terminal when Ctrl+C or SIGTERM ends the view: show the cursor, leave the alternate screen
extern "C" fn restore_terminal_and_exit(_: libc::c_int) {
    const RESTORE: &[u8] = b"\x1b[?25h\x1b[?1049l";
    // SAFETY: write and _exit are async-signal-safe, and RESTORE is a valid buffer
    unsafe {
        libc::write(1, RESTORE.as_ptr().cast(), RESTORE.len());
        libc::_exit(0);
    }
}

/// One frame: the view of a sample, or why there is none
fn frame(style: &Style, url: &str, history: &mut History) -> Result<String, String> {
    let info: Info = info::fetch(url)?;
    history.record(&info);
    Ok(render(style, url, &info, history))
}

pub fn run(args: &[String]) -> ExitCode {
    let options = match parse_options(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            println!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if options.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let stdout = std::io::stdout();
    let style = Style::new(stdout.is_terminal() && std::env::var("NO_COLOR").map_or(true, |value| value.is_empty()));
    let mut history = History::default();

    if options.once {
        return match frame(&style, &options.url, &mut history) {
            Ok(view) => {
                println!("{view}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("Cannot reach {}: {message}", options.url);
                ExitCode::FAILURE
            }
        };
    }

    let screen = stdout.is_terminal();
    if screen {
        print!("\x1b[?1049h\x1b[?25l"); // alternate screen, hide cursor
        // SAFETY: the handler only calls async-signal-safe functions
        unsafe {
            libc::signal(libc::SIGINT, restore_terminal_and_exit as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, restore_terminal_and_exit as *const () as libc::sighandler_t);
        }
    }
    let interval = interval(&options.interval);
    let mut next = Instant::now();
    loop {
        let view = frame(&style, &options.url, &mut history).unwrap_or_else(|message| format!("Cannot reach {}: {message}", options.url));
        let text = if screen { format!("\x1b[H\x1b[J{view}\n\nCtrl+C to quit\n") } else { format!("\n{view}\n") };
        let mut out = stdout.lock();
        if out.write_all(text.as_bytes()).and_then(|_| out.flush()).is_err() {
            return ExitCode::SUCCESS; // the reader went away
        }
        drop(out);
        next += interval;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_may_be_given_with_a_space_or_an_equals() {
        let options = parse_options(&args(&["--url", "http://a:1", "--interval=2", "--once"])).unwrap();
        assert_eq!((options.url.as_str(), options.interval.as_str(), options.once, options.help), ("http://a:1", "2", true, false));
        let options = parse_options(&args(&["--url=http://b:2"])).unwrap();
        assert_eq!(options.url, "http://b:2");
    }

    #[test]
    fn help_can_be_asked_in_two_ways() {
        assert!(parse_options(&args(&["-h"])).unwrap().help);
        assert!(parse_options(&args(&["--help"])).unwrap().help);
    }

    #[test]
    fn what_is_not_an_option_is_an_error() {
        assert_eq!(parse_options(&args(&["--nope"])).err().as_deref(), Some("Unknown option '--nope'"));
        assert!(parse_options(&args(&["--url"])).is_err());
        assert!(parse_options(&args(&["--interval"])).is_err());
    }

    #[test]
    fn the_interval_is_at_least_a_fifth_of_a_second_and_one_when_it_is_not_a_number() {
        assert_eq!(interval("2"), Duration::from_secs(2));
        assert_eq!(interval(" 0.5 "), Duration::from_millis(500));
        assert_eq!(interval("0.01"), Duration::from_millis(200));
        for text in ["abc", "", "0", "inf", "NaN"] {
            assert_eq!(interval(text), Duration::from_secs(1), "{text:?}");
        }
    }
}
