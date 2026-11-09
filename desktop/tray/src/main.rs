// SPDX-License-Identifier: GPL-3.0-or-later

//! `pasteget-tray`: the paste-get tray icon.
//!
//! It runs the daemon in this process, unless one already runs for the profile (started with
//! `pasteget daemon`, for instance): then it only shows that one. Either way it talks to the
//! daemon through the same control channel as the `pasteget` command, so the menu shows exactly
//! what `pasteget status` and `pasteget recent` would.
//!
//! A newer tray replaces an older paste-get still running for the profile (after an update),
//! and a tray asked to quit through the control channel exits.

// No console window on Windows. Debug builds keep it, for the logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
#[cfg(test)]
mod i18n_check;
mod linux_tray;
mod menu;
mod network;
mod pairing;
mod settings;
mod startup;

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use pasteget_desktop::autostart::{self, Autostart, Launch};
use pasteget_desktop::clipboard::SystemClipboard;
use pasteget_desktop::config::{DEFAULT_PROFILE, Profile};
use pasteget_desktop::control::{self, Endpoint, Request, Response};
use pasteget_desktop::daemon::{Daemon, DaemonOptions};
use pasteget_desktop::hotkeys::{self, Action, Runner};
use pasteget_desktop::notify::{DesktopNotifier, Notifier};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, RunEvent, Runtime};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::menu::{Command, Entry, State};

/// How often the menu is brought up to date when nothing is clicked.
const REFRESH_EVERY: Duration = Duration::from_secs(2);
/// How long to wait before trying again to start a daemon that failed to start.
const RESTART_EVERY: Duration = Duration::from_secs(15);
/// How long a control request may take before the daemon counts as stuck.
const ASK_TIMEOUT: Duration = Duration::from_secs(5);
/// Restarting the hosted daemon: tries, and the pause before each.
const RESTART_ATTEMPTS: usize = 10;
const RESTART_PAUSE: Duration = Duration::from_millis(300);

const TRAY_ID: &str = "paste-get";
const ICON: &[u8] = include_bytes!("../icons/tray.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/tray-paused.png");

#[derive(Parser)]
#[command(name = "pasteget-tray", version, about = "paste-get tray icon.")]
struct Cli {
    /// Profile to use, as with `pasteget --profile`.
    #[arg(long, default_value = DEFAULT_PROFILE)]
    profile: String,
    /// Started at login rather than by the user: no notice saying it runs.
    #[arg(long, hide = true)]
    at_login: bool,
    /// Do what a hotkey does in the running paste-get, then exit: `toggle-pause` or
    /// `send-clipboard`. For desktops that bind keys to commands (Hyprland), with the AppImage,
    /// which has no `pasteget` command.
    #[arg(long, value_name = "ACTION")]
    action: Option<String>,
}

/// Shared by the menu handler and the refresh loop.
struct Tray {
    profile: Profile,
    endpoint: Endpoint,
    /// The daemon this process runs, if it runs one.
    hosted: Mutex<Option<Hosted>>,
    /// `None` where starting at login isn't supported.
    autostart: Option<Autostart>,
    /// Wakes the refresh loop right after a click.
    refresh: Notify,
    /// Rebuild the menu even if nothing changed: a click ticks a checkbox by itself, even when
    /// the action behind it fails.
    redraw: AtomicBool,
    /// The pairing window's task. A new one replaces it.
    pairing: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// Held while the hosted daemon starts or restarts, so only one runs.
    starting: tokio::sync::Mutex<()>,
    /// Started at login: no "paste-get is running" notice.
    at_login: bool,
}

struct Hosted {
    _daemon: Arc<Daemon>,
    server: JoinHandle<()>,
    /// [`Daemon::quit_signal`].
    quit: watch::Receiver<bool>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(action) = &cli.action {
        return match run_action(&cli.profile, action) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    let log = startup::init_logs(&cli.profile);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), profile = %cli.profile, "tray starting");
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:#}");
            startup::show_fatal(&format!("{e:#}"), log.as_deref());
            ExitCode::FAILURE
        }
    }
}

