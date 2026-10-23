//! SB-119/SB-120 (operator 2026-10-14): which coding agents this computer has, which of them
//! Switchboard may hand work to, the machine's default agent, and the OpenRouter models agents
//! launch on — the recommended ones first. The choices live in `<data>/agents-settings.json`
//! (0600, carried in backups as a setting); the installed set is found, never stored.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub const FILE: &str = "agents-settings.json";
/// Equal to the backup's per-setting limit, so every file this writes is carried in backups.
const MAX_BYTES: u64 = 4096;
pub const NOT_AN_AGENT: &str = "No such agent in the catalog. Run switchboard agents list.";
pub const CANNOT_CONTINUE: &str =
    "Switchboard cannot hand work to this agent yet, so it cannot be the default.";
pub const NOT_INSTALLED: &str =
    "This agent is not installed on this computer. Install it, then press Refresh.";
pub const DISABLED: &str =
    "This agent is disabled for taking over work. Enable it on the Agents page, then try again.";
pub const NAME_AN_ACCOUNT: &str = "Name the account or agent to continue on (--account), or choose a default agent on the Agents page.";
pub const UNREADABLE: &str = "Agent settings (agents-settings.json) could not be read. Restore it from a backup, or delete it to start over with every agent enabled and no default.";
/// OpenRouter's public model list (no key).
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const MODELS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MODELS_KEEP: i64 = 6 * 3600;
const MAX_MODELS_BYTES: usize = 8 * 1024 * 1024;

/// Unknown fields are ignored, so a file a newer Switchboard wrote (or a newer backup restored)
/// still reads here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The machine's default agent; none is *Auto*.
    #[serde(default)]
    pub default: Option<String>,
    /// Agents the person keeps work away from.
    #[serde(default)]
    pub disabled: Vec<String>,
}

pub fn read(root: &Path) -> Result<Settings, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    serde_json::from_slice(&bytes).map_err(|_| UNREADABLE.into())
}

