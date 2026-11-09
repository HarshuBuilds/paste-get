// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user typed in the search box, read in Rust and never handed to FTS5 as it is.
//!
//! Words and `"quoted phrases"` must all be found. Operators narrow the search: `site:`,
//! `from:`, `type:`, `is:pinned`, `after:` and `before:` (`YYYY-MM-DD`). An unknown operator, or
//! one with a value that doesn't fit, is a plain word.

use std::fmt;

use super::fold::fold_str;
use crate::detect::Category;

/// Only the start of a very long query is read.
pub const MAX_QUERY_CHARS: usize = 500;
/// At most this many words or phrases count.
pub const MAX_TERMS: usize = 16;
/// Longer words are cut to this many characters.
pub const MAX_TERM_CHARS: usize = 100;

/// A day, as typed in `after:` or `before:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// A parsed query.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Words and phrases, folded (see `fold`): each must be found.
    pub terms: Vec<String>,
    /// `site:github.com`, lowercase: that site and its subdomains.
    pub site: Option<String>,
    /// `from:hp`, folded: part of the name of the device it was first copied on.
    pub from: Option<String>,
    /// `type:link`.
    pub category: Option<Category>,
    /// `is:pinned`.
    pub pinned: bool,
    /// `after:2026-11-01`: last copied on that day or later.
    pub after: Option<Date>,
    /// `before:2026-11-09`: last copied before that day.
    pub before: Option<Date>,
}

/// Never prints what was searched.
impl fmt::Debug for Parsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parsed")
            .field("terms", &self.terms.len())
            .field("category", &self.category)
            .field("pinned", &self.pinned)
            .finish_non_exhaustive()
    }
}

/// Reads a query. Never fails: whatever isn't understood is searched for as it is. Control
/// characters separate words like spaces: FTS5 stops reading a phrase at a NUL.
pub fn parse(text: &str) -> Parsed {
    let chars: Vec<char> = text
        .chars()
        .take(MAX_QUERY_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut parsed = Parsed::default();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        if chars[at] == '"' {
            let (phrase, next) = quoted(&chars, at);
            add_term(&mut parsed, &phrase);
            at = next;
            continue;
        }
        let start = at;
        while at < chars.len() && !chars[at].is_whitespace() && chars[at] != '"' {
            at += 1;
        }
        let word: String = chars[start..at].iter().collect();
        // `from:"HP AIO"`: a quoted value.
        let (value, next) = match word.split_once(':') {
            Some((_, "")) if chars.get(at) == Some(&'"') => {
                let (value, next) = quoted(&chars, at);
                (Some(value), next)
            }
            Some((_, value)) => (Some(value.to_owned()), at),
            None => (None, at),
        };
        let name = word.split_once(':').map(|(name, _)| name.to_lowercase());
        let applied = match (name.as_deref(), value) {
            (Some(name), Some(value)) => apply(&mut parsed, name, &value),
            _ => false,
        };
        if applied {
            at = next;
        } else {
            add_term(&mut parsed, &word);
        }
    }
    parsed
}

/// The text between the quote at `at` and the next one (or the end), and where to go on.
fn quoted(chars: &[char], at: usize) -> (String, usize) {
    let start = at + 1;
    let end = chars[start..]
        .iter()
        .position(|&c| c == '"')
        .map_or(chars.len(), |n| start + n);
    (
        chars[start..end].iter().collect(),
        (end + 1).min(chars.len()),
    )
}

fn add_term(parsed: &mut Parsed, term: &str) {
    let term: String = fold_str(term.trim()).chars().take(MAX_TERM_CHARS).collect();
    if !term.is_empty() && parsed.terms.len() < MAX_TERMS && !parsed.terms.contains(&term) {
        parsed.terms.push(term);
    }
}

/// Applies operator `name` with `value`. `false`: not an operator, or a value that doesn't fit.
fn apply(parsed: &mut Parsed, name: &str, value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    match name {
        "site" => {
            let site = value.trim_start_matches("www.").to_ascii_lowercase();
            let valid = site.len() <= 253
                && site
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'));
            if valid {
                parsed.site = Some(site);
            }
            valid
        }
        "from" => {
            parsed.from = Some(fold_str(value).chars().take(MAX_TERM_CHARS).collect());
            true
        }
        "type" => match category_named(&value.to_ascii_lowercase()) {
            Some(category) => {
                parsed.category = Some(category);
                true
            }
            None => false,
        },
        "is" if value.eq_ignore_ascii_case("pinned") => {
            parsed.pinned = true;
            true
        }
        "after" | "before" => match date(value) {
            Some(date) => {
                if name == "after" {
                    parsed.after = Some(date);
                } else {
                    parsed.before = Some(date);
                }
                true
            }
            None => false,
        },
        _ => false,
    }
}

fn category_named(name: &str) -> Option<Category> {
    Some(match name {
        "text" => Category::Text,
        "link" | "links" | "url" => Category::Link,
        "email" | "emails" | "mail" => Category::Email,
        "phone" | "phones" | "tel" => Category::Phone,
        "code" => Category::Code,
        "json" => Category::Json,
        "color" | "colour" | "colors" | "colours" => Category::Color,
        "screenshot" | "screenshots" => Category::Screenshot,
        "image" | "images" | "img" => Category::Image,
        _ => return None,
    })
}

/// `2026-11-09`.
fn date(value: &str) -> Option<Date> {
    let mut parts = value.splitn(3, '-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    let digits = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
        return None;
    }
    let date = Date {
        year: year.parse().ok()?,
        month: month.parse().ok()?,
        day: day.parse().ok()?,
    };
    let leap = date.year % 4 == 0 && (date.year % 100 != 0 || date.year % 400 == 0);
    let days = match date.month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (1..=days).contains(&date.day).then_some(date)
}

