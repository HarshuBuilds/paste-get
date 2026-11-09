// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording clips, the one place that decides what is kept.

use rusqlite::{Connection, OptionalExtension, params};

use super::images::{self, MAX_ORIGINAL_LEN, Uncommitted};
use super::items::NOT_EXPIRED;
use super::types::Code;
use super::{
    Context, Error, Format, HistoryStore, ItemId, Kept, Policy, Reason, Recorded, Source, Thumb,
    now_day,
};
use crate::clip::{Clip, Kind, mime};
use crate::detect::{self, Category};
use crate::identity::DeviceId;

/// Texts up to this size are kept whole in their item; longer ones keep this much there, for
/// lists and search, and the whole text in a deflated body.
pub const HEAD_LEN: usize = 4 * 1024;

/// Longest text kept whole. A longer one keeps its first [`HEAD_LEN`] bytes only, and can't be
/// copied back ("too large to keep") rather than be pasted cut.
pub const MAX_TEXT_LEN: usize = 1024 * 1024;

// Every byte kept is checked for secrets first.
const _: () = assert!(MAX_TEXT_LEN <= detect::MAX_SECRET_SCAN_LEN);

/// How long a number that looks like a code is kept, unless pinned.
pub const BARE_CODE_TTL_MS: i64 = 15 * 60 * 1000;

/// How many one-time codes are remembered (in memory only) to refuse copies of them.
const RECENT_CODES: usize = 16;

