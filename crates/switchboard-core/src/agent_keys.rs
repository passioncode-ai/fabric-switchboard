//! Keys for agents (SB-79, packet XA-02): one key per service — today OpenRouter — that agents
//! launched by Switchboard run on. The value lives only in the vault, under the UUID recorded
//! here with the service, when it was saved and the default model; the metadata never holds it.
use crate::{append_event, now, uuid_valid, vault, Credential, Snapshot, Store};
use serde::{Deserialize, Serialize};

/// The services an agent key may be for.
pub const SERVICES: &[&str] = &["openrouter"];
pub const KEY_INVALID: &str = "That is not an OpenRouter key: it starts with sk-or-.";
pub const MODEL_INVALID: &str =
    "A model is an OpenRouter id such as moonshotai/kimi-k2 (letters, digits and / . _ : -).";
pub const NO_AGENT_KEY: &str = "No OpenRouter key is saved. Add one under Agents.";
const MAX_KEY: usize = 512;
const MAX_MODEL: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentKey {
    /// `openrouter`.
    pub service: String,
    /// The vault item holding the value.
    pub id: String,
    pub saved_at: i64,
    /// The model agents start on unless a launch names another (an OpenRouter id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

fn service_valid(service: &str) -> bool {
    SERVICES.contains(&service)
}

/// An OpenRouter model id: `vendor/model`, optionally with a `:variant`.
pub fn model_valid(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL
        && model.contains('/')
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b':' | b'-'))
}

fn key_valid(service: &str, key: &str) -> bool {
    let shaped =
        !key.is_empty() && key.len() <= MAX_KEY && key.bytes().all(|b| b.is_ascii_graphic());
    shaped && (service != "openrouter" || key.starts_with("sk-or-"))
}

pub(crate) fn validate_agent_keys(s: &Snapshot) -> Result<(), String> {
    let mut services = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for k in &s.agent_keys {
        if !service_valid(&k.service)
            || !services.insert(&k.service)
            || !uuid_valid(&k.id)
            || !ids.insert(&k.id)
            || s.accounts.iter().any(|a| a.id == k.id)
            || k.saved_at <= 0
            || k.model.as_deref().is_some_and(|m| !model_valid(m))
        {
            return Err("Invalid agent key metadata".into());
        }
    }
    Ok(())
}

impl Store {
    /// Saves (or replaces) the key for a service; `model` sets the default model when given and
    /// keeps the saved one otherwise. A metadata failure puts the previous value back.
    pub fn set_agent_key(
        &self,
        service: &str,
        key: &str,
        model: Option<String>,
    ) -> Result<AgentKey, String> {
        if !service_valid(service) {
            return Err("Unknown key service.".into());
        }
        let key = key.trim();
        if !key_valid(service, key) {
            return Err(KEY_INVALID.into());
        }
        if model.as_deref().is_some_and(|m| !model_valid(m)) {
            return Err(MODEL_INVALID.into());
        }
        let credential = Credential {
            access_token: key.to_owned(),
            refresh_token: None,
            id_token: None,
            native_context: None,
            expires_at: None,
            account_id: None,
        };
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let existing = candidate
            .agent_keys
            .iter()
            .position(|k| k.service == service);
        let (entry, old) = match existing {
            Some(i) => {
                let entry = &mut candidate.agent_keys[i];
                let old = self
                    .vault
                    .get(&entry.id)
                    .map_err(vault::surface("Credential storage unavailable"))?;
                entry.saved_at = now();
                if model.is_some() {
                    entry.model = model;
                }
                (entry.clone(), Some(old))
            }
            None => {
                let entry = AgentKey {
                    service: service.to_owned(),
                    id: uuid::Uuid::new_v4().to_string(),
                    saved_at: now(),
                    model,
                };
                candidate.agent_keys.push(entry.clone());
                (entry, None)
            }
        };
        append_event(&mut candidate, "agent_key", None, "saved");
        crate::validate_snapshot(&candidate)?;
        self.vault
            .put(&entry.id, &credential)
            .map_err(vault::surface("Credential storage unavailable"))?;
        if self.publish(&mut state, candidate).is_err() {
            let restored = match old {
                Some(old) => self.vault.put(&entry.id, &old),
                None => self.vault.delete(&entry.id),
            };
            restored.map_err(|_| "Storage failure; credential cleanup requires recovery")?;
            return Err("Account metadata could not be saved".into());
        }
        self.changed();
        Ok(entry)
    }

    /// Changes only the default model of a saved key.
    pub fn set_agent_key_model(&self, service: &str, model: &str) -> Result<AgentKey, String> {
        if !model_valid(model) {
            return Err(MODEL_INVALID.into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let entry = candidate
            .agent_keys
            .iter_mut()
            .find(|k| k.service == service)
            .ok_or(NO_AGENT_KEY)?;
        entry.model = Some(model.to_owned());
        let entry = entry.clone();
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(entry)
    }

    /// The saved key's metadata, never its value.
    pub fn agent_key(&self, service: &str) -> Result<Option<AgentKey>, String> {
        Ok(self
            .snapshot()?
            .agent_keys
            .into_iter()
            .find(|k| k.service == service))
    }

    /// The key's value, for a launched agent's environment only. Never returned to the window.
    pub fn agent_key_value(&self, service: &str) -> Result<String, String> {
        let entry = self.agent_key(service)?.ok_or(NO_AGENT_KEY)?;
        Ok(self
            .vault
            .get(&entry.id)
            .map_err(vault::surface("Credential storage unavailable"))?
            .access_token)
    }

    /// Removes the key: the metadata first, then the vault item (a leftover item is harmless and
    /// unreachable; a leftover entry without its item would not be).
    pub fn remove_agent_key(&self, service: &str) -> Result<bool, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let Some(i) = candidate
            .agent_keys
            .iter()
            .position(|k| k.service == service)
        else {
            return Ok(false);
        };
        let entry = candidate.agent_keys.remove(i);
        append_event(&mut candidate, "agent_key", None, "removed");
        self.publish(&mut state, candidate)?;
        self.changed();
        let _ = self.vault.delete(&entry.id);
        Ok(true)
    }
}