/// Unix ms at the start of `date`, for a user `utc_offset_minutes` ahead of UTC.
pub fn start_of_day(date: Date, utc_offset_minutes: i32) -> i64 {
    // Days from 1970-01-01 to the date (Howard Hinnant's days_from_civil).
    let year = i64::from(date.year) - i64::from(date.month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(date.month);
    let day_of_year =
        (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(date.day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * super::DAY_MS - i64::from(utc_offset_minutes) * 60_000
}

/// The FTS5 query for `terms`: each a quoted phrase, all required. Only terms of 3 characters
/// or more: the trigram index finds nothing shorter.
pub fn fts_match(terms: &[String]) -> Option<String> {
    let phrases: Vec<String> = terms
        .iter()
        .filter(|term| term.chars().count() >= 3)
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    (!phrases.is_empty()).then(|| phrases.join(" AND "))
}

/// Terms of 1 or 2 characters, matched in folded text instead.
pub fn short_terms(terms: &[String]) -> impl Iterator<Item = &String> {
    terms.iter().filter(|term| term.chars().count() < 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(text: &str) -> Vec<String> {
        parse(text).terms
    }

    #[test]
    fn words_and_phrases() {
        assert_eq!(terms("  Vérif  CODE "), ["verif", "code"]);
        assert_eq!(terms("\"mot de passe\" wifi"), ["mot de passe", "wifi"]);
        assert_eq!(terms("say\"hi there\""), ["say", "hi there"]);
        assert_eq!(terms("\"unfinished phrase"), ["unfinished phrase"]);
        assert_eq!(terms("a a A"), ["a"]);
        assert!(terms("\"\" \"   \"").is_empty());
        assert!(terms("").is_empty());
    }

    #[test]
    fn operators() {
        let parsed = parse("site:www.GitHub.com from:\"HP AIO\" type:links is:Pinned rust");
        assert_eq!(parsed.site.as_deref(), Some("github.com"));
        assert_eq!(parsed.from.as_deref(), Some("hp aio"));
        assert_eq!(parsed.category, Some(Category::Link));
        assert!(parsed.pinned);
        assert_eq!(parsed.terms, ["rust"]);

        let parsed = parse("after:2026-11-01 before:2024-02-29 FROM:poco");
        assert_eq!(
            parsed.after,
            Some(Date {
                year: 2026,
                month: 11,
                day: 1
            })
        );
        assert_eq!(parsed.before.map(|d| d.day), Some(29));
        assert_eq!(parsed.from.as_deref(), Some("poco"));
        assert!(parsed.terms.is_empty());
    }

    #[test]
    fn what_doesnt_fit_is_a_plain_word() {
        for (query, word) in [
            ("type:spreadsheet", "type:spreadsheet"),
            ("is:secret", "is:secret"),
            ("after:yesterday", "after:yesterday"),
            ("after:2026-02-30", "after:2026-02-30"),
            ("before:2026-13-01", "before:2026-13-01"),
            ("after:26-1-1", "after:26-1-1"),
            ("site:exa%mple", "site:exa%mple"),
            ("lang:fr", "lang:fr"),
            ("12:30", "12:30"),
            ("site:", "site:"),
            ("http://x", "http://x"),
        ] {
            assert_eq!(terms(query), [word], "{query}");
        }
    }

    #[test]
    fn queries_are_bounded() {
        let many = (0..100)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(terms(&many).len(), MAX_TERMS);
        let long = "x".repeat(1000);
        assert_eq!(terms(&long)[0].chars().count(), MAX_TERM_CHARS);
    }

    #[test]
    fn fts_queries_are_quoted_phrases() {
        let parsed = parse(r#"abc "x" y NEAR(a b) -no col:x * ^start ab"#);
        assert_eq!(
            fts_match(&parsed.terms).unwrap(),
            r#""abc" AND "near(a" AND "-no" AND "col:x" AND "^start""#
        );
        assert_eq!(
            short_terms(&parsed.terms).collect::<Vec<_>>(),
            ["x", "y", "b)", "*", "ab"]
        );
        assert_eq!(fts_match(&["a\"b\"c".into()]).unwrap(), r#""a""b""c""#);
        assert_eq!(fts_match(&["ab".into()]), None);
    }

    #[test]
    fn control_characters_separate_words() {
        assert_eq!(terms("abc\0def \u{1}x\u{7f}"), ["abc", "def", "x"]);
        assert_eq!(terms("\"ab\0cd\""), ["ab cd"]);
        assert_eq!(parse("from:a\0b").from.as_deref(), Some("a"));
        let parsed = parse("\0\0\0 xyz\0");
        assert_eq!(fts_match(&parsed.terms).unwrap(), r#""xyz""#);
    }

    #[test]
    fn days_start_at_local_midnight() {
        let day = |year, month, day| Date { year, month, day };
        assert_eq!(start_of_day(day(1970, 1, 1), 0), 0);
        assert_eq!(start_of_day(day(2026, 11, 9), 0), 1_794_182_400_000);
        assert_eq!(start_of_day(day(2000, 3, 1), 0), 951_868_800_000);
        // Paris in winter is one hour ahead: its midnight is 23:00 UTC the day before.
        assert_eq!(
            start_of_day(day(2026, 11, 9), 60),
            1_794_182_400_000 - 3_600_000
        );
    }
}
