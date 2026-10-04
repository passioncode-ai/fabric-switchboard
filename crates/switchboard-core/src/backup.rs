//! Automatic encrypted backups of saved accounts (0.5). A backup is one JSON file: a readable
//! header (format, version, time, account count) and the accounts, policies and credentials
//! sealed with AES-256-GCM under a 256-bit key kept on this machine — the login Keychain
//! through `/usr/bin/security` on macOS, a DPAPI-protected file on Windows. Operator decision:
//! no passphrase, so a backup restores after reinstalling Switchboard, not on another machine
//! or after the Keychain is lost (docs/PLAN-0.5.md, D-5).
use crate::{private_fs, Account, Credential, RotationPolicy, Store};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const FORMAT: &str = "fabric-switchboard-backup";
const VERSION: u32 = 1;
const PREFIX: &str = "switchboard-backup-";
pub const KEEP: usize = 10;
const MAX_FILE: u64 = 32 * 1024 * 1024;
const UNAVAILABLE: &str =
    "Backup storage unavailable. Check the backup folder and Keychain access.";
pub const FOREIGN: &str =
    "This backup was made with another key and cannot be opened on this machine.";

/// Where the backup key lives. `create` must not replace an existing key: it reports `false`
/// when one already exists, and the caller reads that one.
pub trait BackupKey: Send + Sync {
    fn load(&self) -> Result<Option<[u8; 32]>, String>;
    fn create(&self, key: &[u8; 32]) -> Result<bool, String>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Info {
    pub file: String,
    pub created_at: i64,
    pub accounts: usize,
    /// Accounts whose credential could not be read when this backup was made.
    #[serde(default)]
    pub missing: usize,
    /// Written by this store (the same data folder); only these are ever pruned.
    #[serde(default)]
    pub own: bool,
    /// Sealed under this machine's key: Restore can open it.
    #[serde(default)]
    pub openable: bool,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Restored {
    pub added: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    created_at: i64,
    accounts: usize,
    missing: usize,
    /// The writing store's id: pruning is per store, never across stores or machines.
    store: String,
    /// First 16 hex digits of SHA-256 of the key: a cheap "can this machine open it".
    key: String,
    nonce: String,
    data: String,
}
/// The sealed part. Never Debug: it carries credentials.
#[derive(Serialize, Deserialize)]
struct Payload {
    accounts: Vec<Account>,
    policies: Vec<RotationPolicy>,
    credentials: BTreeMap<String, Credential>,
}

fn key(keys: &dyn BackupKey, create: bool) -> Result<(LessSafeKey, String), String> {
    let bytes = match keys.load()? {
        Some(bytes) => bytes,
        None if !create => return Err(FOREIGN.into()),
        None => {
            let mut fresh = [0u8; 32];
            SystemRandom::new()
                .fill(&mut fresh)
                .map_err(|_| UNAVAILABLE)?;
            if keys.create(&fresh)? {
                fresh
            } else {
                keys.load()?.ok_or(UNAVAILABLE)?
            }
        }
    };
    Ok((
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &bytes).map_err(|_| UNAVAILABLE)?),
        key_id(&bytes),
    ))
}
fn key_id(bytes: &[u8; 32]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn aad(created_at: i64, store: &str, accounts: usize, missing: usize) -> Vec<u8> {
    format!("{FORMAT}:v{VERSION}:{created_at}:{store}:{accounts}:{missing}").into_bytes()
}
fn file_name(created_at: i64) -> String {
    format!("{PREFIX}{created_at}.json")
}
fn valid_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(".json"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Writes one backup of every account whose credential can be read, then keeps the newest
/// `KEEP`. A store with no accounts writes nothing and returns None.
pub fn write(
    store: &Store,
    dir: &Path,
    keys: &dyn BackupKey,
    now: i64,
) -> Result<Option<Info>, String> {
    let (snapshot, credentials) = store.export()?;
    if credentials.is_empty() {
        return Ok(None);
    }
    let missing = snapshot.accounts.len() - credentials.len();
    let store_id = store.backup_id()?;
    let accounts: Vec<Account> = snapshot
        .accounts
        .into_iter()
        .filter(|a| credentials.contains_key(&a.id))
        .collect();
    let count = accounts.len();
    let payload = serde_json::to_vec(&Payload {
        accounts,
        policies: snapshot.policies,
        credentials,
    })
    .map_err(|_| UNAVAILABLE)?;
    let (key, key_id) = key(keys, true)?;
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| UNAVAILABLE)?;
    let mut sealed = payload;
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad(now, &store_id, count, missing)),
        &mut sealed,
    )
    .map_err(|_| UNAVAILABLE)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        created_at: now,
        accounts: count,
        missing,
        store: store_id.clone(),
        key: key_id.clone(),
        nonce: STANDARD.encode(nonce),
        data: STANDARD.encode(&sealed),
    };
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| UNAVAILABLE)?;
    private_fs::private_write(&dir.join(file_name(now)), &bytes)?;
    prune(dir, &store_id, KEEP);
    Ok(Some(Info {
        file: file_name(now),
        created_at: now,
        accounts: count,
        missing,
        own: true,
        openable: true,
    }))
}

