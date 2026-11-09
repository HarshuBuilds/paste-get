// SPDX-License-Identifier: GPL-3.0-or-later

//! What makes the tray visible when something goes wrong, or when it seems to do nothing: a log
//! file (a Windows app has no console to print to), an error box when it can't start, one tray
//! per profile, and a notice saying where its icon is. And after an update, the previous version
//! still running makes way for the new one.

use std::cmp::Ordering;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use pasteget_desktop::config::{DEFAULT_PROFILE, Profile};
use pasteget_desktop::control::{self, Endpoint, Request, Response};
use pasteget_desktop::i18n::tr;
use pasteget_desktop::notify::{DesktopNotifier, Notifier};
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Past this size, the log starts over (the previous one is kept as `.old.log`).
const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// How long a running paste-get has to answer, and an older one to quit.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
const QUIT_TIMEOUT: Duration = Duration::from_secs(5);
/// The programs an older paste-get may run as, ended if it doesn't quit when asked.
const PROGRAMS: [&str; 2] = ["pasteget", "pasteget-tray"];

/// Where the tray's icon is, for the notices.
fn icon_place() -> &'static str {
    if cfg!(windows) {
        tr(
            "in the notification area, under ^ in the taskbar",
            "dans la zone de notification, sous ^ dans la barre des tâches",
        )
    } else {
        tr("in the system tray", "dans la zone de notification")
    }
}

/// Logs to stderr and to a file, which it returns. Never clipboard contents: nothing logs them.
pub fn init_logs(profile: &str) -> Option<PathBuf> {
    let filter =
        || tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    match open_log(profile) {
        Some((path, file)) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter())
                .with_ansi(false)
                .with_writer(std::io::stderr.and(Mutex::new(file)))
                .init();
            Some(path)
        }
        None => {
            tracing_subscriber::fmt()
                .with_env_filter(filter())
                .with_writer(std::io::stderr)
                .init();
            None
        }
    }
}

/// `%LOCALAPPDATA%\paste-get\logs\pasteget-tray.log` on Windows,
/// `~/.local/share/paste-get/logs/pasteget-tray.log` on Linux.
fn open_log(profile: &str) -> Option<(PathBuf, File)> {
    let dir = dirs::data_local_dir()?.join("paste-get").join("logs");
    fs::create_dir_all(&dir).ok()?;
    let name = if profile == DEFAULT_PROFILE {
        "pasteget-tray".to_owned()
    } else {
        format!("pasteget-tray-{profile}")
    };
    let path = dir.join(format!("{name}.log"));
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(&path, dir.join(format!("{name}.old.log")));
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some((path, file))
}

