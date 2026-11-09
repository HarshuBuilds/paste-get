// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows: a network marked *Public* may keep the phone from connecting. Windows' firewall then
//! blocks what comes in to paste-get (TCP 47290 and mDNS, UDP 5353), unless the network is made
//! *Private*, where the rule added at paste-get's first start (or the one below) lets it in.
//! The tray says so once in a notice, and the Settings window says how to fix it.
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

/// What the Settings window shows about a Public network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Advice {
    pub text: String,
    /// A firewall rule to add from a terminal opened as administrator.
    pub command: String,
}

/// The firewall rule that lets `program` (the tray, which runs the daemon) in on private
/// networks only, for every port it uses: 47290 (or the one set) and mDNS.
pub fn firewall_command(program: &Path) -> String {
    format!(
        "netsh advfirewall firewall add rule name=\"paste-get\" dir=in action=allow \
         program=\"{}\" enable=yes profile=private",
        program.display()
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

/// The notice, short: details are in Settings.
pub fn notice(networks: &[String]) -> String {
    format!(
        "{}{}",
        treated_as_public(networks),
        tr(
            ": its firewall may keep your phone from reaching paste-get. paste-get's Settings \
             say what to do.",
            " : son pare-feu risque d'empêcher ton téléphone de joindre paste-get. Les réglages \
             de paste-get disent quoi faire.",
        )
    )
}

/// What Settings says about the `findings`, with the rule for `program`. `None` when no network
/// is Public.
pub fn advice(findings: &Findings, program: &Path) -> Option<Advice> {
    if findings.public.is_empty() {
        return None;
    }
    let text = format!(
        "{}{}",
        treated_as_public(&findings.public),
        tr(
            ": its firewall may then keep your other devices from connecting to paste-get. If it \
             is your home or work network, make it Private: Settings → Network & internet → \
             Wi-Fi or Ethernet → the network's properties → Network profile type → Private \
             network. If the phone still can't connect, run this line in a terminal opened as \
             administrator (right-click Start → Terminal (Admin)): it lets paste-get in on \
             private networks only.",
            " : son pare-feu risque alors d'empêcher tes autres appareils de se connecter à \
             paste-get. Si c'est ton réseau à la maison ou au travail, rends-le privé : \
             Paramètres → Réseau et Internet → Wi-Fi ou Ethernet → les propriétés du réseau → \
             Type de profil réseau → Réseau privé. Si le téléphone ne se connecte toujours pas, \
             lance cette ligne dans un terminal ouvert en administrateur (clic droit sur \
             Démarrer → Terminal (administrateur)) : elle laisse passer paste-get sur les \
             réseaux privés seulement.",
        )
    );
    Some(Advice {
        text,
        command: firewall_command(program),
    })
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
/// network is Public that wasn't the last time. Windows only.
#[cfg_attr(not(windows), allow(dead_code))]
pub async fn watch(tell: impl Fn(String)) {
    let Some(program) = this_program() else {
        return;
    };
    let mut told: Vec<String> = Vec::new();
    loop {
        let public = findings_now(program.clone()).await.public;
        if !public.is_empty() && public.iter().any(|n| !told.contains(n)) {
            tracing::info!(count = public.len(), "a network is marked Public");
            tell(notice(&public));
        }
        told = public;
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() {
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
    }

    fn public(names: &[&str]) -> Findings {
        Findings {
            public: names.iter().map(|&n| n.into()).collect(),
        }
    }

    #[test]
    fn the_advice_names_the_network_and_gives_one_command() {
        english();
        let program = Path::new(r"C:\Users\me\AppData\Local\paste-get\pasteget-tray.exe");

        assert_eq!(advice(&Findings::default(), program), None);
        let advice = advice(&public(&["Livebox-1234"]), program).unwrap();
        assert!(advice.text.contains("the network “Livebox-1234” as Public"));
        assert!(advice.text.contains("may then keep"));
        assert!(advice.text.contains("Private network"));
        assert_eq!(
            advice.command,
            r#"netsh advfirewall firewall add rule name="paste-get" dir=in action=allow program="C:\Users\me\AppData\Local\paste-get\pasteget-tray.exe" enable=yes profile=private"#
        );
        assert!(!advice.command.contains('\n'));
    }

    #[test]
    fn the_notice_says_may_and_counts_the_networks() {
        english();
        assert_eq!(
            notice(&["A".into()]),
            "Windows treats the network “A” as Public: its firewall may keep your phone from \
             reaching paste-get. paste-get's Settings say what to do."
        );
        assert!(
            notice(&["A".into(), "B".into(), "C".into()])
                .starts_with("Windows treats the networks “A”, “B” and “C” as Public: ")
        );
    }
}
