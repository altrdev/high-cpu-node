use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use crate::resources::disk::WriteError;
use crate::routes::over_budget_mb;
use crate::system::errno_name;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Writes `mb` megabytes to a file in `DISK_DIR` and keeps it for `hold` seconds. As for
/// `/memory`, `MAX_DISK_MB` is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).disk)?;
    let (mb, hold) = (params.get("mb"), params.get("hold"));

    let dir = state.disk.dir().display();
    Ok(match state.disk.write(mb, hold as u64).await {
        Ok(()) => (StatusCode::OK, format!("Wrote {mb} MB to {dir}, held for {hold} s")).into_response(),
        Err(WriteError::OverBudget(over)) => over_budget_mb(&over),
        Err(WriteError::Write(e)) => {
            (StatusCode::INSUFFICIENT_STORAGE, format!("Could not write {mb} MB to {dir}: {}", errno_name(&e))).into_response()
        }
    })
}
