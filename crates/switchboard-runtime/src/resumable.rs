//! SB-396 wave B (LC-5, LC-7; operator 2026-11-16: «восстановить именно ту, которая ждала, а не
//! новую открыть … чтобы контекст не терялся»): what is needed to resume a session Switchboard
//! closed or saw end, and the resume itself.
//!
//! `resumable.jsonl` (0600, the newest `KEEP` lines, ids and paths only, never text a session
//! wrote) gets one line per end Switchboard causes: the duty officer's close, a session host's idle
//! close. A line names the session id, its Claude Code config folder, its working folder, why it
//! ended and when. `continue_session` resumes exactly that session (`claude --resume <id>`) in its
//! own config folder, so its transcript and context are the ones it had; it is refused while a
//! process still holds the session, and when its transcript is gone (never a fresh session).
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const FILE: &str = "resumable.jsonl";
const KEEP: usize = 1000;
const MAX_BYTES: u64 = 1024 * 1024;
/// How far back the Sessions page and Continue look.
pub const WINDOW_SECONDS: i64 = 7 * 86_400;

pub const LIVE: &str =
    "That session is still running in a process. Open its window, or close it there first.";
pub const NO_TRANSCRIPT: &str = "That session's history is not on this computer any more, so it cannot be continued as the same session.";
pub const UNKNOWN: &str =
    "Switchboard has no record of that session. Refresh the list, then retry.";
pub const NO_FOLDER: &str = "That session's folder is gone, so it cannot be continued there.";
pub const NO_CLAUDE: &str = "Claude Code was not found on this computer.";
pub const NOT_STARTED: &str = "The Terminal window to continue the session could not be opened.";
/// The prompt a session continued by a person goes on with.
pub const CONTINUE_PROMPT: &str =
    "Switchboard continued this session after it ended. Continue the task from where you left off.";

/// The lock file beside the ledger: the owner's duty officer and every session host append to it,
/// each a process of its own (SB-397 W0.2).
const LOCK: &str = "resumable.lock";

/// Appends one end; a line that cannot be written is dropped (it never stops the close). The read
/// and the write happen under an exclusive lock on `resumable.lock`, so two processes ending
/// sessions at once both keep their line.
pub fn record(root: &Path, entry: Value) {
    let path = root.join(FILE);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(LOCK));
    #[cfg(unix)]
    if let Ok(file) = &lock {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    // Held until the end of this function; a lock that cannot be taken still records the line.
    let _held = lock.ok().filter(|f| f.lock().is_ok());
    let mut lines: Vec<String> = switchboard_core::private_fs::read_private(&path, MAX_BYTES)
        .ok()
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    lines.push(entry.to_string());
    let skip = lines.len().saturating_sub(KEEP);
    let text = lines[skip..].join("\n") + "\n";
    let _ = switchboard_core::private_fs::private_write(&path, text.as_bytes());
}

/// The newest line of each session ended at or after `since`, newest first.
pub fn recent(root: &Path, since: i64) -> Vec<Value> {
    let Ok(bytes) = switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(&bytes).lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let (Some(session), Some(at)) = (v["session"].as_str(), v["at"].as_i64()) else {
            continue;
        };
        if at >= since && seen.insert(session.to_owned()) {
            out.push(v);
        }
    }
    out
}

/// Claude Code's transcript of `session` begun in `cwd`, under the config folder `config`.
pub fn transcript(config: &Path, cwd: &str, session: &str) -> PathBuf {
    let folder: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    config
        .join("projects")
        .join(folder)
        .join(format!("{session}.jsonl"))
}

/// Where a session can be resumed from: its config folder and working folder, from the newest
/// end recorded here, else a launch record naming it (its home), else a limit stop (the person's
/// own Claude folder).
pub fn locate(root: &Path, session: &str, now: i64) -> Option<(PathBuf, String)> {
    if let Some(v) = recent(root, now - WINDOW_SECONDS)
        .into_iter()
        .find(|v| v["session"] == session)
    {
        // A session host's line names the launch only: its record says the rest.
        if let (Some(config), Some(cwd)) = (v["config"].as_str(), v["cwd"].as_str()) {
            return Some((PathBuf::from(config), cwd.to_owned()));
        }
    }
    for r in crate::launches::recent_records(root, now - WINDOW_SECONDS) {
        if r["provider_session_id"] == session {
            let config = match r["home_dir"].as_str() {
                Some(home) => root.join(home),
                None => crate::person::claude_dir()?,
            };
            return Some((config, r["working_directory"].as_str()?.to_owned()));
        }
    }
    let stops = crate::recovery::read(root).ok()?;
    let stop = stops.iter().find(|s| s.session == session)?;
    Some((
        crate::person::claude_dir()?,
        stop.directory.to_string_lossy().into_owned(),
    ))
}

