use crate::app::AppState;
use crate::budget::Reservation;
use crate::endpoints::specs;
use crate::routes::memory::over_budget;
use crate::{memory, params};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use memmap2::MmapMut;
use std::sync::Arc;
use std::time::Duration;

struct Leak {
    state: Arc<AppState>,
    rate: i64,
    mb: i64,
    leaked: i64,
    // what has been allocated, and what it takes from the budget
    held: Vec<(MmapMut, Reservation)>,
}

impl Leak {
    // Adds the next step. False when nothing could be added.
    async fn grow(&mut self) -> bool {
        let chunk = self.rate.min(self.mb - self.leaked);
        let Some(reservation) = self.state.memory.reserve(chunk) else {
            return false;
        };
        match tokio::task::spawn_blocking(move || memory::hold(chunk)).await {
            Ok(Ok(map)) => {
                self.held.push((map, reservation));
                self.leaked += chunk;
                true
            }
            _ => false,
        }
    }
}

// Memory that grows by itself, `rate` MB each second up to `mb` MB, and is released after
// `hold` seconds: a leak you can watch from the outside before it ends in an OOM kill. It
// uses the budget of /memory (MAX_MEMORY_MB) as it grows, and stops growing when that is full.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).leak) {
        Ok(params) => params,
        Err(rejection) => return rejection,
    };
    let (rate, mb, hold) = (params.get("rate"), params.get("mb"), params.get("hold"));

    let mut leak = Leak { state: state.clone(), rate, mb, leaked: 0, held: Vec::new() };
    if !leak.grow().await {
        return over_budget(state.memory.held(), state.memory.limit());
    }

    tokio::spawn(async move {
        while leak.leaked < leak.mb {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !leak.grow().await {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(hold as u64)).await;
        drop(leak);
    });

    let seconds = (mb + rate - 1) / rate;
    (StatusCode::OK, format!("Leaking {rate} MB/s up to {mb} MB ({seconds} s), then held for {hold} s")).into_response()
}
