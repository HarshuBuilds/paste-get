// SPDX-License-Identifier: GPL-3.0-or-later

//! The phone side of paste-get, exposed to the Android app through UniFFI.
//!
//! [`PhoneClient`] keeps this phone's identity and the computers it paired with, keeps a
//! session open with each of them (the same `core::session` engine as the desktop), and reports
//! connections and received clips through [`PhoneListener`].
//!
//! The app finds computers on the local network itself (Android's `NsdManager`) and passes what
//! it finds to [`PhoneClient::computer_found`].
//!
//! The clipboard history ([`History`], opened with [`PhoneClient::open_history`]) records what
//! the client copies, sends and receives.
//!
//! Methods that touch the network block: call them off the main thread.

// UniFFI's generated scaffolding is `extern "C"` glue that needs `unsafe`. Our own code here
// uses none.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pasteget_core::clip::{Clip, Kind, mime};
use pasteget_core::echo::EchoGuard;
use pasteget_core::history::{Context, Source};
use pasteget_core::identity::{DeviceId, Identity};
use pasteget_core::pairing::{self, Dial, PairingInvite, sanitize_device_name};
use pasteget_core::proto::{AckStatus, Hello, PROTOCOL_VERSION, Platform};
use pasteget_core::session::{self, Delivery, Peers, SessionEvents, found_first};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::sync::watch;
use tokio::task::JoinHandle;

uniffi::setup_scaffolding!();

pub mod history;
#[cfg(not(windows))]
pub mod wireless_adb;

pub use history::{History, HistorySettings};

const IDENTITY_FILE: &str = "identity.key";
const COMPUTERS_FILE: &str = "computers.json";
/// Addresses remembered per computer, and found addresses tried per announcement.
const MAX_ADDRESSES: usize = 8;
/// Time a session has to tell a computer it was forgotten before its dialer is stopped.
const UNPAIR_FAREWELL: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum PastegetError {
    #[error("this is not a paste-get pairing code")]
    InvalidCode,
    #[error("could not reach the computer: {0}")]
    Unreachable(String),
    #[error("pairing failed: {0}")]
    PairingFailed(String),
    #[error("not a PNG or JPEG image")]
    UnsupportedImage,
    #[error("unknown device")]
    UnknownDevice,
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<std::io::Error> for PastegetError {
    fn from(e: std::io::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

/// A computer this phone paired with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct PairedComputer {
    pub device_id: String,
    /// The name it sent when it last connected (or paired).
    pub name: String,
    /// Socket addresses, e.g. `192.168.1.20:47290`: from the pairing code, then the last one
    /// that worked moves to the front.
    pub addresses: Vec<String>,
    /// When its last connection started or ended (unix ms), whichever came last: while it is
    /// connected, when it connected. `None` until it first connects.
    #[serde(default)]
    #[uniffi(default = None)]
    pub last_seen_ms: Option<i64>,
    /// The system it said it runs when it last connected: "windows", "linux", "macos" or
    /// "android". `None` until it connects, or if it didn't say.
    #[serde(default)]
    #[uniffi(default = None)]
    pub platform: Option<String>,
    /// The paste-get version it said it runs when it last connected, e.g. "0.1.0".
    #[serde(default)]
    #[uniffi(default = None)]
    pub app_version: Option<String>,
}

/// A clip a computer sent, without its contents: [`PhoneClient::last_received`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReceivedClip {
    /// The computer it came from.
    pub device_id: String,
    /// Its name, as [`PhoneClient::paired_computers`] has it.
    pub device_name: String,
    /// The device where it was copied: that computer, or a device paired with it (another
    /// phone, a computer linked with it) that it passed the clip on from.
    pub origin_device_id: String,
    pub mime_type: String,
    pub size: u64,
    pub sensitive: bool,
    /// When it arrived (unix ms).
    pub received_at_ms: i64,
}

/// A clip a computer acknowledged.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClipDelivery {
    pub device_id: String,
    pub mime_type: String,
    pub size: u64,
    /// From sending to the computer's acknowledgement.
    pub millis: u64,
    /// `false` if the computer refused it (too large, unsupported type…).
    pub accepted: bool,
}

/// Implemented by the app to hear what happens. Called from background threads.
#[uniffi::export(with_foreign)]
pub trait PhoneListener: Send + Sync {
    /// When a computer connects, `paired_computers` already has the name it now goes by.
    fn on_connection_changed(&self, device_id: String, connected: bool);

