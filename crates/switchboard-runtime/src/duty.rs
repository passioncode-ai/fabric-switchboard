//! SB-384 (operator 2026-11-15): the session duty officer. Every live interactive Claude Code
//! session on this machine is looked at, and a session is never ended before it finished its work
//! (operator 2026-11-15: «чтобы они не … убивались раньше чем доработают, только когда закончила
//! сессия работу можно ее кикать, либо если диагноз застой»).
//!
//! What it reads: the process table (`ps`: each process's parent, age, CPU time and command), each
//! session's own file `sessions/<pid>.json`, in which Claude Code writes its status (`busy`,
//! `shell`, `waiting`, `idle`) and since when, and the modification time of that session's
//! transcript. Never a transcript's content. A session's own commands are the shells its Bash tool
//! starts (their command line names `.claude/shell-snapshots/`), foreground or background; its MCP
//! servers and helpers (`caffeinate`) are not work.
//!
//! What it does, at most once every five minutes and at most three times a check (DEC-0021):
//! - `idle` (finished): its turn ended `idle_minutes` ago and none of its commands still runs:
//!   closed; it can be resumed from Claude Code's history.
//! - `stuck` (stalled): busy, its status unchanged and its transcript not written for
//!   `stuck_minutes`, its whole process tree used no CPU since the last check, and no command of
//!   its own runs (or one has run for three times `stuck_minutes` without CPU): closed, then
//!   resumed in a new Terminal window (`claude --resume <id>`) with a prompt to continue.
//! - `duplicate`: a second process holding the same session: the newer one is closed.
//!
//! Left alone: a session waiting for the person's answer (a question or a permission), a session
//! with a command running, sessions in Fabric Dashboards' consoles (Dashboards owns them),
//! non-interactive runs (`claude -p`), Codex. Nothing is restarted for its age. A session is
//! restarted at most three times in six hours. `duty.json` (0600) keeps the person's settings,
//! `duty.jsonl` (0600) one line per action (session id, folder, reason; never text the session
//! wrote).
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
    /// Kept for settings written before 0.7.96; a waiting session is no longer acted on.
    pub waiting_minutes: u32,
    pub idle_minutes: u32,
    /// Kept for settings written before 0.7.96; nothing is restarted for its age.
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
    /// The program hosting its terminal: `dashboards`, `orca`, `terminal`, `iterm` or `other`.
    pub host: &'static str,
    /// Its own commands running now (Bash tool shells, foreground or background).
    pub commands: usize,
    /// How long its oldest running command has run, seconds.
    pub command_age: Option<i64>,
    /// CPU seconds its process and every descendant used, as `ps` reads it.
    pub cpu: Option<f64>,
    /// Whether its tree used (almost) no CPU since the previous check; unknown on the first look.
    pub cpu_quiet: Option<bool>,
}

/// The terminal host of a session from its ancestors' executable paths (DEC-0018: an Orca or
/// other host is acted on like Terminal and the resumed copy opens in Terminal).
pub fn host_of(ancestors: &[String]) -> &'static str {
    let has = |needle: &str| ancestors.iter().any(|p| p.contains(needle));
    if has("Fabric Dashboards") {
        "dashboards"
    } else if has("/orca/") {
        "orca"
    } else if has("Terminal.app") {
        "terminal"
    } else if has("iTerm") {
        "iterm"
    } else {
        "other"
    }
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
    let stuck = i64::from(set.stuck_minutes) * 60;
    match s.status.as_str() {
        // Finished: the turn ended long ago and nothing it started still runs (a background task
        // that will wake it is work, not a hang).
        "idle" if since >= i64::from(set.idle_minutes) * 60 && s.commands == 0 => {
            Some((Act::Close, "idle"))
        }
        // Stalled: says it works, writes nothing, uses no CPU, and runs no command of its own, or
        // one that has run very long without CPU. Unknown CPU (a first look) is never a stall.
        "busy" | "shell"
            if since >= stuck
                && quiet >= stuck
                && s.cpu_quiet == Some(true)
                && (s.commands == 0 || s.command_age.is_some_and(|a| a >= 3 * stuck)) =>
        {
            Some((Act::Restart { prompt: true }, "stuck"))
        }
        // `waiting` is a question or a permission for the person: never theirs to lose.
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
            dashboards: false,
            host: "other",
            commands: 0,
            command_age: None,
            cpu: None,
            cpu_quiet: None,
            config,
        });
    }
    let table = process_table();
    for s in &mut seen {
        s.host = host_of(&crate::machine_sessions::ancestor_paths(s.pid));
        s.dashboards = s.host == "dashboards";
        if let Some(table) = &table {
            let tree = tree_of(table, s.pid);
            s.cpu = Some(tree.cpu);
            s.commands = tree.commands.len();
            s.command_age = tree.commands.iter().copied().max();
        }
    }
    seen
}

