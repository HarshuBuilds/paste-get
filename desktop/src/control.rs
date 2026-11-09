// SPDX-License-Identifier: GPL-3.0-or-later

//! Local control channel between the `pasteget` command and a running daemon.
//!
//! One JSON object per line. The client sends one [`Request`]; the daemon answers with one or
//! more [`Response`]s and closes. Fields are only ever added, with defaults, so that a newer
//! client still reads an older daemon (that's how a newer tray finds an older daemon to replace,
//! [`StatusReply::app_version`]). Only the local user can connect:
//! - Unix: a socket in the profile directory, which is private to the user (0700).
//! - Windows: a named pipe whose name includes the user name. The daemon creates it with
//!   `first_pipe_instance`, so it fails rather than share a pipe someone else created, and
//!   remote clients are rejected.

#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use data_encoding::BASE64;
use pasteget_core::clip::mime;
use pasteget_core::detect::Category;
use pasteget_core::history::{Cursor, Format, Item, ItemId, MAX_PAGE_SIZE, PAGE_SIZE, Query};
use pasteget_core::identity::DeviceId;
use pasteget_core::pairing::{PAIRING_TTL, PairingInvite};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio::task::{JoinHandle, JoinSet};

use crate::clipboard::Content;
use crate::config::Profile;
use crate::daemon::{Daemon, DeviceStatus, Event};
use crate::hotkeys::Action;
use crate::recent;

/// Longest request line accepted: a 64 MiB clip, base64-encoded, plus some room.
const MAX_REQUEST_LEN: u64 = 96 * 1024 * 1024;

