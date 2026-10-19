use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Makes the server stop answering for `ms`: every thread of the runtime sleeps, so the process is
/// alive but accepts nothing, to test liveness probes and timeouts. The answer is sent before it starts.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).block)?;
    let ms = params.get("ms");

    if let Err(busy) = state.block.start(ms as u64, state.config.log_format) {
        let message = format!("Already blocked for {} more ms", busy.ms_left);
        return Ok((StatusCode::CONFLICT, message).into_response());
    }
    Ok(format!("Blocking for {ms} ms").into_response())
}
