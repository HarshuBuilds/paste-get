// SPDX-License-Identifier: GPL-3.0-or-later

//! Folding text for search exactly as SQLite's trigram tokenizer does (`remove_diacritics 1`),
//! so words too short for the index, and the highlights, match what the index matches.

use std::ops::Range;

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

/// Previews of search results longer than this are cut around what was found.
pub const SNIPPET_CHARS: usize = 80;

/// A match starting after this many characters gets a snippet around it.
const SNIPPET_AFTER: usize = 60;

/// How much comes before the match in a snippet.
const SNIPPET_LEAD: usize = 20;

/// `c` lowercase and without its accents, as SQLite folds it: 'É' → 'e', 'ấ' → 'a', 'ς' → 'σ'.
/// Accents come off Latin letters only ('ά' stays). `None` for a combining accent SQLite drops,
/// as in a decomposed "é" (e, U+0301); any other character gives one.
pub fn fold(c: char) -> Option<char> {
    if c.is_ascii() {
        return Some(c.to_ascii_lowercase());
    }
    if is_dropped_accent(c) {
        return None;
    }
    let c = case_fold(c);
    // SQLite's own exception.
    if c == 'ǡ' {
        return Some(c);
    }
    let mut parts = Vec::with_capacity(4);
    decompose_canonical(c, |part| parts.push(part));
    Some(match parts[..] {
        [base, ref marks @ ..]
            if !marks.is_empty()
                && base.is_ascii_alphabetic()
                && marks.iter().all(|&mark| is_combining_mark(mark)) =>
        {
            base.to_ascii_lowercase()
        }
        _ => c,
    })
}

/// The combining accents SQLite's tokenizer skips (`sqlite3Fts5UnicodeIsdiacritic`).
fn is_dropped_accent(c: char) -> bool {
    matches!(
        c,
        '\u{300}'..='\u{304}'
            | '\u{306}'..='\u{30c}'
            | '\u{30f}'
            | '\u{311}'
            | '\u{31b}'
            | '\u{323}'..='\u{328}'
            | '\u{32d}'..='\u{32e}'
            | '\u{330}'..='\u{331}'
    )
}

/// Unicode's simple case folding: lowercase, and one form for letters with two lowercase ones.
fn case_fold(c: char) -> char {
    match c {
        'ς' => 'σ',
        'ϐ' => 'β',
        'ϑ' => 'θ',
        'ϕ' => 'φ',
        'ϖ' => 'π',
        'ϰ' => 'κ',
        'ϱ' => 'ρ',
        'ϵ' => 'ε',
        'µ' => 'μ',
        'ſ' => 's',
        'ẛ' => 'ṡ',
        '\u{345}' | '\u{1fbe}' => 'ι',
        // Letters newer than SQLite's tables (Unicode 7.0) keep their case there.
        '\u{37f}' | '\u{528}'..='\u{52f}' => c,
        _ => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(one), None) => one,
                // 'İ' lowercase is "i" and a dot above.
                (Some(first), Some(rest)) if is_combining_mark(rest) => first,
                _ => c,
            }
        }
    }
}

/// `text` folded character by character (see [`fold`]).
pub fn fold_str(text: &str) -> String {
    text.chars().filter_map(fold).collect()
}

/// `chars` folded, and where each folded character is in `chars`.
fn fold_chars(chars: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::with_capacity(chars.len());
    let mut at = Vec::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if let Some(c) = fold(c) {
            folded.push(c);
            at.push(i);
        }
    }
    (folded, at)
}

/// What a list shows of `text`: its start, or with `terms` (folded) the part around the first
/// one found, and where each term is, as UTF-16 ranges.
pub fn preview(text: &str, terms: &[Vec<char>], max_chars: usize) -> (String, Vec<Range<u32>>) {
    let chars: Vec<char> = text.chars().collect();
    let (folded, at) = fold_chars(&chars);
    let first = terms
        .iter()
        .filter_map(|term| find(&folded, term).next())
        .min()
        .map(|start| at[start]);
    let shown: String = match first {
        Some(at) if at > SNIPPET_AFTER && chars.len() > SNIPPET_CHARS => {
            let start = at - SNIPPET_LEAD;
            let end = (start + SNIPPET_CHARS).min(chars.len());
            let mut snippet = String::from("…");
            snippet.extend(&chars[start..end]);
            if end < chars.len() {
                snippet.push('…');
            }
            snippet
        }
        _ => chars.iter().take(max_chars).collect(),
    };
    let highlights = highlights(&shown, terms);
    (shown, highlights)
}

