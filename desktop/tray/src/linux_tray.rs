// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether the tray icon can be seen at all, on Linux.
//!
//! The icon is a StatusNotifierItem: it shows only where a "host" draws such items (KDE Plasma's
//! system tray, Waybar's `tray` module, GNOME with the AppIndicator extension), registered with
//! the `org.kde.StatusNotifierWatcher` service on the session bus. GNOME has none unless the
//! "AppIndicator and KStatusNotifierItem Support" extension is on. Without a host, paste-get
//! runs with nothing to click, so this says so once per start: in the log, and in a notification.

/// Starts the check in the background. Does nothing outside Linux.
pub fn warn_if_no_tray() {
    #[cfg(all(unix, not(target_os = "macos")))]
    tauri::async_runtime::spawn(linux::warn_if_no_tray());
}

/// Shows an error that stops paste-get in a notification that stays until dismissed, since a
/// program started from the menu or at login has no terminal to print to. Returns once the
/// notification is handed over; `false` without a notification service.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn show_error(summary: &str, body: &str) -> bool {
    tauri::async_runtime::block_on(async {
        let result = match zbus::Connection::session().await {
            Ok(connection) => linux::notify(&connection, summary, body, true).await,
            Err(e) => Err(e),
        };
        result
            .inspect_err(|e| tracing::warn!(error = %e, "could not show the error"))
            .is_ok()
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use pasteget_desktop::i18n::tr;
    use zbus::{Connection, OwnedVariant, Variant};

    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    const WATCHER_PATH: &str = "/StatusNotifierWatcher";
    /// At login, the bar or the shell may register its host a little after paste-get starts.
    const WAIT: Duration = Duration::from_secs(15);
    const CHECK_EVERY: Duration = Duration::from_millis(500);

    pub async fn warn_if_no_tray() {
        let connection = match Connection::session().await {
            Ok(connection) => connection,
            Err(e) => {
                tracing::warn!(error = %e, "no session bus: can't tell whether the tray icon shows");
                return;
            }
        };
        if wait_for_host(&connection, WAIT).await {
            return;
        }
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        tracing::warn!(
            desktop,
            wayland,
            "no system tray (StatusNotifierItem host) on the session bus: the tray icon isn't shown"
        );
        if !should_notify(wayland, &desktop) {
            return;
        }
        if let Err(e) = notify(&connection, summary(), body(is_gnome(&desktop)), false).await {
            tracing::warn!(error = %e, "could not show the notification about the tray");
        }
    }

    /// Whether a host is registered now, or within `wait`.
    pub(super) async fn wait_for_host(connection: &Connection, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        loop {
            match host_registered(connection).await {
                Ok(true) => return true,
                Ok(false) => {}
                Err(e) => tracing::debug!(error = %e, "could not ask for a tray host"),
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    }

    /// Whether a StatusNotifierItem host is registered with the watcher.
    pub(super) async fn host_registered(connection: &Connection) -> zbus::Result<bool> {
        let watcher_runs: bool = connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &(WATCHER,),
            )
            .await?
            .body()
            .deserialize()?;
        if !watcher_runs {
            return Ok(false);
        }
        let reply = connection
            .call_method(
                Some(WATCHER),
                WATCHER_PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(WATCHER, "IsStatusNotifierHostRegistered"),
            )
            .await;
        // A watcher that can't say: trust it rather than warn for nothing.
        let Ok(reply) = reply else {
            return Ok(true);
        };
        let registered: OwnedVariant = reply.body().deserialize()?;
        Ok(bool::try_from(registered).unwrap_or(true))
    }

    /// On Wayland nothing else can show the icon. On X11, the app indicator library falls back
    /// to an old-style (XEmbed) tray icon, which some desktops still show; GNOME doesn't.
    pub(super) fn should_notify(wayland: bool, desktop: &str) -> bool {
        wayland || is_gnome(desktop)
    }

    /// `XDG_CURRENT_DESKTOP` is a list, such as `ubuntu:GNOME`.
    pub(super) fn is_gnome(desktop: &str) -> bool {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("gnome"))
    }

    fn summary() -> &'static str {
        tr(
            "paste-get is running, but its icon can't be shown",
            "paste-get tourne, mais son icône ne peut pas s'afficher",
        )
    }

    pub(super) fn body(gnome: bool) -> &'static str {
        if gnome {
            tr(
                "GNOME needs the “AppIndicator and KStatusNotifierItem Support” extension for it: \
                 install it and turn it on, then start paste-get again.",
                "GNOME a besoin pour cela de l'extension « AppIndicator and KStatusNotifierItem \
                 Support » : installe-la et active-la, puis relance paste-get.",
            )
        } else {
            tr(
                "No system tray was found. Add one (on Hyprland, Waybar's tray module), then \
                 start paste-get again. Meanwhile, the pasteget command works.",
                "Aucune zone de notification trouvée. Ajoutes-en une (sur Hyprland, le module \
                 tray de Waybar), puis relance paste-get. En attendant, la commande pasteget \
                 fonctionne.",
            )
        }
    }

    /// Not transient, unlike the other notices: it stays in the notification list to be read.
    /// An `urgent` one (an error) also stays on screen until dismissed. The body is escaped:
    /// servers may read it as markup.
    pub(super) async fn notify(
        connection: &Connection,
        summary: &str,
        body: &str,
        urgent: bool,
    ) -> zbus::Result<()> {
        let mut hints: HashMap<&str, Variant<'_>> = HashMap::new();
        // Never expires: 0. The server's default: -1.
        let mut timeout = -1i32;
        if urgent {
            hints.insert("urgency", Variant::from(2u8));
            timeout = 0;
        }
        connection
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &(
                    "paste-get",
                    0u32,
                    "edit-paste",
                    summary,
                    escape_markup(body),
                    Vec::<&str>::new(),
                    hints,
                    timeout,
                ),
            )
            .await?;
        Ok(())
    }

    /// `text` as plain text in a body that may be read as markup.
    pub(super) fn escape_markup(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

#[cfg(test)]
#[cfg(all(unix, not(target_os = "macos")))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    use futures_util::StreamExt;
    use zbus::connection::Builder;
    use zbus::message::Type;
    use zbus::{Connection, MessageStream, Variant};

    use super::linux::{
        body, escape_markup, host_registered, is_gnome, notify, should_notify, wait_for_host,
    };

    /// A session bus of its own, so the tests never see the real tray.
    struct Bus {
        daemon: Child,
        address: String,
    }

    impl Bus {
        /// `None` without `dbus-daemon`.
        fn start() -> Option<Self> {
            let mut daemon = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let mut address = String::new();
            BufReader::new(daemon.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            Some(Self {
                daemon,
                address: address.trim().to_owned(),
            })
        }
    }

    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    async fn connect(address: &str) -> Connection {
        Builder::address(address).build().await.unwrap()
    }

    /// A watcher that answers `IsStatusNotifierHostRegistered` with `registered`, or with an
    /// error if `None`. It answers until the bus stops.
    async fn fake_watcher(address: &str, registered: Option<bool>) {
        let connection = connect(address).await;
        connection
            .request_name("org.kde.StatusNotifierWatcher")
            .await
            .unwrap();
        let mut calls = MessageStream::from(&connection);
        tauri::async_runtime::spawn(async move {
            while let Some(Ok(call)) = calls.next().await {
                let header = call.header();
                if call.message_type() != Type::MethodCall
                    || header.member().map(|m| m.as_str()) != Some("Get")
                {
                    continue;
                }
                let _ = match registered {
                    Some(registered) => connection.reply(&header, &Variant::from(registered)).await,
                    None => {
                        let error = zbus::fdo::Error::UnknownProperty("no such property".into());
                        connection.reply_dbus_error(&header, error).await
                    }
                };
            }
        });
    }

    /// A notification server that sends what each `Notify` call shows (summary, body, urgency,
    /// timeout) on the returned channel.
    async fn fake_notifications(
        address: &str,
    ) -> tokio::sync::mpsc::UnboundedReceiver<(String, String, Option<u8>, i32)> {
        use std::collections::HashMap;
        use zbus::OwnedVariant;

        let connection = connect(address).await;
        connection
            .request_name("org.freedesktop.Notifications")
            .await
            .unwrap();
        let (shown_tx, shown) = tokio::sync::mpsc::unbounded_channel();
        let mut calls = MessageStream::from(&connection);
        tauri::async_runtime::spawn(async move {
            while let Some(Ok(call)) = calls.next().await {
                let header = call.header();
                if call.message_type() != Type::MethodCall
                    || header.member().map(|m| m.as_str()) != Some("Notify")
                {
                    continue;
                }
                type Notify = (
                    String,
                    u32,
                    String,
                    String,
                    String,
                    Vec<String>,
                    HashMap<String, OwnedVariant>,
                    i32,
                );
                let (_, _, _, summary, body, _, hints, timeout): Notify =
                    call.body().deserialize().unwrap();
                let urgency = hints
                    .get("urgency")
                    .and_then(|v| u8::try_from(v.clone()).ok());
                let _ = shown_tx.send((summary, body, urgency, timeout));
                let _ = connection.reply(&header, &1u32).await;
            }
        });
        shown
    }

    #[test]
    fn errors_stay_on_screen_as_plain_text() {
        with_bus(async |address| {
            let mut shown = fake_notifications(address).await;
            let app = connect(address).await;

            notify(
                &app,
                "paste-get could not start",
                "no <b>bus</b> & co",
                true,
            )
            .await
            .unwrap();
            assert_eq!(
                shown.recv().await.unwrap(),
                (
                    "paste-get could not start".to_owned(),
                    "no &lt;b&gt;bus&lt;/b&gt; &amp; co".to_owned(),
                    Some(2),
                    0
                )
            );

            // The tray notice isn't urgent, and goes away as the server decides.
            notify(&app, "summary", "body", false).await.unwrap();
            assert_eq!(
                shown.recv().await.unwrap(),
                ("summary".to_owned(), "body".to_owned(), None, -1)
            );
        });
        assert_eq!(escape_markup("a<b>&c"), "a&lt;b&gt;&amp;c");
    }

    fn with_bus(test: impl AsyncFnOnce(&str)) {
        let Some(bus) = Bus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        tauri::async_runtime::block_on(test(&bus.address));
    }

    #[test]
    fn no_watcher_means_no_tray() {
        with_bus(async |address| {
            let app = connect(address).await;
            assert!(!host_registered(&app).await.unwrap());
            assert!(!wait_for_host(&app, Duration::from_millis(600)).await);
        });
    }

    #[test]
    fn a_watcher_without_a_host_means_no_tray() {
        with_bus(async |address| {
            let app = connect(address).await;
            fake_watcher(address, Some(false)).await;
            assert!(!host_registered(&app).await.unwrap());
        });
    }

    #[test]
    fn a_watcher_with_a_host_means_a_tray() {
        with_bus(async |address| {
            let app = connect(address).await;
            fake_watcher(address, Some(true)).await;
            assert!(host_registered(&app).await.unwrap());
        });
    }

    #[test]
    fn a_watcher_that_cant_say_is_trusted() {
        with_bus(async |address| {
            let app = connect(address).await;
            fake_watcher(address, None).await;
            assert!(host_registered(&app).await.unwrap());
        });
    }

    #[test]
    fn a_host_that_comes_late_is_waited_for() {
        with_bus(async |address| {
            let app = connect(address).await;
            let late = address.to_owned();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(700)).await;
                fake_watcher(&late, Some(true)).await;
            });
            assert!(wait_for_host(&app, Duration::from_secs(5)).await);
        });
    }

    #[test]
    fn warns_where_nothing_else_can_show_the_icon() {
        // Wayland: no fallback at all.
        assert!(should_notify(true, "KDE"));
        assert!(should_notify(true, "Hyprland"));
        // GNOME on X11 has no old-style tray either.
        assert!(should_notify(false, "ubuntu:GNOME"));
        // Other X11 desktops may show the old-style icon.
        assert!(!should_notify(false, "XFCE"));
        assert!(!should_notify(false, ""));
    }

    #[test]
    fn gnome_is_named_in_the_desktop_list() {
        assert!(is_gnome("GNOME"));
        assert!(is_gnome("ubuntu:GNOME"));
        assert!(!is_gnome("KDE"));
        assert!(!is_gnome("Hyprland"));
        // Both languages name the extension and Waybar as they are.
        assert!(body(true).contains("AppIndicator and KStatusNotifierItem"));
        assert!(body(false).contains("Waybar"));
    }
}