impl HistoryStore {
    /// Offers a clip to the history. Secrets are refused and counted, and nothing else about
    /// them is written; the same content again moves to the top.
    ///
    /// Never refused for a secret but still not kept: empty clips and unknown types.
    pub fn record(&mut self, clip: &Clip, context: &Context) -> Result<Recorded, Error> {
        let now = (self.clock)();
        let decision = self.decide(clip, now);
        let (format, text) = match decision {
            Decision::Keep { format, text } => (format, text),
            Decision::KeptOut(reason) => {
                self.count_kept_out(reason, now)?;
                tracing::debug!(?reason, size = clip.data.len(), "history: kept a clip out");
                return Ok(Recorded::KeptOut(reason));
            }
            Decision::Ignore => return Ok(Recorded::Ignored),
        };
        let expires_at = text
            .filter(|text| detect::bare_code(text))
            .map(|_| now + BARE_CODE_TTL_MS);

        let sha256 = clip.sha256();
        let originals = self.originals_dir();
        // The original written for this clip, removed again unless its row is committed.
        let mut written = Uncommitted(None);
        let tx = self.conn.transaction()?;
        let existing: Option<(i64, i64)> = tx
            .query_row(
                "SELECT id, kept FROM item WHERE sha256 = ?1",
                [&sha256[..]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let recorded = match existing {
            Some((id, kept)) => {
                tx.execute(
                    "UPDATE item SET last_at = max(last_at, ?2), times = times + 1,
                         expires_at = CASE WHEN expires_at IS NULL THEN NULL
                                           ELSE max(expires_at, ?3) END
                     WHERE id = ?1",
                    params![id, now, expires_at],
                )?;
                // An image whose original was removed comes back whole.
                let restorable =
                    matches!(Kept::from_code(kept)?, Kept::OriginalEvicted | Kept::Gone)
                        && text.is_none()
                        && context.media_id.is_none()
                        && clip.data.len() <= MAX_ORIGINAL_LEN;
                if restorable {
                    let name = images::original_name(&sha256, format).ok_or(Error::Corrupt)?;
                    written.0 = Some(images::write_original(&originals, &name, &clip.data)?);
                    tx.execute(
                        "UPDATE item SET kept = ?2,
                             thumb = CASE WHEN thumb = ?3 THEN thumb ELSE ?4 END
                         WHERE id = ?1",
                        params![
                            id,
                            Kept::Original.code(),
                            Thumb::Done.code(),
                            Thumb::Pending.code()
                        ],
                    )?;
                }
                Recorded::Again(ItemId(id))
            }
            None => {
                let origin = if clip.origin == self.this_device {
                    None
                } else {
                    Some(device_row(&tx, &clip.origin)?)
                };
                let via = context.via.map(|via| device_row(&tx, &via)).transpose()?;
                let new = match text {
                    Some(text) => NewItem::text(text),
                    None => NewItem::image(context, clip.data.len(), &self.policy),
                };
                if new.kept == Kept::Original {
                    let name = images::original_name(&sha256, format).ok_or(Error::Corrupt)?;
                    written.0 = Some(images::write_original(&originals, &name, &clip.data)?);
                }
                tx.execute(
                    "INSERT INTO item (sha256, format, size, category, host, source, origin, via,
                                       first_at, last_at, expires_at, text_head, kept, thumb,
                                       media_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        &sha256[..],
                        format.code(),
                        clip.data.len() as i64,
                        new.category.code(),
                        new.host,
                        context.source.code(),
                        origin,
                        via,
                        now,
                        expires_at,
                        new.head,
                        new.kept.code(),
                        new.thumb.code(),
                        context.media_id,
                    ],
                )?;
                let id = tx.last_insert_rowid();
                if let Some(body) = new.body {
                    tx.execute(
                        "INSERT INTO body (item, data) VALUES (?1, ?2)",
                        params![id, body],
                    )?;
                }
                Recorded::Added(ItemId(id))
            }
        };
        tx.commit()?;
        written.0 = None;
        tracing::debug!(
            ?recorded,
            ?format,
            size = clip.data.len(),
            "history: recorded"
        );
        // Expired items go now. The clip is kept either way: a failure here is only logged.
        if let Err(error) = self.after_record() {
            tracing::warn!(%error, "history: retention failed");
        }
        Ok(recorded)
    }

    /// Names a device clips come from, or renames it. Names are kept after unpairing, so old
    /// items still say where they came from.
    pub fn set_device(
        &mut self,
        id: DeviceId,
        name: &str,
        platform: Option<i32>,
    ) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO device (key, name, platform) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET name = excluded.name,
                                             platform = coalesce(excluded.platform, platform)",
            params![&id.as_bytes()[..], name, platform],
        )?;
        Ok(())
    }

    /// The whole text of an item, to copy it back. `None` for an image, an unknown or expired
    /// item, or a text too large to keep.
    pub fn text(&self, id: ItemId) -> Result<Option<String>, Error> {
        let now = (self.clock)();
        let row: Option<(i64, i64, Option<String>)> = self
            .conn
            .query_row(
                &format!(
                    "SELECT format, kept, text_head FROM item WHERE id = ?1 AND {NOT_EXPIRED}"
                ),
                [id.0, now],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((format, kept, head)) = row else {
            return Ok(None);
        };
        if Format::from_code(format)? != Format::Text {
            return Ok(None);
        }
        match Kept::from_code(kept)? {
            Kept::Inline => head.map(Some).ok_or(Error::Corrupt),
            Kept::Body => {
                let body: Vec<u8> = self.conn.query_row(
                    "SELECT data FROM body WHERE item = ?1",
                    [id.0],
                    |row| row.get(0),
                )?;
                let bytes = miniz_oxide::inflate::decompress_to_vec_with_limit(&body, MAX_TEXT_LEN)
                    .map_err(|_| Error::Corrupt)?;
                String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| Error::Corrupt)
            }
            _ => Ok(None),
        }
    }

    /// How many clips were kept out since `since_ms` (unix ms, counted by UTC day), by reason.
    pub fn kept_out(&self, since_ms: i64) -> Result<Vec<(Reason, u64)>, Error> {
        let mut statement = self.conn.prepare(
            "SELECT reason, sum(count) FROM skipped WHERE day >= ?1 GROUP BY reason ORDER BY reason",
        )?;
        let rows = statement.query_map([since_ms.div_euclid(super::DAY_MS)], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = Vec::new();
        for row in rows {
            let (reason, count) = row?;
            counts.push((Reason::from_code(reason)?, count.max(0) as u64));
        }
        Ok(counts)
    }

    /// What to do with a clip, before anything is written.
    fn decide<'a>(&mut self, clip: &'a Clip, now: i64) -> Decision<'a> {
        if let Kind::OneTimeCode { .. } = clip.kind {
            // Remembered, in memory only, so a copy of the same digits is refused too, even
            // after a pause.
            self.recent_codes
                .retain(|(_, at)| now - at < BARE_CODE_TTL_MS && *at <= now);
            if self.recent_codes.len() == RECENT_CODES {
                self.recent_codes.remove(0);
            }
            self.recent_codes.push((clip.sha256(), now));
            return Decision::KeptOut(Reason::OneTimeCode);
        }
        if self.policy.paused_until.is_some_and(|until| now < until) {
            return Decision::KeptOut(Reason::Paused);
        }
        if clip.sensitive {
            return Decision::KeptOut(Reason::Sensitive);
        }
        if clip.data.is_empty() {
            return Decision::Ignore;
        }
        let format = match clip.mime_type.as_str() {
            mime::TEXT => Format::Text,
            mime::PNG => Format::Png,
            mime::JPEG => Format::Jpeg,
            _ => return Decision::Ignore,
        };
        if format != Format::Text {
            if mime::of_image(&clip.data) != Some(clip.mime_type.as_str()) {
                return Decision::Ignore;
            }
            return Decision::Keep { format, text: None };
        }
        let Some(text) = clip.as_text() else {
            return Decision::Ignore;
        };
        if detect::looks_secret(text) {
            return Decision::KeptOut(Reason::LooksSecret);
        }
        if detect::bare_code(text) {
            if !self.policy.keep_bare_codes {
                return Decision::KeptOut(Reason::BareCode);
            }
            let sha256 = clip.sha256();
            let just_seen = self
                .recent_codes
                .iter()
                .any(|(code, at)| *code == sha256 && now - at < BARE_CODE_TTL_MS);
            if just_seen {
                return Decision::KeptOut(Reason::OneTimeCode);
            }
        }
        Decision::Keep {
            format,
            text: Some(text),
        }
    }

    fn count_kept_out(&mut self, reason: Reason, now: i64) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO skipped (day, reason, count) VALUES (?1, ?2, 1)
             ON CONFLICT (day, reason) DO UPDATE SET count = count + 1",
            params![now_day(now), reason.code()],
        )?;
        Ok(())
    }
}

