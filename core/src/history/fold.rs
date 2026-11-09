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
/// Accents come off Latin letters only ('ά' stays). Always one character for one, so positions
/// in folded text are positions in the original.
pub fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let c = case_fold(c);
    // SQLite's own exception.
    if c == 'ǡ' {
        return c;
    }
    let mut parts = Vec::with_capacity(4);
    decompose_canonical(c, |part| parts.push(part));
    match parts[..] {
        [base, ref marks @ ..]
            if !marks.is_empty()
                && base.is_ascii_alphabetic()
                && marks.iter().all(|&mark| is_combining_mark(mark)) =>
        {
            base.to_ascii_lowercase()
        }
        _ => c,
    }
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
    text.chars().map(fold).collect()
}

/// What a list shows of `text`: its start, or with `terms` (folded) the part around the first
/// one found, and where each term is, as UTF-16 ranges.
pub fn preview(text: &str, terms: &[Vec<char>], max_chars: usize) -> (String, Vec<Range<u32>>) {
    let chars: Vec<char> = text.chars().collect();
    let folded: Vec<char> = chars.iter().map(|&c| fold(c)).collect();
    let first = terms
        .iter()
        .filter_map(|term| find(&folded, term).next())
        .min();
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
    let folded: Vec<char> = chars.iter().map(|&c| fold(c)).collect();
    let mut found: Vec<Range<usize>> = terms
        .iter()
        .flat_map(|term| find(&folded, term).map(move |at| at..at + term.len()))
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
        let ranges = [
            0x20..0x7f,
            0x80..0x250,
            0x370..0x530,
            0x1e00..0x2000,
            0x2100..0x2200,
            0xff00..0xff60,
        ];
        let mut inserted = 0;
        for code in ranges.into_iter().flatten() {
            if let Some(c) = char::from_u32(code) {
                inserted += 1;
                conn.execute(
                    "INSERT INTO t (rowid, x) VALUES (?1, ?2)",
                    rusqlite::params![code, format!("a{c}a")],
                )
                .unwrap();
            }
        }
        let mut statement = conn.prepare("SELECT doc, term FROM terms").unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap();
        let mut checked = 0;
        for row in rows {
            let (code, term) = row.unwrap();
            let c = char::from_u32(code).unwrap();
            let theirs: Vec<char> = term.chars().collect();
            assert_eq!(theirs.len(), 3, "U+{code:04X}");
            assert_eq!(fold(c), theirs[1], "U+{code:04X} {c}");
            checked += 1;
        }
        assert_eq!(checked, inserted);
    }

    #[test]
    fn folding_is_one_character_for_one_and_stable() {
        for (from, to) in [
            ("Vérification", "verification"),
            ("ÉTÉ", "ete"),
            ("Ça", "ca"),
            ("İstanbul", "istanbul"),
            ("Straße", "straße"),
            ("Ὀδυσσεύς", "ὀδυσσεύσ"),
            ("Tiếng Việt", "tieng viet"),
            ("👋 x", "👋 x"),
        ] {
            assert_eq!(fold_str(from), to);
            assert_eq!(fold_str(from).chars().count(), from.chars().count());
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

        // Overlapping and touching terms are merged; combining accents count as characters.
        assert_eq!(
            highlights("abcdef", &terms(&["abc", "cde", "f"])),
            vec![Range { start: 0, end: 6 }]
        );
        assert_eq!(
            highlights("e\u{301}te\u{301}", &terms(&["te"])),
            vec![Range { start: 2, end: 4 }]
        );
        assert!(highlights("abc", &terms(&["zz", ""])).is_empty());
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
