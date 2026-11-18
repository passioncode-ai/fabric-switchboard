//! SB-397 R1 (operator 2026-11-16: «перезапустить все работающие сессии … например для
//! перезагрузки скилов … через MCP агенту … он должен понимать что все сессии перезагрузятся и
//! мониторинг должен следить чтобы все поднялись назад и продолжили работу»): a restart wave.
//!
//! `plan` lists every running Claude Code session a wave would restart and every one it skips,
//! with the reason, the caller's own session and a digest; it restarts nothing. `apply` starts the
//! wave for that exact plan. The wave is a record `restarts/<wave>.json` (0600) that the session
//! watch advances every minute (`tick`): each session restarts only between turns (its session
//! file says `idle` and nothing of its own runs), at most `IN_FLIGHT` at once, resumed as itself
//! (`claude --resume <id>` in its own config and working folders); a hosted session restarts in its
//! host and its viewers stay; the caller restarts last, with a prompt naming the wave. A session
//! not back `BACK_SECONDS` after its end is `failed`. Nothing is read from a transcript but its
//! modified time; a record holds ids, folders, states and times, never text a session wrote.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The wave records and their scripts; a host starts a restart program only from this folder
/// (`session_host::RESTART_DIR`, the same name).
pub const DIR: &str = "restarts";
/// Sessions ending or resuming at once.
pub const IN_FLIGHT: usize = 3;
/// How long after its end a session must hold its id again.
pub const BACK_SECONDS: i64 = 300;
/// How long an act a quit cut is left to start its own window before another is opened.
pub const CUT_SETTLE_SECONDS: i64 = 60;
/// How long the caller waits for the others before it restarts anyway.
pub const CALLER_WAIT: i64 = 900;
/// A session still mid-turn this long after the wave began is skipped (`still_busy`), so a wave
/// always ends.
pub const STILL_BUSY: i64 = 86_400;
/// Between turns also means its transcript (subagents included) has not moved for this long.
pub const QUIET_SECONDS: i64 = 120;
const KEEP_SECONDS: i64 = 30 * 86_400;
const KEEP_WAVES: usize = 200;
const MAX_BYTES: u64 = 1024 * 1024;

pub const PLAN_STALE: &str =
    "The sessions changed since that plan was made. Make a new plan, check it, then apply it.";
pub const WAVE_RUNNING: &str =
    "A restart wave is already running. Wait for it to finish, or cancel it first.";
pub const REASON_NEEDED: &str = "Say why the sessions restart, in a few words.";
pub const NO_WAVE: &str = "No restart wave has run on this computer.";
pub const NOT_SAVED: &str = "The restart wave could not be saved.";
pub const UNSUPPORTED: &str = "Restarting sessions works on macOS and Linux for now.";
/// The sentence every plan and apply answer carries, so the agent knows what it does.
pub const WARNING: &str = "Every session listed under restart ends at the end of its current turn and is resumed as itself; the caller's own session restarts last, so end your turn after apply and you will be resumed with the wave's outcome.";
/// R3 review: the caller's own session is skipped (a wait of its own, a limit, a console that is
/// not hosted), so nothing will resume it; it must not end its turn waiting for a prompt.
pub const WARNING_CALLER_STAYS: &str = "Every session listed under restart ends at the end of its current turn and is resumed as itself. Your own session is not in this restart (see skipped), so it is not resumed by the wave: keep working, and read switchboard_sessions_restart_status for the outcome.";

/// Whether the caller's session is among the sessions that restart, and the warning that says so.
fn caller_restarts(restart: &[Entry]) -> (bool, &'static str) {
    let restarts = restart.iter().any(|e| e.caller);
    (
        restarts,
        if restarts {
            WARNING
        } else {
            WARNING_CALLER_STAYS
        },
    )
}

/// One session of a wave.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub session: String,
    pub pid: u32,
    pub config: PathBuf,
    pub cwd: String,
    /// The launch whose host runs it, when it runs in one that restarts in place.
    pub launch_id: Option<String>,
    pub in_place: bool,
    pub caller: bool,
    /// `pending`, `ending` (written before the act, so the next owner finishes one a quit cut),
    /// `resuming`, `back`, `continuing`, `failed`, `skipped` or `cancelled`.
    pub state: String,
    pub reason: Option<String>,
    pub ended_at: Option<i64>,
    pub back_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub session: String,
    pub cwd: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    pub id: String,
    pub reason: String,
    pub at: i64,
    pub digest: String,
    pub caller: Option<String>,
    pub sessions: Vec<Entry>,
    pub skipped: Vec<Skipped>,
    pub done_at: Option<i64>,
}

/// What a session's host answered when asked whether it restarts in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOffer {
    /// It restarts in place.
    Restarts,
    /// It predates restarts in place (or gave no answer at all).
    TooOld,
    /// It took the connection and did not answer within 15 s (SB-413): it is there but busy, so
    /// the plan says so instead of calling it too old.
    Busy,
}

/// What the planner knows of one live session beyond `duty::Seen`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub stopped: HashSet<String>,
    pub waiting: HashSet<String>,
    /// Session id to (launch id, what its host answered about restarting in place).
    pub hosted: HashMap<String, (String, HostOffer)>,
    /// Sessions that cannot be resumed (folder, transcript, `claude` or home missing).
    pub cannot_resume: HashSet<String>,
    pub caller: Option<String>,
}

