//! SB-403 S2 (DEC-0025): a subscription sign-in another machine delegated to this one.
//!
//! The sender moves a sign-in here with `accounts send --delegate` through this machine's
//! restricted ssh entry (`switchboard ssh-serve`, a forced command that names the sender's key).
//! The account it saves is *delegated*: it carries the sender's key and the group it was
//! delegated into, and it stays a guest here. It serves that group's folders only; it is never
//! sent on, given to an agent, lent or shared, moved to another group (the default one included),
//! marked as used elsewhere or made the ordinary login, and only the sender calls it back or
//! removes it. A lease bounds it: the sender's background read of this machine extends it (by
//! default to 48 hours ahead); past it (and only once this machine's owner has run a full read
//! interval past it), the copy is switched off and never renewed, but its credential is kept, so
//! the sender can still call the live sign-in back, or revive the seat by extending its lease.
//! Thirty days after that, the credential is removed.
//!
//! Threat model: the teammate owns this machine. These rules keep an honest teammate's Switchboard
//! from passing a guest on by mistake and let the sender take it back; they do not stop a teammate
//! who reads their own Keychain. Against an uncooperative teammate the revocation is signing the
//! subscription out at the provider.
//!
//! The rules are checked on every publication of the store (`stays`), so a path that forgets
//! them fails closed with [`DELEGATED_STAYS`] instead of passing a guest's sign-in on.
use crate::{now, pool_valid, uuid_valid, Snapshot, Vault, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};

/// The lease a delegation gets when the sender names none, in hours.
pub const DEFAULT_LEASE_HOURS: u32 = 48;
/// The longest lease a sender may give at once: 30 days.
pub const MAX_LEASE_HOURS: u32 = 720;
/// A lease ends only once this machine's owner has run this long past it (and past its own
/// start): one background read interval of the sender, so a sender that just came back online
/// extends it first.
pub const LEASE_GRACE_SECONDS: i64 = 15 * 60;
/// How long the credential of a seat whose lease ended is kept, off, for the sender to call back.
pub const KEEP_AFTER_LEASE_SECONDS: i64 = 30 * 86_400;

pub const DELEGATED_STAYS: &str = "This seat was delegated to this computer by another machine. It stays in the group it was delegated to and is used only here; only that machine calls it back.";
pub const DELEGATED_NOT_SENT: &str = "This seat was delegated to this computer by another machine, so it cannot be moved or handed on. Only that machine calls it back.";
pub const DELEGATED_NOT_GIVEN: &str =
    "This seat was delegated to this computer by another machine, so it is not given to an agent.";
pub const DELEGATED_NOT_REMOVED: &str = "This seat was delegated to this computer by another machine. Ask that machine to call it back; it can be removed here 30 days after its lease ran out.";
pub const DELEGATED_NOT_NATIVE: &str = "This seat was delegated to this computer by another machine. It serves its group's sessions only and never becomes the ordinary Claude Code or Codex sign-in.";
pub const NOT_DELEGATED_BY_YOU: &str =
    "This computer holds no seat with that id that the calling machine delegated.";
pub const LEASE_ENDED: &str = "This seat's lease ran out, so it is switched off here. The machine that delegated it extends its lease to use it again, or calls it back.";
pub const SIGN_IN_REMOVED: &str = "This seat's sign-in was removed from this computer 30 days after its lease ran out. The machine that delegated it signs in to the account again and delegates it again.";
pub const GROUP_HOLDS_GUEST: &str = "This group holds a seat another machine delegated to this computer. Ask that machine to call the seat back before deleting the group.";
pub const LEASE_INVALID: &str = "Give the lease in whole hours, from 1 to 720.";
pub const KEY_INVALID: &str = "That is not a machine's key. Give the key switchboard settings signer printed on the machine that delegates.";
pub const DELEGATE_GROUP: &str = "A delegated seat joins one of this computer's groups, never the default group. Name a group this computer has.";
pub const FROM_INVALID: &str =
    "Name the machine that delegates in at most 80 characters, on one line.";

