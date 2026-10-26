//! Automatic fallback (SB-73, packet XA-01; operator decision D-1, 2026-10-06): when the executor
//! of an open Observatory workflow has hit its limit, the workflow is handed to the next usable
//! agent of the operator's chain for it (SB-71) and that agent's session is launched — the same
//! offer and launch as `switchboard continue`, so every refusal of the manual path applies.
//!
//! Nothing happens without a chain: none is applied until the operator sets one. One offer per
//! workflow per window; the old session is never stopped; the engine's own rules (the executor
//! silent for 120 s, one offer a minute, one executor by lease) stay the authority.
//!
//! Phase 1 hands over to Claude Code and Codex; SB-99 adds Kimi Code on a saved Kimi Code account,
//! both ways — a Kimi executor at its limit is handed on, and a Kimi step takes a workflow; SB-107
//! adds Hermes Agent on the saved OpenRouter key (its own credit ceiling). Another agent, a step
//! pinned to a project's paid key, or Kimi Code on a paid key is skipped with its reason until
//! its launch recipe and the project grant it needs exist (SB-96, SB-73 phase 2).
//!
//! A paid step passes the spend gate first (SB-72, iteration 0.7.0 C-5, [`spend_gate`]): its key's
//! spend is read from OpenRouter only when such a step is reached, and a key nearly spent, a
//! spending ceiling reached, or a covering ceiling whose spend cannot be read skips it.
use crate::{continuation, oplog, Operation, Runtime};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use switchboard_core::{provider_agent, Account, Chain, Provider, Snapshot};

/// At most one scan of Observatory's workflows a minute, and at most this many workflows a pass.
const SCAN_SECONDS: i64 = 60;
const WORKFLOWS_PER_PASS: usize = 4;
/// A workflow handed over (or tried) is left alone this long: an offer lives about this long, and
/// a failed attempt is not repeated every pass.
const RETRY_SECONDS: i64 = 15 * 60;
/// A scan that found nothing to hand over waits this long before the next one, so a limited
/// account with no open workflow does not start the engine every minute (LC-08).
const IDLE_SCAN_SECONDS: i64 = 5 * 60;
/// The monitor waits this long for one engine read; a person running `switchboard continue`
/// waits the engine's full minute.
const MONITOR_ENGINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
pub(crate) struct FallbackState {
    next_scan: Mutex<i64>,
    tried: Mutex<HashMap<String, i64>>,
}

/// One step of a chain resolved to an account, or the reason it was passed over.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Run { agent: String, account_id: String },
    Skip { agent: String, reason: &'static str },
}

fn provider_of_agent(agent: &str) -> Option<Provider> {
    [Provider::Claude, Provider::Codex]
        .into_iter()
        .find(|p| provider_agent(*p) == agent)
}

/// Lower is better: the account's highest window use; an account with no observation goes last.
fn used(account: &Account) -> f64 {
    account.usage.as_ref().map_or(1000.0, |u| u.used_percent)
}

/// The saved Kimi Code accounts a chain may run on (SB-99): their ids, whether one may take a
/// workflow now (signed in, not limited, no `kimi` running in it), and its highest window use.
pub(crate) struct KimiChoice<'a> {
    pub ids: Vec<String>,
    pub usable: &'a (dyn Fn(&str) -> bool + Sync),
    pub used: &'a (dyn Fn(&str) -> f64 + Sync),
    /// Whether a `hermes` step can be tried now (SB-107): Hermes installed, an OpenRouter key
    /// saved, the `switchboard` command present for the session script. Its model is settled at
    /// the hand-over, which refuses before the offer when there is none.
    pub hermes_ready: bool,
    /// Whether an `opencode` step can be tried now (SB-119): OpenCode installed, an OpenRouter
    /// key and model saved, the `switchboard` command present.
    pub opencode_ready: bool,
    /// Agents the person disabled for taking over work on the Agents page (SB-120): skipped.
    pub disabled: HashSet<String>,
}

/// Resolves the chain in order, never to the executor's own account, a disabled one, one with a
/// recent limit, or one that needs a new sign-in — nor to another saved row of the same identity
/// as the executor's or a limited account (one login saved twice shares one limit, SB-62). A
/// pinned account is used only when it is usable. Without saved Kimi Code accounts; the pass uses
/// `resolve_with`.
#[cfg(test)]
pub(crate) fn resolve(
    snapshot: &Snapshot,
    chain: &Chain,
    executor_account: Option<&str>,
    limited: &HashSet<String>,
    sign_in: &HashSet<String>,
    folder: Option<&std::path::Path>,
) -> Vec<Step> {
    let none = KimiChoice {
        ids: Vec::new(),
        usable: &|_| false,
        used: &|_| 0.0,
        hermes_ready: false,
        opencode_ready: false,
        disabled: HashSet::new(),
    };
    resolve_with(
        snapshot,
        chain,
        executor_account,
        limited,
        sign_in,
        folder,
        &none,
    )
}

/// The chain resolved in order under `resolve`'s rules, with the saved Kimi Code accounts: a
/// `kimi-code` step runs on its pinned Kimi account
/// when that one is usable, else on the usable one with the lowest window use (SB-99).
pub(crate) fn resolve_with(
    snapshot: &Snapshot,
    chain: &Chain,
    executor_account: Option<&str>,
    limited: &HashSet<String>,
    sign_in: &HashSet<String>,
    folder: Option<&std::path::Path>,
    kimi: &KimiChoice,
) -> Vec<Step> {
    let kimi_usable =
        |id: &str| Some(id) != executor_account && !limited.contains(id) && (kimi.usable)(id);
    let spent: HashSet<(Provider, &str)> = snapshot
        .accounts
        .iter()
        .filter(|a| Some(a.id.as_str()) == executor_account || limited.contains(&a.id))
        .filter_map(|a| a.identity.as_deref().map(|i| (a.provider, i)))
        .collect();
    // SB-102: a project's reserved account serves only its folders, and an account outside a
    // project never serves a project's folder — decided here, before any renewal or launch.
    let usable = |a: &Account| {
        folder.is_none_or(|f| crate::launch::project_allows(snapshot, a, f).is_ok())
            && a.enabled
            && Some(a.id.as_str()) != executor_account
            && !limited.contains(&a.id)
            && !sign_in.contains(&a.id)
            && !a
                .identity
                .as_deref()
                .is_some_and(|i| spent.contains(&(a.provider, i)))
    };
    chain
        .executors
        .iter()
        .map(|e| {
            // SB-120: an agent the person disabled for taking over work is never handed one.
            if kimi.disabled.contains(&e.agent) {
                return Step::Skip {
                    agent: e.agent.clone(),
                    reason: "agent_disabled",
                };
            }
            let kimi_pin = e
                .account_id
                .as_deref()
                .filter(|id| kimi.ids.iter().any(|k| k == id));
            if e.agent == "kimi-code"
                && e.key.is_none()
                && (kimi_pin.is_some() || e.account_id.is_none())
            {
                let chosen = match kimi_pin {
                    Some(id) => kimi_usable(id).then(|| id.to_owned()),
                    None => kimi
                        .ids
                        .iter()
                        .filter(|id| kimi_usable(id))
                        .min_by(|a, b| (kimi.used)(a).total_cmp(&(kimi.used)(b)))
                        .cloned(),
                };
                return match chosen {
                    Some(account_id) => Step::Run {
                        agent: e.agent.clone(),
                        account_id,
                    },
                    None => Step::Skip {
                        agent: e.agent.clone(),
                        reason: "no_usable_account",
                    },
                };
            }
            // SB-119: OpenCode runs on the saved OpenRouter key and its default model.
            if e.agent == crate::launch::OPENCODE_TARGET {
                return if e.account_id.is_some() || e.key.is_some() {
                    Step::Skip {
                        agent: e.agent.clone(),
                        reason: "agent_not_automatic_yet",
                    }
                } else if kimi.opencode_ready
                    && executor_account != Some(crate::launch::OPENCODE_TARGET)
                {
                    Step::Run {
                        agent: e.agent.clone(),
                        account_id: crate::launch::OPENCODE_TARGET.into(),
                    }
                } else {
                    Step::Skip {
                        agent: e.agent.clone(),
                        reason: "no_usable_account",
                    }
                };
            }
            // SB-107: Hermes runs on the saved OpenRouter key; a step that pins an account or a
            // project's key waits for SB-96.
            if e.agent == crate::hermes::HERMES_TARGET {
                return if e.account_id.is_some() || e.key.is_some() {
                    Step::Skip {
                        agent: e.agent.clone(),
                        reason: "agent_not_automatic_yet",
                    }
                } else if kimi.hermes_ready
                    && executor_account != Some(crate::hermes::HERMES_TARGET)
                {
                    Step::Run {
                        agent: e.agent.clone(),
                        account_id: crate::hermes::HERMES_TARGET.into(),
                    }
                } else {
                    Step::Skip {
                        agent: e.agent.clone(),
                        reason: "no_usable_account",
                    }
                };
            }
            let Some(provider) = provider_of_agent(&e.agent) else {
                return Step::Skip {
                    agent: e.agent.clone(),
                    reason: "agent_not_automatic_yet",
                };
            };
            let account = match &e.account_id {
                Some(id) => snapshot
                    .accounts
                    .iter()
                    .find(|a| &a.id == id && a.provider == provider)
                    .filter(|a| usable(a))
                    .cloned(),
                None => snapshot
                    .accounts
                    .iter()
                    .filter(|a| a.provider == provider && usable(a))
                    .min_by(|a, b| used(a).total_cmp(&used(b)))
                    .cloned(),
            };
            match account {
                Some(a) => Step::Run {
                    agent: e.agent.clone(),
                    account_id: a.id,
                },
                None => Step::Skip {
                    agent: e.agent.clone(),
                    reason: "no_usable_account",
                },
            }
        })
        .collect()
}

