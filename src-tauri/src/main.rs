#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::{json, Value};
use std::sync::Arc;
mod residency;
mod suggestions;
mod updates;
use residency::reveal;

struct SmokeMode(Option<tempfile::TempDir>);

#[tauri::command]
fn frontend_ready(app: tauri::AppHandle, mode: State<'_, SmokeMode>) {
    if mode.0.is_some() {
        // Whether the window is on screen, so the smoke check can tell an ordinary start from a
        // background one (SB-30).
        let visible = app
            .get_webview_window("main")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);
        println!(
            "SWITCHBOARD_WINDOW {}",
            if visible { "visible" } else { "hidden" }
        );
        println!(
            "SWITCHBOARD_TRAY {}",
            if app.tray_by_id("switchboard").is_some() {
                "present"
            } else {
                "missing"
            }
        );
        println!("SWITCHBOARD_FRONTEND_READY {}", env!("CARGO_PKG_VERSION"));
        app.exit(0);
    }
}
use switchboard_core::{AuthKind, Provider, RotationPolicy};
use switchboard_runtime::{default_root, Operation, Owner, Runtime};
use tauri::{Manager, State};

/// The owner of the store. Started at launch; when that fails — another Switchboard instance,
/// `switchboard serve` or a CLI command holds the store — every request reports why and the
/// next one tries again, so Retry recovers without restarting the app.
struct Slot {
    start: Box<dyn Fn() -> Result<std::path::PathBuf, String> + Send + Sync>,
    smoke: bool,
    /// How the app was launched, for analytics (`ordinary` or `background`).
    launch: &'static str,
    owner: tokio::sync::Mutex<Option<Owner>>,
}
const STARTUP_FAILED: &str = "Switchboard could not start its account service. Retry; if it keeps failing, quit and reopen Switchboard.";
const STORE_BUSY: &str = "Switchboard's account store is in use by another Switchboard (a second window, 'switchboard serve' or a command still running). Close it, then retry.";
impl Slot {
    /// The running owner, if any; never starts one (SB-117's watcher must not double the
    /// start-up retry).
    async fn running(&self) -> Option<std::sync::Arc<Runtime>> {
        self.owner.lock().await.as_ref().map(|o| o.runtime.clone())
    }
    async fn runtime(&self) -> Result<std::sync::Arc<Runtime>, String> {
        let mut owner = self.owner.lock().await;
        if owner.is_none() {
            let root = (self.start)()?;
            let started = if self.smoke {
                Owner::start(root, Arc::new(switchboard_core::MemoryVault::default())).await
            } else {
                Owner::desktop(root).await
            };
            let started = started.inspect(|owner| {
                // Release builds only (the App Key), the real data folder only (docs/ANALYTICS.md).
                if !self.smoke {
                    owner.runtime.enable_analytics(self.launch);
                    // SB-183: only an installed build writes the terminal hook's start files
                    // (never a development build); the runtime checks the data folder too.
                    if std::env::current_exe().is_ok_and(|exe| residency::packaged(&exe)) {
                        owner.runtime.enable_shell_hook();
                    }
                }
            });
            *owner = Some(started.map_err(|error| {
                if error == switchboard_core::STORE_LOCKED {
                    STORE_BUSY.to_string()
                } else {
                    // Start-up internals (proxy, control socket, data folder) are not the
                    // user's vocabulary; what they can do is the same for each.
                    STARTUP_FAILED.to_string()
                }
            })?);
        }
        owner
            .as_ref()
            .map(|o| o.runtime.clone())
            .ok_or_else(|| STORE_BUSY.to_string())
    }
}

