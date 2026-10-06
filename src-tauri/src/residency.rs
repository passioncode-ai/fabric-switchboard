//! Residency (SB-28, lifecycle LC-09): Switchboard keeps running after its window closes, starts
//! at login in the background, and ends only through Quit — the tray menu, the app menu or
//! Cmd-Q. Background rotation, renewal and backups need the owner alive; a closed window used
//! to stop them.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "macos"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tauri_plugin_autostart::ManagerExt;

/// The person's choice for opening at login, kept in the data folder. Absent means the default:
/// on (operator decision 2026-10-05).
pub const LOGIN_ITEM_FILE: &str = "login-item";
/// The argument the login item starts the app with: no window, no focus taken (SB-30).
pub const BACKGROUND_ARG: &str = "--background";
/// The macOS LaunchAgent label and file name (`~/Library/LaunchAgents/<name>.plist`).
#[cfg(target_os = "macos")]
pub const LOGIN_ITEM_NAME: &str = "ai.passioncode.fabric-switchboard";

#[derive(Debug, PartialEq, Eq)]
pub enum AtStart {
    /// Register (or refresh the path of) the login item.
    Enable,
    /// Remove a login item the person switched off.
    Disable,
    /// A development build or the smoke check: touch nothing.
    Leave,
}

/// What a start does with the login item, from the saved choice and whether this is an installed
/// build. A development binary must never register itself to open at login.
pub fn at_start(choice: Option<&str>, packaged: bool) -> AtStart {
    if !packaged {
        return AtStart::Leave;
    }
    match choice.map(str::trim) {
        Some("off") => AtStart::Disable,
        _ => AtStart::Enable,
    }
}

/// An installed build: a release binary, and on macOS one inside an app bundle.
pub fn packaged(exe: &Path) -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    if cfg!(target_os = "macos") {
        return exe.to_string_lossy().contains(".app/Contents/MacOS/");
    }
    true
}

fn choice_path(root: &Path) -> PathBuf {
    root.join(LOGIN_ITEM_FILE)
}

fn read_choice(root: &Path) -> Option<String> {
    std::fs::read_to_string(choice_path(root)).ok()
}

fn write_choice(root: &Path, enabled: bool) -> Result<(), String> {
    std::fs::write(choice_path(root), if enabled { "on\n" } else { "off\n" })
        .map_err(|_| "Could not save the choice in Switchboard's data folder.".to_string())
}

/// The autostart plugin, registering the app with `--background`.
pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri_plugin_autostart::Builder::new().arg(BACKGROUND_ARG);
    #[cfg(target_os = "macos")]
    let builder = builder
        .macos_launcher(tauri_plugin_autostart::MacosLauncher::LaunchAgent)
        .app_name(LOGIN_ITEM_NAME);
    builder.build()
}

/// Applies the saved choice at start. Enabling again rewrites the entry with the current path,
/// so a moved app keeps opening at login. Failures are logged, never fatal.
pub fn apply_at_start(app: &AppHandle, root: &Path) {
    let packaged = std::env::current_exe().is_ok_and(|exe| packaged(&exe));
    let choice = read_choice(root);
    let manager = app.autolaunch();
    let outcome = match at_start(choice.as_deref(), packaged) {
        AtStart::Leave => return,
        AtStart::Enable => {
            let done = manager.enable().map_err(|e| e.to_string());
            if done.is_ok() && choice.is_none() {
                let _ = write_choice(root, true);
            }
            done.map(|_| "enabled")
        }
        AtStart::Disable => match manager.is_enabled() {
            Ok(true) => manager
                .disable()
                .map(|_| "disabled")
                .map_err(|e| e.to_string()),
            Ok(false) => Ok("off"),
            Err(e) => Err(e.to_string()),
        },
    };
    let field = match &outcome {
        Ok(code) => switchboard_runtime::oplog::Field::Code(code),
        Err(_) => switchboard_runtime::oplog::Field::Code("failed"),
    };
    switchboard_runtime::oplog::event("login_item", &[("outcome", field)]);
}

/// `{available, enabled}` for the About panel. Not available in a development build.
pub fn status(app: &AppHandle) -> Value {
    let available = std::env::current_exe().is_ok_and(|exe| packaged(&exe));
    let enabled = available && app.autolaunch().is_enabled().unwrap_or(false);
    json!({ "available": available, "enabled": enabled })
}

/// The person's switch: saved first, then applied.
pub fn set(app: &AppHandle, root: &Path, enabled: bool) -> Result<Value, String> {
    if !std::env::current_exe().is_ok_and(|exe| packaged(&exe)) {
        return Err("Opening at login is available in the installed app only.".into());
    }
    write_choice(root, enabled)?;
    let manager = app.autolaunch();
    let applied = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    applied.map_err(|_| {
        if enabled {
            "Could not add Switchboard to the login items. Check the system's login item settings.".to_string()
        } else {
            "Could not remove Switchboard from the login items. Check the system's login item settings.".to_string()
        }
    })?;
    Ok(status(app))
}

