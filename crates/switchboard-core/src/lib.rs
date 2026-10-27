//! Private account storage. Secret-bearing types deliberately do not implement Debug.
pub mod agent_keys;
pub mod backup;
mod credential;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod keychain;
#[cfg(target_os = "macos")]
mod keychain_macos;
pub mod kimi_accounts;
/// Reading items other programs own, without a dialog (macOS).
#[cfg(target_os = "macos")]
pub mod external_keychain {
    pub use crate::keychain_macos::{
        probe_external, read_external_quietly, ItemProbe, EXTERNAL_REFUSED,
    };
}
mod chains;
pub mod language;
pub mod organizations;
mod persistence;
pub mod private_fs;
mod projects;
mod rotation;
#[cfg(target_os = "linux")]
pub mod secret_service;
#[cfg(unix)]
pub mod security_cli;
mod vault;
#[cfg(windows)]
pub mod windows;

pub use chains::{provider_agent, workflow_id_valid, Chain, ChainScope, Executor, MAX_EXECUTORS};
pub use credential::Credential;
pub use organizations::{Attribution, By, Organization};
pub use projects::{pool_from_name, Project, ProjectRule, RuleResolution, FOLDER_NESTED};
pub use rotation::{Fits, RotationDecision, RotationPolicy};
pub use vault::{MemoryVault, NativeVault, Vault};
/// The store's exclusive lock is held by another process: a running, draining or starting
/// owner, or another offline command.
pub const STORE_LOCKED: &str = "Another Switchboard instance owns this account storage";

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
/// Near-limit cadence (SB-106): the account a session is working on right now, in a pool that
/// can switch, is checked every minute from 95 % of its account-wide usage and every 30 seconds
/// (one background pass) from 98 %, so the switch at the limit acts on a fresh figure. The
/// pool's next account is checked every minute meanwhile (`Store::pull_in_next_accounts`). Not
/// for an account whose usage check the provider refused lately: there the provider's wait is
/// the stronger signal, and checking it faster only earns another one.
pub const NEAR_LIMIT_PERCENT: f64 = 95.0;
pub const AT_LIMIT_PERCENT: f64 = 98.0;
pub const CHECK_NEAR_LIMIT_SECONDS: i64 = 60;
pub const CHECK_AT_LIMIT_SECONDS: i64 = 30;
/// A check is stamped when its answer arrives, a few seconds after its pass began; a near-limit
/// schedule is set this much earlier so it falls due on the pass it names, not the one after
/// (a 30-second schedule measured from the answer would otherwise run every minute).
pub const NEAR_LIMIT_SLACK_SECONDS: i64 = 10;
/// The near-limit cadence for the account a session works on, from its account-wide usage:
/// None below 95 % and at 100 % (full until a reset; the switch has happened or nothing is
/// left), and when the usage names no account-wide figure.
pub fn near_limit_check(usage: &Usage, observed: i64) -> Option<i64> {
    let used = usage.account_used_percent()?;
    let delay = if used >= 100.0 {
        return None;
    } else if used >= AT_LIMIT_PERCENT {
        CHECK_AT_LIMIT_SECONDS
    } else if used >= NEAR_LIMIT_PERCENT {
        CHECK_NEAR_LIMIT_SECONDS
    } else {
        return None;
    };
    Some(observed.saturating_add(delay - NEAR_LIMIT_SLACK_SECONDS))
}
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
    /// Keys agents run on, by service (SB-79): metadata only, the value is in the vault.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_keys: Vec<agent_keys::AgentKey>,
    /// Kimi Code subscription accounts (SB-81): metadata only; each credential stays in its own
    /// `KIMI_CODE_HOME`, never in the vault or a backup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kimi_accounts: Vec<kimi_accounts::KimiAccount>,
    /// Organizations accounts may belong to (SB-158). Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub organizations: Vec<organizations::Organization>,
    /// The person's word per account: an organization's id or `personal` (SB-158).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub memberships: BTreeMap<String, String>,
    /// SB-175 (SB-168 D-2): accounts the person marked as used on another machine, with when.
    /// Such an account is disabled here: never renewed, rotated to or switched to.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub elsewhere: BTreeMap<String, i64>,
}

