//! Automatic fallback (SB-73, packet XA-01; operator decision D-1, 2026-10-06): when the executor
//! of an open Observatory workflow has hit its limit, the workflow is handed to the next usable
//! agent of the operator's chain for it (SB-71) and that agent's session is launched — the same
//! offer and launch as `switchboard continue`, so every refusal of the manual path applies.
//!
//! Nothing happens without a chain: none is applied until the operator sets one. One offer per
//! workflow per window; the old session is never stopped; the engine's own rules (the executor
//! silent for 120 s, one offer a minute, one executor by lease) stay the authority.
//!
//! Phase 1 hands over to Claude Code and Codex. Another agent in a chain (Kimi Code, Hermes, …) is
//! skipped with its reason until its launch recipe and paid-key ceilings exist (SB-72, SB-73 phase 2).
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

/// Resolves the chain in order, never to the executor's own account, a disabled one, one with a
/// recent limit, or one that needs a new sign-in — nor to another saved row of the same identity
/// as the executor's or a limited account (one login saved twice shares one limit, SB-62). A
/// pinned account is used only when it is usable.
pub(crate) fn resolve(
    snapshot: &Snapshot,
    chain: &Chain,
    executor_account: Option<&str>,
    limited: &HashSet<String>,
    sign_in: &HashSet<String>,
) -> Vec<Step> {
    let spent: HashSet<(Provider, &str)> = snapshot
        .accounts
        .iter()
        .filter(|a| Some(a.id.as_str()) == executor_account || limited.contains(&a.id))
        .filter_map(|a| a.identity.as_deref().map(|i| (a.provider, i)))
        .collect();
    let usable = |a: &Account| {
        a.enabled
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
    if let Some(id) = executor.get("accountRef").and_then(Value::as_str) {
        return (snapshot.accounts.iter().any(|a| a.id == id) && limited.contains(id))
            .then(|| (provider, Some(id.to_owned())));
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
    pass_with(runtime, now, &bin, |workflow, account, dir| async move {
        runtime
            .execute(Operation::Continue {
                workflow_id: workflow,
                id: account,
                mode: "isolated".into(),
                working_directory: dir,
            })
            .await
            .map(|_| ())
    })
    .await
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
    let limited = runtime.limits.limited_ids(now);
    if limited.is_empty() || !runtime.fallback.due(now) {
        return;
    }
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
        let Some((_, executor_account)) =
            executor_limited(&executor, &snapshot, &current, &limited, &launched)
        else {
            continue;
        };
        examined += 1;
        let pool = executor_account.as_deref().and_then(|a| {
            snapshot
                .accounts
                .iter()
                .find(|x| x.id == a)
                .map(|x| x.pool.clone())
        });
        let chain = match runtime.store.chain_for(Some(id), pool.as_deref()) {
            Ok(Some(chain)) => chain,
            _ => continue,
        };
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
        let steps = resolve(
            &snapshot,
            &chain,
            executor_account.as_deref(),
            &limited,
            &sign_in,
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
                    reason: "agent_not_automatic_yet"
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
