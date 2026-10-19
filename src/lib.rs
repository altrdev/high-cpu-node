//! torchio breaks things on purpose: it burns CPU, holds memory, fills disk, answers slowly or
//! returns any HTTP status, all on demand over HTTP, to see how a setup behaves under pressure.
//!
//! - [`config`]: the environment variables, validated.
//! - [`endpoints`]: the parameters and limits of every endpoint, shared by the routes and `/help`.
//! - [`app`]: what the routes share, and the router.
//! - [`actions`]: what the endpoints that change the state do and say, shared by the routes and the scenarios.
//! - [`routes`]: one file for each endpoint.
//! - [`resources`]: what the server holds or does on purpose (budgets, memory, files, workers…).
//! - [`http`]: what serves requests and is not specific to an endpoint.
//! - [`server`]: starts listening and ends the process.
//! - [`cli`]: the commands besides the server (`watch`, `healthcheck`, and one for each endpoint that starts a test).
//! - [`system`]: what the operating system says about the process.
pub mod actions;
pub mod app;
pub mod cli;
pub mod config;
pub mod endpoints;
pub mod http;
pub mod logging;
pub mod resources;
pub mod routes;
pub mod server;
pub mod snapshot;
pub mod system;
pub mod units;