/// `--action`: what `pasteget pause --toggle` or `pasteget send --clipboard` would do.
fn run_action(profile: &str, action: &str) -> Result<()> {
    let action = Action::from_id(action)
        .ok_or_else(|| anyhow!("unknown action {action:?}: toggle-pause or send-clipboard"))?;
    let request = match action {
        Action::TogglePause => Request::Pause { paused: None },
        Action::SendClipboard => Request::SendClipboard,
    };
    let endpoint = Endpoint::for_profile(&Profile::open(profile)?);
    tauri::async_runtime::block_on(ask(&endpoint, &request))?;
    Ok(())
}

fn run(cli: &Cli) -> Result<()> {
    let profile = Profile::open(&cli.profile)?;
    let endpoint = Endpoint::for_profile(&profile);
    tauri::async_runtime::block_on(startup::replace_older(&profile, &endpoint));
    // One tray per profile: a second one would show a second icon, and look like it did nothing.
    let Some(_lock) = startup::lock_profile(&profile)? else {
        tracing::info!("another tray already runs for this profile");
        startup::notify_once(startup::already_running_notice());
        return Ok(());
    };
    let autostart = if autostart::SUPPORTED {
        Autostart::for_profile(profile.name())
            .inspect_err(|e| tracing::warn!(error = %e, "can't start at login"))
            .ok()
    } else {
        None
    };
    // Rewritten each start: it follows the program if it moved, and gets `--at-login`.
    if let Some(autostart) = &autostart
        && autostart.is_enabled()
        && let Err(e) = login_launch(profile.name()).and_then(|launch| autostart.enable(&launch))
    {
        tracing::warn!(error = %format!("{e:#}"), "could not update starting at login");
    }
    let tray = Arc::new(Tray {
        endpoint,
        profile,
        hosted: Mutex::new(None),
        autostart,
        refresh: Notify::new(),
        redraw: AtomicBool::new(false),
        pairing: Mutex::new(None),
        starting: tokio::sync::Mutex::new(()),
        at_login: cli.at_login,
    });

    let for_setup = Arc::clone(&tray);
    let app = tauri::Builder::default()
        .manage(Arc::clone(&tray))
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            close_window,
            app_info,
            hyprland_lines,
            network_advice
        ])
        .setup(move |app| {
            // A tray app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup(app.handle(), for_setup)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .context("could not start the tray")?;
    app.run(move |_, event| match event {
        // Closing the pairing window, the only window, would end the app: only Quit does.
        // (Quit asks with an exit code; closing the last window asks without one.)
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => tray.stop_hosted(),
        _ => {}
    });
    Ok(())
}

fn setup<R: Runtime>(app: &AppHandle<R>, tray: Arc<Tray>) -> Result<()> {
    let state = State::Starting;
    let for_clicks = Arc::clone(&tray);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .tooltip(tooltip(&state))
        .menu(&build_menu(
            app,
            &menu::entries(&state, tray.start_at_login()),
        )?)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            if let Some(command) = Command::from_id(event.id().as_ref()) {
                on_command(app, &for_clicks, command);
            }
        })
        .build(app)?;
    linux_tray::warn_if_no_tray();
    // Windows: a Public network keeps the phone out. Said once per network.
    #[cfg(windows)]
    tauri::async_runtime::spawn(async {
        let notifier = DesktopNotifier::start();
        network::watch(move |text| notifier.notice(text)).await;
    });
    tauri::async_runtime::spawn(refresh_loop(app.clone(), tray));
    Ok(())
}

fn on_command<R: Runtime>(app: &AppHandle<R>, tray: &Arc<Tray>, command: Command) {
    let request = match command {
        Command::Quit => {
            app.exit(0);
            return;
        }
        Command::Pair => {
            open_pairing(app, tray);
            return;
        }
        Command::Settings => {
            if let Err(e) = settings::open(app) {
                tracing::warn!(error = %format!("{e:#}"), "could not open the settings");
            }
            return;
        }
        Command::About => {
            if let Err(e) = about::open(app) {
                tracing::warn!(error = %format!("{e:#}"), "could not open the About window");
            }
            return;
        }
        Command::ToggleStartAtLogin => {
            if let Err(e) = tray.toggle_start_at_login() {
                tracing::warn!(error = %format!("{e:#}"), "could not change starting at login");
            }
            tray.redraw.store(true, Ordering::Relaxed);
            tray.refresh.notify_one();
            return;
        }
        Command::TogglePause => Request::Pause { paused: None },
        Command::CopyRecent(id) => Request::CopyRecent { id },
        Command::Forget(device) => Request::Forget { device },
    };
    let tray = Arc::clone(tray);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = ask(&tray.endpoint, &request).await {
            tracing::warn!(error = %e, "the daemon refused a menu command");
        }
        tray.redraw.store(true, Ordering::Relaxed);
        tray.refresh.notify_one();
    });
}

