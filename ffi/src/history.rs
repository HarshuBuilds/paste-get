// SPDX-License-Identifier: GPL-3.0-or-later

//! The clipboard history on the phone: [`History`] wraps `core::history` for the app.
//!
//! [`crate::PhoneClient::open_history`] opens it and records into it what the phone copies,
//! sends and receives, on a thread of its own: sessions never wait on the disk, and a history
//! that fails (it can't open, the disk is full) never stops syncing. What it keeps out
//! (passwords, codes, keys) is decided in the core (`HistoryStore::record`).
//!
//! Contents never reach logs, errors or `Debug` output here either. Kotlin's records print every
//! field in `toString()`: never log them.

use std::fmt;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use pasteget_core::clip::Clip;
use pasteget_core::detect::Category;
use pasteget_core::history::{
    self, Context, Cursor, Filter, Format, HistoryStore, Item, ItemId, Kept, Policy, Query, Reason,
    Recorded, Source, Thumb,
};
use pasteget_core::identity::DeviceId;

use crate::{PastegetError, lock, platform_name};

/// Clips waiting to be recorded. When the history can't keep up, newer clips are not kept.
pub(crate) const QUEUE: usize = 16;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

impl From<history::Error> for PastegetError {
    /// The history's errors never quote what was copied or searched.
    fn from(error: history::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Implemented by the app to hear that the history changed by itself: a clip was recorded, or
/// kept out. Called from the history's thread. Not called for the app's own changes (pin,
/// delete…).
#[uniffi::export(with_foreign)]
pub trait HistoryListener: Send + Sync {
    fn on_changed(&self);
}

/// The phone's clipboard history. Calls read or write the disk: make them off the main thread.
#[derive(uniffi::Object)]
pub struct History {
    store: Arc<Mutex<HistoryStore>>,
    queue: SyncSender<Job>,
    listener: Arc<Mutex<Option<Arc<dyn HistoryListener>>>>,
}

impl fmt::Debug for History {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("History").finish_non_exhaustive()
    }
}

/// What the history's thread does, in order.
enum Job {
    Record(Clip, Context),
    /// Names a device clips come from: its name, and its protocol `Platform` number.
    NameDevice(DeviceId, String, Option<i32>),
    /// Answers once every job before it is done.
    Flush(SyncSender<()>),
}

impl History {
    /// Opens the history kept in `dir` for the device `this_device`, and starts its thread.
    pub(crate) fn open(
        dir: &Path,
        this_device: DeviceId,
        settings: &HistorySettings,
    ) -> Result<Arc<Self>, PastegetError> {
        let store = HistoryStore::open(dir, this_device, settings.policy())?;
        let store = Arc::new(Mutex::new(store));
        let listener = Arc::new(Mutex::new(None));
        let (queue, jobs) = mpsc::sync_channel(QUEUE);
        let (thread_store, thread_listener) = (Arc::clone(&store), Arc::clone(&listener));
        std::thread::Builder::new()
            .name("pasteget-history".into())
            .spawn(move || record_loop(&thread_store, &thread_listener, &jobs))?;
        Ok(Arc::new(Self {
            store,
            queue,
            listener,
        }))
    }

    /// Queues a clip to record. Never waits: if the history is that far behind, the clip is
    /// not kept.
    pub(crate) fn record(&self, clip: Clip, context: Context) {
        let size = clip.data.len();
        match self.queue.try_send(Job::Record(clip, context)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!(size, "history: busy, a clip was not kept");
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!(size, "history: stopped, a clip was not kept");
            }
        }
    }

    /// Names a device clips come from, or renames it.
    pub(crate) fn name_device(&self, id: DeviceId, name: String, platform: i32) {
        let platform = (platform != 0).then_some(platform);
        // Best effort: it's named again when it next connects.
        let _ = self.queue.try_send(Job::NameDevice(id, name, platform));
    }

    /// The whole text of an item.
    pub(crate) fn full_text(&self, id: i64) -> Result<Option<String>, PastegetError> {
        Ok(lock(&self.store).text(ItemId(id))?)
    }
}

