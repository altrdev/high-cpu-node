use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Keeps the CPU at a level instead of running a calculation: each worker is busy for `percent`
/// of the time during `hold` seconds, which is what a CPU based autoscaler reacts to. With
/// one worker for each CPU, `percent` is the load of the machine. `workers` loads only the first
/// ones, for an uneven load; by default it is all of them. A new request replaces the previous
/// one, and `percent=0` stops it.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).cpu)?;
    Ok(actions::cpu(&state, params.get("percent"), params.get("hold"), params.get("workers") as usize))
}
