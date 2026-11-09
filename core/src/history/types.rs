// SPDX-License-Identifier: GPL-3.0-or-later

//! The plain data the history takes and gives. The numbers are stored in the database: never
//! reuse one.

use std::fmt;
use std::ops::Range;

use super::Error;
use crate::detect::Category;
use crate::identity::DeviceId;

/// An item of the history. Stable for as long as the item exists, and never given to another
/// item after it is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub i64);

/// How a clip came to this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// Copied on this device.
    CopiedHere = 1,
    /// Received from a paired device.
    Received = 2,
    /// A screenshot taken on this phone.
    Screenshot = 3,
    /// Shared to paste-get from another app.
    Shared = 4,
    /// Sent by hand: the Quick Settings tile, `pasteget send`.
    SentByHand = 5,
}

/// The kind of bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Text = 1,
    Png = 2,
    Jpeg = 3,
}

/// Where the content of an item is, if anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kept {
    /// A text of at most 4 KiB, whole in the item.
    Inline = 0,
    /// A longer text, deflated in its own row.
    Body = 1,
    /// An image, as a file in the originals folder.
    Original = 2,
    /// A phone screenshot, as a reference to the gallery (MediaStore).
    Gallery = 3,
    /// Too large to keep: only the start of a text, or nothing of an image.
    TooLarge = 4,
    /// The original image was removed to save space; its thumbnail is left.
    OriginalEvicted = 5,
    /// The content is gone (a gallery picture deleted, a missing file); the thumbnail may be left.
    Gone = 6,
}

/// The thumbnail of an image, made by the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Thumb {
    /// Not needed: a text, or a screenshot the gallery shows.
    None = 0,
    /// To be made (`pending_thumbnails`).
    Pending = 1,
    Done = 2,
    /// The app couldn't make one.
    Failed = 3,
}

/// Why a clip was kept out of the history. Each one is counted, nothing else is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// Marked sensitive by its source (a password manager, Android's `EXTRA_IS_SENSITIVE`).
    Sensitive = 1,
    /// A 2FA code from a notification, or a copy of one just seen.
    OneTimeCode = 2,
    /// Looked like a key, a token, a card number or a password (`detect::looks_secret`).
    LooksSecret = 3,
    /// A number of 4 to 8 digits alone, while the policy keeps none.
    BareCode = 4,
    /// History was paused.
    Paused = 5,
}

/// How a clip reached [`super::HistoryStore::record`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub source: Source,
    /// The paired device it came from, for a received clip.
    pub via: Option<DeviceId>,
    /// The MediaStore `_ID` of a phone screenshot: the history keeps a reference to the gallery
    /// instead of a copy.
    pub media_id: Option<i64>,
}

impl Context {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            via: None,
            media_id: None,
        }
    }

    /// A clip received from `peer`.
    pub fn received_from(peer: DeviceId) -> Self {
        Self {
            via: Some(peer),
            ..Self::new(Source::Received)
        }
    }
}

/// What [`super::HistoryStore::record`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// A new item.
    Added(ItemId),
    /// Already there: moved to the top, one more copy.
    Again(ItemId),
    /// Refused, and counted (see `kept_out`).
    KeptOut(Reason),
    /// Not something the history keeps: an empty clip, or a type it doesn't know.
    Ignored,
}

/// A device clips came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    /// Empty until the device is named (`set_device`).
    pub name: String,
    /// The protocol's `Platform` number, when known.
    pub platform: Option<i32>,
}

/// One item, as lists and searches show it.
#[derive(Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    /// Of the exact bytes, the same as the protocol's `ClipOffer.sha256`.
    pub sha256: [u8; 32],
    pub format: Format,
    /// Bytes of the original.
    pub size: u64,
    pub category: Category,
    /// The site, for a link alone: "github.com".
    pub host: Option<String>,
    pub source: Source,
    /// The device where it was first copied; `None` for this one.
    pub origin: Option<Device>,
    /// The paired device it came through; `None` when it didn't come from one.
    pub via: Option<Device>,
    /// Unix time in milliseconds.
    pub first_at: i64,
    pub last_at: i64,
    /// How many times it was copied or received.
    pub times: u32,
    pub pinned_at: Option<i64>,
    /// Deleted at this time unless pinned: numbers that look like codes.
    pub expires_at: Option<i64>,
    pub kept: Kept,
    /// MediaStore `_ID` of a phone screenshot.
    pub media_id: Option<i64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub thumb: Thumb,
    /// The start of a text, or the part around what a search found. Empty for an image.
    pub preview: String,
    /// What a search found in `preview`, as UTF-16 ranges (Kotlin's and Compose's indexes).
    pub highlights: Vec<Range<u32>>,
}

/// Never prints contents, or the site of a link.
impl fmt::Debug for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Item")
            .field("id", &self.id.0)
            .field("format", &self.format)
            .field("size", &self.size)
            .field("category", &self.category)
            .field("source", &self.source)
            .field("kept", &self.kept)
            .field("times", &self.times)
            .finish_non_exhaustive()
    }
}

/// A number stored in the database, and back.
pub(super) trait Code: Sized + Copy + 'static {
    const ALL: &'static [Self];
    fn code(self) -> i64;

    fn from_code(code: i64) -> Result<Self, Error> {
        Self::ALL
            .iter()
            .copied()
            .find(|value| value.code() == code)
            .ok_or(Error::Corrupt)
    }
}

impl Code for Source {
    const ALL: &'static [Self] = &[
        Self::CopiedHere,
        Self::Received,
        Self::Screenshot,
        Self::Shared,
        Self::SentByHand,
    ];
    fn code(self) -> i64 {
        self as i64
    }
}

impl Code for Format {
    const ALL: &'static [Self] = &[Self::Text, Self::Png, Self::Jpeg];
    fn code(self) -> i64 {
        self as i64
    }
}

impl Code for Kept {
    const ALL: &'static [Self] = &[
        Self::Inline,
        Self::Body,
        Self::Original,
        Self::Gallery,
        Self::TooLarge,
        Self::OriginalEvicted,
        Self::Gone,
    ];
    fn code(self) -> i64 {
        self as i64
    }
}

impl Code for Thumb {
    const ALL: &'static [Self] = &[Self::None, Self::Pending, Self::Done, Self::Failed];
    fn code(self) -> i64 {
        self as i64
    }
}

impl Code for Reason {
    const ALL: &'static [Self] = &[
        Self::Sensitive,
        Self::OneTimeCode,
        Self::LooksSecret,
        Self::BareCode,
        Self::Paused,
    ];
    fn code(self) -> i64 {
        self as i64
    }
}

impl Code for Category {
    const ALL: &'static [Self] = &[
        Self::Text,
        Self::Link,
        Self::Email,
        Self::Phone,
        Self::Code,
        Self::Json,
        Self::Color,
        Self::Screenshot,
        Self::Image,
    ];
    fn code(self) -> i64 {
        match self {
            Self::Text => 1,
            Self::Link => 2,
            Self::Email => 3,
            Self::Phone => 4,
            Self::Code => 5,
            Self::Json => 6,
            Self::Color => 7,
            Self::Screenshot => 8,
            Self::Image => 9,
        }
    }
}
