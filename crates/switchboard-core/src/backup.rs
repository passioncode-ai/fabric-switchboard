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

fn key(keys: &dyn BackupKey, create: bool) -> Result<LessSafeKey, String> {
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
    Ok(LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, &bytes).map_err(|_| UNAVAILABLE)?,
    ))
}
fn aad(created_at: i64) -> Vec<u8> {
    format!("{FORMAT}:v{VERSION}:{created_at}").into_bytes()
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
    let key = key(keys, true)?;
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| UNAVAILABLE)?;
    let mut sealed = payload;
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad(now)),
        &mut sealed,
    )
    .map_err(|_| UNAVAILABLE)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        created_at: now,
        accounts: count,
        nonce: STANDARD.encode(nonce),
        data: STANDARD.encode(&sealed),
    };
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| UNAVAILABLE)?;
    private_fs::private_write(&dir.join(file_name(now)), &bytes)?;
    prune(dir, KEEP);
    Ok(Some(Info {
        file: file_name(now),
        created_at: now,
        accounts: count,
    }))
}

/// Backups in `dir`, newest first; unreadable files are left out, never deleted.
pub fn list(dir: &Path) -> Vec<Info> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut found: Vec<Info> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if !valid_name(&name) {
                return None;
            }
            let bytes = private_fs::read_private(&entry.path(), MAX_FILE).ok()?;
            let envelope: Envelope = serde_json::from_slice(&bytes).ok()?;
            (envelope.format == FORMAT && envelope.version == VERSION).then_some(Info {
                file: name,
                created_at: envelope.created_at,
                accounts: envelope.accounts,
            })
        })
        .collect();
    found.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    found
}

fn prune(dir: &Path, keep: usize) {
    for old in list(dir).into_iter().skip(keep) {
        let _ = std::fs::remove_file(dir.join(old.file));
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
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(envelope.created_at)),
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
        let known = existing.accounts.iter().any(|a| a.id == account.id)
            || account.external_identity.as_ref().is_some_and(|identity| {
                store
                    .match_external(account.provider, &account.pool, identity)
                    .ok()
                    .flatten()
                    .is_some()
            });
        if known {
            result.skipped += 1;
            continue;
        }
        let Some(credential) = payload.credentials.get(&account.id).cloned() else {
            result.failed += 1;
            continue;
        };
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

/// The platform's key, when this platform has one.
pub fn platform_key() -> Option<std::sync::Arc<dyn BackupKey>> {
    #[cfg(target_os = "macos")]
    {
        Some(std::sync::Arc::new(KeychainKey))
    }
    #[cfg(windows)]
    {
        let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("ai.passioncode.fabric-switchboard");
        Some(std::sync::Arc::new(DpapiKey(root.join("backup.key.dpapi"))))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
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
        assert_eq!(list(&backups), vec![info.clone()]);
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
        let kept = list(&backups);
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
    fn key_text_is_exactly_64_hex_characters() {
        assert_eq!(decode_key(&[b'0'; 64]), Some([0u8; 32]));
        assert!(decode_key(b"ff").is_none());
    }
}
