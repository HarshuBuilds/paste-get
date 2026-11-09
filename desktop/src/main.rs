// SPDX-License-Identifier: GPL-3.0-or-later

//! `pasteget`: desktop daemon and command-line tool.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::SystemTime;

use anstream::{eprintln, println};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use data_encoding::BASE64;
use pasteget_core::clip::{DEFAULT_MAX_CLIP_LEN, mime};
use pasteget_desktop::clipboard::{MemoryClipboard, SystemClipboard};
use pasteget_desktop::config::{DEFAULT_PROFILE, Profile};
use pasteget_desktop::control::{self, Endpoint, HistoryItemReply, Request, Response};
use pasteget_desktop::daemon::{Daemon, DaemonOptions};
use pasteget_desktop::history::{parse_duration, picked_id, plain_line};
use pasteget_desktop::notify::DesktopNotifier;
use pasteget_desktop::recent;
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

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
    Status,
    /// Stop sending copies and receiving clips until `pasteget resume`.
    /// `pasteget send` still works while paused.
    Pause {
        /// Resume instead if already paused. Handy for a hotkey.
        #[arg(long)]
        toggle: bool,
    },
    /// Resume syncing after `pasteget pause`.
    Resume,
    /// List the last clips sent or received, newest first.
    Recent,
    /// Unpair a device: it's disconnected now and can only come back by pairing again.
    Forget {
        /// Its name, as `pasteget status` shows it, or its ID.
        device: String,
    },
    /// Stop the running daemon, and the tray if it runs it.
    Quit,
    /// The clipboard history, once turned on in settings.toml (`[history]`, `enabled = true`).
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
}

#[derive(Subcommand)]
enum HistoryCommand {
    /// List the newest items, pinned ones marked with *.
    List {
        /// How many.
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// One "ID<TAB>preview" line per item, for a picker: fuzzel, wofi, fzf…
        #[arg(long)]
        plain: bool,
    },
    /// List the items that hold these words (any part of a word, accents ignored), newest
    /// first. Also site:github.com, from:<device>, type:link|image|code…, is:pinned,
    /// after:2026-11-01, before:….
    Search {
        #[arg(required = true)]
        words: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        /// One "ID<TAB>preview" line per item, for a picker.
        #[arg(long)]
        plain: bool,
    },
    /// Put an item back on the clipboard. It isn't sent to the other devices.
    Copy {
        /// Its ID, as `list` shows it.
        #[arg(required_unless_present = "stdin")]
        id: Option<i64>,
        /// Read a line of `list --plain` from standard input, as a picker prints it.
        #[arg(long, conflicts_with = "id")]
        stdin: bool,
    },
    /// Pin an item: it stays until deleted.
    Pin {
        id: i64,
    },
    Unpin {
        id: i64,
    },
    /// Delete items, pinned or not.
    Delete {
        #[arg(required = true)]
        ids: Vec<i64>,
    },
    /// Delete what wasn't pinned, or only what was copied in the last 30m, 2h, 7d…
    Clear {
        /// Only the items copied in the last 30s, 15m, 2h, 7d…
        #[arg(long, value_parser = duration)]
        last: Option<i64>,
        /// Pinned items too.
        #[arg(long)]
        pinned: bool,
    },
    /// How many items, how much space, how many secrets were kept out.
    Stats,
}

