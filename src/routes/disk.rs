use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Writes `mb` megabytes to a file in `DISK_DIR` and keeps it for `hold` seconds. As for
/// `/memory`, `MAX_DISK_MB` is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).disk)?;
    Ok(actions::disk(&state, params.get("mb"), params.get("hold")).await)
}
