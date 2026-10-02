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
use switchboard_runtime::{default_root, Operation, Owner};
use tauri::{Manager, State};

#[tauri::command]
async fn snapshot(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Snapshot).await
}
#[tauri::command]
async fn current_accounts(
    state: State<'_, Owner>,
    mode: State<'_, SmokeMode>,
) -> Result<Value, String> {
    if mode.0.is_some() {
        return Ok(json!({
            "claude": {"status":"missing", "identity":null, "account_id":null},
            "codex": {"status":"missing", "identity":null, "account_id":null}
        }));
    }
    state.runtime.execute(Operation::CurrentAccounts).await
}
#[tauri::command]
async fn capture_current(
    provider: Provider,
    label: Option<String>,
    pool: String,
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::CaptureCurrent {
            provider,
            label,
            pool,
        })
        .await
}
#[tauri::command]
async fn import_claude_swap(pool: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::ImportClaudeSwap { pool })
        .await
}
#[tauri::command]
async fn activate_native(id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::ActivateNative { id })
        .await
}
#[tauri::command]
async fn set_policy(policy: RotationPolicy, state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::SetPolicy { policy }).await
}
#[tauri::command]
async fn monitor_status(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::MonitorStatus).await
}
#[tauri::command]
async fn add_account(
    label: String,
    provider: Provider,
    kind: AuthKind,
    pool: String,
    secret: String,
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
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
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::Update { id, label, enabled })
        .await
}
#[tauri::command]
async fn remove_account(id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Remove { id }).await
}
#[tauri::command]
async fn select_account(
    provider: Provider,
    pool: String,
    id: String,
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::Select { provider, pool, id })
        .await
}
#[tauri::command]
async fn runtime_status(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Status).await
}
#[tauri::command]
async fn launch_account(
    id: String,
    mode: String,
    working_directory: String,
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
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
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::BeginLogin {
            provider,
            label,
            pool,
        })
        .await
}
#[tauri::command]
async fn finish_login(login_id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::FinishLogin { login_id })
        .await
}
#[tauri::command]
async fn login_status(login_id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::LoginStatus { login_id })
        .await
}
#[tauri::command]
async fn backups(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Backups).await
}
#[tauri::command]
async fn backup_now(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::BackupNow).await
}
#[tauri::command]
async fn restore_backup(file: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::RestoreBackup { file })
        .await
}
#[tauri::command]
async fn cancel_login(login_id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state
        .runtime
        .execute(Operation::CancelLogin { login_id })
        .await
}
#[tauri::command]
async fn probe_usage(id: String, state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Usage { id }).await
}
#[tauri::command]
async fn set_project_rule(
    path: std::path::PathBuf,
    account_id: String,
    target: String,
    enabled: bool,
    expires_at: Option<i64>,
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
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
    state: State<'_, Owner>,
) -> Result<Value, String> {
    state
        .runtime
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
    let result = tauri::Builder::default()
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
            let owner = if let Some(directory) = &temporary {
                tauri::async_runtime::block_on(Owner::start(
                    directory.path().to_owned(),
                    Arc::new(switchboard_core::MemoryVault::default()),
                ))
            } else {
                let root = default_root().map_err(std::io::Error::other)?;
                tauri::async_runtime::block_on(Owner::desktop(root))
            }
            .map_err(std::io::Error::other)?;
            app.manage(owner);
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
