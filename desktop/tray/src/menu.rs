// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tray menu shows, as plain data so it can be tested without a desktop. `main.rs`
//! turns [`Entry`]s into Tauri menu items and menu clicks back into [`Command`]s.

use pasteget_desktop::control::{DeviceStatusReply, RecentReply, StatusReply};
use pasteget_desktop::i18n::{french, tr};
use pasteget_desktop::recent;

pub const PAIR: &str = "pair";
pub const SETTINGS: &str = "settings";
pub const PAUSE: &str = "pause";
pub const START_AT_LOGIN: &str = "start-at-login";
pub const ABOUT: &str = "about";
pub const QUIT: &str = "quit";
const RECENT_PREFIX: &str = "recent:";
const FORGET_PREFIX: &str = "forget:";

/// What the tray knows about the daemon.
#[derive(Debug)]
pub enum State {
    Starting,
    Running {
        status: StatusReply,
        recent: Vec<RecentReply>,
    },
    /// The daemon isn't running and couldn't be started, and why, in the user's language
    /// ([`start_error`], [`down_reason`]).
    Down(String),
}

impl State {
    pub fn paused(&self) -> bool {
        matches!(self, Self::Running { status, .. } if status.paused)
    }

    /// One line, for the tray icon's tooltip.
    pub fn summary(&self) -> String {
        match self {
            Self::Starting => tr("Starting…", "Démarrage…").into(),
            Self::Down(_) => tr("Not running", "Arrêté").into(),
            Self::Running { status, .. } => {
                let connected = status.devices.iter().filter(|d| d.connected).count();
                if status.paused {
                    tr("Paused", "En pause").into()
                } else if connected == 1 {
                    tr("Syncing with 1 device", "Synchro avec 1 appareil").into()
                } else if connected > 1 {
                    if french() {
                        format!("Synchro avec {connected} appareils")
                    } else {
                        format!("Syncing with {connected} devices")
                    }
                } else {
                    tr("No device connected", "Aucun appareil connecté").into()
                }
            }
        }
    }
}

/// One menu line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Greyed-out text.
    Info(String),
    Separator,
    Action {
        id: String,
        label: String,
    },
    Check {
        id: String,
        label: String,
        checked: bool,
    },
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
}

/// What a click asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Pair,
    Settings,
    TogglePause,
    ToggleStartAtLogin,
    About,
    CopyRecent(u64),
    /// Unpair the device with this ID.
    Forget(String),
    Quit,
}

impl Command {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            PAIR => Some(Self::Pair),
            SETTINGS => Some(Self::Settings),
            PAUSE => Some(Self::TogglePause),
            START_AT_LOGIN => Some(Self::ToggleStartAtLogin),
            ABOUT => Some(Self::About),
            QUIT => Some(Self::Quit),
            _ => {
                if let Some(device) = id.strip_prefix(FORGET_PREFIX) {
                    return Some(Self::Forget(device.into()));
                }
                id.strip_prefix(RECENT_PREFIX)
                    .and_then(|n| n.parse().ok())
                    .map(Self::CopyRecent)
            }
        }
    }
}

/// The menu. `start_at_login` is `None` where that isn't supported.
pub fn entries(state: &State, start_at_login: Option<bool>) -> Vec<Entry> {
    let mut entries = daemon_entries(state);
    entries.push(Entry::Separator);
    entries.push(Entry::Action {
        id: SETTINGS.into(),
        label: tr("Settings…", "Réglages…").into(),
    });
    if let Some(checked) = start_at_login {
        entries.push(Entry::Check {
            id: START_AT_LOGIN.into(),
            label: tr("Start at login", "Lancer à l'ouverture de session").into(),
            checked,
        });
    }
    entries.push(Entry::Action {
        id: ABOUT.into(),
        label: crate::about::menu_label(),
    });
    entries.push(Entry::Action {
        id: QUIT.into(),
        label: tr("Quit", "Quitter").into(),
    });
    entries
}

