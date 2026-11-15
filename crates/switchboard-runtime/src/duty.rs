//! SB-384 (operator 2026-11-15): the session duty officer. Every live Claude Code session on this
//! machine is looked at; one that stopped moving is closed and continued in a new session, one
//! that finished and only holds a terminal is closed, and a long-lived one gets a fresh process at
//! the end of a turn, so stuck and forgotten sessions never pile up.
//!
//! What it reads: the process table and each session's own file `sessions/<pid>.json`, in which
//! Claude Code writes its status (`busy`, `shell`, `waiting`, `idle`) and since when, and the
//! modification time of that session's transcript. Never a transcript's content.
//!
//! What it does, at most once every five minutes and at most three times a check (DEC-0018):
//! - `stuck`: busy (or running a shell command) for `stuck_minutes` and no transcript written for
//!   as long: closed, then resumed in a new Terminal window (`claude --resume <id>`) with a prompt
//!   to continue.
//! - `waiting`: waiting for an answer or a permission for `waiting_minutes`: the same.
//! - `idle`: its turn ended `idle_minutes` ago: closed (it can be resumed from Claude Code's history).
//! - `old`: running for `max_age_hours` and at the end of a turn: resumed in a fresh process, no prompt.
//! - `duplicate`: a second process holding the same session: the newer one is closed.
//!
//! Left alone: sessions in Fabric Dashboards' consoles (Dashboards owns them), non-interactive runs
//! (`claude -p`, dispatched runs end by themselves), Codex. A session is restarted at most three
//! times in six hours. `duty.json` (0600) keeps the person's settings, `duty.jsonl` (0600) one line
//! per action (session id, folder, reason; never text the session wrote).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "duty.json";
pub const JOURNAL: &str = "duty.jsonl";
/// Seconds between two checks that may act.
pub const CHECK_SECONDS: i64 = 300;
const MAX_BYTES: u64 = 4096;
const MAX_ACTIONS: usize = 3;
const RESTARTS_PER_WINDOW: usize = 3;
const RESTART_WINDOW: i64 = 6 * 3600;
const JOURNAL_LINES: usize = 500;
/// A session at the end of its turn for this long counts as between turns (an `old` restart).
const BETWEEN_TURNS: i64 = 120;
/// The prompt a resumed session continues with.
pub const CONTINUE_PROMPT: &str = "Switchboard restarted this session because it had stopped making progress. Continue the task from where you left off.";

pub const SETTINGS_INVALID: &str = "Give the duty officer's limits in range: stuck, waiting and idle minutes from 10 to 1440, maximum age hours from 1 to 72.";
pub const UNREADABLE: &str =
    "The duty officer's settings (duty.json) could not be read. Set them again, or delete duty.json for the defaults.";
pub const NOT_SAVED: &str = "The duty officer's settings could not be saved.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub on: bool,
    pub stuck_minutes: u32,
    pub waiting_minutes: u32,
    pub idle_minutes: u32,
    pub max_age_hours: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            on: true,
            stuck_minutes: 45,
            waiting_minutes: 60,
            idle_minutes: 60,
            max_age_hours: 4,
        }
    }
}
impl Settings {
    pub fn valid(&self) -> bool {
        let minutes = 10..=1440;
        minutes.contains(&self.stuck_minutes)
            && minutes.contains(&self.waiting_minutes)
            && minutes.contains(&self.idle_minutes)
            && (1..=72).contains(&self.max_age_hours)
    }
}

/// The person's settings; the defaults (on) when no file holds any.
pub fn settings(root: &Path) -> Result<Settings, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    let saved: Settings = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    if !saved.valid() {
        return Err(UNREADABLE.into());
    }
    Ok(saved)
}

pub fn save(root: &Path, settings: &Settings) -> Result<(), String> {
    if !settings.valid() {
        return Err(SETTINGS_INVALID.into());
    }
    let bytes = serde_json::to_vec(settings).map_err(|_| NOT_SAVED)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| NOT_SAVED.into())
}

