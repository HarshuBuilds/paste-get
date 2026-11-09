// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows: a network marked *Public* may keep the phone from connecting. Windows' firewall then
//! blocks what comes in to paste-get (TCP 47290 and mDNS, UDP 5353), unless the network is made
//! *Private*, where the rule added at paste-get's first start (or the one below) lets it in. A
//! rule made when Windows' question at the first start was cancelled blocks it everywhere. The
//! tray says so once in a notice, and the Settings window always says how to fix it.
//!
//! What is checked, and how, is in [`pasteget_desktop::firewall`]: PowerShell reads it locally,
//! nothing goes on the network.

use std::path::{Path, PathBuf};

use pasteget_desktop::firewall::{self, Findings};
use pasteget_desktop::i18n::{french, tr};
use serde::Serialize;

/// How often the networks are checked again: one may change (another Wi-Fi).
#[cfg_attr(not(windows), allow(dead_code))]
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// What the Settings window's *Network* part shows, always on Windows: what may keep the phone
/// out now, if anything, and what to do when it can't connect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Advice {
    /// What was found, if anything: shown in red.
    pub warning: Option<String>,
    pub text: String,
    /// Lines to run in a terminal opened as administrator.
    pub command: String,
}

/// The lines that replace every inbound firewall rule of `program` (the tray, which runs the
/// daemon) by one that lets it in on private networks only, for every port it uses: 47290 (or
/// the one set) and mDNS. Removing them first also removes the rules that block it, which would
/// win over the new one. They work pasted in PowerShell or in the Command Prompt, and can be run
/// again.
pub fn firewall_command(program: &Path) -> String {
    let program = program.display();
    format!(
        "netsh advfirewall firewall delete rule name=all dir=in program=\"{program}\"\n\
         netsh advfirewall firewall add rule name=\"paste-get\" dir=in action=allow \
         program=\"{program}\" enable=yes profile=private"
    )
}

/// "“Wi-Fi 5”", or "“A” and “B”".
fn quoted(networks: &[String]) -> String {
    let quoted: Vec<String> = networks.iter().map(|n| format!("“{n}”")).collect();
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} {} {last}", rest.join(", "), tr("and", "et")),
        None => String::new(),
    }
}

/// "Windows treats the network “A” as Public", or "the networks “A” and “B”".
fn treated_as_public(networks: &[String]) -> String {
    let names = quoted(networks);
    match (french(), networks.len() > 1) {
        (false, false) => format!("Windows treats the network {names} as Public"),
        (false, true) => format!("Windows treats the networks {names} as Public"),
        (true, false) => format!("Windows considère le réseau {names} comme public"),
        (true, true) => format!("Windows considère les réseaux {names} comme publics"),
    }
}

/// "Windows' firewall keeps paste-get out on the network “A”", or "the networks “A” and “B”".
fn kept_out(networks: &[String]) -> String {
    let names = quoted(networks);
    match (french(), networks.len() > 1) {
        (false, false) => format!("Windows' firewall keeps paste-get out on the network {names}"),
        (false, true) => format!("Windows' firewall keeps paste-get out on the networks {names}"),
        (true, false) => format!("Le pare-feu de Windows bloque paste-get sur le réseau {names}"),
        (true, true) => format!("Le pare-feu de Windows bloque paste-get sur les réseaux {names}"),
    }
}

