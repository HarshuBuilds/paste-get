// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading items back.

use rusqlite::{OptionalExtension, Row};

use super::types::Code;
use super::{Device, Error, Format, HistoryStore, Item, ItemId, Kept, Source, Thumb};
use crate::detect::Category;
use crate::identity::DeviceId;

/// Longest preview of a text in a list, in characters: about three lines on a phone.
pub const PREVIEW_CHARS: usize = 300;

/// The columns [`item_from_row`] reads, for `SELECT … FROM item`.
pub(super) const ITEM_COLUMNS: &str = "item.id, item.sha256, item.format, item.size,
    item.category, item.host, item.source, item.first_at, item.last_at, item.times,
    item.pinned_at, item.expires_at, item.kept, item.media_id, item.width, item.height,
    item.thumb, item.text_head,
    o.key, o.name, o.platform, v.key, v.name, v.platform";

/// Joins the devices [`ITEM_COLUMNS`] reads.
pub(super) const ITEM_JOINS: &str = "LEFT JOIN device o ON o.id = item.origin
    LEFT JOIN device v ON v.id = item.via";

impl HistoryStore {
    /// One item, if it exists.
    pub fn item(&self, id: ItemId) -> Result<Option<Item>, Error> {
        let sql = format!("SELECT {ITEM_COLUMNS} FROM item {ITEM_JOINS} WHERE item.id = ?1");
        let item = self
            .conn
            .query_row(&sql, [id.0], |row| Ok(item_from_row(row)))
            .optional()?
            .transpose()?;
        Ok(item.map(|mut item| {
            item.preview = item.preview.chars().take(PREVIEW_CHARS).collect();
            item
        }))
    }
}

/// An item from a row of [`ITEM_COLUMNS`], with the whole head of its text as preview: callers
/// cut it.
pub(super) fn item_from_row(row: &Row<'_>) -> Result<Item, Error> {
    let sha256: Vec<u8> = row.get(1)?;
    let head: Option<String> = row.get(17)?;
    Ok(Item {
        id: ItemId(row.get(0)?),
        sha256: sha256.try_into().map_err(|_| Error::Corrupt)?,
        format: Format::from_code(row.get(2)?)?,
        size: u64::try_from(row.get::<_, i64>(3)?).map_err(|_| Error::Corrupt)?,
        category: Category::from_code(row.get(4)?)?,
        host: row.get(5)?,
        source: Source::from_code(row.get(6)?)?,
        first_at: row.get(7)?,
        last_at: row.get(8)?,
        times: u32::try_from(row.get::<_, i64>(9)?).unwrap_or(u32::MAX),
        pinned_at: row.get(10)?,
        expires_at: row.get(11)?,
        kept: Kept::from_code(row.get(12)?)?,
        media_id: row.get(13)?,
        width: row
            .get::<_, Option<i64>>(14)?
            .and_then(|n| u32::try_from(n).ok()),
        height: row
            .get::<_, Option<i64>>(15)?
            .and_then(|n| u32::try_from(n).ok()),
        thumb: Thumb::from_code(row.get(16)?)?,
        preview: head.unwrap_or_default(),
        highlights: Vec::new(),
        origin: device_from_row(row, 18)?,
        via: device_from_row(row, 21)?,
    })
}

/// The device whose key, name and platform start at column `at`, if any.
fn device_from_row(row: &Row<'_>, at: usize) -> Result<Option<Device>, Error> {
    let Some(key) = row.get::<_, Option<Vec<u8>>>(at)? else {
        return Ok(None);
    };
    Ok(Some(Device {
        id: DeviceId::try_from(key.as_slice()).map_err(|_| Error::Corrupt)?,
        name: row.get(at + 1)?,
        platform: row.get(at + 2)?,
    }))
}