/// A process-lifetime exclusive owner of one private metadata directory.
pub struct Store {
    root: PathBuf,
    _lock: File,
    vault: Arc<CountedVault>,
    state: Mutex<Snapshot>,
    /// H-2 (SB-174, SB-206): held by every writer of an account's credential (a save, a sign-in's
    /// rows, a renewal, a restore, a removal) across its Keychain reads and writes, so they never
    /// interleave; the metadata lock (`state`) is taken under it, never the other way round, and
    /// only for the metadata itself. A snapshot therefore never waits for a Keychain write.
    credential_writes: Mutex<()>,
    /// H-2: told how long each take of the metadata lock waited (`control_slow`'s `lock_ms`).
    lock_observer: std::sync::OnceLock<fn(std::time::Duration)>,
    /// Counts changes a backup must capture: credentials, accounts and policies — not quota.
    changes: std::sync::atomic::AtomicU64,
    /// PRD-01: counts credential writes only (every vault put or delete, kept or refused), so
    /// a cache of what the vault holds is not thrown away by a project save, which `changes`
    /// also counts (about 740 folder joins a day).
    credential_changes: Arc<std::sync::atomic::AtomicU64>,
    /// Metadata publications written to disk, for the idle budget (lifecycle LC-08).
    writes: std::sync::atomic::AtomicU64,
    /// SB-185: the slowest credential read since the runtime last took it (ms), so a vault that
    /// stalls is seen in the log as such and not only as slow operations around it.
    slowest_read: std::sync::atomic::AtomicU64,
    /// Since when memory holds timestamps not yet on disk (0: nothing pending).
    pending_since: std::sync::atomic::AtomicI64,
    /// Accounts the ordinary CLIs are signed in to, as the monitor last saw (SB-48). Memory only.
    in_use: Mutex<Option<std::collections::BTreeSet<String>>>,
    /// Accounts a provider limit holds right now, as the runtime's limit evidence last said
    /// (SB-98). Memory only; the proxy reads it to fail over without asking the runtime.
    limited: Mutex<std::collections::HashSet<String>>,
    /// Managed requests the provider refused for a limit since the runtime last looked (SB-98):
    /// account → when. Memory only; the runtime turns each into limit evidence.
    refused: Mutex<std::collections::HashMap<String, i64>>,
    /// Accounts whose usage check the provider refused lately (SB-106), as the runtime last
    /// said: they keep the ordinary cadence. Memory only.
    paced: Mutex<std::collections::HashSet<String>>,
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
pub(crate) fn uuid_valid(s: &str) -> bool {
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

/// SB-175: the refusal for an account marked as used on another machine.
pub const ELSEWHERE: &str = "This account is in use on another machine. Use one of this machine's accounts, or clear the mark if it moved here.";
/// An account update that names neither a label nor an enabled state.
pub const NOTHING_TO_UPDATE: &str = "Give a new label, an enabled state, or both.";
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
        "project" => matches!(detail, "created" | "updated" | "removed" | "sources"),
        "fallback_chain" => matches!(detail, "set" | "cleared"),
        "fallback" => matches!(detail, "offered" | "failed"),
        "agent_key" => matches!(detail, "saved" | "removed"),
        "kimi_account" => matches!(detail, "added" | "removed"),
        "organization" => matches!(detail, "added" | "updated" | "removed" | "assigned"),
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
    organizations::validate(s)?;
    projects::validate_projects(s)?;
    chains::validate_chains(s)?;
    agent_keys::validate_agent_keys(s)?;
    kimi_accounts::validate_kimi_accounts(s)?;
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

// Refusals of a usage observation, coded for the log by `Store::usage_refusal_code` (PRD-17).
const OBSERVATION_INVALID: &str = "Invalid usage observation";
const OBSERVATION_SUPERSEDED: &str = "Usage observation is older than the stored observation";
const OBSERVATION_CREDENTIAL_CHANGED: &str = "Credential changed during usage check";
/// The store's refusal for an account it does not hold: a definitive answer, never a hiccup.
pub const ACCOUNT_NOT_FOUND: &str = "Account not found";
const STORE_UNAVAILABLE: &str = "Account store unavailable";
const VAULT_UNAVAILABLE: &str = "Credential storage unavailable";

/// The store's own view of its vault: counts every write and removal after it returns, kept
/// or refused (a refused write may still have changed the item), for `credential_changes`.
/// H-2: it also marks each item while a write or removal of it runs, since writes no longer hold
/// the metadata lock: a quota check comparing its credential under that lock sees a write in
/// flight (`Store::same_generation`). The count moves before the mark is cleared.
struct CountedVault {
    inner: Arc<dyn Vault>,
    writes: Arc<std::sync::atomic::AtomicU64>,
    writing: Mutex<std::collections::HashMap<String, usize>>,
}
impl CountedVault {
    fn written(&self, id: &str, write: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if let Ok(mut writing) = self.writing.lock() {
            *writing.entry(id.to_owned()).or_default() += 1;
        }
        // The guard counts the write and then clears the mark when `write` returns and when it
        // unwinds, so a panicking write never leaves its item marked (every later quota check
        // of it refused) and the count has moved before a check can find the item unmarked.
        let _mark = WriteMark { vault: self, id };
        write()
    }
    /// Whether a write or removal of `id` runs now; true when that cannot be known.
    fn is_writing(&self, id: &str) -> bool {
        self.writing.lock().map_or(true, |w| w.contains_key(id))
    }
}
/// One write's mark on `id` in `CountedVault::writing`; dropping it counts the write, then
/// clears the mark.
struct WriteMark<'a> {
    vault: &'a CountedVault,
    id: &'a str,
}
impl Drop for WriteMark<'_> {
    fn drop(&mut self) {
        self.vault
            .writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut writing = match self.vault.writing.lock() {
            Ok(writing) => writing,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(n) = writing.get_mut(self.id) {
            *n -= 1;
            if *n == 0 {
                writing.remove(self.id);
            }
        }
    }
}
impl Vault for CountedVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        self.inner.get(id)
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        self.written(id, || self.inner.put(id, value))
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.written(id, || self.inner.delete(id))
    }
}

