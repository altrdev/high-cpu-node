use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Writes `lines` lines to stdout at `rate` lines each second, then stops, to test log collection,
/// rotation and what the logs of a container do to a disk. A new request replaces the one still
/// running, and the lines already written stay where they are.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<String, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).log)?;
    let (lines, rate) = (params.get("lines"), params.get("rate"));

    state.log.start(lines, rate);
    let seconds = (lines + rate - 1) / rate;
    Ok(format!("Writing {lines} lines at {rate} lines/s ({seconds} s)"))
}
