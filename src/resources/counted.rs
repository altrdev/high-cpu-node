//! A count of what is going on right now, kept by a guard: it goes up when the guard is made and
//! down when it is dropped, whichever way that happens (the end, an error, a client that leaves).
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts something as going on until it is dropped
#[must_use = "it stops counting as soon as it is dropped"]
pub struct Counted(Arc<AtomicUsize>);

impl Counted {
    pub fn new(counter: &Arc<AtomicUsize>) -> Counted {
        counter.fetch_add(1, Ordering::Relaxed);
        Counted(Arc::clone(counter))
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_for_as_long_as_the_guard_lives() {
        let counter = Arc::new(AtomicUsize::new(0));
        let first = Counted::new(&counter);
        let second = Counted::new(&counter);
        assert_eq!(counter.load(Ordering::Relaxed), 2);
        drop(first);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        drop(second);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