/// One process as `ps` reads it: parent, age and CPU time in seconds, command line.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub ppid: u32,
    pub age: i64,
    pub cpu: f64,
    pub command: String,
}

/// `[[dd-]hh:]mm:ss[.ff]` (`ps` `etime` and `time`) in seconds.
fn clock_seconds(text: &str) -> Option<f64> {
    let (days, rest) = match text.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, text),
    };
    let mut total = 0.0;
    for part in rest.split(':') {
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(days * 86_400.0 + total)
}

/// The rows of `ps -axww -o pid=,ppid=,etime=,time=,command=`, by pid.
pub fn parse_table(text: &str) -> HashMap<u32, Row> {
    let mut rows = HashMap::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(pid), Some(ppid), Some(age), Some(cpu)) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            continue;
        };
        let (Ok(pid), Ok(ppid), Some(age), Some(cpu)) = (
            pid.parse::<u32>(),
            ppid.parse::<u32>(),
            clock_seconds(age),
            clock_seconds(cpu),
        ) else {
            continue;
        };
        let command = words.collect::<Vec<_>>().join(" ");
        rows.insert(
            pid,
            Row {
                ppid,
                age: age as i64,
                cpu,
                command,
            },
        );
    }
    rows
}

/// The whole process table once per check; None when `ps` could not be read (then no session
/// is found stalled or finished by its commands).
fn process_table() -> Option<HashMap<u32, Row>> {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("/bin/ps")
            .args(["-axww", "-o", "pid=,ppid=,etime=,time=,command="])
            .env("LC_ALL", "C")
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let rows = parse_table(&String::from_utf8_lossy(&out.stdout));
        (!rows.is_empty()).then_some(rows)
    }
    #[cfg(not(unix))]
    None
}

/// What a session's process tree holds: the CPU of the process and every descendant, and the
/// ages of its own commands (the shells its Bash tool starts, foreground or background).
pub struct Tree {
    pub cpu: f64,
    pub commands: Vec<i64>,
}
pub fn tree_of(table: &HashMap<u32, Row>, root: u32) -> Tree {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, row) in table {
        children.entry(row.ppid).or_default().push(*pid);
    }
    let mut tree = Tree {
        cpu: table.get(&root).map_or(0.0, |r| r.cpu),
        commands: Vec::new(),
    };
    let mut stack = children.get(&root).cloned().unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        let Some(row) = table.get(&pid) else {
            continue;
        };
        tree.cpu += row.cpu;
        if row.ppid == root && row.command.contains("/.claude/shell-snapshots/") {
            tree.commands.push(row.age);
        }
        stack.extend(children.get(&pid).cloned().unwrap_or_default());
    }
    tree
}

/// CPU seconds per session process at the previous check, and when.
static CPU_SEEN: std::sync::Mutex<Option<HashMap<u32, (f64, i64)>>> = std::sync::Mutex::new(None);
/// Less than this many CPU seconds since the previous check is no work.
const CPU_QUIET: f64 = 2.0;

