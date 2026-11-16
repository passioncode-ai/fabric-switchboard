//! SB-392 K-1 (operator 2026-11-16: «чтобы мы могли выбрать на каких ключах запускаться, какие
//! ключи у нас есть и на чем мы будем запускаться»): every key an agent can start on here, by name,
//! never a value. OpenRouter stays the default: the key of the folder's group when it has one, else
//! this computer's (`switchboard_core::agent_keys::key_in`, the rule a launch spends by). The other
//! OpenRouter keys are listed with the group whose folders use them. Anthropic keys follow for the
//! agents that take one (Hermes, SB-390).
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::Store;

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
            }));
        }
    }
    let usable = keys.iter().any(|k| k["usable"] == true);
    Ok(json!({
        "agent": agent,
        "folder": folder.map(|f| f.to_string_lossy().into_owned()),
        "openrouter": openrouter,
        "anthropic": crate::launch::takes_anthropic(agent),
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
        crate::anthropic_keys::set(root, &store, "appvillis", "sk-ant-api03-x", None, 1).unwrap();
        let answer = list(root, &store, "hermes", None).unwrap();
        let keys = answer["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0]["kind"], "openrouter");
        assert_eq!(keys[0]["name"], "This computer");
        assert_eq!(keys[0]["default"], true);
        assert_eq!(keys[1]["kind"], "anthropic");
        assert_eq!(keys[1]["name"], "appvillis");
        // An agent with no Anthropic route never lists an Anthropic key.
        let other = list(root, &store, "opencode", None).unwrap();
        assert!(other["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["kind"] == "openrouter"));
        // No value anywhere in the answer.
        assert!(!answer.to_string().contains("sk-"));
    }
}
