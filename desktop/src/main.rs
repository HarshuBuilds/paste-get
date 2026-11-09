// SPDX-License-Identifier: GPL-3.0-or-later

//! `pasteget`: desktop daemon and command-line tool.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anstream::{eprintln, println};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use data_encoding::BASE64;
use pasteget_core::clip::{DEFAULT_MAX_CLIP_LEN, mime};
use pasteget_desktop::clipboard::{MemoryClipboard, SystemClipboard};
use pasteget_desktop::config::{DEFAULT_PROFILE, Profile};
use pasteget_desktop::control::{self, Endpoint, Request, Response, StatusReply};
use pasteget_desktop::daemon::{Daemon, DaemonOptions};
use pasteget_desktop::i18n::{french, tr};
use pasteget_desktop::notify::DesktopNotifier;
use pasteget_desktop::recent;
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "pasteget",
    version,
    about = "Copy on your phone, paste on your PC.",
    long_about = None
)]
struct Cli {
    /// Profile to use. Each profile is a separate device with its own pairings.
    #[arg(long, global = true, default_value = DEFAULT_PROFILE)]
    profile: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon in the foreground (Ctrl+C to stop).
    Daemon {
        /// Use a private in-memory clipboard instead of the system one. For running two
        /// daemons on one machine while testing.
        #[arg(long)]
        memory_clipboard: bool,
        /// Port to listen on, instead of the one in settings.toml.
        #[arg(long)]
        port: Option<u16>,
    },
    /// Show a pairing QR code and wait for a device to pair.
    Pair,
    /// Pair with another paste-get device using the code it shows.
    Join {
        /// The pairing code (starts with pasteget://pair?).
        invite: String,
    },
    /// Send text, a text or image file, or the clipboard, to every connected device.
    Send {
        /// Text to send.
        #[arg(required_unless_present_any = ["file", "clipboard"], conflicts_with = "file")]
        text: Option<String>,
        /// File to send instead: UTF-8 text, PNG or JPEG.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Send what's on the clipboard now, even while paused. Handy for a hotkey.
        #[arg(long, conflicts_with_all = ["text", "file"])]
        clipboard: bool,
    },
    /// Show this device and its paired devices.
    Status {
        /// Print one line of JSON instead, for scripts and status bars such as Waybar (see
        /// docs/TESTING.md, section 9). Also when paste-get isn't running: "running": false.
        #[arg(long)]
        json: bool,
    },
    /// Stop sending copies and receiving clips until `pasteget resume`.
    /// `pasteget send` still works while paused.
    Pause {
        /// Resume instead if already paused. Handy for a hotkey.
        #[arg(long)]
        toggle: bool,
    },
    /// Resume syncing after `pasteget pause`.
    Resume,
    /// Do what a hotkey does, with its notification. For desktops that bind keys to commands,
    /// such as Hyprland.
    Hotkey {
        #[arg(value_parser = ["toggle-pause", "send-clipboard"])]
        action: String,
    },
    /// List the last clips sent or received, newest first.
    Recent,
    /// Unpair a device: it's disconnected now and can only come back by pairing again.
    Forget {
        /// Its name, as `pasteget status` shows it, or its ID.
        device: String,
    },
    /// Stop the running daemon, and the tray if it runs it.
    Quit,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let profile = Profile::open(&cli.profile)?;
    let endpoint = Endpoint::for_profile(&profile);
    match cli.command {
        Command::Daemon {
            memory_clipboard,
            port,
        } => daemon(profile, &endpoint, memory_clipboard, port).await,
        Command::Pair => pair(&endpoint).await,
        Command::Join { invite } => join(&endpoint, invite).await,
        Command::Send {
            clipboard: true, ..
        } => send_request(&endpoint, &Request::SendClipboard).await,
        Command::Send { text, file, .. } => send(&endpoint, text, file).await,
        Command::Status { json: false } => status(&endpoint).await,
        Command::Status { json: true } => status_json(&endpoint).await,
        Command::Pause { toggle } => pause(&endpoint, if toggle { None } else { Some(true) }).await,
        Command::Resume => pause(&endpoint, Some(false)).await,
        // The notification says what happened.
        Command::Hotkey { action } => {
            expect_one(&endpoint, &Request::Perform { action }, |response| {
                matches!(response, Response::Performed)
            })
            .await
        }
        Command::Recent => recent(&endpoint).await,
        Command::Forget { device } => forget(&endpoint, device).await,
        Command::Quit => {
            expect_one(&endpoint, &Request::Quit, |response| {
                matches!(response, Response::Quitting)
            })
            .await?;
            println!("Stopped.");
            Ok(())
        }
    }
}