fn write(root: &Path, settings: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec(settings).map_err(|_| UNREADABLE)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

fn known(id: &str) -> Option<Value> {
    crate::agent_catalog::catalog()["agents"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

/// Whether an agent's program is on this computer (the lookup launches use).
pub fn installed(agent: &Value) -> Option<std::path::PathBuf> {
    let path = agent["binary"]
        .as_str()
        .filter(|b| !b.is_empty())
        .and_then(crate::launch::find_program)?;
    // Windows's own programs are never an agent (`cmd` is Command Code's name and cmd.exe's).
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot") {
        if under_folder(&path, std::path::Path::new(&system)) {
            return None;
        }
    }
    Some(path)
}

/// Whether `path` lies in `folder`, compared as Windows does: without the `\\?\` prefix a
/// canonical path carries, case-insensitively, with either separator.
#[cfg_attr(not(windows), allow(dead_code))]
fn under_folder(path: &Path, folder: &Path) -> bool {
    let plain = |p: &Path| {
        let text = p.to_string_lossy().replace('/', "\\").to_lowercase();
        let text = text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned();
        text.trim_end_matches('\\').to_owned()
    };
    let (path, folder) = (plain(path), plain(folder));
    !folder.is_empty() && (path == folder || path.starts_with(&format!("{folder}\\")))
}

/// An agent can take over work when it is installed, Switchboard can hand it work, and it
/// was not disabled.
pub fn usable(root: &Path, id: &str) -> Result<(), String> {
    let agent = known(id).ok_or(NOT_AN_AGENT)?;
    if !crate::agent_catalog::switchable(id) {
        return Err(CANNOT_CONTINUE.into());
    }
    if read(root)?.disabled.iter().any(|d| d == id) {
        return Err(DISABLED.into());
    }
    if installed(&agent).is_none() {
        return Err(NOT_INSTALLED.into());
    }
    Ok(())
}

/// Whether a person disabled this agent (fallback chains skip it; `continue` refuses it). An
/// unreadable file is an error, never "nothing disabled".
pub fn disabled(root: &Path, id: &str) -> Result<bool, String> {
    Ok(read(root)?.disabled.iter().any(|d| d == id))
}

pub fn set_enabled(root: &Path, id: &str, enabled: bool) -> Result<Settings, String> {
    known(id).ok_or(NOT_AN_AGENT)?;
    let mut s = read(root)?;
    s.disabled.retain(|d| d != id);
    if !enabled {
        s.disabled.push(id.to_owned());
        s.disabled.sort();
        // A disabled agent cannot stay the default (SCN-058).
        if s.default.as_deref() == Some(id) {
            s.default = None;
        }
    }
    write(root, &s)?;
    Ok(s)
}

pub fn set_default(root: &Path, id: Option<&str>) -> Result<Settings, String> {
    let mut s = read(root)?;
    if let Some(id) = id {
        usable(root, id)?;
    }
    s.default = id.map(str::to_owned);
    write(root, &s)?;
    Ok(s)
}

/// The command Switchboard runs for an agent, as the Agents page shows it: the program and the
/// arguments of an OpenRouter launch with `model` (no key, no environment).
pub fn command(agent: &Value, model: Option<&str>) -> String {
    argv(agent, model).join(" ")
}

/// `command` as an argument vector: the program, then its arguments (SB-120, for Fabric).
pub fn argv(agent: &Value, model: Option<&str>) -> Vec<String> {
    let binary = agent["binary"].as_str().unwrap_or_default();
    let id = agent["id"].as_str().unwrap_or_default();
    let mut parts = vec![binary.to_owned()];
    if let (Ok(recipe), Some(model)) = (crate::agent_catalog::openrouter(id), model) {
        if !recipe.model_flag.is_empty() {
            parts.extend(recipe.args.iter().cloned());
            parts.extend(
                recipe
                    .model_flag
                    .iter()
                    .map(|a| a.replace("{model}", model)),
            );
        }
    }
    parts
}

/// Every catalog agent with what the Agents page needs: installed or not, enabled for taking
/// over work, default, the command, and its links. Fabric reads it (`agents status --json`):
/// `schema` changes only when a field changes meaning or goes.
pub fn overview(root: &Path, model: Option<&str>) -> Result<Value, String> {
    let settings = read(root)?;
    let agents: Vec<Value> = crate::agent_catalog::catalog()["agents"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|agent| {
            let id = agent["id"].as_str().unwrap_or_default().to_owned();
            let path = installed(&agent);
            let switchable = crate::agent_catalog::switchable(&id);
            let disabled = settings.disabled.iter().any(|d| d == &id);
            json!({
                // SB-154: the contract's catalogue id (DEC-0093), for a host that keys agents by it.
                "contract_id": crate::agent_catalog::contract_id(&id),
                "id": id,
                "name": agent["name"],
                "maker": agent["maker"],
                "url": agent["url"],
                "install": agent["install"],
                "installed": path.is_some(),
                "path": path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "switchable": switchable,
                "enabled": path.is_some() && switchable && !disabled,
                "disabled": disabled,
                "default": settings.default.as_deref() == Some(id.as_str()),
                "openrouter": crate::agent_catalog::openrouter(&id).is_ok(),
                "mcp": agent["mcp"]["supported"],
                "command": command(&agent, model),
                "argv": argv(&agent, model),
            })
        })
        .collect();
    let installed = agents.iter().filter(|a| a["installed"] == true).count();
    // Whether the saved default can take work now: installed, enabled and able to continue.
    let default_ready = settings
        .default
        .as_deref()
        .is_some_and(|id| agents.iter().any(|a| a["id"] == id && a["enabled"] == true));
    // A cache key for readers such as Fabric: it changes whenever anything in the answer does.
    let revision = {
        let digest = <sha2::Sha256 as sha2::Digest>::digest(
            json!([&settings.default, default_ready, &agents])
                .to_string()
                .as_bytes(),
        );
        digest
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let updated_at = std::fs::metadata(root.join(FILE))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Ok(json!({
        "schema": 1,
        "revision": revision,
        "updated_at": updated_at,
        "default": settings.default,
        "default_ready": default_ready,
        "installed": installed,
        "agents": agents,
        "recommended_models": recommended(),
    }))
}

/// The OpenRouter models Switchboard recommends, in order (operator 2026-10-14).
pub fn recommended() -> Value {
    crate::agent_catalog::catalog()["openrouter_recommended"].clone()
}

/// OpenRouter's tool-capable models, newest list within six hours; on any failure the answer
/// says so and the recommended models alone still serve (SCN-059).
#[derive(Default)]
pub struct ModelCache {
    at: i64,
    models: Vec<Value>,
}

/// Only what the picker shows: id, name, context, prices. Models that cannot call tools are
/// left out: a coding agent needs them.
pub fn parse_models(body: &[u8]) -> Option<Vec<Value>> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let mut out: Vec<Value> = value["data"]
        .as_array()?
        .iter()
        .filter(|m| {
            m["supported_parameters"]
                .as_array()
                .is_some_and(|p| p.iter().any(|x| x == "tools"))
        })
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            switchboard_core::agent_keys::model_valid(id).then(|| {
                json!({
                    "id": id,
                    "name": m["name"],
                    "context": m["context_length"],
                    "prompt": m["pricing"]["prompt"],
                    "completion": m["pricing"]["completion"],
                })
            })
        })
        .collect();
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Some(out)
}