/// Records what it's given until the [`History`] is gone.
fn record_loop(
    store: &Mutex<HistoryStore>,
    listener: &Mutex<Option<Arc<dyn HistoryListener>>>,
    jobs: &Receiver<Job>,
) {
    while let Ok(job) = jobs.recv() {
        let changed = match job {
            Job::Record(clip, context) => match lock(store).record(&clip, &context) {
                Ok(recorded) => recorded != Recorded::Ignored,
                Err(error) => {
                    tracing::warn!(%error, size = clip.data.len(), "history: a clip was not kept");
                    false
                }
            },
            Job::NameDevice(id, name, platform) => {
                if let Err(error) = lock(store).set_device(id, &name, platform) {
                    tracing::warn!(%error, "history: a device was not named");
                }
                false
            }
            Job::Flush(done) => {
                let _ = done.send(());
                false
            }
        };
        let listener = lock(listener).clone();
        if let (true, Some(listener)) = (changed, listener) {
            listener.on_changed();
        }
    }
}

#[uniffi::export]
impl History {
    /// Tells `listener` when the history changes by itself; `None` stops.
    pub fn set_listener(&self, listener: Option<Arc<dyn HistoryListener>>) {
        *lock(&self.listener) = listener;
    }

    /// One page of items, newest first. Items copied meanwhile come before it: the next page
    /// (`query.after = page.next`) starts where this one ended.
    pub fn page(&self, query: HistoryQuery) -> Result<HistoryPage, PastegetError> {
        let page = lock(&self.store).page(&query.to_core())?;
        Ok(HistoryPage {
            items: page.items.into_iter().map(HistoryItem::from).collect(),
            next: page.next.map(|next| HistoryCursor {
                last_at_ms: next.last_at,
                id: next.id,
            }),
        })
    }

    /// [`History::page`] for what the user typed in the search box, without filters.
    pub fn search(
        &self,
        text: String,
        after: Option<HistoryCursor>,
    ) -> Result<HistoryPage, PastegetError> {
        self.page(HistoryQuery {
            text,
            after,
            ..HistoryQuery::default()
        })
    }

    /// The sites of links alone, with how many items each has, most first.
    pub fn sites(&self, limit: u32) -> Result<Vec<HistorySite>, PastegetError> {
        let sites = lock(&self.store).sites(limit)?;
        Ok(sites
            .into_iter()
            .map(|(host, items)| HistorySite { host, items })
            .collect())
    }

    /// One item, if it exists and hasn't expired.
    pub fn item(&self, id: i64) -> Result<Option<HistoryItem>, PastegetError> {
        Ok(lock(&self.store).item(ItemId(id))?.map(HistoryItem::from))
    }

    /// The whole text of an item, for the item sheet. `None` for an image, an unknown or
    /// expired item, or a text too large to keep. To put it back on the clipboard, use
    /// [`crate::PhoneClient::copy_back`], which doesn't send it again.
    pub fn text(&self, id: i64) -> Result<Option<String>, PastegetError> {
        self.full_text(id)
    }

    /// Pins an item (never deleted by retention, never expires) or unpins it.
    pub fn pin(&self, id: i64, pinned: bool) -> Result<(), PastegetError> {
        Ok(lock(&self.store).pin(ItemId(id), pinned)?)
    }

    /// Deletes items, pinned or not, leaving nothing of them. Returns how many.
    pub fn delete(&self, ids: Vec<i64>) -> Result<u32, PastegetError> {
        let ids: Vec<ItemId> = ids.into_iter().map(ItemId).collect();
        Ok(count(lock(&self.store).delete(&ids)?))
    }

    /// Deletes the items last copied from `since_ms` to `until_ms` (unix ms, end excluded),
    /// and the pinned ones too only if `pinned_too`. Everything: `Long.MIN_VALUE` to
    /// `Long.MAX_VALUE`. Returns how many.
    pub fn clear(
        &self,
        since_ms: i64,
        until_ms: i64,
        pinned_too: bool,
    ) -> Result<u32, PastegetError> {
        Ok(count(
            lock(&self.store).clear(since_ms..until_ms, pinned_too)?,
        ))
    }

    /// Deletes what was copied on a device or came through it: "Also delete what came from
    /// it", when unpairing. Returns how many.
    pub fn delete_from_device(&self, device_id: String) -> Result<u32, PastegetError> {
        let device = device_id
            .parse()
            .map_err(|_| PastegetError::UnknownDevice)?;
        Ok(count(lock(&self.store).delete_from_device(device)?))
    }

    pub fn stats(&self) -> Result<HistoryStats, PastegetError> {
        let stats = lock(&self.store).stats()?;
        Ok(HistoryStats {
            items: stats.items,
            pinned: stats.pinned,
            images: stats.images,
            originals_bytes: stats.originals_bytes,
            originals_budget: stats.originals_budget,
            database_bytes: stats.database_bytes,
        })
    }

