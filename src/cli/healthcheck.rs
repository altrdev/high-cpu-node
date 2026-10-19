//! `torchio healthcheck`, for the `HEALTHCHECK` of an image that has no `wget` or `curl`.
use super::http;
use std::process::ExitCode;
use std::time::Duration;

/// Exits with 0 when `/` answers with a 2xx. It does not look at readiness: an instance that is
/// not ready is alive.
pub fn run() -> ExitCode {
    match http::get(&format!("http://127.0.0.1:{}/", http::port()), Duration::from_secs(3)) {
        Ok((status, _)) if (200..300).contains(&status) => return ExitCode::SUCCESS,
        Ok((status, _)) => eprintln!("unhealthy: HTTP {status}"),
        Err(message) => eprintln!("unhealthy: {message}"),
    }
    ExitCode::FAILURE
}
