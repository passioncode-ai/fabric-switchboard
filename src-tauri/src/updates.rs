//! Automatic updates (SB-55): an installed Switchboard checks the latest GitHub release at start
//! and every six hours, downloads a newer version in the background, verifies it against the
//! minisign public key in `tauri.conf.json` (`plugins.updater`) and against the version that
//! signature names, and installs it without the person doing anything. On by default; one switch
//! in About → "Running in the background" turns it off (`<data>/auto-update`, absent = on).
//!
//! - **macOS:** installed into the app bundle as soon as the download is verified. The running
//!   app keeps its old code; the next start, or "Restart to update", runs the new version.
//!   Nothing is installed on the quit path, where the hard-exit deadline (LC-01) could cut a
//!   bundle swap in half. A bundle the person cannot replace without an administrator password
//!   is not installed in the background: "Restart to update" asks for the password.
//! - **Windows:** the NSIS installer starts from the exit path once the owner has drained
//!   (LC-01), as `/P /UPDATE`: passive, and in update mode, which never uninstalls and never
//!   deletes the app data (docs/DISTRIBUTION.md → Automatic updates).
//!
//! No check runs in a development build, in the smoke check, or from an app macOS is running
//! from a translocated copy.
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use switchboard_runtime::oplog::{event, Field};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The person's choice, kept in the data folder beside `login-item`. Absent means on.
pub const AUTO_UPDATE_FILE: &str = "auto-update";
/// The first check waits for the start-up work (owner, monitor, renewals) to settle.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(90);
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// While an update waits to start (and only then), how often the app looks for an idle moment to
/// start it (LC-16 activation). No other timer runs while nothing is ready.
pub const IDLE_LOOK_EVERY: Duration = Duration::from_secs(5 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// A bundle swap is a few renames and an unpack of ~15 MB; one that has not finished in five
/// minutes is stuck (a hung volume), and the loop must not wait on it.
#[cfg(target_os = "macos")]
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The release ships one universal app; `latest.json` names it under this key.
#[cfg(target_os = "macos")]
pub const MACOS_TARGET: &str = "darwin-universal";

pub const UNAVAILABLE_DEVELOPMENT: &str = "Automatic updates work in the installed app only.";
pub const UNAVAILABLE_TRANSLOCATED: &str =
    "Move Fabric Switchboard to the Applications folder to receive updates.";
pub const UNAVAILABLE_PLATFORM: &str = "Automatic updates are available on macOS and Windows.";
pub const CHECK_FAILED: &str =
    "Could not check for updates. Switchboard tries again within the hour.";
pub const DOWNLOAD_FAILED: &str =
    "Could not download the update. Switchboard tries again within the hour.";
pub const SIGNATURE_FAILED: &str = "The downloaded update did not pass its signature check and was discarded. Switchboard tries again within the hour.";
pub const INSTALL_FAILED: &str =
    "Could not install the update. Switchboard tries again within the hour.";
/// The bundle swap outlived its deadline. Its thread may still be writing, so nothing is retried
/// until the app restarts — the text says so instead of promising a retry within the hour.
pub const INSTALL_STALLED: &str =
    "Installing the update did not finish. Quit and reopen Switchboard to try again.";
pub const NOT_READY: &str = "No update is ready to install yet.";
/// The release's feed marks a step a person must take first (a data migration, LC-16 "held
/// releases"): the update is not downloaded or installed. The mark sits in `latest.json`, which
/// is not signed; a forged one can only hold an update back, never install anything.
pub const NEEDS_MIGRATION: &str = "This version needs a step by a person before it installs; its release notes say what to do. Switchboard keeps the current version.";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const PERMISSION_REFUSED: &str = "The update needs an administrator password to replace Switchboard in this folder. It was not installed.";
const SAVE_FAILED: &str = "Could not save the choice in Switchboard's data folder.";

/// Whether checking is on, from the saved choice. Anything but "off" is the default: on.
pub fn enabled(choice: Option<&str>) -> bool {
    !matches!(choice.map(str::trim), Some("off"))
}

/// Whether this copy can update itself: an installed build, on a supported platform, and on
/// macOS not a translocated copy (Gatekeeper runs a quarantined app from a read-only random
/// path until it is moved; an update installed there would vanish).
pub fn availability(exe: &Path, packaged: bool) -> Result<(), &'static str> {
    if !cfg!(any(target_os = "macos", windows)) {
        return Err(UNAVAILABLE_PLATFORM);
    }
    if !packaged {
        return Err(UNAVAILABLE_DEVELOPMENT);
    }
    if cfg!(target_os = "macos") && exe.to_string_lossy().contains("/AppTranslocation/") {
        return Err(UNAVAILABLE_TRANSLOCATED);
    }
    Ok(())
}

