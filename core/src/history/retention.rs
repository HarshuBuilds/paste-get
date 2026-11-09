// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeping the history small and forgetting for real: retention, deletes, pins, stats.
//!
//! Retention never touches a pinned item, and never removes an original whose thumbnail is still
//! to be made. Files are removed after the commit that stops pointing to them.

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, ToSql, params};

use super::images::original_name;
use super::types::Code;
use super::{DAY_MS, Error, Format, HistoryStore, ItemId, Kept, Thumb, now_day};
use crate::identity::DeviceId;

/// How long the "kept out" counter keeps its numbers.
pub const KEPT_OUT_DAYS: i64 = 90;

/// Items deleted at a time while the database is over its size: small, so it stops soon after
/// it is back under.
const BATCH: i64 = 10;

/// What retention removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub items_deleted: usize,
    pub originals_removed: usize,
}

/// How much the history holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub items: u64,
    pub pinned: u64,
    pub images: u64,
    /// Bytes of the originals kept as files.
    pub originals_bytes: u64,
    /// What they may take now: the policy's budget, halved when space is low.
    pub originals_budget: u64,
    /// The database file and its write-ahead log.
    pub database_bytes: u64,
}

impl HistoryStore {
    /// Pins an item (never evicted, never expires) or unpins it.
    pub fn pin(&mut self, id: ItemId, pinned: bool) -> Result<(), Error> {
        let now = (self.clock)();
        self.conn.execute(
            "UPDATE item SET pinned_at = CASE WHEN ?2 THEN coalesce(pinned_at, ?3) END
             WHERE id = ?1",
            params![id.0, pinned, now],
        )?;
        Ok(())
    }

    /// Deletes items, pinned or not, with their bodies, thumbnails, originals and search
    /// entries, and leaves nothing of them in the database file or its log. Returns how many
    /// were deleted.
    pub fn delete(&mut self, ids: &[ItemId]) -> Result<usize, Error> {
        let originals = self.originals_dir();
        let tx = self.conn.transaction()?;
        let mut files = Vec::new();
        let mut deleted = 0;
        for id in ids {
            deleted += delete_where(&tx, &originals, "id = ?1", &[&id.0], &mut files)?;
        }
        tx.commit()?;
        self.forget_for_real(files)?;
        Ok(deleted)
    }

    /// Deletes the items last copied in `last_at` (unix ms): "the last hour", "today",
    /// "everything" (`i64::MIN..i64::MAX`). Pinned items too only if `pinned_too`.
    pub fn clear(&mut self, last_at: Range<i64>, pinned_too: bool) -> Result<usize, Error> {
        let originals = self.originals_dir();
        let tx = self.conn.transaction()?;
        let mut files = Vec::new();
        let deleted = delete_where(
            &tx,
            &originals,
            "last_at >= ?1 AND last_at < ?2 AND (?3 OR pinned_at IS NULL)",
            &[&last_at.start, &last_at.end, &pinned_too],
            &mut files,
        )?;
        tx.commit()?;
        self.forget_for_real(files)?;
        Ok(deleted)
    }