/// Fills `cpu_quiet` against the previous check's sample (one taken at most two checks ago), and
/// keeps this check's samples for the next.
fn compare_cpu(seen: &mut [Seen], now: i64, keep: bool) {
    let Ok(mut kept) = CPU_SEEN.lock() else {
        return;
    };
    let before = kept.clone().unwrap_or_default();
    let mut next = HashMap::new();
    for s in seen.iter_mut() {
        let Some(cpu) = s.cpu else { continue };
        if let Some((was, at)) = before.get(&s.pid) {
            if now - at <= 2 * CHECK_SECONDS + 60 {
                s.cpu_quiet = Some(cpu - was < CPU_QUIET);
            }
        }
        next.insert(s.pid, (cpu, now));
    }
    if keep {
        *kept = Some(next);
    }
}

/// The pids to close among processes holding one session: each copy that is finished (idle, no
/// command of its own running) while another copy remains; when every copy is finished the oldest
/// stays. Copies that work are never closed, so two working copies are left for the person to
/// choose between (operator 2026-11-15: nothing is ended before it finished).
pub fn newer_copies(seen: &[Seen]) -> std::collections::HashSet<u32> {
    let mut by: HashMap<&str, Vec<&Seen>> = HashMap::new();
    for s in seen {
        by.entry(s.session.as_str()).or_default().push(s);
    }
    let finished = |s: &Seen| s.status == "idle" && s.commands == 0;
    by.values()
        .filter(|v| v.len() > 1)
        .flat_map(|v| {
            let keep = if v.iter().all(|s| finished(s)) {
                v.iter()
                    .min_by_key(|s| (s.started_at, s.pid))
                    .map(|s| s.pid)
            } else {
                None
            };
            v.iter()
                .filter(move |s| finished(s) && Some(s.pid) != keep)
                .map(|s| s.pid)
                .collect::<Vec<_>>()
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
    // A Terminal window runs the script without the person's login shell: the PATH a run started
    // by the owner gets, so the session's hooks find `node` (operator 2026-11-15: every hook failed
    // with "node: command not found").
    out.push_str(&format!(
        "export PATH={}\n",
        quote(&crate::launch::headless_path(program).to_string_lossy())
    ));
    out.push_str(&format!(
        "cd {} || exit 1\n{}",
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
    // Ended by a signal (the duty officer closing it, the person's Ctrl-C) is a clean end: the
    // window settings close the window on status 0, so it does not stay behind.
    out.push_str("\nstatus=$?\n[ \"$status\" -ge 128 ] && exit 0\nexit \"$status\"\n");
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
    let mut seen = observe(root);
    compare_cpu(&mut seen, now, false);
    let copies = newer_copies(&seen);
    let due: Vec<Value> = seen
        .iter()
        .filter_map(|s| {
            decide(s, &set, now, copies.contains(&s.pid)).map(|(act, reason)| {
                json!({"pid": s.pid, "session": s.session, "folder": s.cwd, "reason": reason, "host": s.host,
                    "action": if act == Act::Close { "close" } else { "restart" }})
            })
        })
        .collect();
    let lines = journal(root);
    let recent: Vec<Value> = lines.iter().rev().take(20).cloned().collect();
    let watched: Vec<&Seen> = seen
        .iter()
        .filter(|s| s.kind == "interactive" && !s.dashboards)
        .collect();
    Ok(json!({
        "settings": set,
        "check_seconds": CHECK_SECONDS,
        "watched": watched.len(),
        "left_to_dashboards": seen.iter().filter(|s| s.dashboards).count(),
        "waiting_for_person": watched.iter().filter(|s| s.status == "waiting").count(),
        "running_commands": watched.iter().filter(|s| s.commands > 0).count(),
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
    let mut seen = observe(root);
    // Only the five-minute checks keep their CPU samples: a stall is measured across them.
    compare_cpu(&mut seen, now, !dry_run);
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
            "session": s.session, "folder": s.cwd, "pid": s.pid, "host": s.host});
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
            host: "terminal",
            commands: 0,
            command_age: None,
            cpu: Some(10.0),
            cpu_quiet: Some(true),
        }
    }

    #[test]
    fn the_host_is_read_from_the_ancestors_and_dashboards_wins() {
        let paths = |v: &[&str]| v.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            host_of(&paths(&[
                "/Applications/Fabric Dashboards.app/Contents/MacOS/x",
                "/a/orca/daemon-host/macos/terminal-host-1"
            ])),
            "dashboards"
        );
        assert_eq!(
            host_of(&paths(&[
                "/Users/u/Library/Application Support/orca/daemon-host/macos/terminal-host-1"
            ])),
            "orca"
        );
        assert_eq!(
            host_of(&paths(&[
                "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
            ])),
            "terminal"
        );
        assert_eq!(host_of(&paths(&["/bin/zsh"])), "other");
    }

    #[test]
    fn only_a_finished_or_a_stalled_session_is_acted_on() {
        let set = Settings::default();
        let now = 1_000_000;
        // Stalled: busy, nothing written, no CPU, no command of its own.
        assert_eq!(
            decide(&seen("busy", 50, 1, Some(50)), &set, now, false),
            Some((Act::Restart { prompt: true }, "stuck"))
        );
        assert_eq!(
            decide(&seen("shell", 50, 1, Some(46)), &set, now, false),
            Some((Act::Restart { prompt: true }, "stuck"))
        );
        // Busy for long but still writing: working.
        assert_eq!(
            decide(&seen("busy", 300, 2, Some(1)), &set, now, false),
            None
        );
        // Writing nothing but using CPU (a long answer, a build): working.
        let thinking = Seen {
            cpu_quiet: Some(false),
            ..seen("busy", 90, 2, Some(90))
        };
        assert_eq!(decide(&thinking, &set, now, false), None);
        // The first look has no CPU sample yet: never a stall.
        let unknown = Seen {
            cpu_quiet: None,
            ..seen("busy", 90, 2, Some(90))
        };
        assert_eq!(decide(&unknown, &set, now, false), None);
        // A command of its own runs (a gate, a wait on a release): working, until it has run
        // three times the limit with no CPU at all.
        let waiting_on_a_command = Seen {
            commands: 1,
            command_age: Some(40 * 60),
            ..seen("busy", 50, 1, Some(50))
        };
        assert_eq!(decide(&waiting_on_a_command, &set, now, false), None);
        let hung_command = Seen {
            commands: 1,
            command_age: Some(3 * 45 * 60),
            ..seen("busy", 140, 3, Some(140))
        };
        assert_eq!(
            decide(&hung_command, &set, now, false),
            Some((Act::Restart { prompt: true }, "stuck"))
        );
        // Waiting for the person's answer: never acted on.
        assert_eq!(
            decide(&seen("waiting", 600, 9, None), &set, now, false),
            None
        );
        // Finished: idle long and nothing of its own runs.
        assert_eq!(
            decide(&seen("idle", 61, 1, None), &set, now, false),
            Some((Act::Close, "idle"))
        );
        // Idle with a background task that will wake it: not finished.
        let background = Seen {
            commands: 1,
            command_age: Some(3600),
            ..seen("idle", 120, 1, None)
        };
        assert_eq!(decide(&background, &set, now, false), None);
        // Nothing is restarted for its age.
        assert_eq!(decide(&seen("idle", 5, 30, None), &set, now, false), None);
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
    fn the_process_table_gives_a_sessions_commands_and_its_trees_cpu() {
        let table = parse_table(
            "  100     1 01-17:44:36  40:47.26 /opt/claude\n\
               101   100 01-17:44:36   0:01.33 switchboard mcp\n\
               102   100       33:13   0:00.03 /bin/zsh -c source /Users/u/.claude/shell-snapshots/snapshot-zsh-1.sh && eval 'cargo build'\n\
               103   102       33:13   1:00.50 cargo build\n\
               104   100       01:28   0:00.00 caffeinate -i -t 300\n\
               200     1    2-03:00:00  0:00:05 unrelated\n\
               bad line\n",
        );
        assert_eq!(table.len(), 6);
        assert_eq!(table[&103].cpu, 60.5);
        assert_eq!(table[&200].age, 2 * 86_400 + 3 * 3600);
        let tree = tree_of(&table, 100);
        assert_eq!(
            tree.commands,
            vec![33 * 60 + 13],
            "only the Bash tool's shell is a command"
        );
        assert!((tree.cpu - (2447.26 + 1.33 + 0.03 + 60.5)).abs() < 0.01);
        assert!(tree_of(&table, 999).commands.is_empty());
    }

    #[test]
    fn cpu_is_compared_across_checks_and_a_status_read_keeps_no_sample() {
        let mut a = vec![Seen {
            pid: 4242,
            cpu: Some(10.0),
            cpu_quiet: None,
            ..seen("busy", 1, 1, None)
        }];
        compare_cpu(&mut a, 1_000, false);
        assert_eq!(a[0].cpu_quiet, None);
        compare_cpu(&mut a, 1_000, true);
        let mut b = vec![Seen {
            pid: 4242,
            cpu: Some(10.5),
            cpu_quiet: None,
            ..seen("busy", 1, 1, None)
        }];
        compare_cpu(&mut b, 1_300, true);
        assert_eq!(b[0].cpu_quiet, Some(true));
        let mut c = vec![Seen {
            pid: 4242,
            cpu: Some(30.0),
            cpu_quiet: None,
            ..seen("busy", 1, 1, None)
        }];
        compare_cpu(&mut c, 1_600, true);
        assert_eq!(c[0].cpu_quiet, Some(false));
        let mut d = vec![Seen {
            pid: 4242,
            cpu: Some(30.0),
            cpu_quiet: None,
            ..seen("busy", 1, 1, None)
        }];
        compare_cpu(&mut d, 1_600 + 3 * CHECK_SECONDS, true);
        assert_eq!(d[0].cpu_quiet, None, "a sample from long ago says nothing");
    }

    #[test]
    fn only_a_finished_copy_of_a_session_is_closed() {
        let mut a = seen("busy", 1, 3, None);
        let mut b = seen("waiting", 1, 1, None);
        a.pid = 10;
        b.pid = 11;
        let other = Seen {
            session: "s-2".into(),
            pid: 12,
            ..seen("idle", 1, 1, None)
        };
        // Both copies work: the person decides, nothing is closed.
        assert!(newer_copies(&[a.clone(), b.clone(), other]).is_empty());
        // The older copy sits idle (a resumed copy nobody uses): it is closed, not the one in use.
        a.status = "idle".into();
        assert_eq!(
            newer_copies(&[a.clone(), b.clone()])
                .into_iter()
                .collect::<Vec<_>>(),
            vec![10]
        );
        // An idle copy with a background command is not finished.
        a.commands = 1;
        assert!(newer_copies(&[a.clone(), b.clone()]).is_empty());
        // Every copy finished: the oldest stays.
        a.commands = 0;
        b.status = "idle".into();
        assert_eq!(
            newer_copies(&[a, b]).into_iter().collect::<Vec<_>>(),
            vec![11]
        );
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
        assert!(!script.contains("export SWITCHBOARD_LAUNCH_ID"));
        assert!(script.contains("export CLAUDE_CONFIG_DIR="));
        assert!(script.contains("cd '/w/it'\\''s' || exit 1"));
        assert!(script
            .contains("'--dangerously-skip-permissions' --resume 's-1' 'Switchboard restarted"));
        let quiet = resume_script(home.path(), &s, Path::new("/bin/claude"), &[], false);
        assert!(quiet.contains("'/bin/claude' --resume 's-1'\nstatus=$?"));
        // The person's tools are found (hooks run `node`), and an end by a signal closes the
        // window (status 0 for the window settings), not `exec`.
        assert!(quiet.contains("export PATH='/bin:"));
        assert!(quiet.contains("/opt/homebrew/bin"));
        assert!(!quiet.contains("exec "));
        assert!(quiet.trim_end().ends_with("exit \"$status\""));
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
