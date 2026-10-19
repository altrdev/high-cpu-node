use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Opens `count` file descriptors and holds them for `hold` seconds, to test what happens when a
/// process runs out of them (`EMFILE`). `MAX_FD` is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).fd)?;
    let (count, hold) = (params.get("count"), params.get("hold"));

    Ok(match state.fd.hold(count, hold as u64) {
        Ok(opened) => match opened.failure {
            None => (StatusCode::OK, format!("Opened {count} descriptors, held for {hold} s")).into_response(),
            Some(code) => {
                let message = format!("Opened {} of {count} descriptors, then {code}. Held for {hold} s", opened.opened);
                (StatusCode::SERVICE_UNAVAILABLE, message).into_response()
            }
        },
        Err(over) => {
            (StatusCode::TOO_MANY_REQUESTS, format!("{} descriptors already held, limit is {}", over.held, over.limit)).into_response()
        }
    })
}
