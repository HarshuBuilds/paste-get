// SPDX-License-Identifier: GPL-3.0-or-later

//! Limits on the connections the daemon accepts before it knows they come from a paired device:
//! those in their TLS handshake, and those of a device that isn't paired (pairing, or being
//! refused). Anyone on the network can open them; without limits, a device could keep thousands
//! open and use up the daemon's sockets and memory.
//!
//! - At most [`PER_IP`] at once from one IP address: one more is closed at once.
//! - At most [`AT_ONCE`] in all: one more makes the oldest close ([`Ticket::closed`]), so that
//!   silent connections from many addresses can't keep a paired device out.
//!
//! Each of them lasts a few seconds at most anyway (`daemon.rs`, `UNKNOWN_DEADLINE`). A
//! connection is no longer counted once its TLS handshake proved it comes from a paired device.

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::oneshot;

/// Connections not known to come from a paired device, at once.
pub const AT_ONCE: usize = 64;
/// The same, from one IP address.
pub const PER_IP: usize = 8;

/// The connections let in and not known yet. Shared by the accept loop and the connections.
#[derive(Debug, Default)]
pub struct Admission {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    /// Oldest first.
    admitted: VecDeque<Admitted>,
    next: u64,
}

#[derive(Debug)]
struct Admitted {
    number: u64,
    ip: IpAddr,
    /// Tells the connection to close, to make room.
    close: oneshot::Sender<()>,
}

/// A connection let in. Dropping it frees its place.
#[derive(Debug)]
pub struct Ticket {
    admission: Arc<Admission>,
    number: u64,
    closed: oneshot::Receiver<()>,
}

impl Admission {
    /// Lets a connection from `ip` in, or `None` if [`PER_IP`] from it are in already. When
    /// [`AT_ONCE`] are in, the oldest is told to close, to make room.
    pub fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<Ticket> {
        let ip = ip.to_canonical();
        let mut state = lock(&self.state);
        if state.admitted.iter().filter(|a| a.ip == ip).count() >= PER_IP {
            return None;
        }
        if state.admitted.len() >= AT_ONCE
            && let Some(oldest) = state.admitted.pop_front()
        {
            let _ = oldest.close.send(());
        }
        let number = state.next;
        state.next += 1;
        let (close, closed) = oneshot::channel();
        state.admitted.push_back(Admitted { number, ip, close });
        Some(Ticket {
            admission: Arc::clone(self),
            number,
            closed,
        })
    }

    /// How many are in.
    pub fn len(&self) -> usize {
        lock(&self.state).admitted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Ticket {
    /// Resolves once the connection must close to make room for a newer one.
    pub async fn closed(&mut self) {
        // An error means the same: its place is gone.
        let _ = (&mut self.closed).await;
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        lock(&self.admission.state)
            .admitted
            .retain(|a| a.number != self.number);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Poisoning means a panic elsewhere; the list is still consistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// Whether `ticket` was told to close.
    async fn told_to_close(ticket: &mut Ticket) -> bool {
        tokio::time::timeout(Duration::from_millis(10), ticket.closed())
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn one_address_gets_eight_places() {
        let admission = Arc::new(Admission::default());
        let mut held: Vec<Ticket> = (0..PER_IP)
            .map(|_| admission.admit(ip("192.168.1.66")).unwrap())
            .collect();

        assert!(admission.admit(ip("192.168.1.66")).is_none());
        // The same address written as IPv6.
        assert!(admission.admit(ip("::ffff:192.168.1.66")).is_none());
        // Another address still gets in, and nobody was told to close.
        assert!(admission.admit(ip("192.168.1.20")).is_some());
        for ticket in &mut held {
            assert!(!told_to_close(ticket).await);
        }

        // A place freed is a place for the next one.
        held.pop();
        assert!(admission.admit(ip("192.168.1.66")).is_some());
    }

    #[tokio::test]
    async fn when_full_the_oldest_makes_room() {
        let admission = Arc::new(Admission::default());
        let mut held: Vec<Ticket> = (0..AT_ONCE)
            .map(|n| {
                admission
                    .admit(ip(&format!("10.0.0.{}", n / PER_IP)))
                    .unwrap()
            })
            .collect();
        assert_eq!(admission.len(), AT_ONCE);

        let newest = admission.admit(ip("192.168.1.20"));

        assert!(newest.is_some());
        assert_eq!(admission.len(), AT_ONCE);
        assert!(told_to_close(&mut held[0]).await);
        for ticket in &mut held[1..] {
            assert!(!told_to_close(ticket).await);
        }
        // Places come back as connections end; the oldest had lost its own already.
        drop(held);
        assert_eq!(admission.len(), 1);
        drop(newest);
        assert!(admission.is_empty());
    }
}
