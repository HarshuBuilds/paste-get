// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows: a network marked *Public* keeps the phone from connecting. Windows' firewall then
//! blocks what comes in to paste-get (TCP 47290 and mDNS, UDP 5353), unless the network is made
//! *Private*, where the rule added at paste-get's first start (or the one below) lets it in.
//! The tray says so once in a notice, and the Settings window says how to fix it.
//!
//! The network profiles come from PowerShell's `Get-NetConnectionProfile`, which reads them
//! locally: nothing goes on the network.

use std::path::Path;

use pasteget_desktop::i18n::{french, tr};
use serde::Serialize;

/// How often the networks are checked again: one may change (another Wi-Fi).
#[cfg_attr(not(windows), allow(dead_code))]
const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// A network this computer is connected to, as Windows sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// `Public`, `Private` or `DomainAuthenticated`.
    pub category: String,
    /// The adapter: "Wi-Fi", "Ethernet".
    pub interface: String,
    /// The network's name: the Wi-Fi's, or "Network 2".
    pub name: String,
}

/// What the Settings window shows about a Public network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Advice {
    pub text: String,
    /// A firewall rule to add from a terminal opened as administrator.
    pub command: String,
}

/// The connections in PowerShell's output: one per line, category, adapter and name separated
/// by tabs. Other lines are skipped.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse(output: &str) -> Vec<Connection> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim_end_matches('\r').splitn(3, '\t');
            let category = fields.next()?.trim();
            let interface = fields.next()?.trim();
            let name = fields.next().unwrap_or_default().trim();
            (!category.is_empty() && !interface.is_empty()).then(|| Connection {
                category: category.to_owned(),
                interface: interface.to_owned(),
                name: if name.is_empty() { interface } else { name }.to_owned(),
            })
        })
        .collect()
}