#[tauri::command]
async fn snapshot(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::Snapshot).await
}
#[tauri::command]
async fn current_accounts(
    state: State<'_, Slot>,
    mode: State<'_, SmokeMode>,
) -> Result<Value, String> {
    if mode.0.is_some() {
        return Ok(json!({
            "claude": {"status":"missing", "identity":null, "account_id":null},
            "codex": {"status":"missing", "identity":null, "account_id":null}
        }));
    }
    state
        .runtime()
        .await?
        .execute(Operation::CurrentAccounts)
        .await
}
#[tauri::command]
async fn capture_current(
    provider: Provider,
    label: Option<String>,
    pool: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::CaptureCurrent {
            provider,
            label,
            pool,
        })
        .await
}
#[tauri::command]
async fn import_claude_swap(pool: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::ImportClaudeSwap { pool })
        .await
}
#[tauri::command]
async fn activate_native(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::ActivateNative { id })
        .await
}
#[tauri::command]
async fn set_policy(policy: RotationPolicy, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::SetPolicy { policy })
        .await
}
/// The sessions Switchboard launched that start or run (SB-65 over SB-95's list).
#[tauri::command]
async fn sessions(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Sessions { all: false })
        .await
}
#[tauri::command]
async fn monitor_status(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::MonitorStatus)
        .await
}
#[tauri::command]
async fn add_account(
    label: String,
    provider: Provider,
    kind: AuthKind,
    pool: String,
    secret: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Add {
            label,
            provider,
            kind,
            pool,
            secret,
        })
        .await
}
#[tauri::command]
async fn update_account(
    id: String,
    label: String,
    enabled: bool,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Update {
            id,
            label: Some(label),
            enabled: Some(enabled),
            elsewhere: None,
        })
        .await
}
/// SB-175: mark an account as used on another machine, or clear the mark.
#[tauri::command]
async fn mark_elsewhere(id: String, on: bool, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Update {
            id,
            label: None,
            enabled: None,
            elsewhere: Some(on),
        })
        .await
}
#[tauri::command]
async fn remove_account(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Remove { id })
        .await
}
#[tauri::command]
async fn select_account(
    provider: Provider,
    pool: String,
    id: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Select { provider, pool, id })
        .await
}
#[tauri::command]
async fn runtime_status(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::Status).await
}
#[tauri::command]
async fn launch_account(
    id: String,
    mode: String,
    working_directory: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Launch {
            id,
            mode,
            working_directory: working_directory.into(),
            in_place: false,
            args: Vec::new(),
        })
        .await
}
/// The window's interface language (L10N-01), for the tray menu. Anything but "ru" is English.
#[tauri::command]
fn set_language(app: tauri::AppHandle, locale: String) {
    residency::set_language(&app, locale == "ru");
}
#[tauri::command]
async fn begin_login(
    provider: Provider,
    label: String,
    pool: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::BeginLogin {
            provider,
            label,
            pool,
        })
        .await
}
#[tauri::command]
async fn finish_login(login_id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::FinishLogin { login_id })
        .await
}
/// Sign-ins the owner holds, so a window opened after a restart shows their banner (SB-65).
#[tauri::command]
async fn pending_logins(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::PendingLogins)
        .await
}
#[tauri::command]
async fn login_status(login_id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::LoginStatus { login_id })
        .await
}
#[tauri::command]
async fn backups(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::Backups).await
}
#[tauri::command]
async fn backup_now(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::BackupNow).await
}
#[tauri::command]
async fn restore_backup(file: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::RestoreBackup { file })
        .await
}
/// The picker could not answer (no window to attach it to, or it closed without a reply).
const FOLDER_PICKER_FAILED: &str = "The folder picker could not open. Try again.";
/// A folder chosen with the system picker: every folder Switchboard asks for is picked, never
/// typed (operator decision 2026-10-09). `start` opens the picker there when it is a folder.
/// `None` when the person closed the picker without choosing.
#[tauri::command]
async fn pick_folder(
    start: Option<String>,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let window = app.get_webview_window("main").ok_or(FOLDER_PICKER_FAILED)?;
    let (sender, picked) = tokio::sync::oneshot::channel();
    let mut picker = app.dialog().file().set_parent(&window);
    if let Some(start) = start.filter(|s| std::path::Path::new(s).is_dir()) {
        picker = picker.set_directory(start);
    }
    picker.pick_folder(move |folder| {
        let _ = sender.send(folder);
    });
    let folder = picked.await.map_err(|_| FOLDER_PICKER_FAILED)?;
    Ok(folder
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}
#[tauri::command]
async fn cancel_login(login_id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::CancelLogin { login_id })
        .await
}
#[tauri::command]
async fn probe_usage(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Usage { id })
        .await
}
#[tauri::command]
async fn set_project_rule(
    path: std::path::PathBuf,
    account_id: String,
    target: String,
    enabled: bool,
    expires_at: Option<i64>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::SetProjectRule {
            path,
            account_id,
            target,
            enabled,
            expires_at,
        })
        .await
}
#[tauri::command]
async fn remove_project_rule(
    path: std::path::PathBuf,
    provider: Provider,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::RemoveProjectRule { path, provider })
        .await
}
/// A project: its folders and the accounts reserved for them (0.6). `pool` updates one.
#[tauri::command]
async fn save_project(
    pool: Option<String>,
    name: String,
    folders: Vec<std::path::PathBuf>,
    account_ids: Vec<String>,
    base_folders: Option<Vec<std::path::PathBuf>>,
    organization: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::SaveProject {
            pool,
            name,
            folders,
            account_ids,
            base_folders,
            organization,
        })
        .await
}
/// SB-117/SB-131: folders added to projects or taken out as deleted, and those still being added.
#[tauri::command]
async fn project_suggestions(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::ProjectSuggestions)
        .await
}
#[tauri::command]
async fn accept_project_suggestion(
    folder: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::ProjectSuggestionAccept { folder })
        .await
}
#[tauri::command]
async fn dismiss_project_suggestion(
    folder: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::ProjectSuggestionDismiss { folder })
        .await
}
/// SB-122: an account into a project's pool, or back to `default`.
#[tauri::command]
async fn move_account(id: String, pool: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::MoveAccount { id, pool })
        .await
}
/// SB-158: organizations accounts belong to.
#[tauri::command]
async fn organization_add(
    name: String,
    domains: Vec<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::OrganizationAdd { name, domains })
        .await
}
#[tauri::command]
async fn organization_edit(
    id: String,
    name: Option<String>,
    domains: Option<Vec<String>>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::OrganizationEdit { id, name, domains })
        .await
}
#[tauri::command]
async fn organization_remove(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::OrganizationRemove { id })
        .await
}
#[tauri::command]
async fn organization_assign(
    account: String,
    choice: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::OrganizationAssign { account, choice })
        .await
}
/// SB-159: the PassionCode.ai connection: `status`, `connect` (browser sign-in, waits up to
/// 5 minutes), `cancel`, `read` or `disconnect`.
#[tauri::command]
async fn passioncode(action: String, state: State<'_, Slot>) -> Result<Value, String> {
    let operation = match action.as_str() {
        "status" => Operation::VestaStatus,
        "connect" => Operation::VestaConnect,
        "cancel" => Operation::VestaCancel,
        "read" => Operation::VestaSync,
        "disconnect" => Operation::VestaDisconnect,
        _ => return Err("Unknown action".into()),
    };
    state.runtime().await?.execute(operation).await
}
/// SB-143: Codex in project folders: the state, or `enabled` to switch it.
#[tauri::command]
async fn codex_route(enabled: Option<bool>, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::CodexRoute { enabled })
        .await
}
/// SB-135: a repository with its worktrees and clones into another project, in one step.
#[tauri::command]
async fn move_root(
    folder: String,
    pool: String,
    also: Option<bool>,
    from: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::MoveRoot {
            folder: folder.into(),
            pool,
            from,
            also: also.unwrap_or(false),
        })
        .await
}
/// SB-135: the Git repositories under the chosen folders, with their projects.
#[tauri::command]
async fn repository_map(under: Vec<String>, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::RepositoryMap {
            under: under.into_iter().map(Into::into).collect(),
        })
        .await
}
/// SB-145: what the proxy relayed in the last `days` days.
#[tauri::command]
async fn statistics(days: i64, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Statistics { days: Some(days) })
        .await
}
/// SB-135: every project's folders as roots with their worktrees and clones, read from Git.
#[tauri::command]
async fn project_roots(state: State<'_, Slot>) -> Result<Value, String> {
    let snapshot = state.runtime().await?.execute(Operation::Snapshot).await?;
    let projects: Vec<(String, Vec<String>)> = snapshot["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let folders = p["folders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect();
            (p["pool"].as_str().unwrap_or_default().to_owned(), folders)
        })
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = serde_json::Map::new();
        for (pool, folders) in projects {
            out.insert(
                pool,
                serde_json::to_value(switchboard_runtime::roots::tree(&folders))
                    .unwrap_or_default(),
            );
        }
        Value::Object(out)
    })
    .await
    .map_err(|_| "Project folders could not be read.".to_owned())
}
#[tauri::command]
async fn remove_project(pool: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::RemoveProject {
            pool,
            by: switchboard_runtime::spend::Surface::Window,
        })
        .await
}
/// Hermes's model and provider (SB-80): read, and changed only on the person's request.
#[tauri::command]
async fn hermes_model(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::HermesModel).await
}
#[tauri::command]
async fn hermes_set_model(
    provider: Option<String>,
    model: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::HermesSetModel { provider, model })
        .await
}
/// Kimi Code subscription accounts (SB-81): metadata and plan usage, never a token.
#[tauri::command]
async fn kimi_accounts(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiAccounts)
        .await
}
#[tauri::command]
async fn kimi_login_begin(
    label: String,
    region: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiLoginBegin { label, region })
        .await
}
#[tauri::command]
async fn kimi_login_status(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiLoginStatus { id })
        .await
}
#[tauri::command]
async fn kimi_login_finish(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiLoginFinish { id })
        .await
}
#[tauri::command]
async fn kimi_login_cancel(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiLoginCancel { id })
        .await
}
#[tauri::command]
async fn kimi_sign_in_again(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiSignInAgain { id })
        .await
}
#[tauri::command]
async fn kimi_launch(
    id: String,
    working_directory: std::path::PathBuf,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiLaunch {
            id,
            working_directory,
        })
        .await
}
#[tauri::command]
async fn kimi_remove(id: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::KimiRemove { id })
        .await
}
/// The OpenRouter key agents run on (SB-79): metadata and what it may still spend, asked of
/// OpenRouter. Never the key — `AgentKeyValue` is the CLI's alone.
#[tauri::command]
async fn openrouter_status(pool: Option<String>, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyStatus {
            service: "openrouter".into(),
            credit: true,
            pool,
        })
        .await
}
/// SB-144: the machine's OpenRouter key and each project's own, with the projects that have none.
/// Never a key.
#[tauri::command]
async fn openrouter_keys(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeys {
            service: "openrouter".into(),
        })
        .await
}
/// Saves the key typed into the panel; the answer is metadata only.
#[tauri::command]
async fn openrouter_save(
    key: String,
    model: Option<String>,
    pool: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeySet {
            service: "openrouter".into(),
            key,
            model,
            pool,
            by: switchboard_runtime::spend::Surface::Window,
        })
        .await
}
/// SB-71 (C-2): every fallback chain, the presets, what each step would do now (`readiness`) and
/// the last hand-over. Labels and codes only, never a credential.
#[tauri::command]
async fn chains(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::Chains {
            workflow: None,
            pool: None,
            folder: None,
        })
        .await
}
/// Sets a scope's chain in order (an empty list clears it); a preset id may stand for the list.
#[tauri::command]
async fn set_chain(
    scope: switchboard_core::ChainScope,
    executors: Vec<switchboard_core::Executor>,
    preset: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::SetChain {
            scope,
            executors,
            preset,
        })
        .await
}
/// SB-72 (C-7): every spending ceiling, the open questions oldest first and the last hand-over;
/// numbers and codes only. An unreadable spend.json is the core's error, never "none".
#[tauri::command]
async fn ceilings(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.execute(Operation::Ceilings).await
}
/// The window's own operations on ceilings: always as the window (C-Q1), which alone may raise,
/// clear or double a ceiling. They run in the app's process, never through the control listener.
fn window_set_ceiling(scope: switchboard_core::ChainScope, usd: Option<f64>) -> Operation {
    Operation::SetCeiling {
        scope,
        usd,
        by: switchboard_runtime::spend::Surface::Window,
    }
}
fn window_answer_ceiling(id: String, answer: switchboard_runtime::spend::Answer) -> Operation {
    Operation::AnswerCeiling {
        id,
        answer,
        by: switchboard_runtime::spend::Surface::Window,
    }
}
/// SB-230: the refusal for an amount that is neither dollars nor `null`, in the window's own words.
const CEILING_AMOUNT: &str = "Enter an amount from $0.01 to $1,000.";
/// `set_ceiling`'s amount: a number of dollars sets, `null` clears, anything else is refused.
fn ceiling_amount(usd: Value) -> Result<Option<f64>, String> {
    match usd {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .as_f64()
            .map(Some)
            .ok_or_else(|| CEILING_AMOUNT.to_owned()),
        _ => Err(CEILING_AMOUNT.to_owned()),
    }
}
/// Sets a scope's ceiling in dollars, or clears it with `usd: null`. SB-230: `usd` is a JSON value,
/// not `Option<f64>`, so a call that leaves it out is refused by Tauri's argument lookup rather than
/// read as a clear nobody asked for (as `SetCeiling`'s `spend::present` refuses it on the wire).
#[tauri::command]
async fn set_ceiling(
    scope: switchboard_core::ChainScope,
    usd: Value,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    let usd = ceiling_amount(usd)?;
    state
        .runtime()
        .await?
        .execute(window_set_ceiling(scope, usd))
        .await
}
/// Answers a reached ceiling's question: `double` or `keep`, from the person in the window.
#[tauri::command]
async fn answer_ceiling(
    id: String,
    answer: switchboard_runtime::spend::Answer,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(window_answer_ceiling(id, answer))
        .await
}
/// SB-120: every catalog agent — installed, enabled for switching, default, command.
#[tauri::command]
async fn agents_overview(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentsOverview)
        .await
}
#[tauri::command]
async fn agent_enable(
    agent: String,
    enabled: bool,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentEnable { agent, enabled })
        .await
}
#[tauri::command]
async fn agent_default(agent: Option<String>, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentDefault { agent })
        .await
}
/// SB-119: the recommended OpenRouter models and OpenRouter's tool-capable list.
#[tauri::command]
async fn openrouter_models(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::OpenrouterModels)
        .await
}
#[tauri::command]
async fn openrouter_model(
    model: String,
    pool: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyModel {
            service: "openrouter".into(),
            model,
            pool,
        })
        .await
}
#[tauri::command]
async fn openrouter_remove(pool: Option<String>, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyRemove {
            service: "openrouter".into(),
            pool,
            by: switchboard_runtime::spend::Surface::Window,
        })
        .await
}
/// How one third-party agent connects (catalog/agents.json); commands only, never a key.
#[tauri::command]
async fn agent_connect(
    agent: String,
    pool: String,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentConnect { agent, pool })
        .await
}
#[tauri::command]
async fn launch_agent(
    agent: String,
    pool: String,
    working_directory: std::path::PathBuf,
    via: Option<switchboard_runtime::AgentVia>,
    model: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::LaunchAgent {
            agent,
            pool,
            working_directory,
            via: via.unwrap_or_default(),
            model,
        })
        .await
}
/// The Agents panel's setup facts, plus the proving call this app's data folder recorded (SB-93).
#[tauri::command]
async fn agent_setup(state: State<'_, Slot>) -> Result<Value, String> {
    let mut setup = switchboard_runtime::agents::setup();
    if let Ok(runtime) = state.runtime().await {
        setup["proved"] =
            switchboard_runtime::onboarding::proof(&runtime.root).unwrap_or(Value::Null);
    }
    Ok(setup)
}
/// SB-202 (B-8): the organization checklist for the window's first run, the same steps
/// `switchboard onboard` reads, filled from this owner. Only the organization steps are read by
/// the window; the agent's MCP registration is not looked up here.
#[tauri::command]
async fn onboard(state: State<'_, Slot>) -> Result<Value, String> {
    let runtime = state.runtime().await?;
    let snapshot = runtime.execute(Operation::Snapshot).await?;
    let (saved, partial) = switchboard_runtime::onboarding::read_snapshot(&snapshot);
    let organizations = saved.as_ref().is_some_and(|s| !s.organizations.is_empty());
    let (monitor, sessions, shell) = if organizations {
        (
            runtime.execute(Operation::MonitorStatus).await.ok(),
            runtime
                .execute(Operation::Sessions { all: true })
                .await
                .ok(),
            Some(runtime.shell_hook_status()),
        )
    } else {
        (None, None, None)
    };
    let root = runtime.root.clone();
    let passioncode = tokio::task::spawn_blocking(move || {
        switchboard_runtime::vesta::status(&root).is_ok_and(|v| v["connected"] == true)
    })
    .await
    .unwrap_or(false);
    let probe = onboard_probe(OnboardReads {
        snapshot: &snapshot,
        saved,
        partial,
        monitor: monitor.as_ref(),
        sessions: sessions.as_ref(),
        shell,
        passioncode,
    });
    Ok(switchboard_runtime::onboarding::steps(&probe))
}
/// What `onboard` read; a read that failed is `None` and named in `degraded`.
struct OnboardReads<'a> {
    snapshot: &'a Value,
    saved: Option<switchboard_core::Snapshot>,
    partial: bool,
    monitor: Option<&'a Value>,
    sessions: Option<&'a Value>,
    shell: Option<Value>,
    passioncode: bool,
}
fn onboard_probe(reads: OnboardReads<'_>) -> switchboard_runtime::onboarding::Probe {
    let mut probe = switchboard_runtime::onboarding::Probe {
        owner_online: true,
        passioncode: reads.passioncode,
        ..Default::default()
    };
    for provider in ["claude", "codex"] {
        let count = reads.snapshot["accounts"].as_array().map_or(0, |accounts| {
            accounts
                .iter()
                .filter(|a| a["provider"] == provider && a["enabled"] == true)
                .count()
        });
        probe.accounts.push((provider.into(), count));
    }
    if reads.partial {
        probe.degraded.push("snapshot".into());
    }
    if reads
        .saved
        .as_ref()
        .is_some_and(|s| !s.organizations.is_empty())
    {
        let ids = |value: Option<&Value>, list: &str, field: Option<&str>| -> Option<Vec<String>> {
            Some(
                value?[list]
                    .as_array()?
                    .iter()
                    .filter_map(|item| match field {
                        Some(field) => item[field].as_str(),
                        None => item.as_str(),
                    })
                    .map(str::to_owned)
                    .collect(),
            )
        };
        match ids(reads.monitor, "sign_in_required", None) {
            Some(list) => probe.sign_in_required = list,
            None => probe.degraded.push("sign_ins".into()),
        }
        match ids(reads.sessions, "sessions", Some("context")) {
            Some(list) => probe.contexts = list,
            None => probe.degraded.push("sessions".into()),
        }
        probe.shell = reads.shell;
    }
    probe.snapshot = reads.saved;
    probe
}
#[tauri::command]
fn link_cli() -> Result<Value, String> {
    switchboard_runtime::agents::link_bundled_cli()
}
/// How the desktop was launched. Only these flags mean anything; every other argument — a
/// Launch Services `-psn_…`, a flag a launcher adds — is ignored, never a failed start (SB-30).
#[derive(Debug, PartialEq, Eq, Default)]
struct Launch {
    /// `--smoke-test`: a temporary store and memory vault, exiting once the UI is ready.
    smoke: bool,
    /// `--background`: start without showing the window or taking focus — the local lifecycle
    /// broker's always-on start. The window appears when the app is opened again.
    background: bool,
}
fn launch(arguments: impl IntoIterator<Item = String>) -> Launch {
    let mut launch = Launch::default();
    for argument in arguments.into_iter().skip(1) {
        match argument.as_str() {
            "--smoke-test" => launch.smoke = true,
            "--background" => launch.background = true,
            _ => {}
        }
    }
    launch
}
/// Anonymous usage analytics (docs/ANALYTICS.md): `{available, enabled, disclosed, sent_before}`;
/// `set_analytics` is also the person's answer that lets anything be sent (issue #128); `question`
/// carries the switch the question showed, absent for About's switch.
#[tauri::command]
async fn analytics_status(slot: State<'_, Slot>) -> Result<Value, String> {
    Ok(slot.runtime().await?.analytics_status())
}
#[tauri::command]
async fn set_analytics(
    slot: State<'_, Slot>,
    enabled: bool,
    question: Option<bool>,
) -> Result<Value, String> {
    slot.runtime().await?.set_analytics(enabled, question)
}
/// SB-183 (node A-3): the terminal hook's first-time notice, `{organization, group}`, or null.
/// It waits in `shell-hook.json` until the window acknowledges it, so a restart before the window
/// showed it keeps it; node A-7 draws it.
#[tauri::command]
async fn shell_hook_notice(state: State<'_, Slot>) -> Result<Value, String> {
    Ok(state
        .runtime()
        .await?
        .shell_hook_notice()
        .unwrap_or(Value::Null))
}
/// SB-183 (node A-7): the Groups page's *Terminals in organization folders* section: the hook's
/// state, each start file, the organizations it follows and whether this app may change it.
#[tauri::command]
async fn shell_hook_status(state: State<'_, Slot>) -> Result<Value, String> {
    Ok(state.runtime().await?.shell_hook_status())
}
/// *Turn off* / *Turn on* in that section: the same as `switchboard shell off|on`, through the
/// installed owner; any other app refuses and changes nothing.
#[tauri::command]
async fn set_shell_hook(state: State<'_, Slot>, on: bool) -> Result<Value, String> {
    state.runtime().await?.set_shell_hook(on)
}
/// The window showed the first-time notice: never shown again.
#[tauri::command]
async fn acknowledge_shell_hook_notice(state: State<'_, Slot>) -> Result<Value, String> {
    state.runtime().await?.acknowledge_shell_hook_notice();
    Ok(Value::Null)
}
/// Whether Switchboard opens at login (SB-28). Unavailable in a development build and the smoke
/// check.
#[tauri::command]
fn login_item(app: tauri::AppHandle, mode: State<'_, SmokeMode>) -> Value {
    if mode.0.is_some() {
        return json!({ "available": false, "enabled": false });
    }
    residency::status(&app)
}
#[tauri::command]
fn set_login_item(
    app: tauri::AppHandle,
    mode: State<'_, SmokeMode>,
    enabled: bool,
) -> Result<Value, String> {
    if mode.0.is_some() {
        return Err("Opening at login is available in the installed app only.".into());
    }
    residency::set(&app, &default_root()?, enabled)
}
/// Automatic updates (SB-55): `{available, reason, enabled, state, current, version, …}`.
#[tauri::command]
fn update_status(updates: State<'_, updates::Updates>) -> Value {
    updates.status()
}
#[tauri::command]
fn set_auto_update(
    app: tauri::AppHandle,
    updates: State<'_, updates::Updates>,
    enabled: bool,
) -> Result<Value, String> {
    let pending = updates.has_pending_install();
    let status = updates.set(enabled)?;
    // Switched off with an installer waiting (Windows): it was discarded, so the tray's
    // "Restart to update" goes too.
    if pending && !updates.has_pending_install() {
        residency::hide_update(&app);
    }
    Ok(status)
}
#[tauri::command]
async fn restart_to_update(app: tauri::AppHandle) -> Result<Value, String> {
    updates::restart(app).await
}
/// The person's *Check for updates* (LC-16): runs with the switch off too.
#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<Value, String> {
    updates::check_now(app).await
}