/// Whether a release is offered at all: only a version greater than the running one. This is the
/// plugin's default rule (it never offers an equal or older version unless `allowDowngrades`,
/// which is off), stated here so it is tested and cannot drift with the plugin. Downgrades are
/// also refused by the signature: `requireSignedVersion` makes the version signed into the
/// package match the one announced, so a manifest cannot dress an old package as new.
pub fn offered(current: &semver::Version, announced: &semver::Version) -> bool {
    announced > current
}

/// An idle moment to start a ready update on its own (LC-16 activation): updates are on, the
/// update can start without the person (macOS: already in the bundle; Windows: the installer is
/// downloaded), nobody is looking at the window, and nothing a restart would cut short is running
/// — no managed request in flight, no sign-in waiting. Seen on two looks in a row, never one, so
/// a request that is just starting is not cut.
pub fn idle_moment(
    enabled: bool,
    starts_alone: bool,
    window_hidden: bool,
    busy: bool,
    idle_before: bool,
) -> (bool, bool) {
    let idle = enabled && starts_alone && window_hidden && !busy;
    (idle, idle && idle_before)
}

/// The arguments a relaunch after an update starts with: the program, and `--background` when
/// the window was hidden, so an app that was working in the background stays there.
#[cfg_attr(windows, allow(dead_code))]
pub fn relaunch_args(program: OsString, background: bool) -> Vec<OsString> {
    std::iter::once(program)
        .chain(background.then(|| OsString::from(crate::residency::BACKGROUND_ARG)))
        .collect()
}

/// The NSIS arguments the updater adds after its own `/P /UPDATE` when the app should come back:
/// `/R` relaunches it, `/ARGS` hands it the background flag.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_relaunch_args(background: bool) -> Vec<&'static str> {
    if background {
        vec!["/R", "/ARGS", crate::residency::BACKGROUND_ARG]
    } else {
        vec!["/R"]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Checking,
    Downloading,
    Ready,
    Failed,
}

impl Phase {
    pub fn code(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Checking => "checking",
            Phase::Downloading => "downloading",
            Phase::Ready => "ready",
            Phase::Failed => "failed",
        }
    }
}

/// A verified update. `bytes` is the package still to install: on Windows always (the installer
/// runs on the exit path); on macOS only when the bundle needs an administrator password.
struct Ready {
    update: Update,
    bytes: Option<Vec<u8>>,
}

impl Ready {
    fn waits_for_quit(&self) -> bool {
        self.bytes.is_some()
    }
}

/// "Restart to update" was pressed: the exit path installs (Windows) and relaunches.
struct Restart {
    #[cfg_attr(windows, allow(dead_code))]
    background: bool,
    /// Windows: the same release, checked again with the relaunch arguments for this window
    /// state. None falls back to relaunching with the arguments this process started with.
    #[cfg(windows)]
    relaunching: Option<Update>,
}

struct Inner {
    phase: Phase,
    version: Option<String>,
    error: Option<&'static str>,
    checked_at: Option<i64>,
    ready: Option<Ready>,
    restart: Option<Restart>,
    /// The quit came from a signal (logout, shutdown, `kill`): no installer is started then.
    signal: bool,
    /// A macOS bundle swap outlived INSTALL_TIMEOUT. Its thread may still be running, so no
    /// further check starts until the app restarts.
    stalled: bool,
    /// The idle-moment watch for a ready update is running (at most one).
    watching: bool,
}

