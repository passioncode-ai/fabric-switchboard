//! SB-390 (operator, 2026-11-16: «добавить возможность запустить гермеса на антропик токене
//! апвиллис и моем на выбор тоже как опция»): Anthropic API keys agents can run on directly, each
//! under a name the person picks (`team-a`, `personal`), chosen at launch.
//!
//! The value lives only in the vault (`Store::put_secret`); `anthropic-keys.json` (0600) holds
//! each key's name, its vault item, when it was saved and its default model, never the value. It
//! is a file of its own, not `accounts.json`, so an older Switchboard still reads that file. A
//! session script reads the value when it starts (`switchboard agents anthropic key <name>`), as
//! an OpenRouter launch does; nothing writes it to disk. Switchboard does not meter these keys:
//! Anthropic offers no read of a key's spend, so no ceiling counts them.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::Store;

pub const FILE: &str = "anthropic-keys.json";
const MAX_KEYS: usize = 16;
const MAX_KEY: usize = 512;

pub const NAME_INVALID: &str =
    "A key's name is 1 to 32 lowercase letters, digits and hyphens, such as team-a or personal.";
pub const KEY_INVALID: &str = "That is not an Anthropic API key: it starts with sk-ant-api.";
/// SB-392 K-3: an Admin key (`sk-ant-admin`) manages an organization and cannot run a model.
pub const ADMIN_KEY: &str = "That is an Anthropic Admin key: it manages an organization and cannot run a model. Create an API key in Claude Console (platform.claude.com, Settings, API keys) and save that one.";
pub const MODEL_INVALID: &str =
    "A model is an Anthropic id such as claude-sonnet-5-5 (letters, digits and . _ : -).";
pub const NO_KEY: &str = "No Anthropic key is saved under that name. Save one with switchboard agents anthropic set <name>.";
pub const TOO_MANY: &str = "At most 16 Anthropic keys can be saved.";
pub const UNREADABLE: &str =
    "The Anthropic keys file (anthropic-keys.json) could not be read. Save a key again to replace it.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub name: String,
    /// The vault item holding the value.
    pub item: String,
    pub saved_at: i64,
    /// The model agents start on unless a launch names another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Keys {
    pub(crate) keys: Vec<Key>,
}

pub fn name_valid(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// An Anthropic model id (`claude-sonnet-5-5`, `claude-opus-5-5-20261101`): no `/`, unlike an
/// OpenRouter id.
pub fn model_valid(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 120
        && model.as_bytes()[0].is_ascii_alphanumeric()
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// A standard API key (`sk-ant-api…`); an Admin key (`sk-ant-admin…`) calls no model.
pub fn key_valid(key: &str) -> bool {
    key.starts_with("sk-ant-api")
        && key.len() <= MAX_KEY
        && key.bytes().all(|b| b.is_ascii_graphic())
}

pub(crate) fn read(root: &Path) -> Result<Keys, String> {
    match std::fs::read(root.join(FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| UNREADABLE.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Keys::default()),
        Err(_) => Err(UNREADABLE.into()),
    }
}

fn write(root: &Path, keys: &Keys) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(keys).map_err(|_| "Anthropic keys could not be saved.")?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| "Anthropic keys could not be saved.".to_string())
}

/// The saved keys by name, without their values.
pub fn list(root: &Path) -> Result<Value, String> {
    let keys = read(root)?;
    Ok(json!({
        "keys": keys.keys.iter().map(|k| json!({"name": k.name, "saved_at": k.saved_at, "model": k.model})).collect::<Vec<_>>(),
    }))
}

pub fn find(root: &Path, name: &str) -> Result<Key, String> {
    read(root)?
        .keys
        .into_iter()
        .find(|k| k.name == name)
        .ok_or_else(|| NO_KEY.to_string())
}

/// Saves (or replaces) the key named `name`; `model` sets its default model when given and keeps
/// the saved one otherwise. The value goes to the vault first; a file that cannot be written
/// leaves a new vault item removed again.
pub fn set(
    root: &Path,
    store: &Store,
    name: &str,
    key: &str,
    model: Option<&str>,
    now: i64,
) -> Result<Value, String> {
    if !name_valid(name) {
        return Err(NAME_INVALID.into());
    }
    let key = key.trim();
    if key.starts_with("sk-ant-admin") {
        return Err(ADMIN_KEY.into());
    }
    if !key_valid(key) {
        return Err(KEY_INVALID.into());
    }
    if model.is_some_and(|m| !model_valid(m)) {
        return Err(MODEL_INVALID.into());
    }
    let mut keys = read(root)?;
    let existing = keys.keys.iter().position(|k| k.name == name);
    if existing.is_none() && keys.keys.len() >= MAX_KEYS {
        return Err(TOO_MANY.into());
    }
    let item = store.put_secret(existing.map(|i| keys.keys[i].item.as_str()), key)?;
    match existing {
        Some(i) => {
            let entry = &mut keys.keys[i];
            entry.saved_at = now;
            if let Some(m) = model {
                entry.model = Some(m.to_owned());
            }
        }
        None => keys.keys.push(Key {
            name: name.to_owned(),
            item: item.clone(),
            saved_at: now,
            model: model.map(str::to_owned),
        }),
    }
    if let Err(error) = write(root, &keys) {
        if existing.is_none() {
            let _ = store.delete_secret(&item);
        }
        return Err(error);
    }
    crate::oplog::event(
        "anthropic_key",
        &[("outcome", crate::oplog::Field::Code("saved"))],
    );
    let saved = keys.keys.iter().find(|k| k.name == name).cloned();
    Ok(json!({"name": name, "model": saved.and_then(|k| k.model)}))
}

