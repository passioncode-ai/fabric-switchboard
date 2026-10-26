//! What each step of a fallback chain would do now (SB-71 screen, SB-73 status; iteration 0.7.0
//! dossier C part A, node C-2): one read for the window, the CLI and MCP. Built on the fallback's
//! own resolution (`fallback::resolve_with`, unchanged rules) with no executor and no workflow, so
//! the view names exactly what the automatic fallback would weigh. A read never starts the
//! workflow engine, a hand-over or a network call: a paid step shows `spend: null` (not read yet)
//! until the hand-over gate records a read (C-5).
use crate::fallback::{KimiChoice, Step};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use switchboard_core::{AuthKind, Chain, ChainScope, Snapshot};

/// What the view weighs besides the snapshot; the owner fills it from its own state, a test with
/// fixtures.
pub(crate) struct Inputs<'a> {
    pub limited: &'a HashSet<String>,
    pub sign_in: &'a HashSet<String>,
    pub kimi: &'a KimiChoice<'a>,
    /// Whether the agent's program is on this computer (the lookup launches use).
    pub installed: &'a dyn Fn(&str) -> bool,
    /// `agents-settings.json` could be read; the fallback hands nothing over when it cannot.
    pub settings_readable: bool,
}

/// The key `readiness` files a chain under: `machine`, `project:<pool>` or `workflow:<id>`, the
/// CLI's own scope words.
pub(crate) fn scope_key(scope: &ChainScope) -> String {
    match scope {
        ChainScope::Machine => "machine".into(),
        ChainScope::Project { pool } => format!("project:{pool}"),
        ChainScope::Workflow { id } => format!("workflow:{id}"),
    }
}

/// The folder a scope's readiness is read for: a group's first folder, so the group's account
/// rules (SB-102) are honoured; none for this computer or a task (its checkout is the engine's).
pub(crate) fn folder_for(snapshot: &Snapshot, scope: &ChainScope) -> Option<PathBuf> {
    match scope {
        ChainScope::Project { pool } => snapshot
            .projects
            .iter()
            .find(|p| &p.pool == pool)
            .and_then(|p| p.folders.first())
            .map(PathBuf::from),
        _ => None,
    }
}

/// The OpenRouter key a Hermes or OpenCode step in `folder` spends (`Store::agent_key_in`'s
/// rule): the group's own key, else this computer's; none when neither is saved.
fn key_scope(snapshot: &Snapshot, folder: Option<&Path>) -> Option<&'static str> {
    let openrouter = |pool: Option<&str>| {
        snapshot
            .agent_keys
            .iter()
            .any(|k| k.service == "openrouter" && k.pool.as_deref() == pool)
    };
    let group = folder
        .and_then(|f| snapshot.project_for(f))
        .map(|p| p.pool.as_str());
    if group.is_some_and(|g| openrouter(Some(g))) {
        Some("group")
    } else if openrouter(None) {
        Some("machine")
    } else {
        None
    }
}

