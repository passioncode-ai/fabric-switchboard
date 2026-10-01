use crate::{keychain::Consent, Credential};
use std::{collections::HashMap, sync::Mutex};

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

/// The platform vault: Keychain on macOS (see `keychain.rs`), DPAPI on Windows.
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
#[cfg(not(any(target_os = "macos", windows)))]
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
