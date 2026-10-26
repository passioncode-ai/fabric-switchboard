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

use crate::usage_gate::UsageGate;

/// The active quota cadence (SB-48); also the source-sync and first-failure interval.
pub const INTERVAL_SECONDS: i64 = switchboard_core::CHECK_ACTIVE_SECONDS;
const MAX_BACKOFF: i64 = 1800;
/// The background cadence (lifecycle LC-08: nothing polls faster than 30 seconds). Each pass
/// is cheap when nothing changed: a quiet probe of the sign-in sources, no process spawned.
pub const PASS_SECONDS: u64 = 30;
/// Quota checks per pass; with the 30 s pass this keeps a full store on its 180 s schedule.
const CHECKS_PER_PASS: usize = 4;
/// Timestamps held in memory are written at least this often (a crash loses no more).
const FLUSH_SECONDS: i64 = 900;
/// A renewal that did not happen is tried again after this long, not on every pass.
const RENEW_RETRY_SECONDS: i64 = 60;
/// SB-106: an account whose usage check the provider refused keeps the ordinary cadence for
/// this long after the refusal, near its limit or not.
pub(crate) const PACED_AFTER_REFUSAL_SECONDS: i64 = 3600;

/// SB-185: a credential read at least this slow is logged as `vault_slow`.
const SLOW_VAULT_READ_MS: u64 = 1000;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs() as i64)
        .unwrap_or(0)
}

pub struct MonitorHandle {
    stop: tokio::sync::watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}
impl MonitorHandle {
    pub fn start(runtime: &Arc<Runtime>, native_sources: bool) -> Self {
        let weak = Arc::downgrade(runtime);
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(PASS_SECONDS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut pass = Pass::default();
            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = stopped.changed() => break,
                }
                if *stopped.borrow() {
                    break;
                }
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                pass.run_with_stop(&runtime, native_sources, now(), Some(&stopped))
                    .await;
            }
        });
        Self {
            stop,
            task: Some(task),
        }
    }
    /// Starts no further pass and waits up to `deadline` for the one in flight — a renewal
    /// must store the token the provider just returned — then abandons it. True: drained.
    pub async fn stop(mut self, deadline: Duration) -> bool {
        let _ = self.stop.send(true);
        let Some(mut task) = self.task.take() else {
            return true;
        };
        match tokio::time::timeout(deadline, &mut task).await {
            Ok(_) => true,
            Err(_) => {
                task.abort();
                false
            }
        }
    }
}
impl Drop for MonitorHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// What the background remembers between passes.
#[derive(Default)]
pub(crate) struct Pass {
    last_source_sync: i64,
    /// What the previous pass saw of Claude Swap: whether it ran, and its files.
    swap_was_running: Option<bool>,
    swap_last_signature: Option<(u64, u64, u128)>,
    renew: RenewSchedule,
    /// When the pool's next accounts were last brought forward (SB-106): at most once a minute,
    /// the cadence it sets.
    last_pull_in: i64,
}
/// When each inactive Claude account's token comes due, learned from the vault only when the
/// store's credentials changed — not by reading every credential on every pass (LC-08).
#[derive(Default)]
struct RenewSchedule {
    /// `refresh::credentials_key` when `due_at` was read.
    changes: Option<u64>,
    due_at: Vec<(String, i64)>,
    retry_at: std::collections::HashMap<String, i64>,
}
impl Pass {
    #[cfg(test)]
    pub(crate) async fn run(&mut self, runtime: &Runtime, native_sources: bool, time: i64) {
        self.run_with_stop(runtime, native_sources, time, None)
            .await;
    }
    async fn run_with_stop(
        &mut self,
        runtime: &Runtime,
        native_sources: bool,
        time: i64,
        stop: Option<&tokio::sync::watch::Receiver<bool>>,
    ) {
        // SB-185: a credential read that took a second or more since the last pass, by its time
        // only; the log then tells a stalled vault from the operations that waited on it.
        let slowest = runtime.store.take_slowest_read();
        if slowest >= SLOW_VAULT_READ_MS {
            crate::oplog::event(
                "vault_slow",
                &[("ms", crate::oplog::Field::Number(slowest as i64))],
            );
        }
        // With no saved Claude OAuth account there is nothing of Claude Code's to keep current:
        // no Keychain read and no Claude Swap read. Unassigned work discovery is stop-aware.
        let claude = native_sources && claude_oauth_saved(&runtime.store);
        if native_sources
            && (time - self.last_source_sync >= INTERVAL_SECONDS || time < self.last_source_sync)
        {
            self.sync_sources(runtime, claude, time).await;
        }
        if native_sources {
            let _ = runtime.maybe_backup(time, false);
        }
        // SB-159: the PassionCode.ai organizations, read again at most every 6 hours while
        // connected; network work outside the pass and the owner transaction.
        if native_sources
            && runtime.refresh.grants_allowed()
            && crate::vesta::due(&runtime.root, time)
        {
            let root = runtime.root.clone();
            let store = runtime.store.clone();
            let grants = runtime.refresh.grants_flag();
            tokio::spawn(async move {
                let Some(read) = crate::vesta::sync_if_due(&root, &store, || {
                    grants.load(std::sync::atomic::Ordering::SeqCst)
                })
                .await
                else {
                    return;
                };
                let outcome = match &read {
                    Ok(v) => v["state"].as_str().map_or("failed", |s| match s {
                        "ok" => "ok",
                        "sign_in_again" => "sign_in_again",
                        "provider_sign_in" => "provider_sign_in",
                        _ => "unreachable",
                    }),
                    Err(_) => "failed",
                };
                crate::oplog::event(
                    "vesta_read",
                    &[("outcome", crate::oplog::Field::Code(outcome))],
                );
            });
        }
        // Reads recent transcripts: file work, in a blocking section (2026-10-17 stalls).
        crate::blocking::run(|| scan_limits(runtime, claude, time));
        if stop.as_ref().is_some_and(|s| *s.borrow()) {
            return;
        }
        // SB-117: a project's new folders, found by stamps only (LC-04); a real owner only.
        if native_sources {
            runtime.suggest_pass_with_stop(time, stop).await;
            if stop.as_ref().is_some_and(|s| *s.borrow()) {
                return;
            }
            // SB-118: ordinary Claude Code in a project's folders uses the project's accounts.
            runtime.bindings_pass().await;
        }
        if stop.as_ref().is_some_and(|s| *s.borrow()) {
            return;
        }
        // SB-106: near its limit the account a session works on is checked sooner, and the
        // pool's next account every minute — never one whose checks the provider refused lately.
        runtime.store.set_paced(
            runtime
                .usage_gate
                .refused_within(time, PACED_AFTER_REFUSAL_SECONDS),
        );
        // With the same slack as the schedules it sets: passes 30 s apart, stamped in whole
        // seconds, may be 59 s apart.
        if time.abs_diff(self.last_pull_in)
            >= (switchboard_core::CHECK_NEAR_LIMIT_SECONDS
                - switchboard_core::NEAR_LIMIT_SLACK_SECONDS) as u64
        {
            let _ = runtime.store.pull_in_next_accounts(time);
            self.last_pull_in = time;
        }
        let Ok(snapshot) = runtime.store.snapshot() else {
            return;
        };
        runtime
            .usage_gate
            .forget_missing(snapshot.accounts.iter().map(|a| a.id.as_str()));
        let mut due: Vec<_> = snapshot
            .accounts
            .into_iter()
            .filter(|a| due_now(a, time, &runtime.usage_gate))
            .collect();
        // Oldest due first: a failing first row cannot starve another account.
        due.sort_by_key(|a| {
            a.usage_health
                .as_ref()
                .map(|h| h.next_check_at)
                .unwrap_or(0)
        });
        if native_sources {
            renew_due(runtime, time, &mut self.renew).await;
        }
        for account in due.into_iter().take(CHECKS_PER_PASS) {
            let _ = check(
                &runtime.store,
                runtime.native,
                &runtime.refresh,
                &account.id,
                Some(&runtime.mutations),
                &runtime.usage_gate,
            )
            .await;
        }
        if claude && native_rotation_on(&runtime.store) {
            // A native switch files the live lineage under its account: know its owner
            // first (network, outside the lock; at most once a minute per lineage).
            crate::refresh::learn_live_owner(&runtime.store, runtime.native, &runtime.refresh)
                .await;
        }
        {
            let _mutation = runtime.hold("rotate").await;
            // A native switch writes Claude Code's files and Keychain item: a blocking section, so
            // the proxy and control tasks on this worker do not wait with it (up to 49 s on a
            // swapping machine, 2026-10-19).
            let _ = crate::blocking::run(|| rotate(runtime, native_sources));
        }
        // A workflow whose executor hit its limit goes to the next agent of its chain (SB-73);
        // nothing happens without a chain. Outside the lock: the hand-over takes it itself.
        // SB-99: saved Kimi accounts, seen only while a chain is set (every 10 min, at most 4).
        crate::kimi::watch(&runtime.root, &runtime.store, &runtime.kimi_watch, time).await;
        crate::fallback::pass(runtime, time).await;
        // SB-108: a hand-over no console took in two minutes opens in Terminal.
        crate::handovers::sweep(&runtime.root, time, crate::launch::start_terminal);
        // Analytics: the daily active event and accounts added by any path; sends what waits.
        if let Some(client) = runtime.analytics() {
            if let Ok(snapshot) = runtime.store.snapshot() {
                client.tick(&snapshot, time);
            }
            // Sent beside the pass: a slow analytics server never delays a quota check.
            if client.pending() > 0 {
                tokio::spawn(async move { client.flush().await });
            }
        }
        // Quota timestamps wait in memory; they reach the disk with the next real change or
        // after a quarter of an hour.
        let _ = runtime.store.flush_if_older(time, FLUSH_SECONDS);
        // SB-145: the proxy's counts, written at most every quarter of an hour.
        runtime.absorb_statistics();
        runtime.statistics.save(&runtime.root, time, false);
    }