pub async fn models(cache: &std::sync::Mutex<ModelCache>, now: i64) -> Value {
    if let Ok(c) = cache.lock() {
        if !c.models.is_empty() && now - c.at < MODELS_KEEP {
            return json!({"recommended": recommended(), "models": c.models, "available": true});
        }
    }
    let fetched = async {
        let client = reqwest::Client::builder()
            .timeout(MODELS_TIMEOUT)
            .build()
            .ok()?;
        let response = client.get(MODELS_URL).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_MODELS_BYTES as u64)
        {
            return None;
        }
        // Read by chunks so an oversized body is dropped before it is held whole.
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if body.len() + chunk.len() > MAX_MODELS_BYTES {
                return None;
            }
            body.extend_from_slice(&chunk);
        }
        parse_models(&body)
    }
    .await;
    match fetched {
        Some(models) => {
            if let Ok(mut c) = cache.lock() {
                c.at = now;
                c.models = models.clone();
            }
            json!({"recommended": recommended(), "models": models, "available": true})
        }
        None => json!({"recommended": recommended(), "models": [], "available": false}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_recommended_models_lead_and_are_valid_ids() {
        let r = recommended();
        let ids: Vec<&str> = r
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["id"].as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "xiaomi/mimo-v2.6-pro",
                "anthropic/claude-haiku-5.5",
                "google/gemini-3.8-flash",
                "qwen/qwen3.8-flash",
                "deepseek/deepseek-v4.1-flash"
            ]
        );
        assert!(ids
            .iter()
            .all(|id| switchboard_core::agent_keys::model_valid(id)));
    }

    #[test]
    fn only_tool_capable_models_are_listed() {
        let body = br#"{"data":[
            {"id":"a/tools","name":"T","context_length":100,"pricing":{"prompt":"0.1","completion":"0.2"},"supported_parameters":["tools","temperature"]},
            {"id":"b/plain","name":"P","context_length":100,"pricing":{},"supported_parameters":["temperature"]},
            {"id":"bad id","supported_parameters":["tools"]}
        ]}"#;
        let models = parse_models(body).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "a/tools");
        assert!(parse_models(b"nope").is_none());
    }

    #[test]
    fn enabling_disabling_and_the_default_persist_and_are_checked() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()).unwrap(), Settings::default());
        assert_eq!(
            set_enabled(root.path(), "nope", false).unwrap_err(),
            NOT_AN_AGENT
        );
        set_enabled(root.path(), "hermes", false).unwrap();
        assert!(disabled(root.path(), "hermes").unwrap());
        assert_eq!(usable(root.path(), "hermes").unwrap_err(), DISABLED);
        assert_eq!(
            set_default(root.path(), Some("aider")).unwrap_err(),
            CANNOT_CONTINUE
        );
        set_enabled(root.path(), "hermes", true).unwrap();
        assert!(!disabled(root.path(), "hermes").unwrap());
        // The default needs the agent installed: a test machine has none of these CLIs on PATH
        // reliably, so only the refusal path is asserted here; the CLI test sets one up.
        let s = set_default(root.path(), None).unwrap();
        assert_eq!(s.default, None);
        // Disabling the default returns to Auto.
        let mut with_default = read(root.path()).unwrap();
        with_default.default = Some("codex".into());
        write(root.path(), &with_default).unwrap();
        let s = set_enabled(root.path(), "codex", false).unwrap();
        assert_eq!(s.default, None);
        switchboard_core::private_fs::private_write(&root.path().join(FILE), b"broken").unwrap();
        assert!(read(root.path()).is_err());
    }

    #[test]
    fn windows_programs_are_recognised_through_the_canonical_prefix_and_any_case() {
        let system = Path::new(r"C:\Windows");
        assert!(under_folder(
            Path::new(r"\\?\C:\WINDOWS\System32\cmd.exe"),
            system
        ));
        assert!(under_folder(
            Path::new("c:/windows/system32/cmd.exe"),
            system
        ));
        assert!(!under_folder(Path::new(r"C:\WindowsApps\cmd.exe"), system));
        assert!(!under_folder(
            Path::new(r"\\?\C:\Users\a\.local\bin\cmd.exe"),
            system
        ));
        assert!(!under_folder(Path::new(r"C:\Windows\x"), Path::new("")));
    }

    #[test]
    fn a_newer_file_still_reads_and_an_unusable_default_is_not_ready() {
        let root = tempfile::tempdir().unwrap();
        // A field a newer Switchboard added is ignored, never "unreadable".
        switchboard_core::private_fs::private_write(
            &root.path().join(FILE),
            br#"{"default":"no-such-agent","disabled":["kimi-code"],"later":true}"#,
        )
        .unwrap();
        assert!(disabled(root.path(), "kimi-code").unwrap());
        let view = overview(root.path(), None).unwrap();
        assert_eq!(view["schema"], 1);
        assert_eq!(view["revision"].as_str().map(str::len), Some(16));
        assert!(view["updated_at"].as_u64().is_some());
        let again = overview(root.path(), None).unwrap();
        assert_eq!(again["revision"], view["revision"], "a revision is stable");
        set_enabled(root.path(), "kimi-code", true).unwrap();
        assert_ne!(
            overview(root.path(), None).unwrap()["revision"],
            view["revision"]
        );
        assert_eq!(view["default"], "no-such-agent");
        assert_eq!(
            view["default_ready"], false,
            "the saved default cannot take work"
        );
        let kimi = view["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "kimi-code")
            .unwrap();
        assert_eq!(
            (kimi["disabled"].clone(), kimi["enabled"].clone()),
            (json!(true), json!(false))
        );
    }

    #[test]
    fn the_command_shows_the_openrouter_model() {
        let agent = known("opencode").unwrap();
        assert_eq!(
            argv(&agent, Some("a/b")),
            ["opencode", "-m", "openrouter/a/b"]
        );
        assert_eq!(command(&agent, None), "opencode");
        assert_eq!(
            command(&agent, Some("xiaomi/mimo-v2.6-pro")),
            "opencode -m openrouter/xiaomi/mimo-v2.6-pro"
        );
        assert_eq!(
            command(&known("claude-code").unwrap(), Some("x/y")),
            "claude"
        );
    }
}