/// Version of the control channel, in [`StatusReply::control_version`]. Bumped when a client
/// needs to know what a daemon understands. 1: the first to say it, and to know
/// [`Request::Quit`]. 2: the history requests ([`Request::History`] and the ones after it).
pub const CONTROL_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Pair,
    Join {
        invite: String,
    },
    Send {
        mime_type: String,
        data_base64: String,
    },
    /// Sends what's on the clipboard now, even while paused.
    SendClipboard,
    /// Pauses (`true`) or resumes (`false`) syncing. `None` toggles.
    Pause {
        paused: Option<bool>,
    },
    Recent,
    /// Puts a clip from [`Request::Recent`] back on the clipboard.
    CopyRecent {
        id: u64,
    },
    /// Unpairs a device, given by its ID or by its name when no other device has it.
    Forget {
        device: String,
    },
    /// Reads `settings.toml` again, after it was edited.
    ReloadSettings,
    /// Stops the daemon, and the tray or `pasteget daemon` running it: a newer version takes
    /// over. Unknown to daemons without a [`StatusReply::control_version`].
    Quit,
    /// A page of the clipboard history, newest first: the items `query` finds (words, any part
    /// of them, accents ignored; `site:`, `from:`, `type:`, `is:pinned`, `after:`, `before:`), or
    /// all of them. Answered with [`Response::HistoryPage`]. This one and the history requests
    /// below are unknown to daemons before [`CONTROL_VERSION`] 2.
    History {
        #[serde(default)]
        query: String,
        /// The `next` of the page before.
        #[serde(default)]
        after: Option<HistoryCursorReply>,
        /// At most 200; 50 by default.
        #[serde(default)]
        limit: Option<u32>,
        /// The user's offset from UTC, for `after:` and `before:` dates.
        #[serde(default)]
        utc_offset_minutes: i32,
    },
    /// Puts an item of the history back on the clipboard. It isn't sent again.
    HistoryCopy {
        id: i64,
    },
    HistoryPin {
        id: i64,
        pinned: bool,
    },
    /// Deletes items of the history, pinned or not.
    HistoryDelete {
        ids: Vec<i64>,
    },
    /// Deletes the items last copied since `since_ms` (unix ms), or all of them; pinned ones
    /// too only if `pinned_too`.
    HistoryClear {
        #[serde(default)]
        since_ms: Option<i64>,
        #[serde(default)]
        pinned_too: bool,
    },
    HistoryStats,
    /// Does what a hotkey does, with its notification ("Syncing paused", "Clipboard sent to 1
    /// device"): `action` is an [`Action::id`]. For desktops that bind keys to commands
    /// (Hyprland). Older daemons answer it with "bad request".
    Perform {
        action: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Status(StatusReply),
    Invite {
        uri: String,
        expires_in_secs: u64,
    },
    PairingAttemptFailed {
        reason: String,
    },
    Paired(DeviceReply),
    Joined(DeviceReply),
    Sent {
        devices: usize,
    },
    Paused {
        paused: bool,
    },
    Recent {
        clips: Vec<RecentReply>,
    },
    Copied,
    Forgotten(DeviceReply),
    /// Notifications and links changed at once; the name, port and hotkeys need a restart.
    SettingsReloaded {
        restart_needed: bool,
    },
    /// Answer to [`Request::Quit`]: the daemon stops once it has said so.
    Quitting,
    HistoryPage {
        items: Vec<HistoryItemReply>,
        /// Where the next page starts; `None` when this one is the last.
        next: Option<HistoryCursorReply>,
    },
    HistoryPinned {
        id: i64,
        pinned: bool,
    },
    /// Answer to [`Request::HistoryDelete`] and [`Request::HistoryClear`].
    HistoryDeleted {
        items: usize,
    },
    HistoryStats(HistoryStatsReply),
    /// Answer to [`Request::Perform`], once its notification is on its way.
    Performed,
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusReply {
    pub device_id: String,
    pub device_name: String,
    pub listening_on: String,
    pub pairing_open: bool,
    #[serde(default)]
    pub paused: bool,
    pub devices: Vec<DeviceStatusReply>,
    /// The daemon's paste-get version. `None` from daemons older than this field.
    #[serde(default)]
    pub app_version: Option<String>,
    /// The daemon's [`CONTROL_VERSION`]. `None` from daemons older than this field.
    #[serde(default)]
    pub control_version: Option<u32>,
    /// The ID of the daemon's process (the tray's, when the tray runs it), so that a newer
    /// version can end it if it doesn't quit. `None` from daemons older than this field.
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceStatusReply {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

/// A recent clip. Only text clips have a preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentReply {
    pub id: u64,
    pub mime_type: String,
    pub len: usize,
    /// The device it came from; `None` for copies made on this computer.
    pub from: Option<String>,
    pub age_secs: u64,
    pub preview: Option<String>,
}

impl RecentReply {
    /// "Text: hello world" or "Image (PNG, 1.2 MB)".
    pub fn describe(&self) -> String {
        match &self.preview {
            Some(preview) => format!("{} {preview}", crate::i18n::tr("Text:", "Texte :")),
            None => {
                let kind = self
                    .mime_type
                    .strip_prefix("image/")
                    .map_or_else(|| self.mime_type.clone(), str::to_uppercase);
                format!("Image ({kind}, {})", recent::size(self.len))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceReply {
    pub id: String,
    pub name: String,
}

/// An item of the history.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryItemReply {
    pub id: i64,
    pub mime_type: String,
    /// "text", "link", "email", "phone", "code", "json", "color", "screenshot" or "image".
    pub category: String,
    /// Bytes of the original.
    pub size: u64,
    /// The name of the device where it was first copied, empty if it was never named; `None`
    /// for this computer.
    pub from: Option<String>,
    /// The name of the paired device it came through, empty if it was never named; `None` when
    /// it didn't come from one.
    pub via: Option<String>,
    /// Unix ms.
    pub last_at_ms: i64,
    /// How many times it was copied or received.
    pub times: u32,
    pub pinned: bool,
    /// The start of a text, or the part around what a search found. Empty for an image.
    pub preview: String,
    /// What a search found in `preview`, as `[start, end)` in UTF-16 code units (JavaScript's
    /// string indexes).
    #[serde(default)]
    pub highlights: Vec<[u32; 2]>,
}

/// Never prints contents.
impl std::fmt::Debug for HistoryItemReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryItemReply")
            .field("id", &self.id)
            .field("mime_type", &self.mime_type)
            .field("category", &self.category)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl HistoryItemReply {
    /// Device names stay as the history has them, empty ones too: each client says "another
    /// device" in its own language.
    fn of(item: Item) -> Self {
        let name = |device: Option<pasteget_core::history::Device>| device.map(|d| d.name);
        Self {
            id: item.id.0,
            mime_type: match item.format {
                Format::Text => mime::TEXT,
                Format::Png => mime::PNG,
                Format::Jpeg => mime::JPEG,
            }
            .to_owned(),
            category: category_name(item.category).to_owned(),
            size: item.size,
            from: name(item.origin),
            via: name(item.via),
            last_at_ms: item.last_at,
            times: item.times,
            pinned: item.pinned_at.is_some(),
            preview: item.preview,
            highlights: item
                .highlights
                .into_iter()
                .map(|range| [range.start, range.end])
                .collect(),
        }
    }

    /// "Text: hello world" or "Image (PNG, 1.2 MB)", on one line, escaped.
    pub fn describe(&self) -> String {
        if self.mime_type == mime::TEXT {
            let text = crate::history::escape(&recent::preview(&self.preview));
            format!("{} {text}", crate::i18n::tr("Text:", "Texte :"))
        } else {
            let kind = self
                .mime_type
                .strip_prefix("image/")
                .map_or_else(|| self.mime_type.clone(), str::to_uppercase);
            format!("Image ({kind}, {})", recent::size(self.size as usize))
        }
    }
}

fn category_name(category: Category) -> &'static str {
    match category {
        Category::Text => "text",
        Category::Link => "link",
        Category::Email => "email",
        Category::Phone => "phone",
        Category::Code => "code",
        Category::Json => "json",
        Category::Color => "color",
        Category::Screenshot => "screenshot",
        Category::Image => "image",
    }
}

/// Where the next page of the history starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCursorReply {
    pub last_at_ms: i64,
    pub id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryStatsReply {
    pub items: u64,
    pub pinned: u64,
    pub images: u64,
    /// The database and its log, plus the original images.
    pub bytes: u64,
    /// Passwords, codes and keys kept out in the last 7 days.
    pub kept_out_this_week: u64,
}

/// Where a profile's daemon listens for commands.
#[derive(Clone, Debug)]
pub struct Endpoint {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    pipe_name: String,
}

impl Endpoint {
    pub fn for_profile(profile: &Profile) -> Self {
        #[cfg(unix)]
        {
            Self {
                path: profile.dir().join("control.sock"),
            }
        }
        #[cfg(windows)]
        {
            let user = std::env::var("USERNAME").unwrap_or_default();
            let safe = |s: &str| -> String {
                s.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect()
            };
            // The directory's fingerprint keeps profiles with the same name but different
            // locations (PASTEGET_HOME, tests) on separate pipes.
            let location = fnv1a(profile.dir().to_string_lossy().as_bytes());
            Self {
                pipe_name: format!(
                    r"\\.\pipe\paste-get-{}-{}-{location:016x}",
                    safe(&user),
                    profile.name()
                ),
            }
        }
    }
}

/// FNV-1a: a tiny hash that stays the same across builds, so a newer `pasteget` command still
/// finds a daemon started by an older one. Not a security measure.
#[cfg_attr(not(windows), allow(dead_code))]
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Serves control requests for `daemon` until the returned handle is aborted. Aborting it also
/// ends the requests under way (a pairing waits minutes for a phone), so they let go of the
/// daemon: a new one can then take its port and control channel (the tray's restart).
pub async fn serve(daemon: Arc<Daemon>, endpoint: &Endpoint) -> Result<JoinHandle<()>> {
    #[cfg(unix)]
    {
        use tokio::net::UnixListener;

        if endpoint.path.exists() {
            if tokio::net::UnixStream::connect(&endpoint.path)
                .await
                .is_ok()
            {
                anyhow::bail!("a daemon is already running for this profile");
            }
            std::fs::remove_file(&endpoint.path).context("could not remove a stale socket")?;
        }
        let listener = UnixListener::bind(&endpoint.path)
            .with_context(|| format!("could not listen on {}", endpoint.path.display()))?;
        Ok(tokio::spawn(async move {
            // Dropped with this task, which aborts them.
            let mut requests = JoinSet::new();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        while requests.try_join_next().is_some() {}
                        requests.spawn(handle(Arc::clone(&daemon), stream));
                    }
                    Err(e) => tracing::warn!(error = %e, "control socket error"),
                }
            }
        }))
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;

        let name = endpoint.pipe_name.clone();
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)
            .context("could not create the control pipe: is a daemon already running?")?;
        Ok(tokio::spawn(async move {
            // Dropped with this task, which aborts them and closes their pipe instances: the
            // next daemon's `first_pipe_instance` needs every one closed.
            let mut requests = JoinSet::new();
            loop {
                if let Err(e) = server.connect().await {
                    tracing::warn!(error = %e, "control pipe error");
                    continue;
                }
                let next = match ServerOptions::new()
                    .reject_remote_clients(true)
                    .create(&name)
                {
                    Ok(next) => next,
                    Err(e) => {
                        tracing::warn!(error = %e, "could not reopen the control pipe");
                        return;
                    }
                };
                let connected = std::mem::replace(&mut server, next);
                while requests.try_join_next().is_some() {}
                requests.spawn(handle(Arc::clone(&daemon), connected));
            }
        }))
    }
}