/// Backups in `dir`, newest first, marked as this store's and as openable with this
/// machine's key; unreadable files are left out, never deleted. A missing key is not created.
pub fn list(dir: &Path, store: &Store, keys: &dyn BackupKey) -> Vec<Info> {
    let own = store.backup_id().ok();
    let key = keys.load().ok().flatten().map(|k| key_id(&k));
    envelopes(dir)
        .into_iter()
        .map(|(name, e)| Info {
            file: name,
            created_at: e.created_at,
            accounts: e.accounts,
            missing: e.missing,
            own: own.as_deref() == Some(e.store.as_str()),
            openable: key.as_deref() == Some(e.key.as_str()),
        })
        .collect()
}
fn envelopes(dir: &Path) -> Vec<(String, Envelope)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut found: Vec<(String, Envelope)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if !valid_name(&name) {
                return None;
            }
            let bytes = private_fs::read_private(&entry.path(), MAX_FILE).ok()?;
            let envelope: Envelope = serde_json::from_slice(&bytes).ok()?;
            (envelope.format == FORMAT && envelope.version == VERSION).then_some((name, envelope))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.1.created_at));
    found
}

/// Keeps this store's newest `keep`, and always its most complete one: a run of partial
/// backups (an account unreadable for a while) never pushes out the last complete copy.
/// Other stores' and other machines' files are never touched.
fn prune(dir: &Path, store: &str, keep: usize) {
    let own: Vec<(String, Envelope)> = envelopes(dir)
        .into_iter()
        .filter(|(_, e)| e.store == store)
        .collect();
    let best = own
        .iter()
        .max_by(|a, b| {
            a.1.accounts
                .cmp(&b.1.accounts)
                .then(b.1.missing.cmp(&a.1.missing))
                .then(a.1.created_at.cmp(&b.1.created_at))
        })
        .map(|(name, _)| name.clone());
    for (name, _) in own.into_iter().skip(keep) {
        if Some(&name) != best.as_ref() {
            let _ = std::fs::remove_file(dir.join(name));
        }
    }
}

