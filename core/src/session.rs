// SPDX-License-Identifier: GPL-3.0-or-later

//! Sessions with connected devices, shared by the desktop daemon and the phone app.
//!
//! A session starts with the [`Hello`] exchange, then runs two halves until the connection ends:
//! - a writer task sending queued clips, acks and pings (every [`PING_INTERVAL`]);
//! - a reader loop answering pings, reassembling clips and handing complete ones to
//!   [`SessionEvents`]. [`IDLE_TIMEOUT`] of silence ends the session.
//!
//! Each clip sent is timed from the moment it was queued to the device's [`Ack`]: that is what
//! [`SessionEvents::clip_delivered`] reports, to measure latency.
//!
//! [`Peers`] holds the outgoing queue of every connected device, so the app can send to all of
//! them, and the last clip each paired device hasn't acknowledged, to send again when it
//! reconnects. What to do with received clips (clipboard, relay, notification) is up to the app.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until, timeout};

use crate::clip::{self, Clip, ClipReceiver, Received};
use crate::identity::{DeviceId, Identity};
use crate::proto::envelope::Payload;
use crate::proto::{Ack, AckStatus, Envelope, Hello, Ping, Pong, Unpaired};
use crate::transport::{self, Connection, EnvelopeReader, EnvelopeWriter, TransportError};

/// Time allowed for the TLS handshake, and separately for the Hello exchange.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Every session sends a ping this often.
pub const PING_INTERVAL: Duration = Duration::from_secs(30);
/// A session that hears nothing for this long is considered dead.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Delay between reconnection attempts, doubling from the minimum to the maximum.
pub const RECONNECT_MIN: Duration = Duration::from_secs(1);
pub const RECONNECT_MAX: Duration = Duration::from_secs(30);
/// Clips waiting to be written to one device. Beyond that, new clips skip that device.
pub const OUTGOING_QUEUE: usize = 16;
/// Sent clips still waiting for their [`Ack`]. Beyond that, the oldest is no longer timed.
const AWAITING_ACK: usize = 32;
/// How long [`Peers::unpair`] waits for room in a full queue to say [`Unpaired`].
const FAREWELL_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("{0} timed out")]
    Timeout(&'static str),
    #[error("device broke the protocol: {0}")]
    Protocol(&'static str),
    #[error("the session was closed")]
    Closed,
    /// This device unpaired the other ([`Peers::unpair`]) and told it so.
    #[error("this device unpaired the other one")]
    Unpaired,
    /// The other device unpaired this one and said so ([`Unpaired`]).
    #[error("the device unpaired this one")]
    UnpairedByPeer,
}

/// What a session reports to the app. Called from session tasks: keep it quick.
pub trait SessionEvents: Send + Sync {
    /// [`keep_connected`] reached `peer` at `address`: the TLS handshake proved its identity.
    /// The place to remember a working address.
    fn reached(&self, _peer: DeviceId, _address: SocketAddr) {}

    fn connected(&self, _peer: DeviceId, _hello: &Hello) {}

    /// The device is no longer connected, also after a cancelled session. Not reported when a
    /// newer session with the same device already replaced this one.
    fn disconnected(&self, _peer: DeviceId) {}

    /// A complete clip whose hash and type were verified.
    fn clip_received(&self, peer: DeviceId, clip: Clip);

    /// `peer` acknowledged a clip this side sent.
    fn clip_delivered(&self, _peer: DeviceId, _delivery: &Delivery) {}

    /// `peer` unpaired this device: it refused [`keep_connected`], or said so in a session
    /// ([`Unpaired`]), which then ended, after [`SessionEvents::disconnected`]. Its dialer, if any,
    /// has stopped. The place to forget it and tell the user.
    fn unpaired_by(&self, _peer: DeviceId) {}
}

/// A clip the device acknowledged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub mime_type: String,
    pub len: usize,
    /// `Ok`, or why the device refused it.
    pub status: AckStatus,
    /// From the moment the clip was queued for this device to its acknowledgement.
    pub elapsed: Duration,
}

/// The devices currently connected, and their outgoing queues.
///
/// Lock order: `queues`, then `waiting`.
#[derive(Debug, Default)]
pub struct Peers {
    queues: Mutex<HashMap<DeviceId, PeerQueue>>,
    /// The paired devices, connected or not ([`Peers::expect`]), and the last clip each one
    /// hasn't acknowledged yet.
    waiting: Mutex<HashMap<DeviceId, Option<Numbered>>>,
    next_session: AtomicU64,
    next_clip: AtomicU64,
}

/// A broadcast clip, numbered so that the device's ack can be matched with the clip waiting for
/// it. Shared by every queue and every device it waits for.
#[derive(Clone, Debug)]
struct Numbered {
    number: u64,
    clip: Arc<Clip>,
}

#[derive(Debug)]
struct PeerQueue {
    session: u64,
    sender: mpsc::Sender<Outgoing>,
    /// Ends the session, and says how.
    kick: watch::Sender<Option<Ending>>,
}

/// Why this side ends a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    /// [`Peers::disconnect`].
    Disconnect,
    /// [`Peers::unpair`]: the device is told first.
    Unpair,
}

#[derive(Debug)]
enum Outgoing {
    Clip { clip: Numbered, queued_at: Instant },
    Envelope(Envelope),
}

/// A clip written to the device and not acknowledged yet.
#[derive(Debug)]
struct Sent {
    mime_type: String,
    len: usize,
    queued_at: Instant,
    /// Its [`Numbered::number`].
    number: u64,
}

