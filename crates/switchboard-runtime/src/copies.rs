//! SB-414 (studio-agent request bc06a63d, 2026-11-17): which copy of Switchboard owns the data
//! folder, and the windows that do not.
//!
//! A computer can hold two copies: the app in `/Applications`, run by `switchboard serve` under a
//! LaunchDaemon, and a private copy in `~/Applications` the person opened. The first to start owns
//! the data folder; the other window finds the store busy, owns nothing and, before this, said
//! nothing about its own version (a private 0.7.111 window beside a 0.7.112 `serve` never
//! updated). Now:
//!
//! - the owner says what it is, which copy it runs from and whether it keeps that copy current
//!   (`Operation::Status`: `owner_kind`, `owner_copy`, `owner_updates_copy`), so the window can
//!   compare;
//! - a window that does not own the data folder records itself in `window-run.json` (0600: one
//!   record per window, its pid, the process's start token, version, start time and copy),
//!   removed when it becomes the owner or exits, so the owner can name it (`switchboard app
//!   updates`);
//! - `switchboard app updates` answers what it knows instead of a bare refusal where no app
//!   owns the data folder (`not_app_view`).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const WINDOW_FILE: &str = "window-run.json";

/// A window that runs without owning the data folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowRun {
    pub pid: u32,
    /// When the process started, as the system counts it (`process_token`): a pid the system
    /// gave to another process later reads as another token. Absent where it cannot be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<u64>,
    pub version: String,
    pub started_at: i64,
    /// The copy it runs from (`copy_of`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Windows {
    #[serde(default)]
    windows: Vec<WindowRun>,
}

/// The installed copy a program runs from: the `.app` bundle on macOS, the AppImage on Linux when
/// it runs from one, otherwise the folder that holds the program (the .deb's `/usr/bin`, the
/// Windows install folder, where the CLI sits beside the app).
pub fn copy_of(exe: &Path, appimage: Option<&OsStr>) -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(bundle) = crate::self_update::bundle_of(exe) {
            return bundle;
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(image) = appimage.filter(|i| !i.is_empty()) {
            return PathBuf::from(image);
        }
    }
    exe.parent().map_or_else(|| exe.to_owned(), Path::to_owned)
}

/// This process's program file, resolved: a `serve` started through a link (`~/.local/bin/
/// switchboard`) is the program inside its bundle.
pub fn this_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(std::fs::canonicalize(&exe).unwrap_or(exe))
}

/// This process's copy.
pub fn this_copy() -> Option<PathBuf> {
    let exe = this_program()?;
    Some(copy_of(&exe, std::env::var_os("APPIMAGE").as_deref()))
}

/// Whether two copies are one: equal once both are resolved (a link or `..` does not make two).
pub fn same_copy(a: &Path, b: &Path) -> bool {
    let resolve = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    resolve(a) == resolve(b)
}

/// Whether version `mine` is older than `theirs`; an unreadable one is never older.
pub fn older(mine: &str, theirs: &str) -> bool {
    match (
        semver::Version::parse(mine.trim_start_matches('v')),
        semver::Version::parse(theirs.trim_start_matches('v')),
    ) {
        (Ok(mine), Ok(theirs)) => mine < theirs,
        _ => false,
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only asks whether the process exists; EPERM means it does.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    windows_process(pid).is_some_and(|(running, _)| running)
}
#[cfg(not(any(unix, windows)))]
fn alive(_: u32) -> bool {
    false
}

/// Whether the process runs, and its creation time (FILETIME ticks).
#[cfg(windows)]
fn windows_process(pid: u32) -> Option<(bool, u64)> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    /// `STILL_ACTIVE`: the exit code of a process that has not exited.
    const STILL_ACTIVE: u32 = 259;
    let zero = || FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: a handle opened for a query only, closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut code = 0u32;
        let read = GetExitCodeProcess(handle, &mut code) != 0;
        let (mut created, mut exited, mut kernel, mut user) = (zero(), zero(), zero(), zero());
        let timed = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        if !read {
            return None;
        }
        let token = if timed {
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
        } else {
            0
        };
        Some((code == STILL_ACTIVE, token))
    }
}