    async fn sync_sources(&mut self, runtime: &Runtime, claude: bool, time: i64) {
        // Network first, outside the lock: who owns a lineage not seen before.
        if claude {
            crate::refresh::learn_live_owner(&runtime.store, runtime.native, &runtime.refresh)
                .await;
        }
        // Claude Swap's files are read outside the lock; adopting its newer generations
        // happens under it, with the live sync. Claude Swap only matters for saved Claude
        // accounts.
        let activity = if claude {
            (runtime.native.swap_activity)()
        } else {
            crate::external::SwapActivity::default()
        };
        let running = activity.running;
        runtime
            .refresh
            .set_swap_activity(activity.running, activity.switching);
        let signature = if claude {
            (runtime.native.swap_signature)()
        } else {
            None
        };
        let due = crate::refresh::swap_read_due(
            running,
            self.swap_was_running,
            signature,
            self.swap_last_signature,
        );
        let read = due.then(runtime.native.swap);
        // After a stop, a row that could not be read may hold Swap's last renewal: read
        // again next pass rather than mark the files as seen.
        let partial = matches!(&read, Some(Ok(batch)) if !running && batch.failed > 0);
        let swap = match (running, read) {
            (true, Some(Ok(batch))) => {
                crate::refresh::SwapView::Profiles(batch.profiles, batch.failed_emails)
            }
            (true, _) => crate::refresh::SwapView::Unreadable,
            (false, Some(Ok(batch))) => crate::refresh::SwapView::Stopped(batch.profiles),
            (false, _) => crate::refresh::SwapView::NotRunning,
        };
        // A failed read (Swap was writing its files) is repeated next pass: neither the files
        // nor the stop are marked as seen.
        let failed = due
            && (partial
                || matches!(
                    swap,
                    crate::refresh::SwapView::Unreadable | crate::refresh::SwapView::NotRunning
                ));
        if !failed {
            self.swap_last_signature = signature;
            self.swap_was_running = Some(running);
        }
        let mut mutation = runtime.hold("source_sync").await;
        // Claude Code left its token expired: renew it under Claude Code's locks so managed
        // sessions and quota checks on that account keep working.
        if claude {
            crate::refresh::renew_idle_live(&runtime.store, runtime.native, &runtime.refresh).await;
            mutation.phase("renew_live");
        }
        crate::refresh::follow_claude_swap(&runtime.store, &runtime.refresh, swap);
        mutation.phase("swap");
        sync_live_sources(&runtime.store, runtime.native.current, &runtime.refresh);
        mutation.phase("sync");
        runtime.invalidate_current();
        self.last_source_sync = time;
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

/// Due for this pass. A row the provider asked to wait for keeps no slot, even when its
/// schedule says due (a clock set back); a new credential generation — its usage health
/// cleared — is checked at once. No credential is read here (LC-04).
fn due_now(a: &Account, time: i64, gate: &UsageGate) -> bool {
    due(a, time) && !(a.usage_health.is_some() && gate.held(&a.id, time))
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

/// When the background next checks a failed account. A provider 429 is honoured: never before
/// its not-before (`usage_gate::not_before_delay`), never sooner than the ordinary backoff.
/// The ordinary backoff paces only the background; a person may check again at once.
fn failure_delay(previous: Option<(i64, i64)>, rate_limited: Option<Option<i64>>) -> i64 {
    let ordinary = backoff(previous);
    match rate_limited {
        None => ordinary,
        Some(wait) => ordinary.max(crate::usage_gate::not_before_delay(wait)),
    }
}

/// The `usage_check` outcome of a failed check: the proxy's own code, else the store's code
/// for a refusal to keep the answer (PRD-17: `superseded`, `credential_changed`,
/// `invalid_observation`, `account_gone`, `vault`, `store`), else `no_windows` for an answer
/// without a quota window, else `other`.
fn failure_code(failure: &switchboard_proxy::ProbeFailure) -> &'static str {
    match failure.kind() {
        "other" => Store::usage_refusal_code(&failure.message).unwrap_or(
            if failure.message == NO_WINDOWS {
                "no_windows"
            } else {
                "other"
            },
        ),
        code => code,
    }
}
/// `switchboard_proxy::parse_usage`'s refusal of an answer with no quota window, which the
/// proxy gives no code (pinned by
/// `a_failed_check_the_store_or_an_empty_answer_refused_has_its_own_code`).
const NO_WINDOWS: &str = "Provider returned no usage windows.";
/// The provider's usage endpoint, sent `credential`; a test gate may name a synthetic one.
async fn probe_provider(
    store: Arc<Store>,
    id: &str,
    credential: switchboard_core::Credential,
    gate: &UsageGate,
) -> Result<Usage, switchboard_proxy::ProbeFailure> {
    #[cfg(test)]
    {
        let synthetic = gate.origins.lock().unwrap().clone();
        if let Some((claude, codex)) = synthetic {
            return switchboard_proxy::probe_usage_detailed_at(
                store,
                id.to_owned(),
                credential,
                (&claude, &codex),
            )
            .await;
        }
    }
    let _ = gate;
    switchboard_proxy::probe_usage_detailed(store, id.to_owned(), credential).await
}

/// One quota check of `id`. `rejected` is set to the account's provider and the credential sent
/// when the provider itself answered 401 (B-5). A token that expired here is refused before the network,
/// and that refusal says nothing about the seat.
pub(crate) async fn probe(
    store: Arc<Store>,
    id: &str,
    gate: &UsageGate,
    rejected: &mut Option<(Provider, switchboard_core::Credential)>,
) -> Result<Usage, String> {
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
            crate::oplog::event(
                "usage_check",
                &[("outcome", crate::oplog::Field::Code("vault"))],
            );
            return Err(
                "Usage unavailable. Check sign-in or try again after the next scheduled check."
                    .into(),
            );
        }
    };
    // The hold below binds to this credential: the one the provider actually answered.
    let result = probe_provider(store.clone(), id, generation.clone(), gate).await;
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
            // Nobody — desktop, CLI, MCP or this pass — checks again before the provider's
            // not-before; the hold binds to the token the provider answered. Recorded before the
            // health write, which can fail (a newer observation, storage) and must not drop it.
            if let Some(wait) = failure.rate_limited {
                gate.hold(
                    id,
                    &generation,
                    checked,
                    checked + crate::usage_gate::not_before_delay(wait),
                );
            }
            // Diagnosis only: a fixed code, never the provider's body or a token.
            match failure.status {
                Some(status) => crate::oplog::event(
                    "usage_check",
                    &[
                        ("outcome", crate::oplog::Field::Code(failure_code(&failure))),
                        ("status", crate::oplog::Field::Number(i64::from(status))),
                    ],
                ),
                None => crate::oplog::event(
                    "usage_check",
                    &[("outcome", crate::oplog::Field::Code(failure_code(&failure)))],
                ),
            }
            // B-5 fix cycle 1: a Codex 401 within `CODEX_EXPIRY_SKEW` of the token's local
            // expiry may be a clock running ahead here: no verdict.
            let skew = if account.provider == Provider::Codex {
                CODEX_EXPIRY_SKEW
            } else {
                0
            };
            if failure.message == switchboard_proxy::REJECTED
                && generation.expires_at.is_none_or(|t| t > checked + skew)
            {
                *rejected = Some((account.provider, generation.clone()));
            }
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
/// `lock` is the owner's mutation lock when the caller does not already hold it. Every caller
/// comes through here, so here the provider's not-before is enforced (SB-39): during it no
/// renewal and no request is made. Same-account checks share one request.
pub(crate) async fn check(
    store: &Arc<Store>,
    native: crate::NativeSources,
    refresh: &crate::refresh::RefreshState,
    id: &str,
    lock: Option<&tokio::sync::Mutex<()>>,
    gate: &UsageGate,
) -> Result<Usage, String> {
    check_as(store, native, refresh, id, lock, gate, false).await
}
/// B-5 fix cycle 1: a quota check a person asked for (the owner's Usage operation, the offline
/// CLI's). Past a usage verdict it asks the provider once more: any answer other than a 401
/// takes the verdict back, so a provider's mistake never needs a new sign-in or a restart. The
/// background pass never comes here.
pub(crate) async fn check_by_person(
    store: &Arc<Store>,
    native: crate::NativeSources,
    refresh: &crate::refresh::RefreshState,
    id: &str,
    lock: Option<&tokio::sync::Mutex<()>>,
    gate: &UsageGate,
) -> Result<Usage, String> {
    check_as(store, native, refresh, id, lock, gate, true).await
}
#[allow(clippy::too_many_arguments)]
async fn check_as(
    store: &Arc<Store>,
    native: crate::NativeSources,
    refresh: &crate::refresh::RefreshState,
    id: &str,
    lock: Option<&tokio::sync::Mutex<()>>,
    gate: &UsageGate,
    person: bool,
) -> Result<Usage, String> {
    // SB-179: the owner transaction is held for the checks and the store, not for the grant.
    let refreshed = |force: bool| async move {
        crate::refresh::ensure_fresh_with(store, native, refresh, id, force, lock, "usage_renewal")
            .await
    };
    // A dead lineage leaves the due queue for the long backoff; otherwise it would sort
    // first on every tick and starve every other account's check and renewal.
    let dead = || {
        let time = now();
        let _ = store.usage_health(id, "failed", time, time + MAX_BACKOFF);
        Err(SIGN_IN.to_string())
    };
    let sequence = || async {
        if gate.has_hold(id) {
            if let Ok(credential) = store.stored_credential(id) {
                if gate.active(id, &credential, now()).is_some() {
                    return Err(switchboard_proxy::USAGE_RATE_LIMITED.to_string());
                }
            }
        }
        // B-5 fix cycle 1: a person's check asks once past a usage verdict (never past a
        // lineage the token endpoint refused). Nothing is renewed: a 401 keeps the verdict.
        if person && refresh.usage_verdict_only(store, id) {
            let mut seen = None;
            return match probe(store.clone(), id, gate, &mut seen).await {
                Err(error) if error == switchboard_proxy::REJECTED => dead(),
                other => {
                    refresh.lift_verdict(id);
                    other
                }
            };
        }
        if refreshed(false).await == crate::refresh::Outcome::SignInRequired {
            return dead();
        }
        // B-5: a seat found ended (a Codex one is never renewed) is not asked again until its
        // credential changes. Nothing remembered: no vault read.
        if refresh.is_dead(store, id) {
            return dead();
        }
        let mut seen = None;
        match probe(store.clone(), id, gate, &mut seen).await {
            Err(error) if error == switchboard_proxy::REJECTED => match refreshed(true).await {
                crate::refresh::Outcome::Refreshed => {
                    let mut again = None;
                    let second = probe(store.clone(), id, gate, &mut again).await;
                    // B-5 (SB-160): a 401 on the token just renewed is the provider's verdict.
                    if let Some((_, credential)) = again {
                        refresh.reject(id, &credential);
                        crate::refresh::seat_ended(store, id, "usage");
                        return dead();
                    }
                    second
                }
                crate::refresh::Outcome::SignInRequired => dead(),
                _ => match seen {
                    // B-5: Codex is never renewed here; a 401 on two checks in a row, a minute or
                    // more apart, is the verdict. One 401 is not (B-Q3: nothing is moved either).
                    // B-5 fix cycle 1: the account the ordinary Codex CLI is signed in to is
                    // exempt, as Claude Code's is (never a verdict here).
                    Some((Provider::Codex, _)) if store.is_in_use(id) => {
                        refresh.clear_rejection(id);
                        Err(error)
                    }
                    Some((Provider::Codex, credential)) => {
                        if refresh.note_rejection(id, &credential, now()) {
                            crate::refresh::seat_ended(store, id, "usage");
                            return dead();
                        }
                        Err(error)
                    }
                    _ => {
                        refresh.clear_rejection(id);
                        Err(error)
                    }
                },
            },
            result => {
                refresh.clear_rejection(id);
                result
            }
        }
    };
    // One deadline below the control channel's 30 seconds, including any wait for a check of
    // the same account already running, so a CLI or MCP caller never times out while the
    // owner keeps working on its behalf.
    tokio::time::timeout(CHECK_DEADLINE, gate.coalesce(id, store.changes(), sequence))
        .await
        .unwrap_or_else(|_| {
            Err(
                "Usage unavailable. Check sign-in or try again after the next scheduled check."
                    .into(),
            )
        })
}
use crate::refresh::SIGN_IN;
/// B-5 fix cycle 1: a Codex 401 this close to the token's local expiry is no verdict.
const CODEX_EXPIRY_SKEW: i64 = 300;
const CHECK_DEADLINE: Duration = Duration::from_secs(25);

/// Renews inactive Claude tokens on their own schedule. Quota checks back off up to 30
/// minutes, longer than the renewal margin, so a managed route could otherwise expire
/// between checks. At most two grants per pass. Credentials are read only when the store's
/// credentials changed; an account whose renewal did not happen waits a minute.
async fn renew_due(runtime: &Runtime, time: i64, schedule: &mut RenewSchedule) {
    let Ok(snapshot) = runtime.store.snapshot() else {
        return;
    };
    // N1 round 1 (seam tier): keyed on what the schedule reads, not on `Store::changes`, which
    // every project save moves; the reads run off the async worker (`blocking::run`).
    // N1 round 3: the key is kept only when every read succeeded or found no account; a read
    // that failed (a `security` timeout, a locked keychain) keeps the account's earlier due time
    // for this pass and is read again at the next one, never dropped until a credential is written.
    // B-5 fix cycle 1: and on the verdicts, which change without a credential write.
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        crate::refresh::credentials_key(&runtime.store, &snapshot).hash(&mut h);
        runtime.refresh.verdict_generation().hash(&mut h);
        h.finish()
    };
    if schedule.changes != Some(key) {
        // B-5 fix cycle 1: an ended seat (a lineage refused, a usage verdict) stays out until
        // its credential changes, which moves the key; cached, so no read while nothing moved.
        let ended = runtime.refresh.sign_in_required(&runtime.store);
        let previous = std::mem::take(&mut schedule.due_at);
        let (due_at, certain) = crate::blocking::run(|| {
            let mut certain = true;
            let due_at = snapshot
                .accounts
                .iter()
                .filter(|a| {
                    a.enabled
                        && a.provider == Provider::Claude
                        && a.kind == AuthKind::OAuth
                        && !ended.contains(&a.id)
                })
                .filter_map(|a| match runtime.store.stored_credential(&a.id) {
                    Ok(credential) => {
                        crate::refresh::due_at(&credential).map(|at| (a.id.clone(), at))
                    }
                    Err(e) if e == switchboard_core::ACCOUNT_NOT_FOUND => None,
                    Err(_) => {
                        certain = false;
                        previous.iter().find(|(id, _)| *id == a.id).cloned()
                    }
                })
                .collect();
            (due_at, certain)
        });
        schedule.due_at = due_at;
        schedule.changes = certain.then_some(key);
    }
    let mut done = 0;
    for (id, due_at) in schedule.due_at.clone() {
        if done == 2 {
            break;
        }
        if due_at > time || schedule.retry_at.get(&id).is_some_and(|at| *at > time) {
            continue;
        }
        // SB-179: the transaction is taken for the checks and the store only; while the grant
        // is on the network, control writes go on.
        let outcome = crate::refresh::ensure_fresh_with(
            &runtime.store,
            runtime.native,
            &runtime.refresh,
            &id,
            false,
            Some(&runtime.mutations),
            "renewal",
        )
        .await;
        if outcome == crate::refresh::Outcome::Refreshed {
            done += 1;
            schedule.retry_at.remove(&id);
        } else if outcome == crate::refresh::Outcome::SignInRequired {
            // B-5 fix cycle 1: out of the schedule until a credential is written (the key).
            schedule.due_at.retain(|(due, _)| *due != id);
            schedule.retry_at.remove(&id);
        } else {
            schedule.retry_at.insert(id, time + RENEW_RETRY_SECONDS);
        }
    }
}

