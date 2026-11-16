//! Limit stops a session reported, as `recovery.json` keeps them (SB-114, SB-115, SB-199,
//! PRD-03): the record, its reading and pruning, and the settling a host or a session reports
//! (`finish`, `continued`). SB-397 D-2: the storage half of switchboard-runtime's `recovery`,
//! moved unchanged; deciding which account a stop resumes on stays with the app (`recovery`).
use crate::Provider;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const FILE: &str = "recovery.json";

/// The most rows a file may hold to be read (an older version wrote up to this many).
pub const MAX_ROWS: usize = 1024;

/// PRD-03 (D-4, 2026-10-24): the file keeps stops of the last 7 days, at most this many.
pub const KEEP_ROWS: usize = 512;

pub const KEEP_SECONDS: i64 = 7 * 86_400;

/// SB-199: no host continues a stop older than a day (Fabric Dashboards `RESUME_MAX_AGE_MS`);
/// a waiting or claimed stop past it reads, and is next written, as cancelled.
pub const HORIZON_SECONDS: i64 = 86_400;

pub const MAX_BYTES: usize = 4 * 1024 * 1024;

pub const INVALID: &str = "The recovery record is invalid or conflicts with an existing stop.";

pub const UNREADABLE: &str =
    "Recovery records could not be read. Restore the private file before continuing.";

/// What stopped the session (SB-115). A billing stop needs a person (a payment or top-up).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    #[default]
    Limit,
    Billing,
}

/// Why a stop is ready: an account with fresh quota, or a probe on the backoff (SB-115).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Capacity,
    Probe,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Waiting,
    Claimed,
    Resumed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    pub event: String,
    pub session: String,
    pub account: String,
    pub provider: Provider,
    pub pool: String,
    pub directory: PathBuf,
    pub stopped_at: i64,
    pub state: State,
    pub selected_account: Option<String>,
    /// Absent in SB-114 records: those read as `limit`, attempt 0, no mode.
    #[serde(default)]
    pub kind: StopKind,
    #[serde(default)]
    pub attempt: u32,
    /// How the claim was justified; set only by `claim`.
    #[serde(default)]
    pub mode: Option<Mode>,
}

pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 160
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
}

pub fn read(root: &Path) -> Result<Vec<Wait>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes = crate::private_fs::read_private(&path, MAX_BYTES as u64).map_err(|_| UNREADABLE)?;
    let rows: Vec<Wait> = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    let mut events = HashSet::new();
    if rows.len() > MAX_ROWS
        || rows.iter().any(|r| {
            !identifier(&r.event)
                || !identifier(&r.session)
                || !r.directory.is_absolute()
                || !events.insert(&r.event)
        })
    {
        return Err(UNREADABLE.into());
    }
    Ok(rows)
}

/// SB-199: first a live stop older than a day becomes cancelled. PRD-03 (D-4): drops stops
/// older than 7 days, then, past 512, the oldest finished stops
/// (resumed, cancelled) before the oldest live ones. `written`, the stop this write adds or
/// changes, is never dropped, whatever the rows' dates (N1 round 1: a clock that moved back made
/// it the oldest live row). The order of the rows kept is unchanged. Answers the events dropped.
pub fn prune(rows: &mut Vec<Wait>, now: i64, written: Option<&str>) -> Vec<String> {
    let kept = |r: &Wait| written == Some(r.event.as_str());
    let horizon = now.saturating_sub(HORIZON_SECONDS);
    for r in rows.iter_mut() {
        if !kept(r) && r.stopped_at < horizon && matches!(r.state, State::Waiting | State::Claimed)
        {
            r.state = State::Cancelled;
        }
    }
    let mut dropped = Vec::new();
    rows.retain(|r| {
        let keep = kept(r) || r.stopped_at >= now.saturating_sub(KEEP_SECONDS);
        if !keep {
            dropped.push(r.event.clone());
        }
        keep
    });
    if rows.len() > KEEP_ROWS {
        let mut order: Vec<usize> = (0..rows.len()).filter(|&i| !kept(&rows[i])).collect();
        order.sort_by_key(|&i| {
            let live = matches!(rows[i].state, State::Waiting | State::Claimed);
            (live, rows[i].stopped_at)
        });
        let drop: HashSet<usize> = order[..rows.len() - KEEP_ROWS].iter().copied().collect();
        let mut index = 0;
        rows.retain(|r| {
            let keep = !drop.contains(&index);
            if !keep {
                dropped.push(r.event.clone());
            }
            index += 1;
            keep
        });
    }
    dropped
}

/// The stops as of `now`: the file's rows without those `prune` drops (PRD-03), and the events
/// it dropped. Reading writes nothing; the next write leaves them out of the file.
pub fn current(root: &Path, now: i64) -> Result<(Vec<Wait>, Vec<String>), String> {
    let mut rows = read(root)?;
    let dropped = prune(&mut rows, now, None);
    Ok((rows, dropped))
}

