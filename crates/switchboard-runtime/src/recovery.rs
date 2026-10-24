//! SB-114: durable host-reported stops and claim-once capacity notifications.
//! SB-115: a limit stop is not a wait. Without fresh capacity a stop still becomes ready as a
//! probe on a short backoff (2, 5, 10, then 15 minutes), never at the provider's reset; a spend
//! or billing stop is never probed. Stops can be registered by Claude Code's `StopFailure` hook.
//! No prompt, credential or process control lives here. The session host owns resumption.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use switchboard_core::{AuthKind, Provider, Snapshot};

const FILE: &str = "recovery.json";
/// The most rows a file may hold to be read (an older version wrote up to this many).
const MAX_ROWS: usize = 1024;
/// PRD-03 (D-4, 2026-10-24): the file keeps stops of the last 7 days, at most this many.
const KEEP_ROWS: usize = 512;
const KEEP_SECONDS: i64 = 7 * 86_400;
const MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_AGE: i64 = 300;
const INVALID: &str = "The recovery record is invalid or conflicts with an existing stop.";
const UNREADABLE: &str =
    "Recovery records could not be read. Restore the private file before continuing.";
const NOT_READY: &str = "This stop is not ready on that account. Read recovery status again.";
/// Seconds after a stop before a probe without fresh capacity, by attempt (SB-115).
pub(crate) const PROBE_BACKOFF: [i64; 4] = [120, 300, 600, 900];
/// A stop this soon after the session's previous resumed stop counts as that restart failing.
const REPEAT_WINDOW: i64 = 1800;

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

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 160
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
}

pub(crate) fn read(root: &Path) -> Result<Vec<Wait>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes = switchboard_core::private_fs::read_private(&path, MAX_BYTES as u64)
        .map_err(|_| UNREADABLE)?;
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

/// PRD-03 (D-4): drops stops older than 7 days, then, past 512, the oldest finished stops
/// (resumed, cancelled) before the oldest live ones. `written`, the stop this write adds or
/// changes, is never dropped, whatever the rows' dates (N1 round 1: a clock that moved back made
/// it the oldest live row). The order of the rows kept is unchanged. Answers the events dropped.
fn prune(rows: &mut Vec<Wait>, now: i64, written: Option<&str>) -> Vec<String> {
    let kept = |r: &Wait| written == Some(r.event.as_str());
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
fn current(root: &Path, now: i64) -> Result<(Vec<Wait>, Vec<String>), String> {
    let mut rows = read(root)?;
    let dropped = prune(&mut rows, now, None);
    Ok((rows, dropped))
}

/// Writes the stops as of `now`, pruned again with `written` kept; remembers the events that
/// left the file (`recovery-pruned.json`) and logs how many.
fn keep(
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
        crate::oplog::event(
            "recovery_pruned",
            &[("dropped", crate::oplog::Field::Number(dropped.len() as i64))],
        );
    }
    Ok(())
}

/// N1 round 1 (seam tier): the events pruned lately, ids only, newest last, at most
/// `MAX_PRUNED`, so a host that registers or finishes one again is answered as a replay rather
/// than as a new stop or an unknown one. A file that cannot be read is an empty list: it only
/// keeps replays idempotent, and never blocks a stop.
const PRUNED_FILE: &str = "recovery-pruned.json";
const MAX_PRUNED: usize = 2048;
fn pruned(root: &Path) -> Vec<String> {
    switchboard_core::private_fs::read_private(&root.join(PRUNED_FILE), 512 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
}
fn remember_pruned(root: &Path, events: &[String]) {
    let mut list = pruned(root);
    list.retain(|e| !events.contains(e));
    list.extend(events.iter().filter(|e| identifier(e)).cloned());
    let excess = list.len().saturating_sub(MAX_PRUNED);
    list.drain(..excess);
    if let Ok(bytes) = serde_json::to_vec(&list) {
        let _ = switchboard_core::private_fs::private_write(&root.join(PRUNED_FILE), &bytes);
    }
}
fn was_pruned(root: &Path, event: &str) -> bool {
    pruned(root).iter().any(|e| e == event)
}

/// SB-150: every read-modify-write of the file holds this, so the `StopFailure` registration
/// can run beside the owner transaction (it no longer waits behind a renewal or a sync) without
/// losing another writer's row. Held only for the file's read and rename.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn writing() -> std::sync::MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(|p| p.into_inner())
}

