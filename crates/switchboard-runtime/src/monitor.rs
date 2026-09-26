//! Quota-only polling. No inference retry and no competing OAuth refresh grant.
use crate::{external, Runtime};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use switchboard_core::{AuthKind, Provider, Store, Usage};
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
            loop {
                interval.tick().await;
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                let time = now();
                if native_sources
                    && (time - last_source_sync >= INTERVAL_SECONDS || time < last_source_sync)
                {
                    let _mutation = runtime.mutations.lock().await;
                    sync_live_sources(&runtime.store);
                    runtime.invalidate_current();
                    last_source_sync = time;
                }
                let Ok(snapshot) = runtime.store.snapshot() else {
                    continue;
                };
                let mut due: Vec<_> = snapshot
                    .accounts
                    .into_iter()
                    .filter(|a| {
                        a.enabled
                            && a.kind == AuthKind::OAuth
                            && a.usage_health
                                .as_ref()
                                .is_none_or(|h| h.next_check_at <= time)
                    })
                    .collect();
                // Oldest due first: a failing first row cannot starve another account.
                due.sort_by_key(|a| {
                    a.usage_health
                        .as_ref()
                        .map(|h| h.next_check_at)
                        .unwrap_or(0)
                });
                for account in due.into_iter().take(2) {
                    let _ = probe(runtime.store.clone(), &account.id).await;
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
    let result = switchboard_proxy::probe_usage(store.clone(), id.to_owned()).await;
    let checked = now();
    match result {
        Ok(usage) => Ok(usage),
        Err(_) => {
            let delay = backoff(
                account
                    .usage_health
                    .filter(|h| h.status != "ok")
                    .map(|h| (h.checked_at, h.next_check_at)),
            );
            store.usage_health_credential(id, &generation, "failed", checked, checked + delay)?;
            Err(
                "Usage unavailable. Check sign-in or try again after the next scheduled check."
                    .into(),
            )
        }
    }
}

/// Adopt only a profile already captured by the operator. The ordinary client
/// owns refresh; a new login never silently adds or enables an account.
fn sync_live_sources(store: &Store) {
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
        let Ok(profile) = external::capture_current(provider) else {
            continue;
        };
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

fn rotate(runtime: &Runtime, native_sources: bool) -> Result<(), String> {
    let mut decisions = Vec::new();
    let snapshot = runtime.store.snapshot()?;
    for policy in snapshot.policies.iter().filter(|p| p.enabled) {
        let current = if policy.target == "managed" {
            snapshot
                .routes
                .get(&format!("{}:{}", policy.provider.as_str(), policy.pool))
                .cloned()
        } else if native_sources {
            external::capture_current(Provider::Claude)
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
        let decision = runtime
            .store
            .rotation_decision(policy, current.as_deref(), now())?;
        let mut reason = decision.reason;
        if let Some(id) = decision.candidate_id.as_deref() {
            let result = if policy.target == "managed" {
                runtime
                    .store
                    .select_with_cooldown(policy.provider, &policy.pool, id, now())
            } else if native_sources {
                crate::activate_native(&runtime.store, id, current.as_deref())
            } else {
                Err("Native account source unavailable.".into())
            };
            if result.is_ok() {
                runtime.invalidate_current();
                reason = "switched".into();
            } else {
                reason = "activation_failed".into();
            }
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
