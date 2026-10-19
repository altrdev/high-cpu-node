//! A readiness switch: `/ready` answers 200 or 503, `/ready/off` makes it answer 503 (for `seconds`,
//! or until `/ready/on` when 0) so you can watch what a readiness probe or a load balancer does with
//! an instance that leaves the rotation but is still alive.
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
pub struct Ready {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    down: bool,
    // when it becomes ready again, None if only /ready/on does it
    until: Option<Instant>,
    // changes with every switch, so a timer that was overtaken does nothing
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyState {
    pub ready: bool,
    pub seconds_left: Option<u64>,
}

impl Ready {
    pub fn state(&self) -> ReadyState {
        let inner = self.inner.lock().unwrap();
        let seconds_left = match inner.until {
            Some(until) if inner.down => Some(until.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64),
            _ => None,
        };
        ReadyState { ready: !inner.down, seconds_left }
    }

    pub fn on(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        inner.down = false;
        inner.until = None;
    }

    pub fn off(self: &Arc<Self>, seconds: u64) {
        let generation = {
            let mut inner = self.inner.lock().unwrap();
            inner.generation += 1;
            inner.down = true;
            inner.until = (seconds > 0).then(|| Instant::now() + Duration::from_secs(seconds));
            inner.generation
        };
        if seconds > 0 {
            let ready = Arc::clone(self);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                let mut inner = ready.inner.lock().unwrap();
                if inner.generation == generation {
                    inner.down = false;
                    inner.until = None;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn goes_down_for_a_while_and_comes_back_by_itself() {
        let ready = Arc::new(Ready::default());
        assert_eq!(ready.state(), ReadyState { ready: true, seconds_left: None });

        ready.off(2);
        assert_eq!(ready.state(), ReadyState { ready: false, seconds_left: Some(2) });
        tokio::time::sleep(Duration::from_millis(2100)).await;
        assert!(ready.state().ready);
    }

    #[tokio::test(start_paused = true)]
    async fn stays_down_until_it_is_switched_on() {
        let ready = Arc::new(Ready::default());
        ready.off(0);
        tokio::time::sleep(Duration::from_secs(3600)).await;
        assert_eq!(ready.state(), ReadyState { ready: false, seconds_left: None });
        ready.on();
        assert!(ready.state().ready);
    }

    #[tokio::test(start_paused = true)]
    async fn an_overtaken_timer_does_not_switch_it_back() {
        let ready = Arc::new(Ready::default());
        ready.off(1);
        ready.off(0);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!ready.state().ready);
    }
}
