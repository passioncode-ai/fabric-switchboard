//! SB-287: choosing where a data folder keeps its sign-ins (`switchboard vault use`).
//!
//! A machine with no graphical login has no unlocked platform store, so its `serve` keeps
//! sign-ins in files ([`FileVault`]). The choice is the data folder's (`vault.json`), read by
//! every opener (docs/research/headless-vault-2026-11-07.md).

use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use switchboard_core::{Credential, FileVault, NativeVault, Store, Vault, VaultKind};

/// Whether this computer's own store takes a write now: a throwaway item, put and deleted.
pub fn native_works() -> bool {
    let vault = NativeVault::new();
    let id = uuid::Uuid::new_v4().to_string();
    let probe = Credential {
        access_token: "switchboard-store-probe".into(),
        refresh_token: None,
        id_token: None,
        native_context: None,
        expires_at: None,
        account_id: None,
    };
    let stored = vault.put(&id, &probe).is_ok();
    if stored {
        let _ = vault.delete(&id);
    }
    stored
}

/// Moves the folder's sign-ins into `kind`'s store and records the choice. A file store is
/// refused while the platform's store works, unless `move_from_native` (SB-301): a Mac Studio
/// space opens its login keychain only in a session of its own, so the move runs there, where the
/// platform's store answers, and the daemon then reads the files. With `move_from_native` and a
/// folder already on files, it moves what the platform's store still holds. It opens the store, so it is refused while
/// Switchboard runs (`STORE_LOCKED`), and holding the store's lock keeps an owner from
/// starting while items move. An item that cannot be read or written stays where it was and
/// is named in `not_moved`.
pub fn choose(
    root: &Path,
    kind: VaultKind,
    native: Arc<dyn Vault>,
    native_works: impl Fn() -> bool,
    move_from_native: bool,
) -> Result<Value, String> {
    let current = switchboard_core::vault_kind(root)?;
    if kind == VaultKind::File && current != VaultKind::File && !move_from_native && native_works()
    {
        return Err(switchboard_core::VAULT_NATIVE_WORKS.into());
    }
    let file: Arc<dyn Vault> = Arc::new(FileVault::new(root));
    // The direction is the chosen store's: into files from the platform's, or back.
    let (from, to) = match kind {
        VaultKind::File => (native, file.clone()),
        VaultKind::Native => (file.clone(), native),
    };
    let store = Store::open(root.to_path_buf(), from.clone())?;
    let mut moved = 0usize;
    let mut not_moved = Vec::new();
    if current != kind || (kind == VaultKind::File && move_from_native) {
        let snapshot = store.snapshot()?;
        let mut ids: Vec<String> = snapshot
            .accounts
            .iter()
            .map(|a| a.id.clone())
            .chain(snapshot.agent_keys.iter().map(|k| k.id.clone()))
            .chain(crate::settings_signing::vault_item(root))
            .chain(crate::vesta::vault_item(root))
            .chain(crate::inbox::vault_item(root))
            .collect();
        if kind == VaultKind::Native {
            ids.extend(FileVault::new(root).ids());
        }
        ids.sort();
        ids.dedup();
        for id in &ids {
            match from.get(id).and_then(|c| to.put(id, &c)) {
                Ok(()) => {
                    let _ = from.delete(id);
                    moved += 1;
                }
                Err(_) => not_moved.push(id.clone()),
            }
        }
        switchboard_core::set_vault_kind(root, kind)?;
    }
    drop(store);
    Ok(json!({
        "kind": kind,
        "changed": current != kind,
        "moved": moved,
        "not_moved": not_moved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::MemoryVault;

    fn credential(token: &str) -> Credential {
        Credential {
            access_token: token.into(),
            refresh_token: None,
            id_token: None,
            native_context: None,
            expires_at: None,
            account_id: None,
        }
    }

    #[test]
    fn a_file_store_is_refused_while_the_platform_store_works() {
        let dir = tempfile::tempdir().unwrap();
        let native: Arc<dyn Vault> = Arc::new(MemoryVault::default());
        assert_eq!(
            choose(dir.path(), VaultKind::File, native, || true, false).unwrap_err(),
            switchboard_core::VAULT_NATIVE_WORKS
        );
        assert_eq!(
            switchboard_core::vault_kind(dir.path()).unwrap(),
            VaultKind::Native
        );
    }

    #[test]
    fn choosing_moves_every_item_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let native = Arc::new(MemoryVault::default());
        let as_vault: Arc<dyn Vault> = native.clone();
        let item = Store::open(dir.path().to_path_buf(), as_vault.clone())
            .unwrap()
            .put_secret(None, "synthetic-secret")
            .unwrap();
        // A named item: an account's id whose credential the platform store holds.
        let stray = uuid::Uuid::new_v4().to_string();
        native.put(&stray, &credential("stray")).unwrap();

        let answer = choose(
            dir.path(),
            VaultKind::File,
            as_vault.clone(),
            || false,
            false,
        )
        .unwrap();
        assert_eq!(answer["changed"], true);
        assert_eq!(
            switchboard_core::vault_kind(dir.path()).unwrap(),
            VaultKind::File
        );
        // The secret item is not named by any file, so it stays; nothing named moves yet.
        assert_eq!(answer["moved"], 0);
        assert!(native.get(&item).is_ok());

        let file = FileVault::new(dir.path());
        file.put(&stray, &credential("in-file")).unwrap();
        let back = choose(
            dir.path(),
            VaultKind::Native,
            as_vault.clone(),
            || true,
            false,
        )
        .unwrap();
        assert_eq!(back["moved"], 1);
        assert_eq!(native.get(&stray).unwrap().access_token, "in-file");
        assert!(file.ids().is_empty());
        assert_eq!(
            switchboard_core::vault_kind(dir.path()).unwrap(),
            VaultKind::Native
        );
        let again = choose(dir.path(), VaultKind::Native, as_vault, || true, false).unwrap();
        assert_eq!(again["changed"], false);
    }

    /// SB-301: in a session where the platform's store opens, the operator moves its sign-ins
    /// into files on purpose; a second run picks up what the first could not read.
    #[test]
    fn a_working_platform_store_moves_into_files_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let native = Arc::new(MemoryVault::default());
        let as_vault: Arc<dyn Vault> = native.clone();
        let id = Store::open(dir.path().to_path_buf(), as_vault.clone())
            .unwrap()
            .add(
                "Space".into(),
                switchboard_core::Provider::Claude,
                switchboard_core::AuthKind::OAuth,
                "default".into(),
                credential("in-keychain"),
            )
            .unwrap()
            .id;
        let answer = choose(dir.path(), VaultKind::File, as_vault.clone(), || true, true).unwrap();
        assert_eq!(answer["moved"], 1);
        assert_eq!(
            FileVault::new(dir.path()).get(&id).unwrap().access_token,
            "in-keychain"
        );
        assert!(native.get(&id).is_err());
        // Already on files: a later run moves what the platform's store still holds.
        native.put(&id, &credential("late")).unwrap();
        let again = choose(dir.path(), VaultKind::File, as_vault, || true, true).unwrap();
        assert_eq!(again["changed"], false);
        assert_eq!(again["moved"], 1);
        assert_eq!(
            FileVault::new(dir.path()).get(&id).unwrap().access_token,
            "late"
        );
    }

    #[test]
    fn an_offline_call_in_a_file_store_folder_reads_the_file_store() {
        let dir = tempfile::tempdir().unwrap();
        switchboard_core::set_vault_kind(dir.path(), VaultKind::File).unwrap();
        let opened = switchboard_core::open_vault(dir.path(), false).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        opened.put(&id, &credential("synthetic")).unwrap();
        assert_eq!(
            FileVault::new(dir.path()).get(&id).unwrap().access_token,
            "synthetic"
        );
    }

    #[test]
    fn choosing_is_refused_while_switchboard_holds_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let native: Arc<dyn Vault> = Arc::new(MemoryVault::default());
        let _owner = Store::open(dir.path().to_path_buf(), native.clone()).unwrap();
        assert!(choose(dir.path(), VaultKind::File, native, || false, false).is_err());
        assert_eq!(
            switchboard_core::vault_kind(dir.path()).unwrap(),
            VaultKind::Native
        );
    }
}