/// Shared by a session's writer, which adds sent clips, and its reader, which matches acks.
type AwaitingAck = Arc<Mutex<HashMap<Vec<u8>, Sent>>>;

impl Peers {
    pub fn new() -> Self {
        Self::default()
    }

    /// A paired device: the last clip it hasn't acknowledged waits for it (in memory only) and is
    /// sent again when it connects. Call it for every paired device, and after pairing.
    pub fn expect(&self, peer: DeviceId) {
        lock(&self.waiting).entry(peer).or_insert(None);
    }

    /// The device was unpaired: nothing waits for it any more.
    pub fn unexpect(&self, peer: &DeviceId) {
        lock(&self.waiting).remove(peer);
    }

    /// Queues `clip` for every connected device except `except`. Returns how many.
    ///
    /// Until it acknowledges the clip, a paired device gets it again when it connects, if it's
    /// still the last clip by then: one that isn't connected, and one whose session is dead but
    /// hasn't timed out yet (a PC asleep, a phone that just left the Wi-Fi), which queues it for
    /// nothing. Secrets don't wait: a password or a code shouldn't turn up hours later.
    pub fn broadcast(&self, clip: &Clip, except: Option<DeviceId>) -> usize {
        let clip = Numbered {
            number: self.next_clip.fetch_add(1, Ordering::Relaxed),
            clip: Arc::new(clip.clone()),
        };
        // Held throughout, so that a session starting meanwhile gets the clip exactly once:
        // queued, or waiting for it.
        let queues = self.lock();
        if !clip.clip.sensitive {
            for (id, unacknowledged) in lock(&self.waiting).iter_mut() {
                if Some(*id) != except && *id != clip.clip.origin {
                    *unacknowledged = Some(clip.clone());
                }
            }
        }
        let mut queued = 0;
        for (id, queue) in queues.iter() {
            if Some(*id) == except {
                continue;
            }
            let outgoing = Outgoing::Clip {
                clip: clip.clone(),
                queued_at: Instant::now(),
            };
            match queue.sender.try_send(outgoing) {
                Ok(()) => queued += 1,
                Err(_) => tracing::warn!(peer = %id, "device is not keeping up, clip skipped"),
            }
        }
        queued
    }

    pub fn connected(&self) -> Vec<DeviceId> {
        self.lock().keys().copied().collect()
    }

    pub fn is_connected(&self, peer: &DeviceId) -> bool {
        self.lock().contains_key(peer)
    }

    /// Ends the session with `peer`, if one runs, without saying why (it paired again, say).
    /// Returns whether a session was ended.
    pub fn disconnect(&self, peer: &DeviceId) -> bool {
        self.end(peer, Ending::Disconnect)
    }

    /// Tells `peer` it was unpaired ([`Unpaired`]) and ends the session with it, if one runs:
    /// call it after unpairing the device, so that it forgets this one too, at once. Returns
    /// whether a session runs; it then ends with [`SessionError::Unpaired`], which also stops
    /// [`keep_connected`].
    pub fn unpair(&self, peer: &DeviceId) -> bool {
        self.end(peer, Ending::Unpair)
    }

    fn end(&self, peer: &DeviceId, ending: Ending) -> bool {
        match self.lock().get(peer) {
            Some(queue) => {
                // Kept if the session isn't waiting yet.
                queue.kick.send_replace(Some(ending));
                true
            }
            None => false,
        }
    }

    fn register(
        &self,
        peer: DeviceId,
        sender: mpsc::Sender<Outgoing>,
    ) -> (u64, watch::Receiver<Option<Ending>>) {
        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        let (kick, kicked) = watch::channel(None);
        let mut queues = self.lock();
        // What it missed goes first. It keeps waiting until the device acknowledges it, in case
        // this session dies too.
        let missed = lock(&self.waiting).get(&peer).cloned().flatten();
        if let Some(missed) = missed {
            tracing::info!(%peer, mime_type = %missed.clip.mime_type, len = missed.clip.data.len(), "sending the clip it missed");
            let _ = sender.try_send(Outgoing::Clip {
                clip: missed,
                queued_at: Instant::now(),
            });
        }
        // A newer session replaces an older one that hasn't noticed it's dead yet.
        queues.insert(
            peer,
            PeerQueue {
                session,
                sender,
                kick,
            },
        );
        (session, kicked)
    }

    /// `peer` acknowledged the clip `number`: if it's the one waiting for it, it waits no more.
    fn acknowledged(&self, peer: &DeviceId, number: u64) {
        if let Some(waiting) = lock(&self.waiting).get_mut(peer)
            && waiting.as_ref().is_some_and(|w| w.number == number)
        {
            *waiting = None;
        }
    }

