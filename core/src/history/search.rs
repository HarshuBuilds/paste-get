// SPDX-License-Identifier: GPL-3.0-or-later

//! Pages of the history, newest first, with search and filters.

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use super::fold::{fold_str, preview};
use super::items::{ITEM_COLUMNS, ITEM_JOINS, item_from_row};
use super::query::{self, Parsed};
use super::types::Code;
use super::{Error, Format, HistoryStore, Item, PREVIEW_CHARS, Source};
use crate::detect::Category;
use crate::identity::DeviceId;

/// Items per page unless asked otherwise, and at most.
pub const PAGE_SIZE: u32 = 50;
pub const MAX_PAGE_SIZE: u32 = 200;

/// A search made only of words under 3 characters looks at this many newest items.
pub const SHORT_SEARCH_ITEMS: u32 = 5000;

/// What the chips of the history screen narrow it to. Every field set must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub category: Option<Category>,
    pub format: Option<Format>,
    pub source: Option<Source>,
    /// First copied on this device (`Item::origin`).
    pub device: Option<DeviceId>,
    /// `Some(true)`: pinned items only; `Some(false)`: the others. The history screen shows
    /// the pinned section first, then the rest.
    pub pinned: Option<bool>,
    /// The site of a link, exactly: the "Links by site" shelf.
    pub host: Option<String>,
    /// Copied at least this many times: the "Frequently copied" shelf.
    pub min_times: Option<u32>,
    /// Last copied in this time span (unix ms, end excluded).
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
}

/// Where the next page starts: just after this item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub last_at: i64,
    pub id: i64,
}

/// A request for one page.
#[derive(Clone, PartialEq, Eq)]
pub struct Query {
    /// What the user typed in the search box (see `query::parse`). Empty: everything.
    pub text: String,
    pub filter: Filter,
    /// `Page::next` of the page before; `None` for the first page.
    pub after: Option<Cursor>,
    /// At most [`MAX_PAGE_SIZE`].
    pub limit: u32,
    /// The user's offset from UTC, for `after:` and `before:` dates.
    pub utc_offset_minutes: i32,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            text: String::new(),
            filter: Filter::default(),
            after: None,
            limit: PAGE_SIZE,
            utc_offset_minutes: 0,
        }
    }
}

/// Never prints what was searched.
impl std::fmt::Debug for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("text_len", &self.text.len())
            .field("after", &self.after)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

/// Items, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<Item>,
    /// Where the next page starts; `None` when this one is the last.
    pub next: Option<Cursor>,
}

