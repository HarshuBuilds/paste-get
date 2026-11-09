// SPDX-License-Identifier: GPL-3.0-or-later

//! The clipboard history of this computer: `core::history` in the profile's data folder
//! ([`crate::config::Profile::history_dir`]), off unless `settings.toml` turns it on
//! (`[history] enabled = true`).
//!
//! The daemon hands it what is copied here, sent and received; a thread of its own records it,
//! makes the thumbnails of images and deletes what expires, so sessions never wait on the disk.
//! What it keeps out (passwords, codes, keys) is decided in the core (`HistoryStore::record`).
//! Contents never reach logs or errors.
//!
//! Also the text formats of `pasteget history`.

use std::ops::Range;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context as _, Result};
use pasteget_core::clip::{Clip, mime};
use pasteget_core::history::{
    Context, Format, HistoryStore, ItemId, MAX_THUMBNAIL_LEN, ORIGINALS_BUDGET_PC, Page, Policy,
    Query, Reason, Stats,
};
use pasteget_core::identity::DeviceId;

use crate::clipboard::Content;
use crate::config::HistorySettings;

/// Clips waiting to be recorded. When the history can't keep up, newer clips are not kept.
const QUEUE: usize = 16;
/// Largest side of a thumbnail, in pixels.
const THUMBNAIL_SIDE: u32 = 320;
/// Thumbnails made at a time between clips.
const THUMBNAILS_AT_ONCE: u32 = 8;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The history, open. Dropping the last handle stops its thread once its queue is done.
pub struct History {
    store: Arc<Mutex<HistoryStore>>,
    queue: SyncSender<Job>,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    pub fn open(dir: &Path, this_device: DeviceId, settings: &HistorySettings) -> Result<Self> {
        let store = HistoryStore::open(dir, this_device, policy(settings))
            .context("could not open the clipboard history")?;
        let store = Arc::new(Mutex::new(store));
        let (queue, jobs) = mpsc::sync_channel(QUEUE);
        let thread_store = Arc::clone(&store);
        std::thread::Builder::new()
            .name("pasteget-history".into())
            .spawn(move || record_loop(&thread_store, &jobs))
            .context("could not start the history's thread")?;
        Ok(Self { store, queue })
    }

