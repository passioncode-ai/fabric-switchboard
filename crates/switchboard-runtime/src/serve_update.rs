//! SB-333 (operator 2026-11-11: «мы обновляем весь софт автоматически, софт должен обновляться
//! везде. И на студиях ... и у сотрудников»): `switchboard serve` keeps itself current as the
//! installed app does (DEC-0004: a check every five minutes). Before, a computer that runs only
//! `serve` (the Mac Studio's three accounts under LaunchDaemons) stayed on the version it was
//! installed with until someone ran `switchboard update` by hand (SB-305), and three `serve`s
//! sharing one app kept running the old binary after it was replaced.
//!
//! Each pass: if the program file this process runs from was replaced (another `serve`, the
//! app, an administrator's `switchboard update`), `serve` restarts onto it; else it reads the
//! feed and, when a newer version exists and this account may write the app, installs it and
//! restarts. An app it may not write is logged once per version (`update_waiting
//! not_writable`): the account that owns it runs `switchboard update`, and every `serve` then
//! follows by the first rule. `<data>/auto-update` set to `off` installs nothing; a replaced
//! program is still followed.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The first look waits for start-up to settle, as the app's does.
pub const FIRST_AFTER: Duration = Duration::from_secs(90);
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// The exit status that asks the supervisor to start `serve` again on the new program: not 0,
/// which a LaunchDaemon with `KeepAlive {SuccessfulExit: false}` (or systemd `Restart=on-failure`)
/// takes as "stay stopped"; `EX_TEMPFAIL`.
pub const RESTART_STATUS: i32 = 75;

/// The identity of a program file: device, inode, length and modification time. A replacement
/// by rename (an update) changes it; the running process keeps its old inode open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp(u64, u64, u64, i64);

pub fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(Stamp(m.dev(), m.ino(), m.len(), m.mtime()))
    }
    #[cfg(not(unix))]
    {
        let mtime = m
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some(Stamp(0, 0, m.len(), mtime))
    }
}

/// What one pass decided.
#[derive(Debug, PartialEq, Eq)]
pub enum Pass {
    /// Nothing to do now.
    Stay,
    /// Restart onto the program on disk: `replaced` (someone else installed it) or `installed`
    /// (this pass did).
    Restart(&'static str),
}

/// Whether the program this process runs from was replaced since `at_start`.
pub fn replaced(program: &Path, at_start: &Stamp) -> bool {
    stamp(program).is_some_and(|now| &now != at_start)
}

/// One pass: follow a replaced program, else check and install. `version` is this binary's.
pub async fn pass(
    data: &Path,
    program: &Path,
    at_start: &Stamp,
    version: &str,
    told: &mut Option<String>,
) -> Pass {
    if replaced(program, at_start) {
        crate::oplog::event(
            "update_restart",
            &[("outcome", crate::oplog::Field::Code("replaced"))],
        );
        return Pass::Restart("replaced");
    }
    let install = auto_update_on(data);
    // Asked first without installing: an app this account may not replace is said before a
    // 30 MB package is downloaded for nothing every five minutes.
    let check_only = !install
        || match crate::self_update::run(None, true, version, None).await {
            Ok(v) if v["action"] == "available" => !v["app"]
                .as_str()
                .is_some_and(|app| may_replace(Path::new(app))),
            _ => false,
        };
    if install && check_only {
        if told.as_deref() != Some("not_writable") {
            crate::oplog::event(
                "update_waiting",
                &[("reason", crate::oplog::Field::Code("not_writable"))],
            );
            *told = Some("not_writable".to_owned());
        }
        return Pass::Stay;
    }
    match crate::self_update::run(None, check_only, version, None).await {
        Ok(v) if v["action"] == "installed" => {
            crate::oplog::event(
                "update_restart",
                &[("outcome", crate::oplog::Field::Code("installed"))],
            );
            Pass::Restart("installed")
        }
        Ok(v) if v["action"] == "available" => {
            let latest = v["latest"].as_str().unwrap_or_default().to_owned();
            if told.as_deref() != Some(latest.as_str()) {
                crate::oplog::event(
                    "update_waiting",
                    &[("reason", crate::oplog::Field::Code("auto_update_off"))],
                );
                *told = Some(latest);
            }
            Pass::Stay
        }
        Ok(_) => Pass::Stay,
        Err(error) => {
            let reason = if error == crate::self_update::NOT_WRITABLE {
                "not_writable"
            } else if error == crate::self_update::FEED_UNREADABLE {
                "feed_unreadable"
            } else if error == crate::self_update::DOWNLOAD_FAILED {
                "download_failed"
            } else if error == crate::self_update::HELD {
                "held"
            } else {
                "failed"
            };
            // Said once per reason until it changes, so a computer that cannot reach the feed
            // does not log every five minutes.
            if told.as_deref() != Some(reason) {
                crate::oplog::event(
                    "update_waiting",
                    &[("reason", crate::oplog::Field::Code(reason))],
                );
                *told = Some(reason.to_owned());
            }
            Pass::Stay
        }
    }
}

/// Whether this account may replace `app` by rename: write access to the folder that holds it
/// and, for a bundle moved to another folder while it is replaced, to the bundle itself.
pub fn may_replace(app: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let writable = |p: &Path| {
            std::ffi::CString::new(p.as_os_str().as_bytes())
                // SAFETY: a NUL-terminated path; access only reads it.
                .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::W_OK) } == 0)
        };
        app.parent().is_some_and(writable) && (!app.is_dir() || writable(app))
    }
    #[cfg(not(unix))]
    {
        let _ = app;
        true
    }
}

