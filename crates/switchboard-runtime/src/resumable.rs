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

/// Where a session can be resumed from: its config folder and working folder, from the newest by
/// time of the end recorded here (`at`), a launch record naming it (its home; its last turn, else
/// its launch) and the ledger of live sessions (SB-508: a session the person started in a terminal
/// or an editor, seen live in the last week; `last_seen`), so a session resumed elsewhere since its
/// end is found where it ran last; else a limit stop (the person's own Claude folder).
pub fn locate(root: &Path, session: &str, now: i64) -> Option<(PathBuf, String)> {
    let mut found: Vec<(i64, PathBuf, String)> = Vec::new();
    if let Some(v) = recent(root, now - WINDOW_SECONDS)
        .into_iter()
        .find(|v| v["session"] == session)
    {
        // A session host's line names the launch only: its record says the rest.
        if let (Some(config), Some(cwd)) = (v["config"].as_str(), v["cwd"].as_str()) {
            let at = v["at"].as_i64().unwrap_or(0);
            found.push((at, PathBuf::from(config), cwd.to_owned()));
        }
    }
    for r in crate::launches::recent_records(root, now - WINDOW_SECONDS) {
        if r["provider_session_id"] != session {
            continue;
        }
        let config = match r["home_dir"].as_str() {
            Some(home) => Some(root.join(home)),
            None => crate::person::claude_dir(),
        };
        if let (Some(config), Some(cwd)) = (config, r["working_directory"].as_str()) {
            let at = r["last_turn_at"]
                .as_i64()
                .or(r["launched_at"].as_i64())
                .unwrap_or(0);
            found.push((at, config, cwd.to_owned()));
        }
    }
    if let Some(entry) = crate::live_ledger::read(root).sessions.get(session) {
        if entry.last_seen >= now - WINDOW_SECONDS {
            found.push((
                entry.last_seen,
                PathBuf::from(&entry.config),
                entry.cwd.clone(),
            ));
        }
    }
    // The newest wins; on a tie the order above (an end, a launch, the ledger).
    let mut best: Option<(i64, PathBuf, String)> = None;
    for candidate in found {
        if best.as_ref().is_none_or(|b| candidate.0 > b.0) {
            best = Some(candidate);
        }
    }
    if let Some((_, config, cwd)) = best {
        return Some((config, cwd));
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
    crate::launch::start_terminal(&path).map_err(|_| NOT_STARTED)?;
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
    fn a_session_nobody_closed_or_launched_is_found_in_the_live_ledger() {
        // SB-508: started by the person in a terminal, seen live by the watch, never closed by
        // Switchboard: after a reboot `locate` still knows its config and working folders.
        let root = tempfile::tempdir().unwrap();
        let session = "33333333-3333-4333-8333-333333333333";
        assert!(locate(root.path(), session, 5000).is_none());
        let ledger = crate::live_ledger::merge(
            &crate::live_ledger::Ledger::default(),
            &[crate::live_ledger::Observed {
                session: session.into(),
                config: PathBuf::from("/h/.claude-work"),
                cwd: "/w/mine".into(),
                pid: 42,
            }],
            |_| "terminal".to_owned(),
            4000,
        )
        .unwrap();
        std::fs::write(
            root.path().join(crate::live_ledger::FILE),
            serde_json::to_vec(&ledger).unwrap(),
        )
        .unwrap();
        assert_eq!(
            locate(root.path(), session, 5000),
            Some((PathBuf::from("/h/.claude-work"), "/w/mine".to_owned()))
        );
        // An end Switchboard recorded later wins over the ledger; an earlier one does not.
        record(
            root.path(),
            json!({"session": session, "config": "/h/.claude", "cwd": "/w/old", "reason": "idle", "at": 3500}),
        );
        assert_eq!(
            locate(root.path(), session, 5000),
            Some((PathBuf::from("/h/.claude-work"), "/w/mine".to_owned())),
            "seen live after that end: it ran there last"
        );
        record(
            root.path(),
            json!({"session": session, "config": "/h/.claude", "cwd": "/w/other", "reason": "idle", "at": 4500}),
        );
        assert_eq!(
            locate(root.path(), session, 5000),
            Some((PathBuf::from("/h/.claude"), "/w/other".to_owned()))
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