    /// Queues a clip to record. Never waits: if the history is that far behind, the clip is not
    /// kept.
    pub fn record(&self, clip: &Clip, context: Context) {
        let size = clip.data.len();
        match self.queue.try_send(Job::Record(clip.clone(), context)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!(size, "history: busy, a clip was not kept")
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::warn!(size, "history: stopped, a clip was not kept");
            }
        }
    }

    /// Names a device clips come from, or renames it. `platform` is its `Hello.platform`.
    pub fn name_device(&self, id: DeviceId, name: &str, platform: i32) {
        let platform = (platform != 0).then_some(platform);
        // Best effort: it's named again when it next connects.
        let _ = self
            .queue
            .try_send(Job::NameDevice(id, name.to_owned(), platform));
    }

    /// Applies new settings, and their limits at once.
    pub fn set_settings(&self, settings: &HistorySettings) -> Result<()> {
        let mut store = lock(&self.store);
        store.set_policy(policy(settings));
        store.retain()?;
        Ok(())
    }

    /// One page of items, newest first.
    pub fn page(&self, query: &Query) -> Result<Page> {
        Ok(lock(&self.store).page(query)?)
    }

    /// What to put back on the clipboard for an item: its whole text, or its original image.
    /// `None` when it isn't kept whole (too large, removed to save space) or doesn't exist.
    pub fn content(&self, id: ItemId) -> Result<Option<Content>> {
        let store = lock(&self.store);
        if let Some(text) = store.text(id)? {
            return Ok(Some(Content {
                mime_type: mime::TEXT.to_owned(),
                data: text.into_bytes(),
                sensitive: false,
            }));
        }
        let (Some(item), Some(path)) = (store.item(id)?, store.original_path(id)?) else {
            return Ok(None);
        };
        let mime_type = match item.format {
            Format::Png => mime::PNG,
            Format::Jpeg => mime::JPEG,
            Format::Text => return Ok(None),
        };
        let data = std::fs::read(&path).context("could not read the image from the history")?;
        Ok(Some(Content {
            mime_type: mime_type.to_owned(),
            data,
            sensitive: false,
        }))
    }

    pub fn pin(&self, id: ItemId, pinned: bool) -> Result<()> {
        Ok(lock(&self.store).pin(id, pinned)?)
    }

    /// Deletes items, pinned or not, leaving nothing of them. Returns how many.
    pub fn delete(&self, ids: &[ItemId]) -> Result<usize> {
        Ok(lock(&self.store).delete(ids)?)
    }

    /// Deletes the items last copied in `last_at` (unix ms), pinned ones too only if
    /// `pinned_too`. Returns how many.
    pub fn clear(&self, last_at: Range<i64>, pinned_too: bool) -> Result<usize> {
        Ok(lock(&self.store).clear(last_at, pinned_too)?)
    }

    pub fn stats(&self) -> Result<Stats> {
        Ok(lock(&self.store).stats()?)
    }

    /// How many clips were kept out since `since_ms` (unix ms), by reason.
    pub fn kept_out(&self, since_ms: i64) -> Result<Vec<(Reason, u64)>> {
        Ok(lock(&self.store).kept_out(since_ms)?)
    }

    /// Waits until every clip queued so far is recorded, and its thumbnail made. For tests.
    #[doc(hidden)]
    pub fn wait_for_recorder(&self) {
        let (done, wait) = mpsc::sync_channel(1);
        if self.queue.send(Job::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

/// The policy of `settings` on a PC: 500 MB of originals, the other limits the core's.
fn policy(settings: &HistorySettings) -> Policy {
    Policy {
        keep_bare_codes: settings.keep_bare_codes,
        keep_for_ms: (settings.keep_for_days > 0)
            .then(|| i64::from(settings.keep_for_days) * DAY_MS),
        originals_budget: ORIGINALS_BUDGET_PC,
        ..Policy::default()
    }
}

/// Records what it's given until the [`History`] is gone, makes the thumbnails, and deletes
/// items when they expire.
fn record_loop(store: &Mutex<HistoryStore>, jobs: &Receiver<Job>) {
    loop {
        let job = match until_next_expiry(store) {
            Some(wait) => match jobs.recv_timeout(wait) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => {
                    if let Err(error) = lock(store).retain() {
                        tracing::warn!(%error, "history: retention failed");
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match jobs.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        match job {
            Job::Record(clip, context) => {
                if let Err(error) = lock(store).record(&clip, &context) {
                    tracing::warn!(%error, size = clip.data.len(), "history: a clip was not kept");
                }
            }
            Job::NameDevice(id, name, platform) => {
                if let Err(error) = lock(store).set_device(id, &name, platform) {
                    tracing::warn!(%error, "history: a device was not named");
                }
            }
            Job::Flush(done) => {
                make_thumbnails(store);
                let _ = done.send(());
            }
        }
        make_thumbnails(store);
    }
}

/// How long until the next item expires, at least a second. `None` if none does.
fn until_next_expiry(store: &Mutex<HistoryStore>) -> Option<Duration> {
    let at = lock(store).next_expiry().ok()??;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    let ms = u64::try_from(at.saturating_sub(now)).unwrap_or(0).max(1000);
    Some(Duration::from_millis(ms))
}

/// Makes the thumbnails images wait for: until one is made, their original can't be removed to
/// save space. The store is not held while an image is decoded.
fn make_thumbnails(store: &Mutex<HistoryStore>) {
    loop {
        let pending = match lock(store).pending_thumbnails(THUMBNAILS_AT_ONCE) {
            Ok(pending) => pending,
            Err(error) => {
                tracing::warn!(%error, "history: could not list the images");
                return;
            }
        };
        if pending.is_empty() {
            return;
        }
        for image in pending {
            let thumbnail = image
                .path
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|data| thumbnail(&data));
            let saved = match thumbnail {
                Some((jpeg, width, height)) => {
                    lock(store).set_thumbnail(image.id, &jpeg, width, height)
                }
                None => lock(store).thumbnail_failed(image.id),
            };
            if let Err(error) = saved {
                // Not asked again until the next clip.
                tracing::warn!(%error, "history: could not store a thumbnail");
                return;
            }
        }
    }
}

/// A JPEG thumbnail of an image of a paired device, at most [`THUMBNAIL_SIDE`] pixels wide and
/// high and [`MAX_THUMBNAIL_LEN`] bytes, with the image's size. `None` if it doesn't decode.
pub fn thumbnail(image: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // `image`'s default limits apply: at most 512 MiB decoded.
    let decoded = image::load_from_memory(image).ok()?;
    let small = decoded
        .thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE)
        .into_rgb8();
    for quality in [80, 60, 40] {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode_image(&small)
            .ok()?;
        if jpeg.len() <= MAX_THUMBNAIL_LEN {
            return Some((jpeg, decoded.width(), decoded.height()));
        }
    }
    None
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `text` on one line, every character as it is but these: `\` as `\\`, a tab as `\t`, a line
/// break as `\n` or `\r`, and any other control character (a terminal escape, say) as `\u{1b}`.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            c if c.is_control() => escaped.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
            c => escaped.push(c),
        }
    }
    escaped
}

/// One line of `pasteget history list --plain`: the item's ID, a tab, its preview
/// ([`escape`]d). A picker (fuzzel, wofi, fzf) shows one item per line, and
/// `pasteget history copy --stdin` reads the ID of the line it picked ([`picked_id`]).
pub fn plain_line(id: i64, preview: &str) -> String {
    format!("{id}\t{}", escape(preview))
}

/// The ID at the start of a [`plain_line`], or of a line that is only an ID.
pub fn picked_id(line: &str) -> Option<i64> {
    line.trim_start()
        .split(['\t', ' '])
        .next()?
        .trim()
        .parse()
        .ok()
}

/// `30s`, `15m`, `2h` or `7d`, in milliseconds.
pub fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let unit = match text.chars().last()? {
        's' => 1000,
        'm' => 60 * 1000,
        'h' => 60 * 60 * 1000,
        'd' => DAY_MS,
        _ => return None,
    };
    let count: i64 = text[..text.len() - 1].parse().ok()?;
    if count <= 0 {
        return None;
    }
    count.checked_mul(unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasteget_core::history::{Recorded, Source, Thumb};

    const PC: DeviceId = DeviceId::from_bytes([1; DeviceId::LEN]);
    const PHONE: DeviceId = DeviceId::from_bytes([2; DeviceId::LEN]);

    fn on() -> HistorySettings {
        HistorySettings {
            enabled: true,
            ..HistorySettings::default()
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 200, 30, 255]));
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    #[test]
    fn images_get_a_thumbnail_and_come_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(dir.path(), PC, &on()).unwrap();
        let image = png(1920, 1080);

        history.record(
            &Clip::png(image.clone(), PHONE),
            Context::received_from(PHONE),
        );
        history.record(
            &Clip::text("du texte", PC),
            Context::new(Source::CopiedHere),
        );
        history.wait_for_recorder();

        let page = history.page(&Query::default()).unwrap();
        let shot = &page.items[1];
        assert_eq!(shot.thumb, Thumb::Done);
        assert_eq!((shot.width, shot.height), (Some(1920), Some(1080)));
        let back = history.content(shot.id).unwrap().unwrap();
        assert_eq!((back.mime_type.as_str(), back.data), (mime::PNG, image));
        let text = history.content(page.items[0].id).unwrap().unwrap();
        assert_eq!(text.data, b"du texte");
        assert!(history.content(ItemId(999)).unwrap().is_none());
    }

    #[test]
    fn thumbnails_are_small_jpegs() {
        let (jpeg, width, height) = thumbnail(&png(2000, 500)).unwrap();

        assert_eq!((width, height), (2000, 500));
        assert!(jpeg.starts_with(b"\xff\xd8\xff"));
        assert!(jpeg.len() <= MAX_THUMBNAIL_LEN);
        let small = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((small.width(), small.height()), (320, 80));
        assert_eq!(thumbnail(b"\x89PNG\r\n\x1a\nbroken"), None);
    }

    #[test]
    fn settings_change_the_policy() {
        let dir = tempfile::tempdir().unwrap();
        let settings = HistorySettings {
            keep_bare_codes: false,
            ..on()
        };
        let history = History::open(dir.path(), PC, &settings).unwrap();

        history.record(&Clip::text("75011", PC), Context::new(Source::CopiedHere));
        history.wait_for_recorder();
        assert_eq!(history.kept_out(0).unwrap(), [(Reason::BareCode, 1)]);
        history.set_settings(&on()).unwrap();
        history.record(&Clip::text("75011", PC), Context::new(Source::CopiedHere));
        history.wait_for_recorder();
        assert_eq!(history.stats().unwrap().items, 1);

        assert_eq!(policy(&on()).keep_for_ms, Some(90 * DAY_MS));
        let forever = HistorySettings {
            keep_for_days: 0,
            ..on()
        };
        assert_eq!(policy(&forever).keep_for_ms, None);
        assert_eq!(policy(&forever).originals_budget, ORIGINALS_BUDGET_PC);
    }

    #[test]
    fn records_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(dir.path(), PC, &on()).unwrap();
        // A millisecond later at each look, so that the newest is first, not the highest ID.
        let now = std::sync::atomic::AtomicI64::new(1_800_000_000_000);
        lock(&history.store)
            .set_clock(move || now.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        for text in ["un", "deux", "un"] {
            history.record(&Clip::text(text, PC), Context::new(Source::CopiedHere));
        }
        history.wait_for_recorder();

        let page = history.page(&Query::default()).unwrap();
        let texts: Vec<_> = page
            .items
            .iter()
            .map(|i| (i.preview.as_str(), i.times))
            .collect();
        assert_eq!(texts, [("un", 2), ("deux", 1)]);
        // Recording the same store directly gives the same answer.
        let again = lock(&history.store)
            .record(&Clip::text("deux", PC), &Context::new(Source::CopiedHere))
            .unwrap();
        assert!(matches!(again, Recorded::Again(_)));
    }

    /// Each item is one line whatever it holds, and the line gives its ID back.
    #[test]
    fn plain_lines_are_one_line_each() {
        let line = plain_line(42, "a\tb\nc\r\\d \u{1b}[2J é 👋");

        assert_eq!(line, "42\ta\\tb\\nc\\r\\\\d \\u{1b}[2J é 👋");
        assert!(!line.contains('\n') && !line.contains('\u{1b}'));
        assert_eq!(line.matches('\t').count(), 1);
        assert_eq!(picked_id(&line), Some(42));
        assert_eq!(picked_id("  7\n"), Some(7));
        assert_eq!(picked_id("7  2 min ago  POCO"), Some(7));
        assert_eq!(picked_id("Text: hello"), None);
        assert_eq!(picked_id(""), None);
    }

    #[test]
    fn durations_read_like_the_help_says() {
        assert_eq!(parse_duration("30s"), Some(30_000));
        assert_eq!(parse_duration("15m"), Some(900_000));
        assert_eq!(parse_duration("2h"), Some(7_200_000));
        assert_eq!(parse_duration("7d"), Some(7 * DAY_MS));
        for bad in ["", "h", "0h", "-1h", "2w", "1.5h", "99999999999999999d"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
    }
}
