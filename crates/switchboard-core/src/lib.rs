//! Private account storage. Secret-bearing types deliberately do not implement Debug.
pub mod backup;
mod credential;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod keychain;
#[cfg(target_os = "macos")]
mod keychain_macos;
/// Reading items other programs own, without a dialog (macOS).
#[cfg(target_os = "macos")]
pub mod external_keychain {
    pub use crate::keychain_macos::{
        probe_external, read_external_quietly, ItemProbe, EXTERNAL_REFUSED,
    };
}
mod chains;
pub mod language;
mod persistence;
pub mod private_fs;
mod projects;
mod rotation;
#[cfg(unix)]
pub mod security_cli;
mod vault;
#[cfg(windows)]
pub mod windows;

pub use chains::{provider_agent, workflow_id_valid, Chain, ChainScope, Executor, MAX_EXECUTORS};
pub use credential::Credential;
pub use projects::{pool_from_name, Project, ProjectRule, RuleResolution};
pub use rotation::{RotationDecision, RotationPolicy};
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalIdentity {
    pub account_id: Option<String>,
    pub organization_id: Option<String>,
    pub email: Option<String>,
}
impl ExternalIdentity {
    fn valid(&self) -> bool {
        self.account_id
            .as_ref()
            .is_none_or(|v| credential::identity_valid(v))
            && self
                .organization_id
                .as_ref()
                .is_none_or(|v| credential::identity_valid(v))
            && self
                .email
                .as_ref()
                .is_none_or(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
    }
    fn matches(&self, other: &Self) -> bool {
        self.account_id.is_some()
            && self.account_id == other.account_id
            && self.organization_id == other.organization_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageWindow {
    pub name: String,
    pub used_percent: f64,
    pub resets_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageHealth {
    pub status: String,
    pub checked_at: i64,
    pub next_check_at: i64,
}
impl UsageHealth {
    fn valid(&self) -> bool {
        matches!(self.status.as_str(), "ok" | "failed" | "unavailable")
            && self.checked_at > 0
            && self.next_check_at >= self.checked_at
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    #[serde(default)]
    pub windows: Vec<UsageWindow>,
    pub used_percent: f64,
    pub observed_at: i64,
    pub resets_at: Option<i64>,
    pub source: String,
}
/// Windows named with this prefix measure one metered feature of the account (Codex
/// `additional_rate_limits`, SB-40), not the account itself. They stay in `windows` and in the
/// stored `used_percent`, so an older build reads them conservatively.
pub const FEATURE_WINDOW_PREFIX: &str = "feature_";
/// Refusal for anything that would put a project's account into the ordinary Claude Code.
pub const PROJECT_NOT_NATIVE: &str = "This account belongs to a project. The ordinary Claude Code serves every folder, so a project's accounts are used only by sessions launched from the project's folders.";
/// Quota-check cadence (SB-48): an account whose numbers decide something right now — its pool
/// switches automatically, it serves the pool's managed requests, or the ordinary CLI is signed
/// in to it — is checked every three minutes; any other account every ten, and again just after
/// one of its windows resets.
pub const CHECK_ACTIVE_SECONDS: i64 = 180;
pub const CHECK_IDLE_SECONDS: i64 = 600;
/// How soon after a reported reset an idle account is checked again.
pub const CHECK_AFTER_RESET_SECONDS: i64 = 30;
/// How old an observation may be and still count as current where no rotation policy sets
/// `max_age_seconds`: the idle cadence plus slack for a busy pass.
pub const UNPOLICED_MAX_AGE_SECONDS: i64 = 900;
/// The schedule after a successful quota check observed at `observed`.
pub fn next_quota_check(active: bool, usage: &Usage, observed: i64) -> i64 {
    if active {
        return observed.saturating_add(CHECK_ACTIVE_SECONDS);
    }
    let idle = observed.saturating_add(CHECK_IDLE_SECONDS);
    let windows: Vec<Option<i64>> = if usage.windows.is_empty() {
        vec![usage.resets_at]
    } else {
        usage.windows.iter().map(|w| w.resets_at).collect()
    };
    windows
        .into_iter()
        .flatten()
        .filter(|reset| *reset > observed)
        .map(|reset| reset.saturating_add(CHECK_AFTER_RESET_SECONDS))
        .min()
        .map_or(idle, |after| {
            after.clamp(observed + CHECK_ACTIVE_SECONDS.min(60), idle)
        })
}
impl UsageWindow {
    /// True for a window that limits one feature rather than the whole account.
    pub fn is_feature(&self) -> bool {
        self.name.starts_with(FEATURE_WINDOW_PREFIX)
    }
}
impl Usage {
    /// The account's own capacity: the highest use over every window that is not a feature
    /// limit. `None` when the observation has windows but none of them speaks for the account —
    /// unknown, never zero. An observation stored without windows (before 0.4) is its aggregate.
    pub fn account_used_percent(&self) -> Option<f64> {
        if self.windows.is_empty() {
            return Some(self.used_percent);
        }
        self.windows
            .iter()
            .filter(|w| !w.is_feature())
            .map(|w| w.used_percent)
            .reduce(f64::max)
    }
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
    #[serde(default)]
    pub external_identity: Option<ExternalIdentity>,
    #[serde(default)]
    pub usage_health: Option<UsageHealth>,
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
    #[serde(default)]
    pub policies: Vec<RotationPolicy>,
    pub accounts: Vec<Account>,
    pub routes: BTreeMap<String, String>,
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<ProjectRule>,
    /// Projects reserving pools for their folders (0.6). Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<Project>,
    /// Fallback chains per machine, project and workflow (SB-71). Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<Chain>,
}

/// A process-lifetime exclusive owner of one private metadata directory.
pub struct Store {
    root: PathBuf,
    _lock: File,
    vault: Arc<dyn Vault>,
    state: Mutex<Snapshot>,
    /// Counts changes a backup must capture: credentials, accounts and policies — not quota.
    changes: std::sync::atomic::AtomicU64,
    /// Metadata publications written to disk, for the idle budget (lifecycle LC-08).
    writes: std::sync::atomic::AtomicU64,
    /// Since when memory holds timestamps not yet on disk (0: nothing pending).
    pending_since: std::sync::atomic::AtomicI64,
    /// Accounts the ordinary CLIs are signed in to, as the monitor last saw (SB-48). Memory only.
    in_use: Mutex<Option<std::collections::BTreeSet<String>>>,
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0)
}
/// Future bounds apply to new input only. Stored times ahead of a clock that was set
/// back are superseded by new writes instead of making the store unreadable.
pub(crate) fn ahead(time: i64) -> bool {
    time > now() + 60
}
pub(crate) fn label_valid(s: &str) -> bool {
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
    let mut names = std::collections::HashSet::new();
    u.windows.len() <= 16
        && u.windows.iter().all(|w| {
            !w.name.is_empty()
                && w.name.len() <= 64
                && w.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                && names.insert(&w.name)
                && w.used_percent.is_finite()
                && (0.0..=100.0).contains(&w.used_percent)
                && w.resets_at.is_none_or(|t| t >= u.observed_at)
        })
        && (u.windows.is_empty()
            || u.windows.iter().map(|w| w.used_percent).reduce(f64::max) == Some(u.used_percent))
        && u.used_percent.is_finite()
        && (0.0..=100.0).contains(&u.used_percent)
        && u.observed_at > 0
        && u.resets_at.is_none_or(|t| t >= u.observed_at)
        && matches!(
            u.source.as_str(),
            "claude_oauth" | "codex_oauth" | "response_headers" | "provider"
        )
}

fn merge_windows(old: &Usage, new: &Usage) -> Option<Usage> {
    let mut windows = new.windows.clone();
    let mut observed_at = new.observed_at;
    for w in &old.windows {
        if !windows.iter().any(|n| n.name == w.name)
            && w.resets_at.is_none_or(|t| t >= new.observed_at)
        {
            windows.push(w.clone());
            // The aggregate can establish capacity only as recently as every
            // window it contains. A partial header response does not observe
            // the missing weekly/model quota, even when its reset is ahead.
            observed_at = observed_at.min(old.observed_at);
        }
    }
    let worst = windows
        .iter()
        .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))?;
    let merged = Usage {
        used_percent: worst.used_percent,
        resets_at: worst.resets_at,
        observed_at,
        source: new.source.clone(),
        windows,
    };
    usage_valid(&merged).then_some(merged)
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
        "project_rule" => matches!(detail, "saved" | "paused" | "removed" | "applied"),
        "project" => matches!(detail, "created" | "updated" | "removed"),
        "fallback_chain" => matches!(detail, "set" | "cleared"),
        "fallback" => matches!(detail, "offered" | "failed"),
        "activation" => matches!(detail, "completed" | "failed"),
        "rotation" => matches!(detail, "switched" | "failed"),
        "launch" | "login" => matches!(
            detail,
            "started" | "completed" | "failed" | "cancelled" | "isolated" | "managed"
        ),
        _ => false,
    }
}
/// Two observations report the same quota: everything but when it was observed.
fn same_quota(a: &Usage, b: &Usage) -> bool {
    a.used_percent == b.used_percent
        && a.resets_at == b.resets_at
        && a.source == b.source
        && a.windows.len() == b.windows.len()
        && a.windows.iter().zip(&b.windows).all(|(x, y)| {
            x.name == y.name && x.used_percent == y.used_percent && x.resets_at == y.resets_at
        })
}

pub(crate) fn append_event(s: &mut Snapshot, action: &str, id: Option<&str>, detail: &str) {
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
        || s.policies.len() > MAX_ACCOUNTS
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
            || a.external_identity.as_ref().is_some_and(|i| !i.valid())
            || a.usage_health.as_ref().is_some_and(|h| !h.valid())
        {
            return Err("Invalid account metadata".into());
        }
    }
    if s.policies
        .iter()
        .filter(|p| p.enabled && p.target == "claude_cli")
        .count()
        > 1
    {
        return Err(
            "Disable the existing Claude CLI rotation policy before enabling another pool.".into(),
        );
    }
    let mut policies = std::collections::HashSet::new();
    for p in &s.policies {
        p.validate()?;
        if !policies.insert((p.provider, &p.pool, &p.target)) {
            return Err("Duplicate rotation policy".into());
        }
    }
    projects::validate_rules(s)?;
    projects::validate_projects(s)?;
    chains::validate_chains(s)?;
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

/// Closing the store writes timestamps still held in memory.
impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl Store {
    pub fn open(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        let (lock, state) = persistence::open(&root)?;
        Ok(Self {
            root,
            _lock: lock,
            vault,
            state: Mutex::new(state),
            changes: Default::default(),
            writes: Default::default(),
            pending_since: Default::default(),
            in_use: Default::default(),
        })
    }
    /// The private data folder this store owns.
    pub fn root(&self) -> &std::path::Path {
        &self.root
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
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.pending_since
            .store(0, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    /// Takes a candidate that differs from the current state only in timestamps into memory
    /// without writing it; the next publication or flush carries it to disk (LC-08).
    fn hold(&self, state: &mut Snapshot, candidate: Snapshot) -> Result<(), String> {
        validate_snapshot(&candidate)?;
        *state = candidate;
        let _ = self.pending_since.compare_exchange(
            0,
            now().max(1),
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        );
        Ok(())
    }
    /// The accounts the ordinary CLIs are signed in to now (SB-48). One that became the account
    /// in use since the last call is due on the active cadence from its last check; the change
    /// stays in memory like any other timestamp (lifecycle LC-08). The first call only learns the
    /// set: a start moves no schedule.
    pub fn set_in_use(&self, ids: std::collections::BTreeSet<String>) -> Result<(), String> {
        let added: Vec<String> = {
            let mut set = self
                .in_use
                .lock()
                .map_err(|_| "Account store unavailable")?;
            let added = match set.as_ref() {
                Some(known) if *known == ids => return Ok(()),
                Some(known) => ids.difference(known).cloned().collect(),
                None => Vec::new(),
            };
            *set = Some(ids);
            added
        };
        if added.is_empty() {
            return Ok(());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let mut moved = false;
        for a in candidate
            .accounts
            .iter_mut()
            .filter(|a| added.contains(&a.id))
        {
            if let Some(health) = a.usage_health.as_mut().filter(|h| h.status == "ok") {
                let sooner = health.checked_at.saturating_add(CHECK_ACTIVE_SECONDS);
                if sooner < health.next_check_at {
                    health.next_check_at = sooner;
                    moved = true;
                }
            }
        }
        if moved {
            self.hold(&mut state, candidate)
        } else {
            Ok(())
        }
    }
    /// Metadata publications written to disk since the store opened.
    pub fn metadata_writes(&self) -> u64 {
        self.writes.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Writes timestamps held only in memory, if any.
    pub fn flush(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        if self.pending_since.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            return Ok(());
        }
        let current = state.clone();
        self.publish(&mut state, current)
    }
    /// Writes pending timestamps once they have waited `max_age` seconds; a crash then loses
    /// at most that much quota bookkeeping, never an account, credential or setting.
    pub fn flush_if_older(&self, now: i64, max_age: i64) -> Result<(), String> {
        let since = self.pending_since.load(std::sync::atomic::Ordering::SeqCst);
        if since == 0 || now - since < max_age {
            return Ok(());
        }
        self.flush()
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        Ok(self.lock()?.clone())
    }
    /// A random id kept in this data folder, so backups of different stores in one folder
    /// never prune each other. Created on first use.
    pub fn backup_id(&self) -> Result<String, String> {
        let path = self.root.join("backup-id");
        if let Ok(bytes) = private_fs::read_private(&path, 64) {
            if let Ok(text) = String::from_utf8(bytes) {
                if uuid_valid(text.trim()) {
                    return Ok(text.trim().to_owned());
                }
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        private_fs::private_write(&path, id.as_bytes())?;
        Ok(id)
    }
    /// Increases whenever a backup would differ: a credential, account or policy changed.
    pub fn changes(&self) -> u64 {
        self.changes.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(crate) fn changed(&self) {
        self.changes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    /// Metadata plus every credential that can be read, for a backup. Never serialize this
    /// pair anywhere but into the sealed backup payload.
    pub fn export(&self) -> Result<(Snapshot, BTreeMap<String, Credential>), String> {
        let snapshot = self.snapshot()?;
        let mut credentials = BTreeMap::new();
        for a in &snapshot.accounts {
            if let Ok(credential) = self.vault.get(&a.id) {
                if credential.validate(a.provider, a.kind).is_ok() {
                    credentials.insert(a.id.clone(), credential);
                }
            }
        }
        Ok((snapshot, credentials))
    }
    pub fn add(
        &self,
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        credential: Credential,
    ) -> Result<Account, String> {
        self.save_account(label, provider, kind, pool, credential, None, false)
    }
    pub fn upsert(
        &self,
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        credential: Credential,
        external_identity: Option<ExternalIdentity>,
    ) -> Result<Account, String> {
        self.save_account(
            label,
            provider,
            kind,
            pool,
            credential,
            external_identity,
            true,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn save_account(
        &self,
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        credential: Credential,
        external_identity: Option<ExternalIdentity>,
        upsert: bool,
    ) -> Result<Account, String> {
        if !label_valid(&label)
            || !pool_valid(&pool)
            || external_identity.as_ref().is_some_and(|i| !i.valid())
        {
            return Err("Label, pool or identity is invalid".into());
        }
        credential.validate(provider, kind)?;
        if credential
            .account_id
            .as_ref()
            .zip(
                external_identity
                    .as_ref()
                    .and_then(|i| i.account_id.as_ref()),
            )
            .is_some_and(|(a, b)| a != b)
        {
            return Err("Credential identity does not match account identity".into());
        }
        let mut state = self.lock()?;
        let mut matched = None;
        for a in &state.accounts {
            if a.provider != provider || a.pool != pool {
                continue;
            }
            let both_known = a
                .external_identity
                .as_ref()
                .is_some_and(|i| i.account_id.is_some())
                && external_identity
                    .as_ref()
                    .is_some_and(|i| i.account_id.is_some());
            let identity_match = a
                .external_identity
                .as_ref()
                .zip(external_identity.as_ref())
                .is_some_and(|(a, b)| a.matches(b));
            let token_match = if !upsert || !both_known {
                self.vault
                    .get(&a.id)
                    .map_err(vault::surface("Credential storage unavailable"))?
                    .access_token
                    == credential.access_token
            } else {
                false
            };
            if identity_match || token_match {
                if !upsert {
                    return Err("Account credential already exists in this pool".into());
                }
                if matched.is_some() {
                    return Err("Account identity is ambiguous in this pool".into());
                }
                matched = Some(a.clone());
            }
        }
        let (account, old_credential) = if let Some(mut account) = matched {
            let old = self
                .vault
                .get(&account.id)
                .map_err(vault::surface("Credential storage unavailable"))?;
            account.label = label.trim().into();
            account.kind = kind;
            account.identity = credential.account_id.clone();
            account.external_identity = external_identity.or(account.external_identity);
            // A new credential generation cannot inherit an old generation's quota verdict.
            if old.access_token != credential.access_token
                || old.expires_at != credential.expires_at
            {
                account.usage = None;
                account.usage_health = None;
            }
            (account, Some(old))
        } else {
            if state.accounts.len() >= MAX_ACCOUNTS {
                return Err("Account limit reached".into());
            }
            (
                Account {
                    id: uuid::Uuid::new_v4().to_string(),
                    label: label.trim().into(),
                    provider,
                    kind,
                    pool,
                    enabled: true,
                    created_at: now(),
                    identity: credential.account_id.clone(),
                    usage: None,
                    external_identity,
                    usage_health: None,
                },
                None,
            )
        };
        let mut candidate = state.clone();
        if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == account.id) {
            *a = account.clone();
        } else {
            candidate.accounts.push(account.clone());
        }
        append_event(
            &mut candidate,
            if old_credential.is_some() {
                "account_updated"
            } else {
                "account_added"
            },
            Some(&account.id),
            "success",
        );
        validate_snapshot(&candidate)?;
        self.vault
            .put(&account.id, &credential)
            .map_err(vault::surface("Credential storage unavailable"))?;
        if self.publish(&mut state, candidate).is_err() {
            let restored = if let Some(old) = old_credential {
                self.vault.put(&account.id, &old)
            } else {
                self.vault.delete(&account.id)
            };
            restored.map_err(|_| "Storage failure; credential cleanup requires recovery")?;
            return Err("Account metadata could not be saved".into());
        }
        self.changed();
        Ok(account)
    }
    pub fn match_external(
        &self,
        provider: Provider,
        pool: &str,
        identity: &ExternalIdentity,
    ) -> Result<Option<Account>, String> {
        if !pool_valid(pool) || !identity.valid() {
            return Err("Pool or identity is invalid".into());
        }
        let state = self.lock()?;
        let mut matches = state.accounts.iter().filter(|a| {
            a.provider == provider
                && a.pool == pool
                && a.external_identity
                    .as_ref()
                    .is_some_and(|i| i.matches(identity))
        });
        let found = matches.next().cloned();
        if matches.next().is_some() {
            return Err("Account identity is ambiguous in this pool".into());
        }
        Ok(found)
    }
    /// Backend synchronization only: validates format but permits disabled/expired snapshots.
    pub fn stored_credential(&self, id: &str) -> Result<Credential, String> {
        let state = self.lock()?;
        let a = state
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        let credential = self
            .vault
            .get(id)
            .map_err(vault::surface("Credential storage unavailable"))?;
        credential.validate(a.provider, a.kind)?;
        Ok(credential)
    }
    /// A restore gives an account back the credential it lost (`switchboard uninstall
    /// --keep-data`, a removed Keychain item): only when none can be read for it now, so a
    /// working — possibly newer — credential is never replaced by an older backup.
    pub fn restore_missing_credential(
        &self,
        id: &str,
        credential: &Credential,
    ) -> Result<bool, String> {
        let state = self.lock()?;
        let a = state
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        if self
            .vault
            .get(id)
            .is_ok_and(|c| c.validate(a.provider, a.kind).is_ok())
        {
            return Ok(false);
        }
        credential.validate(a.provider, a.kind)?;
        self.vault
            .put(id, credential)
            .map_err(vault::surface("Credential storage unavailable"))?;
        drop(state);
        self.changed();
        Ok(true)
    }
    /// A refresh grant consumed `consumed` and returned `refreshed`. Every stored copy of that
    /// lineage — the same identity may sit in several pools — takes the new generation, or its
    /// next use would present a refresh token the provider has already rotated away. Quota
    /// stays: it describes the account, and a refresh does not change who the account is.
    pub fn adopt_refreshed(
        &self,
        provider: Provider,
        consumed: &str,
        refreshed: &Credential,
    ) -> Result<Vec<String>, String> {
        let (updated, _) = self.adopt_refreshed_for(provider, consumed, refreshed, None)?;
        if updated.is_empty() {
            return Err("Credential changed during refresh".into());
        }
        Ok(updated)
    }
    /// Stores a renewal's successor in every account still holding the spent `consumed`
    /// refresh token, or — when the token endpoint named the `owner` — only in those whose
    /// identity is that account. Returns (updated, holders left with the spent token).
    pub fn adopt_refreshed_for(
        &self,
        provider: Provider,
        consumed: &str,
        refreshed: &Credential,
        owner: Option<&str>,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let state = self.lock()?;
        let mut updated = Vec::new();
        let mut others = Vec::new();
        for a in state
            .accounts
            .iter()
            .filter(|a| a.provider == provider && a.kind == AuthKind::OAuth)
        {
            let Ok(stored) = self.vault.get(&a.id) else {
                continue;
            };
            if stored.refresh_token.as_deref() != Some(consumed) {
                continue;
            }
            if owner.is_some_and(|owner| {
                a.external_identity
                    .as_ref()
                    .and_then(|i| i.account_id.as_deref())
                    != Some(owner)
            }) {
                others.push(a.id.clone());
                continue;
            }
            refreshed.validate(a.provider, a.kind)?;
            if refreshed
                .account_id
                .as_ref()
                .zip(
                    a.external_identity
                        .as_ref()
                        .and_then(|i| i.account_id.as_ref()),
                )
                .is_some_and(|(x, y)| x != y)
            {
                return Err("Credential identity does not match account identity".into());
            }
            if self.vault.put(&a.id, refreshed).is_err() {
                // The copies already written stay written; the caller keeps the successor
                // for the rest, which still hold the spent token.
                if !updated.is_empty() {
                    self.changed();
                }
                return Err("Credential storage unavailable".into());
            }
            updated.push(a.id.clone());
        }
        if !updated.is_empty() {
            self.changed();
        }
        Ok((updated, others))
    }
    pub fn usage_health(
        &self,
        id: &str,
        status: &str,
        checked_at: i64,
        next_check_at: i64,
    ) -> Result<(), String> {
        self.usage_health_generation(id, None, status, checked_at, next_check_at)
    }
    /// Late failed probes must not postpone checking a newly captured credential generation.
    pub fn usage_health_credential(
        &self,
        id: &str,
        credential: &Credential,
        status: &str,
        checked_at: i64,
        next_check_at: i64,
    ) -> Result<(), String> {
        self.usage_health_generation(id, Some(credential), status, checked_at, next_check_at)
    }
    fn usage_health_generation(
        &self,
        id: &str,
        expected: Option<&Credential>,
        status: &str,
        checked_at: i64,
        next_check_at: i64,
    ) -> Result<(), String> {
        let health = UsageHealth {
            status: status.into(),
            checked_at,
            next_check_at,
        };
        if !health.valid() || ahead(checked_at) {
            return Err("Invalid usage health".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        if a.usage_health
            .as_ref()
            .is_some_and(|old| old.checked_at > checked_at && !ahead(old.checked_at))
            || a.usage
                .as_ref()
                .is_some_and(|old| old.observed_at > checked_at && !ahead(old.observed_at))
        {
            return Err("Usage health is older than the stored observation".into());
        }
        if let Some(expected) = expected {
            let current = self
                .vault
                .get(id)
                .map_err(vault::surface("Credential storage unavailable"))?;
            if current.access_token != expected.access_token
                || current.expires_at != expected.expires_at
            {
                return Err("Credential changed during usage check".into());
            }
        }
        // The same verdict again only moves its timestamps (lifecycle LC-08).
        let same_status = a
            .usage_health
            .as_ref()
            .is_some_and(|old| old.status == health.status);
        a.usage_health = Some(health);
        if same_status {
            return self.hold(&mut state, candidate);
        }
        self.publish(&mut state, candidate)
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
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
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
            .map_err(vault::surface("Credential storage unavailable"))?;
        let mut candidate = state.clone();
        candidate.accounts.retain(|a| a.id != id);
        candidate.rules.retain(|r| r.account_id != id);
        chains::prune(&mut candidate);
        append_event(&mut candidate, "account_removed", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    pub fn select(&self, provider: Provider, pool: &str, id: &str) -> Result<(), String> {
        self.select_transaction(provider, pool, id, None)
    }
    /// Publishes the managed route and any matching policy's cooldown in one metadata write.
    pub fn select_with_cooldown(
        &self,
        provider: Provider,
        pool: &str,
        id: &str,
        now: i64,
    ) -> Result<(), String> {
        if now <= 0 {
            return Err("Invalid rotation time".into());
        }
        self.select_transaction(provider, pool, id, Some(now))
    }
    fn select_transaction(
        &self,
        provider: Provider,
        pool: &str,
        id: &str,
        switched_at: Option<i64>,
    ) -> Result<(), String> {
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
        if let Some(time) = switched_at {
            if let Some(policy) = candidate
                .policies
                .iter_mut()
                .find(|p| p.provider == provider && p.pool == pool && p.target == "managed")
            {
                if policy
                    .last_switched_at
                    .is_some_and(|last| last > time && !ahead(last))
                {
                    return Err("Rotation time precedes the last switch".into());
                }
                policy.last_switched_at = Some(time);
            }
        }
        append_event(&mut candidate, "account_selected", Some(id), "success");
        self.publish(&mut state, candidate)
    }
    fn checked_credential(&self, a: &Account) -> Result<Credential, String> {
        if !a.enabled {
            return Err("Account is disabled".into());
        }
        let c = self.vault.get(&a.id).map_err(vault::surface(
            "Credential storage unavailable; reauthenticate this account",
        ))?;
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
        self.observe_generation(id, None, usage)
    }
    /// Only publishes quota if the request's captured credential generation is still stored.
    pub fn observe_credential(
        &self,
        id: &str,
        credential: &Credential,
        usage: Usage,
    ) -> Result<(), String> {
        self.observe_generation(id, Some(credential), usage)
    }
    fn observe_generation(
        &self,
        id: &str,
        expected: Option<&Credential>,
        mut usage: Usage,
    ) -> Result<(), String> {
        if !usage_valid(&usage) || ahead(usage.observed_at) {
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
            .is_some_and(|old| old.observed_at > usage.observed_at && !ahead(old.observed_at))
            // Merged historical windows conservatively retain their older age;
            // the latest check still orders incoming observations. A failure
            // must not let a delayed older success erase that later health.
            || a.usage_health.as_ref().is_some_and(|health| {
                health.checked_at > usage.observed_at && !ahead(health.checked_at)
            })
        {
            return Err("Usage observation is older than the stored observation".into());
        }
        if let Some(expected) = expected {
            let current = self
                .vault
                .get(id)
                .map_err(vault::surface("Credential storage unavailable"))?;
            if current.access_token != expected.access_token
                || current.expires_at != expected.expires_at
            {
                return Err("Credential changed during usage check".into());
            }
        }
        // Inference response headers must not keep postponing the independent
        // quota endpoint poll, especially when they contain only one window.
        let next_check_at = if usage.source == "response_headers" {
            a.usage_health
                .as_ref()
                .map(|h| h.next_check_at)
                .unwrap_or(usage.observed_at)
                .max(usage.observed_at)
        } else {
            let in_use = self
                .in_use
                .lock()
                .map(|set| set.as_ref().is_some_and(|set| set.contains(id)))
                .unwrap_or(true);
            let rotates = candidate
                .policies
                .iter()
                .any(|p| p.enabled && p.provider == a.provider && p.pool == a.pool);
            let routed = candidate
                .routes
                .get(&format!("{}:{}", a.provider.as_str(), a.pool))
                .is_some_and(|route| route == id);
            next_quota_check(in_use || rotates || routed, &usage, usage.observed_at)
        };
        let was_ok = a.usage_health.as_ref().is_some_and(|h| h.status == "ok");
        a.usage_health = Some(UsageHealth {
            status: "ok".into(),
            checked_at: usage.observed_at,
            next_check_at,
        });
        let headers = usage.source == "response_headers";
        if headers {
            // Stored quota is cleared whenever the credential generation changes, so any
            // stored windows belong to this generation. Headers carry only 5h/7d.
            if let Some(merged) = a.usage.as_ref().and_then(|old| merge_windows(old, &usage)) {
                usage = merged;
            }
        }
        // A check that found the quota exactly as it was moves only timestamps: kept in memory,
        // no journal entry, no write (lifecycle LC-08).
        let unchanged = was_ok && a.usage.as_ref().is_some_and(|old| same_quota(old, &usage));
        a.usage = Some(usage);
        if unchanged {
            return self.hold(&mut state, candidate);
        }
        // Per-request header observations would evict account history from the ring.
        if !headers {
            append_event(&mut candidate, "usage", Some(id), "observed");
        }
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
