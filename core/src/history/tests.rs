// SPDX-License-Identifier: GPL-3.0-or-later

use std::ops::Range;
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
fn empty_clips_and_unknown_types_are_ignored() {
    let mut test = Test::new();
    assert_eq!(test.copy(""), Recorded::Ignored);
    let context = Context::new(Source::CopiedHere);
    let not_utf8 = Clip {
        data: vec![0xff, 0xfe],
        ..Clip::text("", HERE)
    };
    let not_a_png = Clip::png(b"GIF89a".to_vec(), HERE);
    let unknown = Clip {
        mime_type: "text/html".into(),
        ..Clip::text("<b>x</b>", HERE)
    };
    for clip in [not_utf8, not_a_png, unknown] {
        assert_eq!(
            test.store.record(&clip, &context).unwrap(),
            Recorded::Ignored
        );
    }
    assert!(walk(&test.dir.path().join(ORIGINALS_DIR)).is_empty());
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

/// Runs `f`, then gives every `tracing` event of this test binary so far, at every level.
///
/// One subscriber for the whole binary, set once: subscribers set per thread miss events while
/// other tests come and go. Each test checks for its own sentinel, so other tests' logs only make
/// the check stricter.
pub(super) fn logs_of(f: impl FnOnce()) -> String {
    static LOGS: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    let logs = LOGS.get_or_init(|| {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
        tracing::callsite::rebuild_interest_cache();
        captured
    });
    f();
    let bytes = logs.0.lock().unwrap().clone();
    String::from_utf8_lossy(&bytes).into_owned()
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

impl Test {
    fn search(&self, text: &str) -> Vec<ItemId> {
        self.page(&Query {
            text: text.into(),
            ..Query::default()
        })
        .items
        .iter()
        .map(|item| item.id)
        .collect()
    }

    fn filtered(&self, filter: Filter) -> Vec<ItemId> {
        self.page(&Query {
            filter,
            ..Query::default()
        })
        .items
        .iter()
        .map(|item| item.id)
        .collect()
    }

    fn page(&self, query: &Query) -> Page {
        self.store.page(query).unwrap()
    }

    fn pin(&self, id: ItemId) {
        self.store
            .conn
            .execute(
                "UPDATE item SET pinned_at = ?2 WHERE id = ?1",
                rusqlite::params![id.0, self.now()],
            )
            .unwrap();
    }
}

#[test]
fn pages_are_newest_first_and_stay_put_while_clips_arrive() {
    let mut test = Test::new();
    let ids: Vec<ItemId> = (0..5)
        .map(|i| {
            test.advance(1000);
            test.added(&format!("clip {i}"))
        })
        .collect();

    let first = test.page(&Query {
        limit: 2,
        ..Query::default()
    });
    assert_eq!(
        first.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [ids[4], ids[3]]
    );
    // New clips arrive: they go before the first page, not into the next ones.
    test.advance(1000);
    test.added("newer");
    let second = test.page(&Query {
        limit: 2,
        after: first.next,
        ..Query::default()
    });
    assert_eq!(
        second.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [ids[2], ids[1]]
    );
    let last = test.page(&Query {
        limit: 2,
        after: second.next,
        ..Query::default()
    });
    assert_eq!(
        last.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [ids[0]]
    );
    assert_eq!(last.next, None);
}

#[test]
fn search_finds_any_part_of_a_word_accents_ignored_both_ways() {
    let mut test = Test::new();
    let accented = test.added("Code de vérification : voir le message");
    let plain = test.added("verification needed");
    let site = test.added("https://github.com/Nexoryx-business/paste-get");
    let summer = test.added("Bon été à tous");

    let mut both = vec![plain, accented];
    assert_eq!(test.search("verif"), both);
    assert_eq!(test.search("VÉRIF"), both);
    assert_eq!(test.search("rificat"), both);
    both.retain(|id| *id == accented);
    assert_eq!(test.search("vérif message"), both);
    assert_eq!(test.search("hub"), [site]);
    assert_eq!(test.search("nexoryx-bus"), [site]);
    assert_eq!(test.search("\"bon ete\""), [summer]);
    assert_eq!(test.search("ÉTÉ"), [summer]);
    assert!(test.search("absent").is_empty());
    assert!(test.search("verif absent").is_empty());
}

#[test]
fn words_under_three_characters_are_searched_too() {
    let mut test = Test::new();
    let cafe = test.added("Un café");
    let code = test.added("id: 42");
    assert_eq!(test.search("é"), [cafe]);
    assert_eq!(test.search("E"), [cafe]);
    assert_eq!(test.search("42"), [code]);
    assert_eq!(test.search("id 42"), [code]);
    // With a longer word, the index narrows first.
    assert_eq!(test.search("café un"), [cafe]);
    assert_eq!(test.search("café zz"), []);
}

#[test]
fn any_query_works_and_errors_never_quote_it() {
    let mut test = Test::new();
    test.added("text with \"quotes\" and (brackets) * and NEAR words");
    for text in [
        "\"",
        "\"\"",
        "\"\"\"",
        "*",
        "a*",
        "NEAR(a b)",
        "a OR b",
        "a AND NOT b",
        "-x",
        "^start",
        "col:x",
        "text:x",
        "{text ocr}: x",
        "(",
        ")",
        "\\",
        "'",
        "'; DROP TABLE item; --",
        "%_",
        "\u{0}",
        "\u{1}\u{7f}",
        "👋🏽",
        "e\u{301}",
        "site:\"\" from:\"\" type: is: after: before:",
        "from:\"unfinished",
        &"x ".repeat(1000),
        &"é".repeat(5000),
    ] {
        let query = Query {
            text: text.into(),
            ..Query::default()
        };
        assert!(test.store.page(&query).is_ok(), "{text:?}");
    }
    assert_eq!(test.search("\"quotes\" (brackets)").len(), 1);
    assert_eq!(test.search("near words").len(), 1);
}

#[test]
fn operators_narrow_the_search() {
    let mut test = Test::new();
    test.store
        .set_device(PC, "HP-AIO (Windows)", Some(2))
        .unwrap();
    let github = test.added("https://github.com/rust-lang/rust");
    let gist = test.added("https://gist.github.com/x/1");
    let other = test.added("https://notgithub.com/rust");
    let Recorded::Added(from_pc) = test
        .store
        .record(
            &Clip::text("rust from the PC", PC),
            &Context::received_from(PC),
        )
        .unwrap()
    else {
        panic!("not added");
    };

    assert_eq!(test.search("site:github.com"), [gist, github]);
    assert_eq!(test.search("site:www.github.com rust"), [github]);
    assert_eq!(test.search("site:notgithub.com"), [other]);
    assert_eq!(test.search("from:hp-aio"), [from_pc]);
    assert_eq!(test.search("from:\"aio (win\" rust"), [from_pc]);
    assert_eq!(test.search("type:link rust"), [other, github]);
    assert_eq!(test.search("type:text"), [from_pc]);
    // An unknown type is a word, found nowhere.
    assert!(test.search("type:pdf").is_empty());

    test.pin(gist);
    assert_eq!(test.search("is:pinned"), [gist]);
}

#[test]
fn dates_are_days_in_the_users_time_zone() {
    let mut test = Test::new();
    // T0 is 2027-01-15 08:00 UTC.
    let today = test.added("today");
    test.advance(-DAY_MS);
    let yesterday = test.added("yesterday");
    let query = |text: &str, utc_offset_minutes| {
        test.page(&Query {
            text: text.into(),
            utc_offset_minutes,
            ..Query::default()
        })
        .items
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>()
    };
    assert_eq!(query("after:2027-01-15", 0), [today]);
    assert_eq!(query("before:2027-01-15", 0), [yesterday]);
    assert_eq!(query("after:2027-01-14 before:2027-01-15", 0), [yesterday]);
    // In Tokyo (UTC+9) the first clip was copied at 17:00 on the 15th, the other on the 14th.
    assert_eq!(query("after:2027-01-15", 9 * 60), [today]);
    // In Los Angeles (UTC-8), the 15th starts at 08:00 UTC: just the first.
    assert_eq!(query("after:2027-01-15", -8 * 60), [today]);
    assert_eq!(query("before:2027-01-15", -8 * 60), [yesterday]);
}

#[test]
fn chips_filter_by_kind_shelf_device_and_pins() {
    let mut test = Test::new();
    let link = test.added("https://example.com/a");
    let email = test.added("jean@example.fr");
    let Recorded::Added(received) = test
        .store
        .record(&Clip::text("from the PC", PC), &Context::received_from(PC))
        .unwrap()
    else {
        panic!("not added");
    };
    test.advance(1000);
    let frequent = test.added("copied often");
    test.copy("copied often");
    test.copy("copied often");
    test.pin(email);

    let filter = |filter: Filter| test.filtered(filter);
    assert_eq!(
        filter(Filter {
            category: Some(Category::Link),
            ..Filter::default()
        }),
        [link]
    );
    assert_eq!(
        filter(Filter {
            host: Some("example.com".into()),
            ..Filter::default()
        }),
        [link]
    );
    assert_eq!(
        filter(Filter {
            device: Some(PC),
            ..Filter::default()
        }),
        [received]
    );
    assert_eq!(
        filter(Filter {
            source: Some(Source::Received),
            ..Filter::default()
        }),
        [received]
    );
    assert_eq!(
        filter(Filter {
            pinned: Some(true),
            ..Filter::default()
        }),
        [email]
    );
    assert_eq!(
        filter(Filter {
            pinned: Some(false),
            format: Some(Format::Text),
            ..Filter::default()
        }),
        [frequent, received, link]
    );
    assert_eq!(
        filter(Filter {
            min_times: Some(3),
            ..Filter::default()
        }),
        [frequent]
    );
    assert_eq!(
        filter(Filter {
            since_ms: Some(T0 + 1000),
            ..Filter::default()
        }),
        [frequent]
    );
    assert_eq!(
        filter(Filter {
            until_ms: Some(T0 + 1000),
            ..Filter::default()
        }),
        [received, email, link]
    );
    assert_eq!(
        test.store.sites(10).unwrap(),
        [("example.com".to_owned(), 1)]
    );
}

#[test]
fn results_carry_highlights_in_utf16() {
    let mut test = Test::new();
    test.added("👋 Vérification e\u{301}té");
    let page = test.page(&Query {
        text: "verif".into(),
        ..Query::default()
    });
    let item = &page.items[0];
    assert_eq!(item.highlights, vec![Range { start: 3, end: 8 }]);
    let utf16: Vec<u16> = item.preview.encode_utf16().collect();
    assert_eq!(String::from_utf16(&utf16[3..8]).unwrap(), "Vérif");

    // A long text shows the part that matched.
    let long = format!("{} the needle {}", "hay ".repeat(500), "hay ".repeat(500));
    test.added(&long);
    let page = test.page(&Query {
        text: "needle".into(),
        ..Query::default()
    });
    assert!(page.items[0].preview.starts_with('…'));
    assert!(page.items[0].preview.contains("needle"));
    assert_eq!(page.items[0].highlights.len(), 1);
    // Without a search, the start, cut for a list.
    let browse = test.page(&Query::default());
    assert_eq!(browse.items[0].preview.chars().count(), PREVIEW_CHARS);
    assert!(browse.items[0].highlights.is_empty());
}

#[test]
fn expired_items_are_hidden_even_before_they_are_deleted() {
    let mut test = Test::new();
    let code = test.added("482913");
    let kept = test.added("483 kept");
    test.advance(BARE_CODE_TTL_MS);
    assert_eq!(test.search("48"), [kept]);
    assert_eq!(test.filtered(Filter::default()), [kept]);
    // Pinned, it stays.
    test.pin(code);
    assert_eq!(test.search("48"), [kept, code]);
}

#[test]
fn search_logs_never_hold_the_query() {
    let logs = logs_of(|| {
        let mut test = Test::new();
        test.added("vbnq-QUERY-SENTINEL");
        test.search("vbnq-QUERY");
        test.search("QUERY-SENTINEL\" OR (");
    });
    assert!(!logs.contains("QUERY-SENTINEL"), "{logs}");
    assert!(!logs.contains("vbnq"), "{logs}");
}

/// `cargo test -p pasteget-core --features history --release -- --ignored --nocapture
/// search_speed`. The design's target is under 30 ms (p95) at 20,000 items on the POCO.
#[test]
#[ignore]
fn search_speed_at_20000_items() {
    let mut test = Test::new();
    let words = [
        "vérification",
        "github",
        "paste",
        "bonjour",
        "facture",
        "rendez-vous",
    ];
    for i in 0..20_000 {
        test.advance(1);
        test.copy(&format!(
            "{} {} item {i} https://example.com/{i}",
            words[i % words.len()],
            words[(i / 7) % words.len()]
        ));
    }
    for text in [
        "verif",
        "hub page",
        "item 1999",
        "é",
        "zz",
        "site:example.com 42",
    ] {
        let mut times: Vec<std::time::Duration> = (0..20)
            .map(|_| {
                let start = std::time::Instant::now();
                test.page(&Query {
                    text: text.into(),
                    ..Query::default()
                });
                start.elapsed()
            })
            .collect();
        times.sort();
        println!("{text:?}: p95 {:?}", times[18]);
    }
}

proptest::proptest! {
    // The `history_query` fuzz target's checks, on queries made of what the parser looks at.
    #[test]
    fn any_query_works_and_highlights_fit(text in "[ a-zé\"*:()^\\-_.%'\\\\]{0,40}|(site|from|type|is|after|before):[ -~]{0,12}") {
        let mut test = Test::new();
        test.added("Code de vérification : 482913 \"quoted\" (x) *");
        test.added("https://github.com/Nexoryx-business/paste-get");
        let page = test.store.page(&Query { text, ..Query::default() });
        proptest::prop_assert!(page.is_ok());
        for item in page.unwrap().items {
            let len = item.preview.encode_utf16().count() as u32;
            let mut end = 0;
            for range in item.highlights {
                proptest::prop_assert!(end <= range.start && range.start < range.end);
                proptest::prop_assert!(range.end <= len);
                end = range.end;
            }
        }
    }
}

fn png(seed: u8, len: usize) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
    data.resize(len, seed);
    data
}

const JPEG_THUMB: &[u8] = b"\xff\xd8\xff\xe0 a small thumbnail";
const WEBP_THUMB: &[u8] = b"RIFF\x10\0\0\0WEBPVP8 thumbnail";

impl Test {
    fn received_image(&mut self, data: Vec<u8>) -> Recorded {
        self.store
            .record(&Clip::png(data, PC), &Context::received_from(PC))
            .unwrap()
    }

    fn screenshot(&mut self, data: Vec<u8>, media_id: i64) -> Recorded {
        let context = Context {
            media_id: Some(media_id),
            ..Context::new(Source::Screenshot)
        };
        self.store.record(&Clip::png(data, HERE), &context).unwrap()
    }

    fn originals(&self) -> Vec<PathBuf> {
        walk(&self.dir.path().join(ORIGINALS_DIR))
    }

    fn reopen(self) -> Self {
        let Test { dir, store, now } = self;
        store.close().unwrap();
        let mut store = HistoryStore::open(dir.path(), HERE, Policy::default()).unwrap();
        let clock = now.clone();
        store.set_clock(move || clock.load(Ordering::Relaxed));
        Test { dir, store, now }
    }
}

#[test]
fn a_received_image_keeps_its_original_as_a_file() {
    let mut test = Test::new();
    let data = png(7, 5000);
    let Recorded::Added(id) = test.received_image(data.clone()) else {
        panic!("not added");
    };

    let item = test.item(id);
    assert_eq!((item.format, item.category), (Format::Png, Category::Image));
    assert_eq!((item.kept, item.thumb), (Kept::Original, Thumb::Pending));
    assert_eq!(item.preview, "");
    let path = test.store.original_path(id).unwrap().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert_eq!(test.originals(), std::slice::from_ref(&path));
    assert_eq!(test.store.text(id).unwrap(), None);
    assert_eq!(
        test.store.pending_thumbnails(10).unwrap(),
        [PendingThumbnail {
            id,
            format: Format::Png,
            path: Some(path),
            media_id: None,
        }]
    );

    // The same image again: one item, one file.
    assert_eq!(test.received_image(data), Recorded::Again(id));
    assert_eq!(test.originals().len(), 1);
    assert_eq!(test.item(id).times, 2);
}

#[test]
fn thumbnails_are_checked_before_they_are_kept() {
    let mut test = Test::new();
    let Recorded::Added(id) = test.received_image(png(1, 100)) else {
        panic!("not added");
    };

    for bad in [
        &b"GIF89a not a thumbnail"[..],
        b"",
        b"RIFF\0\0\0\0WAVE",
        &[0xff; 10],
    ] {
        assert!(matches!(
            test.store.set_thumbnail(id, bad, 10, 10),
            Err(Error::BadThumbnail)
        ));
    }
    let mut too_big = JPEG_THUMB.to_vec();
    too_big.resize(MAX_THUMBNAIL_LEN + 1, 0);
    assert!(matches!(
        test.store.set_thumbnail(id, &too_big, 10, 10),
        Err(Error::BadThumbnail)
    ));
    assert_eq!(test.item(id).thumb, Thumb::Pending);
    assert_eq!(test.store.thumbnail(id).unwrap(), None);

    test.store
        .set_thumbnail(id, WEBP_THUMB, 1920, 1080)
        .unwrap();
    let item = test.item(id);
    assert_eq!(
        (item.thumb, item.width, item.height),
        (Thumb::Done, Some(1920), Some(1080))
    );
    assert_eq!(
        test.store.thumbnail(id).unwrap().as_deref(),
        Some(WEBP_THUMB)
    );
    assert!(test.store.pending_thumbnails(10).unwrap().is_empty());
    // A new one replaces it; PNG and JPEG do too.
    test.store
        .set_thumbnail(id, JPEG_THUMB, 1920, 1080)
        .unwrap();
    test.store
        .set_thumbnail(id, &png(0, 64), 1920, 1080)
        .unwrap();
    assert_eq!(test.store.thumbnail(id).unwrap(), Some(png(0, 64)));

    // A text has no thumbnail, and an unknown item is no error.
    let text = test.added("text");
    test.store.set_thumbnail(text, JPEG_THUMB, 1, 1).unwrap();
    test.store
        .set_thumbnail(ItemId(999), JPEG_THUMB, 1, 1)
        .unwrap();
    assert_eq!(test.store.thumbnail(text).unwrap(), None);
}

#[test]
fn a_thumbnail_the_app_cant_make_is_not_asked_for_again() {
    let mut test = Test::new();
    let Recorded::Added(id) = test.received_image(png(1, 100)) else {
        panic!("not added");
    };
    test.store.thumbnail_failed(id).unwrap();
    assert_eq!(test.item(id).thumb, Thumb::Failed);
    assert!(test.store.pending_thumbnails(10).unwrap().is_empty());
}

#[test]
fn phone_screenshots_are_references_to_the_gallery() {
    let mut test = Test::new();
    let Recorded::Added(id) = test.screenshot(png(2, 3000), 41) else {
        panic!("not added");
    };
    let item = test.item(id);
    assert_eq!(item.category, Category::Screenshot);
    assert_eq!((item.kept, item.thumb), (Kept::Gallery, Thumb::None));
    assert_eq!(item.media_id, Some(41));
    assert!(test.originals().is_empty());
    assert_eq!(test.store.original_path(id).unwrap(), None);
    assert!(test.store.pending_thumbnails(10).unwrap().is_empty());

    // Deleted from the gallery: gone from the history too.
    assert_eq!(test.store.forget_media(&[41, 99]).unwrap(), 1);
    assert!(test.store.item(id).unwrap().is_none());
}

#[test]
fn screenshots_can_outlive_the_gallery_with_their_own_thumbnail() {
    let mut test = Test::with_policy(Policy {
        keep_screenshot_thumbnails: true,
        ..Policy::default()
    });
    let Recorded::Added(kept) = test.screenshot(png(3, 3000), 41) else {
        panic!("not added");
    };
    let Recorded::Added(not_yet) = test.screenshot(png(4, 3000), 42) else {
        panic!("not added");
    };
    assert_eq!(
        test.store
            .pending_thumbnails(10)
            .unwrap()
            .iter()
            .map(|p| (p.id, p.path.clone(), p.media_id))
            .collect::<Vec<_>>(),
        [(kept, None, Some(41)), (not_yet, None, Some(42))]
    );
    test.store
        .set_thumbnail(kept, JPEG_THUMB, 1080, 2400)
        .unwrap();

    assert_eq!(test.store.forget_media(&[41, 42]).unwrap(), 2);

    let item = test.item(kept);
    assert_eq!((item.kept, item.media_id), (Kept::Gone, None));
    assert_eq!(
        test.store.thumbnail(kept).unwrap().as_deref(),
        Some(JPEG_THUMB)
    );
    assert!(test.store.item(not_yet).unwrap().is_none());
}

#[test]
fn images_too_large_keep_no_original() {
    let mut test = Test::new();
    let Recorded::Added(id) = test.received_image(png(5, MAX_ORIGINAL_LEN + 1)) else {
        panic!("not added");
    };
    let item = test.item(id);
    assert_eq!((item.kept, item.thumb), (Kept::TooLarge, Thumb::None));
    assert_eq!(item.size, MAX_ORIGINAL_LEN as u64 + 1);
    assert!(test.originals().is_empty());
}

#[test]
fn the_folder_and_the_rows_agree_after_a_crash() {
    let mut test = Test::new();
    let Recorded::Added(kept) = test.received_image(png(6, 100)) else {
        panic!("not added");
    };
    let Recorded::Added(lost) = test.received_image(png(7, 100)) else {
        panic!("not added");
    };
    let originals = test.dir.path().join(ORIGINALS_DIR);
    // A file written just before a crash, before its row was committed, and a half-written one.
    std::fs::write(
        originals.join(format!("{}.png", "ab".repeat(32))),
        b"orphan",
    )
    .unwrap();
    std::fs::write(originals.join(".tmp-whatever.png"), b"half").unwrap();
    // A file that disappeared.
    let lost_path = test.store.original_path(lost).unwrap().unwrap();
    std::fs::remove_file(&lost_path).unwrap();

    let mut test = test.reopen();

    let kept_path = test.store.original_path(kept).unwrap().unwrap();
    assert_eq!(test.originals(), [kept_path]);
    let item = test.item(lost);
    assert_eq!((item.kept, item.thumb), (Kept::Gone, Thumb::Failed));
    assert_eq!(test.store.original_path(lost).unwrap(), None);

    // Received again, it is whole again.
    assert_eq!(test.received_image(png(7, 100)), Recorded::Again(lost));
    let item = test.item(lost);
    assert_eq!((item.kept, item.thumb), (Kept::Original, Thumb::Pending));
    assert_eq!(std::fs::read(&lost_path).unwrap(), png(7, 100));
}

#[test]
fn an_original_is_removed_if_its_row_is_not_committed() {
    let mut test = Test::new();
    // A trigger makes the insert fail after the file is written.
    test.store
        .conn
        .execute_batch(
            "CREATE TEMP TRIGGER fail BEFORE INSERT ON item
             BEGIN SELECT RAISE(ABORT, 'no'); END;",
        )
        .unwrap();
    assert!(
        test.store
            .record(&Clip::png(png(8, 100), PC), &Context::received_from(PC))
            .is_err()
    );
    assert!(test.originals().is_empty());
}
