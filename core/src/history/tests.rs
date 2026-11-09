// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::clip::Clip;
use crate::detect::Category;

const HERE: DeviceId = DeviceId::from_bytes([1; DeviceId::LEN]);
const PC: DeviceId = DeviceId::from_bytes([2; DeviceId::LEN]);
const LAPTOP: DeviceId = DeviceId::from_bytes([3; DeviceId::LEN]);
/// 2027-01-15, in unix ms.
const T0: i64 = 1_800_000_000_000;

/// A history in a temporary folder, with a clock the test moves.
struct Test {
    dir: tempfile::TempDir,
    store: HistoryStore,
    now: Arc<AtomicI64>,
}

impl Test {
    fn new() -> Self {
        Self::with_policy(Policy::default())
    }

    fn with_policy(policy: Policy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut store = HistoryStore::open(dir.path(), HERE, policy).unwrap();
        let now = Arc::new(AtomicI64::new(T0));
        let clock = now.clone();
        store.set_clock(move || clock.load(Ordering::Relaxed));
        Self { dir, store, now }
    }

    fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::Relaxed);
    }

    fn now(&self) -> i64 {
        self.now.load(Ordering::Relaxed)
    }

    /// Text copied on this device.
    fn copy(&mut self, text: &str) -> Recorded {
        self.store
            .record(&Clip::text(text, HERE), &Context::new(Source::CopiedHere))
            .unwrap()
    }

    fn added(&mut self, text: &str) -> ItemId {
        match self.copy(text) {
            Recorded::Added(id) => id,
            other => panic!("not added: {other:?}"),
        }
    }

    fn item(&self, id: ItemId) -> Item {
        self.store.item(id).unwrap().unwrap()
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn pragma(store: &HistoryStore, name: &str) -> i64 {
    store
        .conn
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn store() -> (tempfile::TempDir, HistoryStore) {
    let test = Test::new();
    (test.dir, test.store)
}

#[test]
fn creates_the_database_and_opens_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history"), HERE, Policy::default()).unwrap();
    assert_eq!(pragma(&store, "user_version"), SCHEMA_VERSION);
    store.close().unwrap();

    let again = HistoryStore::open(&dir.path().join("history"), HERE, Policy::default()).unwrap();
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
    drop(HistoryStore::open(dir.path(), HERE, Policy::default()).unwrap());
    let path = dir.path().join(DATABASE_FILE);
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
    }
    let before = std::fs::read(&path).unwrap();

    let error = HistoryStore::open(dir.path(), HERE, Policy::default()).unwrap_err();

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

#[test]
fn a_copied_text_is_kept_with_its_shelf() {
    let mut test = Test::new();
    let id = test.added("  https://github.com/Nexoryx-business/paste-get\n");

    let item = test.item(id);
    assert_eq!(item.format, Format::Text);
    assert_eq!(item.category, Category::Link);
    assert_eq!(item.host.as_deref(), Some("github.com"));
    assert_eq!(item.source, Source::CopiedHere);
    assert_eq!(item.origin, None);
    assert_eq!(item.via, None);
    assert_eq!((item.first_at, item.last_at, item.times), (T0, T0, 1));
    assert_eq!(item.kept, Kept::Inline);
    assert_eq!(item.thumb, Thumb::None);
    assert_eq!(item.expires_at, None);
    assert_eq!(
        item.size,
        "  https://github.com/Nexoryx-business/paste-get\n".len() as u64
    );
    assert_eq!(
        item.sha256,
        Clip::text("  https://github.com/Nexoryx-business/paste-get\n", HERE).sha256()
    );
    assert!(item.preview.starts_with("  https://github.com"));
    assert_eq!(
        test.store.text(id).unwrap().as_deref(),
        Some("  https://github.com/Nexoryx-business/paste-get\n")
    );
}

#[test]
fn the_same_text_again_moves_to_the_top() {
    let mut test = Test::new();
    let first = test.added("Bonjour");
    test.advance(1000);
    let second = test.added("Salut");
    test.advance(1000);

    assert_eq!(test.copy("Bonjour"), Recorded::Again(first));

    let item = test.item(first);
    assert_eq!(
        (item.first_at, item.last_at, item.times),
        (T0, T0 + 2000, 2)
    );
    assert!(item.last_at > test.item(second).last_at);
    let rows: i64 = test
        .store
        .conn
        .query_row("SELECT count(*) FROM item", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 2);
}

#[test]
fn received_clips_keep_where_they_were_first_copied() {
    let mut test = Test::new();
    test.store
        .set_device(PC, "HP-AIO (Windows)", Some(2))
        .unwrap();
    // Copied on the laptop, relayed by the PC.
    let clip = Clip::text("relayed", LAPTOP);
    let Recorded::Added(id) = test
        .store
        .record(&clip, &Context::received_from(PC))
        .unwrap()
    else {
        panic!("not added");
    };

    let item = test.item(id);
    assert_eq!(item.source, Source::Received);
    let origin = item.origin.unwrap();
    assert_eq!(
        (origin.id, origin.name.as_str(), origin.platform),
        (LAPTOP, "", None)
    );
    let via = item.via.unwrap();
    assert_eq!(
        (via.id, via.name.as_str(), via.platform),
        (PC, "HP-AIO (Windows)", Some(2))
    );

    // Named later, and renamed: old items follow.
    test.store.set_device(LAPTOP, "Laptop", None).unwrap();
    test.store.set_device(PC, "HP-AIO (Arch)", None).unwrap();
    let item = test.item(id);
    assert_eq!(item.origin.unwrap().name, "Laptop");
    let via = item.via.unwrap();
    assert_eq!(
        (via.name.as_str(), via.platform),
        ("HP-AIO (Arch)", Some(2))
    );
}

#[test]
fn secrets_are_kept_out_and_only_counted() {
    let mut test = Test::new();
    let sentinel = "zqxj-SENTINEL-7f3k";
    let sensitive = Clip {
        sensitive: true,
        ..Clip::text(format!("{sentinel} sensitive"), HERE)
    };
    let code = Clip::one_time_code("48291374", "Messages", PC);
    let key = Clip::text(format!("-----BEGIN PRIVATE KEY-----\n{sentinel}"), HERE);
    let password = Clip::text("x7#Kp2!vQz9&mR4t", HERE);
    let context = Context::new(Source::CopiedHere);

    let results: Vec<Recorded> = [&sensitive, &code, &key, &password]
        .into_iter()
        .map(|clip| test.store.record(clip, &context).unwrap())
        .collect();
    // The code copied by hand right after it was seen in a notification.
    let copy_of_code = test.copy("48291374");

    assert_eq!(
        results,
        [
            Recorded::KeptOut(Reason::Sensitive),
            Recorded::KeptOut(Reason::OneTimeCode),
            Recorded::KeptOut(Reason::LooksSecret),
            Recorded::KeptOut(Reason::LooksSecret),
        ]
    );
    assert_eq!(copy_of_code, Recorded::KeptOut(Reason::OneTimeCode));
    assert_eq!(
        test.store.kept_out(T0).unwrap(),
        [
            (Reason::Sensitive, 1),
            (Reason::OneTimeCode, 2),
            (Reason::LooksSecret, 2)
        ]
    );
    let rows: i64 = test
        .store
        .conn
        .query_row("SELECT count(*) FROM item", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);

    // Not a byte of them on disk, in the database, its WAL or anywhere else.
    let dir = test.dir;
    test.store.close().unwrap();
    let mut bytes = Vec::new();
    for file in walk(dir.path()) {
        bytes.extend(std::fs::read(file).unwrap());
    }
    for secret in [sentinel, "48291374", "x7#Kp2!vQz9&mR4t", "Messages"] {
        assert!(!contains(&bytes, secret), "{secret} is on disk");
    }
}

#[test]
fn pausing_keeps_everything_out() {
    let mut test = Test::with_policy(Policy {
        paused_until: Some(T0 + 60_000),
        ..Policy::default()
    });
    assert_eq!(test.copy("hello"), Recorded::KeptOut(Reason::Paused));
    test.advance(60_000);
    assert!(matches!(test.copy("hello"), Recorded::Added(_)));
    assert_eq!(test.store.kept_out(T0).unwrap(), [(Reason::Paused, 1)]);
}

#[test]
fn the_kept_out_counter_counts_by_day() {
    let mut test = Test::new();
    test.copy("x7#Kp2!vQz9&mR4t");
    test.advance(2 * DAY_MS);
    test.copy("x7#Kp2!vQz9&mR4t");
    test.copy("x7#Kp2!vQz9&mR4t");

    assert_eq!(test.store.kept_out(T0).unwrap(), [(Reason::LooksSecret, 3)]);
    assert_eq!(
        test.store.kept_out(test.now()).unwrap(),
        [(Reason::LooksSecret, 2)]
    );
}

#[test]
fn numbers_that_look_like_codes_are_kept_15_minutes() {
    let mut test = Test::new();
    let code = test.added("482913");
    assert_eq!(test.item(code).expires_at, Some(T0 + BARE_CODE_TTL_MS));

    // Copied again: kept 15 minutes from then.
    test.advance(10 * 60 * 1000);
    test.copy("482913");
    assert_eq!(
        test.item(code).expires_at,
        Some(test.now() + BARE_CODE_TTL_MS)
    );

    // Gone once expired, at the next clip.
    test.advance(BARE_CODE_TTL_MS);
    let other = test.added("something else");
    assert!(test.store.item(code).unwrap().is_none());
    assert!(test.store.item(other).unwrap().is_some());

    // Or never kept, if the user says so.
    test.store.set_policy(Policy {
        keep_bare_codes: false,
        ..Policy::default()
    });
    assert_eq!(test.copy("75002"), Recorded::KeptOut(Reason::BareCode));
}

#[test]
fn a_one_time_code_is_refused_again_for_15_minutes_only() {
    let mut test = Test::new();
    let code = Clip::one_time_code("482913", "Messages", PC);
    test.store
        .record(&code, &Context::received_from(PC))
        .unwrap();
    test.advance(BARE_CODE_TTL_MS);
    assert!(matches!(test.copy("482913"), Recorded::Added(_)));
}

#[test]
fn long_texts_are_cut_on_a_character_boundary() {
    let mut test = Test::new();
    for tail in ["é", "👋", "e\u{301}"] {
        let text = format!("{}{tail}{}", "a".repeat(HEAD_LEN - 1), "b".repeat(100));
        let id = test.added(&text);
        let head: String = test
            .store
            .conn
            .query_row("SELECT text_head FROM item WHERE id = ?1", [id.0], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(head.len() <= HEAD_LEN);
        assert!(text.starts_with(&head));
        assert!(head.len() >= HEAD_LEN - 3);
        assert_eq!(test.item(id).kept, Kept::Body);
        assert_eq!(test.item(id).preview.chars().count(), PREVIEW_CHARS);
        // The whole text comes back from its deflated body.
        assert_eq!(test.store.text(id).unwrap().as_deref(), Some(text.as_str()));
    }
}

#[test]
fn long_bodies_are_deflated() {
    let mut test = Test::new();
    let text = "Le presse-papiers de paste-get. ".repeat(1000);
    let id = test.added(&text);
    let body: i64 = test
        .store
        .conn
        .query_row(
            "SELECT length(data) FROM body WHERE item = ?1",
            [id.0],
            |row| row.get(0),
        )
        .unwrap();
    assert!(body < text.len() as i64 / 10, "{body} bytes");
    assert_eq!(test.store.text(id).unwrap().unwrap(), text);
}

#[test]
fn a_text_over_1_mib_keeps_its_start_only() {
    let mut test = Test::new();
    let text = "word ".repeat(MAX_TEXT_LEN / 5 + 1);
    let id = test.added(&text);

    let item = test.item(id);
    assert_eq!(item.kept, Kept::TooLarge);
    assert_eq!(item.size, text.len() as u64);
    assert!(item.preview.starts_with("word word"));
    // Can't be copied back cut.
    assert_eq!(test.store.text(id).unwrap(), None);
    let bodies: i64 = test
        .store
        .conn
        .query_row("SELECT count(*) FROM body", [], |row| row.get(0))
        .unwrap();
    assert_eq!(bodies, 0);
}

#[test]
fn empty_clips_and_images_are_ignored_for_now() {
    let mut test = Test::new();
    assert_eq!(test.copy(""), Recorded::Ignored);
    let not_utf8 = Clip {
        data: vec![0xff, 0xfe],
        ..Clip::text("", HERE)
    };
    let context = Context::new(Source::CopiedHere);
    assert_eq!(
        test.store.record(&not_utf8, &context).unwrap(),
        Recorded::Ignored
    );
    let png = Clip::png(b"\x89PNG\r\n\x1a\nrest".to_vec(), HERE);
    assert_eq!(
        test.store.record(&png, &context).unwrap(),
        Recorded::Ignored
    );
}

#[test]
fn debug_output_hides_contents() {
    let mut test = Test::new();
    let id = test.added("https://secret.example/hunter2");
    let shown = format!("{:?} {:?}", test.item(id), test.store);
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(!shown.contains("secret.example"), "{shown}");
    assert!(shown.contains("Link"));
}

/// Collects what `tracing` writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs `f` with every `tracing` event of this thread, at every level, written to a buffer.
pub(super) fn logs_of(f: impl FnOnce()) -> String {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        // Another test's thread may have met these log lines first, with no subscriber: the
        // interest tracing cached for them then would hide them here.
        tracing::callsite::rebuild_interest_cache();
        f();
    });
    let bytes = captured.0.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn logs_never_hold_contents() {
    let sentinel = "zqxj-SENTINEL-7f3k";
    let logs = logs_of(|| {
        let mut test = Test::new();
        let id = test.added(&format!("{sentinel} https://example.com"));
        test.copy(&format!("{sentinel} https://example.com"));
        test.copy(&format!("-----BEGIN PRIVATE KEY-----\n{sentinel}"));
        test.store.text(id).unwrap();
        test.store.item(id).unwrap();
    });
    assert!(logs.contains("history: recorded"), "{logs}");
    assert!(logs.contains("history: kept a clip out"), "{logs}");
    assert!(!logs.contains(sentinel), "{logs}");
    assert!(!logs.contains("example.com"), "{logs}");
}
