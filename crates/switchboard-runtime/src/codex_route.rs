//! SB-143 (operator 2026-10-19: «кодекс не атрибуцирован»): Codex sessions in a project's folders
//! use the project's Codex accounts. Codex ignores `model_provider`, `model_providers` and
//! `openai_base_url` in a project's `.codex/config.toml` (its project-config denylist), so a folder
//! file cannot route it as `.claude/settings.local.json` routes Claude Code. Instead one
//! user-level provider in `<CODEX_HOME>/config.toml` points every Codex session at the proxy's
//! `/codex/_auto/v1`, which uses a project's account for a request from its folder and passes any
//! other request on with the person's own sign-in.
//!
//! The switch is the person's: `<data>/codex-route` says whether it is on. While it is on, the
//! owner writes the provider at start and removes it at stop, so a Codex started while
//! Switchboard is not running reaches OpenAI directly. Switchboard writes only between its own
//! marker lines and refuses when the person has chosen another `model_provider`.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) const PROVIDER: &str = "switchboard-auto";
const HEAD_OPEN: &str = "# >>> Fabric Switchboard: Codex in project folders (SB-143) >>>";
const TAIL_OPEN: &str = "# >>> Fabric Switchboard: provider (SB-143) >>>";
const CLOSE: &str = "# <<< Fabric Switchboard <<<";
const SETTING: &str = "codex-route";
pub const CODEX_ROUTE_FOREIGN: &str = "Codex already uses another model provider (model_provider in its config.toml). Switchboard leaves it as it is; remove that line to let project folders use their Codex accounts.";
pub const CODEX_ROUTE_UNREADABLE: &str =
    "Codex's config.toml could not be read or would not stay valid. Nothing was changed.";

/// The folders of projects whose pool has an enabled Codex account, with their pools.
pub(crate) fn folders(snapshot: &switchboard_core::Snapshot) -> Vec<(String, String)> {
    // SB-109: with organizations, personal groups first, as `Snapshot::project_for` and the
    // Claude bindings order them (the more private wins).
    let mut projects: Vec<_> = snapshot.projects.iter().collect();
    if !snapshot.organizations.is_empty() {
        projects.sort_by_key(|p| p.organization.is_some());
    }
    projects
        .into_iter()
        .filter(|p| {
            snapshot.accounts.iter().any(|a| {
                a.pool == p.pool && a.enabled && a.provider == switchboard_core::Provider::Codex
            })
        })
        .flat_map(|p| p.folders.iter().map(move |f| (f.clone(), p.pool.clone())))
        // SB-146: a folder in several groups goes to the first with a Codex account.
        .fold(Vec::new(), |mut out: Vec<(String, String)>, (f, pool)| {
            if !out.iter().any(|(g, _)| *g == f) {
                out.push((f, pool));
            }
            out
        })
}

/// `<CODEX_HOME or ~/.codex>/config.toml`, the person's own: never a Switchboard home's
/// (SB-183).
pub(crate) fn config_path() -> Option<PathBuf> {
    config_path_from(&crate::person::Sources::process())
}
pub(crate) fn config_path_from(sources: &crate::person::Sources) -> Option<PathBuf> {
    crate::person::codex_dir_from(sources).map(|d| d.join("config.toml"))
}

pub(crate) fn wanted(root: &Path) -> bool {
    std::fs::read_to_string(root.join(SETTING)).is_ok_and(|s| s.trim() == "on")
}
pub(crate) fn set_wanted(root: &Path, on: bool) -> Result<(), String> {
    crate::launch::private_write(
        &root.join(SETTING),
        if on { b"on\n" } else { b"off\n" },
        false,
    )
}