enum Decision<'a> {
    /// `text` is set for a text clip.
    Keep {
        format: Format,
        text: Option<&'a str>,
    },
    KeptOut(Reason),
    Ignore,
}

/// The columns that depend on the content.
struct NewItem<'a> {
    category: Category,
    host: Option<String>,
    head: Option<&'a str>,
    kept: Kept,
    body: Option<Vec<u8>>,
    thumb: Thumb,
}

impl<'a> NewItem<'a> {
    fn text(text: &'a str) -> Self {
        let category = detect::category(text);
        let host = match category {
            Category::Link => detect::link(text).map(|link| link.host),
            _ => None,
        };
        let (kept, body) = if text.len() <= HEAD_LEN {
            (Kept::Inline, None)
        } else if text.len() <= MAX_TEXT_LEN {
            let deflated = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 6);
            (Kept::Body, Some(deflated))
        } else {
            (Kept::TooLarge, None)
        };
        Self {
            category,
            host,
            head: Some(detect::head(text, HEAD_LEN)),
            kept,
            body,
            thumb: Thumb::None,
        }
    }

    /// A phone screenshot with its MediaStore ID is a reference to the gallery, which has its
    /// own thumbnail; any other image keeps its original, and waits for a thumbnail.
    fn image(context: &Context, len: usize, policy: &Policy) -> Self {
        let (kept, thumb) = if context.media_id.is_some() {
            let thumb = if policy.keep_screenshot_thumbnails {
                Thumb::Pending
            } else {
                Thumb::None
            };
            (Kept::Gallery, thumb)
        } else if len <= MAX_ORIGINAL_LEN {
            (Kept::Original, Thumb::Pending)
        } else {
            (Kept::TooLarge, Thumb::None)
        };
        Self {
            category: match context.source {
                Source::Screenshot => Category::Screenshot,
                _ => Category::Image,
            },
            host: None,
            head: None,
            kept,
            body: None,
            thumb,
        }
    }
}

/// The row of a device, added without a name if new.
fn device_row(conn: &Connection, id: &DeviceId) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO device (key, name) VALUES (?1, '') ON CONFLICT (key) DO NOTHING",
        [&id.as_bytes()[..]],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM device WHERE key = ?1",
        [&id.as_bytes()[..]],
        |row| row.get(0),
    )?)
}