/// One live Claude Code session as the duty officer sees it.
#[derive(Clone, Debug)]
pub struct Seen {
    pub pid: u32,
    pub session: String,
    pub status: String,
    /// When Claude Code last changed `status`, Unix seconds.
    pub status_since: i64,
    pub started_at: i64,
    pub kind: String,
    pub cwd: String,
    /// The Claude Code config folder its session file lies in.
    pub config: PathBuf,
    pub last_write: Option<i64>,
    /// Runs in a Fabric Dashboards console.
    pub dashboards: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Restart { prompt: bool },
    Close,
}

/// What to do with one session now, and why; none to leave it. `newer_copy`: another process
/// holding the same session started before this one.
pub fn decide(s: &Seen, set: &Settings, now: i64, newer_copy: bool) -> Option<(Act, &'static str)> {
    if s.kind != "interactive" || s.dashboards {
        return None;
    }
    if newer_copy {
        return Some((Act::Close, "duplicate"));
    }
    let since = now - s.status_since;
    let quiet = now - s.last_write.unwrap_or(s.status_since).max(s.status_since);
    match s.status.as_str() {
        "idle" if since >= i64::from(set.idle_minutes) * 60 => Some((Act::Close, "idle")),
        "idle"
            if now - s.started_at >= i64::from(set.max_age_hours) * 3600
                && since >= BETWEEN_TURNS =>
        {
            Some((Act::Restart { prompt: false }, "old"))
        }
        "waiting" if since >= i64::from(set.waiting_minutes) * 60 => {
            Some((Act::Restart { prompt: true }, "waiting"))
        }
        "busy" | "shell"
            if since >= i64::from(set.stuck_minutes) * 60
                && quiet >= i64::from(set.stuck_minutes) * 60 =>
        {
            Some((Act::Restart { prompt: true }, "stuck"))
        }
        _ => None,
    }
}

/// The live sessions: a Claude Code process with its session file.
pub fn observe(root: &Path) -> Vec<Seen> {
    let procs: HashMap<u32, crate::machine_sessions::Proc> = crate::machine_sessions::processes()
        .into_iter()
        .filter(|p| p.agent == "claude-code")
        .map(|p| (p.pid, p))
        .collect();
    let mut seen = Vec::new();
    for (config, file) in crate::machine_sessions::session_files(root) {
        let Some(pid) = file["pid"].as_u64().and_then(|p| u32::try_from(p).ok()) else {
            continue;
        };
        let Some(proc) = procs.get(&pid) else {
            continue;
        };
        let (Some(session), Some(cwd)) = (file["sessionId"].as_str(), file["cwd"].as_str()) else {
            continue;
        };
        let millis = |key: &str| file[key].as_i64().map(|m| m / 1000);
        let started_at = proc.started_at.or_else(|| millis("startedAt")).unwrap_or(0);
        seen.push(Seen {
            pid,
            session: session.to_owned(),
            status: file["status"].as_str().unwrap_or("unknown").to_owned(),
            status_since: millis("statusUpdatedAt")
                .or_else(|| millis("updatedAt"))
                .unwrap_or(started_at),
            started_at,
            kind: file["kind"].as_str().unwrap_or("").to_owned(),
            cwd: cwd.to_owned(),
            last_write: crate::machine_sessions::session_last_write(&config, cwd, session),
            dashboards: crate::machine_sessions::ancestor_paths(pid)
                .iter()
                .any(|p| p.contains("Fabric Dashboards")),
            config,
        });
    }
    seen
}

/// The pids holding a session another, older process also holds.
pub fn newer_copies(seen: &[Seen]) -> std::collections::HashSet<u32> {
    let mut by: HashMap<&str, Vec<&Seen>> = HashMap::new();
    for s in seen {
        by.entry(s.session.as_str()).or_default().push(s);
    }
    by.values()
        .filter(|v| v.len() > 1)
        .flat_map(|v| {
            let oldest = v
                .iter()
                .min_by_key(|s| (s.started_at, s.pid))
                .map(|s| s.pid);
            v.iter().map(|s| s.pid).filter(move |p| Some(*p) != oldest)
        })
        .collect()
}

