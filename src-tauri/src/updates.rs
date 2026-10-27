//! Automatic updates (SB-55): an installed Switchboard checks the latest GitHub release at start
//! and every five minutes, downloads a newer version in the background, verifies it against the
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
//! - **Linux (SB-88):** only the AppImage updates itself: the verified AppImage replaces the
//!   running one's file at once (as macOS swaps its bundle) and starts at the next restart. A copy
//!   installed from the .deb package never checks; the newer .deb updates it.
//!
//! No check runs in a development build, in the smoke check, from an app macOS is running
//! from a translocated copy, or from a Linux copy that is not an AppImage.
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
/// Every PassionCode product checks for a new version every five minutes (operator 2026-10-17,
/// «проверка обновления раз в 5 минут во всех продуктах passioncode»). One small request to the
/// release's `latest.json` asset, which is not the rate-limited GitHub API.
pub const CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// While an update waits to start (and only then), how often the app looks for an idle moment to
/// start it (LC-16 activation). No other timer runs while nothing is ready.
pub const IDLE_LOOK_EVERY: Duration = Duration::from_secs(5 * 60);
/// SB-131: a window left open but out of focus this long counts as nobody looking. Before it, an
/// update waited behind a window the person had simply left open: 0.6.31 ran for a day with
/// 0.6.32 and 0.6.33 installed (`update_waiting window_visible` every hour, 2026-10-15).
pub const UNATTENDED_SECONDS: i64 = 30 * 60;
/// An update that waited this long only for requests in flight drains instead (2026-10-18: with
/// two dozen agent sessions there was never a moment with none, and 0.6.40 waited three hours).
pub const DRAIN_AFTER_SECONDS: i64 = 30 * 60;
/// SB-191 (operator decision 2026-10-25): an update downloads at once, but the app restarts into
/// one on its own at most once in this long, and only at an idle moment (12 restarts in 48 hours
/// cut managed sessions). A release the feed marks `"urgent": true` restarts at the next idle
/// moment. *Restart to update* is the person's and is never held.
pub const RESTART_EVERY_SECONDS: i64 = 6 * 60 * 60;
/// When the app last restarted into an update on its own: Unix seconds, in the data folder.
pub const RESTARTED_AT_FILE: &str = "update-restarted-at";
/// How long a drain lets the requests in flight finish before the restart.
pub const DRAIN_WAIT: Duration = Duration::from_secs(90);
/// How often the idle watch says, at most, what keeps a ready update from starting (SB-105): the
/// first look that finds a reason logs it, later ones only once this much time has passed.
pub const IDLE_REPORT_EVERY: i64 = 60 * 60;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// A bundle swap is a few renames and an unpack of ~15 MB; one that has not finished in five
/// minutes is stuck (a hung volume), and the loop must not wait on it.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The release ships one universal app; `latest.json` names it under this key.
#[cfg(target_os = "macos")]
pub const MACOS_TARGET: &str = "darwin-universal";

pub const UNAVAILABLE_DEVELOPMENT: &str = "Automatic updates work in the installed app only.";
pub const UNAVAILABLE_TRANSLOCATED: &str =
    "Move Fabric Switchboard to the Applications folder to receive updates.";
pub const UNAVAILABLE_PLATFORM: &str =
    "Automatic updates are available on macOS, Windows and Linux.";
/// A Linux copy installed from the .deb package: the package manager owns its files (SB-88).
pub const UNAVAILABLE_PACKAGE: &str =
    "This copy was installed from the .deb package. Install the newer .deb from the releases page to update it.";
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
/// `appimage` is `$APPIMAGE`, set by the AppImage runtime to the image's own file (Linux).
pub fn availability(
    exe: &Path,
    packaged: bool,
    appimage: Option<&OsString>,
) -> Result<(), &'static str> {
    if !cfg!(any(target_os = "macos", windows, target_os = "linux")) {
        return Err(UNAVAILABLE_PLATFORM);
    }
    if !packaged {
        return Err(UNAVAILABLE_DEVELOPMENT);
    }
    if cfg!(target_os = "macos") && exe.to_string_lossy().contains("/AppTranslocation/") {
        return Err(UNAVAILABLE_TRANSLOCATED);
    }
    if cfg!(target_os = "linux") && appimage.is_none_or(|image| image.is_empty()) {
        return Err(UNAVAILABLE_PACKAGE);
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

/// What keeps a ready update from starting on its own, as a log code (SB-105), in the order the
/// idle watch looks: `updates_off`, `needs_person` (a bundle that needs a password), `window_visible`,
/// `checking` (a newer release is being fetched), then the runtime's own (`request_in_flight`,
/// `sign_in_waiting`, or `owner_unavailable`). None exactly when `idle_moment` finds it idle.
pub fn idle_blocker(
    enabled: bool,
    starts_alone: bool,
    window_hidden: bool,
    checking: bool,
    busy: Option<&'static str>,
) -> Option<&'static str> {
    if !enabled {
        Some("updates_off")
    } else if !starts_alone {
        Some("needs_person")
    } else if !window_hidden {
        Some("window_visible")
    } else if checking {
        Some("checking")
    } else {
        busy
    }
}

/// The feed's mark for a release that should not wait out RESTART_EVERY_SECONDS (SB-191):
/// `"urgent": true` or a non-blank reason.
pub fn urgent(feed: &Value) -> bool {
    match feed.get("urgent") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.trim().is_empty(),
        _ => false,
    }
}