fn daemon_entries(state: &State) -> Vec<Entry> {
    match state {
        State::Starting => vec![Entry::Info(
            tr("paste-get is starting…", "paste-get démarre…").into(),
        )],
        State::Down(reason) => vec![
            Entry::Info(tr("paste-get is not running", "paste-get ne tourne pas").into()),
            Entry::Info(text(reason)),
        ],
        State::Running { status, recent } => {
            let mut entries = vec![
                Entry::Info(format!("paste-get · {}", text(&status.device_name))),
                Entry::Info(state.summary()),
                Entry::Separator,
            ];
            if status.devices.is_empty() {
                entries.push(Entry::Info(
                    tr("No paired device yet", "Aucun appareil appairé").into(),
                ));
            } else {
                entries.push(Entry::Info(tr("Devices", "Appareils").into()));
                entries.extend(status.devices.iter().map(device_entry));
            }
            entries.push(Entry::Action {
                id: PAIR.into(),
                label: tr("Pair a phone…", "Appairer un téléphone…").into(),
            });
            entries.push(Entry::Separator);
            entries.push(Entry::Submenu {
                label: tr("Recent", "Récents").into(),
                entries: if recent.is_empty() {
                    vec![Entry::Info(tr("Nothing yet", "Rien pour l'instant").into())]
                } else {
                    recent.iter().map(recent_entry).collect()
                },
            });
            entries.push(Entry::Check {
                id: PAUSE.into(),
                label: tr("Pause syncing", "Mettre en pause").into(),
                checked: status.paused,
            });
            entries
        }
    }
}

/// The notice when the daemon can't start: "paste-get could not start: Port 47290 is taken…".
/// `reason` is already in the user's language ([`start_error`]).
pub fn start_failed(reason: &str) -> String {
    format!(
        "{} {reason}",
        tr(
            "paste-get could not start:",
            "paste-get n'a pas pu démarrer :"
        )
    )
}

/// Why the daemon this tray runs couldn't start, in the user's language when the tray knows the
/// reason. A port is said taken only when the system says so; other errors (no permission, a
/// path too long for a socket) keep the system's words, for a report.
pub fn start_error(e: &anyhow::Error) -> String {
    let in_use = e.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::AddrInUse)
    });
    // "could not listen on 0.0.0.0:47290" (the daemon's port) or "… on /run/…/control.sock"
    // (its control channel), the error's own words.
    let listening_on = e.chain().find_map(|cause| {
        cause
            .to_string()
            .strip_prefix("could not listen on ")
            .map(str::to_owned)
    });
    match (in_use, listening_on) {
        (true, Some(address)) => match address.parse::<std::net::SocketAddr>() {
            Ok(address) if french() => format!(
                "Le port {} est déjà pris : un autre paste-get tourne-t-il ?",
                address.port()
            ),
            Ok(address) => format!(
                "Port {} is taken: is another paste-get running?",
                address.port()
            ),
            // Its control channel's socket: two started at once.
            Err(_) => already_running().into(),
        },
        _ => down_reason(&format!("{e:#}")),
    }
}

fn already_running() -> &'static str {
    tr(
        "Another paste-get already runs for this profile.",
        "Un autre paste-get tourne déjà pour ce profil.",
    )
}

/// Why the daemon couldn't be reached (an error of the control channel), in the user's language
/// for the reasons the tray knows. Others are shown as they are, for a report; the log has them
/// all.
pub fn down_reason(reason: &str) -> String {
    if reason.contains("already running") {
        already_running().into()
    } else if reason.contains("did not answer") || reason.contains("is not running") {
        // The daemon this tray runs stopped answering, or one started elsewhere went away (the
        // tray then starts its own at once). Not `pasteget daemon`'s advice: tray users have no
        // terminal, and the AppImage no `pasteget`.
        tr(
            "It doesn't answer: quit paste-get and start it again.",
            "Il ne répond pas : quitte paste-get et relance-le.",
        )
        .into()
    } else {
        reason.to_owned()
    }
}

/// "● POCO M8", with a way to unpair it one level down, so it isn't clicked by accident.
fn device_entry(device: &DeviceStatusReply) -> Entry {
    let dot = if device.connected { "●" } else { "○" };
    Entry::Submenu {
        label: format!("{dot}  {}", text(&device.name)),
        entries: vec![
            Entry::Info(
                if device.connected {
                    tr("Connected", "Connecté")
                } else {
                    tr("Not connected", "Non connecté")
                }
                .into(),
            ),
            Entry::Action {
                id: format!("{FORGET_PREFIX}{}", device.id),
                label: tr("Forget this device", "Oublier cet appareil").into(),
            },
        ],
    }
}

/// "Text: hello world · POCO M8, 2 min". Clicking it copies the clip again.
fn recent_entry(clip: &RecentReply) -> Entry {
    let from = clip
        .from
        .as_deref()
        .unwrap_or(tr("this computer", "cet ordinateur"));
    Entry::Action {
        id: format!("{RECENT_PREFIX}{}", clip.id),
        label: format!(
            "{} · {}, {}",
            text(&clip.describe()),
            text(from),
            recent::ago(clip.age_secs)
        ),
    }
}

