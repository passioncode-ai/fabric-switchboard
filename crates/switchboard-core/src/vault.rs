use crate::{keychain::Consent, private_fs, Credential};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub trait Vault: Send + Sync {
    fn get(&self, id: &str) -> Result<Credential, String>;
    fn put(&self, id: &str, value: &Credential) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// In-memory dependency for synthetic tests; production must use NativeVault.
#[derive(Default)]
pub struct MemoryVault {
    values: Mutex<HashMap<String, Credential>>,
}
impl Vault for MemoryVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        self.values
            .lock()
            .map_err(|_| "Vault unavailable")?
            .get(id)
            .cloned()
            .ok_or_else(|| "Credential unavailable".into())
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| "Vault unavailable")?
            .insert(id.into(), value.clone());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| "Vault unavailable")?
            .remove(id);
        Ok(())
    }
}

/// The platform vault: Keychain on macOS (see `keychain.rs`), DPAPI on Windows, the Secret
/// Service on Linux (`secret_service.rs`).
pub struct NativeVault {
    #[cfg(target_os = "macos")]
    policy: crate::keychain::Policy<crate::keychain_macos::MacKeychain>,
}
impl Default for NativeVault {
    fn default() -> Self {
        Self::new()
    }
}
impl NativeVault {
    /// For the CLI, the MCP server and `serve`: never shows a Keychain dialog.
    pub fn new() -> Self {
        Self::with_consent(Consent::Never)
    }
    /// For the desktop app: may ask once per item saved by an earlier version while moving
    /// it to shared storage.
    pub fn desktop() -> Self {
        Self::with_consent(Consent::Desktop)
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    fn with_consent(consent: Consent) -> Self {
        #[cfg(target_os = "macos")]
        {
            let (build, backend) = crate::keychain_macos::MacKeychain::detect();
            Self {
                policy: crate::keychain::Policy::new(backend, build, consent),
            }
        }
        #[cfg(not(target_os = "macos"))]
        Self {}
    }
}

/// A vault error as the store may show it: actionable Keychain messages pass through,
/// anything else becomes `fallback`.
pub(crate) fn surface(fallback: &'static str) -> impl Fn(String) -> String {
    move |error| {
        if crate::keychain::ACTIONABLE.contains(&error.as_str()) {
            error
        } else {
            fallback.into()
        }
    }
}

#[cfg(target_os = "macos")]
impl Vault for NativeVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        let data = self.policy.get(id)?;
        if data.len() > 64 * 1024 {
            return Err("Stored credential is invalid".into());
        }
        serde_json::from_slice(&data).map_err(|_| "Stored credential is invalid".into())
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        let data = serde_json::to_vec(value).map_err(|_| "Credential serialization failed")?;
        if data.len() > 64 * 1024 {
            return Err("Stored credential is too large".into());
        }
        self.policy.put(id, &data)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        self.policy.delete(id)
    }
}
#[cfg(target_os = "linux")]
impl Vault for NativeVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        let data = crate::secret_service::read(crate::secret_service::SERVICE, id)?
            .ok_or("Credential unavailable")?;
        if data.len() > 64 * 1024 {
            return Err("Stored credential is invalid".into());
        }
        serde_json::from_slice(&data).map_err(|_| "Stored credential is invalid".into())
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        let data = serde_json::to_vec(value).map_err(|_| "Credential serialization failed")?;
        if data.len() > 64 * 1024 {
            return Err("Stored credential is too large".into());
        }
        crate::secret_service::write(crate::secret_service::SERVICE, id, &data)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        crate::secret_service::delete(crate::secret_service::SERVICE, id)
    }
}
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
impl Vault for NativeVault {
    fn get(&self, _: &str) -> Result<Credential, String> {
        Err("Native vault is not implemented on this platform".into())
    }
    fn put(&self, _: &str, _: &Credential) -> Result<(), String> {
        Err("Native vault is not implemented on this platform".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Err("Native vault is not implemented on this platform".into())
    }
}

#[cfg(windows)]
impl Vault for NativeVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        let path = crate::windows::vault_path(id)?;
        let encrypted = crate::private_fs::read_private(&path, 128 * 1024)?;
        let plain = crate::windows::crypt(&encrypted, false)?;
        if plain.len() > 64 * 1024 {
            return Err("Stored credential is invalid".into());
        }
        serde_json::from_slice(&plain).map_err(|_| "Stored credential is invalid".into())
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        let path = crate::windows::vault_path(id)?;
        let data = serde_json::to_vec(value).map_err(|_| "Credential serialization failed")?;
        if data.len() > 64 * 1024 {
            return Err("Stored credential is too large".into());
        }
        let encrypted = crate::windows::crypt(&data, true)?;
        crate::private_fs::private_write(&path, &encrypted)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        let path = crate::windows::vault_path(id)?;
        crate::private_fs::check_path(&path)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Native credential storage unavailable".into()),
        }
    }
}