/// Seconds left before the app may restart into an update on its own (SB-191): none before the
/// first such restart, for an urgent release, or once RESTART_EVERY_SECONDS have passed. A
/// restart time in the future (a clock set back) holds no longer than one period from now.
pub fn cooldown_left(last: Option<i64>, now: i64, urgent: bool) -> i64 {
    match last {
        Some(at) if !urgent => (at.min(now) + RESTART_EVERY_SECONDS - now).max(0),
        _ => 0,
    }
}

/// When a ready update may start on its own (SB-208): the end of SB-191's pause, as Unix seconds,
/// for the About line; none when no update waits to start on its own (`waiting` is `None`), when
/// it is urgent (`Some(true)`), or when the pause is over. A recorded time ahead of the clock is
/// read as now, as the watch reads it, so the time shown is never more than a period away.
pub fn restart_after(waiting: Option<bool>, recorded: Option<i64>, now: i64) -> Option<i64> {
    let urgent = waiting?;
    let (last, _) = recorded_restart(recorded, now);
    let left = cooldown_left(last, now, urgent);
    (left > 0).then_some(now + left)
}

/// The last restart on its own as the watch uses it, and whether to write it back (SB-191): a
/// time recorded ahead of the clock (a clock set back) becomes now, once, so the pause ends one
/// period from now instead of holding until the clock catches up.
pub fn recorded_restart(recorded: Option<i64>, now: i64) -> (Option<i64>, bool) {
    match recorded {
        Some(at) if at > now => (Some(now), true),
        other => (other, false),
    }
}

/// When the half hour before a drain starts counting (SB-191): the download, or the end of the
/// pause when that is later. An urgent release has no pause, so it drains on the same terms as
/// before 0.6.83.
pub fn drain_from(waiting_since: i64, last: Option<i64>, now: i64, urgent: bool) -> i64 {
    match last {
        Some(at) if !urgent => waiting_since.max(at.min(now) + RESTART_EVERY_SECONDS),
        _ => waiting_since,
    }
}

/// Whether a ready update stops waiting for a quiet moment and drains: it has waited
/// DRAIN_AFTER_SECONDS and nothing else keeps it (the window, a sign-in, the switch) but requests
/// in flight.
pub fn drain_due(waited: i64, blocker: Option<&str>) -> bool {
    waited >= DRAIN_AFTER_SECONDS && blocker == Some("request_in_flight")
}

/// When the idle watch logs its reason (SB-105): the first time it has one, then at most once per
/// `IDLE_REPORT_EVERY`. Seconds come from the caller, so a test drives it with a fake clock.
#[derive(Default)]
pub struct IdleReport {
    last: Option<i64>,
}

impl IdleReport {
    pub fn due(&mut self, now: i64) -> bool {
        match self.last {
            Some(at) if now - at < IDLE_REPORT_EVERY => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// Whether a release replaces what this copy has (SB-105): newer than the running version and,
/// while an update already waits, newer than that one too — so a release published while an
/// update waits is still taken, and the waiting one is never fetched twice.
pub fn replaces(
    current: &semver::Version,
    waiting: Option<&semver::Version>,
    announced: &semver::Version,
) -> bool {
    offered(current, announced) && waiting.is_none_or(|waiting| announced > waiting)
}

/// The arguments a relaunch after an update starts with: the program, and `--background` when
/// the window was hidden, so an app that was working in the background stays there.
#[cfg_attr(windows, allow(dead_code))]
pub fn relaunch_args(program: OsString, background: bool) -> Vec<OsString> {
    std::iter::once(program)
        .chain(background.then(|| OsString::from(crate::residency::BACKGROUND_ARG)))
        .collect()
}

/// SB-151: how long the relaunch helper waits for this process to end before starting the new
/// copy anyway (the hard exit ends it after `HARD_EXIT_AFTER`, 10 s).
#[cfg(unix)]
pub const RELAUNCH_WAIT_SECONDS: u32 = 60;

/// SB-151: the helper that starts the new copy once this one has ended, however it ends: a
/// drained exit, or the hard exit when the drain overruns (2026-10-20: the drain overran, the
/// hard exit fired and nothing relaunched for 20 minutes). `/bin/sh` waits while `pid` lives
/// (at most `RELAUNCH_WAIT_SECONDS`) and then execs the program with its arguments.
#[cfg(unix)]
pub fn relaunch_helper(pid: u32, args: &[OsString]) -> std::process::Command {
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!(
            "pid=$1; shift; n=0; while kill -0 \"$pid\" 2>/dev/null && [ $n -lt {} ]; do sleep 0.2; n=$((n+1)); done; exec \"$@\"",
            RELAUNCH_WAIT_SECONDS * 5
        ))
        .arg("switchboard-relaunch")
        .arg(pid.to_string())
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command
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
    /// SB-151: a relaunch helper waits for this process to end; `finish` starts nothing then.
    #[cfg(not(windows))]
    helper: bool,
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
    /// A check runs while an update already waits (SB-105): the idle watch does not start the
    /// waiting one under it, so a newer release is not fetched only to be dropped by the restart.
    rechecking: bool,
}

pub struct Updates {
    available: Result<(), &'static str>,
    root: Option<PathBuf>,
    inner: Mutex<Inner>,
    wake: tokio::sync::Notify,
    /// Held across a bundle swap (macOS, Linux) and across *Restart to update* until the exit is
    /// committed (SB-105): a restart never exits in the middle of a swap, and a swap never starts
    /// once a restart is under way.
    swap: tokio::sync::Mutex<()>,
    /// When the window last had the person's attention: shown, focused or losing focus (SB-131).
    attention: std::sync::atomic::AtomicI64,
}

/// Whether nobody is looking at the window (SB-131): it is hidden (or there is none), or it has
/// been open without focus for UNATTENDED_SECONDS since it last had the person's attention.
pub fn nobody_looking(visible: Option<bool>, focused: bool, attention: i64, now: i64) -> bool {
    match visible {
        Some(true) => !focused && now - attention >= UNATTENDED_SECONDS,
        _ => true,
    }
}

impl Updates {
    /// The window was shown, gained or lost focus: the person's attention is now (SB-131).
    pub fn note_attention(&self) {
        self.attention
            .store(now(), std::sync::atomic::Ordering::Relaxed);
    }
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
                rechecking: false,
            }),
            wake: tokio::sync::Notify::new(),
            swap: tokio::sync::Mutex::new(()),
            attention: std::sync::atomic::AtomicI64::new(now()),
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