pub struct Updates {
    available: Result<(), &'static str>,
    root: Option<PathBuf>,
    inner: Mutex<Inner>,
    wake: tokio::sync::Notify,
}

impl Updates {
    pub fn new(available: Result<(), &'static str>, root: Option<PathBuf>) -> Self {
        Updates {
            available,
            root,
            inner: Mutex::new(Inner {
                phase: Phase::Idle,
                version: None,
                error: None,
                checked_at: None,
                ready: None,
                restart: None,
                signal: false,
                stalled: false,
                watching: false,
            }),
            wake: tokio::sync::Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn choice_path(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|root| root.join(AUTO_UPDATE_FILE))
    }

    pub fn is_enabled(&self) -> bool {
        let choice = self
            .choice_path()
            .and_then(|path| std::fs::read_to_string(path).ok());
        enabled(choice.as_deref())
    }

    /// `{available, reason, enabled, state, current, version, checked_at, error, needs_permission}`
    /// for the About panel.
    pub fn status(&self) -> Value {
        let inner = self.lock();
        json!({
            "available": self.available.is_ok(),
            "reason": self.available.err(),
            "enabled": self.available.is_ok() && self.is_enabled(),
            "state": inner.phase.code(),
            "current": env!("CARGO_PKG_VERSION"),
            "version": inner.version,
            "checked_at": inner.checked_at,
            "error": inner.error,
            "needs_permission": cfg!(target_os = "macos")
                && inner.ready.as_ref().is_some_and(|ready| ready.bytes.is_some()),
        })
    }

    /// The person's switch: saved first; turning it on checks now.
    pub fn set(&self, enabled: bool) -> Result<Value, String> {
        self.available.map_err(str::to_string)?;
        let path = self.choice_path().ok_or(SAVE_FAILED)?;
        switchboard_core::private_fs::private_write(
            &path,
            if enabled { b"on\n" } else { b"off\n" },
        )
        .map_err(|_| SAVE_FAILED.to_string())?;
        event(
            "auto_update",
            &[("outcome", Field::Code(if enabled { "on" } else { "off" }))],
        );
        if enabled {
            self.wake.notify_one();
        } else {
            self.drop_pending_install();
        }
        Ok(self.status())
    }

    /// Switched off (LC-16): a downloaded Windows installer waiting for the quit is discarded, so
    /// turning updates off also stops the install. On macOS the verified version is already in
    /// the bundle and starts at the next launch, as it would have.
    fn drop_pending_install(&self) {
        let mut inner = self.lock();
        if inner.ready.as_ref().is_some_and(Ready::waits_for_quit) {
            inner.ready = None;
            inner.version = None;
            inner.phase = Phase::Idle;
        }
    }

    /// Whether this exit will start the Windows installer (`finish`): the drain before it is
    /// shortened so the installer keeps time before the hard exit (LC-01, SB-78).
    pub fn installs_at_exit(&self) -> bool {
        let inner = self.lock();
        installs_at_exit(
            cfg!(windows),
            inner.ready.as_ref().is_some_and(Ready::waits_for_quit),
            inner.signal,
            inner.restart.is_some(),
            self.is_enabled(),
        )
    }

    /// A downloaded installer waits for the quit (Windows).
    pub fn has_pending_install(&self) -> bool {
        self.lock()
            .ready
            .as_ref()
            .is_some_and(Ready::waits_for_quit)
    }

    pub fn note_signal(&self) {
        self.lock().signal = true;
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Starts the check loop in an installed build. A failed check is retried within the hour, a
/// finished one in six; turning the switch on wakes it at once.
pub fn start(app: &AppHandle) {
    if let Err(reason) = app.state::<Updates>().available {
        // A copy that never checks says why, once (LC-16 "builds that never check").
        let code = match reason {
            UNAVAILABLE_DEVELOPMENT => "dev_build",
            UNAVAILABLE_TRANSLOCATED => "translocated",
            _ => "unsupported_platform",
        };
        event("update_check", &[("outcome", Field::Code(code))]);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = FIRST_CHECK_AFTER;
        loop {
            {
                let state = app.state::<Updates>();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = state.wake.notified() => {}
                }
            }
            delay = if check_once(&app).await == Phase::Failed {
                RETRY_AFTER_FAILURE
            } else {
                CHECK_EVERY
            };
        }
    });
}

/// One check: nothing while switched off or once an update waits to be installed.
async fn check_once(app: &AppHandle) -> Phase {
    check(app, false).await
}

/// One check. `manual` is the person's *Check for updates*: it runs with the switch off too, and
/// what it finds is downloaded and verified like any update (LC-16).
async fn check(app: &AppHandle, manual: bool) -> Phase {
    let state = app.state::<Updates>();
    if !manual && !state.is_enabled() {
        return Phase::Idle;
    }
    {
        let mut inner = state.lock();
        if inner.ready.is_some() || inner.restart.is_some() || inner.stalled {
            return inner.phase;
        }
        inner.phase = Phase::Checking;
        inner.error = None;
    }
    let outcome = fetch(app, &state).await;
    let mut inner = state.lock();
    inner.checked_at = Some(now());
    let code = match outcome {
        Ok(None) => {
            inner.phase = Phase::Idle;
            inner.version = None;
            "current"
        }
        Ok(Some(ready)) => {
            inner.phase = Phase::Ready;
            inner.version = Some(ready.update.version.clone());
            let version = ready.update.version.clone();
            inner.ready = Some(ready);
            drop(inner);
            crate::residency::show_update(app, &version);
            event("update_check", &[("outcome", Field::Code("ready"))]);
            watch_for_idle(app);
            return Phase::Ready;
        }
        Err(message) => {
            inner.phase = Phase::Failed;
            inner.version = None;
            inner.error = Some(message);
            match message {
                SIGNATURE_FAILED => "signature_failed",
                DOWNLOAD_FAILED => "download_failed",
                INSTALL_FAILED | INSTALL_STALLED => "install_failed",
                NEEDS_MIGRATION => "needs_migration",
                _ => "check_failed",
            }
        }
    };
    let phase = inner.phase;
    drop(inner);
    event("update_check", &[("outcome", Field::Code(code))]);
    phase
}

/// The feed's mark for a release that needs a person first: `"needs_migration": true` or a
/// non-empty runbook string in `latest.json`.
pub fn needs_migration(feed: &Value) -> bool {
    match feed.get("needs_migration") {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(runbook)) => !runbook.trim().is_empty(),
        _ => false,
    }
}

