// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows: whether its firewall may keep a phone from reaching paste-get. On a network Windows
//! marks *Public*, the firewall lets paste-get in only if one of its rules allows that, and the
//! rule added at the first start usually covers private networks only. Cancelling Windows'
//! question at the first start adds rules that block paste-get instead, and a block rule wins
//! over any allow rule.
//!
//! The tray runs [`SCRIPT`] in PowerShell, which reads the network profiles and the program's
//! firewall rules locally (nothing goes on the network), then [`parse`]s what it prints and
//! [`check`]s it. Only networks the phone may be on count: adapters of virtual machines,
//! containers and VPNs often show up as Public "Unidentified network" too.

/// Prints one line per fact, tab-separated: `network` (interface index, category, adapter,
/// name), `firewall` (profile, enabled), `rule` (action, profiles, enabled) for each inbound
/// rule of the program in `PASTEGET_PROGRAM`, then `rules read` once they could all be read.
/// Control characters in names are replaced: a Wi-Fi's name is chosen by whoever runs it.
/// One line: PowerShell reads `-Command -` line by line.
pub const SCRIPT: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
    Get-NetConnectionProfile | ForEach-Object { \
    \"network`t$($_.InterfaceIndex)`t$($_.NetworkCategory)`t$($_.InterfaceAlias -replace '\\p{Cc}', ' ')`t$($_.Name -replace '\\p{Cc}', ' ')\" }; \
    Get-NetFirewallProfile -PolicyStore ActiveStore | ForEach-Object { \"firewall`t$($_.Name)`t$($_.Enabled)\" }; \
    try { $program = $env:PASTEGET_PROGRAM; \
    foreach ($filter in @(Get-NetFirewallApplicationFilter -PolicyStore ActiveStore -ErrorAction Stop | \
    Where-Object { $program -and [Environment]::ExpandEnvironmentVariables($_.Program) -ieq $program })) { \
    Get-NetFirewallRule -AssociatedNetFirewallApplicationFilter $filter -ErrorAction Stop | \
    Where-Object { $_.Direction -eq 'Inbound' } | \
    ForEach-Object { \"rule`t$($_.Action)`t$($_.Profile)`t$($_.Enabled)\" } }; \"rules`tread\" } catch {}\n";

/// A network this computer is connected to, as Windows sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    /// The adapter's interface index, as in `if_addrs`.
    pub index: u32,
    /// `Public`, `Private` or `DomainAuthenticated`.
    pub category: String,
    /// The adapter: "Wi-Fi", "Ethernet", "vEthernet (WSL)".
    pub interface: String,
    /// The network's name: the Wi-Fi's, or "Network 2".
    pub name: String,
}

/// An inbound firewall rule for paste-get's program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// `Allow` rather than `Block`.
    pub allow: bool,
    /// The firewall profiles it applies to: "Private", "Domain, Private", "Any".
    pub profiles: String,
    /// Unticked in *Allow an app through firewall*: the rule stays, and does nothing.
    pub enabled: bool,
}

impl Rule {
    fn covers(&self, profile: &str) -> bool {
        self.enabled
            && self
                .profiles
                .split(',')
                .map(str::trim)
                .any(|p| p.eq_ignore_ascii_case("Any") || p.eq_ignore_ascii_case(profile))
    }
}

/// What PowerShell said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub networks: Vec<Network>,
    /// Firewall profiles turned off: "Public", "Private", "Domain".
    pub off: Vec<String>,
    /// The program's rules; `None` if they couldn't be read.
    pub rules: Option<Vec<Rule>>,
}

