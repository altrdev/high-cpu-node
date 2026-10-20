//! The worker threads that `/stress` and `/cpu` use to keep the cores busy.
use serde::Serialize;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

// How long a worker spins and sleeps in a round when /cpu keeps it busy for a share of the time
const PERIOD: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Burn {
    percent: i64,
    ends_at: Instant,
}

enum Message {
    Stress { number: i64, reply: oneshot::Sender<u64> },
    Burn(Option<Burn>),
}

// One thread per core: the calculation of /stress would block the runtime, and requests are
// dispatched round-robin so concurrent ones can use several cores.
pub struct WorkerPool {
    senders: Vec<Sender<Message>>,
    // the requests each worker is being waited on for, which ends when it is answered or the client leaves
    in_flight: Vec<Arc<AtomicUsize>>,
    next: AtomicUsize,
    // what /cpu is burning, and how many workers (the first ones) it loads
    burning: Mutex<Option<(Burn, usize)>>,
}

// Counts a request of /stress until it is dropped
pub struct Waiting(Arc<AtomicUsize>);

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BurnSnapshot {
    pub percent: i64,
    pub seconds_left: u64,
    /// How many workers are loaded, the first ones: all of them unless `/cpu` was asked for fewer
    pub workers: usize,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StressSnapshot {
    pub workers: usize,
    pub in_flight: usize,
    pub per_worker: Vec<usize>,
    pub burn: Option<BurnSnapshot>,
}

impl WorkerPool {
    pub fn new(count: usize) -> WorkerPool {
        let mut senders = Vec::with_capacity(count);
        for index in 0..count {
            let (sender, receiver) = mpsc::channel();
            thread::Builder::new().name(format!("worker-{index}")).spawn(move || work(receiver)).expect("a worker thread");
            senders.push(sender);
        }
        WorkerPool {
            senders,
            in_flight: (0..count).map(|_| Arc::default()).collect(),
            next: AtomicUsize::new(0),
            burning: Mutex::new(None),
        }
    }

    // Hands the calculation to the next worker. The answer comes through `reply`, and never
    // comes if the worker is gone. Keep the returned value for as long as the answer is waited for.
    pub fn run(&self, number: i64, reply: oneshot::Sender<u64>) -> Waiting {
        let worker = self.next.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        self.in_flight[worker].fetch_add(1, Ordering::Relaxed);
        // if the worker is gone the message, and with it the reply, is dropped
        let _ = self.senders[worker].send(Message::Stress { number, reply });
        Waiting(Arc::clone(&self.in_flight[worker]))
    }

    // Keeps the first `workers` workers busy for `percent` of the time during `seconds`, and the others
    // idle; 0 to stop. More workers than there are is all of them.
    pub fn burn(&self, percent: i64, seconds: i64, workers: usize) {
        let workers = workers.min(self.senders.len());
        let burn = (percent > 0 && seconds > 0 && workers > 0)
            .then(|| Burn { percent, ends_at: Instant::now() + Duration::from_secs(seconds as u64) });
        *self.burning.lock().unwrap() = burn.map(|burn| (burn, workers));
        for (index, sender) in self.senders.iter().enumerate() {
            let _ = sender.send(Message::Burn(burn.filter(|_| index < workers)));
        }
    }

    pub fn snapshot(&self) -> StressSnapshot {
        let per_worker: Vec<usize> = self.in_flight.iter().map(|count| count.load(Ordering::Relaxed)).collect();
        let burn = (*self.burning.lock().unwrap()).and_then(|(burn, workers)| {
            let seconds_left = burn.ends_at.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64;
            (seconds_left > 0).then_some(BurnSnapshot { percent: burn.percent, seconds_left, workers })
        });
        StressSnapshot { workers: self.senders.len(), in_flight: per_worker.iter().sum(), per_worker, burn }
    }
}

// A worker answers /stress requests and, when asked, keeps its core busy for a share of the time.
// Between the slices of a burn it waits for messages, so a /stress request is still picked up.
fn work(messages: Receiver<Message>) {
    let mut burn: Option<Burn> = None;
    let mut next_slice: Option<Instant> = None;
    loop {
        let message = match next_slice {
            None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => messages.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match message {
            Ok(Message::Stress { number, reply }) => {
                let _ = reply.send(fibo(number - 1));
            }
            Ok(Message::Burn(new)) => {
                burn = new;
                next_slice = burn.map(|_| Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let (Some(current), Some(at)) = (burn, next_slice)
            && Instant::now() >= at
        {
            next_slice = slice(current);
            if next_slice.is_none() {
                burn = None;
            }
        }
    }
}

// Spins for `percent` of the period. Returns when the next one starts, None when the burn is over.
fn slice(burn: Burn) -> Option<Instant> {
    let start = Instant::now();
    if start >= burn.ends_at {
        return None;
    }
    let busy_until = start + Duration::from_millis(burn.percent as u64);
    while Instant::now() < busy_until {
        std::hint::spin_loop();
    }
    Some(start + PERIOD)
}

// Deliberately the slow recursive version: the point is to keep a core busy
fn fibo(n: i64) -> u64 {
    if black_box(n) < 2 { 1 } else { fibo(n - 2) + fibo(n - 1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_first_terms_of_the_sequence_are_one() {
        assert_eq!(fibo(-1), 1);
        assert_eq!(fibo(0), 1);
        assert_eq!(fibo(19), 6765);
    }

    #[tokio::test]
    async fn answers_a_request_and_spreads_them_over_the_workers() {
        let pool = WorkerPool::new(2);
        let (first, first_reply) = oneshot::channel();
        let (second, second_reply) = oneshot::channel();
        let _a = pool.run(20, first);
        let _b = pool.run(20, second);
        assert_eq!(pool.snapshot().per_worker, [1, 1], "one for each worker, round-robin");
        assert_eq!(first_reply.await.unwrap(), 6765);
        assert_eq!(second_reply.await.unwrap(), 6765);
    }

    #[test]
    fn a_request_is_not_counted_once_it_is_dropped() {
        let pool = WorkerPool::new(1);
        let (reply, _answer) = oneshot::channel();
        let waiting = pool.run(1, reply);
        assert_eq!(pool.snapshot().in_flight, 1);
        drop(waiting);
        assert_eq!(pool.snapshot().in_flight, 0);
    }

    #[test]
    fn burn_is_in_the_snapshot_until_it_is_stopped() {
        let pool = WorkerPool::new(1);
        assert_eq!(pool.snapshot().burn, None);
        pool.burn(40, 20, 1);
        assert_eq!(pool.snapshot().burn, Some(BurnSnapshot { percent: 40, seconds_left: 20, workers: 1 }));
        pool.burn(0, 20, 1);
        assert_eq!(pool.snapshot().burn, None);
        pool.burn(40, 0, 1);
        assert_eq!(pool.snapshot().burn, None);
    }

    #[test]
    fn burn_loads_only_the_first_workers() {
        let pool = WorkerPool::new(4);
        pool.burn(40, 20, 2);
        assert_eq!(pool.snapshot().burn.map(|burn| burn.workers), Some(2));
        pool.burn(40, 20, 10);
        assert_eq!(pool.snapshot().burn.map(|burn| burn.workers), Some(4), "more than there are is all of them");
        pool.burn(40, 20, 0);
        assert_eq!(pool.snapshot().burn, None, "no workers is nothing to burn");
    }

    #[tokio::test]
    async fn a_request_is_still_answered_while_the_worker_burns() {
        let pool = WorkerPool::new(1);
        pool.burn(50, 5, 1);
        tokio::time::sleep(Duration::from_millis(150)).await;
        let (reply, answer) = oneshot::channel();
        let _waiting = pool.run(20, reply);
        let value = tokio::time::timeout(Duration::from_secs(2), answer).await.expect("answered in time").unwrap();
        assert_eq!(value, 6765);
    }
}
