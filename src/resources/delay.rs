//! The requests of `/delay` that are still waiting.
use crate::resources::counted::Counted;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Default)]
pub struct Delay {
    in_flight: Arc<AtomicUsize>,
}

impl Delay {
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Waits without blocking anything else. It counts until it ends or the future is dropped, which
    /// is what happens when the client gives up.
    pub async fn wait(&self, ms: u64) {
        let _waiting = Counted::new(&self.in_flight);
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    async fn counts_while_it_waits() {
        let delay = Arc::new(Delay::default());
        let waiting = tokio::spawn({
            let delay = Arc::clone(&delay);
            async move { delay.wait(1000).await }
        });
        tokio::task::yield_now().await;
        assert_eq!(delay.in_flight(), 1);

        tokio::time::sleep(Duration::from_millis(1100)).await;
        waiting.await.unwrap();
        assert_eq!(delay.in_flight(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn stops_counting_when_the_wait_is_dropped() {
        let delay = Arc::new(Delay::default());
        let waiting = tokio::spawn({
            let delay = Arc::clone(&delay);
            async move { delay.wait(60_000).await }
        });
        tokio::task::yield_now().await;
        assert_eq!(delay.in_flight(), 1);

        waiting.abort();
        let _ = waiting.await;
        assert_eq!(delay.in_flight(), 0);
    }
}
