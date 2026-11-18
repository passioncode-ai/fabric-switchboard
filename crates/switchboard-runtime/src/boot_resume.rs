//! SB-450 (Fabric Dashboards, board request 737b04e6; DEC-0030): sessions go on after this
//! computer restarts.
//!
//! A restart ends every session with no end recorded: its host dies with it, the duty officer
//! looks only at live processes, and recovery only at sessions stopped on a limit. So the
//! one-minute session watch keeps `boot-live.json` (0600: the boot time, and the session id and
//! launch id of every live Claude Code session a Switchboard launch runs; rewritten when that set
//! or the boot changes, every 30 minutes while such sessions run, and once at the owner's drain
//! with when it stopped), and the first watch pass of an owner after a restart reads the ledger the previous boot left and continues,
//! as itself and in a session host of its own, each session the restart cut:
//!
//! - its launch has no end recorded, has not been continued, ran Claude Code and started before
//!   the boot (a dispatched board run is continued only when it has no end either);
//! - it was live at the owner's last look before the boot, and that look was no older than
//!   `WINDOW_SECONDS` before the boot, nor older than the session's last transcript write (when the
//!   owner stopped before the session last wrote, nobody saw it still running);
//! - its transcript was last written in the `WINDOW_SECONDS` before the boot, and not since;
//! - no process and no session host holds it now, no launch names it since the boot, and this
//!   boot's journal does not name it yet;
//! - at most `MAX_PER_BOOT` of them, newest transcript first, each inside the session pool
//!   (`capacity`): a start the pool refuses ends the pass.
//!
//! A continuation is `resumable::continue_as` hosted (`claude --resume <id>` with its prompt, in
//! its own config and working folders), started with `session_host::start_detached`, so Fabric
//! Dashboards and other machines attach to it by its launch id. `boot-resume.jsonl` (0600, the
//! newest 500 lines: boot time, session and launch ids, folder, outcome; never text a session
//! wrote) gets an `intent` line before each start and its outcome after, so an owner that quits in
//! between never starts it twice: the next owner settles that intent (`continued` when a process
//! or host holds the session, `interrupted` otherwise) and never repeats it. `boot-resume.json`
//! (0600) is the switch, on unless the person turns it off.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const FILE: &str = "boot-resume.json";
pub const JOURNAL: &str = "boot-resume.jsonl";
pub const LIVE: &str = "boot-live.json";
/// How long before the boot a session's transcript may have been written last.
pub const WINDOW_SECONDS: i64 = 2 * 3600;
/// At most this many sessions are continued after one boot.
pub const MAX_PER_BOOT: usize = 5;
/// While sessions run, the ledger's look is renewed this often even when nothing changed, so a
/// set that held for hours still reads as seen shortly before the boot.
pub const REFRESH_SECONDS: i64 = 1800;
/// A transcript written this long after the owner's last look still counts as seen then.
const LOOK_SLACK_SECONDS: i64 = 120;
const JOURNAL_LINES: usize = 500;
const MAX_BYTES: u64 = 256 * 1024;
/// Who the continuations are recorded as launched by.
pub const BY: &str = "boot-resume";

pub const UNREADABLE: &str = "The setting for continuing sessions after a restart (boot-resume.json) could not be read. Set it again, or delete the file for the default.";
pub const NOT_SAVED: &str =
    "The setting for continuing sessions after a restart could not be saved.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub on: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings { on: true }
    }
}

/// The person's setting; on when no file holds one.
pub fn settings(root: &Path) -> Result<Settings, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes = switchboard_core::private_fs::read_private(&path, 4096).map_err(|_| UNREADABLE)?;
    serde_json::from_slice(&bytes).map_err(|_| UNREADABLE.into())
}

pub fn save(root: &Path, settings: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec(settings).map_err(|_| NOT_SAVED)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| NOT_SAVED.into())
}

/// `btime` of Linux's `/proc/stat`: the boot time in Unix seconds.
pub fn parse_btime(stat: &str) -> Option<i64> {
    stat.lines()
        .find_map(|l| l.strip_prefix("btime "))
        .and_then(|v| v.trim().parse().ok())
}

