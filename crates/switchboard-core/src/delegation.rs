//! SB-403 S2 (DEC-0024): a subscription sign-in another machine delegated to this one.
//!
//! The sender moves a sign-in here with `accounts send --delegate` through this machine's
//! restricted ssh entry (`switchboard ssh-serve`, a forced command that names the sender's key).
//! The account it saves is *delegated*: it carries the sender's key and the group it was
//! delegated into, and it stays a guest here. It serves that group's folders only; it is never
//! sent on, given to an agent, lent or shared, moved to another group (the default one included),
//! marked as used elsewhere or made the ordinary login, and only the sender calls it back or
//! removes it. A lease bounds it: the sender's background read of this machine extends it (by
//! default to 48 hours ahead); past it, the copy is switched off and its credential removed.
//!
//! The rules are checked on every publication of the store (`stays`), so a path that forgets
//! them fails closed with [`DELEGATED_STAYS`] instead of passing a guest's sign-in on.
use crate::{now, pool_valid, uuid_valid, Snapshot, Vault, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};

/// The lease a delegation gets when the sender names none, in hours.
pub const DEFAULT_LEASE_HOURS: u32 = 48;
/// The longest lease a sender may give at once: 30 days.
pub const MAX_LEASE_HOURS: u32 = 720;

pub const DELEGATED_STAYS: &str = "This seat was delegated to this computer by another machine. It stays in the group it was delegated to and is used only here; only that machine calls it back.";
pub const DELEGATED_NOT_SENT: &str = "This seat was delegated to this computer by another machine, so it cannot be moved or handed on. Only that machine calls it back.";
pub const DELEGATED_NOT_GIVEN: &str =
    "This seat was delegated to this computer by another machine, so it is not given to an agent.";
pub const DELEGATED_NOT_REMOVED: &str = "This seat was delegated to this computer by another machine. Ask that machine to call it back; it can be removed here once its lease has run out.";
pub const DELEGATED_NOT_NATIVE: &str = "This seat was delegated to this computer by another machine. It serves its group's sessions only and never becomes the ordinary Claude Code or Codex sign-in.";
pub const NOT_DELEGATED_BY_YOU: &str =
    "This computer holds no seat with that id that the calling machine delegated.";
pub const LEASE_ENDED: &str = "This seat's lease ran out, so its sign-in was removed from this computer. The machine that delegated it calls it back, then delegates it again.";
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
    /// When its lease ran out and its credential was removed here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
}

impl Delegation {
    /// The lease is over (by its time, or marked so).
    pub fn ended(&self, now: i64) -> bool {
        self.ended_at.is_some() || now >= self.lease_until
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
        {
            return invalid();
        }
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
                if o.sender_key != d.sender_key
                    || o.pool != d.pool
                    || o.granted_at != d.granted_at
                    || (o.ended_at.is_some() && d.ended_at.is_none())
                {
                    return Err(DELEGATED_STAYS.into());
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
    /// The delegated seats whose lease is over at `now` but not yet ended here.
    pub fn leases_due(&self, now: i64) -> Vec<String> {
        self.delegated
            .iter()
            .filter(|(_, d)| d.ended_at.is_none() && now >= d.lease_until)
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
        };
        self.save_delegated(label, provider, pool, credential, delegation)
    }
    /// The sender extends a seat's lease to `hours` from now (shorter or longer). An ended lease
    /// is not revived: the sender calls the seat back and delegates it again.
    pub fn extend_lease(&self, id: &str, sender_key: &str, hours: u32) -> Result<i64, String> {
        if !lease_valid(hours) {
            return Err(LEASE_INVALID.into());
        }
        let time = now();
        let mut state = self.lock()?;
        let d = owned(&state, id, sender_key)?;
        if d.ended(time) {
            return Err(LEASE_ENDED.into());
        }
        let until = time + i64::from(hours) * 3600;
        let mut candidate = state.clone();
        if let Some(d) = candidate.delegated.get_mut(id) {
            d.lease_until = until.max(d.granted_at + 1);
        }
        // A lease's time is not worth a write of its own every 15 minutes, but a lease is what
        // keeps the seat working: it is written, as any other change.
        self.publish(&mut state, candidate)?;
        Ok(until)
    }
    /// A seat whose lease is over: its credential is removed, then the account is switched off
    /// and marked ended. A crash in between leaves the record to end again on the next pass.
    pub fn end_lease(&self, id: &str) -> Result<(), String> {
        let time = now();
        let _writes = self.credential_lock()?;
        {
            let state = self.lock()?;
            let d = state.delegated.get(id).ok_or(NOT_DELEGATED_BY_YOU)?;
            if d.ended_at.is_some() || time < d.lease_until {
                return Ok(());
            }
        }
        self.vault
            .delete(id)
            .map_err(crate::vault::surface("Credential storage unavailable"))?;
        let mut state = self.lock()?;
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
    /// The sender calls a seat back: it is switched off here (never renewed again) and its
    /// tokens are answered; `undo` switches it on again when the sender could not take them.
    /// Answers None for a seat whose lease ended: its sign-in is no longer here.
    pub fn surrender_delegated(
        &self,
        id: &str,
        sender_key: &str,
        undo: bool,
    ) -> Result<Option<crate::Credential>, String> {
        let time = now();
        let ended = {
            let state = self.lock()?;
            owned(&state, id, sender_key)?.ended(time)
        };
        if ended {
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
            a.enabled = undo;
            if !undo {
                candidate.routes.retain(|_, v| v != id);
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
    fn past_its_lease_a_seat_is_switched_off_and_its_credential_removed() {
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
        // Due: the lease is set in the past by hand, as time passing would.
        {
            let mut state = store.lock().unwrap();
            let mut candidate = state.clone();
            let d = candidate.delegated.get_mut(&a.id).unwrap();
            d.granted_at -= 100 * 3600;
            d.lease_until = now() - 1;
            *state = candidate;
            store.republish(&state);
        }
        assert_eq!(
            store.snapshot().unwrap().leases_due(now()),
            vec![a.id.clone()]
        );
        store.end_lease(&a.id).unwrap();
        let s = store.snapshot().unwrap();
        assert!(!s.accounts[0].enabled);
        assert!(s.delegated[&a.id].ended_at.is_some());
        assert!(store.stored_credential(&a.id).is_err());
        // Switching it on again is refused; extending it too; the sender's call back answers
        // that the sign-in is gone, and it may be removed here now.
        assert_eq!(
            store.update_fields(&a.id, None, Some(true)).unwrap_err(),
            LEASE_ENDED
        );
        assert_eq!(store.extend_lease(&a.id, KEY, 48).unwrap_err(), LEASE_ENDED);
        assert!(store
            .surrender_delegated(&a.id, KEY, false)
            .unwrap()
            .is_none());
        store.remove(&a.id).unwrap();
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
            },
        );
        assert_eq!(
            store.publish(&mut state, candidate).unwrap_err(),
            DELEGATED_STAYS
        );
    }
}