    /// Forgets `session`. Returns `false` if a newer session with the device replaced it.
    fn unregister(&self, peer: DeviceId, session: u64) -> bool {
        let mut queues = self.lock();
        let current = queues.get(&peer).is_some_and(|q| q.session == session);
        if current {
            queues.remove(&peer);
        }
        current
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<DeviceId, PeerQueue>> {
        // Poisoning means a panic elsewhere; the map itself is still consistent.
        self.queues.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Runs one session until the connection ends. Returns `Ok` when the peer closed it cleanly.
pub async fn run_session<S>(
    mut connection: Connection<S>,
    hello: Hello,
    peers: &Peers,
    events: &dyn SessionEvents,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let their_hello = timeout(HANDSHAKE_TIMEOUT, connection.exchange_hello(hello))
        .await
        .map_err(|_| SessionError::Timeout("Hello exchange"))??;
    run(connection, their_hello, peers, events).await
}

/// Like [`run_session`], for the accepting side once it has read the device's Hello
/// ([`crate::pairing::read_opening`]): answers with `hello`, checks the device's version, then
/// runs the session.
pub async fn answer_session<S>(
    mut connection: Connection<S>,
    their_hello: Hello,
    hello: Hello,
    peers: &Peers,
    events: &dyn SessionEvents,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let ours = Envelope {
        payload: Some(Payload::Hello(hello)),
    };
    timeout(HANDSHAKE_TIMEOUT, connection.send(&ours))
        .await
        .map_err(|_| SessionError::Timeout("Hello exchange"))??;
    let their_hello = transport::checked_hello(their_hello)?;
    run(connection, their_hello, peers, events).await
}

/// A session once both Hellos went through.
async fn run<S>(
    connection: Connection<S>,
    their_hello: Hello,
    peers: &Peers,
    events: &dyn SessionEvents,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let peer = connection.peer();
    let (reader, writer) = connection.into_split();
    let (sender, queue) = mpsc::channel(OUTGOING_QUEUE);
    let awaiting = AwaitingAck::default();
    let (session, mut kick) = peers.register(peer, sender.clone());
    let mut guard = SessionGuard {
        peers,
        events,
        peer,
        session,
        was_current: None,
        writer: Some(tokio::spawn(write_loop(
            writer,
            queue,
            Arc::clone(&awaiting),
        ))),
    };
    events.connected(peer, &their_hello);

    let result = tokio::select! {
        result = read_loop(peer, reader, &sender, &awaiting, peers, events) => result,
        ending = kicked(&mut kick) => match ending {
            Ending::Disconnect => {
                tracing::info!(%peer, "session ended by this device");
                Err(SessionError::Closed)
            }
            Ending::Unpair => {
                tracing::info!(%peer, "session ended: this device unpaired it");
                // The writer sends what's queued before it closes the connection.
                let farewell = reply(&sender, Payload::Unpaired(Unpaired {}));
                let _ = timeout(FAREWELL_TIMEOUT, farewell).await;
                Err(SessionError::Unpaired)
            }
        }
    };

    // Graceful end: once no queue sender is left, the writer flushes and closes the connection.
    guard.unregister();
    drop(sender);
    if let Some(mut writer) = guard.writer.take()
        && timeout(Duration::from_secs(1), &mut writer).await.is_err()
    {
        tracing::debug!(%peer, "writer did not stop in time");
        writer.abort();
    }
    // Reports the disconnection first.
    drop(guard);
    if matches!(result, Err(SessionError::UnpairedByPeer)) {
        tracing::info!(%peer, "the device unpaired this one");
        events.unpaired_by(peer);
    }
    result
}

/// Resolves once the session is kicked; never if its queue was replaced by a newer session's.
async fn kicked(kick: &mut watch::Receiver<Option<Ending>>) -> Ending {
    let ending = kick
        .wait_for(Option::is_some)
        .await
        .map(|ending| ending.unwrap_or(Ending::Disconnect));
    match ending {
        Ok(ending) => ending,
        Err(_) => std::future::pending().await,
    }
}

/// Tears a session down even if `run_session` is cancelled: forgets the device, stops the
/// writer task (which would otherwise keep the connection alive with its pings) and reports the
/// disconnection.
struct SessionGuard<'a> {
    peers: &'a Peers,
    events: &'a dyn SessionEvents,
    peer: DeviceId,
    session: u64,
    /// Set once unregistered: whether this was still the device's current session.
    was_current: Option<bool>,
    writer: Option<tokio::task::JoinHandle<()>>,
}

impl SessionGuard<'_> {
    fn unregister(&mut self) {
        if self.was_current.is_none() {
            self.was_current = Some(self.peers.unregister(self.peer, self.session));
        }
    }
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        self.unregister();
        if let Some(writer) = self.writer.take() {
            writer.abort();
        }
        if self.was_current == Some(true) {
            self.events.disconnected(self.peer);
        }
    }
}

/// Keeps a connection to `peer` open: dials its addresses in turn, runs a session, and redials
/// with backoff when it ends. Stops once either side unpaired the other; otherwise cancel by
/// dropping or aborting the task.
///
/// `addresses` may change while it runs, for example when the device is found at a new address.
/// Every update, even one with the same addresses, also cuts short the wait before the next
/// attempt: it's the hint that the device or the network just came back.
pub async fn keep_connected(
    identity: &Identity,
    peer: DeviceId,
    mut addresses: watch::Receiver<Vec<SocketAddr>>,
    hello: &Hello,
    peers: &Peers,
    events: &dyn SessionEvents,
) {
    let mut delay = RECONNECT_MIN;
    loop {
        let round = Instant::now();
        let current = addresses.borrow_and_update().clone();
        for address in current {
            let connected = timeout(HANDSHAKE_TIMEOUT, async {
                let socket = TcpStream::connect(address)
                    .await
                    .map_err(TransportError::from)?;
                transport::connect(socket, identity, peer).await
            })
            .await;
            match connected {
                Ok(Ok(connection)) => {
                    delay = RECONNECT_MIN;
                    events.reached(peer, address);
                    match run_session(connection, hello.clone(), peers, events).await {
                        Ok(()) => {}
                        // The refusal is genuine: the TLS handshake proved the peer's key.
                        Err(SessionError::Transport(TransportError::NotPaired)) => {
                            tracing::info!(%peer, "the device unpaired this one: no longer dialing it");
                            events.unpaired_by(peer);
                            return;
                        }
                        // Said in the session, as genuine; the session reported it.
                        Err(SessionError::UnpairedByPeer) => {
                            tracing::info!(%peer, "no longer dialing it");
                            return;
                        }
                        // This device unpaired it.
                        Err(SessionError::Unpaired) => return,
                        Err(e) => tracing::info!(%peer, error = %e, "session ended"),
                    }
                    break;
                }
                Ok(Err(e)) => tracing::debug!(%peer, %address, error = %e, "dial failed"),
                Err(_) => tracing::debug!(%peer, %address, "dial timed out"),
            }
        }
        tokio::select! {
            () = sleep(delay) => delay = (delay * 2).min(RECONNECT_MAX),
            // A closed channel disables this branch: then only the backoff applies.
            Ok(()) = addresses.changed() => {
                // At most one round per RECONNECT_MIN, however often hints arrive.
                sleep_until(round + RECONNECT_MIN).await;
                delay = RECONNECT_MIN;
            }
        }
    }
}