    /// Deletes what was first copied on a device, or came through it: "Also delete what came
    /// from it", when unpairing.
    pub fn delete_from_device(&mut self, device: DeviceId) -> Result<usize, Error> {
        let row: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM device WHERE key = ?1",
                [&device.as_bytes()[..]],
                |row| row.get(0),
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(0);
        };
        let originals = self.originals_dir();
        let tx = self.conn.transaction()?;
        let mut files = Vec::new();
        let deleted = delete_where(
            &tx,
            &originals,
            "origin = ?1 OR via = ?1",
            &[&row],
            &mut files,
        )?;
        tx.commit()?;
        self.forget_for_real(files)?;
        Ok(deleted)
    }

    /// Applies the policy's limits, in this order: expired and too old items; originals older
    /// than the policy allows; the oldest originals while over the budget; the oldest items over
    /// the count; then, while the database is over its size, the oldest images, then texts.
    /// Pinned items are never touched. Runs after every clip recorded; apps also call it at
    /// [`Self::next_expiry`].
    ///
    /// Expired items leave nothing behind, as with [`Self::delete`]. Items deleted for their age
    /// or the limits leave the database at once, and its write-ahead log at the next checkpoint.
    pub fn retain(&mut self) -> Result<Swept, Error> {
        let now = (self.clock)();
        let policy = self.policy.clone();
        let budget = self.originals_budget();
        let originals = self.originals_dir();
        let tx = self.conn.transaction()?;
        let mut files = Vec::new();
        let mut swept = Swept::default();

        let expired = delete_where(
            &tx,
            &originals,
            "pinned_at IS NULL AND expires_at <= ?1",
            &[&now],
            &mut files,
        )?;
        swept.items_deleted += expired;
        let too_old = policy
            .keep_for_ms
            .map_or(i64::MIN, |keep| now.saturating_sub(keep));
        swept.items_deleted += delete_where(
            &tx,
            &originals,
            "pinned_at IS NULL AND last_at < ?1",
            &[&too_old],
            &mut files,
        )?;

        // Originals that could go, oldest first: unpinned, with their thumbnail made (or failed).
        // `kept` is written in the SQL, not bound, so SQLite can use the `item_originals` index.
        let kept = Kept::Original.code();
        let removable: Vec<(i64, i64, i64, Vec<u8>, i64)> = {
            let mut statement = tx.prepare(&format!(
                "SELECT id, last_at, size, sha256, format FROM item
                 WHERE kept = {kept} AND pinned_at IS NULL AND thumb != ?1
                 ORDER BY last_at, id"
            ))?;
            let rows = statement.query_map([Thumb::Pending.code()], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let mut total: i64 = tx.query_row(
            &format!("SELECT coalesce(sum(size), 0) FROM item WHERE kept = {kept}"),
            [],
            |row| row.get(0),
        )?;
        let old = now.saturating_sub(policy.originals_for_ms);
        for (id, last_at, size, sha256, format) in &removable {
            let over_budget = u64::try_from(total).unwrap_or(0) > budget;
            if *last_at >= old && !over_budget {
                break;
            }
            tx.execute(
                "UPDATE item SET kept = ?2 WHERE id = ?1",
                params![id, Kept::OriginalEvicted.code()],
            )?;
            if let Some(name) = original_name(sha256, Format::from_code(*format)?) {
                files.push(originals.join(name));
            }
            total -= size;
            swept.originals_removed += 1;
        }

        // All, less the pinned: both counted on an index, without reading the rows.
        let unpinned: i64 = tx.query_row(
            "SELECT (SELECT count(*) FROM item)
                    - (SELECT count(*) FROM item WHERE pinned_at IS NOT NULL)",
            [],
            |row| row.get(0),
        )?;
        let over = unpinned - i64::from(policy.max_items);
        if over > 0 {
            swept.items_deleted += delete_where(
                &tx,
                &originals,
                "id IN (SELECT id FROM item WHERE pinned_at IS NULL
                        ORDER BY last_at, id LIMIT ?1)",
                &[&over],
                &mut files,
            )?;
        }

        // Under database pressure, images (their thumbnails are the largest rows) go first.
        for images_first in [true, false] {
            while live_bytes(&tx)? > policy.max_database_bytes {
                let deleted = delete_where(
                    &tx,
                    &originals,
                    "id IN (SELECT id FROM item WHERE pinned_at IS NULL AND ?1 = (format != ?2)
                            ORDER BY last_at, id LIMIT ?3)",
                    &[&images_first, &Format::Text.code(), &BATCH],
                    &mut files,
                )?;
                if deleted == 0 {
                    break;
                }
                swept.items_deleted += deleted;
            }
        }
        tx.commit()?;
        if expired > 0 || self.log_to_empty {
            self.forget_for_real(files)?;
        } else {
            remove_files(files);
        }
        if swept != Swept::default() {
            tracing::debug!(?swept, "history: retention");
        }
        Ok(swept)
    }

    /// When the next item expires (unix ms), if one does. An expired item is hidden at once, and
    /// deleted at the next [`Self::retain`]: apps call it then, so it doesn't wait for a clip.
    pub fn next_expiry(&self) -> Result<Option<i64>, Error> {
        Ok(self.conn.query_row(
            "SELECT min(expires_at) FROM item WHERE expires_at IS NOT NULL AND pinned_at IS NULL",
            [],
            |row| row.get(0),
        )?)
    }

    /// [`Self::retain`], then the daily cleaning: old "kept out" numbers, files no row points
    /// to, and the space freed given back to the file system (incremental vacuum). Runs at most
    /// once a day after clips are recorded; apps may also call it when idle.
    pub fn sweep(&mut self) -> Result<Swept, Error> {
        let swept = self.retain()?;
        let now = (self.clock)();
        self.conn.execute(
            "DELETE FROM skipped WHERE day < ?1",
            [now_day(now) - KEPT_OUT_DAYS],
        )?;
        self.sweep_files()?;
        incremental_vacuum(&self.conn)?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('last_sweep_at', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [now],
        )?;
        Ok(swept)
    }

    /// Runs after each clip recorded: [`Self::retain`], and [`Self::sweep`] once a day.
    pub(super) fn after_record(&mut self) -> Result<(), Error> {
        let now = (self.clock)();
        let last: Option<i64> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'last_sweep_at'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if last.is_none_or(|last| now - last >= DAY_MS || now < last) {
            self.sweep()?;
        } else {
            self.retain()?;
        }
        Ok(())
    }

    /// How much the history holds.
    pub fn stats(&self) -> Result<Stats, Error> {
        let (items, pinned, images, originals): (i64, i64, i64, i64) = self.conn.query_row(
            "SELECT count(*), count(pinned_at), count(*) FILTER (WHERE format != ?1),
                    coalesce(sum(size) FILTER (WHERE kept = ?2), 0)
             FROM item",
            params![Format::Text.code(), Kept::Original.code()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let file_size =
            |name: &str| fs::metadata(self.dir.join(name)).map_or(0, |metadata| metadata.len());
        let database =
            file_size(super::DATABASE_FILE) + file_size(&format!("{}-wal", super::DATABASE_FILE));
        let count = |n: i64| u64::try_from(n).unwrap_or(0);
        Ok(Stats {
            items: count(items),
            pinned: count(pinned),
            images: count(images),
            originals_bytes: count(originals),
            originals_budget: self.originals_budget(),
            database_bytes: database,
        })
    }

    fn originals_budget(&self) -> u64 {
        if self.policy.low_on_space {
            self.policy.originals_budget / 2
        } else {
            self.policy.originals_budget
        }
    }

    /// After a delete the user asked for, or an expiry: removes the files, then gives the freed
    /// pages back and empties the write-ahead log, which still holds the deleted rows. If another
    /// connection keeps the log from being emptied, it is tried again at the next retention.
    fn forget_for_real(&mut self, files: Vec<PathBuf>) -> Result<(), Error> {
        remove_files(files);
        // Deleted entries leave the index's pages at once, but the emptied pages stay until FTS5
        // merges them with the next clips' entries. With nothing left, merging costs nothing.
        let empty: bool =
            self.conn
                .query_row("SELECT NOT EXISTS (SELECT 1 FROM item)", [], |row| {
                    row.get(0)
                })?;
        if empty {
            self.conn
                .execute("INSERT INTO item_fts (item_fts) VALUES ('optimize')", [])?;
        }
        incremental_vacuum(&self.conn)?;
        // 1 in the first column: a reader kept the checkpoint from finishing.
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        self.log_to_empty = busy != 0;
        if self.log_to_empty {
            tracing::warn!(
                "history: another connection keeps the write-ahead log from being emptied"
            );
        }
        Ok(())
    }
}

/// Deletes the items matching `condition` (bound to `args`), and adds their originals to
/// `files`, to remove once committed. The triggers and foreign keys take their search entries,
/// bodies and thumbnails with them.
fn delete_where(
    conn: &Connection,
    originals: &Path,
    condition: &str,
    args: &[&dyn ToSql],
    files: &mut Vec<PathBuf>,
) -> Result<usize, Error> {
    let kept = Kept::Original.code();
    let sql = format!("SELECT sha256, format FROM item WHERE kept = {kept} AND ({condition})");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(args, |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (sha256, format) = row?;
        if let Some(name) = original_name(&sha256, Format::from_code(format)?) {
            files.push(originals.join(name));
        }
    }
    Ok(conn.execute(&format!("DELETE FROM item WHERE {condition}"), args)?)
}

/// Gives the free pages back to the file system. Each step of the pragma frees one page, so it
/// is stepped to the end.
fn incremental_vacuum(conn: &Connection) -> Result<(), Error> {
    let mut statement = conn.prepare("PRAGMA incremental_vacuum")?;
    let mut rows = statement.query([])?;
    while rows.next()?.is_some() {}
    Ok(())
}

/// Bytes of the database in use, not counting free pages.
fn live_bytes(conn: &Connection) -> Result<u64, Error> {
    let (pages, free, size): (i64, i64, i64) = conn.query_row(
        "SELECT page_count, freelist_count, page_size
         FROM pragma_page_count(), pragma_freelist_count(), pragma_page_size()",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(u64::try_from((pages - free) * size).unwrap_or(0))
}

fn remove_files(files: Vec<PathBuf>) {
    for file in files {
        if let Err(error) = fs::remove_file(&file) {
            // Swept at the next open.
            tracing::warn!(%error, "history: could not remove an original");
        }
    }
}
