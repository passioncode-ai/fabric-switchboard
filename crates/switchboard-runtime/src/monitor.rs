//! Quota polling and renewal of inactive Claude tokens (PLAN-0.5 D-2). No inference retry;
//! the account signed in to the ordinary Claude Code is never refreshed here.
use crate::{external, Runtime};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use switchboard_core::{Account, AuthKind, Provider, Store, Usage};
use tokio::task::JoinHandle;

pub const INTERVAL_SECONDS: i64 = 180;
const MAX_BACKOFF: i64 = 1800;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs() as i64)
        .unwrap_or(0)
}

pub struct MonitorHandle(JoinHandle<()>);
impl MonitorHandle {
    pub fn start(runtime: &Arc<Runtime>, native_sources: bool) -> Self {
        let weak = Arc::downgrade(runtime);
        Self(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_source_sync = 0;
            let mut last_limit_scan = 0;
            // What the previous pass saw of Claude Swap: whether it ran, and its files.
            let mut swap_was_running: Option<bool> = None;
            let mut swap_last_signature = None;
            loop {
                interval.tick().await;
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                let time = now();
                if native_sources
                    && (time - last_source_sync >= INTERVAL_SECONDS || time < last_source_sync)
                {
                    // Network first, outside the lock: who owns a lineage not seen before.
                    crate::refresh::learn_live_owner(
                        &runtime.store,
                        runtime.native,
                        &runtime.refresh,
                    )
                    .await;
                    // Claude Swap's files are read outside the lock; adopting its newer
                    // generations happens under it, with the live sync.
                    let activity = crate::external::claude_swap_activity();
                    let running = activity.running;
                    runtime.refresh.set_swap_switching(activity.switching);
                    let signature = crate::external::claude_swap_signature();
                    let due = crate::refresh::swap_read_due(
                        running,
                        swap_was_running,
                        signature,
                        swap_last_signature,
                    );
                    let read = due.then(crate::external::read_claude_swap);
                    let swap = match (running, read) {
                        (true, Some(Ok(batch))) => {
                            crate::refresh::SwapView::Profiles(batch.profiles, batch.failed)
                        }
                        (true, _) => crate::refresh::SwapView::Unreadable,
                        (false, Some(Ok(batch))) => {
                            crate::refresh::SwapView::Stopped(batch.profiles)
                        }
                        (false, _) => crate::refresh::SwapView::NotRunning,
                    };
                    // A failed read (Swap was writing its files) is repeated next pass: neither
                    // the files nor the stop are marked as seen.
                    let failed = due
                        && matches!(
                            swap,
                            crate::refresh::SwapView::Unreadable
                                | crate::refresh::SwapView::NotRunning
                        );
                    if !failed {
                        swap_last_signature = signature;
                        swap_was_running = Some(running);
                    }
                    let _mutation = runtime.mutations.lock().await;
                    // Claude Code left its token expired: renew it under Claude Code's locks so
                    // managed sessions and quota checks on that account keep working.
                    crate::refresh::renew_idle_live(
                        &runtime.store,
                        runtime.native,
                        &runtime.refresh,
                    )
                    .await;
                    crate::refresh::follow_claude_swap(&runtime.store, &runtime.refresh, swap);
                    sync_live_sources(&runtime.store, runtime.native.current, &runtime.refresh);
                    runtime.invalidate_current();
                    last_source_sync = time;
                }
                if native_sources {
                    let _ = runtime.maybe_backup(time, false);
                }
                if time - last_limit_scan >= 30 || time < last_limit_scan {
                    scan_limits(&runtime, native_sources, time);
                    last_limit_scan = time;
                }
                let Ok(snapshot) = runtime.store.snapshot() else {
                    continue;
                };
                let mut due: Vec<_> = snapshot
                    .accounts
                    .into_iter()
                    .filter(|a| due(a, time))
                    .collect();
                // Oldest due first: a failing first row cannot starve another account.
                due.sort_by_key(|a| {
                    a.usage_health
                        .as_ref()
                        .map(|h| h.next_check_at)
                        .unwrap_or(0)
                });
                if native_sources {
                    renew_due(&runtime).await;
                }
                for account in due.into_iter().take(2) {
                    let _ = check(
                        &runtime.store,
                        runtime.native,
                        &runtime.refresh,
                        &account.id,
                        Some(&runtime.mutations),
                    )
                    .await;
                }
                if native_sources {
                    // A native switch files the live lineage under its account: know its owner
                    // first (network, outside the lock; at most once a minute per lineage).
                    crate::refresh::learn_live_owner(
                        &runtime.store,
                        runtime.native,
                        &runtime.refresh,
                    )
                    .await;
                }
                let _mutation = runtime.mutations.lock().await;
                let _ = rotate(&runtime, native_sources);
            }
        }))
    }
}
impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A schedule stamped ahead of a clock that was set back would otherwise stall probes.
fn due(a: &Account, time: i64) -> bool {
    a.enabled
        && a.kind == AuthKind::OAuth
        && a.usage_health
            .as_ref()
            .is_none_or(|h| h.next_check_at <= time || h.checked_at > time + 60)
}