async fn read_loop<R: AsyncRead + Unpin>(
    peer: DeviceId,
    mut reader: EnvelopeReader<R>,
    replies: &mpsc::Sender<Outgoing>,
    awaiting: &AwaitingAck,
    peers: &Peers,
    events: &dyn SessionEvents,
) -> Result<(), SessionError> {
    let mut receiver = ClipReceiver::default();
    // When the offer of the clip being received arrived.
    let mut offered_at = Instant::now();
    loop {
        let Some(envelope) = timeout(IDLE_TIMEOUT, reader.recv())
            .await
            .map_err(|_| SessionError::Timeout("waiting for the device"))??
        else {
            return Ok(());
        };
        let received = match envelope.payload {
            Some(Payload::ClipOffer(offer)) => {
                offered_at = Instant::now();
                receiver.on_offer(offer)
            }
            Some(Payload::ClipChunk(chunk)) => receiver.on_chunk(chunk),
            Some(Payload::Ping(Ping { nonce })) => {
                reply(replies, Payload::Pong(Pong { nonce })).await?;
                continue;
            }
            Some(Payload::Ack(ack)) => {
                acknowledged(peer, &ack, awaiting, peers, events);
                continue;
            }
            Some(Payload::Unpaired(Unpaired {})) => return Err(SessionError::UnpairedByPeer),
            // Pongs only prove the peer is alive; unknown payloads come from newer peers.
            Some(Payload::Pong(_)) | None => continue,
            Some(
                Payload::Hello(_)
                | Payload::PairRequest(_)
                | Payload::PairResponse(_)
                | Payload::Refusal(_),
            ) => {
                return Err(SessionError::Protocol("handshake message mid-session"));
            }
        };
        match received {
            Received::Pending => {}
            Received::Rejected { ack } => {
                tracing::warn!(%peer, status = ?ack.status(), "clip from device rejected");
                reply(replies, Payload::Ack(ack)).await?;
            }
            Received::Clip { clip, ack } => {
                reply(replies, Payload::Ack(ack)).await?;
                tracing::info!(
                    %peer,
                    mime_type = %clip.mime_type,
                    len = clip.data.len(),
                    ms = offered_at.elapsed().as_millis(),
                    "clip received"
                );
                events.clip_received(peer, clip);
            }
        }
    }
}