/// One view per step of `chain`, in order.
pub(crate) fn preview(
    snapshot: &Snapshot,
    chain: &Chain,
    folder: Option<&Path>,
    inputs: &Inputs,
) -> Vec<Value> {
    let catalog = crate::agent_catalog::catalog();
    let name = |agent: &str| {
        catalog["agents"]
            .as_array()
            .and_then(|all| all.iter().find(|a| a["id"] == agent))
            .map(|a| a["name"].clone())
            .unwrap_or(Value::Null)
    };
    let resolved = crate::fallback::resolve_with(
        snapshot,
        chain,
        None,
        inputs.limited,
        inputs.sign_in,
        folder,
        inputs.kimi,
    );
    chain
        .executors
        .iter()
        .zip(resolved)
        .map(|(e, step)| {
            let mut view = serde_json::Map::new();
            view.insert("agent".into(), Value::String(e.agent.clone()));
            view.insert("name".into(), name(&e.agent));
            let (state, reason) = if !inputs.settings_readable {
                // The fallback hands nothing over while agents-settings.json cannot be read.
                ("skipped", Some("agent_settings_unreadable"))
            } else if let Some(k) = &e.key {
                // A vault name, never a value; waits for the project's grant (SB-96).
                view.insert("key".into(), Value::String(k.clone()));
                ("not_automatic", Some("key_pinned"))
            } else if !crate::agent_catalog::switchable(&e.agent) {
                ("not_automatic", Some("agent_not_automatic_yet"))
            } else if inputs.kimi.disabled.contains(&e.agent) {
                ("skipped", Some("agent_disabled"))
            } else if !(inputs.installed)(&e.agent) {
                ("not_installed", None)
            } else {
                match step {
                    Step::Skip { reason, .. } => {
                        let signed_out = reason == "no_usable_account"
                            && e.account_id
                                .as_ref()
                                .is_some_and(|id| inputs.sign_in.contains(id));
                        (
                            "skipped",
                            Some(if signed_out {
                                "sign_in_required"
                            } else {
                                reason
                            }),
                        )
                    }
                    Step::Run { account_id, .. } => {
                        ready(snapshot, folder, &e.agent, &account_id, &mut view);
                        ("ready", None)
                    }
                }
            };
            view.insert("state".into(), Value::String(state.into()));
            view.insert(
                "reason".into(),
                reason.map_or(Value::Null, |r| Value::String(r.into())),
            );
            view.entry("paid").or_insert(Value::Bool(false));
            Value::Object(view)
        })
        .collect()
}

/// A ready step's account, and for a paid step the key it spends; its spend is read only at a
/// hand-over (C-5), so a view says `spend: null`.
fn ready(
    snapshot: &Snapshot,
    folder: Option<&Path>,
    agent: &str,
    account_id: &str,
    view: &mut serde_json::Map<String, Value>,
) {
    let openrouter =
        agent == crate::hermes::HERMES_TARGET || agent == crate::launch::OPENCODE_TARGET;
    if openrouter {
        view.insert("paid".into(), Value::Bool(true));
        view.insert(
            "key_scope".into(),
            key_scope(snapshot, folder).map_or(Value::Null, |k| Value::String(k.into())),
        );
        view.insert("spend".into(), Value::Null);
        return;
    }
    let label = snapshot
        .accounts
        .iter()
        .find(|a| a.id == account_id)
        .map(|a| (a.label.clone(), a.kind == AuthKind::ApiKey))
        .or_else(|| {
            snapshot
                .kimi_accounts
                .iter()
                .find(|k| k.id == account_id)
                .map(|k| (k.label.clone(), false))
        });
    view.insert("account_id".into(), Value::String(account_id.into()));
    if let Some((label, api_key)) = label {
        view.insert("account_label".into(), Value::String(label));
        // C-Q3: an API-key Claude Code or Codex account spends money Switchboard cannot read.
        view.insert("paid".into(), Value::Bool(api_key));
    }
}