/// Shows the window and brings the app forward (it may have started in the background or had
/// its window closed).
pub fn reveal(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Closing the window hides it; the owner keeps working. On macOS the Dock icon goes with it and
/// the menu-bar icon stays.
pub fn hide(window: &tauri::Window) {
    let _ = window.hide();
    #[cfg(target_os = "macos")]
    let _ = window
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// A second launch reveals the window unless it is itself a background start (the login item or
/// the lifecycle broker starting an app that already runs).
pub fn second_launch(app: &AppHandle, args: &[String]) {
    if !args.iter().any(|a| a == BACKGROUND_ARG) {
        reveal(app);
    }
}

/// The tray menu's language (L10N-01): the system's at start, then the window's choice.
static RUSSIAN: std::sync::OnceLock<std::sync::atomic::AtomicBool> = std::sync::OnceLock::new();
/// The version a ready update offers, kept so a language change rebuilds the same menu.
static READY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
fn russian() -> &'static std::sync::atomic::AtomicBool {
    RUSSIAN.get_or_init(|| {
        std::sync::atomic::AtomicBool::new(switchboard_core::language::system_prefers_russian())
    })
}
fn in_russian() -> bool {
    russian().load(std::sync::atomic::Ordering::Relaxed)
}
/// The tray's labels: Open, Quit.
fn labels(ru: bool) -> (&'static str, &'static str) {
    if ru {
        ("Открыть Switchboard", "Завершить Switchboard")
    } else {
        ("Open Switchboard", "Quit Switchboard")
    }
}

/// The tray menu: Open and Quit, and "Restart to update" once an update is ready (SB-55).
fn menu(app: &AppHandle, update: Option<&str>) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let (open_label, quit_label) = labels(in_russian());
    let open = MenuItemBuilder::with_id("open", open_label).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", quit_label).build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    match update {
        Some(version) => {
            let restart =
                MenuItemBuilder::with_id("update", restart_label_in(version, in_russian()))
                    .build(app)?;
            MenuBuilder::new(app)
                .items(&[&open, &restart, &separator, &quit])
                .build()
        }
        None => MenuBuilder::new(app)
            .items(&[&open, &separator, &quit])
            .build(),
    }
}

fn restart_label_in(version: &str, ru: bool) -> String {
    if ru {
        format!("Перезапустить для обновления до {version}")
    } else {
        format!("Restart to update to {version}")
    }
}

/// Adds "Restart to update" to the tray menu once an update is ready.
pub fn show_update(app: &AppHandle, version: &str) {
    if let Ok(mut ready) = READY.lock() {
        *ready = Some(version.to_owned());
    }
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("switchboard"), menu(app, Some(version))) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// The window's language choice reaches the tray (L10N-01): it rebuilds the menu, keeping a
/// ready update's item.
pub fn set_language(app: &AppHandle, ru: bool) {
    russian().store(ru, std::sync::atomic::Ordering::Relaxed);
    let ready = READY.lock().ok().and_then(|r| r.clone());
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("switchboard"), menu(app, ready.as_deref())) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// The menu-bar (macOS) or notification-area (Windows) icon: Open and Quit.
pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu(app, None)?;
    let builder = TrayIconBuilder::with_id("switchboard")
        .tooltip("Fabric Switchboard")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => reveal(app),
            // Through the exit event, which drains the owner (LC-01).
            "quit" => app.exit(0),
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if crate::updates::restart(app.clone()).await.is_err() {
                        // A refused password or a vanished update: the window says why.
                        reveal(&app);
                    }
                });
            }
            _ => {}
        });
    #[cfg(target_os = "macos")]
    let builder = builder
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray-template.png"
        ))?)
        .icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = match app.default_window_icon() {
        Some(icon) => builder.icon(icon.clone()),
        None => builder,
    }
    // Windows: a left click opens the window, the menu stays on the right button.
    .show_menu_on_left_click(false)
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            reveal(tray.app_handle());
        }
    });
    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_item_is_on_by_default_and_follows_the_saved_choice() {
        assert_eq!(at_start(None, true), AtStart::Enable);
        assert_eq!(at_start(Some("on\n"), true), AtStart::Enable);
        assert_eq!(at_start(Some("off\n"), true), AtStart::Disable);
        // An unreadable choice falls back to the default rather than failing the start.
        assert_eq!(at_start(Some("garbage"), true), AtStart::Enable);
    }

    #[test]
    fn a_development_build_never_registers_itself() {
        assert_eq!(at_start(None, false), AtStart::Leave);
        assert_eq!(at_start(Some("off"), false), AtStart::Leave);
        // Tests run a debug binary: never packaged.
        assert!(!packaged(Path::new(
            "/Applications/Fabric Switchboard.app/Contents/MacOS/fabric-switchboard"
        )));
    }

    #[test]
    fn the_tray_names_the_version_it_restarts_into() {
        assert_eq!(
            restart_label_in("0.7.0", false),
            "Restart to update to 0.7.0"
        );
        assert_eq!(
            restart_label_in("0.7.0", true),
            "Перезапустить для обновления до 0.7.0"
        );
        assert_eq!(
            labels(true),
            ("Открыть Switchboard", "Завершить Switchboard")
        );
        assert_eq!(labels(false), ("Open Switchboard", "Quit Switchboard"));
    }

    #[test]
    fn the_choice_is_saved_in_the_data_folder() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(read_choice(folder.path()), None);
        write_choice(folder.path(), false).unwrap();
        assert_eq!(
            at_start(read_choice(folder.path()).as_deref(), true),
            AtStart::Disable
        );
        write_choice(folder.path(), true).unwrap();
        assert_eq!(
            at_start(read_choice(folder.path()).as_deref(), true),
            AtStart::Enable
        );
    }
}
