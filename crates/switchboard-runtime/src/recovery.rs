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
const MAX_ROWS: usize = 1024;
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
    let mut rows = read(root)?;
    if let Some(old) = rows.iter().find(|r| r.event == event) {
        return if old.session == session && old.account == account && old.directory == directory {
            Ok(())
        } else {
            Err(INVALID.into())
        };
    }
    if rows
        .iter()
        .any(|r| r.session == session && matches!(r.state, State::Waiting | State::Claimed))
    {
        return Err(INVALID.into());
    }
    let row = new_row(&rows, snapshot, event, session, account, directory, now)?;
    rows.push(row);
    write(root, &rows)
}

fn new_row(
    rows: &[Wait],
    snapshot: &Snapshot,
    event: &str,
    session: &str,
    account: &str,
    directory: PathBuf,
    now: i64,
) -> Result<Wait, String> {
    if rows.len() >= MAX_ROWS {
        return Err(INVALID.into());
    }
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
    let mut rows = read(root)?;
    if let Some(live) = rows
        .iter_mut()
        .find(|r| r.session == session && r.state == State::Waiting)
    {
        live.stopped_at = now;
        live.attempt = live.attempt.saturating_add(1);
        live.kind = kind;
        let event = live.event.clone();
        write(root, &rows)?;
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
    let mut row = new_row(&rows, snapshot, &event, session, account, directory, now)?;
    row.kind = kind;
    row.attempt = attempt;
    rows.push(row);
    write(root, &rows)?;
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
    let mut rows = read(root)?;
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
    write(root, &rows)?;
    Ok(result)
}

pub(crate) fn finish(root: &Path, event: &str, resumed: bool) -> Result<(), String> {
    let mut rows = read(root)?;
    let row = rows.iter_mut().find(|r| r.event == event).ok_or(INVALID)?;
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
    write(root, &rows)
}

pub(crate) fn status(
    root: &Path,
    snapshot: &Snapshot,
    limited: &HashSet<String>,
    capped: &HashSet<String>,
    sign_in: &HashSet<String>,
    now: i64,
) -> Result<serde_json::Value, String> {
    let rows = read(root)?;
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
        assert!(finish(root.path(), "limit:1", true).is_err());
        let claimed = claim(root.path(), "limit:1", "new", &s, &none, &none, &none, 102).unwrap();
        assert_eq!(claimed.state, State::Claimed);
        assert!(claim(root.path(), "limit:1", "new", &s, &none, &none, &none, 102).is_err());
        assert_eq!(read(root.path()).unwrap()[0].state, State::Claimed);
        finish(root.path(), "limit:1", true).unwrap();
        finish(root.path(), "limit:1", true).unwrap();
        assert!(finish(root.path(), "limit:1", false).is_err());
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
        finish(root.path(), "limit:1", false).unwrap();
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
        register(root.path(), &s, "limit:1", "session-1", "old", root.path(), 100).unwrap();
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
        assert_eq!((fresh.mode, fresh.accounts), (Some(Mode::Capacity), vec!["new".into()]));
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
            root.path(), &s, "session-1", "old", root.path(), StopKind::Billing, 100,
        )
        .unwrap();
        let Stopped::New(event) = stopped else { panic!("a first stop is new") };
        let wait = &read(root.path()).unwrap()[0];
        assert_eq!(readiness(wait, &s, &held(), &none, &none, 100_000), Ready::default());
        assert!(claim(root.path(), &event, "new", &s, &held(), &none, &none, 100_000).is_err());
        // A limit stop in a pool whose every account is spend-capped is not probed either.
        register(root.path(), &s, "limit:2", "session-2", "old", root.path(), 100).unwrap();
        let wait = read(root.path()).unwrap().into_iter().find(|w| w.event == "limit:2").unwrap();
        assert_eq!(readiness(&wait, &s, &held(), &held(), &none, 100_000), Ready::default());
        let one_capped = readiness(&wait, &s, &held(), &HashSet::from(["old".into()]), &none, 100_000);
        assert_eq!(one_capped.accounts, vec!["new"]);
    }

    #[test]
    fn a_probe_claim_is_checked_by_the_same_rule_and_taken_once() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        register(root.path(), &s, "limit:1", "session-1", "old", root.path(), 100).unwrap();
        assert!(claim(root.path(), "limit:1", "new", &s, &held(), &none, &none, 219).is_err());
        let status = status(root.path(), &s, &held(), &none, &none, 220).unwrap();
        assert_eq!(status["ready_sessions"], 1);
        assert_eq!(status["sessions"][0]["mode"], "probe");
        let claimed = claim(root.path(), "limit:1", "new", &s, &held(), &none, &none, 220).unwrap();
        assert_eq!((claimed.state, claimed.mode), (State::Claimed, Some(Mode::Probe)));
        assert!(claim(root.path(), "limit:1", "old", &s, &held(), &none, &none, 221).is_err());
        assert_eq!(read(root.path()).unwrap()[0].mode, Some(Mode::Probe));
        // A signed-out account is not a probe target.
        let root = tempfile::tempdir().unwrap();
        register(root.path(), &s, "limit:1", "session-1", "old", root.path(), 100).unwrap();
        let sign_in = HashSet::from(["new".into()]);
        assert!(claim(root.path(), "limit:1", "new", &s, &held(), &none, &sign_in, 220).is_err());
    }

    #[test]
    fn repeated_stops_of_one_session_raise_the_attempt() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let none = HashSet::new();
        let at = |t| register_stop(root.path(), &s, "session-1", "old", root.path(), StopKind::Limit, t);
        let Stopped::New(first) = at(100).unwrap() else { panic!() };
        // Stopping again while it waits keeps the event, one attempt later, from the new time.
        assert_eq!(at(150).unwrap(), Stopped::Again(first.clone()));
        let rows = read(root.path()).unwrap();
        assert_eq!((rows.len(), rows[0].attempt, rows[0].stopped_at), (1, 1, 150));
        claim(root.path(), &first, "new", &s, &held(), &none, &none, 450).unwrap();
        // A stop after the claim: the session ran, so the claim is closed as resumed.
        let Stopped::New(second) = at(500).unwrap() else { panic!() };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows[0].state, State::Resumed);
        let row = rows.iter().find(|r| r.event == second).unwrap();
        assert_eq!((row.attempt, row.state.clone()), (2, State::Waiting));
        finish(root.path(), &second, false).unwrap();
        // Cancelled work does not count; a resumed stop within 30 minutes does.
        let Stopped::New(third) = at(600).unwrap() else { panic!() };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.iter().find(|r| r.event == third).unwrap().attempt, 2);
        finish(root.path(), &third, false).unwrap();
        let Stopped::New(fourth) = at(150 + REPEAT_WINDOW + 1).unwrap() else { panic!() };
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.iter().find(|r| r.event == fourth).unwrap().attempt, 0);
        // An unknown session id shape or account registers nothing.
        assert!(register_stop(root.path(), &s, "bad id", "old", root.path(), StopKind::Limit, 700).is_err());
        assert!(register_stop(root.path(), &s, "session-2", "missing", root.path(), StopKind::Limit, 700).is_err());
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
        assert_eq!((rows[0].kind, rows[0].attempt, rows[0].mode), (StopKind::Limit, 0, None));
    }
}