/// The networks to warn about: those marked Public. Private and domain networks are fine; with
/// none connected there is nothing to say here.
pub fn public_networks(connections: &[Connection]) -> Vec<String> {
    let mut names: Vec<String> = connections
        .iter()
        .filter(|c| c.category.eq_ignore_ascii_case("Public"))
        .map(|c| c.name.clone())
        .collect();
    names.dedup();
    names
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

/// The notice, short: details are in Settings.
pub fn notice(networks: &[String]) -> String {
    if french() {
        format!(
            "Windows considère le réseau {} comme public : ton téléphone ne peut pas joindre \
             paste-get. Les réglages de paste-get disent comment corriger.",
            quoted(networks)
        )
    } else {
        format!(
            "Windows treats the network {} as Public: your phone can't reach paste-get. \
             paste-get's Settings say how to fix it.",
            quoted(networks)
        )
    }
}

/// What Settings says about `networks`, with the rule for `program`. `None` when no network is
/// Public.
pub fn advice(networks: &[String], program: &Path) -> Option<Advice> {
    if networks.is_empty() {
        return None;
    }
    let names = quoted(networks);
    let text = if french() {
        format!(
            "Windows considère le réseau {names} comme public : son pare-feu empêche alors tes \
             autres appareils de se connecter à paste-get. Si c'est ton réseau à la maison ou au \
             travail, rends-le privé : Paramètres → Réseau et Internet → Wi-Fi (ou Ethernet) → \
             {names} → Réseau privé. Si le téléphone ne se connecte toujours pas, lance cette \
             ligne dans un terminal ouvert en administrateur (clic droit sur Démarrer → Terminal \
             (administrateur)) : elle laisse passer paste-get sur les réseaux privés seulement."
        )
    } else {
        format!(
            "Windows treats the network {names} as Public: its firewall then keeps your other \
             devices from connecting to paste-get. If it is your home or work network, make it \
             Private: Settings → Network & internet → Wi-Fi (or Ethernet) → {names} → Private \
             network. If the phone still can't connect, run this line in a terminal opened as \
             administrator (right-click Start → Terminal (Admin)): it lets paste-get in on \
             private networks only."
        )
    };
    Some(Advice {
        text,
        command: firewall_command(program),
    })
}

/// The connections now, from PowerShell. `None` if it can't say.
#[cfg(windows)]
pub fn connections() -> Option<Vec<Connection>> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const LIST: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
        Get-NetConnectionProfile | ForEach-Object { \
        \"$($_.NetworkCategory)`t$($_.InterfaceAlias)`t$($_.Name)\" }\n";

    let mut child = Command::new(crate::startup::system32(
        r"WindowsPowerShell\v1.0\powershell.exe",
    ))
    .args(["-NoProfile", "-NonInteractive", "-Command", "-"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .creation_flags(crate::startup::CREATE_NO_WINDOW)
    .spawn()
    .inspect_err(|e| tracing::warn!(error = %e, "could not run PowerShell to list networks"))
    .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(LIST.as_bytes());
    }
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| parse(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(not(windows))]
pub fn connections() -> Option<Vec<Connection>> {
    None
}

/// The Public networks now (Windows only; empty elsewhere or if Windows can't say).
pub async fn public_now() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(connections)
        .await
        .ok()
        .flatten()
        .map(|connections| public_networks(&connections))
        .unwrap_or_default()
}

/// Checks the networks now and every few minutes, and calls `tell` with the notice when a
/// network is Public that wasn't the last time. Windows only.
#[cfg_attr(not(windows), allow(dead_code))]
pub async fn watch(tell: impl Fn(String)) {
    let mut told: Vec<String> = Vec::new();
    loop {
        let public = public_now().await;
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

    fn connection(category: &str, interface: &str, name: &str) -> Connection {
        Connection {
            category: category.into(),
            interface: interface.into(),
            name: name.into(),
        }
    }

    #[test]
    fn powershells_list_gives_each_network() {
        let output = "Public\tWi-Fi\tLivebox-1234\r\n\
                      Private\tEthernet\tNetwork 2\r\n\
                      DomainAuthenticated\tVPN\t\r\n\
                      \r\n\
                      not a line\r\n";

        assert_eq!(
            parse(output),
            [
                connection("Public", "Wi-Fi", "Livebox-1234"),
                connection("Private", "Ethernet", "Network 2"),
                // No name: the adapter's.
                connection("DomainAuthenticated", "VPN", "VPN"),
            ]
        );
    }

    /// The decision table: which connections are warned about.
    #[test]
    fn only_public_networks_are_warned_about() {
        let home = connection("Private", "Wi-Fi", "Home");
        let cafe = connection("Public", "Wi-Fi", "Café");
        let office = connection("DomainAuthenticated", "Ethernet", "corp.example");
        let hotspot = connection("Public", "Ethernet 2", "Phone hotspot");
        let cases: [(&[Connection], &[&str]); 7] = [
            // Not connected: nothing to say here.
            (&[], &[]),
            (std::slice::from_ref(&home), &[]),
            (std::slice::from_ref(&office), &[]),
            (std::slice::from_ref(&cafe), &["Café"]),
            // Ethernet private, Wi-Fi public: the phone is likely on the Wi-Fi.
            (&[office.clone(), cafe.clone()], &["Café"]),
            (&[cafe.clone(), hotspot.clone()], &["Café", "Phone hotspot"]),
            // However Windows writes it.
            (&[connection("public", "Wi-Fi", "Lower")], &["Lower"]),
        ];

        for (connections, warned) in cases {
            assert_eq!(public_networks(connections), warned, "{connections:?}");
        }
    }

    #[test]
    fn the_advice_names_the_network_and_gives_one_command() {
        english();
        let program = Path::new(r"C:\Users\me\AppData\Local\paste-get\pasteget-tray.exe");

        assert_eq!(advice(&[], program), None);
        let advice = advice(&["Livebox-1234".into()], program).unwrap();
        assert!(advice.text.contains("“Livebox-1234” as Public"));
        assert!(advice.text.contains("Private network"));
        assert_eq!(
            advice.command,
            r#"netsh advfirewall firewall add rule name="paste-get" dir=in action=allow program="C:\Users\me\AppData\Local\paste-get\pasteget-tray.exe" enable=yes profile=private"#
        );
        assert!(!advice.command.contains('\n'));
        assert_eq!(
            notice(&["A".into(), "B".into(), "C".into()]),
            "Windows treats the network “A”, “B” and “C” as Public: your phone can't reach \
             paste-get. paste-get's Settings say how to fix it."
        );
    }
}