    /// A clip arrived from a computer. `sensitive` clips must stay out of history.
    /// [`PhoneClient::last_received`], called from here, says which computer sent it.
    fn on_clip_received(&self, mime_type: String, data: Vec<u8>, sensitive: bool);

    /// A computer acknowledged a clip sent with `send_text` or `send_image`.
    fn on_clip_delivered(&self, delivery: ClipDelivery);

    /// `computer` unpaired this phone. It is already forgotten here and no longer dialed: tell
    /// the user, who can pair again.
    fn on_unpaired(&self, computer: PairedComputer);
}

#[derive(uniffi::Object)]
pub struct PhoneClient {
    runtime: Runtime,
    state: Arc<State>,
    /// One per paired computer while the client runs.
    dialers: Mutex<HashMap<DeviceId, Dialer>>,
    /// Set by [`PhoneClient::pair_on_loopback_for_tests`] only.
    pair_on_loopback: AtomicBool,
}

/// Keeps a session with one computer.
struct Dialer {
    task: JoinHandle<()>,
    /// Where it dials. Any update also makes it dial now rather than after its backoff.
    addresses: watch::Sender<Vec<SocketAddr>>,
}

struct State {
    identity: Identity,
    device_name: String,
    data_dir: PathBuf,
    computers: Mutex<Vec<PairedComputer>>,
    peers: Peers,
    guard: Mutex<EchoGuard>,
    listener: Mutex<Option<Arc<dyn PhoneListener>>>,
    /// Held while the app hears of a received clip, so that it hears of them one at a time and
    /// `last_received` describes the clip it is hearing of.
    reporting_clip: Mutex<()>,
    last_received: Mutex<Option<ReceivedClip>>,
    /// Where clips are recorded, once the app opened it.
    history: Mutex<Option<Arc<History>>>,
}

