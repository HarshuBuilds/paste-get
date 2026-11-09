// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks that the windows (`dist/`) are translated: every key a page or script uses has its
//! French in `dist/i18n.js`, every French text there is used, both languages have the same
//! `{placeholders}`, and no text in a page is left without a key. The Rust side needs no such
//! check: `i18n::tr` takes both languages at once.

use std::collections::{BTreeMap, BTreeSet};

const I18N: &str = include_str!("../dist/i18n.js");
const PAGES: [(&str, &str); 3] = [
    ("about.html", include_str!("../dist/about.html")),
    ("pair.html", include_str!("../dist/pair.html")),
    ("settings.html", include_str!("../dist/settings.html")),
];
const SCRIPTS: [(&str, &str); 4] = [
    ("about.js", include_str!("../dist/about.js")),
    ("copy.js", include_str!("../dist/copy.js")),
    ("pair.js", include_str!("../dist/pair.js")),
    ("settings.js", include_str!("../dist/settings.js")),
];

/// The French texts of `window.pastegetFr`, by key.
fn french() -> BTreeMap<String, String> {
    let start = I18N
        .find("window.pastegetFr = {")
        .expect("no pastegetFr in i18n.js");
    let body = &I18N[start..];
    let body = &body[..body.find("\n};").expect("pastegetFr isn't closed")];
    body.lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| {
            let (key, text) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("not `key: \"text\",`: {line}"));
            let text = text
                .strip_suffix(',')
                .and_then(|t| t.strip_prefix('"'))
                .and_then(|t| t.strip_suffix('"'))
                .unwrap_or_else(|| panic!("not a plain string: {line}"));
            assert!(
                !text.contains(['"', '\\']),
                "use « » rather than quotes: {line}"
            );
            (key.to_owned(), text.to_owned())
        })
        .collect()
}

/// The value of each `attribute="…"` in `html`.
fn attributes<'a>(html: &'a str, attribute: &str) -> Vec<&'a str> {
    let pattern = format!(" {attribute}=\"");
    html.match_indices(&pattern)
        .filter_map(|(at, _)| {
            let value = &html[at + pattern.len()..];
            value.find('"').map(|end| &value[..end])
        })
        .collect()
}

/// The next JavaScript string literal at the start of `code` (after spaces and commas), and
/// what follows it.
fn string_literal(code: &str) -> Option<(&str, &str)> {
    let code = code.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    let quote = code
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let rest = &code[1..];
    let end = rest.find(quote)?;
    Some((&rest[..end], &rest[end + 1..]))
}

/// Each `pastegetText("key", "English")` and `pastegetFormat(…)` call in `script`: its key and
/// English text.
fn calls(script: &str) -> Vec<(String, String)> {
    ["pastegetText(", "pastegetFormat("]
        .iter()
        .flat_map(|function| script.match_indices(function))
        .filter_map(|(at, function)| {
            let after = &script[at + function.len()..];
            // The definition in i18n.js takes names, not strings.
            let (key, after) = string_literal(after)?;
            let (english, _) = string_literal(after)?;
            Some((key.to_owned(), english.to_owned()))
        })
        .collect()
}

/// `{time}`, `{name}`… in `text`.
fn placeholders(text: &str) -> BTreeSet<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name))
        .collect()
}

#[test]
fn every_key_has_its_french_and_every_french_text_is_used() {
    let french = french();
    let mut used = BTreeMap::new();
    for (page, html) in PAGES {
        for attribute in ["data-i18n", "data-i18n-placeholder", "data-i18n-label"] {
            for key in attributes(html, attribute) {
                used.insert(key.to_owned(), format!("{page}, {attribute}"));
            }
        }
    }
    for (script, code) in SCRIPTS {
        for (key, english) in calls(code) {
            assert!(!english.is_empty(), "{script}: {key} has no English text");
            if let Some(text) = french.get(&key) {
                assert_eq!(
                    placeholders(&english),
                    placeholders(text),
                    "{script}: {key} has other placeholders in French"
                );
            }
            used.insert(key, script.to_owned());
        }
    }
    assert!(used.len() > 20, "the pages weren't read: {used:?}");

    let missing: Vec<_> = used
        .iter()
        .filter(|(key, _)| !french.contains_key(*key))
        .collect();
    assert!(missing.is_empty(), "no French in i18n.js for {missing:?}");
    let unused: Vec<_> = french
        .keys()
        .filter(|key| !used.contains_key(*key))
        .collect();
    assert!(unused.is_empty(), "French texts nothing uses: {unused:?}");
}

#[test]
fn no_text_in_a_page_is_left_without_a_key() {
    for (page, html) in PAGES {
        let body = &html[html.find("<body>").expect("no <body>")..];
        for (at, _) in body.match_indices('>') {
            let text = body[at + 1..].split('<').next().unwrap_or_default().trim();
            // The name of the app is the same in every language.
            if !text.chars().any(char::is_alphabetic) || text == "paste-get" {
                continue;
            }
            let tag = &body[body[..at].rfind('<').unwrap_or(0)..at];
            assert!(
                tag.contains(" data-i18n=\""),
                "{page}: {text:?} has no data-i18n key"
            );
        }
    }
}

#[test]
fn calls_and_placeholders_are_read_as_written() {
    let script = r#"
        window.pastegetText("badKey",
          "Hotkeys use a letter, a digit or F1 to F24.");
        window.pastegetFormat("pairRenews", "A new code comes in {time}.", { time });
    "#;
    assert_eq!(
        calls(script),
        [
            (
                "badKey".to_owned(),
                "Hotkeys use a letter, a digit or F1 to F24.".to_owned()
            ),
            (
                "pairRenews".to_owned(),
                "A new code comes in {time}.".to_owned()
            ),
        ]
    );
    assert_eq!(
        placeholders("{name} and {time}."),
        BTreeSet::from(["name", "time"])
    );
    assert!(french().contains_key("pairRenews"));
}
