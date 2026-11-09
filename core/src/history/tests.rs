// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn store() -> (tempfile::TempDir, HistoryStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(dir.path()).unwrap();
    (dir, store)
}

fn pragma(store: &HistoryStore, name: &str) -> i64 {
    store
        .conn
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

#[test]
fn creates_the_database_and_opens_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history")).unwrap();
    assert_eq!(pragma(&store, "user_version"), SCHEMA_VERSION);
    store.close().unwrap();

    let again = HistoryStore::open(&dir.path().join("history")).unwrap();
    assert_eq!(pragma(&again, "user_version"), SCHEMA_VERSION);
    let classifier: i64 = again
        .conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'classifier_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(classifier, 1);
}

#[test]
fn a_newer_schema_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    drop(HistoryStore::open(dir.path()).unwrap());
    let path = dir.path().join(DATABASE_FILE);
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
    }
    let before = std::fs::read(&path).unwrap();

    let error = HistoryStore::open(dir.path()).unwrap_err();

    assert!(matches!(error, Error::NewerVersion { found, known }
        if found == SCHEMA_VERSION + 1 && known == SCHEMA_VERSION));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!dir.path().join("history.db-wal").exists());
}

#[test]
fn every_connection_has_the_safe_settings() {
    let (_dir, store) = store();
    let mode: String = store
        .conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    // 2 = incremental.
    assert_eq!(pragma(&store, "auto_vacuum"), 2);
    assert_eq!(pragma(&store, "secure_delete"), 1);
    assert_eq!(pragma(&store, "foreign_keys"), 1);
    // 1 = normal.
    assert_eq!(pragma(&store, "synchronous"), 1);
    assert_eq!(pragma(&store, "journal_size_limit"), 4 * 1024 * 1024);
    // 2 = memory.
    assert_eq!(pragma(&store, "temp_store"), 2);
    assert_eq!(pragma(&store, "trusted_schema"), 0);
    assert_eq!(pragma(&store, "busy_timeout"), 5000);
}

#[test]
fn sqlite_has_fts5_with_the_trigram_tokenizer() {
    let (_dir, store) = store();
    // remove_diacritics for trigram needs 3.45, contentless_delete 3.43.
    assert!(
        rusqlite::version_number() >= 3_045_000,
        "{}",
        rusqlite::version()
    );
    let sql: String = store
        .conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'item_fts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(sql.contains("contentless_delete=1"), "{sql}");
    assert!(sql.contains("trigram remove_diacritics 1"), "{sql}");

    // The triggers keep the index in step with `item`: any part of a word, accents ignored.
    store
        .conn
        .execute(
            "INSERT INTO item (sha256, format, size, category, source, first_at, last_at,
                               text_head, kept)
             VALUES (x'01', 1, 12, 1, 1, 0, 0, 'Vérification', 0)",
            [],
        )
        .unwrap();
    let found = |query: &str| -> i64 {
        store
            .conn
            .query_row(
                "SELECT count(*) FROM item_fts WHERE item_fts MATCH ?",
                [query],
                |row| row.get(0),
            )
            .unwrap()
    };
    assert_eq!(found("\"verif\""), 1);
    assert_eq!(found("\"RIFI\""), 1);
    assert_eq!(found("\"cation\""), 1);
    assert_eq!(found("\"nothing\""), 0);
    // Deleting by id works only on a contentless_delete table.
    store.conn.execute("DELETE FROM item", []).unwrap();
    assert_eq!(found("\"verif\""), 0);
}

#[test]
fn errors_never_quote_sqlite_messages() {
    let message = "fts5: syntax error near \"hunter2\"".to_owned();
    let error = Error::from(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
        Some(message),
    ));
    assert!(matches!(error, Error::Database(1)));
    assert!(!error.to_string().contains("hunter2"));
    assert!(!format!("{error:?}").contains("hunter2"));
}
