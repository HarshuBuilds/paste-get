// SPDX-License-Identifier: GPL-3.0-or-later

//! Limits on the connections the daemon accepts before it knows they come from a paired device:
//! those in their TLS handshake, and those of a device that isn't paired (pairing, or being
//! refused). Anyone on the network can open them; without limits, a device could keep thousands
//! open and use up the daemon's sockets and memory.
//!
//! - At most [`PER_IP`] at once from one IP address: one more is closed at once.
//! - At most [`AT_ONCE`] in all. One more makes another close ([`Ticket::closed`]): the oldest of
//!   the address that holds the most places, so that a device flooding from a few addresses only
//!   closes its own connections. A connection from an address a paired device connected from
//!   lately ([`Ticket::proven_paired`]) is never closed for one from another address: when only
//!   those are left, the newcomer is refused instead.
//!
//! So a paired device gets in during a flood, even one that never pauses, if it connects from an
//! address a paired device connected from since the daemon started, or if the flood comes from
//! fewer addresses than there are places. Each connection lasts a few seconds at most anyway (`daemon.rs`,
//! `UNKNOWN_DEADLINE`). It is no longer counted once its TLS handshake proved it comes from a
//! paired device.

use std::cmp::Reverse;
use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::oneshot;

/// Connections not known to come from a paired device, at once.
pub const AT_ONCE: usize = 64;
/// The same, from one IP address.
pub const PER_IP: usize = 8;
/// Addresses paired devices connected from that are remembered, the latest.
pub const PAIRED_ADDRESSES: usize = 16;

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
    /// Where paired devices connected from, latest first, in memory only.
    paired: VecDeque<IpAddr>,
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
    ip: IpAddr,
    closed: oneshot::Receiver<()>,
}