impl HistoryStore {
    /// One page of items, newest first, that match `query`.
    ///
    /// Items copied while a page is read come before it: the next pages stay where they were.
    /// Every query works, whatever characters it holds; errors never quote it.
    pub fn page(&self, query: &Query) -> Result<Page, Error> {
        let now = (self.clock)();
        let parsed = query::parse(&query.text);
        let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
        let mut sql = format!(
            "SELECT {ITEM_COLUMNS} FROM item {ITEM_JOINS}
             WHERE (item.expires_at IS NULL OR item.expires_at > ? OR item.pinned_at IS NOT NULL)"
        );
        let mut values = vec![Value::Integer(now)];
        let mut and = |clause: &str, args: Vec<Value>| {
            sql.push_str(" AND ");
            sql.push_str(clause);
            values.extend(args);
        };

        add_search(&mut and, &parsed, query.utc_offset_minutes);
        add_filter(&mut and, &query.filter);
        if let Some(after) = query.after {
            and(
                "(item.last_at, item.id) < (?, ?)",
                vec![after.last_at.into(), after.id.into()],
            );
        }
        sql.push_str(" ORDER BY item.last_at DESC, item.id DESC LIMIT ?");
        values.push(i64::from(limit + 1).into());

        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(values))?;
        let terms: Vec<Vec<char>> = parsed.terms.iter().map(|t| t.chars().collect()).collect();
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let mut item = item_from_row(row)?;
            let (shown, highlights) = preview(&item.preview, &terms, PREVIEW_CHARS);
            item.preview = shown;
            item.highlights = highlights;
            items.push(item);
        }
        let next = if items.len() > limit as usize {
            items.truncate(limit as usize);
            items.last().map(|item| Cursor {
                last_at: item.last_at,
                id: item.id.0,
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }

    /// The sites of links alone, with how many items each has, most first: the "Links by
    /// site" shelf.
    pub fn sites(&self, limit: u32) -> Result<Vec<(String, u64)>, Error> {
        let mut statement = self.conn.prepare(
            "SELECT host, count(*) AS n FROM item WHERE host IS NOT NULL
             GROUP BY host ORDER BY n DESC, max(last_at) DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.min(MAX_PAGE_SIZE)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut sites = Vec::new();
        for row in rows {
            let (host, count) = row?;
            sites.push((host, count.max(0) as u64));
        }
        Ok(sites)
    }
}

/// The conditions of the search box.
fn add_search(and: &mut impl FnMut(&str, Vec<Value>), parsed: &Parsed, utc_offset_minutes: i32) {
    let fts = query::fts_match(&parsed.terms);
    let mut short = query::short_terms(&parsed.terms).peekable();
    if fts.is_none() && short.peek().is_some() {
        and(
            "item.id IN (SELECT id FROM item ORDER BY last_at DESC, id DESC LIMIT ?)",
            vec![i64::from(SHORT_SEARCH_ITEMS).into()],
        );
    }
    for term in short {
        and(
            "instr(fold(item.text_head), ?) > 0",
            vec![term.clone().into()],
        );
    }
    if let Some(fts) = fts {
        and(
            "item.id IN (SELECT rowid FROM item_fts WHERE item_fts MATCH ?)",
            vec![fts.into()],
        );
    }
    if let Some(site) = &parsed.site {
        // The site and its subdomains: github.com, gist.github.com.
        let suffix = format!(".{site}");
        and(
            "(item.host = ? OR substr(item.host, -?) = ?)",
            vec![
                site.clone().into(),
                (suffix.len() as i64).into(),
                suffix.into(),
            ],
        );
    }
    if let Some(from) = &parsed.from {
        and("instr(fold(o.name), ?) > 0", vec![from.clone().into()]);
    }
    if let Some(category) = parsed.category {
        and("item.category = ?", vec![category.code().into()]);
    }
    if parsed.pinned {
        and("item.pinned_at IS NOT NULL", vec![]);
    }
    if let Some(after) = parsed.after {
        let at = query::start_of_day(after, utc_offset_minutes);
        and("item.last_at >= ?", vec![at.into()]);
    }
    if let Some(before) = parsed.before {
        let at = query::start_of_day(before, utc_offset_minutes);
        and("item.last_at < ?", vec![at.into()]);
    }
}

/// The conditions of the chips.
fn add_filter(and: &mut impl FnMut(&str, Vec<Value>), filter: &Filter) {
    if let Some(category) = filter.category {
        and("item.category = ?", vec![category.code().into()]);
    }
    if let Some(format) = filter.format {
        and("item.format = ?", vec![format.code().into()]);
    }
    if let Some(source) = filter.source {
        and("item.source = ?", vec![source.code().into()]);
    }
    if let Some(device) = filter.device {
        and("o.key = ?", vec![device.as_bytes().to_vec().into()]);
    }
    match filter.pinned {
        Some(true) => and("item.pinned_at IS NOT NULL", vec![]),
        Some(false) => and("item.pinned_at IS NULL", vec![]),
        None => {}
    }
    if let Some(host) = &filter.host {
        and("item.host = ?", vec![host.clone().into()]);
    }
    if let Some(times) = filter.min_times {
        and("item.times >= ?", vec![i64::from(times).into()]);
    }
    if let Some(since) = filter.since_ms {
        and("item.last_at >= ?", vec![since.into()]);
    }
    if let Some(until) = filter.until_ms {
        and("item.last_at < ?", vec![until.into()]);
    }
}

/// `fold(text)` for SQL: [`fold_str`], so the database matches what Rust highlights.
pub(super) fn register_fold(conn: &rusqlite::Connection) -> Result<(), Error> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "fold",
        1,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_DETERMINISTIC
            | FunctionFlags::SQLITE_INNOCUOUS,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|text| fold_str(&text))),
    )?;
    Ok(())
}