fn main() {
    let Launch { smoke, background } = launch(std::env::args());
    // The folder picker is called from Rust (`pick_folder`); the window holds no dialog or
    // filesystem permission of its own.
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());
    // A second launch (double-click on Windows, `open -n` on macOS) focuses this window
    // instead of starting another owner that would find the store locked. The packaged smoke
    // check runs beside an installed app on purpose, with its own temporary store.
    if !smoke {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                residency::second_launch(app, &args);
            }))
            .plugin(residency::plugin())
            .plugin(tauri_plugin_updater::Builder::new().build());
    }
    if !smoke {
        if let Some(log) = switchboard_runtime::oplog::Log::default_location() {
            switchboard_runtime::oplog::install(log);
        }
    }
    let built = builder
        .setup(move |app| {
            let temporary = if smoke {
                Some(
                    tempfile::Builder::new()
                        .prefix("switchboard-smoke-")
                        .tempdir()?,
                )
            } else {
                None
            };
            let smoke_root = temporary.as_ref().map(|t| t.path().to_owned());
            let slot = Slot {
                start: Box::new(move || match &smoke_root {
                    Some(root) => Ok(root.clone()),
                    None => default_root(),
                }),
                smoke,
                launch: if background { "background" } else { "ordinary" },
                owner: tokio::sync::Mutex::new(None),
            };
            // SIGTERM and SIGINT are caught before the owner starts and publishes its control
            // descriptor, so a stop during start-up drains as Quit does (LC-01).
            let signals = tauri::async_runtime::block_on(async {
                switchboard_runtime::StopSignals::listen()
            });
            // Start now so the monitor runs from launch; a failure is reported by the first
            // request instead of aborting the app (Tauri turns a setup error into a crash).
            let started = tauri::async_runtime::block_on(slot.runtime()).is_ok();
            app.manage(slot);
            if !started {
                // Keep trying in the background as well: renewals, rotation and backups must
                // resume once the store is free even while the window stays hidden.
                // SB-181 seam review: after an update restart a `switchboard` command that
                // waited out the drain can hold the store offline for a moment; the first 15
                // tries come a second apart, so the new copy owns it as soon as it is free.
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    for attempt in 0u32.. {
                        let wait = if attempt < 15 { 1 } else { 30 };
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        if handle.state::<Slot>().runtime().await.is_ok() {
                            break;
                        }
                    }
                });
            }
            app.manage(SmokeMode(temporary));

            // Residency (SB-28): the tray keeps Open and Quit reachable while the window is
            // hidden; the login item starts Switchboard in the background.
            if let Err(error) = residency::tray(app.handle()) {
                eprintln!("Switchboard could not add its menu-bar icon: {error}");
            }
            // SB-117: folders found for a project — a notification when one appears, the count
            // next to the tray icon until each is answered, and the window's banner. Started
            // after the tray, so the first count has an icon to sit next to.
            if !smoke {
                suggestions::watch(app.handle().clone());
            }
            if !smoke {
                if let Ok(root) = default_root() {
                    residency::apply_at_start(app.handle(), &root);
                }
            }
            // Automatic updates (SB-55): never in the smoke check or a development build.
            let available = if smoke {
                Err(updates::UNAVAILABLE_DEVELOPMENT)
            } else {
                std::env::current_exe()
                    .map_err(|_| updates::UNAVAILABLE_DEVELOPMENT)
                    .and_then(|exe| {
                        updates::availability(
                            &exe,
                            residency::packaged(&exe),
                            std::env::var_os("APPIMAGE").as_ref(),
                        )
                    })
            };
            let root = if smoke { None } else { default_root().ok() };
            app.manage(updates::Updates::new(available, root));
            updates::start(app.handle());
            // The window is created hidden (tauri.conf.json): shown now on an ordinary start;
            // in the background, left hidden — opening the app again (`reveal`) brings it.
            if !background {
                reveal(app.handle());
            }
            // SIGTERM (logout, `kill`, an updater) and SIGINT end the app the way Quit does:
            // through the exit event below, which drains the owner (lifecycle LC-01).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let Ok(signals) = signals else {
                    return;
                };
                if let Ok(signal) = signals.requested().await {
                    switchboard_runtime::oplog::event(
                        "stop_requested",
                        &[("signal", switchboard_runtime::oplog::Field::Code(signal))],
                    );
                    handle.state::<updates::Updates>().note_signal();
                    switchboard_runtime::arm_hard_exit(switchboard_runtime::HARD_EXIT_AFTER);
                    handle.exit(0);
                }
            });
            Ok(())
        })
        // Closing the window hides it; only Quit ends Switchboard (SB-28).
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                residency::hide(window);
            }
            // SB-131: focus gained or lost is the person's attention; an update waits for a
            // window that has had none for half an hour, not for one merely left open.
            tauri::WindowEvent::Focused(_) => {
                if let Some(updates) = window.app_handle().try_state::<updates::Updates>() {
                    updates.note_attention();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            frontend_ready,
            set_language,
            login_item,
            set_login_item,
            shell_hook_notice,
            acknowledge_shell_hook_notice,
            shell_hook_status,
            onboard,
            set_shell_hook,
            update_status,
            set_auto_update,
            restart_to_update,
            check_for_updates,
            analytics_status,
            set_analytics,
            snapshot,
            current_accounts,
            capture_current,
            import_claude_swap,
            activate_native,
            set_policy,
            monitor_status,
            sessions,
            add_account,
            update_account,
            mark_elsewhere,
            remove_account,
            select_account,
            runtime_status,
            launch_account,
            begin_login,
            finish_login,
            login_status,
            pending_logins,
            backups,
            backup_now,
            restore_backup,
            cancel_login,
            pick_folder,
            probe_usage,
            set_project_rule,
            remove_project_rule,
            save_project,
            agent_connect,
            launch_agent,
            openrouter_status,
            openrouter_keys,
            openrouter_save,
            openrouter_model,
            openrouter_remove,
            kimi_accounts,
            hermes_model,
            hermes_set_model,
            kimi_login_begin,
            kimi_login_status,
            kimi_login_finish,
            kimi_login_cancel,
            kimi_sign_in_again,
            kimi_launch,
            kimi_remove,
            remove_project,
            project_roots,
            move_root,
            codex_route,
            repository_map,
            statistics,
            move_account,
            organization_add,
            organization_edit,
            organization_remove,
            organization_assign,
            passioncode,
            agents_overview,
            chains,
            set_chain,
            ceilings,
            set_ceiling,
            answer_ceiling,
            agent_enable,
            agent_default,
            openrouter_models,
            project_suggestions,
            accept_project_suggestion,
            dismiss_project_suggestion,
            agent_setup,
            link_cli
        ])
        .build(tauri::generate_context!());
    let Ok(mut app) = built else {
        eprintln!("Fabric Switchboard could not start. Check app-data permissions, another running instance and native vault access.");
        std::process::exit(1);
    };
    // macOS: in the background the app runs with the Prohibited policy — no Dock icon, and the
    // launch's own activation request is refused (measured: Accessory still came forward). Set on the
    // built app, before the event loop launches, so the launch itself never runs as a regular
    // app (setup runs only after the launch would already have activated it).
    #[cfg(target_os = "macos")]
    if background {
        app.set_activation_policy(tauri::ActivationPolicy::Prohibited);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = &mut app;
    app.run(move |handle, event| {
        // Background start: once launched, the Prohibited policy gives way to Accessory so the
        // menu-bar icon works; still no Dock icon and no window.
        #[cfg(target_os = "macos")]
        if background {
            if let tauri::RunEvent::Ready = event {
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                return;
            }
        }
        // The last window closing is not a quit (SB-28); Quit, Cmd-Q and a signal carry a code.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = &event
        {
            api.prevent_exit();
            return;
        }
        // macOS: opening the app again (Finder, Spotlight, `open -a`) while its window is hidden
        // shows it.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = event
        {
            reveal(handle);
            return;
        }
        // Every quit path — the tray's Quit, the app menu's Quit, Cmd-Q, a signal — ends here: the
        // owner stops its timers, finishes or abandons work in flight by the deadline,
        // removes its descriptor and releases the store (lifecycle LC-01).
        if let tauri::RunEvent::Exit = event {
            switchboard_runtime::arm_hard_exit(switchboard_runtime::HARD_EXIT_AFTER);
            let slot = handle.state::<Slot>();
            let owner = tauri::async_runtime::block_on(async { slot.owner.lock().await.take() });
            if let Some(owner) = owner {
                // An exit that starts the Windows installer drains for less, so the installer
                // keeps half of the hard-exit window (SB-78).
                let deadline = if handle.state::<updates::Updates>().installs_at_exit() {
                    updates::DRAIN_BEFORE_INSTALL
                } else {
                    switchboard_runtime::DRAIN_DEADLINE
                };
                tauri::async_runtime::block_on(owner.shutdown(deadline));
            }
            // After the drain: the Windows installer, or the relaunch after "Restart to update".
            updates::finish(handle);
        }
    });
}

