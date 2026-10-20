use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Memory that grows by itself, `rate` MB each second up to `mb` MB, and is released after
/// `hold` seconds: a leak you can watch from the outside before it ends in an OOM kill. It
/// uses the budget of `/memory` (`MAX_MEMORY_MB`) as it grows, and stops growing when that is full.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &state.specs.leak)?;
    Ok(actions::leak(&state, params.get("rate"), params.get("mb"), params.get("hold")).await)
}