/// `<data>/auto-update`: absent or anything but `off` is on, as the app reads it.
pub fn auto_update_on(data: &Path) -> bool {
    std::fs::read_to_string(data.join("auto-update"))
        .map(|c| c.trim() != "off")
        .unwrap_or(true)
}

/// The program file to watch: this process's executable, only when it is an installed copy (in
/// an app bundle on macOS, an AppImage on Linux); a development build or a test's binary never
/// updates itself, as the app's own check never runs there.
pub fn program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let installed = if cfg!(target_os = "macos") {
        crate::self_update::bundle_of(&exe).is_some()
    } else if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some_and(|p| !p.is_empty())
    } else {
        false
    };
    installed.then_some(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_replaced_by_rename_reads_as_replaced_and_an_untouched_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("switchboard");
        std::fs::write(&program, b"old").unwrap();
        let at_start = stamp(&program).unwrap();
        assert!(!replaced(&program, &at_start));
        let next = dir.path().join("switchboard.new");
        std::fs::write(&next, b"newer").unwrap();
        std::fs::rename(&next, &program).unwrap();
        assert!(replaced(&program, &at_start));
        assert!(
            !replaced(&dir.path().join("gone"), &at_start),
            "a program not there is not followed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_app_in_a_folder_this_account_cannot_write_is_not_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("Applications");
        let app = apps.join("Fabric Switchboard.app");
        std::fs::create_dir_all(&app).unwrap();
        assert!(may_replace(&app));
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o555)).unwrap();
        let refused = !may_replace(&app);
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o755)).unwrap();
        // root may write anywhere; the check is about an ordinary account.
        if unsafe { libc::getuid() } != 0 {
            assert!(refused);
        }
    }

    #[test]
    fn auto_update_is_on_unless_the_file_says_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(auto_update_on(dir.path()));
        std::fs::write(dir.path().join("auto-update"), "off\n").unwrap();
        assert!(!auto_update_on(dir.path()));
        std::fs::write(dir.path().join("auto-update"), "on\n").unwrap();
        assert!(auto_update_on(dir.path()));
    }

    #[tokio::test]
    async fn a_replaced_program_restarts_before_any_feed_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("switchboard");
        std::fs::write(&program, b"old").unwrap();
        let at_start = stamp(&program).unwrap();
        let next = dir.path().join("n");
        std::fs::write(&next, b"newer").unwrap();
        std::fs::rename(&next, &program).unwrap();
        let mut told = None;
        assert_eq!(
            pass(dir.path(), &program, &at_start, "0.0.1", &mut told).await,
            Pass::Restart("replaced")
        );
    }
}