/// When this computer last booted, Unix seconds: `kern.boottime` on macOS, `btime` on Linux.
#[cfg(target_os = "macos")]
pub fn boot_time() -> Option<i64> {
    let mut time = std::mem::MaybeUninit::<libc::timeval>::zeroed();
    let mut size = std::mem::size_of::<libc::timeval>();
    // SAFETY: a read of one fixed-size kernel value into a buffer of its size.
    let got = unsafe {
        libc::sysctlbyname(
            c"kern.boottime".as_ptr(),
            time.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if got != 0 || size != std::mem::size_of::<libc::timeval>() {
        return None;
    }
    // SAFETY: filled above, its size checked.
    let time = unsafe { time.assume_init() };
    (time.tv_sec > 0).then_some(time.tv_sec)
}
#[cfg(target_os = "linux")]
pub fn boot_time() -> Option<i64> {
    parse_btime(&std::fs::read_to_string("/proc/stat").ok()?)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn boot_time() -> Option<i64> {
    None
}

/// One session a Switchboard launch ran, as the watch saw it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Seen {
    pub session: String,
    pub launch_id: String,
}

/// The ledger: the live launched sessions at the owner's last look, in the boot it looked in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub boot: i64,
    /// When the set last changed.
    pub at: i64,
    pub sessions: Vec<Seen>,
    /// When the owner that kept it stopped through its drain; none while it runs, or after a
    /// crash or a power loss.
    #[serde(default)]
    pub stopped_at: Option<i64>,
}

fn read_ledger(root: &Path) -> Option<Ledger> {
    let bytes = switchboard_core::private_fs::read_private(&root.join(LIVE), MAX_BYTES).ok()?;
    serde_json::from_slice(&bytes).ok()
}
fn write_ledger(root: &Path, ledger: &Ledger) {
    if let Ok(bytes) = serde_json::to_vec(ledger) {
        let _ = switchboard_core::private_fs::private_write(&root.join(LIVE), &bytes);
    }
}

/// The live launched sessions of a scan: each row with a launch whose pid holds a session.
pub fn seen_of(rows: &[Value], pairs: &[(String, u32)]) -> Vec<Seen> {
    let session_of: HashMap<u32, &str> = pairs.iter().map(|(s, p)| (*p, s.as_str())).collect();
    let mut seen: Vec<Seen> = rows
        .iter()
        .filter(|r| r["agent"] == "claude-code")
        .filter_map(|r| {
            let launch_id = r["launch_id"].as_str()?;
            let pid = u32::try_from(r["pid"].as_u64()?).ok()?;
            Some(Seen {
                session: session_of.get(&pid)?.to_string(),
                launch_id: launch_id.to_owned(),
            })
        })
        .collect();
    seen.sort();
    seen.dedup();
    seen
}

/// The watch's look: the ledger is rewritten when its boot or its sessions changed, and while
/// sessions run every `REFRESH_SECONDS`; with no session running nothing is written. Called after
/// `tick`, which reads the previous boot's.
pub fn observe(root: &Path, boot: i64, seen: Vec<Seen>, now: i64) {
    let kept = read_ledger(root);
    if kept.as_ref().is_some_and(|l| {
        l.boot == boot
            && l.sessions == seen
            && l.stopped_at.is_none()
            && (seen.is_empty() || (0..REFRESH_SECONDS).contains(&(now - l.at)))
    }) {
        return;
    }
    write_ledger(
        root,
        &Ledger {
            boot,
            at: now,
            sessions: seen,
            stopped_at: None,
        },
    );
}

/// The owner's drain: the ledger of this boot says when its watch stopped looking.
pub fn owner_stopped(root: &Path, now: i64) {
    let Some(boot) = boot_time() else { return };
    if let Some(mut ledger) = read_ledger(root).filter(|l| l.boot == boot) {
        ledger.stopped_at = Some(now);
        write_ledger(root, &ledger);
    }
}

fn journal(root: &Path) -> Vec<Value> {
    switchboard_core::private_fs::read_private(&root.join(JOURNAL), MAX_BYTES)
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect()
        })
        .unwrap_or_default()
}
fn append(root: &Path, line: Value) {
    // The owner's pass and an offline status never write at once: only the pass writes.
    let mut lines = journal(root);
    lines.push(line);
    let keep = lines.len().saturating_sub(JOURNAL_LINES);
    let text: String = lines[keep..].iter().map(|l| format!("{l}\n")).collect();
    let _ = switchboard_core::private_fs::private_write(&root.join(JOURNAL), text.as_bytes());
}