fn backoff(previous: Option<(i64, i64)>) -> i64 {
    previous
        .map(|(checked, next)| {
            (next - checked)
                .max(INTERVAL_SECONDS)
                .saturating_mul(2)
                .min(MAX_BACKOFF)
        })
        .unwrap_or(INTERVAL_SECONDS)
}

/// A usage check the provider answered 429 waits at least this long without `Retry-After`
/// (Claude Swap waits 900 s too), and at most `MAX_RETRY_AFTER` with one.
const RATE_LIMIT_FLOOR: i64 = 900;
const MAX_RETRY_AFTER: i64 = 6 * 3600;
/// When the next check of a failed account is due. A provider 429 is honoured: its `Retry-After`
/// (bounded) or the floor, never sooner than the ordinary backoff.
fn failure_delay(previous: Option<(i64, i64)>, rate_limited: Option<Option<i64>>) -> i64 {
    let ordinary = backoff(previous);
    match rate_limited {
        None => ordinary,
        Some(wait) => ordinary.max(wait.unwrap_or(RATE_LIMIT_FLOOR).min(MAX_RETRY_AFTER)),
    }
}

pub async fn probe(store: Arc<Store>, id: &str) -> Result<Usage, String> {
    let account = store
        .snapshot()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    let started = now();
    if account.kind != AuthKind::OAuth {
        store.usage_health(id, "unavailable", started, started + MAX_BACKOFF)?;
        return Err("Usage is available for OAuth profiles only.".into());
    }
    let generation = match store.stored_credential(id) {
        Ok(generation) => generation,
        Err(_) => {
            // Unreadable vault rows must also move out of the due queue;
            // otherwise the first two broken rows starve every later account.
            store.usage_health(id, "failed", started, started + INTERVAL_SECONDS)?;
            return Err(
                "Usage unavailable. Check sign-in or try again after the next scheduled check."
                    .into(),
            );
        }
    };
    let result = switchboard_proxy::probe_usage_detailed(store.clone(), id.to_owned()).await;
    let checked = now();
    match result {
        Ok(usage) => Ok(usage),
        Err(failure) => {
            let delay = failure_delay(
                account
                    .usage_health
                    .filter(|h| h.status != "ok")
                    .map(|h| (h.checked_at, h.next_check_at)),
                failure.rate_limited,
            );
            store.usage_health_credential(id, &generation, "failed", checked, checked + delay)?;
            Err(
                if failure.message == switchboard_proxy::REJECTED
                    || failure.message == switchboard_proxy::USAGE_RATE_LIMITED
                {
                    failure.message
                } else {
                    "Usage unavailable. Check sign-in or try again after the next scheduled check."
                        .into()
                },
            )
        }
    }
}

