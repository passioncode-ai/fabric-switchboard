//! One store/proxy/control owner shared by the desktop and CLI.
mod activity;
pub mod agent_catalog;
pub mod agent_keys;
pub mod agent_settings;
pub mod agents;
pub mod analytics;
mod bindings;
mod blocking;
mod carry;
mod codex_route;
pub mod continuation;
pub mod control;
pub mod external;
#[cfg(target_os = "macos")]
mod external_keychain;
mod fallback;
pub mod handovers;
pub mod hermes;
pub mod kimi;
pub mod launch;
mod limits;
mod monitor;
pub mod onboarding;
pub mod oplog;
pub mod projects;
pub mod recovery;
mod refresh;
pub mod roots;
mod statistics;
pub mod suggest;
pub mod uninstall;
mod usage_gate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
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
    /// A field left out keeps its saved value. Both present serializes as before, so an owner
    /// of an earlier version still understands the full form.
    Update {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
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
        /// Prepare the session and return its script for the caller to run in its own terminal,
        /// instead of opening Terminal (SB-75).
        #[serde(default)]
        in_place: bool,
        /// Arguments for the agent after Switchboard's own (e.g. `--resume <id>`); in place only.
        #[serde(default)]
        args: Vec<String>,
    },
    /// The account Switchboard would use for a session of `provider` in a folder (SB-75).
    LaunchAccount {
        provider: Provider,
        working_directory: PathBuf,
    },
    /// Continue an Observatory workflow on this account (SB-52): read, offer, launch.
    Continue {
        workflow_id: String,
        id: String,
        mode: String,
        working_directory: PathBuf,
        /// Prepare the continuing session and return its script for the caller to run in its own
        /// terminal (an embedded console, SB-108), instead of opening Terminal.
        #[serde(default)]
        in_place: bool,
    },
    /// SB-108, by hand: offer a workflow like `Continue` and leave its prepared session for
    /// Fabric's console (`switchboard handovers`) instead of opening Terminal; nobody taking it
    /// within two minutes opens it in Terminal, as for the automatic fallback.
    ContinueToConsole {
        workflow_id: String,
        id: String,
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
    /// The sign-ins this owner holds, including those taken back after a restart (SB-65).
    PendingLogins,
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
    /// The sessions Switchboard launched and their state (SB-95); `all` adds ended ones.
    Sessions {
        #[serde(default)]
        all: bool,
    },
    /// Creates (`pool: None`) or updates a project: its name, folders and whole account set.
    SaveProject {
        pool: Option<String>,
        name: String,
        folders: Vec<PathBuf>,
        account_ids: Vec<String>,
        /// SB-131: the folders the edit started from (the window's dialog). With it, only the
        /// folders the person took out count as removed; folders Switchboard added or took out
        /// meanwhile are kept as Switchboard left them. Without it (the CLI), `folders` is the
        /// whole list.
        #[serde(default)]
        base_folders: Option<Vec<PathBuf>>,
    },
    RemoveProject {
        pool: String,
    },
    /// Makes a pool's folder capability worthless and writes a new one into its folders (an
    /// exposure, 2026-10-18). Other pools and managed sessions keep theirs.
    RotatePoolCapability {
        pool: String,
    },
    /// SB-134: change a project by delta — add or take out accounts and folders, rename it —
    /// without restating its whole set. Everything not named stays as it is.
    ProjectEdit {
        pool: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        add_accounts: Vec<String>,
        #[serde(default)]
        remove_accounts: Vec<String>,
        #[serde(default)]
        add_folders: Vec<PathBuf>,
        #[serde(default)]
        remove_folders: Vec<PathBuf>,
    },
    /// SB-143: whether Codex sessions in a project's folders use its Codex accounts (one
    /// user-level provider at the proxy). `enabled` none: the state only.
    CodexRoute {
        #[serde(default)]
        enabled: Option<bool>,
    },
    /// SB-122: moves one account into a project's pool, or back to `default`. Joining or leaving
    /// a project is that project's save (`ProjectEdit`), with its refusals; an account left in a
    /// deleted project's pool goes back to `default` by itself.
    MoveAccount {
        id: String,
        pool: String,
    },
    /// SB-135 (part 2): moves a project root, with the worktrees and clones under it, into another
    /// project in one step; a repository in no project joins the named one. Two project saves
    /// (`ProjectEdit`): out of the old project, then into the new; a refused second save puts the
    /// folders back.
    /// SB-146: `also` adds it to `pool` and keeps it in its other groups; `from` names the group
    /// it leaves when it is in several (default: the first that holds it).
    MoveRoot {
        folder: PathBuf,
        pool: String,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        also: bool,
    },
    /// SB-135 (part 2): the Git repositories under `under` (the person's folders, `~/DATA` by
    /// default in the window), each with the project it belongs to or none. Reads only.
    RepositoryMap {
        under: Vec<PathBuf>,
    },
    /// SB-145: what the proxy relayed in the last `days` days (1 to 90, default 30), by project,
    /// account, coding agent and day: requests, outcomes and the provider's token counts. Reads
    /// only.
    Statistics {
        #[serde(default)]
        days: Option<i64>,
    },
    /// The third-party agent catalog (`agent_catalog`).
    AgentCatalog,
    /// How to connect one agent to `switchboard mcp` and, when it can, the proxy's `pool`.
    AgentConnect {
        agent: String,
        pool: String,
    },
    /// The agents' proxy capability, for an agent's `key_cmd` or an `export`.
    AgentKey,
    /// Saves the key agents run on for a service (SB-79); `model` sets the default model.
    AgentKeySet {
        service: String,
        key: String,
        #[serde(default)]
        model: Option<String>,
        /// SB-144: a project's pool for its own key; none: the machine's key.
        #[serde(default)]
        pool: Option<String>,
    },
    /// The saved key's metadata and, with `credit`, what it may still spend (asked of the
    /// service). Never the key.
    AgentKeyStatus {
        service: String,
        #[serde(default)]
        credit: bool,
        #[serde(default)]
        pool: Option<String>,
    },
    /// Changes the default model of the saved key.
    AgentKeyModel {
        service: String,
        model: String,
        #[serde(default)]
        pool: Option<String>,
    },
    /// Removes the saved key and its vault item.
    AgentKeyRemove {
        service: String,
        #[serde(default)]
        pool: Option<String>,
    },
    /// SB-144: every saved key of a service with the project it is for (none: the machine's),
    /// and every project with or without one. Never a key.
    AgentKeys {
        service: String,
    },
    /// The key's value, for a launched agent's environment. Only the CLI asks for it (a session
    /// script's `export`); the desktop window and `switchboard mcp` never send it.
    AgentKeyValue {
        service: String,
        /// SB-144: the folder the agent starts in; its project's key wins over the machine's.
        #[serde(default)]
        folder: Option<PathBuf>,
    },
    /// Durable stopped-session registrations and capacity notices (SB-114), CLI/host only.
    Recovery,
    RecoveryRegister {
        event: String,
        session: String,
        account: String,
        directory: PathBuf,
    },
    RecoveryClaim {
        event: String,
        account: String,
    },
    RecoveryFinish {
        event: String,
        resumed: bool,
    },
    /// A Claude Code session reported its own stop (`StopFailure` hook, SB-115). The account is
    /// the one its limits were charged to, else the ordinary Claude Code's; `billing` marks a
    /// spend or credit stop, which is never probed.
    RecoveryHook {
        session: String,
        directory: PathBuf,
        billing: bool,
    },
    /// The open project suggestions (SB-117): folders Switchboard found that look like a
    /// project's: those still being added (retried, never asked; SB-131), and those added or taken
    /// out as deleted in the last day.
    ProjectSuggestions,
    /// Records `folder` when it is a project's (CLI `project suggest`, the plugin's hook).
    ProjectSuggest {
        folder: PathBuf,
    },
    /// Adds a suggested folder to its project.
    ProjectSuggestionAccept {
        folder: String,
    },
    /// Declines a suggested folder for good.
    ProjectSuggestionDismiss {
        folder: String,
    },
    /// The project folders whose ordinary Claude Code sessions use the project's pool (SB-118).
    ProjectBindings,
    /// Every catalog agent: installed or not, enabled for taking over work, default, command (SB-120).
    AgentsOverview,
    /// Lets an agent take over work, or keeps work away from it (SB-120).
    AgentEnable {
        agent: String,
        enabled: bool,
    },
    /// The machine's default agent; none is Auto (SB-120).
    AgentDefault {
        agent: Option<String>,
    },
    /// The recommended OpenRouter models and OpenRouter's tool-capable list (SB-119).
    OpenrouterModels,
    /// Replaces the repository owners whose new repositories a project is offered.
    ProjectSources {
        pool: String,
        sources: Vec<String>,
    },
    /// The hand-overs waiting for a console (SB-108) and whether Fabric gets them.
    Handovers,
    /// Takes a waiting hand-over once, for the caller's terminal (`handovers run`).
    HandoverTake {
        id: String,
    },
    /// Reads, or with `on` changes, the "Open hand-overs in Fabric" setting.
    HandoverSetting {
        #[serde(default)]
        on: Option<bool>,
    },
    /// Fallback chains (SB-71): every chain, presets, and effective workflow/project chain.
    Chains {
        #[serde(default)]
        workflow: Option<String>,
        #[serde(default)]
        pool: Option<String>,
    },
    /// Sets the chain of a scope in order; an empty list clears it. A preset id may stand for
    /// the list.
    SetChain {
        scope: switchboard_core::ChainScope,
        #[serde(default)]
        executors: Vec<switchboard_core::Executor>,
        #[serde(default)]
        preset: Option<String>,
    },
    /// Starts an agent in a folder: through the proxy on the pool's API-key account, or on the
    /// saved OpenRouter key (`via: openrouter`, SB-79) with `model` (default: the key's model).
    LaunchAgent {
        agent: String,
        #[serde(default)]
        pool: String,
        working_directory: PathBuf,
        #[serde(default)]
        via: AgentVia,
        #[serde(default)]
        model: Option<String>,
    },
    ApplyProject {
        path: PathBuf,
        session: projects::Session,
        global: bool,
    },
    /// The ordinary Hermes's model and provider (SB-80), from `hermes config get model --json`.
    HermesModel,
    /// Changes Hermes's provider and/or default model with `hermes config set` (person's request).
    HermesSetModel {
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    /// Kimi Code subscription accounts (SB-81) with their plan windows, and the ordinary `kimi`'s
    /// sign-in (shown, never switched).
    KimiAccounts,
    /// Starts the official `kimi login` in Terminal for a new account home.
    KimiLoginBegin {
        #[serde(default)]
        label: String,
        #[serde(default = "kimi_default_region")]
        region: String,
    },
    KimiLoginStatus {
        id: String,
    },
    /// Saves a completed sign-in as an account.
    KimiLoginFinish {
        id: String,
    },
    KimiLoginCancel {
        id: String,
    },
    /// Runs `kimi login` again in an account's own home.
    KimiSignInAgain {
        id: String,
    },
    /// Starts the official `kimi` on an account in a folder.
    KimiLaunch {
        id: String,
        working_directory: PathBuf,
    },
    KimiRename {
        id: String,
        label: String,
    },
    /// Forgets an account and deletes its home.
    KimiRemove {
        id: String,
    },
    Backups,
    BackupNow,
    RestoreBackup {
        file: String,
    },
}

fn kimi_default_region() -> String {
    "mainland-cn".into()
}

/// What a launched third-party agent runs on (XA-02).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentVia {
    /// Switchboard's proxy, on the pool's API-key account.
    #[default]
    Proxy,
    /// The operator's saved OpenRouter key; no proxy, pool or account.
    Openrouter,
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
    /// Codex's recent rollouts, for the repositories its sessions change (SB-132).
    pub(crate) rollouts: fn(i64) -> Vec<PathBuf>,
    /// SB-143: Codex's own `config.toml`; None for a synthetic owner, which never writes it.
    pub(crate) codex_config: fn() -> Option<PathBuf>,
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
    rollouts: |_| Vec::new(),
    codex_config: || None,
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
    rollouts: limits::codex_rollout_files,
    codex_config: codex_route::config_path,
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
    completed_logins: Mutex<std::collections::VecDeque<(String, Account, bool)>>,
    mutations: tokio::sync::Mutex<()>,
    current_cache: Mutex<CurrentCache>,
    monitor_decisions: Mutex<Vec<Value>>,
    /// Candidates whose automatic switch failed, set aside until the time given (SB-98).
    failed_switches: Mutex<HashMap<String, i64>>,
    native: NativeSources,
    refresh: refresh::RefreshState,
    limits: limits::LimitState,
    /// Automatic fallback along the operator's chains (SB-73).
    fallback: fallback::FallbackState,
    /// Saved Kimi Code accounts seen in the background while a chain is set (SB-99).
    kimi_watch: kimi::KimiWatch,
    /// Provider not-before per account, and one quota request per account at a time (SB-39).
    usage_gate: usage_gate::UsageGate,
    /// Set only for a real owner; synthetic owners never write a backup anywhere.
    backup_key: Mutex<Option<Arc<dyn switchboard_core::backup::BackupKey>>>,
    backup_folder: Mutex<Option<PathBuf>>,
    backup_state: Mutex<BackupState>,
    /// Anonymous usage analytics; only the desktop app of the real data folder, in a release
    /// build, starts it (docs/ANALYTICS.md).
    analytics: Mutex<Option<Arc<analytics::Analytics>>>,
    /// The backup restored automatically when this install started with a new data folder.
    restored_at_start: Mutex<Option<Value>>,
    /// The background search for a project's new folders (SB-117).
    suggest: Mutex<suggest::Scanner>,
    /// Moves on every new, added or declined suggestion; the app reads the list only then.
    suggestions_seq: std::sync::atomic::AtomicU64,
    /// SB-145: the proxy's counts, kept for 90 days in `statistics.json`.
    statistics: statistics::Statistics,
    /// SB-121/SB-131: when the folders that could not be added are tried again (never asked).
    suggest_retry_at: std::sync::atomic::AtomicI64,
    /// SB-131: how far recent Claude Code transcripts were read for agents' work.
    activity: Mutex<activity::Activity>,
    /// SB-131: repositories already settled this run (added, refused or claimed by two
    /// projects), forgotten when the projects change.
    worked: Mutex<(u64, HashSet<PathBuf>)>,
    /// SB-131: project folders seen missing, and since when; one still missing a while later
    /// leaves its project.
    vanished: Mutex<Vanish>,
    /// SB-131: refusals already logged this run (a folder that cannot be added, a project's
    /// last folder gone), so a retry every few minutes logs each once.
    told: Mutex<HashSet<String>>,
    /// The recent transcript list, walked once per pass and shared by the limit scan and the
    /// work scan (SB-131): `(when, files)`.
    transcript_list: Mutex<Option<(i64, Vec<PathBuf>)>>,
    /// The recent subagent transcripts, listed at most every SUBAGENTS_EVERY (SB-131).
    subagent_list: Mutex<Option<(i64, Vec<PathBuf>)>>,
    vanish_at: std::sync::atomic::AtomicI64,
    /// OpenRouter's model list, kept six hours (SB-119).
    openrouter_models: Mutex<agent_settings::ModelCache>,
    /// What the project folders' Claude Code route was last synced for (SB-118).
    bindings_key: Mutex<Option<u64>>,
    /// The bindings pass and a capability rotation write the same files: one at a time, without
    /// the owner transaction (a pass over 200 folders held it up to 13 s, 2026-10-18).
    bindings_lock: tokio::sync::Mutex<()>,
    /// The project folders' resolved repositories, re-read only when a `.git` stamp moves.
    bindings_repos: Mutex<bindings::RepoCache>,
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
/// The backup folder a caller without a running owner falls back to — only for the default data
/// folder. A `--data-dir` store must never list the operator's backups or restore them into a
/// scratch store (SB-61).
fn fallback_backup_dir(root: &std::path::Path) -> Option<PathBuf> {
    if default_root().ok().as_deref() == Some(root) {
        backup_dir()
    } else {
        None
    }
}
pub const BACKUPS_DEFAULT_FOLDER_ONLY: &str =
    "Backups belong to the default data folder; this data folder has none.";
/// Saved sign-ins remembered for a repeated Finish (SB-42).
const COMPLETED_LOGINS: usize = 16;

#[derive(Default)]
struct CurrentCache {
    generation: u64,
    entry: Option<(i64, Value)>,
}
impl Runtime {
    /// Something a restart would cut short: a managed request in flight (streams included) or a
    /// sign-in waiting to finish. An update activates only when this is false (LC-16).
    pub fn busy(&self) -> bool {
        self.busy_reason().is_some()
    }
    /// What `busy` found, as a log code (SB-105): `request_in_flight` or `sign_in_waiting`.
    pub fn busy_reason(&self) -> Option<&'static str> {
        if self.proxy.in_flight() > 0 {
            Some("request_in_flight")
        } else if self.logins.lock().map_or(true, |l| !l.is_empty()) {
            Some("sign_in_waiting")
        } else {
            None
        }
    }
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
        let statistics = statistics::Statistics::load(&root);
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
            failed_switches: Mutex::new(HashMap::new()),
            native,
            refresh,
            limits,
            fallback: fallback::FallbackState::default(),
            kimi_watch: kimi::KimiWatch::default(),
            suggest: Mutex::new(suggest::Scanner::default()),
            suggestions_seq: std::sync::atomic::AtomicU64::new(0),
            statistics,
            suggest_retry_at: std::sync::atomic::AtomicI64::new(0),
            activity: Mutex::new(activity::Activity::default()),
            worked: Mutex::new((0, HashSet::new())),
            vanished: Mutex::new(Vanish::default()),
            told: Mutex::new(HashSet::new()),
            transcript_list: Mutex::new(None),
            subagent_list: Mutex::new(None),
            vanish_at: std::sync::atomic::AtomicI64::new(0),
            bindings_key: Mutex::new(None),
            bindings_lock: tokio::sync::Mutex::new(()),
            openrouter_models: Mutex::new(agent_settings::ModelCache::default()),
            bindings_repos: Mutex::new(bindings::RepoCache::default()),
            usage_gate,
            backup_key: Mutex::new(None),
            backup_folder: Mutex::new(None),
            backup_state: Mutex::new(BackupState::default()),
            analytics: Mutex::new(None),
            restored_at_start: Mutex::new(None),
        }))
        .inspect(|rt| rt.codex_route_sync(true))
    }
    /// SB-143: with the switch on, Codex's config carries the provider while this owner runs
    /// (`up`) and loses it when it stops, so a Codex started meanwhile reaches OpenAI directly.
    pub(crate) fn codex_route_sync(&self, up: bool) {
        let Some(path) = (self.native.codex_config)() else {
            return;
        };
        if !codex_route::wanted(&self.root) {
            return;
        }
        let address = self.proxy.address().to_string();
        let outcome = if up {
            codex_route::apply(&path, &address, &self.proxy.auto_capability())
        } else {
            codex_route::remove(&path)
        };
        oplog::event(
            "codex_route",
            &[(
                "outcome",
                oplog::Field::Code(match (&outcome, up) {
                    (Ok(true), true) => "applied",
                    (Ok(true), false) => "removed",
                    (Ok(false), _) => "unchanged",
                    (Err(_), _) => "refused",
                }),
            )],
        );
    }
    pub async fn execute(&self, operation: Operation) -> Result<Value, String> {
        if let Operation::ContinueToConsole {
            workflow_id,
            id,
            working_directory,
        } = operation
        {
            return self
                .continue_to_console(workflow_id, id, working_directory)
                .await;
        }
        // Metadata reads do not wait behind OS credential prompts or network probes.
        // Store snapshot has its own lock; these operations cannot change auth or homes.
        // Current accounts only read native sources and a cache: a usage probe, import or
        // monitor sync must not push them past the UI read deadline.
        if matches!(
            operation,
            Operation::Snapshot
                | Operation::AgentCatalog
                | Operation::AgentConnect { .. }
                | Operation::AgentKey
                | Operation::AgentKeyStatus { .. }
                | Operation::AgentKeyValue { .. }
                | Operation::AgentKeys { .. }
                | Operation::KimiAccounts
                | Operation::HermesModel
                | Operation::KimiLoginStatus { .. }
                | Operation::Chains { .. }
                | Operation::LaunchAccount { .. }
                | Operation::Status
                | Operation::MonitorStatus
                | Operation::ResolveProject { .. }
                | Operation::Sessions { .. }
                | Operation::CurrentAccounts
                | Operation::LoginStatus { .. }
                | Operation::PendingLogins
                | Operation::Backups
                | Operation::ProjectSuggestions
                | Operation::ProjectBindings
                | Operation::RepositoryMap { .. }
                | Operation::Statistics { .. }
                | Operation::AgentsOverview
                | Operation::OpenrouterModels
                // Hand-overs are files: listing reads them, and taking one is claimed by an
                // exclusive create (`handovers::take`). Behind the owner transaction a console's
                // read waited out renewals and syncs for up to 366 s (Fabric Dashboards FD-71).
                | Operation::Handovers
                | Operation::HandoverTake { .. }
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
    /// Moves when the open project suggestions may have changed (SB-117): the app reads the
    /// list, updates the tray count and notifies only then.
    pub fn suggestions_seq(&self) -> u64 {
        self.suggestions_seq
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    fn suggestions_changed(&self) {
        self.suggestions_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    /// SB-118: keeps each project folder's Claude Code route in step with the projects, their
    /// pools' Claude accounts and the proxy's address. Runs only when that fingerprint moved, so
    /// an idle pass reads a few stamps and writes nothing.
    pub(crate) async fn bindings_pass(&self) {
        let Ok(snapshot) = self.store.snapshot() else {
            return;
        };
        // SB-143: the proxy's Codex folder map follows the projects and their Codex accounts.
        self.proxy
            .set_codex_folders(codex_route::folders(&snapshot));
        let address = self.proxy.address().to_string();
        let key = {
            let Ok(mut repos) = self.bindings_repos.lock() else {
                return;
            };
            let mut key = bindings::key(&snapshot, &address, &mut repos);
            // A rotated capability is a change the folders must get.
            {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                key.hash(&mut h);
                let mut salts: Vec<(String, String)> =
                    read_pool_salts(&self.root).into_iter().collect();
                salts.sort();
                salts.hash(&mut h);
                key = h.finish();
            }
            key
        };
        if self.bindings_key.lock().ok().and_then(|k| *k) == Some(key) {
            return;
        }
        let _bindings = self.bindings_lock.lock().await;
        let Ok(snapshot) = self.store.snapshot() else {
            return;
        };
        let salts = read_pool_salts(&self.root);
        let synced = blocking::run(|| {
            bindings::sync_salted(&self.root, &snapshot, &address, self.proxy.token(), &salts)
        });
        match synced {
            Ok(report) => {
                self.proxy.set_folder_tokens(report.tokens.clone());
                self.proxy.set_stat_folders(stat_folders(&snapshot));
                // SB-155: a sync that changed nothing is not an event (178 a day on 2026-10-20).
                let changed = report.bound
                    + report.removed
                    + report.skipped.len()
                    + report.user_values_present.len()
                    > 0;
                if changed {
                    oplog::event(
                        "claude_binding",
                        &[
                            ("bound", oplog::Field::Number(report.bound as i64)),
                            ("removed", oplog::Field::Number(report.removed as i64)),
                            ("skipped", oplog::Field::Number(report.skipped.len() as i64)),
                            (
                                "user_values",
                                oplog::Field::Number(report.user_values_present.len() as i64),
                            ),
                        ],
                    );
                }
                if let Ok(mut k) = self.bindings_key.lock() {
                    *k = Some(key);
                }
            }
            Err(_) => {
                // The record is unreadable, but the proxy still follows the project folders.
                self.proxy.set_folder_tokens(bindings::folder_tokens(
                    &bindings::desired(&snapshot),
                    self.proxy.token(),
                    &salts,
                ));
                self.proxy.set_stat_folders(stat_folders(&snapshot));
                oplog::event(
                    "claude_binding",
                    &[("result", oplog::Field::Code("unreadable"))],
                )
            }
        }
    }
    /// `Operation::ContinueToConsole`: the hand-over the automatic fallback makes with "Open
    /// hand-overs in Fabric" on, for one workflow and account the person names.
    async fn continue_to_console(
        &self,
        workflow_id: String,
        id: String,
        working_directory: PathBuf,
    ) -> Result<Value, String> {
        if !handovers::enabled(&self.root) {
            return Err(handovers::OFF.into());
        }
        let answer = Box::pin(self.execute(Operation::Continue {
            workflow_id: workflow_id.clone(),
            id: id.clone(),
            mode: "isolated".into(),
            working_directory: working_directory.clone(),
            in_place: true,
        }))
        .await?;
        let snapshot = self.store.snapshot().ok();
        let handover = fallback::queue_handover(
            &self.root,
            snapshot.as_ref(),
            workflow_id,
            // The account that continues it, as `Continue` resolved it (the default agent's
            // account when none was named).
            answer["account"].as_str().unwrap_or(id.as_str()),
            working_directory,
            &answer,
            monitor::now(),
        )?;
        // Not queued means the queue could not take it and Terminal opened it instead.
        let queued = handovers::pending(&self.root)
            .iter()
            .any(|h| h.id == handover.id);
        Ok(
            json!({"handover": handover.id, "queued": queued, "workflow": handover.workflow,
            "agent": handover.agent, "folder": handover.dir, "offeredUntil": handover.offered_until}),
        )
    }
    /// SB-121: found folders join their projects at once. One that cannot (a refused admission)
    /// stays pending and is tried again later — the person is never asked (SB-131). Runs under the
    /// owner transaction.
    fn add_found(&self, found: Vec<suggest::Suggestion>, time: i64) {
        if found.is_empty() {
            return;
        }
        let mut added = 0;
        for s in &found {
            match add_suggested_folder(&self.store, &self.root, s, time) {
                Ok(_) => added += 1,
                Err(_) => {
                    if self.first_time(&format!("add:{}", s.folder)) {
                        oplog::event(
                            "project_suggestion",
                            &[("add", oplog::Field::Code("refused"))],
                        );
                    }
                }
            }
        }
        // A retry that adds nothing changes nothing the app shows: no log line, no refresh.
        if added > 0 {
            oplog::event(
                "project_suggestion",
                &[
                    ("found", oplog::Field::Number(found.len() as i64)),
                    ("added", oplog::Field::Number(added)),
                ],
            );
            self.suggestions_changed();
        }
    }
    /// Whether `key` is logged for the first time this run (SB-131).
    fn first_time(&self, key: &str) -> bool {
        self.told
            .lock()
            .map(|mut told| told.len() < 4096 && told.insert(key.to_owned()))
            .unwrap_or(false)
    }
    /// The background half of SB-117/SB-121/SB-131, under the owner transaction like
    /// any other write: folders found next to a project's, repositories its agents changed, an
    /// agent's unassigned Git root, folders that could not be added yet, and deleted folders.
    /// The owner transaction for a background step, logged when held long.
    pub(crate) async fn hold(&self, step: &'static str) -> Held<'_> {
        Held {
            _guard: self.mutations.lock().await,
            step,
            since: std::time::Instant::now(),
        }
    }
    #[cfg(test)]
    pub(crate) async fn suggest_pass(&self, time: i64) {
        self.suggest_pass_with_stop(time, None).await;
    }
    async fn suggest_pass_with_stop(
        &self,
        time: i64,
        stop: Option<&tokio::sync::watch::Receiver<bool>>,
    ) {
        let stopped = || stop.as_ref().is_some_and(|s| *s.borrow());
        if stopped() {
            return;
        }
        let Ok(snapshot) = self.store.snapshot() else {
            return;
        };
        // Folders that could not be added (and those an older version left as questions) are
        // tried again every RETRY_PENDING, never asked about.
        let retry_at = self
            .suggest_retry_at
            .load(std::sync::atomic::Ordering::Relaxed);
        // A clock set back far is a fresh start, not a long wait.
        if time >= retry_at || time < retry_at - 2 * RETRY_PENDING {
            self.suggest_retry_at
                .store(time + RETRY_PENDING, std::sync::atomic::Ordering::Relaxed);
            if let Ok(waiting) = suggest::pending(&self.root, &snapshot) {
                if !waiting.is_empty() {
                    let _transaction = self.hold("suggest_retry").await;
                    self.add_found(waiting, time);
                }
            }
        }
        self.worked_pass_with_stop(&snapshot, time, stop).await;
        if stopped() {
            return;
        }
        self.vanish_pass(&snapshot, time).await;
        if stopped() {
            return;
        }
        // Listing folders writes nothing: it runs outside the owner transaction, which only
        // the recording below takes, and only when something changed.
        let candidates = match self.suggest.lock() {
            Ok(mut scanner) => blocking::run(|| scanner.collect(&snapshot, time)),
            Err(_) => return,
        };
        if candidates.is_empty() || stopped() {
            return;
        }
        // Reading the candidates' and the projects' Git files is file work too: outside the
        // transaction, which held every control request for up to 57 s while it ran under it on a
        // swapping machine (2026-10-19).
        let assessed = blocking::run(|| suggest::assess_all(&snapshot, candidates));
        if stopped() {
            return;
        }
        let _transaction = self.hold("suggest_found").await;
        let Ok(fresh) = self.store.snapshot() else {
            return;
        };
        // Projects changed meanwhile: what was decided may be stale. The scanner starts over on
        // the new projects at the next pass (`collect`), so nothing found is lost.
        if suggest::projects_key(&fresh) != suggest::projects_key(&snapshot) {
            return;
        }
        blocking::run(|| {
            let found = match self.suggest.lock() {
                Ok(mut scanner) => scanner.settle_assessed(&self.root, assessed, time),
                Err(_) => return,
            };
            self.add_found(found, time);
        });
    }
    /// SB-131: the repositories agents of a project changed since the last pass join it. The
    /// project is the one the session works in (its `cwd`); a repository changed by agents of two
    /// projects in one pass joins neither.
    async fn worked_pass_with_stop(
        &self,
        snapshot: &switchboard_core::Snapshot,
        time: i64,
        stop: Option<&tokio::sync::watch::Receiver<bool>>,
    ) {
        if stop.as_ref().is_some_and(|s| *s.borrow()) {
            return;
        }
        // A synthetic owner reads no transcript of the machine it runs on. Listing and reading
        // transcripts is file work: it runs as a blocking section, so the control listener and the
        // proxy never wait behind it (a pass that read ~6,700 subagent entries on a worker held a
        // control request for 12 s, 2026-10-17).
        let files = blocking::run(|| self.work_files(time));
        self.worked_from_with_stop(snapshot, &files, time, stop)
            .await;
    }
    /// The transcripts the work scan reads: the sessions' and their subagents', the subagent
    /// directories listed again at most every SUBAGENTS_EVERY.
    fn work_files(&self, time: i64) -> Vec<PathBuf> {
        let sessions = self.transcripts(time);
        let Ok(mut cached) = self.subagent_list.lock() else {
            return sessions;
        };
        let fresh = cached
            .as_ref()
            .is_some_and(|(at, _)| time >= *at && time - at < SUBAGENTS_EVERY);
        if !fresh {
            let all = with_subagents(&sessions, time);
            *cached = Some((time, all[sessions.len()..].to_vec()));
        }
        let mut files = sessions;
        if let Some((_, subagents)) = cached.as_ref() {
            files.extend(subagents.iter().cloned());
        }
        // SB-132: Codex sessions' rollouts, read by the same scan.
        files.extend((self.native.rollouts)(time));
        files
    }
    /// The recent Claude Code transcripts, walked at most once per background pass (SB-131).
    pub(crate) fn transcripts(&self, time: i64) -> Vec<PathBuf> {
        if let Ok(list) = self.transcript_list.lock() {
            if let Some((at, files)) = list.as_ref() {
                if (time - at).abs() < 20 {
                    return files.clone();
                }
            }
        }
        let files = (self.native.transcripts)(time);
        if let Ok(mut list) = self.transcript_list.lock() {
            *list = Some((time, files.clone()));
        }
        files
    }
    #[cfg(test)]
    pub(crate) async fn worked_from(
        &self,
        snapshot: &switchboard_core::Snapshot,
        files: &[PathBuf],
        time: i64,
    ) {
        self.worked_from_with_stop(snapshot, files, time, None)
            .await;
    }
    async fn worked_from_with_stop(
        &self,
        snapshot: &switchboard_core::Snapshot,
        files: &[PathBuf],
        time: i64,
        stop: Option<&tokio::sync::watch::Receiver<bool>>,
    ) {
        let evidence = match self.activity.lock() {
            Ok(mut activity) => blocking::run(|| {
                activity.collect_while(files, || stop.as_ref().is_some_and(|s| *s.borrow()))
            }),
            Err(_) => return,
        };
        let Some(evidence) = evidence else {
            return;
        };
        if evidence.is_empty() {
            return;
        }
        let key = suggest::projects_key(snapshot);
        let mut claims: HashMap<PathBuf, HashSet<String>> = HashMap::new();
        // Folders of one project another project's agent works in: (folder, its project).
        let mut contested: HashMap<String, String> = HashMap::new();
        {
            let Ok(mut worked) = self.worked.lock() else {
                return;
            };
            if worked.0 != key {
                *worked = (key, HashSet::new());
            }
            for w in &evidence {
                // One spelling per folder: `/tmp/x` and `/private/tmp/x`, a symlinked checkout.
                let w = activity::Worked {
                    cwd: canonical_near(&w.cwd),
                    target: canonical_near(&w.target),
                };
                // SB-146: an agent's repository in no group stays in the default group; it is
                // listed under the repositories on disk, never made a group of its own.
                let Some(project) = snapshot.project_for(&w.cwd) else {
                    continue;
                };
                // Work inside a project folder is that project's already — unless another
                // project's agent did it: a folder that joined by work is then shared (below).
                if let Some(owner) = snapshot.project_for(&w.target) {
                    if owner.pool != project.pool {
                        if let Some(folder) = owner
                            .folders
                            .iter()
                            .filter(|f| w.target.starts_with(f))
                            .max_by_key(|f| f.len())
                        {
                            contested.insert(folder.clone(), owner.pool.clone());
                        }
                    }
                    continue;
                }
                let Some(root) = suggest::repo_root(&w.target) else {
                    continue;
                };
                if worked.1.contains(&root) {
                    continue;
                }
                claims.entry(root).or_default().insert(project.pool.clone());
            }
        }
        if claims.is_empty() && contested.is_empty() {
            return;
        }
        let _transaction = self.hold("worked").await;
        // Recording reads Git files and writes the list: a blocking section (2026-10-19: up to
        // 10 s on a swapping machine).
        blocking::run(|| {
            let Ok(snapshot) = self.store.snapshot() else {
                return;
            };
            let mut found = Vec::new();
            let mut settled = Vec::new();
            let mut changed = self.unshare(&snapshot, &contested, time);
            for (root, pools) in claims {
                if pools.len() != 1 {
                    // Two projects' work: it joins neither, now or later.
                    oplog::event(
                        "project_suggestion",
                        &[("worked", oplog::Field::Code("two_projects"))],
                    );
                    if let Ok(text) = root
                        .canonicalize()
                        .map(|r| r.to_string_lossy().into_owned())
                    {
                        if suggest::mark_shared(&self.root, &text, time).is_ok() {
                            changed = true;
                        }
                    }
                    settled.push(root);
                    continue;
                }
                let pool = pools.into_iter().next().unwrap_or_default();
                match suggest::record_worked(&self.root, &snapshot, &root, &pool, time) {
                    Ok(suggest::Outcome::New(s)) => {
                        found.push(s);
                        settled.push(root);
                    }
                    // A failed recording is tried again with the next evidence.
                    Err(_) => oplog::event(
                        "project_suggestion",
                        &[("worked", oplog::Field::Code("unrecorded"))],
                    ),
                    Ok(_) => settled.push(root),
                }
            }
            if let Ok(mut worked) = self.worked.lock() {
                worked.1.extend(settled);
            }
            if found.is_empty() && changed {
                self.suggestions_changed();
            }
            self.add_found(found, time);
        });
    }
    /// SB-131: a folder that joined its project because that project's agents worked in it, and
    /// that another project's agents now work in too, is a shared repository (an organisation
    /// wiki, a common docs repository): it leaves the project and is never added by work again.
    /// A folder the person added, or one found as a worktree or clone, is never touched. Under
    /// the owner transaction. Answers whether anything changed.
    fn unshare(
        &self,
        snapshot: &switchboard_core::Snapshot,
        contested: &HashMap<String, String>,
        time: i64,
    ) -> bool {
        if contested.is_empty() {
            return false;
        }
        let Ok(rows) = suggest::read(&self.root) else {
            return false;
        };
        let mut changed = false;
        for (folder, pool) in contested {
            let joined_by_work = rows.iter().any(|r| {
                &r.folder == folder
                    && &r.pool == pool
                    && r.state == suggest::State::Added
                    && r.reason == suggest::Reason::Worked
            });
            if !joined_by_work {
                continue;
            }
            let Some(project) = snapshot.projects.iter().find(|p| &p.pool == pool) else {
                continue;
            };
            let kept: Vec<PathBuf> = project
                .folders
                .iter()
                .filter(|f| *f != folder)
                .map(PathBuf::from)
                .collect();
            if kept.is_empty() || kept.len() == project.folders.len() {
                continue;
            }
            let accounts: Vec<String> = snapshot
                .accounts
                .iter()
                .filter(|a| &a.pool == pool)
                .map(|a| a.id.clone())
                .collect();
            if self
                .store
                .save_project(Some(pool), &project.name, &kept, &accounts)
                .is_ok()
                && suggest::mark_shared(&self.root, folder, time).is_ok()
            {
                oplog::event(
                    "project_suggestion",
                    &[("worked", oplog::Field::Code("shared"))],
                );
                changed = true;
            }
        }
        changed
    }
    /// SB-131: a project folder deleted from disk leaves its project once it is still missing
    /// VANISH_CONFIRM later. A folder whose drive is not mounted, or whose whole tree up to the
    /// filesystem root is gone, is not deleted as far as Switchboard can tell, and stays; a
    /// project keeps its last folder.
    async fn vanish_pass(&self, snapshot: &switchboard_core::Snapshot, time: i64) {
        let at = self.vanish_at.load(std::sync::atomic::Ordering::Relaxed);
        if time < at && time >= at - 2 * VANISH_LOOK {
            return;
        }
        self.vanish_at
            .store(time + VANISH_LOOK, std::sync::atomic::Ordering::Relaxed);
        let mut confirmed: Vec<(String, String)> = Vec::new();
        // A stat of every project folder (~200 here): file work, in a blocking section.
        blocking::run(|| {
            let Ok(mut vanish) = self.vanished.lock() else {
                return;
            };
            let mut still: HashMap<String, i64> = HashMap::new();
            let mut devices: HashMap<String, u64> = HashMap::new();
            for project in &snapshot.projects {
                for folder in &project.folders {
                    let path = Path::new(folder);
                    // SB-152: a removed checkout's leftover folder counts as deleted too.
                    let husk = projects::husk(path);
                    if !husk {
                        if let Some(dev) = projects::device(path) {
                            devices.insert(folder.clone(), dev);
                            continue;
                        }
                    }
                    let known = vanish.devices.get(folder).copied();
                    if !husk && !projects::deleted(path, known) {
                        continue;
                    }
                    if let Some(dev) = known {
                        devices.insert(folder.clone(), dev);
                    }
                    let since = vanish
                        .missing
                        .get(folder)
                        .copied()
                        .filter(|since| *since <= time)
                        .unwrap_or(time);
                    if time - since >= VANISH_CONFIRM {
                        confirmed.push((project.pool.clone(), folder.clone()));
                    } else {
                        still.insert(folder.clone(), since);
                    }
                }
            }
            vanish.missing = still;
            vanish.devices = devices;
        });
        if confirmed.is_empty() {
            return;
        }
        let _transaction = self.hold("vanish").await;
        let Ok(snapshot) = self.store.snapshot() else {
            return;
        };
        // Saving the projects writes the store: a blocking section, so the async worker and the
        // tasks queued on it do not wait with it (12 s on a swapping machine, 2026-10-19).
        blocking::run(|| {
            let mut removed = 0;
            for project in &snapshot.projects {
                let gone: Vec<String> = confirmed
                    .iter()
                    .filter(|(pool, f)| pool == &project.pool && project.folders.contains(f))
                    .map(|(_, f)| f.clone())
                    .collect();
                if gone.is_empty() {
                    continue;
                }
                let mut kept: Vec<PathBuf> = project
                    .folders
                    .iter()
                    .filter(|f| !gone.contains(f))
                    .map(PathBuf::from)
                    .collect();
                let mut gone = gone;
                if kept.is_empty() {
                    // A project keeps its last folder: it has to have one.
                    if self.first_time(&format!("last:{}", project.pool)) {
                        oplog::event(
                            "project_folder_gone",
                            &[("outcome", oplog::Field::Code("kept_last"))],
                        );
                    }
                    let Some(last) = gone.pop() else {
                        continue;
                    };
                    kept.push(PathBuf::from(last));
                    if gone.is_empty() {
                        continue;
                    }
                }
                let accounts: Vec<String> = snapshot
                    .accounts
                    .iter()
                    .filter(|a| a.pool == project.pool)
                    .map(|a| a.id.clone())
                    .collect();
                match self
                    .store
                    .save_project(Some(&project.pool), &project.name, &kept, &accounts)
                {
                    Ok(_) => {
                        removed += gone.len();
                        if suggest::mark_gone(&self.root, &project.pool, &gone, time).is_err() {
                            oplog::event(
                                "project_suggestion",
                                &[("remember", oplog::Field::Code("failed"))],
                            );
                        }
                    }
                    Err(_) => oplog::event(
                        "project_folder_gone",
                        &[("outcome", oplog::Field::Code("refused"))],
                    ),
                }
            }
            if removed > 0 {
                oplog::event(
                    "project_folder_gone",
                    &[("removed", oplog::Field::Number(removed as i64))],
                );
                self.suggestions_changed();
            }
        });
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
        if logins.len() >= launch::MAX_LOGINS {
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
        let receipt = |account: &Account, cleaned: bool, again: bool| {
            let mut value = json!(account);
            value["login_cleanup"] = json!(if cleaned { "done" } else { "pending" });
            value["signed_in_again"] = json!(again);
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
                .find(|(done, _, _)| done == id)
                .map(|(_, account, again)| (account.clone(), *again))
                .ok_or("Sign-in not found. Start again.")?;
            return Ok(receipt(&account.0, self.retry_cleanup_of(id), account.1));
        };
        let account = if let Some(account) = &login.saved {
            account.clone()
        } else {
            let captured = launch::capture_login_profile(login)?;
            let (account, again) = file_sign_in(
                &self.store,
                login.provider,
                &login.label,
                &login.pool,
                captured,
            )?;
            if again {
                login.signed_in_again = true;
            }
            login.saved = Some(account.clone());
            account
        };
        let again = login.signed_in_again;
        // SB-103: the account is saved; a restart must never offer this sign-in again (and
        // finishing it twice would write its first credential over a newer one).
        launch::forget_login_journal(login);
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
            completed.retain(|(done, _, _)| done != id);
            completed.push_back((id.to_owned(), account.clone(), again));
            while completed.len() > COMPLETED_LOGINS {
                completed.pop_front();
            }
        }
        self.invalidate_current();
        Ok(receipt(&account, cleaned, again))
    }
    /// Tries once more to remove a saved sign-in's staging home. True when nothing is left.
    fn retry_cleanup_of(&self, id: &str) -> bool {
        let Ok(mut stale) = self.stale_logins.lock() else {
            return false;
        };
        stale.retain(|login| login.id != id || launch::clean_login(login).is_err());
        !stale.iter().any(|login| login.id == id)
    }
    /// The sign-ins in the registry, newest last: enough for a window started after the owner
    /// (or a restarted owner) to show its banner and finish them.
    fn pending_logins(&self) -> Value {
        let logins = self.logins.lock().map(|logins| {
            // SB-103: oldest first, so a window restoring one banner takes the newest.
            let mut ordered: Vec<_> = logins.values().collect();
            ordered.sort_by_key(|login| launch::login_started(login));
            ordered
                .into_iter()
                .map(|login| {
                    json!({
                        "login_id": login.id,
                        "provider": login.provider.as_str(),
                        "label": login.label,
                        "pool": login.pool,
                        "state": launch::login_state(login),
                    })
                })
                .collect::<Vec<_>>()
        });
        json!({ "logins": logins.unwrap_or_default() })
    }
    /// Takes back the sign-ins an earlier owner of this data folder left open (SB-65).
    fn adopt_logins(&self) -> usize {
        let Ok(mut logins) = self.logins.lock() else {
            return 0;
        };
        let mut adopted = 0;
        for login in launch::adopt_logins(&self.root) {
            if logins.len() >= launch::MAX_LOGINS {
                break;
            }
            logins.insert(login.id.clone(), login);
            adopted += 1;
        }
        adopted
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
                .is_ok_and(|completed| completed.iter().any(|(done, _, _)| done == id));
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
    /// A new data folder with backups this machine can open beside it: the newest is restored at
    /// once, so a reinstall gets its accounts, projects, rules, selections and settings back
    /// without a step. Never on a store that existed before this start — a person who removed
    /// their accounts keeps them removed.
    fn restore_at_start(&self) {
        let (Some(dir), Some(key)) = (self.backup_folder(), self.backup_key()) else {
            return;
        };
        let Ok(snapshot) = self.store.snapshot() else {
            return;
        };
        if !snapshot.accounts.is_empty() {
            return;
        }
        let Some(newest) = switchboard_core::backup::list(&dir, &self.store, key.as_ref())
            .into_iter()
            .filter(|b| b.openable && b.accounts > 0)
            .max_by_key(|b| b.created_at)
        else {
            return;
        };
        match switchboard_core::backup::restore(&self.store, &dir, &newest.file, key.as_ref()) {
            Ok(restored) => {
                oplog::event(
                    "backup_restored_at_start",
                    &[("accounts", oplog::Field::Number(restored.added as i64))],
                );
                if let Ok(mut slot) = self.restored_at_start.lock() {
                    *slot = Some(
                        json!({"file": newest.file, "created_at": newest.created_at, "restored": restored}),
                    );
                }
            }
            Err(_) => oplog::event(
                "backup_restored_at_start",
                &[("outcome", oplog::Field::Code("failed"))],
            ),
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
    pub(crate) fn current_accounts(&self) -> Result<Value, String> {
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
/// The pools whose folder capability was rotated, and their salts (0600). Losing it reverts those
/// pools to the unsalted capability, as a new session token would replace every capability.
const POOL_SALTS_FILE: &str = "pool-salts.json";
fn read_pool_salts(root: &Path) -> HashMap<String, String> {
    switchboard_core::private_fs::read_private(&root.join(POOL_SALTS_FILE), 64 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
fn write_pool_salts(root: &Path, salts: &HashMap<String, String>) -> Result<(), String> {
    let bytes = serde_json::to_vec(salts).map_err(|_| "Could not save the new capability.")?;
    switchboard_core::private_fs::private_write(&root.join(POOL_SALTS_FILE), &bytes)
}
/// SB-133: when the folders' own capabilities became the only ones accepted. Recorded at the first
/// start of a version that writes them, so a later restart does not move it.
const CAPABILITY_MIGRATION_FILE: &str = "capability-migration.json";
/// SB-133: how long a pool's capability still stands for its folders after the folders got their
/// own: sessions started before keep the old value until they restart.
const LEGACY_CAPABILITY_SECONDS: i64 = 7 * 86_400;
fn legacy_until(root: &Path, now: i64) -> i64 {
    let path = root.join(CAPABILITY_MIGRATION_FILE);
    let saved = switchboard_core::private_fs::read_private(&path, 4096)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v["legacy_until"].as_i64());
    if let Some(until) = saved {
        return until;
    }
    let until = now + LEGACY_CAPABILITY_SECONDS;
    let bytes = serde_json::to_vec(&json!({"legacy_until": until})).unwrap_or_default();
    if switchboard_core::private_fs::private_write(&path, &bytes).is_err() {
        oplog::event(
            "capability_migration",
            &[("result", oplog::Field::Code("unsaved"))],
        );
    }
    until
}
/// A fresh salt: two random UUIDs, hashed.
fn new_salt() -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
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
    let proxy_store = store.clone();
    let record = load_proxy_record(root);
    let proxy = ProxyHandle::start_on(
        store,
        record.as_ref().map(|r| r.port),
        record.as_ref().map(|r| r.token.clone()),
    )
    .await?;
    let salts = read_pool_salts(root);
    proxy.set_pool_salts(salts.clone());
    // SB-133: the folders' own capabilities from the first request on, and the window in which a
    // pool's capability (what 0.6.39 and earlier wrote) still stands for its folders.
    proxy.set_legacy_until(legacy_until(root, monitor::now()));
    if let Ok(snapshot) = proxy_store.snapshot() {
        proxy.set_folder_tokens(bindings::folder_tokens(
            &bindings::desired(&snapshot),
            proxy.token(),
            &salts,
        ));
        proxy.set_stat_folders(stat_folders(&snapshot));
        proxy.set_codex_folders(codex_route::folders(&snapshot));
    }
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
        // A data folder created by this start: a new install, or one after `switchboard uninstall`.
        let fresh = !root.join("accounts.json").exists();
        let runtime = Runtime::open_with(root, vault, sources).await?;
        let control = control::ControlHandle::start(runtime.clone()).await?;
        // Only the real data folder is backed up: a `--data-dir` scratch store must never
        // write into the operator's backup folder with the operator's key.
        let default_store = default_root().ok().as_deref() == Some(runtime.root.as_path());
        if native_sources && default_store {
            if let Some(dir) = backup_dir() {
                runtime.enable_backups(switchboard_core::backup::platform_key(&dir), Some(dir));
            }
            if fresh {
                runtime.restore_at_start();
            }
        }
        // One renewer per lineage: only the owner of the real data folder spends refresh
        // tokens. A `--data-dir` store holding the same accounts would be a second renewer.
        if native_sources && !default_store {
            runtime.refresh.set_grants(false);
        }
        // SB-65: sign-ins an earlier owner left open come back, so the window can finish them.
        let adopted = runtime.adopt_logins();
        if adopted > 0 {
            oplog::event(
                "login_adopted",
                &[("count", oplog::Field::Number(adopted as i64))],
            );
        }
        // SB-87: sign-in homes an earlier owner forgot; off the start path (it may ask `ps` and
        // Keychain, each bounded), and only the real owner of its data folder does it.
        if native_sources {
            let root = runtime.root.clone();
            let store = runtime.store.clone();
            tokio::task::spawn_blocking(move || {
                // SB-99: Kimi sign-ins that ended without signing in, the same way.
                let kimi = kimi::sweep_logins(&root, &store);
                if kimi > 0 {
                    oplog::event(
                        "kimi_login_leftovers",
                        &[("removed", oplog::Field::Number(kimi as i64))],
                    );
                }
                let sweep = launch::sweep_logins(&root);
                if sweep != launch::Sweep::default() {
                    oplog::event(
                        "login_leftovers",
                        &[
                            ("removed", oplog::Field::Number(sweep.removed as i64)),
                            ("running", oplog::Field::Number(sweep.running as i64)),
                            ("finishable", oplog::Field::Number(sweep.finishable as i64)),
                            ("kept", oplog::Field::Number(sweep.kept as i64)),
                        ],
                    );
                }
            });
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
        // SB-143: no Codex should be pointed at a proxy that is gone.
        runtime.codex_route_sync(false);
        runtime.statistics.absorb(runtime.proxy.take_statistics());
        runtime.statistics.save(&runtime.root, monitor::now(), true);
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

/// A suggestion as the window, the CLI and the tray show it (SB-117).
fn suggestion_view(snapshot: &switchboard_core::Snapshot, s: &suggest::Suggestion) -> Value {
    let name = Path::new(&s.folder)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| s.folder.clone());
    let project = snapshot
        .projects
        .iter()
        .find(|p| p.pool == s.pool)
        .map(|p| p.name.clone());
    json!({"folder": s.folder, "name": name, "pool": s.pool, "project": project,
        "reason": s.reason, "repository": s.repository, "first_seen": s.first_seen})
}
/// Adds a found folder to its project (SB-117 Add; SB-121 does it unasked) with the same admission
/// as a folder added by hand, then remembers it as added. Answers the project's view.
fn add_suggested_folder(
    store: &Store,
    root: &Path,
    suggestion: &suggest::Suggestion,
    now: i64,
) -> Result<Value, String> {
    if !Path::new(&suggestion.folder).is_dir() {
        return Err("This folder is gone. Read the suggestions again.".into());
    }
    let admitted = projects::project_dir(root, Path::new(&suggestion.folder))?;
    let snapshot = store.snapshot()?;
    let project = snapshot
        .projects
        .iter()
        .find(|p| p.pool == suggestion.pool)
        .ok_or("Project not found.")?;
    let mut folders: Vec<PathBuf> = project.folders.iter().map(PathBuf::from).collect();
    folders.push(admitted);
    let accounts: Vec<String> = snapshot
        .accounts
        .iter()
        .filter(|a| a.pool == project.pool)
        .map(|a| a.id.clone())
        .collect();
    let saved = store.save_project(Some(&project.pool), &project.name, &folders, &accounts)?;
    // The folder is in its project now; a list that could not be rewritten only loses the memory
    // that it was added (a later removal by the person might then be undone once).
    if suggest::mark_added(root, &suggestion.folder, now).is_err() {
        oplog::event(
            "project_suggestion",
            &[("remember", oplog::Field::Code("failed"))],
        );
    }
    Ok(projects::project_view(&store.snapshot()?, &saved))
}

/// SB-147: every group folder with its group, and the worktrees the bindings route, for the
/// proxy's statistics (`ProxyHandle::set_stat_folders`).
fn stat_folders(snapshot: &switchboard_core::Snapshot) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = snapshot
        .projects
        .iter()
        .flat_map(|p| p.folders.iter().map(move |f| (f.clone(), p.pool.clone())))
        .collect();
    out.extend(
        bindings::desired(snapshot)
            .into_iter()
            .filter_map(|(f, pool)| f.to_str().map(|t| (t.to_owned(), pool))),
    );
    out.sort();
    out.dedup();
    out
}

/// How long a folder added by Switchboard stays in `ProjectSuggestions`' `added`, for the app's
/// one notification (SB-121).
const ADDED_SHOWN: i64 = 24 * 3600;
/// How often the subagent transcript directories are listed again: a long session keeps
/// hundreds of finished subagents there, and each listing stats every one.
const SUBAGENTS_EVERY: i64 = 120;
/// SB-131: the recent sessions' transcripts and their subagents' (`<session>/subagents/*.jsonl`,
/// where implementer subagents record their edits), each written within the limit scan's window.
fn with_subagents(files: &[PathBuf], time: i64) -> Vec<PathBuf> {
    let mut all = files.to_vec();
    for file in files {
        let Ok(entries) = std::fs::read_dir(file.with_extension("").join("subagents")) else {
            continue;
        };
        // Only recent ones count; a long session has hundreds of finished subagents.
        let recent = entries
            .flatten()
            .filter(|entry| {
                entry.path().extension().is_some_and(|e| e == "jsonl")
                    && entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .is_some_and(|m| time - (m.as_secs() as i64) <= limits::WINDOW_SECONDS)
            })
            .map(|entry| entry.path())
            .take(64);
        all.extend(recent);
    }
    all
}
/// SB-134: the whole save a project delta amounts to. A folder or account named to be taken out
/// that the project does not hold is refused (a typo never passes as done); one named to be added
/// that it already holds is no change. The current folders are the save's base, so only the
/// folders named here count as taken out by the person.
fn project_edit(
    snapshot: &switchboard_core::Snapshot,
    pool: &str,
    name: Option<String>,
    add_accounts: &[String],
    remove_accounts: &[String],
    add_folders: &[PathBuf],
    remove_folders: &[PathBuf],
) -> Result<Operation, String> {
    let project = snapshot
        .projects
        .iter()
        .find(|p| p.pool == pool)
        .ok_or("Project not found.")?;
    let held: Vec<PathBuf> = project.folders.iter().map(PathBuf::from).collect();
    let same = |a: &Path, b: &Path| a == b || canonical_near(a) == canonical_near(b);
    for f in remove_folders {
        if !held.iter().any(|h| same(h, f)) {
            return Err(PROJECT_EDIT_NO_FOLDER.into());
        }
    }
    let mut folders: Vec<PathBuf> = held
        .iter()
        .filter(|h| !remove_folders.iter().any(|f| same(h, f)))
        .cloned()
        .collect();
    for f in add_folders {
        if !folders.iter().any(|h| same(h, f)) {
            folders.push(f.clone());
        }
    }
    let mut accounts: Vec<String> = snapshot
        .accounts
        .iter()
        .filter(|a| a.pool == project.pool)
        .map(|a| a.id.clone())
        .collect();
    for id in remove_accounts {
        if !accounts.contains(id) {
            return Err(PROJECT_EDIT_NO_ACCOUNT.into());
        }
    }
    accounts.retain(|id| !remove_accounts.contains(id));
    for id in add_accounts {
        if !snapshot.accounts.iter().any(|a| &a.id == id) {
            return Err("Account not found.".into());
        }
        if !accounts.contains(id) {
            accounts.push(id.clone());
        }
    }
    Ok(Operation::SaveProject {
        pool: Some(project.pool.clone()),
        name: name.unwrap_or_else(|| project.name.clone()),
        folders,
        account_ids: accounts,
        base_folders: Some(held),
    })
}
/// The account a stopped session in a project's folder was using: the pool's selected Claude
/// account, else its first enabled Claude OAuth account.
fn project_stop_account(snapshot: &switchboard_core::Snapshot, pool: &str) -> Option<String> {
    let routed = snapshot
        .routes
        .get(&format!("claude:{pool}"))
        .filter(|id| snapshot.accounts.iter().any(|a| &a.id == *id));
    routed.cloned().or_else(|| {
        snapshot
            .accounts
            .iter()
            .find(|a| {
                a.pool == pool
                    && a.provider == Provider::Claude
                    && a.enabled
                    && a.kind == AuthKind::OAuth
            })
            .map(|a| a.id.clone())
    })
}
/// SB-122: an account moves only into a project's pool or back to `default`.
pub const MOVE_ACCOUNT_NO_PROJECT: &str =
    "No project uses that pool. Choose a project, or Default.";
/// SB-135: a root moves only into a project.
/// SB-146: `from` names a group that does not hold the repository.
pub const MOVE_ROOT_NOT_IN_GROUP: &str =
    "That group does not hold this repository. Choose one of the groups it is in.";
pub const MOVE_ROOT_NO_PROJECT: &str = "No project uses that pool. Choose a project.";
/// SB-135: a project keeps at least one folder.
pub const MOVE_ROOT_LAST: &str = "This is the project's only repository. Add another folder to the project first, or delete the project.";
/// SB-135: a repository inside a project folder belongs to that folder.
pub const MOVE_ROOT_INSIDE: &str =
    "This repository is inside a folder of a project. Move that folder instead.";
/// Refusals of a project delta (SB-134).
pub const PROJECT_EDIT_NO_FOLDER: &str =
    "This project does not hold that folder. List the project's folders and name one of them.";
pub const PROJECT_EDIT_NO_ACCOUNT: &str =
    "This account is not in the project. List the project's accounts and name one of them.";
/// A path with its nearest existing ancestor resolved (symlinks, `/tmp` → `/private/tmp`) and the
/// missing rest joined back (SB-131).
fn canonical_near(path: &Path) -> PathBuf {
    let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
    let mut at = Some(path);
    while let Some(dir) = at {
        if let Ok(real) = dir.canonicalize() {
            return rest.iter().rev().fold(real, |p, part| p.join(part));
        }
        match dir.file_name() {
            Some(name) => rest.push(name),
            None => break,
        }
        at = dir.parent();
    }
    path.to_owned()
}
/// The owner transaction, held by a background step, logged when held long (2026-10-17: reads
/// waited minutes behind it). `step` is a fixed code naming the holder.
pub(crate) struct Held<'a> {
    _guard: tokio::sync::MutexGuard<'a, ()>,
    step: &'static str,
    since: std::time::Instant,
}
impl Drop for Held<'_> {
    fn drop(&mut self) {
        let held = self.since.elapsed();
        if held >= HELD_LONG {
            oplog::event(
                "transaction_held",
                &[
                    ("step", oplog::Field::Code(self.step)),
                    ("millis", oplog::Field::Number(held.as_millis() as i64)),
                ],
            );
        }
    }
}
/// A background step holding the owner transaction this long is logged.
const HELD_LONG: std::time::Duration = std::time::Duration::from_secs(5);
/// SB-131: what the deletion check remembers between looks: since when each folder is missing,
/// and the device each folder was on when last seen.
#[derive(Default)]
struct Vanish {
    missing: HashMap<String, i64>,
    devices: HashMap<String, u64>,
}
/// SB-131: folders that could not be added are tried again this often.
const RETRY_PENDING: i64 = 600;
/// SB-131: project folders are checked for deletion this often, and one is taken out of its
/// project when it is still missing this long after it was first seen missing.
const VANISH_LOOK: i64 = 300;
const VANISH_CONFIRM: i64 = 300;
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
            Operation::ProjectEdit { .. } => Self::Project("updated"),
            Operation::MoveAccount { .. } => Self::Project("updated"),
            Operation::MoveRoot { .. } => Self::Project("updated"),
            Operation::ProjectSuggestionAccept { .. } => Self::Project("updated"),
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
            store.update_fields(&id, label, enabled)?;
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
        Operation::AgentCatalog => Ok(agent_catalog::catalog()),
        Operation::Recovery => {
            let rt = needs_owner()?;
            let time = monitor::now();
            let limited = rt.limits.limited_ids(time);
            let sign_in = rt.refresh.sign_in_required(&store).into_iter().collect();
            let snapshot = store.snapshot()?;
            let blocked = rt.limits.recovery_blocked(&snapshot, time);
            let capped = rt.limits.capped_ids(time);
            let mut status = recovery::status(root, &snapshot, &blocked, &capped, &sign_in, time)?;
            status["limited_accounts"] = json!(limited.len());
            status["awaiting_post_limit_probe_accounts"] =
                json!(blocked.difference(&limited).count());
            status["limits"] = json!(rt.limits.report(time));
            Ok(status)
        }
        Operation::RecoveryRegister {
            event,
            session,
            account,
            directory,
        } => {
            needs_owner()?;
            recovery::register(
                root,
                &store.snapshot()?,
                &event,
                &session,
                &account,
                &directory,
                monitor::now(),
            )?;
            Ok(json!({"registered": true}))
        }
        Operation::RecoveryClaim { event, account } => {
            let rt = needs_owner()?;
            let time = monitor::now();
            let sign_in = rt.refresh.sign_in_required(&store).into_iter().collect();
            let _credential = store.credential(&account)?;
            let snapshot = store.snapshot()?;
            let blocked = rt.limits.recovery_blocked(&snapshot, time);
            let capped = rt.limits.capped_ids(time);
            let claimed = recovery::claim(
                root, &event, &account, &snapshot, &blocked, &capped, &sign_in, time,
            )?;
            Ok(json!({"claimed": claimed, "resumed": false}))
        }
        Operation::RecoveryFinish { event, resumed } => {
            needs_owner()?;
            recovery::finish(root, &event, resumed)?;
            Ok(json!({"recorded": true}))
        }
        Operation::RecoveryHook {
            session,
            directory,
            billing,
        } => {
            let rt = needs_owner()?;
            let snapshot = store.snapshot()?;
            let saved = |id: &String| snapshot.accounts.iter().any(|a| &a.id == id);
            // A session in a project's folder ran on the project's pool (SB-118), never on the
            // ordinary CLI's account: before 0.6.41 every such stop was refused (1,202 in a day).
            let in_project = directory
                .canonicalize()
                .ok()
                .and_then(|d| snapshot.project_for(&d).map(|p| p.pool.clone()));
            let account = match rt
                .limits
                .bound(&session)
                .and_then(|ids| ids.into_iter().find(saved))
            {
                Some(id) => Some(id),
                None => match &in_project {
                    Some(pool) => project_stop_account(&snapshot, pool),
                    None => rt.current_accounts()?["claude"]["account_id"]
                        .as_str()
                        .map(str::to_owned)
                        .filter(saved),
                },
            };
            let Some(account) = account else {
                oplog::event(
                    "recovery_hook",
                    &[("result", oplog::Field::Code("no_account"))],
                );
                return Err("No saved account matches the stopped session.".into());
            };
            let kind = if billing {
                recovery::StopKind::Billing
            } else {
                recovery::StopKind::Limit
            };
            let time = monitor::now();
            match recovery::register_stop(
                root, &snapshot, &session, &account, &directory, kind, time,
            ) {
                Ok(stopped) => {
                    let (result, event) = match stopped {
                        recovery::Stopped::New(e) => ("registered", e),
                        recovery::Stopped::Again(e) => ("again", e),
                    };
                    oplog::event("recovery_hook", &[("result", oplog::Field::Code(result))]);
                    Ok(json!({"registered": true, "event": event, "result": result}))
                }
                Err(e) => {
                    oplog::event(
                        "recovery_hook",
                        &[("result", oplog::Field::Code("refused"))],
                    );
                    Err(e)
                }
            }
        }
        Operation::Handovers => Ok(json!({
            "enabled": handovers::enabled(root),
            "handovers": handovers::pending(root),
        })),
        Operation::HandoverTake { id } => {
            let taken = handovers::take(root, &id)?;
            Ok(json!({"handover": taken, "script": taken.script}))
        }
        Operation::HandoverSetting { on } => {
            if let Some(on) = on {
                handovers::set_enabled(root, on)?;
            }
            Ok(json!({"enabled": handovers::enabled(root)}))
        }
        Operation::Chains { workflow, pool } => {
            let effective = store.chain_for(workflow.as_deref(), pool.as_deref())?;
            Ok(json!({
                "chains": store.snapshot()?.chains,
                "effective": effective,
                "presets": agent_catalog::presets(),
            }))
        }
        Operation::SetChain {
            scope,
            executors,
            preset,
        } => {
            let executors = match preset {
                Some(id) => {
                    if !executors.is_empty() {
                        return Err("Give a preset or a list of agents, not both.".into());
                    }
                    agent_catalog::PRESETS
                        .iter()
                        .find(|(p, _)| *p == id)
                        .ok_or("Unknown preset.")?
                        .1
                        .iter()
                        .map(|a| switchboard_core::Executor {
                            agent: (*a).to_owned(),
                            account_id: None,
                            key: None,
                        })
                        .collect()
                }
                None => executors,
            };
            for e in &executors {
                if !agent_catalog::can_continue(&e.agent) {
                    return Err(format!(
                        "{} cannot take over a workflow: an agent in a chain loads MCP servers and takes a prompt without a person (switchboard agents list).",
                        e.agent.chars().take(40).collect::<String>()
                    ));
                }
            }
            Ok(json!({"chain": store.set_chain(scope, executors)?}))
        }
        Operation::AgentConnect { agent, pool } => {
            let address = runtime
                .map(|r| r.proxy.address().to_string())
                .or_else(|| load_proxy_record(root).map(|r| format!("127.0.0.1:{}", r.port)));
            let cli = launch::agent_cli()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "switchboard".into());
            agent_catalog::connect(&agent, address.as_deref(), &pool, &cli)
        }
        Operation::AgentKeySet {
            service,
            key,
            model,
            pool,
        } => {
            let saved = store.set_agent_key_for(&service, pool.as_deref(), &key, model)?;
            Ok(
                json!({"service": saved.service, "pool": saved.pool, "saved_at": saved.saved_at, "model": saved.model}),
            )
        }
        Operation::AgentKeyStatus {
            service,
            credit,
            pool,
        } => agent_keys::status_for(&store, &service, pool.as_deref(), credit).await,
        Operation::AgentKeyModel {
            service,
            model,
            pool,
        } => {
            let saved = store.set_agent_key_model_for(&service, pool.as_deref(), &model)?;
            Ok(json!({"service": saved.service, "pool": saved.pool, "model": saved.model}))
        }
        Operation::AgentKeyRemove { service, pool } => {
            Ok(json!({"removed": store.remove_agent_key_for(&service, pool.as_deref())?}))
        }
        Operation::AgentKeys { service } => agent_keys::list(&store, &service),
        Operation::AgentKeyValue { service, folder } => {
            Ok(json!({"key": store.agent_key_value_in(&service, folder.as_deref())?}))
        }
        Operation::HermesModel => tokio::task::spawn_blocking(hermes::status)
            .await
            .map_err(|_| hermes::HERMES_FAILED.to_string()),
        Operation::HermesSetModel { provider, model } => {
            tokio::task::spawn_blocking(move || hermes::set(provider.as_deref(), model.as_deref()))
                .await
                .map_err(|_| hermes::HERMES_FAILED.to_string())?
        }
        Operation::KimiAccounts => kimi::accounts(root, &store, monitor::now()).await,
        Operation::KimiLoginBegin { label, region } => {
            kimi::begin_login(root, &label, &region, launch::start_terminal)
        }
        Operation::KimiLoginStatus { id } => Ok(json!({"state": kimi::login_state(root, &id)})),
        Operation::KimiLoginFinish { id } => {
            kimi::finish_login(root, &store, &id, monitor::now()).await
        }
        Operation::KimiLoginCancel { id } => {
            kimi::cancel_login(root, &store, &id).map(|()| json!({"cancelled": true}))
        }
        Operation::KimiSignInAgain { id } => {
            kimi::sign_in_again(root, &store, &id, launch::start_terminal)
        }
        Operation::KimiLaunch {
            id,
            working_directory,
        } => kimi::launch(
            root,
            &store,
            &id,
            &working_directory,
            launch::start_terminal,
        ),
        Operation::KimiRename { id, label } => Ok(json!({
            "account": store.update_kimi_account(&id, Some(&label), None, None)?
        })),
        Operation::KimiRemove { id } => Ok(json!({"removed": kimi::remove(root, &store, &id)?})),
        Operation::AgentKey => {
            let token = match runtime {
                Some(r) => r.proxy.token().to_owned(),
                None => load_proxy_record(root)
                    .map(|r| r.token)
                    .ok_or("Start the desktop app or 'switchboard serve' once first.")?,
            };
            Ok(json!({"key": switchboard_proxy::agent_token(&token)}))
        }
        Operation::LaunchAgent {
            agent,
            working_directory,
            via: AgentVia::Openrouter,
            model,
            ..
        } => launch::launch_agent_openrouter(
            root,
            &store,
            &agent,
            model.as_deref(),
            &working_directory,
        ),
        Operation::LaunchAgent {
            agent,
            pool,
            working_directory,
            via: AgentVia::Proxy,
            ..
        } => {
            let runtime = needs_owner()?;
            launch::launch_agent(
                root,
                &store,
                &runtime.proxy,
                &agent,
                &pool,
                &working_directory,
            )
        }
        Operation::LaunchAccount {
            provider,
            working_directory,
        } => {
            let folder = projects::project_dir(root, &working_directory)?;
            let id = projects::account_for_folder(&store, provider, &folder, monitor::now())?;
            Ok(json!({"id": id}))
        }
        Operation::Launch {
            id,
            mode,
            working_directory,
            in_place,
            args,
        } => {
            let runtime = needs_owner()?;
            if !in_place && !args.is_empty() {
                return Err(launch::EXTRA_ARGS_INVALID.into());
            }
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
            if in_place {
                let (agent_tools, script) = launch::launch_here(
                    root,
                    &store,
                    &runtime.proxy,
                    &id,
                    &mode,
                    &working_directory,
                    &args,
                )?;
                return Ok(json!({"script": script, "agent_tools": agent_tools}));
            }
            let agent_tools =
                launch::launch(root, &store, &runtime.proxy, &id, &mode, &working_directory)?;
            Ok(
                json!({"message": "Terminal launch requested. Provider response is not yet verified.", "agent_tools": agent_tools}),
            )
        }
        // Taken by `Runtime::execute` before this point; an offline caller has no owner.
        Operation::ContinueToConsole { .. } => Err(needs_owner()
            .err()
            .unwrap_or_else(|| "Hand-over unavailable.".into())),
        Operation::Continue {
            workflow_id,
            id,
            mode,
            working_directory,
            in_place,
        } => {
            if in_place && cfg!(windows) {
                return Err(launch::IN_PLACE_UNSUPPORTED.into());
            }
            // In place, the session script is prepared and returned; nothing opens Terminal.
            let start: fn(&Path) -> Result<(), String> = if in_place {
                |_| Ok(())
            } else {
                launch::start_terminal
            };
            let runtime = needs_owner()?;
            // SB-120: no account named → the machine's default agent: OpenCode and Hermes run on
            // the OpenRouter key; Claude Code, Codex and Kimi Code on the account a one-step chain
            // of that agent would choose now.
            let id = if id.is_empty() {
                match agent_settings::read(root)?.default.as_deref() {
                    Some(hermes::HERMES_TARGET) => hermes::HERMES_TARGET.to_owned(),
                    Some(launch::OPENCODE_TARGET) => launch::OPENCODE_TARGET.to_owned(),
                    Some(agent) => {
                        agent_settings::usable(root, agent)?;
                        if !continuation::valid_workflow_id(&workflow_id) {
                            return Err(continuation::BAD_WORKFLOW_ID.into());
                        }
                        let folder = working_directory
                            .canonicalize()
                            .map_err(|_| "Choose an existing project directory.")?;
                        // The executor is read first, so its own exhausted account is never the
                        // one chosen.
                        let shown =
                            continuation::show(&continuation::observatory()?, &workflow_id)?;
                        fallback::account_for_default(
                            runtime,
                            agent,
                            &continuation::executor_of(&shown),
                            &folder,
                            monitor::now(),
                        )?
                    }
                    None => return Err(agent_settings::NAME_AN_ACCOUNT.into()),
                }
            } else {
                id
            };
            // SB-120: an agent the person disabled for taking over work takes no workflow.
            let agent = if id == hermes::HERMES_TARGET || id == launch::OPENCODE_TARGET {
                Some(id.clone())
            } else if store.kimi_account(&id).is_ok() {
                Some("kimi-code".to_owned())
            } else {
                store
                    .snapshot()?
                    .accounts
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| switchboard_core::provider_agent(a.provider).to_owned())
            };
            if let Some(a) = agent {
                if agent_settings::disabled(root, &a)? {
                    return Err(agent_settings::DISABLED.into());
                }
            }
            // Everything that can refuse runs before the offer, so a refusal leaves the workflow
            // exactly as it was; after the offer, a failed launch leaves it to lapse.
            if !continuation::valid_workflow_id(&workflow_id) {
                return Err(continuation::BAD_WORKFLOW_ID.into());
            }
            let bin = continuation::observatory()?;
            let directory = working_directory
                .canonicalize()
                .map_err(|_| "Choose an existing project directory.")?;
            // SB-107: Hermes Agent on the saved OpenRouter key, with the same order — every
            // refusal before the offer, a failed launch after it left to lapse.
            let other_agent = id == hermes::HERMES_TARGET
                || id == launch::OPENCODE_TARGET
                || store.kimi_account(&id).is_ok();
            if other_agent && mode == "managed" {
                return Err(continuation::MANAGED_NOT_FOR_AGENT.into());
            }
            // SB-119: OpenCode on the saved OpenRouter key and its default model, as Hermes.
            if id == launch::OPENCODE_TARGET {
                let shown = continuation::show(&bin, &workflow_id)?;
                let plan = continuation::plan_named(
                    &shown,
                    &workflow_id,
                    launch::OPENCODE_TARGET,
                    &id,
                    true,
                    &directory,
                )?;
                launch::opencode_continuation_preflight(&store)?;
                launch::agent_folder(root, &directory)?;
                let observatory = continuation::observatory_mcp(&bin)?;
                let offer = continuation::offer(&bin, &plan, &id)?;
                let resumed = continuation::Continuation {
                    workflow_id: workflow_id.clone(),
                    handoff_id: offer.handoff_id.clone(),
                    observatory,
                    previous_provider: plan.from_provider.clone(),
                };
                let launched =
                    launch::opencode_continuation(root, &store, &directory, &resumed, start)
                        .map_err(|e| continuation::launch_failed(&offer, &e))?;
                return Ok(json!({
                    "script": if in_place { launched["script"].clone() } else { Value::Null },
                    "message": "Terminal launch requested; the OpenCode session accepts the handoff itself. Provider response is not yet verified.",
                    "workflow": workflow_id,
                    "handoff": offer.handoff_id,
                    "offeredUntil": offer.expires_at,
                "account": id,
                    "model": launched["model"],
                    "modelFrom": launched["model_from"],
                }));
            }
            if id == hermes::HERMES_TARGET {
                let shown = continuation::show(&bin, &workflow_id)?;
                let plan = continuation::plan_named(
                    &shown,
                    &workflow_id,
                    hermes::HERMES_TARGET,
                    &id,
                    true,
                    &directory,
                )?;
                // Cheap refusals first; then the model Hermes itself runs on OpenRouter, read off
                // the async workers within five seconds.
                launch::hermes_continuation_basics(&store)?;
                let own_model = tokio::task::spawn_blocking(hermes::openrouter_model)
                    .await
                    .ok()
                    .flatten();
                launch::hermes_continuation_preflight(&store, own_model.as_deref())?;
                launch::agent_folder(root, &directory)?;
                let observatory = continuation::observatory_mcp(&bin)?;
                let offer = continuation::offer(&bin, &plan, &id)?;
                let resumed = continuation::Continuation {
                    workflow_id: workflow_id.clone(),
                    handoff_id: offer.handoff_id.clone(),
                    observatory,
                    previous_provider: plan.from_provider.clone(),
                };
                let launched = launch::hermes_continuation(
                    root,
                    &store,
                    &directory,
                    &resumed,
                    own_model.as_deref(),
                    start,
                )
                .map_err(|e| continuation::launch_failed(&offer, &e))?;
                return Ok(json!({
                    "script": if in_place { launched["script"].clone() } else { Value::Null },
                    "message": "Terminal launch requested; the Hermes session accepts the handoff itself. Provider response is not yet verified.",
                    "workflow": workflow_id,
                    "handoff": offer.handoff_id,
                    "offeredUntil": offer.expires_at,
                "account": id,
                    "model": launched["model"],
                    "modelFrom": launched["model_from"],
                }));
            }
            // SB-99: a saved Kimi Code account continues through `kimi`, with the same order —
            // every refusal before the offer, a failed launch after it left to lapse.
            if store.kimi_account(&id).is_ok() {
                let shown = continuation::show(&bin, &workflow_id)?;
                let plan = continuation::plan_named(
                    &shown,
                    &workflow_id,
                    continuation::KIMI,
                    &id,
                    true,
                    &directory,
                )?;
                kimi::continuation_preflight(root, &store, &id)?;
                launch::agent_folder(root, &directory)?;
                let observatory = continuation::observatory_mcp(&bin)?;
                let offer = continuation::offer(&bin, &plan, &id)?;
                let resumed = continuation::Continuation {
                    workflow_id: workflow_id.clone(),
                    handoff_id: offer.handoff_id.clone(),
                    observatory,
                    previous_provider: plan.from_provider.clone(),
                };
                let launched =
                    kimi::launch_continuation(root, &store, &id, &directory, &resumed, start)
                        .map_err(|e| continuation::launch_failed(&offer, &e))?;
                return Ok(json!({
                    "script": if in_place { launched["script"].clone() } else { Value::Null },
                    "message": "Terminal launch requested; the Kimi Code session accepts the handoff itself. Provider response is not yet verified.",
                    "workflow": workflow_id,
                    "handoff": offer.handoff_id,
                    "offeredUntil": offer.expires_at,
                "account": id,
                }));
            }
            let account = store
                .snapshot()?
                .accounts
                .into_iter()
                .find(|a| a.id == id)
                .ok_or("Account not found.")?;
            let shown = continuation::show(&bin, &workflow_id)?;
            let plan = continuation::plan(&shown, &workflow_id, &account, &directory)?;
            if mode == "isolated" {
                refresh::learn_live_owner(&store, native, refresh_state).await;
                let short = store
                    .stored_credential(&id)
                    .is_ok_and(|c| short_lived(&c, monitor::now()));
                if refresh::ensure_fresh(&store, native, refresh_state, &id, short).await
                    == refresh::Outcome::Active
                {
                    adopt_live(&store, native, refresh_state, &id);
                }
            }
            launch::preflight(root, &store, &id, &mode, &directory)?;
            let observatory = continuation::observatory_mcp(&bin)?;
            let offer = continuation::offer(&bin, &plan, &id)?;
            let resumed = continuation::Continuation {
                workflow_id: workflow_id.clone(),
                handoff_id: offer.handoff_id.clone(),
                observatory,
                previous_provider: plan.from_provider.clone(),
            };
            let (agent_tools, script) = if in_place {
                let (tools, script) = launch::continuation_here(
                    root,
                    &store,
                    &runtime.proxy,
                    &id,
                    &mode,
                    &directory,
                    &resumed,
                )
                .map_err(|e| continuation::launch_failed(&offer, &e))?;
                (tools, Some(script))
            } else {
                let tools = launch::launch_continuation(
                    root,
                    &store,
                    &runtime.proxy,
                    &id,
                    &mode,
                    &directory,
                    &resumed,
                )
                .map_err(|e| continuation::launch_failed(&offer, &e))?;
                (tools, None)
            };
            Ok(json!({
                "script": script,
                "message": "Terminal launch requested; the session accepts the handoff itself. Provider response is not yet verified.",
                "workflow": workflow_id,
                "handoff": offer.handoff_id,
                "offeredUntil": offer.expires_at,
                "account": id,
                "agent_tools": agent_tools,
            }))
        }
        Operation::BeginLogin {
            provider,
            label,
            pool,
        } => needs_owner()?.begin_login(provider, label, pool),
        Operation::FinishLogin { login_id } => needs_owner()?.finish_login(&login_id),
        Operation::LoginStatus { login_id } => needs_owner()?.login_status(&login_id),
        Operation::PendingLogins => Ok(match runtime {
            Some(runtime) => runtime.pending_logins(),
            None => json!({ "logins": [] }),
        }),
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
            base_folders,
        } => {
            let before = store.snapshot()?;
            let held: Vec<String> = pool
                .as_deref()
                .and_then(|p| before.projects.iter().find(|x| x.pool == p))
                .map(|x| x.folders.clone())
                .unwrap_or_default();
            let is_held = |f: &Path| held.iter().any(|h| Path::new(h) == f);
            let in_base = |f: &Path| {
                base_folders
                    .as_ref()
                    .is_some_and(|b| b.iter().any(|x| x == f))
            };
            // SB-131: folders Switchboard added while the dialog was open stay, unless the person
            // took them out in it (they were never in the dialog, so they cannot have been).
            let mut folders = folders;
            if base_folders.is_some() {
                for h in &held {
                    let h = PathBuf::from(h);
                    if !in_base(&h) && !folders.contains(&h) {
                        folders.push(h);
                    }
                }
            }
            // A folder this project already holds that is gone from disk (a removed worktree) is
            // dropped, not a refusal: otherwise no edit of the project — a moved account, a
            // renamed project — could be saved again (SB-128, CALL-07). One on a drive that may
            // only be unmounted is kept as it is (SB-131); one Switchboard took out while the
            // dialog was open is dropped quietly.
            let mut gone: Vec<PathBuf> = Vec::new();
            let mut kept: Vec<PathBuf> = Vec::new();
            for f in folders {
                if base_folders.is_some() && in_base(&f) && !is_held(&f) {
                    // Switchboard took it out while the dialog was open (deleted, or shared
                    // between projects): the person did not choose it in this dialog.
                    gone.push(f);
                } else if f.exists() {
                    kept.push(projects::project_dir(root, &f)?);
                } else if is_held(&f) && projects::maybe_unmounted(&f) {
                    kept.push(f);
                } else if is_held(&f) {
                    gone.push(f);
                } else {
                    // Admission names the problem: a folder that never existed is refused.
                    kept.push(projects::project_dir(root, &f)?);
                }
            }
            let folders = kept;
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
            // SB-121: the folders the person takes out of an existing project are remembered as
            // declined, so the automatic adding never brings back a folder the person removed. A
            // folder dropped because it is gone from disk is remembered as gone (SB-131): it is
            // found again if it comes back.
            let removed: Vec<String> = pool
                .as_deref()
                .and_then(|p| snapshot.projects.iter().find(|x| x.pool == p))
                .map(|old| {
                    old.folders
                        .iter()
                        .filter(|f| !folders.iter().any(|n| n == Path::new(f)))
                        .filter(|f| !gone.iter().any(|g| g == Path::new(f)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let project = store.save_project(pool.as_deref(), &name, &folders, &account_ids)?;
            let gone_text: Vec<String> = gone
                .iter()
                .filter(|g| is_held(g))
                .map(|g| g.to_string_lossy().into_owned())
                .collect();
            if (!removed.is_empty()
                && suggest::remember_removed(root, &project.pool, &removed, monitor::now())
                    .is_err())
                || (!gone_text.is_empty()
                    && suggest::mark_gone(root, &project.pool, &gone_text, monitor::now()).is_err())
            {
                oplog::event(
                    "project_suggestion",
                    &[("remember", oplog::Field::Code("failed"))],
                );
            }
            // A folder added by hand, or a project's folders changed, can answer a suggestion.
            if let Some(rt) = runtime {
                rt.suggestions_changed();
            }
            let mut view = projects::project_view(&store.snapshot()?, &project);
            if !gone.is_empty() {
                view["dropped_missing"] = json!(gone);
            }
            Ok(view)
        }
        Operation::AgentsOverview => {
            let model = store
                .agent_key("openrouter")
                .ok()
                .flatten()
                .and_then(|k| k.model);
            agent_settings::overview(root, model.as_deref())
        }
        Operation::AgentEnable { agent, enabled } => {
            let s = agent_settings::set_enabled(root, &agent, enabled)?;
            Ok(json!({"agent": agent, "enabled": enabled, "default": s.default}))
        }
        Operation::AgentDefault { agent } => {
            let s = agent_settings::set_default(root, agent.as_deref())?;
            Ok(json!({"default": s.default}))
        }
        Operation::OpenrouterModels => match runtime {
            Some(rt) => Ok(agent_settings::models(&rt.openrouter_models, monitor::now()).await),
            None => {
                let cache = Mutex::new(agent_settings::ModelCache::default());
                Ok(agent_settings::models(&cache, monitor::now()).await)
            }
        },
        Operation::ProjectBindings => {
            let rows: Vec<Value> = bindings::read(root)?
                .into_iter()
                .map(|b| json!({"folder": b.folder, "pool": b.pool, "base_url": b.base_url}))
                .collect();
            Ok(json!({"bindings": rows}))
        }
        Operation::ProjectSuggestions => {
            let snapshot = store.snapshot()?;
            let rows: Vec<Value> = suggest::pending(root, &snapshot)?
                .into_iter()
                .map(|s| suggestion_view(&snapshot, &s))
                .collect();
            let added: Vec<Value> = suggest::added_since(root, monitor::now() - ADDED_SHOWN)?
                .into_iter()
                .map(|s| suggestion_view(&snapshot, &s))
                .collect();
            // SB-131: folders taken out of their project because they were deleted.
            let removed: Vec<Value> = suggest::gone_since(root, monitor::now() - ADDED_SHOWN)?
                .into_iter()
                .map(|s| suggestion_view(&snapshot, &s))
                .collect();
            Ok(json!({"suggestions": rows, "added": added, "removed": removed}))
        }
        Operation::ProjectSuggest { folder } => {
            let snapshot = store.snapshot()?;
            let repos = suggest::project_repos(&snapshot);
            let now = monitor::now();
            let outcome = match suggest::record(root, &snapshot, &repos, &folder, now)? {
                suggest::Outcome::NotOurs { .. } => {
                    suggest::unassigned(root, &snapshot, &repos, &folder)?
                }
                other => other,
            };
            match outcome {
                suggest::Outcome::New(s) => {
                    // SB-121: a project's folder joins it at once; only a refused add waits.
                    let added = add_suggested_folder(&store, root, &s, now);
                    if let Some(rt) = runtime {
                        oplog::event(
                            "project_suggestion",
                            &[(
                                if added.is_ok() { "added" } else { "found" },
                                oplog::Field::Number(1),
                            )],
                        );
                        rt.suggestions_changed();
                    }
                    let view = suggestion_view(&store.snapshot()?, &s);
                    Ok(match added {
                        Ok(project) => {
                            json!({"suggested": false, "added": true, "suggestion": view, "project": project})
                        }
                        Err(e) => {
                            json!({"suggested": true, "added": false, "suggestion": view, "message": e})
                        }
                    })
                }
                other => Ok(
                    json!({"suggested": false, "reason": other.code(), "message": other.message()}),
                ),
            }
        }
        Operation::ProjectSuggestionAccept { folder } => {
            let suggestion = suggest::find_pending(root, &folder)?;
            let saved = add_suggested_folder(&store, root, &suggestion, monitor::now())?;
            if let Some(rt) = runtime {
                rt.suggestions_changed();
            }
            Ok(json!({"added": suggestion.folder, "project": saved}))
        }
        Operation::ProjectSuggestionDismiss { folder } => {
            suggest::dismiss(root, &folder)?;
            if let Some(rt) = runtime {
                rt.suggestions_changed();
            }
            Ok(json!({"dismissed": folder}))
        }
        Operation::ProjectSources { pool, sources } => {
            let project = store.set_project_sources(&pool, &sources)?;
            if let Some(rt) = runtime {
                rt.suggestions_changed();
            }
            Ok(projects::project_view(&store.snapshot()?, &project))
        }
        Operation::RotatePoolCapability { pool } => {
            let rt = needs_owner()?;
            if !store.snapshot()?.projects.iter().any(|p| p.pool == pool) {
                return Err("Project not found.".into());
            }
            let _bindings = rt.bindings_lock.lock().await;
            let mut salts = read_pool_salts(root);
            salts.insert(pool.clone(), new_salt());
            write_pool_salts(root, &salts)?;
            rt.proxy.set_pool_salts(salts.clone());
            // Written at once, not at the next pass: the old capability is already refused.
            let snapshot = store.snapshot()?;
            let address = rt.proxy.address().to_string();
            let report =
                bindings::sync_salted(root, &snapshot, &address, rt.proxy.token(), &salts)?;
            rt.proxy.set_folder_tokens(report.tokens.clone());
            rt.proxy.set_stat_folders(stat_folders(&snapshot));
            if let Ok(mut key) = rt.bindings_key.lock() {
                *key = None;
            }
            oplog::event(
                "capability_rotated",
                &[("folders", oplog::Field::Number(report.bound as i64))],
            );
            Ok(
                json!({"pool": pool, "rotated": true, "folders_rewritten": report.bound,
                "skipped": report.skipped.len(),
                "note": "The old capability is refused now. A Claude Code session already running in one of these folders keeps the old one until it restarts."}),
            )
        }
        Operation::ProjectEdit {
            pool,
            name,
            add_accounts,
            remove_accounts,
            add_folders,
            remove_folders,
        } => {
            let snapshot = store.snapshot()?;
            // SB-135: a root taken out takes its worktrees and clones with it.
            let mut remove_folders = remove_folders;
            if let Some(project) = snapshot.projects.iter().find(|p| p.pool == pool) {
                let roots = blocking::run(|| roots::tree(&project.folders));
                let mut extra: Vec<PathBuf> = Vec::new();
                for f in &remove_folders {
                    let named = canonical_near(f);
                    for root in &roots {
                        if canonical_near(Path::new(&root.folder)) == named {
                            extra.extend(root.derived.iter().map(|d| PathBuf::from(&d.folder)));
                        }
                    }
                }
                for f in extra {
                    if !remove_folders.contains(&f) {
                        remove_folders.push(f);
                    }
                }
            }
            let save = project_edit(
                &snapshot,
                &pool,
                name,
                &add_accounts,
                &remove_accounts,
                &add_folders,
                &remove_folders,
            )?;
            // The same admission, refusals and memory of removals as a whole save (SaveProject).
            Box::pin(execute(store, root, runtime, save)).await
        }
        Operation::MoveAccount { id, pool } => {
            let snapshot = store.snapshot()?;
            let from = snapshot
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or("Account not found.")?
                .pool
                .clone();
            if from == pool {
                return Ok(json!({"id": id, "from": from, "pool": pool, "moved": false}));
            }
            let edit =
                |pool: String, add: Vec<String>, remove: Vec<String>| Operation::ProjectEdit {
                    pool,
                    name: None,
                    add_accounts: add,
                    remove_accounts: remove,
                    add_folders: Vec::new(),
                    remove_folders: Vec::new(),
                };
            if pool != "default" {
                if snapshot.project_of_pool(&pool).is_none() {
                    return Err(MOVE_ACCOUNT_NO_PROJECT.into());
                }
                Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(pool.clone(), vec![id.clone()], Vec::new()),
                ))
                .await?;
            } else if snapshot.project_of_pool(&from).is_some() {
                Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(from.clone(), Vec::new(), vec![id.clone()]),
                ))
                .await?;
            } else {
                store.return_to_default(&id)?;
            }
            Ok(json!({"id": id, "from": from, "pool": pool, "moved": true}))
        }
        Operation::CodexRoute { enabled } => {
            let path = runtime.and_then(|r| (r.native.codex_config)());
            if let Some(on) = enabled {
                let rt = needs_owner()?;
                let path = path.clone().ok_or(codex_route::CODEX_ROUTE_UNREADABLE)?;
                if on {
                    codex_route::apply(
                        &path,
                        &rt.proxy.address().to_string(),
                        &rt.proxy.auto_capability(),
                    )?;
                    codex_route::set_wanted(root, true)?;
                } else {
                    codex_route::set_wanted(root, false)?;
                    codex_route::remove(&path)?;
                }
                oplog::event(
                    "codex_route",
                    &[("switch", oplog::Field::Code(if on { "on" } else { "off" }))],
                );
            }
            let snapshot = store.snapshot()?;
            let mut status = codex_route::status(root, path.as_deref());
            status["projects"] = json!(snapshot
                .projects
                .iter()
                .filter(|p| codex_route::folders(&snapshot)
                    .iter()
                    .any(|(_, pool)| pool == &p.pool))
                .map(|p| p.name.clone())
                .collect::<Vec<_>>());
            Ok(status)
        }
        Operation::MoveRoot {
            folder,
            pool,
            from: leave,
            also,
        } => {
            let snapshot = store.snapshot()?;
            if snapshot.project_of_pool(&pool).is_none() {
                return Err(MOVE_ROOT_NO_PROJECT.into());
            }
            let named = canonical_near(&folder);
            let edit =
                |pool: &str, add: Vec<PathBuf>, remove: Vec<PathBuf>| Operation::ProjectEdit {
                    pool: pool.to_owned(),
                    name: None,
                    add_accounts: Vec::new(),
                    remove_accounts: Vec::new(),
                    add_folders: add,
                    remove_folders: remove,
                };
            let holds = |p: &&switchboard_core::Project| {
                p.folders
                    .iter()
                    .any(|f| canonical_near(Path::new(f)) == named)
            };
            if leave
                .as_deref()
                .is_some_and(|l| !snapshot.projects.iter().any(|p| p.pool == l && holds(&p)))
            {
                return Err(MOVE_ROOT_NOT_IN_GROUP.into());
            }
            let from = snapshot
                .projects
                .iter()
                .filter(holds)
                .find(|p| leave.as_deref().is_none_or(|l| p.pool == l))
                .and_then(|p| {
                    p.folders
                        .iter()
                        .find(|f| canonical_near(Path::new(f)) == named)
                        .map(|f| (p, f.clone()))
                });
            let Some((from, stored)) = from else {
                if snapshot.project_for(&named).is_some() {
                    return Err(MOVE_ROOT_INSIDE.into());
                }
                Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(&pool, vec![named.clone()], Vec::new()),
                ))
                .await?;
                return Ok(
                    json!({"folder": named, "from": null, "pool": pool, "moved": true, "folders": 1}),
                );
            };
            let target_holds = snapshot.project_of_pool(&pool).is_some_and(|p| holds(&p));
            if from.pool == pool || (also && target_holds) {
                return Ok(
                    json!({"folder": stored, "from": from.pool, "pool": pool, "moved": false, "folders": 0}),
                );
            }
            let folders = from.folders.clone();
            let roots = blocking::run(|| roots::tree(&folders));
            let mut tree: Vec<PathBuf> = vec![PathBuf::from(&stored)];
            tree.extend(
                roots::derived_of(&roots, &stored)
                    .into_iter()
                    .map(PathBuf::from),
            );
            // SB-146: into one more group, out of none.
            if also {
                let count = tree.len();
                Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(&pool, tree, Vec::new()),
                ))
                .await?;
                return Ok(
                    json!({"folder": stored, "from": null, "pool": pool, "moved": true, "also": true, "folders": count}),
                );
            }
            // Already in the target too: leaving the source is the whole move.
            if target_holds {
                if tree.len() >= from.folders.len() {
                    return Err(MOVE_ROOT_LAST.into());
                }
                let count = tree.len();
                let source = from.pool.clone();
                Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(&source, Vec::new(), tree),
                ))
                .await?;
                return Ok(
                    json!({"folder": stored, "from": source, "pool": pool, "moved": true, "folders": count}),
                );
            }
            if tree.len() >= from.folders.len() {
                return Err(MOVE_ROOT_LAST.into());
            }
            let source = from.pool.clone();
            Box::pin(execute(
                store.clone(),
                root,
                runtime,
                edit(&source, Vec::new(), tree.clone()),
            ))
            .await?;
            if let Err(refused) = Box::pin(execute(
                store.clone(),
                root,
                runtime,
                edit(&pool, tree.clone(), Vec::new()),
            ))
            .await
            {
                // Back where they were: a move is all or nothing.
                if Box::pin(execute(
                    store.clone(),
                    root,
                    runtime,
                    edit(&source, tree.clone(), Vec::new()),
                ))
                .await
                .is_err()
                {
                    oplog::event("move_root", &[("rollback", oplog::Field::Code("failed"))]);
                }
                return Err(refused);
            }
            Ok(
                json!({"folder": stored, "from": source, "pool": pool, "moved": true, "folders": tree.len()}),
            )
        }
        Operation::Statistics { days } => {
            let snapshot = store.snapshot()?;
            let days = days.unwrap_or(30);
            // SB-147: folders resolve to projects through Git, off the async thread.
            let index = blocking::run(|| statistics::ProjectIndex::build(&snapshot));
            Ok(match runtime {
                Some(rt) => {
                    rt.statistics.absorb(rt.proxy.take_statistics());
                    rt.statistics
                        .report(&snapshot, &index, days, monitor::now())
                }
                None => statistics::Statistics::load(root).report(
                    &snapshot,
                    &index,
                    days,
                    monitor::now(),
                ),
            })
        }
        Operation::RepositoryMap { under } => {
            let snapshot = store.snapshot()?;
            let under = if under.is_empty() {
                vec![roots::default_under().ok_or("Home folder unavailable. Choose a folder.")?]
            } else {
                under
            };
            let map = blocking::run(|| roots::disk_map(&under, &snapshot));
            serde_json::to_value(map).map_err(|_| "Repositories could not be listed.".into())
        }
        Operation::RemoveProject { pool } => {
            let project = store.remove_project(&pool)?;
            // SB-121: its folders are not adopted by another project by themselves afterwards.
            if suggest::remember_removed(root, &project.pool, &project.folders, monitor::now())
                .is_err()
            {
                oplog::event(
                    "project_suggestion",
                    &[("remember", oplog::Field::Code("failed"))],
                );
            }
            if let Some(rt) = runtime {
                rt.suggestions_changed();
            }
            Ok(json!({"removed": project.pool, "name": project.name}))
        }
        Operation::RemoveProjectRule { path, provider } => {
            let path = projects::rule_path(root, &path)?;
            let rule = store.remove_rule(&path, provider)?;
            Ok(projects::rule_view(&store, &rule, monitor::now()))
        }
        Operation::Backups => {
            let enabled_dir = runtime.and_then(|r| r.backup_folder());
            let dir = enabled_dir.clone().or_else(|| fallback_backup_dir(root));
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
                "restored_at_start": runtime.and_then(|r| r.restored_at_start.lock().ok().and_then(|s| s.clone())),
            }))
        }
        Operation::BackupNow => match runtime {
            Some(runtime) => Ok(json!(runtime.maybe_backup(monitor::now(), true)?)),
            None => Err("Backups are written by the desktop app or switchboard serve.".into()),
        },
        Operation::RestoreBackup { file } => {
            let dir = runtime
                .and_then(|r| r.backup_folder())
                .or_else(|| fallback_backup_dir(root))
                .ok_or(BACKUPS_DEFAULT_FOLDER_ONLY)?;
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
        Operation::Sessions { all } => {
            let snapshot = store.snapshot()?;
            // SB-103: the liveness checks may start `kill -0`/`ps`: never on an async worker.
            let listed = crate::blocking::run(|| launch::sessions(root, all));
            let sessions: Vec<Value> = listed
                .into_iter()
                .map(|mut session| {
                    let id = session["account_id"].as_str().map(str::to_owned);
                    let label = snapshot
                        .accounts
                        .iter()
                        .find(|a| Some(&a.id) == id.as_ref())
                        .map(|a| a.label.clone())
                        .or_else(|| {
                            snapshot
                                .kimi_accounts
                                .iter()
                                .find(|k| Some(&k.id) == id.as_ref())
                                .map(|k| k.label.clone())
                        })
                        // A Hermes hand-over names its model itself (SB-107).
                        .or_else(|| session["label"].as_str().map(str::to_owned));
                    session["label"] = label.into();
                    session
                })
                .collect();
            Ok(json!({ "sessions": sessions }))
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
/// Files a finished official sign-in (SB-62). A sign-in started from an account's row names its
/// label and pool and updates that row. One started without a label (+ Add account, or the
/// sign-in banner's Try again) files the new sign-in into every saved copy of that identity where
/// it already is, keeping each copy's label and pool, and creates an account only when the
/// identity is saved nowhere: it never renames an account to its email, and never copies an
/// account (a project's, say) into another pool. Either way every other saved copy of the
/// identity gets the new sign-in too — one lineage in every pool (PLAN-0.5 C-6). Returns the
/// account to report and whether the identity was saved before.
fn file_sign_in(
    store: &Store,
    provider: Provider,
    label: &str,
    pool: &str,
    captured: external::CapturedProfile,
) -> Result<(Account, bool), String> {
    let copies: Vec<Account> = matching_accounts(store, provider, &captured.identity)?
        .into_iter()
        .filter(|a| a.kind == AuthKind::OAuth)
        .collect();
    let label = label.trim();
    if copies.is_empty() {
        let label = if label.is_empty() {
            captured.label.clone()
        } else {
            label.to_owned()
        };
        let account = store.upsert(
            label,
            provider,
            AuthKind::OAuth,
            pool.to_owned(),
            captured.credential,
            Some(captured.identity),
        )?;
        return Ok((account, false));
    }
    // Signed in again: every saved row of this login takes the new credential in one
    // transaction (SB-62, SB-78). The row in the requested pool is the one reported (and
    // renamed, when a label was given); without one, the first saved.
    let in_pool = copies.iter().find(|a| a.pool == pool).map(|a| a.id.clone());
    let rows: Vec<(String, Option<String>)> = copies
        .iter()
        .map(|a| {
            let rename =
                (!label.is_empty() && Some(&a.id) == in_pool.as_ref()).then(|| label.to_owned());
            (a.id.clone(), rename)
        })
        .collect();
    let renewed = store.renew_rows(provider, &rows, &captured.credential, &captured.identity)?;
    if in_pool.is_none() && !label.is_empty() {
        // A new pool named with a label: the copies are renewed first, so a failure here
        // leaves every saved row on the new credential and only this one unsaved.
        let account = store.upsert(
            label.to_owned(),
            provider,
            AuthKind::OAuth,
            pool.to_owned(),
            captured.credential,
            Some(captured.identity),
        )?;
        return Ok((account, true));
    }
    let primary = in_pool.unwrap_or_else(|| copies[0].id.clone());
    let account = renewed
        .into_iter()
        .find(|a| a.id == primary)
        .ok_or("Account not found")?;
    Ok((account, true))
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
    let mut credential = store.stored_credential(&account.id)?;
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
    let mut owner = identity.clone();
    if let Some(current) = &current {
        // The account being switched away from stays "recently active" for renewal purposes.
        state.note_active(&current.identity);
        let attributed = refresh::attribution(store, state, &current.identity, &current.credential);
        // Already the account in use: writing its stored copy would replace Claude Code's
        // newer live generation with an older one. Nothing to do — unless the token is known
        // to be another account's, when the settings name this account but Claude Code runs on
        // someone else's sign-in and the switch must really happen.
        if current.identity.account_id.is_some()
            && current.identity.account_id == identity.account_id
            && current.identity.organization_id == identity.organization_id
            && !matches!(attributed, refresh::Attribution::Other(_))
        {
            return Ok(());
        }
        // The live token is filed under the account it belongs to. Usually that is the one
        // Claude Code's settings name; after an interrupted switch or a half-finished `/login`
        // it can be another saved account's, and once the provider confirms whose, it goes to
        // that account and the switch goes on — refusing left the person unable to switch at
        // all. A token nobody can attribute, one only a stored copy calls another account's,
        // or one of an account Switchboard does not hold, is never written over.
        // An account in use that nothing saved is refused as such first (the token cannot be
        // anyone's we hold), before any question of attribution.
        if !matches!(attributed, refresh::Attribution::Other(Some(_)))
            && matching_accounts(store, Provider::Claude, &current.identity)?.is_empty()
        {
            return Err(UNSAVED_CURRENT.into());
        }
        owner = refresh::owner_to_file(store, state, &current.identity, &current.credential)?;
        copies = matching_accounts(store, Provider::Claude, &owner)?;
        if copies.is_empty() {
            return Err(UNSAVED_CURRENT.into());
        }
        // The live token is the target's own newest generation: Claude Code gets that one.
        if owner.account_id == identity.account_id
            && owner.organization_id == identity.organization_id
            && current.credential.refresh_token.is_some()
            && current.credential.expires_at.unwrap_or(0) >= credential.expires_at.unwrap_or(0)
        {
            credential = current.credential.clone();
        }
    }
    let mut preserve = |outgoing: &external::Outgoing<'_>| -> Result<(), String> {
        let live = external::profile_of(outgoing)?;
        if expected.as_ref().is_some_and(|e| e != &live.identity) {
            return Err("Current Claude account changed during activation. Try again.".into());
        }
        // The bytes read under the locks are the newest generation; Claude Code may have
        // refreshed since the check above, and must still be the same owner's lineage.
        if refresh::owner_to_file(store, state, &live.identity, &live.credential)? != owner {
            return Err("Current Claude account changed during activation. Try again.".into());
        }
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
                Some(owner.clone()),
            )?;
        }
        Ok(())
    };
    (native.activate)(&credential, identity, expected.as_ref(), &mut preserve)?;
    // SB-102: the switch has happened; failing to note its time must not report it as failed
    // (the monitor would set the account now in use aside and try another).
    if store.snapshot().is_ok_and(|s| {
        s.policies.iter().any(|p| {
            p.provider == Provider::Claude && p.pool == account.pool && p.target == "claude_cli"
        })
    }) {
        let _ = store.mark_rotated(
            Provider::Claude,
            &account.pool,
            "claude_cli",
            monitor::now(),
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use switchboard_core::MemoryVault;
    const EXPIRES: i64 = 4_000_000_000;
    pub(crate) const EXPIRES_MS: i64 = EXPIRES * 1000;
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
        assert_eq!(runtime.busy_reason(), None);
        runtime.logins.lock().unwrap().insert(id.clone(), login);
        // SB-105: the idle watch is told why it waits.
        assert_eq!(runtime.busy_reason(), Some("sign_in_waiting"));
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
        assert_eq!(runtime.busy_reason(), None);
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
    async fn a_new_data_folder_restores_the_newest_backup_and_an_existing_one_never_does() {
        struct Key;
        impl switchboard_core::backup::BackupKey for Key {
            fn load(&self) -> Result<Option<[u8; 32]>, String> {
                Ok(Some([7; 32]))
            }
            fn create(&self, _: &[u8; 32]) -> Result<bool, String> {
                Ok(false)
            }
        }
        let backups = tempfile::tempdir().unwrap();
        // The install that wrote the backups.
        let old = tempfile::tempdir().unwrap();
        let first = fixtures::runtime(old.path(), signed_out, activates).await;
        first.enable_backups(Some(Arc::new(Key)), Some(backups.path().to_owned()));
        save(&first.store, "synthetic-a", "default");
        first.maybe_backup(1_000, true).unwrap();
        save(&first.store, "synthetic-b", "default");
        first.maybe_backup(2_000, true).unwrap();
        // A reinstall: a new, empty data folder restores the newest backup on its own.
        let new = tempfile::tempdir().unwrap();
        let second = fixtures::runtime(new.path(), signed_out, activates).await;
        second.enable_backups(Some(Arc::new(Key)), Some(backups.path().to_owned()));
        second.restore_at_start();
        assert_eq!(second.store.snapshot().unwrap().accounts.len(), 2);
        let status = second.execute(Operation::Backups).await.unwrap();
        assert_eq!(status["restored_at_start"]["restored"]["added"], 2);
        // A store that already has accounts is never touched.
        let kept = tempfile::tempdir().unwrap();
        let third = fixtures::runtime(kept.path(), signed_out, activates).await;
        third.enable_backups(Some(Arc::new(Key)), Some(backups.path().to_owned()));
        save(&third.store, "synthetic-c", "default");
        third.restore_at_start();
        assert_eq!(third.store.snapshot().unwrap().accounts.len(), 1);
        assert!(third.execute(Operation::Backups).await.unwrap()["restored_at_start"].is_null());
    }
    #[tokio::test]
    async fn a_third_party_agent_never_starts_on_a_subscription_account() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let oauth = save(&runtime.store, "synthetic-a", "agents");
        runtime
            .store
            .select(Provider::Claude, "agents", &oauth.id)
            .unwrap();
        let repo = tempfile::tempdir().unwrap();
        let launch = |agent: &str| Operation::LaunchAgent {
            agent: agent.into(),
            pool: "agents".into(),
            working_directory: repo.path().to_owned(),
            via: AgentVia::Proxy,
            model: None,
        };
        assert_eq!(
            runtime.execute(launch("goose")).await.unwrap_err(),
            switchboard_proxy::AGENT_SUBSCRIPTION_REFUSED
        );
        assert!(runtime
            .execute(launch("no-such-agent"))
            .await
            .unwrap_err()
            .contains("Unknown agent"));
        // An MCP-only agent has nothing to launch.
        assert!(runtime.execute(launch("cursor-cli")).await.is_err());
        // The key is derived, never stored, and matches what the proxy accepts.
        let key = runtime.execute(Operation::AgentKey).await.unwrap();
        assert_eq!(
            key["key"],
            switchboard_proxy::agent_token(runtime.proxy.token())
        );
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
            base_folders: None,
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
                base_folders: None,
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
    async fn a_project_still_saves_after_one_of_its_folders_is_deleted() {
        // SB-128 / CALL-07: a removed worktree used to block every later edit of its project.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (a, b) = (work.join("a"), work.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let save = |name: &str, folders: Vec<PathBuf>| Operation::SaveProject {
            pool: Some("p".into()),
            name: name.into(),
            folders,
            account_ids: vec![],
            base_folders: None,
        };
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "P".into(),
                folders: vec![a.clone(), b.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        std::fs::remove_dir_all(&b).unwrap();
        let saved = runtime
            .execute(save("P renamed", vec![a.clone(), b.clone()]))
            .await
            .unwrap();
        assert_eq!(saved["name"], "P renamed");
        assert_eq!(saved["folders"].as_array().unwrap().len(), 1);
        assert_eq!(saved["dropped_missing"][0], json!(b));
        // A missing folder that was never the project's is still refused.
        let never = work.join("never");
        assert_eq!(
            runtime
                .execute(save("P", vec![a.clone(), never]))
                .await
                .unwrap_err(),
            "Choose an existing project folder."
        );
    }
    /// A path as a shell command on this system names it: Claude Code's shell on Windows is Git
    /// Bash, where `\` escapes and paths are written with `/`.
    fn shell_path(path: &Path) -> String {
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        if cfg!(windows) {
            text.replace('\\', "/")
        } else {
            text.to_owned()
        }
    }

    #[tokio::test]
    async fn an_unassigned_git_repository_stays_in_the_default_group_and_no_group_is_made() {
        // SB-146 (operator 2026-10-19): REQ-002 made each such repository a group of its own,
        // without accounts, scattering what belonged together. It now stays in the default group.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let repo = work.join("solo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:org/solo.git\n",
        )
        .unwrap();
        let default = save(&runtime.store, "synthetic-default", "default");
        runtime
            .store
            .select(Provider::Claude, "default", &default.id)
            .unwrap();
        let answer = runtime
            .execute(Operation::ProjectSuggest {
                folder: repo.clone(),
            })
            .await
            .unwrap();
        assert_eq!(answer["reason"], "unassigned", "{answer}");
        assert_eq!(answer["message"], suggest::UNASSIGNED, "{answer}");
        let snapshot = runtime.store.snapshot().unwrap();
        assert!(snapshot.projects.is_empty());
        assert!(launch::project_allows(&snapshot, &default, &repo).is_ok());

        // The background pass leaves it alone too.
        let transcript = work.join("session.jsonl");
        std::fs::write(
            &transcript,
            json!({"type": "assistant", "cwd": repo, "message": {"content": [
                {"type": "tool_use", "name": "Edit", "input": {"file_path": repo.join("src/a.rs")}}]}})
                .to_string()
                + "\n",
        )
        .unwrap();
        let now = monitor::now();
        *runtime.transcript_list.lock().unwrap() = Some((now, vec![transcript]));
        runtime.suggest_pass(now).await;
        assert!(runtime.store.snapshot().unwrap().projects.is_empty());
    }

    #[tokio::test]
    async fn a_repository_may_be_in_several_groups_and_sessions_take_the_one_with_accounts() {
        // SB-146: «проект может сразу в несколько групп входить».
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (shared, own) = (work.join("shared"), work.join("own"));
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&own).unwrap();
        for (name, folders) in [
            ("Empty", vec![shared.clone()]),
            ("Agents", vec![own.clone()]),
        ] {
            runtime
                .execute(Operation::SaveProject {
                    pool: None,
                    name: name.into(),
                    folders,
                    account_ids: vec![],
                    base_folders: None,
                })
                .await
                .unwrap();
        }
        let account = save(&runtime.store, "synthetic-agents", "agents");
        let moved = runtime
            .execute(Operation::MoveRoot {
                folder: shared.clone(),
                pool: "agents".into(),
                from: None,
                also: true,
            })
            .await
            .unwrap();
        assert_eq!(moved["also"], true, "{moved}");
        let snapshot = runtime.store.snapshot().unwrap();
        let text = shared.to_string_lossy().into_owned();
        assert!(snapshot
            .projects
            .iter()
            .all(|p| p.folders.contains(&text) || p.pool != "empty"));
        assert_eq!(snapshot.projects_for(&shared).len(), 2);
        // The group with an account is the one a session there works in.
        assert_eq!(snapshot.project_for(&shared).unwrap().pool, "agents");
        assert!(launch::project_allows(&snapshot, &account, &shared).is_ok());
        // One group's folder inside another group's folder stays refused.
        let nested = runtime
            .execute(Operation::SaveProject {
                pool: Some("empty".into()),
                name: "Empty".into(),
                folders: vec![shared.clone(), own.join("sub")],
                account_ids: vec![],
                base_folders: None,
            })
            .await;
        assert!(nested.is_err());
        // Leaving one group keeps it in the other.
        let left = runtime
            .execute(Operation::MoveRoot {
                folder: shared.clone(),
                pool: "agents".into(),
                from: Some("empty".into()),
                also: false,
            })
            .await;
        assert_eq!(
            left.unwrap_err(),
            MOVE_ROOT_LAST,
            "a group keeps one folder"
        );
        let wrong = runtime
            .execute(Operation::MoveRoot {
                folder: own.clone(),
                pool: "agents".into(),
                from: Some("empty".into()),
                also: false,
            })
            .await;
        assert_eq!(wrong.unwrap_err(), MOVE_ROOT_NOT_IN_GROUP);
    }

    #[tokio::test]
    async fn a_repository_a_projects_agent_changes_joins_it_and_a_shared_one_joins_neither() {
        // SB-131: «автодобавление всех репозиториев … когда там агент работает».
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let git = |dir: &Path, origin: &str| {
            std::fs::create_dir_all(dir.join(".git")).unwrap();
            std::fs::create_dir_all(dir.join("src")).unwrap();
            std::fs::write(
                dir.join(".git/config"),
                format!("[remote \"origin\"]\n\turl = git@github.com:{origin}.git\n"),
            )
            .unwrap();
        };
        let (app, other, lib, shared, read_only) = (
            work.join("app"),
            work.join("other"),
            work.join("lib"),
            work.join("shared"),
            work.join("docs"),
        );
        git(&app, "a/app");
        git(&other, "b/other");
        git(&lib, "c/lib");
        git(&shared, "d/shared");
        git(&read_only, "e/docs");
        for (name, folder) in [("App", &app), ("Other", &other)] {
            runtime
                .execute(Operation::SaveProject {
                    pool: None,
                    name: name.into(),
                    folders: vec![folder.clone()],
                    account_ids: vec![],
                    base_folders: None,
                })
                .await
                .unwrap();
        }
        let entry = |cwd: &Path, name: &str, input: Value| -> String {
            json!({"type": "assistant", "cwd": cwd, "message": {"content": [
                {"type": "tool_use", "name": name, "input": input}]}})
            .to_string()
        };
        let transcript = work.join("session.jsonl");
        let lines = [
            entry(&app, "Edit", json!({"file_path": lib.join("src/a.rs")})),
            entry(
                &app,
                "Read",
                json!({"file_path": read_only.join("README.md")}),
            ),
            entry(
                &app,
                "Bash",
                json!({"command": format!("cd {} && git commit -m x", shell_path(&shared))}),
            ),
            entry(
                &other,
                "Write",
                json!({"file_path": shared.join("src/b.rs")}),
            ),
        ];
        std::fs::write(&transcript, lines.join("\n") + "\n").unwrap();
        let seq = runtime.suggestions_seq();
        let snapshot = runtime.store.snapshot().unwrap();
        runtime
            .worked_from(&snapshot, std::slice::from_ref(&transcript), monitor::now())
            .await;
        let folders = |pool: &str| -> Vec<String> {
            runtime
                .store
                .snapshot()
                .unwrap()
                .projects
                .iter()
                .find(|p| p.pool == pool)
                .unwrap()
                .folders
                .clone()
        };
        let lib_text = lib.to_string_lossy().into_owned();
        assert!(folders("app").contains(&lib_text), "{:?}", folders("app"));
        assert_eq!(folders("app").len(), 2, "a repository only read stays out");
        assert_eq!(
            folders("other").len(),
            1,
            "two projects' work joins neither"
        );
        assert!(runtime.suggestions_seq() > seq, "the app is told");
        let listed = runtime
            .execute(Operation::ProjectSuggestions)
            .await
            .unwrap();
        assert_eq!(listed["suggestions"], json!([]), "nothing is asked");
        assert_eq!(listed["added"][0]["reason"], "worked");
        assert_eq!(listed["added"][0]["project"], "App");
        // The person's Edit dialog was opened before `lib` joined: saving it keeps `lib`, and
        // `lib` is not taken for a folder the person removed.
        runtime
            .execute(Operation::SaveProject {
                pool: Some("app".into()),
                name: "App".into(),
                folders: vec![app.clone()],
                account_ids: vec![],
                base_folders: Some(vec![app.clone()]),
            })
            .await
            .unwrap();
        assert!(folders("app").contains(&lib_text), "{:?}", folders("app"));
        let append = |line: String| {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .unwrap();
            std::io::Write::write_all(&mut file, (line + "\n").as_bytes()).unwrap();
        };
        // Another project's agent works in `lib` too: it is shared, leaves App, and work never
        // brings it back.
        append(entry(
            &other,
            "Edit",
            json!({"file_path": lib.join("src/d.rs")}),
        ));
        let snapshot = runtime.store.snapshot().unwrap();
        runtime
            .worked_from(
                &snapshot,
                std::slice::from_ref(&transcript),
                monitor::now() + 1,
            )
            .await;
        assert_eq!(folders("app"), [app.to_string_lossy().into_owned()]);
        assert_eq!(folders("other").len(), 1);
        append(entry(
            &app,
            "Edit",
            json!({"file_path": lib.join("src/e.rs")}),
        ));
        let snapshot = runtime.store.snapshot().unwrap();
        runtime
            .worked_from(
                &snapshot,
                std::slice::from_ref(&transcript),
                monitor::now() + 2,
            )
            .await;
        assert_eq!(folders("app"), [app.to_string_lossy().into_owned()]);
        let rows = suggest::read(root.path()).unwrap();
        assert!(rows
            .iter()
            .any(|r| r.folder == lib_text && r.state == suggest::State::Shared));
        // `shared`, claimed by both projects in one pass, is remembered the same way.
        let shared_text = shared
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(rows
            .iter()
            .any(|r| r.folder == shared_text && r.state == suggest::State::Shared));
        // A dialog opened while `lib` was still in App does not bring it back on Save.
        runtime
            .execute(Operation::SaveProject {
                pool: Some("app".into()),
                name: "App".into(),
                folders: vec![app.clone(), lib.clone()],
                account_ids: vec![],
                base_folders: Some(vec![app.clone(), lib.clone()]),
            })
            .await
            .unwrap();
        assert_eq!(folders("app"), [app.to_string_lossy().into_owned()]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hand_overs_answer_while_a_background_step_holds_the_transaction() {
        // Fabric Dashboards FD-71: `switchboard handovers` waited up to 366 s behind renewals and
        // syncs holding the owner transaction. It reads files and claims by exclusive create.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let held = runtime.hold("test").await;
        let listed = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            runtime.execute(Operation::Handovers),
        )
        .await
        .expect("answered while the transaction is held")
        .unwrap();
        assert_eq!(listed["handovers"], json!([]));
        let taken = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            runtime.execute(Operation::HandoverTake {
                id: "h-nothing".into(),
            }),
        )
        .await
        .expect("a take is answered too");
        assert!(taken.is_err(), "no such hand-over");
        drop(held);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subagent_directories_are_listed_again_only_every_two_minutes() {
        // 2026-10-17: listing ~6,700 subagent transcripts on every 30 s pass held the owner's
        // workers for seconds; the list is kept for SUBAGENTS_EVERY.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let now = monitor::now();
        let first = runtime.work_files(now);
        assert!(first.is_empty(), "a synthetic owner lists no transcript");
        let listed_at = runtime
            .subagent_list
            .lock()
            .unwrap()
            .as_ref()
            .map(|(at, _)| *at);
        assert_eq!(listed_at, Some(now));
        runtime.work_files(now + SUBAGENTS_EVERY - 1);
        let kept = runtime
            .subagent_list
            .lock()
            .unwrap()
            .as_ref()
            .map(|(at, _)| *at);
        assert_eq!(kept, Some(now), "not listed again within the interval");
        runtime.work_files(now + SUBAGENTS_EVERY);
        let again = runtime
            .subagent_list
            .lock()
            .unwrap()
            .as_ref()
            .map(|(at, _)| *at);
        assert_eq!(again, Some(now + SUBAGENTS_EVERY));
        // A clock set back lists again rather than keeping a list from the future.
        runtime.work_files(now - 10);
        let back = runtime
            .subagent_list
            .lock()
            .unwrap()
            .as_ref()
            .map(|(at, _)| *at);
        assert_eq!(back, Some(now - 10));
    }

    #[test]
    fn a_sessions_subagents_count_as_its_work() {
        // SB-131: implementer subagents record their edits under `<session>/subagents/`.
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("s1.jsonl");
        std::fs::write(&session, b"").unwrap();
        std::fs::create_dir_all(dir.path().join("s1/subagents")).unwrap();
        let agent = dir.path().join("s1/subagents/agent-a.jsonl");
        std::fs::write(&agent, b"").unwrap();
        std::fs::write(dir.path().join("s1/subagents/notes.txt"), b"").unwrap();
        let files = with_subagents(std::slice::from_ref(&session), monitor::now());
        assert_eq!(files, [session.clone(), agent]);
        // Written long ago: not part of the recent work.
        let later = monitor::now() + 2 * limits::WINDOW_SECONDS;
        assert_eq!(
            with_subagents(std::slice::from_ref(&session), later),
            [session]
        );
    }

    #[tokio::test]
    async fn a_folder_moved_out_of_its_project_cannot_spend_the_projects_accounts() {
        // SB-133: a renamed project folder kept the project's route and capability in its
        // settings and went on using the project's accounts.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let app = work.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let one = fixtures::save(&runtime.store, "synthetic-one", "default").id;
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![app.clone()],
                account_ids: vec![one],
                base_folders: None,
            })
            .await
            .unwrap();
        runtime.bindings_pass().await;
        let settings = |folder: &Path| -> Value {
            serde_json::from_slice(
                &std::fs::read(folder.join(".claude/settings.local.json")).unwrap(),
            )
            .unwrap()
        };
        let header = settings(&app)["env"]["ANTHROPIC_CUSTOM_HEADERS"]
            .as_str()
            .unwrap()
            .strip_prefix("X-Switchboard-Capability: ")
            .unwrap()
            .to_owned();
        // An authorised request with a query the proxy refuses never reaches a provider: 400
        // after the capability check, 401 before it.
        let ask = |value: String| {
            let address = runtime.proxy.address();
            async move {
                reqwest::Client::new()
                    .post(format!("http://{address}/claude/app/v1/messages?probe=1"))
                    .header(switchboard_proxy::CAPABILITY_HEADER, value)
                    .body("{}")
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .as_u16()
            }
        };
        assert_eq!(
            ask(header.clone()).await,
            400,
            "the folder is the project's"
        );
        let moved = work.join("app-renamed");
        std::fs::rename(&app, &moved).unwrap();
        assert_eq!(
            settings(&moved)["env"]["ANTHROPIC_CUSTOM_HEADERS"],
            format!("X-Switchboard-Capability: {header}"),
            "the moved folder still carries the old value"
        );
        runtime.bindings_pass().await;
        assert_eq!(ask(header).await, 401, "and the proxy no longer takes it");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn taking_a_root_out_of_a_project_takes_its_worktrees_too() {
        // SB-135 (operator 2026-10-18): the main checkout is the project's folder; its worktrees
        // are listed under it and go with it.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (main, wt, other) = (work.join("bot"), work.join("bot-card"), work.join("web"));
        let git = |args: &[&str], dir: &Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        for d in [&main, &other] {
            std::fs::create_dir_all(d).unwrap();
            git(&["init", "-q", "-b", "main"], d);
        }
        git(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.test",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
            &main,
        );
        git(
            &["worktree", "add", "-q", "-b", "card", wt.to_str().unwrap()],
            &main,
        );
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Apps".into(),
                folders: vec![wt.clone(), main.clone(), other.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        let folders = runtime.store.snapshot().unwrap().projects[0]
            .folders
            .clone();
        let tree = roots::tree(&folders);
        assert_eq!(tree.len(), 2, "{tree:?}");
        assert_eq!(tree[0].folder, main.to_string_lossy());
        assert_eq!(tree[0].derived[0].folder, wt.to_string_lossy());
        runtime
            .execute(Operation::ProjectEdit {
                pool: "apps".into(),
                name: None,
                add_accounts: vec![],
                remove_accounts: vec![],
                add_folders: vec![],
                remove_folders: vec![main.clone()],
            })
            .await
            .unwrap();
        let left = runtime.store.snapshot().unwrap().projects[0]
            .folders
            .clone();
        assert_eq!(left, [other.to_string_lossy().into_owned()]);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn a_root_moves_to_another_project_with_its_worktrees_in_one_step() {
        // SB-135 part 2: moving a repository between projects moves its tree; a repository in
        // no project joins; a project keeps its last folder; a repository inside a project
        // folder moves with that folder.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (main, wt, web, docs, loose) = (
            work.join("bot"),
            work.join("bot-card"),
            work.join("web"),
            work.join("docs"),
            work.join("loose"),
        );
        let git = |args: &[&str], dir: &Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        for d in [&main, &web, &docs, &loose] {
            std::fs::create_dir_all(d).unwrap();
            git(&["init", "-q", "-b", "main"], d);
        }
        git(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.test",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
            &main,
        );
        git(
            &["worktree", "add", "-q", "-b", "card", wt.to_str().unwrap()],
            &main,
        );
        for (name, folders) in [
            ("Apps", vec![main.clone(), wt.clone(), web.clone()]),
            ("Docs", vec![docs.clone()]),
        ] {
            runtime
                .execute(Operation::SaveProject {
                    pool: None,
                    name: name.into(),
                    folders,
                    account_ids: vec![],
                    base_folders: None,
                })
                .await
                .unwrap();
        }
        let folders_of = |pool: &str| {
            let mut f = runtime
                .store
                .snapshot()
                .unwrap()
                .project_of_pool(pool)
                .unwrap()
                .folders
                .clone();
            f.sort();
            f
        };
        let text = |p: &Path| p.to_string_lossy().into_owned();
        let moved = runtime
            .execute(Operation::MoveRoot {
                folder: main.clone(),
                pool: "docs".into(),
                from: None,
                also: false,
            })
            .await
            .unwrap();
        assert_eq!(moved["moved"], true);
        assert_eq!(moved["folders"], 2);
        assert_eq!(moved["from"], "apps");
        assert_eq!(folders_of("apps"), [text(&web)]);
        let mut want = vec![text(&main), text(&wt), text(&docs)];
        want.sort();
        assert_eq!(folders_of("docs"), want);
        // The last repository of a project stays.
        let last = runtime
            .execute(Operation::MoveRoot {
                folder: web.clone(),
                pool: "docs".into(),
                from: None,
                also: false,
            })
            .await
            .unwrap_err();
        assert_eq!(last, MOVE_ROOT_LAST);
        // A repository in no project joins one.
        let joined = runtime
            .execute(Operation::MoveRoot {
                folder: loose.clone(),
                pool: "apps".into(),
                from: None,
                also: false,
            })
            .await
            .unwrap();
        assert_eq!(joined["from"], Value::Null);
        assert!(folders_of("apps").contains(&text(&loose)));
        // Inside a project folder: it goes with that folder.
        let nested = docs.join("module");
        std::fs::create_dir_all(&nested).unwrap();
        git(&["init", "-q", "-b", "main"], &nested);
        let inside = runtime
            .execute(Operation::MoveRoot {
                folder: nested,
                pool: "apps".into(),
                from: None,
                also: false,
            })
            .await
            .unwrap_err();
        assert_eq!(inside, MOVE_ROOT_INSIDE);
        // No such project.
        let none = runtime
            .execute(Operation::MoveRoot {
                folder: web,
                pool: "nowhere".into(),
                from: None,
                also: false,
            })
            .await
            .unwrap_err();
        assert_eq!(none, MOVE_ROOT_NO_PROJECT);
        // The map sees all of them, with their projects.
        let map = runtime
            .execute(Operation::RepositoryMap {
                under: vec![work.clone()],
            })
            .await
            .unwrap();
        let rows = map["repositories"].as_array().unwrap();
        let row = |f: &Path| {
            rows.iter()
                .find(|r| r["folder"] == text(f))
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(row(&main)["pool"], "docs");
        assert_eq!(row(&main)["place"], "folder");
        assert!(
            row(&docs.join("module")).is_null(),
            "the walk does not go into a repository"
        );
        assert_eq!(row(&loose)["pool"], "apps");
        assert!(
            row(&wt).is_null(),
            "a linked worktree is listed under its root only"
        );
    }

    #[tokio::test]
    async fn a_project_changes_by_delta_and_keeps_what_is_not_named() {
        // SB-134: «перенеси claude8 в пул агентов» took a whole-set rewrite of 66 folders and
        // 6 accounts; a delta names only what changes.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (a, b, c) = (work.join("a"), work.join("b"), work.join("c"));
        for d in [&a, &b, &c] {
            std::fs::create_dir_all(d).unwrap();
        }
        let one = fixtures::save(&runtime.store, "synthetic-one", "default").id;
        let two = fixtures::save(&runtime.store, "synthetic-two", "default").id;
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![a.clone(), b.clone()],
                account_ids: vec![one.clone()],
                base_folders: None,
            })
            .await
            .unwrap();
        let edit = |add_accounts: Vec<String>,
                    remove_accounts: Vec<String>,
                    add_folders: Vec<PathBuf>,
                    remove_folders: Vec<PathBuf>| Operation::ProjectEdit {
            pool: "app".into(),
            name: None,
            add_accounts,
            remove_accounts,
            add_folders,
            remove_folders,
        };
        let view = runtime
            .execute(edit(
                vec![two.clone()],
                vec![],
                vec![c.clone()],
                vec![b.clone()],
            ))
            .await
            .unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        let project = &snapshot.projects[0];
        let text = |p: &PathBuf| p.to_string_lossy().into_owned();
        assert_eq!(project.folders, [text(&a), text(&c)], "{view}");
        let pool_of = |id: &str| {
            snapshot
                .accounts
                .iter()
                .find(|x| x.id == id)
                .unwrap()
                .pool
                .clone()
        };
        assert_eq!((pool_of(&one), pool_of(&two)), ("app".into(), "app".into()));
        // A folder the person takes out by delta stays out of the automatic adding.
        let rows = suggest::read(root.path()).unwrap();
        assert!(rows
            .iter()
            .any(|r| r.folder == text(&b) && r.state == suggest::State::Dismissed));
        // Taking out what the project does not hold is refused; adding what it holds is no change.
        let refused = runtime
            .execute(edit(
                vec![],
                vec!["synthetic-nobody".into()],
                vec![],
                vec![],
            ))
            .await
            .unwrap_err();
        assert_eq!(refused, PROJECT_EDIT_NO_ACCOUNT);
        let refused = runtime
            .execute(edit(vec![], vec![], vec![], vec![work.join("zzz")]))
            .await
            .unwrap_err();
        assert_eq!(refused, PROJECT_EDIT_NO_FOLDER);
        runtime
            .execute(edit(
                vec![one.clone()],
                vec![two.clone()],
                vec![a.clone()],
                vec![],
            ))
            .await
            .unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        assert_eq!(snapshot.projects[0].folders.len(), 2);
        assert_eq!(
            snapshot.accounts.iter().find(|x| x.id == two).unwrap().pool,
            "default",
            "an account taken out goes back to the default pool"
        );
    }

    #[tokio::test]
    async fn an_account_moves_between_projects_and_back_to_default() {
        // SB-122: «аккаунты между пулами перемещать через UI, три точки, добавить в пул, выбор».
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (a, b) = (work.join("a"), work.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
        }
        let one = fixtures::save(&runtime.store, "synthetic-one", "default").id;
        for (name, folder) in [("App", &a), ("Web", &b)] {
            runtime
                .execute(Operation::SaveProject {
                    pool: None,
                    name: name.into(),
                    folders: vec![folder.clone()],
                    account_ids: vec![],
                    base_folders: None,
                })
                .await
                .unwrap();
        }
        let move_to = |pool: &str| Operation::MoveAccount {
            id: one.clone(),
            pool: pool.into(),
        };
        let pool_of = |runtime: &Runtime| {
            runtime
                .store
                .snapshot()
                .unwrap()
                .accounts
                .iter()
                .find(|x| x.id == one)
                .unwrap()
                .pool
                .clone()
        };
        let view = runtime.execute(move_to("app")).await.unwrap();
        assert_eq!(
            (view["from"].as_str(), view["moved"].as_bool()),
            (Some("default"), Some(true))
        );
        assert_eq!(pool_of(&runtime), "app");
        // From one project straight into another: the first lets it go.
        runtime.execute(move_to("web")).await.unwrap();
        assert_eq!(pool_of(&runtime), "web");
        let snapshot = runtime.store.snapshot().unwrap();
        assert!(snapshot.accounts.iter().all(|x| x.pool != "app"));
        // The same pool is no change; a pool no project uses is refused.
        let same = runtime.execute(move_to("web")).await.unwrap();
        assert_eq!(same["moved"], false);
        assert_eq!(
            runtime.execute(move_to("nowhere")).await.unwrap_err(),
            MOVE_ACCOUNT_NO_PROJECT
        );
        runtime.execute(move_to("default")).await.unwrap();
        assert_eq!(pool_of(&runtime), "default");
        // An account left in a deleted project's pool goes back to default by itself.
        runtime.execute(move_to("app")).await.unwrap();
        runtime
            .execute(Operation::RemoveProject { pool: "app".into() })
            .await
            .unwrap();
        assert_eq!(pool_of(&runtime), "app");
        runtime.execute(move_to("default")).await.unwrap();
        assert_eq!(pool_of(&runtime), "default");
        assert_eq!(
            runtime
                .execute(Operation::MoveAccount {
                    id: "synthetic-nobody".into(),
                    pool: "web".into()
                })
                .await
                .unwrap_err(),
            "Account not found."
        );
    }

    #[tokio::test]
    async fn a_removed_worktree_whose_folder_kept_only_its_claude_settings_leaves_the_group() {
        // SB-152 (operator 2026-10-20): «отдельные проекты это какие-то work trees».
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (main, worktree) = (work.join("app"), work.join("app-wt"));
        for dir in [&main, &worktree] {
            std::fs::create_dir_all(dir.join(".git")).unwrap();
        }
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![main.clone(), worktree.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        // `git worktree remove` takes the checkout; the binding Switchboard wrote stays.
        std::fs::remove_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(worktree.join(".claude")).unwrap();
        std::fs::write(worktree.join(".claude/settings.local.json"), "{}").unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        let t = monitor::now();
        runtime.vanish_pass(&snapshot, t).await;
        runtime.vanish_pass(&snapshot, t + VANISH_CONFIRM).await;
        assert_eq!(
            runtime.store.snapshot().unwrap().projects[0].folders,
            [main.to_string_lossy().into_owned()]
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn a_deleted_project_folder_leaves_its_project_once_it_stays_gone() {
        // SB-131: «следить, чтобы репы, которые в проекте, если они удаляются, то и из свитчера
        // тоже удалять».
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let (main, worktree) = (work.join("app"), work.join("app-wt"));
        for dir in [&main, &worktree] {
            std::fs::create_dir_all(dir.join(".git")).unwrap();
        }
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![main.clone(), worktree.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        let folders = |rt: &Runtime| rt.store.snapshot().unwrap().projects[0].folders.clone();
        let t = monitor::now();
        let snapshot = runtime.store.snapshot().unwrap();
        // Seen present first: its device is what a later disappearance is checked against.
        runtime.vanish_pass(&snapshot, t - VANISH_LOOK).await;
        std::fs::remove_dir_all(&worktree).unwrap();
        runtime.vanish_pass(&snapshot, t).await;
        assert_eq!(folders(&runtime).len(), 2, "one look is not enough");
        // Back before the second look (a checkout being recreated): it stays.
        std::fs::create_dir_all(&worktree).unwrap();
        runtime.vanish_pass(&snapshot, t + VANISH_LOOK).await;
        assert_eq!(folders(&runtime).len(), 2);
        std::fs::remove_dir_all(&worktree).unwrap();
        runtime.vanish_pass(&snapshot, t + 2 * VANISH_LOOK).await;
        runtime
            .vanish_pass(&snapshot, t + 2 * VANISH_LOOK + 10)
            .await;
        assert_eq!(
            folders(&runtime).len(),
            2,
            "looks closer than VANISH_LOOK count once"
        );
        let seq = runtime.suggestions_seq();
        runtime
            .vanish_pass(&snapshot, t + 2 * VANISH_LOOK + VANISH_CONFIRM)
            .await;
        assert_eq!(folders(&runtime), [main.to_string_lossy().into_owned()]);
        assert!(runtime.suggestions_seq() > seq, "the app is told");
        let listed = runtime
            .execute(Operation::ProjectSuggestions)
            .await
            .unwrap();
        assert_eq!(listed["removed"][0]["name"], "app-wt");
        assert_eq!(listed["removed"][0]["project"], "App");
        // The project keeps its last folder even when that one goes too.
        std::fs::remove_dir_all(&main).unwrap();
        let snapshot = runtime.store.snapshot().unwrap();
        let later = t + 10 * VANISH_LOOK;
        runtime.vanish_pass(&snapshot, later).await;
        runtime
            .vanish_pass(&snapshot, later + VANISH_LOOK + VANISH_CONFIRM)
            .await;
        assert_eq!(folders(&runtime).len(), 1);
        // Remembered only as gone, never as declined: a folder that comes back is found again.
        let rows = suggest::read(root.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, suggest::State::Gone);
    }

    #[tokio::test]
    async fn a_projects_new_folders_join_it_by_themselves_and_a_removed_one_stays_out() {
        // SB-121: no question; one record per folder for the app's notification.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let git = |dir: &Path| {
            std::fs::create_dir_all(dir.join(".git")).unwrap();
            std::fs::write(
                dir.join(".git/config"),
                "[remote \"origin\"]\n\turl = git@github.com:org/app.git\n",
            )
            .unwrap();
        };
        let main = work.join("app");
        git(&main);
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![main.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        // A question left by an earlier version is answered by the first pass.
        let older = work.join("app-older");
        git(&older);
        let snapshot = runtime.store.snapshot().unwrap();
        let repos = suggest::project_repos(&snapshot);
        assert!(matches!(
            suggest::record(root.path(), &snapshot, &repos, &older, 1).unwrap(),
            suggest::Outcome::New(_)
        ));
        // A new clone beside it is found by the same pass.
        let clone = work.join("app-2");
        git(&clone);
        let seq = runtime.suggestions_seq();
        let now = monitor::now();
        runtime.suggest_pass(now).await;
        runtime.suggest_pass(now + 1).await;
        let folders = |rt: &Runtime| -> Vec<String> {
            rt.store.snapshot().unwrap().projects[0].folders.clone()
        };
        let joined = folders(&runtime);
        assert_eq!(joined.len(), 3, "{joined:?}");
        assert!(runtime.suggestions_seq() > seq, "the app is told");
        let listed = runtime
            .execute(Operation::ProjectSuggestions)
            .await
            .unwrap();
        assert_eq!(listed["suggestions"], json!([]), "nothing is asked");
        assert_eq!(listed["added"].as_array().unwrap().len(), 2);
        assert_eq!(listed["added"][0]["project"], "App");
        // The person takes the clone out: it does not come back.
        runtime
            .execute(Operation::SaveProject {
                pool: Some("app".into()),
                name: "App".into(),
                folders: vec![main.clone(), older.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        let answer = runtime
            .execute(Operation::ProjectSuggest {
                folder: clone.clone(),
            })
            .await
            .unwrap();
        assert_eq!(answer["reason"], "declined");
        assert_eq!(folders(&runtime).len(), 2);
        // A folder the person added by hand and later removes stays out as well.
        let manual = work.join("app-manual");
        git(&manual);
        let with_manual = vec![main.clone(), older.clone(), manual.clone()];
        runtime
            .execute(Operation::SaveProject {
                pool: Some("app".into()),
                name: "App".into(),
                folders: with_manual,
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: Some("app".into()),
                name: "App".into(),
                folders: vec![main.clone(), older.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        let answer = runtime
            .execute(Operation::ProjectSuggest { folder: manual })
            .await
            .unwrap();
        assert_eq!(answer["reason"], "declined", "{answer}");
        assert_eq!(folders(&runtime).len(), 2);
        // Removing the whole project remembers its folders too.
        runtime
            .execute(Operation::RemoveProject { pool: "app".into() })
            .await
            .unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App again".into(),
                folders: vec![main.clone()],
                account_ids: vec![],
                base_folders: None,
            })
            .await
            .unwrap();
        let answer = runtime
            .execute(Operation::ProjectSuggest {
                folder: older.clone(),
            })
            .await
            .unwrap();
        assert_eq!(answer["reason"], "declined", "{answer}");
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
    /// Paired with `a_named_b_held`: what the real adapter reads back from Claude Code's own
    /// files during activation — B's token in the auth, A's name in the settings.
    fn activates_a_named_b_held(
        _: &Credential,
        _: &ExternalIdentity,
        expected: Option<&ExternalIdentity>,
        preserve: &mut dyn FnMut(&external::Outgoing<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        if expected.is_some() {
            let auth = json!({"claudeAiOauth":{"accessToken":"synthetic-b-token","refreshToken":"synthetic-b-refresh","expiresAt":EXPIRES_MS}}).to_string();
            let config = json!({"oauthAccount":{"accountUuid":"synthetic-a","organizationUuid":"synthetic-org","emailAddress":"synthetic-a@example.invalid"}}).to_string();
            preserve(&external::Outgoing {
                auth: auth.as_bytes(),
                config: config.as_bytes(),
            })?;
        }
        Ok(())
    }
    #[tokio::test]
    async fn a_foreign_lineage_under_this_name_is_filed_under_its_owner_and_the_switch_goes_on() {
        // Claude Code's settings name A, its token is B's (the provider says so): the switch to
        // C files the live token under B, leaves A's own lineage alone and goes on.
        let root = tempfile::tempdir().unwrap();
        let runtime =
            fixtures::runtime(root.path(), a_named_b_held, activates_a_named_b_held).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        let b = save(&runtime.store, "synthetic-b", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        runtime
            .refresh
            .learn_owner("synthetic-b-refresh", identity("synthetic-b"));
        runtime
            .execute(Operation::ActivateNative { id: c.id.clone() })
            .await
            .unwrap();
        assert_eq!(
            runtime.store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-token",
            "A keeps its own lineage"
        );
        let b_now = runtime.store.stored_credential(&b.id).unwrap();
        assert_eq!(b_now.refresh_token.as_deref(), Some("synthetic-b-refresh"));
        let b_row = runtime
            .store
            .snapshot()
            .unwrap()
            .accounts
            .into_iter()
            .find(|x| x.id == b.id)
            .unwrap();
        assert_eq!(
            b_row.external_identity.unwrap().account_id.as_deref(),
            Some("synthetic-b"),
            "B's row keeps B's identity"
        );
        let events = runtime.store.snapshot().unwrap().events;
        assert!(events
            .iter()
            .any(|e| e.action == "activation" && e.detail == "completed"));
    }
    /// A finished official sign-in of `account` with a fresh token (SB-62 tests).
    fn fresh_sign_in(account: &str) -> external::CapturedProfile {
        let mut credential = credential(account);
        credential.access_token = format!("{account}-fresh-token");
        credential.refresh_token = Some(format!("{account}-fresh-refresh"));
        external::CapturedProfile {
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            credential,
            identity: identity(account),
            label: format!("{account}@example.invalid"),
        }
    }
    fn accounts_of(store: &Store) -> Vec<Account> {
        store.snapshot().unwrap().accounts
    }
    #[tokio::test]
    async fn a_sign_in_without_a_label_updates_the_saved_account_where_it_is() {
        // SB-62: the banner's Try again (no label, default pool) used to rename the account to its
        // email in `default`, or copy an account saved elsewhere into `default`.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = runtime
            .store
            .upsert(
                "Work A".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "work".into(),
                credential("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let (account, again) = file_sign_in(
            &runtime.store,
            Provider::Claude,
            "",
            "default",
            fresh_sign_in("synthetic-a"),
        )
        .unwrap();
        assert!(again);
        assert_eq!(account.id, work.id);
        let all = accounts_of(&runtime.store);
        assert_eq!(all.len(), 1, "no copy appears in default");
        assert_eq!(
            all[0].label, "Work A",
            "the label is not replaced by the email"
        );
        assert_eq!(all[0].pool, "work");
        assert_eq!(
            runtime
                .store
                .stored_credential(&work.id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-a-fresh-refresh")
        );
    }
    #[tokio::test]
    async fn a_sign_in_without_a_label_keeps_a_custom_label_in_the_same_pool() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        runtime
            .store
            .upsert(
                "Personal".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        file_sign_in(
            &runtime.store,
            Provider::Claude,
            "",
            "default",
            fresh_sign_in("synthetic-a"),
        )
        .unwrap();
        let all = accounts_of(&runtime.store);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].label, "Personal");
    }
    #[tokio::test]
    async fn a_new_identity_is_added_under_its_email_in_the_requested_pool() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        save(&runtime.store, "synthetic-b", "default");
        let (account, again) = file_sign_in(
            &runtime.store,
            Provider::Claude,
            "",
            "default",
            fresh_sign_in("synthetic-a"),
        )
        .unwrap();
        assert!(!again);
        assert_eq!(account.label, "synthetic-a@example.invalid");
        assert_eq!(account.pool, "default");
        assert_eq!(accounts_of(&runtime.store).len(), 2);
    }
    #[tokio::test]
    async fn a_row_sign_in_updates_its_row_and_every_other_copy_of_the_identity() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = save(&runtime.store, "synthetic-a", "work");
        let personal = runtime
            .store
            .upsert(
                "Mine".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "personal".into(),
                credential("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let (account, again) = file_sign_in(
            &runtime.store,
            Provider::Claude,
            "synthetic-a",
            "work",
            fresh_sign_in("synthetic-a"),
        )
        .unwrap();
        assert!(again);
        assert_eq!(account.id, work.id);
        assert_eq!(accounts_of(&runtime.store).len(), 2);
        for id in [&work.id, &personal.id] {
            assert_eq!(
                runtime
                    .store
                    .stored_credential(id)
                    .unwrap()
                    .refresh_token
                    .as_deref(),
                Some("synthetic-a-fresh-refresh"),
                "one lineage in every pool"
            );
        }
        let mine = accounts_of(&runtime.store)
            .into_iter()
            .find(|a| a.id == personal.id)
            .unwrap();
        assert_eq!(mine.label, "Mine");
    }
    #[tokio::test]
    async fn fallback_chains_take_a_preset_refuse_agents_that_cannot_continue_and_resolve() {
        // SB-71: the operator's order as a preset, a task's own chain, and an agent that loads no
        // MCP server (Aider) refused before anything is stored.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let set = runtime
            .execute(Operation::SetChain {
                scope: switchboard_core::ChainScope::Machine,
                executors: vec![],
                preset: Some("subscriptions-first".into()),
            })
            .await
            .unwrap();
        let agents: Vec<&str> = set["chain"]["executors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["agent"].as_str().unwrap())
            .collect();
        assert_eq!(agents, ["claude-code", "codex", "kimi-code", "hermes"]);
        let refused = runtime
            .execute(Operation::SetChain {
                scope: switchboard_core::ChainScope::Machine,
                executors: vec![switchboard_core::Executor {
                    agent: "aider".into(),
                    account_id: None,
                    key: None,
                }],
                preset: None,
            })
            .await
            .unwrap_err();
        assert!(refused.contains("cannot take over a workflow"), "{refused}");
        runtime
            .execute(Operation::SetChain {
                scope: switchboard_core::ChainScope::Workflow {
                    id: "wf_0123456789abcdef".into(),
                },
                executors: vec![switchboard_core::Executor {
                    agent: "codex".into(),
                    account_id: None,
                    key: None,
                }],
                preset: None,
            })
            .await
            .unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: Some("wf_0123456789abcdef".into()),
                pool: None,
            })
            .await
            .unwrap();
        assert_eq!(view["effective"]["executors"][0]["agent"], "codex");
        assert_eq!(view["chains"].as_array().unwrap().len(), 2);
        assert_eq!(view["presets"][0]["id"], "subscriptions-first");
        let machine = runtime
            .execute(Operation::Chains {
                workflow: None,
                pool: None,
            })
            .await
            .unwrap();
        assert_eq!(machine["effective"]["scope"]["kind"], "machine");
        assert!(runtime
            .execute(Operation::SetChain {
                scope: switchboard_core::ChainScope::Machine,
                executors: vec![],
                preset: Some("no-such-preset".into()),
            })
            .await
            .is_err());
    }
    #[tokio::test]
    async fn a_stored_copy_alone_never_moves_the_live_token_to_another_account() {
        // Only a stored copy of B calls the live token B's, and the provider cannot be asked:
        // that copy may be the misfiled one, so nothing is filed under B — B's sign-in in its
        // other pool is not overwritten — and the switch is refused as unconfirmed.
        let root = tempfile::tempdir().unwrap();
        let runtime =
            fixtures::runtime(root.path(), a_named_b_held, activates_a_named_b_held).await;
        let a = save(&runtime.store, "synthetic-a", "default");
        save(&runtime.store, "synthetic-b", "default");
        let b_elsewhere = runtime
            .store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "work".into(),
                credential("synthetic-b-own"),
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let c = save(&runtime.store, "synthetic-c", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: c.id.clone() })
                .await
                .unwrap_err(),
            refresh::UNCONFIRMED_LIVE
        );
        assert_eq!(
            runtime
                .store
                .stored_credential(&b_elsewhere.id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-b-own-refresh"),
            "B's real sign-in in its other pool is untouched"
        );
        assert_eq!(
            runtime.store.stored_credential(&a.id).unwrap().access_token,
            "synthetic-a-token"
        );
    }
    #[tokio::test]
    async fn a_live_token_of_an_account_switchboard_does_not_hold_is_never_written_over() {
        // The token is held by no saved copy and the provider is unreachable: nothing attributes
        // it, so the switch is refused rather than losing that sign-in.
        let root = tempfile::tempdir().unwrap();
        let runtime =
            fixtures::runtime(root.path(), a_named_b_held, activates_a_named_b_held).await;
        save(&runtime.store, "synthetic-a", "default");
        let c = save(&runtime.store, "synthetic-c", "default");
        assert_eq!(
            runtime
                .execute(Operation::ActivateNative { id: c.id.clone() })
                .await
                .unwrap_err(),
            refresh::UNCONFIRMED_LIVE
        );
    }
    #[tokio::test]
    async fn capturing_another_accounts_lineage_under_this_name_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let runtime =
            fixtures::runtime(root.path(), a_named_b_held, activates_a_named_b_held).await;
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

    /// Starts an owner, also on a store another owner of this test just released. Tests beside it
    /// start child processes; a child forked at that moment shares the store lock's open file
    /// until it execs, so the lock can stay taken for a few milliseconds after the release (seen
    /// on the Linux arm64 release build of 0.6.20, as #161 saw for the storage tests).
    async fn owner(root: &Path) -> Owner {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match Owner::start(root.to_owned(), Arc::new(MemoryVault::default())).await {
                Ok(owner) => return owner,
                // Only the lock a forked child still holds is waited out; anything else fails
                // at once.
                Err(error)
                    if error.contains("Another Switchboard instance")
                        && std::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(error) => panic!("{error}"),
            }
        }
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
        // Another program took the port while Switchboard was closed. A parallel test can grab
        // the freed port first (seen once on Linux x64, nightly 38641898912): then it is taken
        // all the same, so a bind that keeps failing with AddrInUse still sets up the case.
        let squatter = {
            let mut held = None;
            for _ in 0..50 {
                match std::net::TcpListener::bind(old) {
                    Ok(l) => {
                        held = Some(l);
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        tokio::time::sleep(Duration::from_millis(20)).await
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            held
        };
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
        // The store is free for the next owner. The lock goes with the last handle on the store,
        // which a background task of the drained owner may drop a moment later (release run
        // 38967680225, Linux): free within a second.
        let free = (0..50).any(|_| {
            let open = Store::open(tmp.path().to_owned(), Arc::new(MemoryVault::default()));
            if open.is_err() {
                std::thread::sleep(Duration::from_millis(20));
            }
            open.is_ok()
        });
        assert!(free, "the store is free within a second of the stop");
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
                label: Some("renamed".into()),
                enabled: Some(true),
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
    async fn a_scratch_data_folder_never_sees_the_real_backups() {
        // SB-61: without an owner, only the default data folder falls back to the backup folder.
        // A `--data-dir` store lists none and restores none — the real folder is never read.
        let root = tempfile::tempdir().unwrap();
        assert_ne!(default_root().ok().as_deref(), Some(root.path()));
        assert_eq!(fallback_backup_dir(root.path()), None);
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let listed = execute(store.clone(), root.path(), None, Operation::Backups)
            .await
            .unwrap();
        assert!(listed["directory"].is_null());
        assert_eq!(listed["backups"], json!([]));
        assert_eq!(listed["enabled"], false);
        let restored = execute(
            store.clone(),
            root.path(),
            None,
            Operation::RestoreBackup {
                file: "switchboard-backup-20261007T000000Z.sbbackup".into(),
            },
        )
        .await;
        assert_eq!(restored.unwrap_err(), BACKUPS_DEFAULT_FOLDER_ONLY);
        assert!(store.snapshot().unwrap().accounts.is_empty());
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
