//! What serves requests and is not specific to an endpoint: content negotiation, the token, the
//! query parameters, the count of requests, what is open and `TRUST_PROXY`.
pub mod accept;
pub mod auth;
pub mod counters;
pub mod open;
pub mod params;
pub mod trust;