/// SB-400: how a continued session runs. The default: a Terminal window of its own.
#[derive(Clone, Debug, Default)]
pub struct How {
    /// The answer carries the script for the calling terminal; no window opens.
    pub in_place: bool,
    /// With `in_place`: under a launch of its own (recorded, its hooks reporting to it), which
    /// the caller runs in a session host.
    pub hosted: bool,
    /// Who asked, for the launch record.
    pub by: crate::launches::LaunchedBy,
}

/// SB-450: held while a continuation checks and records itself.
#[cfg(unix)]
static CONTINUING: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// SB-450: how long after its launch a continuation with no end counts as starting.
pub const JUST_STARTED_SECONDS: i64 = 120;
/// SB-450: the folder of the marks a Terminal resume leaves (`resuming/<session>`, 0600, its
/// start time): a resume in a Terminal window writes no launch record, and its process takes
/// seconds to hold the session.
pub const RESUMING_DIR: &str = "resuming";

/// SB-450 (review round 1 of #836): marks `session` as being resumed in a Terminal window now,
/// before the window opens, so a continuation in the next `JUST_STARTED_SECONDS` sees it. Marks
/// older than a day are removed on the way.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn mark_resuming(root: &Path, session: &str, now: i64) {
    if !crate::launches::valid_id(session) {
        return;
    }
    let dir = root.join(RESUMING_DIR);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if resuming_at(&e.path()).is_none_or(|at| now - at > 86_400) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = switchboard_core::private_fs::private_write(
            &dir.join(session),
            now.to_string().as_bytes(),
        );
    }
}

/// SB-450: takes back the mark of a resume whose window did not open.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn unmark_resuming(root: &Path, session: &str) {
    if crate::launches::valid_id(session) {
        let _ = std::fs::remove_file(root.join(RESUMING_DIR).join(session));
    }
}

fn resuming_at(path: &Path) -> Option<i64> {
    let bytes = switchboard_core::private_fs::read_private(path, 64).ok()?;
    String::from_utf8_lossy(&bytes).trim().parse().ok()
}

/// SB-450: whether `session` is being continued now, so another continuation must not start:
/// it runs in a session host (its launch's record names it); a launch that names it started less
/// than `JUST_STARTED_SECONDS` ago and has not ended; a Terminal resume (the duty officer's
/// restart, a restart wave's, a person's Continue) marked it less than `JUST_STARTED_SECONDS`
/// ago; the duty officer's restart of it is begun and not finished (its intent open, which its
/// next check finishes by resuming it); or a running restart wave holds it.
pub fn being_continued(root: &Path, session: &str, now: i64) -> bool {
    if crate::launches::valid_id(session)
        && resuming_at(&root.join(RESUMING_DIR).join(session))
            .is_some_and(|at| (0..JUST_STARTED_SECONDS).contains(&(now - at)))
    {
        return true;
    }
    if crate::duty::restart_open(root, session, now)
        || crate::restarts::in_wave(root, now).contains(session)
    {
        return true;
    }
    #[cfg(unix)]
    if crate::session_host::live_hosts(root)
        .iter()
        .any(|(id, _)| crate::launches::session_of(root, id).as_deref() == Some(session))
    {
        return true;
    }
    crate::launches::recent_records(root, now - JUST_STARTED_SECONDS)
        .iter()
        .any(|r| {
            r["provider_session_id"] == session
                && r["ended"].is_null()
                && r["launched_at"]
                    .as_i64()
                    .is_some_and(|at| (0..JUST_STARTED_SECONDS).contains(&(now - at)))
        })
}

/// Resumes `session` as itself in a Terminal window of its own.
pub fn continue_session(root: &Path, session: &str, now: i64) -> Result<Value, String> {
    continue_as(root, session, now, &How::default())
}