/// The plan: who restarts, who is skipped and why. Pure.
pub fn plan_of(seen: &[crate::duty::Seen], facts: &Facts) -> (Vec<Entry>, Vec<Skipped>) {
    let mut copies: HashMap<&str, usize> = HashMap::new();
    for s in seen {
        *copies.entry(s.session.as_str()).or_default() += 1;
    }
    let mut restart = Vec::new();
    let mut skipped = Vec::new();
    let mut done = HashSet::new();
    for s in seen {
        if !done.insert(s.session.as_str()) {
            continue;
        }
        let hosted = facts.hosted.get(&s.session);
        let reason = if s.kind != "interactive" {
            Some("not_interactive")
        } else if copies[s.session.as_str()] > 1 {
            Some("duplicate")
        } else if facts.stopped.contains(&s.session) {
            Some("limit")
        } else if facts.waiting.contains(&s.session) {
            Some("has_waits")
        } else if hosted.is_some_and(|(_, offer)| *offer == HostOffer::TooOld) {
            Some("host_too_old")
        } else if hosted.is_some_and(|(_, offer)| *offer == HostOffer::Busy) {
            Some("host_busy")
        } else if hosted.is_none() && s.dashboards {
            Some("console_not_hosted")
        } else if facts.cannot_resume.contains(&s.session) {
            Some("cannot_resume")
        } else {
            None
        };
        match reason {
            Some(reason) => skipped.push(Skipped {
                session: s.session.clone(),
                cwd: s.cwd.clone(),
                reason: reason.into(),
            }),
            None => restart.push(Entry {
                session: s.session.clone(),
                pid: s.pid,
                config: s.config.clone(),
                cwd: s.cwd.clone(),
                launch_id: hosted.map(|(id, _)| id.clone()),
                in_place: hosted.is_some(),
                caller: facts.caller.as_deref() == Some(s.session.as_str()),
                state: "pending".into(),
                reason: None,
                ended_at: None,
                back_at: None,
            }),
        }
    }
    restart.sort_by(|a, b| a.session.cmp(&b.session));
    skipped.sort_by(|a, b| a.session.cmp(&b.session));
    (restart, skipped)
}

/// The digest of a plan: which sessions restart, which are skipped and why, and the caller.
pub fn digest(restart: &[Entry], skipped: &[Skipped], caller: Option<&str>) -> String {
    let mut lines: Vec<String> = restart
        .iter()
        .map(|e| format!("{}|restart|", e.session))
        .chain(
            skipped
                .iter()
                .map(|s| format!("{}|skip|{}", s.session, s.reason)),
        )
        .collect();
    lines.sort();
    lines.push(format!("caller|{}", caller.unwrap_or("")));
    Sha256::digest(lines.join("\n").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What a session looks like now, for `advance`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Now {
    /// The pid holding the session id now, if any.
    pub pid: Option<u32>,
    /// Between turns: `idle`, and nothing of its own runs.
    pub between_turns: bool,
    /// Its transcript's last write.
    pub last_write: Option<i64>,
}

/// One step of a wave: the states that follow from what the sessions look like now, and the
/// sessions to restart now. Pure.
pub fn advance(wave: &mut Wave, view: &HashMap<String, Now>, now: i64) -> Vec<String> {
    for e in wave.sessions.iter_mut() {
        let seen = view.get(&e.session).cloned().unwrap_or_default();
        match e.state.as_str() {
            "resuming" => {
                if seen.pid.is_some_and(|p| p != e.pid) {
                    e.state = "back".into();
                    e.back_at = Some(now);
                } else if e.ended_at.is_some_and(|t| now - t >= BACK_SECONDS) {
                    e.state = "failed".into();
                    e.reason = Some("not_back".into());
                }
            }
            "back" if e.caller => {
                // From the pass that saw it back: its resume prompt itself is not work (R4 review).
                if seen.last_write.zip(e.back_at).is_some_and(|(w, b)| w > b) {
                    e.state = "continuing".into();
                }
            }
            "pending" if seen.pid.is_none() => {
                // Ended by itself, or by a person, before its turn came: nothing to restart.
                e.state = "failed".into();
                e.reason = Some("gone".into());
            }
            "pending" if seen.pid.is_some_and(|p| p != e.pid) => {
                // Resumed by a person into another process meanwhile: the plan's pid is not its own
                // any more, and signalling it could reach another program (R3 review).
                e.state = "skipped".into();
                e.reason = Some("changed_since_plan".into());
            }
            "pending" if now - wave.at >= STILL_BUSY => {
                e.state = "skipped".into();
                e.reason = Some("still_busy".into());
            }
            _ => {}
        }
    }
    let busy = |e: &Entry| matches!(e.state.as_str(), "pending" | "ending" | "resuming");
    let in_flight = wave
        .sessions
        .iter()
        .filter(|e| matches!(e.state.as_str(), "ending" | "resuming"))
        .count();
    let others_settled = !wave.sessions.iter().any(|e| !e.caller && busy(e));
    let mut start = Vec::new();
    for e in &wave.sessions {
        if in_flight + start.len() >= IN_FLIGHT {
            break;
        }
        if e.state != "pending" {
            continue;
        }
        if e.caller && !(others_settled || now - wave.at >= CALLER_WAIT) {
            continue;
        }
        if view.get(&e.session).is_some_and(|s| s.between_turns) {
            start.push(e.session.clone());
        }
    }
    if wave.done_at.is_none()
        && !wave.sessions.iter().any(|e| {
            busy(e)
                || (e.caller
                    && e.state == "back"
                    && e.back_at.is_some_and(|b| now - b < 2 * BACK_SECONDS))
        })
    {
        wave.done_at = Some(now);
    }
    start
}

fn path(root: &Path, id: &str) -> Option<PathBuf> {
    crate::launches::valid_id(id).then(|| root.join(DIR).join(format!("{id}.json")))
}

fn lock(root: &Path) -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(root.join(DIR));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(DIR).join("waves.lock"))
        .ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    file.lock().ok().map(|()| file)
}

fn read(root: &Path, id: &str) -> Option<Wave> {
    switchboard_core::private_fs::read_private(&path(root, id)?, MAX_BYTES)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn write(root: &Path, wave: &Wave) -> Result<(), String> {
    let path = path(root, &wave.id).ok_or(NOT_SAVED)?;
    let bytes = serde_json::to_vec(wave).map_err(|_| NOT_SAVED)?;
    switchboard_core::private_fs::private_write(&path, &bytes).map_err(|_| NOT_SAVED.to_string())
}

/// Every wave kept, newest first; waves older than `KEEP_SECONDS` and past `KEEP_WAVES` removed.
pub fn waves(root: &Path, now: i64) -> Vec<Wave> {
    let Ok(entries) = std::fs::read_dir(root.join(DIR)) else {
        return Vec::new();
    };
    let mut all: Vec<Wave> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?;
            read(root, id)
        })
        .collect();
    all.sort_by(|a, b| b.at.cmp(&a.at).then(b.id.cmp(&a.id)));
    for (n, w) in all.iter().enumerate() {
        if n >= KEEP_WAVES || now - w.at > KEEP_SECONDS {
            if let Some(p) = path(root, &w.id) {
                let _ = std::fs::remove_file(p);
            }
            // The scripts a wave wrote go with it.
            for e in &w.sessions {
                remove_scripts(root, &w.id, &e.session);
            }
            let _ = std::fs::remove_dir_all(root.join(DIR).join(&w.id));
        }
    }
    all.truncate(KEEP_WAVES);
    all.retain(|w| now - w.at <= KEEP_SECONDS);
    all
}

