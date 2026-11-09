// SPDX-License-Identifier: GPL-3.0-or-later

//! Pairing a new device by scanning a QR code. See `docs/PROTOCOL.md`, section 5.
//!
//! 1. The accepting device (the PC) opens a [`PairingWindow`] and shows its [`PairingInvite`]
//!    as a QR code: device ID, addresses and a one-time secret.
//! 2. The new device scans it and calls [`pair_with`]: it connects pinned to the device ID from
//!    the QR code, so it can't be tricked into pairing with another machine.
//! 3. It sends the secret inside the TLS connection. [`accept_pairing`] checks it, which proves
//!    the new device could see the QR code.
//!
//! Each side then stores the other's [`PairedDevice`]; later connections are ordinary pinned TLS.
//! A device that forgot the accepting one, which still knows it, can pair again the same way: the
//! accepting side finds its request with [`read_opening`].
//!
//! A QR code can also arrive as a `pasteget://pair` link, which any web page or message can open.
//! So only the addresses of a code that are on the local network are dialed ([`Dial`]): a code
//! never makes a device connect across the Internet, nor a phone pair with another app on it.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, Instant};

use data_encoding::BASE32_NOPAD;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use ring::rand::{SecureRandom, SystemRandom};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;
use zeroize::Zeroizing;

use crate::identity::{DeviceId, Identity};
use crate::proto::{Envelope, Hello, PairRequest, PairResponse, envelope::Payload};
use crate::session::HANDSHAKE_TIMEOUT;
use crate::transport::{self, Connection, TransportError, UnverifiedConnection};

/// How long a pairing QR code stays valid.
pub const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);

/// Longest device name kept, in characters.
pub const MAX_DEVICE_NAME_CHARS: usize = 64;

/// Longest invite URI accepted, in bytes.
pub const MAX_INVITE_LEN: usize = 1024;

const SECRET_LEN: usize = 16;
const INVITE_PREFIX: &str = "pasteget://pair?";

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("could not generate a random secret")]
    Random,
    #[error("the pairing code has expired")]
    Expired,
    #[error("the pairing code was already used")]
    Closed,
    #[error("wrong pairing code")]
    WrongSecret,
    #[error("the other device refused to pair")]
    Rejected,
    /// It refused the connection: its pairing code expired, was used or was closed.
    #[error("the other device is not showing this pairing code any more: show a new one")]
    NoLongerShown,
    #[error("the other device broke the pairing protocol")]
    Protocol,
}

/// The one-time secret shown in the pairing QR code.
#[derive(Clone)]
pub struct PairingSecret(Zeroizing<[u8; SECRET_LEN]>);

impl PairingSecret {
    fn generate() -> Result<Self, PairingError> {
        let mut bytes = Zeroizing::new([0; SECRET_LEN]);
        SystemRandom::new()
            .fill(bytes.as_mut())
            .map_err(|_| PairingError::Random)?;
        Ok(Self(bytes))
    }

    fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; SECRET_LEN] = bytes.try_into().ok()?;
        Some(Self(Zeroizing::new(array)))
    }

    /// Constant-time comparison, so response timing reveals nothing about the secret.
    fn matches(&self, candidate: &[u8]) -> bool {
        candidate.ct_eq(self.0.as_slice()).into()
    }
}

impl fmt::Debug for PairingSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingSecret(..)")
    }
}

/// Everything the QR code carries.
#[derive(Clone, Debug)]
pub struct PairingInvite {
    pub device_id: DeviceId,
    pub device_name: String,
    /// Where the inviting device listens, likeliest first. The new device tries them all at
    /// once ([`pair_with_any_address`]).
    pub addresses: Vec<SocketAddr>,
    pub secret: PairingSecret,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a valid paste-get pairing code")]
pub struct InvalidInvite;

impl PairingInvite {
    /// The URI to encode in the QR code.
    pub fn to_uri(&self) -> String {
        let addresses = self
            .addresses
            .iter()
            .map(SocketAddr::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("v", "0")
            .append_pair("id", &self.device_id.to_string())
            .append_pair("s", &BASE32_NOPAD.encode(self.secret.0.as_slice()))
            .append_pair("a", &addresses)
            .append_pair("n", &self.device_name)
            .finish();
        format!("{INVITE_PREFIX}{query}")
    }
}

impl FromStr for PairingInvite {
    type Err = InvalidInvite;

    /// Parses a scanned QR code. Input is untrusted (fuzzed in `fuzz/`).
    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        if uri.len() > MAX_INVITE_LEN {
            return Err(InvalidInvite);
        }
        let query = uri.strip_prefix(INVITE_PREFIX).ok_or(InvalidInvite)?;