/// Menu-safe text from a device or a clip: one short line, and `&` shown as itself rather
/// than as a keyboard mnemonic (Windows and GTK menus both use it).
fn text(s: &str) -> String {
    recent::preview(s).replace('&', "&&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(paused: bool, devices: &[(&str, bool)]) -> StatusReply {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        StatusReply {
            device_id: "ID".into(),
            device_name: "Desk & Co (Linux)".into(),
            listening_on: "0.0.0.0:47290".into(),
            pairing_open: false,
            paused,
            devices: devices
                .iter()
                .map(|(name, connected)| DeviceStatusReply {
                    id: format!("id-{name}"),
                    name: (*name).into(),
                    connected: *connected,
                })
                .collect(),
            app_version: None,
            control_version: None,
            pid: None,
        }
    }

    fn clip(id: u64, preview: Option<&str>, from: Option<&str>) -> RecentReply {
        RecentReply {
            id,
            mime_type: if preview.is_some() {
                "text/plain".into()
            } else {
                "image/jpeg".into()
            },
            len: 1_258_291,
            from: from.map(Into::into),
            age_secs: 120,
            preview: preview.map(Into::into),
        }
    }

    fn labels(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Info(label) | Entry::Action { label, .. } => label.clone(),
                Entry::Check { label, checked, .. } => {
                    format!("[{}] {label}", if *checked { "x" } else { " " })
                }
                Entry::Submenu { label, entries } => {
                    format!("{label} > {}", labels(entries).join(" | "))
                }
                Entry::Separator => "---".into(),
            })
            .collect()
    }

    #[test]
    fn shows_status_devices_recent_clips_and_pause() {
        let state = State::Running {
            status: status(false, &[("POCO M8 5G", true), ("Laptop", false)]),
            recent: vec![
                clip(7, Some("hello world"), Some("POCO M8 5G")),
                clip(6, None, None),
            ],
        };
        let about = format!("About paste-get {}", env!("CARGO_PKG_VERSION"));

        assert_eq!(
            labels(&entries(&state, Some(true))),
            [
                "paste-get · Desk && Co (Linux)",
                "Syncing with 1 device",
                "---",
                "Devices",
                "●  POCO M8 5G > Connected | Forget this device",
                "○  Laptop > Not connected | Forget this device",
                "Pair a phone…",
                "---",
                "Recent > Text: hello world · POCO M8 5G, 2 min | Image (JPEG, 1.2 MB) · this computer, 2 min",
                "[ ] Pause syncing",
                "---",
                "Settings…",
                "[x] Start at login",
                about.as_str(),
                "Quit",
            ]
        );
    }

    #[test]
    fn says_when_paused_or_alone() {
        let paused = State::Running {
            status: status(true, &[("POCO", true)]),
            recent: Vec::new(),
        };
        assert!(paused.paused());
        assert_eq!(paused.summary(), "Paused");
        let shown = labels(&entries(&paused, None));
        assert!(shown.contains(&"[x] Pause syncing".to_owned()));
        assert!(shown.contains(&"Recent > Nothing yet".to_owned()));

        let alone = State::Running {
            status: status(false, &[]),
            recent: Vec::new(),
        };
        assert!(!alone.paused());
        assert_eq!(alone.summary(), "No device connected");
        let shown = labels(&entries(&alone, None));
        assert!(shown.contains(&"No paired device yet".to_owned()));
        assert!(shown.contains(&"Pair a phone…".to_owned()));
    }

    #[test]
    fn a_stopped_daemon_still_offers_quit() {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        let state = State::Down("Port 47290 is taken: is another paste-get running?".into());

        let entries = entries(&state, Some(false));
        assert!(!state.paused());
        assert_eq!(
            entries.last(),
            Some(&Entry::Action {
                id: QUIT.into(),
                label: "Quit".into()
            })
        );
        assert_eq!(labels(&entries)[0], "paste-get is not running");
        assert_eq!(
            labels(&entries)[1],
            "Port 47290 is taken: is another paste-get running?"
        );
        assert!(labels(&entries).contains(&"[ ] Start at login".to_owned()));
    }

    /// What `Daemon::start` and `control::serve` fail with: the error's words, then the
    /// system's.
    fn listen_error(kind: std::io::ErrorKind, on: &str) -> anyhow::Error {
        anyhow::Error::new(std::io::Error::from(kind)).context(format!("could not listen on {on}"))
    }

    #[test]
    fn a_port_is_said_taken_only_when_it_is() {
        use std::io::ErrorKind::{AddrInUse, InvalidInput, PermissionDenied};
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);

        assert_eq!(
            start_error(&listen_error(AddrInUse, "0.0.0.0:47290")),
            "Port 47290 is taken: is another paste-get running?"
        );
        // From a restart after new settings.
        assert_eq!(
            start_error(
                &listen_error(AddrInUse, "0.0.0.0:47291").context("paste-get could not restart")
            ),
            "Port 47291 is taken: is another paste-get running?"
        );
        // Not allowed (Linux EACCES, Windows WSAEACCES 10013): the system's words stay.
        let denied = start_error(&listen_error(PermissionDenied, "0.0.0.0:80"));
        assert!(
            denied.starts_with("could not listen on 0.0.0.0:80: "),
            "{denied}"
        );
        assert!(!denied.contains("taken"), "{denied}");
        // The control channel's socket, not a port.
        let socket = "/home/me/.config/paste-get/default/control.sock";
        let too_long = start_error(&listen_error(InvalidInput, socket));
        assert!(too_long.starts_with(&format!("could not listen on {socket}: ")));
        assert_eq!(
            start_error(&listen_error(AddrInUse, socket)),
            "Another paste-get already runs for this profile."
        );
        assert_eq!(
            start_error(&anyhow::anyhow!("no D-Bus session")),
            "no D-Bus session"
        );
        assert_eq!(
            start_failed(&start_error(&listen_error(AddrInUse, "0.0.0.0:47290"))),
            "paste-get could not start: Port 47290 is taken: is another paste-get running?"
        );
    }

    #[test]
    fn reasons_the_tray_knows_are_explained_and_others_kept() {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        assert_eq!(
            down_reason("a daemon is already running for this profile"),
            "Another paste-get already runs for this profile."
        );
        assert_eq!(
            down_reason("the daemon did not answer in time"),
            "It doesn't answer: quit paste-get and start it again."
        );
        assert_eq!(down_reason("no D-Bus session"), "no D-Bus session");
        assert_eq!(
            labels(&entries(&State::Starting, None))[0],
            "paste-get is starting…"
        );
    }

    /// What the control channel says when nothing answers on it: not its advice for the
    /// command line (`pasteget daemon`), which tray users don't have.
    #[tokio::test]
    async fn no_daemon_on_the_control_channel_is_explained() {
        use pasteget_desktop::control::{self, Endpoint, Request};
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        let home = tempfile::tempdir().unwrap();
        let profile = pasteget_desktop::config::Profile::open_in(home.path(), "pc").unwrap();

        let e = control::call(&Endpoint::for_profile(&profile), &Request::Status, |_| {})
            .await
            .unwrap_err();

        let gone = State::Down(down_reason(&format!("{e:#}")));
        assert_eq!(
            labels(&entries(&gone, None))[..2],
            [
                "paste-get is not running",
                "It doesn't answer: quit paste-get and start it again."
            ]
        );
    }

    #[test]
    fn clicks_map_back_to_commands() {
        assert_eq!(Command::from_id(PAUSE), Some(Command::TogglePause));
        assert_eq!(Command::from_id(QUIT), Some(Command::Quit));
        assert_eq!(Command::from_id(PAIR), Some(Command::Pair));
        assert_eq!(Command::from_id(SETTINGS), Some(Command::Settings));
        assert_eq!(Command::from_id(ABOUT), Some(Command::About));
        assert_eq!(
            Command::from_id(START_AT_LOGIN),
            Some(Command::ToggleStartAtLogin)
        );
        assert_eq!(Command::from_id("recent:42"), Some(Command::CopyRecent(42)));
        assert_eq!(Command::from_id("recent:x"), None);
        assert_eq!(
            Command::from_id("forget:ABC"),
            Some(Command::Forget("ABC".into()))
        );
        assert_eq!(Command::from_id("other"), None);

        // Every clickable entry maps to a command.
        let state = State::Running {
            status: status(false, &[("POCO", true)]),
            recent: vec![clip(3, Some("a"), None)],
        };
        fn ids(entries: &[Entry]) -> Vec<String> {
            entries
                .iter()
                .flat_map(|entry| match entry {
                    Entry::Action { id, .. } | Entry::Check { id, .. } => vec![id.clone()],
                    Entry::Submenu { entries, .. } => ids(entries),
                    _ => Vec::new(),
                })
                .collect()
        }
        for id in ids(&entries(&state, Some(false))) {
            assert!(Command::from_id(&id).is_some(), "{id}");
        }
    }
}
