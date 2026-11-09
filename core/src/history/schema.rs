// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening the database: connection settings and migrations.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use super::Error;

/// The schema, one migration per version: `MIGRATIONS[0]` takes version 0 to 1. Released
/// migrations never change (`migrations_are_pinned`).
pub(super) const MIGRATIONS: &[&str] = &[include_str!("migrations/001_init.sql")];

/// The schema version this code writes.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Opens or creates the database at `path` and brings it to [`SCHEMA_VERSION`].
///
/// A database from a newer paste-get is left exactly as it is: [`Error::NewerVersion`].
pub(super) fn open(path: &Path) -> Result<Connection, Error> {
    let conn = Connection::open(path)?;
    // Settings of this connection only: they write nothing.
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA secure_delete = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA trusted_schema = OFF;",
    )?;

    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Error::NewerVersion {
            found: version,
            known: SCHEMA_VERSION,
        });
    }
    if version == 0 {
        // Only takes effect before the first table is created.
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
    }
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::Database(rusqlite::ffi::SQLITE_CANTOPEN));
    }
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL;
         PRAGMA journal_size_limit = 4194304;",
    )?;

    let mut conn = conn;
    for (from, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", from as i64 + 1)?;
        tx.commit()?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    use ring::digest::{SHA256, digest};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn migrations_are_pinned() {
        // A released migration is never edited: users' databases already ran it. If this fails,
        // undo the change and put it in a new migration instead.
        let pinned = ["7acb42d30687ad989082516938ffc096b3e36a7ef350e07e70a258843e45ad25"];
        assert_eq!(
            MIGRATIONS.len(),
            pinned.len(),
            "pin the new migration's hash here"
        );
        for (sql, pinned) in MIGRATIONS.iter().zip(pinned) {
            // Git may check the file out with CRLF line ends on Windows.
            let sql = sql.replace("\r\n", "\n");
            assert_eq!(hex(digest(&SHA256, sql.as_bytes()).as_ref()), pinned);
        }
    }
}
