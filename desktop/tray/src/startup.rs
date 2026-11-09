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
use pasteget_desktop::control::{self, Endpoint, Request, Response, StatusReply};
use pasteget_desktop::i18n::tr;
use pasteget_desktop::notify::{DesktopNotifier, Notifier};
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Past this size, the log starts over (the previous one is kept as `.old.log`).
const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// How long a running paste-get has to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an older one has to quit: long enough for a tray that only showed a `pasteget
/// daemon` to start a daemon of its own, and quit with it.
const QUIT_TIMEOUT: Duration = Duration::from_secs(10);
/// The programs an older paste-get may run as, ended if it doesn't quit when asked: the tray,
/// and the command line's `pasteget daemon`.
const TRAY: &str = "pasteget-tray";
const CLI: &str = "pasteget";

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
/// version or newer, or doesn't answer, is left alone, as before, and so is everything that
/// serves another profile.
///
/// One that doesn't quit in time is ended by the process ID it reported. Versions from before
/// [`Request::Quit`] know neither: their processes that serve this profile, the tray and
/// `pasteget daemon` of this user, found by their command line, are ended instead (Windows and
/// Linux).
pub async fn replace_older(profile: &Profile, endpoint: &Endpoint) {
    replace_older_than(profile, endpoint, env!("CARGO_PKG_VERSION"), |pid| {
        end_older(profile.name(), pid);
    })
    .await;
}

/// [`replace_older`], for version `ours`, ending processes with `end_processes`, given the
/// process ID the older one reported.
async fn replace_older_than(
    profile: &Profile,
    endpoint: &Endpoint,
    ours: &str,
    end_processes: impl FnOnce(Option<u32>),
) {
    let Some(status) = older(endpoint, ours, ANSWER_TIMEOUT).await else {
        return;
    };
    tracing::info!(
        running = status.app_version.as_deref().unwrap_or("unknown"),
        ours,
        "an older paste-get runs for this profile: asking it to quit"
    );
    if matches!(
        answer(endpoint, &Request::Quit, ANSWER_TIMEOUT).await,
        Some(Response::Quitting)
    ) && gone(profile, endpoint, ours).await
    {
        tracing::info!("the older paste-get quit");
        return;
    }
    // Too old to know Quit, or it didn't go.
    end_processes(status.pid);
    if gone(profile, endpoint, ours).await {
        tracing::info!("the older paste-get was ended");
    } else {
        tracing::warn!("the older paste-get is still running");
    }
}

