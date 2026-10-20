use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use crate::resources::payload;
use crate::units::MB;
use axum::body::Body;
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

/// `mb` MB of data, streamed: nothing is held in memory but a chunk. The length is announced, so
/// a client or a proxy can tell a body that was cut short.
pub async fn download(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).payload)?;
    let bytes = params.get("mb") as u64 * MB;
    let headers = [(header::CONTENT_TYPE, "application/octet-stream".to_string()), (header::CONTENT_LENGTH, bytes.to_string())];
    Ok((headers, Body::from_stream(payload::stream(bytes))).into_response())
}

/// Takes an upload and throws it away as it arrives, then says how much came and how long it took.
/// More than `MAX_PAYLOAD_MB` is a `413`, which is also what a proxy with a limit answers.
pub async fn upload(State(state): State<Arc<AppState>>, body: Body) -> Response {
    let limit = state.config.max_payload_mb as u64 * MB;
    let started = Instant::now();
    let mut received: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(piece) = stream.next().await {
        match piece {
            Ok(piece) => received += piece.len() as u64,
            Err(error) => {
                return (StatusCode::BAD_REQUEST, format!("The upload was interrupted after {received} bytes: {error}")).into_response();
            }
        }
        if received > limit {
            let message = format!("Payload too large: the limit is {} MB", state.config.max_payload_mb);
            return (StatusCode::PAYLOAD_TOO_LARGE, message).into_response();
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let mb_per_second = if seconds > 0.0 { received as f64 / MB as f64 / seconds } else { 0.0 };
    axum::Json(json!({
        "bytes": received,
        "seconds": (seconds * 1000.0).round() / 1000.0,
        "mbPerSecond": (mb_per_second * 100.0).round() / 100.0,
    }))
    .into_response()
}