/// Adds the accounts of a backup that this store does not already hold. An account present
/// here — same id, or the same identity in the same provider and pool — is skipped, so a
/// restore never replaces a newer sign-in with an older copy of it.
pub fn restore(
    store: &Store,
    dir: &Path,
    file: &str,
    keys: &dyn BackupKey,
) -> Result<Restored, String> {
    if !valid_name(file) {
        return Err("Choose a backup from the backup folder.".into());
    }
    let bytes = private_fs::read_private(&dir.join(file), MAX_FILE)
        .map_err(|_| "Backup not found. Refresh the list and choose another.")?;
    let envelope: Envelope =
        serde_json::from_slice(&bytes).map_err(|_| "This file is not a Switchboard backup.")?;
    if envelope.format != FORMAT || envelope.version != VERSION {
        return Err("This file is not a Switchboard backup.".into());
    }
    let nonce: [u8; NONCE_LEN] = STANDARD
        .decode(&envelope.nonce)
        .ok()
        .and_then(|n| n.try_into().ok())
        .ok_or("This file is not a Switchboard backup.")?;
    let mut sealed = STANDARD
        .decode(&envelope.data)
        .map_err(|_| "This file is not a Switchboard backup.")?;
    let plain = key(keys, false)?
        .0
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(
                envelope.created_at,
                &envelope.store,
                envelope.accounts,
                envelope.missing,
            )),
            &mut sealed,
        )
        .map_err(|_| FOREIGN)?;
    let payload: Payload = serde_json::from_slice(plain)
        .map_err(|_| "This backup is damaged and cannot be restored.")?;
    let mut result = Restored {
        added: 0,
        skipped: 0,
        failed: 0,
    };
    let existing = store.snapshot()?;
    for account in payload.accounts {
        let token = payload
            .credentials
            .get(&account.id)
            .map(|c| c.access_token.as_str());
        // Same rule as `upsert` when identities are unknown: the same token in the same
        // provider and pool is the same account, so a second restore changes nothing.
        let known = existing.accounts.iter().any(|a| a.id == account.id)
            || account.external_identity.as_ref().is_some_and(|identity| {
                store
                    .match_external(account.provider, &account.pool, identity)
                    .ok()
                    .flatten()
                    .is_some()
            })
            || existing.accounts.iter().any(|a| {
                a.provider == account.provider
                    && a.pool == account.pool
                    && token.is_some_and(|t| {
                        store
                            .stored_credential(&a.id)
                            .is_ok_and(|c| c.access_token == t)
                    })
            });
        if known {
            result.skipped += 1;
            continue;
        }
        let Some(mut credential) = payload.credentials.get(&account.id).cloned() else {
            result.failed += 1;
            continue;
        };
        // The same account already saved in another pool may hold a newer generation than the
        // backup: the restored copy starts from it, never from a token since spent.
        if let Some(identity) = account
            .external_identity
            .as_ref()
            .filter(|i| i.account_id.is_some())
        {
            let newest = existing
                .accounts
                .iter()
                .filter(|a| {
                    a.provider == account.provider
                        && a.external_identity.as_ref().is_some_and(|other| {
                            other.account_id == identity.account_id
                                && other.organization_id == identity.organization_id
                        })
                })
                .filter_map(|a| store.stored_credential(&a.id).ok())
                .filter(|stored| {
                    stored.refresh_token != credential.refresh_token
                        && stored.expires_at.unwrap_or(0) >= credential.expires_at.unwrap_or(0)
                })
                .max_by_key(|stored| stored.expires_at.unwrap_or(0));
            if let Some(newest) = newest {
                credential = newest;
            }
        }
        match store.upsert(
            account.label.clone(),
            account.provider,
            account.kind,
            account.pool.clone(),
            credential,
            account.external_identity.clone(),
        ) {
            Ok(saved) => {
                if !account.enabled {
                    let _ = store.update(&saved.id, saved.label.clone(), false);
                }
                result.added += 1;
            }
            Err(_) => result.failed += 1,
        }
    }
    let policies = store.snapshot()?.policies;
    for policy in payload.policies {
        let present = policies.iter().any(|p| {
            p.provider == policy.provider && p.pool == policy.pool && p.target == policy.target
        });
        if !present {
            // Restored switched off: rotation is the operator's to turn on again.
            let mut policy = policy;
            policy.enabled = false;
            let _ = store.set_policy(policy);
        }
    }
    Ok(result)
}