/// What [`SCRIPT`] printed. Lines it doesn't know are skipped.
pub fn parse(output: &str) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let mut rules = Vec::new();
    let mut rules_read = false;
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        let Some((kind, rest)) = line.split_once('\t') else {
            continue;
        };
        match kind {
            "network" => {
                let mut fields = rest.splitn(4, '\t').map(str::trim);
                let (Some(index), Some(category), Some(interface)) =
                    (fields.next(), fields.next(), fields.next())
                else {
                    continue;
                };
                let Ok(index) = index.parse() else {
                    continue;
                };
                if category.is_empty() {
                    continue;
                }
                let name = fields.next().unwrap_or_default();
                snapshot.networks.push(Network {
                    index,
                    category: category.to_owned(),
                    interface: interface.to_owned(),
                    name: if name.is_empty() { interface } else { name }.to_owned(),
                });
            }
            "firewall" => {
                if let Some((profile, enabled)) = rest.split_once('\t')
                    && enabled.trim().eq_ignore_ascii_case("False")
                {
                    snapshot.off.push(profile.trim().to_owned());
                }
            }
            "rule" => {
                let mut fields = rest.splitn(3, '\t').map(str::trim);
                if let (Some(action), Some(profiles), enabled) =
                    (fields.next(), fields.next(), fields.next())
                {
                    rules.push(Rule {
                        allow: action.eq_ignore_ascii_case("Allow"),
                        profiles: profiles.to_owned(),
                        enabled: !enabled.is_some_and(|e| e.eq_ignore_ascii_case("False")),
                    });
                }
            }
            "rules" => rules_read = rest.trim() == "read",
            _ => {}
        }
    }
    snapshot.rules = rules_read.then_some(rules);
    snapshot
}

/// What may keep a phone out, network by network (their names). A network may be in both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Findings {
    /// Marked Public, and no rule lets paste-get in there (or the rules couldn't be read).
    pub public: Vec<String>,
    /// A rule blocks paste-get there, or, on a private or domain network, paste-get has rules
    /// but none lets it in.
    pub blocked: Vec<String>,
}

/// The firewall profile of a network category.
fn profile_of(category: &str) -> Option<&'static str> {
    ["Public", "Private"]
        .into_iter()
        .find(|profile| category.eq_ignore_ascii_case(profile))
        .or_else(|| {
            category
                .eq_ignore_ascii_case("DomainAuthenticated")
                .then_some("Domain")
        })
}

/// The networks of `snapshot` on the adapters in `lan` ([`lan_interfaces`]) that the firewall
/// may close to paste-get.
pub fn check(snapshot: &Snapshot, lan: &[u32]) -> Findings {
    let mut findings = Findings::default();
    for network in &snapshot.networks {
        if !lan.contains(&network.index) || crate::daemon::looks_virtual(&network.interface) {
            continue;
        }
        let Some(profile) = profile_of(&network.category) else {
            continue;
        };
        if snapshot
            .off
            .iter()
            .any(|off| off.eq_ignore_ascii_case(profile))
        {
            continue;
        }
        // No rules at all, even unticked ones: Windows hasn't asked yet, or is asking now.
        let rules = snapshot.rules.as_deref().filter(|rules| !rules.is_empty());
        let any = |allow: bool| {
            rules.is_some_and(|rules| {
                rules
                    .iter()
                    .any(|rule| rule.allow == allow && rule.covers(profile))
            })
        };
        if profile == "Public" && !any(true) {
            add(&mut findings.public, &network.name);
        }
        if any(false) || (profile != "Public" && rules.is_some() && !any(true)) {
            add(&mut findings.blocked, &network.name);
        }
    }
    findings
}

/// Adds `name` to `names`, once.
fn add(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_owned());
    }
}