fn write(root: &Path, rows: &[Wait]) -> Result<(), String> {
    let bytes = serde_json::to_vec(rows).map_err(|_| INVALID)?;
    if bytes.len() > MAX_BYTES {
        return Err(INVALID.into());
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

pub(crate) fn register(
    root: &Path,
    snapshot: &Snapshot,
    event: &str,
    session: &str,
    account: &str,
    directory: &Path,
    now: i64,
) -> Result<(), String> {
    if !identifier(event) || !identifier(session) {
        return Err(INVALID.into());
    }
    let directory = directory.canonicalize().map_err(|_| INVALID)?;
    let _writing = writing();
    let (mut rows, dropped) = current(root, now)?;
    if let Some(old) = rows.iter().find(|r| r.event == event) {
        return if old.session == session && old.account == account && old.directory == directory {
            Ok(())
        } else {
            Err(INVALID.into())
        };
    }
    // A replay of a stop that aged out of the file stores nothing again.
    if dropped.iter().any(|e| e == event) || was_pruned(root, event) {
        return Ok(());
    }
    if rows
        .iter()
        .any(|r| r.session == session && matches!(r.state, State::Waiting | State::Claimed))
    {
        return Err(INVALID.into());
    }
    let row = new_row(snapshot, event, session, account, directory, now)?;
    rows.push(row);
    keep(root, &mut rows, now, dropped, Some(event))
}

fn new_row(
    snapshot: &Snapshot,
    event: &str,
    session: &str,
    account: &str,
    directory: PathBuf,
    now: i64,
) -> Result<Wait, String> {
    // PRD-03: no row limit refuses a stop here: `keep` makes room.
    let a = snapshot
        .accounts
        .iter()
        .find(|a| a.id == account)
        .ok_or(INVALID)?;
    if a.kind != AuthKind::OAuth {
        return Err(INVALID.into());
    }
    crate::launch::project_allows(snapshot, a, &directory)?;
    Ok(Wait {
        event: event.into(),
        session: session.into(),
        account: account.into(),
        provider: a.provider,
        pool: a.pool.clone(),
        directory,
        stopped_at: now,
        state: State::Waiting,
        selected_account: None,
        kind: StopKind::Limit,
        attempt: 0,
        mode: None,
    })
}

/// What a `StopFailure` registration did (SB-115), for the hook's log line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stopped {
    New(String),
    /// The session's waiting stop happened again: it keeps its event, one attempt later.
    Again(String),
}

/// Registers a stop the session reported itself (Claude Code's `StopFailure` hook, SB-115).
/// One live stop per session still holds: a waiting stop is moved to now with one more attempt;
/// a claimed one is closed as resumed — the session ran, since it stopped again — and a new
/// stop follows it. A stop within 30 minutes of the session's previous resumed stop is one
/// attempt later; otherwise the backoff starts again.
pub(crate) fn register_stop(
    root: &Path,
    snapshot: &Snapshot,
    session: &str,
    account: &str,
    directory: &Path,
    kind: StopKind,
    now: i64,
) -> Result<Stopped, String> {
    if !identifier(session) {
        return Err(INVALID.into());
    }
    let directory = directory.canonicalize().map_err(|_| INVALID)?;
    let _writing = writing();
    let (mut rows, dropped) = current(root, now)?;
    if let Some(live) = rows
        .iter_mut()
        .find(|r| r.session == session && r.state == State::Waiting)
    {
        live.stopped_at = now;
        live.attempt = live.attempt.saturating_add(1);
        live.kind = kind;
        let event = live.event.clone();
        keep(root, &mut rows, now, dropped, Some(&event))?;
        return Ok(Stopped::Again(event));
    }
    let mut attempt = 0;
    if let Some(claimed) = rows
        .iter_mut()
        .find(|r| r.session == session && r.state == State::Claimed)
    {
        claimed.state = State::Resumed;
        attempt = claimed.attempt.saturating_add(1);
    } else if let Some(last) = rows
        .iter()
        .filter(|r| r.session == session && r.state == State::Resumed)
        .max_by_key(|r| r.stopped_at)
    {
        if now - last.stopped_at <= REPEAT_WINDOW {
            attempt = last.attempt.saturating_add(1);
        }
    }
    let event = format!("cc-{session}-{now}");
    if !identifier(&event) || rows.iter().any(|r| r.event == event) {
        return Err(INVALID.into());
    }
    let mut row = new_row(snapshot, &event, session, account, directory, now)?;
    row.kind = kind;
    row.attempt = attempt;
    rows.push(row);
    keep(root, &mut rows, now, dropped, Some(&event))?;
    Ok(Stopped::New(event))
}

pub(crate) fn candidates(
    wait: &Wait,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    sign_in: &HashSet<String>,
    now: i64,
) -> Vec<String> {
    if wait.state != State::Waiting {
        return vec![];
    }
    // A changed/deleted source binding requires the host to cancel and register a new stop.
    if !snapshot
        .accounts
        .iter()
        .any(|a| a.id == wait.account && a.provider == wait.provider && a.pool == wait.pool)
    {
        return vec![];
    }
    let held_identities: HashSet<_> = snapshot
        .accounts
        .iter()
        .filter(|a| limited.contains(&a.id))
        .filter_map(|a| a.identity.as_ref().map(|i| (a.provider, i)))
        .collect();
    let mut eligible: Vec<_> = snapshot
        .accounts
        .iter()
        .filter(|a| {
            a.enabled
                && a.kind == AuthKind::OAuth
                && a.provider == wait.provider
                && a.pool == wait.pool
                && !limited.contains(&a.id)
                && !sign_in.contains(&a.id)
                && !a
                    .identity
                    .as_ref()
                    .is_some_and(|i| held_identities.contains(&(a.provider, i)))
                && crate::launch::project_allows(snapshot, a, &wait.directory).is_ok()
                && a.usage_health.as_ref().is_some_and(|h| {
                    h.status == "ok" && h.checked_at <= now && now - h.checked_at <= MAX_AGE
                })
                && a.usage.as_ref().is_some_and(|u| {
                    u.observed_at > wait.stopped_at
                        && u.observed_at <= now
                        && now - u.observed_at <= MAX_AGE
                        && u.account_used_percent()
                            .is_some_and(|n| n.is_finite() && (0.0..100.0).contains(&n))
                        && u.resets_at.is_none_or(|t| t > now)
                        && u.windows
                            .iter()
                            .all(|w| w.resets_at.is_none_or(|t| t > now))
                })
        })
        .collect();
    eligible.sort_by(|a, b| a.id.cmp(&b.id));
    eligible.into_iter().map(|a| a.id.clone()).collect()
}

/// Accounts a probe may go to (SB-115): the same provider, pool and folder permission as
/// capacity, but no fresh quota is required. A spend cap or a needed sign-in still excludes an
/// account; accounts without an active hold come first.
fn probe_accounts(
    wait: &Wait,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    capped: &HashSet<String>,
    sign_in: &HashSet<String>,
) -> Vec<String> {
    let mut accounts: Vec<_> = snapshot
        .accounts
        .iter()
        .filter(|a| {
            a.enabled
                && a.kind == AuthKind::OAuth
                && a.provider == wait.provider
                && a.pool == wait.pool
                && !capped.contains(&a.id)
                && !sign_in.contains(&a.id)
                && crate::launch::project_allows(snapshot, a, &wait.directory).is_ok()
        })
        .map(|a| (limited.contains(&a.id), a.id.clone()))
        .collect();
    accounts.sort();
    accounts.into_iter().map(|(_, id)| id).collect()
}

/// Whether a stop is ready, and on which accounts (SB-115). Both `status` and `claim` read it,
/// so a probe can be claimed exactly when status offers it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ready {
    pub mode: Option<Mode>,
    pub accounts: Vec<String>,
    /// When a probe becomes due; none for a billing stop or when no account may be probed.
    pub probe_at: Option<i64>,
}

