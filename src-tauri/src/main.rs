#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::{json, Value};
use std::sync::Arc;

struct SmokeMode(Option<tempfile::TempDir>);

#[tauri::command]
fn frontend_ready(app: tauri::AppHandle, mode: State<'_, SmokeMode>) {
    if mode.0.is_some() {
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
        .execute(Operation::Update { id, label, enabled })
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
        })
        .await
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
#[tauri::command]
fn agent_setup() -> Value {
    switchboard_runtime::agents::setup()
}
#[tauri::command]
fn link_cli() -> Result<Value, String> {
    switchboard_runtime::agents::link_bundled_cli()
}
fn main() {
    let smoke = std::env::args().any(|argument| argument == "--smoke-test");
    let mut builder = tauri::Builder::default();
    // A second launch (double-click on Windows, `open -n` on macOS) focuses this window
    // instead of starting another owner that would find the store locked. The packaged smoke
    // check runs beside an installed app on purpose, with its own temporary store.
    if !smoke {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }
    let result = builder
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
                owner: tokio::sync::Mutex::new(None),
            };
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            frontend_ready,
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
            agent_setup,
            link_cli
        ])
        .run(tauri::generate_context!());
    if result.is_err() {
        eprintln!("Fabric Switchboard could not start. Check app-data permissions, another running instance and native vault access.");
        std::process::exit(1);
    }
}