/// Every chain set, keyed by `scope_key`.
pub(crate) fn readiness(snapshot: &Snapshot, inputs: &Inputs) -> serde_json::Map<String, Value> {
    snapshot
        .chains
        .iter()
        .map(|c| {
            let folder = folder_for(snapshot, &c.scope);
            (
                scope_key(&c.scope),
                Value::Array(preview(snapshot, c, folder.as_deref(), inputs)),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::agent_keys::AgentKey;
    use switchboard_core::{Account, Executor, Project, Provider};

    fn account(id: &str, provider: Provider, kind: AuthKind, pool: &str) -> Account {
        Account {
            id: id.into(),
            label: format!("{id} label"),
            provider,
            kind,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        }
    }
    fn group(pool: &str) -> Project {
        Project {
            pool: pool.into(),
            name: pool.into(),
            folders: vec![format!("/work/{pool}"), format!("/work/{pool}-second")],
            created_at: 1,
            sources: vec![],
            organization: None,
        }
    }
    fn key(pool: Option<&str>) -> AgentKey {
        AgentKey {
            service: "openrouter".into(),
            id: format!("key-{}", pool.unwrap_or("machine")),
            saved_at: 1,
            pool: pool.map(str::to_owned),
            model: None,
        }
    }
    fn step(agent: &str) -> Executor {
        Executor {
            agent: agent.into(),
            account_id: None,
            key: None,
        }
    }
    fn chain(scope: ChainScope, executors: Vec<Executor>) -> Chain {
        Chain {
            scope,
            executors,
            updated_at: 1,
        }
    }
    /// Groups alpha (its own OpenRouter key, one Codex account) and beta; a machine key; a
    /// Claude subscription and a Claude API-key account outside every group.
    fn snapshot() -> Snapshot {
        Snapshot {
            accounts: vec![
                account("claude-sub", Provider::Claude, AuthKind::OAuth, "default"),
                account("claude-api", Provider::Claude, AuthKind::ApiKey, "default"),
                account("codex-alpha", Provider::Codex, AuthKind::OAuth, "alpha"),
            ],
            projects: vec![group("alpha"), group("beta")],
            agent_keys: vec![key(None), key(Some("alpha"))],
            ..Default::default()
        }
    }
    struct Fixture {
        limited: HashSet<String>,
        sign_in: HashSet<String>,
        disabled: HashSet<String>,
        hermes_ready: bool,
        missing: HashSet<String>,
        settings_readable: bool,
    }
    impl Fixture {
        fn new() -> Self {
            Fixture {
                limited: HashSet::new(),
                sign_in: HashSet::new(),
                disabled: HashSet::new(),
                hermes_ready: true,
                missing: HashSet::new(),
                settings_readable: true,
            }
        }
        fn run(&self, snapshot: &Snapshot, chain: &Chain, folder: Option<&Path>) -> Vec<Value> {
            let kimi = KimiChoice {
                ids: vec![],
                usable: &|_| false,
                used: &|_| 0.0,
                hermes_ready: self.hermes_ready,
                opencode_ready: false,
                disabled: self.disabled.clone(),
            };
            let installed = |agent: &str| !self.missing.contains(agent);
            let inputs = Inputs {
                limited: &self.limited,
                sign_in: &self.sign_in,
                kimi: &kimi,
                installed: &installed,
                settings_readable: self.settings_readable,
            };
            preview(snapshot, chain, folder, &inputs)
        }
    }

    #[test]
    fn a_key_pinned_step_reads_not_automatic_and_a_disabled_agent_reads_disabled() {
        let s = snapshot();
        let mut f = Fixture::new();
        f.disabled.insert("codex".into());
        let pinned = Executor {
            agent: "hermes".into(),
            account_id: None,
            key: Some("alpha/prod/OPENROUTER_API_KEY".into()),
        };
        let c = chain(
            ChainScope::Machine,
            vec![step("codex"), pinned, step("aider"), step("claude-code")],
        );
        let steps = f.run(&s, &c, None);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["agent"], "codex");
        assert_eq!(steps[0]["state"], "skipped");
        assert_eq!(steps[0]["reason"], "agent_disabled");
        // A key pin is kept and named (a vault name, never a value), waiting for SB-96.
        assert_eq!(steps[1]["state"], "not_automatic");
        assert_eq!(steps[1]["reason"], "key_pinned");
        assert_eq!(steps[1]["key"], "alpha/prod/OPENROUTER_API_KEY");
        // An agent the fallback cannot run yet (set from the CLI before 0.7.0).
        assert_eq!(steps[2]["state"], "not_automatic");
        assert_eq!(steps[2]["reason"], "agent_not_automatic_yet");
        // The least used usable Claude account; names come from the catalog.
        assert_eq!(steps[3]["state"], "ready");
        assert_eq!(steps[3]["name"], "Claude Code");
        assert!(steps[3]["account_id"].is_string());
        assert!(steps[3].get("reason").is_none_or(Value::is_null));
    }

    #[test]
    fn a_ready_step_names_its_account_and_a_paid_step_its_key_with_no_spend_read() {
        let s = snapshot();
        let f = Fixture::new();
        let pinned_api = Executor {
            agent: "claude-code".into(),
            account_id: Some("claude-api".into()),
            key: None,
        };
        let c = chain(
            ChainScope::Project {
                pool: "alpha".into(),
            },
            vec![step("codex"), pinned_api, step("hermes")],
        );
        let alpha = Path::new("/work/alpha");
        let steps = f.run(&s, &c, Some(alpha));
        assert_eq!(steps[0]["state"], "ready");
        assert_eq!(steps[0]["account_id"], "codex-alpha");
        assert_eq!(steps[0]["account_label"], "codex-alpha label");
        assert_eq!(steps[0]["paid"], false);
        // An API-key account is paid (C-Q3); it spends no OpenRouter key.
        assert_eq!(steps[1]["state"], "ready");
        assert_eq!(steps[1]["paid"], true);
        assert!(steps[1].get("key_scope").is_none_or(Value::is_null));
        // Hermes in alpha's folder spends alpha's own key; its spend is not read by a view.
        assert_eq!(steps[2]["state"], "ready");
        assert_eq!(steps[2]["paid"], true);
        assert_eq!(steps[2]["key_scope"], "group");
        assert_eq!(steps[2]["spend"], Value::Null);
        // On this computer's chain Hermes spends the machine key.
        let machine = chain(ChainScope::Machine, vec![step("hermes")]);
        assert_eq!(f.run(&s, &machine, None)[0]["key_scope"], "machine");
    }

    #[test]
    fn a_groups_readiness_follows_its_first_folder_and_its_account_rules() {
        let s = snapshot();
        let f = Fixture::new();
        // Beta's chain: Codex has only alpha's reserved account, which never serves beta.
        let beta = ChainScope::Project {
            pool: "beta".into(),
        };
        assert_eq!(
            folder_for(&s, &beta).as_deref(),
            Some(Path::new("/work/beta"))
        );
        assert_eq!(folder_for(&s, &ChainScope::Machine), None);
        assert_eq!(
            folder_for(
                &s,
                &ChainScope::Workflow {
                    id: "wf_0123456789abcdef".into()
                }
            ),
            None
        );
        let mut with_chain = s.clone();
        with_chain.chains = vec![
            chain(beta.clone(), vec![step("codex")]),
            chain(ChainScope::Machine, vec![step("codex")]),
        ];
        let kimi = KimiChoice {
            ids: vec![],
            usable: &|_| false,
            used: &|_| 0.0,
            hermes_ready: false,
            opencode_ready: false,
            disabled: HashSet::new(),
        };
        let inputs = Inputs {
            limited: &f.limited,
            sign_in: &f.sign_in,
            kimi: &kimi,
            installed: &|_| true,
            settings_readable: true,
        };
        let all = readiness(&with_chain, &inputs);
        assert_eq!(all["project:beta"][0]["state"], "skipped");
        assert_eq!(all["project:beta"][0]["reason"], "no_usable_account");
        // This computer's chain is read with no folder: no group's account rules apply.
        assert_eq!(all["machine"][0]["state"], "ready");
    }

    #[test]
    fn not_installed_a_sign_in_and_unreadable_agent_settings_are_named() {
        let s = snapshot();
        let mut f = Fixture::new();
        f.missing.insert("codex".into());
        f.sign_in.insert("claude-api".into());
        let pinned = Executor {
            agent: "claude-code".into(),
            account_id: Some("claude-api".into()),
            key: None,
        };
        let c = chain(ChainScope::Machine, vec![step("codex"), pinned]);
        let steps = f.run(&s, &c, None);
        assert_eq!(steps[0]["state"], "not_installed");
        assert_eq!(steps[1]["state"], "skipped");
        assert_eq!(steps[1]["reason"], "sign_in_required");
        // A limited pinned account has no room.
        let mut g = Fixture::new();
        g.limited.insert("claude-api".into());
        let pinned = Executor {
            agent: "claude-code".into(),
            account_id: Some("claude-api".into()),
            key: None,
        };
        let one = chain(ChainScope::Machine, vec![pinned]);
        assert_eq!(g.run(&s, &one, None)[0]["reason"], "no_usable_account");
        // agents-settings.json unreadable: the fallback hands nothing over, and says so.
        let mut h = Fixture::new();
        h.settings_readable = false;
        let read = h.run(&s, &c, None);
        assert!(read
            .iter()
            .all(|v| v["state"] == "skipped" && v["reason"] == "agent_settings_unreadable"));
    }
}