/// The wave still running, if any.
pub fn running(root: &Path, now: i64) -> Option<Wave> {
    waves(root, now).into_iter().find(|w| w.done_at.is_none())
}

/// Sessions in a running wave: the duty officer leaves them alone.
pub fn in_wave(root: &Path, now: i64) -> HashSet<String> {
    running(root, now)
        .map(|w| {
            w.sessions
                .into_iter()
                .filter(|e| matches!(e.state.as_str(), "pending" | "ending" | "resuming"))
                .map(|e| e.session)
                .collect()
        })
        .unwrap_or_default()
}

fn safe(session: &str) -> String {
    session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// A host's restart program: directly in `restarts/`, the only folder a host starts one from.
fn script_path(root: &Path, wave: &str, session: &str) -> PathBuf {
    root.join(DIR)
        .join(format!("{wave}-{}.command", safe(session)))
}

#[cfg_attr(not(unix), allow(dead_code))]
/// A Terminal resume: a folder of its own, since the window's wrapper (`attach.command`) is
/// written beside its script and two resumes must never share one.
fn terminal_script_path(root: &Path, wave: &str, session: &str) -> PathBuf {
    root.join(DIR)
        .join(wave)
        .join(safe(session))
        .join("resume.command")
}

/// The scripts of one session of a wave, removed once it is settled: they carry its home's
/// exports.
fn remove_scripts(root: &Path, wave: &str, session: &str) {
    let _ = std::fs::remove_file(script_path(root, wave, session));
    let _ = std::fs::remove_dir_all(root.join(DIR).join(wave).join(safe(session)));
}

#[cfg_attr(not(unix), allow(dead_code))]
/// MON-2, MON-3 for a wave: a known end still ahead (or within `GRACE`), or any cron job; unlike
/// the idle close, a cron job holds its session however long ago its last turn was.
fn holds_wait(root: &Path, session: &str, now: i64) -> bool {
    let w = crate::monitors::read(root, session);
    w.crons > 0
        || w.until.is_some_and(|u| now < u + crate::monitors::GRACE)
        // SB-399: a session holding a Fabric task's claim.
        || w.hold_until.is_some_and(|u| now < u)
}

#[cfg_attr(not(unix), allow(dead_code))]
/// Whether the session can still be resumed: its folder and transcript are there.
fn resumable_now(e: &Entry) -> bool {
    Path::new(&e.cwd).is_dir()
        && crate::resumable::transcript(&e.config, &e.cwd, &e.session).is_file()
}

/// The prompt the caller resumes with.
pub fn caller_prompt(wave: &Wave) -> String {
    let count = |state: &str| {
        wave.sessions
            .iter()
            .filter(|e| !e.caller && e.state == state)
            .count()
    };
    format!(
        "Switchboard restarted this session in restart wave {} to load new skills and tools. Of the other sessions, {} came back, {} failed and {} still wait for their turn to end. Call switchboard_sessions_restart_status for the details, then continue your task from where you left off.",
        wave.id,
        count("back") + count("continuing"),
        count("failed"),
        count("pending") + count("ending") + count("resuming"),
    )
}

/// What `session_host::offers_restart` answered, as the plan reads it.
#[cfg(unix)]
fn host_offer(answer: Result<bool, String>) -> HostOffer {
    match answer {
        Ok(true) => HostOffer::Restarts,
        Err(e) if e == crate::session_host::HOST_BUSY => HostOffer::Busy,
        Ok(false) | Err(_) => HostOffer::TooOld,
    }
}

/// The live sessions as `duty::observe` sees them and the facts the plan needs.
#[cfg(unix)]
fn gather(root: &Path, caller_pid: Option<u32>, now: i64) -> (Vec<crate::duty::Seen>, Facts) {
    let seen = crate::duty::observe(root);
    let stopped = crate::recovery::live_stop_sessions(root, now);
    let waiting = seen
        .iter()
        .filter(|s| holds_wait(root, &s.session, now))
        .map(|s| s.session.clone())
        .collect();
    // A session's launch: the launch record naming it; hosted when its host is live.
    let mut live = Vec::new();
    for r in crate::launches::recent_records(root, now - KEEP_SECONDS) {
        let (Some(session), Some(launch)) =
            (r["provider_session_id"].as_str(), r["launch_id"].as_str())
        else {
            continue;
        };
        if seen.iter().any(|s| s.session == session)
            && crate::session_host::attachable(root, launch).is_some()
        {
            live.push((session.to_owned(), launch.to_owned()));
        }
    }
    // Every host is asked at once: a busy one takes up to 15 s to be called busy, and never
    // one after another (SB-413).
    let hosted: HashMap<String, (String, HostOffer)> = std::thread::scope(|scope| {
        let asks: Vec<_> = live
            .into_iter()
            .map(|(session, launch)| {
                scope.spawn(move || {
                    let offer = host_offer(crate::session_host::offers_restart(root, &launch));
                    (session, (launch, offer))
                })
            })
            .collect();
        asks.into_iter().filter_map(|ask| ask.join().ok()).collect()
    });
    let program = crate::launch::find_program("claude");
    let person = crate::person::claude_dir();
    let cannot_resume = seen
        .iter()
        .filter(|s| {
            program.is_none()
                || !Path::new(&s.cwd).is_dir()
                || !crate::resumable::transcript(&s.config, &s.cwd, &s.session).is_file()
                || (person.as_deref() != Some(s.config.as_path())
                    && !s.config.join("launch.command").is_file())
        })
        .map(|s| s.session.clone())
        .collect();
    // A session that comes back in a new window cannot on a Linux computer with no terminal
    // (no display, no emulator): it is left as it is rather than ended (R3 review).
    #[cfg(target_os = "linux")]
    let cannot_resume = {
        let mut cannot_resume: HashSet<String> = cannot_resume;
        if fabric_sessions_core::terminal::linux_terminal(
            std::env::var_os("TERMINAL"),
            fabric_sessions_core::terminal::find_program,
        )
        .is_err()
        {
            cannot_resume.extend(
                seen.iter()
                    .filter(|s| {
                        !hosted
                            .get(&s.session)
                            .is_some_and(|(_, offer)| *offer == HostOffer::Restarts)
                    })
                    .map(|s| s.session.clone()),
            );
        }
        cannot_resume
    };
    let caller = caller_pid.and_then(|pid| caller_session(&seen, pid));
    (
        seen,
        Facts {
            stopped,
            waiting,
            hosted,
            cannot_resume,
            caller,
        },
    )
}

/// The session whose process is the nearest ancestor of `pid` (the CLI or MCP server an agent ran).
#[cfg(unix)]
fn caller_session(seen: &[crate::duty::Seen], pid: u32) -> Option<String> {
    let table = crate::duty::process_table()?;
    let mut current = pid;
    for _ in 0..32 {
        if let Some(s) = seen.iter().find(|s| s.pid == current) {
            return Some(s.session.clone());
        }
        current = table.get(&current)?.ppid;
        if current <= 1 {
            return None;
        }
    }
    None
}

#[cfg_attr(not(unix), allow(dead_code))]
fn plan_value(restart: &[Entry], skipped: &[Skipped], caller: Option<&str>) -> Value {
    json!({
        "restart": restart.iter().map(|e| json!({
            "session_id": e.session, "folder": e.cwd, "caller": e.caller,
            "how": if e.in_place { "in_place" } else { "terminal" },
        })).collect::<Vec<_>>(),
        "skipped": skipped.iter().map(|s| json!({
            "session_id": s.session, "folder": s.cwd, "reason": s.reason,
        })).collect::<Vec<_>>(),
        "caller": caller,
        "caller_restarts": caller_restarts(restart).0,
        "digest": digest(restart, skipped, caller),
        "warning": caller_restarts(restart).1,
        "permissions": "Resumed sessions use Switchboard's permission choice, as the duty officer's restarts do; a mode a session was started with is not carried.",
    })
}

/// `sessions restart plan`.
#[cfg(unix)]
pub fn plan(root: &Path, caller_pid: Option<u32>, now: i64) -> Result<Value, String> {
    let (seen, facts) = gather(root, caller_pid, now);
    let (restart, skipped) = plan_of(&seen, &facts);
    Ok(plan_value(&restart, &skipped, facts.caller.as_deref()))
}
#[cfg(not(unix))]
pub fn plan(_: &Path, _: Option<u32>, _: i64) -> Result<Value, String> {
    Err(UNSUPPORTED.into())
}

/// `sessions restart apply`: starts the wave of the plan whose digest is `expected`.
#[cfg(unix)]
pub fn apply(
    root: &Path,
    expected: &str,
    reason: &str,
    caller_pid: Option<u32>,
    now: i64,
) -> Result<Value, String> {
    let reason = reason.trim();
    if reason.is_empty() || reason.chars().count() > 300 {
        return Err(REASON_NEEDED.into());
    }
    let _held = lock(root);
    if running(root, now).is_some() {
        return Err(WAVE_RUNNING.into());
    }
    let (seen, facts) = gather(root, caller_pid, now);
    let (restart, skipped) = plan_of(&seen, &facts);
    let made = digest(&restart, &skipped, facts.caller.as_deref());
    if made != expected {
        return Err(PLAN_STALE.into());
    }
    let wave = Wave {
        id: crate::launches::new_id(),
        reason: reason.to_owned(),
        at: now,
        digest: made,
        caller: facts.caller.clone(),
        sessions: restart,
        skipped,
        done_at: None,
    };
    write(root, &wave)?;
    crate::oplog::event(
        "restart_wave",
        &[
            ("phase", crate::oplog::Field::Code("started")),
            (
                "sessions",
                crate::oplog::Field::Number(wave.sessions.len() as i64),
            ),
            (
                "skipped",
                crate::oplog::Field::Number(wave.skipped.len() as i64),
            ),
        ],
    );
    Ok(
        json!({"wave": wave.id, "restart": wave.sessions.len(), "skipped": wave.skipped.len(),
        "caller": wave.caller, "caller_restarts": caller_restarts(&wave.sessions).0,
        "warning": caller_restarts(&wave.sessions).1}),
    )
}
#[cfg(not(unix))]
pub fn apply(_: &Path, _: &str, _: &str, _: Option<u32>, _: i64) -> Result<Value, String> {
    Err(UNSUPPORTED.into())
}

/// `sessions restart status`: the wave named, else the newest.
pub fn status(root: &Path, wave: Option<&str>, now: i64) -> Result<Value, String> {
    let all = waves(root, now);
    let w = match wave {
        Some(id) => all.into_iter().find(|w| w.id == id),
        None => all.into_iter().next(),
    }
    .ok_or(NO_WAVE)?;
    let count = |state: &str| w.sessions.iter().filter(|e| e.state == state).count();
    Ok(json!({
        "wave": w.id, "reason": w.reason, "at": w.at, "done_at": w.done_at, "caller": w.caller,
        "counts": {"pending": count("pending"), "ending": count("ending"), "resuming": count("resuming"), "back": count("back"),
            "continuing": count("continuing"), "failed": count("failed"), "cancelled": count("cancelled"),
            "skipped": w.skipped.len()},
        "sessions": w.sessions.iter().map(|e| json!({
            "session_id": e.session, "folder": e.cwd, "state": e.state, "reason": e.reason,
            "caller": e.caller, "how": if e.in_place { "in_place" } else { "terminal" },
            "ended_at": e.ended_at, "back_at": e.back_at,
        })).collect::<Vec<_>>(),
        "skipped": w.skipped.iter().map(|s| json!({
            "session_id": s.session, "folder": s.cwd, "reason": s.reason,
        })).collect::<Vec<_>>(),
    }))
}

/// `sessions restart cancel`: every session still pending stays as it is.
pub fn cancel(root: &Path, wave: &str, now: i64) -> Result<Value, String> {
    let _held = lock(root);
    let mut w = read(root, wave)
        .filter(|w| now - w.at <= KEEP_SECONDS)
        .ok_or(NO_WAVE)?;
    let mut cancelled = 0;
    for e in w.sessions.iter_mut().filter(|e| e.state == "pending") {
        e.state = "cancelled".into();
        cancelled += 1;
    }
    // A session already ending (its process stopped, a quit between) still finishes its restart:
    // the wave stays open for it (review of 2026-11-16, defect 4).
    if !w
        .sessions
        .iter()
        .any(|e| matches!(e.state.as_str(), "ending" | "resuming"))
    {
        w.done_at.get_or_insert(now);
    }
    write(root, &w)?;
    Ok(json!({"wave": w.id, "cancelled": cancelled}))
}

/// What each live session looks like now, by session id.
#[cfg_attr(not(unix), allow(dead_code))]
fn view_of(seen: &[crate::duty::Seen], now: i64) -> HashMap<String, Now> {
    let mut view: HashMap<String, Now> = HashMap::new();
    for s in seen {
        let entry = view.entry(s.session.clone()).or_default();
        entry.pid = Some(s.pid);
        entry.between_turns = s.status == "idle"
            && s.commands_known
            && s.commands == 0
            && s.last_write.is_none_or(|w| now - w >= QUIET_SECONDS);
        entry.last_write = s.last_write;
    }
    view
}

/// From the one-minute session watch: advances the running wave.
#[cfg(unix)]
pub fn tick(root: &Path, now: i64) {
    let _held = lock(root);
    let Some(mut wave) = running(root, now) else {
        return;
    };
    let view = view_of(&crate::duty::observe(root), now);
    // An act a quit cut (`ending` left behind): not ended, it waits again; ended, it resumes.
    let cut: Vec<String> = wave
        .sessions
        .iter()
        .filter(|e| e.state == "ending")
        .map(|e| e.session.clone())
        .collect();
    for session in cut {
        let Some(e) = wave.sessions.iter_mut().find(|e| e.session == session) else {
            continue;
        };
        let pid = view.get(&session).and_then(|v| v.pid);
        if pid == Some(e.pid) {
            e.state = "pending".into();
        } else if pid.is_some() || e.in_place {
            e.state = "resuming".into();
        } else if e.ended_at.is_some_and(|t| now - t < CUT_SETTLE_SECONDS) {
            // A window the cut act opened may still be starting its session: opening a second one
            // now would run two copies (R3 review). It is looked at again on a later pass.
            continue;
        } else {
            let path = terminal_script_path(root, &wave.id, &session);
            let started = path.is_file() && crate::launch::start_terminal(&path).is_ok();
            e.state = if started { "resuming" } else { "failed" }.into();
            if !started {
                e.reason = Some("not_started".into());
            }
        }
    }
    let resuming: HashSet<String> = wave
        .sessions
        .iter()
        .filter(|e| e.state == "resuming")
        .map(|e| e.session.clone())
        .collect();
    let start = advance(&mut wave, &view, now);
    // A session the wave ended and that came back is not "ended by a restart" any more: a later
    // close by the person must not read as one (R3 review). Only the Terminal path recorded it.
    for e in wave
        .sessions
        .iter()
        .filter(|e| e.state == "back" && !e.in_place && resuming.contains(&e.session))
    {
        crate::resumable::record(
            root,
            json!({"session": e.session, "config": e.config, "cwd": e.cwd,
                "reason": "restart_back", "at": now, "by": "wave"}),
        );
    }
    let stopped = crate::recovery::live_stop_sessions(root, now);
    let mut held = _held;
    for session in start {
        let Some(e) = wave.sessions.iter().find(|e| e.session == session).cloned() else {
            continue;
        };
        // A cancel while an earlier act of this pass ran leaves it as it is (R4 review).
        if e.state != "pending" {
            continue;
        }
        // Read again just before the act: earlier acts of this pass can take seconds each, and a
        // session that went busy meanwhile, or now runs in another process, is not ended (R3 review).
        let fresh = view_of(&crate::duty::observe(root), now);
        if !fresh
            .get(&session)
            .is_some_and(|v| v.pid == Some(e.pid) && v.between_turns)
        {
            continue;
        }
        // Checked again at the act: the plan may be hours old (SL-5, MON-2).
        let skip = if stopped.contains(&session) || holds_wait(root, &session, now) {
            Some("changed_since_plan")
        } else if !resumable_now(&e) {
            Some("cannot_resume")
        } else {
            None
        };
        if let Some(reason) = skip {
            set(&mut wave, &session, "skipped", Some(reason), None);
            continue;
        }
        // The intent first, so the next owner finishes an act a quit cut.
        set(&mut wave, &session, "ending", None, Some(now));
        let _ = write(root, &wave);
        // The lock is let go for the act (it may take seconds), so a cancel or a status read is
        // not held behind it; the wave is read again afterwards (R3 review).
        drop(held);
        let (state, reason) = restart_one(root, &wave, &e);
        held = lock(root);
        if let Some(again) = read(root, &wave.id) {
            wave = again;
        }
        set(&mut wave, &session, state, reason, Some(now));
        let _ = write(root, &wave);
        if held.is_none() {
            // Without the lock a cancel could be overwritten: this act's outcome is kept and the
            // rest wait for the next pass (R5 review).
            crate::oplog::event(
                "restart_wave",
                &[("phase", crate::oplog::Field::Code("lock_unavailable"))],
            );
            return;
        }
    }
    let _held = held;
    // A script is kept while its session may still need it: a host that answered unclearly may
    // run it within seconds, so it goes once the wave is done or the end is older than the wait
    // for its return (R3 review).
    for e in &wave.sessions {
        let settled = !matches!(e.state.as_str(), "pending" | "ending" | "resuming");
        if settled && (wave.done_at.is_some() || e.ended_at.is_none_or(|t| now - t >= BACK_SECONDS))
        {
            remove_scripts(root, &wave.id, &e.session);
        }
    }
    if wave.done_at.is_some() {
        let count = |s: &str| wave.sessions.iter().filter(|e| e.state == s).count() as i64;
        crate::oplog::event(
            "restart_wave",
            &[
                ("phase", crate::oplog::Field::Code("done")),
                (
                    "back",
                    crate::oplog::Field::Number(count("back") + count("continuing")),
                ),
                ("failed", crate::oplog::Field::Number(count("failed"))),
                ("skipped", crate::oplog::Field::Number(count("skipped"))),
            ],
        );
    }
    let _ = write(root, &wave);
}

#[cfg_attr(not(unix), allow(dead_code))]
fn set(wave: &mut Wave, session: &str, state: &str, reason: Option<&str>, ended: Option<i64>) {
    if let Some(e) = wave.sessions.iter_mut().find(|e| e.session == session) {
        e.state = state.into();
        e.reason = reason.map(str::to_owned);
        if ended.is_some() {
            e.ended_at = ended;
        }
    }
}
#[cfg(not(unix))]
pub fn tick(_: &Path, _: i64) {}

/// The resume script with the launch it continues exported after its `unset` line: that line
/// clears every variable the launch must not inherit, `SWITCHBOARD_LAUNCH_ID` among them, so an
/// export before it would be cleared at once (review of 2026-11-16, blocker 1).
pub(crate) fn with_launch_id(script: &str, launch: &str) -> String {
    let export = format!(
        "export SWITCHBOARD_LAUNCH_ID={}\n",
        fabric_sessions_core::cli::quoted(launch)
    );
    match script.find("\nunset ") {
        Some(at) => {
            let end = script[at + 1..]
                .find('\n')
                .map_or(script.len(), |n| at + 1 + n + 1);
            format!("{}{export}{}", &script[..end], &script[end..])
        }
        None => format!("{script}{export}"),
    }
}

/// The state a restart in a session's host leaves, and for a failure why.
#[cfg(unix)]
fn in_place_outcome(
    answer: Result<crate::session_host::Restart, String>,
) -> (&'static str, Option<&'static str>) {
    match answer {
        Ok(crate::session_host::Restart::Accepted) => ("resuming", None),
        // The request went out and no clear answer came: it may restart, so the wait for its
        // return decides (R3 review).
        Ok(crate::session_host::Restart::Unknown) => ("resuming", None),
        Ok(crate::session_host::Restart::Refused) => ("failed", Some("host_refused")),
        // SB-413: a host that is there but did not answer in time is not a host that is gone.
        Err(e) if e == crate::session_host::HOST_BUSY => ("failed", Some("host_busy")),
        Err(_) => ("failed", Some("host_gone")),
    }
}

/// Restarts one session: in its host when it runs in one that restarts in place, else as the duty
/// officer does (stopped, then resumed in a Terminal window, through a host when it has a launch).
/// The state that follows and, for a failure, why: `cannot_resume`, `not_written`,
/// `host_refused` (the host would not), `host_gone` (no host answered), `host_busy` (a host took
/// the connection and did not answer in time, SB-413), `still_running` (the
/// process did not end) or `not_started` (no Terminal window opened).
#[cfg(unix)]
fn restart_one(root: &Path, wave: &Wave, e: &Entry) -> (&'static str, Option<&'static str>) {
    let Some(program) = crate::launch::find_program("claude") else {
        return ("failed", Some("cannot_resume"));
    };
    let bypass = crate::launch::bypass_args(root, switchboard_core::Provider::Claude, &[]);
    let prompt = e.caller.then(|| caller_prompt(wave));
    let mut script = crate::duty::resume_script_for(
        &e.config,
        &e.cwd,
        &e.session,
        &program,
        &bypass,
        prompt.as_deref(),
    );
    if let Some(launch) = &e.launch_id {
        // The host's own launch continues: its record and its viewers stay the session's.
        script = with_launch_id(&script, launch);
    }
    if let (true, Some(launch)) = (e.in_place, &e.launch_id) {
        let path = script_path(root, &wave.id, &e.session);
        if crate::launch::write_script_at(&path, &script).is_err() {
            return ("failed", Some("not_written"));
        }
        return in_place_outcome(crate::session_host::restart(root, launch, &path));
    }
    let path = terminal_script_path(root, &wave.id, &e.session);
    let written = path
        .parent()
        .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
        && crate::launch::write_script_at(&path, &script).is_ok();
    if !written {
        return ("failed", Some("not_written"));
    }
    if !crate::duty::stop(e.pid) {
        return ("failed", Some("still_running"));
    }
    // Ended by the wave: *Continue* finds it even when the window below does not open
    // (review of 2026-11-16, defect 6).
    crate::resumable::record(
        root,
        json!({"session": e.session, "config": e.config, "cwd": e.cwd, "reason": "restart",
            "at": crate::monitor::now(), "by": "wave"}),
    );
    match crate::launch::start_terminal(&path) {
        Ok(()) => ("resuming", None),
        Err(_) => ("failed", Some("not_started")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_busy_host_is_neither_too_old_nor_gone() {
        use crate::session_host::{HOST_BUSY, NOT_FOUND};
        assert_eq!(host_offer(Ok(true)), HostOffer::Restarts);
        assert_eq!(host_offer(Ok(false)), HostOffer::TooOld);
        assert_eq!(host_offer(Err(HOST_BUSY.into())), HostOffer::Busy);
        assert_eq!(host_offer(Err(NOT_FOUND.into())), HostOffer::TooOld);
    }

    #[cfg(unix)]
    #[test]
    fn a_busy_host_is_not_a_gone_one() {
        use crate::session_host::{Restart, HOST_BUSY, NOT_FOUND};
        assert_eq!(
            in_place_outcome(Err(HOST_BUSY.into())),
            ("failed", Some("host_busy"))
        );
        assert_eq!(
            in_place_outcome(Err(NOT_FOUND.into())),
            ("failed", Some("host_gone"))
        );
        assert_eq!(in_place_outcome(Ok(Restart::Accepted)), ("resuming", None));
        assert_eq!(
            in_place_outcome(Ok(Restart::Refused)),
            ("failed", Some("host_refused"))
        );
    }
    use crate::duty::Seen;

    fn seen(session: &str, pid: u32) -> Seen {
        Seen {
            pid,
            session: session.into(),
            status: "idle".into(),
            status_since: 0,
            started_at: 0,
            kind: "interactive".into(),
            cwd: format!("/w/{session}"),
            config: PathBuf::from("/h/.claude"),
            last_write: None,
            dashboards: false,
            host: "terminal",
            commands: 0,
            commands_known: true,
            command_age: None,
            cpu: None,
            cpu_quiet: None,
            own_launches: None,
        }
    }

    #[test]
    fn the_launch_id_is_exported_after_the_line_that_clears_it() {
        let script = "#!/bin/zsh\nset +x\nunset A SWITCHBOARD_LAUNCH_ID B\nexport PATH='/bin'\nexec claude\n";
        let out = with_launch_id(script, "L-1");
        let unset = out.find("unset A").unwrap();
        let export = out.find("export SWITCHBOARD_LAUNCH_ID='L-1'").unwrap();
        assert!(export > unset, "{out}");
        assert!(out.ends_with("exec claude\n"));
        // The script the duty officer writes has the same shape.
        let real = crate::duty::resume_script_for(
            Path::new("/h/.claude"),
            "/w/app",
            "S1",
            Path::new("/usr/bin/claude"),
            &[],
            None,
        );
        let out = with_launch_id(&real, "L-2");
        assert!(
            out.find("export SWITCHBOARD_LAUNCH_ID='L-2'").unwrap() > out.find("\nunset ").unwrap()
        );
    }

    #[test]
    fn the_plan_restarts_what_can_come_back_and_names_every_skip() {
        let mut list = vec![
            seen("A", 1),
            seen("B", 2),
            seen("C", 3),
            seen("D", 4),
            seen("D", 5),
            seen("E", 6),
            seen("F", 7),
            seen("G", 8),
            seen("H", 9),
            seen("I", 10),
            seen("J", 11),
        ];
        list[2].kind = "print".into();
        list[7].dashboards = true;
        let facts = Facts {
            stopped: ["E".to_string()].into(),
            waiting: ["F".to_string()].into(),
            hosted: [
                ("B".to_string(), ("L-B".to_string(), HostOffer::Restarts)),
                ("H".to_string(), ("L-H".to_string(), HostOffer::TooOld)),
                ("J".to_string(), ("L-J".to_string(), HostOffer::Busy)),
            ]
            .into(),
            cannot_resume: ["I".to_string()].into(),
            caller: Some("A".into()),
        };
        let (restart, skipped) = plan_of(&list, &facts);
        let ids: Vec<_> = restart
            .iter()
            .map(|e| (e.session.as_str(), e.in_place, e.caller))
            .collect();
        assert_eq!(ids, vec![("A", false, true), ("B", true, false)]);
        let skips: Vec<_> = skipped
            .iter()
            .map(|s| (s.session.as_str(), s.reason.as_str()))
            .collect();
        assert_eq!(
            skips,
            vec![
                ("C", "not_interactive"),
                ("D", "duplicate"),
                ("E", "limit"),
                ("F", "has_waits"),
                ("G", "console_not_hosted"),
                ("H", "host_too_old"),
                ("I", "cannot_resume"),
                ("J", "host_busy"),
            ]
        );
        // The digest names the plan: a new skip, or another caller, is another plan.
        let d = digest(&restart, &skipped, Some("A"));
        assert_eq!(d, digest(&restart, &skipped, Some("A")));
        assert_ne!(d, digest(&restart, &skipped, Some("B")));
        assert_ne!(d, digest(&restart[1..], &skipped, Some("A")));
    }

    fn wave(sessions: Vec<Entry>) -> Wave {
        Wave {
            id: "W1".into(),
            reason: "skills".into(),
            at: 1000,
            digest: String::new(),
            caller: Some("A".into()),
            sessions,
            skipped: Vec::new(),
            done_at: None,
        }
    }

    fn entry(session: &str, pid: u32, caller: bool) -> Entry {
        Entry {
            session: session.into(),
            pid,
            config: PathBuf::from("/h/.claude"),
            cwd: "/w".into(),
            launch_id: None,
            in_place: false,
            caller,
            state: "pending".into(),
            reason: None,
            ended_at: None,
            back_at: None,
        }
    }

    fn now(pid: u32, between: bool) -> Now {
        Now {
            pid: Some(pid),
            between_turns: between,
            last_write: None,
        }
    }

    #[test]
    fn a_wave_restarts_between_turns_three_at_a_time_and_the_caller_last() {
        let mut w = wave(vec![
            entry("A", 1, true),
            entry("B", 2, false),
            entry("C", 3, false),
            entry("D", 4, false),
            entry("E", 5, false),
        ]);
        let mut view: HashMap<String, Now> = [
            ("A".to_string(), now(1, true)),
            ("B".to_string(), now(2, true)),
            ("C".to_string(), now(3, true)),
            ("D".to_string(), now(4, false)),
            ("E".to_string(), now(5, true)),
        ]
        .into();
        // D is mid-turn; the caller waits for the others; three at most.
        assert_eq!(advance(&mut w, &view, 1000), vec!["B", "C", "E"]);
        for e in w
            .sessions
            .iter_mut()
            .filter(|e| ["B", "C", "E"].contains(&e.session.as_str()))
        {
            e.state = "resuming".into();
            e.ended_at = Some(1000);
        }
        // B and C come back under new pids; E does not within five minutes.
        view.insert("B".into(), now(22, true));
        view.insert("C".into(), now(33, true));
        view.insert("E".into(), Now::default());
        assert!(
            advance(&mut w, &view, 1060).is_empty(),
            "D is still mid-turn"
        );
        let state = |w: &Wave, s: &str| {
            w.sessions
                .iter()
                .find(|e| e.session == s)
                .unwrap()
                .state
                .clone()
        };
        assert_eq!(
            (state(&w, "B"), state(&w, "C"), state(&w, "E")),
            ("back".into(), "back".into(), "resuming".into())
        );
        assert_eq!(
            advance(&mut w, &view, 1000 + BACK_SECONDS),
            Vec::<String>::new()
        );
        assert_eq!(state(&w, "E"), "failed");
        // D's turn ends; the caller still waits for it.
        view.insert("D".into(), now(4, true));
        assert_eq!(advance(&mut w, &view, 1400), vec!["D"]);
        w.sessions
            .iter_mut()
            .find(|e| e.session == "D")
            .unwrap()
            .state = "resuming".into();
        w.sessions
            .iter_mut()
            .find(|e| e.session == "D")
            .unwrap()
            .ended_at = Some(1400);
        view.insert("D".into(), now(44, true));
        assert_eq!(
            advance(&mut w, &view, 1460),
            vec!["A"],
            "the others settled: the caller goes"
        );
        assert!(w.done_at.is_none());
        let a = w.sessions.iter_mut().find(|e| e.session == "A").unwrap();
        a.state = "resuming".into();
        a.ended_at = Some(1460);
        view.insert(
            "A".into(),
            Now {
                pid: Some(11),
                between_turns: false,
                last_write: None,
            },
        );
        advance(&mut w, &view, 1520);
        assert_eq!(state(&w, "A"), "back");
        assert!(
            w.done_at.is_none(),
            "the caller is watched until it continues"
        );
        view.insert(
            "A".into(),
            Now {
                pid: Some(11),
                between_turns: false,
                last_write: Some(1530),
            },
        );
        advance(&mut w, &view, 1580);
        assert_eq!(state(&w, "A"), "continuing");
        assert_eq!(w.done_at, Some(1580));
    }

    #[test]
    fn the_caller_goes_after_fifteen_minutes_even_when_a_session_stays_mid_turn() {
        let mut w = wave(vec![entry("A", 1, true), entry("B", 2, false)]);
        let view: HashMap<String, Now> = [
            ("A".to_string(), now(1, true)),
            ("B".to_string(), now(2, false)),
        ]
        .into();
        assert!(advance(&mut w, &view, 1000 + CALLER_WAIT - 1).is_empty());
        assert_eq!(advance(&mut w, &view, 1000 + CALLER_WAIT), vec!["A"]);
        // A session gone before its turn came is not restarted.
        let mut w = wave(vec![entry("B", 2, false)]);
        advance(&mut w, &HashMap::new(), 1000);
        assert_eq!(w.sessions[0].state, "failed");
        assert_eq!(w.done_at, Some(1000));
    }

    #[test]
    fn a_session_now_in_another_process_is_skipped_never_signalled() {
        // R3 review: resumed by a person into a new pid while it waited; the plan's pid is stale.
        let mut w = wave(vec![entry("B", 2, false)]);
        let view: HashMap<String, Now> = [("B".to_string(), now(7, true))].into();
        assert!(advance(&mut w, &view, 1000).is_empty());
        assert_eq!(
            (
                w.sessions[0].state.as_str(),
                w.sessions[0].reason.as_deref()
            ),
            ("skipped", Some("changed_since_plan"))
        );
    }

    #[test]
    fn a_caller_left_out_of_the_restart_is_told_to_keep_working() {
        let (restarts, warning) = caller_restarts(&[entry("B", 2, false)]);
        assert!(!restarts);
        assert_eq!(warning, WARNING_CALLER_STAYS);
        assert_eq!(caller_restarts(&[entry("A", 1, true)]), (true, WARNING));
    }

    #[test]
    fn a_resumed_caller_reads_continuing_once_it_writes_after_it_is_back() {
        let mut e = entry("A", 1, true);
        e.state = "back".into();
        e.ended_at = Some(1000);
        e.back_at = Some(1100);
        let mut w = wave(vec![e]);
        // Its resume prompt, written before the pass that saw it back, is not work.
        let mut seen = now(9, true);
        seen.last_write = Some(1050);
        advance(&mut w, &[("A".to_string(), seen.clone())].into(), 1200);
        assert_eq!(w.sessions[0].state, "back");
        seen.last_write = Some(1150);
        advance(&mut w, &[("A".to_string(), seen)].into(), 1200);
        assert_eq!(w.sessions[0].state, "continuing");
    }

    #[test]
    fn a_session_busy_for_a_day_is_skipped_so_the_wave_ends() {
        let mut w = wave(vec![entry("B", 2, false)]);
        let view: HashMap<String, Now> = [("B".to_string(), now(2, false))].into();
        advance(&mut w, &view, 1000 + STILL_BUSY - 1);
        assert_eq!(w.sessions[0].state, "pending");
        advance(&mut w, &view, 1000 + STILL_BUSY);
        assert_eq!(
            (
                w.sessions[0].state.as_str(),
                w.sessions[0].reason.as_deref()
            ),
            ("skipped", Some("still_busy"))
        );
        assert_eq!(w.done_at, Some(1000 + STILL_BUSY));
    }

    #[test]
    fn a_wait_of_its_own_holds_a_session_out_of_a_wave_however_old_its_cron_job() {
        let root = tempfile::tempdir().unwrap();
        let cron = serde_json::json!({"session_id": "S1", "hook_event_name": "PreToolUse",
            "tool_name": "CronCreate", "tool_input": {}});
        crate::monitors::hook(root.path(), &cron, 0);
        assert!(
            holds_wait(root.path(), "S1", 10 * 86_400),
            "a cron job has no end"
        );
        assert!(!holds_wait(root.path(), "S2", 0));
        crate::monitors::hold(root.path(), "S2", 500, 0, None).unwrap();
        assert!(
            holds_wait(root.path(), "S2", 100),
            "a held session is left out of a wave"
        );
    }

    #[test]
    fn cancel_keeps_a_wave_open_while_a_session_is_ending() {
        let root = tempfile::tempdir().unwrap();
        let mut w = wave(vec![entry("A", 1, true), entry("B", 2, false)]);
        w.id = crate::launches::new_id();
        w.sessions[0].state = "ending".into();
        write(root.path(), &w).unwrap();
        assert_eq!(cancel(root.path(), &w.id, 1100).unwrap()["cancelled"], 1);
        assert!(
            running(root.path(), 1100).is_some(),
            "the ending session still comes back"
        );
        assert_eq!(
            status(root.path(), None, 1100).unwrap()["counts"]["ending"],
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_wave_folder_is_the_one_a_host_starts_restarts_from() {
        assert_eq!(DIR, crate::session_host::RESTART_DIR);
    }

    #[test]
    fn a_wave_is_kept_privately_and_cancel_leaves_the_pending_sessions_alone() {
        let root = tempfile::tempdir().unwrap();
        let mut w = wave(vec![entry("A", 1, true), entry("B", 2, false)]);
        w.id = crate::launches::new_id();
        write(root.path(), &w).unwrap();
        assert_eq!(running(root.path(), 1000).unwrap().id, w.id);
        assert_eq!(
            in_wave(root.path(), 1000),
            ["A".to_string(), "B".to_string()].into()
        );
        let answer = cancel(root.path(), &w.id, 1100).unwrap();
        assert_eq!(answer["cancelled"], 2);
        assert!(running(root.path(), 1100).is_none());
        let st = status(root.path(), None, 1100).unwrap();
        assert_eq!(st["counts"]["cancelled"], 2);
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(path(root.path(), &w.id).unwrap())
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
        // Kept thirty days.
        assert!(waves(root.path(), 1000 + KEEP_SECONDS + 1).is_empty());
        assert_eq!(
            status(root.path(), None, 1000 + KEEP_SECONDS + 1),
            Err(NO_WAVE.into())
        );
    }
}
