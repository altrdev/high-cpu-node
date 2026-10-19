//! The view of `torchio watch` on a terminal, with keys that start the tests.
//!
//! A key opens a command line with the name of a command already written (`c` writes `cpu `), you
//! add the values and press Enter. `:` opens an empty one, for any command. The request runs in a
//! thread, so the view keeps refreshing while `/stress` or `/delay` wait for their answer.
use super::action;
use super::info::{self, Info};
use super::view::{History, Style, render};
use std::io::Write;
use std::process::ExitCode;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// The key that starts each command
const KEYS: &[(u8, &str)] = &[
    (b'c', "cpu"),
    (b'm', "memory"),
    (b'l', "leak"),
    (b'd', "disk"),
    (b'f', "fd"),
    (b'g', "log"),
    (b's', "stress"),
    (b'r', "ready"),
    (b'x', "crash"),
];

const LEAVE_SCREEN: &[u8] = b"\x1b[?25h\x1b[?1049l";

/// What the keys do now: move around, or write a command
#[derive(Debug, PartialEq)]
enum Mode {
    Browse,
    Prompt(String),
}

/// What a key asks the program to do
#[derive(Debug, PartialEq)]
enum Effect {
    None,
    Quit,
    Run(String),
}

struct Input {
    mode: Mode,
}

impl Input {
    /// Handles what one read of the terminal returned. Escape sequences (arrows, function keys) are
    /// ignored; a lone Escape cancels the command line. The first effect wins.
    fn feed(&mut self, bytes: &[u8]) -> Effect {
        if bytes.first() == Some(&0x1b) && bytes.len() > 1 {
            return Effect::None;
        }
        for byte in bytes {
            match self.key(*byte) {
                Effect::None => {}
                effect => return effect,
            }
        }
        Effect::None
    }

    fn key(&mut self, byte: u8) -> Effect {
        match &mut self.mode {
            Mode::Browse => match byte {
                b'q' | 0x03 => return Effect::Quit,
                b':' => self.mode = Mode::Prompt(String::new()),
                other => {
                    if let Some((_, name)) = KEYS.iter().find(|(key, _)| *key == other) {
                        self.mode = Mode::Prompt(format!("{name} "));
                    }
                }
            },
            Mode::Prompt(line) => match byte {
                0x1b | 0x03 => self.mode = Mode::Browse,
                b'\r' | b'\n' => {
                    let line = line.trim().to_string();
                    self.mode = Mode::Browse;
                    if !line.is_empty() {
                        return Effect::Run(line);
                    }
                }
                0x7f | 0x08 => {
                    line.pop();
                }
                0x20..=0x7e => line.push(byte as char),
                _ => {}
            },
        }
        Effect::None
    }
}

/// The terminal as it was before the raw mode, for the signal handler too
static ORIGINAL: OnceLock<libc::termios> = OnceLock::new();

/// Raw mode on the terminal and the alternate screen, until it is dropped
struct Screen;

impl Screen {
    fn enter() -> Option<Screen> {
        // SAFETY: tcgetattr fills a termios we own; fd 0 is the standard input
        let original = unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut termios) != 0 {
                return None;
            }
            termios
        };
        let _ = ORIGINAL.set(original);
        let mut raw = original;
        // SAFETY: raw is a valid termios, and the handler only calls async-signal-safe functions
        unsafe {
            libc::cfmakeraw(&mut raw);
            if libc::tcsetattr(0, libc::TCSANOW, &raw) != 0 {
                return None;
            }
            libc::signal(libc::SIGTERM, restore_and_exit as *const () as libc::sighandler_t);
            libc::signal(libc::SIGHUP, restore_and_exit as *const () as libc::sighandler_t);
        }
        print!("\x1b[?1049h\x1b[?25l");
        let _ = std::io::stdout().flush();
        Some(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    if let Some(original) = ORIGINAL.get() {
        // SAFETY: original is the termios read at the start; write is given a valid buffer
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, original);
            libc::write(1, LEAVE_SCREEN.as_ptr().cast(), LEAVE_SCREEN.len());
        }
    }
}

extern "C" fn restore_and_exit(_: libc::c_int) {
    restore();
    // SAFETY: _exit is async-signal-safe
    unsafe { libc::_exit(0) }
}

/// The text of the bottom of the screen: the keys, then the command line or the last answer
fn footer(input: &Input, message: &str) -> String {
    let keys: Vec<String> = KEYS.iter().map(|(key, name)| format!("{} {name}", *key as char)).collect();
    let mut text = format!("{}  : command  q quit\n", keys.join("  "));
    match &input.mode {
        Mode::Prompt(line) => {
            let hint = action::usage(line.split_whitespace().next().unwrap_or("")).unwrap_or_default();
            text.push_str(&format!("> {line}_\n{hint}"));
        }
        Mode::Browse => text.push_str(message),
    }
    text
}

/// One line of an answer, short enough for a terminal
fn one_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    if line.chars().count() > 100 { format!("{}…", line.chars().take(99).collect::<String>()) } else { line.to_string() }
}

/// Waits up to `timeout` for the keyboard and returns what was typed
fn read_keys(timeout: Duration) -> Vec<u8> {
    let mut fd = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    // SAFETY: fd is one valid pollfd
    let ready = unsafe { libc::poll(&mut fd, 1, timeout.as_millis().min(i32::MAX as u128) as i32) };
    if ready <= 0 {
        return Vec::new();
    }
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is valid for its length
    let n = unsafe { libc::read(0, buffer.as_mut_ptr().cast(), buffer.len()) };
    if n <= 0 { vec![b'q'] } else { buffer[..n as usize].to_vec() } // the end of the input is a quit
}