/// The notice, as a title and a line under it, short: the names and the details are in
/// Settings. `None` when nothing was found.
pub fn notice(findings: &Findings) -> Option<(String, &'static str)> {
    let several = |names: &[String]| names.len() > 1;
    let (title, body) = if !findings.blocked.is_empty() {
        (
            tr(
                "The firewall blocks paste-get",
                "Le pare-feu bloque paste-get",
            ),
            tr(
                "Your phone can't reach paste-get on this network. paste-get's Settings say what \
                 to do.",
                "Ton téléphone ne peut pas joindre paste-get sur ce réseau. Les réglages de \
                 paste-get disent quoi faire.",
            ),
        )
    } else if !findings.public.is_empty() {
        (
            if several(&findings.public) {
                tr("Networks marked Public", "Réseaux marqués publics")
            } else {
                tr("Network marked Public", "Réseau marqué public")
            },
            tr(
                "Windows' firewall may keep your phone from reaching paste-get. paste-get's \
                 Settings say what to do.",
                "Le pare-feu de Windows risque d'empêcher ton téléphone de joindre paste-get. Les \
                 réglages de paste-get disent quoi faire.",
            ),
        )
    } else {
        return None;
    };
    Some((title.to_owned(), body))
}

/// What Settings says about the `findings`, with the lines for `program`.
pub fn advice(findings: &Findings, program: &Path) -> Advice {
    let mut warnings = Vec::new();
    if !findings.public.is_empty() {
        warnings.push(format!(
            "{}{}",
            treated_as_public(&findings.public),
            tr(
                ": its firewall may keep your phone from reaching paste-get. If it is your home \
                 or work network, make it Private, as below.",
                " : son pare-feu risque d'empêcher ton téléphone de joindre paste-get. Si c'est \
                 ton réseau à la maison ou au travail, rends-le privé, comme ci-dessous.",
            )
        ));
    }
    if !findings.blocked.is_empty() {
        warnings.push(format!(
            "{}{}",
            kept_out(&findings.blocked),
            tr(
                ": run the lines below to let it in.",
                " : lance les lignes ci-dessous pour le laisser passer.",
            )
        ));
    }
    let text = tr(
        "Phone can't connect? Its network must be Private on this computer: Settings → Network & \
         internet → Wi-Fi or Ethernet → the network's properties → Network profile type → \
         Private network. If it still can't, run these lines in a terminal opened as \
         administrator (right-click Start → Terminal (Admin)). They replace paste-get's firewall \
         rules, including any that block it (made when Windows' question at the first start was \
         cancelled), by one that lets it in on private networks only.",
        "Le téléphone ne se connecte pas ? Son réseau doit être privé sur cet ordinateur : \
         Paramètres → Réseau et Internet → Wi-Fi ou Ethernet → les propriétés du réseau → Type \
         de profil réseau → Réseau privé. Si ça ne suffit pas, lance ces lignes dans un terminal \
         ouvert en administrateur (clic droit sur Démarrer → Terminal (administrateur)). Elles \
         remplacent les règles de pare-feu de paste-get, y compris celles qui le bloquent \
         (créées quand la question de Windows au premier démarrage a été annulée), par une seule \
         qui le laisse passer sur les réseaux privés seulement.",
    );
    Advice {
        warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
        text: text.into(),
        command: firewall_command(program),
    }
}

/// What PowerShell says now about the networks and `program`'s rules. `None` if it can't say.
#[cfg(windows)]
fn snapshot(program: &Path) -> Option<firewall::Snapshot> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut child = Command::new(crate::startup::system32(
        r"WindowsPowerShell\v1.0\powershell.exe",
    ))
    .args(["-NoProfile", "-NonInteractive", "-Command", "-"])
    // Read by the script: no quoting of the path to get wrong.
    .env("PASTEGET_PROGRAM", program)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .creation_flags(crate::startup::CREATE_NO_WINDOW)
    .spawn()
    .inspect_err(|e| tracing::warn!(error = %e, "could not run PowerShell to list networks"))
    .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(firewall::SCRIPT.as_bytes());
    }
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| firewall::parse(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(windows))]
fn snapshot(_program: &Path) -> Option<firewall::Snapshot> {
    None
}

/// The tray's own program: it runs the daemon, so the firewall rules are for it.
pub fn this_program() -> Option<PathBuf> {
    std::env::current_exe()
        .inspect_err(|e| tracing::warn!(error = %e, "could not find this program"))
        .ok()
}

