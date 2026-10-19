use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use crate::resources::block;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Makes the server stop answering for `ms`: every thread of the runtime sleeps, so the process is
/// alive but accepts nothing, to test liveness probes and timeouts. The answer is sent before it starts.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).block)?;
    let ms = params.get("ms");

    let end = match state.block.reserve(ms as u64) {
        Ok(end) => end,
        Err(busy) => {
            let message = format!("Already blocked for {} more ms", busy.ms_left);
            return Ok((StatusCode::CONFLICT, message).into_response());
        }
    };
    let format = state.config.log_format;
    format.warn(&format!("Blocking every thread of the server for {ms} ms"));
    block::freeze(end, move || format.info("The server answers again"));
    Ok(format!("Blocking for {ms} ms").into_response())
}
