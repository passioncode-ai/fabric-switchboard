#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde_json::Value;
use switchboard_core::{AuthKind, Provider};
use switchboard_runtime::{default_root, Operation, Owner};
use tauri::{Manager, State};

#[tauri::command]
async fn snapshot(state: State<'_, Owner>) -> Result<Value, String> {
    state.runtime.execute(Operation::Snapshot).await
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
fn main() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let root = default_root().map_err(std::io::Error::other)?;
            let owner = tauri::async_runtime::block_on(Owner::native(root))
                .map_err(std::io::Error::other)?;
            app.manage(owner);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            add_account,
            update_account,
            remove_account,
            select_account,
            runtime_status,
            launch_account,
            begin_login,
            finish_login,
            cancel_login,
            probe_usage
        ])
        .run(tauri::generate_context!());
    if result.is_err() {
        eprintln!("Fabric Switchboard could not start. Check app-data permissions, another running instance and native vault access.");
    }
}