/// When process `pid` started, as an opaque number that stays the same for the process's life and
/// differs for a later process given the same pid: macOS microseconds since 1970
/// (`proc_pidinfo`), Linux clock ticks since boot (`/proc/<pid>/stat`), Windows its creation time.
pub fn process_token(pid: u32) -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        crate::launch::process_started(pid)
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_micros()).ok())
    }
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // After the command name in parentheses, field 22 (`starttime`) is the 20th.
        let rest = &stat[stat.rfind(')')? + 1..];
        rest.split_whitespace().nth(19)?.parse().ok()
    }
    #[cfg(windows)]
    {
        windows_process(pid)
            .map(|(_, token)| token)
            .filter(|t| *t != 0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = pid;
        None
    }
}

/// Whether a record names a process that still runs: alive, and the same process when a start
/// token was recorded and can be read now.
fn runs(run: &WindowRun) -> bool {
    alive(run.pid)
        && match (run.token, process_token(run.pid)) {
            (Some(recorded), Some(now)) => recorded == now,
            _ => true,
        }
}

fn read_windows(root: &Path) -> Windows {
    std::fs::read(root.join(WINDOW_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_windows(root: &Path, windows: &Windows) {
    let path = root.join(WINDOW_FILE);
    if windows.windows.is_empty() {
        let _ = std::fs::remove_file(path);
    } else if let Ok(bytes) = serde_json::to_vec(windows) {
        let _ = switchboard_core::private_fs::private_write(&path, &bytes);
    }
}

/// The window found the store busy: it records itself beside the other windows that still run.
pub fn note_window(root: &Path, now: i64) {
    let me = std::process::id();
    let mut all = read_windows(root);
    all.windows.retain(|w| w.pid != me && runs(w));
    all.windows.push(WindowRun {
        pid: me,
        token: process_token(me),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        started_at: now,
        copy: this_copy().map(|c| c.to_string_lossy().into_owned()),
    });
    write_windows(root, &all);
}

/// The window became the owner, or exits: its own record goes, and records of processes gone.
pub fn clear_window(root: &Path) {
    let me = std::process::id();
    let all = read_windows(root);
    if all.windows.is_empty() {
        return;
    }
    let before = all.windows.len();
    let kept = Windows {
        windows: all
            .windows
            .into_iter()
            .filter(|w| w.pid != me && runs(w))
            .collect(),
    };
    if kept.windows.len() != before {
        write_windows(root, &kept);
    }
}

/// The windows that run without owning the data folder, while their processes run.
pub fn windows(root: &Path) -> Vec<WindowRun> {
    read_windows(root)
        .windows
        .into_iter()
        .filter(runs)
        .collect()
}

/// What `switchboard app updates` answers where no app owns the data folder: `owner` is
/// `Some(version)` when this process is a `serve` that owns it, `None` when nothing does (an
/// offline command); `updates_copy` whether that `serve` keeps its copy current
/// (`serve_update::updates_copy`). `serve_update` is its own update loop's last pass.
pub fn not_app_view(
    owner: Option<&str>,
    updates_copy: bool,
    windows: &[WindowRun],
    serve_update: Value,
) -> Value {
    // The oldest window is the one the person needs to hear about.
    let oldest = windows
        .iter()
        .map(|w| w.version.as_str())
        .reduce(|a, b| if older(b, a) { b } else { a });
    let window_older = match (owner, oldest) {
        (Some(owner), Some(window)) => older(window, owner),
        _ => false,
    };
    let where_ = if cfg!(target_os = "macos") {
        "open Switchboard from the Applications folder"
    } else {
        "open the installed Switchboard"
    };
    let keeps = if updates_copy {
        ", and keeps itself current"
    } else {
        ""
    };
    let message = match (owner, oldest) {
        (Some(owner), Some(window)) if window_older => format!(
            "Switchboard runs here as switchboard serve, version {owner}{keeps}. A window of version {window} is open beside it: quit it and {where_}."
        ),
        (Some(owner), _) if updates_copy => format!(
            "Switchboard runs here as switchboard serve, version {owner}{keeps}. The app's update settings apply when the app runs Switchboard itself."
        ),
        (Some(owner), _) => format!(
            "Switchboard runs here as switchboard serve, version {owner}, which does not update this copy itself here: run switchboard update as the account that owns the app, or open the app."
        ),
        (None, _) => "No Switchboard runs here now. Open Switchboard, then try again.".to_owned(),
    };
    let mut view = json!({
        "available": false,
        "enabled": false,
        "reason": if owner.is_some() { "not_owner" } else { "no_owner" },
        "owner": owner.map(|_| "serve"),
        "owner_version": owner,
        "owner_updates_copy": owner.is_some() && updates_copy,
        "window_version": oldest,
        "window_older": window_older,
        "windows": windows.iter().map(|w| json!({"pid": w.pid, "version": w.version})).collect::<Vec<_>>(),
        "message": message,
    });
    if owner.is_some() && !serve_update.is_null() {
        view["serve_update"] = serve_update;
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pid: u32, version: &str) -> WindowRun {
        WindowRun {
            pid,
            token: None,
            version: version.into(),
            started_at: 1,
            copy: None,
        }
    }

    #[test]
    fn a_copy_is_the_bundle_the_appimage_or_the_programs_folder() {
        let exe = Path::new("/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard");
        let copy = copy_of(exe, None);
        if cfg!(target_os = "macos") {
            assert_eq!(copy, Path::new("/Applications/Fabric Switchboard.app"));
        } else {
            assert_eq!(
                copy,
                Path::new("/Applications/Fabric Switchboard.app/Contents/MacOS")
            );
        }
        let image = std::ffi::OsString::from("/home/u/Fabric-Switchboard.AppImage");
        let inside = Path::new("/tmp/.mount_x/usr/bin/fabric-switchboard");
        if cfg!(target_os = "linux") {
            assert_eq!(copy_of(inside, Some(image.as_os_str())), Path::new(&image));
            assert_eq!(
                copy_of(inside, Some(OsStr::new(""))),
                Path::new("/tmp/.mount_x/usr/bin")
            );
        }
        assert_eq!(
            copy_of(Path::new("/usr/bin/switchboard"), None),
            Path::new("/usr/bin")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_reached_through_a_link_is_the_same_copy() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Fabric Switchboard.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/switchboard"), b"x").unwrap();
        let link = dir.path().join("switchboard");
        std::os::unix::fs::symlink(bundle.join("Contents/MacOS/switchboard"), &link).unwrap();
        let resolved = std::fs::canonicalize(&link).unwrap();
        let copy = copy_of(&resolved, None);
        if cfg!(target_os = "macos") {
            assert!(same_copy(&copy, &bundle));
        }
        assert!(this_program().is_some_and(|p| p.is_absolute()));
    }

    #[test]
    fn versions_compare_by_semver_and_an_unreadable_one_is_never_older() {
        assert!(older("0.7.111", "0.7.112"));
        assert!(older("0.7.9", "0.7.10"));
        assert!(!older("0.7.112", "0.7.112"));
        assert!(!older("0.7.113", "0.7.112"));
        assert!(!older("garbage", "0.7.112"));
        assert!(older("v0.7.1", "0.7.2"));
    }

    #[test]
    fn this_process_has_a_start_token_that_does_not_change() {
        let me = std::process::id();
        if cfg!(any(target_os = "macos", target_os = "linux", windows)) {
            let token = process_token(me).expect("a token for this process");
            assert_eq!(process_token(me), Some(token));
        }
    }

    #[test]
    fn each_window_keeps_its_own_record_and_takes_only_its_own_away() {
        let dir = tempfile::tempdir().unwrap();
        assert!(windows(dir.path()).is_empty());
        // Another window that still runs (this test process's parent stands in for it).
        #[cfg(unix)]
        let other = {
            let parent = std::os::unix::process::parent_id();
            WindowRun {
                token: process_token(parent),
                ..run(parent, "0.7.111")
            }
        };
        #[cfg(not(unix))]
        let other = run(0, "0.7.111");
        write_windows(
            dir.path(),
            &Windows {
                windows: vec![other.clone()],
            },
        );
        note_window(dir.path(), 100);
        let seen = windows(dir.path());
        let me = seen
            .iter()
            .find(|w| w.pid == std::process::id())
            .expect("this process runs");
        assert_eq!(me.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(me.started_at, 100);
        if cfg!(unix) {
            assert_eq!(seen.len(), 2, "two windows, one record each: {seen:?}");
        }
        // Noting again replaces this window's record, never adds a second one.
        note_window(dir.path(), 200);
        assert_eq!(
            windows(dir.path())
                .iter()
                .filter(|w| w.pid == std::process::id())
                .count(),
            1
        );
        clear_window(dir.path());
        let left = windows(dir.path());
        assert!(left.iter().all(|w| w.pid != std::process::id()));
        if cfg!(unix) {
            assert_eq!(left, [other]);
        } else {
            assert!(!dir.path().join(WINDOW_FILE).exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_record_whose_process_is_gone_or_was_replaced_names_no_window() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        // A pid given to another process later: the start token differs.
        let me = std::process::id();
        let reused = WindowRun {
            token: process_token(me).map(|t| t + 1),
            ..run(me, "0.7.110")
        };
        write_windows(
            dir.path(),
            &Windows {
                windows: vec![run(pid, "0.7.111"), reused],
            },
        );
        if process_token(me).is_some() {
            assert!(windows(dir.path()).is_empty());
        }
    }

    #[test]
    fn app_updates_without_the_app_says_who_owns_the_data_folder() {
        let state = json!({"state": "current"});
        let view = not_app_view(
            Some("0.7.112"),
            true,
            &[run(1, "0.7.111"), run(2, "0.7.110")],
            state.clone(),
        );
        assert_eq!(view["available"], false);
        assert_eq!(view["reason"], "not_owner");
        assert_eq!(view["owner"], "serve");
        assert_eq!(view["owner_version"], "0.7.112");
        assert_eq!(view["owner_updates_copy"], true);
        assert_eq!(view["window_version"], "0.7.110", "the oldest window");
        assert_eq!(view["window_older"], true);
        assert_eq!(view["windows"].as_array().unwrap().len(), 2);
        assert_eq!(view["serve_update"], state);
        let message = view["message"].as_str().unwrap();
        assert!(message.contains("quit it") && message.contains("keeps itself current"));
        let alone = not_app_view(Some("0.7.112"), true, &[], Value::Null);
        assert_eq!(alone["window_version"], Value::Null);
        assert_eq!(alone["window_older"], false);
        assert!(alone.get("serve_update").is_none());
        // A serve that does not update its copy never says it keeps itself current.
        let stuck = not_app_view(Some("0.7.112"), false, &[], Value::Null);
        assert_eq!(stuck["owner_updates_copy"], false);
        assert!(!stuck["message"]
            .as_str()
            .unwrap()
            .contains("keeps itself current"));
        assert!(stuck["message"]
            .as_str()
            .unwrap()
            .contains("switchboard update"));
        let none = not_app_view(None, false, &[], Value::Null);
        assert_eq!(none["reason"], "no_owner");
        assert_eq!(none["owner"], Value::Null);
        for v in [&view, &alone, &stuck, &none] {
            assert!(!v["message"].as_str().unwrap().contains('\u{2014}'));
        }
    }
}