/// What may keep the phone out now (Windows only; nothing elsewhere or if Windows can't say).
pub async fn findings_now(program: PathBuf) -> Findings {
    tauri::async_runtime::spawn_blocking(move || {
        snapshot(&program).map(|snapshot| firewall::check(&snapshot, &firewall::lan_interfaces()))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// Checks the networks now and every few minutes, and calls `tell` with the notice when a
/// network may keep the phone out that didn't the last time. Windows only.
#[cfg_attr(not(windows), allow(dead_code))]
pub async fn watch(tell: impl Fn((String, &'static str))) {
    let Some(program) = this_program() else {
        return;
    };
    let mut told = Findings::default();
    loop {
        let findings = findings_now(program.clone()).await;
        let new = |now: &[String], before: &[String]| now.iter().any(|n| !before.contains(n));
        if (new(&findings.public, &told.public) || new(&findings.blocked, &told.blocked))
            && let Some(notice) = notice(&findings)
        {
            tracing::info!(
                public = findings.public.len(),
                blocked = findings.blocked.len(),
                "the firewall may keep the phone out"
            );
            tell(notice);
        }
        told = findings;
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
    }

    fn found(public: &[&str], blocked: &[&str]) -> Findings {
        let names = |names: &[&str]| names.iter().map(|&n| n.into()).collect();
        Findings {
            public: names(public),
            blocked: names(blocked),
        }
    }

    #[test]
    fn the_advice_is_always_there_and_says_what_was_found() {
        english();
        let program = Path::new(r"C:\Users\me\AppData\Local\paste-get\pasteget-tray.exe");

        // Nothing found: still the way to fix it, without a warning.
        let calm = advice(&Findings::default(), program);
        assert_eq!(calm.warning, None);
        assert!(calm.text.starts_with("Phone can't connect? "));
        assert!(calm.text.contains("Private network"));
        assert_eq!(
            calm.command,
            "netsh advfirewall firewall delete rule name=all dir=in program=\"C:\\Users\\me\\AppData\\Local\\paste-get\\pasteget-tray.exe\"\n\
             netsh advfirewall firewall add rule name=\"paste-get\" dir=in action=allow program=\"C:\\Users\\me\\AppData\\Local\\paste-get\\pasteget-tray.exe\" enable=yes profile=private"
        );
        assert_eq!(calm.command.lines().count(), 2);

        let public = advice(&found(&["Livebox-1234"], &[]), program);
        let warning = public.warning.unwrap();
        assert!(warning.starts_with("Windows treats the network “Livebox-1234” as Public: "));
        assert!(warning.contains("may keep"));
        let blocked = advice(&found(&[], &["Home", "Office"]), program);
        assert_eq!(
            blocked.warning.as_deref(),
            Some(
                "Windows' firewall keeps paste-get out on the networks “Home” and “Office”: \
                 run the lines below to let it in."
            )
        );
        assert_eq!(blocked.text, calm.text);
    }

    #[test]
    fn the_notice_has_a_short_title_and_says_may() {
        english();
        assert_eq!(notice(&Findings::default()), None);
        let (title, body) = notice(&found(&["Livebox-1234"], &[])).unwrap();
        assert_eq!(title, "Network marked Public");
        assert_eq!(
            body,
            "Windows' firewall may keep your phone from reaching paste-get. paste-get's Settings \
             say what to do."
        );
        assert_eq!(
            notice(&found(&["A", "B"], &[])).unwrap().0,
            "Networks marked Public"
        );
        // A rule that blocks it is the surer cause.
        assert_eq!(
            notice(&found(&["A"], &["A"])).unwrap().0,
            "The firewall blocks paste-get"
        );
        // A Windows toast's title holds two lines; the line under it holds the rest.
        for findings in [
            found(&["A"], &[]),
            found(&["A", "B"], &[]),
            found(&[], &["A"]),
        ] {
            let (title, body) = notice(&findings).unwrap();
            assert!(title.chars().count() <= 32, "{title}");
            assert!(body.chars().count() <= 120, "{body}");
        }
    }
}
