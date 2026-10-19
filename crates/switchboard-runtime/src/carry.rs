//! SB-137 (Fabric board finding 2087ac21, 2026-10-16) and SB-130: a session Switchboard launches
//! in a home of its own (`CLAUDE_CONFIG_DIR`, Pi's `PI_CODING_AGENT_DIR`) lost the operator's
//! global instructions, settings, hooks, plugins and skills, and a Claude Code continuation sat
//! on the first-run and folder-trust dialogs nobody answers in a console. The operator's own
//! files are carried over as links, so they stay current; settings are merged in without anything
//! that signs in or looks like a secret. Nothing here reads or writes a credential.
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// What a Claude Code home links from the operator's (`~/.claude`, or their `CLAUDE_CONFIG_DIR`).
pub(crate) const CLAUDE_LINKS: &[&str] = &[
    "CLAUDE.md",
    "skills",
    "commands",
    "agents",
    "output-styles",
    "plugins",
    "hooks",
];
/// What a Pi home links from the operator's (`~/.pi/agent`).
pub(crate) const PI_LINKS: &[&str] = &["AGENTS.md", "skills", "extensions", "prompts", "themes"];
/// Settings that sign in or run a helper that hands out credentials: never carried.
const CLAUDE_AUTH_KEYS: &[&str] = &[
    "env",
    "apiKeyHelper",
    "awsAuthRefresh",
    "awsCredentialExport",
    "otelHeadersHelper",
    "forceLoginMethod",
    "forceLoginOrgUUID",
];

/// The operator's Claude Code user folder, as their own sessions see it.
pub(crate) fn claude_user_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}
/// The operator's Pi folder (`PI_CODING_AGENT_DIR`, else `~/.pi/agent`).
pub(crate) fn pi_user_dir() -> Option<PathBuf> {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".pi").join("agent")))
}

/// Links each of `names` that exists in `source` into `home`. A link Switchboard made before is
/// renewed; a real file or folder in `home` is never replaced. Returns how many links point there.
pub(crate) fn link_into(home: &Path, source: &Path, names: &[&str]) -> usize {
    if same_dir(home, source) {
        return 0;
    }
    let mut linked = 0;
    for name in names {
        let from = source.join(name);
        if std::fs::symlink_metadata(&from).is_err() {
            continue;
        }
        let to = home.join(name);
        match std::fs::symlink_metadata(&to) {
            Ok(m) if m.file_type().is_symlink() => {
                if std::fs::read_link(&to).ok().as_deref() == Some(from.as_path()) {
                    linked += 1;
                    continue;
                }
                if std::fs::remove_file(&to).is_err() {
                    continue;
                }
            }
            Ok(_) => continue,
            Err(_) => {}
        }
        if symlink(&from, &to).is_ok() {
            linked += 1;
        }
    }
    linked
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}
/// Windows needs a privilege for symbolic links; nothing is linked there yet.
#[cfg(not(unix))]
fn symlink(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("links are not made on this platform"))
}

/// A key that names a secret by its shape (`apiKey`, `token`, `password`, …).
fn secret_shaped(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "password",
        "auth",
        "credential",
        "cookie",
    ]
    .iter()
    .any(|w| k.contains(w))
}

/// The operator's settings without what signs in or looks like a secret (top-level keys only;
/// `hooks`, `permissions`, `enabledPlugins` and the like are kept whole). Absent, unreadable or
/// not an object: nothing.
pub(crate) fn carried_settings(path: &Path, drop: &[&str]) -> Map<String, Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Map::new();
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(&text) else {
        return Map::new();
    };
    settings.retain(|k, _| !drop.contains(&k.as_str()) && !secret_shaped(k));
    settings
}

/// A Claude Code home's `settings.json`: the operator's carried settings, then this launch's
/// own `env` (the isolated account's credential, or nothing in managed mode) on top.
pub(crate) fn claude_settings(user: Option<&Path>, env: Option<Value>) -> Value {
    let mut settings = user
        .map(|u| carried_settings(&u.join("settings.json"), CLAUDE_AUTH_KEYS))
        .unwrap_or_default();
    if let Some(env) = env {
        settings.insert("env".into(), env);
    }
    Value::Object(settings)
}

/// A Pi home's `settings.json`: the operator's, without what signs in or looks like a secret.
pub(crate) fn pi_settings(user: &Path) -> Option<Value> {
    let settings = carried_settings(&user.join("settings.json"), &[]);
    (!settings.is_empty()).then_some(Value::Object(settings))
}