/// Adopt only a profile already captured by the operator. The ordinary client
/// owns refresh; a new login never silently adds or enables an account.
/// A saved Claude OAuth account exists: the only reason to look at Claude Code's sign-in.
fn claude_oauth_saved(store: &Store) -> bool {
    store.snapshot().is_ok_and(|s| {
        s.accounts
            .iter()
            .any(|a| a.provider == Provider::Claude && a.kind == AuthKind::OAuth)
    })
}
/// Automatic switching of the ordinary Claude Code is on somewhere.
fn native_rotation_on(store: &Store) -> bool {
    store.snapshot().is_ok_and(|s| {
        s.policies
            .iter()
            .any(|p| p.enabled && p.target == "claude_cli")
    })
}
fn sync_live_sources(
    store: &Store,
    current: fn(Provider) -> Result<external::CapturedProfile, String>,
    refresh: &crate::refresh::RefreshState,
) {
    let Ok(snapshot) = store.snapshot() else {
        return;
    };
    // Which saved accounts the ordinary CLIs are signed in to: those are checked on the active
    // cadence (SB-48). Learned from the same cached read, before any lineage decision.
    let mut in_use = std::collections::BTreeSet::new();
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
        for account in &accounts {
            if store
                .match_external(provider, &account.pool, &profile.identity)
                .ok()
                .flatten()
                .is_some_and(|matched| matched.id == account.id)
            {
                in_use.insert(account.id.clone());
            }
        }
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
    let _ = store.set_in_use(in_use);
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
        runtime.transcripts(time)
    } else {
        vec![]
    };
    runtime
        .limits
        .forget_missing(snapshot.accounts.iter().map(|a| a.id.as_str()));
    runtime.limits.refresh(&snapshot, native, &files, time);
    // SB-98: what the proxy refused since the last pass is evidence at once, and the proxy
    // reads the held accounts back from the store when it next has to move a request.
    let refusals = runtime.store.take_refusals();
    if !refusals.is_empty() {
        runtime.limits.refused(&snapshot, &refusals, time);
    }
    runtime.store.set_limited(runtime.limits.limited_ids(time));
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
        // SB-102: refusals the proxy recorded since the last scan count at once, so the pass
        // never moves a pool back onto the account a request was just refused on.
        excluded.extend(runtime.store.limited());
        excluded.extend(
            runtime
                .refresh
                .sign_in_required(&runtime.store)
                .into_iter()
                .filter(|id| current.as_deref() != Some(id.as_str())),
        );
        // SB-98: a candidate whose switch fails is set aside for ten minutes and the next one
        // is tried in the same pass — up to three — instead of the same one every 30 s.
        // SB-102: only ever as a candidate — the account in use is never "limited" for an earlier
        // failed switch to it; expired entries go.
        if let Ok(mut failed) = runtime.failed_switches.lock() {
            failed.retain(|_, until| *until > now());
            excluded.extend(
                failed
                    .keys()
                    .filter(|id| current.as_deref() != Some(id.as_str()))
                    .cloned(),
            );
        }
        let mut attempt = 0;
        // The last failed switch: when nothing else can be tried, that failure is the answer.
        let mut last_failure: Option<(String, String)> = None;
        let (candidate, reason) = loop {
            attempt += 1;
            let decision = runtime.store.rotation_decision_with(
                policy,
                current.as_deref(),
                now(),
                &excluded,
            )?;
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
                    // SB-102: a successful switch clears any earlier set-aside of the account.
                    if let Ok(mut failed) = runtime.failed_switches.lock() {
                        failed.remove(id);
                    }
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
                    if let Some(client) = runtime.analytics() {
                        client.switched(
                            policy.provider.as_str(),
                            if policy.target == "managed" {
                                "managed"
                            } else {
                                "native"
                            },
                            if reason == "switched_on_limit" {
                                "limit"
                            } else {
                                "rotation"
                            },
                        );
                    }
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
            let failed = reason == "activation_failed" || reason == "switch_failed";
            match decision.candidate_id.clone() {
                Some(id) if failed && attempt < 3 => {
                    if let Ok(mut set_aside) = runtime.failed_switches.lock() {
                        set_aside.insert(id.clone(), now() + 600);
                    }
                    excluded.insert(id.clone());
                    last_failure = Some((id, reason));
                }
                Some(id) if failed => {
                    if let Ok(mut set_aside) = runtime.failed_switches.lock() {
                        set_aside.insert(id, now() + 600);
                    }
                    break (decision.candidate_id, reason);
                }
                None => match last_failure.take() {
                    Some((id, failure)) => break (Some(id), failure),
                    None => break (None, reason),
                },
                _ => break (decision.candidate_id, reason),
            }
        };
        decisions.push(json!({"provider":policy.provider,"pool":policy.pool,"target":policy.target,"reason":reason,"candidate_id":candidate}));
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
    struct CountingVault {
        inner: MemoryVault,
        gets: std::sync::atomic::AtomicUsize,
    }
    impl switchboard_core::Vault for CountingVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.get(id)
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// N1 round 1 (seam tier): the renewal schedule read every Claude account's credential
    /// from the vault whenever `Store::changes` moved, which every project save does.
    #[tokio::test]
    async fn a_project_save_does_not_reread_the_renewal_schedule() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault {
            inner: MemoryVault::default(),
            gets: Default::default(),
        });
        let runtime = Runtime::open(root.path().to_owned(), vault.clone())
            .await
            .unwrap();
        let mut credential = crate::fixtures::credential("synthetic-a");
        credential.expires_at = Some(now() + 86_400);
        credential.refresh_token = Some("synthetic-a-refresh".into());
        runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential,
                Some(crate::fixtures::identity("synthetic-a")),
            )
            .unwrap();
        let mut schedule = RenewSchedule::default();
        renew_due(&runtime, now(), &mut schedule).await;
        assert_eq!(schedule.due_at.len(), 1);
        let reads = vault.gets.load(std::sync::atomic::Ordering::SeqCst);
        let work = tempfile::tempdir().unwrap();
        let mut folders = Vec::new();
        for n in 0..5 {
            let folder = work.path().join(format!("repo-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            folders.push(folder.canonicalize().unwrap());
            runtime
                .store
                .save_project(Some("app"), "App", &folders, &[])
                .or_else(|_| runtime.store.save_project(None, "App", &folders, &[]))
                .unwrap();
            renew_due(&runtime, now(), &mut schedule).await;
        }
        assert_eq!(vault.gets.load(std::sync::atomic::Ordering::SeqCst), reads);
        // Disabling the account is seen without a credential write.
        let id = schedule.due_at[0].0.clone();
        runtime.store.update_fields(&id, None, Some(false)).unwrap();
        renew_due(&runtime, now(), &mut schedule).await;
        assert!(schedule.due_at.is_empty());
    }
    /// B-5 fix cycle 1 (idle budget): an ended Claude seat whose token comes due was asked about
    /// every minute (three vault reads and the owner transaction each). It leaves the renewal
    /// schedule until its credential changes, and a schedule rebuilt meanwhile leaves it out.
    #[tokio::test]
    async fn an_ended_seat_leaves_the_renewal_schedule_until_its_credential_changes() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault {
            inner: MemoryVault::default(),
            gets: Default::default(),
        });
        let runtime = Runtime::open(root.path().to_owned(), vault.clone())
            .await
            .unwrap();
        let mut credential = crate::fixtures::credential("synthetic-b");
        credential.expires_at = Some(now() + 60);
        credential.refresh_token = Some("synthetic-b-refresh".into());
        let id = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential.clone(),
                Some(crate::fixtures::identity("synthetic-b")),
            )
            .unwrap()
            .id;
        // A usage check found the seat ended (B-5).
        runtime.refresh.reject(&id, &credential);
        let start = now();
        let mut schedule = RenewSchedule::default();
        renew_due(&runtime, start, &mut schedule).await;
        let reads = vault.gets.load(std::sync::atomic::Ordering::SeqCst);
        // An idle hour, one renewal look a minute.
        for k in 1..=60 {
            renew_due(&runtime, start + k * RENEW_RETRY_SECONDS, &mut schedule).await;
        }
        assert_eq!(
            vault.gets.load(std::sync::atomic::Ordering::SeqCst) - reads,
            0,
            "an ended seat is not read again on a timer"
        );
        assert!(schedule.due_at.iter().all(|(i, _)| *i != id));
        // A schedule built while the seat is ended leaves it out.
        let mut rebuilt = RenewSchedule::default();
        renew_due(&runtime, start, &mut rebuilt).await;
        assert!(rebuilt.due_at.iter().all(|(i, _)| *i != id));
        // A verdict a person's check took back puts it back without a credential write.
        runtime.refresh.lift_verdict(&id);
        renew_due(&runtime, start, &mut rebuilt).await;
        assert!(rebuilt.due_at.iter().any(|(i, _)| *i == id));
        // *Sign in again*: the new credential is scheduled.
        let mut again = crate::fixtures::credential("synthetic-b");
        again.expires_at = Some(now() + 86_400);
        again.refresh_token = Some("synthetic-b-new-login".into());
        runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                again,
                Some(crate::fixtures::identity("synthetic-b")),
            )
            .unwrap();
        renew_due(&runtime, start, &mut schedule).await;
        assert!(schedule.due_at.iter().any(|(i, _)| *i == id));
    }
    /// N1 round 3 (adversarial): a vault read that failed while the renewal schedule was built (a
    /// locked keychain, a `security` timeout) must not drop the account from the schedule until
    /// some credential is written: the next pass reads it again.
    #[tokio::test]
    async fn a_failed_read_does_not_stick_in_the_renewal_schedule() {
        #[derive(Default)]
        struct FlakyGets {
            inner: MemoryVault,
            fail: std::sync::atomic::AtomicBool,
        }
        impl switchboard_core::Vault for FlakyGets {
            fn get(&self, id: &str) -> Result<Credential, String> {
                if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                    return Err("Credential storage unavailable".into());
                }
                self.inner.get(id)
            }
            fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
                self.inner.put(id, value)
            }
            fn delete(&self, id: &str) -> Result<(), String> {
                self.inner.delete(id)
            }
        }
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(FlakyGets::default());
        let runtime = Runtime::open(root.path().to_owned(), vault.clone())
            .await
            .unwrap();
        let mut credential = crate::fixtures::credential("synthetic-a");
        credential.expires_at = Some(now() + 86_400);
        credential.refresh_token = Some("synthetic-a-refresh".into());
        runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential,
                Some(crate::fixtures::identity("synthetic-a")),
            )
            .unwrap();
        let mut schedule = RenewSchedule::default();
        vault.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        renew_due(&runtime, now(), &mut schedule).await;
        vault.fail.store(false, std::sync::atomic::Ordering::SeqCst);
        renew_due(&runtime, now(), &mut schedule).await;
        assert_eq!(
            schedule.due_at.len(),
            1,
            "the account came back once the vault answered"
        );
    }
    /// PRD-17 (iteration 2026-10-24): 15 quota checks in 48 h were logged `usage_check
    /// outcome=other`. The store's refusals of the answer (`Store::usage_refusal_code`) and an
    /// answer with no quota window had no code; each has one now, and the provider's own codes
    /// stay as they were.
    #[test]
    fn a_failed_check_the_store_or_an_empty_answer_refused_has_its_own_code() {
        use switchboard_proxy::ProbeFailure;
        let code = |message: &str| failure_code(&ProbeFailure::from(message));
        assert_eq!(
            code("Usage observation is older than the stored observation"),
            "superseded"
        );
        assert_eq!(
            code("Credential changed during usage check"),
            "credential_changed"
        );
        assert_eq!(code("Invalid usage observation"), "invalid_observation");
        assert_eq!(code("Account not found"), "account_gone");
        // The empty answer's text, as the proxy writes it today.
        let empty =
            switchboard_proxy::parse_usage_at(Provider::Claude, &serde_json::json!({}), 1_000_000)
                .unwrap_err();
        assert_eq!(code(&empty), "no_windows");
        assert_eq!(code(switchboard_proxy::REJECTED), "rejected");
        let vault = ProbeFailure {
            code: Some("vault"),
            ..ProbeFailure::from("Keychain says no")
        };
        assert_eq!(failure_code(&vault), "vault");
        assert_eq!(code("anything else"), "other");
    }
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
    /// Fails to activate synthetic-b only; every other account activates.
    fn fails_for_b(
        credential: &switchboard_core::Credential,
        target: &switchboard_core::ExternalIdentity,
        expected: Option<&switchboard_core::ExternalIdentity>,
        preserve: &mut dyn FnMut(&crate::external::Outgoing<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        if target.account_id.as_deref() == Some("synthetic-b") {
            return Err("Claude activation failed; previous account restored.".into());
        }
        crate::fixtures::activates(credential, target, expected, preserve)
    }
    #[tokio::test]
    async fn an_account_set_aside_earlier_is_not_limited_once_it_is_in_use() {
        // SB-102: the person switched to B after an earlier failed switch to it; B is in use
        // and below the threshold, so nothing moves.
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        quota(&runtime.store, &a.id, 10.0);
        quota(&runtime.store, &b.id, 20.0);
        runtime
            .store
            .select(Provider::Claude, "default", &b.id)
            .unwrap();
        runtime.store.set_policy(policy("managed")).unwrap();
        runtime
            .failed_switches
            .lock()
            .unwrap()
            .insert(b.id.clone(), now() + 600);
        rotate(&runtime, false).unwrap();
        assert_eq!(reason(&runtime), "below_threshold");
        assert_eq!(
            runtime.store.snapshot().unwrap().routes["claude:default"],
            b.id
        );
    }
    #[tokio::test]
    async fn a_candidate_that_fails_to_activate_is_set_aside_and_the_next_one_tried_at_once() {
        // SB-98: the same failing candidate every 30 s kept Claude Code on a spent account.
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, fails_for_b).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        quota(&runtime.store, &a.id, 95.0);
        quota(&runtime.store, &b.id, 10.0);
        quota(&runtime.store, &c.id, 20.0);
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        rotate(&runtime, true).unwrap();
        assert_eq!(reason(&runtime), "switched");
        let decisions = runtime.monitor_decisions.lock().unwrap().clone();
        assert_eq!(
            decisions[0]["candidate_id"],
            c.id.as_str(),
            "b failed, c took over in the same pass"
        );
        let set_aside = runtime.failed_switches.lock().unwrap().clone();
        assert!(set_aside.get(&b.id).is_some_and(|until| *until > now()));
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
    async fn without_saved_claude_accounts_the_monitor_leaves_claude_code_alone() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        assert!(!claude_oauth_saved(&runtime.store));
        assert!(!native_rotation_on(&runtime.store));
        save(&runtime.store, "synthetic-a", "default");
        assert!(claude_oauth_saved(&runtime.store));
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        assert!(native_rotation_on(&runtime.store));
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
            ..crate::UNAVAILABLE
        };
        let lock = tokio::sync::Mutex::new(());
        assert_eq!(
            check(
                &store,
                native,
                &state,
                &b.id,
                Some(&lock),
                &UsageGate::default()
            )
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
        renew_due(&runtime, now(), &mut RenewSchedule::default()).await;
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
    fn the_account_the_cli_is_signed_in_to_is_checked_on_the_active_cadence() {
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let add = |name: &str| {
            store
                .upsert(
                    name.into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    credential(name),
                    Some(identity(name)),
                )
                .unwrap()
                .id
        };
        let (a, b) = (add("synthetic-a"), add("synthetic-b"));
        let state = crate::refresh::RefreshState::default();
        sync_live_sources(&store, signed_in, &state);
        let time = now() - 5;
        let observed = |id: &str| {
            store
                .observe(
                    id,
                    Usage {
                        used_percent: 10.,
                        observed_at: time,
                        resets_at: Some(time + 86_400),
                        source: "claude_oauth".into(),
                        windows: vec![],
                    },
                )
                .unwrap();
            store
                .snapshot()
                .unwrap()
                .accounts
                .into_iter()
                .find(|x| x.id == id)
                .unwrap()
                .usage_health
                .unwrap()
                .next_check_at
                - time
        };
        assert_eq!(observed(&a), switchboard_core::CHECK_ACTIVE_SECONDS);
        assert_eq!(observed(&b), switchboard_core::CHECK_IDLE_SECONDS);
    }
    #[test]
    fn a_rate_limited_usage_check_waits_as_the_provider_asks() {
        // No 429: the ordinary backoff.
        assert_eq!(failure_delay(None, None), 180);
        // 429 without Retry-After: the 900 s floor.
        assert_eq!(failure_delay(None, Some(None)), 900);
        // Retry-After longer than the backoff is honoured whole — beyond the six hours 0.5.1
        // allowed — up to the seven-day sanity bound (SB-39).
        assert_eq!(failure_delay(None, Some(Some(2400))), 2400);
        assert_eq!(failure_delay(None, Some(Some(9 * 3600))), 9 * 3600);
        assert_eq!(failure_delay(None, Some(Some(999_999))), 7 * 86_400);
        // `retry-after: 0` (or a date already past) is no permission to ask again at once.
        assert_eq!(failure_delay(None, Some(Some(0))), 900);
        // A short Retry-After never makes the check sooner than the backoff already is.
        assert_eq!(failure_delay(Some((100, 1000)), Some(Some(5))), 1800);
    }
    #[test]
    fn a_held_row_takes_no_slot_of_the_pass() {
        let root = tempfile::tempdir().unwrap();
        let gate = UsageGate::kept_in(root.path().join(crate::usage_gate::HOLDS_FILE), 10_000);
        let mut account = switchboard_core::Account {
            id: "id-a".into(),
            label: "Synthetic".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            // Stamped ahead of a clock that was set back: the schedule alone says due.
            usage_health: Some(switchboard_core::UsageHealth {
                status: "failed".into(),
                checked_at: 100_000,
                next_check_at: 100_900,
            }),
        };
        assert!(due_now(&account, 10_000, &gate));
        gate.hold(
            "id-a",
            &crate::fixtures::credential("synthetic-a"),
            100_000,
            100_900,
        );
        // Held — and rebased on the spot: the background waits the 900 s the provider asked
        // for from the corrected clock, not the 90 000 s of the correction.
        assert!(!due_now(&account, 10_000, &gate));
        assert!(gate.held("id-a", 10_899));
        assert!(!gate.held("id-a", 10_900));
        // A new credential generation clears the usage health: checked at once.
        account.usage_health = None;
        assert!(due_now(&account, 10_000, &gate));
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
        assert!(
            probe(store.clone(), &account.id, &UsageGate::default(), &mut None)
                .await
                .is_err()
        );
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
        assert!(
            probe(store.clone(), &account.id, &UsageGate::default(), &mut None)
                .await
                .is_err()
        );
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
    /// Lifecycle LC-08's check: one idle hour of passes on a fake clock — fifteen saved Claude
    /// accounts, native rotation on, a signed-in Claude Code, backups on, nothing changing —
    /// counted against the idle budget: no sign-in read after the first, no credential read
    /// on a timer, no metadata write, no second backup.
    #[tokio::test]
    async fn an_idle_hour_on_a_fake_clock_stays_inside_the_budget() {
        use crate::fixtures::*;
        use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
        static CLOCK: AtomicI64 = AtomicI64::new(0);
        // When Claude Code last wrote its sign-in: an hour before the test starts.
        static WRITTEN: AtomicI64 = AtomicI64::new(0);
        static SIGN_IN_READS: AtomicU64 = AtomicU64::new(0);
        static GATE: external::Watch = external::Watch::new();
        fn watched(provider: Provider) -> Result<external::CapturedProfile, String> {
            let settled = WRITTEN.load(Ordering::SeqCst);
            GATE.get(
                provider,
                || CLOCK.load(Ordering::SeqCst),
                || external::Probe::Ready {
                    stamps: vec![external::Stamp::Item(Some((settled, settled)))],
                    consent: false,
                    newest: settled,
                },
                || {
                    SIGN_IN_READS.fetch_add(1, Ordering::SeqCst);
                    signed_in(provider)
                },
            )
        }
        #[derive(Default)]
        struct CountingVault {
            inner: MemoryVault,
            gets: AtomicU64,
        }
        impl switchboard_core::Vault for CountingVault {
            fn get(&self, id: &str) -> Result<Credential, String> {
                self.gets.fetch_add(1, Ordering::SeqCst);
                self.inner.get(id)
            }
            fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
                self.inner.put(id, value)
            }
            fn delete(&self, id: &str) -> Result<(), String> {
                self.inner.delete(id)
            }
        }
        struct Key;
        impl switchboard_core::backup::BackupKey for Key {
            fn load(&self) -> Result<Option<[u8; 32]>, String> {
                Ok(Some([7; 32]))
            }
            fn create(&self, _: &[u8; 32]) -> Result<bool, String> {
                Ok(false)
            }
        }
        let root = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault::default());
        let runtime = Runtime::open_with(
            root.path().to_owned(),
            vault.clone(),
            crate::NativeSources {
                current: watched,
                activate: activates,
                ..crate::UNAVAILABLE
            },
        )
        .await
        .unwrap();
        // Nothing here may reach a real provider.
        runtime
            .refresh
            .set_endpoint("http://127.0.0.1:9/token".into());
        runtime
            .refresh
            .set_profile_endpoint("http://127.0.0.1:9/profile".into());
        runtime.enable_backups(Some(Arc::new(Key)), Some(backups.path().to_owned()));
        let start = now();
        WRITTEN.store(start - 3600, Ordering::SeqCst);
        save(&runtime.store, "synthetic-a", "default");
        for n in 1..15 {
            let account = save(&runtime.store, &format!("synthetic-{n}"), "default");
            // Quota checked a moment ago; the next check is due after these two hours.
            runtime
                .store
                .usage_health(&account.id, "ok", start - 10, start + 3 * 3600)
                .unwrap();
        }
        let a = runtime.store.snapshot().unwrap().accounts[0].id.clone();
        runtime
            .store
            .usage_health(&a, "ok", start - 10, start + 3 * 3600)
            .unwrap();
        runtime.store.set_policy(policy("claude_cli")).unwrap();
        runtime.store.flush().unwrap();
        let mut pass = Pass::default();
        // The first pass learns what is there: the sign-in, the renewal schedule, a backup.
        CLOCK.store(start, Ordering::SeqCst);
        pass.run(&runtime, true, start).await;
        let reads = SIGN_IN_READS.load(Ordering::SeqCst);
        let gets = vault.gets.load(Ordering::SeqCst);
        let writes = runtime.store.metadata_writes();
        let backup_files = || std::fs::read_dir(backups.path()).unwrap().count();
        let first_backups = backup_files();
        // Then an hour of passes with nothing changing.
        let passes = 3600 / PASS_SECONDS as i64;
        for k in 1..=passes {
            let time = start + k * PASS_SECONDS as i64;
            CLOCK.store(time, Ordering::SeqCst);
            pass.run(&runtime, true, time).await;
        }
        let gets = vault.gets.load(Ordering::SeqCst) - gets;
        eprintln!(
            "idle hour: {passes} passes, sign-in reads {}, credential reads {gets}, metadata writes {}, backups {}",
            SIGN_IN_READS.load(Ordering::SeqCst) - reads,
            runtime.store.metadata_writes() - writes,
            backup_files() - first_backups
        );
        assert_eq!(passes, 120, "nothing polls faster than every 30 seconds");
        assert_eq!(
            SIGN_IN_READS.load(Ordering::SeqCst),
            reads,
            "no sign-in read"
        );
        assert_eq!(runtime.store.metadata_writes(), writes, "no metadata write");
        assert_eq!(backup_files(), first_backups, "no backup without a change");
        // One per pass for native rotation's current account, one per three-minute source
        // sync for the signed-in account (before: every account's credential, every 10 s).
        let syncs = (3600 / INTERVAL_SECONDS) as u64;
        assert!(
            gets <= passes as u64 + syncs,
            "credential reads {gets} per idle hour"
        );
        // Without rotation, only the source sync reads one.
        let mut off = policy("claude_cli");
        off.enabled = false;
        runtime.store.set_policy(off).unwrap();
        // The change itself is answered once (schedule, backup); then the idle hour.
        let next = start + (passes + 1) * PASS_SECONDS as i64;
        CLOCK.store(next, Ordering::SeqCst);
        pass.run(&runtime, true, next).await;
        let gets = vault.gets.load(Ordering::SeqCst);
        let writes = runtime.store.metadata_writes();
        for k in passes + 2..=2 * passes + 1 {
            let time = start + k * PASS_SECONDS as i64;
            CLOCK.store(time, Ordering::SeqCst);
            pass.run(&runtime, true, time).await;
        }
        let gets = vault.gets.load(Ordering::SeqCst) - gets;
        // The first sync after the change rebuilds which account holds which lineage once.
        assert!(
            gets <= syncs + 15,
            "credential reads {gets} per idle hour without rotation"
        );
        assert_eq!(runtime.store.metadata_writes(), writes);
        assert_eq!(SIGN_IN_READS.load(Ordering::SeqCst), reads);
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

    // B-5 (SB-160, SCN-078): a seat the organization removed reads as ended. A usage check whose
    // 401 survives a fresh renewal (Claude), or a 401 on two checks in a row at least a minute
    // apart (Codex, never renewed), joins `sign_in_required` until the credential changes.
    // Nothing else is a verdict: one Codex 401, any 403, a 5xx, a token that only expired here.
    mod seat {
        use super::super::*;
        use crate::fixtures::*;
        use crate::refresh::{RefreshState, REJECTION_SPACING};
        use axum::{
            http::StatusCode,
            routing::{get, post},
            Router,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;
        use switchboard_core::{Credential, MemoryVault};

        pub(super) struct Server {
            pub base: String,
            pub probes: Arc<AtomicUsize>,
            pub grants: Arc<AtomicUsize>,
        }
        /// Usage answers in order (the last one repeats), for both providers' usage paths; the
        /// token endpoint answers `token` every time.
        pub(super) async fn server(usage: &[u16], token: (u16, serde_json::Value)) -> Server {
            let script = Arc::new(Mutex::new(usage.to_vec()));
            let probes = Arc::new(AtomicUsize::new(0));
            let grants = Arc::new(AtomicUsize::new(0));
            let answer = {
                let (script, probes) = (script.clone(), probes.clone());
                move || {
                    let (script, probes) = (script.clone(), probes.clone());
                    async move {
                        probes.fetch_add(1, Ordering::SeqCst);
                        let status = {
                            let mut s = script.lock().unwrap();
                            if s.len() > 1 {
                                s.remove(0)
                            } else {
                                s[0]
                            }
                        };
                        (
                            StatusCode::from_u16(status).unwrap(),
                            axum::Json(json!({
                                "five_hour": {"utilization": 10.0, "resets_at": null},
                                "rate_limit": {"primary_window": {"used_percent": 10.0}}
                            })),
                        )
                    }
                }
            };
            let token = {
                let grants = grants.clone();
                move || {
                    let (grants, token) = (grants.clone(), token.clone());
                    async move {
                        grants.fetch_add(1, Ordering::SeqCst);
                        (StatusCode::from_u16(token.0).unwrap(), axum::Json(token.1))
                    }
                }
            };
            let app = Router::new()
                .route("/api/oauth/usage", get(answer.clone()))
                .route("/backend-api/wham/usage", get(answer))
                .route("/token", post(token));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Server {
                base,
                probes,
                grants,
            }
        }
        pub(super) fn renewed() -> (u16, serde_json::Value) {
            (
                200,
                json!({"access_token":"synthetic-b-fresh","expires_in":28800,"refresh_token":"synthetic-b-fresh-refresh"}),
            )
        }
        pub(super) const NATIVE: crate::NativeSources = crate::NativeSources {
            current: signed_in,
            activate: activates,
            live: crate::no_live,
            swap: crate::no_swap,
            ..crate::UNAVAILABLE
        };
        pub(super) fn store() -> (tempfile::TempDir, Arc<Store>) {
            let root = tempfile::tempdir().unwrap();
            let store = Arc::new(
                Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
            );
            (root, store)
        }
        /// An inactive Claude seat (Claude Code is signed in as `synthetic-a`).
        pub(super) fn claude_seat(store: &Store) -> String {
            save(store, "synthetic-b", "default").id
        }
        pub(super) fn codex_credential(token: &str, expires_at: i64) -> Credential {
            Credential {
                access_token: token.into(),
                refresh_token: Some(format!("{token}-refresh")),
                id_token: None,
                native_context: None,
                expires_at: Some(expires_at),
                account_id: Some("synthetic-c".into()),
            }
        }
        pub(super) fn codex_seat(store: &Store, token: &str, expires_at: i64) -> String {
            store
                .upsert(
                    "synthetic-c".into(),
                    Provider::Codex,
                    AuthKind::OAuth,
                    "default".into(),
                    codex_credential(token, expires_at),
                    Some(identity("synthetic-c")),
                )
                .unwrap()
                .id
        }
        pub(super) fn gate(server: &Server) -> UsageGate {
            let gate = UsageGate::default();
            *gate.origins.lock().unwrap() = Some((server.base.clone(), server.base.clone()));
            gate
        }
        pub(super) async fn run(
            store: &Arc<Store>,
            state: &RefreshState,
            id: &str,
            gate: &UsageGate,
        ) -> Result<Usage, String> {
            let lock = tokio::sync::Mutex::new(());
            check(store, NATIVE, state, id, Some(&lock), gate).await
        }

        #[tokio::test]
        async fn a_claude_seat_rejected_after_a_fresh_renewal_reads_sign_in_required() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let b = claude_seat(&store);
            let state = RefreshState::at(format!("{}/token", server.base));
            let gate = gate(&server);
            let (result, events) =
                crate::oplog::capture_async(run(&store, &state, &b, &gate)).await;
            assert_eq!(result.unwrap_err(), SIGN_IN);
            assert_eq!(server.grants.load(Ordering::SeqCst), 1, "one fresh renewal");
            assert_eq!(
                server.probes.load(Ordering::SeqCst),
                2,
                "probed before and after it"
            );
            assert_eq!(state.sign_in_required(&store), vec![b.clone()]);
            assert_eq!(
                events
                    .iter()
                    .filter(|(e, _)| *e == "seat_ended")
                    .map(|(_, f)| f.clone())
                    .collect::<Vec<_>>(),
                vec![vec![
                    ("provider", crate::oplog::Field::Code("claude")),
                    ("by", crate::oplog::Field::Code("usage")),
                    ("organization", crate::oplog::Field::Flag(false)),
                ]]
            );
            // It leaves the due queue for the long backoff, and nothing keeps spending it: the
            // next check asks neither the usage endpoint nor the token endpoint.
            let health = store.snapshot().unwrap().accounts[0]
                .usage_health
                .clone()
                .unwrap();
            assert!(health.next_check_at >= now() + MAX_BACKOFF - 5);
            assert_eq!(run(&store, &state, &b, &gate).await.unwrap_err(), SIGN_IN);
            assert_eq!(server.grants.load(Ordering::SeqCst), 1);
            assert_eq!(server.probes.load(Ordering::SeqCst), 2);
            assert!(
                state.is_dead(&store, &b),
                "never handed to Claude Code either"
            );
            assert_eq!(
                crate::refresh::ensure_fresh(&store, NATIVE, &state, &b, false).await,
                crate::refresh::Outcome::SignInRequired,
                "the scheduled renewal does not spend it"
            );
            assert_eq!(server.grants.load(Ordering::SeqCst), 1);
        }

        #[tokio::test]
        async fn a_codex_seat_rejected_twice_reads_sign_in_required() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let acme = store
                .add_organization("Acme", &["example.invalid".into()])
                .unwrap();
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let account = store.snapshot().unwrap().accounts.remove(0);
            assert_eq!(
                store.snapshot().unwrap().attribution(&account).organization,
                Some(acme.id)
            );
            let state = RefreshState::default();
            let gate = gate(&server);
            assert_eq!(
                run(&store, &state, &c, &gate).await.unwrap_err(),
                switchboard_proxy::REJECTED
            );
            assert!(
                state.sign_in_required(&store).is_empty(),
                "one 401 is not a verdict"
            );
            state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            let (result, events) =
                crate::oplog::capture_async(run(&store, &state, &c, &gate)).await;
            assert_eq!(result.unwrap_err(), SIGN_IN);
            assert_eq!(state.sign_in_required(&store), vec![c.clone()]);
            assert_eq!(
                server.grants.load(Ordering::SeqCst),
                0,
                "Codex is never renewed"
            );
            assert!(events.contains(&(
                "seat_ended",
                vec![
                    ("provider", crate::oplog::Field::Code("codex")),
                    ("by", crate::oplog::Field::Code("usage")),
                    ("organization", crate::oplog::Field::Flag(true)),
                ]
            )));
            let probes = server.probes.load(Ordering::SeqCst);
            assert_eq!(run(&store, &state, &c, &gate).await.unwrap_err(), SIGN_IN);
            assert_eq!(
                server.probes.load(Ordering::SeqCst),
                probes,
                "not asked again"
            );
        }

        #[tokio::test]
        async fn a_single_codex_401_is_not_a_verdict() {
            // 401, then an answer that is not a 401, then 401 again: not two in a row.
            let server = server(&[401, 200, 401], renewed()).await;
            let (_root, store) = store();
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let state = RefreshState::default();
            let gate = gate(&server);
            let _ = run(&store, &state, &c, &gate).await;
            state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            let _ = run(&store, &state, &c, &gate).await;
            state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            assert_eq!(
                run(&store, &state, &c, &gate).await.unwrap_err(),
                switchboard_proxy::REJECTED
            );
            assert_eq!(server.probes.load(Ordering::SeqCst), 3);
            assert!(state.sign_in_required(&store).is_empty());
            assert!(!state.is_dead(&store, &c));
        }

        #[tokio::test]
        async fn two_quick_codex_401s_are_not_two_scheduled_checks() {
            // A person pressing *Check* twice within a minute is not two scheduled checks.
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let state = RefreshState::default();
            let gate = gate(&server);
            let _ = run(&store, &state, &c, &gate).await;
            let _ = run(&store, &state, &c, &gate).await;
            assert_eq!(server.probes.load(Ordering::SeqCst), 2);
            assert!(state.sign_in_required(&store).is_empty());
            // The first 401 still counts: a third one a minute after it is the verdict.
            state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            assert_eq!(run(&store, &state, &c, &gate).await.unwrap_err(), SIGN_IN);
            assert_eq!(state.sign_in_required(&store), vec![c]);
        }

        #[tokio::test]
        async fn a_403_is_never_a_verdict() {
            let server = server(&[403], renewed()).await;
            let (_root, store) = store();
            let b = claude_seat(&store);
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let state = RefreshState::at(format!("{}/token", server.base));
            let gate = gate(&server);
            for _ in 0..3 {
                for id in [&b, &c] {
                    let error = run(&store, &state, id, &gate).await.unwrap_err();
                    assert_ne!(error, SIGN_IN);
                }
                state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            }
            assert_eq!(
                server.grants.load(Ordering::SeqCst),
                0,
                "a 403 renews nothing"
            );
            assert!(state.sign_in_required(&store).is_empty());
        }

        #[tokio::test]
        async fn a_new_credential_clears_the_rejection() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let b = claude_seat(&store);
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let state = RefreshState::at(format!("{}/token", server.base));
            let gate = gate(&server);
            assert_eq!(run(&store, &state, &b, &gate).await.unwrap_err(), SIGN_IN);
            let _ = run(&store, &state, &c, &gate).await;
            state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            assert_eq!(run(&store, &state, &c, &gate).await.unwrap_err(), SIGN_IN);
            let mut both = state.sign_in_required(&store);
            both.sort();
            let mut expected = vec![b.clone(), c.clone()];
            expected.sort();
            assert_eq!(both, expected);
            // *Sign in again*: a new credential for each.
            let mut again = credential("synthetic-b");
            again.refresh_token = Some("synthetic-b-new-login".into());
            store
                .upsert(
                    "synthetic-b".into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    again,
                    Some(identity("synthetic-b")),
                )
                .unwrap();
            assert_eq!(state.sign_in_required(&store), vec![c.clone()]);
            assert!(!state.is_dead(&store, &b));
            codex_seat(&store, "synthetic-c-new-login", now() + 86_400);
            assert!(state.sign_in_required(&store).is_empty());
            assert!(!state.is_dead(&store, &c));
        }

        /// Adversarial: a 401 the provider took back (a blip at the edge) is answered by the
        /// renewal path as today; only a 401 on the fresh token is a verdict, and a 5xx or a
        /// renewal that did not happen never is.
        #[tokio::test]
        async fn a_transient_401_or_a_5xx_never_ends_a_claude_seat() {
            for (usage, token) in [
                (vec![401, 200], renewed()),
                (vec![401, 503], renewed()),
                (vec![401], (500, json!({"error":"server_error"}))),
                (vec![503], renewed()),
            ] {
                let server = server(&usage, token).await;
                let (_root, store) = store();
                let b = claude_seat(&store);
                let state = RefreshState::at(format!("{}/token", server.base));
                let gate = gate(&server);
                let first = run(&store, &state, &b, &gate).await;
                assert_ne!(
                    first.as_ref().err().map(String::as_str),
                    Some(SIGN_IN),
                    "{usage:?}"
                );
                assert!(state.sign_in_required(&store).is_empty(), "{usage:?}");
                assert!(!state.is_dead(&store, &b), "{usage:?}");
            }
        }

        /// Adversarial: a Codex token that only expired here (never renewed while inactive) is
        /// refused before the network; that is no answer from the provider, never a verdict.
        #[tokio::test]
        async fn an_expired_codex_token_is_not_a_removed_seat() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let c = codex_seat(&store, "synthetic-c-token", now() - 60);
            let state = RefreshState::default();
            let gate = gate(&server);
            for _ in 0..3 {
                assert_eq!(
                    run(&store, &state, &c, &gate).await.unwrap_err(),
                    switchboard_proxy::REJECTED
                );
                state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            }
            assert_eq!(
                server.probes.load(Ordering::SeqCst),
                0,
                "nothing reached the provider"
            );
            assert!(state.sign_in_required(&store).is_empty());
        }

        /// The ordinary Claude Code's own account is never renewed here, so its 401 never meets
        /// a fresh renewal: never a verdict.
        #[tokio::test]
        async fn the_account_claude_code_uses_is_never_marked_ended() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let a = save(&store, "synthetic-a", "default").id;
            let state = RefreshState::at(format!("{}/token", server.base));
            let gate = gate(&server);
            for _ in 0..3 {
                assert_eq!(
                    run(&store, &state, &a, &gate).await.unwrap_err(),
                    switchboard_proxy::REJECTED
                );
            }
            assert_eq!(server.grants.load(Ordering::SeqCst), 0);
            assert!(state.sign_in_required(&store).is_empty());
        }

        /// B-5 fix cycle 1: a false verdict has a way back. A person's *Check* (the owner's
        /// Usage operation) asks the provider once past it, and any answer other than a 401
        /// lifts it; the background pass alone still asks nothing.
        #[tokio::test]
        async fn a_person_check_lifts_a_verdict_the_provider_takes_back() {
            for provider in [Provider::Claude, Provider::Codex] {
                let server = server(&[401, 401, 200], renewed()).await;
                let (_root, store) = store();
                let id = match provider {
                    Provider::Claude => claude_seat(&store),
                    _ => codex_seat(&store, "synthetic-c-token", now() + 86_400),
                };
                let state = RefreshState::at(format!("{}/token", server.base));
                let gate = gate(&server);
                let _ = run(&store, &state, &id, &gate).await;
                state.age_first_rejection_for_test(&id, REJECTION_SPACING);
                let _ = run(&store, &state, &id, &gate).await;
                assert_eq!(
                    state.sign_in_required(&store),
                    vec![id.clone()],
                    "{provider:?}"
                );
                let probes = server.probes.load(Ordering::SeqCst);
                assert_eq!(run(&store, &state, &id, &gate).await.unwrap_err(), SIGN_IN);
                assert_eq!(
                    server.probes.load(Ordering::SeqCst),
                    probes,
                    "the background pass still asks nothing ({provider:?})"
                );
                let lock = tokio::sync::Mutex::new(());
                let usage = check_by_person(&store, NATIVE, &state, &id, Some(&lock), &gate).await;
                assert!(usage.is_ok(), "{provider:?}: {usage:?}");
                assert_eq!(server.probes.load(Ordering::SeqCst), probes + 1);
                assert!(state.sign_in_required(&store).is_empty(), "{provider:?}");
                assert!(!state.is_dead(&store, &id), "{provider:?}");
            }
        }

        /// B-5 fix cycle 1: a person's check answered 401 keeps the verdict and spends nothing.
        #[tokio::test]
        async fn a_person_check_answered_401_keeps_the_verdict() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let b = claude_seat(&store);
            let state = RefreshState::at(format!("{}/token", server.base));
            let gate = gate(&server);
            assert_eq!(run(&store, &state, &b, &gate).await.unwrap_err(), SIGN_IN);
            let (grants, probes) = (
                server.grants.load(Ordering::SeqCst),
                server.probes.load(Ordering::SeqCst),
            );
            let lock = tokio::sync::Mutex::new(());
            assert_eq!(
                check_by_person(&store, NATIVE, &state, &b, Some(&lock), &gate)
                    .await
                    .unwrap_err(),
                SIGN_IN
            );
            assert_eq!(server.probes.load(Ordering::SeqCst), probes + 1);
            assert_eq!(server.grants.load(Ordering::SeqCst), grants, "no renewal");
            assert_eq!(state.sign_in_required(&store), vec![b]);
        }

        /// B-5 fix cycle 1 (clock skew): a Codex 401 within five minutes of the token's local
        /// expiry may be a clock that runs ahead here; it is no verdict.
        #[tokio::test]
        async fn a_codex_401_near_its_local_expiry_is_no_verdict() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let c = codex_seat(&store, "synthetic-c-token", now() + 30);
            let state = RefreshState::default();
            let gate = gate(&server);
            for _ in 0..2 {
                assert_eq!(
                    run(&store, &state, &c, &gate).await.unwrap_err(),
                    switchboard_proxy::REJECTED
                );
                state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            }
            assert_eq!(
                server.probes.load(Ordering::SeqCst),
                2,
                "the provider answered"
            );
            assert!(state.sign_in_required(&store).is_empty());
        }

        /// The ordinary Codex CLI is signed in as `synthetic-c`.
        fn codex_signed_in(provider: Provider) -> Result<external::CapturedProfile, String> {
            match provider {
                Provider::Codex => Ok(external::CapturedProfile {
                    provider,
                    kind: AuthKind::OAuth,
                    credential: codex_credential("synthetic-c-token", 4_000_000_000),
                    identity: identity("synthetic-c"),
                    label: "synthetic-c".into(),
                }),
                _ => signed_in(provider),
            }
        }

        /// B-5 fix cycle 1: the account the ordinary Codex CLI is signed in to is exempt, as
        /// Claude Code's is: its 401s are the CLI's business, never a verdict here.
        #[tokio::test]
        async fn the_account_codex_uses_is_never_marked_ended() {
            let server = server(&[401], renewed()).await;
            let (_root, store) = store();
            let c = codex_seat(&store, "synthetic-c-token", now() + 86_400);
            let state = RefreshState::default();
            sync_live_sources(&store, codex_signed_in, &state);
            let gate = gate(&server);
            for _ in 0..2 {
                assert_eq!(
                    run(&store, &state, &c, &gate).await.unwrap_err(),
                    switchboard_proxy::REJECTED
                );
                state.age_first_rejection_for_test(&c, REJECTION_SPACING);
            }
            assert_eq!(server.probes.load(Ordering::SeqCst), 2);
            assert!(state.sign_in_required(&store).is_empty());
        }
    }
}