/// What the pass reads of the machine now; the real one, or a test's.
pub trait Machine {
    /// Sessions a live Claude Code process holds.
    fn live_sessions(&self) -> HashSet<String>;
    /// Whether `session` runs in a host, or a launch began to continue it a moment ago.
    fn being_continued(&self, session: &str) -> bool;
    /// The newest write of the session's transcript in any config folder.
    fn transcript_at(&self, folder: &str, session: &str) -> Option<i64>;
    /// Whether the session pool admits one more interactive session; the held reservation.
    fn admit(&self) -> Result<Option<Box<dyn std::any::Any>>, ()>;
    /// Continues `session` hosted; the new launch id, or the reason it could not.
    fn continue_hosted(&self, session: &str, parent: &str) -> Result<String, String>;
}

/// One session the restart cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub session: String,
    pub launch_id: String,
    pub folder: String,
    pub written_at: i64,
}

/// The rule (see the module's text): the sessions of `ledger` this boot continues, newest
/// transcript first, at most `MAX_PER_BOOT`; and those past the limit.
pub fn candidates(
    ledger: &Ledger,
    records: &HashMap<String, Value>,
    boot: i64,
    done: &HashSet<String>,
    machine: &dyn Machine,
) -> (Vec<Candidate>, Vec<Candidate>) {
    if ledger.boot >= boot || ledger.at < boot - WINDOW_SECONDS {
        return (Vec::new(), Vec::new());
    }
    let mut after_boot: HashSet<&str> = HashSet::new();
    for r in records.values() {
        if r["launched_at"].as_i64().is_some_and(|t| t >= boot) {
            if let Some(s) = r["provider_session_id"].as_str() {
                after_boot.insert(s);
            }
        }
    }
    let live = machine.live_sessions();
    let mut found: Vec<Candidate> = Vec::new();
    for seen in &ledger.sessions {
        let Some(r) = records.get(&seen.launch_id) else {
            continue;
        };
        let claude = r["provider"].as_str().is_none_or(|p| p == "claude");
        let before = r["launched_at"].as_i64().is_some_and(|t| t < boot);
        if !claude || !before || !r["ended"].is_null() || !r["continued_as"].is_null() {
            continue;
        }
        let Some(folder) = r["working_directory"].as_str() else {
            continue;
        };
        let Some(written) = machine.transcript_at(folder, &seen.session) else {
            continue;
        };
        let looked = ledger.stopped_at.unwrap_or(i64::MAX);
        if written < boot - WINDOW_SECONDS
            || written > boot
            || looked < boot - WINDOW_SECONDS
            || written > looked.saturating_add(LOOK_SLACK_SECONDS)
        {
            continue;
        }
        if done.contains(&seen.session)
            || live.contains(&seen.session)
            || after_boot.contains(seen.session.as_str())
            || found.iter().any(|c| c.session == seen.session)
            || machine.being_continued(&seen.session)
        {
            continue;
        }
        found.push(Candidate {
            session: seen.session.clone(),
            launch_id: seen.launch_id.clone(),
            folder: folder.to_owned(),
            written_at: written,
        });
    }
    found.sort_by(|a, b| b.written_at.cmp(&a.written_at));
    let over = found.split_off(found.len().min(MAX_PER_BOOT));
    (found, over)
}

