use std::sync::{Arc, Mutex};

// A total that concurrent requests share: what is held right now against a limit,
// so they can't grow it unbounded.
pub struct Budget {
    limit: i64,
    held: Mutex<i64>,
}

impl Budget {
    pub fn new(limit: i64) -> Arc<Budget> {
        Arc::new(Budget { limit, held: Mutex::new(0) })
    }

    pub fn limit(&self) -> i64 {
        self.limit
    }

    pub fn held(&self) -> i64 {
        *self.held.lock().unwrap()
    }

    // Reserves `amount`. The reservation gives it back when it is dropped; None when it doesn't fit.
    pub fn reserve(self: &Arc<Self>, amount: i64) -> Option<Reservation> {
        let mut held = self.held.lock().unwrap();
        if *held + amount > self.limit {
            return None;
        }
        *held += amount;
        Some(Reservation { budget: Arc::clone(self), amount })
    }
}

#[must_use = "a reservation is given back as soon as it is dropped"]
pub struct Reservation {
    budget: Arc<Budget>,
    amount: i64,
}

impl Reservation {
    // Gives back part of it, keeping `amount`
    pub fn shrink_to(&mut self, amount: i64) {
        if amount < self.amount {
            *self.budget.held.lock().unwrap() -= self.amount - amount;
            self.amount = amount;
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        *self.budget.held.lock().unwrap() -= self.amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_up_to_the_limit_and_gives_back_on_drop() {
        let budget = Budget::new(300);
        let first = budget.reserve(100).unwrap();
        assert_eq!(budget.held(), 100);
        assert!(budget.reserve(250).is_none(), "over what is left");
        let second = budget.reserve(200).unwrap();
        assert_eq!(budget.held(), 300);
        drop(first);
        assert_eq!(budget.held(), 200);
        drop(second);
        assert_eq!(budget.held(), 0);
    }

    #[test]
    fn can_give_back_part_of_a_reservation() {
        let budget = Budget::new(100);
        let mut reservation = budget.reserve(80).unwrap();
        reservation.shrink_to(30);
        assert_eq!(budget.held(), 30);
        reservation.shrink_to(50); // never grows
        assert_eq!(budget.held(), 30);
        drop(reservation);
        assert_eq!(budget.held(), 0);
    }
}