/// Resumes `session` as itself, as `how` says: in a Terminal window of its own, or (SB-400) as
/// a script the calling terminal runs, with a launch of its own when it runs hosted.
#[cfg(unix)]
pub fn continue_as(root: &Path, session: &str, now: i64, how: &How) -> Result<Value, String> {
    if !crate::launches::valid_id(session) {
        return Err(UNKNOWN.into());
    }
    if crate::machine_sessions::live_sessions(root).contains_key(session) {
        return Err(LIVE.into());
    }
    // SB-450: one continuation at a time in this process, and none of a session a host runs or a
    // launch began to continue a moment ago (its process may not hold the session yet): the
    // owner's continuation after a restart and a person's, or another agent's, never both start.
    let _one = CONTINUING.lock().unwrap_or_else(|p| p.into_inner());
    if being_continued(root, session, now) {
        return Err(LIVE.into());
    }
    let (config, cwd) = locate(root, session, now).ok_or(UNKNOWN)?;
    if !Path::new(&cwd).is_dir() {
        return Err(NO_FOLDER.into());
    }
    if !transcript(&config, &cwd, session).is_file() {
        return Err(NO_TRANSCRIPT.into());
    }
    let program = crate::launch::find_program("claude").ok_or(NO_CLAUDE)?;
    // SB-515: the permission choice as Fabric Sessions asks it (D-7), the one answer it reads.
    let bypass = crate::session_accounts::resume_flags(root);
    let mut script = crate::duty::resume_script_for(
        &config,
        &cwd,
        session,
        &program,
        &bypass,
        Some(CONTINUE_PROMPT),
    );
    let launch_id = how.hosted.then(crate::launches::new_id);
    if let Some(id) = &launch_id {
        // The session's hooks report to its own launch (its record, its turns, its end).
        script = crate::restarts::with_launch_id(&script, id);
    }
    let dir = root.join("duty");
    let path = dir.join(format!("continue-{session}.command"));
    std::fs::create_dir_all(&dir).map_err(|_| NOT_STARTED)?;
    crate::launch::write_script_at(&path, &script).map_err(|_| NOT_STARTED)?;
    if how.in_place {
        if let Some(id) = &launch_id {
            let record = json!({
                "provider": "claude", "mode": "resume", "working_directory": cwd,
                "in_place": true, "hosted": true, "launched_at": now,
                "provider_session_id": session,
                "home_dir": config.strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/")),
            });
            crate::launches::record_resume(root, id, record, &how.by)?;
        }
        // SB-509: an offer to continue it (Needs attention) is taken.
        crate::session_watch::continued(root, session, now);
        crate::oplog::event(
            "session_continue",
            &[("outcome", crate::oplog::Field::Code("in_place"))],
        );
        return Ok(
            json!({"continued": session, "folder": cwd, "script": path, "launch_id": launch_id}),
        );
    }
    mark_resuming(root, session, now);
    if crate::launch::start_terminal(&path).is_err() {
        unmark_resuming(root, session);
        return Err(NOT_STARTED.into());
    }
    crate::session_watch::continued(root, session, now);
    crate::oplog::event(
        "session_continue",
        &[("outcome", crate::oplog::Field::Code("started"))],
    );
    Ok(json!({"continued": session, "folder": cwd}))
}
#[cfg(not(unix))]
pub fn continue_as(_: &Path, _: &str, _: i64, _: &How) -> Result<Value, String> {
    Err(crate::duty::CLOSE_UNSUPPORTED.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_end_is_kept_and_found_and_the_newest_line_of_a_session_wins() {
        let root = tempfile::tempdir().unwrap();
        record(
            root.path(),
            json!({"session": "S1", "config": "/h/.claude", "cwd": "/w/a", "reason": "idle", "at": 100}),
        );
        record(
            root.path(),
            json!({"session": "S2", "config": "/h/.claude", "cwd": "/w/b", "reason": "duplicate", "at": 200}),
        );
        record(
            root.path(),
            json!({"session": "S1", "config": "/h2", "cwd": "/w/a", "reason": "host_idle", "at": 300}),
        );
        let list = recent(root.path(), 0);
        assert_eq!(list.len(), 2);
        assert_eq!(
            (list[0]["session"].clone(), list[0]["reason"].clone()),
            (json!("S1"), json!("host_idle"))
        );
        assert!(recent(root.path(), 250)
            .iter()
            .all(|v| v["session"] == "S1"));
        let (config, cwd) = locate(root.path(), "S2", 300).unwrap();
        assert_eq!(
            (config, cwd.as_str()),
            (PathBuf::from("/h/.claude"), "/w/b")
        );
        assert!(locate(root.path(), "S9", 300).is_none());
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.path().join(FILE))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
    }

    #[test]
    fn two_writers_at_once_both_keep_every_line() {
        let root = tempfile::tempdir().unwrap();
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let dir = root.path().to_path_buf();
                std::thread::spawn(move || {
                    for n in 0..25 {
                        record(&dir, json!({"session": format!("S{t}-{n}"), "at": 1}));
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(
            recent(root.path(), 0).len(),
            200,
            "no line is lost to a race"
        );
    }

    #[test]
    fn the_transcript_is_found_where_claude_code_writes_it() {
        assert_eq!(
            transcript(Path::new("/h/.claude"), "/Users/a/DATA/my app", "S1"),
            PathBuf::from("/h/.claude/projects/-Users-a-DATA-my-app/S1.jsonl")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_session_a_launch_began_to_continue_a_moment_ago_is_not_continued_again() {
        // SB-450: the owner's continuation after a restart and another agent's never both start.
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let cwd = work.path().to_string_lossy().into_owned();
        let config = root.path().join("cfg");
        record(
            root.path(),
            json!({"session": "S1", "config": config, "cwd": cwd, "reason": "idle", "at": 100}),
        );
        let t = transcript(&config, &cwd, "S1");
        std::fs::create_dir_all(t.parent().unwrap()).unwrap();
        std::fs::write(&t, b"{}\n").unwrap();
        let id = crate::launches::new_id();
        crate::launches::record_resume(
            root.path(),
            &id,
            json!({"provider": "claude", "provider_session_id": "S1", "launched_at": 250}),
            &crate::launches::LaunchedBy::default(),
        )
        .unwrap();
        assert!(being_continued(root.path(), "S1", 300));
        assert_eq!(continue_session(root.path(), "S1", 300), Err(LIVE.into()));
        assert!(!being_continued(root.path(), "S2", 300), "another session");
        assert!(
            !being_continued(root.path(), "S1", 250 + JUST_STARTED_SECONDS),
            "a continuation whose process never came is not waited for"
        );
        crate::launches::end_once(root.path(), &id, "not_started", 260, Some(1));
        assert!(!being_continued(root.path(), "S1", 300), "an ended one");
    }

    #[test]
    fn a_terminal_resume_or_a_duty_restart_begun_holds_the_session() {
        // Review round 1 of #836, P1: the duty officer's restart and a restart wave resume in a
        // Terminal window, which writes no launch record; a continuation after a restart must
        // not start a second copy beside it.
        let root = tempfile::tempdir().unwrap();
        let session = "11111111-2222-4333-8444-555555555555";
        assert!(!being_continued(root.path(), session, 1000));
        mark_resuming(root.path(), session, 1000);
        assert!(being_continued(root.path(), session, 1000 + 30));
        assert!(
            !being_continued(root.path(), session, 1000 + JUST_STARTED_SECONDS),
            "a window that never brought the session up is not waited for"
        );
        unmark_resuming(root.path(), session);
        assert!(!being_continued(root.path(), session, 1000 + 30));
        // A mark is never written outside its folder.
        mark_resuming(root.path(), "../x", 1000);
        assert!(!root.path().join("x").exists());
        // The duty officer's restart with its intent open: its act, or the check that finishes
        // it, resumes the session.
        let journal = root.path().join(crate::duty::JOURNAL);
        let intent = json!({"at": 2000, "phase": "intent", "action": "restart",
            "reason": "stuck", "session": session, "pid": 7});
        std::fs::write(&journal, format!("{intent}\n")).unwrap();
        assert!(being_continued(root.path(), session, 2010));
        assert!(being_continued(root.path(), session, 2600), "still open");
        assert!(!being_continued(root.path(), "S2", 2010), "another session");
        let outcome = json!({"at": 2000, "action": "restart", "reason": "stuck",
            "outcome": "restarted", "session": session, "pid": 7});
        std::fs::write(&journal, format!("{intent}\n{outcome}\n")).unwrap();
        assert!(!being_continued(root.path(), session, 2010), "finished");
        // A close begun is not a resume: the session may be continued.
        let close = json!({"at": 2000, "phase": "intent", "action": "close",
            "reason": "idle", "session": session, "pid": 7});
        std::fs::write(&journal, format!("{close}\n")).unwrap();
        assert!(!being_continued(root.path(), session, 2010));
    }

    #[cfg(unix)]
    #[test]
    fn a_session_without_its_history_or_record_is_never_continued_as_a_new_one() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            continue_session(root.path(), "S1", 300),
            Err(UNKNOWN.into())
        );
        let work = tempfile::tempdir().unwrap();
        let cwd = work.path().to_string_lossy().into_owned();
        record(
            root.path(),
            json!({"session": "S1", "config": root.path().join("cfg"), "cwd": cwd, "reason": "idle", "at": 290}),
        );
        assert_eq!(
            continue_session(root.path(), "S1", 300),
            Err(NO_TRANSCRIPT.into())
        );
        assert_eq!(
            continue_session(root.path(), "../x", 300),
            Err(UNKNOWN.into())
        );
    }
}