/// Marks a Claude Code home's `.claude.json` as past the first-run screens and, when `trust` names
/// a folder, that folder as trusted (SB-130: a continuation works in the folder its previous agent
/// worked in; operator rule 2026-10-17, decided 2026-10-19). A file that is not a JSON object is
/// left as it is. Returns whether it was written.
pub(crate) fn claude_state(home: &Path, trust: Option<&Path>) -> Result<bool, String> {
    let path = home.join(".claude.json");
    let mut state = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => m,
            _ => return Ok(false),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(_) => return Ok(false),
    };
    state.insert("hasCompletedOnboarding".into(), Value::Bool(true));
    if let Some(folder) = trust {
        let projects = state
            .entry("projects")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(projects) = projects {
            let entry = projects
                .entry(folder.to_string_lossy().into_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(entry) = entry {
                entry.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
            }
        }
    }
    // A plain JSON object always serialises; an empty result would mean nothing to write.
    let bytes = serde_json::to_vec(&Value::Object(state)).unwrap_or_default();
    if bytes.is_empty() {
        return Ok(false);
    }
    crate::launch::private_write(&path, &bytes, false)?;
    Ok(true)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_operators_files_are_linked_and_a_real_file_in_the_home_is_kept() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("CLAUDE.md"), "rules").unwrap();
        std::fs::create_dir(user.path().join("skills")).unwrap();
        std::fs::create_dir(user.path().join("commands")).unwrap();
        std::fs::write(home.path().join("commands"), "the home's own").unwrap();
        assert_eq!(link_into(home.path(), user.path(), CLAUDE_LINKS), 2);
        assert_eq!(
            std::fs::read_to_string(home.path().join("CLAUDE.md")).unwrap(),
            "rules",
            "linked, so it stays current"
        );
        assert!(home.path().join("skills").is_dir());
        assert_eq!(
            std::fs::read_to_string(home.path().join("commands")).unwrap(),
            "the home's own"
        );
        // Again: nothing changes, nothing breaks.
        assert_eq!(link_into(home.path(), user.path(), CLAUDE_LINKS), 2);
        // A link to an older place is renewed.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("CLAUDE.md"), "new").unwrap();
        link_into(home.path(), elsewhere.path(), CLAUDE_LINKS);
        assert_eq!(
            std::fs::read_to_string(home.path().join("CLAUDE.md")).unwrap(),
            "new"
        );
        // The operator's own folder is never linked into itself.
        assert_eq!(link_into(user.path(), user.path(), CLAUDE_LINKS), 0);
    }

    #[test]
    fn settings_keep_hooks_and_permissions_and_drop_what_signs_in() {
        let user = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("settings.json"),
            json!({
                "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "/x"}]}]},
                "permissions": {"deny": ["Bash(rm -rf /)"]},
                "enabledPlugins": {"p@m": true},
                "env": {"ANTHROPIC_API_KEY": "synthetic-not-a-key"},
                "apiKeyHelper": "/bin/helper",
                "someToken": "synthetic",
                "model": "opus"
            })
            .to_string(),
        )
        .unwrap();
        let settings = claude_settings(
            Some(user.path()),
            Some(json!({"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-launch"})),
        );
        let keys: Vec<&String> = settings.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["enabledPlugins", "env", "hooks", "model", "permissions"]
        );
        assert_eq!(
            settings["env"],
            json!({"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-launch"}),
            "the launch's own env only, never the operator's"
        );
        // Managed mode: no env at all; an absent or broken file carries nothing.
        assert!(claude_settings(Some(user.path()), None)
            .get("env")
            .is_none());
        std::fs::write(user.path().join("settings.json"), "{broken").unwrap();
        assert_eq!(claude_settings(Some(user.path()), None), json!({}));
        assert_eq!(claude_settings(None, None), json!({}));
    }

    #[test]
    fn a_continuation_trusts_its_own_folder_and_nothing_else() {
        let home = tempfile::tempdir().unwrap();
        assert!(claude_state(home.path(), None).unwrap());
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert!(
            state.get("projects").is_none(),
            "an ordinary launch keeps the trust dialog"
        );
        std::fs::write(
            home.path().join(".claude.json"),
            json!({"userID": "u", "projects": {"/w/other": {"x": 1}}}).to_string(),
        )
        .unwrap();
        assert!(claude_state(home.path(), Some(Path::new("/w/app"))).unwrap());
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["userID"], "u", "what Claude Code wrote is kept");
        assert_eq!(state["projects"]["/w/app"]["hasTrustDialogAccepted"], true);
        assert_eq!(state["projects"]["/w/other"], json!({"x": 1}));
        std::fs::write(home.path().join(".claude.json"), "[1]").unwrap();
        assert!(!claude_state(home.path(), Some(Path::new("/w/app"))).unwrap());
        assert_eq!(
            std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
            "[1]",
            "not an object: left as it is"
        );
    }

    #[test]
    fn pi_settings_drop_what_looks_like_a_secret() {
        let user = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("settings.json"),
            json!({"skills": ["-x"], "apiKey": "synthetic", "theme": "dark"}).to_string(),
        )
        .unwrap();
        assert_eq!(
            pi_settings(user.path()),
            Some(json!({"skills": ["-x"], "theme": "dark"}))
        );
        assert_eq!(pi_settings(tempfile::tempdir().unwrap().path()), None);
    }
}
