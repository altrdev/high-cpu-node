use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::app::AppState;

// Requests answered, by route and status code, for /metrics. The route is the pattern
// (/status/:code), not the URL, so the number of series stays small.
#[derive(Default)]
pub struct Counters {
    counts: Mutex<BTreeMap<(String, u16), u64>>,
}

pub struct Sample {
    pub route: String,
    pub code: u16,
    pub count: u64,
}

// What the router matched, which only the middleware of a route can see: it leaves it on the response
#[derive(Clone)]
struct Route(String);

// axum writes /status/{code} where the endpoints are documented as /status/:code
fn pattern(matched: &str) -> String {
    matched.replace('{', ":").replace('}', "")
}

impl Counters {
    fn record(&self, route: &str, code: u16) {
        *self.counts.lock().unwrap().entry((route.to_string(), code)).or_insert(0) += 1;
    }

    pub fn entries(&self) -> Vec<Sample> {
        self.counts
            .lock()
            .unwrap()
            .iter()
            .map(|((route, code), count)| Sample { route: route.clone(), code: *code, count: *count })
            .collect()
    }
}

// Goes on every route (Router::route_layer): remembers which one answered
pub async fn mark(matched: MatchedPath, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.extensions_mut().insert(Route(pattern(matched.as_str())));
    response
}

// Goes around the whole router: counts the answer, 404s included as "other"
pub async fn count(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let route = response.extensions().get::<Route>().map_or("other", |route| route.0.as_str());
    state.counters.record(route, response.status().as_u16());
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
        counters.record("/status/:code", 418);
        counters.record("/status/:code", 418);
        counters.record("other", 404);
        let entries = counters.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].route.as_str(), entries[0].code, entries[0].count), ("/status/:code", 418, 2));
    }
}
