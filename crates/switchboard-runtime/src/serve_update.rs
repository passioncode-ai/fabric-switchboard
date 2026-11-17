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

/// How long a restart onto a new program waits for a quiet moment before it drains anyway, and
/// how often it looks (the app's own watch waits as long, `DRAIN_AFTER_SECONDS`).
pub const QUIET_WAIT: Duration = Duration::from_secs(30 * 60);
pub const QUIET_LOOK: Duration = Duration::from_secs(30);

/// Waits until `busy` answers nothing on two looks in a row (no managed request in flight or
/// started in the last two minutes, no sign-in waiting), at most `max`; what kept it, the first
/// time, is logged (`update_waiting`). Before 0.7.96 `serve` restarted at once and a managed
/// session's next request met a closed port (studio-agent's finding `b18dd31c`).
pub async fn wait_quiet(
    busy: impl Fn() -> Option<&'static str>,
    look: Duration,
    max: Duration,
) -> bool {
    let started = std::time::Instant::now();
    let mut quiet_before = false;
    let mut told = false;
    loop {
        match busy() {
            None if quiet_before => return true,
            None => quiet_before = true,
            Some(reason) => {
                quiet_before = false;
                if !told {
                    crate::oplog::event(
                        "update_waiting",
                        &[("reason", crate::oplog::Field::Code(reason))],
                    );
                    told = true;
                }
            }
        }
        if started.elapsed() >= max {
            return false;
        }
        tokio::time::sleep(look).await;
    }
}

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

/// SB-414: what this `serve`'s last pass found, for `switchboard app updates` answered by it:
/// `{state: current|available|waiting|restarting, latest, reason, checked_at}`; null before the
/// first pass or where `serve` does not update itself.
static LAST: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

fn note(state: &str, latest: Option<&str>, reason: Option<&str>) {
    let checked_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if let Ok(mut last) = LAST.lock() {
        *last = Some(serde_json::json!({
            "state": state, "latest": latest, "reason": reason, "checked_at": checked_at,
        }));
    }
}

/// The last pass of this process (`note`), with whether `<data>/auto-update` lets it install.
pub fn last(data: &std::path::Path) -> serde_json::Value {
    let mut value = LAST
        .lock()
        .ok()
        .and_then(|last| last.clone())
        .unwrap_or_else(|| serde_json::json!({"state": null}));
    value["auto_update"] = auto_update_on(data).into();
    value
}

/// When this `serve`'s last update pass succeeded: it read the feed and found nothing newer, or
/// installed what it found (SB-414, review round 2).
static LAST_OK: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// How long a successful pass stands for `updates_copy`: three passes' worth, so one feed that
/// could not be read does not hand the copy to a window at once.
pub const UPDATES_COPY_FOR: Duration = Duration::from_secs(15 * 60);

fn note_ok() {
    if let Ok(mut last) = LAST_OK.lock() {
        *last = Some(std::time::Instant::now());
    }
}

/// When this `serve`'s update loop started (`note_started`): until its first pass, a window from
/// the same copy holds back for `UPDATES_COPY_FOR` rather than install beside it (review round 3).
static STARTED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// The update loop of an installed `serve` starts: called once, before its first pass waits.
pub fn note_started() {
    if let Ok(mut started) = STARTED.lock() {
        started.get_or_insert_with(std::time::Instant::now);
    }
}

/// SB-414: whether this `serve` keeps the copy it runs from current: it updates itself at all
/// (`program`: an installed copy, never on Windows), `<data>/auto-update` is not `off`, and a pass
/// succeeded within `UPDATES_COPY_FOR`, or no pass has succeeded yet and its update loop started
/// within `UPDATES_COPY_FOR` (its first pass comes 90 s after start: a window holds back for it
/// rather than install beside it). A loop whose passes failed for that long, or none that
/// started, does not. A window from the same copy installs updates itself only when this is false.
pub fn updates_copy(data: &Path) -> bool {
    let last_ok = LAST_OK.lock().ok().and_then(|last| *last);
    let started = STARTED.lock().ok().and_then(|started| *started);
    updates_copy_at(
        program().is_some(),
        auto_update_on(data),
        last_ok,
        started,
        std::time::Instant::now(),
    )
}

/// `updates_copy` with its inputs named, for a test with a clock of its own.
pub fn updates_copy_at(
    installed: bool,
    auto_update: bool,
    last_ok: Option<std::time::Instant>,
    started: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    let recent = |at: std::time::Instant| now.saturating_duration_since(at) <= UPDATES_COPY_FOR;
    installed
        && auto_update
        && match last_ok {
            Some(at) => recent(at),
            None => started.is_some_and(recent),
        }
}

