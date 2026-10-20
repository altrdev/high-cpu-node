use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Opens `count` file descriptors and holds them for `hold` seconds, to test what happens when a
/// process runs out of them (`EMFILE`). `MAX_FD` is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &state.specs.fd)?;
    Ok(actions::fd(&state, params.get("count"), params.get("hold")))
}