/// Opens the pairing window, or brings it to the front.
fn open_pairing<R: Runtime>(app: &AppHandle<R>, tray: &Arc<Tray>) {
    let mut slot = tray.pairing.lock().unwrap_or_else(PoisonError::into_inner);
    let window_open = app.get_webview_window(pairing::WINDOW).is_some();
    if let Some(task) = slot.take() {
        if window_open {
            // Still pairing: just show it again.
            *slot = Some(task);
            drop(slot);
            if let Some(window) = app.get_webview_window(pairing::WINDOW) {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            return;
        }
        // Closed earlier: start over with a fresh code.
        task.abort();
    }
    let app = app.clone();
    let tray = Arc::clone(tray);
    let endpoint = tray.endpoint.clone();
    *slot = Some(tauri::async_runtime::spawn(async move {
        if let Err(e) = pairing::run(app, endpoint).await {
            tracing::warn!(error = %format!("{e:#}"), "pairing window failed");
        }
        tray.refresh.notify_one();
    }));
}

/// Keeps the menu and icon in step with the daemon, and starts the daemon when none runs.
async fn refresh_loop<R: Runtime>(app: AppHandle<R>, tray: Arc<Tray>) {
    let mut shown: Option<(Vec<Entry>, bool)> = None;
    let mut next_start = Instant::now();
    // The first time paste-get runs, nothing is paired yet: show the code right away.
    let mut offered_pairing = false;
    // Said once, until the daemon runs: the menu says it too, but not every desktop shows the
    // icon (a bar without a tray).
    let mut told_down = false;
    loop {
        let mut state = fetch(&tray.endpoint).await;
        if matches!(state, State::Down(_)) && !tray.is_hosting() && Instant::now() >= next_start {
            let _starting = tray.starting.lock().await;
            let started = if tray.is_hosting() {
                // Restarted by the settings meanwhile.
                Ok(())
            } else {
                tray.start_hosted().await
            };
            match started {
                Ok(()) => state = fetch(&tray.endpoint).await,
                Err(e) => {
                    tracing::warn!(error = %format!("{e:#}"), "could not start the daemon");
                    let reason = menu::start_error(&e);
                    next_start = Instant::now() + RESTART_EVERY;
                    if !told_down {
                        told_down = true;
                        DesktopNotifier::start().notice(menu::start_failed(&reason));
                    }
                    state = State::Down(reason);
                }
            }
        }
        if matches!(state, State::Running { .. }) {
            told_down = false;
        }

        if !offered_pairing && let State::Running { status, .. } = &state {
            offered_pairing = true;
            if status.devices.is_empty() {
                open_pairing(&app, &tray);
            } else if !tray.at_login {
                // Opened by the user, who sees no window: say where the icon is.
                DesktopNotifier::start().notice(startup::running_notice());
            }
        }

        let wanted = (menu::entries(&state, tray.start_at_login()), state.paused());
        let redraw = tray.redraw.swap(false, Ordering::Relaxed);
        if redraw || shown.as_ref() != Some(&wanted) {
            if let Err(e) = show(&app, &state, &wanted.0) {
                tracing::warn!(error = %e, "could not update the tray menu");
            }
            shown = Some(wanted);
        }

        let quit = tray.quit_signal();
        tokio::select! {
            _ = tokio::time::timeout(REFRESH_EVERY, tray.refresh.notified()) => {}
            () = asked_to_quit(quit) => {
                tracing::info!("asked to quit: a newer paste-get takes over");
                app.exit(0);
                return;
            }
        }
    }
}

/// Resolves once the hosted daemon is asked to quit; never without one.
async fn asked_to_quit(quit: Option<watch::Receiver<bool>>) {
    if let Some(mut quit) = quit
        && quit.wait_for(|quit| *quit).await.is_ok()
    {
        return;
    }
    std::future::pending().await
}

fn show<R: Runtime>(app: &AppHandle<R>, state: &State, entries: &[Entry]) -> Result<()> {
    let icon: TrayIcon<R> = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| anyhow!("the tray icon is gone"))?;
    icon.set_menu(Some(build_menu(app, entries)?))?;
    icon.set_icon(Some(Image::from_bytes(if state.paused() {
        ICON_PAUSED
    } else {
        ICON
    })?))?;
    icon.set_tooltip(Some(tooltip(state)))?;
    Ok(())
}

