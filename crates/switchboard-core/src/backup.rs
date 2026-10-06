//! Automatic encrypted backups of saved accounts (0.5). A backup is one JSON file: a readable
//! header (format, version, time, account count) and the accounts, policies and credentials
//! sealed with AES-256-GCM under a 256-bit key kept on this machine — the login Keychain
//! through `/usr/bin/security` on macOS, a DPAPI-protected file on Windows. Operator decision:
//! no passphrase, so a backup restores after reinstalling Switchboard, not on another machine
//! or after the Keychain is lost (docs/PLAN-0.5.md, D-5).
use crate::{private_fs, Account, Credential, Project, ProjectRule, RotationPolicy, Store};
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
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Restored {
    pub added: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Projects, project rules and managed selections put back (0.6); settings restored.
    #[serde(default)]
    pub projects: usize,
    #[serde(default)]
    pub rules: usize,
    #[serde(default)]
    pub routes: usize,
    #[serde(default)]
    pub settings: usize,
}
/// Settings Switchboard keeps as small files in its data folder, carried by every backup so a
/// reinstall gets them back (`login-item`: open at login, SB-28; `auto-update`: the updater).
pub const SETTING_FILES: &[&str] = &["login-item", "auto-update"];
const MAX_SETTING: u64 = 64;

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
    // 0.6: everything else a reinstall must get back. A 0.5 backup has none of them (default);
    // a 0.5 reader ignores them.
    #[serde(default)]
    rules: Vec<ProjectRule>,
    #[serde(default)]
    projects: Vec<Project>,
    /// Managed selections, `provider:pool` → account id.
    #[serde(default)]
    routes: BTreeMap<String, String>,
    #[serde(default)]
    settings: BTreeMap<String, String>,
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
    format!(
        "{PREFIX}{created_at}-{}.json",
        uuid::Uuid::new_v4().simple()
    )
}
fn valid_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(".json"))
        .is_some_and(|generation| {
            let (digits, suffix) = match generation.split_once('-') {
                Some((digits, suffix)) => (digits, Some(suffix)),
                None => (generation, None), // Timestamp-only backups from earlier versions.
            };
            !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && suffix
                    .is_none_or(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        })
}

