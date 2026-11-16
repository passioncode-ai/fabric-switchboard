//! Windows native boundaries. No plaintext vault fallback or machine-wide DPAPI scope. The
//! private storage and process identity half lives in fabric-sessions-core (SB-397 D-2) and is
//! re-exported here unchanged; the credential vault (DPAPI) stays Switchboard's.
pub use fabric_sessions_core::windows::*;
use std::{
    ffi::c_void,
    mem::zeroed,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::*, Security::Cryptography::*};
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
pub(crate) fn vault_path(id: &str) -> Result<PathBuf, String> {
    if !crate::uuid_valid(id) {
        return Err("Invalid credential identifier".into());
    }
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?)
        .join("ai.passioncode.fabric-switchboard")
        .join("vault");
    private_dir(&root)?;
    Ok(root.join(format!("{id}.dpapi")))
}
pub(crate) fn crypt(data: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    if data.len() > 128 * 1024 {
        return Err("Stored credential is too large".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = unsafe { zeroed::<CRYPT_INTEGER_BLOB>() };
    let okay = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if okay == 0 {
        return Err("Native credential storage unavailable".into());
    }
    let _allocation = Local(output.pbData.cast());
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    // Clear native decrypted allocation before LocalFree; Rust credential buffers retain normal lifetime.
    if !encrypt {
        for i in 0..output.cbData as usize {
            unsafe {
                std::ptr::write_volatile(output.pbData.add(i), 0);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{private_fs, AuthKind, Credential, NativeVault, Provider, Vault};
    use std::fs;
    #[test]
    fn windows_dpapi_roundtrip_large_oauth_and_tamper_refusal() {
        let id = uuid::Uuid::new_v4().to_string();
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = NativeVault::new().delete(&self.0);
            }
        }
        let _cleanup = Cleanup(id.clone());
        let vault = NativeVault::new();
        let material=serde_json::json!({"tokens":{"access_token":"a".repeat(16384),"refresh_token":"r".repeat(16384),"id_token":"i".repeat(16384),"account_id":"synthetic"}}).to_string();
        let credential = Credential::parse(Provider::Codex, AuthKind::OAuth, &material).unwrap();
        vault.put(&id, &credential).unwrap();
        assert_eq!(
            vault.get(&id).unwrap().access_token,
            credential.access_token
        );
        let path = vault_path(&id).unwrap();
        let encrypted = fs::read(&path).unwrap();
        assert!(!encrypted
            .windows(128)
            .any(|w| w == &credential.access_token.as_bytes()[..128]));
        private_fs::private_write(&path, b"corrupt synthetic ciphertext").unwrap();
        assert!(vault.get(&id).is_err());
        vault.delete(&id).unwrap();
        vault.delete(&id).unwrap();
    }
}
