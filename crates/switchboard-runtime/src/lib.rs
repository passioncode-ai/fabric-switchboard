//! One store/proxy/control owner shared by the desktop and CLI.
pub mod agents;
pub mod control;
pub mod external;
#[cfg(target_os = "macos")]
mod external_keychain;
pub mod launch;
mod monitor;
pub mod projects;
mod refresh;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use switchboard_core::{
    Account, AuthKind, Credential, ExternalIdentity, NativeVault, Provider, RotationPolicy, Store,
    Vault,
};
use switchboard_proxy::ProxyHandle;

pub const APP_ID: &str = "ai.passioncode.fabric-switchboard";
pub fn default_root() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let directory = dirs::data_local_dir();
    #[cfg(not(windows))]
    let directory = dirs::data_dir();
    directory.map(|p| p.join(APP_ID)).ok_or_else(|| {
        "App-data directory unavailable. Use --data-dir with an absolute private directory.".into()
    })
}

/// RFC 3339 UTC text for a stored Unix-seconds time; None outside the supported range.
pub fn rfc3339(seconds: i64) -> Option<String> {
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

/// No credential retrieval operation exists. Never derive Debug on this type.
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Snapshot,
    Status,
    CurrentAccounts,
    CaptureCurrent {
        provider: Provider,
        label: Option<String>,
        pool: String,
    },
    ImportClaudeSwap {
        pool: String,
    },
    ActivateNative {
        id: String,
    },
    SetPolicy {
        policy: RotationPolicy,
    },
    MonitorStatus,
    Add {
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        secret: String,
    },
    Update {
        id: String,
        label: String,
        enabled: bool,
    },
    Remove {
        id: String,
    },
    Select {
        provider: Provider,
        pool: String,
        id: String,
    },
    Usage {
        id: String,
    },
    Launch {
        id: String,
        mode: String,
        working_directory: PathBuf,
    },
    BeginLogin {
        provider: Provider,
        label: String,
        pool: String,
    },
    FinishLogin {
        login_id: String,
    },
    LoginStatus {
        login_id: String,
    },
    CancelLogin {
        login_id: String,
    },
    SetProjectRule {
        path: PathBuf,
        account_id: String,
        target: String,
        enabled: bool,
        expires_at: Option<i64>,
    },
    RemoveProjectRule {
        path: PathBuf,
        provider: Provider,
    },
    ResolveProject {
        path: PathBuf,
    },
    ApplyProject {
        path: PathBuf,
        session: projects::Session,
        global: bool,
    },
}

/// The ordinary CLI sign-in: read by capture/current/rotation, written only by activation.
/// Tests substitute synthetic sources so they never touch real Claude or Codex auth.
#[derive(Clone, Copy)]
pub(crate) struct NativeSources {
    pub(crate) current: fn(Provider) -> Result<external::CapturedProfile, String>,
    pub(crate) activate:
        fn(&Credential, &ExternalIdentity, Option<&ExternalIdentity>) -> Result<(), String>,
}
/// Synthetic owners (tests, the packaged smoke check) never read or write real sign-in.
pub(crate) const UNAVAILABLE: NativeSources = NativeSources {
    current: |_| Err("External sign-in unavailable or its files are unsafe.".into()),
    activate: |_, _, _| {
        Err("Current Claude credential is unavailable; activation was cancelled.".into())
    },
};
pub(crate) const NATIVE: NativeSources = NativeSources {
    current: external::capture_current,
    activate: external::activate_claude,
};