/// The status of the paste-get running for the profile, if it's older than `ours`.
async fn older(endpoint: &Endpoint, ours: &str, wait: Duration) -> Option<StatusReply> {
    match answer(endpoint, &Request::Status, wait).await {
        Some(Response::Status(status)) if is_older(status.app_version.as_deref(), ours) => {
            Some(status)
        }
        _ => None,
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

/// The single answer to `request`, if the daemon gives one within `wait`.
async fn answer(endpoint: &Endpoint, request: &Request, wait: Duration) -> Option<Response> {
    let mut answer = None;
    let call = control::call(endpoint, request, |response| answer = Some(response));
    // An error after the answer (the daemon ended at once) still leaves the answer.
    let _ = tokio::time::timeout(wait, call).await;
    answer
}

/// Waits for nothing to run for the profile any more: no daemon answers, and no tray holds it.
///
/// An older daemon that answers meanwhile is asked to quit too: a tray that only showed a
/// daemon (`pasteget daemon` in a terminal) doesn't quit with it, but starts one of its own, and
/// quits with that one.
async fn gone(profile: &Profile, endpoint: &Endpoint, ours: &str) -> bool {
    let deadline = Instant::now() + QUIT_TIMEOUT;
    loop {
        let running = control::is_running(endpoint).await;
        if !running && matches!(lock_profile(profile), Ok(Some(_))) {
            return true;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        if running && older(endpoint, ours, left).await.is_some() {
            let left = deadline.saturating_duration_since(Instant::now());
            let _ = answer(endpoint, &Request::Quit, left).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Ends the older paste-get: the process it reported (`pid`), or for versions from before that,
/// the processes of this user that serve `profile`, found by their command line. Never this
/// process.
fn end_older(profile: &str, pid: Option<u32>) {
    let ours = std::process::id();
    let ending: Vec<u32> = match pid {
        Some(pid) => vec![pid],
        None => serving(profile),
    }
    .into_iter()
    .filter(|pid| *pid != ours)
    .collect();
    if ending.is_empty() {
        tracing::warn!("found no process of the older paste-get to end");
        return;
    }
    tracing::info!(?ending, "ending the older paste-get's processes");
    end(&ending);
}

/// This user's processes that serve `profile`, from PowerShell's list of the paste-get processes
/// (`Win32_Process`, which knows their command lines).
#[cfg(windows)]
fn serving(profile: &str) -> Vec<u32> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // One line per process: its ID, name and command line, tab-separated.
    const LIST: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
        Get-CimInstance Win32_Process \
        -Filter \"Name = 'pasteget.exe' OR Name = 'pasteget-tray.exe'\" | \
        Where-Object { (Invoke-CimMethod -InputObject $_ -MethodName GetOwner).User -eq $env:USERNAME } | \
        ForEach-Object { \"$($_.ProcessId)`t$($_.Name)`t$($_.CommandLine)\" }\n";

    let powershell = system32(r"WindowsPowerShell\v1.0\powershell.exe");
    let child = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!(error = %e, "could not run PowerShell to list processes");
            return Vec::new();
        }
    };
    // Closed once written: PowerShell runs what it read, then exits.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(LIST.as_bytes());
    }
    match child.wait_with_output() {
        Ok(output) => serving_in_list(&String::from_utf8_lossy(&output.stdout), profile),
        Err(e) => {
            tracing::warn!(error = %e, "could not list processes");
            Vec::new()
        }
    }
}

/// Ends `pids` with `taskkill`.
#[cfg(windows)]
fn end(pids: &[u32]) {
    use std::os::windows::process::CommandExt;

    let ended = std::process::Command::new(system32("taskkill.exe"))
        .args(taskkill_args(pids))
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if let Err(e) = ended {
        tracing::warn!(error = %e, "could not run taskkill");
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `program` in `%SystemRoot%\System32`.
#[cfg(windows)]
fn system32(program: &str) -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(root) => PathBuf::from(root).join("System32").join(program),
        None => PathBuf::from(program),
    }
}

/// This user's processes that serve `profile`, from `/proc`.
#[cfg(target_os = "linux")]
fn serving(profile: &str) -> Vec<u32> {
    use std::os::unix::fs::MetadataExt;

    let (Ok(me), Ok(all)) = (fs::metadata("/proc/self"), fs::read_dir("/proc")) else {
        return Vec::new();
    };
    let processes = all.flatten().filter_map(|entry| {
        let pid = entry.file_name().to_str()?.parse().ok()?;
        let uid = entry.metadata().ok()?.uid();
        let name = fs::read_to_string(entry.path().join("comm")).ok()?;
        let cmdline = fs::read(entry.path().join("cmdline")).ok()?;
        Some(Process {
            pid,
            uid,
            name: name.trim_end().to_owned(),
            args: cmdline_args(&cmdline),
        })
    });
    to_end(processes, me.uid(), profile)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn serving(_profile: &str) -> Vec<u32> {
    tracing::warn!("finding an older paste-get isn't supported here: quit it yourself");
    Vec::new()
}

/// Ends `pids` with `kill`.
#[cfg(unix)]
fn end(pids: &[u32]) {
    let ended = std::process::Command::new("kill")
        .arg("-TERM")
        .args(pids.iter().map(u32::to_string))
        .status();
    if let Err(e) = ended {
        tracing::warn!(error = %e, "could not run kill");
    }
}

#[cfg(not(any(unix, windows)))]
fn serving(_profile: &str) -> Vec<u32> {
    Vec::new()
}

#[cfg(not(any(unix, windows)))]
fn end(_pids: &[u32]) {
    tracing::warn!("ending an older paste-get isn't supported here: quit it yourself");
}

/// Whether a process running `program` with `args` (those after the program) serves `profile`:
/// the tray, or `pasteget daemon`, with `--profile <profile>`, or no `--profile` for the default
/// one. Other `pasteget` commands, such as a `join` under way, are left alone.
fn serves(program: &str, args: &[String], profile: &str) -> bool {
    let program = program.to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program);
    let mut given = None;
    let mut command = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--profile" {
            given = args.next().map(String::as_str);
        } else if let Some(value) = arg.strip_prefix("--profile=") {
            given = Some(value);
        } else if command.is_none() && !arg.starts_with('-') {
            command = Some(arg.as_str());
        }
    }
    if given.unwrap_or(DEFAULT_PROFILE) != profile {
        return false;
    }
    match program {
        TRAY => true,
        CLI => command == Some("daemon"),
        _ => false,
    }
}