/// SB-287: the data folder's choice of sign-in store, `<data>/vault.json`. Absent is the
/// platform's own store; `file` is [`FileVault`], for a machine with no graphical login.
pub const CHOICE_FILE: &str = "vault.json";
/// [`FileVault`]'s folder. Not `vault`: on Windows that folder holds DPAPI items.
pub const FILE_DIR: &str = "vault-files";
pub const CHOICE_UNREADABLE: &str = "The sign-in store's choice (vault.json) could not be read. Delete vault.json to use this computer's own store, or choose again with switchboard vault use.";
pub const NATIVE_WORKS: &str = "This computer's own sign-in store works here, so Switchboard keeps sign-ins in it. A file store is only for a machine with no graphical login.";
const ITEM_CAP: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultKind {
    Native,
    File,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Choice {
    kind: VaultKind,
    since: i64,
}

/// The store this data folder keeps its sign-ins in.
pub fn vault_kind(root: &Path) -> Result<VaultKind, String> {
    let path = root.join(CHOICE_FILE);
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(VaultKind::Native);
    }
    let bytes =
        private_fs::read_private_strict(&path, 4096).map_err(|_| CHOICE_UNREADABLE.to_string())?;
    serde_json::from_slice::<Choice>(&bytes)
        .map(|c| c.kind)
        .map_err(|_| CHOICE_UNREADABLE.into())
}

/// Records the choice; `native` removes the file, so a folder that never chose reads the same.
pub fn set_vault_kind(root: &Path, kind: VaultKind) -> Result<(), String> {
    let path = root.join(CHOICE_FILE);
    match kind {
        VaultKind::Native => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(CHOICE_UNREADABLE.into()),
        },
        VaultKind::File => {
            let choice = Choice {
                kind,
                since: crate::now(),
            };
            let bytes = serde_json::to_vec(&choice).map_err(|_| CHOICE_UNREADABLE)?;
            private_fs::private_write(&path, &bytes)
        }
    }
}

/// Every opener (`serve`, the app, an offline CLI or MCP call) builds its store here, so they
/// all read and write the same one. `desktop`: the app, which may ask once per item saved by an
/// earlier version (docs/KEYCHAIN.md).
pub fn open_vault(root: &Path, desktop: bool) -> Result<Arc<dyn Vault>, String> {
    Ok(match vault_kind(root)? {
        VaultKind::File => Arc::new(FileVault::new(root)),
        VaultKind::Native if desktop => Arc::new(NativeVault::desktop()),
        VaultKind::Native => Arc::new(NativeVault::new()),
    })
}

/// SB-287: sign-ins as 0600 files in `<data>/vault-files/` (folder 0700), for a machine with no
/// graphical login, where the platform's store is locked. It protects a sign-in as well as the
/// user account does, as Claude Code's own `.credentials.json` does where no keychain exists.
pub struct FileVault {
    dir: PathBuf,
}
impl FileVault {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(FILE_DIR),
        }
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        Ok(self.dir.join(id))
    }
    /// The items it holds, by id; files with any other name are not its own.
    pub fn ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| crate::uuid_valid(name))
            .collect();
        ids.sort();
        ids
    }
}
impl Vault for FileVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        let path = self.path(id)?;
        if std::fs::symlink_metadata(&path).is_err() {
            return Err("Credential unavailable".into());
        }
        let data = private_fs::read_private_strict(&path, ITEM_CAP)
            .map_err(|_| "Stored credential is invalid".to_string())?;
        serde_json::from_slice(&data).map_err(|_| "Stored credential is invalid".into())
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        let path = self.path(id)?;
        let data = serde_json::to_vec(value).map_err(|_| "Credential serialization failed")?;
        if data.len() as u64 > ITEM_CAP {
            return Err("Stored credential is too large".into());
        }
        private_fs::private_write(&path, &data)
            .map_err(|_| "Credential storage unavailable".to_string())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Credential storage unavailable".into()),
        }
    }
}

#[cfg(test)]
mod file_vault_tests {
    use super::*;

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
    fn a_file_vault_keeps_private_items_by_id_and_refuses_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileVault::new(dir.path());
        let id = uuid::Uuid::new_v4().to_string();
        assert!(vault.get(&id).is_err());
        vault.put(&id, &credential("synthetic")).unwrap();
        assert_eq!(vault.get(&id).unwrap().access_token, "synthetic");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir.path().join(FILE_DIR)), 0o700);
            assert_eq!(mode(&dir.path().join(FILE_DIR).join(&id)), 0o600);
        }
        assert_eq!(vault.ids(), vec![id.clone()]);
        assert!(vault.put("../escape", &credential("x")).is_err());
        assert!(vault.get("not-a-uuid").is_err());
        vault.delete(&id).unwrap();
        vault.delete(&id).unwrap();
        assert!(vault.get(&id).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_vault_does_not_follow_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let vault = FileVault::new(dir.path());
        let id = uuid::Uuid::new_v4().to_string();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::write(
            &elsewhere,
            serde_json::to_vec(&credential("planted")).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(FILE_DIR)).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.path().join(FILE_DIR).join(&id)).unwrap();
        assert!(vault.get(&id).is_err());
    }

    #[test]
    fn the_choice_is_the_data_folders_and_native_when_none_was_made() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(vault_kind(dir.path()).unwrap(), VaultKind::Native);
        set_vault_kind(dir.path(), VaultKind::File).unwrap();
        assert_eq!(vault_kind(dir.path()).unwrap(), VaultKind::File);
        set_vault_kind(dir.path(), VaultKind::Native).unwrap();
        assert!(!dir.path().join(CHOICE_FILE).exists());
        std::fs::write(dir.path().join(CHOICE_FILE), b"{").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.path().join(CHOICE_FILE),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        assert_eq!(vault_kind(dir.path()).unwrap_err(), CHOICE_UNREADABLE);
    }
}
