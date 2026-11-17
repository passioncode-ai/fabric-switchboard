//! SB-414 (studio-agent request bc06a63d, 2026-11-17): which copy of Switchboard owns the data
//! folder, and the window that does not.
//!
//! A computer can hold two copies: the app in `/Applications`, run by `switchboard serve` under a
//! LaunchDaemon, and a private copy in `~/Applications` the person opened. The first to start owns
//! the data folder; the other window finds the store busy, owns nothing and, before this, said
//! nothing about its own version (a private 0.7.111 window beside a 0.7.112 `serve` never
//! updated). Now:
//!
//! - the owner says what it is and which copy it runs from (`Operation::Status`: `owner_kind`,
//!   `owner_copy`), so the window can compare;
//! - a window that does not own the data folder records itself in `window-run.json` (0600: its
//!   pid, version, start time and copy), removed when it becomes the owner or exits, so the
//!   owner can name it (`switchboard app updates`);
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
    pub version: String,
    pub started_at: i64,
    /// The copy it runs from (`copy_of`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
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

/// This process's copy.
pub fn this_copy() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
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
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    /// `STILL_ACTIVE`: the exit code of a process that has not exited.
    const STILL_ACTIVE: u32 = 259;
    // SAFETY: a handle opened for a query only, closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let read = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        read && code == STILL_ACTIVE
    }
}
#[cfg(not(any(unix, windows)))]
fn alive(_: u32) -> bool {
    false
}

fn read_window(root: &Path) -> Option<WindowRun> {
    std::fs::read(root.join(WINDOW_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// The window found the store busy: it records itself, once per start.
pub fn note_window(root: &Path, now: i64) {
    let run = WindowRun {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        started_at: now,
        copy: this_copy().map(|c| c.to_string_lossy().into_owned()),
    };
    if let Ok(bytes) = serde_json::to_vec(&run) {
        let _ = switchboard_core::private_fs::private_write(&root.join(WINDOW_FILE), &bytes);
    }
}

/// The window became the owner, or exits: its record goes, only when it is this process's.
pub fn clear_window(root: &Path) {
    if read_window(root).is_some_and(|run| run.pid == std::process::id()) {
        let _ = std::fs::remove_file(root.join(WINDOW_FILE));
    }
}

/// The window that runs without owning the data folder, while its process runs.
pub fn window(root: &Path) -> Option<WindowRun> {
    read_window(root).filter(|run| alive(run.pid))
}

/// What `switchboard app updates` answers where no app owns the data folder: `owner` is
/// `Some(version)` when this process is a `serve` that owns it, `None` when nothing does (an
/// offline command). `serve_update` is the owner's own update loop, when it has one.
pub fn not_app_view(owner: Option<&str>, window: Option<&WindowRun>, serve_update: Value) -> Value {
    let window_version = window.map(|w| w.version.clone());
    let window_older = match (owner, window) {
        (Some(owner), Some(window)) => older(&window.version, owner),
        _ => false,
    };
    let where_ = if cfg!(target_os = "macos") {
        "open Switchboard from the Applications folder"
    } else {
        "open the installed Switchboard"
    };
    let message = match (owner, &window_version) {
        (Some(owner), Some(window)) if window_older => format!(
            "Switchboard runs here as switchboard serve, version {owner}, and keeps itself current. A window of version {window} is open beside it: quit it and {where_}."
        ),
        (Some(owner), _) => format!(
            "Switchboard runs here as switchboard serve, version {owner}, and keeps itself current. The app's update settings apply when the app runs Switchboard itself."
        ),
        (None, _) => "No Switchboard runs here now. Open Switchboard, then try again.".to_owned(),
    };
    let mut view = json!({
        "available": false,
        "enabled": false,
        "reason": if owner.is_some() { "not_owner" } else { "no_owner" },
        "owner": owner.map(|_| "serve"),
        "owner_version": owner,
        "window_version": window_version,
        "window_older": window_older,
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
            assert_eq!(copy_of(inside, Some(&image)), Path::new(&image));
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
    fn the_window_record_is_its_own_and_goes_with_it() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(window(dir.path()), None);
        note_window(dir.path(), 100);
        let run = window(dir.path()).expect("this process runs");
        assert_eq!(run.pid, std::process::id());
        assert_eq!(run.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(run.started_at, 100);
        // Another process's record is left alone.
        let other = WindowRun {
            pid: std::process::id() + 1,
            version: "0.0.1".into(),
            started_at: 1,
            copy: None,
        };
        std::fs::write(
            dir.path().join(WINDOW_FILE),
            serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();
        clear_window(dir.path());
        assert!(dir.path().join(WINDOW_FILE).exists());
        note_window(dir.path(), 200);
        clear_window(dir.path());
        assert!(!dir.path().join(WINDOW_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_record_whose_process_is_gone_names_no_window() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        let gone = WindowRun {
            pid,
            version: "0.7.111".into(),
            started_at: 1,
            copy: None,
        };
        std::fs::write(
            dir.path().join(WINDOW_FILE),
            serde_json::to_vec(&gone).unwrap(),
        )
        .unwrap();
        assert_eq!(window(dir.path()), None);
    }

    #[test]
    fn app_updates_without_the_app_says_who_owns_the_data_folder() {
        let window = WindowRun {
            pid: 1,
            version: "0.7.111".into(),
            started_at: 1,
            copy: None,
        };
        let state = json!({"state": "current"});
        let view = not_app_view(Some("0.7.112"), Some(&window), state.clone());
        assert_eq!(view["available"], false);
        assert_eq!(view["reason"], "not_owner");
        assert_eq!(view["owner"], "serve");
        assert_eq!(view["owner_version"], "0.7.112");
        assert_eq!(view["window_version"], "0.7.111");
        assert_eq!(view["window_older"], true);
        assert_eq!(view["serve_update"], state);
        assert!(view["message"].as_str().unwrap().contains("quit it"));
        let alone = not_app_view(Some("0.7.112"), None, Value::Null);
        assert_eq!(alone["window_version"], Value::Null);
        assert_eq!(alone["window_older"], false);
        assert!(alone.get("serve_update").is_none());
        let none = not_app_view(None, None, Value::Null);
        assert_eq!(none["reason"], "no_owner");
        assert_eq!(none["owner"], Value::Null);
        assert!(!none["message"].as_str().unwrap().contains('\u{2014}'));
    }
}
