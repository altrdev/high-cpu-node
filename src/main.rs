mod accept;
mod app;
mod auth;
mod budget;
mod config;
mod endpoints;
mod errno;
mod memory;
mod params;
mod ready;
mod routes;
mod units;
mod workers;

use app::AppState;
use config::Config;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() {
    let config = Config::from_env();
    let state = Arc::new(AppState::new(config.clone()));
    let app = app::router(state.clone());

    tokio::spawn(shutdown(state));

    // START_DELAY_SECONDS: the port stays closed for a while, like an application that is slow to start
    if config.start_delay_seconds > 0 {
        println!("{} will start listening in {} s", config.name, config.start_delay_seconds);
        tokio::time::sleep(Duration::from_secs(config.start_delay_seconds as u64)).await;
    }

    let address = (config.host, u16::try_from(config.port).unwrap_or_else(|_| fail(format!("port {} is out of range", config.port))));
    let listener = TcpListener::bind(address).await.unwrap_or_else(|e| fail(e));
    println!("{} running on {}:{} with {} workers", config.name, config.host, config.port, config.workers);
    axum::serve(listener, app).await.unwrap_or_else(|e| fail(e));
}

fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("Internal Server Error: {error}");
    exit(1);
}

// SIGTERM and SIGINT end the process right away, without waiting for the requests in progress
async fn shutdown(state: Arc<AppState>) {
    let mut term = signal(SignalKind::terminate()).expect("a SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("a SIGINT handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    state.stop();
    exit(0);
}