pub(crate) fn readiness(
    wait: &Wait,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    capped: &HashSet<String>,
    sign_in: &HashSet<String>,
    now: i64,
) -> Ready {
    let capacity = candidates(wait, snapshot, limited, sign_in, now);
    if !capacity.is_empty() {
        return Ready {
            mode: Some(Mode::Capacity),
            accounts: capacity,
            probe_at: None,
        };
    }
    // `candidates` answers empty for a source binding that changed; a probe follows it.
    if wait.state != State::Waiting
        || wait.kind == StopKind::Billing
        || !snapshot
            .accounts
            .iter()
            .any(|a| a.id == wait.account && a.provider == wait.provider && a.pool == wait.pool)
    {
        return Ready::default();
    }
    let accounts = probe_accounts(wait, snapshot, limited, capped, sign_in);
    if accounts.is_empty() {
        return Ready::default();
    }
    let step = PROBE_BACKOFF[(wait.attempt as usize).min(PROBE_BACKOFF.len() - 1)];
    let probe_at = wait.stopped_at.saturating_add(step);
    if now < probe_at {
        return Ready {
            probe_at: Some(probe_at),
            ..Ready::default()
        };
    }
    Ready {
        mode: Some(Mode::Probe),
        accounts,
        probe_at: Some(probe_at),
    }
}

/// Must run under the runtime mutation lock (or the offline store's exclusive lock).
#[allow(clippy::too_many_arguments)]
pub(crate) fn claim(
    root: &Path,
    event: &str,
    account: &str,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    capped: &HashSet<String>,
    sign_in: &HashSet<String>,
    now: i64,
) -> Result<Wait, String> {
    let _writing = writing();
    let (mut rows, dropped) = current(root, now)?;
    let row = rows
        .iter_mut()
        .find(|r| r.event == event)
        .ok_or(NOT_READY)?;
    let ready = readiness(row, snapshot, limited, capped, sign_in, now);
    if !ready.accounts.iter().any(|id| id == account) {
        return Err(NOT_READY.into());
    }
    row.state = State::Claimed;
    row.selected_account = Some(account.into());
    row.mode = ready.mode;
    let result = row.clone();
    keep(root, &mut rows, now, dropped, Some(event))?;
    Ok(result)
}

pub(crate) fn finish(root: &Path, event: &str, resumed: bool, now: i64) -> Result<(), String> {
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
    if row.state == target {
        return Ok(());
    }
    if (resumed && row.state != State::Claimed)
        || matches!(row.state, State::Resumed | State::Cancelled)
    {
        return Err(INVALID.into());
    }
    row.state = target;
    keep(root, &mut rows, now, dropped, Some(event))
}