/// Reports the delivery of the clip `ack` answers. Acks for unknown clips are ignored.
fn acknowledged(
    peer: DeviceId,
    ack: &Ack,
    awaiting: &AwaitingAck,
    peers: &Peers,
    events: &dyn SessionEvents,
) {
    let Some(sent) = lock(awaiting).remove(&ack.clip_id) else {
        tracing::debug!(%peer, "ack for a clip that isn't waiting for one");
        return;
    };
    // Accepted or refused, the device has it: sending it again would change nothing.
    peers.acknowledged(&peer, sent.number);
    let delivery = Delivery {
        mime_type: sent.mime_type,
        len: sent.len,
        status: ack.status(),
        elapsed: sent.queued_at.elapsed(),
    };
    if delivery.status == AckStatus::Ok {
        tracing::info!(
            %peer,
            mime_type = %delivery.mime_type,
            len = delivery.len,
            ms = delivery.elapsed.as_millis(),
            "clip delivered"
        );
    } else {
        tracing::warn!(%peer, status = ?delivery.status, "device refused a clip");
    }
    events.clip_delivered(peer, &delivery);
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Poisoning means a panic elsewhere; the data is still consistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn reply(replies: &mpsc::Sender<Outgoing>, payload: Payload) -> Result<(), SessionError> {
    replies
        .send(Outgoing::Envelope(Envelope {
            payload: Some(payload),
        }))
        .await
        .map_err(|_| SessionError::Closed)
}

fn await_ack(awaiting: &AwaitingAck, clip_id: Vec<u8>, clip: &Numbered, queued_at: Instant) {
    let mut awaiting = lock(awaiting);
    if awaiting.len() >= AWAITING_ACK
        && let Some(oldest) = awaiting
            .iter()
            .min_by_key(|(_, sent)| sent.queued_at)
            .map(|(id, _)| id.clone())
    {
        awaiting.remove(&oldest);
    }
    awaiting.insert(
        clip_id,
        Sent {
            mime_type: clip.clip.mime_type.clone(),
            len: clip.clip.data.len(),
            queued_at,
            number: clip.number,
        },
    );
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: EnvelopeWriter<W>,
    mut queue: mpsc::Receiver<Outgoing>,
    awaiting: AwaitingAck,
) {
    let mut ping = interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ping.tick().await;
    let mut nonce = 0u64;

    let result: Result<(), SessionError> = async {
        loop {
            tokio::select! {
                next = queue.recv() => match next {
                    Some(Outgoing::Clip { clip, queued_at }) => {
                        let envelopes = clip::outgoing(&clip.clip)
                            .map_err(|_| SessionError::Protocol("no randomness for a clip ID"))?;
                        for envelope in envelopes {
                            // Recorded before the offer leaves, so its ack can't come first.
                            if let Some(Payload::ClipOffer(offer)) = &envelope.payload {
                                await_ack(&awaiting, offer.clip_id.clone(), &clip, queued_at);
                            }
                            writer.send(&envelope).await?;
                        }
                    }
                    Some(Outgoing::Envelope(envelope)) => writer.send(&envelope).await?,
                    None => return Ok(()),
                },
                _ = ping.tick() => {
                    nonce = nonce.wrapping_add(1);
                    let ping = Envelope { payload: Some(Payload::Ping(Ping { nonce })) };
                    writer.send(&ping).await?;
                }
            }
        }
    }
    .await;
    if let Err(e) = result {
        tracing::debug!(error = %e, "writer stopped");
    }
    let _ = writer.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{PROTOCOL_VERSION, Platform};
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tokio::io::duplex;
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct Recorder {
        clips: Mutex<Vec<(DeviceId, Clip)>>,
        connected: Mutex<Vec<String>>,
        disconnected: Mutex<Vec<DeviceId>>,
        deliveries: Mutex<Vec<(DeviceId, Delivery)>>,
        unpaired_by: Mutex<Vec<DeviceId>>,
    }

    impl SessionEvents for Recorder {
        fn unpaired_by(&self, peer: DeviceId) {
            self.unpaired_by.lock().unwrap().push(peer);
        }

        fn connected(&self, _peer: DeviceId, hello: &Hello) {
            self.connected
                .lock()
                .unwrap()
                .push(hello.device_name.clone());
        }

        fn disconnected(&self, peer: DeviceId) {
            self.disconnected.lock().unwrap().push(peer);
        }

        fn clip_received(&self, peer: DeviceId, clip: Clip) {
            self.clips.lock().unwrap().push((peer, clip));
        }

        fn clip_delivered(&self, peer: DeviceId, delivery: &Delivery) {
            self.deliveries
                .lock()
                .unwrap()
                .push((peer, delivery.clone()));
        }
    }

    fn hello(name: &str) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            device_name: name.into(),
            app_version: "test".into(),
            platform: Platform::Linux.into(),
        }
    }

    async fn until(condition: impl Fn() -> bool) {
        for _ in 0..500 {
            if condition() {
                return;
            }
            sleep(Duration::from_millis(5)).await;
        }
        panic!("condition not met in time");
    }

    #[tokio::test]
    async fn a_device_that_was_unpaired_stops_dialing_and_says_so() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let (_addresses, watched) = watch::channel(vec![listener.local_addr().unwrap()]);
        let pc_id = pc.device_id();
        // The PC no longer knows the phone.
        let pc_side = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let unverified = transport::accept(socket, &pc).await.unwrap();
            unverified
                .refuse(crate::proto::RefusalReason::NotPaired)
                .await
                .unwrap();
        });
        let events = Recorder::default();
        let peers = Peers::new();

        timeout(
            Duration::from_secs(10),
            keep_connected(&phone, pc_id, watched, &hello("phone"), &peers, &events),
        )
        .await
        .expect("keep_connected should stop");

        pc_side.await.unwrap();
        assert_eq!(*events.unpaired_by.lock().unwrap(), [pc_id]);
        assert!(events.connected.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unpaired_device_is_disconnected() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (phone_peers, pc_peers) = (Arc::new(Peers::new()), Arc::new(Peers::new()));
        let pc_events = Arc::new(Recorder::default());
        let phone_session = tokio::spawn({
            let peers = Arc::clone(&phone_peers);
            async move {
                run_session(
                    client.unwrap(),
                    hello("phone"),
                    &peers,
                    &Recorder::default(),
                )
                .await
            }
        });
        let pc_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&pc_peers), Arc::clone(&pc_events));
            async move { run_session(server.unwrap(), hello("pc"), &peers, &*events).await }
        });
        until(|| pc_peers.is_connected(&phone.device_id())).await;

        assert!(pc_peers.disconnect(&phone.device_id()));

        let ended = timeout(Duration::from_secs(5), pc_session)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(ended, Err(SessionError::Closed)));
        assert!(!pc_peers.is_connected(&phone.device_id()));
        assert_eq!(*pc_events.disconnected.lock().unwrap(), [phone.device_id()]);
        // The phone sees the connection close.
        assert!(timeout(Duration::from_secs(5), phone_session).await.is_ok());
        // Nothing left to end.
        assert!(!pc_peers.disconnect(&phone.device_id()));
    }

    #[tokio::test]
    async fn an_unpaired_device_is_told_and_forgets_this_one() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (phone_peers, pc_peers) = (Arc::new(Peers::new()), Arc::new(Peers::new()));
        let (phone_events, pc_events) =
            (Arc::new(Recorder::default()), Arc::new(Recorder::default()));
        let phone_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&phone_peers), Arc::clone(&phone_events));
            async move { run_session(client.unwrap(), hello("phone"), &peers, &*events).await }
        });
        let pc_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&pc_peers), Arc::clone(&pc_events));
            async move { run_session(server.unwrap(), hello("pc"), &peers, &*events).await }
        });
        until(|| {
            phone_peers.is_connected(&pc.device_id()) && pc_peers.is_connected(&phone.device_id())
        })
        .await;

        assert!(pc_peers.unpair(&phone.device_id()));

        let pc_ended = timeout(Duration::from_secs(5), pc_session).await.unwrap();
        assert!(matches!(pc_ended.unwrap(), Err(SessionError::Unpaired)));
        let phone_ended = timeout(Duration::from_secs(5), phone_session)
            .await
            .unwrap();
        assert!(matches!(
            phone_ended.unwrap(),
            Err(SessionError::UnpairedByPeer)
        ));
        assert_eq!(*phone_events.unpaired_by.lock().unwrap(), [pc.device_id()]);
        assert_eq!(*phone_events.disconnected.lock().unwrap(), [pc.device_id()]);
        // The side that unpaired isn't told it was.
        assert!(pc_events.unpaired_by.lock().unwrap().is_empty());
        assert!(!pc_peers.unpair(&phone.device_id()));
    }

    /// Accepts any device on `listener` and runs a session with it.
    fn serve(listener: TcpListener, pc: Identity, peers: Arc<Peers>, events: Arc<Recorder>) {
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let Ok(unverified) = transport::accept(socket, &pc).await else {
                    continue;
                };
                let connection = unverified.authorize(|_| true).unwrap();
                let _ = run_session(connection, hello("pc"), &peers, &*events).await;
            }
        });
    }

    #[tokio::test]
    async fn keep_connected_stops_when_this_side_unpairs() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_id, pc_id) = (phone.device_id(), pc.device_id());
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (_addresses, watched) = watch::channel(vec![listener.local_addr().unwrap()]);
        let (pc_peers, pc_events) = (Arc::new(Peers::new()), Arc::new(Recorder::default()));
        serve(listener, pc, Arc::clone(&pc_peers), Arc::clone(&pc_events));
        let phone_peers = Arc::new(Peers::new());
        let phone_events = Arc::new(Recorder::default());
        let dialer = tokio::spawn({
            let (peers, events) = (Arc::clone(&phone_peers), Arc::clone(&phone_events));
            async move {
                keep_connected(&phone, pc_id, watched, &hello("phone"), &peers, &*events).await;
            }
        });
        until(|| phone_peers.is_connected(&pc_id)).await;

        assert!(phone_peers.unpair(&pc_id));

        timeout(Duration::from_secs(5), dialer)
            .await
            .expect("keep_connected should stop")
            .unwrap();
        until(|| !pc_events.unpaired_by.lock().unwrap().is_empty()).await;
        assert_eq!(*pc_events.unpaired_by.lock().unwrap(), [phone_id]);
        assert!(phone_events.unpaired_by.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn keep_connected_stops_when_the_other_side_unpairs() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_id, pc_id) = (phone.device_id(), pc.device_id());
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (_addresses, watched) = watch::channel(vec![listener.local_addr().unwrap()]);
        let pc_peers = Arc::new(Peers::new());
        serve(listener, pc, Arc::clone(&pc_peers), Arc::default());
        let phone_events = Arc::new(Recorder::default());
        let dialer = tokio::spawn({
            let events = Arc::clone(&phone_events);
            async move {
                let peers = Peers::new();
                keep_connected(&phone, pc_id, watched, &hello("phone"), &peers, &*events).await;
            }
        });
        until(|| pc_peers.is_connected(&phone_id)).await;

        assert!(pc_peers.unpair(&phone_id));

        timeout(Duration::from_secs(5), dialer)
            .await
            .expect("keep_connected should stop")
            .unwrap();
        // Reported once, by the session.
        assert_eq!(*phone_events.unpaired_by.lock().unwrap(), [pc_id]);
    }

    #[tokio::test]
    async fn sessions_deliver_broadcast_clips_both_ways() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (phone_peers, pc_peers) = (Arc::new(Peers::new()), Arc::new(Peers::new()));
        let (phone_events, pc_events) =
            (Arc::new(Recorder::default()), Arc::new(Recorder::default()));

        let phone_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&phone_peers), Arc::clone(&phone_events));
            async move { run_session(client.unwrap(), hello("phone"), &peers, &*events).await }
        });
        let pc_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&pc_peers), Arc::clone(&pc_events));
            async move { run_session(server.unwrap(), hello("pc"), &peers, &*events).await }
        });
        until(|| {
            phone_peers.is_connected(&pc.device_id()) && pc_peers.is_connected(&phone.device_id())
        })
        .await;
        assert_eq!(*pc_events.connected.lock().unwrap(), ["phone"]);

        let text = Clip::text("from the phone", phone.device_id());
        assert_eq!(phone_peers.broadcast(&text, None), 1);
        let image = Clip::png(b"\x89PNG\r\n\x1a\nrest".to_vec(), pc.device_id());
        assert_eq!(pc_peers.broadcast(&image, None), 1);

        until(|| !pc_events.clips.lock().unwrap().is_empty()).await;
        until(|| !phone_events.clips.lock().unwrap().is_empty()).await;
        assert_eq!(
            pc_events.clips.lock().unwrap()[0],
            (phone.device_id(), text)
        );
        assert_eq!(
            phone_events.clips.lock().unwrap()[0],
            (pc.device_id(), image)
        );

        // Each sender hears that its clip was delivered, and how long it took.
        until(|| !phone_events.deliveries.lock().unwrap().is_empty()).await;
        let (to, delivery) = phone_events.deliveries.lock().unwrap()[0].clone();
        assert_eq!(to, pc.device_id());
        assert_eq!(delivery.mime_type, clip::mime::TEXT);
        assert_eq!(delivery.len, "from the phone".len());
        assert_eq!(delivery.status, AckStatus::Ok);
        assert!(delivery.elapsed < Duration::from_secs(5));

        // When one side goes away, the other ends its session and forgets the device.
        phone_session.abort();
        let _ = pc_session.await;
        assert!(pc_peers.connected().is_empty());
    }

    #[tokio::test]
    async fn the_accepting_side_answers_a_hello_it_read_first() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (mut client, server) = (client.unwrap(), server.unwrap());
        let mut newer = hello("phone");
        newer.protocol_version = PROTOCOL_VERSION + 1;
        let peers = Peers::new();

        let (theirs, ended) = tokio::join!(client.exchange_hello(newer), async {
            let Ok(crate::pairing::Opening::Hello(connection, read_first)) =
                crate::pairing::read_opening(server).await
            else {
                panic!("expected a Hello");
            };
            answer_session(
                connection,
                read_first,
                hello("pc"),
                &peers,
                &Recorder::default(),
            )
            .await
        });

        // A newer device still learns this one's version, to talk down if it can.
        assert_eq!(theirs.unwrap().protocol_version, PROTOCOL_VERSION);
        assert!(matches!(
            ended,
            Err(SessionError::Transport(TransportError::VersionMismatch(v))) if v == PROTOCOL_VERSION + 1
        ));
        assert!(peers.connected().is_empty());
    }

    #[tokio::test]
    async fn cancelling_a_session_closes_its_connection() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (phone_peers, pc_peers) = (Arc::new(Peers::new()), Arc::new(Peers::new()));
        let (phone_events, pc_events) =
            (Arc::new(Recorder::default()), Arc::new(Recorder::default()));

        let phone_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&phone_peers), Arc::clone(&phone_events));
            async move { run_session(client.unwrap(), hello("phone"), &peers, &*events).await }
        });
        let pc_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&pc_peers), Arc::clone(&pc_events));
            async move { run_session(server.unwrap(), hello("pc"), &peers, &*events).await }
        });
        until(|| pc_peers.is_connected(&phone.device_id())).await;

        phone_session.abort();

        assert!(timeout(Duration::from_secs(5), pc_session).await.is_ok());
        assert!(phone_peers.connected().is_empty());
        assert!(pc_peers.connected().is_empty());
        // Both sides report it, the cancelled one included.
        until(|| !phone_events.disconnected.lock().unwrap().is_empty()).await;
        assert_eq!(*phone_events.disconnected.lock().unwrap(), [pc.device_id()]);
        assert_eq!(*pc_events.disconnected.lock().unwrap(), [phone.device_id()]);
    }

    #[tokio::test]
    async fn new_addresses_cut_the_reconnect_wait() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (addresses, watched) = watch::channel(Vec::new());
        let dialer = tokio::spawn(async move {
            let (peers, events) = (Peers::new(), Recorder::default());
            keep_connected(
                &phone,
                pc.device_id(),
                watched,
                &hello("phone"),
                &peers,
                &events,
            )
            .await;
        });

        // With no address to dial, the waits grow: 1 s, 2 s, then 4 s from t = 3 s.
        sleep(Duration::from_millis(3500)).await;
        addresses.send_replace(vec![listener.local_addr().unwrap()]);

        // Without the update, the next attempt would come at t = 7 s, and to no address.
        let dialed = timeout(Duration::from_secs(2), listener.accept()).await;
        dialer.abort();
        assert!(dialed.is_ok(), "the new address was not dialed in time");
    }

    #[tokio::test]
    async fn broadcast_skips_the_excluded_device() {
        let peers = Peers::new();
        let (sender, _queue) = mpsc::channel(1);
        let id = Identity::generate().unwrap().device_id();
        peers.register(id, sender);

        let clip = Clip::text("x", id);

        assert_eq!(peers.broadcast(&clip, Some(id)), 0);
        assert_eq!(peers.broadcast(&clip, None), 1);
        // The queue holds one clip: a slow device skips the next one instead of blocking.
        assert_eq!(peers.broadcast(&clip, None), 0);
    }

    #[tokio::test]
    async fn the_last_clip_waits_for_a_paired_device_that_is_away() {
        let peers = Peers::new();
        let origin = Identity::generate().unwrap().device_id();
        let (pc, stranger) = (
            Identity::generate().unwrap().device_id(),
            Identity::generate().unwrap().device_id(),
        );
        peers.expect(pc);

        // While the PC is off: only the last copy waits, secrets don't.
        assert_eq!(peers.broadcast(&Clip::text("first", origin), None), 0);
        assert_eq!(peers.broadcast(&Clip::text("last", origin), None), 0);
        let mut secret = Clip::text("hunter2", origin);
        secret.sensitive = true;
        peers.broadcast(&secret, None);

        let (sender, mut received) = mpsc::channel(4);
        peers.register(pc, sender);
        let missed = next_clip(&mut received).expect("the waiting clip was not sent");
        assert_eq!(missed.clip.data, b"last");
        assert!(received.try_recv().is_err());

        // Sent again until the PC acknowledges it: that session may have died too.
        let (again, mut received) = mpsc::channel(4);
        peers.register(pc, again);
        assert_eq!(next_clip(&mut received).unwrap().number, missed.number);
        peers.acknowledged(&pc, missed.number);
        let (acknowledged, mut nothing) = mpsc::channel(4);
        peers.register(pc, acknowledged);
        assert!(nothing.try_recv().is_err());

        // A device that isn't paired gets nothing.
        let (other, mut none) = mpsc::channel(4);
        peers.register(stranger, other);
        assert!(none.try_recv().is_err());
    }

    fn next_clip(queue: &mut mpsc::Receiver<Outgoing>) -> Option<Numbered> {
        match queue.try_recv() {
            Ok(Outgoing::Clip { clip, .. }) => Some(clip),
            _ => None,
        }
    }

    /// A PC asleep, or a phone that just left the Wi-Fi: its session looks alive until it times
    /// out, and what's queued for it meanwhile goes nowhere.
    #[tokio::test]
    async fn a_clip_queued_into_a_dead_session_is_sent_again_when_the_device_reconnects() {
        let peers = Peers::new();
        let origin = Identity::generate().unwrap().device_id();
        let pc = Identity::generate().unwrap().device_id();
        peers.expect(pc);
        let (dead, mut lost) = mpsc::channel(4);
        peers.register(pc, dead);

        assert_eq!(peers.broadcast(&Clip::text("copied", origin), None), 1);
        let first = next_clip(&mut lost).unwrap();
        // The device reconnects without having acknowledged it.
        let (sender, mut received) = mpsc::channel(4);
        peers.register(pc, sender);

        let again = next_clip(&mut received).expect("the clip was not sent again");
        assert_eq!(again.number, first.number);
        assert_eq!(again.clip.data, b"copied");
    }

    #[tokio::test]
    async fn only_the_ack_of_the_last_clip_stops_it_waiting() {
        let peers = Peers::new();
        let origin = Identity::generate().unwrap().device_id();
        let pc = Identity::generate().unwrap().device_id();
        peers.expect(pc);
        let (sender, mut queue) = mpsc::channel(4);
        peers.register(pc, sender);
        peers.broadcast(&Clip::text("older", origin), None);
        peers.broadcast(&Clip::text("newer", origin), None);
        let older = next_clip(&mut queue).unwrap();
        let newer = next_clip(&mut queue).unwrap();

        // The older one's ack comes first: the newer one still waits.
        peers.acknowledged(&pc, older.number);
        let (sender, mut received) = mpsc::channel(4);
        peers.register(pc, sender);
        assert_eq!(next_clip(&mut received).unwrap().clip.data, b"newer");

        peers.acknowledged(&pc, newer.number);
        let (sender, mut received) = mpsc::channel(4);
        peers.register(pc, sender);
        assert!(received.try_recv().is_err());
    }

    /// Through real sessions: the device's ack clears what waits for it, so a clip it got is never
    /// sent twice.
    #[tokio::test]
    async fn a_delivered_clip_is_not_sent_again() {
        let (phone, pc) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, pc.device_id()),
            async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
        );
        let (phone_peers, pc_peers) = (Arc::new(Peers::new()), Arc::new(Peers::new()));
        pc_peers.expect(phone.device_id());
        let (phone_events, pc_events) =
            (Arc::new(Recorder::default()), Arc::new(Recorder::default()));
        let _phone_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&phone_peers), Arc::clone(&phone_events));
            async move { run_session(client.unwrap(), hello("phone"), &peers, &*events).await }
        });
        let _pc_session = tokio::spawn({
            let (peers, events) = (Arc::clone(&pc_peers), Arc::clone(&pc_events));
            async move { run_session(server.unwrap(), hello("pc"), &peers, &*events).await }
        });
        until(|| pc_peers.is_connected(&phone.device_id())).await;

        assert_eq!(
            pc_peers.broadcast(&Clip::text("delivered", pc.device_id()), None),
            1
        );
        until(|| !pc_events.deliveries.lock().unwrap().is_empty()).await;

        let (sender, mut received) = mpsc::channel(4);
        pc_peers.register(phone.device_id(), sender);
        assert!(received.try_recv().is_err());
        assert_eq!(phone_events.clips.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_unpaired_device_stops_waiting() {
        let peers = Peers::new();
        let origin = Identity::generate().unwrap().device_id();
        let pc = Identity::generate().unwrap().device_id();
        peers.expect(pc);
        peers.unexpect(&pc);
        peers.broadcast(&Clip::text("later", origin), None);

        let (sender, mut received) = mpsc::channel(4);
        peers.register(pc, sender);
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_stale_session_does_not_unregister_its_replacement() {
        let peers = Peers::new();
        let id = Identity::generate().unwrap().device_id();
        let (old, _a) = mpsc::channel(1);
        let (new, _b) = mpsc::channel(1);

        let (old_session, _) = peers.register(id, old);
        let (new_session, _) = peers.register(id, new);

        assert!(!peers.unregister(id, old_session));
        assert!(peers.is_connected(&id));
        assert!(peers.unregister(id, new_session));
        assert!(!peers.is_connected(&id));
    }
}