/// The sessions this boot's journal names, and its intents with no outcome.
fn this_boot(lines: &[Value], boot: i64) -> (HashSet<String>, Vec<Value>) {
    let mut named = HashSet::new();
    let mut open: Vec<Value> = Vec::new();
    for l in lines.iter().filter(|l| l["boot"] == boot) {
        let Some(session) = l["session"].as_str() else {
            continue;
        };
        named.insert(session.to_owned());
        if l["phase"] == "intent" {
            open.push(l.clone());
        } else {
            open.retain(|i| i["session"] != session);
        }
    }
    (named, open)
}

/// One pass after a boot: settles intents a quit cut, then continues what the rule finds.
/// Answers what it did. Never acts on a ledger of this boot.
pub fn run(root: &Path, boot: i64, now: i64, machine: &dyn Machine) -> Value {
    let Some(ledger) = read_ledger(root).filter(|l| l.boot < boot) else {
        return json!({"continued": [], "reason": "no_earlier_boot"});
    };
    let (done, open) = this_boot(&journal(root), boot);
    let live = machine.live_sessions();
    for intent in open {
        let session = intent["session"].as_str().unwrap_or_default();
        let held = live.contains(session) || machine.being_continued(session);
        let mut line = intent.clone();
        line["phase"] = "outcome".into();
        line["at"] = now.into();
        line["outcome"] = if held { "continued" } else { "interrupted" }.into();
        append(root, line);
    }
    let records: HashMap<String, Value> = ledger
        .sessions
        .iter()
        .filter_map(|s| {
            Some((
                s.launch_id.clone(),
                crate::launches::get(root, &s.launch_id)?,
            ))
        })
        .chain(
            crate::launches::recent_records(root, boot)
                .into_iter()
                .filter_map(|r| Some((r["launch_id"].as_str()?.to_owned(), r))),
        )
        .collect();
    let (due, over) = candidates(&ledger, &records, boot, &done, machine);
    let mut continued = Vec::new();
    let mut skipped = Vec::new();
    let line = |c: &Candidate| {
        json!({"at": now, "boot": boot, "session": c.session, "launch_id": c.launch_id,
            "folder": c.folder})
    };
    let mut full = false;
    for c in &due {
        if full {
            skipped.push(c.session.clone());
            let mut l = line(c);
            l["phase"] = "outcome".into();
            l["outcome"] = "at_capacity".into();
            append(root, l);
            continue;
        }
        let Ok(reservation) = machine.admit() else {
            full = true;
            skipped.push(c.session.clone());
            let mut l = line(c);
            l["phase"] = "outcome".into();
            l["outcome"] = "at_capacity".into();
            append(root, l);
            continue;
        };
        let mut intent = line(c);
        intent["phase"] = "intent".into();
        append(root, intent);
        let mut outcome = line(c);
        outcome["phase"] = "outcome".into();
        match machine.continue_hosted(&c.session, &c.launch_id) {
            Ok(id) => {
                outcome["outcome"] = "continued".into();
                outcome["new_launch_id"] = id.clone().into();
                crate::launches::continued_by_boot(root, &c.launch_id, &id, now);
                continued.push(json!({"session": c.session, "launch_id": id}));
            }
            Err(reason) => {
                outcome["outcome"] = "failed".into();
                outcome["reason"] = reason.into();
            }
        }
        append(root, outcome);
        drop(reservation);
    }
    for c in &over {
        let mut l = line(c);
        l["phase"] = "outcome".into();
        l["outcome"] = "over_limit".into();
        append(root, l);
        skipped.push(c.session.clone());
    }
    crate::oplog::event(
        "boot_resume",
        &[
            (
                "continued",
                crate::oplog::Field::Number(continued.len() as i64),
            ),
            ("skipped", crate::oplog::Field::Number(skipped.len() as i64)),
        ],
    );
    json!({"continued": continued, "skipped": skipped})
}

