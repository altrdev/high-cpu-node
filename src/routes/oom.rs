use crate::app::AppState;
use crate::resources::oom;
use crate::system;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Allocates memory without a budget until the kernel kills the process. Refused unless `ALLOW_OOM`
/// is set, since it ends the instance. The answer is sent before it starts.
pub async fn handler(State(state): State<Arc<AppState>>) -> Response {
    if !state.config.allow_oom {
        return (StatusCode::FORBIDDEN, "Forbidden: /oom ends the process, set ALLOW_OOM=1 to allow it").into_response();
    }
    let limit = match system::container_limit_bytes() {
        Some(bytes) => format!("the container limit is {} MB", bytes / crate::units::MB as u64),
        None => "there is no container limit, so it fills the memory of the machine".to_string(),
    };
    state.config.log_format.warn(&format!("Allocating memory until the kernel kills the process: {limit}"));
    oom::start();
    format!("Allocating until the kernel kills the process ({limit})").into_response()
}
