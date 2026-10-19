use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Keeps the CPU at a level instead of running a calculation: each worker is busy for `percent`
/// of the time during `hold` seconds, which is what a CPU based autoscaler reacts to. With
/// one worker for each CPU, `percent` is the load of the machine. A new request replaces
/// the previous one, and `percent=0` stops it.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<String, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).cpu)?;
    let (percent, hold) = (params.get("percent"), params.get("hold"));

    state.pool.burn(percent, hold);
    if percent == 0 || hold == 0 {
        return Ok("Stopped burning CPU".into());
    }
    Ok(format!("Burning {percent}% of {} workers for {hold} s", state.pool.workers()))
}
