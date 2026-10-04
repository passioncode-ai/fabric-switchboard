//! `switchboard uninstall` (SB-29, lifecycle audit F8/F9): removes what Switchboard itself
//! created on this machine, and nothing else. Without `apply` it only reports the plan.
//!
//! Removed: every saved account's credential (the vault's own items — on macOS the shared and the
//! pre-0.4.1 Keychain items, on Windows the DPAPI files), the 0.5.0 `vault-key` Keychain item
//! nothing reads any more (F8), the `~/.local/bin/switchboard` link when it points to a
//! Switchboard CLI, and — unless `keep_data` — the entries Switchboard writes in its data folder.
//! Never touched: the ordinary Claude Code and Codex sign-ins, encrypted backups and their key (a
//! reinstall restores them), files in the data folder Switchboard did not create, and the app
//! bundle itself (moved to the Trash by the person).
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use switchboard_core::{Store, Vault};

/// Entries Switchboard creates in its data folder. Anything else is left in place.
const OWNED: &[&str] = &[
    "accounts.json",
    "instance.lock",
    "control.json",
    "proxy.json",
    "renewal-state.json",
    "usage-holds.json",
    "limit-evidence.json",
    "backup-id",
    "homes",
    "runtimes",
    "logins",
    "vault",
];
/// The 0.5.0 sealed-vault key: Keychain service and account. 0.5.1 and later never read it.
pub(crate) const ORPHAN_KEY: (&str, &str) = ("ai.passioncode.fabric-switchboard.vault-key", "v1");
const CLI_NAME: &str = if cfg!(windows) {
    "switchboard.exe"
} else {
    "switchboard"
};

/// Where uninstall looks besides the data folder; tests pass their own.
pub struct Places {
    /// The folder holding the `switchboard` link (`~/.local/bin`), if any.
    pub bin: Option<PathBuf>,
    /// Removes the orphaned 0.5.0 key item; absent is success.
    pub remove_orphan_key: fn() -> Result<(), String>,
}

/// The real places: `~/.local/bin` and, on macOS, the login Keychain.
pub fn places() -> Places {
    Places {
        bin: dirs::home_dir().map(|h| h.join(".local/bin")),
        remove_orphan_key,
    }
}
#[cfg(target_os = "macos")]
fn remove_orphan_key() -> Result<(), String> {
    switchboard_core::security_cli::delete(ORPHAN_KEY.0, ORPHAN_KEY.1)
}
#[cfg(not(target_os = "macos"))]
fn remove_orphan_key() -> Result<(), String> {
    Ok(())
}

/// The link to remove: `<bin>/switchboard` when it is a symlink to a file named like the CLI.
/// A regular file or a link to anything else belongs to the person.
fn cli_link(bin: &Path) -> Option<PathBuf> {
    let link = bin.join(CLI_NAME);
    let meta = std::fs::symlink_metadata(&link).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(&link).ok()?;
    (target.file_name()? == CLI_NAME).then_some(link)
}