#[cfg(test)]
mod tests {
    /// B-8: the window's checklist is filled from this owner's reads; a read that failed is
    /// named, and a machine without organizations reads nothing more.
    #[test]
    fn the_window_checklist_reads_the_owner_and_names_what_failed() {
        let snapshot = json!({"accounts": [
            {"provider": "claude", "enabled": true},
            {"provider": "claude", "enabled": false},
            {"provider": "codex", "enabled": true}
        ]});
        let mut saved = switchboard_core::Snapshot::default();
        let reads = |saved: Option<switchboard_core::Snapshot>, monitor: Option<&'static Value>| {
            onboard_probe(OnboardReads {
                snapshot: Box::leak(Box::new(snapshot.clone())),
                saved,
                partial: false,
                monitor,
                sessions: None,
                shell: Some(json!({"hook": "off"})),
                passioncode: true,
            })
        };
        let plain = reads(Some(saved.clone()), None);
        assert_eq!(
            plain.accounts,
            vec![("claude".into(), 1), ("codex".into(), 1)]
        );
        assert!(
            plain.degraded.is_empty(),
            "no organization: nothing more read"
        );
        assert!(plain.shell.is_none());
        saved
            .organizations
            .push(switchboard_core::organizations::Organization {
                id: "o-acme".into(),
                name: "Acme".into(),
                domains: vec![],
                provider_organizations: vec![],
                created_at: 1,
                vesta_id: None,
            });
        let monitor: &'static Value = Box::leak(Box::new(json!({"sign_in_required": ["a-1"]})));
        let probe = reads(Some(saved), Some(monitor));
        assert_eq!(probe.sign_in_required, vec!["a-1".to_owned()]);
        assert_eq!(probe.degraded, vec!["sessions".to_owned()]);
        assert_eq!(probe.shell, Some(json!({"hook": "off"})));
        assert!(probe.passioncode);
    }
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("Fabric Switchboard")
            .chain(list.iter().copied())
            .map(String::from)
            .collect()
    }

    /// SB-230: `set_ceiling` takes its amount as a JSON value, so a call without `usd` is refused
    /// by Tauri (a non-optional argument is looked up with `deserialize_any`, which fails on an
    /// absent key, tauri 2.12 `ipc/command.rs` `CommandItem::deserialize_json`) instead of being
    /// read as a clear, as `Option<f64>` was; `null` clears, a number sets, anything else refuses.
    #[test]
    fn set_ceiling_refuses_anything_but_dollars_or_null() {
        assert_eq!(ceiling_amount(json!(null)), Ok(None));
        assert_eq!(ceiling_amount(json!(5.5)), Ok(Some(5.5)));
        assert_eq!(ceiling_amount(json!(5)), Ok(Some(5.0)));
        for refused in [json!("5"), json!(true), json!([5]), json!({"usd": 5})] {
            assert_eq!(ceiling_amount(refused), Err(CEILING_AMOUNT.to_owned()));
        }
    }

    /// C-7 (C-Q1): the window's ceiling commands always speak as the window, and the answer
    /// and amount travel unchanged; `answer` reads the window's words `keep` and `double`.
    #[test]
    fn the_windows_ceiling_commands_speak_as_the_window() {
        use switchboard_runtime::spend::{Answer, Surface};
        let set = window_set_ceiling(switchboard_core::ChainScope::Machine, Some(5.0));
        assert!(matches!(
            set,
            Operation::SetCeiling { scope: switchboard_core::ChainScope::Machine, usd: Some(u), by: Surface::Window } if u == 5.0
        ));
        let clear = window_set_ceiling(
            switchboard_core::ChainScope::Project {
                pool: "alpha".into(),
            },
            None,
        );
        assert!(matches!(
            clear,
            Operation::SetCeiling {
                usd: None,
                by: Surface::Window,
                ..
            }
        ));
        let answer: Answer = serde_json::from_value(json!("double")).unwrap();
        assert!(matches!(
            window_answer_ceiling("q_0123456789abcdef".into(), answer),
            Operation::AnswerCeiling { answer: Answer::Double, by: Surface::Window, ref id } if id == "q_0123456789abcdef"
        ));
        assert!(serde_json::from_value::<Answer>(json!("raise")).is_err());
        // The window's scope shape is the core's.
        let scope: switchboard_core::ChainScope =
            serde_json::from_value(json!({"kind": "project", "pool": "alpha"})).unwrap();
        assert_eq!(
            scope,
            switchboard_core::ChainScope::Project {
                pool: "alpha".into()
            }
        );
    }
    /// SB-30: an argument the app does not know never stops it from starting.
    #[test]
    fn unknown_arguments_are_ignored_and_background_is_recognised() {
        assert_eq!(launch(args(&[])), Launch::default());
        assert_eq!(
            launch(args(&[
                "-psn_0_12345",
                "--unknown",
                "value",
                "--Background"
            ])),
            Launch::default()
        );
        assert_eq!(
            launch(args(&["--background"])),
            Launch {
                smoke: false,
                background: true
            }
        );
        assert_eq!(
            launch(args(&["--smoke-test", "--background"])),
            Launch {
                smoke: true,
                background: true
            }
        );
        // The program name itself is not an argument.
        assert_eq!(launch(["--background".to_string()]), Launch::default());
    }
}