/// This machine as the pass reads it.
pub struct Real<'a> {
    pub root: &'a Path,
    pub now: i64,
}
impl Machine for Real<'_> {
    fn live_sessions(&self) -> HashSet<String> {
        crate::machine_sessions::live_sessions(self.root)
            .into_keys()
            .collect()
    }
    fn being_continued(&self, session: &str) -> bool {
        crate::resumable::being_continued(self.root, session, self.now)
    }
    fn transcript_at(&self, folder: &str, session: &str) -> Option<i64> {
        crate::machine_sessions::transcript_at(self.root, folder, session)
    }
    fn admit(&self) -> Result<Option<Box<dyn std::any::Any>>, ()> {
        crate::capacity::reserve(self.root, crate::capacity::Start::Interactive)
            .map(|r| r.map(|r| Box::new(r) as Box<dyn std::any::Any>))
            .map_err(|_| ())
    }
    #[cfg(unix)]
    fn continue_hosted(&self, session: &str, parent: &str) -> Result<String, String> {
        let by = crate::launches::LaunchedBy {
            surface: crate::launches::Surface::Recovery,
            by: Some(BY.into()),
            parent: Some(parent.to_owned()),
            ..Default::default()
        };
        let how = crate::resumable::How {
            in_place: true,
            hosted: true,
            by,
        };
        let cli = crate::launch::agent_cli().ok_or("no_cli")?;
        let answer = crate::resumable::continue_as(self.root, session, self.now, &how)?;
        let (Some(id), Some(script)) = (answer["launch_id"].as_str(), answer["script"].as_str())
        else {
            return Err("no_script".into());
        };
        if let Err(error) =
            crate::session_host::start_detached(self.root, id, Path::new(script), &cli, 40, 120)
        {
            // A host that never came up ends its record here; one that runs ends it itself.
            if crate::session_host::attachable(self.root, id).is_none() {
                crate::launches::end_once(self.root, id, "not_started", self.now, Some(1));
            }
            return Err(error);
        }
        Ok(id.to_owned())
    }
    #[cfg(not(unix))]
    fn continue_hosted(&self, _: &str, _: &str) -> Result<String, String> {
        Err("unsupported".into())
    }
}

static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BOOT: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
fn boot() -> Option<i64> {
    *BOOT.get_or_init(boot_time)
}

/// The session watch's pass: once per owner, with the switch on, the previous boot's sessions
/// are continued; then (every pass) the ledger is kept for the next boot.
pub fn tick(root: &Path, rows: &[Value], pairs: &[(String, u32)], table_read: bool, now: i64) {
    let Some(boot) = boot() else { return };
    if !DONE.swap(true, std::sync::atomic::Ordering::AcqRel)
        && cfg!(unix)
        && settings(root).is_ok_and(|s| s.on)
    {
        // SB-403 S3: with the person's own sign-in off limits, only sessions in Switchboard's
        // own homes are continued; `continue_as` locates each in its launch's home.
        let _ = run(root, boot, now, &Real { root, now });
    }
    // A scan that could not read the process table says nothing of what runs.
    if table_read {
        observe(root, boot, seen_of(rows, pairs), now);
    }
}