/// A quota check that first keeps an inactive Claude account's token alive and retries once
/// after a refresh when the provider rejects the token (Claude Swap's shape, PLAN-0.5 D-2).
/// `lock` is the owner's mutation lock when the caller does not already hold it.
pub(crate) async fn check(
    store: &Arc<Store>,
    native: crate::NativeSources,
    refresh: &crate::refresh::RefreshState,
    id: &str,
    lock: Option<&tokio::sync::Mutex<()>>,
) -> Result<Usage, String> {
    let refreshed = |force: bool| async move {
        let _guard = match lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
        crate::refresh::ensure_fresh(store, native, refresh, id, force).await
    };
    // A dead lineage leaves the due queue for the long backoff; otherwise it would sort
    // first on every tick and starve every other account's check and renewal.
    let dead = || {
        let time = now();
        let _ = store.usage_health(id, "failed", time, time + MAX_BACKOFF);
        Err(SIGN_IN.to_string())
    };
    let sequence = async {
        if refreshed(false).await == crate::refresh::Outcome::SignInRequired {
            return dead();
        }
        match probe(store.clone(), id).await {
            Err(error) if error == switchboard_proxy::REJECTED => match refreshed(true).await {
                crate::refresh::Outcome::Refreshed => probe(store.clone(), id).await,
                crate::refresh::Outcome::SignInRequired => dead(),
                _ => Err(error),
            },
            result => result,
        }
    };
    // One deadline below the control channel's 30 seconds, so a CLI or MCP caller never
    // times out while the owner keeps working on its behalf.
    tokio::time::timeout(CHECK_DEADLINE, sequence)
        .await
        .unwrap_or_else(|_| {
            Err(
                "Usage unavailable. Check sign-in or try again after the next scheduled check."
                    .into(),
            )
        })
}
use crate::refresh::SIGN_IN;
const CHECK_DEADLINE: Duration = Duration::from_secs(25);

/// Renews inactive Claude tokens on their own schedule. Quota checks back off up to 30
/// minutes, longer than the renewal margin, so a managed route could otherwise expire
/// between checks. At most two grants per pass.
async fn renew_due(runtime: &Runtime) {
    let Ok(snapshot) = runtime.store.snapshot() else {
        return;
    };
    let time = now();
    let mut done = 0;
    for account in snapshot
        .accounts
        .iter()
        .filter(|a| a.enabled && a.provider == Provider::Claude && a.kind == AuthKind::OAuth)
    {
        if done == 2 {
            break;
        }
        let due = runtime
            .store
            .stored_credential(&account.id)
            .is_ok_and(|c| crate::refresh::due(&c, time));
        if !due {
            continue;
        }
        let _mutation = runtime.mutations.lock().await;
        if crate::refresh::ensure_fresh(
            &runtime.store,
            runtime.native,
            &runtime.refresh,
            &account.id,
            false,
        )
        .await
            == crate::refresh::Outcome::Refreshed
        {
            done += 1;
        }
    }
}

/// Adopt only a profile already captured by the operator. The ordinary client
/// owns refresh; a new login never silently adds or enables an account.
fn sync_live_sources(
    store: &Store,
    current: fn(Provider) -> Result<external::CapturedProfile, String>,
    refresh: &crate::refresh::RefreshState,
) {
    let Ok(snapshot) = store.snapshot() else {
        return;
    };
    for provider in [Provider::Claude, Provider::Codex] {
        let accounts: Vec<_> = snapshot
            .accounts
            .iter()
            .filter(|a| {
                a.provider == provider && a.kind == AuthKind::OAuth && a.external_identity.is_some()
            })
            .collect();
        if accounts.is_empty() {
            continue;
        }
        let Ok(profile) = current(provider) else {
            continue;
        };
        if provider == Provider::Claude {
            refresh.note_active(&profile.identity);
            // Only a lineage proven to be this identity's is filed under it: after an
            // interrupted switch or a half-finished `/login` the config can name account A
            // while the Keychain holds account B's token (PLAN-0.5.1, report §P1-3).
            if crate::refresh::lineage(store, refresh, &profile.identity, &profile.credential)
                != crate::refresh::Lineage::Own
            {
                continue;
            }
        }
        for account in accounts {
            if store
                .match_external(provider, &account.pool, &profile.identity)
                .ok()
                .flatten()
                .is_none_or(|matched| matched.id != account.id)
            {
                continue;
            }
            let Ok(old) = store.stored_credential(&account.id) else {
                continue;
            };
            // A copy already newer than Claude Code's item (a renewal Switchboard stored while
            // writing it back failed, or Claude Swap's) is never moved back.
            if old.refresh_token != profile.credential.refresh_token
                && old.expires_at.unwrap_or(0) > profile.credential.expires_at.unwrap_or(0)
            {
                continue;
            }
            if old.access_token == profile.credential.access_token
                && old.refresh_token == profile.credential.refresh_token
                && old.id_token == profile.credential.id_token
                && old.expires_at == profile.credential.expires_at
                && old.native_context == profile.credential.native_context
            {
                continue;
            }
            let _ = store.upsert(
                account.label.clone(),
                provider,
                account.kind,
                account.pool.clone(),
                profile.credential.clone(),
                Some(profile.identity.clone()),
            );
        }
    }
}