/// A running process, as `/proc` lists it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug)]
struct Process {
    pid: u32,
    uid: u32,
    /// What it runs, from `comm`.
    name: String,
    /// Its arguments after the program, from `cmdline`.
    args: Vec<String>,
}

/// The processes of `uid` that serve `profile`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn to_end(processes: impl Iterator<Item = Process>, uid: u32, profile: &str) -> Vec<u32> {
    processes
        .filter(|p| p.uid == uid && serves(&p.name, &p.args, profile))
        .map(|p| p.pid)
        .collect()
}

/// The arguments after the program in `/proc/<pid>/cmdline`: NUL-terminated strings.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn cmdline_args(cmdline: &[u8]) -> Vec<String> {
    let cmdline = cmdline.strip_suffix(&[0]).unwrap_or(cmdline);
    cmdline
        .split(|b| *b == 0)
        .skip(1)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}

/// The processes in `list`, lines of a process ID, name and command line separated by tabs,
/// that serve `profile`.
#[cfg_attr(not(windows), allow(dead_code))]
fn serving_in_list(list: &str, profile: &str) -> Vec<u32> {
    list.lines()
        .filter_map(|line| {
            let mut fields = line.trim_end_matches('\r').splitn(3, '\t');
            let pid = fields.next()?.trim().parse().ok()?;
            let name = fields.next()?;
            let args = windows_args(fields.next().unwrap_or_default());
            serves(name, args.get(1..).unwrap_or_default(), profile).then_some(pid)
        })
        .collect()
}

/// Splits a Windows command line into the program and its arguments, as Rust programs read
/// them: quotes group, and backslashes escape a quote only when they come right before one.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_args(line: &str) -> Vec<String> {
    let mut chars = line.chars().peekable();
    // The program: up to the closing quote, or to the first space. No escapes.
    let mut program = String::new();
    if chars.next_if_eq(&'"').is_some() {
        program.extend(chars.by_ref().take_while(|c| *c != '"'));
    } else {
        while let Some(c) = chars.next_if(|c| !matches!(c, ' ' | '\t')) {
            program.push(c);
        }
    }
    let mut args = vec![program];
    loop {
        while chars.next_if(|c| matches!(c, ' ' | '\t')).is_some() {}
        if chars.peek().is_none() {
            return args;
        }
        let mut arg = String::new();
        let mut quoted = false;
        let mut backslashes = 0;
        while let Some(c) = chars.next_if(|c| quoted || !matches!(c, ' ' | '\t')) {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    arg.extend(std::iter::repeat_n('\\', backslashes / 2));
                    if backslashes % 2 == 1 {
                        arg.push('"');
                    } else {
                        quoted = !quoted;
                    }
                    backslashes = 0;
                }
                c => {
                    arg.extend(std::iter::repeat_n('\\', backslashes));
                    backslashes = 0;
                    arg.push(c);
                }
            }
        }
        arg.extend(std::iter::repeat_n('\\', backslashes));
        args.push(arg);
    }
}

