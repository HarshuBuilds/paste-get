// SPDX-License-Identifier: GPL-3.0-or-later

//! Images: originals as files, thumbnails in the database, phone screenshots as references to
//! the gallery.
//!
//! The core decodes no image: the app makes thumbnails (Kotlin's `ImageDecoder`, the PC's
//! `image` crate) and hands them back with [`HistoryStore::set_thumbnail`].

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use super::types::Code;
use super::{Error, Format, HistoryStore, ItemId, Kept, Thumb};
use crate::clip::mime;

/// The folder of originals, in the history's folder.
pub const ORIGINALS_DIR: &str = "originals";

/// Largest image kept as an original. A larger one is listed, but neither its original nor a
/// thumbnail is kept.
pub const MAX_ORIGINAL_LEN: usize = 32 * 1024 * 1024;

/// Largest thumbnail.
pub const MAX_THUMBNAIL_LEN: usize = 64 * 1024;

/// Files being written start with this; any left at open are from a crash.
const TEMPORARY_PREFIX: &str = ".tmp-";

/// An image waiting for its thumbnail, and where the app reads it from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingThumbnail {
    pub id: ItemId,
    pub format: Format,
    /// The original, in the originals folder.
    pub path: Option<PathBuf>,
    /// Or the screenshot in the phone's gallery (MediaStore `_ID`).
    pub media_id: Option<i64>,
}

