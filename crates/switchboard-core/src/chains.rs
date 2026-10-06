//! Fallback chains (SB-71, packet XA-01): which agents continue a workflow, in what order, when
//! its executor's accounts run out. The operator sets them per machine, per project (its pool) and
//! per task (one workflow); the narrowest set chain applies. A chain names agents by catalog id;
//! an executor may pin an account or a paid key by name, or leave the choice to Switchboard at the
//! moment of the switch. Which agents may be named is the runtime's catalog check, not this one.
use crate::{append_event, now, AuthKind, Provider, Snapshot, Store};
use serde::{Deserialize, Serialize};

/// At most this many chains (one per scope), and this many executors in one chain.
pub const MAX_CHAINS: usize = 256;
pub const MAX_EXECUTORS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChainScope {
    /// This machine's default.
    Machine,
    /// A project, named by its pool.
    Project { pool: String },
    /// One task: an Observatory workflow, `wf_` and 16 hex digits.
    Workflow { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executor {
    /// The agent's catalog id (`claude-code`, `codex`, `kimi-code`, `hermes`, …).
    pub agent: String,
    /// A saved account to run it on; none lets Switchboard choose one at the switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// A paid key by its Observatory vault name (`<project>/<env>/<NAME>`); never a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chain {
    pub scope: ChainScope,
    pub executors: Vec<Executor>,
    pub updated_at: i64,
}

/// The agent a Switchboard provider's own client is, in the catalog.
pub fn provider_agent(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude-code",
        Provider::Codex => "codex",
    }
}

fn agent_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn workflow_id_valid(id: &str) -> bool {
    id.len() == 19
        && id.starts_with("wf_")
        && id[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An Observatory vault name: `project/env/NAME`, three non-empty plain segments.
fn key_name_valid(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    name.len() <= 160
        && parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
        })
}

/// One executor against the saved accounts. A provider's own client runs on one of that
/// provider's accounts; any other agent runs on an API-key account or a paid key — never on a
/// subscription sign-in, which is for the provider's own client (the catalog's subscription rule).
fn validate_executor(s: &Snapshot, e: &Executor) -> Result<(), String> {
    if !agent_id_valid(&e.agent) {
        return Err("Name the agent by its catalog id (switchboard agents list).".into());
    }
    if e.account_id.is_some() && e.key.is_some() {
        return Err("Give an executor an account or a key, not both.".into());
    }
    if let Some(key) = &e.key {
        if !key_name_valid(key) {
            return Err("Name the key as its Observatory vault name: project/env/NAME.".into());
        }
        if e.agent == "claude-code" || e.agent == "codex" {
            return Err("Claude Code and Codex run on saved accounts, not on a key.".into());
        }
    }
    if let Some(id) = &e.account_id {
        let account = s
            .accounts
            .iter()
            .find(|a| &a.id == id)
            .ok_or("Account not found")?;
        let own = [Provider::Claude, Provider::Codex]
            .into_iter()
            .find(|p| provider_agent(*p) == e.agent);
        match own {
            Some(Provider::Claude) if account.provider != Provider::Claude => {
                return Err("Claude Code runs on a Claude account.".into())
            }
            Some(Provider::Codex) if account.provider != Provider::Codex => {
                return Err("Codex runs on a Codex account.".into())
            }
            None if account.kind != AuthKind::ApiKey => {
                return Err(
                    "Another agent runs on an API-key account or a paid key, never on a subscription sign-in."
                        .into(),
                )
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_chains(s: &Snapshot) -> Result<(), String> {
    if s.chains.len() > MAX_CHAINS {
        return Err("Invalid metadata bounds".into());
    }
    let mut scopes = std::collections::HashSet::new();
    for c in &s.chains {
        if c.executors.is_empty() || c.executors.len() > MAX_EXECUTORS || c.updated_at <= 0 {
            return Err("Invalid fallback chain metadata".into());
        }
        if !scopes.insert(&c.scope) {
            return Err("Invalid fallback chain metadata".into());
        }
        match &c.scope {
            ChainScope::Machine => {}
            ChainScope::Project { pool } => {
                if !s.projects.iter().any(|p| &p.pool == pool) {
                    return Err("Invalid fallback chain metadata".into());
                }
            }
            ChainScope::Workflow { id } => {
                if !workflow_id_valid(id) {
                    return Err("Invalid fallback chain metadata".into());
                }
            }
        }
        for e in &c.executors {
            validate_executor(s, e).map_err(|_| "Invalid fallback chain metadata".to_string())?;
        }
    }
    Ok(())
}

/// Drops what a removed account or project leaves behind: executors that pinned the account, and
/// chains left empty or scoped to a project that no longer exists.
pub(crate) fn prune(s: &mut Snapshot) {
    let accounts: std::collections::HashSet<String> =
        s.accounts.iter().map(|a| a.id.clone()).collect();
    let pools: std::collections::HashSet<String> =
        s.projects.iter().map(|p| p.pool.clone()).collect();
    for c in s.chains.iter_mut() {
        c.executors
            .retain(|e| e.account_id.as_ref().is_none_or(|id| accounts.contains(id)));
    }
    s.chains.retain(|c| {
        !c.executors.is_empty()
            && match &c.scope {
                ChainScope::Project { pool } => pools.contains(pool),
                _ => true,
            }
    });
}

impl Store {
    /// Sets the chain of `scope`; an empty list clears it. Returns the chain as stored (None when
    /// cleared). The runtime checks the agents against the catalog before it calls this.
    pub fn set_chain(
        &self,
        scope: ChainScope,
        executors: Vec<Executor>,
    ) -> Result<Option<Chain>, String> {
        if executors.len() > MAX_EXECUTORS {
            return Err("A chain holds at most 8 agents.".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        match &scope {
            ChainScope::Machine => {}
            ChainScope::Project { pool } => {
                if !candidate.projects.iter().any(|p| &p.pool == pool) {
                    return Err("Project not found.".into());
                }
            }
            ChainScope::Workflow { id } => {
                if !workflow_id_valid(id) {
                    return Err("Not a workflow id: it looks like wf_ and 16 hex digits.".into());
                }
            }
        }
        for e in &executors {
            validate_executor(&candidate, e)?;
        }
        let mut seen = std::collections::HashSet::new();
        for e in &executors {
            if !seen.insert((&e.agent, &e.account_id, &e.key)) {
                return Err("The same agent and account appear twice in the chain.".into());
            }
        }
        candidate.chains.retain(|c| c.scope != scope);
        let chain = (!executors.is_empty()).then(|| Chain {
            scope: scope.clone(),
            executors,
            updated_at: now(),
        });
        if let Some(chain) = &chain {
            candidate.chains.push(chain.clone());
        }
        append_event(
            &mut candidate,
            "fallback_chain",
            None,
            if chain.is_some() { "set" } else { "cleared" },
        );
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(chain)
    }

    /// The chain that applies to a workflow in a project: the workflow's own, else the project's,
    /// else the machine's. None when none is set — no chain is applied until the operator sets one.
    pub fn chain_for(
        &self,
        workflow: Option<&str>,
        pool: Option<&str>,
    ) -> Result<Option<Chain>, String> {
        let snapshot = self.snapshot()?;
        let find = |scope: &ChainScope| snapshot.chains.iter().find(|c| &c.scope == scope).cloned();
        Ok(workflow
            .and_then(|id| find(&ChainScope::Workflow { id: id.to_owned() }))
            .or_else(|| pool.and_then(|p| find(&ChainScope::Project { pool: p.to_owned() })))
            .or_else(|| find(&ChainScope::Machine)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Credential, MemoryVault};
    use std::sync::Arc;

    fn store(dir: &std::path::Path) -> Store {
        Store::open(dir.to_owned(), Arc::new(MemoryVault::default())).unwrap()
    }
    fn add(store: &Store, provider: Provider, kind: AuthKind, label: &str) -> String {
        let secret = match (provider, kind) {
            (_, AuthKind::ApiKey) => format!("synthetic-{label}-key"),
            (Provider::Claude, _) => serde_json::json!({"claudeAiOauth":{"accessToken":format!("{label}-token"),"refreshToken":format!("{label}-refresh"),"expiresAt":4_000_000_000_000i64}}).to_string(),
            (Provider::Codex, _) => serde_json::json!({"tokens":{"access_token":format!("{label}-token"),"refresh_token":format!("{label}-refresh"),"account_id":label}}).to_string(),
        };
        store
            .add(
                label.into(),
                provider,
                kind,
                "default".into(),
                Credential::parse(provider, kind, &secret).unwrap(),
            )
            .unwrap()
            .id
    }
    fn agent(agent: &str) -> Executor {
        Executor {
            agent: agent.into(),
            account_id: None,
            key: None,
        }
    }
    const WF: &str = "wf_0123456789abcdef";

    #[test]
    fn the_narrowest_chain_applies_and_none_applies_until_one_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        assert_eq!(store.chain_for(Some(WF), None).unwrap(), None);
        store
            .set_chain(
                ChainScope::Machine,
                vec![
                    agent("claude-code"),
                    agent("codex"),
                    agent("kimi-code"),
                    agent("hermes"),
                ],
            )
            .unwrap();
        store
            .set_chain(ChainScope::Workflow { id: WF.into() }, vec![agent("codex")])
            .unwrap();
        let task = store.chain_for(Some(WF), None).unwrap().unwrap();
        assert_eq!(task.executors, vec![agent("codex")]);
        let other = store
            .chain_for(Some("wf_ffffffffffffffff"), Some("default"))
            .unwrap()
            .unwrap();
        assert_eq!(other.scope, ChainScope::Machine);
        assert_eq!(other.executors.len(), 4);
        // An empty list clears the scope; the machine's chain applies again.
        assert_eq!(
            store
                .set_chain(ChainScope::Workflow { id: WF.into() }, vec![])
                .unwrap(),
            None
        );
        assert_eq!(
            store.chain_for(Some(WF), None).unwrap().unwrap().scope,
            ChainScope::Machine
        );
    }

    #[test]
    fn a_subscription_is_never_handed_to_another_agent() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let claude_oauth = add(&store, Provider::Claude, AuthKind::OAuth, "sub");
        let claude_key = add(&store, Provider::Claude, AuthKind::ApiKey, "api");
        let codex = add(&store, Provider::Codex, AuthKind::OAuth, "gpt");
        let pinned = |agent: &str, id: &str| Executor {
            agent: agent.into(),
            account_id: Some(id.into()),
            key: None,
        };
        let set = |e: Executor| store.set_chain(ChainScope::Machine, vec![e]);
        assert!(set(pinned("claude-code", &claude_oauth)).is_ok());
        assert!(set(pinned("codex", &codex)).is_ok());
        assert!(set(pinned("hermes", &claude_key)).is_ok());
        assert!(set(pinned("hermes", &claude_oauth))
            .unwrap_err()
            .contains("never on a subscription sign-in"));
        assert!(set(pinned("codex", &claude_oauth))
            .unwrap_err()
            .contains("Codex runs on a Codex account."));
        assert!(set(pinned("claude-code", "no-such-account"))
            .unwrap_err()
            .contains("Account not found"));
    }

    #[test]
    fn keys_are_vault_names_and_bad_input_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let keyed = |agent: &str, key: &str| Executor {
            agent: agent.into(),
            account_id: None,
            key: Some(key.into()),
        };
        assert!(store
            .set_chain(
                ChainScope::Machine,
                vec![keyed(
                    "kimi-code",
                    "fabric-switchboard/prod/OPENROUTER_API_KEY"
                )]
            )
            .is_ok());
        for bad in ["sk-or-v1-abc", "a/b", "a//c", "a/b/c/d"] {
            assert!(
                store
                    .set_chain(ChainScope::Machine, vec![keyed("kimi-code", bad)])
                    .is_err(),
                "{bad}"
            );
        }
        assert!(store
            .set_chain(ChainScope::Machine, vec![keyed("codex", "p/e/N")])
            .is_err());
        assert!(store
            .set_chain(ChainScope::Machine, vec![agent("Not An Id")])
            .is_err());
        assert!(store
            .set_chain(
                ChainScope::Workflow {
                    id: "wf_123".into()
                },
                vec![agent("codex")]
            )
            .is_err());
        assert!(store
            .set_chain(
                ChainScope::Project {
                    pool: "nowhere".into()
                },
                vec![agent("codex")]
            )
            .unwrap_err()
            .contains("Project not found"));
        assert!(store
            .set_chain(ChainScope::Machine, vec![agent("codex"), agent("codex")])
            .is_err());
        assert!(store
            .set_chain(ChainScope::Machine, vec![agent("codex"); MAX_EXECUTORS + 1])
            .is_err());
    }

    #[test]
    fn removing_an_account_or_a_project_prunes_its_chains() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let key = add(&store, Provider::Claude, AuthKind::ApiKey, "api");
        let work = tempfile::tempdir().unwrap();
        let project = store
            .save_project(None, "Work", &[work.path().canonicalize().unwrap()], &[])
            .unwrap();
        store
            .set_chain(
                ChainScope::Machine,
                vec![
                    agent("codex"),
                    Executor {
                        agent: "hermes".into(),
                        account_id: Some(key.clone()),
                        key: None,
                    },
                ],
            )
            .unwrap();
        store
            .set_chain(
                ChainScope::Project {
                    pool: project.pool.clone(),
                },
                vec![agent("codex")],
            )
            .unwrap();
        store.remove(&key).unwrap();
        let machine = store.chain_for(None, None).unwrap().unwrap();
        assert_eq!(
            machine.executors,
            vec![agent("codex")],
            "the pinned executor left with its account"
        );
        store.remove_project(&project.pool).unwrap();
        assert_eq!(
            store.snapshot().unwrap().chains.len(),
            1,
            "the project's chain left with it"
        );
    }

    #[test]
    fn chains_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = store(dir.path());
            store
                .set_chain(
                    ChainScope::Machine,
                    vec![agent("claude-code"), agent("codex")],
                )
                .unwrap();
        }
        let reopened = store(dir.path());
        assert_eq!(
            reopened
                .chain_for(None, None)
                .unwrap()
                .unwrap()
                .executors
                .len(),
            2
        );
    }
}
