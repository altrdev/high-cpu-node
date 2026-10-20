use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Opens `count` TCP connections to the server itself and holds them for `hold` seconds, to test
/// connection limits, load balancers and what a shutdown does with open sockets. `MAX_CONNECTIONS`
/// is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &state.specs.connections)?;
    Ok(actions::connections(&state, params.get("count"), params.get("hold")).await)
}