pub struct Runtime {
    pub store: Arc<Store>,
    pub proxy: ProxyHandle,
    pub root: PathBuf,
    logins: Mutex<HashMap<String, launch::Login>>,
    /// Saved sign-ins whose staging home could not be removed yet.
    stale_logins: Mutex<Vec<launch::Login>>,
    mutations: tokio::sync::Mutex<()>,
    current_cache: Mutex<CurrentCache>,
    monitor_decisions: Mutex<Vec<Value>>,
    native: NativeSources,
    refresh: refresh::RefreshState,
}
#[derive(Default)]
struct CurrentCache {
    generation: u64,
    entry: Option<(i64, Value)>,
}
impl Runtime {
    pub async fn open(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Arc<Self>, String> {
        Self::open_with(root, vault, NATIVE).await
    }
    pub(crate) async fn open_with(
        root: PathBuf,
        vault: Arc<dyn Vault>,
        native: NativeSources,
    ) -> Result<Arc<Self>, String> {
        let store = Arc::new(Store::open(root.clone(), vault)?);
        let proxy = ProxyHandle::start(store.clone()).await?;
        Ok(Arc::new(Self {
            store,
            proxy,
            root,
            logins: Mutex::new(HashMap::new()),
            stale_logins: Mutex::new(Vec::new()),
            mutations: tokio::sync::Mutex::new(()),
            current_cache: Mutex::new(CurrentCache::default()),
            monitor_decisions: Mutex::new(Vec::new()),
            native,
            refresh: refresh::RefreshState::default(),
        }))
    }
    pub async fn execute(&self, operation: Operation) -> Result<Value, String> {
        // Metadata reads do not wait behind OS credential prompts or network probes.
        // Store snapshot has its own lock; these operations cannot change auth or homes.
        // Current accounts only read native sources and a cache: a usage probe, import or
        // monitor sync must not push them past the UI read deadline.
        if matches!(
            operation,
            Operation::Snapshot
                | Operation::Status
                | Operation::MonitorStatus
                | Operation::ResolveProject { .. }
                | Operation::CurrentAccounts
                | Operation::LoginStatus { .. }
        ) {
            return execute(self.store.clone(), &self.root, Some(self), operation).await;
        }
        // The Store mutex protects metadata, but launch and removal also mutate
        // private homes. Keep the complete operation in one owner transaction.
        let _mutation = self.mutations.lock().await;
        execute(self.store.clone(), &self.root, Some(self), operation).await
    }
    fn begin_login(
        &self,
        provider: Provider,
        label: String,
        pool: String,
    ) -> Result<Value, String> {
        self.retry_login_cleanup();
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        if logins.len() >= 4 {
            return Err("Finish an existing sign-in before starting another.".into());
        }
        let login = launch::begin_login(&self.root, provider, label, pool)?;
        let id = login.id.clone();
        logins.insert(id.clone(), login);
        Ok(
            json!({"login_id": id, "message": "Finish official sign-in in Terminal, then finish sign-in in Switchboard."}),
        )
    }
    fn finish_login(&self, id: &str) -> Result<Account, String> {
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        let login = logins
            .get_mut(id)
            .ok_or("Sign-in not found. Start again.")?;
        let account = if let Some(account) = &login.saved {
            account.clone()
        } else {
            let captured = launch::capture_login_profile(login)?;
            let label = if login.label.trim().is_empty() {
                captured.label.clone()
            } else {
                login.label.clone()
            };
            let account = self.store.upsert(
                label,
                login.provider,
                AuthKind::OAuth,
                login.pool.clone(),
                captured.credential,
                Some(captured.identity),
            )?;
            login.saved = Some(account.clone());
            account
        };
        let cleaned = launch::clean_login(login);
        // The account is saved: the slot is released even when staging cleanup fails.
        if let Some(login) = logins.remove(id) {
            if cleaned.is_err() {
                if let Ok(mut stale) = self.stale_logins.lock() {
                    stale.push(login);
                }
            }
        }
        self.invalidate_current();
        cleaned.map(|()| account)
    }
    fn login_status(&self, id: &str) -> Result<Value, String> {
        let logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        let login = logins.get(id).ok_or("Sign-in not found. Start again.")?;
        Ok(json!({"state": launch::login_state(login)}))
    }
    fn retry_login_cleanup(&self) {
        if let Ok(mut stale) = self.stale_logins.lock() {
            stale.retain(|login| launch::clean_login(login).is_err());
        }
    }
    fn cancel_login(&self, id: &str) -> Result<(), String> {
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        launch::cancel_login(logins.get(id).ok_or("Sign-in not found. Start again.")?)?;
        logins.remove(id);
        Ok(())
    }
    fn invalidate_current(&self) {
        if let Ok(mut cache) = self.current_cache.lock() {
            cache.generation = cache.generation.wrapping_add(1);
            cache.entry = None;
        }
    }
    fn current_accounts(&self) -> Result<Value, String> {
        let now = monitor::now();
        let generation = {
            let cache = self
                .current_cache
                .lock()
                .map_err(|_| "Current account unavailable.")?;
            if let Some((at, value)) = cache.entry.as_ref() {
                if now >= *at && now - at < 30 {
                    return Ok(value.clone());
                }
            }
            cache.generation
        };
        // Reading may wait on an OS credential prompt; invalidation must not wait for it.
        let value = observe_current(&self.store, self.native.current);
        let mut cache = self
            .current_cache
            .lock()
            .map_err(|_| "Current account unavailable.")?;
        // A capture, import or activation during the read makes this value stale.
        if cache.generation == generation {
            cache.entry = Some((now, value.clone()));
        }
        Ok(value)
    }
}

/// Holding this value keeps both listeners and the exclusive store lease alive.
pub struct Owner {
    pub runtime: Arc<Runtime>,
    _control: control::ControlHandle,
    _monitor: monitor::MonitorHandle,
}
impl Owner {
    pub async fn start(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        Self::start_with_sources(root, vault, false).await
    }
    async fn start_with_sources(
        root: PathBuf,
        vault: Arc<dyn Vault>,
        native_sources: bool,
    ) -> Result<Self, String> {
        let sources = if native_sources { NATIVE } else { UNAVAILABLE };
        let runtime = Runtime::open_with(root, vault, sources).await?;
        let control = control::ControlHandle::start(runtime.clone()).await?;
        let monitor = monitor::MonitorHandle::start(&runtime, native_sources);
        Ok(Self {
            runtime,
            _control: control,
            _monitor: monitor,
        })
    }
    /// `switchboard serve`: native sources, and a vault that never shows a Keychain dialog.
    pub async fn native(root: PathBuf) -> Result<Self, String> {
        Self::start_with_sources(root, Arc::new(NativeVault::new()), true).await
    }
    /// The desktop app: as `native`, and the vault may ask once per item saved by an
    /// earlier version while moving it to shared storage (docs/KEYCHAIN.md).
    pub async fn desktop(root: PathBuf) -> Result<Self, String> {
        Self::start_with_sources(root, Arc::new(NativeVault::desktop()), true).await
    }
}

pub async fn execute_offline(root: PathBuf, operation: Operation) -> Result<Value, String> {
    let store = Arc::new(Store::open(root.clone(), Arc::new(NativeVault::new()))?);
    execute(store, &root, None, operation).await
}
async fn execute(
    store: Arc<Store>,
    root: &Path,
    runtime: Option<&Runtime>,
    operation: Operation,
) -> Result<Value, String> {
    let native = runtime.map_or(NATIVE, |r| r.native);
    // The offline CLI owns the store for one command; its refresh bookkeeping is its own.
    let offline_refresh = refresh::RefreshState::default();
    let refresh_state = runtime.map_or(&offline_refresh, |r| &r.refresh);
    let needs_owner = || {
        runtime.ok_or_else(|| {
            "Start the desktop app or 'switchboard serve' before login or launch.".to_string()
        })
    };
    match operation {
        Operation::Snapshot => {
            serde_json::to_value(store.snapshot()?).map_err(|_| "Snapshot unavailable.".into())
        }
        Operation::Status => Ok(
            json!({"proxy_address": runtime.map(|r| r.proxy.address().to_string()), "platform": std::env::consts::OS, "live_mode": if runtime.is_some() { "Next request · HTTP/SSE" } else { "Offline" }}),
        ),
        Operation::CurrentAccounts => match runtime {
            Some(runtime) => runtime.current_accounts(),
            None => Ok(observe_current(&store, native.current)),
        },
        Operation::CaptureCurrent {
            provider,
            label,
            pool,
        } => {
            let captured = (native.current)(provider)?;
            let account = store.upsert(
                label.unwrap_or(captured.label),
                provider,
                captured.kind,
                pool,
                captured.credential,
                Some(captured.identity),
            )?;
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(json!(account))
        }
        Operation::ImportClaudeSwap { pool } => {
            // Validate before reading external material, even for an empty import.
            if pool.is_empty()
                || pool.len() > 32
                || !pool
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
            {
                return Err("Label or pool is invalid".into());
            }
            let batch = external::read_claude_swap()?;
            let mut imported = Vec::new();
            let mut failed = batch.failed;
            for profile in batch.profiles {
                match store.upsert(
                    profile.label,
                    profile.provider,
                    profile.kind,
                    pool.clone(),
                    profile.credential,
                    Some(profile.identity),
                ) {
                    Ok(account) => imported.push(account),
                    Err(_) => failed += 1,
                }
            }
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(json!({"imported": imported, "failed": failed, "skipped": batch.skipped}))
        }
        Operation::ActivateNative { id } => {
            // The target is inactive by definition; an expired token is renewed first so
            // Claude Code starts on a live one. A failed refresh still activates: Claude
            // Code renews from the refresh token itself.
            if refresh::ensure_fresh(&store, native, refresh_state, &id, false).await
                == refresh::Outcome::SignInRequired
            {
                return Err(refresh::SIGN_IN.into());
            }
            activate_native(&store, &id, None, native, Some(refresh_state))?;
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(Value::Null)
        }
        Operation::SetPolicy { policy } => {
            store.set_policy(policy)?;
            Ok(Value::Null)
        }
        Operation::MonitorStatus => Ok(json!({
            "running": runtime.is_some(), "interval_seconds": monitor::INTERVAL_SECONDS,
            "decisions": runtime.and_then(|r| r.monitor_decisions.lock().ok().map(|v| v.clone())).unwrap_or_default(),
            "sign_in_required": refresh_state.sign_in_required(&store),
        })),
        Operation::Add {
            label,
            provider,
            kind,
            pool,
            secret,
        } => {
            let credential = Credential::parse(provider, kind, &secret)?;
            Ok(json!(store.add(label, provider, kind, pool, credential)?))
        }
        Operation::Update { id, label, enabled } => {
            store.update(&id, label, enabled)?;
            Ok(Value::Null)
        }
        Operation::Remove { id } => {
            if store.snapshot()?.routes.values().any(|v| v == &id) {
                return Err("Select another account before removing this one.".into());
            }
            launch::clean_account(root, &id)?;
            store.remove(&id)?;
            Ok(Value::Null)
        }
        Operation::Select { provider, pool, id } => {
            store.select_with_cooldown(provider, &pool, &id, monitor::now())?;
            Ok(Value::Null)
        }
        // The owner transaction is already held here, so the refresh takes no second lock.
        Operation::Usage { id } => Ok(json!(
            monitor::check(&store, native, refresh_state, &id, None).await?
        )),
        Operation::Launch {
            id,
            mode,
            working_directory,
        } => {
            let runtime = needs_owner()?;
            if mode == "isolated" {
                refresh::ensure_fresh(&store, native, refresh_state, &id, false).await;
            }
            let agent_tools =
                launch::launch(root, &store, &runtime.proxy, &id, &mode, &working_directory)?;
            Ok(
                json!({"message": "Terminal launch requested. Provider response is not yet verified.", "agent_tools": agent_tools}),
            )
        }
        Operation::BeginLogin {
            provider,
            label,
            pool,
        } => needs_owner()?.begin_login(provider, label, pool),
        Operation::FinishLogin { login_id } => Ok(json!(needs_owner()?.finish_login(&login_id)?)),
        Operation::LoginStatus { login_id } => needs_owner()?.login_status(&login_id),
        Operation::CancelLogin { login_id } => {
            needs_owner()?.cancel_login(&login_id)?;
            Ok(Value::Null)
        }
        Operation::SetProjectRule {
            path,
            account_id,
            target,
            enabled,
            expires_at,
        } => {
            let path = projects::project_dir(root, &path)?;
            let rule = store.set_rule(&path, &account_id, &target, enabled, expires_at)?;
            Ok(projects::rule_view(&store, &rule, monitor::now()))
        }
        Operation::RemoveProjectRule { path, provider } => {
            let path = projects::rule_path(root, &path)?;
            let rule = store.remove_rule(&path, provider)?;
            Ok(projects::rule_view(&store, &rule, monitor::now()))
        }
        Operation::ResolveProject { path } => {
            projects::resolve(&store, &projects::project_dir(root, &path)?, monitor::now())
        }
        Operation::ApplyProject {
            path,
            session,
            global,
        } => projects::apply(
            &store,
            runtime,
            &projects::project_dir(root, &path)?,
            &session,
            global,
        ),
    }
}

fn observe_current(
    store: &Store,
    current: fn(Provider) -> Result<external::CapturedProfile, String>,
) -> Value {
    let mut output = serde_json::Map::new();
    for provider in [Provider::Claude, Provider::Codex] {
        let value = match current(provider).and_then(|profile| {
            let matches = matching_accounts(store, provider, &profile.identity)?;
            Ok((profile, matches))
        }) {
            Ok((profile, matches)) => {
                json!({"status":"available", "identity":profile.identity, "account_id":matches.first().map(|a| &a.id), "account_ids":matches.iter().map(|a| &a.id).collect::<Vec<_>>()})
            }
            Err(error) => {
                json!({"status": if error.starts_with("No current ") { "missing" } else { "unavailable" }, "identity":null, "account_id":null, "account_ids":[]})
            }
        };
        output.insert(provider.as_str().into(), value);
    }
    Value::Object(output)
}

/// Every pool can hold the signed-in identity; the oldest profile is reported first.
fn matching_accounts(
    store: &Store,
    provider: Provider,
    identity: &ExternalIdentity,
) -> Result<Vec<Account>, String> {
    let accounts: Vec<Account> = store
        .snapshot()?
        .accounts
        .into_iter()
        .filter(|a| a.provider == provider)
        .collect();
    let mut pools: Vec<&str> = accounts.iter().map(|a| a.pool.as_str()).collect();
    pools.sort_unstable();
    pools.dedup();
    let mut ids = std::collections::HashSet::new();
    for pool in pools {
        ids.extend(
            store
                .match_external(provider, pool, identity)?
                .map(|a| a.id),
        );
    }
    // Stable sort keeps insertion order for profiles saved within the same second.
    let mut matches: Vec<Account> = accounts
        .into_iter()
        .filter(|a| ids.contains(&a.id))
        .collect();
    matches.sort_by_key(|a| a.created_at);
    Ok(matches)
}

fn activate_native(
    store: &Store,
    id: &str,
    expected_id: Option<&str>,
    native: NativeSources,
    refresh: Option<&refresh::RefreshState>,
) -> Result<(), String> {
    // A lineage the provider rejected would sign every ordinary `claude` session out.
    if refresh.is_some_and(|state| state.is_dead(store, id)) {
        return Err(refresh::SIGN_IN.into());
    }
    let account = store
        .snapshot()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    if account.provider != Provider::Claude || account.kind != AuthKind::OAuth {
        return Err("Native activation requires a Claude OAuth profile.".into());
    }
    let identity = account
        .external_identity
        .as_ref()
        .ok_or("Capture or import this profile before native activation.")?;
    let result = replace_native(store, &account, identity, expected_id, native, refresh);
    // The outcome is already decided; a journal failure must not change it.
    let _ = store.record(
        "activation",
        Some(id),
        if result.is_ok() {
            "completed"
        } else {
            "failed"
        },
    );
    result
}
fn replace_native(
    store: &Store,
    account: &Account,
    identity: &ExternalIdentity,
    expected_id: Option<&str>,
    native: NativeSources,
    refresh: Option<&refresh::RefreshState>,
) -> Result<(), String> {
    if !account.enabled {
        return Err("Account is disabled".into());
    }
    // An expired access token is fine while a refresh token remains: Claude Code renews it
    // on first use, exactly as after its own idle expiry (PLAN-0.5 C-5).
    let credential = store.stored_credential(&account.id)?;
    if credential.refresh_token.is_none()
        && credential.expires_at.is_some_and(|t| t <= monitor::now())
    {
        return Err("Credential expired; reauthenticate this account".into());
    }
    // After `claude logout` there is no current identity; the adapter handles that state.
    let current = match (native.current)(Provider::Claude) {
        Ok(current) => Some(current),
        Err(error) if error.starts_with("No current ") => None,
        Err(error) => return Err(error),
    };
    let previous = match &current {
        Some(current) => {
            store.match_external(Provider::Claude, &account.pool, &current.identity)?
        }
        None => None,
    };
    if expected_id.is_some_and(|expected| previous.as_ref().is_none_or(|a| a.id != expected)) {
        return Err("Current CLI account changed. Refresh before switching.".into());
    }
    let expected = current.as_ref().map(|c| c.identity.clone());
    // The account being switched away from stays "recently active" for renewal purposes.
    if let (Some(state), Some(current)) = (refresh, current.as_ref()) {
        state.note_active(&current.identity);
    }
    if let Some(current) = current {
        // Preserve the live client's newest refresh generation in every stored copy of
        // that identity, in every pool: a copy left behind would hold a refresh token
        // Claude Code has already rotated away (PLAN-0.5 C-6).
        for copy in matching_accounts(store, Provider::Claude, &current.identity)? {
            store.upsert(
                copy.label,
                copy.provider,
                copy.kind,
                copy.pool,
                current.credential.clone(),
                Some(current.identity.clone()),
            )?;
        }
    }
    (native.activate)(&credential, identity, expected.as_ref())?;
    if store.snapshot()?.policies.iter().any(|p| {
        p.provider == Provider::Claude && p.pool == account.pool && p.target == "claude_cli"
    }) {
        store.mark_rotated(
            Provider::Claude,
            &account.pool,
            "claude_cli",
            monitor::now(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use switchboard_core::MemoryVault;
    const EXPIRES: i64 = 4_000_000_000;
    pub(crate) fn identity(account: &str) -> ExternalIdentity {
        ExternalIdentity {
            account_id: Some(account.into()),
            organization_id: Some("synthetic-org".into()),
            email: Some(format!("{account}@example.invalid")),
        }
    }
    pub(crate) fn credential(account: &str) -> Credential {
        let mut credential = Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &json!({"claudeAiOauth":{"accessToken":format!("{account}-token"),"refreshToken":"synthetic-refresh","expiresAt":EXPIRES*1000}}).to_string(),
        )
        .unwrap();
        credential.native_context = Some(
            json!({"auth":{"claudeAiOauth":{"accessToken":format!("{account}-token")}},"oauth_account":{"accountUuid":account}}),
        );
        credential
    }
    pub(crate) fn save(store: &Store, account: &str, pool: &str) -> Account {
        store
            .upsert(
                account.into(),
                Provider::Claude,
                AuthKind::OAuth,
                pool.into(),
                credential(account),
                Some(identity(account)),
            )
            .unwrap()
    }
    /// The ordinary Claude CLI is signed in as `synthetic-a`; Codex is signed out.
    pub(crate) fn signed_in(provider: Provider) -> Result<external::CapturedProfile, String> {
        match provider {
            Provider::Claude => Ok(external::CapturedProfile {
                provider,
                kind: AuthKind::OAuth,
                credential: credential("synthetic-a"),
                identity: identity("synthetic-a"),
                label: "synthetic-a".into(),
            }),
            Provider::Codex => Err("No current Codex sign-in found.".into()),
        }
    }
    pub(crate) fn signed_out(provider: Provider) -> Result<external::CapturedProfile, String> {
        Err(format!(
            "No current {} sign-in found.",
            if provider == Provider::Claude {
                "Claude"
            } else {
                "Codex"
            }
        ))
    }
    pub(crate) fn unreadable(_: Provider) -> Result<external::CapturedProfile, String> {
        Err("External sign-in unavailable or its files are unsafe.".into())
    }
    pub(crate) fn activates(
        _: &Credential,
        _: &ExternalIdentity,
        _: Option<&ExternalIdentity>,
    ) -> Result<(), String> {
        Ok(())
    }
    pub(crate) fn fails(
        _: &Credential,
        _: &ExternalIdentity,
        _: Option<&ExternalIdentity>,
    ) -> Result<(), String> {
        Err("Claude activation failed; previous account restored.".into())
    }
    pub(crate) async fn runtime(
        root: &Path,
        current: fn(Provider) -> Result<external::CapturedProfile, String>,
        activate: fn(
            &Credential,
            &ExternalIdentity,
            Option<&ExternalIdentity>,
        ) -> Result<(), String>,
    ) -> Arc<Runtime> {
        Runtime::open_with(
            root.to_owned(),
            Arc::new(MemoryVault::default()),
            NativeSources { current, activate },
        )
        .await
        .unwrap()
    }
    pub(crate) fn events(store: &Store, action: &str) -> Vec<(Option<String>, String)> {
        store
            .snapshot()
            .unwrap()
            .events
            .into_iter()
            .filter(|e| e.action == action)
            .map(|e| (e.account_id, e.detail))
            .collect()
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use fixtures::*;
    use switchboard_core::{private_fs, MemoryVault};
    #[test]
    fn rfc3339_formats_utc_seconds() {
        assert_eq!(
            rfc3339(1_700_000_000).as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert_eq!(rfc3339(i64::MAX), None);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_login_cleanup_releases_the_slot_after_the_account_is_saved() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let account = save(&runtime.store, "synthetic-a", "default");
        let parent = root.path().join("staging");
        let home = parent.join("login");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("auth.json"), "synthetic-test-file").unwrap();
        let login = launch::fixture_login(home.clone(), account);
        let id = login.id.clone();
        runtime.logins.lock().unwrap().insert(id.clone(), login);
        // The staging home cannot be removed from a read-only parent.
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = runtime
            .execute(Operation::FinishLogin {
                login_id: id.clone(),
            })
            .await;
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "Account saved; isolated login cleanup needs attention."
        );
        assert!(runtime.logins.lock().unwrap().is_empty());
        assert!(home.exists());
        // Cleanup is retried before the next sign-in starts.
        runtime.retry_login_cleanup();
        assert!(!home.exists());
        assert!(runtime.stale_logins.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn current_accounts_do_not_wait_behind_a_mutation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let _reservation = runtime.mutations.lock().await;
        let value = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.execute(Operation::CurrentAccounts),
        )
        .await
        .expect("current accounts waited for the mutation lock")
        .unwrap();
        assert_eq!(value["claude"]["status"], "missing");
    }
    #[test]
    fn current_account_matches_across_pools() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        save(&store, "synthetic-b", "default");
        let work = save(&store, "synthetic-a", "work");
        let value = observe_current(&store, signed_in);
        assert_eq!(value["claude"]["status"], "available");
        assert_eq!(value["claude"]["account_id"], work.id.as_str());
        assert_eq!(value["claude"]["account_ids"], json!([work.id]));
        let team = save(&store, "synthetic-a", "team");
        let value = observe_current(&store, signed_in);
        assert_eq!(value["claude"]["account_id"], work.id.as_str());
        assert_eq!(value["claude"]["account_ids"], json!([work.id, team.id]));
        assert_eq!(value["codex"]["status"], "missing");
        assert_eq!(value["codex"]["account_ids"], json!([]));
    }
    #[tokio::test]
    async fn activation_after_claude_logout_proceeds_and_is_journaled() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let b = save(&runtime.store, "synthetic-b", "default");
        runtime
            .execute(Operation::ActivateNative { id: b.id.clone() })
            .await
            .unwrap();
        assert_eq!(
            events(&runtime.store, "activation"),
            [(Some(b.id), "completed".to_string())]
        );
    }
    #[tokio::test]
    async fn failed_or_unreadable_activation_is_journaled_as_failed() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, fails).await;
        save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        assert!(runtime
            .execute(Operation::ActivateNative { id: b.id.clone() })
            .await
            .is_err());
        assert_eq!(
            events(&runtime.store, "activation"),
            [(Some(b.id.clone()), "failed".to_string())]
        );
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), unreadable, activates).await;
        let b = save(&runtime.store, "synthetic-b", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: b.id.clone() })
                .await
                .unwrap_err(),
            "External sign-in unavailable or its files are unsafe."
        );
        assert_eq!(
            events(&runtime.store, "activation"),
            [(Some(b.id), "failed".to_string())]
        );
    }

    /// Captures `synthetic-a` as a stored copy whose token generation is older than the live one.
    fn stale_copy(store: &Store, pool: &str) -> Account {
        let mut old = credential("synthetic-a");
        old.access_token = "synthetic-a-old-token".into();
        old.refresh_token = Some("synthetic-a-old-refresh".into());
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                pool.into(),
                old,
                Some(identity("synthetic-a")),
            )
            .unwrap()
    }
    #[tokio::test]
    async fn switching_away_keeps_the_live_generation_in_every_pool() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let home = stale_copy(&runtime.store, "default");
        let work = stale_copy(&runtime.store, "work");
        let b = save(&runtime.store, "synthetic-b", "default");
        runtime
            .execute(Operation::ActivateNative { id: b.id.clone() })
            .await
            .unwrap();
        for copy in [home, work] {
            let stored = runtime.store.stored_credential(&copy.id).unwrap();
            assert_eq!(
                stored.access_token, "synthetic-a-token",
                "pool {}",
                copy.pool
            );
            assert_eq!(stored.refresh_token.as_deref(), Some("synthetic-refresh"));
        }
    }
    #[tokio::test]
    async fn an_account_with_an_expired_access_token_still_activates() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        save(&runtime.store, "synthetic-a", "default");
        let mut expired = credential("synthetic-b");
        expired.expires_at = Some(1_000);
        let b = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired.clone(),
                Some(identity("synthetic-b")),
            )
            .unwrap();
        // The refresh endpoint is unreachable in tests; Claude Code renews the token itself.
        runtime
            .execute(Operation::ActivateNative { id: b.id.clone() })
            .await
            .unwrap();
        // Without a refresh token nothing could renew it: refused.
        expired.refresh_token = None;
        let c = runtime
            .store
            .upsert(
                "synthetic-c".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                {
                    let mut c = credential("synthetic-c");
                    c.refresh_token = None;
                    c.expires_at = Some(1_000);
                    c
                },
                Some(identity("synthetic-c")),
            )
            .unwrap();
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: c.id })
                .await
                .unwrap_err(),
            "Credential expired; reauthenticate this account"
        );
    }

    #[tokio::test]
    async fn workbench_metadata_stays_available_during_a_reserved_mutation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let _reservation = runtime.mutations.lock().await;
        for operation in [
            Operation::Snapshot,
            Operation::Status,
            Operation::MonitorStatus,
        ] {
            let response = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                runtime.execute(operation),
            )
            .await;
            assert!(response.unwrap().is_ok());
        }
    }
    #[tokio::test]
    async fn home_cleanup_waits_for_the_owners_launch_reservation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let account = runtime
            .store
            .add(
                "Fixture".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "work".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "fixture-only").unwrap(),
            )
            .unwrap();
        let home = root.path().join("homes").join(&account.id);
        private_fs::private_dir(&home).unwrap();
        // Model a launch that has entered the owner transaction but has not yet
        // written its reservation. Removal must not inspect/delete this home.
        let transaction = runtime.mutations.lock().await;
        let removing = runtime.clone();
        let id = account.id.clone();
        let mut remove =
            tokio::spawn(async move { removing.execute(Operation::Remove { id }).await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut remove)
                .await
                .is_err()
        );
        assert!(home.is_dir());
        private_fs::private_write(&home.join(".launch-pending"), b"fixture launch").unwrap();
        drop(transaction);
        assert!(remove.await.unwrap().is_err());
        assert!(home.is_dir());
        assert_eq!(runtime.store.snapshot().unwrap().accounts.len(), 1);
    }
}