/// macOS key: one generic password, 64 hex characters, written through `security -i`.
#[cfg(target_os = "macos")]
pub struct KeychainKey;
#[cfg(target_os = "macos")]
const KEY_SERVICE: &str = "ai.passioncode.fabric-switchboard.backup-key";
#[cfg(target_os = "macos")]
impl BackupKey for KeychainKey {
    fn load(&self) -> Result<Option<[u8; 32]>, String> {
        crate::security_cli::find(KEY_SERVICE, "v1")
            .map_err(|_| UNAVAILABLE.to_string())?
            .map(|text| decode_key(&text).ok_or_else(|| UNAVAILABLE.to_string()))
            .transpose()
    }
    fn create(&self, key: &[u8; 32]) -> Result<bool, String> {
        let text: String = key.iter().map(|b| format!("{b:02x}")).collect();
        match crate::security_cli::add(KEY_SERVICE, "v1", text.as_bytes(), false)
            .map_err(|_| UNAVAILABLE)?
        {
            crate::security_cli::Added::Stored => Ok(true),
            crate::security_cli::Added::Exists => Ok(false),
        }
    }
}

/// Windows key: DPAPI CurrentUser encryption in a private file under LOCALAPPDATA.
#[cfg(windows)]
pub struct DpapiKey(pub std::path::PathBuf);
#[cfg(windows)]
impl BackupKey for DpapiKey {
    fn load(&self) -> Result<Option<[u8; 32]>, String> {
        if !self.0.exists() {
            return Ok(None);
        }
        let sealed = private_fs::read_private(&self.0, 4096)?;
        let plain = crate::windows::crypt(&sealed, false)?;
        plain
            .try_into()
            .map(Some)
            .map_err(|_| UNAVAILABLE.to_string())
    }
    fn create(&self, key: &[u8; 32]) -> Result<bool, String> {
        if self.0.exists() {
            return Ok(false);
        }
        let sealed = crate::windows::crypt(key, true)?;
        private_fs::private_write(&self.0, &sealed)?;
        Ok(true)
    }
}