/// The person's *Check for updates*: works with the switch off; returns the status after.
pub async fn check_now(app: AppHandle) -> Result<Value, String> {
    let state = app.state::<Updates>();
    state.available.map_err(str::to_string)?;
    check(&app, true).await;
    Ok(app.state::<Updates>().status())
}

/// Starts the idle-moment watch for a ready update (one at a time). It ends when the update
/// starts, is discarded or the app quits; until then it looks every IDLE_LOOK_EVERY.
fn watch_for_idle(app: &AppHandle) {
    {
        let state = app.state::<Updates>();
        let mut inner = state.lock();
        if inner.watching {
            return;
        }
        inner.watching = true;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut idle_before = false;
        loop {
            tokio::time::sleep(IDLE_LOOK_EVERY).await;
            let state = app.state::<Updates>();
            let starts_alone = {
                let inner = state.lock();
                if inner.restart.is_some() {
                    break;
                }
                match inner.ready.as_ref() {
                    None => break,
                    Some(ready) => ready_starts_alone(ready),
                }
            };
            let busy = match app.try_state::<crate::Slot>() {
                Some(slot) => slot.runtime().await.map_or(true, |runtime| runtime.busy()),
                None => true,
            };
            let (idle, start) = idle_moment(
                state.is_enabled(),
                starts_alone,
                window_hidden(&app),
                busy,
                idle_before,
            );
            idle_before = idle;
            if start {
                event("update_restart", &[("outcome", Field::Code("idle"))]);
                if restart(app.clone()).await.is_err() {
                    event("update_restart", &[("outcome", Field::Code("refused"))]);
                }
                break;
            }
        }
        app.state::<Updates>().lock().watching = false;
    });
}