/// One delegated account's record (`Snapshot::delegated`, keyed by the account id here).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delegation {
    /// The sender's settings signing key (base64 Ed25519, `switchboard settings signer` there),
    /// as this machine's forced ssh command names it. Only a call carrying it acts on the seat.
    pub sender_key: String,
    /// The sender's name in this machine's words (`ssh-serve --from`), when one was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The group it was delegated into; it never leaves it.
    pub pool: String,
    pub granted_at: i64,
    /// Until when it may be renewed and used here (Unix seconds).
    pub lease_until: i64,
    /// When its lease ran out here: switched off and never renewed, its credential kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// When its credential was removed, `KEEP_AFTER_LEASE_SECONDS` after it ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged_at: Option<i64>,
    /// When its sender called it back (W1.2, DEC-0027): switched off here, its tokens given back.
    /// Cleared when the sender undoes the call back. Absent in records written before 0.7.118,
    /// whose call back only the journal remembers (`off::seat_called_back`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surrendered_at: Option<i64>,
}

/// SB-403 (P2-5): on the machine that delegated a seat, what it needs to extend and call it back:
/// the seat's id there, its lease and the key it was delegated under (this machine's public key).
/// Kept in the store with the account's machine group (`Snapshot::delegated_out`), so it survives
/// `seats.jsonl` rotation, backups and restores.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegatedOut {
    pub remote_id: String,
    pub lease_hours: u32,
    pub sender_key: String,
    /// The print of the refresh token sent (16 hex), so a call back keeps a sign-in renewed here
    /// since (review round 3, P2-3). Absent in records written before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<String>,
}

impl Delegation {
    /// The lease is over (by its time, or marked so).
    pub fn ended(&self, now: i64) -> bool {
        self.ended_at.is_some() || now >= self.lease_until
    }
    /// Its credential is gone from this machine.
    pub fn purged(&self) -> bool {
        self.purged_at.is_some()
    }
}

/// A sender's key as `settings signer` prints it: base64 of 32 bytes.
pub fn key_valid(key: &str) -> bool {
    use base64::{engine::general_purpose::STANDARD, Engine};
    key.len() == 44 && STANDARD.decode(key).is_ok_and(|bytes| bytes.len() == 32)
}

pub fn from_valid(from: &str) -> bool {
    !from.is_empty()
        && from.chars().count() <= 80
        && from.trim() == from
        && !from.chars().any(char::is_control)
}

pub fn lease_valid(hours: u32) -> bool {
    (1..=MAX_LEASE_HOURS).contains(&hours)
}

/// The shape of every record, read or written.
pub(crate) fn validate(s: &Snapshot) -> Result<(), String> {
    let invalid = || Err("Invalid account metadata".to_string());
    if s.delegated.len() > MAX_ACCOUNTS {
        return invalid();
    }
    for (id, d) in &s.delegated {
        if !uuid_valid(id)
            || !s.accounts.iter().any(|a| &a.id == id)
            || !key_valid(&d.sender_key)
            || d.from.as_deref().is_some_and(|f| !from_valid(f))
            || !pool_valid(&d.pool)
            || d.granted_at <= 0
            || d.lease_until <= d.granted_at
            || d.ended_at.is_some_and(|t| t < d.granted_at)
            || d.purged_at
                .is_some_and(|t| d.ended_at.is_none_or(|e| t < e))
            || d.surrendered_at.is_some_and(|t| t < d.granted_at)
        {
            return invalid();
        }
    }
    Ok(())
}