fn tooltip(state: &State) -> String {
    format!("paste-get: {}", state.summary())
}

/// What the daemon reports, or why it can't be reached.
async fn fetch(endpoint: &Endpoint) -> State {
    let status = match ask(endpoint, &Request::Status).await {
        Ok(Response::Status(status)) => status,
        Ok(_) => return State::Down("unexpected answer from the daemon".into()),
        Err(e) => return State::Down(menu::down_reason(&format!("{e:#}"))),
    };
    let recent = match ask(endpoint, &Request::Recent).await {
        Ok(Response::Recent { clips }) => clips,
        _ => Vec::new(),
    };
    State::Running { status, recent }
}

/// Sends a one-answer request.
async fn ask(endpoint: &Endpoint, request: &Request) -> Result<Response> {
    let mut answer = None;
    tokio::time::timeout(
        ASK_TIMEOUT,
        control::call(endpoint, request, |response| answer = Some(response)),
    )
    .await
    .context("the daemon did not answer in time")??;
    match answer {
        Some(Response::Error { message }) => Err(anyhow!(message)),
        Some(response) => Ok(response),
        None => Err(anyhow!("the daemon did not answer")),
    }
}

impl Tray {
    fn start_at_login(&self) -> Option<bool> {
        self.autostart.as_ref().map(Autostart::is_enabled)
    }

    fn toggle_start_at_login(&self) -> Result<()> {
        let Some(autostart) = &self.autostart else {
            return Ok(());
        };
        if autostart.is_enabled() {
            autostart.disable()
        } else {
            autostart.enable(&login_launch(self.profile.name())?)
        }
    }

    fn is_hosting(&self) -> bool {
        self.hosted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    fn quit_signal(&self) -> Option<watch::Receiver<bool>> {
        self.hosted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|hosted| hosted.quit.clone())
    }

    /// Runs the daemon in this process, on the system clipboard, like `pasteget daemon`.
    async fn start_hosted(&self) -> Result<()> {
        let settings = self.profile.settings()?;
        let mut options = DaemonOptions::listen_on(settings.port);
        options.notifier = Some(Arc::new(DesktopNotifier::start()));
        options.hotkeys = true;
        let daemon =
            Arc::new(Daemon::start(self.profile.clone(), SystemClipboard::new()?, options).await?);
        let server = control::serve(Arc::clone(&daemon), &self.endpoint).await?;
        tracing::info!(profile = self.profile.name(), "daemon started in the tray");
        *self.hosted.lock().unwrap_or_else(PoisonError::into_inner) = Some(Hosted {
            quit: daemon.quit_signal(),
            _daemon: daemon,
            server,
        });
        Ok(())
    }

    /// Stops the daemon this tray runs and starts it again, for settings read only at start.
    /// Phones reconnect by themselves within seconds.
    async fn restart_hosted(&self) -> Result<()> {
        let _starting = self.starting.lock().await;
        self.stop_hosted();
        // The old listener and control channel close as their tasks wind down.
        let mut last_error = None;
        for _ in 0..RESTART_ATTEMPTS {
            tokio::time::sleep(RESTART_PAUSE).await;
            match self.start_hosted().await {
                Ok(()) => return Ok(()),
                Err(e) => last_error = Some(e),
            }
        }
        Err(last_error
            .unwrap_or_else(|| anyhow!("the daemon did not start"))
            .context("paste-get could not restart"))
    }

    fn stop_hosted(&self) {
        let hosted = self
            .hosted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(hosted) = hosted {
            hosted.server.abort();
            control::cleanup(&self.endpoint);
            tracing::info!("daemon stopped");
        }
    }
}

/// Run in each window before its page: the language the tray speaks (`PASTEGET_LANG` or the
/// system's), so the page's texts and the ones it gets from here match (`dist/i18n.js`).
fn language_script() -> &'static str {
    if pasteget_desktop::i18n::french() {
        "window.pastegetLang = \"fr\";"
    } else {
        "window.pastegetLang = \"en\";"
    }
}

/// How the tray starts at login: this program, marked so it doesn't announce itself.
fn login_launch(profile: &str) -> Result<Launch> {
    let mut launch = Launch::this_program(profile)?;
    launch.args.push("--at-login".into());
    Ok(launch)
}

