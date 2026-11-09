// SPDX-License-Identifier: GPL-3.0-or-later

//! The language of what the desktop shows people: menus, notifications, windows. French when the
//! system is in French, English otherwise. Logs and the `pasteget` command stay in English,
//! except the tooltip in `pasteget status --json`, which a status bar shows.
//!
//! `PASTEGET_LANG=fr` or `=en` overrides the system language.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

static LANG: OnceLock<Lang> = OnceLock::new();

/// The language to show, decided once.
pub fn lang() -> Lang {
    *LANG.get_or_init(detect)
}

/// Uses `lang` from now on, if nothing was shown yet. For tests, which compare English texts.
pub fn force(lang: Lang) {
    let _ = LANG.set(lang);
}

/// `en` or `fr`, by [`lang`].
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    match lang() {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// Whether to speak French.
pub fn french() -> bool {
    lang() == Lang::Fr
}

fn detect() -> Lang {
    // This crate's tests compare English texts.
    if cfg!(test) {
        return Lang::En;
    }
    if let Some(lang) = std::env::var("PASTEGET_LANG")
        .ok()
        .as_deref()
        .and_then(parse)
    {
        return lang;
    }
    system().unwrap_or(Lang::En)
}

fn parse(tag: &str) -> Option<Lang> {
    let tag = tag.trim().to_ascii_lowercase();
    if tag.starts_with("fr") {
        Some(Lang::Fr)
    } else if tag.starts_with("en") {
        Some(Lang::En)
    } else {
        None
    }
}

/// The Windows display language.
#[cfg(windows)]
#[allow(unsafe_code)]
fn system() -> Option<Lang> {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    /// `LANG_FRENCH`: the low 10 bits of a language ID are its primary language.
    const FRENCH: u16 = 0x0c;
    // SAFETY: no arguments, no state.
    let id = unsafe { GetUserDefaultUILanguage() };
    Some(if id & 0x3ff == FRENCH {
        Lang::Fr
    } else {
        Lang::En
    })
}

/// `LC_ALL`, then `LC_MESSAGES`, then `LANG`, as gettext reads them.
#[cfg(not(windows))]
fn system() -> Option<Lang> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .and_then(|value| parse(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_tags_are_read_by_their_prefix() {
        assert_eq!(parse("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(parse("FR-be"), Some(Lang::Fr));
        assert_eq!(parse("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(parse("C.UTF-8"), None);
        assert_eq!(parse(""), None);
    }
}