/// Reserve a new destination exclusively before atomic publication. Even a collision must
/// never replace a previous generation or another store's file. An interrupted reservation
/// is empty (or remains empty if publication fails) and is ignored by listing and pruning.
fn write_new(dir: &Path, file: &str, bytes: &[u8]) -> Result<(), String> {
    private_fs::private_dir(dir)?;
    let path = dir.join(file);
    drop(private_fs::create_new(&path)?);
    let result = private_fs::private_write(&path, bytes);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
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
    let settings = SETTING_FILES
        .iter()
        .filter_map(|name| {
            let bytes = private_fs::read_private(&store.root().join(name), MAX_SETTING).ok()?;
            let text = String::from_utf8(bytes).ok()?.trim().to_owned();
            (!text.is_empty()).then(|| ((*name).to_owned(), text))
        })
        .collect();
    let payload = serde_json::to_vec(&Payload {
        accounts,
        policies: snapshot.policies,
        credentials,
        rules: snapshot.rules,
        projects: snapshot.projects,
        routes: snapshot.routes,
        settings,
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
    let file = file_name(now);
    write_new(dir, &file, &bytes)?;
    prune(dir, &store_id, KEEP);
    Ok(Some(Info {
        file,
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
        projects: 0,
        rules: 0,
        routes: 0,
        settings: 0,
    };
    let existing = store.snapshot()?;
    // Backup account id → the id it has here, for rules and selections.
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
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
            let here = existing
                .accounts
                .iter()
                .find(|a| a.id == account.id)
                .map(|a| a.id.clone())
                .or_else(|| {
                    account.external_identity.as_ref().and_then(|identity| {
                        store
                            .match_external(account.provider, &account.pool, identity)
                            .ok()
                            .flatten()
                            .map(|a| a.id)
                    })
                });
            if let Some(here) = here {
                // Known but without a readable credential: the backup's copy brings it back.
                if let Some(credential) = payload.credentials.get(&account.id) {
                    if store
                        .restore_missing_credential(&here, credential)
                        .unwrap_or(false)
                    {
                        result.added += 1;
                        ids.insert(account.id.clone(), here);
                        continue;
                    }
                }
                ids.insert(account.id.clone(), here);
            }
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
                ids.insert(account.id.clone(), saved.id.clone());
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
    // Projects first: their accounts are already in their pools; only the reservation returns.
    for project in payload.projects {
        if store.restore_project(project).is_ok() {
            result.projects += 1;
        }
    }
    let now = crate::now();
    let here = store.snapshot()?;
    for rule in payload.rules {
        let Some(id) = ids.get(&rule.account_id) else {
            continue;
        };
        let taken = here
            .rules
            .iter()
            .any(|r| r.path == rule.path && r.provider == rule.provider);
        // An expired rule never applies again; one that is still running keeps its end.
        if taken || rule.expires_at.is_some_and(|t| t <= now) {
            continue;
        }
        if store
            .set_rule(
                Path::new(&rule.path),
                id,
                &rule.target,
                rule.enabled,
                rule.expires_at,
            )
            .is_ok()
        {
            result.rules += 1;
        }
    }
    for (key, old) in payload.routes {
        let (Some((provider, pool)), Some(id)) = (key.split_once(':'), ids.get(&old)) else {
            continue;
        };
        let Ok(provider) = serde_json::from_value::<crate::Provider>(serde_json::json!(provider))
        else {
            continue;
        };
        if store.snapshot()?.routes.contains_key(&key) {
            continue;
        }
        if store.select(provider, pool, id).is_ok() {
            result.routes += 1;
        }
    }
    for (name, value) in payload.settings {
        let path = store.root().join(&name);
        // Only the known settings, never over a choice made on this install.
        if SETTING_FILES.contains(&name.as_str())
            && value.len() as u64 <= MAX_SETTING
            && !path.exists()
            && private_fs::private_write(&path, format!("{value}\n").as_bytes()).is_ok()
        {
            result.settings += 1;
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

/// Windows key: DPAPI CurrentUser encryption in `.backup-key.dpapi` inside the backups folder
/// (`%APPDATA%\Fabric Switchboard Backups`), outside the data folder an uninstaller may remove.
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
    fn a_reinstall_gets_projects_rules_selections_and_settings_back() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let original = store(&temp.path().join("one"));
        let a = save(&original, "synthetic-a", "token-a-secret");
        let b = save(&original, "synthetic-b", "token-b-secret");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        original
            .save_project(
                None,
                "Alpha",
                std::slice::from_ref(&repo),
                std::slice::from_ref(&a.id),
            )
            .unwrap();
        original
            .set_rule(&repo, &a.id, "managed", true, None)
            .unwrap();
        original.select(Provider::Claude, "default", &b.id).unwrap();
        original.select(Provider::Claude, "alpha", &a.id).unwrap();
        std::fs::write(original.root().join("login-item"), "off\n").unwrap();
        let info = write(&original, &backups, &keys, 1_000).unwrap().unwrap();
        // Reinstall: `switchboard uninstall` removed the data folder; the backups stayed.
        let fresh = store(&temp.path().join("two"));
        let restored = restore(&fresh, &backups, &info.file, &keys).unwrap();
        assert_eq!(
            (
                restored.added,
                restored.projects,
                restored.rules,
                restored.routes,
                restored.settings
            ),
            (2, 1, 1, 2, 1)
        );
        let snap = fresh.snapshot().unwrap();
        let by_label = |l: &str| snap.accounts.iter().find(|x| x.label == l).unwrap().clone();
        let (a2, b2) = (by_label("synthetic-a"), by_label("synthetic-b"));
        assert_eq!(snap.projects[0].name, "Alpha");
        assert_eq!(
            a2.pool, "alpha",
            "the project's account is back in its pool"
        );
        assert_eq!(
            snap.rules[0].account_id, a2.id,
            "the rule points at the restored account"
        );
        assert_eq!(snap.routes.get("claude:default"), Some(&b2.id));
        assert_eq!(snap.routes.get("claude:alpha"), Some(&a2.id));
        assert_eq!(
            std::fs::read_to_string(fresh.root().join("login-item"))
                .unwrap()
                .trim(),
            "off"
        );
        // A second restore changes nothing and never overwrites a choice made here.
        std::fs::write(fresh.root().join("login-item"), "on\n").unwrap();
        let again = restore(&fresh, &backups, &info.file, &keys).unwrap();
        assert_eq!(
            (
                again.added,
                again.skipped,
                again.projects,
                again.rules,
                again.routes,
                again.settings
            ),
            (0, 2, 0, 0, 0, 0)
        );
        assert_eq!(
            std::fs::read_to_string(fresh.root().join("login-item"))
                .unwrap()
                .trim(),
            "on"
        );
    }
    #[test]
    fn a_known_account_that_lost_its_credential_gets_it_back() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let vault = Arc::new(MemoryVault::default());
        let original = Store::open(temp.path().join("one"), vault.clone()).unwrap();
        let a = save(&original, "synthetic-a", "token-a-secret");
        let info = write(&original, &backups, &keys, 1_000).unwrap().unwrap();
        // `uninstall --keep-data`: the entry stays, the credential is gone.
        crate::Vault::delete(vault.as_ref(), &a.id).unwrap();
        assert!(original.stored_credential(&a.id).is_err());
        let restored = restore(&original, &backups, &info.file, &keys).unwrap();
        assert_eq!((restored.added, restored.skipped), (1, 0));
        assert_eq!(
            original.stored_credential(&a.id).unwrap().access_token,
            "token-a-secret"
        );
        // With a working credential, a restore never replaces it.
        let again = restore(&original, &backups, &info.file, &keys).unwrap();
        assert_eq!((again.added, again.skipped), (0, 1));
    }
    #[test]
    fn a_rule_change_counts_for_the_next_backup() {
        let temp = tempfile::tempdir().unwrap();
        let original = store(&temp.path().join("one"));
        let a = save(&original, "synthetic-a", "token-a");
        let before = original.changes();
        original
            .set_rule(temp.path(), &a.id, "managed", true, None)
            .unwrap();
        assert!(original.changes() > before);
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
                failed: 0,
                ..Restored::default()
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
                failed: 0,
                ..Restored::default()
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
        assert_eq!(
            restore(&fresh, &backups, "../accounts.json", &keys).unwrap_err(),
            "Choose a backup from the backup folder."
        );
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
    fn same_second_partial_backup_never_replaces_the_complete_one() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let vault = Arc::new(MemoryVault::default());
        let store = Store::open(temp.path().join("one"), vault.clone()).unwrap();
        save(&store, "synthetic-a", "token-a");
        let b = save(&store, "synthetic-b", "token-b");
        let full = write(&store, &backups, &keys, 100).unwrap().unwrap();
        use crate::Vault;
        vault.delete(&b.id).unwrap();
        let partial = write(&store, &backups, &keys, 100).unwrap().unwrap();
        assert_ne!(full.file, partial.file, "each backup is a new generation");
        let kept = list(&backups, &store, &keys);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|i| i.file == full.file && i.missing == 0));
        let fresh = super::tests::store(&temp.path().join("fresh"));
        assert_eq!(
            restore(&fresh, &backups, &full.file, &keys).unwrap().added,
            2
        );
    }
    #[test]
    fn same_second_backups_from_different_stores_both_survive() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let one = store(&temp.path().join("one"));
        let two = store(&temp.path().join("two"));
        save(&one, "synthetic-a", "token-a");
        save(&two, "synthetic-b", "token-b");
        let first = write(&one, &backups, &keys, 100).unwrap().unwrap();
        let second = write(&two, &backups, &keys, 100).unwrap().unwrap();
        assert_ne!(first.file, second.file);
        assert_eq!(
            list(&backups, &one, &keys).iter().filter(|i| i.own).count(),
            1
        );
        assert_eq!(
            list(&backups, &two, &keys).iter().filter(|i| i.own).count(),
            1
        );
    }
    #[test]
    fn timestamp_only_backups_still_list_and_restore() {
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let keys = Keys::default();
        let original = store(&temp.path().join("one"));
        save(&original, "synthetic-a", "token-a");
        let info = write(&original, &backups, &keys, 100).unwrap().unwrap();
        let legacy = "switchboard-backup-100.json";
        std::fs::rename(backups.join(info.file), backups.join(legacy)).unwrap();
        assert_eq!(list(&backups, &original, &keys)[0].file, legacy);
        let fresh = store(&temp.path().join("fresh"));
        assert_eq!(restore(&fresh, &backups, legacy, &keys).unwrap().added, 1);
        for invalid in [
            "switchboard-backup-.json",
            "switchboard-backup-100-.json",
            "switchboard-backup-100-../escape.json",
            "switchboard-backup-100-not-a-generation.json",
        ] {
            assert!(!valid_name(invalid));
        }
    }
    #[test]
    fn immutable_publication_never_replaces_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = "switchboard-backup-100.json";
        let path = temp.path().join(file);
        private_fs::private_write(&path, b"existing generation").unwrap();
        assert!(write_new(temp.path(), file, b"replacement").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"existing generation");
    }
    #[cfg(unix)]
    #[test]
    fn immutable_publication_refuses_symlinks_and_keeps_private_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        private_fs::private_dir(&backups).unwrap();
        let target = temp.path().join("unrelated");
        std::fs::write(&target, b"untouched").unwrap();
        let file = "switchboard-backup-100.json";
        symlink(&target, backups.join(file)).unwrap();
        assert!(write_new(&backups, file, b"replacement").is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"untouched");
        assert!(std::fs::symlink_metadata(backups.join(file))
            .unwrap()
            .file_type()
            .is_symlink());
        write_new(&backups, "switchboard-backup-101.json", b"private").unwrap();
        assert_eq!(
            std::fs::metadata(backups.join("switchboard-backup-101.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
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
                failed: 0,
                ..Restored::default()
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
