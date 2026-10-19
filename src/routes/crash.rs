use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;
use std::time::Duration;

/// Ends the whole process with the exit code you ask for, after `after` seconds, to test restart
/// policies, backoff and `CrashLoopBackOff`. The answer is sent before the process exits.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<String, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).crash)?;
    let (code, after) = (params.get("code"), params.get("after"));

    // with no delay, a moment to let the answer leave
    let wait = if after > 0 { Duration::from_secs(after as u64) } else { Duration::from_millis(50) };
    tokio::spawn(async move {
        tokio::time::sleep(wait).await;
        std::process::exit(code as i32);
    });
    let when = if after > 0 { format!(" in {after} s") } else { String::new() };
    Ok(format!("Exiting with code {code}{when}"))
}