/// `switchboard boot-resume`: the switch, this boot, the last look before it and the newest
/// lines of the journal.
pub fn status(root: &Path) -> Result<Value, String> {
    let set = settings(root)?;
    let mut recent: Vec<Value> = journal(root)
        .into_iter()
        .rev()
        .filter(|l| l["phase"] != "intent")
        .take(20)
        .collect();
    recent.retain(|l| l.is_object());
    let ledger = read_ledger(root);
    Ok(json!({
        "settings": set,
        "boot_time": boot_time(),
        "last_look": ledger.map(|l| json!({"boot": l.boot, "at": l.at, "sessions": l.sessions.len(), "stopped_at": l.stopped_at})),
        "window_minutes": WINDOW_SECONDS / 60,
        "max_per_boot": MAX_PER_BOOT,
        "recent": recent,
        "supported": cfg!(unix),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const BOOT: i64 = 1_000_000;

    #[derive(Default)]
    struct Fake {
        live: HashSet<String>,
        hosted: HashSet<String>,
        written: HashMap<String, i64>,
        full: bool,
        fail: HashSet<String>,
        started: RefCell<Vec<String>>,
    }
    impl Machine for Fake {
        fn live_sessions(&self) -> HashSet<String> {
            self.live.clone()
        }
        fn being_continued(&self, session: &str) -> bool {
            self.hosted.contains(session)
        }
        fn transcript_at(&self, _: &str, session: &str) -> Option<i64> {
            self.written.get(session).copied()
        }
        fn admit(&self) -> Result<Option<Box<dyn std::any::Any>>, ()> {
            if self.full {
                Err(())
            } else {
                Ok(None)
            }
        }
        fn continue_hosted(&self, session: &str, _: &str) -> Result<String, String> {
            if self.fail.contains(session) {
                return Err("no_cli".into());
            }
            self.started.borrow_mut().push(session.to_owned());
            Ok(format!("new-{session}"))
        }
    }

    fn launch(root: &Path, id: &str, session: &str, at: i64, extra: Value) {
        let mut record = json!({"launch_id": id, "provider": "claude", "launched_at": at,
            "working_directory": "/work/app", "provider_session_id": session});
        for (k, v) in extra.as_object().unwrap() {
            record[k] = v.clone();
        }
        let dir = root.join(crate::launches::DIR);
        switchboard_core::private_fs::private_dir(&dir).unwrap();
        switchboard_core::private_fs::private_write(
            &dir.join(format!("{id}.json")),
            record.to_string().as_bytes(),
        )
        .unwrap();
    }

    fn ledger(root: &Path, sessions: &[(&str, &str)], at: i64, stopped_at: Option<i64>) {
        write_ledger(
            root,
            &Ledger {
                boot: BOOT - 86_400,
                at,
                sessions: sessions
                    .iter()
                    .map(|(s, l)| Seen {
                        session: (*s).into(),
                        launch_id: (*l).into(),
                    })
                    .collect(),
                stopped_at,
            },
        );
    }

    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    #[test]
    fn the_boot_time_is_read_from_linux_proc_stat() {
        assert_eq!(
            parse_btime("cpu 1 2 3\nbtime 1763450000\nprocesses 9\n"),
            Some(1_763_450_000)
        );
        assert_eq!(parse_btime("cpu 1\n"), None);
        // This machine answers one, in the past.
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let boot = boot_time().expect("a boot time");
            assert!(boot > 1_500_000_000 && boot <= crate::session_watch::now());
        }
    }

    #[test]
    fn the_sessions_a_restart_cut_are_continued_once_and_the_rest_are_left_with_a_reason() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let before = BOOT - 600;
        // a: cut, written 10 minutes before the boot. b: ended on its own. c: written three hours
        // before. d: written after the boot (someone continued it). e: a process holds it.
        // f: a host runs it (another agent continued it). g: a launch since the boot names it.
        // h: a dispatched run that ended. i: Codex.
        launch(root, &id(1), "a", before - 3600, json!({}));
        launch(
            root,
            &id(2),
            "b",
            before,
            json!({"ended": {"kind": "exited"}}),
        );
        launch(root, &id(3), "c", before, json!({}));
        launch(root, &id(4), "d", before, json!({}));
        launch(root, &id(5), "e", before, json!({}));
        launch(root, &id(6), "f", before, json!({}));
        launch(root, &id(7), "g", before, json!({}));
        launch(root, &id(8), "g", BOOT + 30, json!({}));
        launch(
            root,
            &id(9),
            "h",
            before,
            json!({"dispatch": {"request": "r"}, "ended": {"kind": "exited"}}),
        );
        launch(root, &id(10), "i", before, json!({"provider": "codex"}));
        let ids: Vec<String> = (1..=10).map(id).collect();
        let pairs: Vec<(&str, &str)> = ["a", "b", "c", "d", "e", "f", "g", "h", "i"]
            .iter()
            .zip(ids.iter().filter(|i| **i != id(8)))
            .map(|(s, i)| (*s, i.as_str()))
            .collect();
        ledger(root, &pairs, BOOT - 300, None);
        let mut fake = Fake::default();
        for (s, at) in [
            ("a", BOOT - 600),
            ("b", BOOT - 600),
            ("c", BOOT - 3 * 3600),
            ("d", BOOT + 60),
            ("e", BOOT - 600),
            ("f", BOOT - 600),
            ("g", BOOT - 600),
            ("h", BOOT - 600),
            ("i", BOOT - 600),
        ] {
            fake.written.insert(s.into(), at);
        }
        fake.live.insert("e".into());
        fake.hosted.insert("f".into());
        let answer = run(root, BOOT, BOOT + 120, &fake);
        assert_eq!(*fake.started.borrow(), ["a"]);
        assert_eq!(answer["continued"][0]["launch_id"], "new-a");
        let old = crate::launches::get(root, &id(1)).unwrap();
        assert_eq!(old["continued_as"]["id"], "new-a");
        assert_eq!(old["ended"]["kind"], "reboot");
        let lines = journal(root);
        assert_eq!(
            lines
                .iter()
                .map(|l| (l["phase"].as_str().unwrap(), l["session"].as_str().unwrap()))
                .collect::<Vec<_>>(),
            [("intent", "a"), ("outcome", "a")]
        );
        // A second owner start in the same boot (the ledger still from the earlier boot, say it
        // was never rewritten), with `a`'s continuation not yet holding it, continues nothing
        // again: this boot's journal names `a`, and its record ended.
        let fake2 = Fake {
            written: fake.written.clone(),
            live: fake.live.clone(),
            hosted: fake.hosted.clone(),
            ..Fake::default()
        };
        run(root, BOOT, BOOT + 200, &fake2);
        assert!(fake2.started.borrow().is_empty());
    }

    #[test]
    fn a_continuation_a_quit_cut_is_settled_never_started_again() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        launch(root, &id(1), "a", BOOT - 4000, json!({}));
        launch(root, &id(2), "b", BOOT - 4000, json!({}));
        ledger(root, &[("a", &id(1)), ("b", &id(2))], BOOT - 60, None);
        append(
            root,
            json!({"at": BOOT + 10, "boot": BOOT, "phase": "intent", "session": "a", "launch_id": id(1)}),
        );
        append(
            root,
            json!({"at": BOOT + 10, "boot": BOOT, "phase": "intent", "session": "b", "launch_id": id(2)}),
        );
        let mut fake = Fake::default();
        fake.written.insert("a".into(), BOOT - 100);
        fake.written.insert("b".into(), BOOT - 100);
        fake.hosted.insert("a".into());
        run(root, BOOT, BOOT + 60, &fake);
        assert!(fake.started.borrow().is_empty(), "neither is started again");
        let outcomes: Vec<(String, String)> = journal(root)
            .iter()
            .filter(|l| l["phase"] == "outcome")
            .map(|l| {
                (
                    l["session"].as_str().unwrap().to_owned(),
                    l["outcome"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            outcomes,
            [
                ("a".to_owned(), "continued".to_owned()),
                ("b".to_owned(), "interrupted".to_owned())
            ]
        );
    }

    #[test]
    fn at_most_five_are_continued_newest_first_and_a_full_pool_ends_the_pass() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let names = ["s1", "s2", "s3", "s4", "s5", "s6", "s7"];
        let ids: Vec<String> = (1..=7).map(id).collect();
        let mut fake = Fake::default();
        for (n, (s, i)) in names.iter().zip(&ids).enumerate() {
            launch(root, i, s, BOOT - 5000, json!({}));
            fake.written
                .insert((*s).into(), BOOT - 100 * (n as i64 + 1));
        }
        fake.fail.insert("s2".into());
        let pairs: Vec<(&str, &str)> = names
            .iter()
            .copied()
            .zip(ids.iter().map(String::as_str))
            .collect();
        ledger(root, &pairs, BOOT - 30, Some(BOOT - 20));
        let answer = run(root, BOOT, BOOT + 60, &fake);
        assert_eq!(*fake.started.borrow(), ["s1", "s3", "s4", "s5"]);
        assert_eq!(answer["skipped"], json!(["s6", "s7"]));
        let failed = journal(root)
            .into_iter()
            .find(|l| l["outcome"] == "failed")
            .unwrap();
        assert_eq!(
            (failed["session"].clone(), failed["reason"].clone()),
            (json!("s2"), json!("no_cli"))
        );
        assert!(crate::launches::get(root, &id(2)).unwrap()["continued_as"].is_null());

        // A full pool: nothing starts, and each is named.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        launch(root, &id(1), "a", BOOT - 5000, json!({}));
        ledger(root, &[("a", &id(1))], BOOT - 30, None);
        let mut fake = Fake {
            full: true,
            ..Fake::default()
        };
        fake.written.insert("a".into(), BOOT - 100);
        run(root, BOOT, BOOT + 60, &fake);
        assert!(fake.started.borrow().is_empty());
        assert_eq!(journal(root)[0]["outcome"], "at_capacity");
    }

    #[test]
    fn a_session_written_after_the_owner_stopped_looking_or_an_old_look_is_not_continued() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        launch(root, &id(1), "a", BOOT - 9000, json!({}));
        let mut fake = Fake::default();
        // The owner quit an hour before the boot; the session wrote after that: nobody saw it
        // still running, so the person may have closed it.
        fake.written.insert("a".into(), BOOT - 600);
        ledger(root, &[("a", &id(1))], BOOT - 5000, Some(BOOT - 3600));
        run(root, BOOT, BOOT + 60, &fake);
        assert!(fake.started.borrow().is_empty());
        // A ledger of this boot is never acted on.
        write_ledger(
            root,
            &Ledger {
                boot: BOOT,
                at: BOOT + 10,
                sessions: vec![Seen {
                    session: "a".into(),
                    launch_id: id(1),
                }],
                stopped_at: None,
            },
        );
        assert_eq!(
            run(root, BOOT, BOOT + 60, &fake)["reason"],
            "no_earlier_boot"
        );
    }

    #[test]
    fn the_ledger_is_written_only_when_its_sessions_or_its_boot_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let rows = vec![
            json!({"pid": 10, "agent": "claude-code", "launch_id": "L1"}),
            json!({"pid": 11, "agent": "claude-code", "launch_id": null}),
            json!({"pid": 12, "agent": "codex", "launch_id": "L3"}),
        ];
        let pairs = vec![("s1".to_owned(), 10), ("s2".to_owned(), 11)];
        let seen = seen_of(&rows, &pairs);
        assert_eq!(
            seen,
            [Seen {
                session: "s1".into(),
                launch_id: "L1".into()
            }]
        );
        observe(root, BOOT, seen.clone(), BOOT + 10);
        observe(root, BOOT, seen.clone(), BOOT + 70);
        assert_eq!(
            read_ledger(root).unwrap().at,
            BOOT + 10,
            "unchanged: not rewritten"
        );
        observe(root, BOOT, seen.clone(), BOOT + 10 + REFRESH_SECONDS);
        assert_eq!(
            read_ledger(root).unwrap().at,
            BOOT + 10 + REFRESH_SECONDS,
            "renewed while sessions run"
        );
        observe(root, BOOT, Vec::new(), BOOT + 4000);
        assert_eq!(read_ledger(root).unwrap().at, BOOT + 4000);
        observe(root, BOOT, Vec::new(), BOOT + 9000);
        assert_eq!(
            read_ledger(root).unwrap().at,
            BOOT + 4000,
            "none running: never renewed"
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.join(LIVE)).unwrap().permissions()
            ) & 0o777,
            0o600
        );
    }

    #[test]
    fn the_switch_is_on_by_default_and_kept() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        assert!(settings(root).unwrap().on);
        save(root, &Settings { on: false }).unwrap();
        assert!(!settings(root).unwrap().on);
        std::fs::write(root.join(FILE), b"not json").unwrap();
        assert_eq!(settings(root), Err(UNREADABLE.into()));
        assert_eq!(status(temp.path()), Err(UNREADABLE.into()));
    }
}