/// Whether the workflow's executor has hit its limit. When the engine names the executor's
/// account, only that account decides (an account Switchboard does not hold decides nothing).
/// When it names none, the executor is taken to be the ordinary CLI of its provider — but only
/// while no session Switchboard launched for that provider runs (`launched`), since such a
/// session could be the executor on a healthy account.
pub(crate) fn executor_limited(
    executor: &Value,
    snapshot: &Snapshot,
    current: &Value,
    limited: &HashSet<String>,
    launched: &dyn Fn(Provider) -> bool,
) -> Option<(Provider, Option<String>)> {
    let provider =
        continuation::executor_provider(executor.get("provider").and_then(Value::as_str)?)?;
    if let Some(reference) = executor.get("accountRef").and_then(Value::as_str) {
        return snapshot
            .accounts
            .iter()
            .find(|a| continuation::names_account(reference, &a.id))
            .filter(|a| limited.contains(&a.id))
            .map(|a| (provider, Some(a.id.clone())));
    }
    if launched(provider) {
        return None;
    }
    let key = match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    let ids: Vec<String> = current[key]["account_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    ids.iter()
        .find(|id| limited.contains(*id))
        .map(|id| (provider, Some(id.clone())))
}

/// How long a workflow whose executor names no account may have been silent and still be taken
/// for the ordinary CLI's work (SB-129): an executor that is working writes a checkpoint after
/// every step, so one silent longer than this is not the session that just hit the limit.
pub(crate) const UNNAMED_EXECUTOR_SILENCE: i64 = 3600;
/// A workflow whose executor names no account and has been silent too long (or says nothing of
/// its silence) — the ordinary CLI's limit is not its own (SB-129, live 2026-10-14: a scratch
/// workflow silent for two hours was handed over twice).
pub(crate) fn unnamed_and_stale(workflow: &Value, executor: &Value) -> bool {
    if executor.get("accountRef").and_then(Value::as_str).is_some() {
        return false;
    }
    workflow
        .get("silentSeconds")
        .and_then(Value::as_i64)
        .is_none_or(|s| s > UNNAMED_EXECUTOR_SILENCE)
}

/// A workflow run by Kimi Code on a saved Kimi account that the background view finds limited
/// (SB-99). Only `accountRef` decides: the ordinary `kimi` is shown, not watched.
pub(crate) fn kimi_executor_limited(
    executor: &Value,
    kimi_ids: &[String],
    kimi_limited: &HashSet<String>,
) -> Option<String> {
    let name = executor.get("provider").and_then(Value::as_str)?;
    let reference = executor.get("accountRef").and_then(Value::as_str)?;
    if !continuation::executor_is_kimi(name) {
        return None;
    }
    kimi_ids
        .iter()
        .find(|k| continuation::names_account(reference, k))
        .filter(|k| kimi_limited.contains(*k))
        .cloned()
}

impl FallbackState {
    fn due(&self, now: i64) -> bool {
        let Ok(mut next) = self.next_scan.lock() else {
            return false;
        };
        if now < *next {
            return false;
        }
        *next = now + SCAN_SECONDS;
        true
    }
    /// Nothing to hand over this pass: wait longer before starting the engine again.
    fn idle(&self, now: i64) {
        if let Ok(mut next) = self.next_scan.lock() {
            *next = now + IDLE_SCAN_SECONDS;
        }
    }
    fn recently_tried(&self, workflow: &str, now: i64) -> bool {
        self.tried
            .lock()
            .is_ok_and(|t| t.get(workflow).is_some_and(|at| now - at < RETRY_SECONDS))
    }
    fn mark(&self, workflow: &str, now: i64) {
        if let Ok(mut t) = self.tried.lock() {
            t.retain(|_, at| now - *at < RETRY_SECONDS);
            t.insert(workflow.to_owned(), now);
        }
    }
}

fn log(outcome: &'static str) {
    oplog::event("fallback", &[("outcome", oplog::Field::Code(outcome))]);
}

/// One monitor pass: find a stalled workflow whose executor hit its limit and hand it to the next
/// usable agent of its chain. Quiet and cheap when no chain is set or nothing is limited.
pub(crate) async fn pass(runtime: &Runtime, now: i64) {
    let Ok(bin) = continuation::observatory() else {
        return;
    };
    // SB-108: with "Open hand-overs in Fabric" on, the session is prepared in place and left
    // for a console to take (`handovers`); the monitor opens it in Terminal after two minutes.
    let in_fabric = crate::handovers::enabled(&runtime.root);
    pass_with(runtime, now, &bin, |workflow, account, dir| async move {
        let answer = runtime
            .execute(Operation::Continue {
                workflow_id: workflow.clone(),
                id: account.clone(),
                mode: "isolated".into(),
                working_directory: dir.clone(),
                in_place: in_fabric,
            })
            .await?;
        if in_fabric {
            let snapshot = runtime.store.snapshot().ok();
            queue_handover(
                &runtime.root,
                snapshot.as_ref(),
                workflow,
                &account,
                dir,
                &answer,
                now,
            )?;
        }
        Ok(())
    })
    .await
}

pub const NO_DEFAULT_ACCOUNT: &str = "The default agent has no account that can take this workflow now. Name one with --account, or choose another default agent.";
/// SB-120: `switchboard continue` without an account, on the machine's default agent: the
/// account a one-step chain of that agent would choose now — never the workflow's executor or
/// another saved copy of its login, a limited one, one that needs a new sign-in, or one a project
/// keeps for other folders. `executor` is the engine's (`continuation::executor_of`); with no
/// `accountRef`, the ordinary CLI's current accounts of its provider stand in for it.
pub(crate) fn account_for_default(
    runtime: &Runtime,
    agent: &str,
    executor: &Value,
    folder: &std::path::Path,
    now: i64,
) -> Result<String, String> {
    let snapshot = runtime.store.snapshot()?;
    let kimi_limited = runtime.kimi_watch.limited(now);
    let mut limited = runtime.limits.limited_ids(now);
    limited.extend(kimi_limited.iter().cloned());
    // Without an accountRef the ordinary CLI's accounts are the executor: an unreadable answer
    // refuses rather than risk choosing the account that just ran out.
    let current = if executor.get("accountRef").is_some() {
        Value::Null
    } else {
        runtime.current_accounts()?
    };
    let excluded = executor_accounts(executor, &snapshot, &current);
    let kimi_usable =
        |id: &str| crate::kimi::continuation_preflight(&runtime.root, &runtime.store, id).is_ok();
    let kimi_used = |id: &str| runtime.kimi_watch.used(id);
    let kimi = KimiChoice {
        ids: snapshot
            .kimi_accounts
            .iter()
            .map(|k| k.id.clone())
            .collect(),
        usable: &kimi_usable,
        used: &kimi_used,
        hermes_ready: false,
        opencode_ready: false,
        disabled: HashSet::new(),
    };
    let sign_in: HashSet<String> = runtime
        .refresh
        .sign_in_required(&runtime.store)
        .into_iter()
        .collect();
    pick_for_default(
        &snapshot, agent, &excluded, &limited, &sign_in, folder, &kimi,
    )
}

/// The accounts that are the workflow's executor: the one its `accountRef` names (a saved
/// account or Kimi Code account), else the ordinary CLI's current accounts of its provider.
pub(crate) fn executor_accounts(
    executor: &Value,
    snapshot: &Snapshot,
    current: &Value,
) -> Vec<String> {
    if let Some(reference) = executor.get("accountRef").and_then(Value::as_str) {
        return snapshot
            .accounts
            .iter()
            .map(|a| &a.id)
            .chain(snapshot.kimi_accounts.iter().map(|k| &k.id))
            .filter(|id| continuation::names_account(reference, id))
            .cloned()
            .collect();
    }
    let key = match executor
        .get("provider")
        .and_then(Value::as_str)
        .and_then(continuation::executor_provider)
    {
        Some(Provider::Claude) => "claude",
        Some(Provider::Codex) => "codex",
        None => return Vec::new(),
    };
    current[key]["account_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// `account_for_default` without the runtime: a one-step chain of `agent`, every executor account
/// treated as the executor (so its other saved copies are passed over too).
pub(crate) fn pick_for_default(
    snapshot: &Snapshot,
    agent: &str,
    executor: &[String],
    limited: &HashSet<String>,
    sign_in: &HashSet<String>,
    folder: &std::path::Path,
    kimi: &KimiChoice,
) -> Result<String, String> {
    let chain = Chain {
        scope: switchboard_core::ChainScope::Machine,
        executors: vec![switchboard_core::Executor {
            agent: agent.to_owned(),
            account_id: None,
            key: None,
        }],
        updated_at: 0,
    };
    // The first executor account is "the executor"; the others count as limited, which also
    // passes over every saved copy of their logins.
    let mut limited = limited.clone();
    limited.extend(executor.iter().skip(1).cloned());
    match resolve_with(
        snapshot,
        &chain,
        executor.first().map(String::as_str),
        &limited,
        sign_in,
        Some(folder),
        kimi,
    )
    .into_iter()
    .next()
    {
        Some(Step::Run { account_id, .. }) => Ok(account_id),
        _ => Err(NO_DEFAULT_ACCOUNT.into()),
    }
}
/// Leaves a prepared continuation (`Continue` with `in_place`) for Fabric's console (SB-108); a
/// queue that cannot take it opens it in Terminal now, while the offer is live.
pub(crate) fn queue_handover(
    root: &std::path::Path,
    snapshot: Option<&Snapshot>,
    workflow: String,
    account: &str,
    dir: PathBuf,
    answer: &Value,
    now: i64,
) -> Result<crate::handovers::Handover, String> {
    let script = answer["script"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| "Offered, but the session script is missing.".to_string())?;
    let (agent, label) = describe(snapshot, account);
    let paid = snapshot.and_then(|s| paid_kind(s, account));
    let handover = crate::handovers::Handover {
        id: crate::handovers::new_id(),
        workflow,
        agent,
        account_label: label,
        dir,
        script: script.clone(),
        offered_until: answer["offeredUntil"].as_str().unwrap_or_default().into(),
        created_at: now,
        paid,
    };
    if crate::handovers::record(root, &handover).is_err() {
        if !crate::handovers::offer_live(&handover, crate::monitor::now()) {
            return Err("The hand-over offer expired before it could be opened.".into());
        }
        // Nowhere to wait: Terminal now, as without Fabric.
        crate::launch::start_terminal(&script)
            .map_err(|e| format!("Offered {}: {e}", handover.workflow))?;
    }
    Ok(handover)
}
/// The agent and label a hand-over shows: Hermes, a saved Kimi Code account, or a saved account.
fn describe(snapshot: Option<&Snapshot>, account: &str) -> (String, String) {
    if account == crate::hermes::HERMES_TARGET {
        return ("hermes".into(), "Hermes Agent".into());
    }
    if account == crate::launch::OPENCODE_TARGET {
        return ("opencode".into(), "OpenCode".into());
    }
    let Some(snapshot) = snapshot else {
        return ("unknown".into(), String::new());
    };
    if let Some(k) = snapshot.kimi_accounts.iter().find(|k| k.id == account) {
        return ("kimi-code".into(), k.label.clone());
    }
    snapshot
        .accounts
        .iter()
        .find(|a| a.id == account)
        .map(|a| (provider_agent(a.provider).to_owned(), a.label.clone()))
        .unwrap_or_else(|| ("unknown".into(), String::new()))
}
/// The group whose chain a workflow checked out in `dir` follows (C-8, dossier C part E): the
/// executor account's group when that group holds the checkout; else the one group holding it,
/// whoever's account worked there (plain `claude` in a group's folder runs on the ordinary
/// sign-in, which stays in the default group); else, with several groups holding it (SB-146) and
/// none the executor's, the group `Snapshot::project_for` names, the one the folder's proxy route
/// and paid key use, and `true` says the choice was ambiguous; with no group holding it, the
/// executor account's group as before.
pub(crate) fn chain_pool(
    snapshot: &Snapshot,
    executor_account: Option<&str>,
    dir: &std::path::Path,
) -> (Option<String>, bool) {
    let own = executor_account.and_then(|a| {
        snapshot
            .accounts
            .iter()
            .find(|x| x.id == a)
            .map(|x| x.pool.clone())
    });
    chain_pool_of(snapshot, own, dir)
}
/// [`chain_pool`] with the executor account's group given (`Operation::Chains`' `pool`).
pub(crate) fn chain_pool_of(
    snapshot: &Snapshot,
    own: Option<String>,
    dir: &std::path::Path,
) -> (Option<String>, bool) {
    let holding = snapshot.projects_for(dir);
    if holding.is_empty()
        || own
            .as_deref()
            .is_some_and(|p| holding.iter().any(|g| g.pool == p))
    {
        return (own, false);
    }
    match holding.as_slice() {
        [only] => (Some(only.pool.clone()), false),
        _ => (snapshot.project_for(dir).map(|p| p.pool.clone()), true),
    }
}
/// The pass with the hand-over as a parameter: the monitor passes `switchboard continue`'s own
/// operation, a test a recorder. A paid step's spend is read from OpenRouter (C-5).
pub(crate) async fn pass_with<F, Fut>(runtime: &Runtime, now: i64, bin: &std::path::Path, act: F)
where
    F: Fn(String, String, PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let store = runtime.store.clone();
    let read = move |pool: Option<String>| {
        let store = store.clone();
        async move { crate::agent_keys::spend_read(&store, pool.as_deref()).await }
    };
    pass_with_reads(runtime, now, bin, act, read, GATE_BUDGET).await
}
/// [`pass_with`] with the spend read and the gates' budget as parameters too, so a test never
/// reaches OpenRouter and never waits the whole [`GATE_BUDGET`].
pub(crate) async fn pass_with_reads<F, Fut, R, RFut>(
    runtime: &Runtime,
    now: i64,
    bin: &std::path::Path,
    act: F,
    read: R,
    budget: std::time::Duration,
) where
    F: Fn(String, String, PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
    R: Fn(Option<String>) -> RFut,
    RFut: std::future::Future<Output = Option<crate::spend::KeyRead>> + Send + 'static,
{
    let Ok(snapshot) = runtime.store.snapshot() else {
        return;
    };
    if snapshot.chains.is_empty() {
        return;
    }
    let kimi_limited = runtime.kimi_watch.limited(now);
    let mut limited = runtime.limits.limited_ids(now);
    if (limited.is_empty() && kimi_limited.is_empty()) || !runtime.fallback.due(now) {
        return;
    }
    // A limited Kimi account is never a chain's next step either (SB-99).
    limited.extend(kimi_limited.iter().cloned());
    let kimi_ids: Vec<String> = snapshot
        .kimi_accounts
        .iter()
        .map(|k| k.id.clone())
        .collect();
    // SB-120: an unreadable agents-settings.json hands nothing over — never "nothing disabled".
    let Ok(settings) = crate::agent_settings::read(&runtime.root) else {
        log("agent_settings_unreadable");
        return;
    };
    let disabled: HashSet<String> = settings.disabled.into_iter().collect();
    let wants = |agent: &str| {
        snapshot
            .chains
            .iter()
            .any(|c| c.executors.iter().any(|e| e.agent == agent))
    };
    let kimi_usable =
        |id: &str| crate::kimi::continuation_preflight(&runtime.root, &runtime.store, id).is_ok();
    let kimi_used = |id: &str| runtime.kimi_watch.used(id);
    let kimi = KimiChoice {
        ids: kimi_ids.clone(),
        usable: &kimi_usable,
        used: &kimi_used,
        // The OpenRouter key is read only for a chain that has the agent (the idle budget).
        hermes_ready: wants(crate::hermes::HERMES_TARGET)
            && (test_hermes_ready() || crate::launch::hermes_continuation_ready(&runtime.store)),
        opencode_ready: wants(crate::launch::OPENCODE_TARGET)
            && crate::launch::opencode_continuation_ready(&runtime.store),
        disabled,
    };
    // The engine is a child process with a deadline: off the async workers, so a slow engine
    // never holds the monitor's thread.
    let listed = {
        let bin = bin.to_path_buf();
        tokio::task::spawn_blocking(move || {
            continuation::list_open_within(&bin, MONITOR_ENGINE_TIMEOUT)
        })
        .await
    };
    let workflows = match listed {
        Ok(Ok(w)) => w,
        _ => {
            log("engine_unreadable");
            runtime.fallback.idle(now);
            return;
        }
    };
    let current = runtime.current_accounts().unwrap_or(Value::Null);
    let sign_in: HashSet<String> = runtime
        .refresh
        .sign_in_required(&runtime.store)
        .into_iter()
        .collect();
    let launched = |provider: Provider| {
        let ids: Vec<String> = snapshot
            .accounts
            .iter()
            .filter(|a| a.provider == provider)
            .map(|a| a.id.clone())
            .collect();
        crate::launch::launched_session_running(&runtime.root, provider, &ids)
    };
    // The pass's gates together stay inside the budget: it starts once, before the first
    // workflow, so up to WORKFLOWS_PER_PASS hand-overs (and their waits for the hand-over turn)
    // share it.
    let budget_end = tokio::time::Instant::now() + budget;
    let mut examined = 0;
    for workflow in &workflows {
        if examined >= WORKFLOWS_PER_PASS {
            break;
        }
        let Some(id) = workflow.get("workflowId").and_then(Value::as_str) else {
            continue;
        };
        if workflow.get("pendingHandoff").is_some_and(|h| !h.is_null())
            || runtime.fallback.recently_tried(id, now)
        {
            continue;
        }
        let executor = workflow
            .pointer("/lease/executor")
            .cloned()
            .unwrap_or(Value::Null);
        // SB-129: an executor named by no account is taken to be the ordinary CLI only while the
        // workflow is being worked — it wrote within the hour.
        if unnamed_and_stale(workflow, &executor) {
            continue;
        }
        let executor_account = match kimi_executor_limited(&executor, &kimi_ids, &kimi_limited) {
            Some(id) => Some(id),
            None => match executor_limited(&executor, &snapshot, &current, &limited, &launched) {
                Some((_, account)) => account,
                None => continue,
            },
        };
        examined += 1;
        // C-8: the group's chain is chosen by the checkout below, so the engine is asked for it
        // whenever a chain could apply: the workflow's, the executor group's or the machine's
        // (as before), or any group's.
        let own_pool = executor_account.as_deref().and_then(|a| {
            snapshot
                .accounts
                .iter()
                .find(|x| x.id == a)
                .map(|x| x.pool.clone())
        });
        let may_apply = matches!(
            runtime.store.chain_for(Some(id), own_pool.as_deref()),
            Ok(Some(_))
        ) || snapshot
            .chains
            .iter()
            .any(|c| matches!(c.scope, switchboard_core::ChainScope::Project { .. }));
        if !may_apply {
            continue;
        }
        let shown = {
            let (bin, id) = (bin.to_path_buf(), id.to_owned());
            tokio::task::spawn_blocking(move || {
                continuation::show_within(&bin, &id, MONITOR_ENGINE_TIMEOUT)
            })
            .await
        };
        // A failed read is an attempt like a hand-over: not repeated every pass.
        let Ok(Ok(shown)) = shown else {
            runtime.fallback.mark(id, now);
            log("engine_unreadable");
            continue;
        };
        let Some(dir) = continuation::checkouts(&shown).into_iter().next() else {
            runtime.fallback.mark(id, now);
            log("no_checkout");
            continue;
        };
        let (pool, ambiguous) = chain_pool(&snapshot, executor_account.as_deref(), &dir);
        if ambiguous {
            log("group_ambiguous");
        }
        // No chain for the checkout's group (another group has one): no retry mark, so a chain
        // set meanwhile is used at the next scan.
        let chain = match runtime.store.chain_for(Some(id), pool.as_deref()) {
            Ok(Some(chain)) => chain,
            _ => {
                log("no_chain");
                continue;
            }
        };
        runtime.fallback.mark(id, now);
        let steps = resolve_with(
            &snapshot,
            &chain,
            executor_account.as_deref(),
            &limited,
            &sign_in,
            Some(&dir),
            &kimi,
        );
        let gate = |_agent: String, account: String| {
            let (snapshot, dir, read) = (&snapshot, &dir, &read);
            async move {
                spend_gate(
                    &runtime.root,
                    snapshot,
                    id,
                    dir,
                    &account,
                    now,
                    read,
                    read_limit(budget_end, SPEND_READ_TIMEOUT),
                )
                .await
            }
        };
        let done = hand_over(&act, &gate, id, &dir, &steps).await;
        log(done.outcome);
        record_last(&runtime.root, &snapshot, id, &done, now);
        // The gate's records (a question, a baseline, this hand-over) ride the next backup.
        runtime.store.setting_changed();
        let _ = runtime.store.record(
            "fallback",
            None,
            if done.outcome == "offered" {
                "offered"
            } else {
                "failed"
            },
        );
    }
    if examined == 0 {
        runtime.fallback.idle(now);
    }
}

/// Writes what a hand-over did to spend.json's `last_handover` (C-5): the chains view and the
/// window read it. A file that cannot be written leaves the previous record; the log has it.
fn record_last(
    root: &std::path::Path,
    snapshot: &Snapshot,
    workflow: &str,
    done: &HandOver,
    now: i64,
) {
    let last = crate::spend::LastHandover {
        at: now,
        workflow: workflow.to_owned(),
        outcome: done.outcome.to_owned(),
        skipped: done
            .skipped
            .iter()
            .map(|(agent, reason)| crate::spend::Skipped {
                agent: agent.chars().take(64).collect(),
                reason: (*reason).to_owned(),
            })
            .collect(),
    };
    if crate::spend::record_handover(root, snapshot, last, now).is_err() {
        oplog::event("spend_gate", &[("record", oplog::Field::Code("unwritten"))]);
    }
}

#[cfg(test)]
tokio::task_local! {
    /// A test's Hermes Agent, ready for a chain step without an installed `hermes`.
    pub(crate) static TEST_HERMES_READY: ();
}
fn test_hermes_ready() -> bool {
    #[cfg(test)]
    return TEST_HERMES_READY.try_with(|_| ()).is_ok();
    #[cfg(not(test))]
    false
}

/// One paid start at a time, its gate and launch together (C-5, C-Q8): two never both start on
/// the last dollar under a ceiling. The automatic fallback holds it across a whole hand-over; a
/// start by hand (`Runtime::execute`) takes it before the owner transaction, the same order.
pub(crate) static HAND_OVER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

tokio::task_local! {
    /// Set while the automatic fallback's hand-over runs its step: that step holds
    /// [`HAND_OVER`] and passed its gate, so the `Continue` it runs takes neither again
    /// (a tokio mutex is not reentrant). Task-local, so no request from outside can claim it.
    static GATED: ();
}

/// Runs `work` as a step that holds [`HAND_OVER`] and passed its gate: a start by hand that
/// `Runtime::execute` gated before the owner transaction, so the operation's own gate is not run
/// again.
pub(crate) async fn as_gated<F: std::future::Future>(work: F) -> F::Output {
    GATED.scope((), work).await
}

/// Whether this task is a hand-over step that already holds [`HAND_OVER`] and passed its gate.
pub(crate) fn gated() -> bool {
    GATED.try_with(|_| ()).is_ok()
}

/// How long one pass's spend gates may take together, across all its workflows (at most
/// `WORKFLOWS_PER_PASS`): the budget starts once, before the pass's first workflow, and each read
/// gets what is left of it (at most [`SPEND_READ_TIMEOUT`]), so slow paid steps never hold a pass
/// longer. A read cut short is unknown, never zero. A start by hand has a budget of its own.
pub(crate) const GATE_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// The read limit for a gate that starts now: at most `cap`, inside a budget that ends at `end`.
pub(crate) fn read_limit(
    end: tokio::time::Instant,
    cap: std::time::Duration,
) -> std::time::Duration {
    cap.min(end.saturating_duration_since(tokio::time::Instant::now()))
}

/// What the spend gate says about one step about to be handed work (C-5).
#[derive(Debug, PartialEq)]
pub(crate) enum Gate {
    Run,
    Skip(&'static str),
}

/// What one hand-over did: its outcome code and every step passed over with its reason.
#[derive(Debug, PartialEq)]
pub(crate) struct HandOver {
    pub outcome: &'static str,
    pub skipped: Vec<(String, &'static str)>,
}

/// Tries the chain's steps in order, each through `gate` first (the spend gate for a paid step,
/// C-5). A step the gate skips or a refusal before the offer moves on to the next step; once an
/// offer exists (the launch failed after it) nothing else is tried — the offer lapses by itself.
/// With no step tried, the outcome is the first reason the gate gave, else `no_usable_executor`.
async fn hand_over<F, Fut, G, GFut>(
    act: &F,
    gate: &G,
    workflow: &str,
    dir: &std::path::Path,
    steps: &[Step],
) -> HandOver
where
    F: Fn(String, String, PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
    G: Fn(String, String) -> GFut,
    GFut: std::future::Future<Output = Gate>,
{
    // One hand-over at a time, its gate and launch together: two never both start on the last
    // dollar under a ceiling (each sees what the other's launch left behind).
    let _turn = HAND_OVER.lock().await;
    let act = |workflow: String, account: String, dir: PathBuf| {
        GATED.scope((), act(workflow, account, dir))
    };
    let mut tried = false;
    let mut gated: Option<&'static str> = None;
    let mut skipped = Vec::new();
    let done = |outcome, skipped| HandOver { outcome, skipped };
    for step in steps {
        let (agent, account_id) = match step {
            Step::Skip { agent, reason } => {
                skipped.push((agent.clone(), *reason));
                continue;
            }
            Step::Run { agent, account_id } => (agent, account_id),
        };
        if let Gate::Skip(reason) = gate(agent.clone(), account_id.clone()).await {
            skipped.push((agent.clone(), reason));
            gated.get_or_insert(reason);
            continue;
        }
        tried = true;
        match act(workflow.to_owned(), account_id.clone(), dir.to_path_buf()).await {
            Ok(_) => return done("offered", skipped),
            Err(error) if error.starts_with("Offered ") => return done("launch_failed", skipped),
            Err(_) => continue,
        }
    }
    let outcome = match (tried, gated) {
        (true, _) => "every_step_refused",
        (false, Some(reason)) => reason,
        (false, None) => "no_usable_executor",
    };
    done(outcome, skipped)
}

/// How long the gate waits for one key's spend: OpenRouter's own read gives up at 10 s.
const SPEND_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

/// The spend gate for one step (SB-72, dossier C part D). Not paid (a subscription sign-in, Kimi
/// Code): runs, nothing read. Paid: Hermes and OpenCode on the OpenRouter key a launch in `dir`
/// uses (`spend::key_slot`, never the chain's group), and Claude Code or Codex on an API-key
/// account (C-Q3). `read` asks OpenRouter for one saved key's spend by its pool (none: the
/// machine's); it is called only here, for the step's key and, while a machine ceiling is set,
/// for every other saved key at once, each within `limit`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn spend_gate<R, RFut>(
    root: &std::path::Path,
    snapshot: &Snapshot,
    workflow: &str,
    dir: &std::path::Path,
    account_id: &str,
    now: i64,
    read: &R,
    limit: std::time::Duration,
) -> Gate
where
    R: Fn(Option<String>) -> RFut,
    RFut: std::future::Future<Output = Option<crate::spend::KeyRead>> + Send + 'static,
{
    match paid_kind(snapshot, account_id) {
        Some(kind) => paid_gate(root, snapshot, Some(workflow), dir, kind, now, read, limit).await,
        None => Gate::Run,
    }
}

/// What a paid start spends (C-5, C-Q3, C-Q8). A queued hand-over records it (`paid`), so it is
/// gated again when a console or the background takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paid {
    /// The OpenRouter key a launch in its folder uses: Hermes, OpenCode, any agent launched
    /// `--via openrouter`.
    #[serde(rename = "openrouter")]
    OpenRouter,
    /// A Claude Code or Codex API-key account, whose spend Switchboard cannot read.
    ApiKeyAccount,
}

/// Whether a start on `account_id` is paid, and on what: `None` for a subscription sign-in or a
/// Kimi Code account.
pub(crate) fn paid_kind(snapshot: &Snapshot, account_id: &str) -> Option<Paid> {
    if account_id == crate::hermes::HERMES_TARGET || account_id == crate::launch::OPENCODE_TARGET {
        return Some(Paid::OpenRouter);
    }
    snapshot
        .accounts
        .iter()
        .any(|a| {
            a.id == account_id
                && a.kind == switchboard_core::AuthKind::ApiKey
                && matches!(a.provider, Provider::Claude | Provider::Codex)
        })
        .then_some(Paid::ApiKeyAccount)
}

/// The gate for one paid start, with or without a workflow (a launch by hand has none).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn paid_gate<R, RFut>(
    root: &std::path::Path,
    snapshot: &Snapshot,
    workflow: Option<&str>,
    dir: &std::path::Path,
    kind: Paid,
    now: i64,
    read: &R,
    limit: std::time::Duration,
) -> Gate
where
    R: Fn(Option<String>) -> RFut,
    RFut: std::future::Future<Output = Option<crate::spend::KeyRead>> + Send + 'static,
{
    use crate::spend::{self, KeySlot, Verdict};
    use switchboard_core::ChainScope;
    let openrouter = kind == Paid::OpenRouter;
    let api_key_account = kind == Paid::ApiKeyAccount;
    let verdict = |code: &'static str| {
        oplog::event("spend_gate", &[("verdict", oplog::Field::Code(code))]);
    };
    let skip = |code: &'static str| {
        verdict(code);
        Gate::Skip(code)
    };
    // An unreadable spend.json hands nothing paid over: never "no ceilings".
    let Ok(before) = spend::read(root) else {
        return skip("ceilings_unreadable");
    };
    let slot = spend::key_slot(snapshot, Some(dir));
    let key = slot.clone().unwrap_or(KeySlot::Machine);
    let pool_of = |slot: &KeySlot| match slot {
        KeySlot::Machine => None,
        KeySlot::Group(pool) => Some(pool.clone()),
    };
    // Each read runs at once and is given up at `limit`: unknown, never zero.
    let ask = |pool: Option<String>| {
        let read = tokio::time::timeout(limit, read(pool));
        tokio::spawn(async move { read.await.ok().flatten() })
    };
    let others_pools = match &slot {
        Some(slot) if openrouter => crate::agent_keys::other_keys(snapshot, slot),
        _ => Vec::new(),
    };
    let machine_ceiling =
        |f: &spend::SpendFile| f.ceilings.iter().any(|c| c.scope == ChainScope::Machine);
    // An API-key account spends no OpenRouter key: nothing is read for it (C-Q3).
    let own = match &slot {
        Some(slot) if openrouter => Some(ask(pool_of(slot))),
        _ => None,
    };
    let others_asked: Vec<_> = if machine_ceiling(&before) {
        others_pools.iter().cloned().map(ask).collect()
    } else {
        Vec::new()
    };
    let own = match own {
        Some(handle) => handle.await.ok().flatten(),
        None => None,
    };
    let mut others = Vec::new();
    for handle in others_asked {
        others.push(handle.await.ok().flatten());
    }
    // The ceilings as they are now, after the reads: one set meanwhile is obeyed. A machine
    // ceiling set during the reads counts the keys not read as unknown.
    let Ok(file) = spend::read(root) else {
        return skip("ceilings_unreadable");
    };
    if machine_ceiling(&file) && others.len() < others_pools.len() {
        others = vec![None; others_pools.len()];
    }
    let step = spend::PaidStep {
        key: key.clone(),
        workflow,
        api_key_account,
    };
    let reads = spend::Reads {
        key: own.clone(),
        others,
    };
    match spend::decide(&file, snapshot, &step, &reads) {
        Verdict::Skip { reason, reached } => {
            if let Some((scope, cents, spent)) = reached {
                let _ = spend::open_question(root, snapshot, &scope, cents, spent, now);
            }
            return skip(reason);
        }
        Verdict::RunUnread => verdict("spend_unread"),
        Verdict::Run => verdict("run"),
    }
    // A task's first paid hand-over on the OpenRouter key records what its ceiling counts from,
    // before the launch; a total it could not read records a marker, so a task ceiling set
    // later reads its spend as unknown, never from zero.
    if let (Some(workflow), true) = (workflow, openrouter) {
        let task_ceiling = file
            .ceilings
            .iter()
            .any(|c| matches!(&c.scope, ChainScope::Workflow { id } if id == workflow));
        let usage = own.and_then(|r| r.usage);
        if spend::record_baseline(root, snapshot, workflow, &key, usage, now).is_err()
            && task_ceiling
        {
            return skip("spend_unknown");
        }
    }
    Gate::Run
}

/// A spend read handed to the gate: OpenRouter's, or a test's (`TEST_READ`).
type ReadFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<crate::spend::KeyRead>> + Send + 'static>,
>;

#[cfg(test)]
tokio::task_local! {
    /// A test's spend read for a start by hand, in place of OpenRouter's.
    pub(crate) static TEST_READ: std::sync::Arc<
        dyn Fn(Option<String>) -> Option<crate::spend::KeyRead> + Send + Sync,
    >;
}

/// The spend gate for a paid start by hand (C-Q8): `switchboard continue` (also to the console
/// and on the default agent) onto Hermes, OpenCode or an API-key account, `agents launch` on the
/// OpenRouter key or through the proxy on an API-key account, `launch` on an API-key account.
/// Gated like a hand-over, once: a step of the automatic fallback passed its own gate already
/// ([`gated`]). Refused with the gate's reason before anything is offered or started.
pub(crate) async fn gate_by_hand(
    store: &std::sync::Arc<switchboard_core::Store>,
    root: &std::path::Path,
    workflow: Option<&str>,
    dir: &std::path::Path,
    kind: Paid,
) -> Result<(), String> {
    if gated() {
        return Ok(());
    }
    let snapshot = store.snapshot()?;
    #[cfg(test)]
    let test_read = TEST_READ.try_with(|f| f.clone()).ok();
    #[cfg(not(test))]
    let test_read: Option<()> = None;
    let read = |pool: Option<String>| -> ReadFuture {
        #[cfg(test)]
        if let Some(f) = &test_read {
            // Answered when polled, as a read over the network is.
            let f = f.clone();
            return Box::pin(async move { f(pool) });
        }
        #[cfg(not(test))]
        let _ = &test_read;
        let store = store.clone();
        Box::pin(async move { crate::agent_keys::spend_read(&store, pool.as_deref()).await })
    };
    let budget_end = tokio::time::Instant::now() + GATE_BUDGET;
    match paid_gate(
        root,
        &snapshot,
        workflow,
        dir,
        kind,
        crate::monitor::now(),
        &read,
        read_limit(budget_end, SPEND_READ_TIMEOUT),
    )
    .await
    {
        Gate::Run => Ok(()),
        Gate::Skip(reason) => Err(crate::spend::refusal(reason).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use switchboard_core::{AuthKind, ChainScope, Executor, Usage};

    fn account(id: &str, provider: Provider, used_percent: Option<f64>) -> Account {
        Account {
            id: id.into(),
            label: id.into(),
            provider,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: used_percent.map(|u| Usage {
                windows: vec![],
                used_percent: u,
                observed_at: 1,
                resets_at: None,
                source: "probe".into(),
            }),
            external_identity: None,
            usage_health: None,
        }
    }
    fn chain(agents: &[(&str, Option<&str>)]) -> Chain {
        Chain {
            scope: ChainScope::Machine,
            executors: agents
                .iter()
                .map(|(a, id)| Executor {
                    agent: (*a).into(),
                    account_id: id.map(str::to_owned),
                    key: None,
                })
                .collect(),
            updated_at: 1,
        }
    }

    #[test]
    fn a_projects_reserved_account_never_takes_over_work_outside_its_folders() {
        // SB-102: decided while resolving, so no renewal or launch is spent on a refusal.
        let mut reserved = account("claude-reserved", Provider::Claude, Some(1.0));
        reserved.pool = "client-alpha".into();
        let snapshot = Snapshot {
            accounts: vec![
                reserved,
                account("claude-free", Provider::Claude, Some(60.0)),
            ],
            projects: vec![switchboard_core::Project {
                pool: "client-alpha".into(),
                name: "Client Alpha".into(),
                folders: vec!["/work/alpha".into()],
                created_at: 1,
                sources: vec![],
                organization: None,
            }],
            ..Default::default()
        };
        let resolve_in = |folder: &str| {
            resolve(
                &snapshot,
                &chain(&[("claude-code", None)]),
                Some("someone-else"),
                &HashSet::new(),
                &HashSet::new(),
                Some(std::path::Path::new(folder)),
            )
        };
        assert_eq!(
            resolve_in("/work/beta"),
            [Step::Run {
                agent: "claude-code".into(),
                account_id: "claude-free".into()
            }],
            "the least-used account is reserved elsewhere: the free one serves"
        );
        assert_eq!(
            resolve_in("/work/alpha/api"),
            [Step::Run {
                agent: "claude-code".into(),
                account_id: "claude-reserved".into()
            }],
            "inside the project its own account serves"
        );
    }
    #[test]
    fn a_hermes_step_runs_on_the_openrouter_key_after_the_subscriptions_run_out() {
        // SB-107: claude-code → kimi-code → hermes; every Claude account limited, no Kimi usable.
        let snapshot = Snapshot {
            accounts: vec![
                account("claude-a", Provider::Claude, Some(100.0)),
                account("claude-b", Provider::Claude, Some(99.0)),
            ],
            ..Default::default()
        };
        let limited: HashSet<String> = ["claude-b".to_string()].into();
        let chained = chain(&[("claude-code", None), ("kimi-code", None), ("hermes", None)]);
        let with = |ready: bool, executor: Option<&str>, chain: &Chain| {
            let kimi = KimiChoice {
                ids: vec!["kimi-a".into()],
                usable: &|_| false,
                used: &|_| 0.0,
                hermes_ready: ready,
                opencode_ready: false,
                disabled: HashSet::new(),
            };
            resolve_with(
                &snapshot,
                chain,
                executor,
                &limited,
                &HashSet::new(),
                None,
                &kimi,
            )
        };
        let skip = |agent: &str, reason: &'static str| Step::Skip {
            agent: agent.into(),
            reason,
        };
        assert_eq!(
            with(true, Some("claude-a"), &chained),
            [
                skip("claude-code", "no_usable_account"),
                skip("kimi-code", "no_usable_account"),
                Step::Run {
                    agent: "hermes".into(),
                    account_id: "hermes".into()
                }
            ]
        );
        // No key, no model or no Hermes: passed over, never launched half-ready.
        assert_eq!(
            with(false, Some("claude-a"), &chained)[2],
            skip("hermes", "no_usable_account")
        );
        // A Hermes executor is not handed to Hermes again.
        assert_eq!(
            with(true, Some("hermes"), &chained)[2],
            skip("hermes", "no_usable_account")
        );
        // A step pinned to a project's paid key waits for its ceilings (SB-72, SB-96).
        let mut keyed = chain(&[("hermes", None)]);
        keyed.executors[0].key = Some("proj/prod/OPENROUTER_API_KEY".into());
        assert_eq!(
            with(true, Some("claude-a"), &keyed),
            [skip("hermes", "agent_not_automatic_yet")]
        );
    }
    #[test]
    fn an_opencode_step_runs_on_the_key_and_a_disabled_agent_is_skipped_first() {
        // SB-119/SB-120: claude-code → kimi-code → opencode; every Claude account limited.
        let snapshot = Snapshot {
            accounts: vec![
                account("claude-a", Provider::Claude, Some(100.0)),
                account("claude-b", Provider::Claude, Some(10.0)),
            ],
            ..Default::default()
        };
        let chained = chain(&[
            ("claude-code", None),
            ("kimi-code", None),
            ("opencode", None),
        ]);
        let with = |ready: bool, executor: Option<&str>, disabled: &[&str]| {
            let kimi = KimiChoice {
                ids: vec!["kimi-a".into()],
                usable: &|_| true,
                used: &|_| 0.0,
                hermes_ready: false,
                opencode_ready: ready,
                disabled: disabled.iter().map(|d| d.to_string()).collect(),
            };
            resolve_with(
                &snapshot,
                &chained,
                executor,
                &HashSet::new(),
                &HashSet::new(),
                None,
                &kimi,
            )
        };
        let skip = |agent: &str, reason: &'static str| Step::Skip {
            agent: agent.into(),
            reason,
        };
        let run = |agent: &str, id: &str| Step::Run {
            agent: agent.into(),
            account_id: id.into(),
        };
        // Everything usable and enabled: each step runs on its own account.
        assert_eq!(
            with(true, Some("claude-a"), &[]),
            [
                run("claude-code", "claude-b"),
                run("kimi-code", "kimi-a"),
                run("opencode", "opencode")
            ]
        );
        // Disabled agents are skipped before anything else is weighed, Kimi Code included.
        assert_eq!(
            with(true, Some("claude-a"), &["claude-code", "kimi-code"]),
            [
                skip("claude-code", "agent_disabled"),
                skip("kimi-code", "agent_disabled"),
                run("opencode", "opencode")
            ]
        );
        assert_eq!(
            with(true, Some("claude-a"), &["opencode"])[2],
            skip("opencode", "agent_disabled")
        );
        // Not ready (no OpenCode, key or model), or OpenCode is the executor: passed over.
        assert_eq!(
            with(false, Some("claude-a"), &[])[2],
            skip("opencode", "no_usable_account")
        );
        assert_eq!(
            with(true, Some("opencode"), &[])[2],
            skip("opencode", "no_usable_account")
        );
    }
    #[test]
    fn the_default_agent_never_continues_on_the_executor_or_a_copy_of_its_login() {
        // SB-120: `continue` without an account on Claude Code picks an account with room — not
        // the workflow's executor, not another saved row of the same login, not a limited one.
        let mut exec = account("claude-exec", Provider::Claude, Some(1.0));
        exec.identity = Some("person-a".into());
        let mut twin = account("claude-twin", Provider::Claude, Some(2.0));
        twin.identity = Some("person-a".into());
        let snapshot = Snapshot {
            accounts: vec![
                exec,
                twin,
                account("claude-free", Provider::Claude, Some(40.0)),
                account("claude-limited", Provider::Claude, Some(0.0)),
                account("codex-a", Provider::Codex, Some(0.0)),
            ],
            ..Default::default()
        };
        let none = KimiChoice {
            ids: Vec::new(),
            usable: &|_| false,
            used: &|_| 0.0,
            hermes_ready: false,
            opencode_ready: false,
            disabled: HashSet::new(),
        };
        let limited: HashSet<String> = ["claude-limited".to_string()].into();
        let pick = |executor: &[String], limited: &HashSet<String>| {
            pick_for_default(
                &snapshot,
                "claude-code",
                executor,
                limited,
                &HashSet::new(),
                std::path::Path::new("/work"),
                &none,
            )
        };
        let reference = crate::continuation::account_ref("claude-exec");
        let executor = executor_accounts(
            &json!({"provider": "claude-code", "accountRef": reference}),
            &snapshot,
            &Value::Null,
        );
        assert_eq!(executor, ["claude-exec"]);
        assert_eq!(pick(&executor, &limited).unwrap(), "claude-free");
        // With no accountRef, the ordinary CLI's current accounts stand in for the executor.
        let current = json!({"claude": {"account_ids": ["claude-free"]}});
        let executor = executor_accounts(&json!({"provider": "claude-code"}), &snapshot, &current);
        assert_eq!(executor, ["claude-free"]);
        assert_eq!(pick(&executor, &limited).unwrap(), "claude-exec");
        // Nothing left with room: a named refusal, never the executor.
        let all: HashSet<String> = ["claude-limited", "claude-free"].map(String::from).into();
        assert_eq!(
            pick(&["claude-exec".to_string()], &all).unwrap_err(),
            NO_DEFAULT_ACCOUNT
        );
        assert!(executor_accounts(&json!({"provider": "hermes"}), &snapshot, &current).is_empty());
    }
    #[test]
    fn a_kimi_step_runs_on_a_usable_kimi_account_pinned_or_least_used() {
        // SB-99: no Kimi account saved reads as no usable account; then the pin, then the choice.
        let snapshot = Snapshot::default();
        let (a, b, c) = ("kimi-a", "kimi-b", "kimi-c");
        let usable = |id: &str| id != c; // kimi-c: signed out, or a `kimi` runs in it
        let used = |id: &str| if id == a { 80.0 } else { 20.0 };
        let kimi = KimiChoice {
            ids: vec![a.into(), b.into(), c.into()],
            usable: &usable,
            used: &used,
            hermes_ready: false,
            opencode_ready: false,
            disabled: HashSet::new(),
        };
        let run = |id: &str| Step::Run {
            agent: "kimi-code".into(),
            account_id: id.into(),
        };
        let none = HashSet::new();
        let steps = |chain: &Chain, executor: Option<&str>, limited: &HashSet<String>| {
            resolve_with(&snapshot, chain, executor, limited, &none, None, &kimi)
        };
        assert_eq!(
            steps(&chain(&[("kimi-code", None)]), None, &none),
            vec![run(b)]
        );
        assert_eq!(
            steps(&chain(&[("kimi-code", Some(a))]), None, &none),
            vec![run(a)]
        );
        // The executor's own account, a limited one and an unusable one are never taken.
        let only_b_limited: HashSet<String> = [b.to_string()].into();
        assert_eq!(
            steps(&chain(&[("kimi-code", None)]), Some(a), &only_b_limited),
            vec![Step::Skip {
                agent: "kimi-code".into(),
                reason: "no_usable_account"
            }]
        );
        assert_eq!(
            steps(&chain(&[("kimi-code", Some(c))]), None, &none),
            vec![Step::Skip {
                agent: "kimi-code".into(),
                reason: "no_usable_account"
            }],
            "a pinned account is used only when it is usable"
        );
        // Kimi Code on a paid key stays phase 2.
        let mut keyed = chain(&[("kimi-code", None)]);
        keyed.executors[0].key = Some("p/prod/KEY".into());
        assert_eq!(
            steps(&keyed, None, &none),
            vec![Step::Skip {
                agent: "kimi-code".into(),
                reason: "agent_not_automatic_yet"
            }]
        );
    }

    #[test]
    fn a_kimi_executor_is_limited_only_by_its_named_saved_account() {
        let ids = vec!["kimi-a".to_string()];
        let limited: HashSet<String> = ["kimi-a".to_string()].into();
        let executor = |provider: &str, account: Option<&str>| {
            let mut e = json!({"provider": provider});
            if let Some(a) = account {
                e["accountRef"] = json!(a);
            }
            e
        };
        assert_eq!(
            kimi_executor_limited(&executor("kimi-code", Some("kimi-a")), &ids, &limited),
            Some("kimi-a".into())
        );
        assert_eq!(
            kimi_executor_limited(&executor("kimi", Some("kimi-a")), &ids, &HashSet::new()),
            None,
            "not limited"
        );
        assert_eq!(
            kimi_executor_limited(&executor("kimi", None), &ids, &limited),
            None,
            "the ordinary kimi is not watched"
        );
        assert_eq!(
            kimi_executor_limited(&executor("claude", Some("kimi-a")), &ids, &limited),
            None
        );
        assert_eq!(
            kimi_executor_limited(&executor("kimi", Some("kimi-x")), &ids, &limited),
            None,
            "an account Switchboard does not hold decides nothing"
        );
    }

    #[test]
    fn the_chain_resolves_to_usable_accounts_in_order() {
        let snapshot = Snapshot {
            accounts: vec![
                account("claude-a", Provider::Claude, Some(99.0)),
                account("claude-b", Provider::Claude, Some(40.0)),
                account("claude-c", Provider::Claude, Some(10.0)),
                account("codex-a", Provider::Codex, None),
            ],
            ..Default::default()
        };
        let limited: HashSet<String> = ["claude-a".to_string(), "claude-c".to_string()].into();
        let steps = resolve(
            &snapshot,
            &chain(&[("claude-code", None), ("codex", None), ("kimi-code", None)]),
            Some("claude-a"),
            &limited,
            &HashSet::new(),
            None,
        );
        assert_eq!(
            steps,
            vec![
                // claude-a is the executor and limited, claude-c limited: claude-b remains.
                Step::Run {
                    agent: "claude-code".into(),
                    account_id: "claude-b".into()
                },
                Step::Run {
                    agent: "codex".into(),
                    account_id: "codex-a".into()
                },
                Step::Skip {
                    agent: "kimi-code".into(),
                    reason: "no_usable_account"
                },
            ]
        );
    }

    #[test]
    fn a_scan_that_finds_nothing_waits_longer() {
        // LC-08: a limited account with no open workflow must not start the engine every minute.
        let state = FallbackState::default();
        assert!(state.due(1_000));
        assert!(!state.due(1_000 + SCAN_SECONDS - 1));
        assert!(state.due(1_000 + SCAN_SECONDS));
        state.idle(2_000);
        assert!(!state.due(2_000 + SCAN_SECONDS));
        assert!(state.due(2_000 + IDLE_SCAN_SECONDS));
    }

    #[test]
    fn another_row_of_a_spent_login_is_not_a_way_out() {
        // SB-62 keeps one login saved in two pools as two rows; they share one limit.
        let with = |id: &str, identity: &str, used: f64| {
            let mut a = account(id, Provider::Claude, Some(used));
            a.identity = Some(identity.into());
            a
        };
        let snapshot = Snapshot {
            accounts: vec![
                with("exec", "me@example.test", 99.0),
                with("exec-copy", "me@example.test", 1.0),
                with("limited", "team@example.test", 99.0),
                with("limited-copy", "team@example.test", 2.0),
                with("other", "other@example.test", 50.0),
            ],
            ..Default::default()
        };
        let limited: HashSet<String> = ["limited".to_string()].into();
        let steps = resolve(
            &snapshot,
            &chain(&[("claude-code", None)]),
            Some("exec"),
            &limited,
            &HashSet::new(),
            None,
        );
        assert_eq!(
            steps,
            vec![Step::Run {
                agent: "claude-code".into(),
                account_id: "other".into()
            }]
        );
    }

    #[test]
    fn a_pinned_account_is_used_only_when_usable_and_never_the_executor() {
        let mut disabled = account("codex-off", Provider::Codex, Some(1.0));
        disabled.enabled = false;
        let snapshot = Snapshot {
            accounts: vec![
                account("claude-a", Provider::Claude, Some(1.0)),
                disabled,
                account("codex-signin", Provider::Codex, Some(1.0)),
            ],
            ..Default::default()
        };
        let sign_in: HashSet<String> = ["codex-signin".to_string()].into();
        let steps = resolve(
            &snapshot,
            &chain(&[
                ("claude-code", Some("claude-a")),
                ("codex", Some("codex-off")),
                ("codex", Some("codex-signin")),
                ("codex", Some("claude-a")),
            ]),
            Some("claude-a"),
            &HashSet::new(),
            &sign_in,
            None,
        );
        assert!(
            steps.iter().all(|s| matches!(
                s,
                Step::Skip {
                    reason: "no_usable_account",
                    ..
                }
            )),
            "{steps:?}"
        );
    }

    #[test]
    fn an_executor_named_by_its_handle_is_found_and_judged() {
        // Hand-overs since 0.6.22 name the account by its handle (`continuation::account_ref`).
        let id = "3e7b8cf7-dab5-4d23-bb8f-5a690887db98";
        let snapshot = Snapshot {
            accounts: vec![account(id, Provider::Claude, Some(100.0))],
            ..Default::default()
        };
        let limited: HashSet<String> = [id.to_string()].into();
        let handle = continuation::account_ref(id);
        let executor = json!({"provider": "claude-code", "accountRef": handle});
        assert_eq!(
            executor_limited(&executor, &snapshot, &Value::Null, &limited, &|_| false),
            Some((Provider::Claude, Some(id.to_owned())))
        );
        let kimi = json!({"provider": "kimi", "accountRef": handle});
        assert_eq!(
            kimi_executor_limited(&kimi, &[id.to_owned()], &limited),
            Some(id.to_owned())
        );
        assert_eq!(
            kimi_executor_limited(&kimi, &[id.to_owned()], &HashSet::new()),
            None
        );
    }
    #[test]
    fn the_executor_is_limited_by_its_account_or_by_the_cli_it_runs_in() {
        let snapshot = Snapshot {
            accounts: vec![
                account("claude-a", Provider::Claude, None),
                account("claude-b", Provider::Claude, None),
            ],
            ..Default::default()
        };
        let limited: HashSet<String> = ["claude-a".to_string()].into();
        let current =
            json!({"claude": {"account_ids": ["claude-a"]}, "codex": {"account_ids": []}});
        let none = |_: Provider| false;
        // Named account Switchboard holds.
        assert_eq!(
            executor_limited(
                &json!({"provider": "claude-code", "accountRef": "claude-a"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            Some((Provider::Claude, Some("claude-a".into())))
        );
        assert_eq!(
            executor_limited(
                &json!({"provider": "claude-code", "accountRef": "claude-b"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            None,
            "its own account is not limited"
        );
        // No account named: the ordinary Claude Code is on claude-a, which is limited.
        assert_eq!(
            executor_limited(
                &json!({"provider": "anthropic"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            Some((Provider::Claude, Some("claude-a".into())))
        );
        assert_eq!(
            executor_limited(
                &json!({"provider": "codex"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            None
        );
        assert_eq!(
            executor_limited(
                &json!({"provider": "hermes"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            None
        );
        assert_eq!(
            executor_limited(&Value::Null, &snapshot, &current, &limited, &none),
            None
        );
        // An account the engine names but Switchboard does not hold decides nothing: the
        // ordinary CLI's limit is not blamed on another manager's session.
        assert_eq!(
            executor_limited(
                &json!({"provider": "claude-code", "accountRef": "elsewhere"}),
                &snapshot,
                &current,
                &limited,
                &none
            ),
            None
        );
        // No account named while a Switchboard-launched Claude session runs: it could be the
        // executor on a healthy account, so nothing is handed over.
        let running = |p: Provider| p == Provider::Claude;
        assert_eq!(
            executor_limited(
                &json!({"provider": "anthropic"}),
                &snapshot,
                &current,
                &limited,
                &running
            ),
            None
        );
    }

    /// A stand-in for `project-observatory`: one open workflow executed by `account`, its
    /// checkout `repo`.
    #[cfg(unix)]
    fn engine(dir: &std::path::Path, account: &str, repo: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let list = json!({"workflows": [{"workflowId": WF, "status": "open", "pendingHandoff": null,
            "silentSeconds": 600, "lease": {"executor": {"provider": "claude-code", "accountRef": account}}}]});
        let show = json!({"workflowId": WF, "status": "open", "pendingHandoff": null,
            "checkpoint": {"executor": {"provider": "claude-code"},
                "body": {"artifacts": [{"kind": "git", "path": repo}]}},
            "lease": {"executor": {"provider": "claude-code", "accountRef": account}},
            "credentials": []});
        let path = dir.join("project-observatory");
        std::fs::write(
            &path,
            format!("#!/bin/sh\ncase \"$3\" in\n list) cat <<'J'\n{list}\nJ\n;;\n show) cat <<'J'\n{show}\nJ\n;;\nesac\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    #[cfg(unix)]
    const WF: &str = "wf_0123456789abcdef";

    #[test]
    fn an_unnamed_executor_counts_only_while_its_workflow_is_worked() {
        // SB-129: no accountRef → the ordinary CLI is the executor only within the hour.
        let unnamed = json!({"provider": "claude-code"});
        let named = json!({"provider": "claude-code", "accountRef": "sb-0123456789ab"});
        let silent = |s: i64| json!({"silentSeconds": s});
        assert!(!unnamed_and_stale(&silent(600), &unnamed));
        assert!(
            unnamed_and_stale(&silent(7200), &unnamed),
            "the 2026-10-14 scratch workflow"
        );
        assert!(unnamed_and_stale(&json!({}), &unnamed), "silence unknown");
        assert!(
            !unnamed_and_stale(&silent(7200), &named),
            "a named account decides itself"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_limited_executor_hands_its_workflow_to_the_next_agent_of_its_chain() {
        use crate::fixtures::*;
        use crate::monitor::now;
        use std::sync::Arc;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        // Claude Code wrote a limit marker a minute ago for the account in use (synthetic-a).
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
        assert!(runtime.limits.limited_ids(now()).contains(&a.id));
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().canonicalize().unwrap();
        let bin = engine(root.path(), &a.id, &repo_path);
        let calls: Arc<Mutex<Vec<(String, String, PathBuf)>>> = Default::default();
        let record = |wf: String, account: String, dir: PathBuf| {
            let calls = calls.clone();
            async move {
                calls.lock().unwrap().push((wf, account, dir));
                Ok(())
            }
        };
        // No chain: nothing happens, and the engine is not even asked.
        pass_with(&runtime, now(), &bin, record).await;
        assert!(calls.lock().unwrap().is_empty());
        runtime
            .store
            .set_chain(
                ChainScope::Machine,
                vec![
                    Executor {
                        agent: "claude-code".into(),
                        account_id: None,
                        key: None,
                    },
                    Executor {
                        agent: "codex".into(),
                        account_id: None,
                        key: None,
                    },
                ],
            )
            .unwrap();
        let t = now();
        pass_with(&runtime, t, &bin, record).await;
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(WF.to_string(), b.id.clone(), repo_path.clone())],
            "handed to the usable Claude account, in the workflow's checkout"
        );
        // Inside the retry window nothing is tried again, even on a later scan.
        pass_with(&runtime, t + SCAN_SECONDS, &bin, record).await;
        assert_eq!(calls.lock().unwrap().len(), 1);
        let events = runtime.store.snapshot().unwrap().events;
        assert!(events
            .iter()
            .any(|e| e.action == "fallback" && e.detail == "offered"));
    }

    /// C-8 (dossier C part E): a workflow worked in a group's folder by an account outside that
    /// group (the ordinary Claude Code's account sits in the default group, which an ordinary
    /// sign-in cannot leave) falls back along the group's chain, not the machine's.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_groups_chain_applies_to_a_workflow_in_its_folder_on_the_ordinary_cli() {
        use crate::fixtures::*;
        use crate::monitor::now;
        use std::sync::Arc;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().canonicalize().unwrap();
        let group = runtime
            .store
            .save_project(
                None,
                "Alpha",
                std::slice::from_ref(&repo_path),
                std::slice::from_ref(&c.id),
            )
            .unwrap();
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
        assert!(runtime.limits.limited_ids(now()).contains(&a.id));
        let step = |agent: &str| Executor {
            agent: agent.into(),
            account_id: None,
            key: None,
        };
        // The machine's chain names only Codex (no Codex account here: nothing to hand to); the
        // group's names Claude Code, which its own account serves in its folder.
        runtime
            .store
            .set_chain(ChainScope::Machine, vec![step("codex")])
            .unwrap();
        runtime
            .store
            .set_chain(
                ChainScope::Project {
                    pool: group.pool.clone(),
                },
                vec![step("claude-code")],
            )
            .unwrap();
        let bin = engine(root.path(), &a.id, &repo_path);
        let calls: Arc<Mutex<Vec<(String, String, PathBuf)>>> = Default::default();
        let record = |wf: String, account: String, dir: PathBuf| {
            let calls = calls.clone();
            async move {
                calls.lock().unwrap().push((wf, account, dir));
                Ok(())
            }
        };
        pass_with(&runtime, now(), &bin, record).await;
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(WF.to_string(), c.id.clone(), repo_path.clone())],
            "the group's chain, chosen by the checkout, hands it to the group's account"
        );
    }

    #[test]
    fn a_folder_in_several_groups_takes_the_group_its_proxy_route_uses_and_logs_ambiguous() {
        let group = |pool: &str, folder: &str| switchboard_core::Project {
            pool: pool.into(),
            name: pool.into(),
            folders: vec![folder.into()],
            created_at: 1,
            sources: vec![],
            organization: None,
        };
        let mut in_beta = account("in-beta", Provider::Claude, None);
        in_beta.pool = "beta".into();
        let mut in_svc = account("in-svc", Provider::Claude, None);
        in_svc.pool = "svc".into();
        let snapshot = Snapshot {
            accounts: vec![account("cli", Provider::Claude, None), in_beta, in_svc],
            projects: vec![
                group("alpha", "/work/shared"),
                group("beta", "/work/shared"),
                group("gamma", "/work/gamma"),
                group("empty-1", "/work/bare"),
                group("empty-2", "/work/bare"),
                // A group inside another group's folder; the outer one has no account.
                group("mono", "/work/mono"),
                group("svc", "/work/mono/svc"),
            ],
            ..Default::default()
        };
        let pool_for = |executor: Option<&str>, dir: &str| {
            chain_pool(&snapshot, executor, std::path::Path::new(dir))
        };
        // The executor's own group holds the checkout: it decides, never ambiguous.
        assert_eq!(
            pool_for(Some("in-beta"), "/work/shared/api"),
            (Some("beta".into()), false)
        );
        // One group holds it: that group, whoever's account worked there.
        assert_eq!(
            pool_for(Some("cli"), "/work/gamma/src"),
            (Some("gamma".into()), false)
        );
        assert_eq!(pool_for(None, "/work/gamma"), (Some("gamma".into()), false));
        // Several groups hold it and the executor's is none of them: the group the folder's
        // proxy route and paid key use (`Snapshot::project_for`: the first with an enabled
        // account), and the pass logs `group_ambiguous`.
        assert_eq!(
            pool_for(Some("cli"), "/work/shared"),
            (Some("beta".into()), true)
        );
        assert_eq!(
            pool_for(None, "/work/shared/x"),
            (Some("beta".into()), true)
        );
        // No holding group has an account: the first one, as `project_for` answers.
        assert_eq!(
            pool_for(Some("cli"), "/work/bare"),
            (Some("empty-1".into()), true)
        );
        // Nested groups: the inner folder is held by both; the inner group has the account.
        assert_eq!(
            pool_for(Some("cli"), "/work/mono/svc/api"),
            (Some("svc".into()), true)
        );
        assert_eq!(
            pool_for(Some("cli"), "/work/mono/web"),
            (Some("mono".into()), false)
        );
        // No group holds it: as before, the executor's group.
        assert_eq!(
            pool_for(Some("in-beta"), "/elsewhere"),
            (Some("beta".into()), false)
        );
        assert_eq!(pool_for(None, "/elsewhere"), (None, false));
    }

    /// C-8 fix cycle 1: when no chain applies to the checkout's group, the pass logs `no_chain`
    /// and leaves no retry mark, so a chain set a minute later is used at the next scan.
    #[cfg(unix)]
    #[tokio::test]
    async fn no_chain_for_the_checkouts_group_is_logged_and_tried_again_at_the_next_scan() {
        use crate::fixtures::*;
        use crate::monitor::now;
        use std::sync::Arc;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().canonicalize().unwrap();
        let other = tempfile::tempdir().unwrap();
        let other_path = other.path().canonicalize().unwrap();
        let alpha = runtime
            .store
            .save_project(
                None,
                "Alpha",
                std::slice::from_ref(&repo_path),
                std::slice::from_ref(&c.id),
            )
            .unwrap();
        let beta = runtime
            .store
            .save_project(None, "Beta", std::slice::from_ref(&other_path), &[])
            .unwrap();
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
        let step = |agent: &str| Executor {
            agent: agent.into(),
            account_id: None,
            key: None,
        };
        // Only another group has a chain; none for Alpha, none for the machine.
        runtime
            .store
            .set_chain(
                ChainScope::Project {
                    pool: beta.pool.clone(),
                },
                vec![step("claude-code")],
            )
            .unwrap();
        let bin = engine(root.path(), &a.id, &repo_path);
        let calls: Arc<Mutex<Vec<(String, String, PathBuf)>>> = Default::default();
        let record = |wf: String, account: String, dir: PathBuf| {
            let calls = calls.clone();
            async move {
                calls.lock().unwrap().push((wf, account, dir));
                Ok(())
            }
        };
        let t = now();
        crate::oplog::capture_begin();
        pass_with(&runtime, t, &bin, record).await;
        let logged = crate::oplog::capture_end();
        assert!(calls.lock().unwrap().is_empty());
        assert!(
            logged.contains(&("fallback", vec!["no_chain"])),
            "logged {logged:?}"
        );
        assert!(!runtime.fallback.recently_tried(WF, t), "no retry mark");
        // Alpha gets a chain: the next scan hands the workflow over.
        runtime
            .store
            .set_chain(
                ChainScope::Project {
                    pool: alpha.pool.clone(),
                },
                vec![step("claude-code")],
            )
            .unwrap();
        pass_with(&runtime, t + SCAN_SECONDS, &bin, record).await;
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(WF.to_string(), c.id.clone(), repo_path.clone())]
        );
        assert!(runtime.fallback.recently_tried(WF, t + SCAN_SECONDS));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_hand_over_by_hand_waits_for_the_console_and_needs_the_setting() {
        // SB-108 by hand (`continue --to-console`): refused while the setting is off, before
        // anything is offered; queued like the automatic fallback's when on.
        use crate::fixtures::*;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_out, activates).await;
        let refused = runtime
            .execute(Operation::ContinueToConsole {
                workflow_id: WF.into(),
                id: "opencode".into(),
                working_directory: root.path().to_owned(),
            })
            .await
            .unwrap_err();
        assert_eq!(refused, crate::handovers::OFF);
        assert!(crate::handovers::pending(root.path()).is_empty());
        crate::handovers::set_enabled(root.path(), true).unwrap();
        let script = root.path().join("launch.command");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let answer = serde_json::json!({"script": script, "offeredUntil": "2099-01-01T00:00:00Z"});
        let queued = queue_handover(
            root.path(),
            None,
            WF.into(),
            "opencode",
            root.path().to_owned(),
            &answer,
            crate::monitor::now(),
        )
        .unwrap();
        assert_eq!(queued.agent, "opencode");
        assert_eq!(queued.account_label, "OpenCode");
        let waiting = crate::handovers::pending(root.path());
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].workflow, WF);
        assert!(
            queue_handover(
                root.path(),
                None,
                WF.into(),
                "opencode",
                root.path().to_owned(),
                &serde_json::json!({}),
                0
            )
            .is_err(),
            "no script, nothing queued"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_refusal_moves_down_the_chain_and_a_failed_launch_after_the_offer_stops() {
        let dir = tempfile::tempdir().unwrap();
        let steps = vec![
            Step::Skip {
                agent: "kimi-code".into(),
                reason: "agent_not_automatic_yet",
            },
            Step::Run {
                agent: "claude-code".into(),
                account_id: "refuses".into(),
            },
            Step::Run {
                agent: "codex".into(),
                account_id: "offered-then-failed".into(),
            },
            Step::Run {
                agent: "codex".into(),
                account_id: "never-reached".into(),
            },
        ];
        let tried: std::sync::Arc<Mutex<Vec<String>>> = Default::default();
        let act = |_: String, account: String, _: PathBuf| {
            let tried = tried.clone();
            async move {
                tried.lock().unwrap().push(account.clone());
                match account.as_str() {
                    "refuses" => Err("Enable the account before continuing a workflow.".to_string()),
                    _ => Err("Offered handoff:0123456789abcdef until 2026-10-06T15:00:00Z, but the session did not start: x".to_string()),
                }
            }
        };
        let open = |_: String, _: String| async { Gate::Run };
        assert_eq!(
            hand_over(&act, &open, WF, dir.path(), &steps).await.outcome,
            "launch_failed"
        );
        assert_eq!(*tried.lock().unwrap(), ["refuses", "offered-then-failed"]);
        let only_skips = vec![Step::Skip {
            agent: "hermes".into(),
            reason: "agent_not_automatic_yet",
        }];
        assert_eq!(
            hand_over(&act, &open, WF, dir.path(), &only_skips)
                .await
                .outcome,
            "no_usable_executor"
        );
    }

    #[test]
    fn scans_are_spaced_and_a_workflow_is_not_retried_inside_its_window() {
        let state = FallbackState::default();
        assert!(state.due(1_000));
        assert!(!state.due(1_030));
        assert!(state.due(1_000 + SCAN_SECONDS));
        state.mark("wf_0123456789abcdef", 1_000);
        assert!(state.recently_tried("wf_0123456789abcdef", 1_000 + RETRY_SECONDS - 1));
        assert!(!state.recently_tried("wf_0123456789abcdef", 1_000 + RETRY_SECONDS));
        assert!(!state.recently_tried("wf_ffffffffffffffff", 1_000));
    }

    // ---- C-5: the spend gate (SB-72, dossier C part D) ----

    use crate::spend::{self, KeyRead, Surface};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const T: i64 = 1_800_000_000;
    const WF2: &str = "wf_fedcba9876543210";
    fn hermes() -> Step {
        Step::Run {
            agent: "hermes".into(),
            account_id: crate::hermes::HERMES_TARGET.into(),
        }
    }
    fn run(agent: &str, account: &str) -> Step {
        Step::Run {
            agent: agent.into(),
            account_id: account.into(),
        }
    }
    fn openrouter_key(pool: Option<&str>) -> switchboard_core::agent_keys::AgentKey {
        switchboard_core::agent_keys::AgentKey {
            id: format!("k-{}", pool.unwrap_or("machine")),
            service: "openrouter".into(),
            pool: pool.map(str::to_owned),
            saved_at: 1,
            model: None,
        }
    }
    /// A group `alpha` holding `dir`, a subscription and an API-key Claude account, the machine's
    /// OpenRouter key and, with `group_key`, alpha's own.
    fn paid_snapshot(dir: &std::path::Path, group_key: bool) -> Snapshot {
        let mut api = account("claude-api", Provider::Claude, Some(1.0));
        api.kind = AuthKind::ApiKey;
        let mut keys = vec![openrouter_key(None)];
        if group_key {
            keys.push(openrouter_key(Some("alpha")));
        }
        Snapshot {
            accounts: vec![account("claude-sub", Provider::Claude, Some(1.0)), api],
            projects: vec![switchboard_core::Project {
                pool: "alpha".into(),
                name: "Alpha".into(),
                folders: vec![dir.display().to_string()],
                created_at: 1,
                sources: vec![],
                organization: None,
            }],
            agent_keys: keys,
            ..Default::default()
        }
    }
    fn spend(limit_remaining: f64, usage: f64, usage_daily: f64) -> Option<KeyRead> {
        Some(KeyRead {
            limit_remaining: Some(limit_remaining),
            usage: Some(usage),
            usage_daily: Some(usage_daily),
        })
    }
    /// A read that always answers `answer`, counting its calls.
    fn fixed(
        answer: Option<KeyRead>,
        count: &Arc<AtomicUsize>,
    ) -> impl Fn(Option<String>) -> std::future::Ready<Option<KeyRead>> {
        let count = count.clone();
        move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            std::future::ready(answer.clone())
        }
    }
    /// One hand-over through the real gate with `read`; answers it and the accounts handed work.
    #[allow(clippy::too_many_arguments)]
    async fn walk<R, RFut>(
        root: &std::path::Path,
        snapshot: &Snapshot,
        dir: &std::path::Path,
        workflow: &str,
        steps: &[Step],
        read: &R,
        limit: std::time::Duration,
        refuse: bool,
    ) -> (HandOver, Vec<String>)
    where
        R: Fn(Option<String>) -> RFut,
        RFut: std::future::Future<Output = Option<KeyRead>> + Send + 'static,
    {
        let acted: Arc<Mutex<Vec<String>>> = Default::default();
        let act = |_: String, account: String, _: PathBuf| {
            let acted = acted.clone();
            async move {
                acted.lock().unwrap().push(account);
                if refuse {
                    Err("Enable the account before continuing a workflow.".to_string())
                } else {
                    Ok(())
                }
            }
        };
        let gate = |_: String, account: String| async move {
            spend_gate(root, snapshot, workflow, dir, &account, T, read, limit).await
        };
        let done = hand_over(&act, &gate, workflow, dir, steps).await;
        let acted = acted.lock().unwrap().clone();
        (done, acted)
    }
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(5);
    fn dir_of(tmp: &tempfile::TempDir) -> PathBuf {
        tmp.path().canonicalize().unwrap()
    }
    fn machine_ceiling(root: &std::path::Path, snapshot: &Snapshot, usd: f64) {
        spend::set_ceiling(
            root,
            snapshot,
            ChainScope::Machine,
            Some(usd),
            Surface::Window,
            T,
        )
        .unwrap();
    }
    fn open_questions(root: &std::path::Path) -> usize {
        spend::read(root)
            .unwrap()
            .questions
            .iter()
            .filter(|q| q.answer.is_none())
            .count()
    }

    #[tokio::test]
    async fn a_hermes_step_on_a_key_with_nothing_left_is_skipped_not_offered() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        // $0.20 left of the key's own limit: below the $0.50 floor, with no ceiling set.
        let read = fixed(spend(0.20, 9.8, 1.0), &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes(), run("claude-code", "claude-sub")],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(
            acted,
            ["claude-sub"],
            "Hermes is never offered the workflow"
        );
        assert_eq!(done.outcome, "offered");
        assert_eq!(done.skipped, [("hermes".to_string(), "key_exhausted")]);
        let (alone, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(alone.outcome, "key_exhausted");
    }

    #[tokio::test]
    async fn a_reached_group_ceiling_skips_the_paid_step_and_opens_one_question() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, true);
        let group = ChainScope::Project {
            pool: "alpha".into(),
        };
        spend::set_ceiling(
            root.path(),
            &snapshot,
            group.clone(),
            Some(5.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let read = fixed(spend(40.0, 30.0, 5.0), &count);
        for _ in 0..2 {
            let (done, acted) = walk(
                root.path(),
                &snapshot,
                &dir,
                WF,
                &[hermes()],
                &read,
                LIMIT,
                false,
            )
            .await;
            assert!(acted.is_empty());
            assert_eq!(done.outcome, "ceiling_reached");
        }
        assert_eq!(
            open_questions(root.path()),
            1,
            "one question per ceiling and value"
        );
        let file = spend::read(root.path()).unwrap();
        assert_eq!(file.questions[0].scope, group);
        assert_eq!(file.questions[0].cents, 500);
        // Below it, the step runs: the group's key is the one read (alpha's own key).
        let below = fixed(spend(40.0, 30.0, 4.99), &count);
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &below,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(done.outcome, "offered");
    }

    #[tokio::test]
    async fn an_unread_balance_with_a_ceiling_is_a_denial_and_without_one_runs() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        let unread = fixed(None, &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &unread,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(
            (done.outcome, acted.len()),
            ("offered", 1),
            "no ceiling: it runs"
        );
        machine_ceiling(root.path(), &snapshot, 5.0);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &unread,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "spend_unknown");
    }

    #[tokio::test]
    async fn a_subscription_step_is_never_gated() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        // Even with spend.json unreadable and every key spent, a sign-in and Kimi Code run.
        std::fs::write(root.path().join(spend::FILE), b"not json").unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let read = fixed(spend(0.0, 99.0, 99.0), &count);
        for step in [run("claude-code", "claude-sub"), run("kimi-code", "kimi-a")] {
            let (done, acted) = walk(
                root.path(),
                &snapshot,
                &dir,
                WF,
                &[step],
                &read,
                LIMIT,
                false,
            )
            .await;
            assert_eq!(done.outcome, "offered");
            assert_eq!(acted.len(), 1);
        }
        assert_eq!(count.load(Ordering::SeqCst), 0, "nothing read");
        // A paid step with the file unreadable is skipped: never "no ceilings".
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(done.outcome, "ceilings_unreadable");
    }

    #[tokio::test]
    async fn nothing_is_read_from_openrouter_when_no_paid_step_is_reached() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, true);
        machine_ceiling(root.path(), &snapshot, 5.0);
        let count = Arc::new(AtomicUsize::new(0));
        let read = fixed(spend(40.0, 1.0, 1.0), &count);
        // The subscription takes it first: the Hermes step after it is never reached.
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[run("claude-code", "claude-sub"), hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(done.outcome, "offered");
        assert_eq!(count.load(Ordering::SeqCst), 0);
        // Reached, it reads its own key and, under the machine ceiling, the other one too.
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(done.outcome, "offered");
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_key_whose_read_times_out_is_unknown_under_a_ceiling_and_runs_without_one() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let slow = |_: Option<String>| async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            spend(40.0, 1.0, 1.0)
        };
        let short = std::time::Duration::from_millis(50);
        let started = std::time::Instant::now();
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &slow,
            short,
            false,
        )
        .await;
        assert_eq!(done.outcome, "offered", "no ceiling: unread runs");
        machine_ceiling(root.path(), &snapshot, 5.0);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &slow,
            short,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "spend_unknown");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "never waits out the read"
        );
    }

    #[tokio::test]
    async fn a_key_whose_total_went_down_is_never_counted_as_less_spend() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        // The first paid hand-over records the task's baseline at a total of $10.
        let first = fixed(spend(40.0, 10.0, 2.0), &count);
        walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &first,
            LIMIT,
            false,
        )
        .await;
        let task = ChainScope::Workflow { id: WF.into() };
        spend::set_ceiling(root.path(), &snapshot, task, Some(1.0), Surface::Cli, T).unwrap();
        // The key now reports $4 in total: another key, or a reset. Unknown, never "-$6 spent".
        let lower = fixed(spend(40.0, 4.0, 0.1), &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &lower,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "spend_unknown");
    }

    #[tokio::test]
    async fn two_hand_overs_racing_for_the_last_dollar_are_decided_one_after_the_other() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let mut snapshot = paid_snapshot(&dir, false);
        snapshot.agent_keys.truncate(1);
        machine_ceiling(root.path(), &snapshot, 5.0);
        // $4.80 spent today; each hand-over spends $0.30 as soon as it starts.
        let daily = Arc::new(Mutex::new(4.80_f64));
        let read = {
            let daily = daily.clone();
            move |_: Option<String>| {
                let daily = daily.clone();
                async move {
                    tokio::task::yield_now().await;
                    let d = *daily.lock().unwrap();
                    spend(40.0, 20.0 + d, d)
                }
            }
        };
        let gate_for = |workflow: &'static str| {
            let (root, snapshot, dir, read) = (root.path(), &snapshot, &dir, &read);
            move |_: String, account: String| async move {
                spend_gate(root, snapshot, workflow, dir, &account, T, read, LIMIT).await
            }
        };
        let act = |_: String, _: String, _: PathBuf| {
            let daily = daily.clone();
            async move {
                tokio::task::yield_now().await;
                *daily.lock().unwrap() += 0.30;
                Ok(())
            }
        };
        let (g1, g2) = (gate_for(WF), gate_for(WF2));
        let steps = [hermes()];
        let (a, b) = tokio::join!(
            hand_over(&act, &g1, WF, &dir, &steps),
            hand_over(&act, &g2, WF2, &dir, &steps)
        );
        let mut outcomes = [a.outcome, b.outcome];
        outcomes.sort();
        assert_eq!(outcomes, ["ceiling_reached", "offered"]);
    }

    #[tokio::test]
    async fn a_ceiling_set_while_the_spend_is_read_is_obeyed() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, true);
        // No ceiling when the gate starts; the window sets $1 for the machine during the read.
        let read = {
            let (root, snapshot) = (root.path().to_path_buf(), snapshot.clone());
            move |pool: Option<String>| {
                let (root, snapshot) = (root.clone(), snapshot.clone());
                async move {
                    if spend::read(&root).unwrap().ceilings.is_empty() {
                        machine_ceiling(&root, &snapshot, 1.0);
                    }
                    pool.is_none().then_some(KeyRead {
                        limit_remaining: Some(40.0),
                        usage: Some(3.0),
                        usage_daily: Some(0.2),
                    })
                }
            }
        };
        // Only the machine key was read: the group's own key, which a machine ceiling adds up,
        // was not, so the decision on the ceiling as it is now is unknown, never "under it".
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "spend_unknown");
    }

    #[tokio::test]
    async fn an_api_key_claude_step_under_a_ceiling_is_unknown_and_reads_nothing() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        let read = fixed(spend(40.0, 1.0, 1.0), &count);
        let step = [run("claude-code", "claude-api")];
        let (done, _) = walk(root.path(), &snapshot, &dir, WF, &step, &read, LIMIT, false).await;
        assert_eq!(done.outcome, "offered", "no ceiling: it runs");
        machine_ceiling(root.path(), &snapshot, 5.0);
        let (done, acted) =
            walk(root.path(), &snapshot, &dir, WF, &step, &read, LIMIT, false).await;
        assert!(acted.is_empty());
        assert_eq!(
            done.outcome, "spend_unknown",
            "C-Q3: its spend cannot be read"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "OpenRouter is not its provider"
        );
    }

    #[tokio::test]
    async fn a_restart_between_the_decision_and_the_offer_keeps_the_tasks_baseline() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        // The gate lets Hermes through at a total of $10, then the process ends before an offer
        // (the step is refused here): the baseline is already on disk.
        let at_ten = fixed(spend(40.0, 10.0, 1.0), &count);
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &at_ten,
            LIMIT,
            true,
        )
        .await;
        assert_eq!(done.outcome, "every_step_refused");
        let file = spend::read(root.path()).unwrap();
        assert_eq!(file.baselines.len(), 1);
        assert_eq!(
            (file.baselines[0].usage, file.baselines[0].key.as_str()),
            (Some(10.0), "machine")
        );
        // The next process: a $1 task ceiling, the key at $11.50, so $1.50 counted.
        let task = ChainScope::Workflow { id: WF.into() };
        spend::set_ceiling(root.path(), &snapshot, task, Some(1.0), Surface::Cli, T).unwrap();
        let later = fixed(spend(40.0, 11.5, 2.5), &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &later,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "ceiling_reached");
        assert_eq!(
            spend::read(root.path()).unwrap().baselines[0].usage,
            Some(10.0),
            "kept"
        );
    }

    #[test]
    fn the_last_hand_over_is_recorded_with_its_skipped_steps() {
        let root = tempfile::tempdir().unwrap();
        let snapshot = Snapshot::default();
        let done = HandOver {
            outcome: "offered",
            skipped: vec![
                ("hermes".into(), "ceiling_reached"),
                ("kimi-code".into(), "no_usable_account"),
            ],
        };
        record_last(root.path(), &snapshot, WF, &done, T);
        let last = spend::read(root.path()).unwrap().last_handover.unwrap();
        assert_eq!(
            (last.at, last.workflow.as_str(), last.outcome.as_str()),
            (T, WF, "offered")
        );
        assert_eq!(last.skipped[0].reason, "ceiling_reached");
        assert_eq!(last.skipped[1].agent, "kimi-code");
    }

    // ---- C-5/C-6 fix cycle 1 (C-Q8, C-Q9) ----

    fn opencode() -> Step {
        Step::Run {
            agent: "opencode".into(),
            account_id: crate::launch::OPENCODE_TARGET.into(),
        }
    }

    #[tokio::test]
    async fn an_opencode_step_under_a_ceiling_it_cannot_read_is_gated_like_hermes() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        machine_ceiling(root.path(), &snapshot, 5.0);
        let count = Arc::new(AtomicUsize::new(0));
        let unread = fixed(None, &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[opencode()],
            &unread,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty(), "OpenCode is never offered the workflow");
        assert_eq!(done.outcome, "spend_unknown");
        assert_eq!(done.skipped, [("opencode".to_string(), "spend_unknown")]);
    }

    #[test]
    fn a_ceiling_set_meanwhile_for_a_new_group_survives_the_hand_overs_writes() {
        // Break 2: the pass's writes pruned with the snapshot of its start and dropped a ceiling
        // the window set meanwhile for a group created after it.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let older = Snapshot::default();
        let mut newer = paid_snapshot(&dir, false);
        newer.projects[0].pool = "beta".into();
        newer.agent_keys.push(openrouter_key(Some("beta")));
        let beta = ChainScope::Project {
            pool: "beta".into(),
        };
        spend::set_ceiling(
            root.path(),
            &newer,
            beta.clone(),
            Some(3.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let done = HandOver {
            outcome: "offered",
            skipped: vec![],
        };
        record_last(root.path(), &older, WF, &done, T + 1);
        spend::open_question(root.path(), &older, &ChainScope::Machine, 500, 5.0, T + 1).unwrap();
        spend::record_baseline(
            root.path(),
            &older,
            WF2,
            &spend::KeySlot::Machine,
            Some(1.0),
            T + 1,
        )
        .unwrap();
        let file = spend::read(root.path()).unwrap();
        assert!(
            file.ceilings.iter().any(|c| c.scope == beta),
            "beta's ceiling survives writes made with an older snapshot"
        );
        assert!(file.last_handover.is_some());
    }

    #[tokio::test]
    async fn a_first_paid_hand_over_with_its_total_unread_leaves_a_later_task_ceiling_unknown() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        let count = Arc::new(AtomicUsize::new(0));
        // No ceiling and an unread total: it runs, and a marker is recorded.
        let unread = fixed(None, &count);
        let (done, _) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &unread,
            LIMIT,
            false,
        )
        .await;
        assert_eq!(done.outcome, "offered");
        let file = spend::read(root.path()).unwrap();
        assert_eq!(file.baselines.len(), 1);
        assert_eq!(file.baselines[0].usage, None);
        // A task ceiling set later: the spend since that first hand-over is unknown, not zero.
        let task = ChainScope::Workflow { id: WF.into() };
        spend::set_ceiling(root.path(), &snapshot, task, Some(1.0), Surface::Cli, T).unwrap();
        let read = fixed(spend(40.0, 30.0, 0.1), &count);
        let (done, acted) = walk(
            root.path(),
            &snapshot,
            &dir,
            WF,
            &[hermes()],
            &read,
            LIMIT,
            false,
        )
        .await;
        assert!(acted.is_empty());
        assert_eq!(done.outcome, "spend_unknown");
    }

    #[tokio::test]
    async fn two_slow_paid_steps_end_inside_the_passes_gate_budget() {
        // Risk 7, scaled down: a 2 s budget, 1.5 s per read, reads that answer after 3 s. Each
        // step alone would take its full 1.5 s (3 s together); the budget ends both by 2 s.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let snapshot = paid_snapshot(&dir, false);
        machine_ceiling(root.path(), &snapshot, 5.0);
        let slow = |_: Option<String>| async {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            spend(40.0, 1.0, 1.0)
        };
        let act = |_: String, _: String, _: PathBuf| async { Ok::<(), String>(()) };
        let (budget, cap) = (
            std::time::Duration::from_secs(2),
            std::time::Duration::from_millis(1500),
        );
        let started = tokio::time::Instant::now();
        let budget_end = started + budget;
        let gate = |_: String, account: String| {
            let (root, snapshot, dir, slow) = (root.path(), &snapshot, &dir, &slow);
            async move {
                spend_gate(
                    root,
                    snapshot,
                    WF,
                    dir,
                    &account,
                    T,
                    slow,
                    read_limit(budget_end, cap),
                )
                .await
            }
        };
        let done = hand_over(&act, &gate, WF, &dir, &[hermes(), opencode()]).await;
        assert_eq!(done.outcome, "spend_unknown");
        assert_eq!(done.skipped.len(), 2);
        let took = started.elapsed();
        assert!(
            took <= budget + std::time::Duration::from_millis(600),
            "two slow steps took {took:?}, past the {budget:?} budget"
        );
    }

    /// A runtime with the machine's OpenRouter key saved (a synthetic value) and, with `cap`, a
    /// machine ceiling of that many dollars.
    async fn paid_runtime(root: &std::path::Path, cap: Option<f64>) -> Arc<Runtime> {
        use crate::fixtures::*;
        let runtime = crate::fixtures::runtime(root, signed_out, activates).await;
        runtime
            .store
            .set_agent_key_for("openrouter", None, "sk-or-fixture-only", None)
            .unwrap();
        if let Some(usd) = cap {
            let snapshot = runtime.store.snapshot().unwrap();
            machine_ceiling(root, &snapshot, usd);
        }
        runtime
    }
    fn test_read(
        answer: Option<KeyRead>,
        count: &Arc<AtomicUsize>,
    ) -> Arc<dyn Fn(Option<String>) -> Option<KeyRead> + Send + Sync> {
        let count = count.clone();
        Arc::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            answer.clone()
        })
    }
    fn continue_on(id: &str, dir: &std::path::Path, mode: &str) -> Operation {
        Operation::Continue {
            workflow_id: WF.into(),
            id: id.into(),
            mode: mode.into(),
            working_directory: dir.to_owned(),
            in_place: false,
        }
    }

    #[tokio::test]
    async fn a_paid_continue_by_hand_past_a_reached_machine_ceiling_is_refused_and_offers_nothing()
    {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        let count = Arc::new(AtomicUsize::new(0));
        let spent = test_read(spend(40.0, 30.0, 6.0), &count);
        for id in [crate::hermes::HERMES_TARGET, crate::launch::OPENCODE_TARGET] {
            let refused = TEST_READ
                .scope(
                    spent.clone(),
                    runtime.execute(continue_on(id, &dir, "isolated")),
                )
                .await
                .unwrap_err();
            assert_eq!(refused, spend::STOP_CEILING_REACHED, "{id}");
        }
        assert_eq!(open_questions(root.path()), 1, "one question for the value");
        // An API-key Claude account: its spend cannot be read, so under a ceiling it is unknown.
        let api = runtime
            .store
            .add(
                "API".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                switchboard_core::Credential::parse(
                    Provider::Claude,
                    AuthKind::ApiKey,
                    "fixture-only",
                )
                .unwrap(),
            )
            .unwrap();
        let refused = TEST_READ
            .scope(
                spent.clone(),
                runtime.execute(continue_on(&api.id, &dir, "isolated")),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_SPEND_UNKNOWN);
        // To the console: refused the same way, nothing queued.
        crate::handovers::set_enabled(root.path(), true).unwrap();
        let refused = TEST_READ
            .scope(
                spent.clone(),
                runtime.execute(Operation::ContinueToConsole {
                    workflow_id: WF.into(),
                    id: crate::hermes::HERMES_TARGET.into(),
                    working_directory: dir.clone(),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_CEILING_REACHED);
        assert!(crate::handovers::pending(root.path()).is_empty());
        // On the default agent: Hermes, refused the same way.
        crate::agent_settings::set_default(root.path(), Some(crate::hermes::HERMES_TARGET))
            .unwrap();
        let refused = TEST_READ
            .scope(
                spent.clone(),
                runtime.execute(continue_on("", &dir, "isolated")),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_CEILING_REACHED);
    }

    #[tokio::test]
    async fn a_paid_continue_by_hand_writes_the_tasks_baseline() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let runtime = paid_runtime(root.path(), None).await;
        let count = Arc::new(AtomicUsize::new(0));
        // The gate passes; "managed" is then refused for Hermes, before anything is offered.
        let answer = TEST_READ
            .scope(
                test_read(spend(40.0, 10.0, 1.0), &count),
                runtime.execute(continue_on(crate::hermes::HERMES_TARGET, &dir, "managed")),
            )
            .await
            .unwrap_err();
        assert_eq!(answer, crate::continuation::MANAGED_NOT_FOR_AGENT);
        assert_eq!(count.load(Ordering::SeqCst), 1, "the key was read once");
        let file = spend::read(root.path()).unwrap();
        assert_eq!(file.baselines.len(), 1);
        assert_eq!(
            (file.baselines[0].workflow.as_str(), file.baselines[0].usage),
            (WF, Some(10.0))
        );
    }

    #[tokio::test]
    async fn an_agent_launched_on_the_openrouter_key_past_a_ceiling_is_refused() {
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        let count = Arc::new(AtomicUsize::new(0));
        let refused = TEST_READ
            .scope(
                test_read(spend(40.0, 30.0, 6.0), &count),
                runtime.execute(Operation::LaunchAgent {
                    agent: "no-such-agent".into(),
                    pool: String::new(),
                    working_directory: dir.clone(),
                    via: crate::AgentVia::Openrouter,
                    model: None,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_CEILING_REACHED);
        // Through the proxy on an API-key account: its spend is unknown under the ceiling.
        let api = runtime
            .store
            .add(
                "API".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                switchboard_core::Credential::parse(
                    Provider::Claude,
                    AuthKind::ApiKey,
                    "fixture-only",
                )
                .unwrap(),
            )
            .unwrap();
        runtime
            .store
            .select(Provider::Claude, "default", &api.id)
            .unwrap();
        let refused = TEST_READ
            .scope(
                test_read(None, &count),
                runtime.execute(Operation::LaunchAgent {
                    agent: "no-such-agent".into(),
                    pool: "default".into(),
                    working_directory: dir,
                    via: crate::AgentVia::Proxy,
                    model: None,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_SPEND_UNKNOWN);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_automatic_fallback_gates_its_step_once_and_never_waits_on_itself() {
        // Break 1: with the gate in `Continue` too, the fallback's own step must neither take the
        // hand-over turn again (a deadlock) nor be gated a second time.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        // A machine ceiling with nothing readable: a second gate would refuse (spend_unknown).
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        let gates = Arc::new(AtomicUsize::new(0));
        let errors: Arc<Mutex<Vec<String>>> = Default::default();
        let act = |workflow: String, account: String, dir: PathBuf| {
            let (runtime, errors) = (runtime.clone(), errors.clone());
            async move {
                let answer = runtime
                    .execute(Operation::Continue {
                        workflow_id: workflow,
                        id: account,
                        mode: "managed".into(),
                        working_directory: dir,
                        in_place: false,
                    })
                    .await;
                if let Err(e) = &answer {
                    errors.lock().unwrap().push(e.clone());
                }
                answer.map(|_| ())
            }
        };
        let gate = |_: String, _: String| {
            let gates = gates.clone();
            async move {
                gates.fetch_add(1, Ordering::SeqCst);
                Gate::Run
            }
        };
        let done = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            hand_over(&act, &gate, WF, &dir, &[hermes()]),
        )
        .await
        .expect("the hand-over never waits on its own turn");
        assert_eq!(done.outcome, "every_step_refused");
        assert_eq!(gates.load(Ordering::SeqCst), 1);
        assert_eq!(
            errors.lock().unwrap().as_slice(),
            [crate::continuation::MANAGED_NOT_FOR_AGENT.to_string()],
            "gated once: the step reached the next refusal"
        );
    }

    /// A stand-in for `project-observatory` with two open workflows, `WF` and `WF2`, both executed
    /// by `account` in the checkout `repo`.
    #[cfg(unix)]
    fn engine_two(dir: &std::path::Path, account: &str, repo: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let one = |id: &str| {
            json!({"workflowId": id, "status": "open", "pendingHandoff": null,
            "silentSeconds": 600, "lease": {"executor": {"provider": "claude-code", "accountRef": account}}})
        };
        let list = json!({"workflows": [one(WF), one(WF2)]});
        let show = json!({"workflowId": WF, "status": "open", "pendingHandoff": null,
            "checkpoint": {"executor": {"provider": "claude-code"},
                "body": {"artifacts": [{"kind": "git", "path": repo}]}},
            "lease": {"executor": {"provider": "claude-code", "accountRef": account}},
            "credentials": []});
        let path = dir.join("project-observatory");
        std::fs::write(
            &path,
            format!("#!/bin/sh\ncase \"$3\" in\n list) cat <<'J'\n{list}\nJ\n;;\n show) cat <<'J'\n{show}\nJ\n;;\nesac\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_passes_gates_stay_inside_one_budget_across_its_workflows() {
        // Fix cycle 2, break 2: GATE_BUDGET bounds one pass's gates together, not each
        // workflow's. Scaled down: a 3 s budget, two workflows each with a paid step whose read
        // answers after 10 s. Budgeted per workflow the reads ran 3 s each, 6 s together; per
        // pass, 3 s together. Measured as the time spent inside the reads, since a hand-over
        // turn held by another test (it is one per process) may hold the pass up before them.
        use crate::fixtures::*;
        use crate::monitor::now;
        let root = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
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
        runtime
            .store
            .set_agent_key_for("openrouter", None, "sk-or-fixture-only", None)
            .unwrap();
        machine_ceiling(root.path(), &runtime.store.snapshot().unwrap(), 5.0);
        runtime
            .store
            .set_chain(
                ChainScope::Machine,
                vec![Executor {
                    agent: "hermes".into(),
                    account_id: None,
                    key: None,
                }],
            )
            .unwrap();
        let repo = tempfile::tempdir().unwrap();
        let bin = engine_two(root.path(), &a.id, &dir_of(&repo));
        let acted = Arc::new(AtomicUsize::new(0));
        let act = |_: String, _: String, _: PathBuf| {
            let acted = acted.clone();
            async move {
                acted.fetch_add(1, Ordering::SeqCst);
                Ok::<(), String>(())
            }
        };
        /// Adds the time a read ran, until it answered or was given up, to the total.
        struct Timed(Arc<Mutex<std::time::Duration>>, tokio::time::Instant);
        impl Drop for Timed {
            fn drop(&mut self) {
                *self.0.lock().unwrap() += self.1.elapsed();
            }
        }
        let reading: Arc<Mutex<std::time::Duration>> = Default::default();
        let reads = Arc::new(AtomicUsize::new(0));
        let slow = |_: Option<String>| {
            let (reads, reading) = (reads.clone(), reading.clone());
            async move {
                let _timed = Timed(reading, tokio::time::Instant::now());
                reads.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                spend(40.0, 1.0, 1.0)
            }
        };
        let budget = std::time::Duration::from_secs(3);
        TEST_HERMES_READY
            .scope(
                (),
                pass_with_reads(&runtime, now(), &bin, act, slow, budget),
            )
            .await;
        let took = *reading.lock().unwrap();
        assert_eq!(
            reads.load(Ordering::SeqCst),
            2,
            "both workflows' steps were gated"
        );
        assert_eq!(
            acted.load(Ordering::SeqCst),
            0,
            "unknown spend hands nothing over"
        );
        assert!(
            took < budget + std::time::Duration::from_millis(1500),
            "two workflows' reads ran {took:?}, past the pass's {budget:?} budget"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_slow_paid_gate_by_hand_holds_up_neither_a_read_nor_a_subscription_launch() {
        // Fix cycle 2, risk 3: a paid start by hand takes the hand-over turn and passes its gate
        // before the owner transaction, and a start that is not paid takes no turn at all.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        let sub = crate::fixtures::save(&runtime.store, "synthetic-sub", "default");
        let reading = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let slow: Arc<dyn Fn(Option<String>) -> Option<KeyRead> + Send + Sync> = {
            let reading = reading.clone();
            Arc::new(move |_| {
                reading.store(true, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_secs(10));
                spend(40.0, 1.0, 1.0)
            })
        };
        let paid = runtime.execute(continue_on(crate::hermes::HERMES_TARGET, &dir, "managed"));
        let probes = async {
            let waited = tokio::time::Instant::now();
            while !reading.load(Ordering::SeqCst) {
                assert!(
                    waited.elapsed() < std::time::Duration::from_secs(5),
                    "the paid gate never read"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            let second = std::time::Duration::from_secs(1);
            tokio::time::timeout(second, runtime.execute(Operation::Snapshot))
                .await
                .expect("a snapshot answers within 1 s")
                .unwrap();
            // A subscription launch with arguments it refuses: an answer, not a start.
            let answer = tokio::time::timeout(
                second,
                runtime.execute(Operation::Launch {
                    id: sub.id.clone(),
                    mode: "isolated".into(),
                    working_directory: dir.clone(),
                    in_place: false,
                    args: vec!["--x".into()],
                }),
            )
            .await
            .expect("a subscription launch answers within 1 s");
            assert_eq!(answer.unwrap_err(), crate::launch::EXTRA_ARGS_INVALID);
        };
        let (paid, ()) = TEST_READ
            .scope(slow, async { tokio::join!(paid, probes) })
            .await;
        // The gate passed; "managed" is then refused for Hermes, before anything is offered.
        assert_eq!(
            paid.unwrap_err(),
            crate::continuation::MANAGED_NOT_FOR_AGENT
        );
    }

    #[tokio::test]
    async fn a_queued_paid_hand_over_is_gated_again_when_it_is_taken() {
        // Fix cycle 2, risk 4: a to-console hand-over passed the gate when it was queued; the
        // ceiling reached since then refuses it when a console takes it.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dir = dir_of(&repo);
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        crate::handovers::set_enabled(root.path(), true).unwrap();
        let handover = crate::handovers::Handover {
            id: crate::handovers::new_id(),
            workflow: WF.into(),
            agent: "hermes".into(),
            account_label: "Hermes Agent".into(),
            dir: dir.clone(),
            script: root.path().join("hermes.command"),
            offered_until: "2099-10-13T01:00:00Z".into(),
            created_at: crate::monitor::now(),
            paid: Some(Paid::OpenRouter),
        };
        crate::handovers::record(root.path(), &handover).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let refused = TEST_READ
            .scope(
                test_read(spend(40.0, 30.0, 6.0), &count),
                runtime.execute(Operation::HandoverTake {
                    id: handover.id.clone(),
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, spend::STOP_CEILING_REACHED);
        assert_eq!(count.load(Ordering::SeqCst), 1, "the key was read");
        assert_eq!(
            crate::handovers::pending(root.path()),
            vec![handover.clone()],
            "still waiting: the window may double the ceiling"
        );
        // Under the ceiling it is taken as before.
        let taken = TEST_READ
            .scope(
                test_read(spend(40.0, 30.0, 1.0), &count),
                runtime.execute(Operation::HandoverTake {
                    id: handover.id.clone(),
                }),
            )
            .await
            .unwrap();
        assert_eq!(taken["handover"]["id"], handover.id);
    }

    #[tokio::test]
    async fn a_proxy_launch_on_a_subscription_account_is_refused_as_such_under_a_ceiling() {
        // Fix cycle 2, risk 5: the route's account is read first; a subscription account is
        // refused for what it is, never as an unknown spend.
        let (root, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let runtime = paid_runtime(root.path(), Some(5.0)).await;
        let oauth = crate::fixtures::save(&runtime.store, "synthetic-a", "agents");
        runtime
            .store
            .select(Provider::Claude, "agents", &oauth.id)
            .unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let refused = TEST_READ
            .scope(
                test_read(None, &count),
                runtime.execute(Operation::LaunchAgent {
                    agent: "goose".into(),
                    pool: "agents".into(),
                    working_directory: dir_of(&repo),
                    via: crate::AgentVia::Proxy,
                    model: None,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(refused, switchboard_proxy::AGENT_SUBSCRIPTION_REFUSED);
    }
}
