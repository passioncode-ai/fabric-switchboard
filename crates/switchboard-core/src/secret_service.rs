//! Linux credential storage (SB-88): the freedesktop Secret Service — GNOME Keyring, KWallet or
//! KeePassXC — over D-Bus, the store desktop Linux keeps passwords in. The counterpart of the
//! Keychain on macOS and DPAPI on Windows: every account's credential is one item, service
//! `SERVICE`, account the account's UUID; the backup key is the item `BACKUP_SERVICE` / `v1`.
//!
//! There is no plaintext fallback, as on the other platforms: without a running Secret Service
//! (a headless session, no keyring daemon) storage refuses with a message that names the fix.
use keyring::{Entry, Error};

/// The same service names the macOS Keychain uses, so a person meets one vocabulary.
pub(crate) const SERVICE: &str = "ai.passioncode.fabric-switchboard.shared";
pub(crate) const BACKUP_SERVICE: &str = "ai.passioncode.fabric-switchboard.backup-key";
/// Shown verbatim (src/adapter.ts): the one fix a person can make.
pub const UNAVAILABLE: &str = "Linux credential storage is unavailable. Start a Secret Service keyring (GNOME Keyring, KWallet or KeePassXC) and unlock it, then retry.";
pub const LOCKED: &str = "The Linux keyring is locked. Unlock it, then retry.";

fn entry(service: &str, account: &str) -> Result<Entry, String> {
    Entry::new(service, account).map_err(|_| UNAVAILABLE.to_string())
}
/// Maps a keyring failure to what a person can do; never the platform's text.
fn refusal(error: Error) -> String {
    match error {
        Error::NoStorageAccess(_) => LOCKED.into(),
        _ => UNAVAILABLE.into(),
    }
}
/// The stored bytes, `None` when there is no such item.
pub(crate) fn read(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    match entry(service, account)?.get_secret() {
        Ok(bytes) => Ok(Some(bytes)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(refusal(error)),
    }
}
pub(crate) fn write(service: &str, account: &str, bytes: &[u8]) -> Result<(), String> {
    entry(service, account)?.set_secret(bytes).map_err(refusal)
}
/// Removing an item that is not there is success.
pub(crate) fn delete(service: &str, account: &str) -> Result<(), String> {
    match entry(service, account)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(refusal(error)),
    }
}