pub(crate) fn status(
    root: &Path,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    capped: &HashSet<String>,
    sign_in: &HashSet<String>,
    now: i64,
) -> Result<serde_json::Value, String> {
    let (rows, _) = current(root, now)?;
    let waiting = rows.iter().filter(|r| r.state == State::Waiting).count();
    let claimed = rows.iter().filter(|r| r.state == State::Claimed).count();
    let sessions: Vec<_> = rows
        .iter()
        .map(|r| {
            let ready = readiness(r, snapshot, limited, capped, sign_in, now);
            serde_json::json!({"stop": r, "ready": ready.mode.is_some(), "mode": ready.mode,
                "candidates": ready.accounts, "probe_at": ready.probe_at})
        })
        .collect();
    let ready = sessions.iter().filter(|r| r["ready"] == true).count();
    Ok(serde_json::json!({"version": 1, "observed_at": now,
        "coverage": "registered_sessions_only", "source": "host_reported",
        "accounts": snapshot.accounts.len(), "limit_unavailable_accounts": limited.len(),
        "waiting_sessions": waiting, "ready_sessions": ready, "claimed_sessions": claimed,
        "sessions": sessions}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Account, Usage, UsageHealth};
    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            label: "Synthetic".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            external_identity: None,
            usage: Some(Usage {
                windows: vec![],
                used_percent: 10.0,
                observed_at: 101,
                resets_at: Some(1000),
                source: "provider".into(),
            }),
            usage_health: Some(UsageHealth {
                status: "ok".into(),
                checked_at: 101,
                next_check_at: 281,
            }),
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            accounts: vec![account("old"), account("new")],
            ..Default::default()
        }
    }
    #[test]
    fn a_stop_survives_restart_and_a_reset_is_not_capacity() {
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let waits = read(root.path()).unwrap();
        let held = HashSet::from(["old".into()]);
        assert_eq!(
            candidates(&waits[0], &s, &held, &HashSet::new(), 102),
            vec!["new"]
        );
        assert_eq!(
            candidates(&waits[0], &s, &HashSet::new(), &HashSet::new(), 102).len(),
            2,
            "the old account can return with a fresh post-stop observation"
        );
        s.accounts
            .iter_mut()
            .for_each(|a| a.usage.as_mut().unwrap().observed_at = 100);
        assert!(candidates(&waits[0], &s, &HashSet::new(), &HashSet::new(), 102).is_empty());
        assert_eq!(
            read(root.path()).unwrap().len(),
            1,
            "expiry does not erase the stop"
        );
    }
    #[test]
    fn a_notification_is_claimed_once_and_never_replayed_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            102,
        )
        .unwrap();
        assert!(register(
            root.path(),
            &s,
            "limit:1",
            "session-2",
            "old",
            root.path(),
            102
        )
        .is_err());
        assert!(register(
            root.path(),
            &s,
            "limit:2",
            "session-1",
            "old",
            root.path(),
            102
        )
        .is_err());
        assert!(finish(root.path(), "limit:1", true, 1000).is_err());
        let claimed = claim(root.path(), "limit:1", "new", &s, &none, &none, &none, 102).unwrap();
        assert_eq!(claimed.state, State::Claimed);
        assert!(claim(root.path(), "limit:1", "new", &s, &none, &none, &none, 102).is_err());
        assert_eq!(read(root.path()).unwrap()[0].state, State::Claimed);
        finish(root.path(), "limit:1", true, 1000).unwrap();
        finish(root.path(), "limit:1", true, 1000).unwrap();
        assert!(finish(root.path(), "limit:1", false, 1000).is_err());
        assert!(claim(root.path(), "limit:1", "new", &s, &none, &none, &none, 102).is_err());
    }
    #[test]
    fn restrictions_hidden_holds_and_stale_observations_win_over_percentages() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let wait = &read(root.path()).unwrap()[0];
        let held = HashSet::from(["old".into(), "new".into()]);
        assert!(candidates(wait, &s, &held, &none, 102).is_empty());
        assert!(claim(root.path(), "limit:1", "new", &s, &held, &none, &none, 102).is_err());
        assert!(candidates(wait, &s, &none, &none, 402).is_empty());
        for mutate in [
            (|a: &mut Account| a.pool = "other-project".into()) as fn(&mut Account),
            |a| a.provider = Provider::Codex,
            |a| a.enabled = false,
            |a| a.kind = AuthKind::ApiKey,
            |a| a.usage_health.as_mut().unwrap().status = "failed".into(),
            |a| a.usage.as_mut().unwrap().resets_at = Some(102),
            |a| a.usage.as_mut().unwrap().used_percent = 100.0,
        ] {
            let mut changed = snapshot();
            mutate(&mut changed.accounts[1]);
            assert!(
                candidates(wait, &changed, &HashSet::from(["old".into()]), &none, 102).is_empty()
            );
        }
        assert!(candidates(
            wait,
            &s,
            &HashSet::from(["old".into()]),
            &HashSet::from(["new".into()]),
            102
        )
        .is_empty());
    }
    #[test]
    fn cancelled_work_is_not_ready_and_corrupt_storage_is_not_an_empty_queue() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        finish(root.path(), "limit:1", false, 1000).unwrap();
        assert_eq!(
            status(root.path(), &s, &none, &none, &none, 102).unwrap()["waiting_sessions"],
            0
        );
        switchboard_core::private_fs::private_write(&root.path().join(FILE), b"broken").unwrap();
        assert!(read(root.path()).is_err());
        assert!(register(
            root.path(),
            &s,
            "limit:2",
            "session-2",
            "old",
            root.path(),
            103
        )
        .is_err());
    }

    #[test]
    fn changed_project_permissions_and_duplicate_identity_refuse_capacity() {
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let wait = &read(root.path()).unwrap()[0];
        let held = HashSet::from(["old".into()]);
        let none = HashSet::new();
        s.accounts[0].identity = Some("same-login".into());
        s.accounts[1].identity = Some("same-login".into());
        assert!(candidates(wait, &s, &held, &none, 102).is_empty());
        s.accounts[1].identity = Some("different-login".into());
        assert_eq!(candidates(wait, &s, &held, &none, 102), vec!["new"]);
        // The folder becomes reserved after registration. Neither default-pool account
        // may run there even though both have successful, fresh quota observations.
        s.projects.push(switchboard_core::Project {
            pool: "reserved".into(),
            name: "Synthetic".into(),
            folders: vec![wait.directory.to_string_lossy().into_owned()],
            created_at: 101,
            sources: vec![],
            organization: None,
        });
        let mut reserved = account("reserved-account");
        reserved.pool = "reserved".into();
        s.accounts.push(reserved);
        assert!(candidates(wait, &s, &held, &none, 102).is_empty());
        assert!(claim(root.path(), "limit:1", "new", &s, &held, &none, &none, 102).is_err());
        assert_eq!(read(root.path()).unwrap()[0].state, State::Waiting);
    }

    #[tokio::test]
    async fn simultaneous_host_claims_are_serialized_by_the_owner() {
        use crate::{fixtures, Operation};
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), fixtures::signed_out, fixtures::fails).await;
        let old = fixtures::save(&runtime.store, "old", "default");
        let new = fixtures::save(&runtime.store, "new", "default");
        let now = crate::monitor::now();
        register(
            root.path(),
            &runtime.store.snapshot().unwrap(),
            "limit:1",
            "session-1",
            &old.id,
            root.path(),
            now - 1,
        )
        .unwrap();
        let mut usage = account("fixture").usage.unwrap();
        usage.observed_at = now;
        usage.resets_at = Some(now + 600);
        runtime.store.observe(&new.id, usage).unwrap();
        runtime
            .store
            .usage_health(&new.id, "ok", now, now + 180)
            .unwrap();
        let op = || Operation::RecoveryClaim {
            event: "limit:1".into(),
            account: new.id.clone(),
        };
        let (a, b) = tokio::join!(runtime.execute(op()), runtime.execute(op()));
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert_eq!(read(root.path()).unwrap()[0].state, State::Claimed);
        let status = runtime.execute(Operation::Recovery).await.unwrap();
        assert_eq!(status["claimed_sessions"], 1);
        assert_eq!(status["ready_sessions"], 0);
    }

    #[test]
    fn a_probe_during_a_hidden_hold_cannot_wake_work_after_its_deadline() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(crate::limits::EVIDENCE_FILE);
        let state = crate::limits::LimitState::kept_in(file.clone(), 100);
        let mut s = snapshot();
        state.refused(&s, &[("old".into(), 100)], 100);
        let until = state.report(100)[0]["until"].as_i64().unwrap();
        assert_eq!(state.report(100)[0]["hidden"], true);
        // A successful 98% probe before expiry does not contradict the hidden hold.
        let usage = s.accounts[0].usage.as_mut().unwrap();
        usage.observed_at = until - 1;
        usage.used_percent = 98.0;
        usage.resets_at = Some(until + 3600);
        state.refresh(&s, None, &[], until + 1);
        assert!(state.limited_ids(until + 1).is_empty());
        assert!(state.report(until + 1).is_empty());
        assert!(state.recovery_blocked(&s, until + 1).contains("old"));
        let restarted = crate::limits::LimitState::kept_in(file, until + 1);
        assert!(restarted.recovery_blocked(&s, until + 1).contains("old"));
        s.accounts[0].usage.as_mut().unwrap().observed_at = until + 2;
        assert!(!restarted.recovery_blocked(&s, until + 2).contains("old"));
    }

    #[test]
    fn an_oversized_update_preserves_the_readable_registry() {
        let root = tempfile::tempdir().unwrap();
        register(
            root.path(),
            &snapshot(),
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let mut rows = read(root.path()).unwrap();
        rows[0].directory = format!("/{}", "x".repeat(MAX_BYTES)).into();
        assert!(write(root.path(), &rows).is_err());
        assert_eq!(
            read(root.path()).unwrap()[0].directory,
            root.path().canonicalize().unwrap()
        );
    }

    /// Both accounts held: no fresh capacity anywhere, the situation SB-115 changes.
    fn held() -> HashSet<String> {
        HashSet::from(["old".into(), "new".into()])
    }

    #[test]
    fn a_stop_without_capacity_is_probed_on_the_backoff_not_at_the_reset() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let wait = &read(root.path()).unwrap()[0];
        // The accounts' reset is at 1000; the probe comes 120 s after the stop.
        let early = readiness(wait, &s, &held(), &none, &none, 219);
        assert_eq!(early.mode, None);
        assert_eq!(early.probe_at, Some(220));
        let due = readiness(wait, &s, &held(), &none, &none, 220);
        assert_eq!(due.mode, Some(Mode::Probe));
        assert_eq!(due.accounts, vec!["new", "old"]);
        // Fresh capacity is still ready at once, and says so.
        let fresh = readiness(wait, &s, &HashSet::from(["old".into()]), &none, &none, 102);
        assert_eq!(
            (fresh.mode, fresh.accounts),
            (Some(Mode::Capacity), vec!["new".into()])
        );
        let mut later = wait.clone();
        for (attempt, step) in [(1, 300), (2, 600), (3, 900), (9, 900)] {
            later.attempt = attempt;
            let r = readiness(&later, &s, &held(), &none, &none, 100);
            assert_eq!(r.probe_at, Some(100 + step), "attempt {attempt}");
        }
    }

    #[test]
    fn a_billing_stop_is_never_probed() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        let stopped = register_stop(
            root.path(),
            &s,
            "session-1",
            "old",
            root.path(),
            StopKind::Billing,
            100,
        )
        .unwrap();
        let Stopped::New(event) = stopped else {
            panic!("a first stop is new")
        };
        let wait = &read(root.path()).unwrap()[0];
        assert_eq!(
            readiness(wait, &s, &held(), &none, &none, 100_000),
            Ready::default()
        );
        assert!(claim(
            root.path(),
            &event,
            "new",
            &s,
            &held(),
            &none,
            &none,
            100_000
        )
        .is_err());
        // A limit stop in a pool whose every account is spend-capped is not probed either.
        register(
            root.path(),
            &s,
            "limit:2",
            "session-2",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let wait = read(root.path())
            .unwrap()
            .into_iter()
            .find(|w| w.event == "limit:2")
            .unwrap();
        assert_eq!(
            readiness(&wait, &s, &held(), &held(), &none, 100_000),
            Ready::default()
        );
        let one_capped = readiness(
            &wait,
            &s,
            &held(),
            &HashSet::from(["old".into()]),
            &none,
            100_000,
        );
        assert_eq!(one_capped.accounts, vec!["new"]);
    }

    #[test]
    fn a_probe_claim_is_checked_by_the_same_rule_and_taken_once() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        assert!(claim(
            root.path(),
            "limit:1",
            "new",
            &s,
            &held(),
            &none,
            &none,
            219
        )
        .is_err());
        let status = status(root.path(), &s, &held(), &none, &none, 220).unwrap();
        assert_eq!(status["ready_sessions"], 1);
        assert_eq!(status["sessions"][0]["mode"], "probe");
        let claimed = claim(
            root.path(),
            "limit:1",
            "new",
            &s,
            &held(),
            &none,
            &none,
            220,
        )
        .unwrap();
        assert_eq!(
            (claimed.state, claimed.mode),
            (State::Claimed, Some(Mode::Probe))
        );
        assert!(claim(
            root.path(),
            "limit:1",
            "old",
            &s,
            &held(),
            &none,
            &none,
            221
        )
        .is_err());
        assert_eq!(read(root.path()).unwrap()[0].mode, Some(Mode::Probe));
        // A signed-out account is not a probe target.
        let root = tempfile::tempdir().unwrap();
        register(
            root.path(),
            &s,
            "limit:1",
            "session-1",
            "old",
            root.path(),
            100,
        )
        .unwrap();
        let sign_in = HashSet::from(["new".into()]);
        assert!(claim(
            root.path(),
            "limit:1",
            "new",
            &s,
            &held(),
            &none,
            &sign_in,
            220
        )
        .is_err());
    }

    #[test]
    fn repeated_stops_of_one_session_raise_the_attempt() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        let at = |t| {
            register_stop(
                root.path(),
                &s,
                "session-1",
                "old",
                root.path(),
                StopKind::Limit,
                t,
            )
        };
        let Stopped::New(first) = at(100).unwrap() else {
            panic!()
        };
        // Stopping again while it waits keeps the event, one attempt later, from the new time.
        assert_eq!(at(150).unwrap(), Stopped::Again(first.clone()));
        let rows = read(root.path()).unwrap();
        assert_eq!(
            (rows.len(), rows[0].attempt, rows[0].stopped_at),
            (1, 1, 150)
        );
        claim(root.path(), &first, "new", &s, &held(), &none, &none, 450).unwrap();
        // A stop after the claim: the session ran, so the claim is closed as resumed.
        let Stopped::New(second) = at(500).unwrap() else {
            panic!()
        };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows[0].state, State::Resumed);
        let row = rows.iter().find(|r| r.event == second).unwrap();
        assert_eq!((row.attempt, row.state.clone()), (2, State::Waiting));
        finish(root.path(), &second, false, 1000).unwrap();
        // Cancelled work does not count; a resumed stop within 30 minutes does.
        let Stopped::New(third) = at(600).unwrap() else {
            panic!()
        };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.iter().find(|r| r.event == third).unwrap().attempt, 2);
        finish(root.path(), &third, false, 1000).unwrap();
        let Stopped::New(fourth) = at(150 + REPEAT_WINDOW + 1).unwrap() else {
            panic!()
        };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.iter().find(|r| r.event == fourth).unwrap().attempt, 0);
        // An unknown session id shape or account registers nothing.
        assert!(register_stop(
            root.path(),
            &s,
            "bad id",
            "old",
            root.path(),
            StopKind::Limit,
            700
        )
        .is_err());
        assert!(register_stop(
            root.path(),
            &s,
            "session-2",
            "missing",
            root.path(),
            StopKind::Limit,
            700
        )
        .is_err());
    }

    fn stop(n: usize, at: i64, state: State, directory: &Path) -> Wait {
        Wait {
            event: format!("limit:{n}"),
            session: format!("session-{n}"),
            account: "old".into(),
            provider: Provider::Claude,
            pool: "default".into(),
            directory: directory.to_owned(),
            stopped_at: at,
            state,
            selected_account: None,
            kind: StopKind::Limit,
            attempt: 0,
            mode: None,
        }
    }
    const DAY: i64 = 86_400;
    /// PRD-03 (iteration 2026-10-24, D-4): recovery.json was never pruned (120 rows, the oldest
    /// 9.5 days old, on the operator's Mac). A stop older than 7 days leaves the answer at once
    /// and the file at the next write.
    #[test]
    fn stops_older_than_a_week_are_dropped_at_read_and_at_the_next_write() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        write(
            root.path(),
            &[
                stop(1, now - 8 * DAY, State::Waiting, &dir),
                stop(2, now - 8 * DAY, State::Resumed, &dir),
                stop(3, now - DAY, State::Waiting, &dir),
            ],
        )
        .unwrap();
        let s = snapshot();
        let none = HashSet::new();
        let answer = status(root.path(), &s, &none, &none, &none, now).unwrap();
        assert_eq!(answer["sessions"].as_array().unwrap().len(), 1, "{answer}");
        assert_eq!(answer["waiting_sessions"], 1);
        assert_eq!(read(root.path()).unwrap().len(), 3, "a read writes nothing");
        register(root.path(), &s, "limit:4", "session-4", "old", &dir, now).unwrap();
        let events: Vec<_> = read(root.path())
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(events, ["limit:3", "limit:4"]);
    }
    /// PRD-03: a file at its row limit refused every new stop as "invalid or conflicts with an
    /// existing stop". A new stop is always kept; the file keeps the newest 512, finished stops
    /// leaving before waiting ones.
    #[test]
    fn a_full_file_keeps_a_new_stop_and_drops_finished_ones_first() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let mut rows = Vec::new();
        for n in 0..MAX_ROWS {
            let state = if n < 600 {
                State::Resumed
            } else {
                State::Waiting
            };
            rows.push(stop(n, now - DAY + n as i64, state, &dir));
        }
        write(root.path(), &rows).unwrap();
        let s = snapshot();
        let stopped = register_stop(
            root.path(),
            &s,
            "session-new",
            "old",
            &dir,
            StopKind::Limit,
            now,
        )
        .unwrap();
        let Stopped::New(event) = stopped else {
            panic!("{stopped:?}")
        };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.len(), KEEP_ROWS);
        assert!(
            rows.iter().any(|r| r.event == event),
            "the new stop is kept"
        );
        let waiting = rows.iter().filter(|r| r.state == State::Waiting).count();
        assert_eq!(waiting, MAX_ROWS - 600 + 1, "every waiting stop is kept");
        // The finished stops kept are the newest ones.
        let oldest_resumed = rows
            .iter()
            .filter(|r| r.state == State::Resumed)
            .map(|r| r.stopped_at)
            .min()
            .unwrap();
        assert_eq!(
            oldest_resumed,
            now - DAY + (600 - (KEEP_ROWS - waiting)) as i64
        );
    }

    /// N1 round 1 (unit tier): with more than 512 live stops stamped later than now (a clock
    /// that moved back), the stop just written was the oldest live row and was dropped while
    /// `register_stop` answered `New`. The row just written is always kept.
    #[test]
    fn the_stop_just_written_is_kept_however_the_rows_are_dated() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let rows: Vec<_> = (0..600)
            .map(|n| stop(n, now + 60, State::Waiting, &dir))
            .collect();
        write(root.path(), &rows).unwrap();
        let Stopped::New(event) = register_stop(
            root.path(),
            &snapshot(),
            "session-new",
            "old",
            &dir,
            StopKind::Limit,
            now,
        )
        .unwrap() else {
            panic!()
        };
        let rows = read(root.path()).unwrap();
        assert!(rows.iter().any(|r| r.event == event));
        assert_eq!(rows.len(), KEEP_ROWS);
    }
    /// N1 round 1: `finish` wrote the file without pruning it, so a file past its bounds stayed
    /// there until the next stop.
    #[test]
    fn finishing_a_stop_prunes_the_file_too() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = now_seconds();
        let mut rows = vec![stop(1, now - 8 * DAY, State::Waiting, &dir)];
        rows.push(stop(2, now, State::Waiting, &dir));
        write(root.path(), &rows).unwrap();
        // N1 round 3 (planted defect D15): finishing records the pruned event and logs it, as
        // every other write does, so a replay of `limit:1` is still answered as one.
        let (finished, logged) =
            crate::oplog::capture(|| finish(root.path(), "limit:2", false, now));
        finished.unwrap();
        let events: Vec<_> = read(root.path())
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(events, ["limit:2"]);
        assert_eq!(pruned(root.path()), ["limit:1"], "the pruned id is kept");
        assert_eq!(logged, ["recovery_pruned"]);
        finish(root.path(), "limit:1", true, now).unwrap();
    }
    /// N1 round 1 (seam tier): a host that registers an event again after it was pruned is
    /// answered as a replay (nothing new is stored); finishing it is a no-op; claiming it is not
    /// ready. The pruned event ids are kept, ids only and bounded, in recovery-pruned.json.
    #[test]
    fn a_pruned_event_replayed_is_not_a_new_stop() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = now_seconds();
        write(
            root.path(),
            &[
                stop(1, now - 8 * DAY, State::Waiting, &dir),
                stop(2, now, State::Waiting, &dir),
            ],
        )
        .unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(root.path(), &s, "limit:3", "session-3", "old", &dir, now).unwrap();
        assert_eq!(read(root.path()).unwrap().len(), 2, "limit:1 left the file");
        register(root.path(), &s, "limit:1", "session-1", "old", &dir, now).unwrap();
        let events: Vec<_> = read(root.path())
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(events, ["limit:2", "limit:3"], "the replay stored nothing");
        finish(root.path(), "limit:1", false, now).unwrap();
        assert_eq!(
            claim(root.path(), "limit:1", "new", &s, &none, &none, &none, now).unwrap_err(),
            NOT_READY
        );
        assert!(
            finish(root.path(), "limit:9", false, now).is_err(),
            "an unknown event"
        );
    }
    fn now_seconds() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// Adversarial round: the 7-day edge. A stop exactly 7 days old is not older than a week
    /// and stays; a second more and it leaves; one 6.5 days old stays (a 6-day cut fails).
    #[test]
    fn the_week_is_seven_days_to_the_second() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        write(
            root.path(),
            &[
                stop(1, now - 7 * DAY - 1, State::Waiting, &dir),
                stop(2, now - 7 * DAY, State::Waiting, &dir),
                stop(3, now - 6 * DAY - DAY / 2, State::Waiting, &dir),
            ],
        )
        .unwrap();
        let s = snapshot();
        let none = HashSet::new();
        let answer = status(root.path(), &s, &none, &none, &none, now).unwrap();
        assert_eq!(answer["waiting_sessions"], 2, "{answer}");
        register(root.path(), &s, "limit:9", "session-9", "old", &dir, now).unwrap();
        let events: Vec<_> = read(root.path())
            .unwrap()
            .into_iter()
            .map(|r| r.event)
            .collect();
        assert_eq!(events, ["limit:2", "limit:3", "limit:9"]);
    }
    /// Adversarial round: a 1024-row file an older version wrote, through `register` (the
    /// other entry point): the file stays readable and keeps the new stop.
    #[test]
    fn a_full_file_stays_readable_after_register() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let rows: Vec<_> = (0..MAX_ROWS)
            .map(|n| stop(n, now - DAY + n as i64, State::Waiting, &dir))
            .collect();
        write(root.path(), &rows).unwrap();
        let s = snapshot();
        register(
            root.path(),
            &s,
            "limit:new",
            "session-new",
            "old",
            &dir,
            now,
        )
        .unwrap();
        let rows = read(root.path()).expect("still readable");
        assert_eq!(rows.len(), KEEP_ROWS);
        assert!(rows.iter().any(|r| r.event == "limit:new"));
        // The next hook is not refused either.
        register_stop(
            root.path(),
            &s,
            "session-x",
            "old",
            &dir,
            StopKind::Limit,
            now + 1,
        )
        .unwrap();
        assert_eq!(read(root.path()).unwrap().len(), KEEP_ROWS);
    }
    /// Adversarial round: hooks registering at once on a full file each keep their stop.
    #[test]
    fn concurrent_hooks_on_a_full_file_each_keep_their_stop() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let rows: Vec<_> = (0..MAX_ROWS)
            .map(|n| stop(n, now - DAY + n as i64, State::Resumed, &dir))
            .collect();
        write(root.path(), &rows).unwrap();
        let s = snapshot();
        let events: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|n| {
                    let (root, s, dir) = (root.path(), &s, &dir);
                    scope.spawn(move || {
                        match register_stop(
                            root,
                            s,
                            &format!("session-c{n}"),
                            "old",
                            dir,
                            StopKind::Limit,
                            now,
                        )
                        .unwrap()
                        {
                            Stopped::New(e) => e,
                            other => panic!("{other:?}"),
                        }
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.len(), KEEP_ROWS);
        for event in events {
            assert!(rows.iter().any(|r| r.event == event), "{event} kept");
        }
    }
    /// Adversarial round: the clock moved back an hour after 512 live stops were written. A
    /// new stop is still kept (D-4: "a new stop is always kept"), not answered `registered`
    /// and dropped in the same write.
    #[test]
    fn a_new_stop_is_kept_when_the_clock_moved_back() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let rows: Vec<_> = (0..KEEP_ROWS)
            .map(|n| stop(n, now + 3600 + n as i64, State::Waiting, &dir))
            .collect();
        write(root.path(), &rows).unwrap();
        let s = snapshot();
        let stopped = register_stop(
            root.path(),
            &s,
            "session-new",
            "old",
            &dir,
            StopKind::Limit,
            now,
        )
        .unwrap();
        let Stopped::New(event) = stopped else {
            panic!("{stopped:?}")
        };
        let rows = read(root.path()).unwrap();
        assert!(
            rows.iter().any(|r| r.event == event),
            "answered registered, so it is in the file"
        );
    }
    /// N1 round 3 (adversarial): recovery-pruned.json is 0600 once written; a garbage or oversized
    /// file never blocks a stop and is replaced by a valid list; a symlink in its place is not
    /// followed (the target is untouched) and the stop is still kept.
    #[test]
    fn the_pruned_list_is_private_and_never_blocks_a_stop() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let now = 30 * DAY;
        let s = snapshot();
        let old = |n| stop(n, now - 8 * DAY, State::Waiting, &dir);
        write(root.path(), &[old(1)]).unwrap();
        register(root.path(), &s, "limit:a", "session-a", "old", &dir, now).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(PRUNED_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "pruned list mode {mode:o}");
        }
        assert_eq!(pruned(root.path()), ["limit:1"]);
        // Garbage in the list.
        std::fs::write(root.path().join(PRUNED_FILE), b"{not json").unwrap();
        write(root.path(), &[old(2), stop(3, now, State::Waiting, &dir)]).unwrap();
        register(root.path(), &s, "limit:b", "session-b", "old", &dir, now).unwrap();
        assert!(read(root.path())
            .unwrap()
            .iter()
            .any(|r| r.event == "limit:b"));
        assert_eq!(
            pruned(root.path()),
            ["limit:2"],
            "rewritten as a valid list"
        );
        // Oversized list (over the 512 KiB read bound).
        std::fs::write(root.path().join(PRUNED_FILE), vec![b' '; 600 * 1024]).unwrap();
        write(root.path(), &[old(4)]).unwrap();
        register(root.path(), &s, "limit:c", "session-c", "old", &dir, now).unwrap();
        assert!(read(root.path())
            .unwrap()
            .iter()
            .any(|r| r.event == "limit:c"));
        // A symlink in its place.
        #[cfg(unix)]
        {
            let target = root.path().join("elsewhere.json");
            std::fs::write(&target, b"[]").unwrap();
            std::fs::remove_file(root.path().join(PRUNED_FILE)).unwrap();
            std::os::unix::fs::symlink(&target, root.path().join(PRUNED_FILE)).unwrap();
            write(root.path(), &[old(5)]).unwrap();
            register(root.path(), &s, "limit:d", "session-d", "old", &dir, now).unwrap();
            assert!(read(root.path())
                .unwrap()
                .iter()
                .any(|r| r.event == "limit:d"));
            assert_eq!(
                std::fs::read(&target).unwrap(),
                b"[]",
                "the link was not followed"
            );
        }
    }
    /// N1 round 3 (adversarial): the pruned list is bounded at 2048 ids, newest kept, and a replay
    /// of the newest pruned event still answers as a replay after the bound is passed.
    #[test]
    fn the_pruned_list_keeps_the_newest_2048() {
        let root = tempfile::tempdir().unwrap();
        let ids: Vec<String> = (0..MAX_PRUNED + 10)
            .map(|n| format!("limit:p{n}"))
            .collect();
        remember_pruned(root.path(), &ids[..MAX_PRUNED]);
        remember_pruned(root.path(), &ids[MAX_PRUNED..]);
        let list = pruned(root.path());
        assert_eq!(list.len(), MAX_PRUNED);
        assert!(was_pruned(root.path(), ids.last().unwrap()));
        assert!(!was_pruned(root.path(), &ids[0]), "the oldest left");
        // 2048 ids of the longest identifier still fit the 512 KiB read bound.
        let long: Vec<String> = (0..MAX_PRUNED).map(|n| format!("{n:0>160}")).collect();
        remember_pruned(root.path(), &long);
        assert!(
            was_pruned(root.path(), long.last().unwrap()),
            "a full list of long ids still reads"
        );
    }
    #[test]
    fn an_sb114_record_still_loads() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        let old = serde_json::json!([{"event": "limit:1", "session": "session-1", "account": "old",
            "provider": "claude", "pool": "default", "directory": dir, "stopped_at": 100,
            "state": "waiting", "selected_account": null}]);
        switchboard_core::private_fs::private_write(
            &root.path().join(FILE),
            &serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let rows = read(root.path()).unwrap();
        assert_eq!(
            (rows[0].kind, rows[0].attempt, rows[0].mode),
            (StopKind::Limit, 0, None)
        );
    }

    #[tokio::test]
    async fn the_hook_names_the_account_the_sessions_limit_was_charged_to() {
        use crate::{fixtures, Operation};
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), fixtures::signed_out, fixtures::fails).await;
        let old = fixtures::save(&runtime.store, "old", "default");
        let op = |billing| Operation::RecoveryHook {
            session: "s-1".into(),
            directory: root.path().into(),
            billing,
        };
        // Signed out and no limit charged to the session: nothing to name, nothing written.
        assert!(runtime.execute(op(false)).await.is_err());
        assert!(read(root.path()).unwrap().is_empty());
        runtime
            .limits
            .bind("s-1", std::slice::from_ref(&old.id), crate::monitor::now());
        let first = runtime.execute(op(false)).await.unwrap();
        assert_eq!(first["result"], "registered");
        assert_eq!(read(root.path()).unwrap()[0].account, old.id);
        let again = runtime.execute(op(true)).await.unwrap();
        assert_eq!(
            (again["result"].clone(), again["event"].clone()),
            ("again".into(), first["event"].clone())
        );
        let row = &read(root.path()).unwrap()[0];
        assert_eq!((row.attempt, row.kind), (1, StopKind::Billing));
    }

    #[tokio::test]
    async fn a_stop_in_a_project_folder_is_the_projects_account() {
        // 2026-10-18: 1,202 stops in project folders were refused in one day: the hook named the
        // ordinary CLI's account, which a project's folder never uses.
        use crate::{fixtures, Operation};
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), fixtures::signed_in, fixtures::fails).await;
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().canonicalize().unwrap();
        let mine = fixtures::save(&runtime.store, "synthetic-project", "default");
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![folder.clone()],
                account_ids: vec![mine.id.clone()],
                base_folders: None,
                organization: None,
            })
            .await
            .unwrap();
        let done = runtime
            .execute(Operation::RecoveryHook {
                session: "s-2".into(),
                directory: folder.join("src"),
                billing: false,
            })
            .await;
        // A subfolder that does not exist is not this case; the project folder itself is.
        assert!(done.is_err());
        let done = runtime
            .execute(Operation::RecoveryHook {
                session: "s-2".into(),
                directory: folder,
                billing: false,
            })
            .await
            .unwrap();
        assert_eq!(done["result"], "registered", "{done}");
        let row = &read(root.path()).unwrap()[0];
        assert_eq!(
            (row.account.as_str(), row.pool.as_str()),
            (mine.id.as_str(), "app")
        );
    }
}