    /// When the app last restarted into an update on its own (SB-191); none when unknown.
    fn restarted_at(&self) -> Option<i64> {
        let path = self.root.as_ref()?.join(RESTARTED_AT_FILE);
        std::fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    /// Records an automatic restart, before it happens: a restart that then fails still counts,
    /// so a broken relaunch is not retried every five minutes.
    fn record_restart(&self, at: i64) {
        if let Some(root) = self.root.as_ref() {
            let _ = std::fs::write(root.join(RESTARTED_AT_FILE), at.to_string());
        }
    }

    pub fn is_enabled(&self) -> bool {
        let choice = self
            .choice_path()
            .and_then(|path| std::fs::read_to_string(path).ok());
        enabled(choice.as_deref())
    }

    /// `{available, reason, enabled, state, current, version, checked_at, error, needs_permission,
    /// restart_after}` for the About panel. `restart_after` (SB-208): when a ready update that
    /// starts on its own may do so, while SB-191's pause holds it; null otherwise.
    pub fn status(&self) -> Value {
        let inner = self.lock();
        let waiting = inner
            .ready
            .as_ref()
            .filter(|ready| ready_starts_alone(ready))
            .map(|ready| urgent(&ready.update.raw_json));
        let restart_after = restart_after(waiting, self.restarted_at(), now());
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
            "restart_after": restart_after,
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
            UNAVAILABLE_PACKAGE => "deb_package",
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

/// One check: nothing while switched off. While an update waits it still runs (SB-105), so a later
/// release replaces the waiting one.
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
    let waiting = {
        let mut inner = state.lock();
        if inner.restart.is_some() || inner.stalled || inner.rechecking {
            return inner.phase;
        }
        let waiting = inner
            .ready
            .as_ref()
            .and_then(|ready| semver::Version::parse(&ready.update.version).ok());
        if inner.ready.is_some() {
            // The About panel keeps showing the waiting update while the check runs.
            inner.rechecking = true;
        } else {
            inner.phase = Phase::Checking;
            inner.error = None;
        }
        waiting
    };
    let outcome = fetch(app, &state, waiting).await;
    let mut inner = state.lock();
    inner.checked_at = Some(now());
    if inner.rechecking {
        inner.rechecking = false;
        return recheck_done(app, inner, outcome);
    }
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
    if logs_check(code) {
        event("update_check", &[("outcome", Field::Code(code))]);
    }
    phase
}

/// The outcome of the last logged check. A check every five minutes that finds nothing new is
/// logged once, not 288 times a day; every other outcome is logged each time.
static LAST_LOGGED: Mutex<Option<&'static str>> = Mutex::new(None);
fn logs_check(code: &'static str) -> bool {
    let Ok(mut last) = LAST_LOGGED.lock() else {
        return true;
    };
    let repeat = code == "current" && *last == Some("current");
    *last = Some(code);
    !repeat
}

/// The same rule for the checks made while an update waits (PRD-14, iteration 2026-10-24): one
/// line when nothing newer is found, not one every five minutes until the restart.
static LAST_RECHECK_LOGGED: Mutex<Option<&'static str>> = Mutex::new(None);
fn logs_recheck(code: &'static str) -> bool {
    let Ok(mut last) = LAST_RECHECK_LOGGED.lock() else {
        return true;
    };
    let repeat = code == "current" && *last == Some("current");
    *last = Some(code);
    !repeat
}

/// The end of a check that ran while an update waited (SB-105). Nothing newer, or a check or
/// download that failed, leaves the waiting update as it was; a newer verified release takes its
/// place — unless a restart into the waiting one is already under way. A swap that failed or stalled leaves the bundle in an unknown state, so the waiting
/// update is dropped and the failure shown, as for a first install.
fn recheck_done(
    app: &AppHandle,
    mut inner: MutexGuard<'_, Inner>,
    outcome: Result<Option<Ready>, &'static str>,
) -> Phase {
    let (code, phase) = match outcome {
        Ok(None) => ("current", Phase::Ready),
        // A restart into the waiting update is already recorded with that update's installer
        // (Windows `relaunching`): the newer one waits for the next start's check.
        Ok(Some(_)) if inner.restart.is_some() => ("deferred", Phase::Ready),
        Ok(Some(newer)) => {
            let version = newer.update.version.clone();
            inner.version = Some(version.clone());
            inner.ready = Some(newer);
            inner.phase = Phase::Ready;
            drop(inner);
            crate::residency::show_update(app, &version);
            event("update_check", &[("outcome", Field::Code("replaced"))]);
            watch_for_idle(app);
            return Phase::Ready;
        }
        Err(message @ (INSTALL_FAILED | INSTALL_STALLED)) => {
            inner.ready = None;
            inner.phase = Phase::Failed;
            inner.version = None;
            inner.error = Some(message);
            ("install_failed", Phase::Failed)
        }
        Err(SIGNATURE_FAILED) => ("signature_failed", Phase::Failed),
        Err(DOWNLOAD_FAILED) => ("download_failed", Phase::Failed),
        Err(NEEDS_MIGRATION) => ("needs_migration", Phase::Ready),
        Err(_) => ("check_failed", Phase::Failed),
    };
    drop(inner);
    if logs_recheck(code) {
        event(
            "update_check",
            &[
                ("outcome", Field::Code(code)),
                ("waiting", Field::Code("yes")),
            ],
        );
    }
    // Failed: the loop retries within the hour; the waiting update (if kept) still starts.
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
        let mut report = IdleReport::default();
        let waiting_since = now();
        loop {
            tokio::time::sleep(IDLE_LOOK_EVERY).await;
            let state = app.state::<Updates>();
            let (starts_alone, checking, is_urgent) = {
                let inner = state.lock();
                if inner.restart.is_some() {
                    break;
                }
                match inner.ready.as_ref() {
                    None => break,
                    Some(ready) => (
                        ready_starts_alone(ready),
                        inner.rechecking,
                        urgent(&ready.update.raw_json),
                    ),
                }
            };
            // SB-191: at most one restart on its own in RESTART_EVERY_SECONDS.
            let time = now();
            let (last, rewrite) = recorded_restart(state.restarted_at(), time);
            if rewrite {
                state.record_restart(time);
            }
            let cooling = cooldown_left(last, time, is_urgent);
            let busy = match app.try_state::<crate::Slot>() {
                Some(slot) => slot
                    .runtime()
                    .await
                    .map_or(Some("owner_unavailable"), |runtime| runtime.busy_reason()),
                None => Some("owner_unavailable"),
            };
            let enabled = state.is_enabled();
            let hidden = window_hidden(&app);
            // SB-105: a watch that never starts the update says why, at most hourly.
            let blocker = if enabled && starts_alone && cooling > 0 {
                Some("cooldown")
            } else {
                idle_blocker(enabled, starts_alone, hidden, checking, busy)
            };
            if let Some(reason) = blocker {
                if report.due(now()) {
                    event("update_waiting", &[("reason", Field::Code(reason))]);
                }
            }
            // The half hour before a drain counts from the end of the pause, not from the
            // download: a restart that waited out the pause first gets its quiet moment.
            let since = drain_from(waiting_since, last, time, is_urgent);
            if cooling == 0 && drain_due(time - since, blocker) {
                state.record_restart(time);
                drain_and_restart(&app).await;
                break;
            }
            let (idle, start) = idle_moment(
                enabled,
                starts_alone,
                hidden,
                busy.is_some() || checking || cooling > 0,
                idle_before,
            );
            idle_before = idle;
            if start {
                state.record_restart(time);
                event("update_restart", &[("outcome", Field::Code("idle"))]);
                // Nobody was looking: the relaunch keeps the window out of the way (SB-131).
                if restart_as(app.clone(), true).await.is_err() {
                    event("update_restart", &[("outcome", Field::Code("refused"))]);
                }
                break;
            }
        }
        app.state::<Updates>().lock().watching = false;
    });
}