/// The interface indexes of the adapters a phone may reach this computer through: up, with an
/// IPv4 address that isn't loopback or link-local (like the addresses in a pairing code), and
/// neither point-to-point (VPNs) nor named like a virtual machine's or a container's.
pub fn lan_interfaces() -> Vec<u32> {
    let mut indexes: Vec<u32> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| {
            interface.is_oper_up()
                && !interface.is_loopback()
                && !interface.is_link_local()
                && interface.ip().is_ipv4()
                && !interface.is_p2p
                && !crate::daemon::looks_virtual(&interface.name)
        })
        .filter_map(|interface| interface.index)
        .collect();
    indexes.dedup();
    indexes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(index: u32, category: &str, interface: &str, name: &str) -> Network {
        Network {
            index,
            category: category.into(),
            interface: interface.into(),
            name: name.into(),
        }
    }

    fn rule(allow: bool, profiles: &str) -> Rule {
        Rule {
            allow,
            profiles: profiles.into(),
            enabled: true,
        }
    }

    #[test]
    fn powershells_lines_give_networks_profiles_and_rules() {
        let output = "network\t12\tPublic\tWi-Fi\tLivebox-1234\r\n\
                      network\t7\tPrivate\tEthernet\tNetwork 2\r\n\
                      network\t30\tDomainAuthenticated\tVPN\t\r\n\
                      network\tx\tPublic\tWi-Fi\tbad index\r\n\
                      network\t9\tPublic\tWi-Fi\tA\ttab\r\n\
                      firewall\tDomain\tTrue\r\n\
                      firewall\tPublic\tFalse\r\n\
                      rule\tAllow\tPrivate\tTrue\r\n\
                      rule\tBlock\tDomain, Public\tTrue\r\n\
                      rule\tAllow\tPublic\tFalse\r\n\
                      rules\tread\r\n\
                      \r\n\
                      not a line\r\n";

        let snapshot = parse(output);
        assert_eq!(
            snapshot.networks,
            [
                network(12, "Public", "Wi-Fi", "Livebox-1234"),
                network(7, "Private", "Ethernet", "Network 2"),
                // No name: the adapter's.
                network(30, "DomainAuthenticated", "VPN", "VPN"),
                // A tab in a Wi-Fi's name stays in it.
                network(9, "Public", "Wi-Fi", "A\ttab"),
            ]
        );
        assert_eq!(snapshot.off, ["Public"]);
        assert_eq!(
            snapshot.rules,
            Some(vec![
                rule(true, "Private"),
                rule(false, "Domain, Public"),
                Rule {
                    enabled: false,
                    ..rule(true, "Public")
                },
            ])
        );
        // Rules that couldn't all be read are unknown, not "none".
        assert_eq!(parse("rule\tAllow\tAny\n").rules, None);
        assert_eq!(parse("rules\tread\n").rules, Some(Vec::new()));
    }

    /// The decision table: which networks are warned about.
    #[test]
    fn public_networks_of_the_phone_are_warned_about_unless_allowed() {
        let home = network(1, "Private", "Wi-Fi", "Home");
        let cafe = network(2, "Public", "Wi-Fi", "Café");
        let office = network(3, "DomainAuthenticated", "Ethernet", "corp.example");
        let hotspot = network(4, "Public", "Ethernet 2", "Phone hotspot");
        let wsl = network(5, "Public", "vEthernet (WSL)", "Unidentified network");
        let hyper_v = network(
            6,
            "Public",
            "vEthernet (Default Switch)",
            "Unidentified network",
        );
        let vmware = network(
            7,
            "Public",
            "VMware Network Adapter VMnet8",
            "Unidentified network",
        );
        let vpn = network(8, "Public", "ProtonVPN", "Network 3");
        // Up with an IPv4 address: every one but the VPN, which is point-to-point.
        let lan = [1, 2, 3, 4, 5, 6, 7];
        let snapshot = |networks: &[&Network], rules: Option<Vec<Rule>>| Snapshot {
            networks: networks.iter().copied().cloned().collect(),
            off: Vec::new(),
            rules,
        };
        let only_private = Some(vec![rule(true, "Private")]);
        let cases: [(Snapshot, &[&str]); 12] = [
            // Not connected: nothing to say here.
            (snapshot(&[], None), &[]),
            (snapshot(&[&home], None), &[]),
            (snapshot(&[&office], None), &[]),
            (snapshot(&[&cafe], None), &["Café"]),
            (snapshot(&[&cafe], only_private.clone()), &["Café"]),
            // Ethernet private, Wi-Fi public: the phone is likely on the Wi-Fi.
            (snapshot(&[&office, &cafe], None), &["Café"]),
            (
                snapshot(&[&cafe, &hotspot], None),
                &["Café", "Phone hotspot"],
            ),
            // The home Wi-Fi is Private: virtual adapters marked Public don't matter.
            (snapshot(&[&home, &wsl, &hyper_v, &vmware, &vpn], None), &[]),
            // Public allowed at the first start ("Any" too).
            (
                snapshot(&[&cafe], Some(vec![rule(true, "Private, Public")])),
                &[],
            ),
            (snapshot(&[&cafe], Some(vec![rule(true, "Any")])), &[]),
            // An adapter without an address the phone could use.
            (
                snapshot(&[&network(9, "Public", "Wi-Fi 2", "Old")], None),
                &[],
            ),
            // However Windows writes it.
            (
                snapshot(&[&network(1, "public", "Wi-Fi", "Lower")], None),
                &["Lower"],
            ),
        ];

        for (snapshot, warned) in cases {
            let findings = check(&snapshot, &lan);
            assert_eq!(findings.public, warned, "{snapshot:?}");
            assert!(findings.blocked.is_empty(), "{snapshot:?}");
        }
        // The Public firewall turned off (another firewall in charge, say).
        let mut off = snapshot(&[&cafe], only_private.clone());
        off.off.push("Public".into());
        assert_eq!(check(&off, &lan), Findings::default());

        // Windows' first-start question cancelled: rules that block paste-get, which win over
        // any rule that allows it.
        let blocks = |profiles: &str| Some(vec![rule(true, "Private"), rule(false, profiles)]);
        let unticked = Rule {
            enabled: false,
            ..rule(true, "Private")
        };
        // The networks, their rules, then which are said Public and which blocked.
        type Case<'a> = (
            &'a [&'a Network],
            Option<Vec<Rule>>,
            &'a [&'a str],
            &'a [&'a str],
        );
        let cases: [Case<'_>; 8] = [
            (&[&home], blocks("Private"), &[], &["Home"]),
            (&[&home], blocks("Public"), &[], &[]),
            (
                &[&cafe, &office],
                blocks("Any"),
                &["Café"],
                &["Café", "corp.example"],
            ),
            // Allowed on Public only: a Private network keeps it out too.
            (&[&home], Some(vec![rule(true, "Public")]), &[], &["Home"]),
            // Unticked in *Allow an app through firewall*: Windows won't ask again.
            (&[&home], Some(vec![unticked]), &[], &["Home"]),
            // No rule yet: Windows is still to ask.
            (&[&home], Some(Vec::new()), &[], &[]),
            (&[&home], None, &[], &[]),
            // Virtual adapters still don't count.
            (&[&wsl], blocks("Any"), &[], &[]),
        ];
        for (networks, rules, public, blocked) in cases {
            let findings = check(&snapshot(networks, rules.clone()), &lan);
            assert_eq!(findings.public, public, "{networks:?} {rules:?}");
            assert_eq!(findings.blocked, blocked, "{networks:?} {rules:?}");
        }
    }

    #[test]
    fn the_script_is_one_line_that_reads_only() {
        assert_eq!(SCRIPT.lines().count(), 1);
        assert!(SCRIPT.ends_with('\n'));
        // Reads, never changes: no Set-, New-, Remove- or netsh.
        for verb in ["Set-", "New-", "Remove-", "Enable-", "Disable-", "netsh"] {
            assert!(!SCRIPT.contains(verb), "{verb}");
        }
        assert!(SCRIPT.contains("$env:PASTEGET_PROGRAM"));
    }
}
