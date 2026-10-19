//! Freezes the runtime: every thread that serves requests sleeps, so the process is alive but
//! accepts and answers nothing, to test liveness probes and client timeouts. The workers of `/stress`
//! and `/cpu` have their own threads and don't count.
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A moment to let the answer leave before the threads go to sleep
const SETTLE: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct Block {
    until: Mutex<Option<Instant>>,
    count: AtomicU64,
    last_ms: AtomicU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockSnapshot {
    pub count: u64,
    pub last_ms: u64,
    pub ms_left: u64,
}

/// There is a block going on, or about to start
#[derive(Debug, PartialEq)]
pub struct Busy {
    pub ms_left: u64,
}

impl Block {
    pub fn snapshot(&self) -> BlockSnapshot {
        BlockSnapshot { count: self.count.load(Ordering::Relaxed), last_ms: self.last_ms.load(Ordering::Relaxed), ms_left: self.ms_left() }
    }

    fn ms_left(&self) -> u64 {
        let until = *self.until.lock().unwrap();
        until.map_or(0, |until| until.saturating_duration_since(Instant::now()).as_millis() as u64)
    }

    /// Reserves the block, or says that there is one already
    pub fn reserve(&self, ms: u64) -> Result<Instant, Busy> {
        let mut until = self.until.lock().unwrap();
        let now = Instant::now();
        if let Some(until) = *until
            && until > now
        {
            return Err(Busy { ms_left: (until - now).as_millis() as u64 });
        }
        let end = now + SETTLE + Duration::from_millis(ms);
        *until = Some(end);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.last_ms.store(ms, Ordering::Relaxed);
        Ok(end)
    }
}

impl Block {
    /// Reserves the block, says so in the log and freezes the threads after the answer has left
    pub fn start(&self, ms: u64, format: crate::logging::LogFormat) -> Result<(), Busy> {
        let end = self.reserve(ms)?;
        format.warn(&format!("Blocking every thread of the server for {ms} ms"));
        freeze(end, move || format.info("The server answers again"));
        Ok(())
    }
}

/// Puts every thread of the runtime to sleep until `end`. Each task holds its thread, so the next
/// one is taken by a thread that is still free; a task that starts after `end` has nothing to do.
pub fn freeze(end: Instant, then: impl Fn() + Send + Sync + 'static) {
    let handle = tokio::runtime::Handle::current();
    let threads = handle.metrics().num_workers();
    let then = std::sync::Arc::new(then);
    handle.spawn(async move {
        tokio::time::sleep(SETTLE).await;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(threads));
        for _ in 0..threads {
            let (running, then) = (std::sync::Arc::clone(&running), std::sync::Arc::clone(&then));
            tokio::spawn(async move {
                if let Some(left) = end.checked_duration_since(Instant::now()) {
                    std::thread::sleep(left);
                }
                if running.fetch_sub(1, Ordering::SeqCst) == 1 {
                    then();
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_block_waits_for_the_first() {
        let block = Block::default();
        assert_eq!(block.snapshot(), BlockSnapshot { count: 0, last_ms: 0, ms_left: 0 });
        block.reserve(10_000).unwrap();
        assert!(block.reserve(1).unwrap_err().ms_left > 9_000);
        let snapshot = block.snapshot();
        assert_eq!((snapshot.count, snapshot.last_ms), (1, 10_000));
        assert!(snapshot.ms_left > 9_000);
    }

    #[test]
    fn a_block_that_is_over_leaves_room_for_the_next() {
        let block = Block::default();
        block.reserve(0).unwrap();
        std::thread::sleep(SETTLE + Duration::from_millis(20));
        assert!(block.reserve(0).is_ok());
        assert_eq!(block.snapshot().count, 2);
    }
}