/// Plans and — with `apply` — performs the removal. The store is opened exclusively, so this
/// refuses while the desktop app, `switchboard serve` or another command holds it.
pub fn uninstall(
    root: &Path,
    vault: Arc<dyn Vault>,
    keep_data: bool,
    apply: bool,
    places: &Places,
) -> Result<Value, String> {
    let store = Store::open(root.to_owned(), vault.clone())?;
    let accounts: Vec<String> = store
        .snapshot()?
        .accounts
        .into_iter()
        .map(|a| a.id)
        .collect();
    let link = places.bin.as_deref().and_then(cli_link);
    let entries: Vec<String> = std::fs::read_dir(root)
        .map_err(|_| "Private account storage unavailable".to_string())?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let (owned, foreign): (Vec<String>, Vec<String>) = entries
        .into_iter()
        .partition(|name| OWNED.contains(&name.as_str()) || name.starts_with(".private-"));
    let mut plan = json!({
        "data_folder": root,
        "accounts": accounts.len(),
        "cli_link": link,
        "orphan_keychain_item": cfg!(target_os = "macos").then_some(ORPHAN_KEY.0),
        "data_entries": if keep_data { json!([]) } else { json!(owned) },
        "kept": {
            "foreign_entries": foreign,
            "backups": "kept, with their key: a reinstall restores them",
            "provider_sign_ins": "the ordinary Claude Code and Codex sign-ins are not touched",
        },
        "applied": apply,
    });
    if !apply {
        return Ok(plan);
    }
    let mut failures = Vec::new();
    for id in &accounts {
        if let Err(error) = vault.delete(id) {
            failures.push(json!({"account_id": id, "error": error}));
        }
    }
    if let Err(error) = (places.remove_orphan_key)() {
        failures.push(json!({"item": ORPHAN_KEY.0, "error": error}));
    }
    if let Some(link) = &link {
        if std::fs::remove_file(link).is_err() {
            failures.push(json!({"path": link, "error": "Could not remove the link."}));
        }
    }
    // The store's lock file goes last: release it first.
    drop(store);
    if !keep_data {
        for name in &owned {
            let path = root.join(name);
            let removed = match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&path),
                Ok(_) => std::fs::remove_file(&path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
            if removed.is_err() {
                failures.push(json!({"path": path, "error": "Could not remove."}));
            }
        }
        // An empty folder goes too; one holding the person's files stays.
        let _ = std::fs::remove_dir(root);
    }
    plan["failures"] = json!(failures);
    plan["data_folder_removed"] = json!(!root.exists());
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{credential, identity};
    use switchboard_core::{AuthKind, MemoryVault, Provider};

    fn removed_ok() -> Result<(), String> {
        Ok(())
    }

    fn setup() -> (tempfile::TempDir, PathBuf, Arc<MemoryVault>) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let vault = Arc::new(MemoryVault::default());
        {
            let store = Store::open(root.clone(), vault.clone()).unwrap();
            store
                .upsert(
                    "synthetic-a".into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    credential("synthetic-a"),
                    Some(identity("synthetic-a")),
                )
                .unwrap();
        }
        std::fs::create_dir_all(root.join("homes/x")).unwrap();
        std::fs::write(root.join("usage-holds.json"), "{}").unwrap();
        (temp, root, vault)
    }

    #[test]
    fn without_apply_nothing_changes() {
        let (temp, root, vault) = setup();
        let places = Places {
            bin: Some(temp.path().join("bin")),
            remove_orphan_key: removed_ok,
        };
        let plan = uninstall(&root, vault.clone(), false, false, &places).unwrap();
        assert_eq!(plan["accounts"], 1);
        assert_eq!(plan["applied"], false);
        assert!(root.join("accounts.json").exists());
        let id = Store::open(root.clone(), vault.clone())
            .unwrap()
            .snapshot()
            .unwrap()
            .accounts[0]
            .id
            .clone();
        assert!(vault.get(&id).is_ok(), "credential untouched");
    }

    #[cfg(unix)]
    #[test]
    fn apply_removes_owned_things_and_keeps_the_persons_files() {
        let (temp, root, vault) = setup();
        let id = Store::open(root.clone(), vault.clone())
            .unwrap()
            .snapshot()
            .unwrap()
            .accounts[0]
            .id
            .clone();
        std::fs::write(root.join("my-notes.txt"), "the person's file").unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let app_cli = temp.path().join("App.app/Contents/MacOS/switchboard");
        std::fs::create_dir_all(app_cli.parent().unwrap()).unwrap();
        std::fs::write(&app_cli, "#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&app_cli, bin.join("switchboard")).unwrap();
        let places = Places {
            bin: Some(bin.clone()),
            remove_orphan_key: removed_ok,
        };
        let done = uninstall(&root, vault.clone(), false, true, &places).unwrap();
        assert_eq!(done["failures"], json!([]));
        assert!(vault.get(&id).is_err(), "the account's credential is gone");
        assert!(
            !bin.join("switchboard").exists()
                && std::fs::symlink_metadata(bin.join("switchboard")).is_err()
        );
        assert!(!root.join("accounts.json").exists() && !root.join("homes").exists());
        assert!(
            root.join("my-notes.txt").exists(),
            "a file Switchboard did not create stays"
        );
        assert_eq!(done["data_folder_removed"], false);
        assert_eq!(done["kept"]["foreign_entries"], json!(["my-notes.txt"]));
    }

    #[cfg(unix)]
    #[test]
    fn keep_data_keeps_the_folder_and_a_foreign_link_is_left_alone() {
        let (temp, root, vault) = setup();
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink("/usr/bin/true", bin.join("switchboard")).unwrap();
        let places = Places {
            bin: Some(bin.clone()),
            remove_orphan_key: removed_ok,
        };
        let done = uninstall(&root, vault, true, true, &places).unwrap();
        assert!(root.join("accounts.json").exists());
        assert!(done["cli_link"].is_null());
        assert!(
            std::fs::symlink_metadata(bin.join("switchboard")).is_ok(),
            "a link to another program stays"
        );
    }

    #[test]
    fn an_empty_folder_is_removed_and_a_running_owner_refuses() {
        let (_temp, root, vault) = setup();
        let held = Store::open(root.clone(), vault.clone()).unwrap();
        let places = Places {
            bin: None,
            remove_orphan_key: removed_ok,
        };
        assert!(
            uninstall(&root, vault.clone(), false, true, &places).is_err(),
            "store in use"
        );
        drop(held);
        let done = uninstall(&root, vault, false, true, &places).unwrap();
        assert_eq!(done["data_folder_removed"], true);
    }
}
