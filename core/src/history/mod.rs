// SPDX-License-Identifier: GPL-3.0-or-later

//! The clipboard history: one SQLite database per device, the same code on the phone and the PC.
//!
//! - One row per unique content, keyed by the SHA-256 of its bytes (= `ClipOffer.sha256`).
//! - Searched on any part of a word, accents ignored (FTS5 trigram index on the first 4 KiB).
//! - Passwords, codes, keys and card numbers are refused before anything is written, and only
//!   counted. Clip contents never reach logs, errors or `Debug` output.
//! - At rest it relies on the OS's disk encryption (Android's file-based encryption, BitLocker,
//!   LUKS). See `docs/AUDIT.md`, section 10.
//!
//! The API is synchronous: apps call it from one thread of their own.

mod schema;

use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub use schema::SCHEMA_VERSION;

/// The database file, in the folder given to [`HistoryStore::open`].
pub const DATABASE_FILE: &str = "history.db";

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

/// The history of one device.
pub struct HistoryStore {
    conn: Connection,
    dir: PathBuf,
}

impl fmt::Debug for HistoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistoryStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl HistoryStore {
    /// Opens the history kept in `dir`, creating it if needed.
    ///
    /// `dir` should be private to the app and left out of backups: the phone's no-backup files
    /// folder, a user-only folder on the PC.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(dir)?;
        let conn = schema::open(&dir.join(DATABASE_FILE))?;
        Ok(Self {
            conn,
            dir: dir.to_owned(),
        })
    }

    /// Closes the history, folding its write-ahead log back into the database file. Dropping
    /// the store does the same but can't report a failure.
    pub fn close(self) -> Result<(), Error> {
        self.conn.close().map_err(|(_, error)| error.into())
    }
}

#[cfg(test)]
mod tests;