#[cfg(unix)]
/// The resume script of a session: the home's own exports (a managed or isolated home's
/// `launch.command`), its config folder, then `claude --resume <id>` in its folder.
pub fn resume_script(
    root: &Path,
    s: &Seen,
    program: &Path,
    bypass: &[&str],
    prompt: bool,
) -> String {
    let quote = |v: &str| format!("'{}'", v.replace('\'', "'\\''"));
    let mut out = format!(
        "#!{}\nset +x\nunset {}\n",
        crate::launch::SCRIPT_SHELL,
        crate::launch::CONFLICTS.join(" ")
    );
    let person = crate::person::claude_dir();
    if person.as_deref() != Some(s.config.as_path()) {
        if let Ok(text) = std::fs::read_to_string(s.config.join("launch.command")) {
            for line in text.lines().filter(|l| l.starts_with("export ")) {
                if !line.starts_with("export SWITCHBOARD_LAUNCH_ID=")
                    && !line.starts_with("export CLAUDE_CONFIG_DIR=")
                {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out.push_str(&format!(
            "export CLAUDE_CONFIG_DIR={}\n",
            quote(&s.config.to_string_lossy())
        ));
    }
    let _ = root;
    out.push_str(&format!(
        "cd {} || exit 1\nexec {}",
        quote(&s.cwd),
        quote(&program.to_string_lossy())
    ));
    for arg in bypass {
        out.push(' ');
        out.push_str(&quote(arg));
    }
    out.push_str(&format!(" --resume {}", quote(&s.session)));
    if prompt {
        out.push(' ');
        out.push_str(&quote(CONTINUE_PROMPT));
    }
    out.push('\n');
    out
}

fn journal(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(root.join(JOURNAL))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

fn append(root: &Path, line: Value) {
    let mut lines = journal(root);
    lines.push(line);
    let keep = lines.len().saturating_sub(JOURNAL_LINES);
    let text: String = lines[keep..].iter().map(|l| format!("{l}\n")).collect();
    let _ = switchboard_core::private_fs::private_write(&root.join(JOURNAL), text.as_bytes());
}

/// Restarts of one session within the window, from the journal.
fn restarts(lines: &[Value], session: &str, now: i64) -> usize {
    lines
        .iter()
        .filter(|l| {
            l["session"] == session
                && l["action"] == "restart"
                && l["at"].as_i64().is_some_and(|t| now - t < RESTART_WINDOW)
        })
        .count()
}

/// `switchboard duty`: the settings, the last actions and what a check would do now.
pub fn status(root: &Path, now: i64) -> Result<Value, String> {
    let set = settings(root)?;
    let seen = observe(root);
    let copies = newer_copies(&seen);
    let due: Vec<Value> = seen
        .iter()
        .filter_map(|s| {
            decide(s, &set, now, copies.contains(&s.pid)).map(|(act, reason)| {
                json!({"pid": s.pid, "session": s.session, "folder": s.cwd, "reason": reason,
                    "action": if act == Act::Close { "close" } else { "restart" }})
            })
        })
        .collect();
    let lines = journal(root);
    let recent: Vec<Value> = lines.iter().rev().take(20).cloned().collect();
    Ok(json!({
        "settings": set,
        "check_seconds": CHECK_SECONDS,
        "watched": seen.iter().filter(|s| s.kind == "interactive" && !s.dashboards).count(),
        "left_to_dashboards": seen.iter().filter(|s| s.dashboards).count(),
        "due": due,
        "recent": recent,
        "supported": cfg!(unix),
    }))
}

static LAST_CHECK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// From the one-minute session watch: at most every five minutes, with the settings on, acts.
pub fn tick(root: &Path, now: i64) {
    let last = LAST_CHECK.load(std::sync::atomic::Ordering::Relaxed);
    if now - last < CHECK_SECONDS {
        return;
    }
    LAST_CHECK.store(now, std::sync::atomic::Ordering::Relaxed);
    if settings(root).is_ok_and(|s| s.on) {
        let _ = check(root, now, false);
    }
}

/// One check: what is due, done (or only listed with `dry_run`).
pub fn check(root: &Path, now: i64, dry_run: bool) -> Result<Value, String> {
    let set = settings(root)?;
    let seen = observe(root);
    let copies = newer_copies(&seen);
    let lines = journal(root);
    let mut done = Vec::new();
    for s in &seen {
        if done.len() >= MAX_ACTIONS {
            break;
        }
        let Some((mut act, reason)) = decide(s, &set, now, copies.contains(&s.pid)) else {
            continue;
        };
        if matches!(act, Act::Restart { .. })
            && restarts(&lines, &s.session, now) >= RESTARTS_PER_WINDOW
        {
            // A session that keeps stalling is closed rather than restarted again.
            act = Act::Close;
        }
        let action = if act == Act::Close {
            "close"
        } else {
            "restart"
        };
        let outcome = if dry_run {
            "planned"
        } else {
            act_on(root, s, act)
        };
        let line = json!({"at": now, "action": action, "reason": reason, "outcome": outcome,
            "session": s.session, "folder": s.cwd, "pid": s.pid});
        if !dry_run {
            append(root, line.clone());
            crate::oplog::event(
                "session_duty",
                &[
                    ("action", crate::oplog::Field::Code(action)),
                    ("reason", crate::oplog::Field::Code(reason)),
                    ("outcome", crate::oplog::Field::Code(outcome)),
                ],
            );
        }
        done.push(line);
    }
    Ok(json!({"acted": done, "dry_run": dry_run}))
}

#[cfg(unix)]
fn act_on(root: &Path, s: &Seen, act: Act) -> &'static str {
    if !stop(s.pid) {
        return "still_running";
    }
    let Act::Restart { prompt } = act else {
        return "closed";
    };
    // Another process may hold the session by now (a host continued it): never a second copy.
    if crate::machine_sessions::live_sessions(root).contains_key(&s.session) {
        return "closed_held_elsewhere";
    }
    if !Path::new(&s.cwd).is_dir() {
        return "closed_no_folder";
    }
    let Some(program) = crate::launch::find_program("claude") else {
        return "closed_no_claude";
    };
    let bypass = crate::launch::bypass_args(root, switchboard_core::Provider::Claude, &[]);
    let script = resume_script(root, s, &program, &bypass, prompt);
    let dir = root.join("duty");
    let path = dir.join(format!(
        "{}.command",
        s.session
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
    ));
    if std::fs::create_dir_all(&dir).is_err()
        || crate::launch::write_script_at(&path, &script).is_err()
    {
        return "closed_not_restarted";
    }
    match crate::launch::start_terminal(&path) {
        Ok(()) => "restarted",
        Err(_) => "closed_not_restarted",
    }
}
#[cfg(not(unix))]
fn act_on(_: &Path, _: &Seen, _: Act) -> &'static str {
    "unsupported"
}

/// SIGTERM, then SIGKILL after ten seconds; whether the process is gone.
#[cfg(unix)]
fn stop(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    let alive = || unsafe { libc::kill(pid, 0) } == 0;
    // SAFETY: signals to a process of this user found in its own process table.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..20 {
        if !alive() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    unsafe { libc::kill(pid, libc::SIGKILL) };
    std::thread::sleep(std::time::Duration::from_millis(500));
    !alive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(status: &str, since_min: i64, age_h: i64, quiet_min: Option<i64>) -> Seen {
        let now = 1_000_000;
        Seen {
            pid: 7,
            session: "s-1".into(),
            status: status.into(),
            status_since: now - since_min * 60,
            started_at: now - age_h * 3600,
            kind: "interactive".into(),
            cwd: "/w/app".into(),
            config: PathBuf::from("/h/.claude"),
            last_write: quiet_min.map(|q| now - q * 60),
            dashboards: false,
        }
    }

    #[test]
    fn each_kind_of_stall_gets_its_action_and_a_working_session_is_left_alone() {
        let set = Settings::default();
        let now = 1_000_000;
        assert_eq!(
            decide(&seen("busy", 50, 1, Some(50)), &set, now, false),
            Some((Act::Restart { prompt: true }, "stuck"))
        );
        // Busy for long but still writing: working.
        assert_eq!(
            decide(&seen("busy", 300, 2, Some(1)), &set, now, false),
            None
        );
        assert_eq!(
            decide(&seen("shell", 50, 1, Some(46)), &set, now, false),
            Some((Act::Restart { prompt: true }, "stuck"))
        );
        assert_eq!(
            decide(&seen("waiting", 61, 1, None), &set, now, false),
            Some((Act::Restart { prompt: true }, "waiting"))
        );
        assert_eq!(
            decide(&seen("waiting", 30, 1, None), &set, now, false),
            None
        );
        assert_eq!(
            decide(&seen("idle", 61, 1, None), &set, now, false),
            Some((Act::Close, "idle"))
        );
        assert_eq!(
            decide(&seen("idle", 5, 5, None), &set, now, false),
            Some((Act::Restart { prompt: false }, "old"))
        );
        assert_eq!(
            decide(&seen("idle", 1, 5, None), &set, now, false),
            None,
            "a turn that just ended is left a moment"
        );
        assert_eq!(decide(&seen("idle", 5, 1, None), &set, now, false), None);
        assert_eq!(
            decide(&seen("idle", 5, 1, None), &set, now, true),
            Some((Act::Close, "duplicate"))
        );
        let mut console = seen("busy", 90, 9, Some(90));
        console.dashboards = true;
        assert_eq!(
            decide(&console, &set, now, false),
            None,
            "Fabric Dashboards owns its consoles"
        );
        let mut print = seen("busy", 90, 9, Some(90));
        print.kind = "print".into();
        assert_eq!(decide(&print, &set, now, false), None);
    }

    #[test]
    fn the_newer_copy_of_a_session_is_the_one_closed() {
        let mut a = seen("busy", 1, 3, None);
        let mut b = seen("busy", 1, 1, None);
        a.pid = 10;
        b.pid = 11;
        let other = Seen {
            session: "s-2".into(),
            pid: 12,
            ..seen("busy", 1, 1, None)
        };
        let copies = newer_copies(&[a, b, other]);
        assert_eq!(copies.into_iter().collect::<Vec<_>>(), vec![11]);
    }

    #[test]
    fn settings_default_on_reject_out_of_range_and_round_trip() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(settings(root.path()).unwrap(), Settings::default());
        assert!(save(
            root.path(),
            &Settings {
                stuck_minutes: 5,
                ..Settings::default()
            }
        )
        .is_err());
        let off = Settings {
            on: false,
            max_age_hours: 3,
            ..Settings::default()
        };
        save(root.path(), &off).unwrap();
        assert_eq!(settings(root.path()).unwrap(), off);
    }

    #[test]
    #[cfg(unix)]
    fn a_resume_script_quotes_every_word_and_keeps_a_homes_exports() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("launch.command"),
            "#!/bin/zsh\nexport ANTHROPIC_BASE_URL='http://127.0.0.1:9/x'\nexport SWITCHBOARD_LAUNCH_ID='L1'\nexec claude\n",
        )
        .unwrap();
        let s = Seen {
            config: home.path().to_owned(),
            cwd: "/w/it's".into(),
            ..seen("busy", 1, 1, None)
        };
        let script = resume_script(
            home.path(),
            &s,
            Path::new("/bin/claude"),
            &["--dangerously-skip-permissions"],
            true,
        );
        assert!(script.contains("export ANTHROPIC_BASE_URL='http://127.0.0.1:9/x'"));
        assert!(!script.contains("SWITCHBOARD_LAUNCH_ID"));
        assert!(script.contains("export CLAUDE_CONFIG_DIR="));
        assert!(script.contains("cd '/w/it'\\''s' || exit 1"));
        assert!(script
            .contains("'--dangerously-skip-permissions' --resume 's-1' 'Switchboard restarted"));
        let quiet = resume_script(home.path(), &s, Path::new("/bin/claude"), &[], false);
        assert!(quiet.trim_end().ends_with("--resume 's-1'"));
    }

    #[test]
    fn a_session_restarted_three_times_in_six_hours_is_counted() {
        let lines = vec![
            json!({"at": 100, "action": "restart", "session": "s-1"}),
            json!({"at": 200, "action": "restart", "session": "s-1"}),
            json!({"at": 300, "action": "close", "session": "s-1"}),
            json!({"at": 300, "action": "restart", "session": "s-2"}),
        ];
        assert_eq!(restarts(&lines, "s-1", 400), 2);
        assert_eq!(restarts(&lines, "s-1", 400 + RESTART_WINDOW), 0);
    }
}
