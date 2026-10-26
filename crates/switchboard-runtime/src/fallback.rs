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
//! its launch recipe and paid-key ceilings exist (SB-72, SB-96, SB-73 phase 2).
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
            // project's key waits for SB-72/SB-96.
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
    let handover = crate::handovers::Handover {
        id: crate::handovers::new_id(),
        workflow,
        agent,
        account_label: label,
        dir,
        script: script.clone(),
        offered_until: answer["offeredUntil"].as_str().unwrap_or_default().into(),
        created_at: now,
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
/// none the executor's, the executor account's group, and `true` says the choice was ambiguous;
/// with no group holding it, the executor account's group as before.
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
        _ => (own, true),
    }
}
/// The pass with the hand-over as a parameter: the monitor passes `switchboard continue`'s own
/// operation, a test a recorder.
pub(crate) async fn pass_with<F, Fut>(runtime: &Runtime, now: i64, bin: &std::path::Path, act: F)
where
    F: Fn(String, String, PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
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
            && crate::launch::hermes_continuation_ready(&runtime.store),
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
        runtime.fallback.mark(id, now);
        let shown = {
            let (bin, id) = (bin.to_path_buf(), id.to_owned());
            tokio::task::spawn_blocking(move || {
                continuation::show_within(&bin, &id, MONITOR_ENGINE_TIMEOUT)
            })
            .await
        };
        let Ok(Ok(shown)) = shown else {
            log("engine_unreadable");
            continue;
        };
        let Some(dir) = continuation::checkouts(&shown).into_iter().next() else {
            log("no_checkout");
            continue;
        };
        let (pool, ambiguous) = chain_pool(&snapshot, executor_account.as_deref(), &dir);
        if ambiguous {
            log("group_ambiguous");
        }
        let chain = match runtime.store.chain_for(Some(id), pool.as_deref()) {
            Ok(Some(chain)) => chain,
            _ => continue,
        };
        let steps = resolve_with(
            &snapshot,
            &chain,
            executor_account.as_deref(),
            &limited,
            &sign_in,
            Some(&dir),
            &kimi,
        );
        let outcome = hand_over(&act, id, &dir, &steps).await;
        log(outcome);
        let _ = runtime.store.record(
            "fallback",
            None,
            if outcome == "offered" {
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

/// Tries the chain's steps in order. A refusal before the offer moves on to the next step; once an
/// offer exists (the launch failed after it) nothing else is tried — the offer lapses by itself.
async fn hand_over<F, Fut>(
    act: &F,
    workflow: &str,
    dir: &std::path::Path,
    steps: &[Step],
) -> &'static str
where
    F: Fn(String, String, PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let mut skipped_only = true;
    for step in steps {
        let Step::Run { account_id, .. } = step else {
            continue;
        };
        skipped_only = false;
        match act(workflow.to_owned(), account_id.clone(), dir.to_path_buf()).await {
            Ok(_) => return "offered",
            Err(error) if error.starts_with("Offered ") => return "launch_failed",
            Err(_) => continue,
        }
    }
    if skipped_only {
        "no_usable_executor"
    } else {
        "every_step_refused"
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
    fn a_folder_in_two_groups_keeps_the_executors_group_and_logs_ambiguous() {
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
        let snapshot = Snapshot {
            accounts: vec![account("cli", Provider::Claude, None), in_beta],
            projects: vec![
                group("alpha", "/work/shared"),
                group("beta", "/work/shared"),
                group("gamma", "/work/gamma"),
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
        // Several groups hold it and the executor's is none of them: the executor's group
        // (default here) stays, and the pass logs `group_ambiguous`.
        assert_eq!(
            pool_for(Some("cli"), "/work/shared"),
            (Some("default".into()), true)
        );
        // No group holds it: as before, the executor's group.
        assert_eq!(
            pool_for(Some("in-beta"), "/elsewhere"),
            (Some("beta".into()), false)
        );
        assert_eq!(pool_for(None, "/elsewhere"), (None, false));
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
        assert_eq!(
            hand_over(&act, WF, dir.path(), &steps).await,
            "launch_failed"
        );
        assert_eq!(*tried.lock().unwrap(), ["refuses", "offered-then-failed"]);
        let only_skips = vec![Step::Skip {
            agent: "hermes".into(),
            reason: "agent_not_automatic_yet",
        }];
        assert_eq!(
            hand_over(&act, WF, dir.path(), &only_skips).await,
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
}
