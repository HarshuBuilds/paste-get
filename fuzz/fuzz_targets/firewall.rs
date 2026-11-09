// SPDX-License-Identifier: GPL-3.0-or-later

//! Parses arbitrary text as what PowerShell prints about the networks and the firewall rules
//! (network names come from whoever runs the Wi-Fi), then checks it.
//! Run with `cargo +nightly fuzz run firewall`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pasteget_desktop::firewall;

fuzz_target!(|data: &[u8]| {
    let snapshot = firewall::parse(&String::from_utf8_lossy(data));
    let lan: Vec<u32> = snapshot.networks.iter().map(|network| network.index).collect();
    let findings = firewall::check(&snapshot, &lan);
    // Only networks it was told about are warned about, each once per list.
    for list in [&findings.public, &findings.blocked] {
        for (i, name) in list.iter().enumerate() {
            assert!(snapshot.networks.iter().any(|network| &network.name == name));
            assert!(!list[..i].contains(name));
        }
    }
});