/// New requests are asked to retry (503, `Retry-After`), the ones in flight get DRAIN_WAIT to
/// finish, then the app restarts into the update in the background. A refused restart ends the
/// drain, so sessions are never left refused.
async fn drain_and_restart(app: &AppHandle) {
    let Some(slot) = app.try_state::<crate::Slot>() else {
        return;
    };
    let Ok(runtime) = slot.runtime().await else {
        return;
    };
    runtime.proxy.set_draining(true);
    let started = std::time::Instant::now();
    while runtime.proxy.in_flight() > 0 && started.elapsed() < DRAIN_WAIT {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    event(
        "update_restart",
        &[
            ("outcome", Field::Code("drained")),
            ("in_flight", Field::Number(runtime.proxy.in_flight() as i64)),
            (
                "millis",
                Field::Number(started.elapsed().as_millis() as i64),
            ),
        ],
    );
    if restart_as(app.clone(), true).await.is_err() {
        runtime.proxy.set_draining(false);
        event("update_restart", &[("outcome", Field::Code("refused"))]);
    }
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

/// SB-162: where a package being downloaded is kept between attempts, under the data folder.
pub const UPDATES_DIR: &str = "updates";

/// SB-162: the package is fetched into `<data>/updates/<version>-<url hash>.part`, resumed with an
/// HTTP `Range` request on the next check when a slow or dropping link cut it, then handed to the
/// updater plugin over a one-shot loopback server, so the plugin's own signature check runs on
/// the whole file exactly as before. A package that fails that check is deleted.
async fn resumable_download(update: &Update, dir: &Path) -> Result<Vec<u8>, &'static str> {
    let part = dir.join(part_name(&update.version, update.download_url.as_str()));
    prune_parts(dir, &part);
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    event(
        "update_download",
        &[
            (
                "outcome",
                Field::Code(if have > 0 { "resumed" } else { "started" }),
            ),
            ("from", Field::Number(have as i64)),
        ],
    );
    let complete = resume_into(update.download_url.as_str(), &part, DOWNLOAD_TIMEOUT).await;
    let size = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) as i64;
    if !complete {
        event(
            "update_download",
            &[
                ("outcome", Field::Code("partial")),
                ("bytes", Field::Number(size)),
            ],
        );
        return Err(DOWNLOAD_FAILED);
    }
    let body = std::fs::read(&part).map_err(|_| DOWNLOAD_FAILED)?;
    let (url, server) = serve_once(body).await.map_err(|_| DOWNLOAD_FAILED)?;
    let mut local = update.clone();
    local.download_url = url;
    local.no_proxy = true;
    local.proxy = None;
    let checked = local.download(|_, _| {}, || {}).await;
    server.abort();
    match checked {
        Ok(bytes) => {
            let _ = std::fs::remove_file(&part);
            event(
                "update_download",
                &[
                    ("outcome", Field::Code("done")),
                    ("bytes", Field::Number(size)),
                ],
            );
            Ok(bytes)
        }
        Err(error) => {
            let error = classify(error);
            if error == SIGNATURE_FAILED {
                // A file that is not what was signed is never resumed again.
                let _ = std::fs::remove_file(&part);
            }
            Err(error)
        }
    }
}