/// Where `terms` (folded) are in `text`, as sorted, merged UTF-16 ranges.
pub fn highlights(text: &str, terms: &[Vec<char>]) -> Vec<Range<u32>> {
    let chars: Vec<char> = text.chars().collect();
    let (folded, at) = fold_chars(&chars);
    // Back to positions in `chars`. Dropped accents after the last character found are its own.
    let mut found: Vec<Range<usize>> = terms
        .iter()
        .flat_map(|term| find(&folded, term).map(move |start| start..start + term.len()))
        .map(|range| at[range.start]..at.get(range.end).copied().unwrap_or(chars.len()))
        .collect();
    found.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in found {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    // UTF-16 offset of each character, and of the end.
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0u32;
    for c in &chars {
        offsets.push(offset);
        offset += c.len_utf16() as u32;
    }
    offsets.push(offset);
    merged
        .into_iter()
        .map(|range| offsets[range.start]..offsets[range.end])
        .collect()
}

/// Where `term` starts in `text`, for every occurrence.
fn find<'a>(text: &'a [char], term: &'a [char]) -> impl Iterator<Item = usize> + 'a {
    let count = if term.is_empty() || term.len() > text.len() {
        0
    } else {
        text.len() - term.len() + 1
    };
    (0..count).filter(move |&at| text[at..at + term.len()] == *term)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::Connection;

    fn terms(words: &[&str]) -> Vec<Vec<char>> {
        words
            .iter()
            .map(|word| fold_str(word).chars().collect())
            .collect()
    }

    #[test]
    fn folding_matches_sqlite_for_latin_greek_and_cyrillic() {
        // The index folds with SQLite's tables; short words and highlights fold here. Ask SQLite
        // how it folds each character: the middle of the trigram "a?a".
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE t USING fts5(x, tokenize='trigram remove_diacritics 1');
             CREATE VIRTUAL TABLE terms USING fts5vocab(t, 'instance');",
        )
        .unwrap();
        // Combining marks included: SQLite drops some of them.
        let ranges = [
            0x20..0x7f,
            0x80..0x530,
            0x1e00..0x2000,
            0x2100..0x2200,
            0xff00..0xff60,
        ];
        let mut ours = std::collections::BTreeMap::new();
        for code in ranges.into_iter().flatten() {
            if let Some(c) = char::from_u32(code) {
                ours.insert(code, fold(c));
                conn.execute(
                    "INSERT INTO t (rowid, x) VALUES (?1, ?2)",
                    rusqlite::params![code, format!("a{c}a")],
                )
                .unwrap();
            }
        }
        // A dropped character leaves "aa", too short for a trigram.
        let mut theirs = std::collections::BTreeMap::new();
        let mut statement = conn.prepare("SELECT doc, term FROM terms").unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap();
        for row in rows {
            let (code, term) = row.unwrap();
            let term: Vec<char> = term.chars().collect();
            assert_eq!(term.len(), 3, "U+{code:04X}");
            theirs.insert(code, term[1]);
        }
        for (code, ours) in ours {
            let c = char::from_u32(code).unwrap();
            assert_eq!(ours, theirs.get(&code).copied(), "U+{code:04X} {c}");
        }
    }

    #[test]
    fn folding_is_stable_and_one_for_one_but_dropped_accents() {
        for (from, to) in [
            ("Vérification", "verification"),
            ("ÉTÉ", "ete"),
            ("Ça", "ca"),
            ("İstanbul", "istanbul"),
            ("Straße", "straße"),
            ("Ὀδυσσεύς", "ὀδυσσεύσ"),
            ("Tiếng Việt", "tieng viet"),
            ("👋 x", "👋 x"),
            // Decomposed: the accents SQLite drops go, the others stay.
            ("Re\u{301}sume\u{301}", "resume"),
            ("Tie\u{302}\u{301}ng", "tieng"),
            ("x\u{305}", "x\u{305}"),
        ] {
            assert_eq!(fold_str(from), to);
            let kept = from.chars().filter(|&c| !is_dropped_accent(c)).count();
            assert_eq!(fold_str(from).chars().count(), kept);
            assert_eq!(fold_str(to), to);
        }
    }

    #[test]
    fn highlights_are_utf16_ranges() {
        let text = "👋 Vérification du code, vérifié";
        let ranges = highlights(text, &terms(&["verif"]));
        // The emoji is two UTF-16 units.
        assert_eq!(ranges, [3..8, 25..30]);
        let utf16: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(String::from_utf16(&utf16[3..8]).unwrap(), "Vérif");

        // Overlapping and touching terms are merged.
        assert_eq!(
            highlights("abcdef", &terms(&["abc", "cde", "f"])),
            vec![Range { start: 0, end: 6 }]
        );
        assert!(highlights("abc", &terms(&["zz", ""])).is_empty());

        // Decomposed accents SQLite drops are found as the index finds them, and go with the
        // letter before them.
        assert_eq!(
            highlights("e\u{301}te\u{301}", &terms(&["te"])),
            vec![Range { start: 2, end: 5 }]
        );
        assert_eq!(
            highlights("Re\u{301}sume\u{301}!", &terms(&["résumé"])),
            vec![Range { start: 0, end: 8 }]
        );
        assert_eq!(
            highlights("Re\u{301}sume\u{301}", &terms(&["e\u{301}s"])),
            vec![Range { start: 1, end: 4 }]
        );
    }

    #[test]
    fn long_texts_show_the_part_that_matched() {
        let text = format!(
            "{}needle in a haystack{}",
            "hay ".repeat(50),
            " hay".repeat(50)
        );
        let (shown, ranges) = preview(&text, &terms(&["NEEDLE"]), 300);
        assert!(shown.starts_with('…') && shown.ends_with('…'), "{shown}");
        assert_eq!(shown.chars().count(), SNIPPET_CHARS + 2);
        let utf16: Vec<u16> = shown.encode_utf16().collect();
        let range = ranges[0].start as usize..ranges[0].end as usize;
        assert_eq!(String::from_utf16(&utf16[range]).unwrap(), "needle");

        // Near the start, or without a search: the start.
        let (shown, ranges) = preview("needle and more", &terms(&["needle"]), 300);
        assert_eq!(shown, "needle and more");
        assert_eq!(ranges, vec![Range { start: 0, end: 6 }]);
        let (shown, ranges) = preview(&text, &[], 10);
        assert_eq!((shown.as_str(), ranges.len()), ("hay hay ha", 0));
    }
}