        let (mut version, mut id, mut secret, mut addresses, mut name) =
            (None, None, None, None, None);
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            let slot = match key.as_ref() {
                "v" => &mut version,
                "id" => &mut id,
                "s" => &mut secret,
                "a" => &mut addresses,
                "n" => &mut name,
                _ => continue, // Unknown keys are reserved for newer versions.
            };
            if slot.replace(value).is_some() {
                return Err(InvalidInvite); // Duplicate key.
            }
        }

        if version.as_deref() != Some("0") {
            return Err(InvalidInvite);
        }
        let device_id = id
            .ok_or(InvalidInvite)?
            .parse()
            .map_err(|_| InvalidInvite)?;
        let secret_bytes = Zeroizing::new(
            BASE32_NOPAD
                .decode(secret.ok_or(InvalidInvite)?.as_bytes())
                .map_err(|_| InvalidInvite)?,
        );
        let secret = PairingSecret::from_slice(&secret_bytes).ok_or(InvalidInvite)?;
        let addresses = addresses
            .ok_or(InvalidInvite)?
            .split(',')
            .map(SocketAddr::from_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| InvalidInvite)?;

        Ok(Self {
            device_id,
            device_name: sanitize_device_name(&name.unwrap_or_default()),
            addresses,
            secret,
        })
    }
}

/// Whether `ip` is on a local network: private IPv4 (10/8, 172.16/12, 192.168/16), link-local
/// (169.254/16, fe80::/10) or IPv6 unique local (fc00::/7). Not public addresses, nor the
/// carrier and VPN range 100.64/10, loopback, multicast, broadcast or unspecified ones.
pub fn on_local_network(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

/// Which addresses of a pairing code [`pair_with_any_address`] pairs through: those on the local
/// network ([`on_local_network`]), and this device's own ones only where they make sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dial {
    /// Also this device's own addresses: a computer may pair with another paste-get running on
    /// it (another profile).
    LocalNetwork,
    /// Not this device's own addresses: on a phone, what listens there is another app, which
    /// pairing would let read every copy. Loopback ones aren't dialed; a connection that turns
    /// out to reach this device through another of its addresses (its Wi-Fi or hotspot address,
    /// say) is closed before anything is sent.
    OtherDevicesOnly,
}

impl Dial {
    /// Why `address` isn't dialed, if it isn't.
    fn refuses(self, address: SocketAddr) -> Option<AttemptError> {
        let ip = address.ip().to_canonical();
        if ip.is_loopback() {
            return (self == Self::OtherDevicesOnly).then_some(AttemptError::ThisDevice);
        }
        (!on_local_network(ip)).then_some(AttemptError::NotLocal)
    }

    pub fn allows(self, address: SocketAddr) -> bool {
        self.refuses(address).is_none()
    }
}

/// A device both sides have agreed to trust. The caller stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedDevice {
    pub device_id: DeviceId,
    pub device_name: String,
}

/// A pairing QR code being shown on the accepting device.
#[derive(Debug)]
pub struct PairingWindow {
    secret: PairingSecret,
    expires_at: Instant,
    closed: bool,
}

impl PairingWindow {
    /// Opens a window that lasts [`PAIRING_TTL`].
    pub fn open(now: Instant) -> Result<Self, PairingError> {
        Ok(Self {
            secret: PairingSecret::generate()?,
            expires_at: now + PAIRING_TTL,
            closed: false,
        })
    }

    /// The invite to show as a QR code.
    pub fn invite(
        &self,
        identity: &Identity,
        device_name: &str,
        addresses: Vec<SocketAddr>,
    ) -> PairingInvite {
        PairingInvite {
            device_id: identity.device_id(),
            device_name: sanitize_device_name(device_name),
            addresses,
            secret: self.secret.clone(),
        }
    }

    /// Whether new pairing attempts can still succeed.
    pub fn is_open(&self, now: Instant) -> bool {
        !self.closed && now < self.expires_at
    }

    /// Checks a secret. A match closes the window: each code pairs one device.
    ///
    /// Wrong secrets don't close it: anyone on the network can send them, and could otherwise
    /// close every window as it opens. Guessing a 128-bit secret is hopeless however many tries
    /// the 5 minutes allow.
    pub fn check(&mut self, candidate: &[u8], now: Instant) -> Result<(), PairingError> {
        if self.closed {
            return Err(PairingError::Closed);
        }
        if now >= self.expires_at {
            return Err(PairingError::Expired);
        }
        if self.secret.matches(candidate) {
            self.closed = true;
            return Ok(());
        }
        Err(PairingError::WrongSecret)
    }
}