/// Writes the stops as of `now`, pruned again with `written` kept; remembers the events that
/// left the file (`recovery-pruned.json`) and logs how many.
pub fn keep(
    root: &Path,
    rows: &mut Vec<Wait>,
    now: i64,
    mut dropped: Vec<String>,
    written: Option<&str>,
) -> Result<(), String> {
    dropped.extend(prune(rows, now, written));
    write(root, rows)?;
    if !dropped.is_empty() {
        remember_pruned(root, &dropped);
        crate::events::event(
            "recovery_pruned",
            &[(
                "dropped",
                crate::events::Field::Number(dropped.len() as i64),
            )],
        );
    }
    Ok(())
}

/// N1 round 1 (seam tier): the events pruned lately, ids only, newest last, at most
/// `MAX_PRUNED`, so a host that registers or finishes one again is answered as a replay rather
/// than as a new stop or an unknown one. A file that cannot be read is an empty list: it only
/// keeps replays idempotent, and never blocks a stop.
pub const PRUNED_FILE: &str = "recovery-pruned.json";

pub const MAX_PRUNED: usize = 2048;

pub fn pruned(root: &Path) -> Vec<String> {
    crate::private_fs::read_private(&root.join(PRUNED_FILE), 512 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
}

pub fn remember_pruned(root: &Path, events: &[String]) {
    let mut list = pruned(root);
    list.retain(|e| !events.contains(e));
    list.extend(events.iter().filter(|e| identifier(e)).cloned());
    let excess = list.len().saturating_sub(MAX_PRUNED);
    list.drain(..excess);
    if let Ok(bytes) = serde_json::to_vec(&list) {
        let _ = crate::private_fs::private_write(&root.join(PRUNED_FILE), &bytes);
    }
}

pub fn was_pruned(root: &Path, event: &str) -> bool {
    pruned(root).iter().any(|e| e == event)
}

/// SB-150: every read-modify-write of the file holds this, so the `StopFailure` registration
/// can run beside the owner transaction (it no longer waits behind a renewal or a sync) without
/// losing another writer's row. Held only for the file's read and rename.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn writing() -> std::sync::MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn write(root: &Path, rows: &[Wait]) -> Result<(), String> {
    let bytes = serde_json::to_vec(rows).map_err(|_| INVALID)?;
    if bytes.len() > MAX_BYTES {
        return Err(INVALID.into());
    }
    crate::private_fs::private_write(&root.join(FILE), &bytes)
}

pub fn finish(root: &Path, event: &str, resumed: bool, now: i64) -> Result<(), String> {
    let _writing = writing();
    let (mut rows, dropped) = current(root, now)?;
    let Some(row) = rows.iter_mut().find(|r| r.event == event) else {
        // A stop that aged out of the file is finished already: nothing to record.
        return if dropped.iter().any(|e| e == event) || was_pruned(root, event) {
            Ok(())
        } else {
            Err(INVALID.into())
        };
    };
    let target = if resumed {
        State::Resumed
    } else {
        State::Cancelled
    };
    // A stop already settled (the host's own outcome repeated, or settled meanwhile by its
    // session's `Stop` hook or the day's horizon, 0.6.83) is
    // finished: the host's report changes nothing and is not an error.
    if matches!(row.state, State::Resumed | State::Cancelled) {
        return Ok(());
    }
    if resumed && row.state != State::Claimed {
        return Err(INVALID.into());
    }
    row.state = target;
    keep(root, &mut rows, now, dropped, Some(event))
}

/// SB-204: the session went on by itself (Claude Code's `Stop` hook after a successful turn): its
/// waiting or claimed stop is resumed. Answers how many stops it settled; writes only then.
pub fn continued(root: &Path, session: &str, now: i64) -> Result<usize, String> {
    if !identifier(session) {
        return Err(INVALID.into());
    }
    let _writing = writing();
    let (mut rows, dropped) = current(root, now)?;
    let mut settled = Vec::new();
    for r in rows.iter_mut().filter(|r| r.session == session) {
        if matches!(r.state, State::Waiting | State::Claimed) {
            r.state = State::Resumed;
            settled.push(r.event.clone());
        }
    }
    let Some(last) = settled.last().cloned() else {
        return Ok(0);
    };
    keep(root, &mut rows, now, dropped, Some(&last))?;
    Ok(settled.len())
}

/// SB-204: whether `session` has a live stop, read without the lock or the owner, so the `Stop`
/// hook that runs after every turn of every session asks the owner only when there is one. An
/// unreadable file answers false: nothing could settle in it, and every turn of every session
/// would otherwise ask the owner for nothing.
pub fn session_has_live_stop(root: &Path, session: &str, now: i64) -> bool {
    match current(root, now) {
        Ok((rows, _)) => rows
            .iter()
            .any(|r| r.session == session && matches!(r.state, State::Waiting | State::Claimed)),
        Err(_) => false,
    }
}

/// SB-396 D2: every session with a waiting or claimed stop, read without the lock or the owner,
/// so the duty officer never closes a session that waits for an account. An unreadable file
/// answers none, as `session_has_live_stop` does.
pub fn live_stop_sessions(root: &Path, now: i64) -> HashSet<String> {
    current(root, now)
        .map(|(rows, _)| {
            rows.into_iter()
                .filter(|r| matches!(r.state, State::Waiting | State::Claimed))
                .map(|r| r.session)
                .collect()
        })
        .unwrap_or_default()
}