#[uniffi::export]
impl PhoneClient {
    /// Loads this phone's identity and pairings from `data_dir` (the app's private files
    /// directory), creating the identity on first use. Pairings that can't be read are set
    /// aside (`computers.json.bad`): the phone then starts with none.
    #[uniffi::constructor]
    pub fn new(data_dir: String, device_name: String) -> Result<Arc<Self>, PastegetError> {
        let data_dir = PathBuf::from(data_dir);
        fs::create_dir_all(&data_dir)?;
        let identity = load_or_create_identity(&data_dir)?;
        let computers = load_computers(&data_dir)?;
        // What's copied while a computer is off waits for it.
        let peers = Peers::new();
        for id in computers.iter().filter_map(|c| c.device_id.parse().ok()) {
            peers.expect(id);
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("pasteget")
            .enable_all()
            .build()?;
        Ok(Arc::new(Self {
            runtime,
            state: Arc::new(State {
                guard: Mutex::new(EchoGuard::new(identity.device_id())),
                identity,
                device_name: sanitize_device_name(&device_name),
                data_dir,
                computers: Mutex::new(computers),
                peers,
                listener: Mutex::new(None),
                reporting_clip: Mutex::new(()),
                last_received: Mutex::new(None),
                history: Mutex::new(None),
            }),
            dialers: Mutex::new(HashMap::new()),
            pair_on_loopback: AtomicBool::new(false),
        }))
    }

    pub fn device_id(&self) -> String {
        self.state.identity.device_id().to_string()
    }

    pub fn paired_computers(&self) -> Vec<PairedComputer> {
        lock(&self.state.computers).clone()
    }

    /// Pairs with the computer whose QR code was scanned. Tries all the code's addresses on the
    /// local network at once, never one of this phone (another app listens there), and blocks
    /// for up to ~10 s. The error names each address and why it failed.
    pub fn pair(&self, invite_uri: String) -> Result<PairedComputer, PastegetError> {
        let invite: PairingInvite = invite_uri
            .trim()
            .parse()
            .map_err(|_| PastegetError::InvalidCode)?;
        let dial = if self.pair_on_loopback.load(Ordering::Relaxed) {
            Dial::LocalNetwork
        } else {
            Dial::OtherDevicesOnly
        };
        let (paired, worked) = self
            .runtime
            .block_on(pairing::pair_with_any_address(
                &self.state.identity,
                &invite,
                &self.state.device_name,
                dial,
            ))
            .map_err(|failed| {
                if failed.reached_the_device() {
                    PastegetError::PairingFailed(failed.to_string())
                } else {
                    PastegetError::Unreachable(failed.to_string())
                }
            })?;
        let computer = PairedComputer {
            device_id: paired.device_id.to_string(),
            name: paired.device_name,
            // The address that worked is dialed first, then the others it may dial.
            addresses: found_first(
                [worked],
                invite.addresses.iter().copied().filter(|a| dial.allows(*a)),
            )
            .iter()
            .map(SocketAddr::to_string)
            .collect(),
            last_seen_ms: None,
            platform: None,
            app_version: None,
        };
        {
            let mut computers = lock(&self.state.computers);
            computers.retain(|c| c.device_id != computer.device_id);
            computers.push(computer.clone());
            save_computers(&self.state.data_dir, &computers)?;
        }
        self.state.peers.expect(paired.device_id);
        // Connect right away if the client is running.
        if self.is_running() {
            self.start_dialer(&computer);
        }
        Ok(computer)
    }

    /// Unpairs a computer and drops its connection. If it's connected, it's told, and forgets
    /// this phone too. Doesn't wait for the network.
    pub fn forget(&self, device_id: String) -> Result<(), PastegetError> {
        {
            let mut computers = lock(&self.state.computers);
            let before = computers.len();
            computers.retain(|c| c.device_id != device_id);
            if computers.len() == before {
                return Err(PastegetError::UnknownDevice);
            }
            save_computers(&self.state.data_dir, &computers)?;
        }
        if let Ok(id) = device_id.parse::<DeviceId>() {
            self.state.peers.unexpect(&id);
            let dialer = lock(&self.dialers).remove(&id);
            let told = self.state.peers.unpair(&id);
            if let Some(dialer) = dialer {
                if told {
                    // Its session says so, then ends, and the dialer with it. Stopped anyway in
                    // case the session ended first for another reason.
                    let task = dialer.task;
                    self.runtime.spawn(async move {
                        tokio::time::sleep(UNPAIR_FAREWELL).await;
                        task.abort();
                    });
                } else {
                    dialer.task.abort();
                }
            }
        }
        Ok(())
    }

    /// Connects to every paired computer and keeps reconnecting until [`PhoneClient::stop`].
    pub fn start(&self, listener: Arc<dyn PhoneListener>) {
        *lock(&self.state.listener) = Some(listener);
        let computers = lock(&self.state.computers).clone();
        for computer in &computers {
            self.start_dialer(computer);
        }
    }

    /// Closes every connection.
    pub fn stop(&self) {
        for (_, dialer) in lock(&self.dialers).drain() {
            dialer.task.abort();
        }
        *lock(&self.state.listener) = None;
    }

    /// The computer `device_id` was found on the local network at `ips` and `port`. If it isn't
    /// connected, it's dialed there right away. Returns `false` if it isn't a paired computer.
    ///
    /// Found addresses are only tried: one is remembered once a connection through it proved
    /// the computer's identity, so a forged announcement can't push the real addresses out.
    pub fn computer_found(&self, device_id: String, ips: Vec<String>, port: u16) -> bool {
        let Ok(id) = device_id.parse::<DeviceId>() else {
            return false;
        };
        let Some(known) = lock(&self.state.computers)
            .iter()
            .find(|c| c.device_id == device_id)
            .map(|c| parse_addresses(&c.addresses))
        else {
            return false;
        };
        let found = ips
            .iter()
            .filter_map(|ip| ip.parse::<IpAddr>().ok())
            .map(|ip| SocketAddr::new(ip, port))
            .take(MAX_ADDRESSES);
        if let Some(dialer) = lock(&self.dialers).get(&id) {
            dialer.addresses.send_replace(found_first(found, known));
        }
        true
    }

    /// The phone joined a network: computers that aren't connected are dialed right away
    /// instead of after their reconnection wait.
    pub fn network_changed(&self) {
        for dialer in lock(&self.dialers).values() {
            dialer.addresses.send_modify(|_| {});
        }
    }

    pub fn connected_count(&self) -> u32 {
        u32::try_from(self.state.peers.connected().len()).unwrap_or(u32::MAX)
    }

    /// The last clip a computer sent, since the client was created. Called from
    /// [`PhoneListener::on_clip_received`], it is the clip that call reports: the app hears of
    /// received clips one at a time.
    pub fn last_received(&self) -> Option<ReceivedClip> {
        lock(&self.state.last_received).clone()
    }

    /// Sends text to every connected computer. Returns how many.
    pub fn send_text(&self, text: String) -> u32 {
        self.send(
            mime::TEXT,
            text.into_bytes(),
            Context::new(Source::SentByHand),
        )
    }

    /// Sends a PNG or JPEG image, as it is, to every connected computer. Returns how many.
    /// Other formats must be converted to PNG first.
    pub fn send_image(&self, image: Vec<u8>) -> Result<u32, PastegetError> {
        let mime_type = mime::of_image(&image).ok_or(PastegetError::UnsupportedImage)?;
        Ok(self.send(mime_type, image, Context::new(Source::SentByHand)))
    }

    /// Sends a screenshot taken on the phone, like [`PhoneClient::send_image`]. The history
    /// keeps it as a reference to the gallery when `media_id` (its MediaStore `_ID`) is given,
    /// instead of a copy.
    pub fn send_screenshot(
        &self,
        image: Vec<u8>,
        media_id: Option<i64>,
    ) -> Result<u32, PastegetError> {
        let mime_type = mime::of_image(&image).ok_or(PastegetError::UnsupportedImage)?;
        Ok(self.send(mime_type, image, screenshot(media_id)))
    }

    /// Text the phone's clipboard now holds, seen by the automatic copy rather than shared by
    /// the user. Sent to every connected computer unless the clipboard already held it: a clip
    /// from a computer, or a copy reported twice. Returns how many computers it went to, or
    /// `None` if it wasn't new.
    ///
    /// `sensitive` copies (passwords, codes) are sent marked as such, so computers keep them out
    /// of history and clipboard managers.
    pub fn copied_text(&self, text: String, sensitive: bool) -> Option<u32> {
        let data = text.into_bytes();
        if !lock(&self.state.guard).should_send(mime::TEXT, &data) {
            return None;
        }
        let clip = self.local_clip(mime::TEXT, data, sensitive);
        self.state.keep(&clip, Context::new(Source::CopiedHere));
        Some(self.broadcast_clip(&clip))
    }

    /// Like [`PhoneClient::copied_text`] while sending copies is off: kept in the history only,
    /// never sent. Returns whether it was new.
    pub fn record_copied_text(&self, text: String, sensitive: bool) -> bool {
        let data = text.into_bytes();
        if !lock(&self.state.guard).should_send(mime::TEXT, &data) {
            return false;
        }
        let clip = self.local_clip(mime::TEXT, data, sensitive);
        self.state.keep(&clip, Context::new(Source::CopiedHere));
        true
    }

    /// Like [`PhoneClient::send_screenshot`] while sending screenshots is off: kept in the
    /// history only, never sent.
    pub fn record_screenshot(
        &self,
        image: Vec<u8>,
        media_id: Option<i64>,
    ) -> Result<(), PastegetError> {
        let mime_type = mime::of_image(&image).ok_or(PastegetError::UnsupportedImage)?;
        let clip = self.local_clip(mime_type, image, false);
        self.state.keep(&clip, screenshot(media_id));
        Ok(())
    }

    /// Sends a one-time code found in a notification of `app` (its name, e.g. "Messages") to
    /// every connected computer, which puts it on its clipboard as a secret. Returns how many.
    pub fn send_one_time_code(&self, code: String, app: String) -> u32 {
        let clip = Clip::one_time_code(code, &app, self.state.identity.device_id());
        // If the user also copies it, the automatic copy doesn't send it a second time.
        lock(&self.state.guard).should_send(&clip.mime_type, &clip.data);
        // Never kept: the history remembers its digits for a while, to keep copies of it out.
        self.state.keep(&clip, Context::new(Source::CopiedHere));
        self.broadcast_clip(&clip)
    }

    /// Opens the clipboard history kept in `dir` (a folder only the app reads, left out of
    /// backups: Android's no-backup files folder), and records into it from now on what the
    /// phone copies, sends and receives. If it is already open, applies `settings` and returns
    /// it. A history that can't be opened is an error, and syncing goes on without one.
    pub fn open_history(
        &self,
        dir: String,
        settings: HistorySettings,
    ) -> Result<Arc<History>, PastegetError> {
        if let Some(history) = self.state.history() {
            history.set_settings(settings)?;
            return Ok(history);
        }
        // Not under the lock: sessions look for the history meanwhile.
        let history = History::open(Path::new(&dir), self.state.identity.device_id(), &settings)?;
        for computer in lock(&self.state.computers).iter() {
            if let Ok(id) = computer.device_id.parse() {
                let platform = platform_number(computer.platform.as_deref());
                history.name_device(id, computer.name.clone(), platform);
            }
        }
        let mut open = lock(&self.state.history);
        Ok(Arc::clone(open.get_or_insert(history)))
    }

    /// Stops recording into the history ("Keep a history" off). The [`History`] the app holds
    /// still lists and deletes, until it lets it go.
    pub fn close_history(&self) {
        *lock(&self.state.history) = None;
    }

    /// An item of the history to put back on the phone's clipboard: its whole text. When the
    /// automatic copy then reports it, it isn't sent to the computers (the app may offer to,
    /// with [`PhoneClient::send_text`]). `None` for an image ([`History::original_path`]), a
    /// text too large to keep, or without a history.
    pub fn copy_back(&self, id: i64) -> Result<Option<String>, PastegetError> {
        let Some(history) = self.state.history() else {
            return Ok(None);
        };
        let text = history.full_text(id)?;
        if let Some(text) = &text {
            lock(&self.state.guard).should_send(mime::TEXT, text.as_bytes());
        }
        Ok(text)
    }
}

/// The one-time code (2FA) in a notification's text, digits only, if it has one.
#[uniffi::export]
pub fn find_one_time_code(text: String) -> Option<String> {
    pasteget_core::detect::one_time_code(&text)
}

impl PhoneClient {
    /// Lets [`PhoneClient::pair`] dial loopback addresses, for tests against a desktop daemon on
    /// the same machine. Not exported to the app: a phone never pairs with something on itself.
    #[doc(hidden)]
    pub fn pair_on_loopback_for_tests(&self) {
        self.pair_on_loopback.store(true, Ordering::Relaxed);
    }