    /// How many clips were kept out since `since_ms` (unix ms, counted by UTC day), by reason:
    /// "14 passwords and codes were kept out this week".
    pub fn kept_out(&self, since_ms: i64) -> Result<Vec<KeptOutCount>, PastegetError> {
        let counts = lock(&self.store).kept_out(since_ms)?;
        Ok(counts
            .into_iter()
            .map(|(reason, count)| KeptOutCount {
                reason: reason.into(),
                count,
            })
            .collect())
    }

    pub fn settings(&self) -> HistorySettings {
        HistorySettings::from(lock(&self.store).policy())
    }

    /// Applies new settings, and their limits at once.
    pub fn set_settings(&self, settings: HistorySettings) -> Result<(), PastegetError> {
        let mut store = lock(&self.store);
        store.set_policy(settings.policy());
        store.retain()?;
        Ok(())
    }

    /// When the next item expires (unix ms), if one does: call [`History::retain`] then, so it
    /// is deleted without waiting for the next clip.
    pub fn next_expiry(&self) -> Result<Option<i64>, PastegetError> {
        Ok(lock(&self.store).next_expiry()?)
    }

    /// Applies the settings' limits now. Returns how many items were deleted.
    pub fn retain(&self) -> Result<u32, PastegetError> {
        Ok(count(lock(&self.store).retain()?.items_deleted))
    }

    /// The thumbnail of an image, once the app made one.
    pub fn thumbnail(&self, id: i64) -> Result<Option<Vec<u8>>, PastegetError> {
        Ok(lock(&self.store).thumbnail(ItemId(id))?)
    }

    /// Images waiting for the app to make their thumbnail, oldest first.
    pub fn pending_thumbnails(&self, limit: u32) -> Result<Vec<PendingThumbnail>, PastegetError> {
        let pending = lock(&self.store).pending_thumbnails(limit)?;
        Ok(pending
            .into_iter()
            .map(|pending| PendingThumbnail {
                id: pending.id.0,
                format: pending.format.into(),
                path: pending.path.map(|path| path.to_string_lossy().into_owned()),
                media_id: pending.media_id,
            })
            .collect())
    }

    /// Stores the thumbnail the app made: a JPEG, PNG or WebP image of at most 64 KiB, with
    /// the original's size in pixels.
    pub fn set_thumbnail(
        &self,
        id: i64,
        thumbnail: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), PastegetError> {
        Ok(lock(&self.store).set_thumbnail(ItemId(id), &thumbnail, width, height)?)
    }

    /// The app couldn't make a thumbnail: it isn't asked again.
    pub fn thumbnail_failed(&self, id: i64) -> Result<(), PastegetError> {
        Ok(lock(&self.store).thumbnail_failed(ItemId(id))?)
    }

    /// Where the original of an image is, to copy it back or show it full size.
    pub fn original_path(&self, id: i64) -> Result<Option<String>, PastegetError> {
        let path = lock(&self.store).original_path(ItemId(id))?;
        Ok(path.map(|path| path.to_string_lossy().into_owned()))
    }

    /// Screenshots deleted from the gallery (MediaStore `_ID`s): their items go too, unless
    /// the history kept its own thumbnail of them. Returns how many items changed.
    pub fn forget_media(&self, media_ids: Vec<i64>) -> Result<u32, PastegetError> {
        Ok(count(lock(&self.store).forget_media(&media_ids)?))
    }
}