/// The text without Switchboard's blocks.
fn stripped(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if trimmed == HEAD_OPEN || trimmed == TAIL_OPEN {
            inside = true;
            continue;
        }
        if inside {
            if trimmed == CLOSE {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

fn toml_quote(value: &str) -> String {
    toml::Value::String(value.into()).to_string()
}

/// The config with Switchboard's provider: the selection at the top (a top-level key must come
/// before any table), the provider table at the end, the person's text between, untouched.
pub(crate) fn with_route(text: &str, address: &str, capability: &str) -> Result<String, String> {
    with_provider(text, address, capability, true)
}
/// [`with_route`] with `requires_openai_auth` chosen: an organization's Codex shell home (SB-183)
/// holds no sign-in, so it sends none (measured: Codex 0.163.0 sends the capability header and no
/// `Authorization` either way, `docs/evidence/shell-home-measurement-2026-10.md`).
pub(crate) fn with_provider(
    text: &str,
    address: &str,
    capability: &str,
    requires_auth: bool,
) -> Result<String, String> {
    let own = stripped(text);
    let parsed: toml::Table = own.parse().map_err(|_| CODEX_ROUTE_UNREADABLE)?;
    if let Some(chosen) = parsed.get("model_provider") {
        if chosen.as_str() != Some(PROVIDER) {
            return Err(CODEX_ROUTE_FOREIGN.into());
        }
    }
    let head = format!(
        "{HEAD_OPEN}\nmodel_provider = {}\n{CLOSE}\n",
        toml_quote(PROVIDER)
    );
    let tail = format!(
        "{TAIL_OPEN}\n[model_providers.{PROVIDER}]\nname = \"Fabric Switchboard\"\nbase_url = {}\nwire_api = \"responses\"\nrequires_openai_auth = {requires_auth}\nsupports_websockets = false\nhttp_headers = {{ \"X-Switchboard-Capability\" = {} }}\n{CLOSE}\n",
        toml_quote(&format!("http://{address}/codex/{}/v1", switchboard_proxy::AUTO_POOL)),
        toml_quote(capability),
    );
    let mut body = own;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    // Nothing outside the markers is added, so taking the blocks out gives the text back exactly.
    let out = format!("{head}{body}{tail}");
    out.parse::<toml::Table>()
        .map_err(|_| CODEX_ROUTE_UNREADABLE)?;
    Ok(out)
}

fn write_config(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| CODEX_ROUTE_UNREADABLE)?;
    }
    // The provider carries a local capability: the file stays the person's own (0600).
    crate::launch::private_write(path, text.as_bytes(), false)
}

/// Writes the provider (when the text changes). Ok(true): written now.
pub(crate) fn apply(path: &Path, address: &str, capability: &str) -> Result<bool, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(CODEX_ROUTE_UNREADABLE.into()),
    };
    let next = with_route(&text, address, capability)?;
    if next == text {
        return Ok(false);
    }
    write_config(path, &next)?;
    Ok(true)
}

/// Takes Switchboard's blocks out (when there are any). Ok(true): removed now.
pub(crate) fn remove(path: &Path) -> Result<bool, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(false);
    };
    let next = stripped(&text);
    if next == text {
        return Ok(false);
    }
    write_config(path, &next)?;
    Ok(true)
}

