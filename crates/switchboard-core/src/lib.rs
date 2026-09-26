//! Private account storage. Secret-bearing types deliberately do not implement Debug.
mod credential;
mod persistence;
mod vault;

pub use credential::Credential;
pub use vault::{MemoryVault, NativeVault, Vault};

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_ACCOUNTS: usize = 256;
pub(crate) const MAX_EVENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}
impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    SetupToken,
    #[serde(rename = "oauth")]
    OAuth,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub used_percent: f64,
    pub observed_at: i64,
    pub resets_at: Option<i64>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub provider: Provider,
    pub kind: AuthKind,
    pub pool: String,
    pub enabled: bool,
    pub created_at: i64,
    pub identity: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at: i64,
    pub action: String,
    pub account_id: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub accounts: Vec<Account>,
    pub routes: BTreeMap<String, String>,
    pub events: Vec<Event>,
}

/// A process-lifetime exclusive owner of one private metadata directory.
pub struct Store {
    root: PathBuf,
    _lock: File,
    vault: Arc<dyn Vault>,
    state: Mutex<Snapshot>,
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0)
}
fn label_valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 80 && !s.chars().any(char::is_control)
}
pub(crate) fn pool_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
fn uuid_valid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|u| u.to_string() == s)
}
fn usage_valid(u: &Usage) -> bool {
    u.used_percent.is_finite()
        && (0.0..=100.0).contains(&u.used_percent)
        && u.observed_at > 0
        && u.observed_at <= now() + 60
        && u.resets_at.is_none_or(|t| t >= u.observed_at)
        && matches!(
            u.source.as_str(),
            "claude_oauth" | "codex_oauth" | "response_headers" | "provider"
        )
}

/// Fixed vocabulary prevents callers from accidentally persisting upstream error bodies.
pub(crate) fn event_valid(action: &str, detail: &str) -> bool {
    match action {
        "account_added" | "account_updated" | "account_removed" | "account_selected" => {
            detail == "success"
        }
        "usage" => matches!(detail, "observed" | "unavailable" | "failed"),
        "request" => matches!(
            detail,
            "success" | "unauthorized" | "rate_limited" | "upstream_error" | "network_error"
        ),
        "proxy" => matches!(
            detail,
            "started"
                | "stopped"
                | "completed"
                | "aborted"
                | "connection_failed"
                | "timeout"
                | "rejected"
                | "2xx"
                | "3xx"
                | "4xx"
                | "5xx"
        ),
        "launch" | "login" => matches!(
            detail,
            "started" | "completed" | "failed" | "cancelled" | "isolated" | "managed"
        ),
        _ => false,
    }
}
fn append_event(s: &mut Snapshot, action: &str, id: Option<&str>, detail: &str) {
    s.events.push(Event {
        at: now(),
        action: action.into(),
        account_id: id.map(str::to_owned),
        detail: detail.into(),
    });
    if s.events.len() > MAX_EVENTS {
        s.events.remove(0);
    }
}

pub(crate) fn validate_snapshot(s: &Snapshot) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    if s.accounts.len() > MAX_ACCOUNTS
        || s.events.len() > MAX_EVENTS
        || s.routes.len() > MAX_ACCOUNTS
    {
        return Err("Invalid metadata bounds".into());
    }
    for a in &s.accounts {
        if !uuid_valid(&a.id)
            || !ids.insert(&a.id)
            || !label_valid(&a.label)
            || !pool_valid(&a.pool)
            || a.created_at <= 0
            || (a.provider == Provider::Codex && a.kind == AuthKind::SetupToken)
            || a.identity
                .as_ref()
                .is_some_and(|v| !credential::identity_valid(v))
            || a.usage.as_ref().is_some_and(|u| !usage_valid(u))
        {
            return Err("Invalid account metadata".into());
        }
    }
    for (key, id) in &s.routes {
        if !s.accounts.iter().any(|a| {
            a.id == *id && a.enabled && *key == format!("{}:{}", a.provider.as_str(), a.pool)
        }) {
            return Err("Invalid route metadata".into());
        }
    }
    for e in &s.events {
        if e.at <= 0
            || !event_valid(&e.action, &e.detail)
            || e.account_id.as_ref().is_some_and(|id| !uuid_valid(id))
        {
            return Err("Invalid event metadata".into());
        }
    }
    Ok(())
}