pub fn set_model(root: &Path, name: &str, model: &str) -> Result<Value, String> {
    if !model_valid(model) {
        return Err(MODEL_INVALID.into());
    }
    let mut keys = read(root)?;
    let entry = keys
        .keys
        .iter_mut()
        .find(|k| k.name == name)
        .ok_or(NO_KEY)?;
    entry.model = Some(model.to_owned());
    write(root, &keys)?;
    Ok(json!({"name": name, "model": model}))
}

/// Removes the key and its vault item; a name with no key removes nothing.
pub fn remove(root: &Path, store: &Store, name: &str) -> Result<Value, String> {
    let mut keys = read(root)?;
    let Some(i) = keys.keys.iter().position(|k| k.name == name) else {
        return Ok(json!({"removed": false}));
    };
    let entry = keys.keys.remove(i);
    write(root, &keys)?;
    let _ = store.delete_secret(&entry.item);
    crate::oplog::event(
        "anthropic_key",
        &[("outcome", crate::oplog::Field::Code("removed"))],
    );
    Ok(json!({"removed": true}))
}

/// Every saved key's vault item, for `switchboard uninstall`; an unreadable file names none.
pub fn vault_items(root: &Path) -> Vec<String> {
    read(root)
        .map(|k| k.keys.into_iter().map(|k| k.item).collect())
        .unwrap_or_default()
}

/// The value of the key named `name`, for a session script's `export` (the CLI only).
pub fn value(root: &Path, store: &Store, name: &str) -> Result<String, String> {
    let entry = find(root, name)?;
    store.secret(&entry.item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keys_and_models_are_checked() {
        assert!(name_valid("team-a") && name_valid("personal") && name_valid("team-2"));
        assert!(!name_valid("") && !name_valid("Team-A") && !name_valid("-x"));
        assert!(!name_valid(&"a".repeat(33)) && !name_valid("a b") && !name_valid("a/b"));
        assert!(key_valid("sk-ant-api03-abcDEF_123"));
        assert!(
            !key_valid("sk-ant-admin01-abc"),
            "an Admin key calls no model"
        );
        assert!(!key_valid("sk-or-v1-abc") && !key_valid("sk-ant-api03 abc"));
        assert!(model_valid("claude-sonnet-5-5") && model_valid("claude-opus-5-5-20261101"));
        assert!(
            !model_valid("anthropic/claude-sonnet-5.5"),
            "an OpenRouter id is refused"
        );
    }

    #[test]
    fn a_key_is_saved_in_the_vault_replaced_by_name_and_removed() {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(
            dir.path().into(),
            Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap();
        let root = dir.path();
        assert_eq!(
            set(root, &store, "team-a", "sk-ant-admin01-x", None, 1).unwrap_err(),
            ADMIN_KEY
        );
        assert_eq!(
            set(root, &store, "team-a", "sk-or-v1-x", None, 1).unwrap_err(),
            KEY_INVALID
        );
        set(
            root,
            &store,
            "team-a",
            " sk-ant-api03-first ",
            Some("claude-sonnet-5-5"),
            1,
        )
        .unwrap();
        set(root, &store, "personal", "sk-ant-api03-mine", None, 2).unwrap();
        assert_eq!(value(root, &store, "team-a").unwrap(), "sk-ant-api03-first");
        let item = find(root, "team-a").unwrap().item;
        // Saved again under the same name: the same vault item, the model kept.
        set(root, &store, "team-a", "sk-ant-api03-second", None, 3).unwrap();
        let again = find(root, "team-a").unwrap();
        assert_eq!(again.item, item);
        assert_eq!(again.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(
            value(root, &store, "team-a").unwrap(),
            "sk-ant-api03-second"
        );
        assert_eq!(list(root).unwrap()["keys"].as_array().unwrap().len(), 2);
        assert_eq!(remove(root, &store, "personal").unwrap()["removed"], true);
        assert_eq!(remove(root, &store, "personal").unwrap()["removed"], false);
        assert_eq!(value(root, &store, "personal").unwrap_err(), NO_KEY);
    }

    #[test]
    fn a_file_of_names_never_holds_a_value_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys {
            keys: vec![Key {
                name: "team-a".into(),
                item: "00000000-0000-4000-8000-000000000001".into(),
                saved_at: 10,
                model: Some("claude-sonnet-5-5".into()),
            }],
        };
        write(dir.path(), &keys).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!text.contains("sk-ant"));
        let listed = list(dir.path()).unwrap();
        assert_eq!(listed["keys"][0]["name"], "team-a");
        assert_eq!(listed["keys"][0]["model"], "claude-sonnet-5-5");
        assert!(
            listed["keys"][0].get("item").is_none(),
            "the vault item stays inside"
        );
        assert_eq!(find(dir.path(), "personal").unwrap_err(), NO_KEY);
        assert_eq!(
            set_model(dir.path(), "team-a", "claude-opus-5-5").unwrap()["model"],
            "claude-opus-5-5"
        );
        std::fs::write(dir.path().join(FILE), b"{oops").unwrap();
        assert_eq!(list(dir.path()).unwrap_err(), UNREADABLE);
    }
}