impl HistoryStore {
    /// Where the original of an image is, to copy it back or show it full size. `None` when it
    /// isn't kept (a gallery reference, removed to save space, too large) or for a text.
    pub fn original_path(&self, id: ItemId) -> Result<Option<PathBuf>, Error> {
        let row: Option<(Vec<u8>, i64, i64)> = self
            .conn
            .query_row(
                "SELECT sha256, format, kept FROM item WHERE id = ?1",
                [id.0],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((sha256, format, kept)) = row else {
            return Ok(None);
        };
        if Kept::from_code(kept)? != Kept::Original {
            return Ok(None);
        }
        let name = original_name(&sha256, Format::from_code(format)?).ok_or(Error::Corrupt)?;
        Ok(Some(self.originals_dir().join(name)))
    }

    /// Images waiting for a thumbnail, oldest first.
    pub fn pending_thumbnails(&self, limit: u32) -> Result<Vec<PendingThumbnail>, Error> {
        let mut statement = self.conn.prepare(
            "SELECT id, sha256, format, kept, media_id FROM item
             WHERE thumb = ?1 AND kept IN (?2, ?3) ORDER BY id LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                Thumb::Pending.code(),
                Kept::Original.code(),
                Kept::Gallery.code(),
                limit
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )?;
        let mut pending = Vec::new();
        for row in rows {
            let (id, sha256, format, kept, media_id) = row?;
            let format = Format::from_code(format)?;
            let path = match Kept::from_code(kept)? {
                Kept::Original => Some(
                    self.originals_dir()
                        .join(original_name(&sha256, format).ok_or(Error::Corrupt)?),
                ),
                _ => None,
            };
            pending.push(PendingThumbnail {
                id: ItemId(id),
                format,
                path,
                media_id,
            });
        }
        Ok(pending)
    }

    /// Stores the thumbnail the app made for an image, with the original's size in pixels.
    /// Refused unless it is a JPEG, PNG or WebP image of at most [`MAX_THUMBNAIL_LEN`] bytes.
    pub fn set_thumbnail(
        &mut self,
        id: ItemId,
        thumbnail: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), Error> {
        if thumbnail.len() > MAX_THUMBNAIL_LEN || !is_thumbnail_image(thumbnail) {
            return Err(Error::BadThumbnail);
        }
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE item SET thumb = ?2, width = ?3, height = ?4 WHERE id = ?1 AND format != ?5",
            params![id.0, Thumb::Done.code(), width, height, Format::Text.code()],
        )?;
        if updated > 0 {
            tx.execute(
                "INSERT INTO thumb (item, data) VALUES (?1, ?2)
                 ON CONFLICT (item) DO UPDATE SET data = excluded.data",
                params![id.0, thumbnail],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The app couldn't make a thumbnail: don't ask again.
    pub fn thumbnail_failed(&mut self, id: ItemId) -> Result<(), Error> {
        self.conn.execute(
            "UPDATE item SET thumb = ?2 WHERE id = ?1 AND thumb = ?3",
            params![id.0, Thumb::Failed.code(), Thumb::Pending.code()],
        )?;
        Ok(())
    }

    /// The thumbnail of an image, if one was made.
    pub fn thumbnail(&self, id: ItemId) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .conn
            .query_row("SELECT data FROM thumb WHERE item = ?1", [id.0], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Screenshots deleted from the phone's gallery (MediaStore `_ID`s): their items go too,
    /// unless the history kept its own thumbnail of them, which then stays. Returns how many
    /// items changed.
    pub fn forget_media(&mut self, media_ids: &[i64]) -> Result<usize, Error> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for &media_id in media_ids {
            changed += tx.execute(
                "UPDATE item SET kept = ?2, media_id = NULL
                 WHERE media_id = ?1 AND kept = ?3 AND thumb = ?4",
                params![
                    media_id,
                    Kept::Gone.code(),
                    Kept::Gallery.code(),
                    Thumb::Done.code()
                ],
            )?;
            changed += tx.execute(
                "DELETE FROM item WHERE media_id = ?1 AND kept = ?2",
                params![media_id, Kept::Gallery.code()],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    pub(super) fn originals_dir(&self) -> PathBuf {
        self.dir.join(ORIGINALS_DIR)
    }

    /// Makes the files and the rows agree, at open: files no row points to (left by a crash
    /// between writing and committing) are deleted, and rows whose file is missing say so.
    pub(super) fn sweep_files(&mut self) -> Result<(), Error> {
        let dir = self.originals_dir();
        super::create_private_dir(&dir)?;
        let mut expected = HashSet::new();
        let mut missing = Vec::new();
        {
            let mut statement = self
                .conn
                .prepare("SELECT id, sha256, format FROM item WHERE kept = ?1")?;
            let rows = statement.query_map([Kept::Original.code()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (id, sha256, format) = row?;
                let name =
                    original_name(&sha256, Format::from_code(format)?).ok_or(Error::Corrupt)?;
                if !dir.join(&name).is_file() {
                    missing.push(id);
                }
                expected.insert(name);
            }
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let known = name.to_str().is_some_and(|name| expected.contains(name));
            if !known && entry.file_type()?.is_file() {
                // Another program may hold it open (a viewer, an antivirus scan on Windows): the
                // history opens anyway, and tries again next time.
                if let Err(error) = fs::remove_file(entry.path()) {
                    tracing::warn!(%error, "history: could not remove a leftover file");
                }
            }
        }
        if !missing.is_empty() {
            let tx = self.conn.transaction()?;
            for id in &missing {
                tx.execute(
                    "UPDATE item SET kept = ?2,
                         thumb = CASE WHEN thumb = ?3 THEN ?4 ELSE thumb END
                     WHERE id = ?1",
                    params![
                        id,
                        Kept::Gone.code(),
                        Thumb::Pending.code(),
                        Thumb::Failed.code()
                    ],
                )?;
            }
            tx.commit()?;
            tracing::info!(count = missing.len(), "history: originals missing");
        }
        Ok(())
    }
}

/// "<sha256 in hex>.png" or ".jpg". `None` for a text or a hash of the wrong length.
pub(super) fn original_name(sha256: &[u8], format: Format) -> Option<String> {
    let extension = match format {
        Format::Png => "png",
        Format::Jpeg => "jpg",
        Format::Text => return None,
    };
    if sha256.len() != 32 {
        return None;
    }
    let hex: String = sha256.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{hex}.{extension}"))
}

/// Writes `data` as `dir/name` so that the file is either whole or absent: written under a
/// temporary name, synced, then renamed.
pub(super) fn write_original(dir: &Path, name: &str, data: &[u8]) -> io::Result<PathBuf> {
    let temporary = dir.join(format!("{TEMPORARY_PREFIX}{name}"));
    let path = dir.join(name);
    let mut file = File::create(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, &path)?;
    // The new name itself is durable once the folder is synced. Not possible on Windows, where
    // a folder can't be opened as a file: NTFS journals the rename anyway.
    if let Ok(folder) = File::open(dir) {
        let _ = folder.sync_all();
    }
    Ok(path)
}

/// Removes a just-written original unless the row pointing to it was committed.
pub(super) struct Uncommitted(pub(super) Option<PathBuf>);

impl Drop for Uncommitted {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

/// A JPEG, PNG or WebP image, by its signature.
fn is_thumbnail_image(data: &[u8]) -> bool {
    let webp = data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP";
    webp || mime::of_image(data).is_some()
}