/// `taskkill` arguments that end `pids`.
#[cfg_attr(not(windows), allow(dead_code))]
fn taskkill_args(pids: &[u32]) -> Vec<String> {
    let mut args = vec!["/F".to_owned()];
    for pid in pids {
        args.push("/PID".to_owned());
        args.push(pid.to_string());
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
        let home = tempfile::tempdir().unwrap();
        let profile = Profile::open_in(home.path(), "pc").unwrap();
        Running {
            daemon: serve_daemon(&profile).await,
            endpoint: Endpoint::for_profile(&profile),
            profile,
            _home: home,
        }
    }

    /// Starts a daemon for `profile` and serves its control channel until it's asked to quit.
    async fn serve_daemon(profile: &Profile) -> std::sync::Arc<pasteget_desktop::daemon::Daemon> {
        use pasteget_desktop::clipboard::MemoryClipboard;
        use pasteget_desktop::daemon::{Daemon, DaemonOptions};
        use std::net::{Ipv4Addr, SocketAddr};

        let mut options = DaemonOptions::listen_on(0);
        options.listen = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        options.announce = false;
        let daemon = std::sync::Arc::new(
            Daemon::start(profile.clone(), MemoryClipboard::default(), options)
                .await
                .unwrap(),
        );
        let endpoint = Endpoint::for_profile(profile);
        let server = control::serve(std::sync::Arc::clone(&daemon), &endpoint)
            .await
            .unwrap();
        let mut quit = daemon.quit_signal();
        tokio::spawn(async move {
            let _ = quit.wait_for(|quit| *quit).await;
            server.abort();
            control::cleanup(&endpoint);
        });
        daemon
    }

    #[test]
    fn an_older_paste_get_is_asked_to_quit() {
        tauri::async_runtime::block_on(async {
            let older = running_daemon().await;

            replace_older_than(&older.profile, &older.endpoint, "99.0.0", |_| {
                panic!("it quit when asked: nothing to end")
            })
            .await;

            assert!(*older.daemon.quit_signal().borrow());
            assert!(!control::is_running(&older.endpoint).await);
        });
    }

    /// A tray that only showed a `pasteget daemon` doesn't quit with it: it starts a daemon of its
    /// own, which is asked to quit too, and the tray quits with that one.
    #[test]
    fn a_daemon_started_again_meanwhile_is_asked_to_quit_too() {
        tauri::async_runtime::block_on(async {
            let older = running_daemon().await;
            let tray_lock = lock_profile(&older.profile).unwrap().unwrap();
            let (profile, endpoint) = (older.profile.clone(), older.endpoint.clone());
            let mut first = older.daemon.quit_signal();
            let tray = tokio::spawn(async move {
                let _ = first.wait_for(|quit| *quit).await;
                while control::is_running(&endpoint).await {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                let own = serve_daemon(&profile).await;
                let _ = own.quit_signal().wait_for(|quit| *quit).await;
                drop(tray_lock);
            });

            replace_older_than(&older.profile, &older.endpoint, "99.0.0", |_| {
                panic!("both quit when asked: nothing to end")
            })
            .await;

            assert!(tray.is_finished());
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
                |_| panic!("nothing to end"),
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
            replace_older_than(&profile, &endpoint, "0.1.0", |pid| {
                // It reports no process ID: its processes are found by their command line.
                assert_eq!(pid, None);
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

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn only_the_tray_and_daemon_of_this_profile_serve_it() {
        // No `--profile`: the default one.
        assert!(serves("pasteget-tray", &[], "default"));
        assert!(serves("pasteget-tray", &args(&["--at-login"]), "default"));
        assert!(serves("pasteget", &args(&["daemon"]), "default"));
        assert!(!serves("pasteget-tray", &[], "work"));
        // Both spellings, before or after the command.
        assert!(serves(
            "pasteget-tray",
            &args(&["--profile", "work"]),
            "work"
        ));
        assert!(serves("pasteget-tray", &args(&["--profile=work"]), "work"));
        assert!(serves(
            "pasteget",
            &args(&["--profile", "work", "daemon"]),
            "work"
        ));
        assert!(serves(
            "pasteget",
            &args(&["daemon", "--port", "47291", "--profile", "work"]),
            "work"
        ));
        assert!(!serves(
            "pasteget-tray",
            &args(&["--profile", "work"]),
            "default"
        ));
        assert!(!serves(
            "pasteget",
            &args(&["--profile", "test", "daemon"]),
            "default"
        ));
        // Other commands are left alone, a pairing under way included.
        assert!(!serves(
            "pasteget",
            &args(&["join", "pasteget://pair?x"]),
            "default"
        ));
        assert!(!serves("pasteget", &args(&["status"]), "default"));
        assert!(!serves("pasteget", &[], "default"));
        // Windows names.
        assert!(serves("pasteget-tray.exe", &[], "default"));
        assert!(serves("PasteGet.EXE", &args(&["daemon"]), "default"));
        // Not exactly the name.
        assert!(!serves("pasteget-tray-old", &[], "default"));
        assert!(!serves("bash", &args(&["daemon"]), "default"));
    }

    #[test]
    fn only_this_users_processes_of_this_profile_are_ended() {
        let process = |pid, uid, name: &str, arguments: &[&str]| Process {
            pid,
            uid,
            name: name.into(),
            args: args(arguments),
        };
        let processes = vec![
            process(
                10,
                1000,
                "pasteget-tray",
                &["--at-login", "--profile", "default"],
            ),
            process(11, 1000, "pasteget", &["daemon"]),
            // Another profile's.
            process(12, 1000, "pasteget-tray", &["--profile", "work"]),
            process(13, 1000, "pasteget", &["--profile", "test", "daemon"]),
            // Pairing.
            process(14, 1000, "pasteget", &["join", "pasteget://pair?x"]),
            // Another user's.
            process(15, 1001, "pasteget-tray", &[]),
            process(16, 1000, "bash", &[]),
        ];

        assert_eq!(to_end(processes.into_iter(), 1000, "default"), [10, 11]);
    }

    #[test]
    fn proc_cmdline_gives_the_arguments_after_the_program() {
        assert_eq!(
            cmdline_args(b"/usr/bin/pasteget\0--profile\0work\0daemon\0"),
            ["--profile", "work", "daemon"]
        );
        assert_eq!(cmdline_args(b"pasteget-tray\0"), Vec::<String>::new());
        assert_eq!(cmdline_args(b""), Vec::<String>::new());
    }

    #[test]
    fn windows_command_lines_are_split_like_programs_read_them() {
        assert_eq!(
            windows_args(
                r#""C:\Program Files\paste-get\pasteget-tray.exe" --at-login --profile work"#
            ),
            [
                r"C:\Program Files\paste-get\pasteget-tray.exe",
                "--at-login",
                "--profile",
                "work"
            ]
        );
        assert_eq!(
            windows_args("pasteget.exe  daemon\t--port 47291"),
            ["pasteget.exe", "daemon", "--port", "47291"]
        );
        // Quotes group; backslashes escape a quote only right before one.
        assert_eq!(
            windows_args(r#"p "a b" c\d "e\"f" "g\\" h"#),
            ["p", "a b", r"c\d", r#"e"f"#, r"g\", "h"]
        );
        assert_eq!(windows_args(""), [""]);
    }

    #[test]
    fn powershells_list_gives_the_processes_of_this_profile() {
        let list = "4100\tpasteget-tray.exe\t\"C:\\pg\\pasteget-tray.exe\" --at-login --profile default\r\n\
            4200\tpasteget.exe\tpasteget  daemon\r\n\
            4300\tpasteget-tray.exe\t\"C:\\pg\\pasteget-tray.exe\" --profile work\r\n\
            4400\tpasteget.exe\tpasteget join pasteget://pair?x\r\n\
            not a line\r\n";

        assert_eq!(serving_in_list(list, "default"), [4100, 4200]);
        assert_eq!(serving_in_list(list, "work"), [4300]);
    }

    #[test]
    fn taskkill_ends_the_processes_given() {
        assert_eq!(
            taskkill_args(&[4100, 4200]),
            ["/F", "/PID", "4100", "/PID", "4200"]
        );
    }
}
