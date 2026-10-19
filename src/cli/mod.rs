//! The commands of the binary:
//!
//! - `torchio`: the server;
//! - `torchio watch [--url <url>] [--interval <seconds>] [--once]`: a terminal view of `/info`;
//! - `torchio healthcheck`: exits with 0 when the server answers.
mod healthcheck;
mod http;
mod info;
mod view;
mod watch;

use std::process::ExitCode;

pub const USAGE: &str = "Usage: torchio watch [--url <url>] [--interval <seconds>] [--once]

  --url       server to watch (default: http://127.0.0.1:$PORT or :3000)
  --interval  seconds between samples (default: 1)
  --once      print one sample and exit, with status 1 if the server can't be reached

Other commands:
  torchio              start the server
  torchio healthcheck  exit with status 0 when the server answers on $PORT (or 3000)";

/// Runs the command in `args` (what follows the name of the program) and says how it ended
pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None => crate::server::run(),
        Some("watch") => watch::run(&args[1..]),
        Some("healthcheck") => healthcheck::run(),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(_) => {
            println!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
