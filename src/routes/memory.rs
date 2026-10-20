use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Allocates `mb` megabytes and holds them for `hold` seconds. `MAX_MEMORY_MB` is the
/// total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &state.specs.memory)?;
    Ok(actions::memory(&state, params.get("mb"), params.get("hold")).await)
}