/// A `--last` value, in milliseconds.
fn duration(text: &str) -> Result<i64, String> {
    parse_duration(text)
        .ok_or_else(|| format!("{text:?} is not a duration like 30s, 15m, 2h or 7d"))
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
        Command::Status => status(&endpoint).await,
        Command::Pause { toggle } => pause(&endpoint, if toggle { None } else { Some(true) }).await,
        Command::Resume => pause(&endpoint, Some(false)).await,
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
        Command::History { command } => history(&endpoint, command).await,
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

async fn history(endpoint: &Endpoint, command: HistoryCommand) -> Result<()> {
    let page = |query: String, limit: u32| Request::History {
        query,
        after: None,
        limit: Some(limit),
        utc_offset_minutes: 0,
    };
    match command {
        HistoryCommand::List { limit, plain } => {
            list_history(endpoint, &page(String::new(), limit), plain).await
        }
        HistoryCommand::Search {
            words,
            limit,
            plain,
        } => list_history(endpoint, &page(words.join(" "), limit), plain).await,
        HistoryCommand::Copy { id, stdin } => {
            let id = match (id, stdin) {
                (Some(id), _) => id,
                (None, _) => {
                    let mut line = String::new();
                    std::io::stdin().lock().read_line(&mut line)?;
                    // Nothing picked: the picker was closed.
                    if line.trim().is_empty() {
                        return Ok(());
                    }
                    picked_id(&line).context("that line doesn't start with an item's ID")?
                }
            };
            expect_one(endpoint, &Request::HistoryCopy { id }, |response| {
                matches!(response, Response::Copied)
            })
            .await
        }
        HistoryCommand::Pin { id } => pin(endpoint, id, true).await,
        HistoryCommand::Unpin { id } => pin(endpoint, id, false).await,
        HistoryCommand::Delete { ids } => deleted(endpoint, &Request::HistoryDelete { ids }).await,
        HistoryCommand::Clear { last, pinned } => {
            let since_ms = last.map(|last| control::unix_ms(SystemTime::now()) - last);
            let request = Request::HistoryClear {
                since_ms,
                pinned_too: pinned,
            };
            deleted(endpoint, &request).await
        }
        HistoryCommand::Stats => {
            expect_one(endpoint, &Request::HistoryStats, |response| {
                let Response::HistoryStats(stats) = response else {
                    return false;
                };
                println!(
                    "{} items, {} pinned, {} images",
                    stats.items, stats.pinned, stats.images
                );
                println!("{} on disk", recent::size(stats.bytes as usize));
                println!(
                    "{} passwords, codes and keys kept out in the last 7 days",
                    stats.kept_out_this_week
                );
                true
            })
            .await
        }
    }
}

async fn list_history(endpoint: &Endpoint, request: &Request, plain: bool) -> Result<()> {
    expect_one(endpoint, request, |response| {
        let Response::HistoryPage { items, .. } = response else {
            return false;
        };
        if items.is_empty() && !plain {
            println!("Nothing found.");
        }
        let now = control::unix_ms(SystemTime::now());
        for item in items {
            println!("{}", history_line(&item, now, plain));
        }
        true
    })
    .await
}

/// One item as `pasteget history list` prints it.
fn history_line(item: &HistoryItemReply, now_ms: i64, plain: bool) -> String {
    if plain {
        let preview = if item.mime_type == mime::TEXT {
            item.preview.clone()
        } else {
            item.describe()
        };
        return plain_line(item.id, &preview);
    }
    let age_secs = u64::try_from((now_ms - item.last_at_ms) / 1000).unwrap_or(0);
    let from = item.from.as_deref().unwrap_or("this computer");
    format!(
        "{:>6} {}{:>9}  {:<20}  {}",
        item.id,
        if item.pinned { '*' } else { ' ' },
        recent::ago(age_secs),
        pasteget_desktop::history::escape(from),
        item.describe()
    )
}

async fn pin(endpoint: &Endpoint, id: i64, pinned: bool) -> Result<()> {
    expect_one(endpoint, &Request::HistoryPin { id, pinned }, |response| {
        matches!(response, Response::HistoryPinned { .. })
    })
    .await
}

async fn deleted(endpoint: &Endpoint, request: &Request) -> Result<()> {
    expect_one(endpoint, request, |response| {
        let Response::HistoryDeleted { items } = response else {
            return false;
        };
        println!("Deleted {items} item(s).");
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

    fn item(preview: &str) -> HistoryItemReply {
        HistoryItemReply {
            id: 12,
            mime_type: mime::TEXT.into(),
            category: "text".into(),
            size: preview.len() as u64,
            from: Some("POCO".into()),
            via: Some("POCO".into()),
            last_at_ms: 1_000_000 - 120_000,
            times: 1,
            pinned: true,
            preview: preview.into(),
            highlights: Vec::new(),
        }
    }

    /// Whatever a clip holds, each item is one line, and no terminal escape gets through.
    #[test]
    fn history_lines_are_escaped() {
        let clip = "first line\n\tsecond\u{1b}[2J";

        let shown = history_line(&item(clip), 1_000_000, false);
        let plain = history_line(&item(clip), 1_000_000, true);

        assert_eq!(
            shown,
            "    12 *    2 min  POCO                  Text: first line second\\u{1b}[2J"
        );
        assert_eq!(plain, "12\tfirst line\\n\\tsecond\\u{1b}[2J");
        for line in [&shown, &plain] {
            assert!(!line.contains('\n') && !line.contains('\u{1b}'), "{line:?}");
        }
        assert_eq!(picked_id(&plain), Some(12));
    }

    #[test]
    fn images_are_described_in_plain_lines_too() {
        let image = HistoryItemReply {
            mime_type: mime::PNG.into(),
            size: 1_500_000,
            preview: String::new(),
            ..item("")
        };

        assert_eq!(
            history_line(&image, 1_000_000, true),
            format!("12\tImage (PNG, {})", recent::size(1_500_000))
        );
    }
}
