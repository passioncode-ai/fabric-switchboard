//! One store/proxy/control owner shared by the desktop and CLI.
pub mod agents;
pub mod analytics;
mod blocking;
pub mod control;
pub mod external;
#[cfg(target_os = "macos")]
mod external_keychain;
pub mod launch;
mod limits;
mod monitor;
pub mod oplog;
pub mod projects;
mod refresh;
pub mod uninstall;
mod usage_gate;
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
    /// Creates (`pool: None`) or updates a project: its name, folders and whole account set.
    SaveProject {
        pool: Option<String>,
        name: String,
        folders: Vec<PathBuf>,
        account_ids: Vec<String>,
    },
    RemoveProject {
        pool: String,
    },
    ApplyProject {
        path: PathBuf,
        session: projects::Session,
        global: bool,
    },
    Backups,
    BackupNow,
    RestoreBackup {
        file: String,
    },
}

/// The ordinary CLI sign-in: read by capture/current/rotation, written only by activation.
/// Tests substitute synthetic sources so they never touch real Claude or Codex auth.
#[derive(Clone, Copy)]
pub(crate) struct NativeSources {
    /// The sign-in as the background may read it: probed quietly, read only on change, never
    /// in a way that can show a dialog (lifecycle LC-04).
    pub(crate) current: fn(Provider) -> Result<external::CapturedProfile, String>,
    /// The read a person asked for (capture); None: the same as `current`.
    pub(crate) fresh: Option<fn(Provider) -> Result<external::CapturedProfile, String>>,
    /// Writes the target under Claude Code's locks; `preserve` receives the outgoing live
    /// credential under those same locks, before anything is written.
    pub(crate) activate: Activate,
    /// Claude Code's live credential item under its locks, for renewing it while idle.
    pub(crate) live: fn() -> Result<Box<dyn external::LiveItem>, String>,
    /// Claude Swap's saved profiles, read on demand before switching to an account it holds.
    pub(crate) swap: fn() -> Result<external::ImportBatch, String>,
    /// Whether Claude Swap runs and switches, read from the process table in-process.
    pub(crate) swap_activity: fn() -> external::SwapActivity,
    /// A cheap fingerprint of Claude Swap's files.
    pub(crate) swap_signature: fn() -> Option<(u64, u64, u128)>,
    /// Claude Code's recent session transcripts, for its limit markers.
    pub(crate) transcripts: fn(i64) -> Vec<PathBuf>,
}
pub(crate) type Activate = fn(
    &Credential,
    &ExternalIdentity,
    Option<&ExternalIdentity>,
    &mut dyn FnMut(&external::Outgoing<'_>) -> Result<(), String>,
) -> Result<(), String>;
/// Synthetic owners (tests, the packaged smoke check) never read or write real sign-in.
pub(crate) const UNAVAILABLE: NativeSources = NativeSources {
    current: |_| Err("External sign-in unavailable or its files are unsafe.".into()),
    fresh: None,
    activate: |_, _, _, _| {
        Err("Current Claude credential is unavailable; activation was cancelled.".into())
    },
    live: no_live,
    swap: no_swap,
    swap_activity: external::SwapActivity::none,
    swap_signature: || None,
    transcripts: |_| Vec::new(),
};
/// Claude Swap as a synthetic owner sees it: absent.
pub(crate) fn no_swap() -> Result<external::ImportBatch, String> {
    Err("Claude Swap profiles not found.".into())
}
/// The live sign-in of a synthetic owner: never reachable.
pub(crate) fn no_live() -> Result<Box<dyn external::LiveItem>, String> {
    Err("External sign-in unavailable or its files are unsafe.".into())
}
pub(crate) const NATIVE: NativeSources = NativeSources {
    current: external::capture_current_cached,
    fresh: Some(external::capture_current),
    activate: external::activate_claude,
    live: external::lock_live,
    swap: external::read_claude_swap,
    swap_activity: external::claude_swap_activity,
    swap_signature: external::claude_swap_signature,
    transcripts: limits::transcript_files,
};
impl NativeSources {
    /// The read for an operation a person asked for.
    pub(crate) fn fresh(&self, provider: Provider) -> Result<external::CapturedProfile, String> {
        (self.fresh.unwrap_or(self.current))(provider)
    }
}

pub struct Runtime {
    pub store: Arc<Store>,
    pub proxy: ProxyHandle,
    pub root: PathBuf,
    logins: Mutex<HashMap<String, launch::Login>>,
    /// Saved sign-ins whose staging home could not be removed yet.
    stale_logins: Mutex<Vec<launch::Login>>,
    /// Sign-ins that saved their account (SB-42): login id → account, newest last, at most
    /// `COMPLETED_LOGINS`. A repeated Finish returns the same account without capturing again;
    /// they hold no sign-in slot.
    completed_logins: Mutex<std::collections::VecDeque<(String, Account)>>,
    mutations: tokio::sync::Mutex<()>,
    current_cache: Mutex<CurrentCache>,
    monitor_decisions: Mutex<Vec<Value>>,
    native: NativeSources,
    refresh: refresh::RefreshState,
    limits: limits::LimitState,
    /// Provider not-before per account, and one quota request per account at a time (SB-39).
    usage_gate: usage_gate::UsageGate,
    /// Set only for a real owner; synthetic owners never write a backup anywhere.
    backup_key: Mutex<Option<Arc<dyn switchboard_core::backup::BackupKey>>>,
    backup_folder: Mutex<Option<PathBuf>>,
    backup_state: Mutex<BackupState>,
    /// Anonymous usage analytics; only the desktop app of the real data folder, in a release
    /// build, starts it (docs/ANALYTICS.md).
    analytics: Mutex<Option<Arc<analytics::Analytics>>>,
}
#[derive(Default)]
pub(crate) struct BackupState {
    pub(crate) written_changes: Option<u64>,
    pub(crate) written_at: i64,
    pub(crate) last_error: Option<String>,
    pub(crate) writing: bool,
}
/// `SWITCHBOARD_BACKUP_DIR` (absolute), else `Fabric Switchboard Backups` beside — not inside —
/// the app's data folder: `~/Library/Application Support` on macOS (no privacy prompt, unlike
/// Documents), `%APPDATA%` on Windows. Removing the app's data folder leaves it.
pub fn backup_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SWITCHBOARD_BACKUP_DIR").map(PathBuf::from) {
        return dir.is_absolute().then_some(dir);
    }
    dirs::data_dir().map(|d| d.join("Fabric Switchboard Backups"))
}
/// Saved sign-ins remembered for a repeated Finish (SB-42).
const COMPLETED_LOGINS: usize = 16;

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
        let proxy = start_proxy(&root, store.clone()).await?;
        let refresh = refresh::RefreshState::default();
        refresh.remember_in(root.join("renewal-state.json"));
        let usage_gate =
            usage_gate::UsageGate::kept_in(root.join(usage_gate::HOLDS_FILE), monitor::now());
        let limits = limits::LimitState::kept_in(root.join(limits::EVIDENCE_FILE), monitor::now());
        Ok(Arc::new(Self {
            store,
            proxy,
            root,
            logins: Mutex::new(HashMap::new()),
            stale_logins: Mutex::new(Vec::new()),
            completed_logins: Mutex::new(std::collections::VecDeque::new()),
            mutations: tokio::sync::Mutex::new(()),
            current_cache: Mutex::new(CurrentCache::default()),
            monitor_decisions: Mutex::new(Vec::new()),
            native,
            refresh,
            limits,
            usage_gate,
            backup_key: Mutex::new(None),
            backup_folder: Mutex::new(None),
            backup_state: Mutex::new(BackupState::default()),
            analytics: Mutex::new(None),
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
                | Operation::Backups
        ) {
            return execute(self.store.clone(), &self.root, Some(self), operation).await;
        }
        // A quota check reads the network and writes only its own observation; it takes the
        // owner transaction only for a renewal, as the background pass does, so a slow
        // provider never holds every other operation (SB-39).
        if let Operation::Usage { id } = &operation {
            return Ok(json!(
                monitor::check(
                    &self.store,
                    self.native,
                    &self.refresh,
                    id,
                    Some(&self.mutations),
                    &self.usage_gate,
                )
                .await?
            ));
        }
        // The Store mutex protects metadata, but launch and removal also mutate
        // private homes. Keep the complete operation in one owner transaction.
        let _mutation = self.mutations.lock().await;
        let reported = Reported::of(&operation, &self.store);
        let result = execute(self.store.clone(), &self.root, Some(self), operation).await;
        if let (Ok(_), Some(reported)) = (&result, reported) {
            self.report(&reported);
        }
        result
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
    /// Saves the signed-in account and cleans its staging home. The result is the account with
    /// `login_cleanup: "done" | "pending"`: a saved account is a success even when the staging
    /// home could not be removed yet (it is retried before the next sign-in and on every repeated
    /// Finish). A repeated Finish returns the same account and never captures again (SB-42).
    fn finish_login(&self, id: &str) -> Result<Value, String> {
        let receipt = |account: &Account, cleaned: bool| {
            let mut value = json!(account);
            value["login_cleanup"] = json!(if cleaned { "done" } else { "pending" });
            value
        };
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        let Some(login) = logins.get_mut(id) else {
            drop(logins);
            let account = self
                .completed_logins
                .lock()
                .map_err(|_| "Sign-in state unavailable.")?
                .iter()
                .find(|(done, _)| done == id)
                .map(|(_, account)| account.clone())
                .ok_or("Sign-in not found. Start again.")?;
            return Ok(receipt(&account, self.retry_cleanup_of(id)));
        };
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
        let cleaned = launch::clean_login(login).is_ok();
        // The account is saved: the slot is released even when staging cleanup fails.
        if let Some(login) = logins.remove(id) {
            if !cleaned {
                if let Ok(mut stale) = self.stale_logins.lock() {
                    stale.push(login);
                }
            }
        }
        drop(logins);
        if let Ok(mut completed) = self.completed_logins.lock() {
            completed.retain(|(done, _)| done != id);
            completed.push_back((id.to_owned(), account.clone()));
            while completed.len() > COMPLETED_LOGINS {
                completed.pop_front();
            }
        }
        self.invalidate_current();
        Ok(receipt(&account, cleaned))
    }
    /// Tries once more to remove a saved sign-in's staging home. True when nothing is left.
    fn retry_cleanup_of(&self, id: &str) -> bool {
        let Ok(mut stale) = self.stale_logins.lock() else {
            return false;
        };
        stale.retain(|login| login.id != id || launch::clean_login(login).is_err());
        !stale.iter().any(|login| login.id == id)
    }
    fn login_status(&self, id: &str) -> Result<Value, String> {
        let logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        let Some(login) = logins.get(id) else {
            // A saved sign-in reads as complete; Finish then returns its receipt.
            let saved = self
                .completed_logins
                .lock()
                .is_ok_and(|completed| completed.iter().any(|(done, _)| done == id));
            return if saved {
                Ok(json!({"state": "complete"}))
            } else {
                Err("Sign-in not found. Start again.".into())
            };
        };
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
    pub(crate) fn enable_backups(
        &self,
        key: Option<Arc<dyn switchboard_core::backup::BackupKey>>,
        folder: Option<PathBuf>,
    ) {
        if let Ok(mut slot) = self.backup_key.lock() {
            *slot = key;
        }
        if let Ok(mut slot) = self.backup_folder.lock() {
            *slot = folder;
        }
    }
    /// Starts analytics for the desktop app (docs/ANALYTICS.md): a release build (it carries
    /// the App Key) owning the real data folder. `launch` is `ordinary` or `background`.
    pub fn enable_analytics(self: &Arc<Self>, launch: &str) {
        let (Some(key), Some(shared)) = (analytics::APP_KEY, analytics::shared_folder()) else {
            return;
        };
        if default_root().ok().as_deref() != Some(self.root.as_path()) {
            return;
        }
        self.enable_analytics_with(key, analytics::HOST, shared, launch);
    }
    pub(crate) fn enable_analytics_with(
        self: &Arc<Self>,
        key: &str,
        host: &str,
        shared: PathBuf,
        launch: &str,
    ) {
        let client = Arc::new(analytics::Analytics::new(key, host, shared, &self.root));
        if let Ok(snapshot) = self.store.snapshot() {
            client.started(launch, &snapshot);
        }
        if let Ok(mut slot) = self.analytics.lock() {
            *slot = Some(client.clone());
        }
        tokio::spawn(async move { client.flush().await });
    }
    pub(crate) fn analytics(&self) -> Option<Arc<analytics::Analytics>> {
        self.analytics.lock().ok().and_then(|slot| slot.clone())
    }
    /// `{available, enabled}` for About. Unavailable outside a release desktop build.
    pub fn analytics_status(&self) -> Value {
        self.analytics().map_or_else(
            || json!({"available": false, "enabled": false}),
            |a| a.status(),
        )
    }
    /// The person's switch; shared by every PassionCode app on this machine.
    pub fn set_analytics(&self, enabled: bool) -> Result<Value, String> {
        self.analytics()
            .ok_or_else(|| "Usage analytics are available in the installed app only.".to_string())?
            .set_enabled(enabled)
    }
    /// After an operation: accounts it added or removed, a switch it made; then a send.
    fn report(&self, kind: &Reported) {
        let Some(client) = self.analytics() else {
            return;
        };
        match kind {
            Reported::Accounts(method) => {
                if let Ok(snapshot) = self.store.snapshot() {
                    client.reconcile(&snapshot, method);
                }
            }
            Reported::Switch { provider, target } => {
                client.switched(provider.as_str(), target, "manual");
            }
            Reported::Project(change) => {
                if let Ok(snapshot) = self.store.snapshot() {
                    client.project(change, &snapshot);
                }
            }
        }
        if client.pending() > 0 {
            tokio::spawn(async move { client.flush().await });
        }
    }
    fn backup_folder(&self) -> Option<PathBuf> {
        self.backup_folder.lock().ok().and_then(|f| f.clone())
    }
    fn backup_key(&self) -> Option<Arc<dyn switchboard_core::backup::BackupKey>> {
        self.backup_key.lock().ok().and_then(|k| k.clone())
    }
    /// Writes a backup when accounts changed (at most once a minute) or once a day. The state
    /// lock is held only to decide and to record: the write reads every credential, and the
    /// About panel's listing must not wait behind it.
    pub(crate) fn maybe_backup(
        &self,
        time: i64,
        force: bool,
    ) -> Result<Option<switchboard_core::backup::Info>, String> {
        let (Some(key), Some(dir)) = (self.backup_key(), self.backup_folder()) else {
            return if force {
                Err("Backups are written by the desktop app or switchboard serve.".into())
            } else {
                Ok(None)
            };
        };
        let changes = self.store.changes();
        {
            let mut state = self
                .backup_state
                .lock()
                .map_err(|_| "Backup state unavailable.")?;
            let changed = state.written_changes != Some(changes);
            let due = force
                || (changed && time - state.written_at >= 60)
                || time - state.written_at >= 86_400
                || time < state.written_at;
            if !due || state.writing {
                return Ok(None);
            }
            state.writing = true;
        }
        // The backup key comes through `/usr/bin/security`: a blocking section (SB-23).
        let result = blocking::run(|| {
            switchboard_core::backup::write(&self.store, &dir, key.as_ref(), time)
        });
        let mut state = self
            .backup_state
            .lock()
            .map_err(|_| "Backup state unavailable.")?;
        state.writing = false;
        // Success or not, the next attempt waits a minute, not a tick.
        state.written_at = time;
        match result {
            Ok(info) => {
                state.written_changes = Some(changes);
                state.last_error = info.as_ref().filter(|i| i.missing > 0).map(|i| {
                    format!(
                        "{} of the accounts could not be read and are missing from the latest backup.",
                        i.missing
                    )
                });
                Ok(info)
            }
            Err(error) => {
                state.last_error = Some(error.clone());
                Err(error)
            }
        }
    }
    fn invalidate_current(&self) {
        external::forget_current();
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
        // The window shows what is signed in now: the watched read probes the sources and
        // reads again only when one changed (lifecycle LC-04), so a refresh timer never
        // spawns a Keychain read and never shows a dialog.
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

/// Where the proxy's port and token are recorded, so a managed session started before a
/// restart or an update still reaches the proxy (lifecycle LC-11).
const PROXY_FILE: &str = "proxy.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyRecord {
    protocol: u8,
    port: u16,
    token: String,
}
fn load_proxy_record(root: &Path) -> Option<ProxyRecord> {
    let bytes =
        switchboard_core::private_fs::read_private_strict(&root.join(PROXY_FILE), 4096).ok()?;
    let record: ProxyRecord = serde_json::from_slice(&bytes).ok()?;
    (record.protocol == 1
        && record.port != 0
        && record.token.len() == 64
        && record.token.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(record)
}
/// Starts the proxy on its recorded port and token. When another program holds that port it
/// starts on a fresh one, keeps the token, and points the managed homes' generated files at
/// the new port, so a session launched from them again reaches the proxy.
async fn start_proxy(root: &Path, store: Arc<Store>) -> Result<ProxyHandle, String> {
    let record = load_proxy_record(root);
    let proxy = ProxyHandle::start_on(
        store,
        record.as_ref().map(|r| r.port),
        record.as_ref().map(|r| r.token.clone()),
    )
    .await?;
    let port = proxy.address().port();
    let unchanged = record
        .as_ref()
        .is_some_and(|r| r.port == port && r.token == proxy.token());
    if !unchanged {
        let saved = ProxyRecord {
            protocol: 1,
            port,
            token: proxy.token().to_owned(),
        };
        if let Ok(bytes) = serde_json::to_vec(&saved) {
            let _ = switchboard_core::private_fs::private_write(&root.join(PROXY_FILE), &bytes);
        }
    }
    if let Some(old) = record.filter(|r| r.port != port) {
        let homes = launch::repoint_managed(root, old.port, port);
        oplog::event(
            "proxy_port_changed",
            &[
                ("old_port", oplog::Field::Number(old.port.into())),
                ("new_port", oplog::Field::Number(port.into())),
                ("files_repointed", oplog::Field::Number(homes as i64)),
            ],
        );
    }
    Ok(proxy)
}

/// Lifecycle LC-01's deadlines: the owner's drain gets `DRAIN_DEADLINE`; if the process has
/// not exited by `HARD_EXIT_AFTER` from the stop request, it exits regardless.
pub const DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(8);
pub const HARD_EXIT_AFTER: std::time::Duration = std::time::Duration::from_secs(10);
/// Arms the backstop: a plain thread that ends the process if the orderly stop overruns.
pub fn arm_hard_exit(after: std::time::Duration) {
    let _ = std::thread::Builder::new()
        .name("switchboard-hard-exit".into())
        .spawn(move || {
            std::thread::sleep(after);
            oplog::event("hard_exit", &[]);
            std::process::exit(1);
        });
}
/// Resolves on the first stop request the process receives: `SIGTERM` or `SIGINT` (Unix),
/// Ctrl-C (elsewhere). Names the signal as a code for the log.
/// SIGTERM and SIGINT (Ctrl-C on Windows), caught from the moment `listen` returns. An owner
/// creates this before it publishes its control descriptor: a stop that arrives during start-up
/// then runs the drain instead of the signal's default action, which would end the process
/// with the descriptor left behind (lifecycle LC-01).
pub struct StopSignals {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}
impl StopSignals {
    /// Must be called inside a Tokio runtime.
    pub fn listen() -> Result<Self, String> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            Ok(Self {
                terminate: signal(SignalKind::terminate())
                    .map_err(|_| "Shutdown signal unavailable.")?,
                interrupt: signal(SignalKind::interrupt())
                    .map_err(|_| "Shutdown signal unavailable.")?,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {})
        }
    }
    /// Waits for the first stop and names it.
    pub async fn requested(mut self) -> Result<&'static str, String> {
        #[cfg(unix)]
        {
            Ok(tokio::select! {
                _ = self.terminate.recv() => "sigterm",
                _ = self.interrupt.recv() => "sigint",
            })
        }
        #[cfg(not(unix))]
        {
            let _ = &mut self;
            tokio::signal::ctrl_c()
                .await
                .map_err(|_| "Shutdown signal unavailable.")?;
            Ok("ctrl_c")
        }
    }
}
/// Waits for SIGTERM or SIGINT, listening only from this call on; `StopSignals` listens from
/// earlier.
pub async fn stop_requested() -> Result<&'static str, String> {
    StopSignals::listen()?.requested().await
}

/// How an owner's shutdown ended (lifecycle LC-01).
pub struct Stopped {
    /// Everything in flight finished inside the deadline; false: the rest was abandoned.
    pub drained: bool,
    pub millis: u64,
}
/// Holding this value keeps both listeners and the exclusive store lease alive.
pub struct Owner {
    pub runtime: Arc<Runtime>,
    control: control::ControlHandle,
    monitor: monitor::MonitorHandle,
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
        // Only the real data folder is backed up: a `--data-dir` scratch store must never
        // write into the operator's backup folder with the operator's key.
        let default_store = default_root().ok().as_deref() == Some(runtime.root.as_path());
        if native_sources && default_store {
            if let Some(dir) = backup_dir() {
                runtime.enable_backups(switchboard_core::backup::platform_key(&dir), Some(dir));
            }
        }
        // One renewer per lineage: only the owner of the real data folder spends refresh
        // tokens. A `--data-dir` store holding the same accounts would be a second renewer.
        if native_sources && !default_store {
            runtime.refresh.set_grants(false);
        }
        let monitor = monitor::MonitorHandle::start(&runtime, native_sources);
        oplog::event(
            "owner_started",
            &[
                ("pid", oplog::Field::Number(std::process::id().into())),
                (
                    "proxy_port",
                    oplog::Field::Number(runtime.proxy.address().port().into()),
                ),
            ],
        );
        Ok(Self {
            runtime,
            control,
            monitor,
        })
    }
    /// The one shutdown every quit path runs (lifecycle LC-01): no new background pass or
    /// request starts; the pass and the operation in flight — a renewal must store the token
    /// the provider just returned — get until `deadline`; the proxy lets its streams finish
    /// inside what is left (at most two seconds); pending timestamps are written; the
    /// descriptor is removed. Returns once the store's lock is free, or at the deadline.
    pub async fn shutdown(self, deadline: std::time::Duration) -> Stopped {
        let started = std::time::Instant::now();
        let left = |cap: std::time::Duration| deadline.saturating_sub(started.elapsed()).min(cap);
        let Self {
            runtime,
            control,
            monitor,
        } = self;
        // No refresh grant starts once the drain begins: a quota check in flight holds no
        // transaction (SB-39), and a grant it began after the drain would leave the provider's
        // new token only in this exiting process's memory. A grant already sent still records
        // its successor before returning.
        runtime.refresh.set_grants(false);
        let mut drained = monitor.stop(left(deadline)).await;
        control.close().await;
        // The operation in flight holds the owner's transaction; taking it means it finished,
        // and holding it means no other one starts.
        let transaction = tokio::time::timeout(left(deadline), runtime.mutations.lock()).await;
        drained &= transaction.is_ok();
        drained &= runtime
            .proxy
            .stop(left(std::time::Duration::from_secs(2)))
            .await;
        let _ = runtime.store.flush();
        drop(transaction);
        let millis = started.elapsed().as_millis() as u64;
        oplog::event(
            "owner_stopped",
            &[
                (
                    "outcome",
                    oplog::Field::Code(if drained { "drained" } else { "abandoned" }),
                ),
                ("millis", oplog::Field::Number(millis as i64)),
            ],
        );
        drop(runtime);
        Stopped { drained, millis }
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

/// Refusal: an account the ordinary CLI is signed in to cannot join a project.
pub const PROJECT_ACCOUNT_IN_CLI: &str = "This account is signed in to the ordinary Claude Code or Codex, which every folder uses. Switch the CLI to another account first, then add this one to the project.";
/// What an operation reports to analytics once it succeeds (docs/ANALYTICS.md).
enum Reported {
    /// Accounts may have been added or removed; additions are attributed to this method.
    Accounts(&'static str),
    Switch {
        provider: Provider,
        target: &'static str,
    },
    /// A project was created, changed or removed: counts only.
    Project(&'static str),
}
impl Reported {
    fn of(operation: &Operation, store: &Store) -> Option<Self> {
        Some(match operation {
            Operation::CaptureCurrent { .. } => Self::Accounts("capture"),
            Operation::ImportClaudeSwap { .. } => Self::Accounts("import_claude_swap"),
            Operation::Add { .. } => Self::Accounts("manual"),
            Operation::FinishLogin { .. } => Self::Accounts("sign_in"),
            Operation::RestoreBackup { .. } => Self::Accounts("restore"),
            Operation::Remove { .. } => Self::Accounts("removed"),
            Operation::SaveProject { pool, .. } => {
                Self::Project(if pool.is_some() { "updated" } else { "created" })
            }
            Operation::RemoveProject { .. } => Self::Project("removed"),
            Operation::Select { provider, .. } => Self::Switch {
                provider: *provider,
                target: "managed",
            },
            Operation::ActivateNative { id } => Self::Switch {
                provider: store
                    .snapshot()
                    .ok()?
                    .accounts
                    .into_iter()
                    .find(|a| &a.id == id)?
                    .provider,
                target: "native",
            },
            _ => return None,
        })
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
    // It never spends a refresh token: it cannot see Claude Swap's holds or a running owner's
    // state, and its kept successors would die with it (audit P1-2). Rejected lineages are
    // still refused, from the owner's journal.
    let offline_refresh = refresh::RefreshState::offline(root.join("renewal-state.json"));
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
            // The CLI's own account is every folder's: it cannot land in a project's pool.
            if store.snapshot()?.project_of_pool(&pool).is_some() {
                return Err(PROJECT_ACCOUNT_IN_CLI.into());
            }
            external::forget_current();
            if provider == Provider::Claude {
                refresh::learn_live_owner(&store, native, refresh_state).await;
            }
            let captured = native.fresh(provider)?;
            // A sign-in that is another account's lineage never lands under this name, and one
            // nothing attributes never overwrites a copy already saved (audit P2-1).
            if provider == Provider::Claude && captured.kind == AuthKind::OAuth {
                if let Err(refusal) = refresh::filable(
                    &store,
                    refresh_state,
                    &captured.identity,
                    &captured.credential,
                ) {
                    let saved =
                        !matching_accounts(&store, provider, &captured.identity)?.is_empty();
                    if refusal == refresh::FOREIGN_LIVE || saved {
                        return Err(refusal);
                    }
                }
            }
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
            // Claude Swap switches the ordinary Claude Code: its profiles are every folder's.
            if store.snapshot()?.project_of_pool(&pool).is_some() {
                return Err(switchboard_core::PROJECT_NOT_NATIVE.into());
            }
            let batch = (native.swap)()?;
            let (imported, failed, skipped) =
                import_profiles(&store, refresh_state, &pool, batch.profiles);
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(
                json!({"imported": imported, "failed": batch.failed + failed, "skipped": batch.skipped + skipped, "claude_swap_running": external::claude_swap_running()}),
            )
        }
        Operation::ActivateNative { id } => {
            // A switch the user asked for starts from what is signed in now.
            external::forget_current();
            // The target is inactive by definition; an expired token is renewed first so
            // Claude Code starts on a live one. A failed refresh still activates: Claude
            // Code renews from the refresh token itself.
            refresh::learn_live_owner(&store, native, refresh_state).await;
            if refresh::ensure_fresh(&store, native, refresh_state, &id, false).await
                == refresh::Outcome::SignInRequired
            {
                return Err(refresh::SIGN_IN.into());
            }
            activate_native(&store, &id, None, native, Some(refresh_state))?;
            // A manual switch starts the new account's error slate after the grace, too.
            if let Some(runtime) = runtime {
                let identity = store
                    .snapshot()?
                    .accounts
                    .into_iter()
                    .find(|a| a.id == id)
                    .and_then(|a| a.external_identity);
                runtime.limits.switched(identity.as_ref(), monitor::now());
            }
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
            "running": runtime.is_some(), "interval_seconds": monitor::INTERVAL_SECONDS, "idle_interval_seconds": switchboard_core::CHECK_IDLE_SECONDS,
            "decisions": runtime.and_then(|r| r.monitor_decisions.lock().ok().map(|v| v.clone())).unwrap_or_default(),
            "sign_in_required": refresh_state.sign_in_required(&store),
            "limited": runtime.map(|r| r.limits.report(monitor::now())).unwrap_or_default(),
            "claude_swap_accounts": refresh::swap_held(refresh_state),
            "renewal_blocked": refresh_state.renewal_blocked(),
            "claude_swap_switching": refresh_state.swap_switching(),
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
        // Only the offline CLI reaches this arm (`Runtime::execute` answers a quota check
        // before taking its transaction); it owns the store alone, so no lock is needed. It
        // honours the owner's recorded provider waits and records its own.
        Operation::Usage { id } => {
            let offline_gate;
            let gate = match runtime {
                Some(runtime) => &runtime.usage_gate,
                None => {
                    offline_gate = usage_gate::UsageGate::kept_in(
                        root.join(usage_gate::HOLDS_FILE),
                        monitor::now(),
                    );
                    &offline_gate
                }
            };
            Ok(json!(
                monitor::check(&store, native, refresh_state, &id, None, gate).await?
            ))
        }
        Operation::Launch {
            id,
            mode,
            working_directory,
        } => {
            let runtime = needs_owner()?;
            if mode == "isolated" {
                // An isolated session holds an access token only and is never renewed while it
                // runs: start it with hours left, not minutes (report §P2-10).
                refresh::learn_live_owner(&store, native, refresh_state).await;
                let short = store
                    .stored_credential(&id)
                    .is_ok_and(|c| short_lived(&c, monitor::now()));
                if refresh::ensure_fresh(&store, native, refresh_state, &id, short).await
                    == refresh::Outcome::Active
                {
                    // The account Claude Code is on: its live token is the newest one.
                    adopt_live(&store, native, refresh_state, &id);
                }
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
        Operation::FinishLogin { login_id } => needs_owner()?.finish_login(&login_id),
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
        Operation::SaveProject {
            pool,
            name,
            folders,
            account_ids,
        } => {
            let folders = folders
                .iter()
                .map(|f| projects::project_dir(root, f))
                .collect::<Result<Vec<_>, _>>()?;
            // The account the ordinary CLI is signed in to would keep serving every folder.
            let current = match runtime {
                Some(runtime) => runtime.current_accounts()?,
                None => observe_current(&store, native.current),
            };
            let in_cli = |id: &String| {
                ["claude", "codex"].iter().any(|p| {
                    current[*p]["account_ids"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|v| v == id.as_str()))
                })
            };
            let snapshot = store.snapshot()?;
            if account_ids.iter().any(|id| {
                in_cli(id)
                    && snapshot
                        .accounts
                        .iter()
                        .find(|a| &a.id == id)
                        .is_some_and(|a| Some(a.pool.as_str()) != pool.as_deref())
            }) {
                return Err(PROJECT_ACCOUNT_IN_CLI.into());
            }
            let project = store.save_project(pool.as_deref(), &name, &folders, &account_ids)?;
            Ok(projects::project_view(&store.snapshot()?, &project))
        }
        Operation::RemoveProject { pool } => {
            let project = store.remove_project(&pool)?;
            Ok(json!({"removed": project.pool, "name": project.name}))
        }
        Operation::RemoveProjectRule { path, provider } => {
            let path = projects::rule_path(root, &path)?;
            let rule = store.remove_rule(&path, provider)?;
            Ok(projects::rule_view(&store, &rule, monitor::now()))
        }
        Operation::Backups => {
            let enabled_dir = runtime.and_then(|r| r.backup_folder());
            let dir = enabled_dir.clone().or_else(backup_dir);
            let key = runtime.and_then(|r| r.backup_key()).or_else(|| {
                dir.as_deref()
                    .and_then(switchboard_core::backup::platform_key)
            });
            let (last_error, written_at) = runtime
                .and_then(|r| {
                    r.backup_state
                        .lock()
                        .ok()
                        .map(|s| (s.last_error.clone(), s.written_at))
                })
                .unwrap_or((None, 0));
            let enabled =
                enabled_dir.is_some() && runtime.is_some_and(|r| r.backup_key().is_some());
            let backups = match (dir.as_deref(), key) {
                (Some(dir), Some(key)) => switchboard_core::backup::list(dir, &store, key.as_ref()),
                _ => vec![],
            };
            Ok(json!({
                "directory": dir.as_ref().map(|d| d.to_string_lossy().into_owned()),
                "enabled": enabled,
                "backups": backups,
                "last_error": last_error,
                "last_written_at": (written_at > 0).then_some(written_at),
            }))
        }
        Operation::BackupNow => match runtime {
            Some(runtime) => Ok(json!(runtime.maybe_backup(monitor::now(), true)?)),
            None => Err("Backups are written by the desktop app or switchboard serve.".into()),
        },
        Operation::RestoreBackup { file } => {
            let dir = runtime
                .and_then(|r| r.backup_folder())
                .or_else(backup_dir)
                .ok_or("The backup folder is unavailable.")?;
            let key = runtime
                .and_then(|r| r.backup_key())
                .or_else(|| switchboard_core::backup::platform_key(&dir))
                .ok_or("Backups are not available on this platform.")?;
            let restored = switchboard_core::backup::restore(&store, &dir, &file, key.as_ref())?;
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(json!(restored))
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
    // A project's account never becomes the ordinary Claude Code's: that serves every folder.
    let snapshot = store.snapshot()?;
    if snapshot
        .accounts
        .iter()
        .find(|a| a.id == id)
        .is_some_and(|a| snapshot.project_of_pool(&a.pool).is_some())
    {
        return Err(switchboard_core::PROJECT_NOT_NATIVE.into());
    }
    // A renewal of this account is in flight: its stored token is being spent right now.
    if refresh.is_some_and(|state| state.in_flight(store, id)) {
        return Err(refresh::RENEWING.into());
    }
    // An account Claude Swap renews: its newest generation may be in Swap's files, not here —
    // take it first, or Claude Code would get a token Swap already spent (audit P2-3).
    if let Some(state) = refresh {
        refresh::catch_up_with_claude_swap(store, state, native, id)?;
    }
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
/// An isolated launch starts with at least this much token life (seconds).
const ISOLATED_MIN_LIFE: i64 = 4 * 3600;
fn short_lived(credential: &Credential, now: i64) -> bool {
    credential
        .expires_at
        .is_some_and(|t| t - now < ISOLATED_MIN_LIFE)
}
/// Files the ordinary Claude Code's live credential under `id` when it is that account's own
/// lineage — for launching the account Claude Code is on, whose stored copy may lag.
fn adopt_live(store: &Store, native: NativeSources, state: &refresh::RefreshState, id: &str) {
    let Ok(live) = (native.current)(Provider::Claude) else {
        return;
    };
    let Some(account) = store
        .snapshot()
        .ok()
        .and_then(|s| s.accounts.into_iter().find(|a| a.id == id))
    else {
        return;
    };
    if account.external_identity.as_ref() != Some(&live.identity)
        || refresh::lineage(store, state, &live.identity, &live.credential) != refresh::Lineage::Own
    {
        return;
    }
    let _ = store.upsert(
        account.label,
        account.provider,
        account.kind,
        account.pool,
        live.credential,
        Some(live.identity),
    );
}
/// Claude Swap profiles into `pool`, never moving an account backwards: an identity already
/// saved with a newer generation (another refresh token that expires no earlier) keeps — and
/// seeds a new copy with — that generation, and another account's lineage is not imported
/// (audit P1-3). Returns (imported, failed, skipped).
fn import_profiles(
    store: &Store,
    state: &refresh::RefreshState,
    pool: &str,
    profiles: Vec<external::CapturedProfile>,
) -> (Vec<Account>, usize, usize) {
    let (mut imported, mut failed, mut skipped) = (Vec::new(), 0, 0);
    for profile in profiles {
        let mut credential = profile.credential;
        if profile.kind == AuthKind::OAuth && profile.identity.account_id.is_some() {
            if refresh::lineage(store, state, &profile.identity, &credential)
                == refresh::Lineage::Foreign
            {
                skipped += 1;
                continue;
            }
            let newest = matching_accounts(store, profile.provider, &profile.identity)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|copy| store.stored_credential(&copy.id).ok())
                .filter(|stored| {
                    stored.refresh_token != credential.refresh_token
                        && stored.expires_at.unwrap_or(0) >= credential.expires_at.unwrap_or(0)
                })
                .max_by_key(|stored| stored.expires_at.unwrap_or(0));
            if let Some(newest) = newest {
                credential = newest;
            }
        }
        match store.upsert(
            profile.label,
            profile.provider,
            profile.kind,
            pool.to_owned(),
            credential,
            Some(profile.identity),
        ) {
            Ok(account) => imported.push(account),
            Err(_) => failed += 1,
        }
    }
    (imported, failed, skipped)
}
pub(crate) const UNSAVED_CURRENT: &str = "Claude Code is signed in to an account Switchboard has not saved; switching would sign it out. Add it first (In use now → Add to Switchboard).";
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
    // A switch is decided on what is signed in now, never on a remembered read: a stale one
    // could call the target "already in use" and record a switch that never happened.
    external::forget_current();
    // A renewed generation that could not be stored yet is the only valid one.
    if let Some(state) = refresh {
        match refresh::adopt_stash(store, state, &account.id) {
            Some(refresh::Outcome::Transient) => return Err(
                "Switchboard has not stored this account's renewed sign-in yet. Retry in a minute."
                    .into(),
            ),
            // Its successor belonged to another account: what this row holds is spent.
            Some(refresh::Outcome::SignInRequired) => return Err(refresh::SIGN_IN.into()),
            _ => {}
        }
    }
    // An expired access token is fine while a refresh token remains: Claude Code renews it
    // on first use, exactly as after its own idle expiry (PLAN-0.5 C-5).
    let credential = store.stored_credential(&account.id)?;
    if credential.refresh_token.is_none()
        && credential.expires_at.is_some_and(|t| t <= monitor::now())
    {
        return Err("Credential expired; reauthenticate this account".into());
    }
    // After `claude logout`, or once Claude Code wiped an ended sign-in, nothing is current.
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
    let offline_state = refresh::RefreshState::default();
    let state = refresh.unwrap_or(&offline_state);
    let mut copies = Vec::new();
    if let Some(current) = &current {
        // The account being switched away from stays "recently active" for renewal purposes.
        state.note_active(&current.identity);
        // Already the account in use: writing its stored copy would replace Claude Code's
        // newer live generation with an older one. Nothing to do.
        if current.identity.account_id.is_some()
            && current.identity.account_id == identity.account_id
            && current.identity.organization_id == identity.organization_id
        {
            return Ok(());
        }
        copies = matching_accounts(store, Provider::Claude, &current.identity)?;
        if copies.is_empty() {
            return Err(UNSAVED_CURRENT.into());
        }
        // Another account's lineage under this account's name is never filed here, and one
        // nobody can attribute is not filed either (the caller asked the provider first).
        refresh::filable(store, state, &current.identity, &current.credential)?;
    }
    let mut preserve = |outgoing: &external::Outgoing<'_>| -> Result<(), String> {
        let live = external::profile_of(outgoing)?;
        if expected.as_ref().is_some_and(|e| e != &live.identity) {
            return Err("Current Claude account changed during activation. Try again.".into());
        }
        // The bytes read under the locks are the newest generation; Claude Code may have
        // refreshed since the check above, and a different lineage now is not this account's.
        refresh::filable(store, state, &live.identity, &live.credential)?;
        // Every stored copy of that identity, in every pool (PLAN-0.5 C-6) — except one that
        // already holds a newer generation than Claude Code's (a renewal Switchboard stored
        // while writing it back to Claude Code failed): the live token is the spent one then.
        for copy in &copies {
            if store.stored_credential(&copy.id).is_ok_and(|stored| {
                stored.refresh_token != live.credential.refresh_token
                    && stored.expires_at.unwrap_or(0) > live.credential.expires_at.unwrap_or(0)
            }) {
                continue;
            }
            store.upsert(
                copy.label.clone(),
                copy.provider,
                copy.kind,
                copy.pool.clone(),
                live.credential.clone(),
                Some(live.identity.clone()),
            )?;
        }
        Ok(())
    };
    (native.activate)(&credential, identity, expected.as_ref(), &mut preserve)?;
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
            &json!({"claudeAiOauth":{"accessToken":format!("{account}-token"),"refreshToken":format!("{account}-refresh"),"expiresAt":EXPIRES*1000}}).to_string(),
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
    /// Behaves like the real adapter: when an account is signed in (`signed_in` is
    /// `synthetic-a`), its live credential reaches `preserve` before anything is written.
    pub(crate) fn activates(
        _: &Credential,
        _: &ExternalIdentity,
        expected: Option<&ExternalIdentity>,
        preserve: &mut dyn FnMut(&external::Outgoing<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(current) = expected {
            let account = current.account_id.clone().unwrap_or_default();
            let auth = json!({"claudeAiOauth":{"accessToken":format!("{account}-token"),"refreshToken":format!("{account}-refresh"),"expiresAt":EXPIRES*1000}}).to_string();
            let config = json!({"oauthAccount":{"accountUuid":account,"organizationUuid":current.organization_id,"emailAddress":current.email}}).to_string();
            preserve(&external::Outgoing {
                auth: auth.as_bytes(),
                config: config.as_bytes(),
            })?;
        }
        Ok(())
    }
    pub(crate) fn fails(
        _: &Credential,
        _: &ExternalIdentity,
        _: Option<&ExternalIdentity>,
        _: &mut dyn FnMut(&external::Outgoing<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        Err("Claude activation failed; previous account restored.".into())
    }
    pub(crate) async fn runtime(
        root: &Path,
        current: fn(Provider) -> Result<external::CapturedProfile, String>,
        activate: Activate,
    ) -> Arc<Runtime> {
        Runtime::open_with(
            root.to_owned(),
            Arc::new(MemoryVault::default()),
            NativeSources {
                current,
                activate,
                live: no_live,
                swap: no_swap,
                ..UNAVAILABLE
            },
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
        let account_id = account.id.clone();
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
        // SB-42: the account is saved, so Finish succeeds and says the cleanup is pending.
        let first = result.unwrap();
        assert_eq!(first["login_cleanup"], "pending");
        assert_eq!(first["id"], json!(account_id));
        assert!(runtime.logins.lock().unwrap().is_empty(), "no slot is held");
        assert!(home.exists());
        // A repeated Finish (another window, a retry) returns the same account, captures
        // nothing, and the sign-in still reads as complete; the cleanup is still pending.
        let again = runtime
            .execute(Operation::FinishLogin {
                login_id: id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            (again["id"].clone(), again["login_cleanup"].clone()),
            (json!(account_id), json!("pending"))
        );
        assert_eq!(
            runtime
                .execute(Operation::LoginStatus {
                    login_id: id.clone()
                })
                .await
                .unwrap()["state"],
            "complete"
        );
        // Once the folder can be removed, the next Finish cleans it and says so.
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let done = runtime
            .execute(Operation::FinishLogin {
                login_id: id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(done["login_cleanup"], "done");
        assert!(!home.exists());
        assert!(runtime.stale_logins.lock().unwrap().is_empty());
        assert_eq!(
            runtime.store.snapshot().unwrap().accounts.len(),
            1,
            "one account, saved once"
        );
        // An unknown sign-in is still unknown.
        assert_eq!(
            runtime
                .execute(Operation::FinishLogin {
                    login_id: "unknown".into()
                })
                .await
                .unwrap_err(),
            "Sign-in not found. Start again."
        );
    }
    #[test]
    fn completed_sign_ins_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(fixtures::runtime(root.path(), signed_out, activates));
        let account = save(&runtime.store, "synthetic-a", "default");
        for i in 0..(COMPLETED_LOGINS + 5) {
            let home = root.path().join(format!("staging-{i}"));
            std::fs::create_dir_all(&home).unwrap();
            let login = launch::fixture_login(home, account.clone());
            let id = login.id.clone();
            runtime.logins.lock().unwrap().insert(id.clone(), login);
            assert_eq!(runtime.finish_login(&id).unwrap()["login_cleanup"], "done");
        }
        assert_eq!(
            runtime.completed_logins.lock().unwrap().len(),
            COMPLETED_LOGINS
        );
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
    async fn the_account_the_cli_uses_cannot_join_a_project() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        let repo = tempfile::tempdir().unwrap();
        let save_with = |ids: Vec<String>| Operation::SaveProject {
            pool: None,
            name: "Alpha".into(),
            folders: vec![repo.path().to_owned()],
            account_ids: ids,
        };
        assert_eq!(
            runtime
                .execute(save_with(vec![a.id.clone()]))
                .await
                .unwrap_err(),
            PROJECT_ACCOUNT_IN_CLI
        );
        assert!(runtime.store.snapshot().unwrap().projects.is_empty());
        runtime
            .execute(save_with(vec![b.id.clone()]))
            .await
            .unwrap();
        // Nor can the CLI's account be captured, or Claude Swap's profiles imported, into it.
        assert_eq!(
            runtime
                .execute(Operation::CaptureCurrent {
                    provider: Provider::Claude,
                    label: None,
                    pool: "alpha".into(),
                })
                .await
                .unwrap_err(),
            PROJECT_ACCOUNT_IN_CLI
        );
        assert_eq!(
            runtime
                .execute(Operation::ImportClaudeSwap {
                    pool: "alpha".into()
                })
                .await
                .unwrap_err(),
            switchboard_core::PROJECT_NOT_NATIVE
        );
    }
    #[tokio::test]
    async fn a_project_account_never_becomes_the_ordinary_claude_code() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let b = save(&runtime.store, "synthetic-b", "default");
        let repo = tempfile::tempdir().unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Alpha".into(),
                folders: vec![repo.path().to_owned()],
                account_ids: vec![b.id.clone()],
            })
            .await
            .unwrap();
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: b.id.clone() })
                .await
                .unwrap_err(),
            switchboard_core::PROJECT_NOT_NATIVE
        );
        assert!(
            events(&runtime.store, "activation").is_empty(),
            "nothing was attempted"
        );
        // The folder resolves to the project, with its account and no credential.
        let inside = repo.path().canonicalize().unwrap().join("src");
        std::fs::create_dir_all(&inside).unwrap();
        let resolved = runtime
            .execute(Operation::ResolveProject { path: inside })
            .await
            .unwrap();
        assert_eq!(resolved["project"]["pool"], "alpha");
        assert_eq!(resolved["project"]["accounts"][0]["id"], b.id.as_str());
        assert!(!resolved.to_string().contains("synthetic-b-token"));
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
    /// A local `/api/oauth/profile` that attributes every token to `account`.
    async fn profile_of(account: &'static str) -> String {
        use axum::{routing::get, Json, Router};
        let app = Router::new().route(
            "/profile",
            get(move || async move {
                Json(json!({"account":{"uuid":account},"organization":{"uuid":"synthetic-org"}}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}/profile")
    }
    #[tokio::test]
    async fn a_live_generation_nobody_can_attribute_is_never_filed() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        // Claude Code renewed synthetic-a on its own: the stored copy has an older lineage, and
        // the provider cannot be asked (offline, or the access token has lapsed).
        let home = stale_copy(&runtime.store, "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: b.id.clone() })
                .await
                .unwrap_err(),
            refresh::UNCONFIRMED_LIVE
        );
        assert_eq!(
            runtime
                .store
                .stored_credential(&home.id)
                .unwrap()
                .access_token,
            "synthetic-a-old-token",
            "nothing filed"
        );
    }
    #[tokio::test]
    async fn switching_away_keeps_the_live_generation_in_every_pool() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        // The provider names the live lineage's owner, so it is synthetic-a's newest generation.
        runtime
            .refresh
            .set_profile_endpoint(profile_of("synthetic-a").await);
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
            assert_eq!(stored.refresh_token.as_deref(), Some("synthetic-a-refresh"));
        }
    }
    #[tokio::test]
    async fn a_copy_newer_than_claude_codes_item_is_not_overwritten_on_switch() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        runtime
            .refresh
            .set_profile_endpoint(profile_of("synthetic-a").await);
        // Switchboard stored a renewal of synthetic-a, but writing it back to Claude Code
        // failed: Claude Code's item still holds the spent generation.
        let mut newer = credential("synthetic-a");
        newer.access_token = "synthetic-a-newer-token".into();
        newer.refresh_token = Some("synthetic-a-newer-refresh".into());
        newer.expires_at = newer.expires_at.map(|t| t + 1000);
        let a = runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                newer,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let b = save(&runtime.store, "synthetic-b", "default");
        runtime
            .execute(Operation::ActivateNative { id: b.id })
            .await
            .unwrap();
        assert_eq!(
            runtime
                .store
                .stored_credential(&a.id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-a-newer-refresh")
        );
    }
    fn email_only(provider: Provider) -> Result<external::CapturedProfile, String> {
        let mut live = signed_in(provider)?;
        live.identity = ExternalIdentity {
            account_id: None,
            organization_id: None,
            email: Some("someone@example.invalid".into()),
        };
        Ok(live)
    }
    #[tokio::test]
    async fn identities_without_an_account_id_are_never_the_same_account() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), email_only, activates).await;
        let target = runtime
            .store
            .upsert(
                "other".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential("synthetic-b"),
                Some(ExternalIdentity {
                    account_id: None,
                    organization_id: None,
                    email: Some("other@example.invalid".into()),
                }),
            )
            .unwrap();
        // Two email-only identities once compared equal (None == None): a silent no-op.
        assert!(runtime
            .execute(Operation::ActivateNative { id: target.id })
            .await
            .is_err());
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

    #[derive(Default)]
    struct FakeKey(Mutex<Option<[u8; 32]>>);
    impl switchboard_core::backup::BackupKey for FakeKey {
        fn load(&self) -> Result<Option<[u8; 32]>, String> {
            Ok(*self.0.lock().unwrap())
        }
        fn create(&self, key: &[u8; 32]) -> Result<bool, String> {
            *self.0.lock().unwrap() = Some(*key);
            Ok(true)
        }
    }
    #[tokio::test]
    async fn backups_follow_changes_and_restore_into_a_fresh_install() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("backups");
        let key: Arc<dyn switchboard_core::backup::BackupKey> = Arc::new(FakeKey::default());
        let runtime = fixtures::runtime(&root.path().join("one"), signed_out, activates).await;
        // A synthetic owner has no key: nothing is ever written.
        assert!(runtime.maybe_backup(1_000_000, true).is_err());
        runtime.enable_backups(Some(key.clone()), Some(folder.clone()));
        assert_eq!(
            runtime.maybe_backup(1_000_000, false).unwrap(),
            None,
            "no accounts"
        );
        save(&runtime.store, "synthetic-a", "default");
        let first = runtime.maybe_backup(1_000_100, false).unwrap().unwrap();
        assert_eq!(first.accounts, 1);
        // Nothing changed: no new file until a day has passed.
        assert_eq!(runtime.maybe_backup(1_000_200, false).unwrap(), None);
        save(&runtime.store, "synthetic-b", "default");
        assert_eq!(
            runtime.maybe_backup(1_000_130, false).unwrap(),
            None,
            "within a minute"
        );
        let second = runtime.maybe_backup(1_000_300, false).unwrap().unwrap();
        assert_eq!(second.accounts, 2);
        let listed = runtime.execute(Operation::Backups).await.unwrap();
        assert_eq!(listed["backups"].as_array().unwrap().len(), 2);
        assert_eq!(listed["enabled"], true);
        // Reinstall on the same machine: an empty data folder, the same key and backups.
        let fresh = fixtures::runtime(&root.path().join("two"), signed_out, activates).await;
        fresh.enable_backups(Some(key), Some(folder));
        let restored = fresh
            .execute(Operation::RestoreBackup { file: second.file })
            .await
            .unwrap();
        assert_eq!(restored["added"], 2);
        assert_eq!(fresh.store.snapshot().unwrap().accounts.len(), 2);
    }
    /// The ordinary Claude Code names synthetic-a but holds synthetic-b's lineage — what an
    /// interrupted switch or a half-finished `/login` leaves behind.
    fn a_named_b_held(_: Provider) -> Result<external::CapturedProfile, String> {
        let mut credential = credential("synthetic-b");
        credential.native_context = Some(
            json!({"auth":{"claudeAiOauth":{"accessToken":"synthetic-b-token"}},"oauth_account":{"accountUuid":"synthetic-a"}}),
        );
        Ok(external::CapturedProfile {
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            credential,
            identity: identity("synthetic-a"),
            label: "synthetic-a".into(),
        })
    }
    #[tokio::test]
    async fn a_foreign_lineage_under_this_name_is_never_filed_or_switched_from() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), a_named_b_held, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: c.id.clone() })
                .await
                .unwrap_err(),
            refresh::FOREIGN_LIVE
        );
        // A's copy keeps its own lineage; B's is untouched.
        assert_eq!(
            runtime.store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-token"
        );
        assert_eq!(
            runtime.store.stored_credential(&b.id).unwrap().access_token,
            "synthetic-b-token"
        );
    }
    #[tokio::test]
    async fn capturing_another_accounts_lineage_under_this_name_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), a_named_b_held, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        save(&runtime.store, "synthetic-b", "default");
        assert_eq!(
            runtime
                .execute(Operation::CaptureCurrent {
                    provider: Provider::Claude,
                    label: None,
                    pool: "default".into()
                })
                .await
                .unwrap_err(),
            refresh::FOREIGN_LIVE
        );
        assert_eq!(
            runtime.store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-token",
            "A keeps its own lineage"
        );
    }
    #[tokio::test]
    async fn capturing_an_unattributed_sign_in_never_overwrites_a_saved_copy() {
        let root = tempfile::tempdir().unwrap();
        // Signed in as synthetic-a; nothing attributes its token (provider unreachable).
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let capture = || Operation::CaptureCurrent {
            provider: Provider::Claude,
            label: None,
            pool: "default".into(),
        };
        // A first capture of a new account is allowed: nothing can be overwritten.
        let first = runtime.execute(capture()).await.unwrap();
        let id = first["id"].as_str().unwrap().to_owned();
        // The saved copy now holds that lineage: a second capture is Own and goes through.
        runtime.execute(capture()).await.unwrap();
        // A saved copy with a different, unattributed lineage is not overwritten.
        let mut other = credential("synthetic-a");
        other.refresh_token = Some("an-older-lineage".into());
        runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                other,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        assert_eq!(
            runtime.execute(capture()).await.unwrap_err(),
            refresh::UNCONFIRMED_LIVE
        );
        assert_eq!(
            runtime
                .store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("an-older-lineage")
        );
    }
    #[test]
    fn importing_from_claude_swap_never_moves_an_account_backwards() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let state = refresh::RefreshState::default();
        // Switchboard renewed A to a generation only it holds.
        let mut newest = credential("synthetic-a");
        newest.refresh_token = Some("a-g5".into());
        newest.expires_at = Some(5_000_000_000);
        let a = store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                newest,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        save(&store, "synthetic-b", "default");
        let profile = |account: &str, refresh: &str, expires: i64| external::CapturedProfile {
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            credential: {
                let mut c = credential(account);
                c.refresh_token = Some(refresh.into());
                c.expires_at = Some(expires);
                c
            },
            identity: identity(account),
            label: account.into(),
        };
        let (imported, failed, skipped) = import_profiles(
            &store,
            &state,
            "work",
            vec![
                // Swap's spent G1 of A.
                profile("synthetic-a", "a-g1", 1_000),
                // B's own refresh token under C's name: another account's lineage.
                profile("synthetic-c", "synthetic-b-refresh", 9_000_000_000),
            ],
        );
        assert_eq!((imported.len(), failed, skipped), (1, 0, 1));
        assert_eq!(
            store
                .stored_credential(&a.id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("a-g5"),
            "the newer generation stays"
        );
        assert_eq!(
            store
                .stored_credential(&imported[0].id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("a-g5"),
            "a new copy in another pool starts from the newest generation"
        );
    }
    #[tokio::test]
    async fn switching_away_from_an_unsaved_account_is_refused_and_the_account_in_use_is_a_no_op() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, fails).await;
        let b = save(&runtime.store, "synthetic-b", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: b.id.clone() })
                .await
                .unwrap_err(),
            UNSAVED_CURRENT
        );
        // Activating the account Claude Code already uses writes nothing (the fixture would fail).
        let a = save(&runtime.store, "synthetic-a", "work");
        runtime
            .execute(Operation::ActivateNative { id: a.id })
            .await
            .unwrap();
    }
    #[test]
    fn an_isolated_launch_wants_hours_of_token_life() {
        let mut c = credential("synthetic-a");
        c.expires_at = Some(10_000 + 3 * 3600);
        assert!(short_lived(&c, 10_000));
        c.expires_at = Some(10_000 + 5 * 3600);
        assert!(!short_lived(&c, 10_000));
        c.expires_at = None;
        assert!(
            !short_lived(&c, 10_000),
            "no expiry known: nothing to renew"
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

#[cfg(test)]
mod lifecycle_tests {
    //! Lifecycle rules LC-01 (quit within a deadline) and LC-11 (addresses survive a restart).
    use super::*;
    use std::time::Duration;
    use switchboard_core::MemoryVault;

    async fn owner(root: &Path) -> Owner {
        Owner::start(root.to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap()
    }
    /// A managed session holds the proxy's address and token in its environment.
    async fn status_as_holder(address: std::net::SocketAddr, token: &str) -> reqwest::StatusCode {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://{address}/claude/default/v1/messages"))
            .bearer_auth(token)
            .body("{}")
            .send()
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn a_restarted_owner_keeps_the_address_and_token_a_session_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let first = owner(tmp.path()).await;
        let address = first.runtime.proxy.address();
        let token = first.runtime.proxy.token().to_owned();
        assert_ne!(
            status_as_holder(address, &token).await,
            reqwest::StatusCode::UNAUTHORIZED
        );
        first.shutdown(Duration::from_secs(5)).await;
        let second = owner(tmp.path()).await;
        assert_eq!(second.runtime.proxy.address(), address);
        assert_eq!(second.runtime.proxy.token(), token);
        // The session started before the restart still gets through.
        assert_ne!(
            status_as_holder(address, &token).await,
            reqwest::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status_as_holder(address, "not-the-token").await,
            reqwest::StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn a_taken_port_moves_the_proxy_and_repoints_managed_homes_with_the_same_token() {
        let tmp = tempfile::tempdir().unwrap();
        let first = owner(tmp.path()).await;
        let old = first.runtime.proxy.address();
        let token = first.runtime.proxy.token().to_owned();
        let runtimes = tmp.path().join("runtimes");
        for home in ["claude-team", "codex-team"] {
            switchboard_core::private_fs::private_dir(&runtimes.join(home)).unwrap();
        }
        let script = runtimes.join("claude-team/launch.command");
        let config = runtimes.join("codex-team/config.toml");
        switchboard_core::private_fs::private_write(
            &script,
            format!("export ANTHROPIC_BASE_URL='http://{old}/claude/team'\n").as_bytes(),
        )
        .unwrap();
        switchboard_core::private_fs::private_write(
            &config,
            format!("base_url = \"http://{old}/codex/team/v1\"\n").as_bytes(),
        )
        .unwrap();
        first.shutdown(Duration::from_secs(5)).await;
        // Another program took the port while Switchboard was closed.
        let squatter = std::net::TcpListener::bind(old).unwrap();
        let second = owner(tmp.path()).await;
        let new = second.runtime.proxy.address();
        assert_ne!(new.port(), old.port());
        assert_eq!(
            second.runtime.proxy.token(),
            token,
            "sessions keep their token"
        );
        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            format!("export ANTHROPIC_BASE_URL='http://{new}/claude/team'\n")
        );
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            format!("base_url = \"http://{new}/codex/team/v1\"\n")
        );
        drop(squatter);
        // The new port is the recorded one from now on.
        second.shutdown(Duration::from_secs(5)).await;
        let third = owner(tmp.path()).await;
        assert_eq!(third.runtime.proxy.address(), new);
    }

    #[tokio::test]
    async fn an_idle_owner_stops_at_once_and_removes_its_descriptor() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = owner(tmp.path()).await;
        let descriptor = tmp.path().join("control.json");
        assert!(descriptor.exists());
        let started = std::time::Instant::now();
        let stopped = owner.shutdown(Duration::from_secs(5)).await;
        assert!(stopped.drained);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!descriptor.exists());
        // The store is free for the next owner.
        Store::open(tmp.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
    }

    #[tokio::test]
    async fn a_busy_owner_finishes_the_operation_in_flight_within_the_deadline() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = owner(tmp.path()).await;
        let runtime = owner.runtime.clone();
        // An operation in flight holds the owner's transaction for two seconds.
        let (held, wait) = tokio::sync::oneshot::channel();
        let work = tokio::spawn(async move {
            let _transaction = runtime.mutations.lock().await;
            let _ = held.send(());
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        wait.await.unwrap();
        let started = std::time::Instant::now();
        let stopped = owner.shutdown(Duration::from_secs(8)).await;
        assert!(
            stopped.drained,
            "the operation finished inside the deadline"
        );
        assert!(started.elapsed() >= Duration::from_millis(1500));
        assert!(started.elapsed() < Duration::from_secs(8));
        work.await.unwrap();
        // One that outlives the deadline is abandoned at the deadline, not waited for.
        let owner = super::lifecycle_tests::owner(tmp.path()).await;
        let runtime = owner.runtime.clone();
        let (held, wait) = tokio::sync::oneshot::channel();
        let _stuck = tokio::spawn(async move {
            let _transaction = runtime.mutations.lock().await;
            let _ = held.send(());
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        wait.await.unwrap();
        let started = std::time::Instant::now();
        let stopped = owner.shutdown(Duration::from_secs(1)).await;
        assert!(!stopped.drained);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!tmp.path().join("control.json").exists());
    }

    /// A quota check in flight at quit holds no transaction, so the drain does not wait for it;
    /// if the provider then rejects the token, the check must not spend the refresh token after
    /// the drain (SB-39 seam review).
    #[tokio::test]
    async fn a_quota_check_left_running_at_quit_spends_no_refresh_token() {
        use axum::{
            http::StatusCode,
            routing::{get, post},
            Router,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        // Claude Code reads as signed out, so nothing marks the account as the one in use and
        // a rejected token would be renewed.
        let runtime =
            fixtures::runtime(tmp.path(), fixtures::signed_out, fixtures::activates).await;
        let owner = Owner {
            runtime: runtime.clone(),
            control: control::ControlHandle::start(runtime.clone())
                .await
                .unwrap(),
            monitor: monitor::MonitorHandle::start(&runtime, false),
        };
        let account = fixtures::save(&runtime.store, "synthetic-a", "default");
        let probes = Arc::new(AtomicUsize::new(0));
        let grants = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (p, g, s) = (probes.clone(), grants.clone(), gate.clone());
        let app = Router::new()
            .route(
                "/api/oauth/usage",
                get(move || {
                    let (p, s) = (p.clone(), s.clone());
                    async move {
                        p.fetch_add(1, Ordering::SeqCst);
                        s.acquire().await.unwrap().forget();
                        StatusCode::UNAUTHORIZED
                    }
                }),
            )
            .route(
                "/token",
                post(move || {
                    let g = g.clone();
                    async move {
                        g.fetch_add(1, Ordering::SeqCst);
                        StatusCode::BAD_REQUEST
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        *runtime.usage_gate.origins.lock().unwrap() = Some((base.clone(), base.clone()));
        runtime.refresh.set_endpoint(format!("{base}/token"));
        let check = {
            let (runtime, id) = (runtime.clone(), account.id.clone());
            tokio::spawn(async move { runtime.execute(Operation::Usage { id }).await })
        };
        while probes.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        // A background pass may have joined the stuck check and be abandoned at the deadline;
        // what matters is what happens after the drain.
        let started = std::time::Instant::now();
        owner.shutdown(Duration::from_secs(2)).await;
        assert!(started.elapsed() < Duration::from_secs(3));
        // The provider answers after the drain: the token is rejected.
        gate.add_permits(10);
        assert!(check.await.unwrap().is_err());
        assert_eq!(
            grants.load(Ordering::SeqCst),
            0,
            "no grant after the drain began"
        );
    }
}

/// SB-39 end to end: the provider's not-before holds for every caller, a check never holds the
/// owner's transaction, and same-account checks share one request.
#[cfg(test)]
mod usage_gate_tests {
    use super::*;
    use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
    use fixtures::*;
    use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
    use switchboard_core::MemoryVault;

    struct Upstream {
        calls: Arc<AtomicUsize>,
        status: Arc<AtomicU16>,
        retry_after: Arc<Mutex<&'static str>>,
        hold: Arc<tokio::sync::Semaphore>,
    }
    /// A synthetic Claude usage endpoint: answers `status` (200 with a quota body), counts
    /// requests, and waits for a permit when `hold` has none.
    async fn upstream(runtime: &Runtime) -> Upstream {
        let calls = Arc::new(AtomicUsize::new(0));
        let status = Arc::new(AtomicU16::new(200));
        let retry_after = Arc::new(Mutex::new("600"));
        let hold = Arc::new(tokio::sync::Semaphore::new(1_000));
        let (c, s, r, h) = (
            calls.clone(),
            status.clone(),
            retry_after.clone(),
            hold.clone(),
        );
        let app = Router::new().route(
            "/api/oauth/usage",
            get(move || {
                let (c, s, r, h) = (c.clone(), s.clone(), r.clone(), h.clone());
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    h.acquire().await.unwrap().forget();
                    match s.load(Ordering::SeqCst) {
                        200 => {
                            axum::Json(json!({"five_hour":{"utilization":42.0,"resets_at":null}}))
                                .into_response()
                        }
                        429 => (
                            StatusCode::TOO_MANY_REQUESTS,
                            [("retry-after", *r.lock().unwrap())],
                            "upstream detail that must not leak",
                        )
                            .into_response(),
                        code => StatusCode::from_u16(code).unwrap().into_response(),
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let base = format!("http://{address}");
        *runtime.usage_gate.origins.lock().unwrap() = Some((base.clone(), base));
        Upstream {
            calls,
            status,
            retry_after,
            hold,
        }
    }
    async fn usage(runtime: &Runtime, id: &str) -> Result<Value, String> {
        runtime.execute(Operation::Usage { id: id.into() }).await
    }

    #[tokio::test]
    async fn no_caller_checks_before_the_providers_not_before() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;

        up.status.store(429, Ordering::SeqCst);
        assert_eq!(
            usage(&runtime, &a.id).await.unwrap_err(),
            switchboard_proxy::USAGE_RATE_LIMITED
        );
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
        // The desktop, the CLI and MCP all send this operation: none of them reaches the
        // provider again inside its wait, even once it would answer.
        up.status.store(200, Ordering::SeqCst);
        for _ in 0..3 {
            assert_eq!(
                usage(&runtime, &a.id).await.unwrap_err(),
                switchboard_proxy::USAGE_RATE_LIMITED
            );
        }
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
        // The background leaves the row out of its pass, and its schedule is past the wait.
        let health = runtime.store.snapshot().unwrap().accounts[0]
            .usage_health
            .clone()
            .unwrap();
        assert!(health.next_check_at - health.checked_at >= 600);
        assert!(runtime.usage_gate.held(&a.id, monitor::now()));
        // Nothing of the provider's answer is kept: the file has times and a fingerprint.
        let file = std::fs::read_to_string(root.path().join(usage_gate::HOLDS_FILE)).unwrap();
        assert!(!file.contains("synthetic-a-token") && !file.contains("upstream detail"));
    }

    #[tokio::test]
    async fn the_first_check_after_the_wait_is_one_request() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;
        up.status.store(429, Ordering::SeqCst);
        // Two seconds, not one: the clock has whole-second resolution, and a one-second wait
        // recorded at x.999 s legitimately ends before an immediate second call.
        *up.retry_after.lock().unwrap() = "2";
        assert!(usage(&runtime, &a.id).await.is_err());
        up.status.store(200, Ordering::SeqCst);
        assert!(usage(&runtime, &a.id).await.is_err());
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
        // Wait for the hold itself to end rather than for a fixed time.
        while runtime.usage_gate.held(&a.id, monitor::now()) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(usage(&runtime, &a.id).await.unwrap()["used_percent"], 42.0);
        assert_eq!(up.calls.load(Ordering::SeqCst), 2);
        assert!(
            !runtime.usage_gate.has_hold(&a.id),
            "a passed wait is forgotten"
        );
    }

    #[tokio::test]
    async fn a_wait_is_kept_even_when_its_health_record_cannot_be_written() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;
        // An observation stamped a little ahead of the clock makes the failed-health write
        // refuse ("older than the stored observation").
        let ahead = monitor::now() + 30;
        let observation = switchboard_proxy::parse_usage_at(
            Provider::Claude,
            &json!({"five_hour":{"utilization":10.0,"resets_at":null}}),
            ahead,
        )
        .unwrap();
        runtime.store.observe(&a.id, observation).unwrap();
        up.status.store(429, Ordering::SeqCst);
        assert!(usage(&runtime, &a.id).await.is_err());
        up.status.store(200, Ordering::SeqCst);
        assert_eq!(
            usage(&runtime, &a.id).await.unwrap_err(),
            switchboard_proxy::USAGE_RATE_LIMITED
        );
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_ordinary_failure_leaves_a_manual_retry_open() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;
        up.status.store(503, Ordering::SeqCst);
        assert!(usage(&runtime, &a.id).await.is_err());
        up.status.store(200, Ordering::SeqCst);
        assert_eq!(usage(&runtime, &a.id).await.unwrap()["used_percent"], 42.0);
        assert_eq!(up.calls.load(Ordering::SeqCst), 2);
        assert!(!runtime.usage_gate.has_hold(&a.id));
    }

    #[tokio::test]
    async fn a_new_sign_in_is_checked_despite_the_old_tokens_wait() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;
        up.status.store(429, Ordering::SeqCst);
        assert!(usage(&runtime, &a.id).await.is_err());
        // The same account signs in again: a new credential generation.
        let mut renewed = credential("synthetic-a");
        renewed.access_token = "synthetic-a-renewed".into();
        runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                renewed,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        assert!(snapshot.accounts[0].usage_health.is_none());
        up.status.store(200, Ordering::SeqCst);
        assert_eq!(usage(&runtime, &a.id).await.unwrap()["used_percent"], 42.0);
        assert_eq!(up.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn same_account_checks_share_one_request_and_hold_no_transaction() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let up = upstream(&runtime).await;
        // The provider is slow: requests wait until a permit is given.
        let held = up.hold.acquire_many(1_000).await.unwrap();
        held.forget();
        let first = {
            let (runtime, id) = (runtime.clone(), a.id.clone());
            tokio::spawn(async move { usage(&runtime, &id).await })
        };
        while up.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let second = {
            let (runtime, id) = (runtime.clone(), a.id.clone());
            tokio::spawn(async move { usage(&runtime, &id).await })
        };
        // The second caller asks while the first check is still waiting on the provider.
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        // Any other operation proceeds while the provider is still answering.
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            runtime.execute(Operation::Update {
                id: a.id.clone(),
                label: "renamed".into(),
                enabled: true,
            }),
        )
        .await
        .expect("a quota check does not hold the owner's transaction")
        .unwrap();
        up.hold.add_permits(1_000);
        assert_eq!(first.await.unwrap().unwrap()["used_percent"], 42.0);
        assert_eq!(second.await.unwrap().unwrap()["used_percent"], 42.0);
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_offline_cli_honours_a_wait_the_owner_recorded() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        // An expired token: whatever happens, this test never reaches the network.
        let mut expired = credential("synthetic-a");
        expired.expires_at = Some(1_000);
        let a = store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired.clone(),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let now = monitor::now();
        store.usage_health(&a.id, "failed", now, now + 900).unwrap();
        usage_gate::UsageGate::kept_in(root.path().join(usage_gate::HOLDS_FILE), now).hold(
            &a.id,
            &expired,
            now,
            now + 900,
        );
        let answer = execute(
            store.clone(),
            root.path(),
            None,
            Operation::Usage { id: a.id.clone() },
        )
        .await;
        assert_eq!(answer.unwrap_err(), switchboard_proxy::USAGE_RATE_LIMITED);
    }
}