/// Why pairing through one address of a pairing code failed.
#[derive(Debug)]
pub enum AttemptError {
    /// Not dialed: the address isn't on the local network ([`Dial`]).
    NotLocal,
    /// Not paired through: the address is this device's own ([`Dial::OtherDevicesOnly`]).
    ThisDevice,
    /// Nothing answered within [`HANDSHAKE_TIMEOUT`].
    TimedOut,
    /// No connection: refused, unreachable…
    Connect(std::io::Error),
    /// Something answered, and pairing failed.
    Pairing(PairingError),
}

impl fmt::Display for AttemptError {
    /// A few words: it goes in a list of every address tried.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLocal => f.write_str("not on the local network, not tried"),
            Self::ThisDevice => f.write_str("an address of this device, not tried"),
            Self::TimedOut => write!(f, "no answer in {} s", HANDSHAKE_TIMEOUT.as_secs()),
            Self::Connect(e) | Self::Pairing(PairingError::Transport(TransportError::Io(e))) => {
                // The TLS handshake pins the key of the device in the code.
                let wrong_key = e
                    .get_ref()
                    .and_then(|inner| inner.downcast_ref::<rustls::Error>())
                    .is_some_and(|tls| matches!(tls, rustls::Error::InvalidCertificate(_)));
                if wrong_key {
                    f.write_str("a different device answered")
                } else {
                    f.write_str(&short_io_reason(e))
                }
            }
            Self::Pairing(e) => e.fmt(f),
        }
    }
}

/// "connection refused" rather than "Connection refused (os error 111)".
fn short_io_reason(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match e.kind() {
        kind @ (ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::NetworkDown
        | ErrorKind::TimedOut
        | ErrorKind::AddrNotAvailable) => kind.to_string(),
        _ => e.to_string(),
    }
}

/// Pairing failed through every address of a pairing code.
#[derive(Debug)]
pub struct NoAddressWorked {
    /// Each address, in the code's order, and why it failed.
    pub attempts: Vec<(SocketAddr, AttemptError)>,
}

impl NoAddressWorked {
    /// Whether the device itself answered through one of them (and refused), rather than none
    /// reaching it.
    pub fn reached_the_device(&self) -> bool {
        self.attempts.iter().any(|(_, error)| {
            matches!(error, AttemptError::Pairing(e) if !matches!(e, PairingError::Transport(_)))
        })
    }
}

impl fmt::Display for NoAddressWorked {
    /// "192.168.1.20:47290: connection refused; 10.0.0.2:47290: no answer in 10 s".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.attempts.is_empty() {
            return f.write_str("the pairing code has no address");
        }
        for (i, (address, error)) in self.attempts.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{address}: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for NoAddressWorked {}

/// New-device side: pairs with the device from `invite`, through the first of its addresses
/// that works. Returns that address too.
///
/// Only the addresses `dial` allows are dialed; the others fail at once. Those are tried all at
/// once, each for at most [`HANDSHAKE_TIMEOUT`]; the first to pair wins and the others are
/// dropped. That is safe because a code pairs one device once: two attempts that both reach the
/// device can't both succeed, so none that succeeded is dropped.
pub async fn pair_with_any_address(
    identity: &Identity,
    invite: &PairingInvite,
    device_name: &str,
    dial: Dial,
) -> Result<(PairedDevice, SocketAddr), NoAddressWorked> {
    let mut failed = Vec::new();
    let mut allowed = Vec::new();
    for (index, &address) in invite.addresses.iter().enumerate() {
        match dial.refuses(address) {
            Some(refused) => failed.push((index, address, refused)),
            None => allowed.push((index, address)),
        }
    }
    let mut attempts: FuturesUnordered<_> = allowed
        .into_iter()
        .map(|(index, address)| async move {
            let attempt = pair_through(address, dial, identity, invite, device_name);
            let result = timeout(HANDSHAKE_TIMEOUT, attempt)
                .await
                .unwrap_or(Err(AttemptError::TimedOut));
            (index, address, result)
        })
        .collect();

    while let Some((index, address, result)) = attempts.next().await {
        match result {
            Ok(paired) => return Ok((paired, address)),
            Err(e) => failed.push((index, address, e)),
        }
    }
    failed.sort_by_key(|(index, ..)| *index);
    Err(NoAddressWorked {
        attempts: failed.into_iter().map(|(_, a, e)| (a, e)).collect(),
    })
}

