// SPDX-License-Identifier: GPL-3.0-or-later

//! The "About paste-get" window: the version, the licence and where the source code is. It
//! opens nothing on the internet: the addresses are text to copy (`dist/about.html`).

use anyhow::{Context, Result};
use pasteget_desktop::i18n::tr;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const WINDOW: &str = "about";

/// This build's version, as the menu and the windows show it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The repository, from the workspace's `Cargo.toml`.
pub const SOURCE: &str = env!("CARGO_PKG_REPOSITORY");

/// What the windows show about paste-get itself (the `app_info` command).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Info {
    pub version: &'static str,
    pub licence: &'static str,
    /// The source code.
    pub source: &'static str,
    /// Where the packages are, the Android app's among them.
    pub releases: String,
}

impl Info {
    pub fn of_this_build() -> Self {
        Self {
            version: VERSION,
            licence: env!("CARGO_PKG_LICENSE"),
            source: SOURCE,
            releases: format!("{SOURCE}/releases"),
        }
    }
}

/// "About paste-get 0.1.0", for the menu.
pub fn menu_label() -> String {
    format!(
        "{} {VERSION}",
        tr("About paste-get", "À propos de paste-get")
    )
}

/// Opens the window, or brings it to the front.
pub fn open<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        window.set_focus()?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("about.html".into()))
        .title(tr("About paste-get", "À propos de paste-get"))
        .initialization_script(crate::language_script())
        .inner_size(440.0, 400.0)
        .resizable(false)
        .center()
        .focused(true)
        .build()
        .context("could not open the About window")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_about_window_shows_this_build() {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        let info = Info::of_this_build();

        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.licence, "GPL-3.0-or-later");
        assert_eq!(info.source, "https://github.com/Nexoryx-business/paste-get");
        assert_eq!(
            info.releases,
            "https://github.com/Nexoryx-business/paste-get/releases"
        );
        assert_eq!(menu_label(), format!("About paste-get {VERSION}"));
    }
}
