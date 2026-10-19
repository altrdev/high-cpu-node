//! What serves requests and is not specific to an endpoint: content negotiation, the token, the
//! query parameters, the count of requests and `TRUST_PROXY`.
pub mod accept;
pub mod auth;
pub mod counters;
pub mod params;
pub mod trust;
