#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod launch;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use switchboard_core::{
    Account, AuthKind, Credential, NativeVault, Provider, Snapshot, Store, Usage,
};
use switchboard_proxy::ProxyHandle;
use tauri::{Manager, State};
struct AppState {
    store: Arc<Store>,
    proxy: ProxyHandle,
    root: std::path::PathBuf,
    logins: Mutex<HashMap<String, launch::Login>>,
}
#[tauri::command]
fn snapshot(state: State<AppState>) -> Result<Snapshot, String> {
    state.store.snapshot()
}
#[tauri::command]
fn add_account(
    label: String,
    provider: Provider,
    kind: AuthKind,
    pool: String,
    secret: String,
    state: State<AppState>,
) -> Result<Account, String> {
    let credential = Credential::parse(provider, kind, &secret)?;
    state.store.add(label, provider, kind, pool, credential)
}
#[tauri::command]
fn update_account(
    id: String,
    label: String,
    enabled: bool,
    state: State<AppState>,
) -> Result<(), String> {
    state.store.update(&id, label, enabled)
}
#[tauri::command]
fn remove_account(id: String, state: State<AppState>) -> Result<(), String> {
    let snap = state.store.snapshot()?;
    if snap.routes.values().any(|v| v == &id) {
        return Err("Select another account before removing this one.".into());
    }
    launch::clean_account(&state.root, &id)?;
    state.store.remove(&id)
}
#[tauri::command]
fn select_account(
    provider: Provider,
    pool: String,
    id: String,
    state: State<AppState>,
) -> Result<(), String> {
    state.store.select(provider, &pool, &id)
}
#[derive(Serialize)]
struct RuntimeStatus {
    proxy_address: String,
    platform: String,
    live_mode: String,
}
#[tauri::command]
fn runtime_status(state: State<AppState>) -> RuntimeStatus {
    RuntimeStatus {
        proxy_address: state.proxy.address().to_string(),
        platform: std::env::consts::OS.into(),
        live_mode: "Next request · HTTP/SSE".into(),
    }
}
#[derive(Serialize)]
struct Message {
    message: String,
}
#[tauri::command]
fn launch_account(
    id: String,
    mode: String,
    working_directory: String,
    state: State<AppState>,
) -> Result<Message, String> {
    launch::launch(
        &state.root,
        &state.store,
        &state.proxy,
        &id,
        &mode,
        std::path::Path::new(&working_directory),
    )?;
    Ok(Message {
        message: "Terminal launch requested. Provider response is not yet verified.".into(),
    })
}
#[derive(Serialize)]
struct LoginStarted {
    login_id: String,
    message: String,
}
#[tauri::command]
fn begin_login(
    provider: Provider,
    label: String,
    pool: String,
    state: State<AppState>,
) -> Result<LoginStarted, String> {
    let mut logins = state
        .logins
        .lock()
        .map_err(|_| "Sign-in state unavailable.")?;
    if logins.len() >= 4 {
        return Err("Finish an existing sign-in before starting another.".into());
    }
    let login = launch::begin_login(&state.root, provider, label, pool)?;
    let login_id = login.id.clone();
    logins.insert(login_id.clone(), login);
    Ok(LoginStarted {
        login_id,
        message: "Finish official sign-in in Terminal, then choose Finish sign-in.".into(),
    })
}
#[tauri::command]
fn finish_login(login_id: String, state: State<AppState>) -> Result<Account, String> {
    let mut logins = state
        .logins
        .lock()
        .map_err(|_| "Sign-in state unavailable.")?;
    let login = logins
        .get_mut(&login_id)
        .ok_or("Sign-in not found. Start again.")?;
    let account = if let Some(account) = &login.saved {
        account.clone()
    } else {
        let credential = launch::capture_login(login)?;
        let account = state.store.add(
            login.label.clone(),
            login.provider,
            AuthKind::OAuth,
            login.pool.clone(),
            credential,
        )?;
        login.saved = Some(account.clone());
        account
    };
    launch::clean_login(login)?;
    logins.remove(&login_id);
    Ok(account)
}
#[tauri::command]
fn cancel_login(login_id: String, state: State<AppState>) -> Result<(), String> {
    let mut logins = state
        .logins
        .lock()
        .map_err(|_| "Sign-in state unavailable.")?;
    let login = logins
        .get(&login_id)
        .ok_or("Sign-in not found. Start again.")?;
    launch::cancel_login(login)?;
    logins.remove(&login_id);
    Ok(())
}
#[tauri::command]
async fn probe_usage(id: String, state: State<'_, AppState>) -> Result<Usage, String> {
    switchboard_proxy::probe_usage(state.store.clone(), id).await
}
fn main() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let store = Arc::new(
                Store::open(root.clone(), Arc::new(NativeVault::new()))
                    .map_err(std::io::Error::other)?,
            );
            let proxy = tauri::async_runtime::block_on(ProxyHandle::start(store.clone()))
                .map_err(std::io::Error::other)?;
            app.manage(AppState {
                store,
                proxy,
                root,
                logins: Mutex::new(HashMap::new()),
            });
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
        eprintln!("Fabric Switchboard could not start. Check app-data permissions, another running instance and Keychain access.");
    }
}