/// SB-408 (review rounds 2 and 3): whether a supervisor starts this process again after it exits
/// with `RESTART_STATUS`, read from what only the supervisor sets for the process it started:
///
/// - systemd (248 and later) sets `SYSTEMD_EXEC_PID` to the pid of the process it started; a
///   child inherits the variable but not the pid, so only the unit's own main process matches
///   (`INVOCATION_ID` is not used: a shell in a terminal and everything it starts inherit it).
/// - launchd starts a job's process with `XPC_SERVICE_NAME` set to the job's label and itself as
///   the parent; a Terminal shell carries `0`, an app and what it starts an `application.` label,
///   and a child of a job has the job's process as its parent.
///
/// Init as the parent alone is not supervision: `(switchboard serve &)`, `nohup … &`, `setsid -f`
/// and a cron `@reboot … &` all end there with nothing to start them again. An older systemd, a
/// unit that starts `serve` through a shell, Windows and every other start read unsupervised, the
/// safe side: such a `serve` keeps running on the old program.
pub fn supervised(
    pid: u32,
    systemd_exec_pid: Option<&std::ffi::OsStr>,
    xpc_service: Option<&std::ffi::OsStr>,
    parent: Option<u32>,
) -> bool {
    let systemd = systemd_exec_pid
        .and_then(|v| v.to_str())
        .and_then(|v| v.trim().parse::<u32>().ok())
        == Some(pid);
    let launchd = xpc_service
        .and_then(|v| v.to_str())
        .is_some_and(|name| !name.is_empty() && name != "0" && !name.starts_with("application."))
        && parent == Some(1);
    systemd || launchd
}

/// `supervised` for this process.
pub fn supervised_now() -> bool {
    #[cfg(unix)]
    let parent = Some(std::os::unix::process::parent_id());
    #[cfg(not(unix))]
    let parent = None;
    supervised(
        std::process::id(),
        std::env::var_os("SYSTEMD_EXEC_PID").as_deref(),
        std::env::var_os("XPC_SERVICE_NAME").as_deref(),
        parent,
    )
}

/// What a `serve`'s update passes remember: the reason or version last said, and whether an
/// unsupervised restart was said.
#[derive(Debug, Default)]
pub struct Told {
    pub said: Option<String>,
    pub unsupervised: bool,
}

/// A pass that would restart `serve` (`why`: `replaced` or `installed`): under a supervisor it
/// logs `update_restart <why>` and restarts; unsupervised it logs `update_restart unsupervised`
/// once and keeps running on the old program, since exiting would stop it for good (review
/// round 3).
fn restart(why: &'static str, latest: Option<&str>, supervised: bool, told: &mut Told) -> Pass {
    if supervised {
        crate::oplog::event(
            "update_restart",
            &[("outcome", crate::oplog::Field::Code(why))],
        );
        note("restarting", latest, Some(why));
        return Pass::Restart(why);
    }
    if !told.unsupervised {
        crate::oplog::event(
            "update_restart",
            &[("outcome", crate::oplog::Field::Code("unsupervised"))],
        );
        told.unsupervised = true;
    }
    note("waiting", latest, Some("unsupervised"));
    Pass::Stay
}

