// SPDX-License-Identifier: GPL-3.0-or-later

//! The clipboard history: one SQLite database per device, the same code on the phone and the PC.
//!
//! - One row per unique content, keyed by the SHA-256 of its bytes (= `ClipOffer.sha256`).
//! - Searched on any part of a word, accents ignored (FTS5 trigram index on the first 4 KiB).
//! - Passwords, codes, keys and card numbers are refused before anything is written, and only
//!   counted ([`HistoryStore::record`] is the one place that decides). Clip contents never reach
//!   logs, errors or `Debug` output.
//! - At rest it relies on the OS's disk encryption (Android's file-based encryption, BitLocker,
//!   LUKS). See `docs/AUDIT.md`, section 10.
//!
//! The API is small and synchronous: apps call it from one thread of their own. Everything it
//! takes and gives is plain data ([`Item`], [`Recorded`]…), so an app can put its own trait in
//! front of it and fake it in tests; a store in a temporary folder is fast too.
//!
//! Open one store per folder, and nothing else on its database: a delete empties the
//! write-ahead log only when no other connection is reading it.

mod fold;
mod images;
mod items;
mod query;
mod record;
mod retention;
mod schema;
mod search;
mod types;

use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::identity::DeviceId;

pub use images::{MAX_ORIGINAL_LEN, MAX_THUMBNAIL_LEN, ORIGINALS_DIR, PendingThumbnail};
pub use items::PREVIEW_CHARS;
pub use query::{MAX_QUERY_CHARS, MAX_TERMS};
pub use record::{BARE_CODE_TTL_MS, HEAD_LEN, MAX_TEXT_LEN};
pub use retention::{KEPT_OUT_DAYS, Stats, Swept};
pub use schema::SCHEMA_VERSION;
pub use search::{Cursor, Filter, MAX_PAGE_SIZE, PAGE_SIZE, Page, Query, SHORT_SEARCH_ITEMS};
pub use types::{Context, Device, Format, Item, ItemId, Kept, Reason, Recorded, Source, Thumb};

/// The database file, in the folder given to [`HistoryStore::open`].
pub const DATABASE_FILE: &str = "history.db";

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The budget of originals on a phone, which also has the gallery's copy of its screenshots.
pub const ORIGINALS_BUDGET_PHONE: u64 = 150 * 1024 * 1024;
/// The budget of originals on a PC, which has no other copy of the phone's screenshots.
pub const ORIGINALS_BUDGET_PC: u64 = 500 * 1024 * 1024;

/// What can go wrong. Never holds clip contents or search text: SQLite's own messages can quote
/// them, so only its error code is kept.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The history was written by a newer paste-get. It is left untouched; keep history off.
    #[error(
        "the history was written by a newer paste-get (schema {found}, this one knows {known})"
    )]
    NewerVersion { found: i64, known: i64 },
    /// SQLite failed, with this extended result code.
    #[error("history database error (SQLite code {0})")]
    Database(i32),
    /// A row this version can't read.
    #[error("the history holds data this version can't read")]
    Corrupt,
    /// A thumbnail that is not a JPEG, PNG or WebP image of at most 64 KiB.
    #[error("not a thumbnail the history keeps")]
    BadThumbnail,
    /// The history's folder or files.
    #[error("history files: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_extended_error_code() {
            Some(code) => Self::Database(code),
            // Type and conversion errors: a value that doesn't fit what the code expects.
            None => Self::Corrupt,
        }
    }
}

/// The user's settings, as far as the history needs them. The default is the phone's: three
/// months, 20,000 items, 150 MB of originals for 7 days.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Until this time (unix ms), every clip is kept out: "Pause history".
    pub paused_until: Option<i64>,
    /// Keep numbers of 4 to 8 digits alone for 15 minutes. Off: never keep them.
    pub keep_bare_codes: bool,
    /// Keep a thumbnail of each phone screenshot, so it stays in the history after it is
    /// deleted from the gallery. Off: screenshots follow the gallery.
    pub keep_screenshot_thumbnails: bool,
    /// Unpinned items are deleted this long after they were last copied. `None`: never.
    pub keep_for_ms: Option<i64>,
    /// At most this many unpinned items; the oldest go first.
    pub max_items: u32,
    /// A guard on the database's size: past it, the oldest unpinned images go, then texts.
    pub max_database_bytes: u64,
    /// What originals may take: [`ORIGINALS_BUDGET_PHONE`] or [`ORIGINALS_BUDGET_PC`]. Past
    /// it, the oldest unpinned originals go, and their thumbnails stay.
    pub originals_budget: u64,
    /// Originals are kept this long after their last copy, then only their thumbnail.
    pub originals_for_ms: i64,
    /// The disk has under 1 GiB free, as the app sees it: the originals budget is halved.
    pub low_on_space: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            paused_until: None,
            keep_bare_codes: true,
            keep_screenshot_thumbnails: false,
            keep_for_ms: Some(90 * DAY_MS),
            max_items: 20_000,
            max_database_bytes: 200 * 1024 * 1024,
            originals_budget: ORIGINALS_BUDGET_PHONE,
            originals_for_ms: 7 * DAY_MS,
            low_on_space: false,
        }
    }
}

/// The history of one device.
pub struct HistoryStore {
    conn: Connection,
    dir: PathBuf,
    this_device: DeviceId,
    policy: Policy,
    clock: Box<dyn Fn() -> i64 + Send>,
    /// One-time codes seen lately, as (SHA-256, unix ms). In memory only.
    recent_codes: Vec<([u8; 32], i64)>,
    /// The write-ahead log still holds deleted rows: another connection kept it from being
    /// emptied.
    log_to_empty: bool,
}

impl fmt::Debug for HistoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistoryStore")
            .field("dir", &self.dir)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl HistoryStore {
    /// Opens the history kept in `dir`, creating it if needed. `this_device` is the ID of the
    /// device it runs on: clips copied here have it as their origin.
    ///
    /// `dir` should be private to the app and left out of backups: the phone's no-backup files
    /// folder, a user-only folder on the PC.
    pub fn open(dir: &Path, this_device: DeviceId, policy: Policy) -> Result<Self, Error> {
        std::fs::create_dir_all(dir)?;
        let conn = schema::open(&dir.join(DATABASE_FILE))?;
        search::register_fold(&conn)?;
        let mut store = Self {
            conn,
            dir: dir.to_owned(),
            this_device,
            policy,
            clock: Box::new(system_now),
            recent_codes: Vec::new(),
            log_to_empty: false,
        };
        store.sweep_files()?;
        Ok(store)
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Applies new settings from the next clip on; call [`Self::retain`] to apply new limits
    /// at once.
    pub fn set_policy(&mut self, policy: Policy) {
        self.policy = policy;
    }

    /// Replaces the system clock (unix ms), for tests.
    pub fn set_clock(&mut self, clock: impl Fn() -> i64 + Send + 'static) {
        self.clock = Box::new(clock);
    }

    /// Closes the history, folding its write-ahead log back into the database file. Dropping
    /// the store does the same but can't report a failure.
    pub fn close(self) -> Result<(), Error> {
        self.conn.close().map_err(|(_, error)| error.into())
    }
}

fn system_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Days since 1970, UTC: the unit of the "kept out" counter.
fn now_day(now_ms: i64) -> i64 {
    now_ms.div_euclid(DAY_MS)
}

#[cfg(test)]
mod tests;
