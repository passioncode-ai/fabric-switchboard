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
/// How the desktop was launched. Only these flags mean anything; every other argument — a
/// Launch Services `-psn_…`, a flag a launcher adds — is ignored, never a failed start (SB-30).
#[derive(Debug, PartialEq, Eq, Default)]
struct Launch {
    /// `--smoke-test`: a temporary store and memory vault, exiting once the UI is ready.
    smoke: bool,
    /// `--background`: start without showing the window or taking focus — the local lifecycle
    /// broker's always-on start. The window appears on the next launch or a Dock click.
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
/// Shows the window and brings the app forward (it may have started in the background).
fn reveal(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    let Launch { smoke, background } = launch(std::env::args());
    let mut builder = tauri::Builder::default();
    // A second launch (double-click on Windows, `open -n` on macOS) focuses this window
    // instead of starting another owner that would find the store locked. The packaged smoke
    // check runs beside an installed app on purpose, with its own temporary store.
    if !smoke {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            reveal(app);
        }));
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
            // The window is created hidden (tauri.conf.json): shown now on an ordinary start;
            // in the background, left hidden, and on macOS without a Dock icon, so nothing comes
            // forward — the next launch or `reveal` brings it.
            if background {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            } else {
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
                    switchboard_runtime::arm_hard_exit(switchboard_runtime::HARD_EXIT_AFTER);
                    handle.exit(0);
                }
            });
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
        .build(tauri::generate_context!());
    let Ok(app) = built else {
        eprintln!("Fabric Switchboard could not start. Check app-data permissions, another running instance and native vault access.");
        std::process::exit(1);
    };
    app.run(|handle, event| {
        // Every quit path — Quit, Cmd-Q, the last window closing, a signal — ends here: the
        // owner stops its timers, finishes or abandons work in flight by the deadline,
        // removes its descriptor and releases the store (lifecycle LC-01).
        // macOS: clicking the Dock icon of an app started in the background shows its window.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = event
        {
            reveal(handle);
            return;
        }
        if let tauri::RunEvent::Exit = event {
            switchboard_runtime::arm_hard_exit(switchboard_runtime::HARD_EXIT_AFTER);
            let slot = handle.state::<Slot>();
            let owner = tauri::async_runtime::block_on(async { slot.owner.lock().await.take() });
            if let Some(owner) = owner {
                tauri::async_runtime::block_on(owner.shutdown(switchboard_runtime::DRAIN_DEADLINE));
            }
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
