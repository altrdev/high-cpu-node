//! Whether the server is draining after SIGTERM, and what is still open: the requests being
//! answered and the connections. None of it knows about HTTP.
use crate::resources::counted::Counted;
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::watch;

pub struct Shutdown {
    grace_seconds: i64,
    requests: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
    since: Mutex<Option<Instant>>,
    draining: watch::Sender<bool>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownSnapshot {
    pub draining: bool,
    pub grace_seconds: i64,
    /// Seconds since SIGTERM, `None` while the server is not draining
    pub drain_seconds: Option<f64>,
    pub open_requests: i64,
    pub open_connections: i64,
}

impl Shutdown {
    pub fn new(grace_seconds: i64) -> Shutdown {
        Shutdown {
            grace_seconds,
            requests: Arc::default(),
            connections: Arc::default(),
            since: Mutex::new(None),
            draining: watch::channel(false).0,
        }
    }

    /// A request is being answered, for as long as the result is kept
    pub fn request(&self) -> Counted {
        Counted::new(&self.requests)
    }

    /// A connection is open, for as long as the result is kept
    pub fn connection(&self) -> Counted {
        Counted::new(&self.connections)
    }

    /// Starts draining. `false` when it already was.
    pub fn begin(&self) -> bool {
        let mut since = self.since.lock().unwrap();
        if since.is_some() {
            return false;
        }
        *since = Some(Instant::now());
        self.draining.send_replace(true);
        true
    }

    /// Changes to `true` when the draining begins, which is when the listener stops accepting
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.draining.subscribe()
    }

    pub fn snapshot(&self) -> ShutdownSnapshot {
        let drain_seconds = self.since.lock().unwrap().map(|since| (since.elapsed().as_secs_f64() * 10.0).round() / 10.0);
        ShutdownSnapshot {
            draining: drain_seconds.is_some(),
            grace_seconds: self.grace_seconds,
            drain_seconds,
            open_requests: self.requests.load(Ordering::Relaxed) as i64,
            open_connections: self.connections.load(Ordering::Relaxed) as i64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_open_is_counted_until_it_is_dropped() {
        let shutdown = Shutdown::new(0);
        let (request, first, second) = (shutdown.request(), shutdown.connection(), shutdown.connection());
        let open = |shutdown: &Shutdown| (shutdown.snapshot().open_requests, shutdown.snapshot().open_connections);
        assert_eq!(open(&shutdown), (1, 2));
        drop((request, first));
        assert_eq!(open(&shutdown), (0, 1));
        drop(second);
        assert_eq!(open(&shutdown), (0, 0));
    }

    #[test]
    fn it_drains_once_and_says_for_how_long() {
        let shutdown = Shutdown::new(30);
        let before = shutdown.snapshot();
        assert_eq!((before.draining, before.grace_seconds, before.drain_seconds), (false, 30, None));

        let draining = shutdown.subscribe();
        assert!(!*draining.borrow());
        assert!(shutdown.begin());
        assert!(!shutdown.begin(), "a second signal does not start it again");
        assert!(*draining.borrow());
        let after = shutdown.snapshot();
        assert!(after.draining && after.drain_seconds.is_some_and(|s| s < 1.0));
    }
}