/// Held while this tray runs. `None` if another tray already runs for the profile.
pub fn lock_profile(profile: &Profile) -> Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(profile.dir().join("tray.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

/// After an update, the previous version may still run for this profile (the tray, or
/// `pasteget daemon`): it's asked to quit, so that this one takes its place. One that is the same
/// version or newer, or doesn't answer, is left alone, as before.
///
/// Versions from before [`Request::Quit`] don't know it: their processes, `pasteget` and
/// `pasteget-tray` of this user, are ended instead (Windows and Linux).
pub async fn replace_older(profile: &Profile, endpoint: &Endpoint) {
    replace_older_than(
        profile,
        endpoint,
        env!("CARGO_PKG_VERSION"),
        end_other_processes,
    )
    .await;
}

/// [`replace_older`], for version `ours`, ending processes with `end_processes`.
async fn replace_older_than(
    profile: &Profile,
    endpoint: &Endpoint,
    ours: &str,
    end_processes: impl FnOnce(),
) {
    let Some(Response::Status(status)) = answer(endpoint, &Request::Status).await else {
        return;
    };
    if !is_older(status.app_version.as_deref(), ours) {
        return;
    }
    tracing::info!(
        running = status.app_version.as_deref().unwrap_or("unknown"),
        ours,
        "an older paste-get runs for this profile: asking it to quit"
    );
    if matches!(
        answer(endpoint, &Request::Quit).await,
        Some(Response::Quitting)
    ) && gone(profile, endpoint).await
    {
        tracing::info!("the older paste-get quit");
        return;
    }
    // Too old to know Quit, or it didn't go.
    end_processes();
    if gone(profile, endpoint).await {
        tracing::info!("the older paste-get was ended");
    } else {
        tracing::warn!("the older paste-get is still running");
    }
}

/// Whether `running`, the version a running paste-get reported, is older than `ours`. No version
/// at all is a version from before they were reported. One that doesn't read as a version is
/// left alone.
pub fn is_older(running: Option<&str>, ours: &str) -> bool {
    let Some(running) = running else {
        return true;
    };
    match (
        semver::Version::parse(running),
        semver::Version::parse(ours),
    ) {
        (Ok(running), Ok(ours)) => running.cmp_precedence(&ours) == Ordering::Less,
        _ => false,
    }
}

/// The single answer to `request`, if the daemon gives one in time.
async fn answer(endpoint: &Endpoint, request: &Request) -> Option<Response> {
    let mut answer = None;
    let call = control::call(endpoint, request, |response| answer = Some(response));
    // An error after the answer (the daemon ended at once) still leaves the answer.
    let _ = tokio::time::timeout(ANSWER_TIMEOUT, call).await;
    answer
}

/// Waits for nothing to run for the profile any more: no daemon answers, and no tray holds it.
async fn gone(profile: &Profile, endpoint: &Endpoint) -> bool {
    let deadline = Instant::now() + QUIT_TIMEOUT;
    loop {
        if !control::is_running(endpoint).await && matches!(lock_profile(profile), Ok(Some(_))) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Ends the `pasteget.exe` and `pasteget-tray.exe` processes of this user, except this one.
#[cfg(windows)]
fn end_other_processes() {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Without the filter it would try other users' processes too.
    let Some(user) = std::env::var("USERNAME").ok().filter(|u| !u.is_empty()) else {
        return;
    };
    let taskkill = std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from("taskkill.exe"),
        |root| PathBuf::from(root).join(r"System32\taskkill.exe"),
    );
    tracing::info!("ending the older paste-get's processes");
    let ended = std::process::Command::new(taskkill)
        .args(taskkill_args(&user, std::process::id()))
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if let Err(e) = ended {
        tracing::warn!(error = %e, "could not run taskkill");
    }
}

/// Ends the `pasteget` and `pasteget-tray` processes of this user, by their exact name, except
/// this one.
#[cfg(target_os = "linux")]
fn end_other_processes() {
    use std::os::unix::fs::MetadataExt;

    let (Ok(me), Ok(all)) = (fs::metadata("/proc/self"), fs::read_dir("/proc")) else {
        return;
    };
    let processes = all.flatten().filter_map(|entry| {
        let pid = entry.file_name().to_str()?.parse().ok()?;
        let uid = entry.metadata().ok()?.uid();
        let name = fs::read_to_string(entry.path().join("comm")).ok()?;
        Some(Process {
            pid,
            uid,
            name: name.trim_end().to_owned(),
        })
    });
    let ending = to_end(processes, std::process::id(), me.uid());
    if ending.is_empty() {
        return;
    }
    tracing::info!(?ending, "ending the older paste-get's processes");
    let ended = std::process::Command::new("kill")
        .arg("-TERM")
        .args(ending.iter().map(u32::to_string))
        .status();
    if let Err(e) = ended {
        tracing::warn!(error = %e, "could not run kill");
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn end_other_processes() {
    tracing::warn!("ending an older paste-get isn't supported here: quit it yourself");
}

/// A running process, as `/proc` lists it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug)]
struct Process {
    pid: u32,
    uid: u32,
    name: String,
}

/// The processes of `uid` named like a paste-get program, except `ours`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn to_end(processes: impl Iterator<Item = Process>, ours: u32, uid: u32) -> Vec<u32> {
    processes
        .filter(|p| p.uid == uid && p.pid != ours && PROGRAMS.contains(&p.name.as_str()))
        .map(|p| p.pid)
        .collect()
}

/// `taskkill` arguments that end this user's paste-get programs, except `ours`.
#[cfg_attr(not(windows), allow(dead_code))]
fn taskkill_args(user: &str, ours: u32) -> Vec<String> {
    let mut args = vec![
        "/F".to_owned(),
        "/FI".to_owned(),
        format!("USERNAME eq {user}"),
        "/FI".to_owned(),
        format!("PID ne {ours}"),
    ];
    for program in PROGRAMS {
        args.push("/IM".to_owned());
        args.push(format!("{program}.exe"));
    }
    args
}

pub fn already_running_notice() -> String {
    format!(
        "{} {}",
        tr(
            "paste-get is already running: its icon is",
            "paste-get tourne déjà : son icône est"
        ),
        icon_place()
    )
}

pub fn running_notice() -> String {
    format!(
        "{} {}",
        tr(
            "paste-get is running: its icon is",
            "paste-get tourne : son icône est"
        ),
        icon_place()
    )
}

/// For the second tray, which has no daemon of its own: shows `summary`, then waits for the
/// notification to be handed over before the process ends.
pub fn notify_once(summary: String) {
    tauri::async_runtime::block_on(async move {
        let notifier = DesktopNotifier::start();
        notifier.notice(summary);
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
}

/// The tray couldn't start: say so where the user sees it, not only in the log.
pub fn show_fatal(error: &str, log: Option<&Path>) {
    let mut text = format!(
        "{}\n\n{error}",
        tr(
            "paste-get could not start.",
            "paste-get n'a pas pu démarrer."
        )
    );
    if let Some(log) = log {
        text.push_str(&format!(
            "\n\n{} {}",
            tr("Details:", "Détails :"),
            log.display()
        ));
    }
    #[cfg(windows)]
    message_box(&text);
    #[cfg(not(windows))]
    eprintln!("{text}");
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn message_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("paste-get"));
    // SAFETY: both strings are NUL-terminated and outlive the call; no owner window.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_tray_for_a_profile_is_turned_away() {
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "pc").unwrap();
        let other = Profile::open_in(home.path(), "laptop").unwrap();

        let first = lock_profile(&profile).unwrap();
        assert!(first.is_some());
        assert!(lock_profile(&profile).unwrap().is_none());
        // Other profiles have trays of their own.
        assert!(lock_profile(&other).unwrap().is_some());

        drop(first);
        assert!(lock_profile(&profile).unwrap().is_some());
    }

    struct Running {
        daemon: std::sync::Arc<pasteget_desktop::daemon::Daemon>,
        profile: Profile,
        endpoint: Endpoint,
        _home: tempfile::TempDir,
    }

    /// A daemon serving its control channel, run like the tray runs it: it stops when asked to
    /// quit.
    async fn running_daemon() -> Running {
        use pasteget_desktop::clipboard::MemoryClipboard;
        use pasteget_desktop::daemon::{Daemon, DaemonOptions};
        use std::net::{Ipv4Addr, SocketAddr};

        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "pc").unwrap();
        let mut options = DaemonOptions::listen_on(0);
        options.listen = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        options.announce = false;
        let daemon = std::sync::Arc::new(
            Daemon::start(profile.clone(), MemoryClipboard::default(), options)
                .await
                .unwrap(),
        );
        let endpoint = Endpoint::for_profile(&profile);
        let server = control::serve(std::sync::Arc::clone(&daemon), &endpoint)
            .await
            .unwrap();
        let mut quit = daemon.quit_signal();
        let stopping = endpoint.clone();
        tokio::spawn(async move {
            let _ = quit.wait_for(|quit| *quit).await;
            server.abort();
            control::cleanup(&stopping);
        });
        Running {
            daemon,
            profile,
            endpoint,
            _home: home,
        }
    }

    #[test]
    fn an_older_paste_get_is_asked_to_quit() {
        tauri::async_runtime::block_on(async {
            let older = running_daemon().await;

            replace_older_than(&older.profile, &older.endpoint, "99.0.0", || {
                panic!("it quit when asked: nothing to end")
            })
            .await;

            assert!(*older.daemon.quit_signal().borrow());
            assert!(!control::is_running(&older.endpoint).await);
        });
    }

    #[test]
    fn the_same_or_a_newer_paste_get_is_left_alone() {
        tauri::async_runtime::block_on(async {
            let running = running_daemon().await;

            replace_older_than(
                &running.profile,
                &running.endpoint,
                env!("CARGO_PKG_VERSION"),
                || panic!("nothing to end"),
            )
            .await;

            assert!(!*running.daemon.quit_signal().borrow());
            assert!(control::is_running(&running.endpoint).await);
        });
    }

    /// A version from before Quit: its status has no version, and it doesn't know Quit.
    #[cfg(unix)]
    #[test]
    fn a_paste_get_too_old_to_quit_is_ended() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        tauri::async_runtime::block_on(async {
            let home = tempfile::tempdir().unwrap();
            let profile = Profile::open_in(home.path(), "pc").unwrap();
            let endpoint = Endpoint::for_profile(&profile);
            let socket = profile.dir().join("control.sock");
            let listener = tokio::net::UnixListener::bind(&socket).unwrap();
            let old = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let (read, mut write) = stream.into_split();
                    let mut line = String::new();
                    let _ = BufReader::new(read).read_line(&mut line).await;
                    let answer = if line.contains("\"status\"") {
                        r#"{"status":{"device_id":"ID","device_name":"PC","listening_on":"0.0.0.0:47290","pairing_open":false,"paused":false,"devices":[]}}"#
                    } else {
                        r#"{"error":{"message":"bad request: unknown variant `quit`"}}"#
                    };
                    let _ = write.write_all(format!("{answer}\n").as_bytes()).await;
                }
            });
            let ended = std::sync::atomic::AtomicBool::new(false);

            let started = Instant::now();
            replace_older_than(&profile, &endpoint, "0.1.0", || {
                ended.store(true, std::sync::atomic::Ordering::Relaxed);
                old.abort();
                let _ = fs::remove_file(&socket);
            })
            .await;

            assert!(ended.load(std::sync::atomic::Ordering::Relaxed));
            // Ended at once, without waiting for it to quit.
            assert!(started.elapsed() < QUIT_TIMEOUT);
            assert!(!control::is_running(&endpoint).await);
        });
    }

    #[test]
    fn only_an_older_version_is_replaced() {
        // Before versions were reported.
        assert!(is_older(None, "0.1.0"));
        assert!(is_older(Some("0.1.0"), "0.2.0"));
        assert!(is_older(Some("0.9.9"), "0.10.0"));
        assert!(is_older(Some("0.2.0-beta.2"), "0.2.0"));
        assert!(is_older(Some("0.2.0-beta.2"), "0.2.0-beta.10"));
        assert!(!is_older(Some("0.2.0"), "0.2.0"));
        assert!(!is_older(Some("0.2.0+build.5"), "0.2.0"));
        assert!(!is_older(Some("0.3.0"), "0.2.0"));
        assert!(!is_older(Some("0.2.0"), "0.2.0-beta.1"));
        // Unreadable: left alone.
        assert!(!is_older(Some("dev"), "0.2.0"));
        assert!(!is_older(Some("0.1.0"), "dev"));
    }

    #[test]
    fn only_this_users_other_paste_get_processes_are_ended() {
        let process = |pid, uid, name: &str| Process {
            pid,
            uid,
            name: name.into(),
        };
        let processes = vec![
            process(10, 1000, "pasteget-tray"),
            process(11, 1000, "pasteget"),
            // This one.
            process(12, 1000, "pasteget-tray"),
            // Another user's.
            process(13, 1001, "pasteget-tray"),
            // Not exactly the name.
            process(14, 1000, "pasteget-tray-old"),
            process(15, 1000, "bash"),
        ];

        assert_eq!(to_end(processes.into_iter(), 12, 1000), [10, 11]);
    }

    #[test]
    fn taskkill_ends_this_users_programs_except_this_one() {
        assert_eq!(
            taskkill_args("Léa", 4242),
            [
                "/F",
                "/FI",
                "USERNAME eq Léa",
                "/FI",
                "PID ne 4242",
                "/IM",
                "pasteget.exe",
                "/IM",
                "pasteget-tray.exe"
            ]
        );
    }
}
