//! Connecting coding agents to `switchboard mcp`: where the CLI is, the exact commands
//! that register it, and a user-level link so plugins can find it on PATH.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::launch::agent_cli;

#[cfg(windows)]
const CLI_NAME: &str = "switchboard.exe";
#[cfg(not(windows))]
const CLI_NAME: &str = "switchboard";

/// The CLI shipped inside the desktop bundle, beside its executable. None while macOS runs the
/// app from a temporary translocated copy: a path into it disappears when the app quits.
pub fn bundled_cli() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?.canonicalize().ok()?;
    let candidate = current.parent()?.join(CLI_NAME);
    (candidate.is_file() && candidate != current && !translocated(&candidate)).then_some(candidate)
}
/// macOS App Translocation: an app opened straight from a download runs from a read-only copy
/// under a random `/AppTranslocation/` folder that is gone after it quits.
pub fn translocated(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "AppTranslocation")
}
/// The running desktop app is a translocated copy.
pub fn running_translocated() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .is_some_and(|p| translocated(&p))
}
fn user_bin() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|home| PathBuf::from(home).join(".local").join("bin"))
}
#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[cfg(windows)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn setup() -> Value {
    let cli = agent_cli();
    let bundled = bundled_cli();
    let linked = user_bin()
        .map(|dir| dir.join(CLI_NAME))
        .filter(|p| p.exists());
    let command = cli
        .as_ref()
        .map(|p| shell_quote(&p.to_string_lossy()))
        .unwrap_or_else(|| "switchboard".into());
    json!({
        "cli_path": cli,
        "bundled_cli": bundled,
        "linked_cli": linked,
        "can_link": cfg!(unix) && bundled.is_some(),
        "translocated": running_translocated(),
        "commands": {
            "claude_code": format!("claude mcp add --scope user switchboard -- {command} mcp"),
            "codex": format!("codex mcp add switchboard -- {command} mcp"),
            "claude_plugin": "claude plugin marketplace add passioncode-ai/fabric-switchboard && claude plugin install switchboard@switchboard",
        },
    })
}

/// Links `<bin>/switchboard` to the bundled CLI. An existing file or a link to anything
/// other than a Switchboard CLI is left alone.
#[cfg(unix)]
pub fn link_cli(source: &Path, bin: &Path) -> Result<PathBuf, String> {
    let source = source
        .canonicalize()
        .map_err(|_| "The bundled command-line tool is unavailable.")?;
    if source.file_name().is_none_or(|n| n != CLI_NAME) {
        return Err("The bundled command-line tool is unavailable.".into());
    }
    std::fs::create_dir_all(bin).map_err(|_| "Could not create ~/.local/bin.")?;
    let link = bin.join(CLI_NAME);
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = std::fs::read_link(&link).map_err(|_| "Existing link unreadable.")?;
            let resolved = link.parent().unwrap_or(bin).join(&target);
            if resolved.canonicalize().ok().as_deref() == Some(source.as_path()) {
                return Ok(link);
            }
            if target.file_name().is_none_or(|n| n != CLI_NAME) {
                return Err(
                    "~/.local/bin/switchboard already points elsewhere. Remove it first.".into(),
                );
            }
            std::fs::remove_file(&link).map_err(|_| "Could not replace the old link.")?;
        }
        Ok(_) => {
            return Err(
                "~/.local/bin/switchboard already exists and is not a link. Remove it first."
                    .into(),
            )
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Could not inspect ~/.local/bin.".into()),
    }
    std::os::unix::fs::symlink(&source, &link).map_err(|_| "Could not create the link.")?;
    Ok(link)
}
#[cfg(not(unix))]
pub fn link_cli(_: &Path, _: &Path) -> Result<PathBuf, String> {
    Err("Keep switchboard.exe from the download folder, or add it to PATH.".into())
}
pub fn link_bundled_cli() -> Result<Value, String> {
    let source = bundled_cli().ok_or("This build has no bundled command-line tool.")?;
    let bin = user_bin().ok_or("Home folder unavailable.")?;
    let link = link_cli(&source, &bin)?;
    Ok(json!({"linked_cli": link}))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn link_is_created_reused_and_never_replaces_foreign_files() {
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("App.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        let cli = app.join(CLI_NAME);
        std::fs::write(&cli, b"#!/bin/sh\n").unwrap();
        let bin = temp.path().join("bin");
        let link = link_cli(&cli, &bin).unwrap();
        assert_eq!(link.canonicalize().unwrap(), cli.canonicalize().unwrap());
        assert_eq!(link_cli(&cli, &bin).unwrap(), link);

        let newer = temp.path().join("New.app/Contents/MacOS");
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(newer.join(CLI_NAME), b"#!/bin/sh\n").unwrap();
        link_cli(&newer.join(CLI_NAME), &bin).unwrap();
        assert_eq!(
            link.canonicalize().unwrap(),
            newer.join(CLI_NAME).canonicalize().unwrap()
        );

        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(temp.path().join("other-tool"), &link).unwrap();
        assert!(link_cli(&cli, &bin).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::write(&link, b"user file").unwrap();
        assert!(link_cli(&cli, &bin).is_err());
        assert_eq!(std::fs::read(&link).unwrap(), b"user file");
        assert!(link_cli(&temp.path().join("missing"), &bin).is_err());
    }
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    #[test]
    fn a_hostile_cli_path_stays_one_shell_word() {
        assert_eq!(shell_quote("/plain/switchboard"), "'/plain/switchboard'");
        // POSIX shells close the quote around an escaped one; PowerShell doubles it.
        let expected = if cfg!(windows) {
            "'/it''s; rm -rf ~/switchboard'"
        } else {
            r#"'/it'\''s; rm -rf ~/switchboard'"#
        };
        assert_eq!(shell_quote("/it's; rm -rf ~/switchboard"), expected);
    }
    #[test]
    fn a_translocated_copy_is_never_linked_or_offered() {
        assert!(translocated(Path::new(
            "/private/var/folders/x/T/AppTranslocation/0A1B/d/Fabric Switchboard.app/Contents/MacOS/switchboard"
        )));
        assert!(!translocated(Path::new(
            "/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard"
        )));
        assert!(!translocated(Path::new(
            "/Users/x/AppTranslocationNotes/switchboard"
        )));
        assert_eq!(
            setup()["translocated"],
            false,
            "a test binary is not translocated"
        );
    }
    #[test]
    fn setup_offers_commands_for_the_found_cli_and_never_a_credential() {
        // Read-only: it looks for the CLI and a link, and writes nothing.
        let setup = setup();
        for key in [
            "cli_path",
            "bundled_cli",
            "linked_cli",
            "can_link",
            "commands",
        ] {
            assert!(setup.get(key).is_some(), "{key}");
        }
        let claude = setup["commands"]["claude_code"].as_str().unwrap();
        assert!(claude.starts_with("claude mcp add --scope user switchboard -- "));
        assert!(claude.ends_with(" mcp"));
        match setup["cli_path"].as_str() {
            Some(path) => assert!(claude.contains(&shell_quote(path))),
            None => assert!(claude.contains("-- switchboard mcp")),
        }
        // A test binary is never a bundled desktop CLI, so nothing can be linked.
        assert_eq!(setup["can_link"], false);
    }
}