/// `0.6.61-<16 hex digits of the URL's FNV-1a>.part`: a new release or a moved file never
/// continues another's bytes.
fn part_name(version: &str, url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let version: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{version}-{hash:016x}.part")
}

/// Leaves only `keep` among the partial downloads (an older release's are dropped).
fn prune_parts(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep && path.extension().is_some_and(|e| e == "part") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Appends what is missing of `url` to `part`, asking for the rest with `Range` when part of it is
/// there. True once the file holds the whole package: the stream ended, with the length the
/// server gave (if it gave one). A server that ignores `Range` answers 200 and the file starts
/// over; 416 means it is already whole. Whatever arrived before a failure stays for the next try.
async fn resume_into(url: &str, part: &Path, timeout: Duration) -> bool {
    use std::io::Write;
    if let Some(dir) = part.parent() {
        if switchboard_core::private_fs::private_dir(dir).is_err() {
            return false;
        }
    }
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let Ok(client) = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("fabric-switchboard-updater")
        .build()
    else {
        return false;
    };
    let mut request = client.get(url).header("Accept", "application/octet-stream");
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let Ok(mut response) = request.send().await else {
        return false;
    };
    let status = response.status().as_u16();
    if status == 416 && have > 0 {
        return true;
    }
    let append = status == 206 && have > 0;
    if !(status == 200 || append) {
        return false;
    }
    // The whole package's length: from Content-Range on a 206, Content-Length on a 200.
    let total = if append {
        response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok())
    } else {
        response.content_length()
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part);
    let Ok(mut file) = file else {
        return false;
    };
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if file.write_all(&chunk).is_err() {
                    return false;
                }
            }
            Ok(None) => break,
            Err(_) => {
                let _ = file.flush();
                return false;
            }
        }
    }
    if file.flush().is_err() {
        return false;
    }
    let size = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    total.is_none_or(|total| size == total)
}

/// Serves `body` once over `127.0.0.1` at a random path, for the plugin's own download and
/// signature check; any other path is refused. The task ends after the right request or a minute.
async fn serve_once(body: Vec<u8>) -> std::io::Result<(tauri::Url, tokio::task::JoinHandle<()>)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    // A random path, so another local process cannot take the one answer meant for the plugin.
    let path = uuid::Uuid::new_v4().simple().to_string();
    let url = tauri::Url::parse(&format!("http://{address}/{path}"))
        .map_err(|_| std::io::Error::other("url"))?;
    let task = tokio::spawn(async move {
        let serve = async {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut head = Vec::new();
                let mut buffer = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buffer[..n]),
                    }
                }
                let wanted = format!("GET /{path} ");
                if head.starts_with(wanted.as_bytes()) {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                    return;
                }
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(60), serve).await;
    });
    Ok((url, task))
}

async fn fetch(
    app: &AppHandle,
    state: &Updates,
    waiting: Option<semver::Version>,
) -> Result<Option<Ready>, &'static str> {
    let builder = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .version_comparator(move |current, release| {
            replaces(&current, waiting.as_ref(), &release.version)
        });
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
        if inner.ready.is_none() {
            inner.phase = Phase::Downloading;
            inner.version = Some(update.version.clone());
        }
    }
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let bytes = match state.root.as_ref() {
        Some(root) => resumable_download(&update, &root.join(UPDATES_DIR)).await?,
        None => {
            event("update_download", &[("outcome", Field::Code("started"))]);
            let bytes = update.download(|_, _| {}, || {}).await.map_err(classify)?;
            event("update_download", &[("outcome", Field::Code("done"))]);
            bytes
        }
    };
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
            let _swap = state.swap.lock().await;
            if state.lock().restart.is_some() {
                // A restart into the waiting update is under way; this release waits for the
                // next start's check.
                return Ok(None);
            }
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
    // Linux AppImage: the verified image replaces the running one's file now; the running process
    // keeps its mounted copy until the restart starts the new one (SB-88).
    #[cfg(target_os = "linux")]
    {
        let _swap = state.swap.lock().await;
        if state.lock().restart.is_some() {
            return Ok(None);
        }
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
            "installed" => Ok(Some(Ready {
                update,
                bytes: None,
            })),
            "timeout" => Err(INSTALL_STALLED),
            _ => Err(INSTALL_FAILED),
        }
    }
    #[cfg(not(target_os = "linux"))]
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

/// Whether the window is shown now; a missing window is not.
fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Nobody is looking at the window: hidden, or open and out of focus for UNATTENDED_SECONDS
/// (SB-131). A focused window is attention now.
fn window_hidden(app: &AppHandle) -> bool {
    let window = app.get_webview_window("main");
    let visible = window.as_ref().and_then(|w| w.is_visible().ok());
    let focused = window
        .as_ref()
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    let state = app.state::<Updates>();
    if visible == Some(true) && focused {
        state.note_attention();
    }
    nobody_looking(
        visible,
        focused,
        state.attention.load(std::sync::atomic::Ordering::Relaxed),
        now(),
    )
}

