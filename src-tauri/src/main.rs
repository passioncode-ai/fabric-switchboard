#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::{json, Value};
use std::sync::Arc;
mod residency;
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
                }
            });
            *owner = Some(started.map_err(|error| {
                if error == "Another Switchboard instance owns this account storage" {
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
        })
        .await
}
#[tauri::command]
async fn remove_project(pool: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::RemoveProject { pool })
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
async fn openrouter_status(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyStatus {
            service: "openrouter".into(),
            credit: true,
        })
        .await
}
/// Saves the key typed into the panel; the answer is metadata only.
#[tauri::command]
async fn openrouter_save(
    key: String,
    model: Option<String>,
    state: State<'_, Slot>,
) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeySet {
            service: "openrouter".into(),
            key,
            model,
        })
        .await
}
#[tauri::command]
async fn openrouter_model(model: String, state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyModel {
            service: "openrouter".into(),
            model,
        })
        .await
}
#[tauri::command]
async fn openrouter_remove(state: State<'_, Slot>) -> Result<Value, String> {
    state
        .runtime()
        .await?
        .execute(Operation::AgentKeyRemove {
            service: "openrouter".into(),
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
#[tauri::command]
fn agent_setup() -> Value {
    switchboard_runtime::agents::setup()
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
/// Anonymous usage analytics (docs/ANALYTICS.md): `{available, enabled}`.
#[tauri::command]
async fn analytics_status(slot: State<'_, Slot>) -> Result<Value, String> {
    Ok(slot.runtime().await?.analytics_status())
}
#[tauri::command]
async fn set_analytics(slot: State<'_, Slot>, enabled: bool) -> Result<Value, String> {
    slot.runtime().await?.set_analytics(enabled)
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
    let mut builder = tauri::Builder::default();
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
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
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
                    .and_then(|exe| updates::availability(&exe, residency::packaged(&exe)))
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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                residency::hide(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            frontend_ready,
            set_language,
            login_item,
            set_login_item,
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
            add_account,
            update_account,
            remove_account,
            select_account,
            runtime_status,
            launch_account,
            begin_login,
            finish_login,
            login_status,
            backups,
            backup_now,
            restore_backup,
            cancel_login,
            probe_usage,
            set_project_rule,
            remove_project_rule,
            save_project,
            agent_connect,
            launch_agent,
            openrouter_status,
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
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("Fabric Switchboard")
            .chain(list.iter().copied())
            .map(String::from)
            .collect()
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
