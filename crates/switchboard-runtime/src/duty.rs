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
//! - `idle` (finished): its turn ended `idle_minutes` ago, and at least `WAKE_UP_FLOOR` ago, and
//!   none of its commands still runs: closed; it can be resumed from Claude Code's history. The
//!   floor (SB-392 S-1): a session that set itself a wake-up (`ScheduleWakeup`, at most an hour)
//!   reads `idle` while it waits and must not be closed in the minute before it wakes.
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
/// SB-392 S-1: the longest wake-up Claude Code lets a session set itself (`ScheduleWakeup` clamps
/// to 3600 s) plus five minutes: an `idle` session is finished only after it, whatever
/// `idle_minutes` says, so a loop waiting for its next turn is never closed.
pub const WAKE_UP_FLOOR: i64 = 3600 + 300;
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
    /// SB-396 D1: whether `commands` was read (`ps` answered). When it was not, nothing is known
    /// of the session's commands, and a session that might run one is never closed as finished.
    pub commands_known: bool,
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

/// SB-396 wave C (MON-2, MON-3): the session waits on a wake-up, a monitor or a cron job it set
/// up itself (`monitors`); nothing closes it while that wait holds.
fn monitoring(root: &Path, s: &Seen, now: i64) -> bool {
    crate::monitors::holds(
        &crate::monitors::read(root, &s.session),
        now,
        s.status_since,
    )
}

