use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Makes the server stop answering for `ms`: every thread of the runtime sleeps, so the process is
/// alive but accepts nothing, to test liveness probes and timeouts. The answer is sent before it starts.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Outcome, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).block)?;
    Ok(actions::block(&state, params.get("ms")))
}
