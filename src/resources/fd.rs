//! The file descriptors that `/fd` opens and holds, against the budget `MAX_FD`.
use crate::resources::budget::{Budget, OverBudget};
use crate::system::errno_name;
use std::fs::File;
use std::sync::Arc;
use std::time::Duration;

pub struct Fd {
    budget: Arc<Budget>,
}

/// How many descriptors could be opened, and why the others could not
#[derive(Debug, PartialEq)]
pub struct Opened {
    pub opened: usize,
    pub failure: Option<String>,
}

impl Fd {
    pub fn new(limit: i64) -> Fd {
        Fd { budget: Budget::new(limit) }
    }

    pub fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }

    /// Opens `count` descriptors and closes them after `seconds`. When the limit of the process is
    /// lower the ones that could be opened stay open until then: the server then can't accept
    /// connections either, which is the case being tested.
    pub fn hold(&self, count: i64, seconds: u64) -> Result<Opened, OverBudget> {
        let mut reservation = self.budget.reserve(count)?;

        let mut files = Vec::with_capacity(count as usize);
        let mut failure = None;
        for _ in 0..count {
            match File::open("/dev/null") {
                Ok(file) => files.push(file),
                Err(e) => {
                    failure = Some(errno_name(&e));
                    break;
                }
            }
        }
        // what was reserved but not opened goes back
        reservation.shrink_to(files.len() as i64);
        let opened = files.len();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            drop(files);
            drop(reservation);
        });
        Ok(Opened { opened, failure })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn it_holds_descriptors_for_a_while() {
        let fd = Fd::new(50);
        assert_eq!(fd.hold(30, 2).unwrap(), Opened { opened: 30, failure: None });
        assert_eq!(fd.budget().held(), 30);

        tokio::time::sleep(Duration::from_millis(2100)).await;
        assert_eq!(fd.budget().held(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn it_refuses_what_does_not_fit() {
        let fd = Fd::new(50);
        fd.hold(30, 2).unwrap();
        assert_eq!(fd.hold(30, 2).err(), Some(OverBudget { held: 30, limit: 50 }));
    }
}