    /// Sends what the user shared, even if it was sent before.
    fn send(&self, mime_type: &str, data: Vec<u8>, context: Context) -> u32 {
        // Remember it, so the automatic copy doesn't send it a second time.
        lock(&self.state.guard).should_send(mime_type, &data);
        let clip = self.local_clip(mime_type, data, false);
        self.state.keep(&clip, context);
        self.broadcast_clip(&clip)
    }

    /// A clip copied on this phone.
    fn local_clip(&self, mime_type: &str, data: Vec<u8>, sensitive: bool) -> Clip {
        Clip {
            mime_type: mime_type.to_owned(),
            data,
            sensitive,
            origin: self.state.identity.device_id(),
            kind: Kind::Copy,
            origin_clip_id: None,
        }
    }

    fn broadcast_clip(&self, clip: &Clip) -> u32 {
        u32::try_from(self.state.peers.broadcast(clip, None)).unwrap_or(u32::MAX)
    }

    fn is_running(&self) -> bool {
        lock(&self.state.listener).is_some()
    }

    /// Starts keeping a session with `computer`, replacing any earlier dialer for it.
    fn start_dialer(&self, computer: &PairedComputer) {
        let Ok(id) = computer.device_id.parse::<DeviceId>() else {
            return;
        };
        let (addresses, watched) = watch::channel(parse_addresses(&computer.addresses));
        let state = Arc::clone(&self.state);
        let task = self.runtime.spawn(async move {
            let hello = state.hello();
            session::keep_connected(&state.identity, id, watched, &hello, &state.peers, &*state)
                .await;
        });
        if let Some(old) = lock(&self.dialers).insert(id, Dialer { task, addresses }) {
            old.task.abort();
        }
    }
}

impl Drop for PhoneClient {
    fn drop(&mut self) {
        self.stop();
    }
}

impl State {
    fn hello(&self) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            device_name: self.device_name.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            platform: Platform::Android.into(),
        }
    }

    fn listener(&self) -> Option<Arc<dyn PhoneListener>> {
        lock(&self.listener).clone()
    }

    fn history(&self) -> Option<Arc<History>> {
        lock(&self.history).clone()
    }

    /// Records a clip in the history, if one is open. Never waits on the disk.
    fn keep(&self, clip: &Clip, context: Context) {
        if let Some(history) = self.history() {
            history.record(clip.clone(), context);
        }
    }

    /// A connection with `peer` started, with its `hello`, or ended: remembers when, and what
    /// its Hello said (a new name means it was renamed there).
    fn seen(&self, peer: DeviceId, hello: Option<&Hello>) {
        let id = peer.to_string();
        let mut computers = lock(&self.computers);
        let Some(computer) = computers.iter_mut().find(|c| c.device_id == id) else {
            return;
        };
        computer.last_seen_ms = Some(now_ms());
        if let Some(hello) = hello {
            computer.name = sanitize_device_name(&hello.device_name);
            computer.platform = platform_name(hello.platform);
            computer.app_version = clean_version(&hello.app_version);
        }
        // Best effort: if saving fails, it shows what it knew before after a restart, until it
        // connects.
        let _ = save_computers(&self.data_dir, &computers);
    }
}