impl History {
    /// Waits until every clip queued so far is recorded. For tests.
    #[doc(hidden)]
    pub fn wait_for_recorder_for_tests(&self) {
        let (done, wait) = mpsc::sync_channel(1);
        if self.queue.send(Job::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What the user chose in Settings → History. The other limits (20,000 items, 150 MB of
/// originals kept 7 days, 200 MB of database) are the core's defaults for a phone.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HistorySettings {
    /// Until this time (unix ms), every clip is kept out: "Pause history".
    #[uniffi(default = None)]
    pub paused_until_ms: Option<i64>,
    /// Keep numbers of 4 to 8 digits alone for 15 minutes. Off: never keep them.
    #[uniffi(default = true)]
    pub keep_bare_codes: bool,
    /// Keep a thumbnail of each screenshot, so it stays in the history after it is deleted from
    /// the gallery.
    #[uniffi(default = false)]
    pub keep_screenshot_thumbnails: bool,
    /// Unpinned items are deleted this many days after they were last copied. `None`: never.
    #[uniffi(default = Some(90))]
    pub keep_for_days: Option<u32>,
    /// The disk has under 1 GiB free: the budget of originals is halved.
    #[uniffi(default = false)]
    pub low_on_space: bool,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self::from(&Policy::default())
    }
}

impl HistorySettings {
    fn policy(&self) -> Policy {
        Policy {
            paused_until: self.paused_until_ms,
            keep_bare_codes: self.keep_bare_codes,
            keep_screenshot_thumbnails: self.keep_screenshot_thumbnails,
            keep_for_ms: self.keep_for_days.map(|days| i64::from(days) * DAY_MS),
            low_on_space: self.low_on_space,
            ..Policy::default()
        }
    }
}

impl From<&Policy> for HistorySettings {
    fn from(policy: &Policy) -> Self {
        Self {
            paused_until_ms: policy.paused_until,
            keep_bare_codes: policy.keep_bare_codes,
            keep_screenshot_thumbnails: policy.keep_screenshot_thumbnails,
            keep_for_days: policy
                .keep_for_ms
                .map(|ms| u32::try_from(ms / DAY_MS).unwrap_or(u32::MAX)),
            low_on_space: policy.low_on_space,
        }
    }
}

/// A request for one page: the search box and the chips. Every filter set must match.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct HistoryQuery {
    /// What the user typed: words (any part of a word, accents ignored) and `site:`, `from:`,
    /// `type:`, `is:pinned`, `after:`, `before:`. Empty: everything.
    #[uniffi(default = "")]
    pub text: String,
    #[uniffi(default = None)]
    pub category: Option<HistoryCategory>,
    #[uniffi(default = None)]
    pub format: Option<HistoryFormat>,
    #[uniffi(default = None)]
    pub source: Option<HistorySource>,
    /// First copied on this device.
    #[uniffi(default = None)]
    pub device_id: Option<String>,
    /// `true`: pinned items only; `false`: the others.
    #[uniffi(default = None)]
    pub pinned: Option<bool>,
    /// The site of a link, exactly.
    #[uniffi(default = None)]
    pub host: Option<String>,
    /// Copied at least this many times.
    #[uniffi(default = None)]
    pub min_times: Option<u32>,
    /// Last copied in this time span (unix ms, end excluded).
    #[uniffi(default = None)]
    pub since_ms: Option<i64>,
    #[uniffi(default = None)]
    pub until_ms: Option<i64>,
    /// The `next` of the page before; `None` for the first page.
    #[uniffi(default = None)]
    pub after: Option<HistoryCursor>,
    /// At most 200.
    #[uniffi(default = 50)]
    pub limit: u32,
    /// The user's offset from UTC, for `after:` and `before:` dates.
    #[uniffi(default = 0)]
    pub utc_offset_minutes: i32,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            category: None,
            format: None,
            source: None,
            device_id: None,
            pinned: None,
            host: None,
            min_times: None,
            since_ms: None,
            until_ms: None,
            after: None,
            limit: history::PAGE_SIZE,
            utc_offset_minutes: 0,
        }
    }
}

/// Never prints what was searched, or a site.
impl fmt::Debug for HistoryQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistoryQuery")
            .field("text_len", &self.text.len())
            .field("after", &self.after)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl HistoryQuery {
    fn to_core(&self) -> Query {
        Query {
            text: self.text.clone(),
            filter: Filter {
                category: self.category.map(Category::from),
                format: self.format.map(Format::from),
                source: self.source.map(Source::from),
                // An ID that doesn't parse matches nothing.
                device: self.device_id.as_ref().map(|id| {
                    id.parse()
                        .unwrap_or(DeviceId::from_bytes([0; DeviceId::LEN]))
                }),
                pinned: self.pinned,
                host: self.host.clone(),
                min_times: self.min_times,
                since_ms: self.since_ms,
                until_ms: self.until_ms,
            },
            after: self.after.map(|after| Cursor {
                last_at: after.last_at_ms,
                id: after.id,
            }),
            limit: self.limit,
            utc_offset_minutes: self.utc_offset_minutes,
        }
    }
}

/// Where the next page starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HistoryCursor {
    pub last_at_ms: i64,
    pub id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    /// Where the next page starts; `None` when this one is the last.
    pub next: Option<HistoryCursor>,
}