/// `{on, applied, config, conflict}` — whether the switch is on, whether Codex's config carries
/// the provider now, and why it cannot when it cannot.
pub(crate) fn status(root: &Path, path: Option<&Path>) -> Value {
    let text = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let applied = text.lines().any(|l| l.trim_end() == TAIL_OPEN);
    let conflict = stripped(&text)
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| {
            t.get("model_provider")
                .and_then(|v| v.as_str().map(str::to_owned))
        })
        .filter(|p| p != PROVIDER)
        .map(|_| CODEX_ROUTE_FOREIGN);
    json!({
        "on": wanted(root),
        "applied": applied,
        "config": path.map(|p| p.to_string_lossy().into_owned()),
        "conflict": conflict,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str =
        "# my settings\nmodel = \"gpt-5\"\n\n[projects.\"/w/app\"]\ntrust_level = \"trusted\"\n";

    #[test]
    fn the_route_is_never_written_into_a_shell_home() {
        // SB-183: `serve` or an offline CLI started in a hooked terminal sees the group's Codex
        // shell home as CODEX_HOME. SB-143's provider goes into the person's own config.toml.
        let person = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let shell_home = data.path().join("shell").join("codex-acme");
        std::fs::create_dir_all(&shell_home).unwrap();
        let env = |name: &str| (name == "CODEX_HOME").then(|| shell_home.clone().into_os_string());
        let sources = crate::person::Sources {
            env: &env,
            home: Some(person.path().to_owned()),
            data: Some(data.path().to_owned()),
        };
        let path = config_path_from(&sources).unwrap();
        assert_eq!(path, person.path().join(".codex").join("config.toml"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert!(apply(&path, "127.0.0.1:1", "synthetic-capability").unwrap());
        assert!(!shell_home.join("config.toml").exists());
        // The person's own CODEX_HOME elsewhere is still theirs.
        let elsewhere = person.path().join("codex-own");
        let env = |name: &str| (name == "CODEX_HOME").then(|| elsewhere.clone().into_os_string());
        let sources = crate::person::Sources {
            env: &env,
            home: Some(person.path().to_owned()),
            data: Some(data.path().to_owned()),
        };
        assert_eq!(
            config_path_from(&sources),
            Some(elsewhere.join("config.toml"))
        );
    }

    #[test]
    fn the_provider_goes_in_around_the_persons_text_and_comes_out_leaving_it_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, PERSON).unwrap();
        assert!(apply(&path, "127.0.0.1:4000", "cap-1").unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Table = text.parse().unwrap();
        assert_eq!(parsed["model_provider"].as_str(), Some(PROVIDER));
        assert_eq!(parsed["model"].as_str(), Some("gpt-5"));
        let provider = &parsed["model_providers"][PROVIDER];
        assert_eq!(
            provider["base_url"].as_str(),
            Some("http://127.0.0.1:4000/codex/_auto/v1")
        );
        assert_eq!(provider["requires_openai_auth"].as_bool(), Some(true));
        assert_eq!(
            provider["http_headers"]["X-Switchboard-Capability"].as_str(),
            Some("cap-1")
        );
        assert_eq!(
            parsed["projects"]["/w/app"]["trust_level"].as_str(),
            Some("trusted")
        );
        assert!(text.contains(PERSON), "the person's text is untouched");
        assert!(
            !apply(&path, "127.0.0.1:4000", "cap-1").unwrap(),
            "no change"
        );
        // A new port or capability rewrites only Switchboard's lines.
        assert!(apply(&path, "127.0.0.1:4001", "cap-2").unwrap());
        assert_eq!(
            std::fs::read_to_string(&path)
                .unwrap()
                .matches(TAIL_OPEN)
                .count(),
            1
        );
        assert!(remove(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), PERSON);
        assert!(!remove(&path).unwrap());
    }

    #[test]
    fn another_provider_chosen_by_the_person_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mine = "model_provider = \"ollama\"\n";
        std::fs::write(&path, mine).unwrap();
        assert_eq!(
            apply(&path, "127.0.0.1:4000", "cap").unwrap_err(),
            CODEX_ROUTE_FOREIGN
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), mine);
        assert_eq!(
            status(dir.path(), Some(&path))["conflict"].as_str(),
            Some(CODEX_ROUTE_FOREIGN)
        );
        // A file that does not parse is left alone.
        std::fs::write(&path, "[broken").unwrap();
        assert_eq!(
            apply(&path, "127.0.0.1:4000", "cap").unwrap_err(),
            CODEX_ROUTE_UNREADABLE
        );
        // No file yet: one with the provider only.
        let fresh = dir.path().join("new").join("config.toml");
        assert!(apply(&fresh, "127.0.0.1:4000", "cap").unwrap());
        assert!(remove(&fresh).unwrap());
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "");
    }

    #[test]
    fn only_projects_with_a_codex_account_route_their_folders() {
        use switchboard_core::{Account, AuthKind, Project, Provider, Snapshot};
        let account = |pool: &str, provider: Provider| Account {
            id: format!("{pool}-{}", provider.as_str()),
            label: "x".into(),
            provider,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            ..serde_json::from_value(serde_json::json!({
                "id": "x", "label": "x", "provider": "codex", "kind": "oauth",
                "pool": "x", "enabled": true, "created_at": 1
            }))
            .unwrap()
        };
        let project = |pool: &str, folder: &str| Project {
            pool: pool.into(),
            name: pool.into(),
            folders: vec![folder.into()],
            created_at: 1,
            sources: vec![],
            organization: None,
        };
        let snapshot = Snapshot {
            projects: vec![project("a", "/w/a"), project("b", "/w/b")],
            accounts: vec![
                account("a", Provider::Codex),
                account("b", Provider::Claude),
            ],
            ..Default::default()
        };
        assert_eq!(folders(&snapshot), [("/w/a".to_owned(), "a".to_owned())]);
        // SB-109: a folder in an organization's group and a personal one goes to the personal one.
        let mut shared = Snapshot {
            projects: vec![project("org", "/w/s"), project("own", "/w/s")],
            accounts: vec![
                account("org", Provider::Codex),
                account("own", Provider::Codex),
            ],
            ..Default::default()
        };
        shared.organizations.push(switchboard_core::Organization {
            id: "00000000-0000-4000-8000-000000000001".into(),
            name: "Acme".into(),
            domains: vec![],
            provider_organizations: vec![],
            created_at: 1,
            vesta_id: None,
        });
        shared.projects[0].organization = Some("00000000-0000-4000-8000-000000000001".into());
        assert_eq!(folders(&shared), [("/w/s".to_owned(), "own".to_owned())]);
    }
}
