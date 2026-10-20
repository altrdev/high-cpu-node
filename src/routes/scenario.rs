use crate::actions::{self, Outcome};
use crate::app::AppState;
use crate::resources::scenario::{Action, Step};
use crate::scenario_parse;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::time::Duration;

/// A moment between two steps, so what the last one held is given back before the next asks for it
const SETTLE: Duration = Duration::from_millis(100);

/// Where the scenario is, with every step and its state
pub async fn status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.scenario.snapshot(true))
}

/// Starts the scenario in the body: every step is checked against the limits first, and a wrong
/// one is a `400` that says which. The steps then run one after the other, by themselves.
pub async fn start(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let steps = match scenario_parse::parse(&body, &state.config) {
        Ok(steps) => steps,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let run = match state.scenario.start(steps.clone()) {
        Ok(run) => run,
        Err(busy) => {
            let message = format!("A scenario is already running, at step {} of {}: stop it with DELETE /scenario", busy.step, busy.steps);
            return (StatusCode::CONFLICT, message).into_response();
        }
    };
    state.config.log_format.info(&format!("Scenario started: {} steps", steps.len()));
    let task = tokio::spawn(run_steps(Arc::clone(&state), run, steps));
    state.scenario.attach(run, task.abort_handle());
    (StatusCode::ACCEPTED, Json(state.scenario.snapshot(true))).into_response()
}

/// Ends the scenario that is running. What the current step holds for a time (memory, disk,
/// descriptors, connections) is given back when its seconds are over; the CPU load and the readiness
/// switch are undone at once.
pub async fn stop(State(state): State<Arc<AppState>>) -> Response {
    let Some(step) = state.scenario.stop() else {
        return (StatusCode::NOT_FOUND, "No scenario is running").into_response();
    };
    match step.action {
        Action::Cpu { .. } => state.pool.burn(0, 0, 0),
        Action::NotReady => state.ready.on(),
        _ => {}
    }
    state.config.log_format.info("Scenario stopped");
    Json(state.scenario.snapshot(true)).into_response()
}

async fn run_steps(state: Arc<AppState>, run: u64, steps: Vec<Step>) {
    let format = state.config.log_format;
    let total = steps.len();
    for (index, step) in steps.iter().enumerate() {
        state.scenario.begin_step(run, index);
        format.info(&format!("Scenario step {} of {total}: {}", index + 1, step.describe()));
        if let Err(message) = perform(&state, step).await {
            let error = format!("step {} ({}): {message}", index + 1, step.action.name());
            format.warn(&format!("Scenario failed at {error}"));
            state.scenario.finish(run, Err(error));
            return;
        }
        tokio::time::sleep(Duration::from_secs(step.seconds)).await;
        if index + 1 < total {
            tokio::time::sleep(SETTLE).await;
        }
    }
    format.info("Scenario finished");
    state.scenario.finish(run, Ok(()));
}

/// Does what a step says, through the same functions as the endpoints, so with their budgets and
/// their words. The error is the refusal, as the endpoint would have answered it.
async fn perform(state: &AppState, step: &Step) -> Result<(), String> {
    let seconds = step.seconds as i64;
    let outcome = match step.action {
        Action::Cpu { percent, workers } => actions::cpu(state, percent, seconds, workers as usize),
        Action::Memory { mb } => actions::memory(state, mb, seconds).await,
        Action::Disk { mb } => actions::disk(state, mb, seconds).await,
        Action::Fd { count } => actions::fd(state, count, seconds),
        Action::Connections { count } => actions::connections(state, count, seconds).await,
        Action::NotReady => actions::ready_off(state, seconds),
        Action::Block { ms } => actions::block(state, ms),
        Action::Pause => return Ok(()),
    };
    match outcome {
        Outcome::Done(_) => Ok(()),
        // opening fewer than asked is the situation being tested: the step goes on
        Outcome::Partial(message) => {
            state.config.log_format.warn(&format!("Scenario: {message}"));
            Ok(())
        }
        Outcome::Refused(_, message) => Err(message),
    }
}