/// A device clips came from.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HistoryDevice {
    pub device_id: String,
    /// Empty when the history never learned it: a device paired with a computer, not with this
    /// phone.
    pub name: String,
    /// "windows", "linux", "macos" or "android", when known.
    pub platform: Option<String>,
}

/// What a search found in a preview, in UTF-16 code units (Kotlin's string indexes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Highlight {
    pub start: u32,
    pub end: u32,
}

/// One item of the history.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct HistoryItem {
    pub id: i64,
    /// Of the exact bytes, in hex: the same as the protocol's `ClipOffer.sha256`.
    pub sha256: String,
    pub format: HistoryFormat,
    /// Bytes of the original.
    pub size: u64,
    pub category: HistoryCategory,
    /// The site, for a link alone: "github.com".
    pub host: Option<String>,
    pub source: HistorySource,
    /// The device where it was first copied; `None` for this phone.
    pub origin: Option<HistoryDevice>,
    /// The computer it came through; `None` when it didn't come from one.
    pub via: Option<HistoryDevice>,
    /// Unix ms.
    pub first_at_ms: i64,
    pub last_at_ms: i64,
    /// How many times it was copied or received.
    pub times: u32,
    pub pinned_at_ms: Option<i64>,
    /// Deleted then unless pinned: a number that looks like a code.
    pub expires_at_ms: Option<i64>,
    pub kept: HistoryKept,
    /// The MediaStore `_ID` of a screenshot.
    pub media_id: Option<i64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub thumb: HistoryThumb,
    /// The start of a text, or the part around what a search found. Empty for an image.
    pub preview: String,
    pub highlights: Vec<Highlight>,
}

/// Never prints contents, or the site of a link.
impl fmt::Debug for HistoryItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistoryItem")
            .field("id", &self.id)
            .field("format", &self.format)
            .field("size", &self.size)
            .field("category", &self.category)
            .field("source", &self.source)
            .field("kept", &self.kept)
            .field("times", &self.times)
            .finish_non_exhaustive()
    }
}

impl From<Item> for HistoryItem {
    fn from(item: Item) -> Self {
        let device = |device: history::Device| HistoryDevice {
            device_id: device.id.to_string(),
            name: device.name,
            platform: device.platform.and_then(platform_name),
        };
        Self {
            id: item.id.0,
            sha256: item.sha256.iter().map(|b| format!("{b:02x}")).collect(),
            format: item.format.into(),
            size: item.size,
            category: item.category.into(),
            host: item.host,
            source: item.source.into(),
            origin: item.origin.map(device),
            via: item.via.map(device),
            first_at_ms: item.first_at,
            last_at_ms: item.last_at,
            times: item.times,
            pinned_at_ms: item.pinned_at,
            expires_at_ms: item.expires_at,
            kept: item.kept.into(),
            media_id: item.media_id,
            width: item.width,
            height: item.height,
            thumb: item.thumb.into(),
            preview: item.preview,
            highlights: item
                .highlights
                .into_iter()
                .map(|range| Highlight {
                    start: range.start,
                    end: range.end,
                })
                .collect(),
        }
    }
}

/// A site of the "Links by site" shelf.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct HistorySite {
    pub host: String,
    pub items: u64,
}

/// Never prints the site.
impl fmt::Debug for HistorySite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistorySite")
            .field("items", &self.items)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HistoryStats {
    pub items: u64,
    pub pinned: u64,
    pub images: u64,
    /// Bytes of the originals kept as files.
    pub originals_bytes: u64,
    /// What they may take now.
    pub originals_budget: u64,
    /// The database file and its log.
    pub database_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct KeptOutCount {
    pub reason: KeptOutReason,
    pub count: u64,
}

/// An image waiting for its thumbnail.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PendingThumbnail {
    pub id: i64,
    pub format: HistoryFormat,
    /// The original, in the history's folder.
    pub path: Option<String>,
    /// Or the screenshot in the gallery.
    pub media_id: Option<i64>,
}

/// The shelf an item is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryCategory {
    Text,
    Link,
    Email,
    Phone,
    Code,
    Json,
    Color,
    Screenshot,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryFormat {
    Text,
    Png,
    Jpeg,
}

/// How a clip came to this phone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistorySource {
    /// Copied on the phone.
    CopiedHere,
    /// Received from a computer.
    Received,
    /// A screenshot taken on the phone.
    Screenshot,
    /// Shared to paste-get from another app.
    Shared,
    /// Sent by hand: the share sheet, the Quick Settings tile.
    SentByHand,
}

