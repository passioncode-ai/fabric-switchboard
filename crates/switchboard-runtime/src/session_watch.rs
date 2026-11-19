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
    let pairs = pairs.into_iter().map(|(s, p, _)| (s, p)).collect();
    (scan, records, pairs)
}

/// SB-407: when a dispatched run's log (`launches/logs/<id>.log`) was last written; a stat only.
fn log_written_at(root: &Path, record: &Value) -> Option<i64> {
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
    // SB-508: every live Claude Code session is kept for a continuation after a reboot or a
    // crash; a scan that did not read the process table changes nothing.
    if table_read(&scan) {
        crate::live_ledger::observe(root, &claude_pids(&scan), now);
    }
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    let mut found = findings(&rows, &records, &pairs, now, table_read(&scan));
    describe_duplicates(root, &mut found);
    let kept = last(root);
    stamp_first_seen(&mut found, &kept, now);
    let before: HashSet<String> = kept.iter().map(key).collect();
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
    if found != kept {
        let bytes = serde_json::to_vec(&json!({"at": now, "findings": found})).unwrap_or_default();
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// SB-510: the field each kept finding carries: when the watch first found it (Unix seconds).
pub const FIRST_SEEN: &str = "first_seen";

/// SB-510: each finding's first sighting, carried from the kept watch (`kept`), else now, so the
/// watchdog can tell a finding that has stood for a while from a new one.
pub fn stamp_first_seen(found: &mut [Value], kept: &[Value], now: i64) {
    for f in found.iter_mut() {
        let k = key(f);
        let first = kept
            .iter()
            .find(|old| key(old) == k)
            .and_then(|old| old[FIRST_SEEN].as_i64())
            .unwrap_or(now);
        f[FIRST_SEEN] = first.into();
    }
}

/// SB-508: the live Claude Code processes a scan read.
fn claude_pids(scan: &Value) -> HashSet<u32> {
    scan["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["agent"] == "claude-code")
        .filter_map(|r| r["pid"].as_u64().and_then(|p| u32::try_from(p).ok()))
        .collect()
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
    fn a_finding_keeps_its_first_sighting_and_only_claude_code_pids_are_live_sessions() {
        // SB-510: the watchdog reads how long a finding has stood.
        let kept = vec![
            json!({"kind": DUPLICATE, "session_id": "s-1", FIRST_SEEN: 100}),
            json!({"kind": GONE, "launch_id": "L-old"}),
        ];
        let mut found = vec![
            json!({"kind": DUPLICATE, "session_id": "s-1"}),
            json!({"kind": GONE, "launch_id": "L-old"}),
            json!({"kind": GONE, "launch_id": "L-new"}),
        ];
        stamp_first_seen(&mut found, &kept, 500);
        let firsts: Vec<i64> = found
            .iter()
            .map(|f| f[FIRST_SEEN].as_i64().unwrap())
            .collect();
        assert_eq!(firsts, [100, 500, 500]);
        let scan = json!({"sessions": [
            {"pid": 10, "agent": "claude-code"}, {"pid": 11, "agent": "codex"},
            {"pid": 12, "agent": "claude-code"}
        ]});
        let mut pids: Vec<u32> = claude_pids(&scan).into_iter().collect();
        pids.sort_unstable();
        assert_eq!(pids, [10, 12]);
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
}
