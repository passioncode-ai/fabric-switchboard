use crate::Credential;
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

pub struct NativeVault;
impl Default for NativeVault {
    fn default() -> Self {
        Self::new()
    }
}
impl NativeVault {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "ai.passioncode.fabric-switchboard";

#[cfg(target_os = "macos")]
impl Vault for NativeVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        let data = security_framework::passwords::get_generic_password(SERVICE, id)
            .map_err(|_| "Native credential storage unavailable")?;
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
        security_framework::passwords::set_generic_password(SERVICE, id, &data)
            .map_err(|_| "Native credential storage unavailable".into())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        if !crate::uuid_valid(id) {
            return Err("Invalid credential identifier".into());
        }
        match security_framework::passwords::delete_generic_password(SERVICE, id) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()), // errSecItemNotFound: retry-safe removal
            Err(_) => Err("Native credential storage unavailable".into()),
        }
    }
}
#[cfg(not(target_os = "macos"))]
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