/// Whether a daemon already answers on `endpoint`.
pub async fn is_running(endpoint: &Endpoint) -> bool {
    #[cfg(unix)]
    {
        tokio::net::UnixStream::connect(&endpoint.path)
            .await
            .is_ok()
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&endpoint.pipe_name)
            .is_ok()
    }
}

/// Removes the socket file, if any. Call on shutdown.
pub fn cleanup(endpoint: &Endpoint) {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&endpoint.path);
    }
    #[cfg(windows)]
    {
        let _ = endpoint;
    }
}

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(daemon: Arc<Daemon>, stream: S) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half.take(MAX_REQUEST_LEN));
    let mut line = String::new();
    let request = match reader.read_line(&mut line).await {
        Ok(0) => return,
        Ok(_) => serde_json::from_str::<Request>(&line).map_err(|e| anyhow!("bad request: {e}")),
        Err(e) => Err(anyhow!("could not read the request: {e}")),
    };
    let mut quit = false;
    let result: Result<()> = async {
        match request? {
            Request::Status => {
                let status = daemon.status().await;
                let response = Response::Status(StatusReply {
                    device_id: status.device_id.to_string(),
                    device_name: status.device_name,
                    listening_on: status.listening_on.to_string(),
                    pairing_open: status.pairing_open,
                    paused: status.paused,
                    devices: status
                        .devices
                        .into_iter()
                        .map(|d| DeviceStatusReply {
                            id: d.device.id.to_string(),
                            name: d.device.name,
                            connected: d.connected,
                        })
                        .collect(),
                    app_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                    control_version: Some(CONTROL_VERSION),
                    pid: Some(std::process::id()),
                });
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::Pair => {
                let mut events = daemon.subscribe();
                let invite = daemon.open_pairing().await?;
                let response = Response::Invite {
                    uri: invite.to_uri(),
                    expires_in_secs: PAIRING_TTL.as_secs(),
                };
                write_half.write_all(line_of(response).as_bytes()).await?;
                let outcome = tokio::time::timeout(PAIRING_TTL, async {
                    loop {
                        match events.recv().await {
                            Ok(Event::Paired(device)) => {
                                return Response::Paired(DeviceReply {
                                    id: device.id.to_string(),
                                    name: device.name,
                                });
                            }
                            Ok(Event::PairingFailed(reason)) => {
                                // Wrong secrets never close the code: it still works.
                                let response = Response::PairingAttemptFailed { reason };
                                let _ = write_half.write_all(line_of(response).as_bytes()).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => {
                                return Response::Error {
                                    message: "the daemon stopped".into(),
                                };
                            }
                        }
                    }
                })
                .await
                .unwrap_or(Response::Error {
                    message: "the pairing code expired".into(),
                });
                write_half.write_all(line_of(outcome).as_bytes()).await?;
            }
            Request::Join { invite } => {
                let invite: PairingInvite = invite.parse()?;
                let device = daemon.join(&invite).await?;
                let response = Response::Joined(DeviceReply {
                    id: device.id.to_string(),
                    name: device.name,
                });
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::Send {
                mime_type,
                data_base64,
            } => {
                let data = BASE64
                    .decode(data_base64.as_bytes())
                    .context("data is not base64")?;
                let devices = daemon.send(Content {
                    mime_type,
                    data,
                    sensitive: false,
                });
                write_half
                    .write_all(line_of(Response::Sent { devices }).as_bytes())
                    .await?;
            }
            Request::SendClipboard => {
                let devices = daemon.send_clipboard().await?;
                write_half
                    .write_all(line_of(Response::Sent { devices }).as_bytes())
                    .await?;
            }
            Request::Pause { paused } => {
                let paused = daemon.set_paused(paused.unwrap_or(!daemon.is_paused()));
                write_half
                    .write_all(line_of(Response::Paused { paused }).as_bytes())
                    .await?;
            }
            Request::Recent => {
                let now = SystemTime::now();
                let clips = daemon
                    .recent()
                    .into_iter()
                    .map(|clip| RecentReply {
                        id: clip.id,
                        len: clip.data.len(),
                        age_secs: now.duration_since(clip.at).unwrap_or_default().as_secs(),
                        preview: (clip.mime_type == mime::TEXT)
                            .then(|| recent::preview(&String::from_utf8_lossy(&clip.data))),
                        mime_type: clip.mime_type,
                        from: clip.from,
                    })
                    .collect();
                write_half
                    .write_all(line_of(Response::Recent { clips }).as_bytes())
                    .await?;
            }
            Request::CopyRecent { id } => {
                daemon.copy_recent(id)?;
                write_half
                    .write_all(line_of(Response::Copied).as_bytes())
                    .await?;
            }
            Request::ReloadSettings => {
                let restart_needed = daemon.reload_settings()?;
                write_half
                    .write_all(line_of(Response::SettingsReloaded { restart_needed }).as_bytes())
                    .await?;
            }
            Request::Forget { device } => {
                let id = find_device(&daemon.status().await.devices, &device)?;
                let device = daemon.forget(&id)?;
                let response = Response::Forgotten(DeviceReply {
                    id: device.id.to_string(),
                    name: device.name,
                });
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::Quit => {
                write_half
                    .write_all(line_of(Response::Quitting).as_bytes())
                    .await?;
                quit = true;
            }
            Request::History {
                query,
                after,
                limit,
                utc_offset_minutes,
            } => {
                let query = Query {
                    text: query,
                    after: after.map(|after| Cursor {
                        last_at: after.last_at_ms,
                        id: after.id,
                    }),
                    limit: limit.unwrap_or(PAGE_SIZE).min(MAX_PAGE_SIZE),
                    utc_offset_minutes,
                    ..Query::default()
                };
                let page = daemon.history()?.page(&query)?;
                let response = Response::HistoryPage {
                    items: page.items.into_iter().map(HistoryItemReply::of).collect(),
                    next: page.next.map(|next| HistoryCursorReply {
                        last_at_ms: next.last_at,
                        id: next.id,
                    }),
                };
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::HistoryCopy { id } => {
                daemon.copy_from_history(ItemId(id))?;
                write_half
                    .write_all(line_of(Response::Copied).as_bytes())
                    .await?;
            }
            Request::HistoryPin { id, pinned } => {
                daemon.history()?.pin(ItemId(id), pinned)?;
                let response = Response::HistoryPinned { id, pinned };
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::HistoryDelete { ids } => {
                let ids: Vec<ItemId> = ids.into_iter().map(ItemId).collect();
                let items = daemon.history()?.delete(&ids)?;
                let response = Response::HistoryDeleted { items };
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::HistoryClear {
                since_ms,
                pinned_too,
            } => {
                let since = since_ms.unwrap_or(i64::MIN);
                let items = daemon.history()?.clear(since..i64::MAX, pinned_too)?;
                let response = Response::HistoryDeleted { items };
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::HistoryStats => {
                let history = daemon.history()?;
                let stats = history.stats()?;
                let week_ago = unix_ms(SystemTime::now()) - 7 * 24 * 60 * 60 * 1000;
                let kept_out = history.kept_out(week_ago)?.iter().map(|(_, n)| n).sum();
                let response = Response::HistoryStats(HistoryStatsReply {
                    items: stats.items,
                    pinned: stats.pinned,
                    images: stats.images,
                    bytes: stats.database_bytes + stats.originals_bytes,
                    kept_out_this_week: kept_out,
                });
                write_half.write_all(line_of(response).as_bytes()).await?;
            }
            Request::Perform { action } => {
                let action = Action::from_id(&action).with_context(|| {
                    format!("unknown action {action:?}: toggle-pause or send-clipboard")
                })?;
                daemon.perform(action).await;
                write_half
                    .write_all(line_of(Response::Performed).as_bytes())
                    .await?;
            }
        }
        Ok(())
    }
    .await;

    if let Err(e) = result {
        let response = Response::Error {
            message: format!("{e:#}"),
        };
        let _ = write_half.write_all(line_of(response).as_bytes()).await;
    }
    let _ = write_half.shutdown().await;
    // Once the answer is out: the process may end right after.
    if quit {
        daemon.request_quit();
    }
}

/// The paired device that `query` names: its ID, or its name (any case) if only one device has
/// that name.
fn find_device(devices: &[DeviceStatus], query: &str) -> Result<DeviceId> {
    let query = query.trim();
    if let Some(device) = devices.iter().find(|d| d.device.id.to_string() == query) {
        return Ok(device.device.id);
    }
    let named: Vec<_> = devices
        .iter()
        .filter(|d| d.device.name.to_lowercase() == query.to_lowercase())
        .collect();
    match named[..] {
        [device] => Ok(device.device.id),
        [] => bail!("no paired device is called {query:?}; `pasteget status` lists them"),
        _ => bail!(
            "{} devices are called {query:?}; give the ID instead: {}",
            named.len(),
            named
                .iter()
                .map(|d| d.device.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// `time` in unix ms.
pub fn unix_ms(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

fn line_of(response: Response) -> String {
    let mut text = serde_json::to_string(&response).unwrap_or_default();
    text.push('\n');
    text
}

/// Client side: sends `request` and calls `on_response` for each answer line.
pub async fn call(
    endpoint: &Endpoint,
    request: &Request,
    mut on_response: impl FnMut(Response),
) -> Result<()> {
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(&endpoint.path)
        .await
        .map_err(|_| not_running())?;
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&endpoint.pipe_name)
        .map_err(|_| not_running())?;

    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut line = serde_json::to_string(request)?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;

    let mut lines = BufReader::new(read_half).lines();
    while let Some(line) = lines.next_line().await? {
        on_response(serde_json::from_str(&line).context("unexpected answer from the daemon")?);
    }
    Ok(())
}

fn not_running() -> anyhow::Error {
    anyhow!("the paste-get daemon is not running for this profile: start it with `pasteget daemon`")
}

/// How long a client waits before giving up on a daemon that doesn't answer.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::{MemoryClipboard, Raw};
    use crate::config::KnownDevice;
    use crate::daemon::DaemonOptions;
    use pasteget_core::identity::Identity;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    struct Running {
        daemon: Arc<Daemon>,
        endpoint: Endpoint,
        clipboard: MemoryClipboard,
        _server: JoinHandle<()>,
        _home: tempfile::TempDir,
    }

    async fn run(name: &str) -> Running {
        run_with(name, None).await
    }

    /// A daemon whose `settings.toml` is `settings`, when given.
    async fn run_with(name: &str, settings: Option<&str>) -> Running {
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), name).unwrap();
        if let Some(settings) = settings {
            std::fs::write(profile.dir().join("settings.toml"), settings).unwrap();
        }
        let clipboard = MemoryClipboard::default();
        let options = DaemonOptions {
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            advertised_ips: Some(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
            poll_interval: Duration::from_millis(10),
            announce: false,
            notifier: None,
            hotkeys: false,
        };
        let daemon = Arc::new(
            Daemon::start(profile.clone(), clipboard.clone(), options)
                .await
                .unwrap(),
        );
        let endpoint = Endpoint::for_profile(&profile);
        let server = serve(Arc::clone(&daemon), &endpoint).await.unwrap();
        Running {
            daemon,
            endpoint,
            clipboard,
            _server: server,
            _home: home,
        }
    }

    async fn ask(endpoint: &Endpoint, request: Request) -> Vec<Response> {
        let mut responses = Vec::new();
        call(endpoint, &request, |r| responses.push(r))
            .await
            .unwrap();
        responses
    }

    #[tokio::test]
    async fn pair_join_send_and_status_through_the_control_channel() {
        let pc = run("pc").await;
        let laptop = run("laptop").await;

        // `pasteget pair` on the PC streams the invite, then the result.
        let pc_endpoint = pc.endpoint.clone();
        let (invite_tx, invite_rx) = tokio::sync::oneshot::channel();
        let pairing = tokio::spawn(async move {
            let mut invite_tx = Some(invite_tx);
            let mut responses = Vec::new();
            call(&pc_endpoint, &Request::Pair, |response| {
                if let (Response::Invite { uri, .. }, Some(tx)) = (&response, invite_tx.take()) {
                    let _ = tx.send(uri.clone());
                }
                responses.push(response);
            })
            .await
            .unwrap();
            responses
        });
        let uri = invite_rx.await.unwrap();

        // `pasteget join <uri>` on the laptop.
        let joined = ask(&laptop.endpoint, Request::Join { invite: uri }).await;
        assert!(
            matches!(&joined[..], [Response::Joined(d)] if d.name == pc.daemon.status().await.device_name)
        );
        let pairing = pairing.await.unwrap();
        assert!(matches!(pairing.last(), Some(Response::Paired(_))));

        // `pasteget send` on the PC reaches the laptop's clipboard.
        for _ in 0..300 {
            if pc.daemon.status().await.devices.iter().any(|d| d.connected) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let sent = ask(
            &pc.endpoint,
            Request::Send {
                mime_type: mime::TEXT.into(),
                data_base64: BASE64.encode(b"from the command line"),
            },
        )
        .await;
        assert!(matches!(&sent[..], [Response::Sent { devices: 1 }]));
        for _ in 0..300 {
            if laptop.clipboard.get() == Some(Raw::Text("from the command line".into())) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            laptop.clipboard.get(),
            Some(Raw::Text("from the command line".into()))
        );

        // `pasteget status` lists the paired device as connected.
        let status = ask(&pc.endpoint, Request::Status).await;
        let [Response::Status(status)] = &status[..] else {
            panic!("expected a status");
        };
        assert_eq!(status.devices.len(), 1);
        assert!(status.devices[0].connected);

        // `pasteget forget <id>` unpairs it.
        let laptop_id = laptop.daemon.device_id().to_string();
        let forgotten = ask(
            &pc.endpoint,
            Request::Forget {
                device: laptop_id.clone(),
            },
        )
        .await;
        assert!(matches!(&forgotten[..], [Response::Forgotten(d)] if d.id == laptop_id));
        let status = ask(&pc.endpoint, Request::Status).await;
        assert!(matches!(&status[..], [Response::Status(s)] if s.devices.is_empty()));
        let again = ask(&pc.endpoint, Request::Forget { device: laptop_id }).await;
        assert!(matches!(&again[..], [Response::Error { .. }]));
    }

    /// A phone that scanned a code just before it was renewed sends the old secret, once per
    /// address. Wrong tries never close the code: each one is reported, the request goes on,
    /// and the phone pairs once it scans the code shown.
    #[tokio::test]
    async fn wrong_attempts_with_an_old_code_leave_the_new_one_working() {
        use pasteget_core::pairing::{self, PairingWindow};
        use std::time::Instant;

        let pc = run("pc").await;
        let endpoint = pc.endpoint.clone();
        let (tx, mut responses) = tokio::sync::mpsc::unbounded_channel();
        let pairing = tokio::spawn(async move {
            call(&endpoint, &Request::Pair, |response| {
                let _ = tx.send(response);
            })
            .await
        });
        async fn next(
            responses: &mut tokio::sync::mpsc::UnboundedReceiver<Response>,
        ) -> Option<Response> {
            tokio::time::timeout(Duration::from_secs(10), responses.recv())
                .await
                .expect("the daemon said nothing")
        }
        let Some(Response::Invite { uri, .. }) = next(&mut responses).await else {
            panic!("expected a pairing code");
        };
        let invite: PairingInvite = uri.parse().unwrap();
        let mut old = PairingWindow::open(Instant::now()).unwrap().invite(
            &Identity::generate().unwrap(),
            "pc",
            invite.addresses.clone(),
        );
        old.device_id = invite.device_id;
        let phone = Identity::generate().unwrap();
        let dial = || tokio::net::TcpStream::connect(pc.daemon.local_addr());

        for _ in 0..3 {
            let socket = dial().await.unwrap();
            assert!(
                pairing::pair_with(socket, &phone, &old, "phone")
                    .await
                    .is_err()
            );
            let failed = next(&mut responses).await;
            assert!(
                matches!(failed, Some(Response::PairingAttemptFailed { .. })),
                "{failed:?}"
            );
        }
        assert!(pc.daemon.status().await.pairing_open);

        let socket = dial().await.unwrap();
        pairing::pair_with(socket, &phone, &invite, "phone")
            .await
            .unwrap();
        let paired = next(&mut responses).await;
        assert!(
            matches!(&paired, Some(Response::Paired(device)) if device.name == "phone"),
            "{paired:?}"
        );
        tokio::time::timeout(Duration::from_secs(10), pairing)
            .await
            .expect("the request goes on once paired")
            .unwrap()
            .unwrap();
    }

    #[test]
    fn devices_are_found_by_id_or_unique_name() {
        let device = |name: &str| DeviceStatus {
            device: KnownDevice {
                id: Identity::generate().unwrap().device_id(),
                name: name.into(),
                addresses: Vec::new(),
            },
            connected: false,
        };
        let devices = [device("POCO M8"), device("Pixel"), device("Pixel")];

        assert_eq!(
            find_device(&devices, "poco m8").unwrap(),
            devices[0].device.id
        );
        assert_eq!(
            find_device(&devices, &devices[2].device.id.to_string()).unwrap(),
            devices[2].device.id
        );
        let ambiguous = find_device(&devices, "Pixel").unwrap_err().to_string();
        assert!(ambiguous.contains(&devices[1].device.id.to_string()));
        assert!(find_device(&devices, "Galaxy").is_err());
    }

    #[tokio::test]
    async fn pause_and_recent_through_the_control_channel() {
        let pc = run("pc").await;

        let paused = ask(&pc.endpoint, Request::Pause { paused: None }).await;
        assert!(matches!(&paused[..], [Response::Paused { paused: true }]));
        let status = ask(&pc.endpoint, Request::Status).await;
        assert!(matches!(&status[..], [Response::Status(s)] if s.paused));
        let resumed = ask(
            &pc.endpoint,
            Request::Pause {
                paused: Some(false),
            },
        )
        .await;
        assert!(matches!(&resumed[..], [Response::Paused { paused: false }]));

        // An explicit send is remembered even with no device connected.
        ask(
            &pc.endpoint,
            Request::Send {
                mime_type: mime::TEXT.into(),
                data_base64: BASE64.encode(b"  remember\nme  "),
            },
        )
        .await;
        let recent = ask(&pc.endpoint, Request::Recent).await;
        let [Response::Recent { clips }] = &recent[..] else {
            panic!("expected the recent clips");
        };
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].preview.as_deref(), Some("remember me"));
        assert_eq!(clips[0].len, 15);
        assert_eq!(clips[0].from, None);

        pc.clipboard.set(Raw::Text("something else".into()));
        let sent = ask(&pc.endpoint, Request::SendClipboard).await;
        assert!(matches!(&sent[..], [Response::Sent { devices: 0 }]));
        let copied = ask(&pc.endpoint, Request::CopyRecent { id: clips[0].id }).await;
        assert!(matches!(&copied[..], [Response::Copied]));
        for _ in 0..300 {
            if pc.clipboard.get() == Some(Raw::Text("  remember\nme  ".into())) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            pc.clipboard.get(),
            Some(Raw::Text("  remember\nme  ".into()))
        );

        let gone = ask(&pc.endpoint, Request::CopyRecent { id: 999 }).await;
        assert!(matches!(&gone[..], [Response::Error { .. }]));
    }

    /// Records the notices a daemon shows.
    #[derive(Debug, Default)]
    struct Notices(std::sync::Mutex<Vec<String>>);

    impl crate::notify::Notifier for Notices {
        fn received(&self, _received: crate::notify::Received) {}

        fn notice(&self, summary: String) {
            self.0.lock().unwrap().push(summary);
        }
    }

    /// Hyprland's key bindings run `pasteget hotkey …`: the same as a hotkey, notification
    /// included.
    #[tokio::test]
    async fn perform_does_what_a_hotkey_does_and_says_so() {
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "pc").unwrap();
        let notices = Arc::new(Notices::default());
        let options = DaemonOptions {
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            advertised_ips: Some(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
            poll_interval: Duration::from_millis(10),
            announce: false,
            notifier: Some(Arc::clone(&notices) as Arc<dyn crate::notify::Notifier>),
            hotkeys: false,
        };
        let daemon = Arc::new(
            Daemon::start(profile.clone(), MemoryClipboard::default(), options)
                .await
                .unwrap(),
        );
        let endpoint = Endpoint::for_profile(&profile);
        let _server = serve(Arc::clone(&daemon), &endpoint).await.unwrap();
        let perform = |action: &str| Request::Perform {
            action: action.into(),
        };

        let paused = ask(&endpoint, perform("toggle-pause")).await;
        assert!(matches!(&paused[..], [Response::Performed]));
        assert!(daemon.is_paused());
        let sent = ask(&endpoint, perform("send-clipboard")).await;
        assert!(matches!(&sent[..], [Response::Performed]));
        let unknown = ask(&endpoint, perform("nonsense")).await;
        assert!(
            matches!(&unknown[..], [Response::Error { message }] if message.contains("toggle-pause or send-clipboard"))
        );

        assert_eq!(
            *notices.0.lock().unwrap(),
            ["Syncing paused", "Nothing to send"]
        );
        let request = serde_json::to_string(&perform("toggle-pause")).unwrap();
        assert_eq!(request, r#"{"cmd":"perform","action":"toggle-pause"}"#);
    }

    #[tokio::test]
    async fn status_says_the_version_and_quit_stops_the_daemon() {
        let pc = run("pc").await;
        let mut quit = pc.daemon.quit_signal();

        let status = ask(&pc.endpoint, Request::Status).await;
        let [Response::Status(status)] = &status[..] else {
            panic!("expected a status");
        };
        assert_eq!(
            status.app_version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(status.control_version, Some(CONTROL_VERSION));
        assert_eq!(status.pid, Some(std::process::id()));
        assert!(!*quit.borrow());

        let answer = ask(&pc.endpoint, Request::Quit).await;
        assert!(matches!(&answer[..], [Response::Quitting]));
        tokio::time::timeout(Duration::from_secs(5), quit.wait_for(|quit| *quit))
            .await
            .expect("the daemon was not asked to quit")
            .unwrap();
    }

    /// What the tray does to restart its daemon after a new name or hotkeys: stop serving, let
    /// the daemon go, and start another on the same port. A pairing request still under way
    /// (the pairing window, even closed, waits minutes for a phone) must not keep the old daemon,
    /// its port and its control channel alive.
    #[tokio::test]
    async fn a_daemon_stopped_while_a_request_waits_lets_a_new_one_start() {
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "pc").unwrap();
        let options = |port| DaemonOptions {
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            advertised_ips: Some(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
            poll_interval: Duration::from_millis(10),
            announce: false,
            notifier: None,
            hotkeys: false,
        };
        let daemon = Arc::new(
            Daemon::start(profile.clone(), MemoryClipboard::default(), options(0))
                .await
                .unwrap(),
        );
        let port = daemon.local_addr().port();
        let endpoint = Endpoint::for_profile(&profile);
        let server = serve(Arc::clone(&daemon), &endpoint).await.unwrap();
        let (shown_tx, shown) = tokio::sync::oneshot::channel();
        let waiting = endpoint.clone();
        let pairing = tokio::spawn(async move {
            let mut shown_tx = Some(shown_tx);
            let _ = call(&waiting, &Request::Pair, |_| {
                if let Some(tx) = shown_tx.take() {
                    let _ = tx.send(());
                }
            })
            .await;
        });
        shown.await.unwrap();

        // As the tray's restart does it.
        server.abort();
        cleanup(&endpoint);
        drop(daemon);
        let mut started = Err(anyhow!("not tried"));
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            started =
                Daemon::start(profile.clone(), MemoryClipboard::default(), options(port)).await;
            if started.is_ok() {
                break;
            }
        }
        let restarted = Arc::new(started.expect("the old daemon still holds its port"));
        let _server = serve(restarted, &endpoint).await.unwrap();

        // The request ended with the old daemon, rather than wait for a phone it can't take.
        tokio::time::timeout(Duration::from_secs(5), pairing)
            .await
            .expect("the pairing request still waits on the old daemon")
            .unwrap();
        let status = ask(&endpoint, Request::Status).await;
        assert!(matches!(&status[..], [Response::Status(_)]));
    }

    #[test]
    fn status_replies_round_trip_and_older_ones_still_read() {
        let reply = Response::Status(StatusReply {
            device_id: "ID".into(),
            device_name: "PC".into(),
            listening_on: "0.0.0.0:47290".into(),
            pairing_open: false,
            paused: true,
            devices: vec![DeviceStatusReply {
                id: "PHONE".into(),
                name: "POCO".into(),
                connected: true,
            }],
            app_version: Some("0.2.0".into()),
            control_version: Some(CONTROL_VERSION),
            pid: Some(4242),
        });
        let Response::Status(read) =
            serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap()
        else {
            panic!("expected a status");
        };
        assert_eq!(read.app_version.as_deref(), Some("0.2.0"));
        assert_eq!(read.control_version, Some(CONTROL_VERSION));
        assert_eq!(read.pid, Some(4242));
        assert!(read.paused && read.devices[0].connected);

        // What a daemon from before the versions were sent answers.
        let older = r#"{"status":{"device_id":"ID","device_name":"PC","listening_on":"0.0.0.0:47290","pairing_open":false,"paused":false,"devices":[]}}"#;
        let Response::Status(older) = serde_json::from_str(older).unwrap() else {
            panic!("expected a status");
        };
        assert_eq!(older.app_version, None);
        assert_eq!(older.control_version, None);
        assert_eq!(older.pid, None);
        assert_eq!(older.device_name, "PC");
    }

    #[test]
    fn quit_messages_round_trip() {
        let request = serde_json::to_string(&Request::Quit).unwrap();
        assert_eq!(request, r#"{"cmd":"quit"}"#);
        assert!(matches!(
            serde_json::from_str::<Request>(&request).unwrap(),
            Request::Quit
        ));
        let answer = serde_json::to_string(&Response::Quitting).unwrap();
        assert!(matches!(
            serde_json::from_str::<Response>(&answer).unwrap(),
            Response::Quitting
        ));
    }

    /// The history's requests and answers, and `perform`, as clients send and read them: each
    /// reads back as it was written.
    #[test]
    fn history_and_perform_messages_round_trip() {
        let requests = [
            (
                Request::History {
                    query: "site:github.com".into(),
                    after: Some(HistoryCursorReply {
                        last_at_ms: 5,
                        id: 7,
                    }),
                    limit: Some(20),
                    utc_offset_minutes: 60,
                },
                r#"{"cmd":"history","query":"site:github.com","after":{"last_at_ms":5,"id":7},"limit":20,"utc_offset_minutes":60}"#,
            ),
            (
                Request::HistoryCopy { id: 7 },
                r#"{"cmd":"history_copy","id":7}"#,
            ),
            (
                Request::HistoryPin {
                    id: 7,
                    pinned: true,
                },
                r#"{"cmd":"history_pin","id":7,"pinned":true}"#,
            ),
            (
                Request::HistoryDelete { ids: vec![7, 8] },
                r#"{"cmd":"history_delete","ids":[7,8]}"#,
            ),
            (
                Request::HistoryClear {
                    since_ms: Some(1000),
                    pinned_too: true,
                },
                r#"{"cmd":"history_clear","since_ms":1000,"pinned_too":true}"#,
            ),
            (Request::HistoryStats, r#"{"cmd":"history_stats"}"#),
            (
                Request::Perform {
                    action: "send-clipboard".into(),
                },
                r#"{"cmd":"perform","action":"send-clipboard"}"#,
            ),
        ];
        for (request, json) in requests {
            assert_eq!(serde_json::to_string(&request).unwrap(), json);
            let read: Request = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&read).unwrap(), json);
        }

        let answers = [
            (
                Response::HistoryPage {
                    items: Vec::new(),
                    next: Some(HistoryCursorReply {
                        last_at_ms: 5,
                        id: 7,
                    }),
                },
                r#"{"history_page":{"items":[],"next":{"last_at_ms":5,"id":7}}}"#,
            ),
            (
                Response::HistoryPinned {
                    id: 7,
                    pinned: false,
                },
                r#"{"history_pinned":{"id":7,"pinned":false}}"#,
            ),
            (
                Response::HistoryDeleted { items: 2 },
                r#"{"history_deleted":{"items":2}}"#,
            ),
            (
                Response::HistoryStats(HistoryStatsReply {
                    items: 3,
                    pinned: 1,
                    images: 1,
                    bytes: 4096,
                    kept_out_this_week: 2,
                }),
                r#"{"history_stats":{"items":3,"pinned":1,"images":1,"bytes":4096,"kept_out_this_week":2}}"#,
            ),
            (Response::Performed, r#""performed""#),
        ];
        for (answer, json) in answers {
            assert_eq!(serde_json::to_string(&answer).unwrap(), json);
            let read: Response = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&read).unwrap(), json);
        }
    }

    /// A device the history knows by its ID only comes with an empty name, whatever the
    /// daemon's language: never a word a device could be named.
    #[test]
    fn a_device_never_named_has_an_empty_name() {
        use pasteget_core::history::{Device, Kept, Source, Thumb};
        let device = |name: &str| Device {
            id: DeviceId::from_bytes([3; DeviceId::LEN]),
            name: name.into(),
            platform: None,
        };
        let item = Item {
            id: ItemId(7),
            sha256: [0; 32],
            format: Format::Text,
            size: 5,
            category: Category::Text,
            host: None,
            source: Source::Received,
            origin: Some(device("")),
            via: Some(device("HP-AIO")),
            first_at: 0,
            last_at: 0,
            times: 1,
            pinned_at: None,
            expires_at: None,
            kept: Kept::Original,
            media_id: None,
            width: None,
            height: None,
            thumb: Thumb::None,
            preview: "hello".into(),
            highlights: Vec::new(),
        };

        let reply = HistoryItemReply::of(item);

        assert_eq!(reply.from.as_deref(), Some(""));
        assert_eq!(reply.via.as_deref(), Some("HP-AIO"));
    }

    /// The history requests, as `pasteget history` sends them.
    #[tokio::test]
    async fn the_history_through_the_control_channel() {
        let pc = run_with("pc", Some("[history]\nenabled = true\n")).await;
        for text in ["  remember\nme  ", "vérification 482"] {
            let sent = ask(
                &pc.endpoint,
                Request::Send {
                    mime_type: mime::TEXT.into(),
                    data_base64: BASE64.encode(text.as_bytes()),
                },
            )
            .await;
            assert!(matches!(&sent[..], [Response::Sent { devices: 0 }]));
        }
        pc.daemon.history().unwrap().wait_for_recorder();
        let page =
            |query: &str, after: Option<HistoryCursorReply>, limit: Option<u32>| Request::History {
                query: query.into(),
                after,
                limit,
                utc_offset_minutes: 0,
            };

        let listed = ask(&pc.endpoint, page("", None, None)).await;
        let [Response::HistoryPage { items, next: None }] = &listed[..] else {
            panic!("expected a page");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].preview, "  remember\nme  ");
        assert_eq!(items[1].describe(), "Text: remember me");
        assert_eq!(
            (items[1].from.as_deref(), items[1].category.as_str()),
            (None, "text")
        );
        let remember = items[1].id;

        // Accents ignored, found in UTF-16.
        let found = ask(&pc.endpoint, page("verif", None, None)).await;
        let [Response::HistoryPage { items, .. }] = &found[..] else {
            panic!("expected a page");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].highlights, [[0, 5]]);

        // One item per page, then the next.
        let first = ask(&pc.endpoint, page("", None, Some(1))).await;
        let [
            Response::HistoryPage {
                next: Some(next), ..
            },
        ] = &first[..]
        else {
            panic!("expected a first page");
        };
        let rest = ask(&pc.endpoint, page("", Some(*next), None)).await;
        assert!(
            matches!(&rest[..], [Response::HistoryPage { items, next: None }] if items[0].id == remember)
        );

        let pinned = ask(
            &pc.endpoint,
            Request::HistoryPin {
                id: remember,
                pinned: true,
            },
        )
        .await;
        assert!(matches!(
            &pinned[..],
            [Response::HistoryPinned { pinned: true, .. }]
        ));
        let copied = ask(&pc.endpoint, Request::HistoryCopy { id: remember }).await;
        assert!(matches!(&copied[..], [Response::Copied]));
        for _ in 0..300 {
            if pc.clipboard.get() == Some(Raw::Text("  remember\nme  ".into())) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            pc.clipboard.get(),
            Some(Raw::Text("  remember\nme  ".into()))
        );

        // Clearing leaves the pinned one, deleting takes it.
        let cleared = ask(
            &pc.endpoint,
            Request::HistoryClear {
                since_ms: None,
                pinned_too: false,
            },
        )
        .await;
        assert!(matches!(
            &cleared[..],
            [Response::HistoryDeleted { items: 1 }]
        ));
        let stats = ask(&pc.endpoint, Request::HistoryStats).await;
        assert!(matches!(&stats[..], [Response::HistoryStats(s)] if s.items == 1 && s.pinned == 1));
        let deleted = ask(
            &pc.endpoint,
            Request::HistoryDelete {
                ids: vec![remember],
            },
        )
        .await;
        assert!(matches!(
            &deleted[..],
            [Response::HistoryDeleted { items: 1 }]
        ));
        let gone = ask(&pc.endpoint, Request::HistoryCopy { id: remember }).await;
        assert!(matches!(&gone[..], [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn the_history_requests_say_when_it_is_off() {
        let pc = run("pc").await;

        let answer = ask(&pc.endpoint, Request::HistoryStats).await;

        let [Response::Error { message }] = &answer[..] else {
            panic!("expected an error");
        };
        assert!(message.contains("[history]"), "{message}");
    }

    #[test]
    fn history_requests_read_with_their_defaults() {
        let request: Request = serde_json::from_str(r#"{"cmd":"history"}"#).unwrap();
        assert!(matches!(
            request,
            Request::History {
                ref query,
                after: None,
                limit: None,
                utc_offset_minutes: 0
            } if query.is_empty()
        ));
        let clear: Request = serde_json::from_str(r#"{"cmd":"history_clear"}"#).unwrap();
        assert!(matches!(
            clear,
            Request::HistoryClear {
                since_ms: None,
                pinned_too: false
            }
        ));
    }

    #[test]
    fn fnv1a_matches_the_reference_values() {
        // Published FNV-1a 64-bit test vectors: the pipe name must never change between builds.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[tokio::test]
    async fn bad_requests_get_an_error() {
        let pc = run("pc").await;

        let joined = ask(
            &pc.endpoint,
            Request::Join {
                invite: "not a pairing code".into(),
            },
        )
        .await;

        assert!(matches!(&joined[..], [Response::Error { .. }]));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_to_start_twice_for_one_profile() {
        let pc = run("pc").await;

        assert!(serve(Arc::clone(&pc.daemon), &pc.endpoint).await.is_err());
    }

    #[tokio::test]
    async fn client_reports_a_stopped_daemon() {
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "nobody").unwrap();

        let result = call(&Endpoint::for_profile(&profile), &Request::Status, |_| {}).await;

        assert!(result.unwrap_err().to_string().contains("not running"));
    }
}