#[cfg(any(target_os = "macos", test))]
fn decode_key(text: &[u8]) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (index, pair) in text.chunks(2).enumerate() {
        key[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

/// The platform's key, when this platform has one. On Windows the DPAPI-sealed key sits in
/// the backup folder itself, outside the app's data folder, so reinstalling keeps it.
pub fn platform_key(dir: &Path) -> Option<std::sync::Arc<dyn BackupKey>> {
    #[cfg(target_os = "macos")]
    {
        let _ = dir;
        Some(std::sync::Arc::new(KeychainKey))
    }
    #[cfg(windows)]
    {
        Some(std::sync::Arc::new(DpapiKey(dir.join(".backup-key.dpapi"))))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = dir;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthKind, ExternalIdentity, MemoryVault, Provider};
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    struct Keys(Arc<Mutex<Option<[u8; 32]>>>);
    fn names(dir: &Path, store: &Store, keys: &Keys) -> Vec<Info> {
        list(dir, store, keys)
    }
    impl BackupKey for Keys {
        fn load(&self) -> Result<Option<[u8; 32]>, String> {
            Ok(*self.0.lock().unwrap())
        }
        fn create(&self, key: &[u8; 32]) -> Result<bool, String> {
            let mut k = self.0.lock().unwrap();
            if k.is_some() {
                return Ok(false);
            }
            *k = Some(*key);
            Ok(true)
        }
    }
    fn store(dir: &Path) -> Store {
        Store::open(dir.to_owned(), Arc::new(MemoryVault::default())).unwrap()
    }
    fn identity(id: &str) -> ExternalIdentity {
        ExternalIdentity {
            account_id: Some(id.into()),
            organization_id: Some("synthetic-org".into()),
            email: Some(format!("{id}@example.invalid")),
        }
    }
    fn oauth(id: &str, token: &str) -> Credential {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &serde_json::json!({"claudeAiOauth":{"accessToken":token,"refreshToken":format!("{token}-refresh"),"expiresAt":4_000_000_000_000i64,"accountUuid":id}}).to_string(),
        )
        .unwrap()
    }
    fn save(store: &Store, id: &str, token: &str) -> Account {
        store
            .upsert(
                id.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                oauth(id, token),
                Some(identity(id)),
            )
            .unwrap()
    }

    #[test]
    fn a_backup_restores_into_a_fresh_install_and_is_sealed_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let original = store(&temp.path().join("one"));
        save(&original, "synthetic-a", "token-a-secret");
        let b = save(&original, "synthetic-b", "token-b-secret");
        original.update(&b.id, "synthetic-b".into(), false).unwrap();
        let info = write(&original, &backups, &keys, 1_000).unwrap().unwrap();
        assert_eq!(info.accounts, 2);
        let text = std::fs::read_to_string(backups.join(&info.file)).unwrap();
        assert!(!text.contains("token-a-secret") && !text.contains("synthetic-a@"));
        assert_eq!(names(&backups, &original, &keys), vec![info.clone()]);
        // Reinstall: an empty store on the same machine.
        let fresh = store(&temp.path().join("two"));
        let restored = restore(&fresh, &backups, &info.file, &keys).unwrap();
        assert_eq!(
            restored,
            Restored {
                added: 2,
                skipped: 0,
                failed: 0
            }
        );
        let accounts = fresh.snapshot().unwrap().accounts;
        assert_eq!(accounts.len(), 2);
        assert!(
            accounts
                .iter()
                .any(|a| a.label == "synthetic-b" && !a.enabled),
            "disabled stays disabled"
        );
        let a = accounts.iter().find(|a| a.label == "synthetic-a").unwrap();
        assert_eq!(
            fresh.stored_credential(&a.id).unwrap().access_token,
            "token-a-secret"
        );
    }
    #[test]
    fn restore_never_replaces_a_newer_sign_in() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let store = store(&temp.path().join("one"));
        let a = save(&store, "synthetic-a", "old-token");
        let info = write(&store, &backups, &keys, 1_000).unwrap().unwrap();
        save(&store, "synthetic-a", "new-token");
        let restored = restore(&store, &backups, &info.file, &keys).unwrap();
        assert_eq!(
            restored,
            Restored {
                added: 0,
                skipped: 1,
                failed: 0
            }
        );
        assert_eq!(
            store.stored_credential(&a.id).unwrap().access_token,
            "new-token"
        );
    }
    #[test]
    fn a_restored_copy_starts_from_the_newest_generation_saved_elsewhere() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let store = store(&temp.path().join("one"));
        let a = save(&store, "synthetic-a", "old-token");
        let info = write(&store, &backups, &keys, 1_000).unwrap().unwrap();
        // Later: A was removed from default, and its copy in "work" was renewed since.
        store.remove(&a.id).unwrap();
        let mut newer = oauth("synthetic-a", "new-token");
        newer.expires_at = newer.expires_at.map(|t| t + 3600);
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "work".into(),
                newer,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let restored = restore(&store, &backups, &info.file, &keys).unwrap();
        assert_eq!(restored.added, 1);
        let back = store
            .snapshot()
            .unwrap()
            .accounts
            .into_iter()
            .find(|x| x.pool == "default")
            .unwrap();
        assert_eq!(
            store.stored_credential(&back.id).unwrap().access_token,
            "new-token",
            "never the spent generation from the backup"
        );
    }
    #[test]
    fn another_key_or_a_tampered_file_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let store = store(&temp.path().join("one"));
        save(&store, "synthetic-a", "token");
        let info = write(&store, &backups, &keys, 1_000).unwrap().unwrap();
        let other = Keys(Arc::new(Mutex::new(Some([7u8; 32]))));
        let fresh = super::tests::store(&temp.path().join("two"));
        assert_eq!(
            restore(&fresh, &backups, &info.file, &other).unwrap_err(),
            FOREIGN
        );
        // No key on this machine at all: restore never invents one.
        let empty = Keys::default();
        assert_eq!(
            restore(&fresh, &backups, &info.file, &empty).unwrap_err(),
            FOREIGN
        );
        assert!(empty.0.lock().unwrap().is_none());
        // A header edited to another time no longer authenticates.
        let path = backups.join(&info.file);
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\"created_at\": 1000", "\"created_at\": 1001");
        std::fs::write(&path, text).unwrap();
        assert_eq!(
            restore(&fresh, &backups, &info.file, &keys).unwrap_err(),
            FOREIGN
        );
        assert!(restore(&fresh, &backups, "../escape.json", &keys).is_err());
    }
    #[test]
    fn only_the_newest_ten_are_kept_and_an_empty_store_writes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let store = store(&temp.path().join("one"));
        assert_eq!(write(&store, &backups, &keys, 1).unwrap(), None);
        save(&store, "synthetic-a", "token");
        for at in 1..=12 {
            write(&store, &backups, &keys, at * 100).unwrap();
        }
        let kept = names(&backups, &store, &keys);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0].created_at, 1200);
        assert_eq!(kept[KEEP - 1].created_at, 300);
        std::fs::write(backups.join("notes.txt"), "kept").unwrap();
        write(&store, &backups, &keys, 1300).unwrap();
        assert!(
            backups.join("notes.txt").exists(),
            "foreign files are never touched"
        );
    }
    #[test]
    fn stores_sharing_a_folder_never_prune_each_other() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let one = store(&temp.path().join("one"));
        let two = super::tests::store(&temp.path().join("two"));
        save(&one, "synthetic-a", "token-a");
        save(&two, "synthetic-b", "token-b");
        for at in 1..=12 {
            write(&one, &backups, &keys, at * 10).unwrap();
            write(&two, &backups, &keys, at * 10 + 1).unwrap();
        }
        let all = list(&backups, &one, &keys);
        assert_eq!(all.iter().filter(|i| i.own).count(), KEEP);
        assert_eq!(
            all.iter().filter(|i| !i.own).count(),
            KEEP,
            "the other store kept its ten"
        );
        assert!(all.iter().all(|i| i.openable));
        // Another machine's key: listed, never openable.
        let other = Keys(Arc::new(Mutex::new(Some([5u8; 32]))));
        assert!(list(&backups, &one, &other).iter().all(|i| !i.openable));
    }
    #[test]
    fn partial_backups_never_push_out_the_last_complete_one() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let vault = Arc::new(MemoryVault::default());
        let store = Store::open(temp.path().join("one"), vault.clone()).unwrap();
        save(&store, "synthetic-a", "token-a");
        let b = save(&store, "synthetic-b", "token-b");
        let complete = write(&store, &backups, &keys, 100).unwrap().unwrap();
        assert_eq!((complete.accounts, complete.missing), (2, 0));
        // b's credential becomes unreadable for a while (a locked or refused Keychain item).
        use crate::Vault;
        vault.delete(&b.id).unwrap();
        for at in 2..=15 {
            let partial = write(&store, &backups, &keys, at * 100).unwrap().unwrap();
            assert_eq!((partial.accounts, partial.missing), (1, 1));
        }
        let kept = list(&backups, &store, &keys);
        assert!(
            kept.iter().any(|i| i.file == complete.file),
            "the complete backup survives"
        );
        assert!(kept.len() <= KEEP + 1);
    }
    #[test]
    fn restoring_twice_changes_nothing_for_accounts_without_identity() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let original = store(&temp.path().join("one"));
        let key_account = original
            .add(
                "Old label".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "synthetic-api-key").unwrap(),
            )
            .unwrap();
        original
            .update(&key_account.id, "Old label".into(), false)
            .unwrap();
        let info = write(&original, &backups, &keys, 100).unwrap().unwrap();
        let fresh = super::tests::store(&temp.path().join("two"));
        assert_eq!(
            restore(&fresh, &backups, &info.file, &keys).unwrap().added,
            1
        );
        let restored = fresh.snapshot().unwrap().accounts.remove(0);
        fresh.update(&restored.id, "Renamed".into(), true).unwrap();
        let again = restore(&fresh, &backups, &info.file, &keys).unwrap();
        assert_eq!(
            again,
            Restored {
                added: 0,
                skipped: 1,
                failed: 0
            }
        );
        let after = fresh.snapshot().unwrap().accounts.remove(0);
        assert_eq!((after.label.as_str(), after.enabled), ("Renamed", true));
    }
    #[test]
    fn key_text_is_exactly_64_hex_characters() {
        assert_eq!(decode_key(&[b'0'; 64]), Some([0u8; 32]));
        assert!(decode_key(b"ff").is_none());
    }
}