/// "Restart to update": install what still needs the person (macOS, an administrator password),
/// then quit through the exit path, which drains the owner and relaunches.
pub async fn restart(app: AppHandle) -> Result<Value, String> {
    // The person asked: the window comes back as they left it, shown or hidden.
    let background = !window_visible(&app);
    restart_as(app, background).await
}

/// The restart itself; `background` relaunches with the window hidden.
async fn restart_as(app: AppHandle, background: bool) -> Result<Value, String> {
    let state = app.state::<Updates>();
    state.available.map_err(str::to_string)?;
    // Waits out a swap in progress (SB-105), then reads the waiting update, which that swap may
    // just have replaced; held until the restart is recorded, so no swap starts after it.
    let swap = state.swap.lock().await;
    let (update, bytes) = {
        let inner = state.lock();
        let ready = inner.ready.as_ref().ok_or(NOT_READY)?;
        (ready.update.clone(), ready.bytes.clone())
    };
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
        #[cfg(not(windows))]
        helper: false,
    });
    drop(swap);
    // SB-151: the new copy is started by a helper that outlives this process, before the drain:
    // a drain that overruns into the hard exit must not cost the relaunch.
    #[cfg(not(windows))]
    {
        let program = app.env().args_os.first().cloned().unwrap_or_default();
        let spawned = relaunch_helper(std::process::id(), &relaunch_args(program, background))
            .spawn()
            .is_ok();
        if spawned {
            if let Some(restart) = state.lock().restart.as_mut() {
                restart.helper = true;
            }
        }
        event(
            "update_relaunch_helper",
            &[(
                "outcome",
                Field::Code(if spawned { "started" } else { "failed" }),
            )],
        );
    }
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
        // SB-181: the helper starts the new copy when this process ends, and the store's lock
        // ends with it. The owner has drained; waiting for the event loop's own teardown kept
        // the lock 11 s longer on 2026-10-23 (until the hard exit), while every CLI and MCP
        // read was refused.
        if restart.as_ref().is_some_and(|r| r.helper) {
            event("update_restart", &[("outcome", Field::Code("exiting"))]);
            std::process::exit(0);
        }
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
    /// SB-162: a test server that serves `body`, honours `Range`, and cuts the first `cut` answers
    /// after `after` bytes, as a dropping link does.
    async fn flaky_server(body: Vec<u8>, cut: usize, after: usize) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut served = 0;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut head = Vec::new();
                let mut buffer = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buffer[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head).to_lowercase();
                let from = head
                    .lines()
                    .find_map(|l| l.strip_prefix("range: bytes="))
                    .and_then(|r| {
                        r.trim_end_matches('-')
                            .trim()
                            .trim_end_matches('-')
                            .parse::<usize>()
                            .ok()
                    })
                    .unwrap_or(0);
                let rest = &body[from.min(body.len())..];
                let status = if from > 0 {
                    "206 Partial Content"
                } else {
                    "200 OK"
                };
                let range = if from > 0 {
                    format!(
                        "Content-Range: bytes {from}-{}/{}\r\n",
                        body.len() - 1,
                        body.len()
                    )
                } else {
                    String::new()
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\n{range}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    rest.len()
                );
                let _ = socket.write_all(header.as_bytes()).await;
                if served < cut {
                    let _ = socket.write_all(&rest[..after.min(rest.len())]).await;
                    served += 1;
                    drop(socket);
                    continue;
                }
                let _ = socket.write_all(rest).await;
            }
        });
        format!("http://{address}/package.tar.gz")
    }

    #[tokio::test]
    async fn a_cut_download_resumes_where_it_stopped_and_completes() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let url = flaky_server(body.clone(), 2, 50_000).await;
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("updates").join(part_name("1.2.3", &url));
        let timeout = Duration::from_secs(10);
        assert!(
            !resume_into(&url, &part, timeout).await,
            "the first answer is cut"
        );
        assert_eq!(std::fs::metadata(&part).unwrap().len(), 50_000);
        assert!(
            !resume_into(&url, &part, timeout).await,
            "the second is cut too"
        );
        assert_eq!(std::fs::metadata(&part).unwrap().len(), 100_000);
        assert!(
            resume_into(&url, &part, timeout).await,
            "the third finishes the file"
        );
        assert_eq!(std::fs::read(&part).unwrap(), body);
    }

    #[tokio::test]
    async fn the_loopback_answers_only_its_own_path_and_parts_are_kept_per_release() {
        let (url, task) = serve_once(b"package".to_vec()).await.unwrap();
        let wrong = format!(
            "http://{}/other",
            url.host_str().unwrap().to_owned() + ":" + &url.port().unwrap().to_string()
        );
        let client = reqwest::Client::new();
        assert_eq!(
            client.get(&wrong).send().await.unwrap().status().as_u16(),
            404
        );
        let body = client
            .get(url.as_str())
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&body[..], b"package");
        task.abort();
        assert_ne!(
            part_name("1.0.0", "https://a/x"),
            part_name("1.0.1", "https://a/x")
        );
        assert_ne!(
            part_name("1.0.0", "https://a/x"),
            part_name("1.0.0", "https://a/y")
        );
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join(part_name("2.0.0", "u"));
        let old = dir.path().join(part_name("1.0.0", "u"));
        std::fs::write(&keep, b"k").unwrap();
        std::fs::write(&old, b"o").unwrap();
        prune_parts(dir.path(), &keep);
        assert!(keep.exists() && !old.exists());
    }

    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_relaunch_helper_starts_the_new_copy_only_after_the_old_one_ended() {
        // SB-151: the old copy may end by the hard exit; the helper waits for it either way.
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("relaunched");
        let mut old = std::process::Command::new("/bin/sleep")
            .arg("1")
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let mut helper = relaunch_helper(
            old.id(),
            &[
                OsString::from("/usr/bin/touch"),
                marker.clone().into_os_string(),
            ],
        )
        .spawn()
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert!(!marker.exists(), "not while the old copy lives");
        old.wait().unwrap();
        helper.wait().unwrap();
        assert!(marker.exists(), "started once it ended");
        assert!(started.elapsed() >= std::time::Duration::from_millis(900));
    }

    #[test]
    fn an_update_restarts_on_its_own_at_most_once_in_six_hours_unless_urgent() {
        // SB-191: 12 restarts in 48 hours cut managed sessions; the operator's rule (2026-10-25)
        // is one in six hours, at an idle moment, and an urgent release at the next one.
        let six = RESTART_EVERY_SECONDS;
        assert_eq!(cooldown_left(None, 1_000_000, false), 0, "never restarted");
        assert_eq!(
            cooldown_left(Some(1_000_000), 1_000_000 + 60, false),
            six - 60
        );
        assert_eq!(
            cooldown_left(Some(1_000_000), 1_000_000 + six - 1, false),
            1
        );
        assert_eq!(cooldown_left(Some(1_000_000), 1_000_000 + six, false), 0);
        assert_eq!(cooldown_left(Some(1_000_000), 1_000_000 + 60, true), 0);
        // A recorded time in the future holds one period from now, never longer.
        assert_eq!(cooldown_left(Some(9_000_000), 1_000_000, false), six);
        assert!(urgent(&json!({"version": "0.6.84", "urgent": true})));
        assert!(urgent(&json!({"urgent": "a credential leak fix"})));
        assert!(!urgent(&json!({"urgent": false})));
        assert!(!urgent(&json!({"urgent": "  "})));
        assert!(!urgent(&json!({"version": "0.6.84"})));
        // The drain's half hour counts from the end of the pause, but not for an urgent release:
        // on a Mac never idle it drains 30 minutes after the download, not six hours later.
        let (downloaded, last) = (1_000_000, Some(1_000_000 - 60));
        assert_eq!(
            drain_from(downloaded, last, downloaded + 60, false),
            1_000_000 - 60 + six
        );
        assert_eq!(
            drain_from(downloaded, last, downloaded + 60, true),
            downloaded
        );
        assert_eq!(
            drain_from(downloaded, None, downloaded + 60, false),
            downloaded
        );
        // A time ahead of the clock is moved to now once, so the pause ends a period later.
        let (later, rewrite) = recorded_restart(Some(downloaded + 365 * 86_400), downloaded);
        assert_eq!((later, rewrite), (Some(downloaded), true));
        assert_eq!(cooldown_left(later, downloaded + six, false), 0);
        assert_eq!(recorded_restart(last, downloaded), (last, false));
        assert_eq!(recorded_restart(None, downloaded), (None, false));
        assert_eq!(
            drain_from(downloaded, Some(downloaded + 9_999), downloaded, false),
            downloaded + six,
            "a future time never pushes the drain past a period from now"
        );
    }

    #[test]
    fn a_waiting_update_says_when_the_pause_ends() {
        // SB-208: About said a ready update starts when Switchboard is idle, but during SB-191's
        // pause it does not; the status names the end of the pause.
        let six = RESTART_EVERY_SECONDS;
        let last = 1_000_000;
        assert_eq!(
            restart_after(Some(false), Some(last), last + 60),
            Some(last + six),
            "an hour into the pause it starts at the pause's end"
        );
        assert_eq!(restart_after(Some(false), Some(last), last + six), None);
        assert_eq!(
            restart_after(Some(false), None, last),
            None,
            "never restarted"
        );
        assert_eq!(
            restart_after(Some(true), Some(last), last + 60),
            None,
            "urgent"
        );
        assert_eq!(
            restart_after(None, Some(last), last + 60),
            None,
            "nothing waits to start on its own"
        );
        assert_eq!(
            restart_after(Some(false), Some(last + 365 * 86_400), last),
            Some(last + six),
            "a time ahead of the clock holds one period from now"
        );
        // The status carries the field, null while nothing waits, even inside a pause.
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(RESTARTED_AT_FILE), now().to_string()).unwrap();
        let updates = Updates::new(Ok(()), Some(folder.path().to_owned()));
        let status = updates.status();
        assert!(status.get("restart_after").is_some_and(Value::is_null));
    }

    #[test]
    fn an_update_waiting_only_for_requests_drains_after_half_an_hour() {
        // 2026-10-18: 0.6.40 waited three hours behind two dozen agent sessions.
        assert!(!drain_due(
            DRAIN_AFTER_SECONDS - 1,
            Some("request_in_flight")
        ));
        assert!(drain_due(DRAIN_AFTER_SECONDS, Some("request_in_flight")));
        // Anything else that keeps it waits on: the person, a sign-in, the switch, a check.
        for other in [
            "window_visible",
            "sign_in_waiting",
            "updates_off",
            "needs_person",
            "checking",
            "owner_unavailable",
        ] {
            assert!(!drain_due(10 * DRAIN_AFTER_SECONDS, Some(other)), "{other}");
        }
        assert!(
            !drain_due(10 * DRAIN_AFTER_SECONDS, None),
            "idle starts the usual way"
        );
    }

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
    fn the_idle_watch_names_its_reason_exactly_when_it_is_not_idle() {
        // SB-105: every input that stops an idle start has a code, and none is invented.
        for bits in 0..32u8 {
            let [enabled, alone, hidden, checking, busy] =
                [0, 1, 2, 3, 4].map(|bit| bits & (1 << bit) != 0);
            let busy = busy.then_some("request_in_flight");
            let blocker = idle_blocker(enabled, alone, hidden, checking, busy);
            let (idle, _) = idle_moment(enabled, alone, hidden, busy.is_some() || checking, false);
            assert_eq!(blocker.is_none(), idle, "{bits:05b}");
        }
        assert_eq!(
            idle_blocker(false, false, false, true, Some("x")),
            Some("updates_off")
        );
        assert_eq!(
            idle_blocker(true, false, false, true, Some("x")),
            Some("needs_person")
        );
        assert_eq!(
            idle_blocker(true, true, false, true, Some("x")),
            Some("window_visible")
        );
        assert_eq!(
            idle_blocker(true, true, true, true, Some("x")),
            Some("checking")
        );
        assert_eq!(
            idle_blocker(true, true, true, false, Some("sign_in_waiting")),
            Some("sign_in_waiting")
        );
    }

    #[test]
    fn a_window_left_open_out_of_focus_stops_holding_an_update_back() {
        // SB-131: hidden or missing — nobody looking; focused — someone is; open and out of focus
        // — someone may glance back for half an hour, then it no longer holds the update.
        let t = 1_791_800_000;
        assert!(nobody_looking(Some(false), false, t, t));
        assert!(nobody_looking(None, false, t, t));
        assert!(!nobody_looking(Some(true), true, t - 86_400, t));
        assert!(!nobody_looking(
            Some(true),
            false,
            t - UNATTENDED_SECONDS + 1,
            t
        ));
        assert!(nobody_looking(Some(true), false, t - UNATTENDED_SECONDS, t));
    }

    #[test]
    fn checks_run_every_five_minutes_and_a_quiet_check_is_logged_once() {
        // Operator 2026-10-17: every PassionCode product checks every five minutes.
        assert_eq!(CHECK_EVERY, Duration::from_secs(300));
        assert_eq!(RETRY_AFTER_FAILURE, Duration::from_secs(300));
        assert!(logs_check("current"));
        assert!(!logs_check("current"));
        assert!(logs_check("failed"));
        assert!(logs_check("current"));
        assert!(logs_check("ready"));
    }

    #[test]
    fn a_check_that_finds_nothing_while_an_update_waits_is_logged_once() {
        assert!(logs_recheck("current"));
        assert!(!logs_recheck("current"));
        assert!(logs_recheck("check_failed"));
        assert!(logs_recheck("current"));
    }

    #[test]
    fn the_idle_watch_reports_its_reason_at_once_then_at_most_hourly() {
        // SB-105 on a fake clock: one look every five minutes for a day.
        let mut report = IdleReport::default();
        let look = IDLE_LOOK_EVERY.as_secs() as i64;
        let start = 1_791_800_000;
        let logged: Vec<i64> = (0..24 * 12)
            .map(|n| start + n * look)
            .filter(|&at| report.due(at))
            .collect();
        assert_eq!(
            logged.first(),
            Some(&start),
            "the first reason is logged at once"
        );
        assert_eq!(logged.len(), 24, "once an hour over a day: {logged:?}");
        assert!(logged.windows(2).all(|w| w[1] - w[0] >= IDLE_REPORT_EVERY));
        // A clock that steps back (a corrected wall clock) never logs twice in a row.
        let mut report = IdleReport::default();
        assert!(report.due(start));
        assert!(!report.due(start - 10));
    }

    #[test]
    fn a_newer_release_replaces_a_waiting_update_and_the_same_one_is_not_fetched_again() {
        // SB-105: running 0.6.15 with 0.6.16 waiting (the operator's Mac, 2026-10-12).
        let v = |s: &str| semver::Version::parse(s).unwrap();
        let (running, waiting) = (v("0.6.15"), v("0.6.16"));
        assert!(replaces(&running, Some(&waiting), &v("0.6.18")));
        assert!(
            !replaces(&running, Some(&waiting), &v("0.6.16")),
            "already waiting"
        );
        assert!(!replaces(&running, Some(&waiting), &v("0.6.15")));
        assert!(
            replaces(&running, None, &v("0.6.16")),
            "nothing waits: the plain rule"
        );
        assert!(!replaces(&running, None, &v("0.6.15")));
        assert!(
            !replaces(&v("0.7.0"), Some(&waiting), &v("0.6.18")),
            "never a downgrade"
        );
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
        let image = OsString::from("/home/u/Applications/Fabric-Switchboard.AppImage");
        assert_eq!(
            availability(exe, false, Some(&image)),
            Err(UNAVAILABLE_DEVELOPMENT)
        );
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(availability(exe, true, None), Ok(()));
        } else if cfg!(target_os = "linux") {
            // SB-88: the AppImage updates itself; a .deb install is told how it updates.
            assert_eq!(availability(exe, true, Some(&image)), Ok(()));
            assert_eq!(availability(exe, true, None), Err(UNAVAILABLE_PACKAGE));
            assert_eq!(
                availability(exe, true, Some(&OsString::new())),
                Err(UNAVAILABLE_PACKAGE)
            );
        } else {
            assert_eq!(availability(exe, true, None), Err(UNAVAILABLE_PLATFORM));
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_translocated_copy_is_told_to_move_to_applications() {
        let exe = Path::new("/private/var/folders/x/AppTranslocation/1234/d/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard");
        assert_eq!(availability(exe, true, None), Err(UNAVAILABLE_TRANSLOCATED));
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