/// A window's own Close button. (A page can't close its window by itself.)
#[tauri::command]
fn close_window(window: tauri::WebviewWindow) {
    let _ = window.close();
}

/// paste-get's version, licence and addresses, for the windows that show them.
#[tauri::command]
fn app_info() -> about::Info {
    about::Info::of_this_build()
}

/// On Hyprland, which reads neither login entries nor hotkeys from apps, the lines to add to
/// `hyprland.conf`, for the settings window; `None` elsewhere.
#[tauri::command]
fn hyprland_lines(tray: tauri::State<'_, Arc<Tray>>) -> Result<Option<String>, String> {
    if !hotkeys::is_hyprland() {
        return Ok(None);
    }
    let text = |e: anyhow::Error| format!("{e:#}");
    let settings = tray.profile.settings().map_err(text)?;
    let launch = login_launch(tray.profile.name()).map_err(text)?;
    let lines = hotkeys::hyprland_lines(
        Some(&launch),
        tray.profile.name(),
        &settings.hotkeys.bindings(),
        &Runner::of_this_program(),
    );
    Ok(Some(lines.join("\n")))
}

/// Windows: what to do about a network marked Public, for the settings window; `None` when
/// there is none, and elsewhere.
#[tauri::command]
async fn network_advice() -> Result<Option<network::Advice>, String> {
    if !cfg!(windows) {
        return Ok(None);
    }
    // The tray runs the daemon: the firewall rule is for this program.
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(network::advice(&network::public_now().await, &program))
}

/// The settings window asks for the current values.
#[tauri::command]
fn load_settings(tray: tauri::State<'_, Arc<Tray>>) -> Result<settings::Form, String> {
    let current = tray.profile.settings().map_err(|e| format!("{e:#}"))?;
    Ok(settings::Form::of(&current, tray.start_at_login()))
}

/// The settings window saves: write `settings.toml`, apply what can change at once, and restart
/// the daemon this tray runs if the name or hotkeys changed. Returns what to tell the user.
#[tauri::command]
async fn save_settings(
    tray: tauri::State<'_, Arc<Tray>>,
    form: settings::Form,
) -> Result<String, String> {
    let tray = Arc::clone(&tray);
    let text = |e: anyhow::Error| format!("{e:#}");
    let current = tray.profile.settings().map_err(text)?;
    let new = form.apply(&current)?;
    tray.profile.save_settings(&new).map_err(text)?;
    if let Some(wanted) = form.start_at_login
        && tray.start_at_login().is_some_and(|now| now != wanted)
    {
        tray.toggle_start_at_login().map_err(text)?;
    }
    // A daemon that isn't running reads the file when it starts.
    let restart_needed = matches!(
        ask(&tray.endpoint, &Request::ReloadSettings).await,
        Ok(Response::SettingsReloaded {
            restart_needed: true
        })
    );
    let hosting = tray.is_hosting();
    if restart_needed && hosting {
        tray.restart_hosted().await.map_err(text)?;
    }
    tray.redraw.store(true, Ordering::Relaxed);
    tray.refresh.notify_one();
    Ok(settings::saved_message(restart_needed, hosting).into())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, entries: &[Entry]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in entries {
        menu.append(&*menu_item(app, entry)?)?;
    }
    Ok(menu)
}

fn menu_item<R: Runtime>(
    app: &AppHandle<R>,
    entry: &Entry,
) -> tauri::Result<Box<dyn IsMenuItem<R>>> {
    Ok(match entry {
        Entry::Info(label) => Box::new(MenuItem::new(app, label, false, None::<&str>)?),
        Entry::Separator => Box::new(PredefinedMenuItem::separator(app)?),
        Entry::Action { id, label } => Box::new(MenuItem::with_id(
            app,
            id.as_str(),
            label,
            true,
            None::<&str>,
        )?),
        Entry::Check { id, label, checked } => Box::new(CheckMenuItem::with_id(
            app,
            id.as_str(),
            label,
            true,
            *checked,
            None::<&str>,
        )?),
        Entry::Submenu { label, entries } => {
            let submenu = Submenu::new(app, label, true)?;
            for entry in entries {
                submenu.append(&*menu_item(app, entry)?)?;
            }
            Box::new(submenu)
        }
    })
}
