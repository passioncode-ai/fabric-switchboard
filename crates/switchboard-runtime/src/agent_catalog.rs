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

/// Whether an agent can take over a workflow (SB-71): it loads an MCP server, to accept the
/// handoff over Observatory's, and takes its first prompt without a person (`headless`).
pub fn can_continue(id: &str) -> bool {
    find(id).is_some_and(|a| a["mcp"]["supported"] == true && a["headless"].is_string())
}

/// Ready-made chains (operator, 2026-10-06: subscriptions first, then paid agents). A preset is
/// only a starting order; nothing applies until the operator sets a chain.
pub const PRESETS: &[(&str, &[&str])] = &[(
    "subscriptions-first",
    &["claude-code", "codex", "kimi-code", "hermes"],
)];

/// The presets with each agent's catalog name, for the CLI, MCP and the app.
pub fn presets() -> Value {
    json!(PRESETS
        .iter()
        .map(|(id, agents)| json!({
            "id": id,
            "agents": agents
                .iter()
                .map(|a| json!({"agent": a, "name": find(a).map(|x| x["name"].clone()).unwrap_or(Value::Null)}))
                .collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>())
}

/// How one agent launches on the operator's OpenRouter key (SB-79, XA-02 A-3): the key and the
/// model reach it only through its environment and its own arguments — never a config write.
pub struct OpenrouterRecipe {
    pub id: String,
    pub name: String,
    pub binary: String,
    /// The environment variable that takes the key's value.
    pub key_env: String,
    /// The variable that takes `base_url`, when the agent needs one (Kimi Code, Qwen Code).
    pub base_env: Option<String>,
    pub base_url: Option<String>,
    /// Extra argv entries; `{model}` inside an entry is replaced by the chosen model.
    pub model_flag: Vec<String>,
    /// The environment variable that takes the chosen model, for agents configured by env.
    pub model_env: Option<String>,
    /// Fixed extra argv entries (Qwen Code's `--auth-type openai`).
    pub args: Vec<String>,
    pub extra_env: BTreeMap<String, String>,
    /// Isolation variable pointed at the launch's own home, so a saved login cannot win over
    /// the launch's key (`HERMES_HOME`, `PI_CODING_AGENT_DIR`, `QWEN_HOME`).
    pub isolate_env: Option<String>,
    /// What the operator should know: a saved login that wins over the key, a model chosen in
    /// the agent itself.
    pub notes: Option<String>,
}

/// An agent's OpenRouter launch recipe. Agents without one (Cline's TUI insists on its own
/// onboarding; OpenClaw's sessions run in its Gateway, which never sees a launch's environment)
/// are refused.
pub fn openrouter(id: &str) -> Result<OpenrouterRecipe, String> {
    let agent = find(id).ok_or("Unknown agent. `switchboard agents list` names them.")?;
    let recipe = &agent["openrouter"];
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let list = |v: &Value| {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    match (text(&recipe["key_env"]), text(&agent["binary"])) {
        (Some(key_env), Some(binary)) => Ok(OpenrouterRecipe {
            id: id.to_owned(),
            name: text(&agent["name"]).unwrap_or_else(|| id.to_owned()),
            binary,
            key_env,
            base_env: text(&recipe["base_env"]),
            base_url: text(&recipe["base_url"]),
            model_flag: list(&recipe["model_flag"]),
            model_env: text(&recipe["model_env"]),
            args: list(&recipe["args"]),
            extra_env: recipe["extra_env"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                        .collect()
                })
                .unwrap_or_default(),
            isolate_env: text(&recipe["isolate_env"]),
            notes: text(&recipe["notes"]),
        }),
        _ => Err("This agent cannot launch on the OpenRouter key. `switchboard agents list` marks the ones that can.".into()),
    }
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
    fn every_openrouter_recipe_is_complete_and_never_puts_the_key_on_a_command_line() {
        let env_name = |n: &str| {
            !n.is_empty()
                && n.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        };
        let mut launchable = Vec::new();
        for a in catalog()["agents"].as_array().unwrap() {
            let id = a["id"].as_str().unwrap();
            if a["openrouter"].is_null() {
                assert!(openrouter(id).is_err(), "{id}");
                continue;
            }
            let r = openrouter(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            launchable.push(id.to_owned());
            // Whether the app offers the key first (Kimi Code stays on its subscription).
            assert!(a["openrouter"]["default"].is_boolean(), "{id}");
            assert!(env_name(&r.key_env), "{id}");
            for name in [&r.base_env, &r.model_env, &r.isolate_env]
                .into_iter()
                .flatten()
                .chain(r.extra_env.keys())
            {
                assert!(env_name(name), "{id}: {name}");
            }
            assert_eq!(r.base_env.is_some(), r.base_url.is_some(), "{id}");
            if let Some(url) = &r.base_url {
                assert!(url.starts_with("https://openrouter.ai/"), "{id}");
            }
            assert!(
                r.model_flag.is_empty() || r.model_flag.iter().any(|a| a.contains("{model}")),
                "{id}: a model flag without the model"
            );
            assert!(
                r.model_env.is_none() || r.model_flag.is_empty(),
                "{id}: one way to name the model"
            );
            for arg in r.args.iter().chain(&r.model_flag) {
                assert!(
                    !matches!(
                        arg.as_str(),
                        "--api-key" | "-k" | "--key" | "--openai-api-key"
                    ),
                    "{id}: a key flag lands in process listings"
                );
            }
        }
        for id in [
            "hermes",
            "kimi-code",
            "pi",
            "opencode",
            "goose",
            "aider",
            "qwen-code",
        ] {
            assert!(launchable.iter().any(|l| l == id), "{id} has no recipe");
        }
        // Their sessions never read a launch's environment (the Gateway; the onboarding TUI).
        assert!(openrouter("openclaw").is_err());
        assert!(openrouter("cline").is_err());
        assert!(openrouter("no-such-agent")
            .err()
            .unwrap()
            .contains("Unknown agent"));
        let hermes = openrouter("hermes").unwrap();
        assert_eq!(
            hermes.model_flag,
            ["--provider", "openrouter", "-m", "{model}"]
        );
        assert_eq!(hermes.isolate_env.as_deref(), Some("HERMES_HOME"));
        let kimi = openrouter("kimi-code").unwrap();
        assert_eq!(kimi.key_env, "KIMI_MODEL_API_KEY");
        assert_eq!(kimi.model_env.as_deref(), Some("KIMI_MODEL_NAME"));
        assert_eq!(
            kimi.isolate_env, None,
            "its subscription login stays where it is"
        );
        let agents = catalog()["agents"].clone();
        let kimi_entry = agents
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "kimi-code");
        assert_eq!(kimi_entry.unwrap()["openrouter"]["default"], false);
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
