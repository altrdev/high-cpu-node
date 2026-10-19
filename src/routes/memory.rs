use crate::app::AppState;
use crate::endpoints::specs;
use crate::{memory, params};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

pub fn over_budget(held: i64, limit: i64) -> Response {
    (StatusCode::TOO_MANY_REQUESTS, format!("{held} MB already held, limit is {limit} MB")).into_response()
}

// Allocates `mb` megabytes and holds them for `hold` seconds. MAX_MEMORY_MB is the
// total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).memory) {
        Ok(params) => params,
        Err(rejection) => return rejection,
    };
    let (mb, hold) = (params.get("mb"), params.get("hold"));

    let Some(reservation) = state.memory.reserve(mb) else {
        return over_budget(state.memory.held(), state.memory.limit());
    };

    // the work goes on even if the client hangs up before the memory is allocated
    let (done, answer) = oneshot::channel();
    tokio::spawn(async move {
        let allocated = tokio::task::spawn_blocking(move || memory::hold(mb)).await.expect("the allocation does not panic");
        match allocated {
            Err(e) => {
                drop(reservation);
                let _ = done.send(Err(e));
            }
            Ok(held) => {
                let _ = done.send(Ok(()));
                tokio::time::sleep(Duration::from_secs(hold as u64)).await;
                drop(held);
                drop(reservation);
            }
        }
    });

    match answer.await {
        Ok(Ok(())) => (StatusCode::OK, format!("Allocated {mb} MB, held for {hold} s")).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Could not allocate {mb} MB: {e}")).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
