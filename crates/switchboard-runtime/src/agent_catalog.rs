//! The agents Switchboard works with beyond Claude Code and Codex (operator request 2026-10-05):
//! `catalog/agents.json`, built from sourced research (docs/research/agents-2026-10-05.md) and
//! embedded at build time. Three levels: `mcp` (the agent registers `switchboard mcp`), `proxy`
//! (it can also send its model calls through Switchboard's local proxy, set up in its own config
//! file), `launch` (configured by environment alone, so `switchboard agents launch` starts it).
//! Third-party agents always use the agents' capability, which the proxy serves only from
//! API-key accounts: a subscription sign-in belongs to the provider's own client.
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CATALOG: &str = include_str!("../../../catalog/agents.json");

/// The whole catalog, as embedded.
pub fn catalog() -> Value {
    serde_json::from_str(CATALOG).unwrap_or_else(|_| json!({"agents": []}))
}

fn find(id: &str) -> Option<Value> {
    catalog()["agents"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

/// What a launch needs from a `launch`-level profile.
pub struct Launchable {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub base_env: Option<String>,
    pub key_env: Option<String>,
    pub suffix: String,
    pub extra_env: BTreeMap<String, String>,
    pub args: Vec<String>,
}

pub fn launchable(id: &str) -> Result<Launchable, String> {
    let agent = find(id).ok_or("Unknown agent. `switchboard agents list` names them.")?;
    let anthropic = &agent["anthropic"];
    let text = |v: &Value| v.as_str().map(str::to_owned);
    // `launch`: configured by environment here. `proxy` with a binary: configured once in its
    // own file by `switchboard agents connect`, then started like the others.
    let by_env = agent["level"] == "launch";
    match (
        agent["level"].as_str(),
        text(&agent["binary"]),
        text(&anthropic["base_env"]),
        text(&anthropic["key_env"]),
    ) {
        (Some("launch" | "proxy"), Some(binary), base_env, key_env)
            if !by_env || (base_env.is_some() && key_env.is_some()) =>
        {
            Ok(Launchable {
                id: id.to_owned(),
                name: text(&agent["name"]).unwrap_or_else(|| id.to_owned()),
                binary,
                base_env: if by_env { base_env } else { None },
                key_env: if by_env { key_env } else { None },
                suffix: text(&anthropic["suffix"]).unwrap_or_default(),
                extra_env: anthropic["extra_env"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                            .collect()
                    })
                    .unwrap_or_default(),
                args: if by_env {
                    text(&anthropic["requires_flag"]).into_iter().collect()
                } else {
                    vec![]
                },
            })
        }
        _ => Err(
            "This agent is set up in its own config file; `switchboard agents connect` shows how."
                .into(),
        ),
    }
}

/// How to connect one agent: its MCP registration, and — when it can use the proxy — the base
/// URLs and the key, given by command (`switchboard agents key`) rather than as a value.
pub fn connect(id: &str, proxy: Option<&str>, pool: &str, cli: &str) -> Result<Value, String> {
    let agent = find(id).ok_or("Unknown agent. `switchboard agents list` names them.")?;
    let replace = |text: &Value| {
        text.as_str().map(|t| {
            t.replace("{cli}", cli)
                .replace("command: \"switchboard\"", &format!("command: \"{cli}\""))
        })
    };
    let address = proxy.unwrap_or("127.0.0.1:<port>");
    let anthropic_base = format!(
        "http://{address}/claude/{pool}{}",
        agent["anthropic"]["suffix"].as_str().unwrap_or("")
    );
    let openai_base = format!("http://{address}/codex/{pool}/v1");
    let key_cmd = format!("{cli} agents key");
    let proxy_ok = agent["proxy_ok"].as_bool() == Some(true);
    let anthropic = (proxy_ok && agent["anthropic"]["supported"].as_bool() == Some(true)).then(|| {
        let a = &agent["anthropic"];
        let snippet = a["config_snippet"].as_str().map(|s| {
            s.replace("{base}", &anthropic_base)
                .replace("{key_cmd}", &key_cmd)
                .replace("{key}", &format!("$({key_cmd})"))
        });
        let mut env = serde_json::Map::new();
        if let (Some(base), Some(key)) = (a["base_env"].as_str(), a["key_env"].as_str()) {
            env.insert(base.into(), json!(anthropic_base));
            env.insert(key.into(), json!(format!("$({key_cmd})")));
            for (k, v) in a["extra_env"].as_object().into_iter().flatten() {
                env.insert(k.clone(), v.clone());
            }
        }
        json!({"base_url": anthropic_base, "env": env, "config_snippet": snippet, "requires_flag": a["requires_flag"]})
    });
    let openai = (proxy_ok
        && (agent["openai"]["responses"].as_bool() == Some(true)
            || agent["openai"]["chat_completions"].as_bool() == Some(true)))
    .then(|| json!({"base_url": openai_base, "responses": agent["openai"]["responses"], "chat_completions": agent["openai"]["chat_completions"]}));
    Ok(json!({
        "agent": agent["id"], "name": agent["name"], "level": agent["level"],
        "mcp": {
            "supported": agent["mcp"]["supported"],
            "add_command": replace(&agent["mcp"]["add_command"]),
            "config_path": agent["mcp"]["config_path"],
            "config_snippet": replace(&agent["mcp"]["config_snippet"]),
        },
        "anthropic": anthropic,
        "openai": openai,
        "key_command": (anthropic.is_some() || openai.is_some()).then_some(key_cmd),
        "requires": (anthropic.is_some() || openai.is_some()).then_some(format!("An API-key account selected in the pool {pool}: the proxy refuses subscription sign-ins for third-party agents.")),
        "launch": (matches!(agent["level"].as_str(), Some("launch" | "proxy")) && agent["binary"].is_string()).then(|| format!("{cli} agents launch {id} --pool {pool} --dir <project folder>")),
        "warning": agent["subscription_warning"],
        "notes": agent["notes"],
        "sources": agent["sources"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_well_formed_and_every_launch_profile_is_complete() {
        let catalog = catalog();
        let agents = catalog["agents"].as_array().unwrap();
        assert!(agents.len() >= 30, "the top agents are all listed");
        let mut ids = std::collections::BTreeSet::new();
        for a in agents {
            let id = a["id"].as_str().unwrap();
            assert!(ids.insert(id), "{id} twice");
            assert!(
                matches!(a["level"].as_str(), Some("mcp" | "proxy" | "launch")),
                "{id}"
            );
            assert!(
                !a["sources"].as_array().unwrap().is_empty(),
                "{id} has no source"
            );
            assert!(a["mcp"]["supported"].is_boolean(), "{id}");
            if a["level"] == "launch" {
                let l = launchable(id).unwrap_or_else(|e| panic!("{id}: {e}"));
                assert!(l.base_env.is_some() && l.key_env.is_some(), "{id}");
            }
            if a["level"] == "mcp" {
                assert_ne!(
                    a["proxy_ok"], true,
                    "{id} is mcp-only but says the proxy works"
                );
            }
        }
        assert!(ids.contains("hermes") && ids.contains("claude-code") && ids.contains("codex"));
    }

    #[test]
    fn connect_names_the_key_command_and_never_a_key() {
        for a in catalog()["agents"].as_array().unwrap() {
            let id = a["id"].as_str().unwrap();
            let out = connect(
                id,
                Some("127.0.0.1:4000"),
                "agents",
                "/usr/local/bin/switchboard",
            )
            .unwrap()
            .to_string();
            // No 64-hex capability anywhere: only the command that prints it.
            let hex_run = out
                .split(|c: char| !c.is_ascii_hexdigit())
                .any(|run| run.len() >= 64);
            assert!(!hex_run, "{id} leaked a key-shaped value");
            if a["proxy_ok"] == true && a["anthropic"]["supported"] == true {
                assert!(out.contains("/claude/agents"), "{id}");
                assert!(out.contains("agents key"), "{id}");
            }
        }
        let hermes = connect("hermes", None, "agents", "switchboard").unwrap();
        assert!(hermes["warning"]
            .as_str()
            .unwrap()
            .contains("adopt_external_logins"));
        assert!(connect("no-such-agent", None, "agents", "switchboard").is_err());
    }
}
