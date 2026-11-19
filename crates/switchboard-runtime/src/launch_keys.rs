//! SB-392 K-1 (operator 2026-11-16: «чтобы мы могли выбрать на каких ключах запускаться, какие
//! ключи у нас есть и на чем мы будем запускаться»): every key an agent can start on here, by name,
//! never a value. OpenRouter stays the default: the key of the folder's group when it has one, else
//! this computer's (`switchboard_core::agent_keys::key_in`, the rule a launch spends by). The other
//! OpenRouter keys are listed with the group whose folders use them. Anthropic keys follow for the
//! agents that take one (Hermes, SB-390).
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::Store;

pub const FOLDER_OUTSIDE: &str = "This folder is not in the pay-per-token group. Add it to the group first, then launch on its key.";
pub const NOT_SELECTED: &str =
    "The group's selected key serves its launches. Select this key in the group to launch on it.";

/// SB-404 K3: whether `agent` can run on the pay-per-token group's key through the proxy: one
/// configured by its environment (Claude Code), or Hermes, which the launch configures itself.
pub fn takes_group_key(agent: &str) -> bool {
    agent == "hermes" || crate::agent_catalog::launchable(agent).is_ok_and(|p| p.base_env.is_some())
}

pub fn list(
    root: &Path,
    store: &Store,
    agent: &str,
    folder: Option<&Path>,
) -> Result<Value, String> {
    let snapshot = store.snapshot()?;
    let openrouter = crate::agent_catalog::openrouter(agent).is_ok();
    let chosen = switchboard_core::agent_keys::key_in(&snapshot, "openrouter", folder);
    let group_name = |pool: &str| {
        snapshot
            .projects
            .iter()
            .find(|p| p.pool == pool)
            .map(|p| p.name.clone())
    };
    let mut keys = Vec::new();
    if openrouter {
        let mut saved: Vec<_> = snapshot
            .agent_keys
            .iter()
            .filter(|k| k.service == "openrouter")
            .collect();
        // The key this folder spends first, then this computer's, then each group's by name.
        saved.sort_by_key(|k| {
            (
                Some(*k) != chosen,
                k.pool.is_some(),
                k.pool.as_deref().and_then(group_name),
            )
        });
        for k in saved {
            let here = Some(k) == chosen;
            let name = match &k.pool {
                None => "This computer".to_string(),
                Some(pool) => group_name(pool).unwrap_or_else(|| pool.clone()),
            };
            let why_not = (!here).then(|| match &k.pool {
                Some(_) => format!("Used in the folders of {name}."),
                None => "This folder's group has an OpenRouter key of its own.".to_string(),
            });
            keys.push(json!({
                "kind": "openrouter",
                "scope": if k.pool.is_some() { "group" } else { "machine" },
                "pool": k.pool,
                "name": name,
                "model": k.model,
                "usable": here,
                "default": here,
                "why_not": why_not,
                "metered": true,
            }));
        }
    }
    if crate::launch::takes_anthropic(agent) {
        for k in crate::anthropic_keys::read(root)?.keys {
            keys.push(json!({
                "kind": "anthropic",
                "name": k.name,
                "model": k.model,
                "usable": true,
                "default": false,
                "why_not": null,
                // SB-390: no ceiling counts a launch on a named Anthropic key.
                "metered": false,
            }));
        }
    }
    // SB-404 K3: the pay-per-token group's Claude API keys, for an agent that runs through the
    // proxy on them (Claude Code by its environment, Hermes in a home of its own): metered at
    // list price against the group's daily ceiling (DEC-0023). The proxy serves the group's
    // selected key, so that one is the launch's; the others say how to choose them.
    if takes_group_key(agent) {
        if let Some(group) = snapshot.tokens_group() {
            let selected = snapshot.routes.get(&format!("claude:{}", group.pool));
            let ceiling = crate::spend::group_ceiling(root, &group.pool);
            let in_group =
                folder.is_none_or(|f| group.folders.iter().any(|g| f.starts_with(Path::new(g))));
            for account in snapshot
                .accounts
                .iter()
                .filter(|a| a.pool == group.pool && switchboard_core::Snapshot::is_claude_key(a))
            {
                let chosen = selected == Some(&account.id);
                let why_not = if !account.usable() {
                    Some("This key is disabled. Enable it to launch on it.")
                } else if group.off {
                    Some(switchboard_core::GROUP_OFF)
                } else if ceiling.is_none() {
                    Some(switchboard_core::TOKENS_NO_CEILING)
                } else if !in_group {
                    Some(FOLDER_OUTSIDE)
                } else if !chosen {
                    Some(NOT_SELECTED)
                } else {
                    None
                };
                keys.push(json!({
                    "kind": "group",
                    "pool": group.pool,
                    "group": group.name,
                    "account_id": account.id,
                    "name": account.label,
                    "model": null,
                    "selected": chosen,
                    "usable": why_not.is_none(),
                    "default": false,
                    "why_not": why_not,
                    "metered": true,
                }));
            }
        }
    }
    let usable = keys.iter().any(|k| k["usable"] == true);
    Ok(json!({
        "agent": agent,
        "folder": folder.map(|f| f.to_string_lossy().into_owned()),
        "openrouter": openrouter,
        "anthropic": crate::launch::takes_anthropic(agent),
        "group_keys": takes_group_key(agent),
        "keys": keys,
        "none_usable": !usable,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn the_folders_key_comes_first_and_is_the_only_openrouter_choice() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = Store::open(
            root.into(),
            Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap();
        // No key at all: nothing to run on, and the answer says so.
        let empty = list(root, &store, "hermes", None).unwrap();
        assert_eq!(empty["keys"], json!([]));
        assert_eq!(empty["none_usable"], true);
        store
            .set_agent_key(
                "openrouter",
                "sk-or-v1-machine",
                Some("moonshotai/kimi-k2".into()),
            )
            .unwrap();
        crate::anthropic_keys::set(root, &store, "team-a", "sk-ant-api03-x", None, 1).unwrap();
        let answer = list(root, &store, "hermes", None).unwrap();
        let keys = answer["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0]["kind"], "openrouter");
        assert_eq!(keys[0]["name"], "This computer");
        assert_eq!(keys[0]["default"], true);
        assert_eq!(keys[1]["kind"], "anthropic");
        assert_eq!(keys[1]["name"], "team-a");
        // An agent with no Anthropic route never lists an Anthropic key.
        let other = list(root, &store, "opencode", None).unwrap();
        assert!(other["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["kind"] == "openrouter"));
        // No value anywhere in the answer.
        assert!(!answer.to_string().contains("sk-"));
        assert_eq!(keys[1]["metered"], false);
    }

    /// SB-404 K3: the pay-per-token group's keys are offered to Claude Code and Hermes, metered;
    /// each says why it cannot be launched on yet.
    #[test]
    fn the_pay_per_token_groups_keys_are_offered_with_why_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = Store::open(
            root.into(),
            Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap();
        let folder = root.join("work");
        std::fs::create_dir(&folder).unwrap();
        let credential = switchboard_core::Credential::parse(
            switchboard_core::Provider::Claude,
            switchboard_core::AuthKind::ApiKey,
            "synthetic-key",
        )
        .unwrap();
        let key = store
            .add_routed(
                "Team key".into(),
                switchboard_core::Provider::Claude,
                switchboard_core::AuthKind::ApiKey,
                "default".into(),
                credential,
            )
            .unwrap();
        let why = |agent: &str, folder: Option<&Path>| {
            let answer = list(root, &store, agent, folder).unwrap();
            answer["keys"]
                .as_array()
                .unwrap()
                .iter()
                .find(|k| k["kind"] == "group")
                .cloned()
        };
        let group = why("hermes", None).expect("Hermes is offered the group's key");
        assert_eq!(group["name"], "Team key");
        assert_eq!(group["metered"], true);
        assert_eq!(group["why_not"], switchboard_core::GROUP_OFF);
        assert!(why("claude-code", None).is_some());
        assert!(
            why("opencode", None).is_none(),
            "configured in its own file"
        );
        store.set_group_off("tokens", false).unwrap();
        assert_eq!(
            why("hermes", None).unwrap()["why_not"],
            switchboard_core::TOKENS_NO_CEILING
        );
        crate::spend::set_ceiling(
            root,
            &store.snapshot().unwrap(),
            switchboard_core::ChainScope::Project {
                pool: "tokens".into(),
            },
            Some(5.0),
            crate::spend::Surface::Window,
            1,
        )
        .unwrap();
        assert_eq!(
            why("hermes", Some(&folder)).unwrap()["why_not"],
            FOLDER_OUTSIDE
        );
        store
            .save_project(
                Some("tokens"),
                "Pay-per-token keys",
                &[folder.canonicalize().unwrap()],
                std::slice::from_ref(&key.id),
            )
            .unwrap();
        assert_eq!(
            why("hermes", Some(&folder.canonicalize().unwrap())).unwrap()["why_not"],
            NOT_SELECTED
        );
        store
            .select(switchboard_core::Provider::Claude, "tokens", &key.id)
            .unwrap();
        let ready = why("hermes", Some(&folder.canonicalize().unwrap())).unwrap();
        assert_eq!(ready["usable"], true);
        assert_eq!(ready["selected"], true);
    }
}
