//! Requests answered and how long they took, by route and status code, for `/metrics`. The route is
//! the pattern (`/status/:code`), not the URL, so the number of series stays small.
use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::app::AppState;

/// Upper bounds, in seconds, of the buckets of the duration histogram. The last one is `MAX_DELAY_MS`
/// at its default; slower requests only count in `+Inf`.
pub const BUCKETS: [f64; 13] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0];

#[derive(Default, Clone)]
struct Series {
    count: u64,
    sum: f64,
    // Requests that took at most each bound, not cumulative: the ones above the last bound are in `count` only
    buckets: [u64; BUCKETS.len()],
}

#[derive(Default)]
pub struct Counters {
    series: Mutex<BTreeMap<(String, u16), Series>>,
}

pub struct Sample {
    pub route: String,
    pub code: u16,
    pub count: u64,
    pub sum_seconds: f64,
    /// Requests that took at most each of `BUCKETS`, cumulative as Prometheus wants them
    pub cumulative: Vec<u64>,
}

// What the router matched, which only the middleware of a route can see: it leaves it on the response
#[derive(Clone)]
struct Route(String);

// axum writes /status/{code} where the endpoints are documented as /status/:code
fn pattern(matched: &str) -> String {
    matched.replace('{', ":").replace('}', "")
}

impl Counters {
    fn record(&self, route: &str, code: u16, seconds: f64) {
        let mut series = self.series.lock().unwrap();
        let entry = series.entry((route.to_string(), code)).or_default();
        entry.count += 1;
        entry.sum += seconds;
        if let Some(bucket) = BUCKETS.iter().position(|bound| seconds <= *bound) {
            entry.buckets[bucket] += 1;
        }
    }

    pub fn entries(&self) -> Vec<Sample> {
        self.series
            .lock()
            .unwrap()
            .iter()
            .map(|((route, code), series)| {
                let mut total = 0;
                let cumulative = series
                    .buckets
                    .iter()
                    .map(|n| {
                        total += n;
                        total
                    })
                    .collect();
                Sample { route: route.clone(), code: *code, count: series.count, sum_seconds: series.sum, cumulative }
            })
            .collect()
    }
}

// Goes on every route (Router::route_layer): remembers which one answered
pub async fn mark(matched: MatchedPath, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.extensions_mut().insert(Route(pattern(matched.as_str())));
    response
}

// Goes around the whole router: counts the answer and times it, 404s included as "other"
pub async fn count(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let response = next.run(request).await;
    let route = response.extensions().get::<Route>().map_or("other", |route| route.0.as_str());
    state.counters.record(route, response.status().as_u16(), started.elapsed().as_secs_f64());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_route_is_the_pattern() {
        assert_eq!(pattern("/status/{code}"), "/status/:code");
        assert_eq!(pattern("/ready/off"), "/ready/off");
    }

    #[test]
    fn counts_by_route_and_code() {
        let counters = Counters::default();
        counters.record("/status/:code", 418, 0.001);
        counters.record("/status/:code", 418, 0.001);
        counters.record("other", 404, 0.001);
        let entries = counters.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].route.as_str(), entries[0].code, entries[0].count), ("/status/:code", 418, 2));
    }

    #[test]
    fn times_go_in_cumulative_buckets() {
        let counters = Counters::default();
        counters.record("/delay", 200, 0.004); // first bucket
        counters.record("/delay", 200, 0.1); // exactly on a bound: it counts in that bucket
        counters.record("/delay", 200, 90.0); // above the last bound
        let sample = &counters.entries()[0];
        assert_eq!(sample.count, 3);
        assert!((sample.sum_seconds - 90.104).abs() < 1e-9);
        assert_eq!(sample.cumulative[0], 1);
        assert_eq!(sample.cumulative[3], 1);
        assert_eq!(sample.cumulative[4], 2);
        assert_eq!(*sample.cumulative.last().unwrap(), 2);
    }
}