/// Connects to `address`, one that `dial` allows, and pairs through it.
async fn pair_through(
    address: SocketAddr,
    dial: Dial,
    identity: &Identity,
    invite: &PairingInvite,
    device_name: &str,
) -> Result<PairedDevice, AttemptError> {
    let socket = TcpStream::connect(address)
        .await
        .map_err(AttemptError::Connect)?;
    if dial == Dial::OtherDevicesOnly
        && connected_to_itself(&socket).map_err(AttemptError::Connect)?
    {
        return Err(AttemptError::ThisDevice);
    }
    pair_with(socket, identity, invite, device_name)
        .await
        .map_err(AttemptError::Pairing)
}

/// Whether `socket` reached this device: a connection to one of its own addresses (its Wi-Fi
/// address, say) comes from that same address.
fn connected_to_itself(socket: &TcpStream) -> std::io::Result<bool> {
    let (local, peer) = (socket.local_addr()?, socket.peer_addr()?);
    Ok(local.ip().to_canonical() == peer.ip().to_canonical())
}

/// New-device side: connects to the device from `invite` over `io` and pairs.
///
/// `io` must be connected to one of `invite.addresses`.
pub async fn pair_with<IO>(
    io: IO,
    identity: &Identity,
    invite: &PairingInvite,
    device_name: &str,
) -> Result<PairedDevice, PairingError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    let mut connection = transport::connect(io, identity, invite.device_id).await?;
    connection
        .send(&Envelope {
            payload: Some(Payload::PairRequest(PairRequest {
                secret: invite.secret.0.to_vec(),
                device_name: sanitize_device_name(device_name),
            })),
        })
        .await?;

    let response = connection.recv().await?;
    // The pairing result doesn't depend on a clean shutdown.
    let _ = connection.close().await;
    match response {
        Some(Envelope {
            payload:
                Some(Payload::PairResponse(PairResponse {
                    accepted: true,
                    device_name,
                })),
        }) => Ok(PairedDevice {
            device_id: invite.device_id,
            device_name: sanitize_device_name(&device_name),
        }),
        Some(Envelope {
            payload:
                Some(Payload::PairResponse(PairResponse {
                    accepted: false, ..
                })),
        }) => Err(PairingError::Rejected),
        // Its window closed: it refuses us as any device it doesn't know.
        Some(Envelope {
            payload: Some(Payload::Refusal(_)),
        }) => Err(PairingError::NoLongerShown),
        _ => Err(PairingError::Protocol),
    }
}

/// Accepting side: handles a connection from a device that is not paired yet, checking its
/// secret against `window`.
///
/// Call it only for peers that failed [`UnverifiedConnection::authorize`] while `window` is open.
pub async fn accept_pairing<S>(
    connection: UnverifiedConnection<S>,
    window: &mut PairingWindow,
    device_name: &str,
) -> Result<PairedDevice, PairingError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    read_request(connection)
        .await?
        .answer(|secret| window.check(secret, Instant::now()), device_name)
        .await
}

/// Accepting side: reads the pairing request of a device that is not paired yet. Anything else
/// as its first message is a protocol error.
///
/// Call it only for peers that failed [`UnverifiedConnection::authorize`] while a pairing window
/// is open.
pub async fn read_request<S>(
    connection: UnverifiedConnection<S>,
) -> Result<PairingRequest<S>, PairingError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut connection = connection.into_pairing();
    match connection.recv().await? {
        Some(Envelope {
            payload: Some(Payload::PairRequest(request)),
        }) => Ok(PairingRequest::new(connection, request)),
        _ => Err(PairingError::Protocol),
    }
}

/// What a paired device sends first to the accepting side.
pub enum Opening<S> {
    /// Its Hello, with the name sanitized: answer with [`crate::session::answer_session`].
    Hello(Connection<S>, Hello),
    /// A pairing request: the device forgot this one and scanned its pairing code again.
    PairRequest(PairingRequest<S>),
}

/// Accepting side: reads the first message of a paired device, before sending anything.
pub async fn read_opening<S>(mut connection: Connection<S>) -> Result<Opening<S>, TransportError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match connection.recv().await? {
        Some(Envelope {
            payload: Some(Payload::Hello(mut hello)),
        }) => {
            // The version is checked once ours is sent: a newer device needs it to talk down.
            hello.device_name = sanitize_device_name(&hello.device_name);
            Ok(Opening::Hello(connection, hello))
        }
        Some(Envelope {
            payload: Some(Payload::PairRequest(request)),
        }) => Ok(Opening::PairRequest(PairingRequest::new(
            connection, request,
        ))),
        Some(_) => Err(TransportError::UnexpectedMessage { expected: "Hello" }),
        None => Err(TransportError::Closed),
    }
}

