// SPDX-License-Identifier: GPL-3.0-or-later

//! Looks for a link in arbitrary text, as the PC does with every text clip it receives, and for
//! a one-time code, as the phone does with every notification. Sorts the text into a history
//! shelf and checks it for secrets, as the history does with every clip it is offered.
//! Run with `cargo +nightly fuzz run detect`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pasteget_core::detect::{self, Category};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Some(link) = detect::link(&text) {
        assert!(link.url.len() <= detect::MAX_LINK_LEN);
        assert!(!link.host.is_empty());
        assert!(link.url.is_ascii() && !link.url.contains(char::is_whitespace));
        // What was found is found again, the same.
        assert_eq!(detect::link(&link.url).as_ref(), Some(&link));
    }
    if let Some(code) = detect::one_time_code(&text) {
        assert!(detect::CODE_DIGITS.contains(&code.len()));
        assert!(code.bytes().all(|b| b.is_ascii_digit()));
    }

    // Plain rules: the same text always gets the same answer.
    let category = detect::category(&text);
    assert_eq!(detect::category(&text), category);
    assert!(!matches!(category, Category::Screenshot | Category::Image));
    assert_eq!(category == Category::Link, detect::link(&text).is_some());
    let secret = detect::looks_secret(&text);
    assert_eq!(detect::looks_secret(&text), secret);
    // Spaces around a clip change nothing.
    let padded = format!(" {text}\n");
    if text.len() < detect::MAX_SECRET_SCAN_LEN - 2 {
        assert_eq!(detect::looks_secret(&padded), secret);
    }
    if detect::bare_code(&text) {
        assert!(text.trim().len() <= 9);
        assert!(!secret);
    }
});