/// Whether a ready update can start without the person: macOS when it is already in the bundle
/// (one that needs an administrator password waits for *Restart to update*); Windows when its
/// installer is downloaded.
fn ready_starts_alone(ready: &Ready) -> bool {
    if cfg!(windows) {
        ready.bytes.is_some()
    } else {
        ready.bytes.is_none()
    }
}

fn classify(error: tauri_plugin_updater::Error) -> &'static str {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => SIGNATURE_FAILED,
        _ => DOWNLOAD_FAILED,
    }
}

async fn fetch(app: &AppHandle, state: &Updates) -> Result<Option<Ready>, &'static str> {
    let builder = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .version_comparator(|current, release| offered(&current, &release.version));
    #[cfg(target_os = "macos")]
    let builder = builder.target(MACOS_TARGET);
    #[cfg(windows)]
    let builder = builder.restart_after_install(false);
    let updater = builder.build().map_err(|_| CHECK_FAILED)?;
    let Some(mut update) = updater.check().await.map_err(|_| CHECK_FAILED)? else {
        return Ok(None);
    };
    if needs_migration(&update.raw_json) {
        return Err(NEEDS_MIGRATION);
    }
    {
        let mut inner = state.lock();
        inner.phase = Phase::Downloading;
        inner.version = Some(update.version.clone());
    }
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    event("update_download", &[("outcome", Field::Code("started"))]);
    let bytes = update.download(|_, _| {}, || {}).await.map_err(classify)?;
    event("update_download", &[("outcome", Field::Code("done"))]);
    #[cfg(target_os = "macos")]
    {
        let bundle = std::env::current_exe()
            .ok()
            .and_then(|exe| tauri_plugin_updater::extract_path_from_executable(&exe).ok());
        if bundle.is_none() {
            // Kept for "Restart to update", which installs through the same call and reports it.
            event(
                "update_install",
                &[("outcome", Field::Code("bundle_unknown"))],
            );
        }
        if bundle.as_deref().is_some_and(replaceable_without_password) {
            let installing = update.clone();
            let swap = tauri::async_runtime::spawn_blocking(move || installing.install(&bytes));
            let outcome = match tokio::time::timeout(INSTALL_TIMEOUT, swap).await {
                Ok(Ok(Ok(()))) => "installed",
                Ok(Ok(Err(_))) => "failed",
                Ok(Err(_)) => "panicked",
                Err(_) => {
                    state.lock().stalled = true;
                    "timeout"
                }
            };
            event("update_install", &[("outcome", Field::Code(outcome))]);
            match outcome {
                "installed" => {}
                "timeout" => return Err(INSTALL_STALLED),
                _ => return Err(INSTALL_FAILED),
            }
            return Ok(Some(Ready {
                update,
                bytes: None,
            }));
        }
    }
    Ok(Some(Ready {
        update,
        bytes: Some(bytes),
    }))
}

/// Whether an exit starts the Windows installer — the rule `finish` follows: an installer is
/// waiting, the quit is not a signal, and the person asked (*Restart to update*) or updates are on.
pub fn installs_at_exit(
    windows: bool,
    waiting: bool,
    signal: bool,
    restart: bool,
    enabled: bool,
) -> bool {
    windows && waiting && (restart || (!signal && enabled))
}

/// The drain before an exit that starts the installer: 5 s of the 10 s hard exit stay for it.
pub const DRAIN_BEFORE_INSTALL: Duration = Duration::from_secs(5);

