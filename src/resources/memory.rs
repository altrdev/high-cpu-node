//! The memory that `/memory` and `/leak` hold, taken straight from the OS: every page is written,
//! because a plain allocation may not be committed until it is used, and the allocator may keep
//! the memory after it is freed. A mapping goes back to the OS the moment it is dropped.
use crate::resources::budget::{Budget, OverBudget, Reservation};
use crate::units::MB;
use memmap2::MmapMut;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

pub struct Memory {
    budget: Arc<Budget>,
}

#[derive(Debug)]
pub enum HoldError {
    OverBudget(OverBudget),
    Allocation(io::Error),
}

/// `mb` megabytes with every page written. Blocks while they are.
fn allocate(mb: i64) -> io::Result<MmapMut> {
    let mut map = MmapMut::map_anon(mb as usize * MB as usize)?;
    map.fill(1);
    Ok(map)
}

/// Allocates on the threads meant for blocking work, so the server keeps answering meanwhile
async fn allocate_blocking(mb: i64) -> io::Result<MmapMut> {
    tokio::task::spawn_blocking(move || allocate(mb)).await.map_err(io::Error::other)?
}

impl Memory {
    pub fn new(limit_mb: i64) -> Memory {
        Memory { budget: Budget::new(limit_mb) }
    }

    /// The total that can be held at the same time, in MB
    pub fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }

    /// Allocates `mb` megabytes and answers once they are, then holds them for `seconds`. The
    /// release happens even if nobody waits for the answer any more.
    pub async fn hold(&self, mb: i64, seconds: u64) -> Result<(), HoldError> {
        let reservation = self.budget.reserve(mb).map_err(HoldError::OverBudget)?;
        let (allocated, answer) = oneshot::channel();
        let budget = Arc::clone(&self.budget);
        tokio::spawn(async move {
            match allocate_blocking(mb).await {
                Err(e) => {
                    drop(reservation);
                    let _ = allocated.send(Err(e));
                }
                Ok(held) => {
                    let _ending = budget.ends_in(seconds);
                    let _ = allocated.send(Ok(()));
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                    drop(held);
                    drop(reservation);
                }
            }
        });
        match answer.await {
            Ok(result) => result.map_err(HoldError::Allocation),
            Err(_) => Err(HoldError::Allocation(io::Error::other("the allocation was cancelled"))),
        }
    }

    /// Takes the first step of a leak, `rate` MB, and answers; then adds `rate` MB each second up
    /// to `mb` MB, or until the budget is full, and holds it all for `seconds` after the last step.
    pub async fn leak(&self, rate: i64, mb: i64, seconds: u64) -> Result<(), OverBudget> {
        let mut leak = Leak { budget: Arc::clone(&self.budget), rate, mb, leaked: 0, held: Vec::new() };
        if !leak.grow().await? {
            return Err(OverBudget { held: self.budget.held(), limit: self.budget.limit() });
        }
        tokio::spawn(async move {
            while leak.leaked < leak.mb {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if !matches!(leak.grow().await, Ok(true)) {
                    break;
                }
            }
            let _ending = leak.budget.ends_in(seconds);
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            drop(leak);
        });
        Ok(())
    }
}

struct Leak {
    budget: Arc<Budget>,
    rate: i64,
    mb: i64,
    leaked: i64,
    // what has been allocated, and what it takes from the budget
    held: Vec<(MmapMut, Reservation)>,
}

impl Leak {
    /// Adds the next step. False when the memory could not be taken.
    async fn grow(&mut self) -> Result<bool, OverBudget> {
        let chunk = self.rate.min(self.mb - self.leaked);
        let reservation = self.budget.reserve(chunk)?;
        match allocate_blocking(chunk).await {
            Ok(map) => {
                self.held.push((map, reservation));
                self.leaked += chunk;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocated_memory_is_written() {
        let map = allocate(2).unwrap();
        assert_eq!(map.len(), 2 * MB as usize);
        assert!(map.iter().all(|byte| *byte == 1));
    }

    #[tokio::test]
    async fn hold_takes_from_the_budget_and_gives_it_back() {
        let memory = Memory::new(100);
        memory.hold(60, 0).await.unwrap();
        for _ in 0..100 {
            if memory.budget().held() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(memory.budget().held(), 0);
    }

    #[tokio::test]
    async fn hold_refuses_what_does_not_fit() {
        let memory = Memory::new(100);
        memory.hold(60, 30).await.unwrap();
        match memory.hold(50, 0).await {
            Err(HoldError::OverBudget(over)) => assert_eq!(over, OverBudget { held: 60, limit: 100 }),
            other => panic!("{other:?}"),
        }
    }

    // Real time: the memory is allocated on a blocking thread, which the paused clock does not wait for
    async fn until(what: &str, mut check: impl FnMut() -> bool) {
        for _ in 0..400 {
            if check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {what}");
    }

    #[tokio::test]
    async fn leak_grows_each_second_up_to_its_ceiling_then_lets_go() {
        let memory = Memory::new(100);
        memory.leak(10, 30, 1).await.unwrap();
        assert_eq!(memory.budget().held(), 10, "the first step is taken at once");

        until("the ceiling", || memory.budget().held() == 30).await;
        until("the release", || memory.budget().held() == 0).await;
    }

    #[tokio::test]
    async fn leak_stops_growing_when_the_budget_is_full() {
        let memory = Memory::new(100);
        memory.hold(85, 3600).await.unwrap();
        memory.leak(10, 50, 0).await.unwrap();
        assert_eq!(memory.budget().held(), 95);
        // the next step of 10 does not fit: the leak lets go of its 10 and stops
        until("the leak to stop", || memory.budget().held() == 85).await;
    }

    #[tokio::test]
    async fn leak_is_refused_when_not_even_the_first_step_fits() {
        let memory = Memory::new(100);
        memory.hold(95, 30).await.unwrap();
        assert_eq!(memory.leak(10, 50, 5).await.err(), Some(OverBudget { held: 95, limit: 100 }));
    }
}
