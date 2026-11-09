// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs arbitrary text as a history search, as the search box does with whatever is typed:
//! every query must work (it is parsed in Rust, never handed to FTS5 as it is), and highlights
//! must fall inside the preview. Run with `cargo +nightly fuzz run history_query`.

#![no_main]

use std::sync::{Mutex, OnceLock};

use libfuzzer_sys::fuzz_target;
use pasteget_core::clip::Clip;
use pasteget_core::history::{Context, HistoryStore, Policy, Query, Source};
use pasteget_core::identity::DeviceId;

const HERE: DeviceId = DeviceId::from_bytes([1; DeviceId::LEN]);

/// One store for the whole run, with a few items to find.
fn store() -> &'static Mutex<HistoryStore> {
    static STORE: OnceLock<Mutex<HistoryStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("pasteget-fuzz-history-{}", std::process::id()));
        let mut store = HistoryStore::open(&dir, HERE, Policy::default()).unwrap();
        for text in [
            "Code de vérification : 482913",
            "https://github.com/Nexoryx-business/paste-get",
            "Un café 👋 e\u{301}té",
            "{\"key\": \"value\", \"n\": [1, 2]}",
        ] {
            let context = Context::new(Source::CopiedHere);
            store.record(&Clip::text(text, HERE), &context).unwrap();
        }
        Mutex::new(store)
    })
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data).into_owned();
    let store = store().lock().unwrap();
    let query = Query {
        text,
        ..Query::default()
    };
    let page = store.page(&query).expect("every query works");
    for item in &page.items {
        let len = item.preview.encode_utf16().count() as u32;
        let mut end = 0;
        for range in &item.highlights {
            assert!(end <= range.start && range.start < range.end && range.end <= len);
            end = range.end;
        }
    }
});