/// A pairing request a device sent, not answered yet.
pub struct PairingRequest<S> {
    connection: Connection<S>,
    secret: Zeroizing<Vec<u8>>,
    device_name: String,
}

impl<S> PairingRequest<S> {
    fn new(connection: Connection<S>, request: PairRequest) -> Self {
        Self {
            connection,
            secret: Zeroizing::new(request.secret),
            device_name: sanitize_device_name(&request.device_name),
        }
    }

    /// The device ID the device proved it owns.
    pub fn peer(&self) -> DeviceId {
        self.connection.peer()
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> PairingRequest<S> {
    /// Checks the secret with `check` (a [`PairingWindow::check`]), tells the device whether it
    /// is accepted, and closes the connection. `check` runs before anything is sent, so a lock
    /// it takes is never held across network I/O.
    pub async fn answer(
        mut self,
        check: impl FnOnce(&[u8]) -> Result<(), PairingError>,
        device_name: &str,
    ) -> Result<PairedDevice, PairingError> {
        let verdict = check(&self.secret);
        self.connection
            .send(&Envelope {
                payload: Some(Payload::PairResponse(PairResponse {
                    accepted: verdict.is_ok(),
                    device_name: sanitize_device_name(device_name),
                })),
            })
            .await?;
        let peer = self.connection.peer();
        let _ = self.connection.close().await;

        verdict.map(|()| PairedDevice {
            device_id: peer,
            device_name: self.device_name,
        })
    }
}

/// Removes control characters, trims, and caps the length of a name received from a peer.
pub fn sanitize_device_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Unknown device".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::io::duplex;
    use tokio::net::TcpListener;

    fn sample_invite() -> (Identity, PairingWindow, PairingInvite) {
        let pc = Identity::generate().unwrap();
        let window = PairingWindow::open(Instant::now()).unwrap();
        let addresses = vec![
            "192.168.1.20:47290".parse().unwrap(),
            "[fe80::1]:47290".parse().unwrap(),
        ];
        let invite = window.invite(&pc, "HP-AIO (Arch)", addresses);
        (pc, window, invite)
    }

    #[test]
    fn invite_uri_round_trips() {
        let (_, _, mut invite) = sample_invite();
        invite.device_name = "Bureau d'été & co".into();

        let uri = invite.to_uri();
        let parsed: PairingInvite = uri.parse().unwrap();

        assert!(uri.starts_with("pasteget://pair?v=0&"));
        assert_eq!(parsed.device_id, invite.device_id);
        assert_eq!(parsed.device_name, invite.device_name);
        assert_eq!(parsed.addresses, invite.addresses);
        assert!(parsed.secret.matches(invite.secret.0.as_slice()));
    }

    #[test]
    fn invite_ignores_unknown_keys() {
        let uri = format!("{}&future=1", sample_invite().2.to_uri());

        assert!(uri.parse::<PairingInvite>().is_ok());
    }

    #[test]
    fn rejects_malformed_invites() {
        let valid = sample_invite().2.to_uri();
        let malformed = [
            String::new(),
            "https://example.com/?v=0".into(),
            valid.replace("v=0", "v=1"),
            valid.replacen("id=", "id=X", 1),
            valid.replace("&s=", "&x="),
            valid.replace("47290", "port"),
            format!("{valid}&v=0"),
            format!("{valid}&pad={}", "a".repeat(MAX_INVITE_LEN)),
        ];

        for uri in malformed {
            assert_eq!(
                uri.parse::<PairingInvite>().err(),
                Some(InvalidInvite),
                "{uri}"
            );
        }
    }

    #[test]
    fn window_accepts_the_secret_once() {
        let now = Instant::now();
        let mut window = PairingWindow::open(now).unwrap();
        let secret = window.secret.clone();

        assert!(window.check(secret.0.as_slice(), now).is_ok());
        assert!(matches!(
            window.check(secret.0.as_slice(), now),
            Err(PairingError::Closed)
        ));
        assert!(!window.is_open(now));
    }

    #[test]
    fn window_expires() {
        let now = Instant::now();
        let mut window = PairingWindow::open(now).unwrap();
        let secret = window.secret.clone();
        let later = now + PAIRING_TTL;

        assert!(!window.is_open(later));
        assert!(matches!(
            window.check(secret.0.as_slice(), later),
            Err(PairingError::Expired)
        ));
    }

    #[test]
    fn wrong_secrets_never_close_the_window() {
        let now = Instant::now();
        let mut window = PairingWindow::open(now).unwrap();
        let secret = window.secret.clone();

        // However many junk requests come in, as fast as a script sends them, the right secret
        // still pairs.
        for n in 0..10_000_u32 {
            let mut wrong = [0; SECRET_LEN];
            wrong[..4].copy_from_slice(&n.to_le_bytes());
            assert!(matches!(
                window.check(&wrong, now),
                Err(PairingError::WrongSecret)
            ));
        }
        assert!(window.is_open(now));
        assert!(window.check(secret.0.as_slice(), now).is_ok());
    }

    #[test]
    fn secret_never_shows_in_debug_output() {
        let (_, window, invite) = sample_invite();
        let encoded = BASE32_NOPAD.encode(invite.secret.0.as_slice());

        assert!(!format!("{window:?} {invite:?}").contains(&encoded));
        assert!(!format!("{:?}", invite.secret).contains(&format!("{:?}", &invite.secret.0[..])));
    }

    #[test]
    fn sanitizes_device_names() {
        assert_eq!(sanitize_device_name("\u{7}Evil\nName  "), "EvilName");
        assert_eq!(sanitize_device_name(" \t"), "Unknown device");
        assert_eq!(
            sanitize_device_name(&"x".repeat(200)).chars().count(),
            MAX_DEVICE_NAME_CHARS
        );
    }

    #[tokio::test]
    async fn devices_pair_then_reconnect_as_trusted() {
        let (pc, mut window, invite) = sample_invite();
        let phone = Identity::generate().unwrap();
        let (phone_io, pc_io) = duplex(64 * 1024);

        let (phone_result, pc_result) =
            tokio::join!(pair_with(phone_io, &phone, &invite, "POCO M8 5G"), async {
                let incoming = transport::accept(pc_io, &pc).await?;
                accept_pairing(incoming, &mut window, "HP-AIO (Arch)").await
            });
        let paired_pc = phone_result.unwrap();
        let paired_phone = pc_result.unwrap();

        assert_eq!(paired_pc.device_id, pc.device_id());
        assert_eq!(paired_pc.device_name, "HP-AIO (Arch)");
        assert_eq!(paired_phone.device_id, phone.device_id());
        assert_eq!(paired_phone.device_name, "POCO M8 5G");

        // Afterwards: ordinary pinned TLS, authorized from the stored pairing.
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (client, server) = tokio::join!(
            transport::connect(phone_io, &phone, paired_pc.device_id),
            async {
                transport::accept(pc_io, &pc)
                    .await?
                    .authorize(|id| *id == paired_phone.device_id)
            }
        );
        assert!(client.is_ok());
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn a_paired_device_can_pair_again_or_start_a_session() {
        use crate::proto::{PROTOCOL_VERSION, Platform};
        let (pc, mut window, invite) = sample_invite();
        let phone = Identity::generate().unwrap();
        let accept_paired = |io| {
            let pc = &pc;
            async move {
                let incoming = transport::accept(io, pc).await?;
                let connection = incoming.authorize(|_| true)?;
                read_opening(connection).await
            }
        };

        // A device that forgot the PC sends a pairing request as its first message.
        let (phone_io, pc_io) = duplex(64 * 1024);
        let (phone_result, pc_result) =
            tokio::join!(pair_with(phone_io, &phone, &invite, "POCO M8 5G"), async {
                let Opening::PairRequest(request) = accept_paired(pc_io).await? else {
                    panic!("expected a pairing request");
                };
                assert_eq!(request.peer(), phone.device_id());
                request
                    .answer(|secret| window.check(secret, Instant::now()), "PC")
                    .await
            });
        assert_eq!(pc_result.unwrap().device_name, "POCO M8 5G");
        assert_eq!(phone_result.unwrap().device_name, "PC");

        // Any other device sends its Hello, read before the PC sends anything.
        let (phone_io, pc_io) = duplex(64 * 1024);
        let ours = Hello {
            protocol_version: PROTOCOL_VERSION,
            device_name: "POCO\u{7}".into(),
            app_version: "test".into(),
            platform: Platform::Android.into(),
        };
        let (_phone, opening) = tokio::join!(
            async {
                let mut connection = transport::connect(phone_io, &phone, pc.device_id()).await?;
                connection
                    .send(&Envelope {
                        payload: Some(Payload::Hello(ours)),
                    })
                    .await?;
                Ok::<_, TransportError>(connection)
            },
            accept_paired(pc_io)
        );
        let Ok(Opening::Hello(_, hello)) = opening else {
            panic!("expected a Hello");
        };
        assert_eq!(hello.device_name, "POCO");
    }

    /// A port nothing listens on.
    async fn closed_port() -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        listener.local_addr().unwrap()
    }

    /// Listens as `identity` and answers one pairing attempt from `window`.
    async fn pairing_server(
        identity: Identity,
        mut window: PairingWindow,
    ) -> (
        SocketAddr,
        tokio::task::JoinHandle<Result<PairedDevice, PairingError>>,
    ) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.map_err(TransportError::from)?;
            let incoming = transport::accept(socket, &identity).await?;
            accept_pairing(incoming, &mut window, "PC").await
        });
        (address, server)
    }

    #[tokio::test]
    async fn pairs_through_whichever_address_works() {
        let (pc, window, mut invite) = sample_invite();
        let pc_id = pc.device_id();
        let (pc_address, pc_side) = pairing_server(pc, window).await;
        let (impostor, _) = pairing_server(
            Identity::generate().unwrap(),
            PairingWindow::open(Instant::now()).unwrap(),
        )
        .await;
        invite.addresses = vec![closed_port().await, impostor, pc_address];
        let phone = Identity::generate().unwrap();

        let (paired, address) = pair_with_any_address(&phone, &invite, "POCO", Dial::LocalNetwork)
            .await
            .unwrap();

        assert_eq!(address, pc_address);
        assert_eq!(paired.device_id, pc_id);
        assert_eq!(pc_side.await.unwrap().unwrap().device_name, "POCO");
    }

    #[tokio::test]
    async fn says_why_each_address_failed() {
        let (pc, window, mut invite) = sample_invite();
        let (pc_address, _) = pairing_server(pc, window).await;
        let (impostor, _) = pairing_server(
            Identity::generate().unwrap(),
            PairingWindow::open(Instant::now()).unwrap(),
        )
        .await;
        let closed = closed_port().await;
        invite.addresses = vec![closed, impostor];
        let phone = Identity::generate().unwrap();

        let unreachable = pair_with_any_address(&phone, &invite, "POCO", Dial::LocalNetwork)
            .await
            .unwrap_err();
        assert_eq!(
            unreachable.to_string(),
            format!("{closed}: connection refused; {impostor}: a different device answered")
        );
        assert!(!unreachable.reached_the_device());

        // The PC itself answers, and refuses: a stale code.
        invite.addresses = vec![pc_address];
        invite.secret = PairingSecret::generate().unwrap();
        let refused = pair_with_any_address(&phone, &invite, "POCO", Dial::LocalNetwork)
            .await
            .unwrap_err();
        assert_eq!(
            refused.to_string(),
            format!("{pc_address}: the other device refused to pair")
        );
        assert!(refused.reached_the_device());
    }

    #[test]
    fn only_local_network_addresses_are_dialed() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        for local in [
            "10.0.0.2",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.20",
            "169.254.10.1",
            "fd12:3456::1",
            "fe80::1",
            "::ffff:192.168.1.20",
        ] {
            assert!(on_local_network(ip(local)), "{local}");
        }
        for elsewhere in [
            "203.0.113.9",
            "8.8.8.8",
            "172.32.0.1",
            "100.64.0.7",
            "100.127.255.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.251",
            "127.0.0.1",
            "2001:db8::1",
            "2a00:1450::1",
            "ff02::fb",
            "::",
            "::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!on_local_network(ip(elsewhere)), "{elsewhere}");
        }

        let address = |s: &str| s.parse::<SocketAddr>().unwrap();
        // Loopback: a computer may pair with another profile on itself, a phone never.
        for this_device in ["127.0.0.1:47290", "[::1]:47290", "[::ffff:127.0.0.1]:47290"] {
            assert!(Dial::LocalNetwork.allows(address(this_device)));
            assert!(!Dial::OtherDevicesOnly.allows(address(this_device)));
        }
        assert!(Dial::OtherDevicesOnly.allows(address("192.168.1.20:47290")));
        assert!(!Dial::LocalNetwork.allows(address("203.0.113.9:47290")));
    }

    /// An app on a phone can listen on all of its addresses and open a pairing link with one of
    /// them: the phone doesn't pair through its Wi-Fi address any more than through loopback.
    #[tokio::test]
    async fn a_phone_never_pairs_through_an_address_of_its_own() {
        let own = if_addrs::get_if_addrs()
            .unwrap()
            .into_iter()
            .map(|interface| interface.ip())
            .find(|ip| ip.is_ipv4() && !ip.is_loopback());
        let Some(own) = own else {
            eprintln!("skipped: this machine has no address but loopback");
            return;
        };
        let (app, mut window, invite) = sample_invite();
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.unwrap();
        let address = SocketAddr::new(own, listener.local_addr().unwrap().port());
        let app_side = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                // The phone's first try ends before TLS.
                if let Ok(incoming) = transport::accept(socket, &app).await {
                    return accept_pairing(incoming, &mut window, "HP-AIO").await;
                }
            }
        });
        let phone = Identity::generate().unwrap();

        let refused = pair_through(address, Dial::OtherDevicesOnly, &phone, &invite, "POCO").await;
        assert!(
            matches!(refused, Err(AttemptError::ThisDevice)),
            "{refused:?}"
        );

        // Only that check stopped it: a computer pairs through the same address.
        let paired = pair_through(address, Dial::LocalNetwork, &phone, &invite, "POCO").await;
        assert_eq!(paired.unwrap().device_name, "HP-AIO");
        assert_eq!(app_side.await.unwrap().unwrap().device_name, "POCO");
    }

    /// A link from a web page names a server on the Internet: it's refused without being dialed
    /// (dialing 203.0.113.9, which nothing answers, would take the full timeout).
    #[tokio::test]
    async fn a_code_for_a_server_on_the_internet_is_not_dialed() {
        let (_, _, mut invite) = sample_invite();
        let loopback = closed_port().await;
        invite.addresses = vec![
            "203.0.113.9:47290".parse().unwrap(),
            "100.64.0.7:47290".parse().unwrap(),
            loopback,
        ];
        let phone = Identity::generate().unwrap();

        let refused = timeout(
            Duration::from_secs(1),
            pair_with_any_address(&phone, &invite, "POCO", Dial::OtherDevicesOnly),
        )
        .await
        .expect("an address was dialed")
        .unwrap_err();

        assert_eq!(
            refused.to_string(),
            format!(
                "203.0.113.9:47290: not on the local network, not tried; \
                 100.64.0.7:47290: not on the local network, not tried; \
                 {loopback}: an address of this device, not tried"
            )
        );
        assert!(!refused.reached_the_device());
    }

    #[test]
    fn failed_addresses_read_well() {
        let failed = NoAddressWorked {
            attempts: vec![
                (
                    "192.168.1.20:47290".parse().unwrap(),
                    AttemptError::TimedOut,
                ),
                (
                    "10.0.0.2:47290".parse().unwrap(),
                    AttemptError::Connect(std::io::ErrorKind::NetworkUnreachable.into()),
                ),
            ],
        };

        assert_eq!(
            failed.to_string(),
            "192.168.1.20:47290: no answer in 10 s; 10.0.0.2:47290: network unreachable"
        );
        let none = NoAddressWorked { attempts: vec![] };
        assert_eq!(none.to_string(), "the pairing code has no address");
    }

    #[tokio::test]
    async fn a_code_no_longer_shown_says_so() {
        let (pc, _, invite) = sample_invite();
        let phone = Identity::generate().unwrap();
        let (phone_io, pc_io) = duplex(64 * 1024);

        let (phone_result, refused) =
            tokio::join!(pair_with(phone_io, &phone, &invite, "POCO M8 5G"), async {
                transport::accept(pc_io, &pc)
                    .await?
                    .refuse(crate::proto::RefusalReason::NotPaired)
                    .await
            });

        refused.unwrap();
        assert!(matches!(phone_result, Err(PairingError::NoLongerShown)));
    }

    #[tokio::test]
    async fn wrong_secret_is_rejected() {
        let (pc, mut window, mut invite) = sample_invite();
        invite.secret = PairingSecret::generate().unwrap();
        let phone = Identity::generate().unwrap();
        let (phone_io, pc_io) = duplex(64 * 1024);

        let (phone_result, pc_result) =
            tokio::join!(pair_with(phone_io, &phone, &invite, "POCO M8 5G"), async {
                let incoming = transport::accept(pc_io, &pc).await?;
                accept_pairing(incoming, &mut window, "HP-AIO (Arch)").await
            });

        assert!(matches!(phone_result, Err(PairingError::Rejected)));
        assert!(matches!(pc_result, Err(PairingError::WrongSecret)));
    }

    #[tokio::test]
    async fn refuses_a_machine_other_than_the_one_in_the_qr_code() {
        let (_pc, _, invite) = sample_invite();
        let impostor = Identity::generate().unwrap();
        let mut impostor_window = PairingWindow::open(Instant::now()).unwrap();
        let phone = Identity::generate().unwrap();
        let (phone_io, impostor_io) = duplex(64 * 1024);

        let (phone_result, impostor_result) =
            tokio::join!(pair_with(phone_io, &phone, &invite, "POCO M8 5G"), async {
                let incoming = transport::accept(impostor_io, &impostor).await?;
                accept_pairing(incoming, &mut impostor_window, "Impostor").await
            });

        assert!(matches!(phone_result, Err(PairingError::Transport(_))));
        assert!(impostor_result.is_err());
    }
}