/// What a failed install at *Restart to update* means: the person declined the administrator
/// password only when the bundle is known and could not be replaced without one; anything else
/// is an install failure.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn install_refusal(bundle_known: bool, replaceable: bool) -> &'static str {
    if bundle_known && !replaceable {
        PERMISSION_REFUSED
    } else {
        INSTALL_FAILED
    }
}

/// macOS: the person owns the bundle and may write it and its folder, so the swap needs no
/// administrator password (the updater would otherwise ask for one, unprompted, in the
/// background). The probe file, removed on drop, carries this process's own user id.
#[cfg(target_os = "macos")]
pub fn replaceable_without_password(bundle: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some(parent) = bundle.parent() else {
        return false;
    };
    let Ok(probe) = tempfile::Builder::new()
        .prefix(".switchboard-update-")
        .tempfile_in(parent)
    else {
        return false;
    };
    match (probe.as_file().metadata(), std::fs::metadata(bundle)) {
        (Ok(mine), Ok(app)) => app.uid() == mine.uid() && app.mode() & 0o200 != 0,
        _ => false,
    }
}

fn window_hidden(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .map(|visible| !visible)
        .unwrap_or(true)
}

/// "Restart to update": install what still needs the person (macOS, an administrator password),
/// then quit through the exit path, which drains the owner and relaunches.
pub async fn restart(app: AppHandle) -> Result<Value, String> {
    let state = app.state::<Updates>();
    state.available.map_err(str::to_string)?;
    let (update, bytes) = {
        let inner = state.lock();
        let ready = inner.ready.as_ref().ok_or(NOT_READY)?;
        (ready.update.clone(), ready.bytes.clone())
    };
    let background = window_hidden(&app);
    #[cfg(target_os = "macos")]
    if let Some(bytes) = bytes {
        let installing = update.clone();
        let installed = tauri::async_runtime::spawn_blocking(move || installing.install(&bytes))
            .await
            .map_err(|_| INSTALL_FAILED.to_string())?;
        if installed.is_err() {
            // Only a bundle that needed the password can have been refused it; any other
            // failure is an install failure, not the person's answer (SB-78).
            let bundle = std::env::current_exe()
                .ok()
                .and_then(|exe| tauri_plugin_updater::extract_path_from_executable(&exe).ok());
            let message = install_refusal(
                bundle.is_some(),
                bundle.as_deref().is_some_and(replaceable_without_password),
            );
            event(
                "update_restart",
                &[(
                    "outcome",
                    Field::Code(if message == PERMISSION_REFUSED {
                        "refused"
                    } else {
                        "install_failed"
                    }),
                )],
            );
            return Err(message.into());
        }
        if let Some(ready) = state.lock().ready.as_mut() {
            ready.bytes = None;
        }
    }
    // Windows: the installer always runs the bytes verified when the update became ready
    // (`finish`). The re-check only builds an installer handle that carries the relaunch
    // arguments for this window state; its own package is never downloaded. When it answers
    // anything but the same release, the handle of the ready update relaunches with the
    // arguments this process started with.
    #[cfg(windows)]
    let relaunching = {
        let _ = &bytes;
        let checked = match app
            .updater_builder()
            .timeout(Duration::from_secs(15))
            .version_comparator(|current, release| offered(&current, &release.version))
            .restart_after_install(false)
            .installer_args(windows_relaunch_args(background))
            .build()
        {
            Ok(updater) => updater.check().await.map_err(|_| ()),
            Err(_) => Err(()),
        };
        let (code, handle) = match checked {
            Ok(Some(again))
                if again.version == update.version && again.signature == update.signature =>
            {
                ("same", Some(again))
            }
            Ok(Some(_)) => ("newer", None),
            Ok(None) | Err(_) => ("check_failed", None),
        };
        event("update_relaunch_args", &[("outcome", Field::Code(code))]);
        handle
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = (&update, &bytes);
    state.lock().restart = Some(Restart {
        background,
        #[cfg(windows)]
        relaunching,
    });
    event(
        "update_restart",
        &[(
            "launch",
            Field::Code(if background { "background" } else { "ordinary" }),
        )],
    );
    let status = state.status();
    // Through the exit event, which drains the owner (LC-01) and then calls `finish`.
    app.exit(0);
    Ok(status)
}

/// The exit path, after the owner drained. Windows: start the installer — relaunching after
/// "Restart to update", not after an ordinary quit, never after a signal. macOS: relaunch after
/// "Restart to update" (the bundle already holds the new version).
pub fn finish(app: &AppHandle) {
    let Some(state) = app.try_state::<Updates>() else {
        return;
    };
    let (ready, restart, signal) = {
        let mut inner = state.lock();
        (inner.ready.take(), inner.restart.take(), inner.signal)
    };
    #[cfg(windows)]
    {
        let Some(Ready {
            update,
            bytes: Some(bytes),
        }) = ready
        else {
            return;
        };
        let installer = match restart {
            Some(Restart {
                relaunching: Some(again),
                ..
            }) => again,
            Some(Restart { .. }) => update.restart_after_install(true),
            None if signal => return,
            // An ordinary quit installs only while updates are on (LC-16).
            None if !state.is_enabled() => return,
            None => update.restart_after_install(false),
        };
        event("update_install", &[("outcome", Field::Code("started"))]);
        // Exits the process once the installer has started; an error leaves the quit as it was.
        if installer.install(bytes).is_err() {
            event("update_install", &[("outcome", Field::Code("failed"))]);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (ready, signal);
        if let Some(restart) = restart {
            let mut env = app.env();
            let program = env.args_os.first().cloned().unwrap_or_default();
            env.args_os = relaunch_args(program, restart.background);
            tauri::process::restart(&env);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ready_update_starts_alone_only_on_two_idle_looks_in_a_row() {
        // LC-16 activation: never under a person, a request or a sign-in; never on one look.
        assert_eq!(idle_moment(true, true, true, false, false), (true, false));
        assert_eq!(idle_moment(true, true, true, false, true), (true, true));
        for (enabled, alone, hidden, busy) in [
            (false, true, true, false),
            (true, false, true, false),
            (true, true, false, false),
            (true, true, true, true),
        ] {
            assert_eq!(
                idle_moment(enabled, alone, hidden, busy, true),
                (false, false),
                "{enabled} {alone} {hidden} {busy}"
            );
        }
    }

    #[test]
    fn the_installer_starts_only_on_an_ordinary_quit_with_updates_on_or_a_restart() {
        // (windows, waiting, signal, restart, enabled)
        assert!(installs_at_exit(true, true, false, false, true));
        assert!(
            installs_at_exit(true, true, false, true, false),
            "the person asked"
        );
        assert!(
            !installs_at_exit(true, true, true, false, true),
            "never after a signal"
        );
        assert!(
            !installs_at_exit(true, true, false, false, false),
            "switched off"
        );
        assert!(
            !installs_at_exit(true, false, false, true, true),
            "nothing waiting"
        );
        assert!(
            !installs_at_exit(false, true, false, true, true),
            "macOS installs elsewhere"
        );
        assert!(DRAIN_BEFORE_INSTALL < switchboard_runtime::HARD_EXIT_AFTER);
    }

    #[test]
    fn only_a_bundle_that_needed_the_password_reports_a_refusal() {
        assert_eq!(install_refusal(true, false), PERMISSION_REFUSED);
        assert_eq!(install_refusal(true, true), INSTALL_FAILED);
        assert_eq!(install_refusal(false, false), INSTALL_FAILED);
    }

    #[test]
    fn a_release_that_needs_a_person_is_held() {
        assert!(needs_migration(
            &json!({"version": "0.7.0", "needs_migration": true})
        ));
        assert!(needs_migration(
            &json!({"needs_migration": "docs/OPERATIONS.md#migrate-0-7"})
        ));
        assert!(!needs_migration(&json!({"needs_migration": false})));
        assert!(!needs_migration(&json!({"needs_migration": "  "})));
        assert!(!needs_migration(&json!({"version": "0.7.0"})));
    }

    #[test]
    fn checking_is_on_by_default_and_off_only_when_switched_off() {
        assert!(enabled(None));
        assert!(enabled(Some("on\n")));
        assert!(enabled(Some("garbage")));
        assert!(!enabled(Some("off\n")));
        assert!(!enabled(Some("  off  ")));
    }

    #[test]
    fn a_development_build_never_checks() {
        let exe =
            Path::new("/Applications/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard");
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(availability(exe, false), Err(UNAVAILABLE_DEVELOPMENT));
            assert_eq!(availability(exe, true), Ok(()));
        } else {
            assert_eq!(availability(exe, true), Err(UNAVAILABLE_PLATFORM));
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_translocated_copy_is_told_to_move_to_applications() {
        let exe = Path::new("/private/var/folders/x/AppTranslocation/1234/d/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard");
        assert_eq!(availability(exe, true), Err(UNAVAILABLE_TRANSLOCATED));
    }

    #[test]
    fn only_a_newer_version_is_offered() {
        let v = |text: &str| semver::Version::parse(text).unwrap();
        assert!(offered(&v("0.6.0"), &v("0.7.0")));
        assert!(offered(&v("0.6.0"), &v("0.6.1")));
        assert!(
            !offered(&v("0.6.0"), &v("0.6.0")),
            "the same version is not an update"
        );
        assert!(
            !offered(&v("0.7.0"), &v("0.6.9")),
            "an older version is never offered"
        );
        assert!(
            !offered(&v("0.7.0"), &v("0.7.0-beta.1")),
            "a prerelease of the running version is older"
        );
    }

    #[test]
    fn a_relaunch_keeps_the_background_start() {
        let program = OsString::from(
            "/Applications/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard",
        );
        assert_eq!(relaunch_args(program.clone(), false), vec![program.clone()]);
        assert_eq!(
            relaunch_args(program.clone(), true),
            vec![program, OsString::from("--background")]
        );
        assert_eq!(windows_relaunch_args(false), vec!["/R"]);
        assert_eq!(
            windows_relaunch_args(true),
            vec!["/R", "/ARGS", "--background"]
        );
    }

    #[test]
    fn the_switch_is_saved_in_the_data_folder_and_reported() {
        let folder = tempfile::tempdir().unwrap();
        let updates = Updates::new(Ok(()), Some(folder.path().to_owned()));
        assert_eq!(updates.status()["enabled"], true, "absent means on");
        assert_eq!(updates.set(false).unwrap()["enabled"], false);
        assert_eq!(
            std::fs::read_to_string(folder.path().join(AUTO_UPDATE_FILE)).unwrap(),
            "off\n"
        );
        assert_eq!(updates.set(true).unwrap()["enabled"], true);
        let status = updates.status();
        assert_eq!(status["state"], "idle");
        assert_eq!(status["current"], env!("CARGO_PKG_VERSION"));
        assert_eq!(status["needs_permission"], false);
    }

    #[test]
    fn an_unavailable_copy_refuses_the_switch_with_its_reason() {
        let folder = tempfile::tempdir().unwrap();
        let updates = Updates::new(Err(UNAVAILABLE_DEVELOPMENT), Some(folder.path().to_owned()));
        assert_eq!(updates.set(true), Err(UNAVAILABLE_DEVELOPMENT.to_string()));
        let status = updates.status();
        assert_eq!(status["available"], false);
        assert_eq!(status["enabled"], false);
        assert_eq!(status["reason"], UNAVAILABLE_DEVELOPMENT);
        assert!(!folder.path().join(AUTO_UPDATE_FILE).exists());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_bundle_the_person_owns_is_replaceable_and_a_missing_one_is_not() {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("Fabric Switchboard.app");
        std::fs::create_dir(&bundle).unwrap();
        assert!(replaceable_without_password(&bundle));
        assert!(!replaceable_without_password(
            &folder.path().join("absent.app")
        ));
        // A root-owned system folder: the probe file cannot be created there.
        assert!(!replaceable_without_password(Path::new(
            "/System/Applications/Calculator.app"
        )));
    }
}