fn draw(text: &str) -> bool {
    let body: String = text.lines().map(|line| format!("{line}\x1b[K\r\n")).collect();
    let mut out = std::io::stdout().lock();
    out.write_all(format!("\x1b[H{body}\x1b[J").as_bytes()).and_then(|_| out.flush()).is_ok()
}

fn sample(url: &str, history: &mut History, style: &Style) -> String {
    match info::fetch(url) {
        Ok(info) => view(style, url, &info, history),
        Err(message) => format!("Cannot reach {url}: {message}"),
    }
}

fn view(style: &Style, url: &str, info: &Info, history: &mut History) -> String {
    history.record(info);
    render(style, url, info, history)
}

/// Shows the server at `url` until the user quits, with `token` for the commands that need it
pub fn run(url: &str, token: Option<&str>, interval: Duration) -> ExitCode {
    let Some(_screen) = Screen::enter() else {
        eprintln!("torchio watch needs a terminal to take commands: use --read-only");
        return ExitCode::FAILURE;
    };

    let style = Style::new(std::env::var("NO_COLOR").map_or(true, |value| value.is_empty()));
    let mut history = History::default();
    let mut input = Input { mode: Mode::Browse };
    let mut message = String::new();
    let mut view = String::new();
    let mut next = Instant::now();
    let (sender, results): (_, Receiver<(String, Result<String, String>)>) = channel();

    loop {
        if Instant::now() >= next {
            view = sample(url, &mut history, &style);
            next = Instant::now() + interval;
        }
        while let Ok((line, result)) = results.try_recv() {
            message = match result {
                Ok(body) => format!("{line}: {}", one_line(&body)),
                Err(error) => format!("{line}: {}", one_line(&error)),
            };
        }
        if !draw(&format!("{view}\n\n{}", footer(&input, &message))) {
            return ExitCode::SUCCESS; // the screen went away
        }
        let keys = read_keys(next.saturating_duration_since(Instant::now()).min(Duration::from_millis(100)));
        match input.feed(&keys) {
            Effect::Quit => return ExitCode::SUCCESS,
            Effect::None => {}
            Effect::Run(line) => {
                message = format!("{line}: waiting for the answer…");
                let (sender, url, token) = (sender.clone(), url.to_string(), token.map(String::from));
                std::thread::spawn(move || {
                    let result = action::perform(&line, &url, token.as_deref());
                    let _ = sender.send((line, result));
                });
                next = Instant::now(); // show the effect without waiting for the next sample
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input { mode: Mode::Browse }
    }

    fn type_in(input: &mut Input, text: &str) -> Effect {
        input.feed(text.as_bytes())
    }

    #[test]
    fn a_key_opens_the_command_line_with_the_name_of_the_command() {
        let mut input = input();
        assert_eq!(type_in(&mut input, "c"), Effect::None);
        assert_eq!(input.mode, Mode::Prompt("cpu ".into()));
        assert_eq!(type_in(&mut input, "80 60\r"), Effect::Run("cpu 80 60".into()));
        assert_eq!(input.mode, Mode::Browse);
    }

    #[test]
    fn a_colon_opens_an_empty_command_line() {
        let mut input = input();
        type_in(&mut input, ":status 503\n");
        assert_eq!(input.mode, Mode::Browse);
        type_in(&mut input, ":");
        assert_eq!(input.mode, Mode::Prompt(String::new()));
    }

    #[test]
    fn q_and_ctrl_c_quit_only_outside_the_command_line() {
        assert_eq!(type_in(&mut input(), "q"), Effect::Quit);
        assert_eq!(type_in(&mut input(), "\x03"), Effect::Quit);
        let mut input = input();
        type_in(&mut input, ":q");
        assert_eq!(input.mode, Mode::Prompt("q".into()));
        assert_eq!(type_in(&mut input, "\x03"), Effect::None);
        assert_eq!(input.mode, Mode::Browse);
    }

    #[test]
    fn escape_cancels_the_command_line_and_backspace_edits_it() {
        let mut input = input();
        type_in(&mut input, "m512\x7f\x7f2");
        assert_eq!(input.mode, Mode::Prompt("memory 52".into()));
        type_in(&mut input, "\x1b");
        assert_eq!(input.mode, Mode::Browse);
    }

    #[test]
    fn an_empty_line_runs_nothing_and_arrow_keys_are_ignored() {
        let mut input = input();
        assert_eq!(type_in(&mut input, ":\r"), Effect::None);
        type_in(&mut input, ":");
        assert_eq!(input.feed(b"\x1b[A"), Effect::None);
        assert_eq!(input.mode, Mode::Prompt(String::new()));
    }

    #[test]
    fn keys_that_mean_nothing_do_nothing() {
        let mut input = input();
        assert_eq!(type_in(&mut input, "zZ1 "), Effect::None);
        assert_eq!(input.mode, Mode::Browse);
    }

    #[test]
    fn every_key_starts_a_known_command() {
        for (key, name) in KEYS {
            assert!(action::is_action(name), "{name}");
            assert_eq!(KEYS.iter().filter(|(k, _)| k == key).count(), 1, "{}", *key as char);
        }
    }

    #[test]
    fn the_footer_shows_the_usage_of_the_command_being_written() {
        let mut input = input();
        type_in(&mut input, "c");
        let text = footer(&input, "");
        assert!(text.contains("> cpu _"), "{text}");
        assert!(text.ends_with("cpu [percent] [hold]"), "{text}");
        type_in(&mut input, "\x1b");
        assert!(footer(&input, "memory 5: ok").ends_with("memory 5: ok"));
    }

    #[test]
    fn an_answer_is_cut_to_a_line() {
        assert_eq!(one_line("a\nb"), "a");
        assert_eq!(one_line(&"x".repeat(200)).chars().count(), 100);
    }
}