/// Where the content of an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryKept {
    /// A short text, whole.
    Inline,
    /// A longer text, whole.
    Body,
    /// An image, as a file ([`History::original_path`]).
    Original,
    /// A screenshot, in the gallery ([`HistoryItem::media_id`]).
    Gallery,
    /// Too large to keep: the start of a text only, nothing of an image.
    TooLarge,
    /// The original image was removed to save space; its thumbnail is left.
    OriginalEvicted,
    /// The content is gone; the thumbnail may be left.
    Gone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryThumb {
    /// Not needed: a text, or a screenshot the gallery shows.
    None,
    /// To be made ([`History::pending_thumbnails`]).
    Pending,
    Done,
    /// The app couldn't make one.
    Failed,
}

/// Why clips were kept out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KeptOutReason {
    /// Marked sensitive: a password manager, Android's `EXTRA_IS_SENSITIVE`, a computer.
    Sensitive,
    /// A 2FA code, or a copy of one just seen.
    OneTimeCode,
    /// Looked like a key, a token, a card number or a password.
    LooksSecret,
    /// A number of 4 to 8 digits alone, while the settings keep none.
    BareCode,
    /// History was paused.
    Paused,
}

impl From<Category> for HistoryCategory {
    fn from(category: Category) -> Self {
        match category {
            Category::Text => Self::Text,
            Category::Link => Self::Link,
            Category::Email => Self::Email,
            Category::Phone => Self::Phone,
            Category::Code => Self::Code,
            Category::Json => Self::Json,
            Category::Color => Self::Color,
            Category::Screenshot => Self::Screenshot,
            Category::Image => Self::Image,
        }
    }
}

impl From<HistoryCategory> for Category {
    fn from(category: HistoryCategory) -> Self {
        match category {
            HistoryCategory::Text => Self::Text,
            HistoryCategory::Link => Self::Link,
            HistoryCategory::Email => Self::Email,
            HistoryCategory::Phone => Self::Phone,
            HistoryCategory::Code => Self::Code,
            HistoryCategory::Json => Self::Json,
            HistoryCategory::Color => Self::Color,
            HistoryCategory::Screenshot => Self::Screenshot,
            HistoryCategory::Image => Self::Image,
        }
    }
}

impl From<Format> for HistoryFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Text => Self::Text,
            Format::Png => Self::Png,
            Format::Jpeg => Self::Jpeg,
        }
    }
}

impl From<HistoryFormat> for Format {
    fn from(format: HistoryFormat) -> Self {
        match format {
            HistoryFormat::Text => Self::Text,
            HistoryFormat::Png => Self::Png,
            HistoryFormat::Jpeg => Self::Jpeg,
        }
    }
}

impl From<Source> for HistorySource {
    fn from(source: Source) -> Self {
        match source {
            Source::CopiedHere => Self::CopiedHere,
            Source::Received => Self::Received,
            Source::Screenshot => Self::Screenshot,
            Source::Shared => Self::Shared,
            Source::SentByHand => Self::SentByHand,
        }
    }
}

impl From<HistorySource> for Source {
    fn from(source: HistorySource) -> Self {
        match source {
            HistorySource::CopiedHere => Self::CopiedHere,
            HistorySource::Received => Self::Received,
            HistorySource::Screenshot => Self::Screenshot,
            HistorySource::Shared => Self::Shared,
            HistorySource::SentByHand => Self::SentByHand,
        }
    }
}

impl From<Kept> for HistoryKept {
    fn from(kept: Kept) -> Self {
        match kept {
            Kept::Inline => Self::Inline,
            Kept::Body => Self::Body,
            Kept::Original => Self::Original,
            Kept::Gallery => Self::Gallery,
            Kept::TooLarge => Self::TooLarge,
            Kept::OriginalEvicted => Self::OriginalEvicted,
            Kept::Gone => Self::Gone,
        }
    }
}

impl From<Thumb> for HistoryThumb {
    fn from(thumb: Thumb) -> Self {
        match thumb {
            Thumb::None => Self::None,
            Thumb::Pending => Self::Pending,
            Thumb::Done => Self::Done,
            Thumb::Failed => Self::Failed,
        }
    }
}