async fn daemon(
    profile: Profile,
    endpoint: &Endpoint,
    memory_clipboard: bool,
    port: Option<u16>,
) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    if control::is_running(endpoint).await {
        bail!(
            "a daemon is already running for profile \"{}\"",
            profile.name()
        );
    }
    let mut options = DaemonOptions::listen_on(port.unwrap_or(profile.settings()?.port));
    // A memory-clipboard daemon stands in for another device: its notifications and hotkeys
    // would only confuse.
    if !memory_clipboard {
        options.notifier = Some(Arc::new(DesktopNotifier::start()));
        options.hotkeys = true;
    }
    let daemon = if memory_clipboard {
        Daemon::start(profile, MemoryClipboard::default(), options).await?
    } else {
        Daemon::start(profile, SystemClipboard::new()?, options).await?
    };
    let daemon = Arc::new(daemon);
    let server = control::serve(Arc::clone(&daemon), endpoint).await?;

    let status = daemon.status().await;
    println!(
        "paste-get is running as \"{}\" on port {}.",
        status.device_name,
        status.listening_on.port()
    );
    if status.devices.is_empty() {
        println!("No device paired yet: run `pasteget pair` in another terminal, or use the tray.");
    }
    println!("Press Ctrl+C to stop.");

    let mut quit = daemon.quit_signal();
    tokio::select! {
        stopped = tokio::signal::ctrl_c() => stopped.context("could not wait for Ctrl+C")?,
        // `pasteget quit`, or a newer tray taking over.
        _ = quit.wait_for(|quit| *quit) => {}
    }
    server.abort();
    control::cleanup(endpoint);
    println!("Stopped.");
    Ok(())
}

async fn pair(endpoint: &Endpoint) -> Result<()> {
    let mut failure = None;
    control::call(endpoint, &Request::Pair, |response| match response {
        Response::Invite {
            uri,
            expires_in_secs,
        } => {
            if let Err(e) = print_qr(&uri) {
                eprintln!("(could not draw the QR code: {e})");
            }
            println!("Scan this code with paste-get on your phone.");
            println!("From another computer, run instead:");
            println!("  pasteget join \"{uri}\"");
            println!(
                "The code works once and expires in {} minutes.",
                expires_in_secs / 60
            );
        }
        Response::PairingAttemptFailed { reason } => {
            eprintln!("A device tried to pair but failed: {reason}");
        }
        Response::Paired(device) => println!("✓ Paired with {} ({}).", device.name, device.id),
        Response::Error { message } => failure = Some(message),
        _ => {}
    })
    .await?;
    match failure {
        Some(message) => bail!(message),
        None => Ok(()),
    }
}

async fn join(endpoint: &Endpoint, invite: String) -> Result<()> {
    let request = Request::Join { invite };
    expect_one(endpoint, &request, |response| match response {
        Response::Joined(device) => {
            println!("✓ Paired with {} ({}).", device.name, device.id);
            true
        }
        _ => false,
    })
    .await
}

async fn send(endpoint: &Endpoint, text: Option<String>, file: Option<PathBuf>) -> Result<()> {
    let (mime_type, data) = match (text, file) {
        (Some(text), _) => (mime::TEXT, text.into_bytes()),
        (None, Some(path)) => {
            let data = std::fs::read(&path)
                .with_context(|| format!("could not read {}", path.display()))?;
            if let Some(image) = mime::of_image(&data) {
                (image, data)
            } else if std::str::from_utf8(&data).is_ok() {
                (mime::TEXT, data)
            } else {
                bail!("only UTF-8 text, PNG and JPEG files can be sent for now");
            }
        }
        (None, None) => bail!("nothing to send"),
    };
    if data.len() as u64 > DEFAULT_MAX_CLIP_LEN {
        bail!(
            "too large: the limit is {} MiB",
            DEFAULT_MAX_CLIP_LEN / (1024 * 1024)
        );
    }
    let request = Request::Send {
        mime_type: mime_type.to_owned(),
        data_base64: BASE64.encode(&data),
    };
    send_request(endpoint, &request).await
}

async fn forget(endpoint: &Endpoint, device: String) -> Result<()> {
    expect_one(
        endpoint,
        &Request::Forget { device },
        |response| match response {
            Response::Forgotten(device) => {
                println!(
                    "Unpaired {}. To use it again, pair it again; on a phone, forget this computer \
                 there too.",
                    device.name
                );
                true
            }
            _ => false,
        },
    )
    .await
}

