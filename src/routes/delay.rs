use crate::app::AppState;
use crate::endpoints::specs;
use crate::params;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// The /delay requests still waiting. A request counts until it is answered or the client gives up:
// the guard is dropped with the handler when the connection closes.
#[derive(Default)]
pub struct Delay {
    in_flight: AtomicUsize,
}

impl Delay {
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }
}

struct Waiting<'a>(&'a AtomicUsize);

impl<'a> Waiting<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Waiting(counter)
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

// Answers after `ms` milliseconds without blocking the server
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).delay) {
        Ok(params) => params,
        Err(rejection) => return rejection.into_response(),
    };
    let ms = params.get("ms");

    let _waiting = Waiting::new(&state.delay.in_flight);
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    (StatusCode::OK, format!("Answered after {ms} ms")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::router;
    use crate::config::Config;
    use tokio::io::AsyncWriteExt;
    use tokio::net::{TcpListener, TcpStream};

    async fn until(what: &str, check: impl Fn() -> bool) {
        for _ in 0..100 {
            if check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("timed out waiting for {what}");
    }

    #[tokio::test]
    async fn a_request_the_client_gave_up_on_is_not_counted_any_more() {
        let state = Arc::new(AppState::new(Config::read(|_| None, |_| {})).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router(state.clone())).into_future());

        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(b"GET /delay?ms=4000 HTTP/1.1\r\nHost: test\r\n\r\n").await.unwrap();
        until("the delay to be counted", || state.delay.in_flight() == 1).await;

        drop(client);
        until("the delay to be forgotten", || state.delay.in_flight() == 0).await;
    }
}