/// Collects limit errors: the proxy's own events always; Claude Code's transcript markers only
/// for a real owner, which is the only one allowed to look at the ordinary Claude sign-in.
fn scan_limits(runtime: &Runtime, native_sources: bool, time: i64) {
    let Ok(snapshot) = runtime.store.snapshot() else {
        return;
    };
    let profile = native_sources
        .then(|| (runtime.native.current)(Provider::Claude).ok())
        .flatten();
    let native = profile.as_ref().map(|p| {
        let ids = crate::matching_accounts(&runtime.store, Provider::Claude, &p.identity)
            .map(|accounts| accounts.into_iter().map(|a| a.id).collect())
            .unwrap_or_default();
        (&p.identity, ids)
    });
    let files = if native.is_some() {
        crate::limits::transcript_files(time)
    } else {
        vec![]
    };
    runtime.limits.refresh(&snapshot, native, &files, time);
}

fn rotate(runtime: &Runtime, native_sources: bool) -> Result<(), String> {
    let mut decisions = Vec::new();
    let snapshot = runtime.store.snapshot()?;
    for policy in snapshot.policies.iter().filter(|p| p.enabled) {
        // Claude Swap switches Claude Code by itself: two automatic switchers would undo each
        // other's choice. Manual switches and managed routes are unaffected.
        if policy.target == "claude_cli" && runtime.refresh.swap_switching() {
            decisions.push(json!({"provider":policy.provider,"pool":policy.pool,"target":policy.target,"reason":"claude_swap_switching","candidate_id":null}));
            continue;
        }
        let current = if policy.target == "managed" {
            snapshot
                .routes
                .get(&format!("{}:{}", policy.provider.as_str(), policy.pool))
                .cloned()
        } else if native_sources {
            (runtime.native.current)(Provider::Claude)
                .ok()
                .and_then(|profile| {
                    runtime
                        .store
                        .match_external(Provider::Claude, &policy.pool, &profile.identity)
                        .ok()
                        .flatten()
                })
                .map(|a| a.id)
        } else {
            None
        };
        // A rejected sign-in is never a candidate: activation would refuse it every pass and
        // the next eligible account would never be tried.
        let mut excluded = runtime.limits.limited_ids(now());
        excluded.extend(
            runtime
                .refresh
                .sign_in_required(&runtime.store)
                .into_iter()
                .filter(|id| current.as_deref() != Some(id.as_str())),
        );
        let decision =
            runtime
                .store
                .rotation_decision_with(policy, current.as_deref(), now(), &excluded)?;
        let mut reason = decision.reason;
        if let Some(id) = decision.candidate_id.as_deref() {
            let result = if policy.target == "managed" {
                runtime
                    .store
                    .select_with_cooldown(policy.provider, &policy.pool, id, now())
            } else if native_sources {
                crate::activate_native(
                    &runtime.store,
                    id,
                    current.as_deref(),
                    runtime.native,
                    Some(&runtime.refresh),
                )
            } else {
                Err("Native account source unavailable.".into())
            };
            if result.is_ok() {
                runtime.invalidate_current();
                if policy.target == "claude_cli" {
                    let identity = snapshot
                        .accounts
                        .iter()
                        .find(|a| a.id == id)
                        .and_then(|a| a.external_identity.as_ref());
                    runtime.limits.switched(identity, now());
                }
                reason = if reason.starts_with("limit") {
                    "switched_on_limit"
                } else {
                    "switched"
                }
                .into();
            } else if policy.target == "claude_cli" {
                reason = "activation_failed".into();
            } else {
                reason = "switch_failed".into();
            }
            // Journal after the outcome is fixed; a write failure cannot undo a switch.
            let _ = runtime.store.record(
                "rotation",
                Some(id),
                if result.is_ok() { "switched" } else { "failed" },
            );
        }
        decisions.push(json!({"provider":policy.provider,"pool":policy.pool,"target":policy.target,"reason":reason,"candidate_id":decision.candidate_id}));
    }
    *runtime
        .monitor_decisions
        .lock()
        .map_err(|_| "Monitor state unavailable.")? = decisions;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Credential, MemoryVault};
    #[test]
    fn schedule_written_before_the_clock_moved_back_is_due_now() {
        let account = |checked_at: i64, next_check_at: i64| switchboard_core::Account {
            id: uuid::Uuid::new_v4().to_string(),
            label: "Fixture".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: Some(switchboard_core::UsageHealth {
                status: "ok".into(),
                checked_at,
                next_check_at,
            }),
        };
        let time = now();
        assert!(!due(&account(time, time + 180), time));
        assert!(due(&account(time - 200, time - 20), time));
        assert!(due(&account(time + 3600, time + 3780), time));
    }
    fn quota(store: &Store, id: &str, used: f64) {
        let time = now();
        store
            .observe(
                id,
                Usage {
                    used_percent: used,
                    observed_at: time,
                    resets_at: None,
                    source: "provider".into(),
                    windows: vec![],
                },
            )
            .unwrap();
    }
    fn policy(target: &str) -> switchboard_core::RotationPolicy {
        switchboard_core::RotationPolicy {
            provider: Provider::Claude,
            pool: "default".into(),
            target: target.into(),
            enabled: true,
            threshold_percent: 90.0,
            hysteresis_percent: 10.0,
            cooldown_seconds: 1800,
            max_age_seconds: 300,
            last_switched_at: None,
        }
    }
    fn reason(runtime: &Runtime) -> String {
        runtime.monitor_decisions.lock().unwrap()[0]["reason"]
            .as_str()
            .unwrap()
            .into()
    }
    #[tokio::test]
    async fn automatic_managed_switch_is_journaled_as_rotation() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        runtime
            .store
            .select(Provider::Claude, "default", &a.id)
            .unwrap();
        runtime.store.set_policy(policy("managed")).unwrap();
        rotate(&runtime, false).unwrap();
        assert_eq!(reason(&runtime), "switched");
        assert_eq!(
            events(&runtime.store, "rotation"),
            [(Some(b.id), "switched".to_string())]
        );
    }
    #[tokio::test]
    async fn failed_managed_switch_is_not_reported_as_native_activation() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        runtime
            .store
            .select(Provider::Claude, "default", &a.id)
            .unwrap();
        runtime.store.set_policy(policy("managed")).unwrap();
        // Metadata publication fails: the selection cannot be written.
        let metadata = root.path().join("accounts.json");
        std::fs::remove_file(&metadata).unwrap();
        std::fs::create_dir(&metadata).unwrap();
        rotate(&runtime, false).unwrap();
        assert_eq!(reason(&runtime), "switch_failed");
        assert_eq!(
            runtime.store.snapshot().unwrap().routes["claude:default"],
            a.id
        );
    }
    #[tokio::test]
    async fn automatic_native_switch_journals_activation_and_rotation() {
        use crate::fixtures::*;
        for (activate, expected) in [
            (activates as crate::Activate, "switched"),
            (fails, "activation_failed"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let runtime = crate::fixtures::runtime(root.path(), signed_in, activate).await;
            let a = save(&runtime.store, "synthetic-a", "default");
            let b = save(&runtime.store, "synthetic-b", "default");
            quota(&runtime.store, &a.id, 95.0);
            quota(&runtime.store, &b.id, 10.0);
            runtime.store.set_policy(policy("claude_cli")).unwrap();
            rotate(&runtime, true).unwrap();
            assert_eq!(reason(&runtime), expected);
            let detail = if expected == "switched" {
                ("completed", "switched")
            } else {
                ("failed", "failed")
            };
            assert_eq!(
                events(&runtime.store, "activation"),
                [(Some(b.id.clone()), detail.0.to_string())]
            );
            assert_eq!(
                events(&runtime.store, "rotation"),
                [(Some(b.id), detail.1.to_string())]
            );
        }
    }
    #[tokio::test]
    async fn native_rotation_leaves_switching_to_an_auto_switching_claude_swap() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        runtime.refresh.set_swap_switching(true);
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "claude_swap_switching");
        assert!(
            events(&runtime.store, "activation").is_empty(),
            "nothing switched"
        );
        // Swap stops switching (or quits): Switchboard's rotation runs again.
        runtime.refresh.set_swap_switching(false);
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "switched");
    }
    #[tokio::test]
    async fn rotation_skips_a_rejected_sign_in_for_the_next_eligible_account() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 5.0);
        quota(&runtime.store, &c.id, 20.0);
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        // b looks freest but its lineage was rejected.
        runtime.refresh.mark_dead_for_test(&runtime.store, &b.id);
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "switched");
        assert_eq!(
            events(&runtime.store, "activation"),
            [(Some(c.id), "completed".to_string())]
        );
    }
    #[tokio::test]
    async fn native_rotation_switches_to_an_expired_but_renewable_account() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let mut stale = credential("synthetic-b");
        stale.expires_at = Some(1_000);
        let b = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                stale,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "switched");
        // A managed route presents the access token itself, so it stays ineligible there.
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let mut stale = credential("synthetic-b");
        stale.expires_at = Some(1_000);
        let b = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                stale,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        runtime
            .store
            .select(Provider::Claude, "default", &a.id)
            .unwrap();
        runtime.store.set_policy(policy("managed")).unwrap();
        rotate(&runtime, false).unwrap();
        assert_eq!(reason(&runtime), "no_eligible_account");
    }
    #[tokio::test]
    async fn a_rejected_lineage_reports_sign_in_without_probing() {
        use crate::fixtures::*;
        use axum::{routing::post, Router};
        let app = Router::new().route(
            "/token",
            post(|| async {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    axum::Json(json!({"error":"invalid_grant"})),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let mut stale = credential("synthetic-b");
        stale.expires_at = Some(1_000);
        stale.refresh_token = Some("synthetic-b-refresh".into());
        let b = store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                stale,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let state = crate::refresh::RefreshState::at(url);
        let native = crate::NativeSources {
            current: signed_in,
            activate: activates,
            live: crate::no_live,
            swap: crate::no_swap,
        };
        let lock = tokio::sync::Mutex::new(());
        assert_eq!(
            check(&store, native, &state, &b.id, Some(&lock))
                .await
                .unwrap_err(),
            SIGN_IN
        );
        assert_eq!(state.sign_in_required(&store), vec![b.id.clone()]);
        // It leaves the due queue for the long backoff instead of starving other accounts.
        let account = store.snapshot().unwrap().accounts.remove(0);
        let health = account.usage_health.clone().unwrap();
        assert_eq!(health.status, "failed");
        assert!(health.next_check_at >= now() + MAX_BACKOFF - 5);
        assert!(!due(&account, now()));
    }
    #[tokio::test]
    async fn a_token_expiring_between_backed_off_checks_is_renewed() {
        use crate::fixtures::*;
        use axum::{routing::post, Router};
        let app = Router::new().route(
            "/token",
            post(|| async { axum::Json(json!({"access_token":"renewed","expires_in":28800})) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        runtime.refresh.set_endpoint(url);
        let mut soon = credential("synthetic-b");
        soon.expires_at = Some(now() + 300);
        soon.refresh_token = Some("synthetic-b-refresh".into());
        let b = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                soon,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        // Its quota check is backed off for half an hour, past the token's expiry.
        runtime
            .store
            .usage_health(&b.id, "failed", now(), now() + MAX_BACKOFF)
            .unwrap();
        renew_due(&runtime).await;
        assert_eq!(
            runtime.store.stored_credential(&b.id).unwrap().access_token,
            "renewed"
        );
    }
    #[tokio::test]
    async fn a_limit_error_in_claude_code_switches_with_quota_to_spare() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        quota(&runtime.store, &a.id, 20.0);
        quota(&runtime.store, &b.id, 5.0);
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "below_threshold", "no error, no move");
        // Claude Code wrote a spend-limit marker a minute ago for the account in use.
        let projects = root.path().join("claude").join("projects").join("-p");
        std::fs::create_dir_all(&projects).unwrap();
        let at = time::OffsetDateTime::from_unix_timestamp(now() - 60)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let line = json!({"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","apiErrorStatus":429,"timestamp":at,"quotaLimits":{"resetsAt":now()+3600}});
        let file = projects.join("s.jsonl");
        std::fs::write(&file, format!("{line}\n")).unwrap();
        let profile = signed_in(Provider::Claude).unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        runtime.limits.refresh(
            &snapshot,
            Some((&profile.identity, vec![a.id.clone()])),
            &[file],
            now(),
        );
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "switched_on_limit");
        assert_eq!(
            events(&runtime.store, "activation"),
            [(Some(b.id), "completed".to_string())]
        );
    }
    #[tokio::test]
    async fn background_sync_never_files_another_accounts_lineage() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let a = save(&store, "synthetic-a", "default");
        save(&store, "synthetic-b", "default");
        let state = crate::refresh::RefreshState::default();
        fn a_named_b_held(_: Provider) -> Result<external::CapturedProfile, String> {
            Ok(external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential: credential("synthetic-b"),
                identity: identity("synthetic-a"),
                label: "synthetic-a".into(),
            })
        }
        sync_live_sources(&store, a_named_b_held, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-token"
        );
        // Its own newer generation is adopted.
        fn a_refreshed(_: Provider) -> Result<external::CapturedProfile, String> {
            let mut c = credential("synthetic-a");
            c.access_token = "synthetic-a-newer".into();
            Ok(external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential: c,
                identity: identity("synthetic-a"),
                label: "synthetic-a".into(),
            })
        }
        sync_live_sources(&store, a_refreshed, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-newer"
        );
        // Claude Code rotated the lineage: the new refresh token is stored nowhere, so only the
        // provider's answer decides whose it is.
        fn a_rotated(_: Provider) -> Result<external::CapturedProfile, String> {
            let mut c = credential("synthetic-a");
            c.access_token = "rotated-access".into();
            c.refresh_token = Some("rotated-refresh".into());
            Ok(external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential: c,
                identity: identity("synthetic-a"),
                label: "synthetic-a".into(),
            })
        }
        sync_live_sources(&store, a_rotated, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-newer",
            "unattributed: not filed"
        );
        state.set_owner("rotated-refresh", identity("synthetic-b"));
        sync_live_sources(&store, a_rotated, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-newer",
            "the provider says it is synthetic-b's"
        );
        state.set_owner("rotated-refresh", identity("synthetic-a"));
        sync_live_sources(&store, a_rotated, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "rotated-access"
        );
    }
    #[test]
    fn background_sync_never_moves_a_newer_copy_back() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        // Switchboard holds a renewal Claude Code's item never received.
        let mut newer = credential("synthetic-a");
        newer.access_token = "renewed".into();
        newer.refresh_token = Some("renewed-r".into());
        newer.expires_at = newer.expires_at.map(|t| t + 3600);
        let a = store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                newer,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let state = crate::refresh::RefreshState::default();
        state.set_owner("synthetic-a-refresh", identity("synthetic-a"));
        sync_live_sources(&store, signed_in, &state);
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "renewed"
        );
    }
    #[test]
    fn a_rate_limited_usage_check_waits_as_the_provider_asks() {
        // No 429: the ordinary backoff.
        assert_eq!(failure_delay(None, None), 180);
        // 429 without Retry-After: the 900 s floor.
        assert_eq!(failure_delay(None, Some(None)), 900);
        // Retry-After longer than the backoff is honoured, bounded at six hours.
        assert_eq!(failure_delay(None, Some(Some(2400))), 2400);
        assert_eq!(failure_delay(None, Some(Some(999_999))), 6 * 3600);
        // A short Retry-After never makes the check sooner than the backoff already is.
        assert_eq!(failure_delay(Some((100, 1000)), Some(Some(5))), 1800);
    }
    #[test]
    fn backoff_is_bounded_and_clock_independent() {
        assert_eq!(backoff(None), 180);
        assert_eq!(backoff(Some((100, 280))), 360);
        assert_eq!(backoff(Some((100, 1900))), 1800);
        assert_eq!(backoff(Some((300, 100))), 360);
    }
    #[tokio::test]
    async fn unsupported_quota_is_recorded_without_network() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let account = store
            .add(
                "Fixture".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "synthetic-api-key").unwrap(),
            )
            .unwrap();
        assert!(probe(store.clone(), &account.id).await.is_err());
        let health = store
            .snapshot()
            .unwrap()
            .accounts
            .remove(0)
            .usage_health
            .unwrap();
        assert_eq!(health.status, "unavailable");
        assert_eq!(health.next_check_at - health.checked_at, 1800);
    }
    #[tokio::test]
    async fn unreadable_credentials_do_not_stay_at_the_front_of_the_due_queue() {
        use switchboard_core::Vault;
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryVault::default());
        let store = Arc::new(Store::open(root.path().to_owned(), vault.clone()).unwrap());
        let raw = json!({"claudeAiOauth":{"accessToken":"synthetic-missing","expiresAt":(now()+3600)*1000}}).to_string();
        let account = store
            .add(
                "Missing".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential::parse(Provider::Claude, AuthKind::OAuth, &raw).unwrap(),
            )
            .unwrap();
        vault.delete(&account.id).unwrap();
        assert!(probe(store.clone(), &account.id).await.is_err());
        let health = store
            .snapshot()
            .unwrap()
            .accounts
            .remove(0)
            .usage_health
            .unwrap();
        assert_eq!(health.status, "failed");
        assert!(health.next_check_at > now());
    }
    #[tokio::test]
    async fn monitor_drops_without_retaining_the_runtime() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let weak = Arc::downgrade(&runtime);
        let monitor = MonitorHandle::start(&runtime, false);
        tokio::task::yield_now().await;
        drop(monitor);
        drop(runtime);
        tokio::task::yield_now().await;
        assert!(weak.upgrade().is_none());
    }
    #[tokio::test]
    async fn automatic_selection_yields_to_manual_cooldown() {
        use switchboard_core::{RotationPolicy, Usage};
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let time = now();
        let mut ids = Vec::new();
        for (name, used) in [("A", 95.0), ("B", 10.0)] {
            let raw = json!({"claudeAiOauth":{"accessToken":format!("synthetic-{name}"),"expiresAt":(time+3600)*1000}}).to_string();
            let account = runtime
                .store
                .add(
                    name.into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    Credential::parse(Provider::Claude, AuthKind::OAuth, &raw).unwrap(),
                )
                .unwrap();
            runtime
                .store
                .observe(
                    &account.id,
                    Usage {
                        used_percent: used,
                        observed_at: time,
                        resets_at: None,
                        source: "provider".into(),
                        windows: vec![],
                    },
                )
                .unwrap();
            runtime
                .store
                .usage_health(&account.id, "ok", time, time + 180)
                .unwrap();
            ids.push(account.id);
        }
        runtime
            .store
            .select(Provider::Claude, "default", &ids[0])
            .unwrap();
        runtime
            .store
            .set_policy(RotationPolicy {
                provider: Provider::Claude,
                pool: "default".into(),
                target: "managed".into(),
                enabled: true,
                threshold_percent: 90.0,
                hysteresis_percent: 10.0,
                cooldown_seconds: 1800,
                max_age_seconds: 300,
                last_switched_at: None,
            })
            .unwrap();
        rotate(&runtime, false).unwrap();
        assert_eq!(
            runtime.store.snapshot().unwrap().routes["claude:default"],
            ids[1]
        );
        runtime
            .execute(crate::Operation::Select {
                provider: Provider::Claude,
                pool: "default".into(),
                id: ids[0].clone(),
            })
            .await
            .unwrap();
        rotate(&runtime, false).unwrap();
        assert_eq!(
            runtime.store.snapshot().unwrap().routes["claude:default"],
            ids[0]
        );
        assert_eq!(
            runtime.monitor_decisions.lock().unwrap()[0]["reason"],
            "cooldown"
        );
    }
}