async fn send_request(endpoint: &Endpoint, request: &Request) -> Result<()> {
    expect_one(endpoint, request, |response| match response {
        Response::Sent { devices: 0 } => {
            println!("No device is connected right now.");
            true
        }
        Response::Sent { devices } => {
            println!("Sent to {devices} device(s).");
            true
        }
        _ => false,
    })
    .await
}

async fn status(endpoint: &Endpoint) -> Result<()> {
    expect_one(endpoint, &Request::Status, |response| {
        let Response::Status(status) = response else {
            return false;
        };
        println!("{}", status.device_name);
        println!("  ID:        {}", status.device_id);
        println!("  Listening: {}", status.listening_on);
        if let Some(version) = &status.app_version {
            println!("  Version:   {version}");
        }
        if status.pairing_open {
            println!("  Pairing:   a pairing code is being shown");
        }
        if status.paused {
            println!("  Paused:    nothing is synced until `pasteget resume`");
        }
        if status.devices.is_empty() {
            println!("No paired device. Run `pasteget pair` to add one.");
        } else {
            println!("Paired devices:");
            for device in status.devices {
                let state = if device.connected {
                    "● connected"
                } else {
                    "○ offline"
                };
                println!("  {state}  {}", device.name);
            }
        }
        true
    })
    .await
}

/// `pasteget status --json`: one line, even when no daemon answers, so that a bar polling it
/// shows that too.
async fn status_json(endpoint: &Endpoint) -> Result<()> {
    let mut status = None;
    let call = control::call(endpoint, &Request::Status, |response| {
        if let Response::Status(reply) = response {
            status = Some(reply);
        }
    });
    // A bar asks every few seconds: don't wait as long as the other commands.
    let _ = tokio::time::timeout(Duration::from_secs(5), call).await;
    println!("{}", status_value(status.as_ref()));
    Ok(())
}

/// What `pasteget status --json` prints. The daemon's facts, plus `text`, `alt`, `class` and
/// `tooltip` for a Waybar `custom` module (`return-type: json`): the number of devices
/// connected, and one of `stopped`, `paused`, `connected`, `away` (none connected) or
/// `unpaired`. Waybar reads `text` and `tooltip` as Pango markup: names are escaped there.
/// The tooltip is in the user's language, like the tray: the bar shows it.
fn status_value(status: Option<&StatusReply>) -> serde_json::Value {
    let Some(status) = status else {
        return json!({
            "running": false,
            "text": "0",
            "alt": "stopped",
            "class": "stopped",
            "tooltip": tr("paste-get is not running", "paste-get ne tourne pas"),
        });
    };
    let connected: Vec<&str> = status
        .devices
        .iter()
        .filter(|device| device.connected)
        .map(|device| device.name.as_str())
        .collect();
    let (state, tooltip) = if status.paused {
        (
            "paused",
            tr("paste-get: paused", "paste-get : en pause").to_owned(),
        )
    } else if !connected.is_empty() {
        let names = markup(&connected.join(", "));
        let tooltip = if french() {
            format!("paste-get : connecté à {names}")
        } else {
            format!("paste-get: connected to {names}")
        };
        ("connected", tooltip)
    } else if !status.devices.is_empty() {
        let tooltip = tr(
            "paste-get: no device connected",
            "paste-get : aucun appareil connecté",
        );
        ("away", tooltip.to_owned())
    } else {
        let tooltip = tr(
            "paste-get: no paired device yet",
            "paste-get : aucun appareil appairé",
        );
        ("unpaired", tooltip.to_owned())
    };
    json!({
        "running": true,
        "name": status.device_name,
        "id": status.device_id,
        "version": status.app_version,
        "paused": status.paused,
        "pairing": status.pairing_open,
        "devices": status.devices.iter().map(|device| json!({
            "id": device.id,
            "name": device.name,
            "connected": device.connected,
        })).collect::<Vec<_>>(),
        "connected": connected.len(),
        "text": connected.len().to_string(),
        "alt": state,
        "class": state,
        "tooltip": tooltip,
    })
}

/// `text` as plain text in Pango markup.
fn markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn pause(endpoint: &Endpoint, paused: Option<bool>) -> Result<()> {
    expect_one(
        endpoint,
        &Request::Pause { paused },
        |response| match response {
            Response::Paused { paused: true } => {
                println!("Paused: nothing is synced until `pasteget resume`.");
                true
            }
            Response::Paused { paused: false } => {
                println!("Syncing.");
                true
            }
            _ => false,
        },
    )
    .await
}

