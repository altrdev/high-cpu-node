//! The TCP connections that `/connections` opens to the server itself and holds, against the budget
//! `MAX_CONNECTIONS`.
use crate::resources::budget::{Budget, OverBudget};
use crate::system::errno_name;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

pub struct Connections {
    budget: Arc<Budget>,
    port: u16,
}

/// How many connections could be opened, and why the others could not
#[derive(Debug, PartialEq)]
pub struct Opened {
    pub opened: usize,
    pub failure: Option<String>,
}

impl Connections {
    pub fn new(limit: i64, port: u16) -> Connections {
        Connections { budget: Budget::new(limit), port }
    }

    pub fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }

    /// Opens `count` connections to the port of the server and closes them after `seconds`. Each one
    /// takes a descriptor in the client side and another in the server side, so a process whose limit
    /// of descriptors is lower stops earlier: the ones that could be opened stay open until then.
    pub async fn hold(&self, count: i64, seconds: u64) -> Result<Opened, OverBudget> {
        let mut reservation = self.budget.reserve(count)?;

        let mut streams = Vec::with_capacity(count as usize);
        let mut failure = None;
        for _ in 0..count {
            match TcpStream::connect(("127.0.0.1", self.port)).await {
                Ok(stream) => streams.push(stream),
                Err(e) => {
                    failure = Some(errno_name(&e));
                    break;
                }
            }
        }
        // what was reserved but not opened goes back
        reservation.shrink_to(streams.len() as i64);
        let opened = streams.len();

        let ending = self.budget.ends_in(seconds);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            drop(ending);
            drop(streams);
            drop(reservation);
        });
        Ok(Opened { opened, failure })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    // a port that accepts connections, kept for as long as the listener lives
    async fn listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test(start_paused = true)]
    async fn it_holds_connections_for_a_while() {
        let (_listener, port) = listener().await;
        let connections = Connections::new(50, port);
        assert_eq!(connections.hold(30, 2).await.unwrap(), Opened { opened: 30, failure: None });
        assert_eq!(connections.budget().held(), 30);

        tokio::time::sleep(Duration::from_millis(2100)).await;
        assert_eq!(connections.budget().held(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn it_refuses_what_does_not_fit() {
        let (_listener, port) = listener().await;
        let connections = Connections::new(50, port);
        connections.hold(30, 2).await.unwrap();
        assert_eq!(connections.hold(30, 2).await.err(), Some(OverBudget { held: 30, limit: 50 }));
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_held_when_nobody_listens() {
        let (listener, port) = listener().await;
        drop(listener);
        let connections = Connections::new(50, port);
        let opened = connections.hold(5, 2).await.unwrap();
        assert_eq!(opened.opened, 0);
        assert!(opened.failure.is_some_and(|failure| failure.contains("refused")), "the reason is reported");
        assert_eq!(connections.budget().held(), 0, "what could not be opened goes back");
    }
}