impl Admission {
    /// Lets a connection from `ip` in, or `None` if [`PER_IP`] from it are in already. When
    /// [`AT_ONCE`] are in, another is told to close, to make room (see the module's doc), or
    /// `None` if none may be.
    pub fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<Ticket> {
        let ip = ip.to_canonical();
        let mut state = lock(&self.state);
        if state.places(ip) >= PER_IP {
            return None;
        }
        if state.admitted.len() >= AT_ONCE {
            let closed = state
                .to_close(ip)
                .and_then(|index| state.admitted.remove(index))?;
            let _ = closed.close.send(());
        }
        let number = state.next;
        state.next += 1;
        let (close, closed) = oneshot::channel();
        state.admitted.push_back(Admitted { number, ip, close });
        Some(Ticket {
            admission: Arc::clone(self),
            number,
            ip,
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

impl State {
    /// How many places `ip` holds.
    fn places(&self, ip: IpAddr) -> usize {
        self.admitted.iter().filter(|a| a.ip == ip).count()
    }

    /// Which connection to close to let one from `newcomer` in: the oldest of the address that
    /// holds the most places, the newcomer's counted. Those of addresses paired devices
    /// connected from go last, and only for a newcomer from such an address too: `None` if none
    /// may close.
    fn to_close(&self, newcomer: IpAddr) -> Option<usize> {
        let paired = |ip: IpAddr| self.paired.contains(&ip);
        let places = |ip: IpAddr| self.places(ip) + usize::from(ip == newcomer);
        let (index, ip) = self
            .admitted
            .iter()
            .enumerate()
            .map(|(index, admitted)| (index, admitted.ip))
            .min_by_key(|&(index, ip)| (paired(ip), Reverse(places(ip)), index))?;
        (paired(newcomer) || !paired(ip)).then_some(index)
    }
}

impl Ticket {
    /// Resolves once the connection must close to make room for a newer one.
    pub async fn closed(&mut self) {
        // An error means the same: its place is gone.
        let _ = (&mut self.closed).await;
    }

    /// The TLS handshake proved the connection comes from a paired device: it is no longer
    /// counted, and its address is remembered, so that connections from there go last.
    pub fn proven_paired(self) {
        let mut state = lock(&self.admission.state);
        state.paired.retain(|ip| *ip != self.ip);
        state.paired.push_front(self.ip);
        state.paired.truncate(PAIRED_ADDRESSES);
        drop(state);
        // Dropping the ticket frees its place.
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

    /// Whether `ticket` was told to close, without waiting.
    fn told(ticket: &mut Ticket) -> bool {
        !matches!(
            ticket.closed.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        )
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

    #[test]
    fn when_full_the_address_holding_the_most_gives_way() {
        let admission = Arc::new(Admission::default());
        // The oldest, alone from its address.
        let mut phone = admission.admit(ip("192.168.1.20")).unwrap();
        let mut held: Vec<Ticket> = (1..AT_ONCE)
            .map(|n| {
                admission
                    .admit(ip(&format!("10.0.0.{}", n / PER_IP)))
                    .unwrap()
            })
            .collect();
        assert_eq!(admission.len(), AT_ONCE);

        assert!(admission.admit(ip("10.0.0.99")).is_some());

        // 10.0.0.0 held 7 places, 10.0.0.1 the first to hold 8: its oldest gave way.
        assert!(!told(&mut phone));
        let closed: Vec<usize> = (0..held.len()).filter(|&n| told(&mut held[n])).collect();
        assert_eq!(closed, [PER_IP - 1]);
    }

    /// A device on the network keeps reconnecting from 8 addresses, without pause: each new
    /// connection closes one of its own, never the phone's, slow as its TLS handshake may be.
    #[test]
    fn a_flood_that_never_pauses_closes_only_its_own_connections() {
        let admission = Arc::new(Admission::default());
        let mut phone = admission.admit(ip("192.168.1.20")).unwrap();
        let mut flood = Vec::new();

        for n in 0..10_000 {
            if let Some(ticket) = admission.admit(ip(&format!("192.168.1.{}", 100 + n % 8))) {
                flood.push(ticket);
            }
            // Those told to close do, which gives their address a place again.
            flood.retain_mut(|ticket| !told(ticket));
            assert!(!told(&mut phone), "closed after {n} connections");
        }
        assert_eq!(admission.len(), AT_ONCE);
    }

    /// A flood from more addresses than there are places, one connection each: a connection
    /// from an address a paired device used is never closed for them.
    #[test]
    fn places_are_kept_for_addresses_paired_devices_connected_from() {
        let admission = Arc::new(Admission::default());
        admission.admit(ip("192.168.1.20")).unwrap().proven_paired();
        assert!(admission.is_empty());
        let mut phone = admission.admit(ip("::ffff:192.168.1.20")).unwrap();

        let mut flood = Vec::new();
        for n in 0..1000 {
            let address = format!("10.0.{}.{}", n / 250, n % 250);
            flood.push(admission.admit(ip(&address)).unwrap());
            flood.retain_mut(|ticket| !told(ticket));
            assert!(!told(&mut phone), "closed after {n} connections");
        }
        assert_eq!(admission.len(), AT_ONCE);
    }

    #[test]
    fn when_only_paired_addresses_are_in_others_wait() {
        let admission = Arc::new(Admission::default());
        let paired = |n: usize| ip(&format!("192.168.1.{}", 20 + n));
        for n in 0..=AT_ONCE / PER_IP {
            admission.admit(paired(n)).unwrap().proven_paired();
        }
        let mut held: Vec<Ticket> = (0..AT_ONCE)
            .map(|n| admission.admit(paired(n / PER_IP)).unwrap())
            .collect();

        // Another address is refused, and nobody is told to close.
        assert!(admission.admit(ip("10.0.0.2")).is_none());
        assert!(!held.iter_mut().any(told));
        // A paired device's address still gets in: the oldest of the others makes room.
        assert!(admission.admit(paired(AT_ONCE / PER_IP)).is_some());
        assert!(told(&mut held[0]));
        assert!(!held[1..].iter_mut().any(told));
    }

    #[test]
    fn only_the_latest_paired_addresses_are_remembered() {
        let admission = Arc::new(Admission::default());
        for n in 0..=PAIRED_ADDRESSES {
            admission
                .admit(ip(&format!("192.168.1.{n}")))
                .unwrap()
                .proven_paired();
        }
        // Proven again: the latest once more.
        admission.admit(ip("192.168.1.1")).unwrap().proven_paired();

        let paired = lock(&admission.state).paired.clone();
        assert_eq!(paired.len(), PAIRED_ADDRESSES);
        assert_eq!(paired[0], ip("192.168.1.1"));
        assert!(!paired.contains(&ip("192.168.1.0")));
        assert!(paired.contains(&ip(&format!("192.168.1.{PAIRED_ADDRESSES}"))));
    }
}