async fn recent(endpoint: &Endpoint) -> Result<()> {
    expect_one(endpoint, &Request::Recent, |response| {
        let Response::Recent { clips } = response else {
            return false;
        };
        if clips.is_empty() {
            println!("Nothing yet.");
        }
        for clip in clips {
            println!(
                "{:>8}  {:<20}  {}",
                recent::ago(clip.age_secs),
                clip.from.as_deref().unwrap_or("this computer"),
                clip.describe()
            );
        }
        true
    })
    .await
}

/// Sends a one-answer request. `on_success` returns `false` for unexpected answers.
async fn expect_one(
    endpoint: &Endpoint,
    request: &Request,
    mut on_success: impl FnMut(Response) -> bool,
) -> Result<()> {
    let mut outcome = Err(anyhow::anyhow!("the daemon did not answer"));
    let call = control::call(endpoint, request, |response| {
        outcome = match response {
            Response::Error { message } => Err(anyhow::anyhow!(message)),
            other => {
                if on_success(other) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("unexpected answer from the daemon"))
                }
            }
        };
    });
    tokio::time::timeout(control::CLIENT_TIMEOUT, call)
        .await
        .context("the daemon did not answer in time")??;
    outcome
}

/// Draws a QR code with Unicode half blocks, forced black on white so it scans on dark themes.
fn print_qr(text: &str) -> Result<()> {
    let code = QrCode::new(text.as_bytes())?;
    let rendered = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Dark)
        .light_color(Dense1x2::Light)
        .quiet_zone(true)
        .build();
    for line in rendered.lines() {
        println!("\x1b[30;47m{line}\x1b[0m");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasteget_desktop::control::DeviceStatusReply;

    fn status(paused: bool, devices: &[(&str, bool)]) -> StatusReply {
        // The tooltips compared below are the English ones.
        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        StatusReply {
            device_id: "PCID".into(),
            device_name: "HP-AIO".into(),
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
            app_version: Some("0.1.0".into()),
            control_version: Some(1),
            pid: Some(42),
        }
    }

    #[test]
    fn hotkey_takes_the_actions_hyprland_binds() {
        for action in ["toggle-pause", "send-clipboard"] {
            let cli = Cli::try_parse_from(["pasteget", "hotkey", action]).unwrap();
            assert!(matches!(cli.command, Command::Hotkey { action: a } if a == action));
            // The lines the tray's Settings window gives (`hotkeys::hyprland_lines`).
            let action = pasteget_desktop::hotkeys::Action::from_id(action).unwrap();
            let words: Vec<&str> = action.command().split(' ').collect();
            assert!(Cli::try_parse_from(words).is_ok(), "{}", action.command());
        }
        assert!(Cli::try_parse_from(["pasteget", "hotkey", "nonsense"]).is_err());
    }

    #[test]
    fn status_json_is_one_line_for_scripts_and_waybar() {
        let value = status_value(Some(&status(
            false,
            &[("POCO <M8>", true), ("Laptop", false)],
        )));

        assert_eq!(
            value.to_string(),
            r#"{"alt":"connected","class":"connected","connected":1,"devices":[{"connected":true,"id":"id-POCO <M8>","name":"POCO <M8>"},{"connected":false,"id":"id-Laptop","name":"Laptop"}],"id":"PCID","name":"HP-AIO","pairing":false,"paused":false,"running":true,"text":"1","tooltip":"paste-get: connected to POCO &lt;M8&gt;","version":"0.1.0"}"#
        );
        assert!(!value.to_string().contains('\n'));
    }

    #[test]
    fn status_json_names_each_state() {
        let state = |status: Option<&StatusReply>| {
            let value = status_value(status);
            (
                value["class"].clone(),
                value["text"].clone(),
                value["running"].clone(),
            )
        };

        pasteget_desktop::i18n::force(pasteget_desktop::i18n::Lang::En);
        assert_eq!(state(None), (json!("stopped"), json!("0"), json!(false)));
        assert_eq!(status_value(None)["tooltip"], "paste-get is not running");
        assert_eq!(
            state(Some(&status(true, &[("POCO", true)]))),
            (json!("paused"), json!("1"), json!(true))
        );
        assert_eq!(
            state(Some(&status(false, &[("POCO", false)]))),
            (json!("away"), json!("0"), json!(true))
        );
        assert_eq!(
            state(Some(&status(false, &[]))),
            (json!("unpaired"), json!("0"), json!(true))
        );
    }
}