fn remote_id_valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// The shape of the sender's records.
pub(crate) fn validate_out(s: &Snapshot) -> Result<(), String> {
    if s.delegated_out.len() > MAX_ACCOUNTS
        || s.delegated_out.iter().any(|(id, o)| {
            !s.accounts.iter().any(|a| &a.id == id)
                || !remote_id_valid(&o.remote_id)
                || !lease_valid(o.lease_hours)
                || !key_valid(&o.sender_key)
                || o.sent
                    .as_deref()
                    .is_some_and(|p| p.len() != 16 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
        })
    {
        return Err("Invalid account metadata".into());
    }
    Ok(())
}

/// Whether `id` is in any share or loan: a group's shared accounts, the default group's.
fn shared(s: &Snapshot, id: &str) -> bool {
    s.projects
        .iter()
        .any(|p| p.shared_accounts.iter().any(|x| x == id))
        || s.default_shared.iter().any(|x| x == id)
        || s.default_lent.iter().any(|x| x == id)
}

/// The rules a guest seat keeps, checked on every publication against the state before it.
pub(crate) fn stays(old: &Snapshot, new: &Snapshot) -> Result<(), String> {
    for (id, d) in &new.delegated {
        let Some(account) = new.accounts.iter().find(|a| &a.id == id) else {
            return Err(DELEGATED_STAYS.into());
        };
        if account.pool != d.pool
            || d.pool == crate::projects::DEFAULT_POOL
            || new.elsewhere.contains_key(id)
            || new.assigned_to.contains_key(id)
            || new.returns_to.contains_key(id)
            || shared(new, id)
        {
            return Err(DELEGATED_STAYS.into());
        }
        if d.ended_at.is_some() && (account.enabled || new.routes.values().any(|v| v == id)) {
            return Err(LEASE_ENDED.into());
        }
        match old.delegated.get(id) {
            Some(o) => {
                // A seat may come back from an ended lease (the sender extended it) while its
                // credential is still here; once removed, never.
                if o.sender_key != d.sender_key
                    || o.pool != d.pool
                    || o.granted_at != d.granted_at
                    || (o.purged() && (!d.purged() || d.ended_at.is_none()))
                {
                    return Err(if o.purged() {
                        SIGN_IN_REMOVED
                    } else {
                        DELEGATED_STAYS
                    }
                    .into());
                }
            }
            // A delegation is born with its account, never put on one already here.
            None => {
                if old.accounts.iter().any(|a| &a.id == id) {
                    return Err(DELEGATED_STAYS.into());
                }
            }
        }
    }
    // A record goes only with its account.
    for id in old.delegated.keys() {
        if !new.delegated.contains_key(id) && new.accounts.iter().any(|a| &a.id == id) {
            return Err(DELEGATED_STAYS.into());
        }
    }
    Ok(())
}

impl Snapshot {
    /// SB-403: the account was delegated here by another machine.
    pub fn is_delegated(&self, id: &str) -> bool {
        self.delegated.contains_key(id)
    }
    /// The delegated seats to end at `now`: lease over, and this machine's owner (running since
    /// `owner_since`) has run `LEASE_GRACE_SECONDS` past both the lease and its own start, so the
    /// sender's background read had its chance to extend it.
    pub fn leases_due(&self, now: i64, owner_since: i64) -> Vec<String> {
        self.delegated
            .iter()
            .filter(|(_, d)| {
                d.ended_at.is_none() && now >= d.lease_until.max(owner_since) + LEASE_GRACE_SECONDS
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
    /// The ended seats whose kept credential is due for removal at `now`.
    pub fn purges_due(&self, now: i64) -> Vec<String> {
        self.delegated
            .iter()
            .filter(|(_, d)| {
                !d.purged()
                    && d.ended_at
                        .is_some_and(|e| now >= e + KEEP_AFTER_LEASE_SECONDS)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// What a sender may do to a seat it delegated: the seat must be one, and carry its key.
fn owned<'a>(s: &'a Snapshot, id: &str, sender_key: &str) -> Result<&'a Delegation, String> {
    s.delegated
        .get(id)
        .filter(|d| d.sender_key == sender_key)
        .ok_or_else(|| NOT_DELEGATED_BY_YOU.to_string())
}

impl crate::Store {
    /// SB-403: saves a sign-in another machine delegated (`ssh-serve`): a new account of a
    /// subscription, in a group of this computer with folders, carrying the sender's key and a
    /// lease of `lease_hours` from now. The account and its record are published together.
    #[allow(clippy::too_many_arguments)]
    pub fn add_delegated(
        &self,
        label: String,
        provider: crate::Provider,
        pool: String,
        credential: crate::Credential,
        sender_key: &str,
        from: Option<&str>,
        lease_hours: u32,
    ) -> Result<crate::Account, String> {
        if !key_valid(sender_key) {
            return Err(KEY_INVALID.into());
        }
        if from.is_some_and(|f| !from_valid(f)) {
            return Err(FROM_INVALID.into());
        }
        if !lease_valid(lease_hours) {
            return Err(LEASE_INVALID.into());
        }
        {
            let state = self.lock()?;
            if pool == crate::projects::DEFAULT_POOL
                || state.project_of_pool(&pool).is_none()
                || state.projects.iter().any(|p| p.pool == pool && p.tokens)
            {
                return Err(DELEGATE_GROUP.into());
            }
        }
        let time = now();
        let delegation = Delegation {
            sender_key: sender_key.to_owned(),
            from: from.map(str::to_owned),
            pool: pool.clone(),
            granted_at: time,
            lease_until: time + i64::from(lease_hours) * 3600,
            ended_at: None,
            purged_at: None,
            surrendered_at: None,
        };
        self.save_delegated(label, provider, pool, credential, delegation)
    }
    /// The sender extends a seat's lease to `hours` from now (shorter or longer). A seat whose
    /// lease ended comes back on while its credential is still here (a sender that was offline
    /// over a weekend); once its credential is removed it does not.
    pub fn extend_lease(&self, id: &str, sender_key: &str, hours: u32) -> Result<i64, String> {
        if !lease_valid(hours) {
            return Err(LEASE_INVALID.into());
        }
        let time = now();
        let mut state = self.lock()?;
        let d = owned(&state, id, sender_key)?;
        if d.purged() {
            return Err(SIGN_IN_REMOVED.into());
        }
        let revived = d.ended_at.is_some();
        let until = time + i64::from(hours) * 3600;
        let mut candidate = state.clone();
        if let Some(d) = candidate.delegated.get_mut(id) {
            d.lease_until = until.max(d.granted_at + 1);
            d.ended_at = None;
        }
        if revived {
            if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == id) {
                // W1.2 (DEC-0027): a seat the person here turned off stays off; the lease is
                // extended all the same.
                a.enabled = a.off.is_none();
            }
            crate::append_event(&mut candidate, "delegation", Some(id), "restored");
        }
        // A lease's time is not worth a write of its own every 15 minutes, but a lease is what
        // keeps the seat working: it is written, as any other change.
        self.publish(&mut state, candidate)?;
        Ok(until)
    }
    /// A seat whose lease is over: switched off (never renewed or routed) and marked ended. Its
    /// credential stays, for the sender's call back or a later extension (`purge_lease` removes
    /// it 30 days on). The caller picks the seats with `Snapshot::leases_due`.
    pub fn end_lease(&self, id: &str) -> Result<(), String> {
        let time = now();
        let mut state = self.lock()?;
        let d = state.delegated.get(id).ok_or(NOT_DELEGATED_BY_YOU)?;
        if d.ended_at.is_some() || time < d.lease_until {
            return Ok(());
        }
        let mut candidate = state.clone();
        if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == id) {
            a.enabled = false;
        }
        candidate.routes.retain(|_, v| v != id);
        if let Some(d) = candidate.delegated.get_mut(id) {
            d.ended_at = Some(time);
        }
        crate::append_event(&mut candidate, "delegation", Some(id), "ended");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// A seat ended `KEEP_AFTER_LEASE_SECONDS` ago: its credential is removed, then the record
    /// says so. A crash in between removes it again on the next pass.
    pub fn purge_lease(&self, id: &str) -> Result<(), String> {
        let time = now();
        let _writes = self.credential_lock()?;
        {
            let state = self.lock()?;
            let d = state.delegated.get(id).ok_or(NOT_DELEGATED_BY_YOU)?;
            if d.purged()
                || d.ended_at
                    .is_none_or(|e| time < e + KEEP_AFTER_LEASE_SECONDS)
            {
                return Ok(());
            }
        }
        self.vault
            .delete(id)
            .map_err(crate::vault::surface("Credential storage unavailable"))?;
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if let Some(d) = candidate.delegated.get_mut(id) {
            d.purged_at = Some(time);
        }
        crate::append_event(&mut candidate, "delegation", Some(id), "purged");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// The sender calls a seat back: it is switched off here (never renewed again) and its
    /// tokens are answered, a seat whose lease ended included; `undo` switches it on again when
    /// the sender could not take them (an ended one stays off). Answers None once the seat's
    /// credential was removed (`purge_lease`): its sign-in is no longer here.
    pub fn surrender_delegated(
        &self,
        id: &str,
        sender_key: &str,
        undo: bool,
    ) -> Result<Option<crate::Credential>, String> {
        let (purged, ended) = {
            let state = self.lock()?;
            let d = owned(&state, id, sender_key)?;
            (d.purged(), d.ended_at.is_some())
        };
        if purged {
            return Ok(None);
        }
        {
            let mut state = self.lock()?;
            let mut candidate = state.clone();
            let a = candidate
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or(NOT_DELEGATED_BY_YOU)?;
            // W1.2: an undo never turns on a seat the person here turned off.
            a.enabled = undo && !ended && a.off.is_none();
            if !undo {
                candidate.routes.retain(|_, v| v != id);
            }
            // W1.2 (DEC-0027): the call back is recorded on the seat, so nothing reads it from a
            // journal that keeps only its newest entries.
            if let Some(d) = candidate.delegated.get_mut(id) {
                d.surrendered_at = (!undo).then(|| crate::now().max(d.granted_at));
            }
            crate::append_event(
                &mut candidate,
                "delegation",
                Some(id),
                if undo { "restored" } else { "surrendered" },
            );
            self.publish(&mut state, candidate)?;
        }
        self.changed();
        if undo {
            return Ok(None);
        }
        self.stored_credential(id).map(Some)
    }
    /// SB-403 (P2-5): on the sender, records that `id`, which just joined its machine's group,
    /// is a seat delegated there (its id there, lease and the key named).
    pub fn record_delegated_out(&self, id: &str, out: DelegatedOut) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        candidate.delegated_out.insert(id.to_owned(), out);
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// The sender removes a seat it delegated (after a call back), its credential with it.
    pub fn remove_delegated(&self, id: &str, sender_key: &str) -> Result<(), String> {
        owned(&*self.lock()?, id, sender_key)?;
        self.remove_any(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthKind, Credential, MemoryVault, Provider, Store};
    use std::sync::Arc;

    const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const OTHER: &str = "HxweHRwbGhkYFxYVFBMSERAPDg0MCwoJCAcGBQQDAgE=";

    fn oauth(token: &str) -> Credential {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(
                r#"{{"claudeAiOauth":{{"accessToken":"a-{token}","refreshToken":"r-{token}","expiresAt":4102444800000,"scopes":["user:inference"]}}}}"#
            ),
        )
        .unwrap()
    }

    fn store_with_group() -> (tempfile::TempDir, tempfile::TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let path = std::fs::canonicalize(folder.path()).unwrap();
        store.save_project(None, "Org work", &[path], &[]).unwrap();
        (root, folder, store)
    }

    fn pool(store: &Store) -> String {
        store.snapshot().unwrap().projects[0].pool.clone()
    }

    /// Review round 3, P2-1: a seat left on another machine outside a machine group (its copy
    /// there failed its check and could not be removed) loses its record when the account is
    /// used here again, so a later plain move is not called back or extended under its key.
    #[test]
    fn the_record_of_a_seat_left_outside_a_machine_group_goes_when_the_account_is_used_here() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
        let a = store
            .add(
                "one".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                oauth("1"),
            )
            .unwrap();
        store
            .mark_elsewhere_at(&a.id, true, 10, Some("studio.local"))
            .unwrap();
        let out = DelegatedOut {
            remote_id: "0a1b2c3d".into(),
            lease_hours: 48,
            sender_key: KEY.into(),
            sent: Some("0011223344556677".into()),
        };
        store.record_delegated_out(&a.id, out.clone()).unwrap();
        assert_eq!(store.snapshot().unwrap().delegated_out[&a.id], out);
        // A print that is not one is refused.
        let bad = DelegatedOut {
            sent: Some("not a print".into()),
            ..out
        };
        assert!(store.record_delegated_out(&a.id, bad).is_err());
        store.mark_elsewhere(&a.id, false, 20).unwrap();
        assert!(store.snapshot().unwrap().delegated_out.is_empty());
    }

    #[test]
    fn a_key_is_base64_of_32_bytes() {
        assert!(key_valid(KEY));
        assert!(!key_valid("AAEC"));
        assert!(!key_valid(&format!("{}x", &KEY[..43])));
    }

    #[test]
    fn a_delegated_seat_joins_a_group_with_a_lease_and_never_the_default_group() {
        let (_r, _f, store) = store_with_group();
        let refused = store.add_delegated(
            "seat".into(),
            Provider::Claude,
            "default".into(),
            oauth("1"),
            KEY,
            Some("operator"),
            48,
        );
        assert_eq!(refused.unwrap_err(), DELEGATE_GROUP);
        assert_eq!(
            store
                .add_delegated(
                    "seat".into(),
                    Provider::Claude,
                    pool(&store),
                    oauth("1"),
                    KEY,
                    None,
                    0
                )
                .unwrap_err(),
            LEASE_INVALID
        );
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("1"),
                KEY,
                Some("operator"),
                48,
            )
            .unwrap();
        let s = store.snapshot().unwrap();
        let d = &s.delegated[&a.id];
        assert_eq!(d.lease_until - d.granted_at, 48 * 3600);
        assert_eq!(d.pool, a.pool);
        assert!(s.is_delegated(&a.id));
    }

    #[test]
    fn a_delegated_seat_is_never_moved_shared_given_marked_or_removed_here() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("1"),
                KEY,
                None,
                48,
            )
            .unwrap();
        // Leaving it out of its group's accounts would put it in the default group.
        let group = store.snapshot().unwrap().projects[0].clone();
        let folders: Vec<std::path::PathBuf> = group.folders.iter().map(Into::into).collect();
        assert_eq!(
            store
                .save_project(Some(&group.pool), &group.name, &folders, &[])
                .unwrap_err(),
            DELEGATED_STAYS
        );
        assert_eq!(
            store.mark_elsewhere(&a.id, true, now()).unwrap_err(),
            DELEGATED_STAYS
        );
        assert_eq!(
            store.assign_to_agent(&a.id, Some("an agent")).unwrap_err(),
            DELEGATED_NOT_GIVEN
        );
        assert_eq!(store.remove(&a.id).unwrap_err(), DELEGATED_NOT_REMOVED);
        // The account is still there, a guest in its group.
        let s = store.snapshot().unwrap();
        assert!(s.accounts.iter().any(|x| x.id == a.id && x.pool == a.pool));
    }

    #[test]
    fn only_the_sender_extends_calls_back_or_removes_a_seat() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("1"),
                KEY,
                None,
                48,
            )
            .unwrap();
        assert_eq!(
            store.extend_lease(&a.id, OTHER, 48).unwrap_err(),
            NOT_DELEGATED_BY_YOU
        );
        assert_eq!(
            store
                .surrender_delegated(&a.id, OTHER, false)
                .err()
                .as_deref(),
            Some(NOT_DELEGATED_BY_YOU)
        );
        assert_eq!(
            store.remove_delegated(&a.id, OTHER).unwrap_err(),
            NOT_DELEGATED_BY_YOU
        );
        let until = store.extend_lease(&a.id, KEY, 2).unwrap();
        assert_eq!(
            store.snapshot().unwrap().delegated[&a.id].lease_until,
            until
        );
        let tokens = store
            .surrender_delegated(&a.id, KEY, false)
            .unwrap()
            .unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("r-1"));
        assert!(!store.snapshot().unwrap().accounts[0].enabled);
        store.surrender_delegated(&a.id, KEY, true).unwrap();
        assert!(store.snapshot().unwrap().accounts[0].enabled);
        store.remove_delegated(&a.id, KEY).unwrap();
        let s = store.snapshot().unwrap();
        assert!(s.accounts.is_empty() && s.delegated.is_empty());
    }

    #[test]
    fn past_its_lease_a_seat_is_switched_off_kept_and_removed_30_days_later() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("1"),
                KEY,
                None,
                48,
            )
            .unwrap();
        // Not due: nothing happens.
        store.end_lease(&a.id).unwrap();
        assert!(store.stored_credential(&a.id).is_ok());
        let set = |lease_until: i64, ended_at: Option<i64>| {
            let mut state = store.lock().unwrap();
            let mut candidate = state.clone();
            let d = candidate.delegated.get_mut(&a.id).unwrap();
            d.granted_at = now() - 100 * 86_400;
            d.lease_until = lease_until;
            d.ended_at = ended_at;
            *state = candidate;
            store.republish(&state);
        };
        // The lease ran out a minute ago: not due until the owner ran a full read interval past
        // it and past its own start.
        set(now() - 60, None);
        let s = store.snapshot().unwrap();
        assert!(s.leases_due(now(), now() - 3600).is_empty());
        set(now() - LEASE_GRACE_SECONDS - 1, None);
        let s = store.snapshot().unwrap();
        assert!(
            s.leases_due(now(), now()).is_empty(),
            "the owner just started"
        );
        assert_eq!(s.leases_due(now(), now() - 3600), vec![a.id.clone()]);
        store.end_lease(&a.id).unwrap();
        let s = store.snapshot().unwrap();
        assert!(!s.accounts[0].enabled);
        assert!(s.delegated[&a.id].ended_at.is_some());
        // The credential stays: the sender can still call the live sign-in back.
        assert!(store.stored_credential(&a.id).is_ok());
        assert_eq!(
            store.update_fields(&a.id, None, Some(true)).unwrap_err(),
            LEASE_ENDED
        );
        assert_eq!(store.remove(&a.id).unwrap_err(), DELEGATED_NOT_REMOVED);
        // The sender comes back online and extends it: the seat is on again.
        store.extend_lease(&a.id, KEY, 48).unwrap();
        let s = store.snapshot().unwrap();
        assert!(s.accounts[0].enabled && s.delegated[&a.id].ended_at.is_none());
        // Ended for 30 days: the credential is removed, and nothing brings the seat back.
        set(
            now() - 31 * 86_400,
            Some(now() - KEEP_AFTER_LEASE_SECONDS - 1),
        );
        {
            let mut state = store.lock().unwrap();
            state.accounts[0].enabled = false;
            store.republish(&state);
        }
        assert_eq!(
            store.snapshot().unwrap().purges_due(now()),
            vec![a.id.clone()]
        );
        store.purge_lease(&a.id).unwrap();
        assert!(store.stored_credential(&a.id).is_err());
        assert!(store.snapshot().unwrap().delegated[&a.id].purged());
        assert_eq!(
            store.extend_lease(&a.id, KEY, 48).unwrap_err(),
            SIGN_IN_REMOVED
        );
        assert!(store
            .surrender_delegated(&a.id, KEY, false)
            .unwrap()
            .is_none());
        store.remove(&a.id).unwrap();
    }

    #[test]
    fn a_seat_whose_lease_ended_is_still_given_back_with_its_live_sign_in() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("7"),
                KEY,
                None,
                48,
            )
            .unwrap();
        {
            let mut state = store.lock().unwrap();
            let d = state.delegated.get_mut(&a.id).unwrap();
            d.granted_at = now() - 100 * 3600;
            d.lease_until = now() - 1;
            store.republish(&state);
        }
        store.end_lease(&a.id).unwrap();
        let tokens = store
            .surrender_delegated(&a.id, KEY, false)
            .unwrap()
            .unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("r-7"));
        // An undo leaves an ended seat off.
        store.surrender_delegated(&a.id, KEY, true).unwrap();
        assert!(!store.snapshot().unwrap().accounts[0].enabled);
    }

    /// W1.2 (DEC-0027): the person here turned the seat off; the sender's extension of an ended
    /// lease and the undo of a call back keep it off (before, both turned it on).
    #[test]
    fn a_seat_turned_off_here_stays_off_when_its_lease_is_extended_or_a_call_back_undone() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("9"),
                KEY,
                None,
                48,
            )
            .unwrap();
        store.update_fields(&a.id, None, Some(false)).unwrap();
        let since = store.snapshot().unwrap().accounts[0].off_since().unwrap();
        // The undo of a call back.
        store.surrender_delegated(&a.id, KEY, false).unwrap();
        store.surrender_delegated(&a.id, KEY, true).unwrap();
        let s = store.snapshot().unwrap();
        assert!(!s.accounts[0].enabled && s.accounts[0].off_since() == Some(since));
        // A lease that ran out, then extended by the sender.
        {
            let mut state = store.lock().unwrap();
            let d = state.delegated.get_mut(&a.id).unwrap();
            d.granted_at = now() - 100 * 3600;
            d.lease_until = now() - 1;
            store.republish(&state);
        }
        store.end_lease(&a.id).unwrap();
        store.extend_lease(&a.id, KEY, 48).unwrap();
        let s = store.snapshot().unwrap();
        assert!(
            s.delegated[&a.id].ended_at.is_none(),
            "the lease is extended"
        );
        assert!(!s.accounts[0].usable() && s.accounts[0].off_since() == Some(since));
        // Only the person turns it on.
        store.update_fields(&a.id, None, Some(true)).unwrap();
        assert!(store.snapshot().unwrap().accounts[0].usable());
    }

    /// W1.2 (DEC-0027), review round 1 of PR #821: a rename that names no `enabled` (the
    /// window's dialog with its switch untouched) leaves a seat whose lease ran out as it was,
    /// so the sender's extension still brings it back on; it never becomes the person's off.
    #[test]
    fn a_rename_leaves_a_lapsed_seat_off_for_its_lease_and_never_turned_off() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("9"),
                KEY,
                None,
                48,
            )
            .unwrap();
        {
            let mut state = store.lock().unwrap();
            let d = state.delegated.get_mut(&a.id).unwrap();
            d.granted_at = now() - 100 * 3600;
            d.lease_until = now() - 1;
            store.republish(&state);
        }
        store.end_lease(&a.id).unwrap();
        store
            .update_fields(&a.id, Some("renamed".into()), None)
            .unwrap();
        let s = store.snapshot().unwrap();
        assert_eq!(s.accounts[0].label, "renamed");
        assert!(!s.accounts[0].enabled && s.accounts[0].off.is_none());
        assert_eq!(crate::off::availability(&s, &s.accounts[0]), "lease_ended");
        store.extend_lease(&a.id, KEY, 48).unwrap();
        assert!(store.snapshot().unwrap().accounts[0].usable());
    }

    /// W1.2 (DEC-0027), review round 1 of PR #821 (P2-1): a call back is recorded on the seat,
    /// so a journal that rolled over its 256 entries no longer makes the migration read the seat
    /// as turned off by the person; an undo clears it.
    #[test]
    fn a_called_back_seat_is_recorded_on_its_record_and_never_migrated_to_the_persons_off() {
        let (_r, _f, store) = store_with_group();
        let a = store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("9"),
                KEY,
                None,
                48,
            )
            .unwrap();
        store.surrender_delegated(&a.id, KEY, false).unwrap();
        let mut s = store.snapshot().unwrap();
        assert!(s.delegated[&a.id].surrendered_at.is_some());
        assert_eq!(crate::off::availability(&s, &s.accounts[0]), "called_back");
        // Every journal entry about it rolled out.
        s.events.clear();
        assert!(!crate::off::migrate(&mut s, now()));
        assert!(s.accounts[0].off.is_none());
        assert_eq!(crate::off::availability(&s, &s.accounts[0]), "called_back");
        // Undone: the record is cleared and the seat is on again.
        store.surrender_delegated(&a.id, KEY, true).unwrap();
        let s = store.snapshot().unwrap();
        assert!(s.delegated[&a.id].surrendered_at.is_none() && s.accounts[0].usable());
    }

    #[test]
    fn a_group_holding_a_guest_is_not_deleted() {
        let (_r, _f, store) = store_with_group();
        store
            .add_delegated(
                "seat".into(),
                Provider::Claude,
                pool(&store),
                oauth("8"),
                KEY,
                None,
                48,
            )
            .unwrap();
        assert_eq!(
            store.remove_project(&pool(&store)).unwrap_err(),
            GROUP_HOLDS_GUEST
        );
    }

    #[test]
    fn an_account_already_here_never_becomes_a_delegated_one() {
        let (_r, _f, store) = store_with_group();
        let p = pool(&store);
        let a = store
            .add(
                "mine".into(),
                Provider::Claude,
                AuthKind::OAuth,
                p.clone(),
                oauth("2"),
            )
            .unwrap();
        let mut state = store.lock().unwrap();
        let mut candidate = state.clone();
        candidate.delegated.insert(
            a.id.clone(),
            Delegation {
                sender_key: KEY.into(),
                from: None,
                pool: p,
                granted_at: now(),
                lease_until: now() + 10,
                ended_at: None,
                purged_at: None,
                surrendered_at: None,
            },
        );
        assert_eq!(
            store.publish(&mut state, candidate).unwrap_err(),
            DELEGATED_STAYS
        );
    }
}