/// One pass: follow a replaced program, else check and install. `version` is this binary's.
pub async fn pass(
    data: &Path,
    program: &Path,
    at_start: &Stamp,
    version: &str,
    supervised: bool,
    told_all: &mut Told,
) -> Pass {
    if replaced(program, at_start) {
        return restart("replaced", None, supervised, told_all);
    }
    let told = &mut told_all.said;
    let install = auto_update_on(data);
    // Asked first without installing: an app this account may not replace is said before a
    // 30 MB package is downloaded for nothing every five minutes.
    let mut offered = None;
    let check_only = !install
        || match crate::self_update::run(None, true, version, None).await {
            Ok(v) if v["action"] == "available" => {
                offered = v["latest"].as_str().map(str::to_owned);
                !v["app"]
                    .as_str()
                    .is_some_and(|app| may_replace(Path::new(app)))
            }
            _ => false,
        };
    if install && check_only {
        note("waiting", offered.as_deref(), Some("not_writable"));
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
            note_ok();
            restart("installed", v["latest"].as_str(), supervised, told_all)
        }
        Ok(v) if v["action"] == "available" => {
            let latest = v["latest"].as_str().unwrap_or_default().to_owned();
            note("available", Some(latest.as_str()), Some("auto_update_off"));
            if told.as_deref() != Some(latest.as_str()) {
                crate::oplog::event(
                    "update_waiting",
                    &[("reason", crate::oplog::Field::Code("auto_update_off"))],
                );
                *told = Some(latest);
            }
            Pass::Stay
        }
        Ok(v) => {
            note("current", v["latest"].as_str(), None);
            note_ok();
            Pass::Stay
        }
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
            note("waiting", None, Some(reason));
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

/// SB-408: whether a .deb install's pass says a version: once per newer version (`told` keeps
/// the last one said); the same or an older one, or a feed that could not be read, says nothing.
pub fn deb_tell(notice: &Result<serde_json::Value, String>, told: &mut Option<String>) -> bool {
    let Ok(notice) = notice else {
        return false;
    };
    let Some(latest) = notice["latest"]
        .as_str()
        .filter(|_| notice["action"] == "available")
    else {
        return false;
    };
    if told.as_deref() == Some(latest) {
        return false;
    }
    *told = Some(latest.to_owned());
    true
}

/// SB-408: one pass of a `serve` installed from the .deb package, which never replaces its own
/// files: a program apt replaced since `at_start` (the person installed the newer .deb) restarts
/// `serve` onto it, as a replaced app does, when a supervisor will start it again (`supervised`);
/// unsupervised, it logs `update_restart unsupervised` once and keeps running. Else it reads the
/// feed and logs `update_waiting deb` once per newer version; the person installs it with the
/// command `switchboard update --check` prints.
pub async fn deb_pass(
    program: &Path,
    at_start: &Stamp,
    version: &str,
    supervised: bool,
    told: &mut Told,
) -> Pass {
    if replaced(program, at_start) {
        return restart("replaced", None, supervised, told);
    }
    let told = &mut told.said;
    let notice = crate::deb::check(version).await;
    match &notice {
        Ok(v) if v["action"] == "available" => note("available", v["latest"].as_str(), Some("deb")),
        Ok(v) => note("current", v["latest"].as_str(), Some("deb")),
        Err(_) => note("waiting", None, Some("feed_unreadable")),
    }
    if deb_tell(&notice, told) {
        crate::oplog::event(
            "update_waiting",
            &[("reason", crate::oplog::Field::Code("deb"))],
        );
    }
    Pass::Stay
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
    // Resolved, so a `serve` started through a link (`~/.local/bin/switchboard`) watches and
    // names the program inside its bundle (SB-414).
    let exe = crate::copies::this_program()?;
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
        let mut told = Told::default();
        assert_eq!(
            pass(dir.path(), &program, &at_start, "0.0.1", true, &mut told).await,
            Pass::Restart("replaced")
        );
        // Review round 3: started by hand, it keeps running on the old program, says so once and
        // reads no feed (an exit would stop it for good).
        let mut told = Told::default();
        for _ in 0..2 {
            assert_eq!(
                pass(dir.path(), &program, &at_start, "0.0.1", false, &mut told).await,
                Pass::Stay
            );
            assert!(told.unsupervised);
            assert_eq!(told.said, None);
        }
    }

    #[test]
    fn a_deb_install_says_each_newer_version_once() {
        let available =
            |latest: &str| Ok(serde_json::json!({"action": "available", "latest": latest}));
        let mut told = None;
        assert!(deb_tell(&available("0.7.115"), &mut told));
        assert!(
            !deb_tell(&available("0.7.115"), &mut told),
            "once per version"
        );
        assert!(!deb_tell(
            &Ok(serde_json::json!({"action": "up_to_date", "latest": "0.7.115"})),
            &mut told
        ));
        assert!(!deb_tell(&Err("feed".to_owned()), &mut told));
        assert!(deb_tell(&available("0.7.116"), &mut told));
        assert_eq!(told.as_deref(), Some("0.7.116"));
    }

    #[tokio::test]
    async fn a_deb_program_apt_replaced_restarts_serve_before_any_feed_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("switchboard");
        std::fs::write(&program, b"old").unwrap();
        let at_start = stamp(&program).unwrap();
        let next = dir.path().join("n");
        std::fs::write(&next, b"newer").unwrap();
        std::fs::rename(&next, &program).unwrap();
        let mut told = Told::default();
        assert_eq!(
            deb_pass(&program, &at_start, "0.0.1", true, &mut told).await,
            Pass::Restart("replaced")
        );
        // Unsupervised: it keeps running, says so once, and reads no feed.
        let mut told = Told::default();
        assert_eq!(
            deb_pass(&program, &at_start, "0.0.1", false, &mut told).await,
            Pass::Stay
        );
        assert!(told.unsupervised);
        assert_eq!(told.said, None);
        assert_eq!(
            deb_pass(&program, &at_start, "0.0.1", false, &mut told).await,
            Pass::Stay
        );
    }

    #[test]
    fn only_a_supervisor_that_starts_serve_again_counts() {
        use std::ffi::OsStr;
        let me = 4242;
        // systemd's own main process: SYSTEMD_EXEC_PID is its pid.
        assert!(supervised(me, Some(OsStr::new("4242")), None, Some(1)));
        // A child of a unit, or a shell in a terminal with the variable inherited: another pid.
        assert!(!supervised(me, Some(OsStr::new("812")), None, Some(812)));
        assert!(!supervised(me, Some(OsStr::new("")), None, Some(812)));
        // A launchd job: its label, launchd as the parent.
        assert!(supervised(
            me,
            None,
            Some(OsStr::new("ai.passioncode.serve")),
            Some(1)
        ));
        // A job's child: the label inherited, the job as the parent.
        assert!(!supervised(
            me,
            None,
            Some(OsStr::new("ai.passioncode.serve")),
            Some(812)
        ));
        // A hand start from Terminal (0) or from an app's terminal (an application. label),
        // even orphaned to launchd, is not a job.
        assert!(!supervised(me, None, Some(OsStr::new("0")), Some(1)));
        assert!(!supervised(
            me,
            None,
            Some(OsStr::new("application.com.googlecode.iterm2.123.456")),
            Some(1)
        ));
        // `(switchboard serve &)`, nohup, setsid -f, cron @reboot: init as the parent and nothing
        // a supervisor sets.
        assert!(!supervised(me, None, None, Some(1)));
        assert!(!supervised(me, None, None, None));
    }

    #[test]
    fn a_serve_keeps_its_copy_only_after_a_recent_successful_pass() {
        let ok = std::time::Instant::now();
        let after = |secs| ok + Duration::from_secs(secs);
        assert!(updates_copy_at(true, true, Some(ok), None, after(60)));
        assert!(updates_copy_at(true, true, Some(ok), None, after(15 * 60)));
        assert!(
            !updates_copy_at(true, true, Some(ok), None, after(15 * 60 + 1)),
            "stale"
        );
        assert!(
            !updates_copy_at(true, true, None, None, after(60)),
            "no loop"
        );
        // Review round 3: before its first pass, a loop that started recently holds the copy.
        assert!(updates_copy_at(true, true, None, Some(ok), after(90)));
        assert!(
            !updates_copy_at(true, true, None, Some(ok), after(15 * 60 + 1)),
            "no pass succeeded since the loop started"
        );
        // A pass that succeeded once and then went stale is not saved by a recent start.
        assert!(!updates_copy_at(
            true,
            true,
            Some(ok),
            Some(after(15 * 60)),
            after(15 * 60 + 1)
        ));
        assert!(
            !updates_copy_at(true, false, Some(ok), None, after(60)),
            "updates off"
        );
        assert!(
            !updates_copy_at(false, true, Some(ok), None, after(60)),
            "not installed"
        );
    }

    #[test]
    fn a_serve_that_is_not_an_installed_copy_does_not_keep_one_current() {
        // A test's binary is no installed copy: a window beside it updates its copy itself.
        let dir = tempfile::tempdir().unwrap();
        assert!(program().is_none());
        assert!(!updates_copy(dir.path()));
    }

    #[test]
    fn the_last_pass_is_reported_with_the_switch() {
        let dir = tempfile::tempdir().unwrap();
        note("available", Some("0.7.115"), Some("auto_update_off"));
        std::fs::write(dir.path().join("auto-update"), "off\n").unwrap();
        let last = last(dir.path());
        assert_eq!(last["auto_update"], false);
        assert!(last["checked_at"].as_u64().is_some());
    }

    #[tokio::test]
    async fn a_restart_waits_for_two_quiet_looks_and_gives_up_at_its_limit() {
        let looks = std::sync::atomic::AtomicUsize::new(0);
        // Busy on the first two looks, then quiet: it returns on the second quiet look.
        let quiet = wait_quiet(
            || {
                let n = looks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                (n < 2).then_some("request_recent")
            },
            Duration::from_millis(1),
            Duration::from_secs(5),
        )
        .await;
        assert!(quiet);
        assert_eq!(looks.load(std::sync::atomic::Ordering::Relaxed), 4);
        // Always busy: it stops at its limit and says it was not quiet.
        assert!(
            !wait_quiet(
                || Some("request_in_flight"),
                Duration::from_millis(1),
                Duration::from_millis(20)
            )
            .await
        );
    }
}
