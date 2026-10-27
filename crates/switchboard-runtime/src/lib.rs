//! One store/proxy/control owner shared by the desktop and CLI.
pub mod agent_catalog;
pub mod agent_keys;
pub mod agent_settings;
pub mod agents;
pub mod analytics;
pub mod attribution;
mod bindings;
mod blocking;
mod carry;
mod chain_view;
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
pub mod launches;
mod limits;
mod monitor;
pub mod onboarding;
pub mod oplog;
pub mod person;
pub mod projects;
pub mod recovery;
mod refresh;
pub mod roots;
pub mod settings_bundle;
pub mod shell;
pub mod spend;
mod statistics;
pub mod suggest;
pub mod uninstall;
mod usage_gate;
pub mod vesta;
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
        /// SB-175: `true` marks the account as used on another machine (disabled here), `false`
        /// clears the mark (enabled again).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        elsewhere: Option<bool>,
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
    /// SB-212: a launch operation (`Launch`, `Continue`, `ContinueToConsole`, `LaunchAgent`,
    /// `KimiLaunch`) with who asked for it. The owner checks `by` and `parent`, turns the
    /// caller's folder into its group and records the rest on the launch.
    Launched {
        by: launches::LaunchedBy,
        inner: Box<Operation>,
    },
    /// SB-212: what a launched session's own hook reports (`SessionStart`, `Stop`, a limit).
    LaunchEvent {
        launch_id: String,
        event: launches::Event,
        #[serde(default)]
        provider_session_id: Option<String>,
        #[serde(default)]
        transcript: Option<PathBuf>,
    },
    /// SB-212: every launch record, newest first; with `launch_id`, that launch and its chain.
    Launches {
        #[serde(default)]
        launch_id: Option<String>,
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
        /// SB-109: the group's organization by id or name, or `personal`; left out, a saved
        /// group keeps its own.
        #[serde(default)]
        organization: Option<String>,
    },
    /// Deletes a group. `by` names the surface (C-Q7): a group holding a spending ceiling is
    /// deleted only by the window, since its ceiling goes with it; the CLI and MCP are refused.
    /// Left out, a request counts as the CLI's.
    RemoveProject {
        pool: String,
        #[serde(default = "spend::agent_surface")]
        by: spend::Surface,
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
        /// SB-109: as in `SaveProject`.
        #[serde(default)]
        organization: Option<String>,
    },
    /// SB-143: whether Codex sessions in a project's folders use its Codex accounts (one
    /// user-level provider at the proxy). `enabled` none: the state only.
    CodexRoute {
        #[serde(default)]
        enabled: Option<bool>,
    },
    /// SB-158: the organizations and where each account belongs, with the rule that decided.
    Organizations,
    OrganizationAdd {
        name: String,
        #[serde(default)]
        domains: Vec<String>,
    },
    /// What is left out stays; `domains` replaces the list.
    OrganizationEdit {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        domains: Option<Vec<String>>,
    },
    OrganizationRemove {
        id: String,
    },
    /// `choice`: an organization's id, `personal`, or none to let the rules decide again.
    OrganizationAssign {
        account: String,
        #[serde(default)]
        choice: Option<String>,
    },
    /// SB-159: the PassionCode.ai connection (no token in the answer).
    VestaStatus,
    /// Opens the browser sign-in and waits for it (at most 5 minutes), then reads the
    /// organizations.
    VestaConnect,
    /// Stops a connection still waiting for the browser.
    VestaCancel,
    /// Reads the organizations again now.
    VestaSync,
    /// Deletes the token; the organizations stay as local ones.
    VestaDisconnect,
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
    /// SB-176: this machine's settings as a `switchboard-settings/1` bundle (no credential).
    SettingsExport,
    /// SB-176: what importing `bundle` would do here, finding checkouts under `under` (`~/DATA`
    /// by default); with `apply`, does it.
    SettingsImport {
        bundle: Value,
        #[serde(default)]
        under: Vec<PathBuf>,
        #[serde(default)]
        apply: bool,
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
        /// C-Q9: replacing the own key of a group that holds a spending ceiling is the window's.
        /// Left out, a request counts as the CLI's.
        #[serde(default = "spend::agent_surface")]
        by: spend::Surface,
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
    /// Removes the saved key and its vault item. C-Q9: for a group that holds a spending ceiling,
    /// only the window removes it. Left out, `by` counts as the CLI's.
    AgentKeyRemove {
        service: String,
        #[serde(default)]
        pool: Option<String>,
        #[serde(default = "spend::agent_surface")]
        by: spend::Surface,
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
    /// A Claude Code session finished a turn (`Stop` hook, SB-204): its live stop is resumed.
    RecoveryContinued {
        session: String,
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
    /// With `folder` (a workflow's checkout) the group is found as the fallback finds it
    /// (`fallback::chain_pool`), `pool` standing for the executor account's group.
    Chains {
        #[serde(default)]
        workflow: Option<String>,
        #[serde(default)]
        pool: Option<String>,
        #[serde(default)]
        folder: Option<PathBuf>,
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
    /// Spending ceilings for paid fallback agents (SB-72): every ceiling, the open questions and
    /// the last hand-over (`spend::view`). An unreadable `spend.json` is an error, never "none".
    Ceilings,
    /// Sets the ceiling of a scope in dollars (machine and group: per UTC day; a task: since its
    /// first paid hand-over); `usd: null` clears it, and a body without `usd` is refused. `by`
    /// names the surface: the CLI and MCP may set a first ceiling or a lower one, raising and
    /// clearing are the window's (C-Q1), and `by: window` from outside the app's own process is
    /// refused like a ceiling answer.
    SetCeiling {
        scope: switchboard_core::ChainScope,
        #[serde(deserialize_with = "spend::present")]
        usd: Option<f64>,
        by: spend::Surface,
    },
    /// Answers a reached ceiling's question: `keep` from any surface, `double` only from the
    /// window (`by: window`, C-Q1). The CLI and MCP send their own surface.
    AnswerCeiling {
        id: String,
        answer: spend::Answer,
        by: spend::Surface,
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
    /// SB-143: Codex's own `config.toml`; None for a synthetic owner, which never writes it.
    pub(crate) codex_config: fn() -> Option<PathBuf>,
    /// SB-183: what an organization's shell home carries of the person's (their folders and
    /// board server); nothing for a synthetic owner.
    pub(crate) shell: shell::homes::Sources,
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
    codex_config: || None,
    shell: shell::homes::NO_SOURCES,
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
    codex_config: codex_route::config_path,
    shell: shell::homes::NATIVE,
};
impl NativeSources {
    /// The read for an operation a person asked for.
    pub(crate) fn fresh(&self, provider: Provider) -> Result<external::CapturedProfile, String> {
        (self.fresh.unwrap_or(self.current))(provider)
    }
}
/// The `shell_hook` log event of one installer run: counts and flags only (LC-12).
fn log_shell_hook(report: &shell::install::Report) {
    oplog::event(
        "shell_hook",
        &[
            ("wanted", oplog::Field::Flag(report.wanted)),
            ("written", oplog::Field::Number(report.written as i64)),
            ("removed", oplog::Field::Number(report.removed as i64)),
            ("skipped", oplog::Field::Number(report.skipped as i64)),
            ("changed", oplog::Field::Number(report.changed as i64)),
            ("no_hook", oplog::Field::Flag(report.no_hook)),
        ],
    );
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
    /// SB-183: the person's home for the terminal hook's start files; set only by the installed
    /// app (`enable_shell_hook`), so a test, `serve` or a development build never writes one.
    shell_hook: Mutex<Option<shell::install::Host>>,
    /// Whether a group bound to an organization existed at the last look, for the hook's
    /// default (written when the first appears, taken out when the last leaves).
    shell_present: Mutex<Option<bool>>,
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
    /// SB-156: the groups' folders resolved to projects, rebuilt only when the groups' folders
    /// change: `(digest of every group's folders, index)`.
    stat_index: Mutex<(u64, Arc<statistics::ProjectIndex>)>,
    /// SB-121/SB-131: when the folders that could not be added are tried again (never asked).
    suggest_retry_at: std::sync::atomic::AtomicI64,
    /// SB-131: project folders seen missing, and since when; one still missing a while later
    /// leaves its project.
    vanished: Mutex<Vanish>,
    /// SB-131: refusals already logged this run (a folder that cannot be added, a project's
    /// last folder gone), so a retry every few minutes logs each once.
    told: Mutex<HashSet<String>>,
    /// The recent transcript list the limit scan reads, walked once per pass: `(when, files)`.
    transcript_list: Mutex<Option<(i64, Vec<PathBuf>)>>,
    /// SB-159: stops the PassionCode.ai sign-in waiting for the browser.
    vesta_cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    vanish_at: std::sync::atomic::AtomicI64,
    /// OpenRouter's model list, kept six hours (SB-119).
    openrouter_models: Mutex<agent_settings::ModelCache>,
    /// What the project folders' Claude Code route was last synced for (SB-118).
    bindings_key: Mutex<Option<u64>>,
    /// PRD-13: the folders with the person's own `ANTHROPIC_*` values and the folders skipped at
    /// the last sync, so unchanged sets are not logged again (`binding_changed`).
    binding_user_values: Mutex<Option<BindingSets>>,
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
        // H-2: `control_slow` names the time a request waited for the store's lock.
        store.observe_lock_waits(control::lock_waited);
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
            stat_index: Mutex::new((0, Arc::default())),
            suggest_retry_at: std::sync::atomic::AtomicI64::new(0),
            vanished: Mutex::new(Vanish::default()),
            told: Mutex::new(HashSet::new()),
            transcript_list: Mutex::new(None),
            vesta_cancel: Mutex::new(None),
            vanish_at: std::sync::atomic::AtomicI64::new(0),
            bindings_key: Mutex::new(None),
            binding_user_values: Mutex::new(None),
            bindings_lock: tokio::sync::Mutex::new(()),
            openrouter_models: Mutex::new(agent_settings::ModelCache::default()),
            bindings_repos: Mutex::new(bindings::RepoCache::default()),
            usage_gate,
            shell_hook: Mutex::new(None),
            shell_present: Mutex::new(None),
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
        // SB-212: a launch runs with who asked for it, and its answer names its launch id.
        if let Operation::Launched { by, inner } = operation {
            if !is_launch(&inner) {
                return Err(launches::NOT_A_LAUNCH.into());
            }
            let by = launches::resolve(&self.store.snapshot()?, by)?;
            return launches::scoped(Some(by), Box::pin(self.execute(*inner))).await;
        }
        if is_launch(&operation) && !launches::capturing() {
            return launches::scoped(None, Box::pin(self.execute(operation))).await;
        }
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
                // A file read; spend.json is changed only by the operations below.
                | Operation::Ceilings
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
                | Operation::SettingsExport
                | Operation::Statistics { .. }
                | Operation::Organizations
                // SB-159: each takes the store's lock only to write what it read; a connection
                // waits minutes for the browser and must not hold the owner.
                | Operation::VestaStatus
                | Operation::VestaConnect
                | Operation::VestaCancel
                | Operation::VestaSync
                | Operation::VestaDisconnect
                | Operation::AgentsOverview
                | Operation::OpenrouterModels
                // Hand-overs are files: listing reads them, and taking one is claimed by an
                // exclusive create (`handovers::take`). Behind the owner transaction a console's
                // read waited out renewals and syncs for up to 366 s (Fabric Dashboards FD-71).
                | Operation::Handovers
                | Operation::HandoverTake { .. }
                // The recovery file is written by rename and the limits live in memory; behind
                // the transaction a host's read waited 12 to 15 s for a renewal (SB-166).
                | Operation::Recovery
                // SB-150: a session's own stop report (Claude Code's `StopFailure` hook) writes
                // the recovery file under its own lock (`recovery::writing`); behind the owner
                // transaction the hook waited out its 10 s and the stop went unrecorded.
                | Operation::RecoveryHook { .. }
                // SB-204: the same file and lock, after a turn of a stopped session.
                | Operation::RecoveryContinued { .. }
                // SB-212: a launch record is one small file written under its own lock; a
                // session's hook gives up after 1.5 s and must not wait out the transaction.
                | Operation::LaunchEvent { .. }
                | Operation::Launches { .. }
        ) {
            return execute(self.store.clone(), &self.root, Some(self), operation).await;
        }
        // SB-186: a session's start hook (`project suggest --from-hook`) gives up after 1.5 s;
        // behind a background step it waited 5 to 22 s (2026-10-23 and 2026-10-24) and its folder
        // joined nothing. Most starts are in a folder already in its group, or in no group's:
        // those answers write nothing and are given here. Only a folder to record and add takes
        // the transaction.
        if let Operation::ProjectSuggest { folder } = &operation {
            if let Some(answer) =
                blocking::run(|| suggest_read_only(&self.store, &self.root, folder))?
            {
                return Ok(answer);
            }
        }
        // A quota check reads the network and writes only its own observation; it takes the
        // owner transaction only for a renewal, as the background pass does, so a slow
        // provider never holds every other operation (SB-39).
        if let Operation::Usage { id } = &operation {
            return Ok(json!(
                monitor::check_by_person(
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
        // C-Q8: a paid start by hand takes the hand-over turn and passes its spend gate before
        // the owner transaction, the order the automatic fallback takes them in: its gate and
        // launch run together and never beside a hand-over's, and a slow spend read holds up no
        // other operation. Only a start that is paid takes the turn; its account is resolved
        // first (`paid_start`). A fallback step already holds it and passed its own gate
        // (`fallback::gated`).
        let mut operation = operation;
        let paid = if fallback::gated() {
            None
        } else {
            self.paid_start(&mut operation)?
        };
        let _turn = match &paid {
            Some((workflow, dir, kind)) => {
                let turn = fallback::HAND_OVER.lock().await;
                fallback::gate_by_hand(&self.store, &self.root, workflow.as_deref(), dir, *kind)
                    .await?;
                Some(turn)
            }
            None => None,
        };
        // The Store mutex protects metadata, but launch and removal also mutate
        // private homes. Keep the complete operation in one owner transaction.
        let _mutation = self.mutations.lock().await;
        let reported = Reported::of(&operation, &self.store);
        let run = execute(self.store.clone(), &self.root, Some(self), operation);
        // The gate passed above: the operation's own gate is not run again.
        let result = if paid.is_some() {
            fallback::as_gated(run).await
        } else {
            run.await
        };
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
                // SB-183: the organizations' shell homes and the folder map follow the groups'
                // organizations and Codex accounts too.
                shell::homes::key(&snapshot, &self.native.shell).hash(&mut h);
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
                self.proxy.set_released_tokens(report.released.clone());
                self.proxy.set_stat_folders(stat_folders(&snapshot));
                // SB-155: a sync that changed nothing is not an event (178 a day on 2026-10-20).
                let changed = match self.binding_user_values.lock() {
                    Ok(mut last) => binding_changed(&report, &mut last),
                    Err(_) => true,
                };
                // SB-183: the organizations' shell homes and the terminal hook's folder map.
                // A refusal is a code in the log, never an answer to anyone's request.
                let shell = blocking::run(|| {
                    shell::homes::sync(
                        &self.root,
                        &snapshot,
                        &report.bound_folders,
                        &address,
                        &self.proxy.auto_capability(),
                        &self.native.shell,
                    )
                });
                if shell.homes_written > 0
                    || shell.map_written
                    || shell.failed > 0
                    || shell.map_refused
                {
                    oplog::event(
                        "shell_homes",
                        &[
                            ("rows", oplog::Field::Number(shell.rows as i64)),
                            (
                                "homes_written",
                                oplog::Field::Number(shell.homes_written as i64),
                            ),
                            ("failed", oplog::Field::Number(shell.failed as i64)),
                            (
                                "map",
                                oplog::Field::Code(if shell.map_refused {
                                    "refused"
                                } else if shell.map_written {
                                    "written"
                                } else {
                                    "unchanged"
                                }),
                            ),
                        ],
                    );
                }
                // The hook's default follows the map just written (one rule with `shell status`).
                self.shell_hook_follow(blocking::run(|| {
                    shell::map::organizations_present(&self.root)
                }));
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
    /// C-Q8: whether `operation` is a paid start by hand, and what its spend gate needs: the
    /// workflow (a continue), the folder whose key it spends and what it spends. The account is
    /// resolved first: `Continue` on the default agent is given the account chosen here, so the
    /// operation runs on the account that was gated; `LaunchAgent` through the proxy on the
    /// pool's selected account, which is paid only when it is an API-key account (a subscription
    /// account is refused by the launch itself, never as an unknown spend). A start the operation
    /// refuses before its gate (arguments without `--in-place`, an in-place continue on Windows)
    /// is not gated here; the refusals the gate preceded run here in the same order.
    fn paid_start(
        &self,
        operation: &mut Operation,
    ) -> Result<Option<(Option<String>, PathBuf, fallback::Paid)>, String> {
        let root = &self.root;
        match operation {
            Operation::LaunchAgent {
                working_directory,
                via: AgentVia::Openrouter,
                ..
            } => Ok(Some((
                None,
                projects::project_dir(root, working_directory)?,
                fallback::Paid::OpenRouter,
            ))),
            Operation::LaunchAgent {
                pool,
                working_directory,
                via: AgentVia::Proxy,
                ..
            } => {
                let api_key = self
                    .store
                    .selected_account(Provider::Claude, pool)
                    .is_some_and(|a| a.kind == AuthKind::ApiKey);
                if !api_key {
                    return Ok(None);
                }
                Ok(Some((
                    None,
                    projects::project_dir(root, working_directory)?,
                    fallback::Paid::ApiKeyAccount,
                )))
            }
            Operation::Launch {
                id,
                working_directory,
                in_place,
                args,
                ..
            } => {
                if !*in_place && !args.is_empty() {
                    return Ok(None);
                }
                let Some(kind) = fallback::paid_kind(&self.store.snapshot()?, id) else {
                    return Ok(None);
                };
                Ok(Some((
                    None,
                    projects::project_dir(root, working_directory)?,
                    kind,
                )))
            }
            Operation::Continue {
                workflow_id,
                id,
                working_directory,
                in_place,
                ..
            } => {
                if *in_place && cfg!(windows) {
                    return Ok(None);
                }
                if id.is_empty() {
                    *id = continue_default(root, self, workflow_id, working_directory)?;
                }
                let Some(kind) = fallback::paid_kind(&self.store.snapshot()?, id) else {
                    return Ok(None);
                };
                let directory =
                    continue_checks(&self.store, root, workflow_id, id, working_directory)?;
                Ok(Some((Some(workflow_id.clone()), directory, kind)))
            }
            _ => Ok(None),
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
        for (s, result) in
            found
                .iter()
                .zip(add_suggested_folders(&self.store, &self.root, &found, time))
        {
            match result {
                Ok(()) => added += 1,
                Err(error) => {
                    if self.first_time(&format!("add:{}", s.folder)) {
                        oplog::event(
                            "project_suggestion",
                            &[
                                ("add", oplog::Field::Code("refused")),
                                ("reason", oplog::Field::Code(add_refusal(&error))),
                            ],
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
        hold_on(&self.mutations, step).await
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
        // SB-171 (operator 2026-10-22): a repository never joins a group because a group's agent
        // worked in it; a folder joins only as a worktree or clone of a repository already in the
        // group, through a source the person set, or by the person's own act.
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
        let mut transaction = self.hold("suggest_found").await;
        let Ok(fresh) = self.store.snapshot() else {
            return;
        };
        transaction.phase("snapshot");
        // Projects changed meanwhile: what was decided may be stale. The scanner starts over on
        // the new projects at the next pass (`collect`), so nothing found is lost.
        if suggest::projects_key(&fresh) != suggest::projects_key(&snapshot) {
            return;
        }
        let found = blocking::run(|| match self.suggest.lock() {
            Ok(mut scanner) => Some(scanner.settle_assessed(&self.root, assessed, time)),
            Err(_) => None,
        });
        transaction.phase("settle");
        if let Some(found) = found {
            blocking::run(|| self.add_found(found, time));
            transaction.phase("add");
        }
    }
    /// The recent Claude Code transcripts the limit scan reads, walked at most once per
    /// background pass.
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
    /// SB-131: a project folder deleted from disk leaves its project once it is still missing
    /// VANISH_CONFIRM later. A folder whose drive is not mounted, or whose whole tree up to the
    /// filesystem root is gone, is not deleted as far as Switchboard can tell, and stays; a
    /// project keeps its last folder.
    /// SB-156: hands the proxy's counts to the statistics with each folder already resolved to
    /// its project, so a count outlives the checkout it came from.
    pub(crate) fn absorb_statistics(&self) {
        let new = self.proxy.take_statistics();
        if new.is_empty() {
            return;
        }
        let Ok(snapshot) = self.store.snapshot() else {
            self.statistics.absorb(new);
            return;
        };
        let digest = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for p in &snapshot.projects {
                p.pool.hash(&mut h);
                p.folders.hash(&mut h);
            }
            h.finish()
        };
        let cached = self
            .stat_index
            .lock()
            .ok()
            .filter(|c| c.0 == digest && digest != 0)
            .map(|c| c.1.clone());
        let index = match cached {
            Some(index) => index,
            None => {
                let built = Arc::new(blocking::run(|| statistics::ProjectIndex::build(&snapshot)));
                if let Ok(mut c) = self.stat_index.lock() {
                    *c = (digest, built.clone());
                }
                built
            }
        };
        // SB-173: a folder no group holds (a session in a folder taken out of its group) still
        // counts under its repository.
        let strays: Vec<String> = new
            .keys()
            .filter_map(|k| switchboard_proxy::stats::parts(k).map(|p| p.folder.to_owned()))
            .filter(|f| !f.is_empty() && index.project_of(f) == *f)
            .collect();
        let index = if strays.is_empty() {
            index
        } else {
            let base = (*index).clone();
            Arc::new(blocking::run(move || base.with_strays(&strays)))
        };
        self.statistics.absorb_resolved(new, &index);
    }
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
        // SB-160 (B-4): the receipt reads the account as stored now (a repeated Finish after a
        // move reads where it went), and says `in_cli` when the ordinary CLI is signed in to it,
        // as `accounts capture` does: such an account cannot join a group until the CLI switches.
        let receipt = |account: &Account, cleaned: bool, again: bool| {
            let snapshot = self.store.snapshot().ok();
            let stored = snapshot
                .as_ref()
                .and_then(|s| s.accounts.iter().find(|a| a.id == account.id))
                .unwrap_or(account);
            let mut value = json!(stored);
            value["login_cleanup"] = json!(if cleaned { "done" } else { "pending" });
            value["signed_in_again"] = json!(again);
            if let Some(mut offer) = snapshot
                .as_ref()
                .and_then(|s| organization_offer(s, stored))
            {
                let current = self.current_accounts().unwrap_or(Value::Null);
                if ["claude", "codex"].iter().any(|p| {
                    current[*p]["account_ids"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|v| v == stored.id.as_str()))
                }) {
                    offer["in_cli"] = json!(true);
                }
                value["organization_group"] = offer;
            }
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
    /// the App Key) owning the real data folder. `launch` is `ordinary` or `background`. Until
    /// the person has answered the switch in this app, the start is held and nothing is sent.
    /// SB-183 (node A-3): the installed app's owner writes the terminal hook's start-file
    /// blocks. Only for the app's own data folder; the app calls it only from an installed
    /// build (never a development build or the smoke check), and `serve` never does.
    /// The app must also run from `/Applications` or `~/Applications` on macOS: a bundle opened
    /// from a download or build folder (or translocated) is not installed.
    pub fn enable_shell_hook(self: &Arc<Self>) {
        self.enable_shell_hook_when(
            || {
                std::env::current_exe()
                    .and_then(|exe| exe.canonicalize())
                    .is_ok_and(|exe| {
                        shell::install::installed_app(&exe, dirs::home_dir().as_deref())
                    })
            },
            || shell::install::Host::process(shell::install::hook_cli()),
        );
    }
    /// `enable_shell_hook` with the installed check and the person's home given: the data
    /// folder is checked first, then `installed`, and only then is `host` asked for.
    pub(crate) fn enable_shell_hook_when(
        &self,
        installed: impl FnOnce() -> bool,
        host: impl FnOnce() -> Option<shell::install::Host>,
    ) {
        if !shell::install::is_default_root(&self.root) {
            return;
        }
        if !installed() {
            return;
        }
        if let Some(host) = host() {
            self.enable_shell_hook_with(host);
        }
    }
    pub(crate) fn enable_shell_hook_with(&self, host: shell::install::Host) {
        if let Ok(mut slot) = self.shell_hook.lock() {
            *slot = Some(host);
        }
        let present = blocking::run(|| shell::map::organizations_present(&self.root));
        self.shell_hook_apply(present);
    }
    /// The first-time notice: `{organization, group}` of the organization's group whose
    /// presence wrote the first block, kept in `shell-hook.json` until the window acknowledges
    /// it (a restart in between keeps it). The window shows it (node A-7).
    /// Only the installed app's owner (the one holding the person's home) answers it, so a
    /// development build or `serve` over the same data folder neither posts nor acknowledges it.
    pub fn shell_hook_notice(&self) -> Option<Value> {
        if !self.shell_hook.lock().is_ok_and(|h| h.is_some()) {
            return None;
        }
        shell::install::pending_notice(&self.root)
            .map(|n| json!({"organization": n.organization, "group": n.group}))
    }
    /// SB-183 (node A-7): the window's *Terminals in organization folders* section, the same
    /// state `switchboard shell status` reads (the hook, the choice, the folder map, each start
    /// file) without the terminal's own fields, plus `can_change` (this owner is the installed
    /// app, so the window's switch works), the organizations whose groups the hook follows by
    /// name, the organization groups it does not follow and why (`unmapped`, SB-222), and the
    /// person's home for showing start files as `~/...`.
    pub fn shell_hook_status(&self) -> Value {
        let root = self.root.clone();
        let mut value = blocking::run(move || shell::map::status(Some(&root), None, &|_| None));
        if let Some(object) = value.as_object_mut() {
            for field in ["terminal", "here", "folder"] {
                object.remove(field);
            }
        }
        value["can_change"] = json!(self.shell_hook.lock().is_ok_and(|h| h.is_some()));
        value["organizations"] = json!(self.shell_hook_organizations(false));
        value["codex_organizations"] = json!(self.shell_hook_organizations(true));
        // SB-222: organization groups the map holds no row for, and why, for the window's line.
        value["unmapped"] = json!(self
            .store
            .snapshot()
            .map(|snapshot| blocking::run(move || shell::homes::unmapped(&snapshot)))
            .unwrap_or_default());
        value["home"] = json!(dirs::home_dir().and_then(|h| h.to_str().map(str::to_owned)));
        value
    }
    /// The pools of the folder map's valid rows (with `codex`, only rows whose pool has a Codex
    /// account), each once, in the map's order: the groups the hook follows now.
    fn shell_hook_pools(&self, codex: bool) -> Vec<String> {
        let shell::map::Loaded::Read(map) = blocking::run(|| shell::map::read(&self.root)) else {
            return Vec::new();
        };
        let mut pools: Vec<String> = Vec::new();
        for row in map.valid_rows().filter(|r| !codex || r.codex) {
            if !pools.contains(&row.pool) {
                pools.push(row.pool.clone());
            }
        }
        pools
    }
    /// The names of the organizations whose groups the folder map holds (with `codex`, only
    /// rows that carry a Codex account), sorted, each once: the same rule as the hook's default.
    fn shell_hook_organizations(&self, codex: bool) -> Vec<String> {
        let pools = self.shell_hook_pools(codex);
        let Ok(snapshot) = self.store.snapshot() else {
            return Vec::new();
        };
        let names: std::collections::BTreeSet<String> = pools
            .iter()
            .filter_map(|pool| snapshot.project_of_pool(pool))
            .filter_map(|p| snapshot.organization(p.organization.as_deref()?))
            .map(|o| o.name.clone())
            .collect();
        names.into_iter().collect()
    }
    /// The window's *Turn off* / *Turn on*: saves the choice and brings every start file in line
    /// with it now, as `switchboard shell off|on` does, then answers [`Self::shell_hook_status`].
    /// Only the installed app's owner holds the person's home; any other owner (a development
    /// build, `serve`, another data folder) refuses and changes nothing.
    pub fn set_shell_hook(&self, on: bool) -> Result<Value, String> {
        let Some(host) = self.shell_hook.lock().ok().and_then(|h| h.clone()) else {
            return Err(shell::install::NOT_INSTALLED.into());
        };
        let root = self.root.clone();
        let present = blocking::run(|| shell::map::organizations_present(&self.root));
        let report = blocking::run(move || -> Result<shell::install::Report, String> {
            shell::install::set_choice(&root, on)?;
            Ok(shell::install::apply(&root, &host, present, on))
        })?;
        if let Ok(mut last) = self.shell_present.lock() {
            *last = Some(present);
        }
        log_shell_hook(&report);
        if on && report.no_hook {
            return Err(shell::install::NO_PATH.into());
        }
        Ok(self.shell_hook_status())
    }
    /// The window showed the first-time notice: it is never shown again.
    pub fn acknowledge_shell_hook_notice(&self) {
        if !self.shell_hook.lock().is_ok_and(|h| h.is_some()) {
            return;
        }
        let root = self.root.clone();
        blocking::run(move || shell::install::acknowledge_notice(&root));
    }
    /// Runs the start-file installer when the presence of an organization's group is new or
    /// flipped; never on an idle pass, never without the installed app's home.
    fn shell_hook_follow(&self, present: bool) {
        let changed = match self.shell_present.lock() {
            Ok(last) => *last != Some(present),
            Err(_) => false,
        };
        if changed {
            self.shell_hook_apply(present);
        }
    }
    fn shell_hook_apply(&self, present: bool) {
        if let Ok(mut last) = self.shell_present.lock() {
            *last = Some(present);
        }
        let Some(host) = self.shell_hook.lock().ok().and_then(|h| h.clone()) else {
            return;
        };
        let report = blocking::run(|| shell::install::apply(&self.root, &host, present, false));
        log_shell_hook(&report);
        if report.first_written && shell::map::choice(&self.root) == shell::map::Choice::Default {
            // The group named is one whose folder the map holds: its presence wrote the block.
            let pools = self.shell_hook_pools(false);
            let notice = self.store.snapshot().ok().and_then(|s| {
                let group = pools.iter().find_map(|pool| {
                    s.project_of_pool(pool).filter(|p| {
                        p.organization
                            .as_deref()
                            .is_some_and(|o| s.organization(o).is_some())
                    })
                })?;
                let organization = s.organization(group.organization.as_deref()?)?;
                Some(shell::install::Notice {
                    organization: organization.name.clone(),
                    group: group.name.clone(),
                })
            });
            if let Some(notice) = notice {
                blocking::run(|| shell::install::set_notice(&self.root, notice));
            }
        }
    }
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
    /// `{available, enabled, disclosed, sent_before}` for the first-run notice and About.
    /// Unavailable outside a release desktop build, where no notice is shown.
    pub fn analytics_status(&self) -> Value {
        self.analytics().map_or_else(
            || json!({"available": false, "enabled": false, "disclosed": false, "sent_before": false}),
            |a| a.status(),
        )
    }
    /// The person's switch, shared by every PassionCode app on this machine, and their answer
    /// in this app (issue #128): until it, nothing is sent. `question` is the switch the question
    /// showed when this is its answer (an "on" given after another app changed the switch since
    /// is not written back); `None` is About's switch, which always sets it.
    pub fn set_analytics(&self, enabled: bool, question: Option<bool>) -> Result<Value, String> {
        let client = self.analytics().ok_or_else(|| {
            "Usage analytics are available in the installed app only.".to_string()
        })?;
        match question {
            Some(shown) => client.answer(enabled, shown),
            None => client.set_enabled(enabled),
        }
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
        let want = bindings::desired(&snapshot);
        proxy.set_folder_tokens(bindings::folder_tokens(&want, proxy.token(), &salts));
        // SB-172: folders taken out lately keep their sessions, on the default pool.
        proxy.set_released_tokens(bindings::released_tokens(
            root,
            &[],
            &want,
            proxy.token(),
            &salts,
            monitor::now(),
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
            &owner_started_fields(std::process::id(), runtime.proxy.address().port()),
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
        // SB-179: grants run outside the transaction, and no new one starts once the drain
        // began; a grant still on the network (a quota check from the window, the CLI or MCP)
        // records its successor before it ends, so the drain waits for them, then stores every
        // kept successor. Before, the drain waited for the transaction such a grant held.
        if transaction.is_ok() {
            drained &= runtime.refresh.grants_ended(left(deadline)).await;
            refresh::adopt_all_kept(&runtime.store, &runtime.refresh);
        }
        drained &= runtime
            .proxy
            .stop(left(std::time::Duration::from_secs(2)))
            .await;
        // SB-143: no Codex should be pointed at a proxy that is gone.
        runtime.codex_route_sync(false);
        runtime.absorb_statistics();
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
/// Why a found folder could not join its group, as a fixed code for the log (2026-10-22: the
/// log said only "refused", so a refusal could not be traced).
fn add_refusal(error: &str) -> &'static str {
    if error == switchboard_core::FOLDER_NESTED {
        "nested"
    } else if error.starts_with("This folder is gone") || error.contains("existing project folder")
    {
        "gone"
    } else if error.starts_with("Project not found") {
        "group_removed"
    } else if error.contains("at most 1024 folders") {
        "too_many_folders"
    } else if error == PROJECT_ACCOUNT_IN_CLI {
        "account_in_cli"
    } else {
        "other"
    }
}
/// SB-180: the folders one pass found, added with one `save_project` per group (each save
/// publishes `accounts.json` and syncs the bindings) and one write of the suggestion list. The
/// answer is one result per suggestion, in order. A group whose save is refused falls back to one
/// save per folder, so one folder that cannot join does not keep the others out.
fn add_suggested_folders(
    store: &Store,
    root: &Path,
    found: &[suggest::Suggestion],
    now: i64,
) -> Vec<Result<(), String>> {
    let mut results: Vec<Option<Result<(), String>>> = found.iter().map(|_| None).collect();
    let mut pools: Vec<&str> = found.iter().map(|s| s.pool.as_str()).collect();
    pools.sort_unstable();
    pools.dedup();
    for pool in pools {
        let mut batch: Vec<(usize, PathBuf)> = Vec::new();
        for (i, s) in found.iter().enumerate().filter(|(_, s)| s.pool == pool) {
            if !Path::new(&s.folder).is_dir() {
                results[i] = Some(Err(
                    "This folder is gone. Read the suggestions again.".into()
                ));
                continue;
            }
            match projects::project_dir(root, Path::new(&s.folder)) {
                Ok(admitted) => batch.push((i, admitted)),
                Err(error) => results[i] = Some(Err(error)),
            }
        }
        if batch.is_empty() {
            continue;
        }
        let saved = store.snapshot().and_then(|snapshot| {
            let project = snapshot
                .projects
                .iter()
                .find(|p| p.pool == pool)
                .ok_or("Project not found.")?;
            let mut folders: Vec<PathBuf> = project.folders.iter().map(PathBuf::from).collect();
            folders.extend(batch.iter().map(|(_, f)| f.clone()));
            let accounts: Vec<String> = snapshot
                .accounts
                .iter()
                .filter(|a| a.pool == project.pool)
                .map(|a| a.id.clone())
                .collect();
            store.save_project(Some(&project.pool), &project.name, &folders, &accounts)
        });
        match saved {
            Ok(_) => {
                let folders: Vec<&str> = batch
                    .iter()
                    .map(|(i, _)| found[*i].folder.as_str())
                    .collect();
                if suggest::mark_added_all(root, &folders, now).is_err() {
                    oplog::event(
                        "project_suggestion",
                        &[("remember", oplog::Field::Code("failed"))],
                    );
                }
                for (i, _) in &batch {
                    results[*i] = Some(Ok(()));
                }
            }
            Err(_) if batch.len() > 1 => {
                for (i, _) in &batch {
                    results[*i] =
                        Some(add_suggested_folder(store, root, &found[*i], now).map(|_| ()));
                }
            }
            Err(error) => results[batch[0].0] = Some(Err(error)),
        }
    }
    results
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err("Project not found.".into())))
        .collect()
}
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

/// SB-186: `ProjectSuggest`'s answer when it writes nothing (the folder is in its group, or no
/// group claims it); None when the folder must be recorded and added, under the transaction.
fn suggest_read_only(store: &Store, root: &Path, folder: &Path) -> Result<Option<Value>, String> {
    let snapshot = store.snapshot()?;
    let repos = suggest::project_repos(&snapshot);
    let outcome = match suggest::assess(&snapshot, &repos, folder) {
        suggest::Assessed::Ours { .. } => return Ok(None),
        suggest::Assessed::Decided(suggest::Outcome::NotOurs { .. }) => {
            suggest::unassigned(root, &snapshot, &repos, folder)?
        }
        suggest::Assessed::Decided(outcome) => outcome,
    };
    Ok(Some(
        json!({"suggested": false, "reason": outcome.code(), "message": outcome.message()}),
    ))
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
        organization: None,
    })
}
/// SB-109: a group's organization as the person names it (its id, its name in any case, or
/// `personal`) → the store's choice: `Some(None)` personal, `Some(Some(id))` an organization.
fn group_organization(
    snapshot: &switchboard_core::Snapshot,
    named: Option<&str>,
) -> Result<Option<Option<String>>, String> {
    let Some(named) = named.map(str::trim) else {
        return Ok(None);
    };
    if named.is_empty() || named.eq_ignore_ascii_case(switchboard_core::organizations::PERSONAL) {
        return Ok(Some(None));
    }
    snapshot
        .organizations
        .iter()
        .find(|o| o.id == named || o.name.eq_ignore_ascii_case(named))
        .map(|o| Some(Some(o.id.clone())))
        .ok_or_else(|| switchboard_core::organizations::ORGANIZATION_NOT_FOUND.into())
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
/// What the previous bindings sync left as it was (PRD-13): the folders holding the person's own
/// `ANTHROPIC_*` values and the folders skipped, each sorted.
type BindingSets = (Vec<String>, Vec<String>);
/// Whether a bindings sync is worth a `claude_binding` line (SB-155): a folder bound or removed,
/// or a change in the set of folders skipped or holding the person's own values since `last`,
/// the sets at the previous sync (PRD-13: an unchanged set logged every sync; N1 round 1: the
/// skipped set too). The first sync logs non-empty sets once.
fn binding_changed(report: &bindings::Report, last: &mut Option<BindingSets>) -> bool {
    let sorted = |list: &[String]| {
        let mut list = list.to_vec();
        list.sort();
        list.dedup();
        list
    };
    let sets = (sorted(&report.user_values_present), sorted(&report.skipped));
    let sets_changed = match last.as_ref() {
        Some(previous) => *previous != sets,
        None => !sets.0.is_empty() || !sets.1.is_empty(),
    };
    *last = Some(sets);
    sets_changed || report.bound + report.removed > 0
}
/// The fields of `owner_started`: the process, the proxy's port and the owner's version (PRD-12).
fn owner_started_fields(pid: u32, proxy_port: u16) -> [(&'static str, oplog::Field); 3] {
    [
        ("pid", oplog::Field::Number(pid.into())),
        ("proxy_port", oplog::Field::Number(proxy_port.into())),
        ("version", oplog::Field::Code(env!("CARGO_PKG_VERSION"))),
    ]
}
/// The owner transaction, held by a background step, logged when held long (2026-10-17: reads
/// waited minutes behind it). `step` is a fixed code naming the holder.
pub(crate) struct Held<'a> {
    _guard: tokio::sync::MutexGuard<'a, ()>,
    step: &'static str,
    since: std::time::Instant,
    /// SB-163: where the time went, as `phase` marks: each phase's milliseconds since the mark
    /// before it, logged beside a long hold.
    phases: Vec<(&'static str, i64)>,
    last: std::time::Instant,
}
impl Held<'_> {
    /// SB-163: ends a phase of this step: the time since the previous mark is logged under
    /// `name` when the whole hold is long.
    pub(crate) fn phase(&mut self, name: &'static str) {
        let now = std::time::Instant::now();
        self.phases
            .push((name, now.duration_since(self.last).as_millis() as i64));
        self.last = now;
    }
}
impl Held<'_> {
    /// The phases a long hold logs: each marked phase, then `rest`, the time after the last
    /// mark (PRD-02: 14 s of a hold were in `millis` and in no phase). A hold with no marks has
    /// only `millis`.
    fn fields(&self) -> Vec<(&'static str, i64)> {
        let mut fields = self.phases.clone();
        if !fields.is_empty() {
            fields.push(("rest", self.last.elapsed().as_millis() as i64));
        }
        fields
    }
}
#[cfg(test)]
thread_local! {
    /// The phases of the last hold this thread let go, long or not, for tests.
    pub(crate) static LAST_HOLD: std::cell::RefCell<Vec<(&'static str, i64)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
impl Drop for Held<'_> {
    fn drop(&mut self) {
        let held = self.since.elapsed();
        let phases = self.fields();
        #[cfg(test)]
        LAST_HOLD.with(|last| *last.borrow_mut() = phases.clone());
        if held >= HELD_LONG {
            let mut fields = vec![
                ("step", oplog::Field::Code(self.step)),
                ("millis", oplog::Field::Number(held.as_millis() as i64)),
            ];
            fields.extend(
                phases
                    .iter()
                    .map(|(name, ms)| (*name, oplog::Field::Number(*ms))),
            );
            oplog::event("transaction_held", &fields);
        }
    }
}
/// The owner transaction `lock`, logged under `step` when held long (SB-163); also for a caller
/// that holds only the mutex, such as a renewal outside the transaction (SB-179).
pub(crate) async fn hold_on<'a>(lock: &'a tokio::sync::Mutex<()>, step: &'static str) -> Held<'a> {
    let guard = lock.lock().await;
    let now = std::time::Instant::now();
    Held {
        _guard: guard,
        step,
        since: now,
        phases: Vec::new(),
        last: now,
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
/// SB-160 (B-4, SCN-077): the one group of the account's organization when the account landed
/// outside it, `{pool, name}`, for the person to move it there (nothing moves here). None for a
/// personal account, an organization with no group or several, an account already in it, or one
/// whose identity the group already holds under another account.
fn organization_offer(snapshot: &switchboard_core::Snapshot, account: &Account) -> Option<Value> {
    let organization = snapshot.attribution(account).organization?;
    let group = snapshot.organization_group(&organization)?;
    let held = account.external_identity.as_ref().is_some_and(|identity| {
        snapshot.accounts.iter().any(|a| {
            a.pool == group.pool
                && a.provider == account.provider
                && a.external_identity.as_ref().is_some_and(|i| {
                    // The core store's own identity test (`ExternalIdentity::matches`).
                    i.account_id.is_some()
                        && i.account_id == identity.account_id
                        && i.organization_id == identity.organization_id
                })
        })
    });
    (group.pool != account.pool && !held).then(|| json!({"pool": group.pool, "name": group.name}))
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
/// SB-158: organizations with their account counts, and each account's attribution.
pub fn organizations_view(snapshot: &switchboard_core::Snapshot) -> Value {
    let accounts: Vec<Value> = snapshot
        .accounts
        .iter()
        .map(|a| {
            let at = snapshot.attribution(a);
            let name = at
                .organization
                .as_deref()
                .and_then(|id| snapshot.organization(id))
                .map(|o| o.name.clone());
            json!({"id": a.id, "label": a.label, "provider": a.provider, "pool": a.pool,
                "organization": at.organization, "organization_name": name, "by": at.by})
        })
        .collect();
    let organizations: Vec<Value> = snapshot
        .organizations
        .iter()
        .map(|o| {
            let count = accounts
                .iter()
                .filter(|a| a["organization"].as_str() == Some(o.id.as_str()))
                .count();
            json!({"id": o.id, "name": o.name, "domains": o.domains,
                "provider_organizations": o.provider_organizations.len(), "accounts": count,
                "created_at": o.created_at, "passioncode": o.vesta_id.is_some()})
        })
        .collect();
    json!({"schema": 1, "organizations": organizations, "accounts": accounts})
}
/// C-Q1: the window answers in the app's own process (`Runtime::execute`). An operation that
/// names the window but arrives through the control listener or an offline command (anything
/// holding the control token, an agent running the CLI included) is refused before it runs.
/// A session's record as the list shows it (SB-95, SB-110, SB-201): the account's label, the
/// organization's name and the agent it runs.
fn label_session(snapshot: &switchboard_core::Snapshot, mut session: Value) -> Value {
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
    // SB-110: an organization's context also by its name, for the CLI and agents.
    let context_name = session["context"]
        .as_str()
        .and_then(|c| c.strip_prefix("org:"))
        .and_then(|id| snapshot.organization(id))
        .map(|o| o.name.clone());
    session["context_name"] = context_name.into();
    // SB-201: one field names the agent. A catalog agent's record carries it; a
    // launch of a provider's own CLI is named from its provider.
    if !session["agent"].is_string() {
        let agent = match session["provider"].as_str() {
            Some("claude") => Some("claude-code"),
            Some("kimi") => Some("kimi-code"),
            Some(other @ ("codex" | "hermes" | "opencode")) => Some(other),
            _ => None,
        };
        session["agent"] = agent.into();
    }
    session
}
/// SB-212: the operations that launch a session and get a launch record.
fn is_launch(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Launch { .. }
            | Operation::Continue { .. }
            | Operation::ContinueToConsole { .. }
            | Operation::LaunchAgent { .. }
            | Operation::KimiLaunch { .. }
    )
}
/// SB-212: `operation` with who asked for it, when it launches a session; any other operation
/// as it is.
pub fn launched(operation: Operation, by: launches::LaunchedBy) -> Operation {
    if is_launch(&operation) {
        Operation::Launched {
            by,
            inner: Box::new(operation),
        }
    } else {
        operation
    }
}
/// SB-212: a launch that reached the owner from outside its process (the control listener, an
/// offline command) never says the window asked for it: a bare one is the command line's, and
/// `window` becomes `cli`.
pub(crate) fn from_outside(operation: Operation) -> Operation {
    match operation {
        Operation::Launched { mut by, inner } => {
            if by.surface == launches::Surface::Window {
                by.surface = launches::Surface::Cli;
            }
            Operation::Launched { by, inner }
        }
        operation if is_launch(&operation) => Operation::Launched {
            by: launches::LaunchedBy::surface(launches::Surface::Cli),
            inner: Box::new(operation),
        },
        operation => operation,
    }
}
pub(crate) fn window_only_refusal(operation: &Operation) -> Option<&'static str> {
    matches!(
        operation,
        Operation::AnswerCeiling {
            by: spend::Surface::Window,
            ..
        } | Operation::SetCeiling {
            by: spend::Surface::Window,
            ..
        } | Operation::RemoveProject {
            by: spend::Surface::Window,
            ..
        } | Operation::AgentKeySet {
            by: spend::Surface::Window,
            ..
        } | Operation::AgentKeyRemove {
            by: spend::Surface::Window,
            ..
        }
    )
    .then_some(spend::WINDOW_ONLY)
}
pub async fn execute_offline(root: PathBuf, operation: Operation) -> Result<Value, String> {
    if let Some(refusal) = window_only_refusal(&operation) {
        return Err(refusal.into());
    }
    let store = Arc::new(Store::open(root.clone(), Arc::new(NativeVault::new()))?);
    execute(store, &root, None, from_outside(operation)).await
}
/// How long a command waits for the store when another process holds it (SB-181): an update
/// restart keeps the lock through the old owner's drain and frees it once the new owner runs,
/// and Fabric Dashboards gives each read 10 s.
pub const STORE_WAIT: std::time::Duration = std::time::Duration::from_secs(8);
/// One command from the CLI or `switchboard mcp`: to the running owner, else offline. While the
/// store's lock is held and no owner answers (an owner draining or starting, another offline
/// command), it asks again every 200 ms, the owner first, until `STORE_WAIT` has passed.
pub async fn call(root: &Path, operation: Operation) -> Result<Value, String> {
    call_within(root, operation, STORE_WAIT, execute_offline).await
}
async fn call_within<F, Fut>(
    root: &Path,
    operation: Operation,
    wait: std::time::Duration,
    offline: F,
) -> Result<Value, String>
where
    F: Fn(PathBuf, Operation) -> Fut,
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    // `Operation` is not Clone (it may carry a sign-in's answers): a second attempt rebuilds it
    // from its own bytes.
    let body = serde_json::to_vec(&operation).map_err(|_| "Operation unavailable.")?;
    let started = std::time::Instant::now();
    let mut operation = operation;
    loop {
        if let Some(value) = control::request(root, &operation).await? {
            return Ok(value);
        }
        match offline(root.to_owned(), operation).await {
            Err(error) if error == switchboard_core::STORE_LOCKED && started.elapsed() < wait => {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                operation = serde_json::from_slice(&body).map_err(|_| "Operation unavailable.")?;
            }
            result => return result,
        }
    }
}
/// SB-120: the account a `Continue` that names none runs on, the machine's default agent:
/// OpenCode and Hermes run on the OpenRouter key; Claude Code, Codex and Kimi Code on the account a
/// one-step chain of that agent would choose now. Resolved once, before the spend gate (C-Q8).
fn continue_default(
    root: &Path,
    runtime: &Runtime,
    workflow_id: &str,
    working_directory: &Path,
) -> Result<String, String> {
    match agent_settings::read(root)?.default.as_deref() {
        Some(hermes::HERMES_TARGET) => Ok(hermes::HERMES_TARGET.to_owned()),
        Some(launch::OPENCODE_TARGET) => Ok(launch::OPENCODE_TARGET.to_owned()),
        Some(agent) => {
            agent_settings::usable(root, agent)?;
            if !continuation::valid_workflow_id(workflow_id) {
                return Err(continuation::BAD_WORKFLOW_ID.into());
            }
            let folder = working_directory
                .canonicalize()
                .map_err(|_| "Choose an existing project directory.")?;
            // The executor is read first, so its own exhausted account is never the one chosen.
            let shown = continuation::show(&continuation::observatory()?, workflow_id)?;
            fallback::account_for_default(
                runtime,
                agent,
                &continuation::executor_of(&shown),
                &folder,
                monitor::now(),
            )
        }
        None => Err(agent_settings::NAME_AN_ACCOUNT.into()),
    }
}
/// The refusals a `Continue` on `id` meets before its spend gate and offer: an agent the person
/// disabled for taking over work (SB-120), a malformed workflow id, a folder that does not exist.
/// Answers the canonical folder.
fn continue_checks(
    store: &Store,
    root: &Path,
    workflow_id: &str,
    id: &str,
    working_directory: &Path,
) -> Result<PathBuf, String> {
    let agent = if id == hermes::HERMES_TARGET || id == launch::OPENCODE_TARGET {
        Some(id.to_owned())
    } else if store.kimi_account(id).is_ok() {
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
    if !continuation::valid_workflow_id(workflow_id) {
        return Err(continuation::BAD_WORKFLOW_ID.into());
    }
    working_directory
        .canonicalize()
        .map_err(|_| "Choose an existing project directory.".to_string())
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
        // PRD-12: `owner_version` names the owner build that answered (the client's may differ;
        // null offline, where no owner answered) and `online` says whether an owner runs,
        // beside the display string `live_mode`.
        Operation::Status => Ok(
            json!({"proxy_address": runtime.map(|r| r.proxy.address().to_string()), "platform": std::env::consts::OS, "live_mode": if runtime.is_some() { "Next request · HTTP/SSE" } else { "Offline" }, "owner_version": runtime.map(|_| env!("CARGO_PKG_VERSION")), "online": runtime.is_some()}),
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
            let mut receipt = json!(account);
            // SB-160: the ordinary CLI's account cannot join a group until the CLI switches.
            if let Some(mut offer) = organization_offer(&store.snapshot()?, &account) {
                offer["in_cli"] = json!(true);
                receipt["organization_group"] = offer;
            }
            Ok(receipt)
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
        Operation::Update {
            id,
            label,
            enabled,
            elsewhere,
        } => {
            if label.is_some() || enabled.is_some() || elsewhere.is_none() {
                store.update_fields(&id, label, enabled)?;
            }
            if let Some(on) = elsewhere {
                store.mark_elsewhere(&id, on, monitor::now())?;
            }
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
                monitor::check_by_person(&store, native, refresh_state, &id, None, gate).await?
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
            recovery::finish(root, &event, resumed, monitor::now())?;
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
        // Also offline: with no owner, the CLI holds the store's lock alone and the file is
        // written as the owner writes it (adversarial round 2: the stop stayed waiting).
        Operation::RecoveryContinued { session } => {
            let settled = recovery::continued(root, &session, monitor::now())?;
            if settled > 0 {
                oplog::event(
                    "recovery_continued",
                    &[("settled", oplog::Field::Number(settled as i64))],
                );
            }
            Ok(json!({"settled": settled}))
        }
        Operation::Handovers => Ok(json!({
            "enabled": handovers::enabled(root),
            "handovers": handovers::pending(root),
        })),
        Operation::HandoverTake { id } => {
            // C-Q8: a paid hand-over passed the spend gate when it was queued; it passes it again,
            // with the hand-over turn, when a console takes it. Refused, it stays waiting: the
            // window may double the ceiling meanwhile.
            let paid = handovers::waiting(root, &id).and_then(|h| h.paid.map(|kind| (h, kind)));
            let _turn = match &paid {
                Some((waiting, kind)) => {
                    let turn = if fallback::gated() {
                        None
                    } else {
                        Some(fallback::HAND_OVER.lock().await)
                    };
                    fallback::gate_by_hand(
                        &store,
                        root,
                        Some(&waiting.workflow),
                        &waiting.dir,
                        *kind,
                    )
                    .await?;
                    turn
                }
                None => None,
            };
            let taken = handovers::take(root, &id)?;
            Ok(json!({"handover": taken, "script": taken.script}))
        }
        Operation::HandoverSetting { on } => {
            if let Some(on) = on {
                handovers::set_enabled(root, on)?;
            }
            Ok(json!({"enabled": handovers::enabled(root)}))
        }
        Operation::Chains {
            workflow,
            pool,
            folder,
        } => {
            let snapshot = store.snapshot()?;
            // C-8: a checkout folder finds its group as the fallback does; `pool` is then the
            // executor account's group.
            let pool = match folder {
                Some(folder) => {
                    let folder = folder.canonicalize().unwrap_or(folder);
                    fallback::chain_pool_of(&snapshot, pool, &folder).0
                }
                None => pool,
            };
            let effective = store.chain_for(workflow.as_deref(), pool.as_deref())?;
            // C-2: what each step would do now, read like the fallback's own pass but never
            // starting the engine, a hand-over or a network call (a paid step's spend is null).
            let readiness = if snapshot.chains.is_empty() {
                serde_json::Map::new()
            } else {
                let now = monitor::now();
                let mut limited = runtime.map_or_else(HashSet::new, |r| r.limits.limited_ids(now));
                if let Some(r) = runtime {
                    limited.extend(r.kimi_watch.limited(now));
                }
                let sign_in: HashSet<String> =
                    refresh_state.sign_in_required(&store).into_iter().collect();
                let settings = agent_settings::read(root);
                let settings_readable = settings.is_ok();
                let disabled: HashSet<String> = settings
                    .map(|s| s.disabled.into_iter().collect())
                    .unwrap_or_default();
                let wants = |agent: &str| {
                    snapshot
                        .chains
                        .iter()
                        .any(|c| c.executors.iter().any(|e| e.agent == agent))
                };
                let kimi_usable = |id: &str| kimi::continuation_preflight(root, &store, id).is_ok();
                let kimi_used = |id: &str| runtime.map_or(0.0, |r| r.kimi_watch.used(id));
                let kimi = fallback::KimiChoice {
                    ids: snapshot
                        .kimi_accounts
                        .iter()
                        .map(|k| k.id.clone())
                        .collect(),
                    usable: &kimi_usable,
                    used: &kimi_used,
                    hermes_ready: wants(hermes::HERMES_TARGET)
                        && launch::hermes_continuation_ready(&store),
                    opencode_ready: wants(launch::OPENCODE_TARGET)
                        && launch::opencode_continuation_ready(&store),
                    disabled,
                };
                let catalog = agent_catalog::catalog();
                let installed = |agent: &str| {
                    catalog["agents"]
                        .as_array()
                        .and_then(|all| all.iter().find(|a| a["id"] == agent))
                        .is_some_and(|a| agent_settings::installed(a).is_some())
                };
                chain_view::readiness(
                    &snapshot,
                    &chain_view::Inputs {
                        limited: &limited,
                        sign_in: &sign_in,
                        kimi: &kimi,
                        installed: &installed,
                        settings_readable,
                    },
                )
            };
            // The last hand-over is spend.json's record (C-5 writes it); an unreadable file keeps
            // the chains readable and says so.
            let (last_handover, ceilings_unreadable) = match spend::read(root) {
                Ok(file) => (json!(file.last_handover), false),
                Err(_) => (Value::Null, true),
            };
            Ok(json!({
                "chains": snapshot.chains,
                "effective": effective,
                "presets": agent_catalog::presets(),
                "readiness": readiness,
                "last_handover": last_handover,
                "ceilings_unreadable": ceilings_unreadable,
                // Offline there is no owner: no limit evidence, so `readiness` cannot name a
                // limited account.
                "limits_known": runtime.is_some(),
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
        Operation::Ceilings => spend::view(root, &store.snapshot()?, monitor::now()),
        Operation::SetCeiling { scope, usd, by } => {
            let ceiling =
                spend::set_ceiling(root, &store.snapshot()?, scope, usd, by, monitor::now())?;
            // spend.json is a setting backups carry: the next change-driven backup takes it.
            store.setting_changed();
            Ok(json!({"ceiling": ceiling}))
        }
        Operation::AnswerCeiling { id, answer, by } => {
            let answered =
                spend::answer(root, &store.snapshot()?, &id, answer, by, monitor::now())?;
            store.setting_changed();
            Ok(answered)
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
            by,
        } => {
            // C-Q9: replacing a key whose spend a ceiling counts (a capped group's own, any key
            // under a machine ceiling, a task ceiling's baseline key) starts that count again.
            let replaces = service == "openrouter"
                && store
                    .agent_key_for(&service, pool.as_deref())
                    .ok()
                    .flatten()
                    .is_some();
            let saved = spend::guard_group_key(root, pool.as_deref(), replaces, by, || {
                store.set_agent_key_for(&service, pool.as_deref(), &key, model)
            })?;
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
        Operation::AgentKeyRemove { service, pool, by } => {
            // C-Q9: removing a key whose spend a ceiling counts moves its paid steps to a key the
            // ceiling does not count, or out of a machine ceiling's sum. A key that is not saved
            // removes nothing and loosens nothing.
            let exists = service == "openrouter"
                && store
                    .agent_key_for(&service, pool.as_deref())
                    .ok()
                    .flatten()
                    .is_some();
            let removed = spend::guard_group_key(root, pool.as_deref(), exists, by, || {
                store.remove_agent_key_for(&service, pool.as_deref())
            })?;
            Ok(json!({ "removed": removed }))
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
        } => {
            // C-Q8: on the OpenRouter key a launch in this folder uses, gated like a hand-over.
            let dir = projects::project_dir(root, &working_directory)?;
            fallback::gate_by_hand(&store, root, None, &dir, fallback::Paid::OpenRouter).await?;
            launch::launch_agent_openrouter(
                root,
                &store,
                &agent,
                model.as_deref(),
                &working_directory,
            )
        }
        Operation::LaunchAgent {
            agent,
            pool,
            working_directory,
            via: AgentVia::Proxy,
            ..
        } => {
            let runtime = needs_owner()?;
            // C-Q8: through the proxy on the pool's API-key account, gated like a hand-over. The
            // selected account is read first: a subscription account (or none) is refused by the
            // launch for what it is, never as an unknown spend.
            if store
                .selected_account(Provider::Claude, &pool)
                .is_some_and(|a| a.kind == AuthKind::ApiKey)
            {
                let dir = projects::project_dir(root, &working_directory)?;
                fallback::gate_by_hand(&store, root, None, &dir, fallback::Paid::ApiKeyAccount)
                    .await?;
            }
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
            // C-Q8: a session on an API-key account is a paid start, gated like a hand-over.
            if let Some(kind) = fallback::paid_kind(&store.snapshot()?, &id) {
                let dir = projects::project_dir(root, &working_directory)?;
                fallback::gate_by_hand(&store, root, None, &dir, kind).await?;
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
            // SB-120: no account named → the machine's default agent (`continue_default`).
            let id = if id.is_empty() {
                continue_default(root, runtime, &workflow_id, &working_directory)?
            } else {
                id
            };
            // Everything that can refuse runs before the offer, so a refusal leaves the workflow
            // exactly as it was; after the offer, a failed launch leaves it to lapse.
            let directory = continue_checks(&store, root, &workflow_id, &id, &working_directory)?;
            // C-Q8: a paid continue by hand (Hermes, OpenCode, an API-key account) passes the
            // spend gate as a hand-over does, once: the automatic fallback's step passed its own.
            if let Some(kind) = fallback::paid_kind(&store.snapshot()?, &id) {
                fallback::gate_by_hand(&store, root, Some(&workflow_id), &directory, kind).await?;
            }
            let bin = continuation::observatory()?;
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
            organization,
        } => {
            let before = store.snapshot()?;
            let organization = group_organization(&before, organization.as_deref())?;
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
            let project = store.save_project_in(
                pool.as_deref(),
                &name,
                &folders,
                &account_ids,
                organization.as_ref().map(Option::as_deref),
            )?;
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
            rt.proxy.set_released_tokens(report.released.clone());
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
            organization,
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
            let mut save = project_edit(
                &snapshot,
                &pool,
                name,
                &add_accounts,
                &remove_accounts,
                &add_folders,
                &remove_folders,
            )?;
            if let Operation::SaveProject {
                organization: chosen,
                ..
            } = &mut save
            {
                *chosen = organization;
            }
            // The same admission, refusals and memory of removals as a whole save (SaveProject).
            Box::pin(execute(store, root, runtime, save)).await
        }
        Operation::Organizations => {
            let mut view = organizations_view(&store.snapshot()?);
            view["passioncode"] = vesta::status(root).unwrap_or(Value::Null);
            Ok(view)
        }
        Operation::VestaStatus => vesta::status(root),
        Operation::VestaConnect => {
            let (stop, cancel) = tokio::sync::oneshot::channel();
            if let Some(rt) = runtime {
                let mut slot = rt.vesta_cancel.lock().map_err(|_| vesta::VESTA_FAILED)?;
                if slot.as_ref().is_some_and(|s| !s.is_closed()) {
                    return Err(vesta::VESTA_BUSY.into());
                }
                *slot = Some(stop);
            } else {
                // The CLI waits in its own process; Ctrl-C ends it.
                std::mem::forget(stop);
            }
            let result =
                vesta::connect(root, &store, vesta::BASE, vesta::open_browser, cancel).await;
            if let Some(rt) = runtime {
                if let Ok(mut slot) = rt.vesta_cancel.lock() {
                    *slot = None;
                }
            }
            crate::oplog::event(
                "vesta_connect",
                &[(
                    "outcome",
                    crate::oplog::Field::Code(match &result {
                        Ok(v) if v["state"] == "ok" => "connected",
                        Ok(_) => "connected_unread",
                        Err(e) if e == vesta::VESTA_DENIED => "denied",
                        Err(e) if e == vesta::VESTA_TIMEOUT => "timeout",
                        Err(e) if e == vesta::VESTA_CANCELLED => "cancelled",
                        Err(_) => "failed",
                    }),
                )],
            );
            result
        }
        Operation::VestaCancel => {
            let stop = runtime.and_then(|rt| rt.vesta_cancel.lock().ok()?.take());
            let cancelled = stop.is_some_and(|s| s.send(()).is_ok());
            Ok(json!({"cancelled": cancelled}))
        }
        Operation::VestaSync => vesta::sync(root, &store).await,
        Operation::VestaDisconnect => vesta::disconnect(root, &store).await,
        Operation::OrganizationAdd { name, domains } => {
            let organization = store.add_organization(&name, &domains)?;
            Ok(json!({"organization": organization}))
        }
        Operation::OrganizationEdit { id, name, domains } => {
            let organization = store.edit_organization(&id, name.as_deref(), domains.as_deref())?;
            Ok(json!({"organization": organization}))
        }
        Operation::OrganizationRemove { id } => {
            let organization = store.remove_organization(&id)?;
            Ok(json!({"removed": organization.id, "name": organization.name}))
        }
        Operation::OrganizationAssign { account, choice } => {
            let attribution = store.assign_organization(&account, choice.as_deref())?;
            Ok(
                json!({"account": account, "organization": attribution.organization, "by": attribution.by}),
            )
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
                    organization: None,
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
                    organization: None,
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
            // SB-147: folders resolve to projects through Git, off the async thread; SB-173: so
            // do the folders the counts carry that no group holds now.
            let loaded;
            let stats = match runtime {
                Some(rt) => {
                    rt.absorb_statistics();
                    &rt.statistics
                }
                None => {
                    loaded = statistics::Statistics::load(root);
                    &loaded
                }
            };
            let folders = stats.folders();
            let index =
                blocking::run(|| statistics::ProjectIndex::build(&snapshot).with_strays(&folders));
            Ok(stats.report(&snapshot, &index, days, monitor::now()))
        }
        Operation::SettingsExport => Ok(json!(settings_bundle::export(
            root,
            &store.snapshot()?,
            monitor::now()
        ))),
        Operation::SettingsImport {
            bundle,
            under,
            apply,
        } => {
            let bytes = serde_json::to_vec(&bundle).map_err(|_| settings_bundle::NOT_A_BUNDLE)?;
            let bundle = settings_bundle::parse(&bytes)?;
            let under = if under.is_empty() {
                vec![roots::default_under().ok_or("Home folder unavailable. Choose a folder.")?]
            } else {
                under
            };
            let snapshot = store.snapshot()?;
            let plan = blocking::run(|| settings_bundle::plan(&snapshot, &bundle, &under));
            if !apply {
                return Ok(json!({"preview": plan, "applied": false}));
            }
            let done = settings_bundle::apply(&store, root, &bundle, &plan);
            Ok(json!({"preview": plan, "applied": true, "result": done}))
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
        Operation::RemoveProject { pool, by } => {
            // C-Q7: a group's ceiling goes with it, so spend.json is pruned under its lock. C-Q9:
            // its own OpenRouter key goes too, so a ceiling counting that key keeps the group.
            let (project, pruned) = spend::remove_group(
                root,
                &pool,
                by,
                || Ok(store.agent_key_for("openrouter", Some(&pool))?.is_some()),
                || store.remove_project(&pool),
            )?;
            if pruned {
                store.setting_changed();
            }
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
        // Reached offline (`Runtime::execute` unwraps it before): the same scope around the
        // launch, which then asks for an owner.
        Operation::Launched { by, inner } => {
            if !is_launch(&inner) {
                return Err(launches::NOT_A_LAUNCH.into());
            }
            let by = launches::resolve(&store.snapshot()?, by)?;
            launches::scoped(
                Some(by),
                Box::pin(execute(store.clone(), root, runtime, *inner)),
            )
            .await
        }
        Operation::LaunchEvent {
            launch_id,
            event,
            provider_session_id,
            transcript,
        } => Ok(launches::event(
            root,
            &launch_id,
            event,
            provider_session_id.as_deref(),
            transcript.as_deref(),
            monitor::now(),
        )),
        Operation::Launches { launch_id } => {
            let snapshot = store.snapshot()?;
            // The states may ask `kill -0`/`ps`: never on an async worker (SB-103).
            match launch_id {
                Some(id) => {
                    let mut shown = crate::blocking::run(|| launches::show(root, &id))?;
                    shown["launch"] = label_session(&snapshot, shown["launch"].take());
                    Ok(shown)
                }
                None => {
                    let (listed, total) = crate::blocking::run(|| launches::history(root));
                    let listed: Vec<Value> = listed
                        .into_iter()
                        .map(|r| label_session(&snapshot, r))
                        .collect();
                    Ok(json!({"launches": listed, "total": total}))
                }
            }
        }
        Operation::Sessions { all } => {
            let snapshot = store.snapshot()?;
            // SB-103: the liveness checks may start `kill -0`/`ps`: never on an async worker.
            let listed = crate::blocking::run(|| launch::sessions(root, all));
            let sessions: Vec<Value> = listed
                .into_iter()
                .map(|session| label_session(&snapshot, session))
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
    /// SB-110: the session list carries each session's context and, for an organization's,
    /// its name.
    #[tokio::test]
    async fn the_session_list_names_an_organizations_context() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let acme = runtime.store.add_organization("Acme", &[]).unwrap();
        for (home, context) in [
            ("one", format!("org:{}", acme.id)),
            ("two", "personal".into()),
        ] {
            let dir = root.path().join("homes").join(home);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(launch::SESSION_RECORD),
                json!({"account_id": home, "provider": "claude", "pool": "default",
                    "context": context, "mode": "isolated", "working_directory": "/w",
                    "in_place": false, "launched_at": 1, "continues": null})
                .to_string(),
            )
            .unwrap();
        }
        // SB-201: an agent launched on the OpenRouter key: no account, its model as its label.
        let agent = root.path().join("runtimes").join("agent-goose-openrouter");
        std::fs::create_dir_all(&agent).unwrap();
        std::fs::write(
            agent.join(launch::SESSION_RECORD),
            json!({"account_id": "goose", "provider": "goose", "agent": "goose",
                "pool": "openrouter", "context": format!("org:{}", acme.id), "mode": "isolated",
                "working_directory": "/w", "in_place": false, "launched_at": 2,
                "continues": null, "label": "moonshotai/kimi-k2"})
            .to_string(),
        )
        .unwrap();
        let listed = runtime
            .execute(Operation::Sessions { all: true })
            .await
            .unwrap();
        let by = |c: &str| {
            listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["account_id"] == c)
                .cloned()
                .unwrap()
        };
        assert_eq!(by("one")["context_name"], "Acme");
        assert_eq!(by("two")["context"], "personal");
        assert!(by("two")["context_name"].is_null());
        assert_eq!(by("goose")["provider"], "goose");
        assert_eq!(by("goose")["agent"], "goose");
        assert_eq!(by("goose")["label"], "moonshotai/kimi-k2");
        assert_eq!(by("goose")["context_name"], "Acme");
        // A Claude Code launch's record names no agent: the list names it from the provider.
        assert_eq!(by("one")["agent"], "claude-code");
    }
    /// SB-180: `suggest_found` held the owner transaction while every found folder was saved on
    /// its own, each save publishing `accounts.json` and syncing the bindings. One pass's folders
    /// of one group now join in one save; a folder that cannot join is refused alone.
    #[tokio::test]
    async fn folders_found_in_one_pass_join_their_group_in_one_save() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let names = ["repo", "wt-a", "wt-b", "wt-c"];
        for name in names {
            std::fs::create_dir_all(work.join(name)).unwrap();
        }
        let saved = runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Agents".into(),
                folders: vec![work.join("repo")],
                account_ids: vec![],
                base_folders: None,
                organization: None,
            })
            .await
            .unwrap();
        let pool = saved["pool"].as_str().unwrap().to_owned();
        let row = |folder: PathBuf| suggest::Suggestion {
            folder: folder.to_string_lossy().into_owned(),
            pool: pool.clone(),
            reason: suggest::Reason::SameRepository,
            repository: "github.com/org/repo".into(),
            first_seen: 1,
            state: suggest::State::Pending,
        };
        let mut found: Vec<_> = names[1..].iter().map(|n| row(work.join(n))).collect();
        found.push(row(work.join("gone")));
        let writes = runtime.store.metadata_writes();
        let results = add_suggested_folders(&runtime.store, root.path(), &found, 2);
        assert_eq!(runtime.store.metadata_writes() - writes, 1);
        assert!(results[..3].iter().all(Result::is_ok), "{:?}", results);
        assert_eq!(add_refusal(results[3].as_ref().unwrap_err()), "gone");
        let snapshot = runtime.store.snapshot().unwrap();
        let project = snapshot.projects.iter().find(|p| p.pool == pool).unwrap();
        assert_eq!(project.folders.len(), 4, "{:?}", project.folders);
    }
    #[test]
    fn a_refused_found_folder_logs_why() {
        assert_eq!(add_refusal(switchboard_core::FOLDER_NESTED), "nested");
        assert_eq!(
            add_refusal("This folder is gone. Read the suggestions again."),
            "gone"
        );
        assert_eq!(add_refusal("Choose an existing project folder."), "gone");
        assert_eq!(add_refusal("Project not found."), "group_removed");
        assert_eq!(
            add_refusal("Projects hold at most 1024 folders in all."),
            "too_many_folders"
        );
        assert_eq!(add_refusal(PROJECT_ACCOUNT_IN_CLI), "account_in_cli");
        assert_eq!(add_refusal("Something else"), "other");
    }
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
    #[tokio::test]
    async fn a_new_seat_of_an_organization_outside_its_only_group_is_offered_it() {
        // SB-160 (B-4, SCN-077): a seat of an organization that has one group, signed in
        // elsewhere, comes back with that group in its receipt; nothing moves.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let finish = |account: Account| {
            let home = root.path().join(format!("staging-{}", account.id));
            std::fs::create_dir_all(&home).unwrap();
            let login = launch::fixture_login(home, account);
            let id = login.id.clone();
            runtime.logins.lock().unwrap().insert(id.clone(), login);
            runtime.finish_login(&id).unwrap()
        };
        let a = save(&runtime.store, "synthetic-c", "default");
        // No organization: no field.
        assert!(finish(a.clone()).get("organization_group").is_none());
        let acme = runtime
            .store
            .add_organization("Acme", &["example.invalid".into()])
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let group = runtime
            .store
            .save_project_in(
                None,
                "Acme work",
                &[folder.path().to_path_buf()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        let receipt = finish(a.clone());
        assert_eq!(
            receipt["organization_group"],
            json!({"pool": group.pool, "name": "Acme work"})
        );
        assert_eq!(
            runtime.store.snapshot().unwrap().accounts[0].pool,
            "default",
            "never moved without the person"
        );
        // The account the ordinary CLI is signed in to cannot join a group: said so.
        let captured = runtime
            .execute(Operation::CaptureCurrent {
                provider: Provider::Claude,
                label: None,
                pool: "default".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            captured["organization_group"],
            json!({"pool": group.pool, "name": "Acme work", "in_cli": true})
        );
        // Guards: a personal seat, and a seat already in the group, get no field.
        runtime
            .store
            .assign_organization(&a.id, Some(switchboard_core::organizations::PERSONAL))
            .unwrap();
        assert!(finish(a.clone()).get("organization_group").is_none());
        let b = save(&runtime.store, "synthetic-b", &group.pool);
        assert!(finish(b).get("organization_group").is_none());
        // Two groups for Acme: no offer.
        runtime.store.assign_organization(&a.id, None).unwrap();
        let other = tempfile::tempdir().unwrap();
        runtime
            .store
            .save_project_in(
                None,
                "Acme two",
                &[other.path().to_path_buf()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        assert!(finish(a).get("organization_group").is_none());
    }
    #[tokio::test]
    async fn a_new_seat_the_cli_is_signed_in_to_says_so_in_its_receipt() {
        // SB-160 (B-4): the ordinary CLI is signed in as `synthetic-a`; a sign-in that saved that
        // same identity outside its organization's group is offered the group with `in_cli`, as
        // `accounts capture` is, so the window offers no move the group would refuse.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let acme = runtime
            .store
            .add_organization("Acme", &["example.invalid".into()])
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let group = runtime
            .store
            .save_project_in(
                None,
                "Acme work",
                &[folder.path().to_path_buf()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        let a = save(&runtime.store, "synthetic-a", "default");
        let home = root.path().join("staging-cli");
        std::fs::create_dir_all(&home).unwrap();
        let login = launch::fixture_login(home, a);
        let id = login.id.clone();
        runtime.logins.lock().unwrap().insert(id.clone(), login);
        assert_eq!(
            runtime.finish_login(&id).unwrap()["organization_group"],
            json!({"pool": group.pool, "name": "Acme work", "in_cli": true})
        );
    }
    #[tokio::test]
    async fn a_repeated_finish_reads_where_the_account_is_now() {
        // SB-160 (B-4): finish, move the account into its group, finish again: the second
        // receipt reads the stored account, not the copy kept from the first, and offers nothing.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let acme = runtime
            .store
            .add_organization("Acme", &["example.invalid".into()])
            .unwrap();
        let folder = tempfile::tempdir().unwrap();
        let group = runtime
            .store
            .save_project_in(
                None,
                "Acme work",
                &[folder.path().to_path_buf()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        let a = save(&runtime.store, "synthetic-c", "default");
        let home = root.path().join("staging-again");
        std::fs::create_dir_all(&home).unwrap();
        let login = launch::fixture_login(home, a.clone());
        let id = login.id.clone();
        runtime.logins.lock().unwrap().insert(id.clone(), login);
        assert_eq!(
            runtime.finish_login(&id).unwrap()["organization_group"],
            json!({"pool": group.pool, "name": "Acme work"})
        );
        runtime
            .execute(Operation::MoveAccount {
                id: a.id.clone(),
                pool: group.pool.clone(),
            })
            .await
            .unwrap();
        let again = runtime.finish_login(&id).unwrap();
        assert_eq!(again["pool"], group.pool.as_str(), "{again}");
        assert!(again.get("organization_group").is_none(), "{again}");
        // The group already holds this identity under another account: nothing to offer.
        let d = save(&runtime.store, "synthetic-d", "default");
        save(&runtime.store, "synthetic-d", &group.pool);
        let home = root.path().join("staging-held");
        std::fs::create_dir_all(&home).unwrap();
        let login = launch::fixture_login(home, d);
        let id = login.id.clone();
        runtime.logins.lock().unwrap().insert(id.clone(), login);
        assert!(runtime
            .finish_login(&id)
            .unwrap()
            .get("organization_group")
            .is_none());
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
            organization: None,
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
                organization: None,
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
            organization: None,
        };
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "P".into(),
                folders: vec![a.clone(), b.clone()],
                account_ids: vec![],
                base_folders: None,
                organization: None,
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
                    organization: None,
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
                organization: None,
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
    async fn recovery_answers_while_a_background_step_holds_the_transaction() {
        // SB-166: Fabric Dashboards read `switchboard recovery` in 12.3 s and 14.7 s while a
        // renewal or a sync held the owner transaction (SB-163). It reads the recovery file,
        // written by rename, and the owner's in-memory limits.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let held = runtime.hold("test").await;
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            runtime.execute(Operation::Recovery),
        )
        .await
        .expect("answered while the transaction is held")
        .unwrap();
        assert_eq!(status["sessions"], json!([]));
        drop(held);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_session_start_in_its_groups_folder_is_answered_while_a_step_holds_the_transaction() {
        // SB-186: the start hook gives up after 1.5 s; behind a background step it waited 5 to
        // 22 s. A folder already in its group, or in none, writes nothing and is answered at once.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let main = work.join("app");
        std::fs::create_dir_all(main.join(".git")).unwrap();
        let loose = work.join("loose");
        std::fs::create_dir_all(&loose).unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App".into(),
                folders: vec![main.clone()],
                account_ids: vec![],
                base_folders: None,
                organization: None,
            })
            .await
            .unwrap();
        let held = runtime.hold("test").await;
        for (folder, reason) in [(main.clone(), "in_project"), (loose, "not_ours")] {
            let answer = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                runtime.execute(Operation::ProjectSuggest { folder }),
            )
            .await
            .expect("answered while the transaction is held")
            .unwrap();
            assert_eq!(answer["suggested"], false, "{answer}");
            assert_eq!(answer["reason"], reason, "{answer}");
        }
        drop(held);
    }

    #[tokio::test]
    async fn a_long_hold_names_where_its_time_went() {
        // SB-163: `transaction_held` carries each marked phase's milliseconds.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let mut held = runtime.hold("test").await;
        held.phase("first");
        held.phase("second");
        assert_eq!(
            held.phases.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(held.phases.iter().all(|(_, ms)| *ms >= 0));
    }

    #[test]
    fn a_sync_that_changed_nothing_is_not_logged_again() {
        // PRD-13 (iteration 2026-10-24): 522 of 1082 `claude_binding` lines in 48 h read
        // bound=0 removed=0 skipped=0 user_values=1: one folder with the person's own values
        // made every sync an event, against SB-155.
        let mut last = None;
        let mine = |folders: &[&str]| bindings::Report {
            user_values_present: folders.iter().map(|f| (*f).to_owned()).collect(),
            ..Default::default()
        };
        assert!(
            !binding_changed(&mine(&[]), &mut last),
            "a first sync with nothing"
        );
        assert!(binding_changed(&mine(&["/a"]), &mut last), "first seen");
        assert!(
            !binding_changed(&mine(&["/a", "/a"]), &mut last),
            "the same folder twice"
        );
        assert!(!binding_changed(&mine(&["/a"]), &mut last), "the same set");
        assert!(
            binding_changed(&mine(&["/a", "/b"]), &mut last),
            "a new folder"
        );
        assert!(
            !binding_changed(&mine(&["/b", "/a"]), &mut last),
            "the same set in another order"
        );
        assert!(binding_changed(&mine(&[]), &mut last), "the set emptied");
        assert!(!binding_changed(&mine(&[]), &mut last), "nothing at all");
        let bound = bindings::Report {
            bound: 1,
            ..Default::default()
        };
        assert!(binding_changed(&bound, &mut last), "a folder bound");
        // N1 round 1 (seam tier): a folder whose settings cannot be read is skipped at every
        // sync; the same skipped set is not a change either.
        let skipped = |folders: &[&str]| bindings::Report {
            skipped: folders.iter().map(|f| (*f).to_owned()).collect(),
            ..Default::default()
        };
        assert!(
            binding_changed(&skipped(&["/c"]), &mut last),
            "first skipped"
        );
        assert!(
            !binding_changed(&skipped(&["/c"]), &mut last),
            "skipped again"
        );
        assert!(
            binding_changed(&skipped(&["/c", "/d"]), &mut last),
            "another skipped"
        );
    }

    #[tokio::test]
    async fn status_and_the_start_line_name_the_owners_version() {
        // PRD-12 (iteration 2026-10-24): twelve restarts into new releases in 48 h could not be
        // told apart in the log, and `status` did not say which owner answered.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let status = runtime.execute(Operation::Status).await.unwrap();
        assert_eq!(
            status["owner_version"],
            env!("CARGO_PKG_VERSION"),
            "{status}"
        );
        assert_eq!(status["online"], true, "{status}");
        // N1 round 1 (seam tier): without an owner no owner answered, so no owner's version is
        // named (the CLI's own build is not the owner's).
        let offline = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(offline.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let status = execute(store, offline.path(), None, Operation::Status)
            .await
            .unwrap();
        assert_eq!(status["owner_version"], Value::Null, "{status}");
        assert_eq!(status["online"], false, "{status}");
        let started = owner_started_fields(7, 8080);
        assert!(started.iter().any(|(name, field)| *name == "version"
            && matches!(field, oplog::Field::Code(v) if *v == env!("CARGO_PKG_VERSION"))));
    }

    #[tokio::test]
    async fn a_long_hold_names_the_time_after_its_last_mark() {
        // PRD-02 (iteration 2026-10-24): a hold logged only its marked phases; the time after
        // the last mark (14 s of a `usage_renewal` on 2026-10-24) was in `millis` and nowhere else.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let mut held = runtime.hold("test").await;
        std::thread::sleep(std::time::Duration::from_millis(200));
        held.phase("first");
        std::thread::sleep(std::time::Duration::from_millis(40));
        let fields = held.fields();
        assert!(
            fields[1].1 < 200,
            "rest counts from the last mark: {fields:?}"
        );
        assert_eq!(
            fields.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            ["first", "rest"]
        );
        assert!(fields[1].1 >= 40, "{fields:?}");
        // A hold with no marks has nothing to split.
        drop(held);
        let held = runtime.hold("test").await;
        assert!(held.fields().is_empty());
    }

    #[tokio::test]
    async fn a_stop_report_does_not_wait_behind_a_background_step() {
        // SB-150: the `StopFailure` hook's registration waited for the owner transaction like
        // SB-149's session start did; the hook gives up after 10 s.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let held = runtime.hold("test").await;
        let answer = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            runtime.execute(Operation::RecoveryHook {
                session: "session-1".into(),
                directory: work.path().to_path_buf(),
                billing: false,
            }),
        )
        .await
        .expect("answered while the transaction is held");
        // No saved account matches in this fixture: the answer is a refusal, given at once.
        assert!(answer.is_err());
        drop(held);
    }

    /// SB-183 (node A-3): only an owner the installed app enabled with a home writes start
    /// files. A test or development owner (any owner over a data folder that is not the app's
    /// own) never gets one, whatever groups it holds.
    #[tokio::test]
    async fn no_test_or_dev_build_touches_the_real_home() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let (work, acme) = organization_group(&runtime).await;
        runtime.bindings_pass().await;
        assert!(shell::map::answer(root.path(), &acme).is_some());
        // The installed app's call, on a data folder that is not the app's own: refused.
        runtime.enable_shell_hook();
        runtime.bindings_pass().await;
        assert!(!shell::install::record_path(root.path()).exists());
        assert!(runtime.shell_hook_notice().is_none());
        drop(work);
    }

    /// SB-183 (node A-3): the data-folder guard holds on its own. Even when the app counts as
    /// installed and a home is at hand, an owner over a data folder that is not the app's own
    /// writes no start file and no record.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_installed_app_over_another_data_folder_writes_no_start_file() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let (work, acme) = organization_group(&runtime).await;
        runtime.bindings_pass().await;
        assert!(shell::map::answer(root.path(), &acme).is_some());
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        let zshrc = home.join(".zshrc");
        std::fs::write(&zshrc, "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        let host = shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        };
        let asked = std::sync::atomic::AtomicBool::new(false);
        runtime.enable_shell_hook_when(
            || {
                asked.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            },
            || Some(host),
        );
        runtime.bindings_pass().await;
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "own\n");
        assert!(!shell::install::record_path(root.path()).exists());
        assert!(runtime.shell_hook_notice().is_none());
        // The installed check is not even consulted: the data folder decides first.
        assert!(!asked.load(std::sync::atomic::Ordering::SeqCst));
        drop(work);
    }

    /// SB-183 (node A-3): the installed owner writes the block at start when an organization's
    /// group exists, says so once, takes the default block out when the last one leaves, and
    /// writes it again when one comes back, all in an injected home.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_installed_owner_follows_the_presence_of_an_organization_group() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        let zshrc = home.join(".zshrc");
        std::fs::write(&zshrc, "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        let host = shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        };
        // No organization yet: enabled, nothing written.
        runtime.enable_shell_hook_with(host);
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "own\n");
        assert!(runtime.shell_hook_notice().is_none());
        // The first organization's group appears: the next pass writes the block, once.
        let (work, _acme) = organization_group(&runtime).await;
        runtime.bindings_pass().await;
        let text = std::fs::read_to_string(&zshrc).unwrap();
        assert!(text.contains(shell::install::OPEN), "{text}");
        let notice = runtime.shell_hook_notice().expect("the first-time notice");
        assert_eq!(notice["organization"], "Acme");
        assert_eq!(notice["group"], "Acme");
        runtime.acknowledge_shell_hook_notice();
        assert!(runtime.shell_hook_notice().is_none(), "only once");
        // Idle passes do not touch the file.
        std::fs::write(&zshrc, &text).unwrap();
        runtime.bindings_pass().await;
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), text);
        // The group turns personal: the default block goes.
        let pool = runtime.store.snapshot().unwrap().projects[0].pool.clone();
        let account = runtime.store.snapshot().unwrap().accounts[0].id.clone();
        runtime
            .store
            .assign_organization(&account, Some("personal"))
            .unwrap();
        runtime.store.set_project_organization(&pool, None).unwrap();
        runtime.bindings_pass().await;
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "own\n");
        // The person turns it off; an organization coming back writes nothing.
        shell::install::set_choice(root.path(), false).unwrap();
        let org = runtime.store.snapshot().unwrap().organizations[0]
            .id
            .clone();
        runtime
            .store
            .assign_organization(&account, Some(&org))
            .unwrap();
        runtime
            .store
            .set_project_organization(&pool, Some(&org))
            .unwrap();
        runtime.bindings_pass().await;
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "own\n");
        drop(work);
    }

    /// Fix cycle 1 of node A-7 (B1): the hook the window, MCP and `shell status` report is the
    /// hook the installer wrote. An organization's group whose only folder a personal group
    /// takes (`project_for`: the more private wins) has no row in the folder map, so no block
    /// is written by default and none is reported; when the personal group lets the folder go,
    /// both turn on together. Onboarding's `hook` step reads the same state.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_hook_reported_is_the_hook_written() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        let zshrc = home.join(".zshrc");
        std::fs::write(&zshrc, "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        runtime.enable_shell_hook_with(shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        });
        let (work, acme) = organization_group(&runtime).await;
        // A personal group holds the organization group's only folder.
        let two = fixtures::save(&runtime.store, "synthetic-two", "default").id;
        runtime
            .store
            .assign_organization(&two, Some("personal"))
            .unwrap();
        let mine = runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Mine".into(),
                folders: vec![acme.clone()],
                account_ids: vec![two.clone()],
                base_folders: None,
                organization: Some("personal".into()),
            })
            .await
            .unwrap();
        runtime.bindings_pass().await;
        assert!(shell::map::answer(root.path(), &acme).is_none());
        let agree = |runtime: &Runtime| -> bool {
            let status = runtime.shell_hook_status();
            let written = status["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["state"] == "written");
            assert_eq!(status["hook"] == "default_on", written, "{status}");
            let probe = onboarding::Probe {
                snapshot: runtime.store.snapshot().ok(),
                shell: Some(status),
                ..Default::default()
            };
            let answer = onboarding::steps(&probe);
            let hook = answer["steps"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == "hook")
                .expect("the hook step")
                .clone();
            let follows = hook["summary"].as_str().unwrap().contains("work in");
            assert_eq!(follows, written, "{hook}");
            written
        };
        assert!(!agree(&runtime), "no row: no block, and none reported");
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "own\n");
        // The personal group lets the folder go: the row comes, and the block with it.
        let pool = mine["pool"].as_str().unwrap().to_owned();
        let elsewhere = work.path().canonicalize().unwrap().join("mine");
        std::fs::create_dir_all(&elsewhere).unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: Some(pool),
                name: "Mine".into(),
                folders: vec![elsewhere],
                account_ids: vec![two],
                base_folders: None,
                organization: Some("personal".into()),
            })
            .await
            .unwrap();
        runtime.bindings_pass().await;
        assert!(shell::map::answer(root.path(), &acme).is_some());
        assert!(agree(&runtime), "a row: the block, and default_on");
        drop(work);
    }

    /// Fix cycle 1 of node A-7 (N2): the first-time notice is the installed app's. An owner
    /// without the installed home (a development build or `serve` over the same data folder)
    /// returns none to the watcher and cannot acknowledge it away.
    #[cfg(unix)]
    #[tokio::test]
    async fn only_the_installed_owner_reads_or_acknowledges_the_notice() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        std::fs::write(home.join(".zshrc"), "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        runtime.enable_shell_hook_with(shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        });
        let (work, _acme) = organization_group(&runtime).await;
        runtime.bindings_pass().await;
        assert!(shell::install::pending_notice(root.path()).is_some());
        // The same data folder, an owner without the installed home.
        let host = runtime.shell_hook.lock().unwrap().take();
        assert!(runtime.shell_hook_notice().is_none(), "not posted");
        runtime.acknowledge_shell_hook_notice();
        assert!(
            shell::install::pending_notice(root.path()).is_some(),
            "not acknowledged"
        );
        *runtime.shell_hook.lock().unwrap() = host;
        assert!(runtime.shell_hook_notice().is_some());
        drop(work);
    }

    /// A group bound to the organization "Acme" with one Claude account and one folder.
    pub(crate) async fn organization_group(runtime: &Runtime) -> (tempfile::TempDir, PathBuf) {
        let work = tempfile::tempdir().unwrap();
        let acme = work.path().canonicalize().unwrap().join("acme");
        std::fs::create_dir_all(&acme).unwrap();
        let org = runtime.store.add_organization("Acme", &[]).unwrap();
        let one = fixtures::save(&runtime.store, "synthetic-one", "default").id;
        runtime
            .store
            .assign_organization(&one, Some(&org.id))
            .unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Acme".into(),
                folders: vec![acme.clone()],
                account_ids: vec![one],
                base_folders: None,
                organization: Some(org.id.clone()),
            })
            .await
            .unwrap();
        (work, acme)
    }

    /// SB-183 (node A-4): the bindings pass writes an organization's shell homes and the folder
    /// map, and a pass whose fingerprint did not move writes nothing: a file taken away is not
    /// written back until something changes. A synthetic owner carries nothing of the real home.
    #[tokio::test]
    async fn an_idle_pass_writes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let acme = work.join("acme");
        std::fs::create_dir_all(&acme).unwrap();
        let org = runtime.store.add_organization("Acme", &[]).unwrap();
        let one = fixtures::save(&runtime.store, "synthetic-one", "default").id;
        runtime
            .store
            .assign_organization(&one, Some(&org.id))
            .unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Acme".into(),
                folders: vec![acme.clone()],
                account_ids: vec![one],
                base_folders: None,
                organization: Some(org.id.clone()),
            })
            .await
            .unwrap();
        runtime.bindings_pass().await;
        let map_file = shell::map::path(root.path());
        let found = shell::map::answer(root.path(), &acme).expect("a row for the folder");
        assert_eq!(found.context, format!("org:{}", org.id));
        let home = found.claude_home.clone().unwrap();
        let settings = home.join("settings.json");
        assert!(settings.is_file());
        // Nothing of the real person's folder in a synthetic owner's home.
        assert!(!home.join("CLAUDE.md").exists());
        // Idle: the files taken away stay away.
        let written = std::fs::read(&map_file).unwrap();
        std::fs::remove_file(&map_file).unwrap();
        std::fs::remove_file(&settings).unwrap();
        for _ in 0..5 {
            runtime.bindings_pass().await;
        }
        assert!(!map_file.exists(), "an idle pass wrote the map");
        assert!(!settings.exists(), "an idle pass wrote a shell home");
        // A change (the group turns personal) is written: the map loses its row, the home stays.
        std::fs::write(&map_file, &written).unwrap();
        let pool = found.pool.clone();
        runtime
            .store
            .set_project_organization(&pool, None)
            .unwrap_err();
        runtime
            .store
            .assign_organization(
                &runtime.store.snapshot().unwrap().accounts[0].id,
                Some("personal"),
            )
            .unwrap();
        runtime.store.set_project_organization(&pool, None).unwrap();
        runtime.bindings_pass().await;
        assert_ne!(
            std::fs::read(&map_file).unwrap(),
            written,
            "the map was rewritten"
        );
        assert!(shell::map::answer(root.path(), &acme).is_none());
        assert!(home.is_dir(), "the organization's history stays");
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
                organization: None,
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
        // SB-172: its old capability is released for a week, served on the default pool and
        // never on the project's accounts (`a_folder_taken_out_lately_is_served_on_the_default_pool`).
        let released = std::fs::read_to_string(root.path().join("released-folders.json")).unwrap();
        assert!(released.contains("\"pool\":\"app\""), "{released}");
        assert!(!released.contains(&header), "no capability in the file");
        // Forgotten (a week later), the proxy no longer takes it.
        runtime.proxy.set_released_tokens(Default::default());
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
                organization: None,
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
                organization: None,
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
                    organization: None,
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
                organization: None,
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
            organization: None,
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
                    organization: None,
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
            .execute(Operation::RemoveProject {
                pool: "app".into(),
                by: spend::Surface::Window,
            })
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
                organization: None,
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
                organization: None,
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
                organization: None,
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
                organization: None,
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
                organization: None,
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
                organization: None,
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
            .execute(Operation::RemoveProject {
                pool: "app".into(),
                by: spend::Surface::Window,
            })
            .await
            .unwrap();
        runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "App again".into(),
                folders: vec![main.clone()],
                account_ids: vec![],
                base_folders: None,
                organization: None,
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
                folder: None,
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
                folder: None,
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
    async fn the_chain_that_applies_is_found_through_the_checkout_folder_like_the_fallback() {
        // C-8/C-2 fix cycle 1: `Chains {workflow, folder}` resolves the group the way the
        // fallback does (`fallback::chain_pool`): the group holding the folder, not only a pool
        // the caller names.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().canonicalize().unwrap();
        let group = runtime
            .store
            .save_project(None, "Alpha", std::slice::from_ref(&repo_path), &[])
            .unwrap();
        let step = |agent: &str| switchboard_core::Executor {
            agent: agent.into(),
            account_id: None,
            key: None,
        };
        runtime
            .store
            .set_chain(switchboard_core::ChainScope::Machine, vec![step("codex")])
            .unwrap();
        runtime
            .store
            .set_chain(
                switchboard_core::ChainScope::Project {
                    pool: group.pool.clone(),
                },
                vec![step("claude-code")],
            )
            .unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: Some("wf_0123456789abcdef".into()),
                pool: None,
                folder: Some(repo_path.join("src")),
            })
            .await
            .unwrap();
        assert_eq!(view["effective"]["scope"]["kind"], "project");
        assert_eq!(view["effective"]["scope"]["pool"], group.pool.as_str());
        assert_eq!(view["limits_known"], true, "the owner knows its limits");
        // Outside every group the machine's chain applies.
        let elsewhere = tempfile::tempdir().unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: Some("wf_0123456789abcdef".into()),
                pool: None,
                folder: Some(elsewhere.path().to_owned()),
            })
            .await
            .unwrap();
        assert_eq!(view["effective"]["scope"]["kind"], "machine");
        // An offline answer has no owner's limits: it says so.
        let offline = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(offline.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let view = execute(
            store,
            offline.path(),
            None,
            Operation::Chains {
                workflow: None,
                pool: None,
                folder: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(view["limits_known"], false);
    }
    #[tokio::test]
    async fn spending_ceilings_are_set_read_and_answered_through_the_runtime() {
        // SB-72 (C-4): the operations as the surfaces send them, over spend.json, never Snapshot.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        let op = |v: Value| serde_json::from_value::<Operation>(v).unwrap();
        let empty = runtime.execute(Operation::Ceilings).await.unwrap();
        assert_eq!(empty["ceilings"], json!([]));
        assert_eq!(empty["limits"]["max_usd"], 1000.0);
        // C-4 fix cycle 1: a body that leaves out `usd` or `by` is refused, never read as
        // "clear" or as some surface.
        assert!(serde_json::from_value::<Operation>(
            json!({"operation": "set_ceiling", "scope": {"kind": "machine"}, "by": "window"})
        )
        .is_err());
        assert!(serde_json::from_value::<Operation>(
            json!({"operation": "set_ceiling", "scope": {"kind": "machine"}, "usd": 5})
        )
        .is_err());
        let changes = runtime.store.changes();
        let set = runtime
            .execute(op(
                json!({"operation": "set_ceiling", "scope": {"kind": "machine"}, "usd": 5, "by": "window"}),
            ))
            .await
            .unwrap();
        assert_eq!(set["ceiling"]["cents"], 500);
        // spend.json is carried by backups: a ceiling set counts for the next one.
        assert!(
            runtime.store.changes() > changes,
            "the next backup carries it"
        );
        let out = runtime
            .execute(op(
                json!({"operation": "set_ceiling", "scope": {"kind": "machine"}, "usd": 0, "by": "window"}),
            ))
            .await
            .unwrap_err();
        assert_eq!(out, spend::OUT_OF_RANGE);
        // A question opened by the gate (C-5), answered as each surface would.
        let snapshot = runtime.store.snapshot().unwrap();
        let id = spend::open_question(
            root.path(),
            &snapshot,
            &switchboard_core::ChainScope::Machine,
            500,
            5.1,
            monitor::now(),
        )
        .unwrap()
        .unwrap();
        let listed = runtime.execute(Operation::Ceilings).await.unwrap();
        assert_eq!(listed["questions"][0]["id"], id.as_str());
        assert_eq!(listed["questions"][0]["double_usd"], 10.0);
        let refused = runtime
            .execute(op(
                json!({"operation": "answer_ceiling", "id": id, "answer": "double", "by": "mcp"}),
            ))
            .await
            .unwrap_err();
        assert_eq!(refused, spend::DOUBLE_IN_WINDOW);
        let doubled = runtime
            .execute(op(json!({"operation": "answer_ceiling", "id": id, "answer": "double", "by": "window"})))
            .await
            .unwrap();
        assert_eq!(doubled["ceiling"]["usd"], 10.0);
        assert_eq!(doubled["ceiling"]["doubled_from"], 5.0);
        // A surface must say who it is; an unknown one is refused before anything runs.
        assert!(serde_json::from_value::<Operation>(
            json!({"operation": "answer_ceiling", "id": id, "answer": "keep"})
        )
        .is_err());
        assert!(serde_json::from_value::<Operation>(
            json!({"operation": "answer_ceiling", "id": id, "answer": "keep", "by": "operator"})
        )
        .is_err());
        // Nothing of it reached accounts.json: a 0.6.x build still reads the store.
        let stored = serde_json::to_string(&runtime.store.snapshot().unwrap()).unwrap();
        assert!(!stored.contains("ceiling") && !stored.contains("cents"));
        assert!(root.path().join(spend::FILE).is_file());
        let cleared = runtime
            .execute(op(
                json!({"operation": "set_ceiling", "scope": {"kind": "machine"}, "usd": null, "by": "window"}),
            ))
            .await
            .unwrap();
        assert_eq!(cleared["ceiling"], Value::Null);
    }
    #[tokio::test]
    async fn the_chain_view_names_why_each_step_would_be_skipped() {
        // C-2 (dossier C part A): `Chains` adds, per chain set, what each step would do now and
        // the last hand-over from spend.json, without the engine, a hand-over or a network call.
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_out, activates).await;
        runtime
            .execute(Operation::AgentEnable {
                agent: "codex".into(),
                enabled: false,
            })
            .await
            .unwrap();
        runtime
            .execute(Operation::SetChain {
                scope: switchboard_core::ChainScope::Machine,
                executors: vec![
                    switchboard_core::Executor {
                        agent: "codex".into(),
                        account_id: None,
                        key: None,
                    },
                    switchboard_core::Executor {
                        agent: "hermes".into(),
                        account_id: None,
                        key: Some("demo/prod/OPENROUTER_API_KEY".into()),
                    },
                ],
                preset: None,
            })
            .await
            .unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: None,
                pool: None,
                folder: None,
            })
            .await
            .unwrap();
        let steps = view["readiness"]["machine"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["state"], "skipped");
        assert_eq!(steps[0]["reason"], "agent_disabled");
        assert_eq!(steps[1]["state"], "not_automatic");
        assert_eq!(steps[1]["reason"], "key_pinned");
        assert_eq!(view["last_handover"], Value::Null);
        assert_eq!(view["ceilings_unreadable"], false);
        // The last hand-over is read from spend.json, never from Snapshot events.
        std::fs::write(
            root.path().join(spend::FILE),
            r#"{"version":1,"last_handover":{"at":5,"workflow":"wf_0123456789abcdef","outcome":"no_step","skipped":[{"agent":"codex","reason":"agent_disabled"}]}}"#,
        )
        .unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: None,
                pool: None,
                folder: None,
            })
            .await
            .unwrap();
        assert_eq!(view["last_handover"]["outcome"], "no_step");
        assert_eq!(
            view["last_handover"]["skipped"][0]["reason"],
            "agent_disabled"
        );
        // An unreadable spend.json keeps the chains readable and says so.
        std::fs::write(root.path().join(spend::FILE), b"{broken").unwrap();
        let view = runtime
            .execute(Operation::Chains {
                workflow: None,
                pool: None,
                folder: None,
            })
            .await
            .unwrap();
        assert_eq!(view["ceilings_unreadable"], true);
        assert_eq!(view["last_handover"], Value::Null);
        assert_eq!(view["chains"].as_array().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn an_offline_command_cannot_answer_a_ceiling_as_the_window() {
        // C-Q1 (C-4): with no owner running, the CLI's offline path refuses `by: window` before
        // it opens the store; only the app's own process answers as the window.
        let root = tempfile::tempdir().unwrap();
        let refused = execute_offline(
            root.path().to_owned(),
            Operation::AnswerCeiling {
                id: "q_0123456789abcdef".into(),
                answer: spend::Answer::Double,
                by: spend::Surface::Window,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(refused, spend::WINDOW_ONLY);
        assert!(
            !root.path().join("accounts.json").exists(),
            "the store was never opened"
        );
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

    fn memory_offline(
        root: PathBuf,
        operation: Operation,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
        Box::pin(async move {
            let store = Arc::new(Store::open(root.clone(), Arc::new(MemoryVault::default()))?);
            execute(store, &root, None, operation).await
        })
    }

    /// SB-181: Fabric Dashboards' `handovers` and `recovery` were refused for the length of an
    /// update restart, while the draining owner still held the store and had removed its
    /// descriptor. A command now waits for the lock and is answered once it is free.
    #[tokio::test]
    async fn a_command_waits_for_a_store_a_draining_owner_still_holds() {
        let root = tempfile::tempdir().unwrap();
        let held = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(held);
        });
        let started = std::time::Instant::now();
        let answer = call_within(
            root.path(),
            Operation::Snapshot,
            Duration::from_secs(5),
            memory_offline,
        )
        .await
        .unwrap();
        assert!(answer["accounts"].is_array(), "{answer}");
        assert!(started.elapsed() >= Duration::from_millis(400));
        release.await.unwrap();
    }

    /// The owner that starts during the wait answers it, through its descriptor.
    #[tokio::test]
    async fn a_command_waiting_for_the_store_is_answered_by_the_owner_that_starts() {
        let root = tempfile::tempdir().unwrap();
        let held = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let path = root.path().to_owned();
        let starting = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            drop(held);
            owner(&path).await
        });
        let refused =
            |_: PathBuf, _: Operation| async { Err(switchboard_core::STORE_LOCKED.to_string()) };
        let answer = call_within(
            root.path(),
            Operation::Status,
            Duration::from_secs(5),
            refused,
        )
        .await
        .unwrap();
        assert!(answer["proxy_address"].is_string(), "{answer}");
        starting.await.unwrap().shutdown(DRAIN_DEADLINE).await;
    }

    /// The wait is bounded: a lock still held at its end is reported as before.
    #[tokio::test]
    async fn a_store_held_past_the_wait_is_reported() {
        let root = tempfile::tempdir().unwrap();
        let _held = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let started = std::time::Instant::now();
        let error = call_within(
            root.path(),
            Operation::Snapshot,
            Duration::from_millis(400),
            memory_offline,
        )
        .await
        .unwrap_err();
        assert_eq!(error, switchboard_core::STORE_LOCKED);
        assert!(started.elapsed() >= Duration::from_millis(400));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

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

    /// SB-183 (node A-3): the first-time notice waits for the window, across a restart of the
    /// owner, and is gone once the window acknowledged it.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_first_time_notice_waits_for_the_window_across_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        std::fs::write(home.join(".zshrc"), "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        let host = shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        };
        let first = owner(root.path()).await;
        first.runtime.enable_shell_hook_with(host.clone());
        let (work, _acme) = super::owner_tests::organization_group(&first.runtime).await;
        first.runtime.bindings_pass().await;
        assert!(first.runtime.shell_hook_notice().is_some());
        first.shutdown(Duration::from_secs(5)).await;
        // The app restarted before the window showed it: the installed app enables its home at
        // start, and only then answers the notice (A-7 fix cycle 1, N2).
        let second = owner(root.path()).await;
        let runtime = &second.runtime;
        assert!(runtime.shell_hook_notice().is_none(), "not before its home");
        runtime.enable_shell_hook_with(host.clone());
        let notice = runtime
            .shell_hook_notice()
            .expect("the notice is still waiting after a restart");
        assert_eq!(notice["organization"], "Acme");
        assert_eq!(notice["group"], "Acme");
        assert!(runtime.shell_hook_notice().is_some(), "until acknowledged");
        runtime.acknowledge_shell_hook_notice();
        assert!(runtime.shell_hook_notice().is_none());
        second.shutdown(Duration::from_secs(5)).await;
        let third = owner(root.path()).await;
        third.runtime.enable_shell_hook_with(host);
        third.runtime.bindings_pass().await;
        assert!(third.runtime.shell_hook_notice().is_none(), "said once");
        third.shutdown(Duration::from_secs(5)).await;
        drop(work);
    }

    /// SB-183 (node A-7): the window's *Turn off* / *Turn on* go through the installed owner's
    /// home: off gives the start file back byte for byte and keeps it out, on writes it again;
    /// the section reads the organization by name. An owner that is not the installed app
    /// refuses and changes nothing.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_windows_switch_turns_the_hook_off_and_on_through_the_owner() {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let home = home.path().canonicalize().unwrap();
        let rc = home.join(".zshrc");
        std::fs::write(&rc, "own\n").unwrap();
        let cli = home.join("switchboard");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        let host = shell::install::Host {
            home: home.clone(),
            zdotdir: None,
            login_shell: Some("zsh".into()),
            macos: true,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: Some(unsafe { libc::getuid() }),
            cli: Some(cli),
            windows: None,
        };
        let owner = owner(root.path()).await;
        let runtime = &owner.runtime;
        // Not the installed app yet: the switch is refused and nothing is written.
        assert_eq!(runtime.shell_hook_status()["can_change"], false);
        assert_eq!(
            runtime.set_shell_hook(true).unwrap_err(),
            shell::install::NOT_INSTALLED
        );
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "own\n");
        assert_eq!(shell::map::choice(root.path()), shell::map::Choice::Default);

        runtime.enable_shell_hook_with(host);
        let (work, _acme) = super::owner_tests::organization_group(runtime).await;
        runtime.bindings_pass().await;
        let status = runtime.shell_hook_status();
        assert_eq!(status["can_change"], true);
        assert_eq!(status["hook"], "default_on");
        assert_eq!(status["organizations"], json!(["Acme"]));
        assert_eq!(status["unmapped"], json!([]), "SB-222: its group has a row");
        assert_eq!(status["files"][0]["state"], "written");
        assert!(
            status.get("terminal").is_none(),
            "the window has no terminal of its own"
        );
        assert!(std::fs::read_to_string(&rc)
            .unwrap()
            .contains(shell::install::OPEN));

        let off = runtime.set_shell_hook(false).unwrap();
        assert_eq!(off["hook"], "off");
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "own\n");
        // A background pass after off writes nothing back.
        runtime.bindings_pass().await;
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "own\n");

        let on = runtime.set_shell_hook(true).unwrap();
        assert_eq!(on["hook"], "on");
        assert_eq!(on["files"][0]["state"], "written");
        assert!(std::fs::read_to_string(&rc)
            .unwrap()
            .contains(shell::install::OPEN));
        owner.shutdown(Duration::from_secs(5)).await;
        drop(work);
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
    /// SB-179 seam review: a quota check renews outside the owner transaction, so the drain no
    /// longer waited for its grant; a successor the provider returned after the drain took the
    /// transaction died with the process, and the account needed a new sign-in. The drain now
    /// waits for grants in flight and stores what they kept.
    #[tokio::test]
    async fn the_drain_stores_the_token_a_quota_checks_renewal_returns_during_it() {
        use axum::{
            routing::{get, post},
            Json, Router,
        };
        let tmp = tempfile::tempdir().unwrap();
        let runtime =
            fixtures::runtime(tmp.path(), fixtures::signed_out, fixtures::activates).await;
        let owner = Owner {
            runtime: runtime.clone(),
            control: control::ControlHandle::start(runtime.clone())
                .await
                .unwrap(),
            monitor: monitor::MonitorHandle::start(&runtime, false),
        };
        // The monitor's first pass runs at start; it ends before the account exists, so only
        // the quota check below renews it (a pass that joined it would hold the drain).
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Due for renewal: the access token expired.
        let mut expired = fixtures::credential("synthetic-a");
        expired.expires_at = Some(1_000);
        let account = runtime
            .store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired,
                Some(fixtures::identity("synthetic-a")),
            )
            .unwrap();
        let granted = Arc::new(tokio::sync::Notify::new());
        let notify = granted.clone();
        let app = Router::new()
            .route("/api/oauth/usage", get(|| async { Json(json!({})) }))
            .route(
                "/token",
                post(move || {
                    let notify = notify.clone();
                    async move {
                        notify.notify_one();
                        // Longer than the drain takes to reach the store.
                        tokio::time::sleep(Duration::from_millis(1500)).await;
                        Json(json!({"access_token":"successor","expires_in":28800,"refresh_token":"successor-r"}))
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
        // The grant is on the network when the drain starts.
        granted.notified().await;
        let stopped = owner.shutdown(Duration::from_secs(4)).await;
        assert!(
            stopped.drained,
            "the drain waited for the grant inside its deadline"
        );
        check.abort();
        let stored = runtime.store.stored_credential(&account.id).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("successor-r"));
    }
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

    /// B-5 fix cycle 2: a person's *Check* reaches past a usage verdict through both doors that
    /// carry it: the owner's `Runtime::execute` and the `execute` arm the offline CLI runs. The
    /// provider answering 200 takes the verdict back; the background check would not have asked.
    #[tokio::test]
    async fn a_persons_usage_operation_lifts_a_verdict_through_the_owner_and_the_offline_arm() {
        let root = tempfile::tempdir().unwrap();
        let runtime = fixtures::runtime(root.path(), signed_in, activates).await;
        let b = save(&runtime.store, "synthetic-b", "default");
        let up = upstream(&runtime).await;
        let verdict = || {
            let credential = runtime.store.stored_credential(&b.id).unwrap();
            runtime.refresh.reject(&b.id, &credential);
            assert_eq!(
                runtime.refresh.sign_in_required(&runtime.store),
                vec![b.id.clone()]
            );
        };

        verdict();
        assert!(usage(&runtime, &b.id).await.is_ok());
        assert_eq!(up.calls.load(Ordering::SeqCst), 1);
        assert!(
            runtime.refresh.sign_in_required(&runtime.store).is_empty(),
            "the owner's Usage operation is a person's check"
        );

        verdict();
        let answer = execute(
            runtime.store.clone(),
            &runtime.root,
            Some(runtime.as_ref()),
            Operation::Usage { id: b.id.clone() },
        )
        .await;
        assert!(answer.is_ok(), "{answer:?}");
        assert_eq!(up.calls.load(Ordering::SeqCst), 2);
        assert!(
            runtime.refresh.sign_in_required(&runtime.store).is_empty(),
            "the offline Usage arm is a person's check"
        );
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
                elsewhere: None,
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

#[cfg(test)]
mod launch_record_tests {
    use super::*;
    use fixtures::*;

    #[test]
    fn a_launch_from_outside_the_app_never_reads_as_the_window() {
        let launch = || Operation::KimiLaunch {
            id: "k".into(),
            working_directory: PathBuf::from("/x"),
        };
        let bare = from_outside(launch());
        let Operation::Launched { by, inner } = bare else {
            panic!("a bare launch is wrapped");
        };
        assert_eq!(by.surface, launches::Surface::Cli);
        assert!(matches!(*inner, Operation::KimiLaunch { .. }));
        let claimed = from_outside(Operation::Launched {
            by: launches::LaunchedBy::surface(launches::Surface::Window),
            inner: Box::new(launch()),
        });
        let Operation::Launched { by, .. } = claimed else {
            panic!();
        };
        assert_eq!(by.surface, launches::Surface::Cli);
        let dashboards = from_outside(Operation::Launched {
            by: launches::LaunchedBy::surface(launches::Surface::Dashboards),
            inner: Box::new(launch()),
        });
        let Operation::Launched { by, .. } = dashboards else {
            panic!();
        };
        assert_eq!(by.surface, launches::Surface::Dashboards);
        assert!(matches!(
            from_outside(Operation::Snapshot),
            Operation::Snapshot
        ));
        // The wire form a CLI sends, read back by the owner.
        let wire = serde_json::to_string(&Operation::Launched {
            by: launches::LaunchedBy {
                by: Some("voc-learn".into()),
                ..launches::LaunchedBy::surface(launches::Surface::Cli)
            },
            inner: Box::new(launch()),
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<Operation>(&wire).unwrap(),
            Operation::Launched { .. }
        ));
    }
    #[tokio::test]
    async fn a_malformed_launcher_name_or_a_wrapped_non_launch_is_refused_before_anything_runs() {
        let root = tempfile::tempdir().unwrap();
        let runtime = runtime(root.path(), signed_out, activates).await;
        let refused = runtime
            .execute(Operation::Launched {
                by: launches::LaunchedBy {
                    by: Some("two words".into()),
                    ..launches::LaunchedBy::surface(launches::Surface::Cli)
                },
                inner: Box::new(Operation::KimiLaunch {
                    id: "k".into(),
                    working_directory: root.path().to_owned(),
                }),
            })
            .await
            .unwrap_err();
        assert_eq!(refused, launches::BY_INVALID);
        let refused = runtime
            .execute(Operation::Launched {
                by: launches::LaunchedBy::surface(launches::Surface::Cli),
                inner: Box::new(Operation::Snapshot),
            })
            .await
            .unwrap_err();
        assert_eq!(refused, launches::NOT_A_LAUNCH);
        assert!(!root.path().join(launches::DIR).exists());
    }
    #[tokio::test]
    async fn the_history_and_one_launch_read_through_the_owner_and_a_hook_writes_its_turns() {
        let root = tempfile::tempdir().unwrap();
        let runtime = runtime(root.path(), signed_out, activates).await;
        let home = root.path().join("runtimes").join("claude-default");
        private_fs_dir(&home);
        let id = launches::new_id();
        launches::record(
            root.path(),
            &home,
            json!({"account_id": "a", "provider": "claude", "launched_at": 5}),
            &id,
        )
        .unwrap();
        let listed = runtime
            .execute(Operation::Launches { launch_id: None })
            .await
            .unwrap();
        assert_eq!(listed["total"], 1);
        assert_eq!(listed["launches"][0]["launch_id"], id.as_str());
        assert_eq!(listed["launches"][0]["agent"], "claude-code");
        assert_eq!(listed["launches"][0]["state"], "ended");
        let answer = runtime
            .execute(Operation::LaunchEvent {
                launch_id: id.clone(),
                event: launches::Event::Turn,
                provider_session_id: None,
                transcript: None,
            })
            .await
            .unwrap();
        assert_eq!(answer, json!({"recorded": true}));
        let shown = runtime
            .execute(Operation::Launches {
                launch_id: Some(id.clone()),
            })
            .await
            .unwrap();
        assert_eq!(shown["launch"]["turns"], 1);
        assert_eq!(shown["up"], json!([]));
        assert_eq!(shown["down"], json!([]));
    }
    fn private_fs_dir(path: &Path) {
        switchboard_core::private_fs::private_dir(path).unwrap();
    }
}
