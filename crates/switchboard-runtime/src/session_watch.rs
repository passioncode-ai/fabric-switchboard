//! SB-379 (Q4, Q5; operator 2026-11-15: «проследи за лайфсайклом агентов … какая цепочка агентов,
//! какие запускаются, какие остались живы, где сколько сессий, кто как запускался, какая задача …
//! мониторим каждую минуту, чтобы сессии не засиживались, не заброшены»).
//!
//! `tree` answers every live coding session of this user (Switchboard's or not) joined to its launch
//! record, the launches of the last day with who started whom, and counts. `findings` is the watch:
//! a resumed copy of a live session (one Claude session id in two processes, SB-376), a board run
//! quiet past `STUCK_MINUTES`, and a launch whose process is gone with no end recorded. The owner
//! runs it at most once a minute (`tick`) and keeps the last answer in `<data>/session-watch.json`
//! (0600, ids, pids, folders and times; written only when it changes), which *Needs attention*
//! reads. Nothing here stops or starts a session.
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};

pub const FILE: &str = "session-watch.json";
/// A board run whose folder was not written for this long reads as stuck.
pub const STUCK_MINUTES: i64 = 30;
/// A launch with no end and no live process this long after it started reads as gone.
pub const GONE_MINUTES: i64 = 10;
/// The launches the tree and the watch look at: the last day.
pub const WINDOW_SECONDS: i64 = 24 * 3600;

/// SB-407: the field `read` adds to a dispatched run's record: when its log was last written
/// (Unix seconds), so a run that writes its log is never read as stuck.
pub const LOG_WRITTEN_AT: &str = "log_written_at";
/// Review round 1: the field `read` adds to a dispatched run that has not ended when its home's
/// process runs (`launches::own_home`, `launch::session_running`), so it is never read as gone.
pub const HOME_RUNNING: &str = "home_running";

/// Whether a scan read the process table: its `degraded` names no source.
pub fn table_read(scan: &Value) -> bool {
    scan["degraded"].as_array().is_none_or(|d| d.is_empty())
}

pub const DUPLICATE: &str = "duplicate";
pub const STUCK: &str = "stuck";
pub const GONE: &str = "gone";

/// Unix seconds now.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The surface a launch was started from, as `launched_by.surface` names it.
fn surface(record: &Value) -> &str {
    record["launched_by"]["surface"]
        .as_str()
        .unwrap_or("unknown")
}