/// SB-396 D1: no command of its own runs, as far as the process table was read; unknown is not
/// none.
fn no_commands(s: &Seen) -> bool {
    s.commands_known && s.commands == 0
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
        "idle"
            if since >= (i64::from(set.idle_minutes) * 60).max(WAKE_UP_FLOOR) && no_commands(s) =>
        {
            Some((Act::Close, "idle"))
        }
        // Stalled: says it works, writes nothing, uses no CPU, and runs no command of its own, or
        // one that has run very long without CPU. Unknown CPU (a first look) is never a stall.
        "busy" | "shell"
            if since >= stuck
                && quiet >= stuck
                && s.cpu_quiet == Some(true)
                && (no_commands(s) || s.command_age.is_some_and(|a| a >= 3 * stuck)) =>
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
            commands_known: false,
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
            s.commands_known = true;
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
/// is found stalled or finished: `commands_known` stays false, SB-396 D1).
pub(crate) fn process_table() -> Option<HashMap<u32, Row>> {
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

/// The pids to close among processes holding one session: each copy that is finished (idle past
/// the wake-up floor, no command of its own running) while another copy remains; when every copy
/// is finished the oldest stays. Copies that work are never closed, so two working copies are left
/// for the person to choose between (operator 2026-11-15: nothing is ended before it finished).
/// SB-396 D4: a copy idle for less than `WAKE_UP_FLOOR` may hold a wake-up it set itself.
pub fn newer_copies(seen: &[Seen], now: i64) -> std::collections::HashSet<u32> {
    let mut by: HashMap<&str, Vec<&Seen>> = HashMap::new();
    for s in seen {
        by.entry(s.session.as_str()).or_default().push(s);
    }
    let finished =
        |s: &Seen| s.status == "idle" && no_commands(s) && now - s.status_since >= WAKE_UP_FLOOR;
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
    let _ = root;
    resume_script_for(
        &s.config,
        &s.cwd,
        &s.session,
        program,
        bypass,
        prompt.then_some(CONTINUE_PROMPT),
    )
}

/// SB-396 wave B: the resume script of session `session` begun in `cwd` under the Claude Code
/// config folder `config`, with `prompt` to go on with when given; the duty officer's restart and
/// a person's Continue share it.
pub fn resume_script_for(
    config: &Path,
    cwd: &str,
    session: &str,
    program: &Path,
    bypass: &[&str],
    prompt: Option<&str>,
) -> String {
    let quote = |v: &str| format!("'{}'", v.replace('\'', "'\\''"));
    let mut out = format!(
        "#!{}\nset +x\nunset {}\n",
        crate::launch::SCRIPT_SHELL,
        crate::launch::CONFLICTS.join(" ")
    );
    let person = crate::person::claude_dir();
    if person.as_deref() != Some(config) {
        if let Ok(text) = std::fs::read_to_string(config.join("launch.command")) {
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
            quote(&config.to_string_lossy())
        ));
    }
    // A Terminal window runs the script without the person's login shell: the PATH a run started
    // by the owner gets, so the session's hooks find `node` (operator 2026-11-15: every hook failed
    // with "node: command not found").
    out.push_str(&format!(
        "export PATH={}\n",
        quote(&crate::launch::headless_path(program).to_string_lossy())
    ));
    out.push_str(&format!(
        "cd {} || exit 1\n{}",
        quote(cwd),
        quote(&program.to_string_lossy())
    ));
    for arg in bypass {
        out.push(' ');
        out.push_str(&quote(arg));
    }
    out.push_str(&format!(" --resume {}", quote(session)));
    if let Some(prompt) = prompt {
        out.push(' ');
        out.push_str(&quote(prompt));
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
    // SB-397 W0.1: the owner's checks and an offline `close_copy` both append; one at a time.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("duty.lock"));
    #[cfg(unix)]
    if let Ok(file) = &lock {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    let _held = lock.ok().filter(|f| f.lock().is_ok());
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
                && l["phase"] != "intent"
                && l["at"].as_i64().is_some_and(|t| now - t < RESTART_WINDOW)
        })
        .count()
}

/// `switchboard duty`: the settings, the last actions and what a check would do now.
pub fn status(root: &Path, now: i64) -> Result<Value, String> {
    let set = settings(root)?;
    let mut seen = observe(root);
    compare_cpu(&mut seen, now, false);
    let copies = newer_copies(&seen, now);
    let stopped = crate::recovery::live_stop_sessions(root, now);
    let due: Vec<Value> = seen
        .iter()
        .filter(|s| !stopped.contains(&s.session) && !monitoring(root, s, now))
        .filter_map(|s| {
            decide(s, &set, now, copies.contains(&s.pid)).map(|(act, reason)| {
                json!({"pid": s.pid, "session": s.session, "folder": s.cwd, "reason": reason, "host": s.host,
                    "action": if act == Act::Close { "close" } else { "restart" }})
            })
        })
        .collect();
    let lines = journal(root);
    // An intent line is bookkeeping for a cut act (SB-397 W0.1); the outcome line is the act.
    let recent: Vec<Value> = lines
        .iter()
        .rev()
        .filter(|l| l["phase"] != "intent")
        .take(20)
        .cloned()
        .collect();
    let watched: Vec<&Seen> = seen
        .iter()
        .filter(|s| s.kind == "interactive" && !s.dashboards)
        .collect();
    Ok(json!({
        "settings": set,
        "check_seconds": CHECK_SECONDS,
        // SB-392 S-1: when a finished session is closed, after the wake-up floor.
        "idle_closes_after_minutes": (i64::from(set.idle_minutes) * 60).max(WAKE_UP_FLOOR) / 60,
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
    let copies = newer_copies(&seen, now);
    // SB-396 D2: a session stopped on a limit waits for an account; its host continues it.
    let stopped = crate::recovery::live_stop_sessions(root, now);
    let waving = crate::restarts::in_wave(root, now);
    let mut done = Vec::new();
    // SB-397 W0.1: an act a quitting owner cut between its intent and its outcome is finished.
    if !dry_run {
        done.extend(finish_interrupted(root, now));
    }
    let lines = journal(root);
    for s in &seen {
        if done.len() >= MAX_ACTIONS {
            break;
        }
        // SB-397 R1: a session in a restart wave is the wave's.
        if stopped.contains(&s.session) || monitoring(root, s, now) || waving.contains(&s.session) {
            continue;
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
        let prompt = matches!(act, Act::Restart { prompt: true });
        let outcome = if dry_run {
            "planned"
        } else {
            // SB-397 W0.1: the intent first, with what a later owner needs to finish the act
            // when this one is cut by a quit (a close takes up to 10.5 s, the drain 8 s).
            append(
                root,
                json!({"at": now, "phase": "intent", "action": action, "reason": reason,
                    "session": s.session, "folder": s.cwd, "pid": s.pid, "host": s.host,
                    "config": s.config, "prompt": prompt}),
            );
            act_on(root, s, act)
        };
        let line = json!({"at": now, "action": action, "reason": reason, "outcome": outcome,
            "session": s.session, "folder": s.cwd, "pid": s.pid, "host": s.host});
        if !dry_run {
            append(root, line.clone());
            // SB-396 D5: what a later Continue needs to resume this very session.
            if outcome.starts_with("closed") {
                crate::resumable::record(
                    root,
                    json!({"session": s.session, "config": s.config, "cwd": s.cwd, "reason": reason,
                        "at": now, "by": "duty"}),
                );
            }
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
    resume(root, &s.session, &s.config, &s.cwd, prompt)
}

/// The second half of a restart: the stopped session resumed as itself in a Terminal window.
#[cfg(unix)]
fn resume(root: &Path, session: &str, config: &Path, cwd: &str, prompt: bool) -> &'static str {
    // Another process may hold the session by now (a host continued it): never a second copy.
    if crate::machine_sessions::live_sessions(root).contains_key(session) {
        return "closed_held_elsewhere";
    }
    if !Path::new(cwd).is_dir() {
        return "closed_no_folder";
    }
    let Some(program) = crate::launch::find_program("claude") else {
        return "closed_no_claude";
    };
    let bypass = crate::launch::bypass_args(root, switchboard_core::Provider::Claude, &[]);
    let script = resume_script_for(
        config,
        cwd,
        session,
        &program,
        &bypass,
        prompt.then_some(CONTINUE_PROMPT),
    );
    let dir = root.join("duty");
    let path = dir.join(format!(
        "{}.command",
        session
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

/// How long after its intent a cut act is still finished; older ones are left as they are.
const INTERRUPTED_WINDOW: i64 = 3600;

/// SB-397 W0.1: the acts whose intent has no outcome line, newest intent per session, within
/// `INTERRUPTED_WINDOW`. Pure, for tests.
pub fn interrupted(lines: &[Value], now: i64) -> Vec<Value> {
    let mut open: Vec<Value> = Vec::new();
    for line in lines {
        let key = (line["session"].clone(), line["at"].clone());
        if line["phase"] == "intent" {
            open.retain(|o| o["session"] != line["session"]);
            open.push(line.clone());
        } else {
            open.retain(|o| (o["session"].clone(), o["at"].clone()) != key);
        }
    }
    open.retain(|o| {
        o["at"]
            .as_i64()
            .is_some_and(|at| now - at < INTERRUPTED_WINDOW)
    });
    open
}

/// Finishes each cut act: a close whose process is gone is recorded as closed (and resumable); a
/// restart whose process is gone is resumed; a process still running was never stopped, so the
/// act is recorded as `interrupted` and the next check decides again.
fn finish_interrupted(root: &Path, now: i64) -> Vec<Value> {
    let mut done = Vec::new();
    for intent in interrupted(&journal(root), now) {
        let (Some(session), Some(cwd), Some(pid)) = (
            intent["session"].as_str(),
            intent["folder"].as_str(),
            intent["pid"].as_u64(),
        ) else {
            continue;
        };
        let config = PathBuf::from(intent["config"].as_str().unwrap_or_default());
        let gone = !process_alive(pid as u32);
        let outcome = if !gone {
            "interrupted"
        } else if intent["action"] == "restart" {
            finish_restart(root, session, &config, cwd, intent["prompt"] == true)
        } else {
            "closed"
        };
        let line = json!({"at": intent["at"], "action": intent["action"], "reason": intent["reason"],
            "outcome": outcome, "session": session, "folder": cwd, "pid": pid, "host": intent["host"],
            "finished_at": now});
        append(root, line.clone());
        if outcome.starts_with("closed") {
            crate::resumable::record(
                root,
                json!({"session": session, "config": config, "cwd": cwd,
                    "reason": intent["reason"], "at": now, "by": "duty"}),
            );
        }
        crate::oplog::event(
            "session_duty",
            &[
                ("action", crate::oplog::Field::Code("finish_interrupted")),
                ("outcome", crate::oplog::Field::Code(outcome)),
            ],
        );
        done.push(line);
    }
    done
}

#[cfg(unix)]
fn finish_restart(
    root: &Path,
    session: &str,
    config: &Path,
    cwd: &str,
    prompt: bool,
) -> &'static str {
    resume(root, session, config, cwd, prompt)
}
#[cfg(not(unix))]
fn finish_restart(_: &Path, _: &str, _: &Path, _: &str, _: bool) -> &'static str {
    "closed_not_restarted"
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
#[cfg(not(unix))]
fn process_alive(_: u32) -> bool {
    true
}

pub const NOT_TWO: &str = "That session no longer runs in two processes. Refresh to see it now.";
pub const ALL_WORKING: &str =
    "Both copies of this session are working. Close one in its own window when it finishes.";
pub const COPY_BUSY: &str =
    "That copy is answering right now. Close it in its own window when it finishes.";
pub const COPY_STILL_RUNNING: &str = "The copy did not close. Close it in its own window.";
pub const CLOSE_UNSUPPORTED: &str = "Closing a session's copy works on macOS and Linux for now.";

/// A copy that works: answering (`busy`) or with a command of its own running.
fn working(s: &Seen) -> bool {
    s.status == "busy" || !no_commands(s)
}

/// Of the copies of `session`, the one a person closes and the ones kept: never a copy that is
/// working (busy, or one of its commands running); of the others, the one whose status has not
/// changed for longest (then the one started last). `NOT_TWO` with fewer than two copies,
/// `ALL_WORKING` when every copy works.
pub fn quiet_copy<'a>(
    seen: &'a [Seen],
    session: &str,
) -> Result<(&'a Seen, Vec<&'a Seen>), &'static str> {
    let copies: Vec<&Seen> = seen.iter().filter(|s| s.session == session).collect();
    if copies.len() < 2 {
        return Err(NOT_TWO);
    }
    let close = *copies
        .iter()
        .filter(|s| !working(s))
        .min_by_key(|s| (s.status_since, std::cmp::Reverse((s.started_at, s.pid))))
        .ok_or(ALL_WORKING)?;
    let kept = copies.into_iter().filter(|s| s.pid != close.pid).collect();
    Ok((close, kept))
}

/// SB-389: when every copy works, the one a person may still choose to close: never one that is
/// answering (`busy`); of those waiting on a command of their own, the one whose status has not
/// changed for longest. On the operator's MacBook (2026-11-16) the old copy Switchboard 0.7.93
/// reopened in Terminal had polled a remote job for two hours beside the person's copy at work.
pub fn older_copy<'a>(seen: &'a [Seen], session: &str) -> Option<&'a Seen> {
    let copies: Vec<&Seen> = seen.iter().filter(|s| s.session == session).collect();
    if copies.len() < 2 {
        return None;
    }
    copies
        .into_iter()
        .filter(|s| s.status != "busy")
        .min_by_key(|s| (s.status_since, std::cmp::Reverse((s.started_at, s.pid))))
}

/// SB-389: what Needs attention offers on a duplicate: `{pid, why: quiet}` (a copy that is not
/// working), `{pid, why: older}` (every copy works; the one waiting longest on its command), or
/// null (every copy is answering).
pub fn suggestion(seen: &[Seen], session: &str) -> Value {
    match quiet_copy(seen, session) {
        Ok((close, _)) => json!({"pid": close.pid, "why": "quiet"}),
        Err(ALL_WORKING) => older_copy(seen, session).map_or(Value::Null, |s| {
            json!({"pid": s.pid, "why": "older", "command_age": s.command_age, "status_since": s.status_since})
        }),
        Err(_) => Value::Null,
    }
}

/// SB-389: each copy of `session` as Needs attention shows it: pid, status and since when, start,
/// commands running and the oldest one's age, where it runs. No text the session wrote.
pub fn copies(seen: &[Seen], session: &str) -> Vec<Value> {
    let mut out: Vec<&Seen> = seen.iter().filter(|s| s.session == session).collect();
    out.sort_by_key(|s| (s.started_at, s.pid));
    out.into_iter()
        .map(|s| {
            json!({
                "pid": s.pid, "status": s.status, "status_since": s.status_since,
                "started_at": s.started_at, "commands": s.commands,
                "command_age": s.command_age, "host": s.host,
            })
        })
        .collect()
}

/// SB-388, SB-389: the person's *Close the quiet copy* or *Close the older copy* on a duplicate
/// (Needs attention). Without `pid`, the copy `quiet_copy` names; with it, that copy of `session`,
/// refused while it is answering (`COPY_BUSY`): one of its own commands still running ends with
/// it, as the window said before the press. SIGTERM, then SIGKILL after ten seconds; the other
/// keeps working. Journalled in `duty.jsonl` with `by: person`.
pub fn close_copy(root: &Path, session: &str, pid: Option<u32>, now: i64) -> Result<Value, String> {
    #[cfg(not(unix))]
    {
        let _ = (root, session, pid, now);
        Err(CLOSE_UNSUPPORTED.into())
    }
    #[cfg(unix)]
    {
        let seen = observe(root);
        let (close, kept) = chosen_copy(&seen, session, pid)?;
        let closed = stop(close.pid);
        append(
            root,
            json!({
                "at": now, "session": session, "pid": close.pid, "cwd": close.cwd,
                "reason": "duplicate", "by": "person",
                "result": if closed { "closed" } else { "still_running" },
            }),
        );
        if !closed {
            return Err(COPY_STILL_RUNNING.into());
        }
        if kept.len() < 2 {
            crate::session_watch::forget(root, crate::session_watch::DUPLICATE, session);
        }
        Ok(json!({
            "session_id": session,
            "closed_pid": close.pid,
            "kept_pids": kept.iter().map(|s| s.pid).collect::<Vec<_>>(),
        }))
    }
}

/// The copy `close_copy` closes and the ones kept.
pub fn chosen_copy<'a>(
    seen: &'a [Seen],
    session: &str,
    pid: Option<u32>,
) -> Result<(&'a Seen, Vec<&'a Seen>), &'static str> {
    let Some(pid) = pid else {
        return quiet_copy(seen, session);
    };
    let copies: Vec<&Seen> = seen.iter().filter(|s| s.session == session).collect();
    let close = *copies.iter().find(|s| s.pid == pid).ok_or(NOT_TWO)?;
    if copies.len() < 2 {
        return Err(NOT_TWO);
    }
    if close.status == "busy" {
        return Err(COPY_BUSY);
    }
    let kept = copies.into_iter().filter(|s| s.pid != pid).collect();
    Ok((close, kept))
}

/// SIGTERM, then SIGKILL after ten seconds; whether the process is gone.
#[cfg(unix)]
pub(crate) fn stop(pid: u32) -> bool {
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

    #[test]
    fn an_intent_without_its_outcome_is_an_act_cut_by_a_quit() {
        let intent = |session: &str, at: i64| json!({"at": at, "phase": "intent", "action": "close", "session": session, "pid": 1});
        let outcome = |session: &str, at: i64| json!({"at": at, "action": "close", "outcome": "closed", "session": session});
        let lines = vec![
            intent("S1", 100),
            outcome("S1", 100),
            intent("S2", 200),
            intent("S3", 300),
            outcome("S3", 300),
            intent("S3", 400),
        ];
        let open = interrupted(&lines, 500);
        let open: Vec<_> = open
            .iter()
            .map(|v| (v["session"].clone(), v["at"].clone()))
            .collect();
        assert_eq!(
            open,
            vec![(json!("S2"), json!(200)), (json!("S3"), json!(400))]
        );
        assert!(
            interrupted(&lines, 400 + INTERRUPTED_WINDOW).is_empty(),
            "an act cut more than an hour ago is left as it is"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_cut_close_is_finished_by_the_next_owner_and_a_live_process_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let gone = child.id();
        child.kill().unwrap();
        child.wait().unwrap();
        let live = std::process::id();
        append(
            root.path(),
            json!({"at": 1000, "phase": "intent", "action": "close", "reason": "idle",
                "session": "S1", "folder": "/w/a", "pid": gone, "host": "terminal",
                "config": "/h/.claude", "prompt": false}),
        );
        append(
            root.path(),
            json!({"at": 1000, "phase": "intent", "action": "close", "reason": "idle",
                "session": "S2", "folder": "/w/b", "pid": live, "host": "terminal",
                "config": "/h/.claude", "prompt": false}),
        );
        let done = finish_interrupted(root.path(), 1100);
        let outcomes: Vec<_> = done
            .iter()
            .map(|l| (l["session"].clone(), l["outcome"].clone()))
            .collect();
        assert_eq!(
            outcomes,
            vec![
                (json!("S1"), json!("closed")),
                (json!("S2"), json!("interrupted"))
            ]
        );
        assert_eq!(
            crate::resumable::recent(root.path(), 0)[0]["session"],
            "S1",
            "the closed session can be continued as itself"
        );
        assert!(
            finish_interrupted(root.path(), 1200).is_empty(),
            "finished once"
        );
        assert_eq!(
            restarts(&journal(root.path()), "S1", 1200),
            0,
            "an intent never counts as a restart"
        );
    }

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
            commands_known: true,
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
            decide(&seen("idle", 66, 1, None), &set, now, false),
            Some((Act::Close, "idle"))
        );
        // SB-392 S-1: idle past `idle_minutes` but inside the longest wake-up a session can set
        // itself: it may still be waiting for its next turn, so it is not finished yet, even with
        // `idle_minutes` at its floor.
        assert_eq!(decide(&seen("idle", 61, 1, None), &set, now, false), None);
        let short = Settings {
            idle_minutes: 10,
            ..set.clone()
        };
        assert_eq!(decide(&seen("idle", 30, 1, None), &short, now, false), None);
        assert_eq!(
            decide(&seen("idle", 66, 1, None), &short, now, false),
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
    fn a_person_closes_the_copy_quiet_longest_and_never_a_working_one() {
        // The case on the operator's MacBook (2026-11-15): the older copy works, the newer one has
        // waited for five hours.
        let copy = |pid: u32, status: &str, started_at: i64, status_since: i64| Seen {
            pid,
            started_at,
            status_since,
            ..seen(status, 1, 1, None)
        };
        let all = [
            copy(84681, "busy", 100, 9_000),
            copy(38550, "waiting", 200, 1_000),
            Seen {
                session: "other".into(),
                ..copy(30, "idle", 100, 100)
            },
        ];
        let session = all[0].session.clone();
        let (close, kept) = quiet_copy(&all, &session).unwrap();
        assert_eq!(close.pid, 38550, "the quiet one, though it started later");
        assert_eq!(kept.iter().map(|s| s.pid).collect::<Vec<_>>(), [84681]);
        // Two quiet copies: the one quiet longer; quiet as long: the one started last.
        let two = [copy(1, "idle", 100, 5_000), copy(2, "idle", 200, 4_000)];
        assert_eq!(quiet_copy(&two, &session).unwrap().0.pid, 2);
        let tie = [copy(1, "idle", 100, 5_000), copy(2, "idle", 200, 5_000)];
        assert_eq!(quiet_copy(&tie, &session).unwrap().0.pid, 2);
        // A copy running a command works too; when every copy works nothing is closed.
        let mut running = copy(3, "idle", 300, 1);
        running.commands = 1;
        let both = [copy(4, "busy", 100, 9_000), running];
        assert_eq!(quiet_copy(&both, &session).unwrap_err(), ALL_WORKING);
        assert_eq!(quiet_copy(&all, "other").unwrap_err(), NOT_TWO);
        assert_eq!(quiet_copy(&all, "absent").unwrap_err(), NOT_TWO);
    }

    #[test]
    fn when_both_copies_work_the_one_waiting_longest_on_its_command_is_offered() {
        // The operator's MacBook, 2026-11-16: the old copy (reopened by 0.7.93 in Terminal) polls
        // a remote job, its status unchanged for 108 minutes; the person's copy is answering.
        let copy = |pid: u32, status: &str, commands: usize, status_since: i64| Seen {
            pid,
            commands,
            status_since,
            command_age: (commands > 0).then_some(8_700),
            started_at: pid as i64,
            ..seen(status, 1, 1, None)
        };
        let all = [
            copy(84681, "shell", 1, 1_000),
            copy(55682, "busy", 0, 7_400),
        ];
        let session = all[0].session.clone();
        assert_eq!(quiet_copy(&all, &session).unwrap_err(), ALL_WORKING);
        assert_eq!(older_copy(&all, &session).unwrap().pid, 84681);
        let offer = suggestion(&all, &session);
        assert_eq!(offer["why"], "older");
        assert_eq!(offer["pid"], 84681);
        assert_eq!(copies(&all, &session).len(), 2);
        // The person's press names that copy; the answering one is refused.
        assert_eq!(
            chosen_copy(&all, &session, Some(84681)).unwrap().0.pid,
            84681
        );
        assert_eq!(
            chosen_copy(&all, &session, Some(55682)).unwrap_err(),
            COPY_BUSY
        );
        assert_eq!(chosen_copy(&all, &session, Some(1)).unwrap_err(), NOT_TWO);
        // Both answering: nothing is offered.
        let busy = [copy(1, "busy", 0, 10), copy(2, "busy", 0, 20)];
        assert!(suggestion(&busy, &session).is_null());
        // A quiet copy is still the first choice.
        let quiet = [copy(3, "idle", 0, 5), copy(4, "busy", 0, 50)];
        assert_eq!(suggestion(&quiet, &session)["why"], "quiet");
    }

    #[test]
    fn only_a_finished_copy_of_a_session_is_closed() {
        let now = 1_000_000;
        let mut a = seen("busy", 70, 3, None);
        let mut b = seen("waiting", 70, 1, None);
        a.pid = 10;
        b.pid = 11;
        let other = Seen {
            session: "s-2".into(),
            pid: 12,
            ..seen("idle", 70, 1, None)
        };
        // Both copies work: the person decides, nothing is closed.
        assert!(newer_copies(&[a.clone(), b.clone(), other], now).is_empty());
        // The older copy sits idle (a resumed copy nobody uses): it is closed, not the one in use.
        a.status = "idle".into();
        assert_eq!(
            newer_copies(&[a.clone(), b.clone()], now)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![10]
        );
        // SB-396 D4: idle for less than the wake-up floor, it may hold a wake-up of its own.
        let mut recent = a.clone();
        recent.status_since = now - 30 * 60;
        assert!(newer_copies(&[recent, b.clone()], now).is_empty());
        // An idle copy with a background command is not finished, nor one whose commands are unknown.
        a.commands = 1;
        assert!(newer_copies(&[a.clone(), b.clone()], now).is_empty());
        a.commands = 0;
        a.commands_known = false;
        assert!(newer_copies(&[a.clone(), b.clone()], now).is_empty());
        a.commands_known = true;
        // Every copy finished: the oldest stays.
        b.status = "idle".into();
        assert_eq!(
            newer_copies(&[a, b], now).into_iter().collect::<Vec<_>>(),
            vec![11]
        );
    }

    #[test]
    fn a_session_whose_commands_could_not_be_read_is_never_closed_as_finished() {
        // SB-396 D1: `ps` failed under load; nothing says the session runs no command.
        let set = Settings::default();
        let now = 1_000_000;
        let mut unread = seen("idle", 180, 5, None);
        unread.commands_known = false;
        assert_eq!(decide(&unread, &set, now, false), None);
        let mut stalled = seen("busy", 300, 6, Some(300));
        stalled.commands_known = false;
        assert_eq!(
            decide(&stalled, &set, now, false),
            None,
            "no stall is diagnosed blind"
        );
        unread.commands_known = true;
        assert_eq!(
            decide(&unread, &set, now, false),
            Some((Act::Close, "idle"))
        );
    }

    #[test]
    fn a_session_stopped_on_a_limit_is_left_to_its_host() {
        // SB-396 D2: the duty officer's check skips every session with a waiting or claimed stop.
        let root = tempfile::tempdir().unwrap();
        let now = 1_000_000;
        // A folder that is absolute on every platform (`/w/app` is not one on Windows).
        let folder = root.path().to_path_buf();
        let stop = |session: &str, state: &str| {
            serde_json::json!({
                "event": format!("cc-{session}-1"), "session": session, "account": "a", "provider": "claude",
                "pool": "default", "directory": folder, "stopped_at": now - 4000, "state": state,
                "selected_account": null
            })
        };
        switchboard_core::private_fs::private_write(
            &root.path().join("recovery.json"),
            &serde_json::to_vec(&serde_json::json!([
                stop("s-1", "waiting"),
                stop("s-2", "resumed")
            ]))
            .unwrap(),
        )
        .unwrap();
        let stopped = crate::recovery::live_stop_sessions(root.path(), now);
        assert!(stopped.contains("s-1"));
        assert!(!stopped.contains("s-2"));
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
