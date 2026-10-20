//! The commands of the binary:
//!
//! - `torchio`, or `torchio serve`: the server;
//! - `torchio watch [--url <url>] [--interval <seconds>] [--once] [--read-only]`: a terminal view of
//!   `/info`, with keys that start the tests unless it is read only;
//! - `torchio healthcheck`: exits with 0 when the server answers;
//! - `torchio version`: prints the version;
//! - `torchio completions <bash|zsh|fish>`: the script that completes the commands in a shell;
//! - `torchio cpu 80`, `torchio memory 512`…: start a test on a running server.
mod action;
mod body;
mod completions;
mod healthcheck;
mod http;
mod info;
mod interactive;
mod view;
mod watch;

pub use action::command_for;
use std::process::ExitCode;

pub const USAGE: &str = "Usage: torchio watch [--url <url>] [--interval <seconds>] [--token <token>] [--once] [--read-only]

  --url        server to watch (default: http://127.0.0.1:$PORT or :3000)
  --interval   seconds between samples (default: 1)
  --token      token for the commands (default: $TOKEN)
  --once       print one sample and exit, with status 1 if the server can't be reached
  --read-only  only look: no keys, even on a terminal

On a terminal, watch takes keys that start the tests: c cpu, m memory, l leak, d disk, f fd,
g log, s stress, r ready, x crash, : any command, q quit.

Start a test on a running server (--url, --token or $TOKEN, and --<parameter> <value> work on all):
  torchio stress [number]              torchio memory [mb] [hold]
  torchio cpu [percent] [hold]         torchio leak [rate] [mb] [hold]
  torchio delay [ms]                   torchio disk [mb] [hold]
  torchio status <code>                torchio fd [count] [hold]
  torchio log [lines] [rate]           torchio crash [code] [after]
  torchio block [ms]
  torchio ready on | off [seconds]

The two that send or read a body (--url and --token work on both):
  torchio scenario <file|-> [--wait]   run a scenario from a file or stdin, and with --wait follow it
  torchio scenario status | stop       where it is, or end it
  torchio payload get [mb]             download mb MB, throw them away, report the speed
  torchio payload post <file|->        upload a file or stdin, report what the server counted

Other commands:
  torchio serve        start the server (the same as torchio with no command)
  torchio completions  print the completion script of a shell: bash, zsh or fish
  torchio healthcheck  exit with status 0 when the server answers on $PORT (or 3000)
  torchio version      print the version (also --version and -V)";

/// Runs the command in `args` (what follows the name of the program) and says how it ended
pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None => crate::server::run(),
        // the server takes its settings from the environment: `serve` has nothing to add
        Some("serve") if args.len() == 1 => crate::server::run(),
        Some("watch") => watch::run(&args[1..]),
        Some("scenario") => body::scenario(&args[1..]),
        Some("payload") => body::payload(&args[1..]),
        Some("healthcheck") => healthcheck::run(),
        Some("completions") => completions::run(&args[1..]),
        Some("version" | "--version" | "-V") => {
            println!("torchio {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(name) if action::is_action(name) => action::run(name, &args[1..]),
        Some(_) => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
