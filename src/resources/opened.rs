//! What `/fd` and `/connections` share once they have tried to open what was asked: what could be
//! opened is kept for the time asked, and what could not goes back to the budget.
use crate::resources::budget::{Budget, Reservation};
use std::sync::Arc;
use std::time::Duration;

/// How many could be opened, and why the others could not
#[derive(Debug, PartialEq)]
pub struct Opened {
    pub opened: usize,
    pub failure: Option<String>,
}

/// Keeps `things` (all that was opened) for `seconds`, and gives back to the budget what was reserved
/// for the ones that could not be opened. Everything is let go of when the time is over.
pub fn keep<T: Send + 'static>(
    budget: &Arc<Budget>,
    mut reservation: Reservation,
    things: Vec<T>,
    failure: Option<String>,
    seconds: u64,
) -> Opened {
    reservation.shrink_to(things.len() as i64);
    let opened = things.len();

    let ending = budget.ends_in(seconds);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        drop(ending);
        drop(things);
        drop(reservation);
    });
    Opened { opened, failure }
}