impl From<Reason> for KeptOutReason {
    fn from(reason: Reason) -> Self {
        match reason {
            Reason::Sensitive => Self::Sensitive,
            Reason::OneTimeCode => Self::OneTimeCode,
            Reason::LooksSecret => Self::LooksSecret,
            Reason::BareCode => Self::BareCode,
            Reason::Paused => Self::Paused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasteget_core::clip::mime;

    const PHONE: DeviceId = DeviceId::from_bytes([1; DeviceId::LEN]);
    const PC: DeviceId = DeviceId::from_bytes([2; DeviceId::LEN]);

    fn open(dir: &tempfile::TempDir) -> Arc<History> {
        History::open(dir.path(), PHONE, &HistorySettings::default()).unwrap()
    }

    fn copied(text: &str) -> Clip {
        Clip::text(text, PHONE)
    }

    #[derive(Default)]
    struct Changes(Mutex<u32>);

    impl HistoryListener for Changes {
        fn on_changed(&self) {
            *self.0.lock().unwrap() += 1;
        }
    }

    #[test]
    fn the_app_pages_searches_pins_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let history = open(&dir);
        let changes = Arc::new(Changes::default());
        history.set_listener(Some(changes.clone()));
        history.name_device(PC, "HP-AIO".into(), 2);
        for text in [
            "première copie",
            "https://github.com/rust-lang",
            "vérification",
        ] {
            history.record(copied(text), Context::new(Source::CopiedHere));
        }
        history.record(Clip::text("du PC", PC), Context::received_from(PC));
        history.wait_for_recorder_for_tests();
        assert_eq!(*changes.0.lock().unwrap(), 4);

        let all = history.page(HistoryQuery::default()).unwrap();
        assert_eq!(all.items.len(), 4);
        assert_eq!(all.next, None);
        let from_pc = &all.items[0];
        assert_eq!(from_pc.preview, "du PC");
        assert_eq!(from_pc.source, HistorySource::Received);
        let pc = Some(HistoryDevice {
            device_id: PC.to_string(),
            name: "HP-AIO".into(),
            platform: Some("windows".into()),
        });
        assert_eq!((&from_pc.origin, &from_pc.via), (&pc, &pc));
        assert_eq!(all.items[2].category, HistoryCategory::Link);
        assert_eq!(all.items[2].host.as_deref(), Some("github.com"));
        assert_eq!(all.items[3].origin, None);

        // Accents ignored, highlighted in UTF-16.
        let found = history.search("verif".into(), None).unwrap();
        assert_eq!(found.items.len(), 1);
        assert_eq!(found.items[0].highlights, [Highlight { start: 0, end: 5 }]);
        let sites = history.sites(10).unwrap();
        assert_eq!(
            sites,
            [HistorySite {
                host: "github.com".into(),
                items: 1
            }]
        );

        // Pages, chips.
        let first = history
            .page(HistoryQuery {
                limit: 3,
                ..HistoryQuery::default()
            })
            .unwrap();
        assert_eq!(first.items.len(), 3);
        let rest = history
            .page(HistoryQuery {
                after: first.next,
                ..HistoryQuery::default()
            })
            .unwrap();
        assert_eq!(rest.items.len(), 1);
        let id = all.items[3].id;
        history.pin(id, true).unwrap();
        let pinned = history
            .page(HistoryQuery {
                pinned: Some(true),
                ..HistoryQuery::default()
            })
            .unwrap();
        assert_eq!(pinned.items.len(), 1);
        assert!(pinned.items[0].pinned_at_ms.is_some());
        let from = history
            .page(HistoryQuery {
                device_id: Some(PC.to_string()),
                ..HistoryQuery::default()
            })
            .unwrap();
        assert_eq!(from.items.len(), 1);

        assert_eq!(history.item(id).unwrap().unwrap().preview, "première copie");
        assert_eq!(history.text(id).unwrap().as_deref(), Some("première copie"));
        assert_eq!(history.stats().unwrap().items, 4);
        assert_eq!(history.delete(vec![all.items[2].id]).unwrap(), 1);
        assert_eq!(history.delete_from_device(PC.to_string()).unwrap(), 1);
        // Pinned items stay unless asked.
        assert_eq!(history.clear(i64::MIN, i64::MAX, false).unwrap(), 1);
        assert_eq!(history.stats().unwrap().items, 1);
        assert_eq!(history.clear(i64::MIN, i64::MAX, true).unwrap(), 1);
        assert_eq!(history.stats().unwrap().items, 0);
        // The app's own changes aren't reported.
        assert_eq!(*changes.0.lock().unwrap(), 4);
    }

    #[test]
    fn secrets_are_counted_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let history = open(&dir);
        let password = Clip {
            sensitive: true,
            ..copied("hunter2-from-bitwarden")
        };
        history.record(password, Context::new(Source::CopiedHere));
        history.record(
            Clip::one_time_code("482913", "Messages", PHONE),
            Context::new(Source::CopiedHere),
        );
        history.record(
            copied("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            Context::new(Source::CopiedHere),
        );
        history.wait_for_recorder_for_tests();

        assert_eq!(history.stats().unwrap().items, 0);
        assert_eq!(
            history.kept_out(0).unwrap(),
            [
                KeptOutCount {
                    reason: KeptOutReason::Sensitive,
                    count: 1
                },
                KeptOutCount {
                    reason: KeptOutReason::OneTimeCode,
                    count: 1
                },
                KeptOutCount {
                    reason: KeptOutReason::LooksSecret,
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn settings_reach_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let history = open(&dir);
        assert_eq!(history.settings(), HistorySettings::default());
        assert_eq!(history.settings().keep_for_days, Some(90));

        let paused = HistorySettings {
            paused_until_ms: Some(i64::MAX),
            keep_for_days: None,
            ..HistorySettings::default()
        };
        history.set_settings(paused.clone()).unwrap();
        assert_eq!(history.settings(), paused);
        history.record(copied("while paused"), Context::new(Source::CopiedHere));
        history.wait_for_recorder_for_tests();
        assert_eq!(history.stats().unwrap().items, 0);
        assert_eq!(
            history.kept_out(0).unwrap()[0].reason,
            KeptOutReason::Paused
        );
    }

    /// The thread is stuck (here, the store is held): clips past the queue are dropped, and
    /// recording never waits.
    #[test]
    fn a_full_queue_never_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let history = open(&dir);
        let held = lock(&history.store);
        let started = std::time::Instant::now();
        for n in 0..100 {
            history.record(
                copied(&format!("copy {n}")),
                Context::new(Source::CopiedHere),
            );
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        drop(held);
        history.wait_for_recorder_for_tests();
        let kept = history.stats().unwrap().items;
        assert!((1..=QUEUE as u64 + 1).contains(&kept), "{kept} kept");
    }

    #[test]
    fn images_wait_for_their_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let history = open(&dir);
        let png = b"\x89PNG\r\n\x1a\nnot really".to_vec();
        let clip = Clip {
            mime_type: mime::PNG.into(),
            ..Clip::png(png, PC)
        };
        history.record(clip, Context::received_from(PC));
        let screenshot = Clip {
            mime_type: mime::JPEG.into(),
            ..Clip::png(b"\xff\xd8\xffshot".to_vec(), PHONE)
        };
        history.record(
            screenshot,
            Context {
                media_id: Some(42),
                ..Context::new(Source::Screenshot)
            },
        );
        history.wait_for_recorder_for_tests();

        let pending = history.pending_thumbnails(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].format, HistoryFormat::Png);
        let path = history.original_path(pending[0].id).unwrap().unwrap();
        assert_eq!(pending[0].path.as_deref(), Some(path.as_str()));
        assert!(
            history
                .set_thumbnail(pending[0].id, b"junk".to_vec(), 1, 1)
                .is_err()
        );
        let thumbnail = b"\xff\xd8\xffthumb".to_vec();
        history
            .set_thumbnail(pending[0].id, thumbnail.clone(), 640, 480)
            .unwrap();
        assert_eq!(history.thumbnail(pending[0].id).unwrap(), Some(thumbnail));
        assert!(history.pending_thumbnails(10).unwrap().is_empty());

        let shots = history
            .page(HistoryQuery {
                category: Some(HistoryCategory::Screenshot),
                ..HistoryQuery::default()
            })
            .unwrap();
        assert_eq!(shots.items[0].kept, HistoryKept::Gallery);
        assert_eq!(shots.items[0].media_id, Some(42));
        assert_eq!(history.forget_media(vec![42]).unwrap(), 1);
        assert_eq!(history.stats().unwrap().items, 1);
    }

    /// Errors say what failed, never what was searched or copied.
    #[test]
    fn a_folder_that_cant_be_opened_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not a folder");
        std::fs::write(&file, b"").unwrap();

        let error = History::open(&file, PHONE, &HistorySettings::default()).unwrap_err();

        assert!(matches!(error, PastegetError::Storage(_)), "{error:?}");
    }
}