/// The launch that started this one: its `launched_by.parent`, else the work it continues.
fn parent(record: &Value) -> Option<String> {
    record["launched_by"]["parent"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            record["continued_from"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

/// The task a launch serves: the board request and workflow of a dispatch, else the workflow it
/// continues.
fn task(record: &Value) -> Value {
    let dispatch = &record["dispatch"];
    let workflow = dispatch["workflow"]
        .as_str()
        .or_else(|| record["continues"].as_str());
    json!({
        "workflow": workflow,
        "request": dispatch["request"].as_str(),
        "project": dispatch["project"].as_str(),
    })
}

/// The pure watch: what needs a person now. `rows` are `machine_sessions::scan`'s sessions,
/// `records` the launches of the window, `pairs` the live (session id, pid) pairs. `table_read`
/// says the process table was read (SB-375: a scan that could not read it, and every scan on a
/// system whose table is not read, names it in `degraded`): without it no launch reads as gone,
/// since every process is missing from it.
pub fn findings(
    rows: &[Value],
    records: &[Value],
    pairs: &[(String, u32)],
    now: i64,
    table_read: bool,
) -> Vec<Value> {
    let mut out = Vec::new();
    let mut by_session: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (session, pid) in pairs {
        by_session.entry(session.as_str()).or_default().push(*pid);
    }
    for (session, mut pids) in by_session {
        if pids.len() > 1 {
            pids.sort_unstable();
            // SB-388: the session's folder names the item in Needs attention (not its id).
            let folder = rows
                .iter()
                .find(|r| r["pid"].as_u64() == Some(u64::from(pids[0])))
                .map(|r| r["folder"].clone())
                .unwrap_or(Value::Null);
            out.push(
                json!({"kind": DUPLICATE, "session_id": session, "pids": pids, "folder": folder}),
            );
        }
    }
    let live: HashMap<&str, &Value> = rows
        .iter()
        .filter_map(|r| r["launch_id"].as_str().map(|id| (id, r)))
        .collect();
    for record in records {
        let Some(id) = record["launch_id"].as_str() else {
            continue;
        };
        let started = record["launched_at"].as_i64().unwrap_or(now);
        let dispatched = record["dispatch"].is_object();
        match live.get(id) {
            Some(row) if dispatched => {
                // SB-407: a run that writes its log is working, whatever its folder says.
                let mut quiet = row["quiet_minutes"].as_i64().unwrap_or(0);
                if let Some(written) = record[LOG_WRITTEN_AT].as_i64() {
                    quiet = quiet.min((now - written).max(0) / 60);
                }
                if quiet >= STUCK_MINUTES {
                    out.push(json!({
                        "kind": STUCK, "launch_id": id, "pids": [row["pid"]],
                        "folder": record["working_directory"], "quiet_minutes": quiet,
                        "task": task(record), "dispatch": true,
                    }));
                }
            }
            Some(_) => {}
            // SB-407: a run on another machine has no process here; `dispatch status` reads it.
            None if dispatched && !record["dispatch"]["target"].is_null() => {}
            // Review round 1 (P1): no process table, no gone; and a dispatched run whose home
            // says its process runs is not gone, whatever the scan missed.
            None if !table_read || record[HOME_RUNNING] == true => {}
            None if record["ended"].is_null() && now - started >= GONE_MINUTES * 60 => {
                out.push(json!({
                    "kind": GONE, "launch_id": id, "folder": record["working_directory"],
                    "since": record["last_turn_at"].as_i64().unwrap_or(started),
                    "surface": surface(record), "task": task(record), "dispatch": dispatched,
                    // SB-509: the session it ran, which `offer_continuations` may offer, and
                    // when its last turn ended (or it started), which a cut turn wrote after.
                    "session_id": record["provider_session_id"],
                    "provider": record["provider"],
                    "last_turn_at": record["last_turn_at"],
                    "launched_at": started,
                }));
            }
            None => {}
        }
    }
    out
}

/// The pure tree: live sessions joined to their launches, the day's launches with their parents,
/// and counts by group, surface and state.
pub fn assemble(scan: &Value, records: &[Value], pairs: &[(String, u32)], now: i64) -> Value {
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    let by_id: HashMap<&str, &Value> = records
        .iter()
        .filter_map(|r| r["launch_id"].as_str().map(|id| (id, r)))
        .collect();
    let session_of: HashMap<u32, &str> = pairs.iter().map(|(s, p)| (*p, s.as_str())).collect();
    let live: Vec<Value> = rows
        .iter()
        .map(|row| {
            let record = row["launch_id"].as_str().and_then(|id| by_id.get(id));
            let pid = row["pid"].as_u64().unwrap_or(0) as u32;
            json!({
                "pid": row["pid"], "agent": row["agent"], "folder": row["folder"],
                "started_at": row["started_at"], "quiet": row["quiet"],
                "quiet_minutes": row["quiet_minutes"], "managed": row["managed"],
                "launch_id": row["launch_id"],
                "session_id": session_of.get(&pid),
                "pool": record.map(|r| r["pool"].clone()),
                "account_id": record.map(|r| r["account_id"].clone()),
                "surface": record.map(|r| surface(r).to_owned()),
                "parent": record.and_then(|r| parent(r)),
                "task": record.map(|r| task(r)),
            })
        })
        .collect();
    let live_ids: HashSet<&str> = rows
        .iter()
        .filter_map(|r| r["launch_id"].as_str())
        .collect();
    let mut launches: Vec<Value> = records
        .iter()
        .filter(|r| {
            r["launched_at"]
                .as_i64()
                .is_some_and(|t| now - t <= WINDOW_SECONDS)
        })
        .map(|r| {
            let id = r["launch_id"].as_str().unwrap_or_default();
            let state = if live_ids.contains(id) {
                "running"
            } else if !r["ended"].is_null() {
                "ended"
            } else {
                "gone"
            };
            json!({
                "launch_id": id, "launched_at": r["launched_at"], "pool": r["pool"],
                "account_id": r["account_id"], "provider": r["provider"],
                "folder": r["working_directory"], "surface": surface(r), "parent": parent(r),
                "task": task(r), "state": state, "ended": r["ended"],
                "session_id": r["provider_session_id"],
            })
        })
        .collect();
    launches.sort_by_key(|l| std::cmp::Reverse(l["launched_at"].as_i64().unwrap_or(0)));
    let mut by_pool: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_surface: BTreeMap<String, u64> = BTreeMap::new();
    for row in &live {
        let pool = row["pool"].as_str().unwrap_or("unmanaged").to_owned();
        *by_pool.entry(pool).or_default() += 1;
        let surface = row["surface"].as_str().unwrap_or("unmanaged").to_owned();
        *by_surface.entry(surface).or_default() += 1;
    }
    let quiet = live.iter().filter(|r| r["quiet"] == true).count();
    json!({
        "live": live,
        "launches": launches,
        "counts": {
            "live": rows.len(), "working": rows.len() - quiet, "quiet": quiet,
            "managed": rows.iter().filter(|r| r["managed"] == true).count(),
            "by_pool": by_pool, "by_surface": by_surface,
        },
        "findings": findings(&rows, records, pairs, now, table_read(scan)),
        "observed_at": scan["observed_at"],
        "degraded": scan["degraded"],
    })
}

type Read = (Value, Vec<Value>, Vec<(String, u32)>);
/// Perf audit item 9: the last read (process scan, launch records, session pairs) with its time
/// and data folder. A read under load takes seconds; Fabric Dashboards and the window ask for the
/// tree every few seconds, so a read younger than `READ_FRESH` seconds answers them, and the
/// one-minute watch always reads anew and leaves its read here.
static LAST_READ: std::sync::Mutex<Option<(i64, std::path::PathBuf, Read)>> =
    std::sync::Mutex::new(None);
const READ_FRESH: i64 = 15;

/// SB-500 (operator 2026-11-18: «дежурный работает неправильно»): per live session process, how many
/// launches it started itself (`launched_by.caller_launch_id`, else `launched_by.parent`) run now.
/// Such a launch runs in a session host of its own, never under the session's process, so the
/// process tree does not show it; the duty officer counts it as the session's work.
pub fn running_children(scan: &Value, records: &[Value]) -> HashMap<u32, usize> {
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    let live: HashSet<&str> = rows
        .iter()
        .filter_map(|r| r["launch_id"].as_str())
        .collect();
    let mut per_launch: HashMap<&str, usize> = HashMap::new();
    for r in records {
        let Some(id) = r["launch_id"].as_str() else {
            continue;
        };
        if !live.contains(id) {
            continue;
        }
        let by = &r["launched_by"];
        if let Some(caller) = by["caller_launch_id"]
            .as_str()
            .or_else(|| by["parent"].as_str())
        {
            if caller != id {
                *per_launch.entry(caller).or_default() += 1;
            }
        }
    }
    rows.iter()
        .filter_map(|row| {
            let pid = u32::try_from(row["pid"].as_u64()?).ok()?;
            let n = *per_launch.get(row["launch_id"].as_str()?)?;
            Some((pid, n))
        })
        .collect()
}

/// SB-500: `running_children` from the watch's last read (at most `READ_FRESH` seconds old), and
/// whether that read saw the process table.
pub(crate) fn children_now(root: &Path, now: i64) -> (HashMap<u32, usize>, bool) {
    let (scan, records, _) = read_cached(root, now);
    (running_children(&scan, &records), table_read(&scan))
}

fn read_cached(root: &Path, now: i64) -> Read {
    if let Ok(last) = LAST_READ.lock() {
        if let Some((at, folder, read)) = last.as_ref() {
            if usable(*at, folder, root, now) {
                return read.clone();
            }
        }
    }
    read_fresh(root, now)
}

/// A kept read answers only for the same data folder, within `READ_FRESH` seconds, never from the
/// future (a clock set back).
fn usable(at: i64, folder: &Path, root: &Path, now: i64) -> bool {
    folder == root && (0..READ_FRESH).contains(&(now - at))
}

/// After an act that changes the live sessions (a copy closed), the next tree reads anew.
fn invalidate() {
    if let Ok(mut last) = LAST_READ.lock() {
        *last = None;
    }
}

fn read_fresh(root: &Path, now: i64) -> Read {
    let fresh = read(root, now);
    if let Ok(mut last) = LAST_READ.lock() {
        *last = Some((now, root.to_owned(), fresh.clone()));
    }
    fresh
}

/// What the live processes and records read now give.
fn read(root: &Path, now: i64) -> Read {
    let (scan, pairs) = crate::machine_sessions::scan_and_pairs(root);
    let pairs: Vec<(String, u32)> = pairs.into_iter().map(|(s, p, _)| (s, p)).collect();
    // SB-451: a record naming a session its own process does not hold is corrected before the
    // records are read.
    reconcile_records(root, &scan, &pairs, now);
    let mut records = crate::launches::recent_records(root, now - WINDOW_SECONDS);
    for record in records.iter_mut().filter(|r| r["dispatch"].is_object()) {
        if let Some(at) = log_written_at(root, record) {
            record[LOG_WRITTEN_AT] = at.into();
        }
        if record["ended"].is_null()
            && record["dispatch"]["target"].is_null()
            && crate::launches::own_home(root, record)
                .is_some_and(|home| crate::launch::session_running(&home).unwrap_or(true))
        {
            record[HOME_RUNNING] = true.into();
        }
    }
    (scan, records, pairs)
}

/// SB-451: the launch records corrected from their processes' own session files
/// (`launches::reconcile`); the session files are read only when a record differs. Nothing is
/// corrected from a scan that could not read the process table.
fn reconcile_records(root: &Path, scan: &Value, pairs: &[(String, u32)], now: i64) {
    if !table_read(scan) {
        return;
    }
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    type Files = HashMap<u32, (std::path::PathBuf, String)>;
    let files: std::cell::OnceCell<Files> = std::cell::OnceCell::new();
    let transcript_of = |pid: u32, session: &str| {
        let files = files.get_or_init(|| {
            crate::machine_sessions::session_files(root)
                .into_iter()
                .filter_map(|(config, f)| {
                    let pid = u32::try_from(f["pid"].as_u64()?).ok()?;
                    Some((pid, (config, f["cwd"].as_str()?.to_owned())))
                })
                .collect()
        });
        let (config, cwd) = files.get(&pid)?;
        let path = crate::resumable::transcript(config, cwd, session);
        path.is_file().then_some(path)
    };
    crate::launches::reconcile(root, &rows, pairs, transcript_of, now);
}

/// SB-451: before one launch is shown, its record is checked against its process (a read at most
/// `READ_FRESH` seconds old, the tree's own).
pub fn refresh(root: &Path, now: i64) {
    let _ = read_cached(root, now);
}

/// SB-407: when a dispatched run's log (`launches/logs/<id>.log`) was last written; a stat only.
pub(crate) fn log_written_at(root: &Path, record: &Value) -> Option<i64> {
    let id = record["launch_id"].as_str()?;
    if !crate::launches::valid_id(id) {
        return None;
    }
    let path = root
        .join(crate::launches::DIR)
        .join(crate::launches::LOGS)
        .join(format!("{id}.log"));
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// `switchboard sessions --tree` and MCP `switchboard_sessions` with `tree`.
pub fn tree(root: &Path, now: i64) -> Value {
    let (scan, records, pairs) = read_cached(root, now);
    let mut answer = assemble(&scan, &records, &pairs, now);
    // SB-417: counted per session, as `switchboard capacity` counts.
    answer["capacity"] = crate::capacity::view_of(root, &scan, now).2;
    let map = crate::shell::map::read(root);
    let hook = hook_state(
        crate::shell::map::choice(root),
        crate::shell::map::organizations_mapped(&map),
    );
    answer["context"] = context(&answer["live"], &map, hook);
    answer
}

/// SB-382: the terminal hook's state as `shell status` names it: the person's choice, else on by
/// default while an organization's group has folders in the map.
fn hook_state(choice: crate::shell::map::Choice, mapped: bool) -> &'static str {
    use crate::shell::map::Choice;
    match (choice, mapped) {
        (Choice::On, _) => "on",
        (Choice::Off, _) => "off",
        (Choice::Default, true) => "default_on",
        (Choice::Default, false) => "default_off",
    }
}

/// SB-382 (Vesta's G4, OBS-550): for each organization context of the folder map, the live
/// sessions in its folders, those Switchboard launched in that group (`managed`) and the rest
/// (`other`: started from a terminal, which has the context only through the hook, or from
/// elsewhere), with the hook's state, so a reader sees why a session runs without its context.
/// Read from the map only; no session's environment is read.
pub fn context(live: &Value, map: &crate::shell::map::Loaded, hook: &str) -> Value {
    let mut by: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    if let crate::shell::map::Loaded::Read(map) = map {
        for row in live.as_array().into_iter().flatten() {
            let Some(folder) = row["folder"].as_str() else {
                continue;
            };
            let Some(group) = map.lookup(Path::new(folder)) else {
                continue;
            };
            let entry = by.entry(group.context.clone()).or_default();
            entry.0 += 1;
            if row["managed"] == true && row["pool"].as_str() == Some(group.pool.as_str()) {
                entry.1 += 1;
            }
        }
    }
    let rows: Vec<Value> = by
        .into_iter()
        .map(|(context, (live, managed))| json!({"context": context, "live": live, "managed": managed, "other": live - managed}))
        .collect();
    json!({"hook": hook, "map": map.code(), "organizations": rows})
}

/// The last watch the owner kept; none before its first pass.
pub fn last(root: &Path) -> Vec<Value> {
    std::fs::read(root.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["findings"].as_array().cloned())
        .unwrap_or_default()
}

/// SB-388: a finding fixed by hand (a duplicate's quiet copy closed) leaves the kept watch at
/// once, so Needs attention does not list it until the next minute's pass.
pub fn forget(root: &Path, kind: &str, id: &str) {
    invalidate();
    let _lock = file_lock();
    let Some(mut kept) = std::fs::read(root.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return;
    };
    let gone = format!("{kind}:{id}");
    let Some(list) = kept["findings"].as_array_mut() else {
        return;
    };
    let before = list.len();
    list.retain(|f| key(f) != gone);
    if list.len() != before {
        let bytes = serde_json::to_vec(&kept).unwrap_or_default();
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// Review round 1 of PR #853: one writer of `session-watch.json` at a time in this process (the
/// watch, `forget` and `continued` each read it, change it and write it back).
static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn file_lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

static LAST_TICK: AtomicI64 = AtomicI64::new(0);
/// A watch in flight: under load a scan takes tens of seconds, and a second must not start beside it.
static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The owner's background pass: at most once a minute, the watch is run and kept; a finding that
/// was not there before is logged once.
pub fn tick(root: &Path, now: i64) {
    // A test's monitor never reads this machine's real processes and session files.
    if cfg!(test) {
        return;
    }
    let last = LAST_TICK.load(Ordering::Relaxed);
    if now - last < 60
        || LAST_TICK
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    // The process table and the records are read on a thread of their own, so the monitor's pass
    // never waits for them.
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let root = root.to_owned();
    std::thread::spawn(move || {
        // Cleared however the watch ends, a panic included, so the next minute runs again.
        struct Done;
        impl Drop for Done {
            fn drop(&mut self) {
                RUNNING.store(false, Ordering::Release);
            }
        }
        let _done = Done;
        watch(&root, now);
    });
}

fn watch(root: &Path, now: i64) {
    // SB-384: the duty officer acts at most every five minutes, on this same thread.
    crate::duty::tick(root, now);
    // SB-397 R1: a restart wave advances every minute.
    crate::restarts::tick(root, now);
    let (scan, records, pairs) = read_fresh(root, now);
    // SB-412: room in the pool after a refused dispatch is noted once.
    crate::dispatch_refusals::observe(root, &scan, now);
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    // SB-450: after a restart the sessions it cut are continued once; the live ones are kept for
    // the next restart.
    crate::boot_resume::tick(root, &rows, &pairs, table_read(&scan), now);
    let mut found = findings(&rows, &records, &pairs, now, table_read(&scan));
    describe_duplicates(root, &mut found);
    // The offers and the kept watch are read and written under one lock (`continued`).
    let _lock = file_lock();
    offer_continuations(root, &mut found, &pairs, now);
    let before = last_keys(root);
    for f in &found {
        if !before.contains(&key(f)) {
            let kind = match f["kind"].as_str() {
                Some(DUPLICATE) => DUPLICATE,
                Some(STUCK) => STUCK,
                _ => GONE,
            };
            crate::oplog::event(
                "session_watch",
                &[("kind", crate::oplog::Field::Code(kind))],
            );
        }
    }
    // SB-389: a duplicate's copies and the offer change while its keys stay, so the whole
    // findings are compared, not their keys.
    if found != last(root) {
        let bytes = serde_json::to_vec(&json!({"at": now, "findings": found})).unwrap_or_default();
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// SB-509: who was offered the continuation of a gone launch's session, `<data>/continue-offers.json`
/// (0600, session ids, launch ids and times): each session is offered once, and never again once
/// it was continued (by the person, or by anything that runs it again).
pub const OFFERS_FILE: &str = "continue-offers.json";
/// An offer is kept this long after it was made, past the watch's window.
const OFFERS_KEPT: i64 = 7 * 24 * 3600;

fn offers_read(root: &Path) -> serde_json::Map<String, Value> {
    switchboard_core::private_fs::read_private(&root.join(OFFERS_FILE), 256 * 1024)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn offers_write(root: &Path, offers: &serde_json::Map<String, Value>) {
    let path = root.join(OFFERS_FILE);
    if offers.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(offers) {
        let _ = switchboard_core::private_fs::private_write(&path, &bytes);
    }
}

/// SB-509 (review round 1 of PR #853): a transcript written this long after the last turn's end
/// was cut in a turn; Claude Code's own writes at a turn's end land within it.
pub const CUT_MARGIN_SECONDS: i64 = 30;

/// SB-509: whether the session's last words were the person ending it (`/exit`, `/quit`), read
/// from the transcript's last lines. Pure.
pub fn exited(tail: &str) -> bool {
    tail.lines().rev().take(6).any(|line| {
        [
            "<command-name>/exit</command-name>",
            "<command-name>/quit</command-name>",
        ]
        .iter()
        .any(|c| line.contains(c))
    })
}

/// SB-509: a gone session was cut in a turn: its transcript was written more than
/// `CUT_MARGIN_SECONDS` after its last turn ended (`baseline`: the launch's `last_turn_at`, else
/// its start), and its last words were not the person's `/exit`. A session the person left between
/// turns (closing its window, `/exit`) wrote nothing after its last turn. Pure.
pub fn cut_turn(written: Option<i64>, baseline: i64, tail: &str) -> bool {
    written.is_some_and(|w| w > baseline + CUT_MARGIN_SECONDS) && !exited(tail)
}

/// SB-509: the sessions closed on purpose since `since`: a `resumable.jsonl` line (the duty
/// officer's or a session host's close, a restart wave's) or a duty officer's act in `duty.jsonl`.
/// They are offered elsewhere (the Sessions page's *Continue*) or were meant to end. Pure.
pub fn closed_on_purpose(resumable: &[Value], duty: &[Value], since: i64) -> HashSet<String> {
    resumable
        .iter()
        .chain(duty.iter().filter(|l| l.get("outcome").is_some()))
        .filter(|l| l["at"].as_i64().is_some_and(|at| at >= since))
        .filter_map(|l| l["session"].as_str().map(str::to_owned))
        .collect()
}

/// SB-509: marks gone launches whose Claude Code session can be continued as itself
/// `continuable`, once per session: its turn was cut (`can_continue` with the time its last turn
/// ended), nobody closed it on purpose (`closed`), and no process holds it. An offer is recorded
/// when first made, and a session continued since (live now) is taken and never offered again.
/// Nothing is continued here: Needs attention offers it to the person. Pure but for its reads, so
/// `offers` is the state before and after.
pub fn offer(
    found: &mut [Value],
    offers: &mut serde_json::Map<String, Value>,
    live: &HashSet<&str>,
    closed: &HashSet<String>,
    can_continue: impl Fn(&str, i64) -> bool,
    now: i64,
) {
    for f in found
        .iter_mut()
        .filter(|f| f["kind"] == GONE && f["dispatch"] != true)
    {
        let Some(session) = f["session_id"].as_str().map(str::to_owned) else {
            continue;
        };
        if f["provider"].as_str().is_some_and(|p| p != "claude") || closed.contains(&session) {
            continue;
        }
        let entry = offers.get(&session).cloned();
        if entry.as_ref().is_some_and(|e| !e["taken_at"].is_null()) {
            continue;
        }
        if live.contains(session.as_str()) {
            // Continued since, by the person or a host: the offer is over.
            if let Some(entry) = entry {
                offers.insert(
                    session,
                    json!({"launch_id": entry["launch_id"], "offered_at": entry["offered_at"],
                        "taken_at": now}),
                );
            }
            continue;
        }
        let baseline = f["last_turn_at"]
            .as_i64()
            .or_else(|| f["launched_at"].as_i64())
            .unwrap_or(now);
        if !can_continue(&session, baseline) {
            continue;
        }
        f["continuable"] = true.into();
        if entry.is_none() {
            offers.insert(
                session,
                json!({"launch_id": f["launch_id"], "offered_at": now, "taken_at": null}),
            );
        }
    }
    offers.retain(|_, e| {
        e["offered_at"]
            .as_i64()
            .or_else(|| e["taken_at"].as_i64())
            .is_some_and(|t| now - t < OFFERS_KEPT)
    });
}

/// The last bytes of a transcript, for `exited`.
fn transcript_tail(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(len.saturating_sub(16 * 1024)));
    let mut bytes = Vec::new();
    let _ = file.take(16 * 1024).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn offer_continuations(root: &Path, found: &mut [Value], pairs: &[(String, u32)], now: i64) {
    let gone = found
        .iter()
        .any(|f| f["kind"] == GONE && f["dispatch"] != true && f["session_id"].is_string());
    if !gone && std::fs::symlink_metadata(root.join(OFFERS_FILE)).is_err() {
        return;
    }
    let mut offers = offers_read(root);
    let before = offers.clone();
    let live: HashSet<&str> = pairs.iter().map(|(s, _)| s.as_str()).collect();
    let since = now - WINDOW_SECONDS;
    let closed = if gone {
        closed_on_purpose(
            &crate::resumable::recent(root, since),
            &crate::duty::journal(root),
            since,
        )
    } else {
        HashSet::new()
    };
    offer(
        found,
        &mut offers,
        &live,
        &closed,
        |session, baseline| {
            crate::launches::valid_id(session)
                && crate::resumable::locate(root, session, now).is_some_and(|(config, cwd)| {
                    let path = crate::resumable::transcript(&config, &cwd, session);
                    let written = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64);
                    Path::new(&cwd).is_dir() && cut_turn(written, baseline, &transcript_tail(&path))
                })
        },
        now,
    );
    if offers != before {
        offers_write(root, &offers);
    }
}

/// SB-509: a session continued from Needs attention (or `sessions continue`) is taken: it is never
/// offered again, and leaves the kept watch at once.
pub fn continued(root: &Path, session: &str, now: i64) {
    let _lock = file_lock();
    let mut offers = offers_read(root);
    if let Some(entry) = offers.get_mut(session) {
        if entry["taken_at"].is_null() {
            entry["taken_at"] = now.into();
            offers_write(root, &offers);
        }
    }
    let Some(mut kept) = std::fs::read(root.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return;
    };
    let Some(list) = kept["findings"].as_array_mut() else {
        return;
    };
    let mut changed = false;
    for f in list
        .iter_mut()
        .filter(|f| f["session_id"] == session && f["continuable"] == true)
    {
        f["continuable"] = false.into();
        changed = true;
    }
    if changed {
        let bytes = serde_json::to_vec(&kept).unwrap_or_default();
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// SB-389: each duplicate's copies (status, commands, where they run) and the copy Needs
/// attention offers to close (`duty::suggestion`); read only while a duplicate exists.
fn describe_duplicates(root: &Path, found: &mut [Value]) {
    if !found.iter().any(|f| f["kind"] == DUPLICATE) {
        return;
    }
    let seen = crate::duty::observe(root);
    for f in found.iter_mut().filter(|f| f["kind"] == DUPLICATE) {
        let Some(session) = f["session_id"].as_str().map(str::to_owned) else {
            continue;
        };
        f["copies"] = Value::Array(crate::duty::copies(&seen, &session));
        f["close"] = crate::duty::suggestion(&seen, &session);
    }
}

fn key(f: &Value) -> String {
    format!(
        "{}:{}",
        f["kind"].as_str().unwrap_or(""),
        f["launch_id"]
            .as_str()
            .or_else(|| f["session_id"].as_str())
            .unwrap_or("")
    )
}

fn last_keys(root: &Path) -> HashSet<String> {
    last(root).iter().map(key).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_sessions_running_launches_of_its_own_are_counted_for_its_process() {
        // SB-500: the audit (launch A, process 100) started B and C itself; B runs, C ended; D runs
        // and continues A's earlier work, which is not a launch A started.
        let scan = json!({"sessions": [
            {"pid": 100, "launch_id": "A"}, {"pid": 200, "launch_id": "B"},
            {"pid": 300, "launch_id": "D"}, {"pid": 400}
        ]});
        let records = vec![
            json!({"launch_id": "A", "launched_by": {}}),
            json!({"launch_id": "B", "launched_by": {"caller_launch_id": "A"}}),
            json!({"launch_id": "C", "launched_by": {"caller_launch_id": "A"}, "ended": {"kind": "exited"}}),
            json!({"launch_id": "D", "launched_by": {}, "continued_from": ["A"]}),
            json!({"launch_id": "E", "launched_by": {"parent": "D"}}),
        ];
        let children = running_children(&scan, &records);
        assert_eq!(children.get(&100), Some(&1));
        assert_eq!(children.get(&300), None, "E does not run");
        assert_eq!(children.get(&400), None);
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn a_kept_read_answers_only_the_same_folder_for_fifteen_seconds() {
        let root = Path::new("/data/a");
        assert!(usable(100, root, root, 100));
        assert!(usable(100, root, root, 114));
        assert!(!usable(100, root, root, 115));
        assert!(!usable(100, root, root, 99), "a clock set back reads anew");
        assert!(!usable(100, Path::new("/data/b"), root, 101));
    }

    use super::*;

    #[test]
    fn the_context_block_counts_an_organizations_sessions_and_who_launched_them() {
        // SB-382: a session in an organization's folder is counted there; one Switchboard
        // launched in that group is `managed`, a terminal's is `other`; outside the map, nothing.
        use crate::shell::map::{Loaded, Map, Row};
        let acme = tempfile::tempdir().unwrap();
        let mine = tempfile::tempdir().unwrap();
        let folder = acme.path().canonicalize().unwrap();
        let map = Loaded::Read(Map {
            version: crate::shell::map::VERSION,
            rows: vec![Row {
                folder: folder.to_string_lossy().into_owned(),
                pool: "acme".into(),
                context: "org:acme".into(),
                claude: true,
                codex: false,
            }],
        });
        let sub = folder.join("app");
        std::fs::create_dir_all(&sub).unwrap();
        let live = json!([
            {"folder": sub, "managed": true, "pool": "acme"},
            {"folder": folder, "managed": false, "pool": null},
            {"folder": mine.path(), "managed": false, "pool": null},
        ]);
        let view = context(&live, &map, "default_on");
        assert_eq!(view["hook"], "default_on");
        assert_eq!(
            view["organizations"],
            json!([{"context": "org:acme", "live": 2, "managed": 1, "other": 1}])
        );
        assert_eq!(
            context(&live, &Loaded::Missing, "default_off")["organizations"],
            json!([])
        );
        assert_eq!(
            hook_state(crate::shell::map::Choice::Default, true),
            "default_on"
        );
        assert_eq!(hook_state(crate::shell::map::Choice::Off, true), "off");
    }

    fn row(pid: u32, launch: Option<&str>, quiet: i64) -> Value {
        json!({"pid": pid, "agent": "claude-code", "folder": "/work/a", "launch_id": launch,
            "managed": launch.is_some(), "quiet_minutes": quiet, "quiet": quiet >= 30})
    }

    #[test]
    fn a_copy_of_a_live_session_a_quiet_board_run_and_a_vanished_launch_are_found() {
        let now = 100_000;
        let rows = vec![
            row(10, Some("L-run"), 45),
            row(11, Some("L-cli"), 90),
            row(12, None, 0),
        ];
        let records = vec![
            json!({"launch_id": "L-run", "launched_at": now - 3600, "dispatch": {"workflow": "wf_1", "request": "r-1"}, "launched_by": {"surface": "fabric"}}),
            json!({"launch_id": "L-cli", "launched_at": now - 3600, "launched_by": {"surface": "cli"}}),
            json!({"launch_id": "L-gone", "launched_at": now - 3600, "launched_by": {"surface": "dashboards"}}),
            json!({"launch_id": "L-done", "launched_at": now - 3600, "ended": {"kind": "exited"}}),
            json!({"launch_id": "L-new", "launched_at": now - 60}),
        ];
        let pairs = vec![
            ("s-1".to_owned(), 10),
            ("s-1".to_owned(), 12),
            ("s-2".to_owned(), 11),
        ];
        let found = findings(&rows, &records, &pairs, now, true);
        let kinds: Vec<(&str, &str)> = found
            .iter()
            .map(|f| {
                (
                    f["kind"].as_str().unwrap(),
                    f["launch_id"]
                        .as_str()
                        .or(f["session_id"].as_str())
                        .unwrap(),
                )
            })
            .collect();
        // The same session in pids 10 and 12; the board run quiet 45 minutes (a session started
        // from the CLI may sit quiet: only a board run is stuck); the launch with no process.
        assert_eq!(
            kinds,
            [(DUPLICATE, "s-1"), (STUCK, "L-run"), (GONE, "L-gone")]
        );
        assert_eq!(found[0]["pids"], json!([10, 12]));
        assert_eq!(
            found[0]["folder"], "/work/a",
            "the item is named by its folder"
        );
        // Closing the quiet copy by hand takes the finding out of the kept watch at once.
        let root = tempfile::tempdir().unwrap();
        let kept = json!({"at": now, "findings": found});
        std::fs::write(root.path().join(FILE), serde_json::to_vec(&kept).unwrap()).unwrap();
        forget(root.path(), DUPLICATE, "s-1");
        let left: Vec<String> = last(root.path()).iter().map(key).collect();
        assert_eq!(left, ["stuck:L-run", "gone:L-gone"]);
        assert_eq!(found[1]["task"]["request"], "r-1");
    }

    #[test]
    fn a_board_run_writing_its_log_is_not_stuck_and_a_run_elsewhere_is_never_gone() {
        // SB-407: its folder quiet 45 minutes, its log written five minutes ago: working.
        let now = 100_000;
        let rows = vec![row(10, Some("L-log"), 45), row(11, Some("L-quiet"), 45)];
        let records = vec![
            json!({"launch_id": "L-log", "launched_at": now - 3600, "dispatch": {"workflow": "wf_1"},
                LOG_WRITTEN_AT: now - 300}),
            json!({"launch_id": "L-quiet", "launched_at": now - 3600, "dispatch": {"workflow": "wf_2"},
                LOG_WRITTEN_AT: now - 40 * 60}),
            json!({"launch_id": "L-remote", "launched_at": now - 3600,
                "dispatch": {"target": "studio"}}),
            json!({"launch_id": "L-gone", "launched_at": now - 3600, "dispatch": {"workflow": "wf_3"}}),
        ];
        let found = findings(&rows, &records, &[], now, true);
        let keys: Vec<String> = found.iter().map(key).collect();
        assert_eq!(keys, ["stuck:L-quiet", "gone:L-gone"]);
        assert_eq!(
            found[0]["quiet_minutes"], 40,
            "the log's quiet, the shorter one"
        );
        assert_eq!(found[0]["dispatch"], true);
        assert_eq!(found[1]["dispatch"], true);
        // Review round 1 (P1): with the process table unread (Windows, or a failed read), no
        // launch is gone; and a dispatched run whose home says it runs is never gone.
        let keys_of = |found: Vec<Value>| found.iter().map(key).collect::<Vec<_>>();
        assert_eq!(
            keys_of(findings(&rows, &records, &[], now, false)),
            ["stuck:L-quiet"]
        );
        let mut running = records.clone();
        running[3][HOME_RUNNING] = true.into();
        assert_eq!(
            keys_of(findings(&rows, &running, &[], now, true)),
            ["stuck:L-quiet"]
        );
        assert!(table_read(&json!({"degraded": []})));
        assert!(!table_read(&json!({"degraded": [
            {"source": "process_table", "reason": "not_read_on_this_platform"}
        ]})));
        // The log's time is read from the file, a stat only.
        let root = tempfile::tempdir().unwrap();
        let id = crate::launches::new_id();
        let logs = root
            .path()
            .join(crate::launches::DIR)
            .join(crate::launches::LOGS);
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join(format!("{id}.log")), "x").unwrap();
        let at = log_written_at(root.path(), &json!({"launch_id": id})).unwrap();
        assert!((super::now() - at).abs() < 60);
        assert!(log_written_at(root.path(), &json!({"launch_id": "../x"})).is_none());
    }

    #[test]
    fn the_tree_joins_live_sessions_to_their_launches_and_counts_them() {
        let now = 100_000;
        let scan = json!({"sessions": [row(10, Some("L-child"), 0), row(12, None, 40)], "observed_at": now, "degraded": []});
        let records = vec![
            json!({"launch_id": "L-root", "launched_at": now - 7200, "pool": "acme", "ended": {"kind": "exited"}, "launched_by": {"surface": "cli"}}),
            json!({"launch_id": "L-child", "launched_at": now - 3600, "pool": "acme", "continued_from": ["L-root"], "continues": "wf_2", "launched_by": {"surface": "fallback"}}),
        ];
        let tree = assemble(&scan, &records, &[("s-9".to_owned(), 10)], now);
        assert_eq!(tree["live"][0]["parent"], "L-root");
        assert_eq!(tree["live"][0]["surface"], "fallback");
        assert_eq!(tree["live"][0]["session_id"], "s-9");
        assert_eq!(tree["live"][0]["task"]["workflow"], "wf_2");
        assert_eq!(tree["counts"]["live"], 2);
        assert_eq!(tree["counts"]["quiet"], 1);
        assert_eq!(
            tree["counts"]["by_pool"],
            json!({"acme": 1, "unmanaged": 1})
        );
        assert_eq!(tree["launches"][0]["launch_id"], "L-child");
        assert_eq!(tree["launches"][0]["state"], "running");
        assert_eq!(tree["launches"][1]["state"], "ended");
    }

    #[test]
    fn a_gone_session_with_its_history_is_offered_once_and_never_after_it_was_continued() {
        // SB-509: a gone launch of a Claude Code session whose history is here is offered for
        // continuation; a board run, another agent, one with no session or no history is not.
        let gone = |id: &str, session: Option<&str>, provider: &str, dispatch: bool| {
            json!({"kind": GONE, "launch_id": id, "session_id": session, "provider": provider,
                "dispatch": dispatch})
        };
        let mut found = vec![
            gone("L-1", Some("s-1"), "claude", false),
            gone("L-2", Some("s-2"), "claude", true),
            gone("L-3", Some("s-3"), "codex", false),
            gone("L-4", None, "claude", false),
            gone("L-5", Some("s-5"), "claude", false),
        ];
        let mut offers = serde_json::Map::new();
        let none = HashSet::new();
        let closed = HashSet::new();
        let history = |s: &str, _: i64| s != "s-5";
        offer(&mut found, &mut offers, &none, &closed, history, 1000);
        let offered: Vec<&str> = found
            .iter()
            .filter(|f| f["continuable"] == true)
            .map(|f| f["launch_id"].as_str().unwrap())
            .collect();
        assert_eq!(offered, ["L-1"]);
        assert_eq!(offers["s-1"]["offered_at"], 1000);
        // The next minute: still offered, with its first time kept.
        let mut again = vec![gone("L-1", Some("s-1"), "claude", false)];
        offer(&mut again, &mut offers, &none, &closed, history, 1060);
        assert_eq!(again[0]["continuable"], true);
        assert_eq!(offers["s-1"]["offered_at"], 1000);
        // Continued: it runs, the offer is taken, and a later gone launch of it is not offered.
        let live: HashSet<&str> = ["s-1"].into();
        let mut running = vec![gone("L-1", Some("s-1"), "claude", false)];
        offer(&mut running, &mut offers, &live, &closed, history, 1120);
        assert!(running[0]["continuable"].is_null());
        assert_eq!(offers["s-1"]["taken_at"], 1120);
        let mut later = vec![gone("L-6", Some("s-1"), "claude", false)];
        offer(&mut later, &mut offers, &none, &closed, history, 1180);
        assert!(later[0]["continuable"].is_null(), "offered once");
        // Offers older than a week are forgotten.
        offer(
            &mut [],
            &mut offers,
            &none,
            &closed,
            history,
            1000 + OFFERS_KEPT,
        );
        assert!(offers.is_empty());
    }

    #[test]
    fn only_a_turn_cut_by_the_process_ending_is_offered_never_a_session_ended_on_purpose() {
        // SB-509 (review round 1 of PR #853): a Terminal launch records no end, so a session the
        // person left (window closed, `/exit`) or the duty officer closed reads as gone too.
        // Written 2 minutes after its last turn: cut. Within the margin, never written after, or
        // ended with `/exit`: left on purpose.
        assert!(cut_turn(Some(1120), 1000, ""));
        assert!(!cut_turn(Some(1000 + CUT_MARGIN_SECONDS), 1000, ""));
        assert!(!cut_turn(Some(900), 1000, ""));
        assert!(!cut_turn(None, 1000, ""));
        let exit = r#"{"type":"user","message":{"content":"<command-name>/exit</command-name>"}}"#;
        assert!(!cut_turn(
            Some(1120),
            1000,
            &format!("{{}}\n{exit}\n{{}}\n")
        ));
        // The baseline is the launch's last turn, else its start.
        let gone = |turn: Option<i64>| {
            json!({"kind": GONE, "launch_id": "L-1", "session_id": "s-1", "provider": "claude",
                "dispatch": false, "last_turn_at": turn, "launched_at": 500})
        };
        let seen_baseline = std::cell::Cell::new(0);
        let probe = |_: &str, b: i64| {
            seen_baseline.set(b);
            false
        };
        let (none, closed) = (HashSet::new(), HashSet::new());
        offer(
            &mut [gone(Some(900))],
            &mut serde_json::Map::new(),
            &none,
            &closed,
            probe,
            2000,
        );
        assert_eq!(seen_baseline.get(), 900);
        offer(
            &mut [gone(None)],
            &mut serde_json::Map::new(),
            &none,
            &closed,
            probe,
            2000,
        );
        assert_eq!(seen_baseline.get(), 500);
    }

    #[test]
    fn a_session_closed_on_purpose_is_not_offered() {
        // SB-509: the duty officer's or a host's close (`resumable.jsonl`) and the duty officer's
        // acts (`duty.jsonl`, outcome lines) since the window opened; an intent alone is no act.
        let resumable = vec![
            json!({"session": "s-host", "reason": "host_idle", "at": 900, "by": "host"}),
            json!({"session": "s-old", "reason": "idle", "at": 10, "by": "duty"}),
        ];
        let duty = vec![
            json!({"at": 950, "action": "close", "reason": "idle", "outcome": "closed", "session": "s-duty"}),
            json!({"at": 960, "phase": "intent", "action": "close", "session": "s-intent"}),
        ];
        let closed = closed_on_purpose(&resumable, &duty, 100);
        let mut names: Vec<&str> = closed.iter().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["s-duty", "s-host"]);
        let mut found = vec![
            json!({"kind": GONE, "launch_id": "L-1", "session_id": "s-duty",
            "provider": "claude", "dispatch": false, "last_turn_at": 100}),
        ];
        let mut offers = serde_json::Map::new();
        offer(
            &mut found,
            &mut offers,
            &HashSet::new(),
            &closed,
            |_, _| true,
            1000,
        );
        assert!(found[0]["continuable"].is_null());
        assert!(offers.is_empty());
    }

    #[test]
    fn continuing_from_needs_attention_takes_the_offer_and_clears_the_kept_finding() {
        let root = tempfile::tempdir().unwrap();
        let mut offers = serde_json::Map::new();
        offers.insert(
            "s-1".into(),
            json!({"launch_id": "L-1", "offered_at": 10, "taken_at": null}),
        );
        offers_write(root.path(), &offers);
        let kept = json!({"at": 10, "findings": [
            {"kind": GONE, "launch_id": "L-1", "session_id": "s-1", "continuable": true}]});
        std::fs::write(root.path().join(FILE), kept.to_string()).unwrap();
        continued(root.path(), "s-1", 20);
        assert_eq!(offers_read(root.path())["s-1"]["taken_at"], 20);
        assert_eq!(last(root.path())[0]["continuable"], false);
    }
}