/// How the history records a screenshot: as a reference to the gallery when it has its
/// MediaStore `_ID`.
fn screenshot(media_id: Option<i64>) -> Context {
    Context {
        media_id,
        ..Context::new(Source::Screenshot)
    }
}

/// The protocol's `Platform` number of a [`platform_name`].
fn platform_number(name: Option<&str>) -> i32 {
    let platform = match name {
        Some("android") => Platform::Android,
        Some("windows") => Platform::Windows,
        Some("linux") => Platform::Linux,
        Some("macos") => Platform::Macos,
        _ => Platform::Unspecified,
    };
    platform.into()
}

/// The name of a `Hello.platform`, `None` when unspecified or unknown.
fn platform_name(platform: i32) -> Option<String> {
    let name = match Platform::try_from(platform).ok()? {
        Platform::Unspecified => return None,
        Platform::Android => "android",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
        Platform::Macos => "macos",
    };
    Some(name.to_owned())
}

/// A version a peer sent, without control characters and at most 32 characters long. `None`
/// if it sent none.
fn clean_version(version: &str) -> Option<String> {
    let cleaned: String = version
        .chars()
        .filter(|c| !c.is_control())
        .take(32)
        .collect();
    Some(cleaned.trim().to_owned()).filter(|version| !version.is_empty())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

impl SessionEvents for State {
    /// The computer unpaired this phone, and refused its dialer or said so in their session:
    /// forget it too. Its dialer ends by itself.
    fn unpaired_by(&self, peer: DeviceId) {
        let id = peer.to_string();
        let removed = {
            let mut computers = lock(&self.computers);
            let removed = computers
                .iter()
                .position(|c| c.device_id == id)
                .map(|i| computers.remove(i));
            if removed.is_some() {
                let _ = save_computers(&self.data_dir, &computers);
            }
            self.peers.unexpect(&peer);
            removed
        };
        if let (Some(computer), Some(listener)) = (removed, self.listener()) {
            listener.on_unpaired(computer);
        }
    }

    /// Moves the address that worked to the front, so it's dialed first next time.
    fn reached(&self, peer: DeviceId, address: SocketAddr) {
        let id = peer.to_string();
        let address = address.to_string();
        let mut computers = lock(&self.computers);
        let Some(computer) = computers.iter_mut().find(|c| c.device_id == id) else {
            return;
        };
        if computer.addresses.first() == Some(&address) {
            return;
        }
        computer.addresses.retain(|a| *a != address);
        computer.addresses.insert(0, address);
        computer.addresses.truncate(MAX_ADDRESSES);
        // Best effort: if saving fails, the next connection tries again.
        let _ = save_computers(&self.data_dir, &computers);
    }

    fn connected(&self, peer: DeviceId, hello: &Hello) {
        // Forgotten while this connection was being set up: `forget` came too early to end it.
        let id = peer.to_string();
        if !lock(&self.computers).iter().any(|c| c.device_id == id) {
            self.peers.unpair(&peer);
            return;
        }
        self.seen(peer, Some(hello));
        if let Some(history) = self.history() {
            let name = sanitize_device_name(&hello.device_name);
            history.name_device(peer, name, hello.platform);
        }
        if let Some(listener) = self.listener() {
            listener.on_connection_changed(peer.to_string(), true);
        }
    }

    fn disconnected(&self, peer: DeviceId) {
        self.seen(peer, None);
        if let Some(listener) = self.listener() {
            listener.on_connection_changed(peer.to_string(), false);
        }
    }

    fn clip_received(&self, peer: DeviceId, clip: Clip) {
        {
            let mut guard = lock(&self.guard);
            if !guard.accepts_origin(&clip.origin) {
                return;
            }
            guard.applied_remote(&clip);
        }
        let id = peer.to_string();
        let device_name = lock(&self.computers)
            .iter()
            .find(|c| c.device_id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let received = ReceivedClip {
            device_id: id,
            device_name,
            origin_device_id: clip.origin.to_string(),
            mime_type: clip.mime_type.clone(),
            size: clip.data.len() as u64,
            sensitive: clip.sensitive,
            received_at_ms: now_ms(),
        };
        self.keep(&clip, Context::received_from(peer));
        let _one_at_a_time = lock(&self.reporting_clip);
        *lock(&self.last_received) = Some(received);
        if let Some(listener) = self.listener() {
            listener.on_clip_received(clip.mime_type, clip.data, clip.sensitive);
        }
    }

    fn clip_delivered(&self, peer: DeviceId, delivery: &Delivery) {
        if let Some(listener) = self.listener() {
            listener.on_clip_delivered(ClipDelivery {
                device_id: peer.to_string(),
                mime_type: delivery.mime_type.clone(),
                size: delivery.len as u64,
                millis: u64::try_from(delivery.elapsed.as_millis()).unwrap_or(u64::MAX),
                accepted: delivery.status == AckStatus::Ok,
            });
        }
    }
}

fn parse_addresses(addresses: &[String]) -> Vec<SocketAddr> {
    addresses.iter().filter_map(|a| a.parse().ok()).collect()
}

fn load_or_create_identity(dir: &Path) -> Result<Identity, PastegetError> {
    let path = dir.join(IDENTITY_FILE);
    match fs::read(&path) {
        Ok(der) => {
            Identity::from_pkcs8_der(&der).map_err(|e| PastegetError::Storage(e.to_string()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let identity =
                Identity::generate().map_err(|e| PastegetError::Storage(e.to_string()))?;
            write_atomically(&path, identity.pkcs8_der())?;
            Ok(identity)
        }
        Err(e) => Err(e.into()),
    }
}

/// The saved computers. A file that doesn't read as such (a write cut short, a bug) is moved
/// aside to `computers.json.bad` and none are loaded: the app still starts, and the user pairs
/// again.
fn load_computers(dir: &Path) -> Result<Vec<PairedComputer>, PastegetError> {
    let path = dir.join(COMPUTERS_FILE);
    let json = match fs::read(&path) {
        Ok(json) => json,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    match serde_json::from_slice(&json) {
        Ok(computers) => Ok(computers),
        Err(e) => {
            let aside = dir.join(format!("{COMPUTERS_FILE}.bad"));
            // Where it failed, not the message: that can quote the file.
            tracing::warn!(
                category = ?e.classify(),
                line = e.line(),
                column = e.column(),
                len = json.len(),
                "the saved computers could not be read: moved to {COMPUTERS_FILE}.bad, starting with none"
            );
            if let Err(e) = fs::rename(&path, &aside) {
                tracing::warn!(error = %e, "could not move the unreadable file aside");
            }
            Ok(Vec::new())
        }
    }
}

fn save_computers(dir: &Path, computers: &[PairedComputer]) -> Result<(), PastegetError> {
    let json =
        serde_json::to_vec_pretty(computers).map_err(|e| PastegetError::Storage(e.to_string()))?;
    write_atomically(&dir.join(COMPUTERS_FILE), &json)
}

/// The app's files directory is private to the app on Android, so no extra permissions needed.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), PastegetError> {
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)?;
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasteget_core::session::SessionError;
    use pasteget_core::transport;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    /// Records whether the app was told a computer connected.
    #[derive(Default)]
    struct Connections(Mutex<Vec<bool>>);

    impl PhoneListener for Connections {
        fn on_connection_changed(&self, _device_id: String, connected: bool) {
            self.0.lock().unwrap().push(connected);
        }

        fn on_clip_received(&self, _mime_type: String, _data: Vec<u8>, _sensitive: bool) {}

        fn on_clip_delivered(&self, _delivery: ClipDelivery) {}

        fn on_unpaired(&self, _computer: PairedComputer) {}
    }

    /// Records who unpaired the computer.
    #[derive(Default)]
    struct UnpairedBy(Mutex<Vec<DeviceId>>);

    impl SessionEvents for UnpairedBy {
        fn clip_received(&self, _peer: DeviceId, _clip: Clip) {}

        fn unpaired_by(&self, peer: DeviceId) {
            self.0.lock().unwrap().push(peer);
        }
    }

    /// Forgotten while its connection was being set up, where `forget` can't end it yet: the
    /// session tells the computer itself, and the app never hears it connected.
    #[test]
    fn a_computer_forgotten_while_it_connects_is_told() {
        let dir = tempfile::tempdir().unwrap();
        let phone = PhoneClient::new(dir.path().to_str().unwrap().into(), "POCO".into()).unwrap();
        let connections = Arc::new(Connections::default());
        phone.start(connections.clone());
        let state = Arc::clone(&phone.state);
        let pc = Identity::generate().unwrap();
        let (pc_peers, pc_events) = (Peers::new(), UnpairedBy::default());
        let pc_hello = Hello {
            protocol_version: PROTOCOL_VERSION,
            device_name: "PC".into(),
            ..Hello::default()
        };

        let (phone_ended, pc_ended) = phone.runtime.block_on(async {
            let (phone_io, pc_io) = tokio::io::duplex(64 * 1024);
            let (client, server) = tokio::join!(
                transport::connect(phone_io, &state.identity, pc.device_id()),
                async { transport::accept(pc_io, &pc).await?.authorize(|_| true) }
            );
            let sessions = async {
                tokio::join!(
                    session::run_session(client.unwrap(), state.hello(), &state.peers, &*state),
                    session::run_session(server.unwrap(), pc_hello, &pc_peers, &pc_events)
                )
            };
            tokio::time::timeout(Duration::from_secs(5), sessions)
                .await
                .expect("both sessions should end")
        });

        assert!(matches!(phone_ended, Err(SessionError::Unpaired)));
        assert!(matches!(pc_ended, Err(SessionError::UnpairedByPeer)));
        assert_eq!(*pc_events.0.lock().unwrap(), [state.identity.device_id()]);
        assert!(!connections.0.lock().unwrap().contains(&true));
        assert!(phone.paired_computers().is_empty());
    }

    /// Asks `last_received` from inside `on_clip_received`, after taking its time.
    #[derive(Default)]
    struct SlowAsker {
        client: std::sync::OnceLock<std::sync::Weak<PhoneClient>>,
        /// The size of each clip, and the size `last_received` gave meanwhile.
        heard: Mutex<Vec<(usize, Option<u64>)>>,
    }

    impl PhoneListener for SlowAsker {
        fn on_connection_changed(&self, _device_id: String, _connected: bool) {}

        fn on_clip_received(&self, _mime_type: String, data: Vec<u8>, _sensitive: bool) {
            if data.len() == 1 {
                std::thread::sleep(Duration::from_millis(300));
            }
            let client = self.client.get().and_then(|c| c.upgrade()).unwrap();
            let said = client.last_received().map(|clip| clip.size);
            self.heard.lock().unwrap().push((data.len(), said));
        }

        fn on_clip_delivered(&self, _delivery: ClipDelivery) {}

        fn on_unpaired(&self, _computer: PairedComputer) {}
    }

    /// A clip arriving while the app handles another one waits: `last_received` still describes
    /// the one being handled.
    #[test]
    fn last_received_waits_its_turn() {
        let dir = tempfile::tempdir().unwrap();
        let phone = PhoneClient::new(dir.path().to_str().unwrap().into(), "POCO".into()).unwrap();
        let asker = Arc::new(SlowAsker::default());
        asker.client.set(Arc::downgrade(&phone)).unwrap();
        phone.start(asker.clone());
        let (first, second) = (
            Identity::generate().unwrap().device_id(),
            Identity::generate().unwrap().device_id(),
        );

        std::thread::scope(|threads| {
            let state = &phone.state;
            threads.spawn(|| state.clip_received(first, Clip::text("1", first)));
            std::thread::sleep(Duration::from_millis(50));
            threads.spawn(|| state.clip_received(second, Clip::text("22", second)));
        });

        assert_eq!(*asker.heard.lock().unwrap(), [(1, Some(1)), (2, Some(2))]);
        assert_eq!(phone.last_received().unwrap().device_id, second.to_string());
    }

    /// Saved before computers had a last-seen time, a platform and a version.
    #[test]
    fn computers_saved_by_an_older_version_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::generate().unwrap().device_id().to_string();
        let old = format!(
            r#"[{{"device_id": "{id}", "name": "HP-AIO", "addresses": ["192.168.1.20:47290"]}}]"#
        );
        fs::write(dir.path().join(COMPUTERS_FILE), old).unwrap();

        let computers = load_computers(dir.path()).unwrap();

        assert_eq!(
            computers,
            [PairedComputer {
                device_id: id,
                name: "HP-AIO".into(),
                addresses: vec!["192.168.1.20:47290".into()],
                last_seen_ms: None,
                platform: None,
                app_version: None,
            }]
        );
    }

    #[test]
    fn what_a_hello_says_is_cleaned() {
        assert_eq!(
            platform_name(Platform::Windows.into()).as_deref(),
            Some("windows")
        );
        assert_eq!(
            platform_name(Platform::Linux.into()).as_deref(),
            Some("linux")
        );
        assert_eq!(platform_name(Platform::Unspecified.into()), None);
        assert_eq!(platform_name(99), None);
        assert_eq!(clean_version(" 0.1.0\n"), Some("0.1.0".into()));
        assert_eq!(clean_version(""), None);
        assert_eq!(clean_version(&"9".repeat(100)).unwrap().len(), 32);
    }

    #[test]
    fn unparsable_addresses_are_skipped() {
        let stored = ["192.168.1.20:47290".to_owned(), "not an address".to_owned()];
        assert_eq!(parse_addresses(&stored), [addr("192.168.1.20:47290")]);
    }
}