impl Store {
    pub fn open(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        let (lock, state) = persistence::open(&root)?;
        Ok(Self {
            root,
            _lock: lock,
            vault,
            state: Mutex::new(state),
        })
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Snapshot>, String> {
        self.state
            .lock()
            .map_err(|_| "Account store unavailable".into())
    }
    fn publish(&self, state: &mut Snapshot, candidate: Snapshot) -> Result<(), String> {
        validate_snapshot(&candidate)?;
        persistence::write(&self.root, &candidate)?;
        *state = candidate;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        Ok(self.lock()?.clone())
    }
    pub fn add(
        &self,
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        credential: Credential,
    ) -> Result<Account, String> {
        if !label_valid(&label) || !pool_valid(&pool) {
            return Err("Label or pool is invalid".into());
        }
        credential.validate(provider, kind)?;
        let mut state = self.lock()?;
        if state.accounts.len() >= MAX_ACCOUNTS {
            return Err("Account limit reached".into());
        }
        for a in &state.accounts {
            if a.provider == provider && a.pool == pool {
                let existing = self
                    .vault
                    .get(&a.id)
                    .map_err(|_| "Credential storage unavailable")?;
                if existing.access_token == credential.access_token {
                    return Err("Account credential already exists in this pool".into());
                }
            }
        }
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.trim().into(),
            provider,
            kind,
            pool,
            enabled: true,
            created_at: now(),
            identity: credential.account_id.clone(),
            usage: None,
        };
        self.vault
            .put(&account.id, &credential)
            .map_err(|_| "Credential storage unavailable")?;
        let mut candidate = state.clone();
        candidate.accounts.push(account.clone());
        append_event(
            &mut candidate,
            "account_added",
            Some(&account.id),
            "success",
        );
        if self.publish(&mut state, candidate).is_err() {
            self.vault
                .delete(&account.id)
                .map_err(|_| "Storage failure; credential cleanup requires recovery")?;
            return Err("Account metadata could not be saved".into());
        }
        Ok(account)
    }
    pub fn update(&self, id: &str, label: String, enabled: bool) -> Result<(), String> {
        if !label_valid(&label) {
            return Err("Label is invalid".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        a.label = label.trim().into();
        a.enabled = enabled;
        if !enabled {
            candidate.routes.retain(|_, value| value != id);
        }
        append_event(&mut candidate, "account_updated", Some(id), "success");
        self.publish(&mut state, candidate)
    }
    /// Vault deletion precedes metadata publication. On disk failure the visible account
    /// remains, but its missing credential prevents use. Retrying removal is safe.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        if state.routes.values().any(|v| v == id) {
            return Err("Select another account or disable this account before removing it".into());
        }
        if !state.accounts.iter().any(|a| a.id == id) {
            return Err("Account not found".into());
        }
        self.vault
            .delete(id)
            .map_err(|_| "Credential storage unavailable")?;
        let mut candidate = state.clone();
        candidate.accounts.retain(|a| a.id != id);
        append_event(&mut candidate, "account_removed", Some(id), "success");
        self.publish(&mut state, candidate)
    }
    pub fn select(&self, provider: Provider, pool: &str, id: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        let a = state
            .accounts
            .iter()
            .find(|a| a.id == id && a.provider == provider && a.pool == pool && a.enabled)
            .ok_or("Account is unavailable in this provider and pool")?;
        self.checked_credential(a)?;
        let mut candidate = state.clone();
        candidate
            .routes
            .insert(format!("{}:{}", provider.as_str(), pool), id.into());
        append_event(&mut candidate, "account_selected", Some(id), "success");
        self.publish(&mut state, candidate)
    }
    fn checked_credential(&self, a: &Account) -> Result<Credential, String> {
        if !a.enabled {
            return Err("Account is disabled".into());
        }
        let c = self
            .vault
            .get(&a.id)
            .map_err(|_| "Credential storage unavailable; reauthenticate this account")?;
        c.validate(a.provider, a.kind)?;
        if c.expires_at.is_some_and(|t| t <= now()) {
            return Err("Credential expired; reauthenticate this account".into());
        }
        Ok(c)
    }
    pub fn route(&self, provider: Provider, pool: &str) -> Result<(Account, Credential), String> {
        let state = self.lock()?;
        let id = state
            .routes
            .get(&format!("{}:{}", provider.as_str(), pool))
            .ok_or("No account selected for this provider and pool")?;
        let a = state
            .accounts
            .iter()
            .find(|a| a.id == *id && a.provider == provider && a.pool == pool)
            .ok_or("Selected account unavailable")?;
        Ok((a.clone(), self.checked_credential(a)?))
    }
    pub fn credential(&self, id: &str) -> Result<Credential, String> {
        let state = self.lock()?;
        self.checked_credential(
            state
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or("Account not found")?,
        )
    }
    pub fn observe(&self, id: &str, usage: Usage) -> Result<(), String> {
        if !usage_valid(&usage) {
            return Err("Invalid usage observation".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        if a.usage
            .as_ref()
            .is_some_and(|old| old.observed_at > usage.observed_at)
        {
            return Err("Usage observation is older than the stored observation".into());
        }
        a.usage = Some(usage);
        append_event(&mut candidate, "usage", Some(id), "observed");
        self.publish(&mut state, candidate)
    }
    pub fn record(&self, action: &str, id: Option<&str>, detail: &str) -> Result<(), String> {
        if !event_valid(action, detail) || id.is_some_and(|v| !uuid_valid(v)) {
            return Err("Unsupported event".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        append_event(&mut candidate, action, id, detail);
        self.publish(&mut state, candidate)
    }
}