/// H-2: how many times a save plans again when the metadata moved under its write.
const SAVE_ATTEMPTS: usize = 4;
/// H-2: a save that planned `SAVE_ATTEMPTS` times and found the account's group changed under
/// each write. Nothing is saved; the old credential is back in place.
const SAVE_CONTENDED: &str = "Account groups kept changing while the account was saved";
/// One `save_account` call, planned against the metadata without touching the vault.
struct SaveRequest<'a> {
    label: &'a str,
    provider: Provider,
    kind: AuthKind,
    pool: &'a str,
    credential: &'a Credential,
    external_identity: Option<&'a ExternalIdentity>,
    upsert: bool,
    /// The id a new row takes, the same in every plan of this save.
    new_id: String,
    created_at: i64,
}
/// What a save does against one state of the metadata. A plan lives for one step of one save,
/// so its size does not matter.
#[allow(clippy::large_enum_variant)]
enum SavePlan {
    /// These rows' credentials must be read (outside the metadata lock) before it can say.
    Read(Vec<String>),
    /// The row it writes (a saved one, or a new one with `new_id`), that row's credential before
    /// it, and the metadata to publish.
    Ready {
        account: Account,
        old: Option<Credential>,
        candidate: Snapshot,
    },
}
impl SaveRequest<'_> {
    /// The same decisions `save_account` made under one lock before H-2, from `reads` (each
    /// row's credential, read under the credential lock) instead of the vault.
    fn plan(
        &self,
        state: &Snapshot,
        reads: &BTreeMap<String, Result<Credential, String>>,
    ) -> Result<SavePlan, String> {
        let identity_of = |a: &Account| {
            let both_known = a
                .external_identity
                .as_ref()
                .is_some_and(|i| i.account_id.is_some())
                && self
                    .external_identity
                    .is_some_and(|i| i.account_id.is_some());
            let identity_match = a
                .external_identity
                .as_ref()
                .zip(self.external_identity)
                .is_some_and(|(a, b)| a.matches(b));
            (both_known, identity_match)
        };
        let in_pool = |a: &&Account| a.provider == self.provider && a.pool == self.pool;
        let missing: Vec<String> = state
            .accounts
            .iter()
            .filter(in_pool)
            .filter(|a| {
                let (both_known, identity_match) = identity_of(a);
                (!self.upsert || !both_known || identity_match) && !reads.contains_key(&a.id)
            })
            .map(|a| a.id.clone())
            .collect();
        if !missing.is_empty() {
            return Ok(SavePlan::Read(missing));
        }
        let read = |id: &str| -> Result<Credential, String> {
            match reads.get(id) {
                Some(read) => read.clone(),
                None => Err(VAULT_UNAVAILABLE.into()),
            }
            .map_err(vault::surface("Credential storage unavailable"))
        };
        let mut matched = None;
        for a in state.accounts.iter().filter(in_pool) {
            let (both_known, identity_match) = identity_of(a);
            let token_match = if !self.upsert || !both_known {
                read(&a.id)?.access_token == self.credential.access_token
            } else {
                false
            };
            if identity_match || token_match {
                if !self.upsert {
                    return Err("Account credential already exists in this pool".into());
                }
                if matched.is_some() {
                    return Err("Account identity is ambiguous in this pool".into());
                }
                matched = Some(a.clone());
            }
        }
        let (account, old) = if let Some(mut account) = matched {
            let old = read(&account.id)?;
            account.label = self.label.into();
            account.kind = self.kind;
            account.identity = self.credential.account_id.clone();
            account.external_identity = self
                .external_identity
                .cloned()
                .or(account.external_identity);
            // A new credential generation cannot inherit an old generation's quota verdict.
            if old.access_token != self.credential.access_token
                || old.expires_at != self.credential.expires_at
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
                    id: self.new_id.clone(),
                    label: self.label.into(),
                    provider: self.provider,
                    kind: self.kind,
                    pool: self.pool.into(),
                    enabled: true,
                    created_at: self.created_at,
                    identity: self.credential.account_id.clone(),
                    usage: None,
                    external_identity: self.external_identity.cloned(),
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
            if old.is_some() {
                "account_updated"
            } else {
                "account_added"
            },
            Some(&account.id),
            "success",
        );
        validate_snapshot(&candidate)?;
        Ok(SavePlan::Ready {
            account,
            old,
            candidate,
        })
    }
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
        let credential_changes: Arc<std::sync::atomic::AtomicU64> = Default::default();
        Ok(Self {
            root,
            _lock: lock,
            vault: Arc::new(CountedVault {
                inner: vault,
                writes: credential_changes.clone(),
                writing: Default::default(),
            }),
            state: Mutex::new(state),
            credential_writes: Mutex::new(()),
            lock_observer: std::sync::OnceLock::new(),
            changes: Default::default(),
            credential_changes,
            writes: Default::default(),
            slowest_read: Default::default(),
            pending_since: Default::default(),
            in_use: Default::default(),
            limited: Default::default(),
            refused: Default::default(),
            paced: Default::default(),
        })
    }
    /// The private data folder this store owns.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Snapshot>, String> {
        let guard = match self.state.try_lock() {
            Ok(guard) => {
                self.lock_waited(std::time::Duration::ZERO);
                guard
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                let started = std::time::Instant::now();
                let guard = self.state.lock().map_err(|_| STORE_UNAVAILABLE)?;
                self.lock_waited(started.elapsed());
                guard
            }
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(STORE_UNAVAILABLE.into()),
        };
        Ok(guard)
    }
    fn lock_waited(&self, waited: std::time::Duration) {
        if let Some(observer) = self.lock_observer.get() {
            observer(waited);
        }
    }
    /// H-2: `observer` is told, on the taking thread, how long each take of the store's metadata
    /// lock waited (zero when it was free). Set once; a second observer is ignored.
    pub fn observe_lock_waits(&self, observer: fn(std::time::Duration)) {
        let _ = self.lock_observer.set(observer);
    }
    /// H-2: the lock every credential writer holds across its Keychain calls (see
    /// `credential_writes`). Taken before the metadata lock, never while holding it.
    fn credential_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.credential_writes
            .lock()
            .map_err(|_| STORE_UNAVAILABLE.into())
    }
    /// Puts `old` back as `id`'s credential, or removes the item when there was none.
    fn restore_credential(&self, id: &str, old: Option<&Credential>) -> Result<(), String> {
        match old {
            Some(old) => self.vault.put(id, old),
            None => self.vault.delete(id),
        }
        .map_err(|_| "Storage failure; credential cleanup requires recovery".into())
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
    /// Replaces the accounts a limit holds (SB-98): the runtime publishes its evidence here.
    /// Refusals the proxy recorded and the runtime has not taken yet stay held (SB-102): a
    /// publication racing a failover never frees the account that was just refused.
    pub fn set_limited(&self, ids: std::collections::HashSet<String>) {
        let pending: Vec<String> = self
            .refused
            .lock()
            .map(|refused| refused.keys().cloned().collect())
            .unwrap_or_default();
        if let Ok(mut held) = self.limited.lock() {
            *held = ids;
            held.extend(pending);
        }
    }
    pub fn limited(&self) -> std::collections::HashSet<String> {
        self.limited
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
    }
    /// Replaces the accounts whose usage check the provider refused lately (SB-106): they
    /// keep the ordinary cadence near their limit, and are not pulled in as a pool's next.
    pub fn set_paced(&self, ids: std::collections::HashSet<String>) {
        if let Ok(mut paced) = self.paced.lock() {
            *paced = ids;
        }
    }
    /// SB-106: while the account a session works on is near its limit (95 %) or held by one,
    /// the pool's next account — the one a switch or a failover would take — is due a minute
    /// after its last check, so the switch acts on a fresh figure. Only pools that can switch:
    /// a managed route, or an enabled policy with the ordinary CLI signed in to one of its
    /// accounts. Chosen from metadata (`next_to_check`), no credential read. In memory, like any
    /// schedule (LC-08). Returns how many schedules moved.
    pub fn pull_in_next_accounts(&self, now: i64) -> Result<usize, String> {
        let in_use = self
            .in_use
            .lock()
            .map(|set| set.clone().unwrap_or_default())
            .unwrap_or_default();
        let paced = self.paced.lock().map(|set| set.clone()).unwrap_or_default();
        let limited = self.limited();
        let near: Vec<(Provider, String, std::collections::HashSet<String>)> = {
            let state = self.lock()?;
            let mut pools: BTreeMap<
                String,
                (Provider, String, std::collections::HashSet<String>, bool),
            > = BTreeMap::new();
            for a in &state.accounts {
                let key = format!("{}:{}", a.provider.as_str(), a.pool);
                let routed = state.routes.get(&key).is_some_and(|route| *route == a.id);
                let rotates = state
                    .policies
                    .iter()
                    .any(|p| p.enabled && p.provider == a.provider && p.pool == a.pool);
                if !(routed || (rotates && in_use.contains(&a.id))) {
                    continue;
                }
                let measured_near = a.usage_health.as_ref().is_some_and(|h| {
                    h.status == "ok" && now.saturating_sub(h.checked_at) <= CHECK_IDLE_SECONDS
                }) && a
                    .usage
                    .as_ref()
                    .and_then(Usage::account_used_percent)
                    .is_some_and(|used| used >= NEAR_LIMIT_PERCENT);
                let entry = pools
                    .entry(key)
                    .or_insert_with(|| (a.provider, a.pool.clone(), Default::default(), false));
                entry.2.insert(a.id.clone());
                entry.3 |= measured_near || limited.contains(&a.id);
            }
            pools
                .into_values()
                .filter(|(_, _, _, near)| *near)
                .map(|(provider, pool, current, _)| (provider, pool, current))
                .collect()
        };
        let mut next = Vec::new();
        for (provider, pool, current) in near {
            let policy = self.failover_policy(provider, &pool)?;
            let mut held = limited.clone();
            held.extend(current);
            // Chosen from the metadata alone: no credential is read for a schedule.
            if let Some(id) = self.next_to_check(&policy, &held, now)? {
                if !paced.contains(&id) {
                    next.push(id);
                }
            }
        }
        if next.is_empty() {
            return Ok(0);
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let mut moved = 0;
        for a in candidate
            .accounts
            .iter_mut()
            .filter(|a| next.contains(&a.id))
        {
            if let Some(health) = a.usage_health.as_mut().filter(|h| h.status == "ok") {
                let sooner = health
                    .checked_at
                    .saturating_add(CHECK_NEAR_LIMIT_SECONDS - NEAR_LIMIT_SLACK_SECONDS);
                if sooner < health.next_check_at {
                    health.next_check_at = sooner;
                    moved += 1;
                }
            }
        }
        if moved > 0 {
            self.hold(&mut state, candidate)?;
        }
        Ok(moved)
    }
    /// The refusals recorded by `failover` since the last call, oldest first; taking them
    /// clears them.
    pub fn take_refusals(&self) -> Vec<(String, i64)> {
        let mut taken: Vec<_> = self
            .refused
            .lock()
            .map(|mut refused| refused.drain().collect())
            .unwrap_or_default();
        taken.sort_by_key(|(_, at)| *at);
        taken
    }
    /// The pool's managed policy's figures, switched on for the purpose — a limit or a dead
    /// credential is an outage, not a preference — or the defaults when it has none.
    fn failover_policy(&self, provider: Provider, pool: &str) -> Result<RotationPolicy, String> {
        let mut policy = self
            .lock()?
            .policies
            .iter()
            .find(|p| p.provider == provider && p.pool == pool && p.target == "managed")
            .cloned()
            .unwrap_or(RotationPolicy {
                provider,
                pool: pool.to_owned(),
                target: "managed".into(),
                enabled: true,
                threshold_percent: 90.0,
                hysteresis_percent: 10.0,
                cooldown_seconds: 0,
                max_age_seconds: 300,
                last_switched_at: None,
            });
        policy.enabled = true;
        policy.last_switched_at = None;
        Ok(policy)
    }
    /// A managed request on `from` was refused for a limit (SB-98, SB-102). `from` is held and
    /// handed to the runtime as evidence; the pool moves to its best account that `fits` the
    /// request — only while the route still names `from`: a request that comes back refused
    /// after another one (or the person) moved the pool takes the route as it now is. Returns
    /// the route the request should go to, or None when nothing usable is left.
    pub fn failover(
        &self,
        provider: Provider,
        pool: &str,
        from: &str,
        fits: Fits,
        now: i64,
    ) -> Result<Option<(Account, Credential)>, String> {
        if let Ok(mut shared) = self.limited.lock() {
            shared.insert(from.to_owned());
        }
        if let Ok(mut refused) = self.refused.lock() {
            refused.insert(from.to_owned(), now);
        }
        self.move_route(provider, pool, Some(from), fits, now)
    }
    /// The pool's selected account cannot serve a request — its credential expired, it was
    /// switched off, or it is gone (SB-101, SB-102): the pool moves to its best account that
    /// `fits`, but nothing is held — an expired token is not a limit. A route another request
    /// already repaired is used as it is.
    pub fn failover_unusable(
        &self,
        provider: Provider,
        pool: &str,
        fits: Fits,
        now: i64,
    ) -> Result<Option<(Account, Credential)>, String> {
        if let Ok(route) = self.route(provider, pool) {
            return Ok(fits.admits(&route.0).then_some(route));
        }
        let from = self
            .lock()?
            .routes
            .get(&format!("{}:{}", provider.as_str(), pool))
            .cloned();
        self.move_route(provider, pool, from.as_deref(), fits, now)
    }
    fn move_route(
        &self,
        provider: Provider,
        pool: &str,
        from: Option<&str>,
        fits: Fits,
        now: i64,
    ) -> Result<Option<(Account, Credential)>, String> {
        let policy = self.failover_policy(provider, pool)?;
        let mut held = self.limited();
        held.extend(from.map(str::to_owned));
        let Some(id) = self.failover_candidate(&policy, &held, fits, now)? else {
            return Ok(None);
        };
        // Compare and move: only a route still on `from` changes.
        if self.select_transaction(provider, pool, &id, Some(now), Some(from))? {
            let _ = self.record("rotation", Some(&id), "switched");
        }
        Ok(self
            .route(provider, pool)
            .ok()
            .filter(|(account, _)| fits.admits(account)))
    }
    /// B-5 fix cycle 1: the ordinary CLI was signed in to `id` at the monitor's last look
    /// (`set_in_use`). Unknown before the first look: false.
    pub fn is_in_use(&self, id: &str) -> bool {
        self.in_use
            .lock()
            .is_ok_and(|set| set.as_ref().is_some_and(|ids| ids.contains(id)))
    }
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
    /// A settings file a backup carries changed outside the store (`spend.json`, SB-72): counts
    /// for the next backup like a change of the store itself.
    pub fn setting_changed(&self) {
        self.changed();
    }
    pub(crate) fn changed(&self) {
        self.changes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    /// PRD-01: increases after every credential written to or removed from the vault through
    /// this store, and only then: a cache of vault reads keyed on it stays valid across project,
    /// policy and quota changes. A write that lands while a reader reads is counted after it
    /// lands, so a reader that compares the count before and after never keeps a stale answer.
    pub fn credential_changes(&self) -> u64 {
        self.credential_changes
            .load(std::sync::atomic::Ordering::SeqCst)
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
        // H-2 (SB-206): every Keychain read and write runs under the credential lock only. The
        // metadata lock is taken to plan (which rows to read, which row the save is), and again
        // to publish the same plan against the metadata as it is then. Nothing but a credential
        // writer changes a credential or adds or removes an account, and they all wait for this
        // lock, so the reads stay true; a group change can still land meanwhile, and then the
        // first write is put back and the save planned again.
        let _writes = self.credential_lock()?;
        let save = SaveRequest {
            label: label.trim(),
            provider,
            kind,
            pool: &pool,
            credential: &credential,
            external_identity: external_identity.as_ref(),
            upsert,
            new_id: uuid::Uuid::new_v4().to_string(),
            created_at: now(),
        };
        let mut reads: BTreeMap<String, Result<Credential, String>> = BTreeMap::new();
        for _ in 0..SAVE_ATTEMPTS {
            let planned = save.plan(&*self.lock()?, &reads)?;
            let (id, old) = match planned {
                SavePlan::Read(ids) => {
                    self.read_into(&mut reads, ids);
                    continue;
                }
                SavePlan::Ready { account, old, .. } => (account.id, old),
            };
            // The credential first: a crash before the metadata leaves the saved row naming its
            // previous credential's item, now holding the new one, or an item no row names.
            self.vault
                .put(&id, &credential)
                .map_err(vault::surface("Credential storage unavailable"))?;
            let mut state = self.lock()?;
            match save.plan(&state, &reads) {
                Ok(SavePlan::Ready {
                    account, candidate, ..
                }) if account.id == id => {
                    let published = self.publish(&mut state, candidate);
                    drop(state);
                    if published.is_err() {
                        self.restore_credential(&id, old.as_ref())?;
                        return Err("Account metadata could not be saved".into());
                    }
                    self.changed();
                    return Ok(account);
                }
                replanned => {
                    drop(state);
                    self.restore_credential(&id, old.as_ref())?;
                    if let SavePlan::Read(ids) = replanned? {
                        self.read_into(&mut reads, ids);
                    }
                }
            }
        }
        Err(SAVE_CONTENDED.into())
    }
    /// Reads each of `ids`' credentials into `reads`, outside the metadata lock.
    fn read_into(
        &self,
        reads: &mut BTreeMap<String, Result<Credential, String>>,
        ids: Vec<String>,
    ) {
        for id in ids {
            let read = self.vault.get(&id);
            reads.insert(id, read);
        }
    }
    /// One sign-in renews every saved row of its login at once (SB-62, SB-78): each row keeps its
    /// pool and, unless `labels` names a new one, its label; all take the new credential and
    /// identity, and the metadata is published once. If any credential or the metadata cannot be
    /// written, every credential already replaced is put back — no row is left on the old
    /// credential while another holds the new one. H-2 (SB-206): the Keychain is read and written
    /// under the credential lock only, the metadata lock taken to check the rows and to publish.
    pub fn renew_rows(
        &self,
        provider: Provider,
        rows: &[(String, Option<String>)],
        credential: &Credential,
        external_identity: &ExternalIdentity,
    ) -> Result<Vec<Account>, String> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        if !external_identity.valid()
            || rows
                .iter()
                .any(|(_, label)| label.as_deref().is_some_and(|l| !label_valid(l)))
        {
            return Err("Label, pool or identity is invalid".into());
        }
        credential.validate(provider, AuthKind::OAuth)?;
        let _writes = self.credential_lock()?;
        // The rows are checked before any Keychain read, as before; their credentials and
        // identities cannot change while the credential lock is held.
        let checked = |state: &Snapshot| -> Result<(), String> {
            for (id, _) in rows {
                let account = state
                    .accounts
                    .iter()
                    .find(|a| &a.id == id && a.provider == provider && a.kind == AuthKind::OAuth)
                    .ok_or("Account not found")?;
                if !account
                    .external_identity
                    .as_ref()
                    .is_some_and(|i| i.matches(external_identity))
                {
                    return Err("Credential identity does not match account identity".into());
                }
            }
            Ok(())
        };
        checked(&*self.lock()?)?;
        let mut olds: Vec<(String, Credential)> = Vec::new();
        for (id, _) in rows {
            let old = self
                .vault
                .get(id)
                .map_err(vault::surface("Credential storage unavailable"))?;
            olds.push((id.clone(), old));
        }
        let plan = |state: &Snapshot| -> Result<(Snapshot, Vec<Account>), String> {
            checked(state)?;
            let mut candidate = state.clone();
            let mut renewed = Vec::new();
            for ((id, label), (_, old)) in rows.iter().zip(&olds) {
                let account = candidate
                    .accounts
                    .iter_mut()
                    .find(|a| &a.id == id)
                    .ok_or("Account not found")?;
                if let Some(label) = label {
                    account.label = label.trim().into();
                }
                account.identity = credential.account_id.clone();
                account.external_identity = Some(external_identity.clone());
                if old.access_token != credential.access_token
                    || old.expires_at != credential.expires_at
                {
                    account.usage = None;
                    account.usage_health = None;
                }
                renewed.push(account.clone());
            }
            for account in &renewed {
                append_event(
                    &mut candidate,
                    "account_updated",
                    Some(&account.id),
                    "success",
                );
            }
            validate_snapshot(&candidate)?;
            Ok((candidate, renewed))
        };
        plan(&*self.lock()?)?;
        let mut written: Vec<&(String, Credential)> = Vec::new();
        let mut failure = None;
        for entry in &olds {
            match self.vault.put(&entry.0, credential) {
                Ok(()) => written.push(entry),
                Err(error) => {
                    failure = Some(vault::surface("Credential storage unavailable")(error));
                    break;
                }
            }
        }
        let mut renewed = Vec::new();
        if failure.is_none() {
            let mut state = self.lock()?;
            match plan(&state) {
                Ok((candidate, rows)) => {
                    if self.publish(&mut state, candidate).is_err() {
                        failure = Some("Account metadata could not be saved".into());
                    } else {
                        renewed = rows;
                    }
                }
                Err(error) => failure = Some(error),
            }
        }
        if let Some(error) = failure {
            for (id, old) in written {
                self.restore_credential(id, Some(old))?;
            }
            return Err(error);
        }
        self.changed();
        Ok(renewed)
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
    /// SB-159: a secret of Switchboard's own (the PassionCode.ai refresh token) in the vault,
    /// under `item` or a new one; returns the item. Never in the metadata or a file.
    pub fn put_secret(&self, item: Option<&str>, value: &str) -> Result<String, String> {
        let item = item.map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
        if !uuid_valid(&item) || value.is_empty() || value.len() > 4096 {
            return Err("Invalid secret".into());
        }
        let credential = Credential {
            access_token: value.to_owned(),
            refresh_token: None,
            id_token: None,
            native_context: None,
            expires_at: None,
            account_id: None,
        };
        self.vault
            .put(&item, &credential)
            .map_err(vault::surface("Credential storage unavailable"))?;
        Ok(item)
    }
    pub fn secret(&self, item: &str) -> Result<String, String> {
        Ok(self
            .vault
            .get(item)
            .map_err(vault::surface("Credential storage unavailable"))?
            .access_token)
    }
    pub fn delete_secret(&self, item: &str) -> Result<(), String> {
        self.vault
            .delete(item)
            .map_err(vault::surface("Credential storage unavailable"))
    }
    /// SB-185: the metadata lock is held only to find the account, never across the vault read.
    /// On macOS a Keychain read can take seconds (each `/usr/bin/security` call is bounded at
    /// 5 s), and the proxy reads a credential for every managed request: under the lock, one slow
    /// read stalled every `snapshot()` (recovery, hand-overs, agents) for 5 to 82 s on
    /// 2026-10-23 and 2026-10-24. A write that lands meanwhile is read whole, old or new, as a
    /// reader waiting on the lock would have read it.
    pub fn stored_credential(&self, id: &str) -> Result<Credential, String> {
        let (provider, kind) = {
            let state = self.lock()?;
            let a = state
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or(ACCOUNT_NOT_FOUND)?;
            (a.provider, a.kind)
        };
        let started = std::time::Instant::now();
        let read = self.vault.get(id);
        self.slowest_read.fetch_max(
            started.elapsed().as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        let credential = read.map_err(vault::surface("Credential storage unavailable"))?;
        credential.validate(provider, kind)?;
        Ok(credential)
    }
    /// The slowest credential read since the last call, in ms, and resets it (SB-185).
    pub fn take_slowest_read(&self) -> u64 {
        self.slowest_read
            .swap(0, std::sync::atomic::Ordering::Relaxed)
    }
    /// A restore gives an account back the credential it lost (`switchboard uninstall
    /// --keep-data`, a removed Keychain item): only when none can be read for it now, so a
    /// working — possibly newer — credential is never replaced by an older backup.
    pub fn restore_missing_credential(
        &self,
        id: &str,
        credential: &Credential,
    ) -> Result<bool, String> {
        // H-2: under the credential lock, the metadata lock only to find the account (a removal
        // waits for the credential lock, so it stays saved).
        let _writes = self.credential_lock()?;
        let (provider, kind) = {
            let state = self.lock()?;
            let a = state
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or("Account not found")?;
            (a.provider, a.kind)
        };
        if self
            .vault
            .get(id)
            .is_ok_and(|c| c.validate(provider, kind).is_ok())
        {
            return Ok(false);
        }
        credential.validate(provider, kind)?;
        self.vault
            .put(id, credential)
            .map_err(vault::surface("Credential storage unavailable"))?;
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
        // H-1 (0.6.83): the account list is read under the lock and every Keychain read runs
        // without it. A Keychain read has no deadline and waits behind every other in the
        // process; with the list locked across all of them, a snapshot (and `recovery`,
        // `sessions`, the window) waited out every saved account's read, up to 91 s on a loaded
        // Mac. H-2 (SB-174): each write ran under the lock too, and a renewal's `store` phase
        // held it up to 69.3 s on 2026-10-27; it now runs under the credential lock only.
        let accounts: Vec<Account> = self
            .lock()?
            .accounts
            .iter()
            .filter(|a| a.provider == provider && a.kind == AuthKind::OAuth)
            .cloned()
            .collect();
        let mut updated = Vec::new();
        let mut others = Vec::new();
        for a in &accounts {
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
            // One write, under the credential lock, after checking that the account is still
            // saved and still holds the spent token: a removal or a sign-in writing this account
            // (both hold the credential lock across their own write) is never interleaved with
            // it, and a sign-in stored since the read keeps its place (SB-179). Only the reads of
            // accounts that hold the spent token are repeated here; the N reads above run
            // without either lock. The metadata lock is taken only for the check (H-2): a
            // renewal stores no metadata, so a crash after the write leaves the saved row
            // naming an item that holds the working successor, and a refused write leaves the
            // spent token there and the successor with the caller (`refresh.rs` `stash`).
            let writes = self.credential_lock()?;
            let saved = self.lock()?.accounts.iter().any(|s| s.id == a.id);
            if !saved
                || self
                    .vault
                    .get(&a.id)
                    .ok()
                    .and_then(|c| c.refresh_token)
                    .as_deref()
                    != Some(consumed)
            {
                continue;
            }
            let written = self.vault.put(&a.id, refreshed);
            drop(writes);
            if written.is_err() {
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
        let read = expected.map(|_| self.generation_read(id));
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
        if let (Some(expected), Some(read)) = (expected, read) {
            self.same_generation(id, expected, read)?;
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
        self.update_fields(id, Some(label), Some(enabled))
    }
    /// Changes the label, the enabled state or both, under one lock: a field left out keeps its
    /// saved value (the CLI's `accounts update --label` or `--enabled` alone, SB-68).
    pub fn update_fields(
        &self,
        id: &str,
        label: Option<String>,
        enabled: Option<bool>,
    ) -> Result<(), String> {
        if label.is_none() && enabled.is_none() {
            return Err(NOTHING_TO_UPDATE.into());
        }
        if label.as_deref().is_some_and(|l| !label_valid(l)) {
            return Err("Label is invalid".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        if let Some(label) = label {
            a.label = label.trim().into();
        }
        let enabled = enabled.unwrap_or(a.enabled);
        a.enabled = enabled;
        if !enabled {
            candidate.routes.retain(|_, value| value != id);
        } else {
            // Enabling an account here says it is used here (SB-175).
            candidate.elsewhere.remove(id);
        }
        append_event(&mut candidate, "account_updated", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// SB-175 (SB-168 D-2): marks an account as used on another machine, or clears the mark. A
    /// marked account is disabled and leaves every selection, so this machine never renews,
    /// rotates to or switches to it and the other machine's sign-in is never spent from here;
    /// clearing the mark enables it again.
    pub fn mark_elsewhere(&self, id: &str, on: bool, now: i64) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or("Account not found")?;
        a.enabled = !on;
        if on {
            candidate.elsewhere.insert(id.to_owned(), now);
            candidate.routes.retain(|_, value| value != id);
        } else {
            candidate.elsewhere.remove(id);
        }
        append_event(&mut candidate, "account_updated", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// Vault deletion precedes metadata publication. On disk failure the visible account
    /// remains, but its missing credential prevents use. Retrying removal is safe.
    /// H-2: it takes the credential lock first, so no renewal or save writes the item it
    /// deletes; the deletion itself still runs under the metadata lock, so a selection cannot
    /// route to the account between the route check and the deletion.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let _writes = self.credential_lock()?;
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
        candidate.memberships.remove(id);
        candidate.elsewhere.remove(id);
        chains::prune(&mut candidate);
        append_event(&mut candidate, "account_removed", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    pub fn select(&self, provider: Provider, pool: &str, id: &str) -> Result<(), String> {
        self.select_transaction(provider, pool, id, None, None)
            .map(|_| ())
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
        self.select_transaction(provider, pool, id, Some(now), None)
            .map(|_| ())
    }
    /// Selects `id`. With `expect`, only while the route still names that account (`None`: no
    /// route); Ok(false) when it no longer does, and nothing changes (SB-102).
    fn select_transaction(
        &self,
        provider: Provider,
        pool: &str,
        id: &str,
        switched_at: Option<i64>,
        expect: Option<Option<&str>>,
    ) -> Result<bool, String> {
        let mut state = self.lock()?;
        if let Some(expected) = expect {
            let route = state
                .routes
                .get(&format!("{}:{}", provider.as_str(), pool))
                .map(String::as_str);
            if route != expected {
                return Ok(false);
            }
        }
        if state.elsewhere.contains_key(id) {
            return Err(ELSEWHERE.into());
        }
        let a = state
            .accounts
            .iter()
            .find(|a| a.id == id && a.provider == provider && a.pool == pool && a.enabled)
            .ok_or("Account is unavailable in this provider and pool")?;
        if !state.context_admits(pool, a) {
            return Err(organizations::SEAT_OUTSIDE.into());
        }
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
        self.publish(&mut state, candidate).map(|()| true)
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
    /// The selected account of a pool, usable or not (SB-102: what a broken route was).
    pub fn selected_account(&self, provider: Provider, pool: &str) -> Option<Account> {
        let state = self.lock().ok()?;
        let id = state
            .routes
            .get(&format!("{}:{}", provider.as_str(), pool))?;
        state
            .accounts
            .iter()
            .find(|a| &a.id == id && a.provider == provider && a.pool == pool)
            .cloned()
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
        if !state.context_admits(pool, a) {
            return Err(organizations::SEAT_OUTSIDE.into());
        }
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
    /// N1 round 1: a quota check's credential, read from the vault before the metadata lock is
    /// taken (a Keychain read can take seconds), with the credential write count it was read at.
    fn generation_read(&self, id: &str) -> (Result<Credential, String>, u64) {
        let at = self.credential_changes();
        (
            self.vault
                .get(id)
                .map_err(vault::surface(VAULT_UNAVAILABLE)),
            at,
        )
    }
    /// Under the metadata lock: whether `expected` is still `id`'s stored credential. A
    /// credential written since `read` was taken is read again (only a write landing in that
    /// window costs a read under the lock).
    fn same_generation(
        &self,
        id: &str,
        expected: &Credential,
        read: (Result<Credential, String>, u64),
    ) -> Result<(), String> {
        let (read, at) = read;
        // H-2: a write of this item in flight (it no longer holds the metadata lock) changes
        // the generation the check read; refused as a change, as it would read once landed.
        if self.vault.is_writing(id) {
            return Err(OBSERVATION_CREDENTIAL_CHANGED.into());
        }
        let current = if self.credential_changes() == at {
            read
        } else {
            self.vault
                .get(id)
                .map_err(vault::surface(VAULT_UNAVAILABLE))
        }?;
        if current.access_token != expected.access_token
            || current.expires_at != expected.expires_at
        {
            return Err(OBSERVATION_CREDENTIAL_CHANGED.into());
        }
        Ok(())
    }
    /// PRD-17: the operations log's code for a refusal `observe_credential` gave a quota check
    /// (its text reaches no log): `superseded` (a newer observation or check is already held),
    /// `credential_changed` (renewed or signed in again while the check ran),
    /// `invalid_observation`, `account_gone`, `vault`, `store`. None: not one of its refusals.
    pub fn usage_refusal_code(message: &str) -> Option<&'static str> {
        Some(match message {
            OBSERVATION_SUPERSEDED => "superseded",
            OBSERVATION_CREDENTIAL_CHANGED => "credential_changed",
            OBSERVATION_INVALID => "invalid_observation",
            ACCOUNT_NOT_FOUND => "account_gone",
            STORE_UNAVAILABLE => "store",
            m if m == VAULT_UNAVAILABLE || keychain::ACTIONABLE.contains(&m) => "vault",
            _ => return None,
        })
    }
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
            return Err(OBSERVATION_INVALID.into());
        }
        let read = expected.map(|_| self.generation_read(id));
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(ACCOUNT_NOT_FOUND)?;
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
            return Err(OBSERVATION_SUPERSEDED.into());
        }
        if let (Some(expected), Some(read)) = (expected, read) {
            self.same_generation(id, expected, read)?;
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
            let next = next_quota_check(in_use || rotates || routed, &usage, usage.observed_at);
            // SB-106: the account a session works on, where a switch can follow, is checked
            // sooner near its limit — unless the provider refused its checks lately.
            let paced = self
                .paced
                .lock()
                .map(|set| set.contains(id))
                .unwrap_or(true);
            match near_limit_check(&usage, usage.observed_at) {
                Some(sooner) if ((in_use && rotates) || routed) && !paced => next.min(sooner),
                _ => next,
            }
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

#[cfg(test)]
mod refusal_code_tests {
    use super::*;

    /// N1 round 3 (planted defect D12): a vault refusal that names its own cause and next step
    /// (`keychain::ACTIONABLE`, a locked or refused Keychain) is still a vault failure in the
    /// operations log, not an unknown refusal.
    #[test]
    fn every_actionable_keychain_refusal_is_coded_as_the_vault() {
        assert_eq!(Store::usage_refusal_code(VAULT_UNAVAILABLE), Some("vault"));
        for message in keychain::ACTIONABLE {
            assert_eq!(
                Store::usage_refusal_code(message),
                Some("vault"),
                "{message}"
            );
        }
    }
}

#[cfg(test)]
mod write_mark_tests {
    use super::*;

    /// H-2 adversarial (P4c): a quota check reads the write count, then whether the item is
    /// marked. If the mark cleared before the count moved, a check could find the item unmarked
    /// with the old count and trust a read of the old credential. The test holds the map of
    /// marks while the write ends: the count must move while the mark is still in place.
    #[test]
    fn the_write_count_moves_before_the_in_flight_mark_clears() {
        let vault = Arc::new(CountedVault {
            inner: Arc::new(MemoryVault::default()),
            writes: Default::default(),
            writing: Default::default(),
        });
        let (in_write, wrote) = std::sync::mpsc::channel();
        let (finish, finished) = std::sync::mpsc::channel::<()>();
        let writer = {
            let vault = vault.clone();
            std::thread::spawn(move || {
                vault.written("synthetic-item", || {
                    let _ = in_write.send(());
                    let _ = finished.recv();
                    Ok(())
                })
            })
        };
        wrote
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the write runs");
        assert!(vault.is_writing("synthetic-item"));
        let before = vault.writes.load(std::sync::atomic::Ordering::SeqCst);
        let marks = vault.writing.lock().unwrap();
        finish.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while vault.writes.load(std::sync::atomic::Ordering::SeqCst) == before {
            assert!(
                std::time::Instant::now() < deadline,
                "the count did not move while the mark was held: it clears the mark first"
            );
            std::thread::yield_now();
        }
        assert!(
            marks.contains_key("synthetic-item"),
            "the mark is still in place"
        );
        drop(marks);
        writer.join().unwrap().unwrap();
        assert!(!vault.is_writing("synthetic-item"));
    }
}
