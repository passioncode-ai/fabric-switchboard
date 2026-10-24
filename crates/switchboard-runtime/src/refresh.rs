//! Refresh of **inactive** Claude OAuth accounts, as Claude Swap does (PLAN-0.5 D-2).
//! The identity signed in to the ordinary Claude Code is never refreshed here: Claude Code
//! owns that lineage, and a second refresher would rotate its token away. Callers hold the
//! owner's mutation lock, so an activation cannot hand Claude Code a token mid-grant.
use crate::NativeSources;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use switchboard_core::{
    Account, AuthKind, Credential, ExternalIdentity, Provider, Snapshot, Store,
};

pub(crate) const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub(crate) const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
/// Claude Code's public OAuth client, the one every captured Claude login was issued to.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// Refresh this long before expiry; the monitor checks each account every 180 seconds.
pub(crate) const MARGIN_SECONDS: i64 = 600;
const BACKOFF_FIRST: i64 = 60;
const BACKOFF_MAX: i64 = 1800;
const RESPONSE_CAP: usize = 64 * 1024;
/// An identity seen in the ordinary Claude Code this recently may still be held by a running
/// `claude` process that has not yet re-read the Keychain; it is treated as active.
pub(crate) const RECENTLY_ACTIVE_SECONDS: i64 = 900;
pub(crate) const SIGN_IN: &str = "This account's sign-in has ended. Sign in to it again.";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Nothing to do: not Claude OAuth, no refresh token, not near expiry, or waiting out a backoff.
    NotNeeded,
    /// The account is the ordinary Claude Code sign-in, or that sign-in cannot be read.
    Active,
    Refreshed,
    /// The provider rejected the lineage (`invalid_grant`); only a new sign-in recovers it.
    SignInRequired,
    Transient,
}

/// A renewal's successor and the account the token endpoint said it was issued to.
#[derive(Clone)]
pub(crate) struct Kept {
    refreshed: Credential,
    owner: Option<String>,
}

type SignInCache = Option<(u64, Vec<(String, String)>, Vec<String>)>;
/// In-memory refresh bookkeeping of one owner process.
pub(crate) struct RefreshState {
    endpoint: Mutex<String>,
    /// Account id → hash of the refresh token the provider rejected.
    dead: Mutex<HashMap<String, String>>,
    /// SB-182: the last `sign_in_required` answer, with the store's credential write count
    /// (PRD-01) and the rejected lineages it was computed from.
    sign_in_cache: Mutex<SignInCache>,
    /// Account id → (next attempt, current delay) after a transient failure.
    backoff: Mutex<HashMap<String, (i64, i64)>>,
    /// Identity (`account_id`) → last time it was seen signed in to the ordinary Claude Code.
    seen_active: Mutex<HashMap<String, i64>>,
    profile_endpoint: Mutex<String>,
    /// Where rejected lineages are remembered across restarts (fingerprints only).
    journal: Mutex<Option<std::path::PathBuf>>,
    /// Identities (`account_id`) a running Claude Swap holds: it renews them, Switchboard does not.
    /// Identity (`account_id`) → its email, for the accounts a running Claude Swap holds.
    swap_held: Mutex<HashMap<String, Option<String>>>,
    /// The refresh token a grant spent → its successor, until every holder has it. Shared with
    /// the grant task, which records the successor before anyone can drop it.
    stash: Arc<Mutex<HashMap<String, Kept>>>,
    /// Refresh-token fingerprint → the identity `/api/oauth/profile` reported for it.
    owners: Mutex<HashMap<String, ExternalIdentity>>,
    /// The lineage last asked about without an answer, and when: asked again after a minute.
    profile_tried: Mutex<Option<(String, i64)>>,
    /// When the token endpoint last refused the client itself (`invalid_client`).
    client_rejected: Mutex<Option<i64>>,
    /// Claude Swap switches Claude Code by itself; native rotation leaves switching to it.
    swap_switching: std::sync::atomic::AtomicBool,
    /// Claude Swap ran at the monitor's last look.
    swap_running: std::sync::atomic::AtomicBool,
    /// False for the offline CLI and a `--data-dir` owner: they never spend a refresh token.
    grants: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Refresh tokens a grant is spending right now (the grant outlives a cancelled caller).
    in_flight: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Which stored account holds a refresh token, as of a store change count: the scan of
    /// every stored credential runs again only after credentials changed (lifecycle LC-08).
    holders: Mutex<Option<(u64, Holders)>>,
}
/// Refresh-token fingerprint → the stored account holding it.
type Holders = HashMap<String, Option<String>>;
impl Default for RefreshState {
    fn default() -> Self {
        // Unit tests never reach the provider: the discard port refuses at once.
        Self::at(
            if cfg!(test) {
                "http://127.0.0.1:9/token"
            } else {
                TOKEN_URL
            }
            .into(),
        )
    }
}
impl RefreshState {
    /// The first stored Claude OAuth account (in store order) holding `token`. The map from
    /// token fingerprint to holder is rebuilt only when what it reads changed
    /// (`credentials_key`: a credential written, or an account's identity or kind; N1 round 1:
    /// not a project save), and built outside the map's lock.
    fn holder(&self, store: &Store, snapshot: &Snapshot, token: &str) -> Option<String> {
        let key = credentials_key(store, snapshot);
        let cached = self
            .holders
            .lock()
            .ok()?
            .as_ref()
            .filter(|(at, _)| *at == key)
            .map(|(_, map)| map.get(&fingerprint(token)).cloned().flatten());
        if let Some(answer) = cached {
            return answer;
        }
        let mut map = HashMap::new();
        // N1 round 3: a read that failed (a `security` timeout, a locked keychain) leaves this
        // answer incomplete; the map is kept only when every read succeeded or found no account.
        let mut certain = true;
        for account in snapshot
            .accounts
            .iter()
            .filter(|a| a.provider == Provider::Claude && a.kind == AuthKind::OAuth)
        {
            let held = match store.stored_credential(&account.id) {
                Ok(credential) => credential.refresh_token,
                Err(e) if e == switchboard_core::ACCOUNT_NOT_FOUND => None,
                Err(_) => {
                    certain = false;
                    None
                }
            };
            if let Some(held) = held {
                // One token held by copies of two different accounts names nobody: an
                // interrupted switch filed it under the wrong one, and the first copy read
                // must not decide whose it is.
                let who = account
                    .external_identity
                    .as_ref()
                    .and_then(|i| i.account_id.clone());
                map.entry(fingerprint(&held))
                    .and_modify(|seen: &mut Option<String>| {
                        let other = seen.as_ref().and_then(|id| {
                            snapshot
                                .accounts
                                .iter()
                                .find(|a| &a.id == id)
                                .and_then(|a| a.external_identity.as_ref())
                                .and_then(|i| i.account_id.clone())
                        });
                        if other != who {
                            *seen = None;
                        }
                    })
                    .or_insert_with(|| Some(account.id.clone()));
            }
        }
        let answer = map.get(&fingerprint(token)).cloned().flatten();
        // A credential written while these were read may have changed one: keep nothing then.
        if certain && credentials_key(store, snapshot) == key {
            if let Ok(mut holders) = self.holders.lock() {
                *holders = Some((key, map));
            }
        }
        answer
    }
    pub(crate) fn at(endpoint: String) -> Self {
        Self {
            endpoint: Mutex::new(endpoint),
            dead: Mutex::new(HashMap::new()),
            sign_in_cache: Mutex::new(None),
            backoff: Mutex::new(HashMap::new()),
            seen_active: Mutex::new(HashMap::new()),
            profile_endpoint: Mutex::new(
                if cfg!(test) {
                    "http://127.0.0.1:9/profile"
                } else {
                    PROFILE_URL
                }
                .into(),
            ),
            owners: Mutex::new(HashMap::new()),
            profile_tried: Mutex::new(None),
            client_rejected: Mutex::new(None),
            swap_switching: Default::default(),
            swap_running: Default::default(),
            grants: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            in_flight: Default::default(),
            stash: Arc::new(Mutex::new(HashMap::new())),
            swap_held: Mutex::new(Default::default()),
            journal: Mutex::new(None),
            holders: Mutex::new(None),
        }
    }
    /// The offline CLI's state: no grant is ever made; rejected lineages come from the owner's
    /// journal, which it reads but does not need to write.
    pub(crate) fn offline(journal: std::path::PathBuf) -> Self {
        let state = Self::default();
        state.set_grants(false);
        state.remember_in(journal);
        state
    }
    /// Remembers rejected lineages in `path` (a private file in the data folder) and loads the
    /// ones recorded before: a restart must not spend one more grant on a dead token.
    pub(crate) fn remember_in(&self, path: std::path::PathBuf) {
        if let Ok(bytes) = switchboard_core::private_fs::read_private(&path, 256 * 1024) {
            if let Ok(saved) = serde_json::from_slice::<HashMap<String, String>>(&bytes) {
                if let Ok(mut dead) = self.dead.lock() {
                    dead.extend(saved);
                }
            }
        }
        if let Ok(mut journal) = self.journal.lock() {
            *journal = Some(path);
        }
    }
    fn persist_dead(&self) {
        let (Ok(journal), Ok(dead)) = (self.journal.lock(), self.dead.lock()) else {
            return;
        };
        if let (Some(path), Ok(bytes)) = (journal.as_ref(), serde_json::to_vec(&*dead)) {
            let _ = switchboard_core::private_fs::private_write(path, &bytes);
        }
    }
    #[cfg(test)]
    pub(crate) fn set_endpoint(&self, endpoint: String) {
        *self.endpoint.lock().unwrap() = endpoint;
    }
    #[cfg(test)]
    pub(crate) fn set_profile_endpoint(&self, endpoint: String) {
        *self.profile_endpoint.lock().unwrap() = endpoint;
    }
    /// False once the drain began: nothing may spend a refresh token any more (SB-159 too).
    pub(crate) fn grants_allowed(&self) -> bool {
        self.grants.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// The same switch, for a task that outlives the pass that spawned it.
    pub(crate) fn grants_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.grants.clone()
    }
    pub(crate) fn set_grants(&self, allowed: bool) {
        self.grants
            .store(allowed, std::sync::atomic::Ordering::SeqCst);
    }
    /// True while a grant is spending the refresh token `id` holds.
    /// SB-179: waits until no grant spends `token`, at most `deadline`; false when one still does.
    pub(crate) async fn token_released(&self, token: &str, deadline: Duration) -> bool {
        let until = std::time::Instant::now() + deadline;
        loop {
            if self
                .in_flight
                .lock()
                .map(|f| !f.contains(token))
                .unwrap_or(true)
            {
                return true;
            }
            if std::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    /// SB-179: waits until no grant is on the network, at most `deadline`; false when one still
    /// is. A grant's task records its successor before it unmarks its token.
    pub(crate) async fn grants_ended(&self, deadline: Duration) -> bool {
        let until = std::time::Instant::now() + deadline;
        loop {
            if self.in_flight.lock().map(|f| f.is_empty()).unwrap_or(true) {
                return true;
            }
            if std::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub(crate) fn in_flight(&self, store: &Store, id: &str) -> bool {
        store
            .stored_credential(id)
            .ok()
            .and_then(|c| c.refresh_token)
            .is_some_and(|token| self.in_flight.lock().is_ok_and(|f| f.contains(&token)))
    }
    pub(crate) fn set_swap_activity(&self, running: bool, switching: bool) {
        self.swap_running
            .store(running, std::sync::atomic::Ordering::SeqCst);
        self.swap_switching
            .store(switching, std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(test)]
    pub(crate) fn set_swap_switching(&self, switching: bool) {
        self.set_swap_activity(switching, switching);
    }
    #[cfg(test)]
    pub(crate) fn hold_for_test(&self, account: &str) {
        self.swap_held
            .lock()
            .unwrap()
            .insert(account.into(), Some(format!("{account}@example.invalid")));
    }
    pub(crate) fn swap_switching(&self) -> bool {
        self.swap_switching
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    /// True while renewals are held because the provider refused the client itself.
    pub(crate) fn renewal_blocked(&self) -> bool {
        self.client_rejected
            .lock()
            .ok()
            .and_then(|at| *at)
            .is_some_and(|at| (0..CLIENT_HOLD_SECONDS).contains(&(now() - at)))
    }
    fn client_refused(&self, refused: bool) {
        if let Ok(mut at) = self.client_rejected.lock() {
            *at = refused.then(now);
        }
    }
    #[cfg(test)]
    pub(crate) fn mark_dead_for_test(&self, store: &Store, id: &str) {
        let token = store.stored_credential(id).unwrap().refresh_token.unwrap();
        remember_dead(self, id, &token);
    }
    #[cfg(test)]
    pub(crate) fn set_owner(&self, token: &str, owner: ExternalIdentity) {
        self.owners
            .lock()
            .unwrap()
            .insert(fingerprint(token), owner);
    }
    fn owner(&self, token: &str) -> Option<ExternalIdentity> {
        self.owners
            .lock()
            .ok()
            .and_then(|o| o.get(&fingerprint(token)).cloned())
    }
    /// What `/api/oauth/profile` would have answered for `token`, for tests that never reach it.
    #[cfg(test)]
    pub(crate) fn learn_owner(&self, token: &str, owner: ExternalIdentity) {
        if let Ok(mut owners) = self.owners.lock() {
            owners.insert(fingerprint(token), owner);
        }
    }
    /// Records the identity the ordinary Claude Code uses now (from any observation).
    pub(crate) fn note_active(&self, identity: &ExternalIdentity) {
        if let (Some(account), Ok(mut seen)) =
            (identity.account_id.as_ref(), self.seen_active.lock())
        {
            seen.insert(account.clone(), now());
        }
    }
    fn recently_active(&self, account: &Account, time: i64) -> bool {
        let Some(id) = account
            .external_identity
            .as_ref()
            .and_then(|i| i.account_id.as_ref())
        else {
            return false;
        };
        self.seen_active
            .lock()
            .ok()
            .and_then(|seen| seen.get(id).copied())
            .is_some_and(|at| time >= at && time - at < RECENTLY_ACTIVE_SECONDS)
    }
    /// True while the stored lineage of `id` is the one the provider rejected.
    pub(crate) fn is_dead(&self, store: &Store, id: &str) -> bool {
        let Ok(credential) = store.stored_credential(id) else {
            return false;
        };
        let Some(token) = credential.refresh_token.as_deref() else {
            return false;
        };
        self.dead
            .lock()
            .ok()
            .and_then(|d| d.get(id).cloned())
            .is_some_and(|hash| hash == fingerprint(token))
    }
    /// Accounts whose stored lineage the provider rejected and nothing has replaced since.
    /// SB-182: the answer is kept while neither the stored credentials
    /// (`Store::credential_changes`, PRD-01: not `Store::changes`, which every project save moves)
    /// nor the rejected lineages changed. `recovery`, which Fabric Dashboards reads every 15 s,
    /// read every rejected account's credential from the vault each time, and took up to 18 s
    /// on 2026-10-23 with six of them.
    pub(crate) fn sign_in_required(&self, store: &Store) -> Vec<String> {
        let dead: Vec<(String, String)> = match self.dead.lock() {
            Ok(dead) => {
                let mut pairs: Vec<_> = dead.iter().map(|(i, h)| (i.clone(), h.clone())).collect();
                pairs.sort();
                pairs
            }
            Err(_) => return vec![],
        };
        if dead.is_empty() {
            return vec![];
        }
        let changes = store.credential_changes();
        let mut previous = None;
        if let Ok(cache) = self.sign_in_cache.lock() {
            if let Some((at, of, ids)) = cache.as_ref() {
                if *at == changes && *of == dead {
                    return ids.clone();
                }
                previous = Some((of.clone(), ids.clone()));
            }
        }
        // N1 rounds 1 and 3: an earlier answer speaks for a rejected lineage only if it was
        // computed for that very lineage (`of` holds the same account and fingerprint).
        let kept = |id: &String, hash: &String| {
            previous
                .as_ref()
                .and_then(|(of, ids): &(Vec<(String, String)>, Vec<String>)| {
                    of.iter()
                        .any(|(i, h)| i == id && h == hash)
                        .then(|| ids.contains(id))
                })
        };
        // PRD-01: each read may wait seconds on the Keychain; off the async worker it shares.
        // N1 round 1: a read that failed (a `security` timeout, a locked keychain) says nothing
        // about the lineage: the account keeps the answer it had for that lineage, and nothing
        // is cached. N1 round 3: with no such answer the account needs a sign-in, so an error
        // never hides one.
        let (ids, certain) = crate::blocking::run(|| {
            let mut certain = true;
            let ids: Vec<String> = dead
                .iter()
                .filter(|(id, hash)| match store.stored_credential(id) {
                    Ok(c) => {
                        c.refresh_token.as_deref().map(fingerprint).as_deref()
                            == Some(hash.as_str())
                    }
                    Err(e) if e == switchboard_core::ACCOUNT_NOT_FOUND => false,
                    Err(_) => {
                        certain = false;
                        kept(id, hash).unwrap_or(true)
                    }
                })
                .map(|(id, _)| id.clone())
                .collect();
            (ids, certain)
        });
        // A credential stored while these were read may have replaced one: keep nothing then.
        if certain && store.credential_changes() == changes {
            if let Ok(mut cache) = self.sign_in_cache.lock() {
                *cache = Some((changes, dead, ids.clone()));
            }
        }
        ids
    }
}

/// N1 round 1 (seam tier): what a cache of vault reads over `snapshot`'s accounts depends on:
/// the store's credential writes (`Store::credential_changes`) and each account's id, provider,
/// kind, enabled flag and captured identity. A project save, a policy or a quota change leaves
/// it as it is; `Store::changes` moved with each of them.
pub(crate) fn credentials_key(store: &Store, snapshot: &Snapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    store.credential_changes().hash(&mut h);
    for a in &snapshot.accounts {
        a.id.hash(&mut h);
        a.provider.as_str().hash(&mut h);
        format!("{:?}", a.kind).hash(&mut h);
        a.enabled.hash(&mut h);
        format!("{:?}", a.external_identity).hash(&mut h);
    }
    h.finish()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
fn fingerprint(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// Due when the access token is expired or within the margin, and a refresh token exists.
pub(crate) fn due(credential: &Credential, time: i64) -> bool {
    credential.refresh_token.is_some()
        && credential
            .expires_at
            .is_some_and(|t| t - time <= MARGIN_SECONDS)
}

/// When a credential comes due for renewal; None: it cannot be renewed (no refresh token or
/// no expiry).
pub(crate) fn due_at(credential: &Credential) -> Option<i64> {
    credential.refresh_token.as_ref()?;
    credential.expires_at.map(|t| t - MARGIN_SECONDS)
}

/// Refreshes one account if it is inactive and due (or `force`, after a 401).
pub(crate) async fn ensure_fresh(
    store: &Store,
    native: NativeSources,
    state: &RefreshState,
    id: &str,
    force: bool,
) -> Outcome {
    ensure_fresh_with(store, native, state, id, force, None, "renewal").await
}
/// As `ensure_fresh`, for a caller that does not hold the owner transaction (SB-179): `lock` is
/// taken for the checks and the token's reservation, released while the grant is on the network,
/// and taken again to store its successor. A caller that holds the transaction passes None.
/// While the grant runs, its token is in flight: a native activation of the account is refused
/// (`activate_native`), a second grant of it too, and a credential stored meanwhile (a new
/// sign-in) keeps its place, since the successor is stored only where the spent token still is
/// (`adopt_refreshed_for`).
pub(crate) async fn ensure_fresh_with(
    store: &Store,
    native: NativeSources,
    state: &RefreshState,
    id: &str,
    force: bool,
    lock: Option<&tokio::sync::Mutex<()>>,
    step: &'static str,
) -> Outcome {
    // Each span under the transaction is logged when long (`transaction_held`, SB-163).
    let mut guard = match lock {
        Some(lock) => Some(crate::hold_on(lock, step).await),
        None => None,
    };
    let step_due = reserve_due(store, native, state, id, force);
    // PRD-02: the mark ends the phase, so it follows the checks it names.
    if let Some(held) = guard.as_mut() {
        held.phase("checks");
    }
    let (result, refresh_token) = match step_due {
        Step::Done(outcome) => return outcome,
        // SB-179: a caller that holds the transaction (a launch, a switch) no longer finds the
        // background renewal waiting behind it: it waits for that grant, which needs no
        // transaction to keep its successor, and takes the successor.
        Step::InFlight(token) => {
            if lock.is_none() && state.token_released(&token, IN_FLIGHT_WAIT).await {
                if let Some(outcome) = adopt_stash(store, state, id) {
                    return outcome;
                }
            }
            (Err(Failure::Transient), token)
        }
        Step::Refused(failure, token) => (Err(failure), token),
        Step::Grant(reservation, token) => {
            drop(guard);
            let result = reservation.grant().await;
            let mut guard = match lock {
                Some(lock) => Some(crate::hold_on(lock, step).await),
                None => None,
            };
            let outcome = granted(store, state, id, &token, result);
            if let Some(held) = guard.as_mut() {
                held.phase("store");
            }
            return outcome;
        }
    };
    granted(store, state, id, &refresh_token, result)
}
/// What a grant's end means for the account, under the owner transaction.
fn granted(
    store: &Store,
    state: &RefreshState,
    id: &str,
    refresh_token: &str,
    result: Result<Kept, Failure>,
) -> Outcome {
    let time = now();
    match result {
        Ok(kept) => {
            if let Ok(mut backoff) = state.backoff.lock() {
                backoff.remove(id);
            }
            // The token endpoint names whose token it issued: only that account's copies get
            // the successor, and a copy of another account holding it is a dead lineage here
            // (Claude Swap `autoswitch.py:836-877`).
            // A sign-in stored for this account while the grant was out (its token is neither
            // the spent one nor the successor) stands: the successor is filed only where the
            // spent token still is, never over that sign-in through its owner's copies.
            let replaced = store
                .stored_credential(id)
                .ok()
                .and_then(|c| c.refresh_token)
                .is_some_and(|t| {
                    t != refresh_token && Some(&t) != kept.refreshed.refresh_token.as_ref()
                });
            settled_as(store, state, id, refresh_token, &kept, !replaced)
        }
        Err(Failure::Permanent) => {
            remember_dead(state, id, refresh_token);
            // A sign-in stored while the grant was out replaced the rejected lineage (SB-179):
            // the account does not need one.
            if state.is_dead(store, id) {
                Outcome::SignInRequired
            } else {
                Outcome::NotNeeded
            }
        }
        // Not this account's fault and not its backoff: the hold covers every account.
        Err(Failure::Systemic) => Outcome::Transient,
        Err(Failure::Transient) => {
            if let Ok(mut backoff) = state.backoff.lock() {
                let delay = backoff
                    .get(id)
                    .map(|(_, d)| (d * 2).min(BACKOFF_MAX))
                    .unwrap_or(BACKOFF_FIRST);
                backoff.insert(id.to_owned(), (time + delay, delay));
            }
            Outcome::Transient
        }
    }
}
/// How long a caller under the transaction waits for another grant of the same token: the
/// grant's own timeout and a margin.
const IN_FLIGHT_WAIT: Duration = Duration::from_secs(12);
enum Step<'a> {
    Done(Outcome),
    /// Another grant is spending this token now.
    InFlight(String),
    /// No grant could be reserved (grants off, the client refused, the token in flight).
    Refused(Failure, String),
    Grant(Box<Reservation<'a>>, String),
}
/// Everything `ensure_fresh` decides before the network: Ok is the reserved grant and the
/// token it spends, Err the outcome without one.
fn reserve_due<'a>(
    store: &Store,
    native: NativeSources,
    state: &'a RefreshState,
    id: &str,
    force: bool,
) -> Step<'a> {
    let time = now();
    let Some(account) = store
        .snapshot()
        .ok()
        .and_then(|s| s.accounts.into_iter().find(|a| a.id == id))
    else {
        return Step::Done(Outcome::NotNeeded);
    };
    if account.provider != Provider::Claude || account.kind != AuthKind::OAuth {
        return Step::Done(Outcome::NotNeeded);
    }
    let Ok(credential) = store.stored_credential(id) else {
        return Step::Done(Outcome::NotNeeded);
    };
    // A successor that could not be stored yet goes first; the stored token is already spent.
    if let Some(outcome) = adopt_stash(store, state, id) {
        return Step::Done(outcome);
    }
    let Some(refresh_token) = credential.refresh_token.clone() else {
        return Step::Done(Outcome::NotNeeded);
    };
    // A rejected lineage is reported before anything else, due or not.
    if state.is_dead(store, id) {
        return Step::Done(Outcome::SignInRequired);
    }
    // Without a captured identity nothing proves this is not the live Claude Code lineage
    // in an older generation; renewing it could spend a token Claude Code still needs.
    if account
        .external_identity
        .as_ref()
        .is_none_or(|i| i.account_id.is_none())
    {
        return Step::Done(Outcome::NotNeeded);
    }
    // A running Claude Swap renews this lineage itself; a second renewer would spend its token.
    if account
        .external_identity
        .as_ref()
        .and_then(|i| i.account_id.as_ref())
        .is_some_and(|id| {
            state
                .swap_held
                .lock()
                .is_ok_and(|held| held.contains_key(id))
        })
    {
        return Step::Done(Outcome::NotNeeded);
    }
    if !force && !due(&credential, time) {
        return Step::Done(Outcome::NotNeeded);
    }
    if state
        .backoff
        .lock()
        .ok()
        .and_then(|b| b.get(id).copied())
        .is_some_and(|(next, _)| time < next)
    {
        return Step::Done(Outcome::NotNeeded);
    }
    if state.recently_active(&account, time) || is_active(native, state, &account, &credential) {
        return Step::Done(Outcome::Active);
    }
    if state
        .in_flight
        .lock()
        .is_ok_and(|f| f.contains(&refresh_token))
    {
        return Step::InFlight(refresh_token);
    }
    match reserve(state, &credential) {
        Ok(reservation) => Step::Grant(Box::new(reservation), refresh_token),
        Err(failure) => Step::Refused(failure, refresh_token),
    }
}
/// One refresh grant whose successor survives its caller: the grant runs in its own task and
/// records the successor before returning, so a deadline, a dropped control request or a
/// cancelled caller can never lose a token the provider has already spent.
async fn spend(state: &RefreshState, credential: &Credential) -> Result<Kept, Failure> {
    reserve(state, credential)?.grant().await
}
/// A grant whose refresh token is marked in flight; only its task unmarks it.
struct Reservation<'a> {
    state: &'a RefreshState,
    consumed: String,
    credential: Credential,
    release: Option<Release>,
}
/// Marks the token in flight; nothing goes to the network yet. Taken while the caller still
/// holds the owner transaction (SB-179), so no native activation of the account slips in
/// between the checks and the grant.
fn reserve<'a>(
    state: &'a RefreshState,
    credential: &Credential,
) -> Result<Reservation<'a>, Failure> {
    let Some(consumed) = credential.refresh_token.clone() else {
        return Err(Failure::Transient);
    };
    if !state.grants.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(Failure::Transient);
    }
    // The client itself was refused: every grant would fail the same way until it is fixed.
    if state.renewal_blocked() {
        return Err(Failure::Systemic);
    }
    // One grant per refresh token: a second caller would spend the successor's predecessor.
    // Marked last; the reservation's release unmarks it on every exit, a dropped reservation
    // included.
    if !state
        .in_flight
        .lock()
        .is_ok_and(|mut f| f.insert(consumed.clone()))
    {
        return Err(Failure::Transient);
    }
    Ok(Reservation {
        state,
        release: Some(Release(state.in_flight.clone(), consumed.clone())),
        consumed,
        credential: credential.clone(),
    })
}
impl Reservation<'_> {
    async fn grant(mut self) -> Result<Kept, Failure> {
        let state = self.state;
        let endpoint = state.endpoint.lock().map(|e| e.clone()).unwrap_or_default();
        let stash = state.stash.clone();
        let consumed = self.consumed;
        let credential = self.credential;
        let release = self.release.take();
        let result = tokio::spawn(async move {
            // Releases the token on every exit, a panic included, once the successor is kept.
            let _release = release;
            let result = grant(&endpoint, &credential).await;
            if let Ok((refreshed, owner)) = &result {
                if let Ok(mut stash) = stash.lock() {
                    stash.insert(
                        consumed.clone(),
                        Kept {
                            refreshed: refreshed.clone(),
                            owner: owner.clone(),
                        },
                    );
                }
            }
            result.map(|(refreshed, owner)| Kept { refreshed, owner })
        })
        .await
        .unwrap_or(Err(Failure::Transient));
        match &result {
            Err(Failure::Systemic) => state.client_refused(true),
            Ok(_) | Err(Failure::Permanent) => state.client_refused(false),
            Err(Failure::Transient) => {}
        }
        result
    }
}
/// Unmarks a refresh token as in flight when the grant task ends, however it ends.
struct Release(Arc<Mutex<std::collections::HashSet<String>>>, String);
impl Drop for Release {
    fn drop(&mut self) {
        if let Ok(mut f) = self.0.lock() {
            f.remove(&self.1);
        }
    }
}
/// Hands a kept successor to the copies that should have it. Ok: the ids updated; the other
/// holders of the spent token are remembered as dead and the successor is forgotten. Err: the
/// vault refused a write and the successor stays kept for the next pass.
fn settle(
    store: &Store,
    state: &RefreshState,
    consumed: &str,
    kept: &Kept,
) -> Result<Vec<String>, ()> {
    settle_as(store, state, consumed, kept, true)
}
/// `settle`; `owner_fallback` false leaves the named owner's other copies alone (SB-179: a
/// sign-in stored while the grant was out stands, and its expiry is earlier than the
/// successor's by the grant's own length).
fn settle_as(
    store: &Store,
    state: &RefreshState,
    consumed: &str,
    kept: &Kept,
    owner_fallback: bool,
) -> Result<Vec<String>, ()> {
    let (mut updated, others) = store
        .adopt_refreshed_for(
            Provider::Claude,
            consumed,
            &kept.refreshed,
            kept.owner.as_deref(),
        )
        .map_err(|_| ())?;
    // The named owner's own copies hold an older lineage of theirs: the successor is still
    // that account's newest sign-in, never something to drop (audit P2-2).
    if updated.is_empty() && owner_fallback {
        if let Some(owner) = kept.owner.as_deref() {
            updated = file_with_owner(store, owner, &kept.refreshed)?;
        }
    }
    for other in &others {
        remember_dead(state, other, consumed);
    }
    if let Ok(mut stash) = state.stash.lock() {
        stash.remove(consumed);
    }
    Ok(updated)
}
/// Stores `refreshed` in every saved copy of the account `owner` whose own generation expires
/// earlier. Err only when the vault refused a write.
fn file_with_owner(store: &Store, owner: &str, refreshed: &Credential) -> Result<Vec<String>, ()> {
    let mut updated = Vec::new();
    for copy in store
        .snapshot()
        .map_err(|_| ())?
        .accounts
        .into_iter()
        .filter(|a| {
            a.provider == Provider::Claude
                && a.kind == AuthKind::OAuth
                && a.external_identity
                    .as_ref()
                    .and_then(|i| i.account_id.as_deref())
                    == Some(owner)
        })
    {
        let older = store
            .stored_credential(&copy.id)
            .is_ok_and(|stored| stored.expires_at.unwrap_or(0) < refreshed.expires_at.unwrap_or(0));
        if older {
            store
                .upsert(
                    copy.label.clone(),
                    copy.provider,
                    copy.kind,
                    copy.pool.clone(),
                    refreshed.clone(),
                    copy.external_identity.clone(),
                )
                .map_err(|_| ())?;
            updated.push(copy.id);
        }
    }
    Ok(updated)
}
/// What settling a successor means for the account `id` that held the spent token.
fn settled(store: &Store, state: &RefreshState, id: &str, consumed: &str, kept: &Kept) -> Outcome {
    settled_as(store, state, id, consumed, kept, true)
}
fn settled_as(
    store: &Store,
    state: &RefreshState,
    id: &str,
    consumed: &str,
    kept: &Kept,
    owner_fallback: bool,
) -> Outcome {
    match settle_as(store, state, consumed, kept, owner_fallback) {
        Ok(updated) if updated.iter().any(|u| u == id) => Outcome::Refreshed,
        Ok(_) if state.is_dead(store, id) => Outcome::SignInRequired,
        // Its credential changed meanwhile (a new sign-in); nothing more to do for it.
        Ok(_) => Outcome::NotNeeded,
        Err(()) => Outcome::Transient,
    }
}
/// Stores a kept successor of the token `id` holds. None: nothing kept for it.
pub(crate) fn adopt_stash(store: &Store, state: &RefreshState, id: &str) -> Option<Outcome> {
    let consumed = store.stored_credential(id).ok()?.refresh_token?;
    let kept = state.stash.lock().ok()?.get(&consumed).cloned()?;
    Some(settled(store, state, id, &consumed, &kept))
}
/// SB-179: a grant runs outside the owner transaction, so one that ends while the drain holds
/// it keeps its successor in memory only. The drain stores every kept successor whose spent
/// token an account still holds, before the process ends. Answers how many were stored.
pub(crate) fn adopt_all_kept(store: &Store, state: &RefreshState) -> usize {
    let kept: std::collections::HashSet<String> = match state.stash.lock() {
        Ok(stash) if !stash.is_empty() => stash.keys().cloned().collect(),
        _ => return 0,
    };
    let Ok(snapshot) = store.snapshot() else {
        return 0;
    };
    snapshot
        .accounts
        .iter()
        .filter(|a| a.provider == Provider::Claude && a.kind == AuthKind::OAuth)
        .filter(|a| {
            store
                .stored_credential(&a.id)
                .ok()
                .and_then(|c| c.refresh_token)
                .is_some_and(|token| kept.contains(&token))
        })
        .filter(|a| adopt_stash(store, state, &a.id) == Some(Outcome::Refreshed))
        .count()
}
#[cfg(test)]
pub(crate) fn has_stash(state: &RefreshState, consumed: &str) -> bool {
    state.stash.lock().is_ok_and(|s| s.contains_key(consumed))
}
fn remember_dead(state: &RefreshState, id: &str, token: &str) -> Outcome {
    if let Ok(mut dead) = state.dead.lock() {
        dead.insert(id.to_owned(), fingerprint(token));
    }
    state.persist_dead();
    Outcome::SignInRequired
}

/// True when this lineage belongs to the ordinary Claude Code sign-in — or when that sign-in
/// cannot be read, because refreshing an account Claude Code might hold is the one harm here.
fn is_active(
    native: NativeSources,
    state: &RefreshState,
    account: &Account,
    credential: &Credential,
) -> bool {
    // Never decided on a remembered sign-in: another tool may have switched Claude Code to
    // this very account a moment ago, and renewing it then would spend Claude Code's token.
    crate::external::forget_current();
    match (native.current)(Provider::Claude) {
        Ok(current) => {
            state.note_active(&current.identity);
            account.external_identity.as_ref().is_some_and(|identity| {
                identity.account_id.is_some()
                    && identity.account_id == current.identity.account_id
                    && identity.organization_id == current.identity.organization_id
            }) || (credential.refresh_token.is_some()
                && credential.refresh_token == current.credential.refresh_token)
        }
        Err(error) => !error.starts_with("No current "),
    }
}
pub(crate) enum Failure {
    Permanent,
    Transient,
    /// `invalid_client`: the provider refused Claude Code's client id itself, so no account
    /// can renew and none of their lineages is to blame (Claude Swap `oauth.py:205-214`).
    Systemic,
}

/// One refresh grant. Fixed destination, no redirects, bounded body, no error text kept.
/// One refresh grant: the new credential and the account uuid the endpoint reported, if any.
pub(crate) async fn grant(
    endpoint: &str,
    credential: &Credential,
) -> Result<(Credential, Option<String>), Failure> {
    let refresh_token = credential
        .refresh_token
        .as_deref()
        .ok_or(Failure::Permanent)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| Failure::Transient)?;
    let response = client
        .post(endpoint)
        .json(&json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|_| Failure::Transient)?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Transient)? {
        if body.len() + chunk.len() > RESPONSE_CAP {
            return Err(Failure::Transient);
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        // RFC 6749 §5.2: the verdict is the top-level `error`; anything else may be a blip.
        let failure = match (status, value.get("error").and_then(Value::as_str)) {
            (400 | 401 | 403, Some("invalid_grant")) => Failure::Permanent,
            (400 | 401, Some("invalid_client")) => Failure::Systemic,
            _ => Failure::Transient,
        };
        return Err(failure);
    }
    let owner = value
        .pointer("/account/uuid")
        .and_then(Value::as_str)
        .map(str::to_owned);
    apply(credential, &value, now())
        .map(|c| (c, owner))
        .ok_or(Failure::Transient)
}

/// The refreshed credential: token, expiry, rotated refresh token and scopes, mirrored into
/// the native envelope that activation writes back to Claude Code.
pub(crate) fn apply(old: &Credential, response: &Value, time: i64) -> Option<Credential> {
    let access = response.get("access_token")?.as_str()?.to_owned();
    let expires_in = response
        .get("expires_in")?
        .as_i64()
        .filter(|s| *s > 0 && *s <= 31_536_000)?;
    let refresh = match response.get("refresh_token") {
        None | Some(Value::Null) => old.refresh_token.clone(),
        Some(v) => Some(v.as_str()?.to_owned()),
    };
    let expires_at = time.checked_add(expires_in)?;
    let mut credential = old.clone();
    credential.access_token = access;
    credential.refresh_token = refresh;
    credential.expires_at = Some(expires_at);
    if let Some(oauth) = credential
        .native_context
        .as_mut()
        .and_then(|c| c.get_mut("auth"))
        .and_then(|a| a.get_mut("claudeAiOauth"))
        .and_then(Value::as_object_mut)
    {
        oauth.insert("accessToken".into(), json!(credential.access_token));
        if let Some(refresh) = &credential.refresh_token {
            oauth.insert("refreshToken".into(), json!(refresh));
        }
        oauth.insert("expiresAt".into(), json!(expires_at.saturating_mul(1000)));
        if let Some(scope) = response.get("scope").and_then(Value::as_str) {
            oauth.insert(
                "scopes".into(),
                json!(scope.split_whitespace().collect::<Vec<_>>()),
            );
        }
    }
    Some(credential)
}

/// How long the live token must have been expired before Switchboard renews the account Claude
/// Code is signed in to: by then Claude Code is idle, or it would have renewed it itself.
pub(crate) const IDLE_EXPIRED_SECONDS: i64 = 300;
/// The live item with `refreshed`'s tokens, every other key — MCP OAuth included — unchanged.
pub(crate) fn renewed_item(live: &[u8], refreshed: &Credential) -> Option<Vec<u8>> {
    let mut item: Value = serde_json::from_slice(live).ok()?;
    let oauth = item.get_mut("claudeAiOauth")?.as_object_mut()?;
    oauth.insert("accessToken".into(), json!(refreshed.access_token));
    if let Some(token) = &refreshed.refresh_token {
        oauth.insert("refreshToken".into(), json!(token));
    }
    match refreshed.expires_at {
        Some(at) => oauth.insert("expiresAt".into(), json!(at.saturating_mul(1000))),
        None => oauth.remove("expiresAt"),
    };
    if let Some(scopes) = refreshed
        .native_context
        .as_ref()
        .and_then(|c| c.pointer("/auth/claudeAiOauth/scopes"))
    {
        oauth.insert("scopes".into(), scopes.clone());
    }
    serde_json::to_vec(&item).ok()
}
/// Renews the account the ordinary Claude Code is signed in to, but only once Claude Code has
/// left its token expired for `IDLE_EXPIRED_SECONDS` and only under Claude Code's own locks;
/// the item must be byte-identical under the locks to what was checked, or nothing is sent.
/// Keeps managed sessions and quota checks on that account alive while Claude Code is idle.
pub(crate) async fn renew_idle_live(
    store: &Store,
    native: NativeSources,
    state: &RefreshState,
) -> Outcome {
    let time = now();
    let Ok(live) = (native.current)(Provider::Claude) else {
        return Outcome::NotNeeded;
    };
    let Some(consumed) = live.credential.refresh_token.clone() else {
        return Outcome::NotNeeded;
    };
    // A successor kept from an earlier pass means the live token is already spent: Claude
    // Code needs it now, idle or not.
    let kept = state
        .stash
        .lock()
        .ok()
        .and_then(|stash| stash.get(&consumed).cloned());
    let Ok(copies) = crate::matching_accounts(store, Provider::Claude, &live.identity) else {
        return Outcome::NotNeeded;
    };
    if kept.is_none()
        && (live
            .credential
            .expires_at
            .is_none_or(|t| time - t < IDLE_EXPIRED_SECONDS)
            || copies.is_empty()
            || lineage(store, state, &live.identity, &live.credential) != Lineage::Own)
    {
        return Outcome::NotNeeded;
    }
    let Ok(lock) = (native.live)() else {
        return Outcome::Transient;
    };
    let Ok(Some(before)) = lock.read() else {
        return Outcome::Transient;
    };
    let unchanged =
        crate::external::claude_auth_tokens(&before).is_some_and(|(access, refresh)| {
            access == live.credential.access_token && refresh.as_deref() == Some(&consumed)
        });
    if !unchanged {
        return Outcome::NotNeeded;
    }
    let kept = match kept {
        Some(kept) => kept,
        None => match spend(state, &live.credential).await {
            Ok(kept) => kept,
            Err(Failure::Permanent) => {
                for copy in &copies {
                    remember_dead(state, &copy.id, &consumed);
                }
                return Outcome::SignInRequired;
            }
            Err(Failure::Transient | Failure::Systemic) => return Outcome::Transient,
        },
    };
    let Some(item) = renewed_item(&before, &kept.refreshed) else {
        return Outcome::Transient;
    };
    // A failed write leaves the successor kept; the next pass writes it before anything else.
    // SB-101: the provider has already spent the old token, so the successor goes into the
    // vault now — a quit or crash before the next pass must not leave it only in memory. The
    // kept copy stays, and the next pass still writes Claude Code's item first.
    if lock.write(&item).is_err() {
        let owner = kept
            .owner
            .clone()
            .or_else(|| live.identity.account_id.clone());
        if let Some(owner) = owner {
            let _ = file_with_owner(store, &owner, &kept.refreshed);
        }
        return Outcome::Transient;
    }
    drop(lock);
    let foreign = kept
        .owner
        .as_deref()
        .filter(|owner| live.identity.account_id.as_deref() != Some(*owner));
    if let (Some(owner), Some(token), Ok(mut owners)) = (
        foreign,
        kept.refreshed.refresh_token.as_ref(),
        state.owners.lock(),
    ) {
        // Claude Code keeps it, whoever owns it — dropping it would sign Claude Code out — but
        // it is never filed under the account named in the config.
        owners.insert(
            fingerprint(token),
            ExternalIdentity {
                account_id: Some(owner.to_owned()),
                organization_id: None,
                email: None,
            },
        );
    }
    // The copies get the successor only if it is theirs; otherwise they hold a spent token.
    match settle(store, state, &consumed, &kept) {
        Err(()) => Outcome::Transient,
        Ok(_) if foreign.is_some() => Outcome::SignInRequired,
        Ok(_) => Outcome::Refreshed,
    }
}
/// What the monitor saw of Claude Swap this pass.
pub(crate) enum SwapView {
    NotRunning,
    Unreadable,
    /// Running: the profiles read, and the emails of rows that could not be read this pass.
    Profiles(Vec<crate::external::CapturedProfile>, Vec<String>),
    /// Not running, but its files hold sign-ins it may have renewed before it stopped: take the
    /// newer ones, hold nothing (board SB-21).
    Stopped(Vec<crate::external::CapturedProfile>),
}
/// Whether Claude Swap's files are read this pass: always while it runs; once when Switchboard
/// starts and once when Swap stops (its last renewals are in them); otherwise only when the
/// files changed. Reading is not free — a session sign-in is a `/usr/bin/security` call.
pub(crate) fn swap_read_due(
    running: bool,
    was_running: Option<bool>,
    signature: Option<(u64, u64, u128)>,
    last_signature: Option<(u64, u64, u128)>,
) -> bool {
    running || signature.is_some() && (was_running != Some(false) || signature != last_signature)
}
/// Coexistence with a running Claude Swap (report §P1-5). Records the identities it holds so
/// Switchboard does not renew them, and takes its newer generation of each into every stored
/// copy of that identity — a lineage it renewed would otherwise be spent under Switchboard.
/// Returns how many copies were updated.
pub(crate) fn follow_claude_swap(store: &Store, state: &RefreshState, swap: SwapView) -> usize {
    let profiles = match swap {
        SwapView::NotRunning => {
            if let Ok(mut held) = state.swap_held.lock() {
                held.clear();
            }
            return 0;
        }
        // Running, but its files could not be read this pass (it was writing them): what it
        // held last time it still holds — clearing it would start a second renewer.
        SwapView::Unreadable => return 0,
        SwapView::Profiles(profiles, failed_emails) => (profiles, failed_emails),
        SwapView::Stopped(profiles) => {
            if let Ok(mut held) = state.swap_held.lock() {
                held.clear();
            }
            return adopt_newer(store, state, profiles, None);
        }
    };
    let (profiles, failed_emails) = profiles;
    let mut held = HashMap::new();
    let updated = adopt_newer(store, state, profiles, Some(&mut held));
    if let Ok(mut current) = state.swap_held.lock() {
        // A row that could not be read (Swap rewrites it exactly when it renews that account)
        // is still Swap's: that account stays held. Only that one — a row broken for good, or
        // an account removed from Swap, must not keep anything held forever.
        for (account, email) in current.drain() {
            if email
                .as_deref()
                .is_some_and(|e| failed_emails.iter().any(|f| f.eq_ignore_ascii_case(e)))
            {
                held.entry(account).or_insert(email);
            }
        }
        *current = held;
    }
    updated
}
/// Takes each profile's generation into every stored copy of its identity when it is newer
/// (another refresh token that expires later) and not another account's lineage; `held`
/// collects the identities seen. Returns how many copies were updated.
fn adopt_newer(
    store: &Store,
    state: &RefreshState,
    profiles: Vec<crate::external::CapturedProfile>,
    mut held: Option<&mut HashMap<String, Option<String>>>,
) -> usize {
    let mut updated = 0;
    for profile in profiles {
        let Some(id) = profile.identity.account_id.clone() else {
            continue;
        };
        if let Some(held) = held.as_deref_mut() {
            held.insert(id, profile.identity.email.clone());
        }
        if lineage(store, state, &profile.identity, &profile.credential) == Lineage::Foreign {
            continue;
        }
        let Ok(copies) = crate::matching_accounts(store, Provider::Claude, &profile.identity)
        else {
            continue;
        };
        for copy in copies {
            let Ok(stored) = store.stored_credential(&copy.id) else {
                continue;
            };
            let newer = profile.credential.refresh_token != stored.refresh_token
                && profile.credential.expires_at.unwrap_or(0) > stored.expires_at.unwrap_or(0);
            if newer
                && store
                    .upsert(
                        copy.label.clone(),
                        copy.provider,
                        copy.kind,
                        copy.pool.clone(),
                        profile.credential.clone(),
                        Some(profile.identity.clone()),
                    )
                    .is_ok()
            {
                updated += 1;
            }
        }
    }
    updated
}
pub(crate) const RENEWING: &str =
    "Switchboard is renewing this account's sign-in. Retry in a moment.";
pub(crate) const SWAP_BUSY: &str = "Claude Swap is updating this account. Retry in a moment.";
/// Before switching to `id`: when Claude Swap renews that account, read its profiles now and
/// take a newer generation, so Claude Code never receives a token Swap already spent. Refused
/// while Swap's files cannot be read.
pub(crate) fn catch_up_with_claude_swap(
    store: &Store,
    state: &RefreshState,
    native: NativeSources,
    id: &str,
) -> Result<(), String> {
    let Some((account, email)) = store
        .snapshot()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .and_then(|a| a.external_identity)
        .and_then(|i| i.account_id.map(|account| (account, i.email)))
        .filter(|(account, _)| {
            state
                .swap_held
                .lock()
                .is_ok_and(|h| h.contains_key(account))
        })
    else {
        return Ok(());
    };
    let batch = (native.swap)().map_err(|_| SWAP_BUSY.to_string())?;
    // Its own row is the one Swap is rewriting: the stored copy may be the spent generation.
    let listed = batch
        .profiles
        .iter()
        .any(|p| p.identity.account_id.as_deref() == Some(account.as_str()));
    if !listed
        && email.as_deref().is_some_and(|e| {
            batch
                .failed_emails
                .iter()
                .any(|f| f.eq_ignore_ascii_case(e))
        })
    {
        return Err(SWAP_BUSY.into());
    }
    // Swap stopped since the monitor last looked: take its last generations, hold nothing.
    let view = if state.swap_running.load(std::sync::atomic::Ordering::SeqCst) {
        SwapView::Profiles(batch.profiles, batch.failed_emails)
    } else {
        SwapView::Stopped(batch.profiles)
    };
    follow_claude_swap(store, state, view);
    Ok(())
}
pub(crate) fn swap_held(state: &RefreshState) -> usize {
    state.swap_held.lock().map(|h| h.len()).unwrap_or(0)
}

/// Whose lineage the live Claude Code token is. Claude's credential item carries no account id,
/// so `~/.claude.json` alone cannot say: an interrupted switch or a half-finished `/login` can
/// leave account B's token under account A's name (Claude Swap answers the same question with
/// `/api/oauth/profile`, `switcher.py:6384-6576`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lineage {
    /// A stored copy of this identity holds the refresh token, or the provider said so.
    Own,
    /// The refresh token belongs to another stored account, or the provider named another one.
    Foreign,
    /// Nothing known yet; the provider could not be asked.
    Unresolved,
}
pub(crate) const FOREIGN_LIVE: &str = "The Claude Code sign-in does not match the account named in its settings. Sign in again in Claude Code (claude /login), then retry.";

/// Whose the live token is, as far as Switchboard can tell without the network.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Attribution {
    /// The identity named in Claude Code's settings.
    Own,
    /// Another account: the provider named it, or a stored copy of it holds the token. None when
    /// the holder's identity is not recorded.
    Other(Option<ExternalIdentity>),
    /// Nothing known yet; the provider could not be asked.
    Unresolved,
}
/// The provider's answer (`/api/oauth/profile`, asked by `learn_live_owner`) decides first: it
/// is about this very token, while a stored copy may hold a token an interrupted switch filed
/// under the wrong account. A stored copy decides only when the provider has not answered.
/// Never touches the network.
pub(crate) fn attribution(
    store: &Store,
    state: &RefreshState,
    identity: &ExternalIdentity,
    credential: &Credential,
) -> Attribution {
    let Some(token) = credential.refresh_token.as_deref() else {
        return Attribution::Unresolved;
    };
    if let Some(owner) = state.owner(token) {
        // The same login in another organization is another account.
        return if owner.account_id == identity.account_id
            && (owner.organization_id.is_none()
                || identity.organization_id.is_none()
                || owner.organization_id == identity.organization_id)
        {
            Attribution::Own
        } else {
            Attribution::Other(Some(owner))
        };
    }
    let Ok(snapshot) = store.snapshot() else {
        return Attribution::Unresolved;
    };
    let Some(holder) = state.holder(store, &snapshot, token) else {
        return Attribution::Unresolved;
    };
    let held_by = snapshot
        .accounts
        .iter()
        .find(|a| a.id == holder)
        .and_then(|a| a.external_identity.clone());
    let same = held_by.as_ref().is_some_and(|o| {
        o.account_id.is_some()
            && o.account_id == identity.account_id
            && o.organization_id == identity.organization_id
    });
    if same {
        Attribution::Own
    } else {
        Attribution::Other(held_by)
    }
}
/// Decides from the cached provider answer and stored copies; never touches the network.
pub(crate) fn lineage(
    store: &Store,
    state: &RefreshState,
    identity: &ExternalIdentity,
    credential: &Credential,
) -> Lineage {
    match attribution(store, state, identity, credential) {
        Attribution::Own => Lineage::Own,
        Attribution::Other(_) => Lineage::Foreign,
        Attribution::Unresolved => Lineage::Unresolved,
    }
}
/// After `invalid_client` no grant is tried for an hour; one grant then tests the client again.
const CLIENT_HOLD_SECONDS: i64 = 3600;
/// An unanswered owner question is not repeated sooner (the monitor asks every 10 seconds).
const PROFILE_RETRY_SECONDS: i64 = 60;
pub(crate) const UNCONFIRMED_LIVE: &str = "Switchboard could not confirm which account Claude Code is signed in to. Check the connection, or use Claude Code once, then retry.";
/// Whether the live credential may be filed under `identity`: refused when it is another
/// account's lineage, and when nothing — no stored copy, no provider answer — attributes it.
pub(crate) fn filable(
    store: &Store,
    state: &RefreshState,
    identity: &ExternalIdentity,
    credential: &Credential,
) -> Result<(), String> {
    match lineage(store, state, identity, credential) {
        Lineage::Own => Ok(()),
        Lineage::Foreign => Err(FOREIGN_LIVE.into()),
        Lineage::Unresolved => Err(UNCONFIRMED_LIVE.into()),
    }
}

/// The identity under which a switch files the live Claude Code token before replacing it: the
/// one Claude Code's settings name when the token is its own, the saved account it belongs to
/// otherwise. Refused when nothing attributes it, or when it belongs to an account Switchboard
/// does not hold (writing over it would lose that sign-in for good). Filing it under another
/// account than the settings name needs the provider's word: a stored copy alone may be the
/// misfiled one, and trusting it would overwrite that account's real sign-in in its other pools.
pub(crate) fn owner_to_file(
    store: &Store,
    state: &RefreshState,
    identity: &ExternalIdentity,
    credential: &Credential,
) -> Result<ExternalIdentity, String> {
    let confirmed = credential
        .refresh_token
        .as_deref()
        .is_some_and(|token| state.owner(token).is_some());
    match attribution(store, state, identity, credential) {
        Attribution::Own => Ok(identity.clone()),
        Attribution::Other(Some(_)) if !confirmed => Err(UNCONFIRMED_LIVE.into()),
        // The saved copy's own identity, so its organization is the recorded one.
        Attribution::Other(Some(other)) if other.account_id.is_some() => store
            .snapshot()
            .ok()
            .and_then(|s| {
                s.accounts
                    .into_iter()
                    .filter(|a| a.provider == Provider::Claude)
                    .filter_map(|a| a.external_identity)
                    .find(|i| {
                        i.account_id == other.account_id
                            && (i.organization_id == other.organization_id
                                || other.organization_id.is_none())
                    })
            })
            .ok_or_else(|| FOREIGN_LIVE.into()),
        Attribution::Other(_) => Err(FOREIGN_LIVE.into()),
        Attribution::Unresolved => Err(UNCONFIRMED_LIVE.into()),
    }
}
/// Asks `/api/oauth/profile` once per unknown lineage of the live Claude Code sign-in, outside
/// every lock (5-second timeout). Silent on failure: the answer stays Unresolved.
pub(crate) async fn learn_live_owner(store: &Store, native: NativeSources, state: &RefreshState) {
    let Ok(live) = (native.current)(Provider::Claude) else {
        return;
    };
    let Some(token) = live.credential.refresh_token.clone() else {
        return;
    };
    // A stored copy calling the token another account's may be a misfiled copy: the provider
    // is asked then too, and its answer decides (`attribution`).
    if state.owner(&token).is_some()
        || lineage(store, state, &live.identity, &live.credential) == Lineage::Own
    {
        return;
    }
    let print = fingerprint(&token);
    let time = now();
    if let Ok(mut tried) = state.profile_tried.lock() {
        if tried.as_ref().is_some_and(|(at_print, at)| {
            *at_print == print && (0..PROFILE_RETRY_SECONDS).contains(&(time - at))
        }) {
            return;
        }
        *tried = Some((print.clone(), time));
    }
    let endpoint = state
        .profile_endpoint
        .lock()
        .map(|e| e.clone())
        .unwrap_or_default();
    if let Some(owner) = profile(&endpoint, &live.credential.access_token).await {
        if let Ok(mut owners) = state.owners.lock() {
            if owners.len() > 256 {
                owners.clear();
            }
            owners.insert(fingerprint(&token), owner);
        }
    }
}
async fn profile(endpoint: &str, access_token: &str) -> Option<ExternalIdentity> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let mut response = client
        .get(endpoint)
        .bearer_auth(access_token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if body.len() + chunk.len() > RESPONSE_CAP {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).ok()?;
    let account = value
        .pointer("/account/uuid")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())?;
    Some(ExternalIdentity {
        account_id: Some(account.trim().to_owned()),
        organization_id: value
            .pointer("/organization/uuid")
            .and_then(Value::as_str)
            .map(str::to_owned),
        email: value
            .pointer("/account/email")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use axum::{routing::post, Json, Router};
    use std::sync::{atomic::AtomicUsize, atomic::Ordering, Arc, Mutex};

    const EXPIRED: i64 = 1_000;

    /// A local token endpoint: answers with `reply` and counts grants.
    async fn endpoint(reply: (u16, Value)) -> (String, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = Router::new().route(
            "/token",
            post(move |Json(body): Json<Value>| {
                let counter = counter.clone();
                let reply = reply.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(body["grant_type"], "refresh_token");
                    assert_eq!(body["client_id"], CLIENT_ID);
                    (
                        axum::http::StatusCode::from_u16(reply.0).unwrap(),
                        Json(reply.1),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}/token"), calls)
    }
    fn expired(account: &str) -> Credential {
        let mut credential = credential(account);
        credential.expires_at = Some(EXPIRED);
        credential.refresh_token = Some(format!("{account}-refresh"));
        credential
    }
    fn store_with(root: &std::path::Path, accounts: &[(&str, &str)]) -> Arc<Store> {
        let store = Arc::new(
            Store::open(
                root.to_owned(),
                Arc::new(switchboard_core::MemoryVault::default()),
            )
            .unwrap(),
        );
        for (account, pool) in accounts {
            store
                .upsert(
                    account.to_string(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    pool.to_string(),
                    expired(account),
                    Some(identity(account)),
                )
                .unwrap();
        }
        store
    }
    fn id_of(store: &Store, account: &str, pool: &str) -> String {
        store
            .snapshot()
            .unwrap()
            .accounts
            .into_iter()
            .find(|a| a.label == account && a.pool == pool)
            .unwrap()
            .id
    }
    const SIGNED_IN_A: NativeSources = NativeSources {
        current: signed_in,
        activate: activates,
        live: crate::no_live,
        swap: crate::no_swap,
        ..crate::UNAVAILABLE
    };
    const SIGNED_OUT: NativeSources = NativeSources {
        current: signed_out,
        activate: activates,
        live: crate::no_live,
        swap: crate::no_swap,
        ..crate::UNAVAILABLE
    };
    const UNREADABLE: NativeSources = NativeSources {
        current: unreadable,
        activate: activates,
        live: crate::no_live,
        swap: crate::no_swap,
        ..crate::UNAVAILABLE
    };

    #[tokio::test]
    async fn an_inactive_expired_account_is_refreshed_in_every_pool() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"fresh-access","expires_in":28800,"refresh_token":"fresh-refresh","scope":"user:inference user:profile"}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[("synthetic-b", "default"), ("synthetic-b", "work")],
        );
        let state = RefreshState::at(url);
        let id = id_of(&store, "synthetic-b", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for pool in ["default", "work"] {
            let c = store
                .stored_credential(&id_of(&store, "synthetic-b", pool))
                .unwrap();
            assert_eq!(c.access_token, "fresh-access");
            assert_eq!(c.refresh_token.as_deref(), Some("fresh-refresh"));
            assert!(c.expires_at.unwrap() > now() + 28000);
            let oauth = &c.native_context.unwrap()["auth"]["claudeAiOauth"];
            assert_eq!(oauth["accessToken"], "fresh-access");
            assert_eq!(oauth["refreshToken"], "fresh-refresh");
            assert_eq!(oauth["scopes"], json!(["user:inference", "user:profile"]));
        }
    }
    #[tokio::test]
    async fn the_ordinary_claude_code_account_is_never_refreshed() {
        let (url, calls) = endpoint((200, json!({"access_token":"x","expires_in":60}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-a", "default")]);
        let state = RefreshState::at(url);
        let id = id_of(&store, "synthetic-a", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, true).await,
            Outcome::Active
        );
        // An unreadable current sign-in might be this account: no refresh either.
        assert_eq!(
            ensure_fresh(&store, UNREADABLE, &state, &id, true).await,
            Outcome::Active
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // Signed out of Claude Code: nobody else holds the lineage.
        let (url, calls) = endpoint((200, json!({"access_token":"fresh","expires_in":60}))).await;
        let state = RefreshState::at(url);
        assert_eq!(
            ensure_fresh(&store, SIGNED_OUT, &state, &id, false).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn invalid_grant_requires_sign_in_until_the_credential_is_replaced() {
        let (url, calls) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let id = id_of(&store, "synthetic-b", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::SignInRequired
        );
        assert_eq!(state.sign_in_required(&store), vec![id.clone()]);
        // The dead lineage is not retried.
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, true).await,
            Outcome::SignInRequired
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // A new capture of the account clears the state.
        let mut replaced = expired("synthetic-b");
        replaced.refresh_token = Some("new-login-refresh".into());
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                replaced,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        assert!(state.sign_in_required(&store).is_empty());
    }
    #[tokio::test]
    async fn transient_failures_back_off_and_keep_the_token() {
        let (url, calls) = endpoint((500, json!({"error":"server_error"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let id = id_of(&store, "synthetic-b", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::Transient
        );
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::NotNeeded,
            "inside the backoff window"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-b-refresh")
        );
        assert!(state.sign_in_required(&store).is_empty());
        // A 401 without invalid_grant is not a verdict on the lineage either.
        let (url, _) = endpoint((401, json!({"error":"invalid_client"}))).await;
        let state = RefreshState::at(url);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::Transient
        );
    }
    #[tokio::test]
    async fn a_token_far_from_expiry_is_left_alone() {
        let (url, calls) = endpoint((200, json!({"access_token":"x","expires_in":60}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(
                root.path().to_owned(),
                Arc::new(switchboard_core::MemoryVault::default()),
            )
            .unwrap(),
        );
        let mut fresh = credential("synthetic-b");
        fresh.refresh_token = Some("r".into());
        let account = store
            .upsert(
                "b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                fresh,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let state = RefreshState::at(url);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &account.id, false).await,
            Outcome::NotNeeded
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn rows_without_identity_and_recently_active_identities_are_not_renewed() {
        let (url, calls) = endpoint((200, json!({"access_token":"x","expires_in":60}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let pasted = store
            .add(
                "pasted".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-c"),
            )
            .unwrap();
        let state = RefreshState::at(url);
        assert_eq!(
            ensure_fresh(&store, SIGNED_OUT, &state, &pasted.id, true).await,
            Outcome::NotNeeded,
            "no captured identity"
        );
        let b = id_of(&store, "synthetic-b", "default");
        state.note_active(&identity("synthetic-b"));
        assert_eq!(
            ensure_fresh(&store, SIGNED_OUT, &state, &b, false).await,
            Outcome::Active,
            "just switched away from"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn a_rejected_lineage_is_never_handed_to_claude_code() {
        let (url, _) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await;
        assert!(state.is_dead(&store, &b));
        fn never(
            _: &Credential,
            _: &switchboard_core::ExternalIdentity,
            _: Option<&switchboard_core::ExternalIdentity>,
            _: &mut dyn FnMut(&crate::external::Outgoing<'_>) -> Result<(), String>,
        ) -> Result<(), String> {
            panic!("a dead lineage reached Claude Code")
        }
        let native = NativeSources {
            current: signed_in,
            activate: never,
            live: crate::no_live,
            swap: crate::no_swap,
            ..crate::UNAVAILABLE
        };
        assert_eq!(
            crate::activate_native(&store, &b, None, native, Some(&state)).unwrap_err(),
            SIGN_IN
        );
        // Not yet due by expiry still reports the rejection first.
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::SignInRequired
        );
    }
    /// A vault that refuses writes until told otherwise.
    struct FlakyVault {
        inner: switchboard_core::MemoryVault,
        refuse: std::sync::atomic::AtomicBool,
    }
    impl switchboard_core::Vault for FlakyVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            self.inner.get(id)
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            if self.refuse.load(Ordering::SeqCst) {
                return Err("Native credential storage unavailable".into());
            }
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// A vault that counts reads.
    #[derive(Default)]
    struct CountingVault {
        inner: switchboard_core::MemoryVault,
        gets: AtomicUsize,
    }
    impl switchboard_core::Vault for CountingVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get(id)
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// SB-182: `recovery` read every rejected account's credential from the vault on each call
    /// (Fabric Dashboards calls it every 15 s), up to 18 s on a loaded Mac. The answer is now
    /// kept until a credential is stored or a lineage is rejected, and is still right after one.
    #[test]
    fn rejected_sign_ins_are_read_from_the_vault_only_after_a_change() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault::default());
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        for account in ["synthetic-a", "synthetic-b"] {
            store
                .upsert(
                    account.into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    expired(account),
                    Some(identity(account)),
                )
                .unwrap();
        }
        let (a, b) = (
            id_of(&store, "synthetic-a", "default"),
            id_of(&store, "synthetic-b", "default"),
        );
        let state = RefreshState::default();
        assert!(state.sign_in_required(&store).is_empty());
        remember_dead(&state, &a, "synthetic-a-refresh");
        remember_dead(&state, &b, "synthetic-b-refresh");
        assert_eq!(
            state.sign_in_required(&store),
            sorted(vec![a.clone(), b.clone()])
        );
        let reads = vault.gets.load(Ordering::SeqCst);
        for _ in 0..20 {
            assert_eq!(
                state.sign_in_required(&store),
                sorted(vec![a.clone(), b.clone()])
            );
        }
        assert_eq!(
            vault.gets.load(Ordering::SeqCst),
            reads,
            "no vault read without a change"
        );
        // A new sign-in for b replaces its rejected lineage: b no longer needs one.
        let mut fresh = expired("synthetic-b");
        fresh.refresh_token = Some("signed-in-again".into());
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                fresh,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        // A lineage rejected anew is seen at once, with no credential change.
        let mut dead = state.dead.lock().unwrap();
        dead.remove(&a);
        drop(dead);
        assert!(state.sign_in_required(&store).is_empty());
    }
    struct SlowVault {
        inner: switchboard_core::MemoryVault,
        slow: std::sync::atomic::AtomicBool,
    }
    impl switchboard_core::Vault for SlowVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            if self.slow.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            self.inner.get(id)
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// PRD-02 (iteration 2026-10-24): `transaction_held step=usage_renewal checks=0` with
    /// 14 s held: the `checks` mark came before the checks ran, so their time went unnamed.
    #[tokio::test]
    async fn a_renewals_hold_names_the_time_its_checks_took() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(SlowVault {
            inner: Default::default(),
            slow: Default::default(),
        });
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        let mut fresh = expired("synthetic-a");
        fresh.expires_at = Some(now() + 86_400);
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                fresh,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let id = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        let lock = tokio::sync::Mutex::new(());
        vault.slow.store(true, Ordering::SeqCst);
        ensure_fresh_with(
            &store,
            SIGNED_OUT,
            &state,
            &id,
            false,
            Some(&lock),
            "usage_renewal",
        )
        .await;
        let phases = crate::LAST_HOLD.with(|last| last.borrow().clone());
        let checks = phases
            .iter()
            .find(|(name, _)| *name == "checks")
            .map(|(_, ms)| *ms);
        assert!(
            checks.is_some_and(|ms| ms >= 50),
            "the checks' time is under `checks`: {phases:?}"
        );
    }
    struct FailingVault {
        inner: switchboard_core::MemoryVault,
        gets: AtomicUsize,
        /// Reads of this id fail while `failures` is above zero.
        failing: Mutex<Option<String>>,
        failures: Mutex<usize>,
    }
    impl switchboard_core::Vault for FailingVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            let mut failures = self.failures.lock().unwrap();
            if self.failing.lock().unwrap().as_deref() == Some(id) && *failures > 0 {
                *failures -= 1;
                return Err("Credential storage unavailable".into());
            }
            self.inner.get(id)
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// N1 round 1 (unit tier): a vault read that failed (a `security` timeout, a locked
    /// keychain) read as "not dead", and that answer was cached until the next credential write,
    /// so recovery offered an account whose lineage the provider had rejected. A failed read is
    /// never cached and never hides a sign-in the previous answer needed.
    #[test]
    fn a_failed_vault_read_is_not_cached_and_does_not_hide_a_needed_sign_in() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(FailingVault {
            inner: Default::default(),
            gets: Default::default(),
            failing: Default::default(),
            failures: Default::default(),
        });
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        for account in ["synthetic-a", "synthetic-b"] {
            store
                .upsert(
                    account.into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    expired(account),
                    Some(identity(account)),
                )
                .unwrap();
        }
        let (a, b) = (
            id_of(&store, "synthetic-a", "default"),
            id_of(&store, "synthetic-b", "default"),
        );
        let state = RefreshState::default();
        remember_dead(&state, &a, "synthetic-a-refresh");
        *vault.failing.lock().unwrap() = Some(a.clone());
        *vault.failures.lock().unwrap() = 1;
        // N1 round 3: with no earlier answer to keep, a failed read still names the sign-in.
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().canonicalize().unwrap();
        store.save_project(None, "App", &[folder], &[]).unwrap();
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        let reads = vault.gets.load(Ordering::SeqCst);
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        assert_eq!(
            vault.gets.load(Ordering::SeqCst),
            reads,
            "a good answer is cached"
        );
        // A credential write elsewhere sends it back to the vault; a read that fails then keeps
        // the sign-in the previous answer needed.
        let mut other = expired("synthetic-b");
        other.refresh_token = Some("synthetic-b-new".into());
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                other,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let _ = b;
        *vault.failures.lock().unwrap() = 1;
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
    }
    /// N1 round 3 (adversarial): a failed read while the token holder map is built must not leave
    /// the holder unknown until a credential is written (attribution Unresolved for a stored
    /// token).
    #[test]
    fn a_failed_read_does_not_stick_in_the_token_holders() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(FailingVault {
            inner: Default::default(),
            gets: Default::default(),
            failing: Mutex::new(None),
            failures: Mutex::new(0),
        });
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let a = id_of(&store, "synthetic-a", "default");
        *vault.failing.lock().unwrap() = Some(a.clone());
        *vault.failures.lock().unwrap() = 1;
        let state = RefreshState::default();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(
            state.holder(&store, &snapshot, "synthetic-a-refresh"),
            None,
            "read failed"
        );
        assert_eq!(
            state.holder(&store, &snapshot, "synthetic-a-refresh"),
            Some(a.clone()),
            "the vault answers again: the holder is known"
        );
    }
    /// N1 round 3 (adversarial): a lineage the provider just rejected, whose credential read fails
    /// at the first `sign_in_required` after it (no earlier answer to fall back on), must not read
    /// as "no sign-in needed" (CONTRACTS: "an error never hides a needed sign-in").
    #[test]
    fn a_failed_first_read_does_not_hide_a_new_rejection() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(FailingVault {
            inner: Default::default(),
            gets: Default::default(),
            failing: Default::default(),
            failures: Default::default(),
        });
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let a = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        remember_dead(&state, &a, "synthetic-a-refresh");
        *vault.failing.lock().unwrap() = Some(a.clone());
        *vault.failures.lock().unwrap() = 1;
        assert_eq!(
            state.sign_in_required(&store),
            vec![a.clone()],
            "a failed read hid the sign-in"
        );
        // An earlier answer about another lineage of the same account says nothing about this
        // one: the provider rejected the stored lineage after an answer that found it replaced.
        let state = RefreshState::default();
        remember_dead(&state, &a, "synthetic-a-older");
        assert!(state.sign_in_required(&store).is_empty(), "replaced since");
        remember_dead(&state, &a, "synthetic-a-refresh");
        *vault.failures.lock().unwrap() = 1;
        assert_eq!(
            state.sign_in_required(&store),
            vec![a.clone()],
            "an answer about another lineage hid the sign-in"
        );
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
    }
    /// N1 round 3 (planted defect D16): `credentials_key` names everything the holder map and
    /// the renewal schedule read from the snapshot. Each account's id, provider, kind, enabled
    /// flag and identity moves it; a project save does not.
    #[test]
    fn the_credentials_key_names_each_accounts_identity_and_state() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-a", "default")]);
        let base = store.snapshot().unwrap();
        let key = credentials_key(&store, &base);
        let changed = |edit: &dyn Fn(&mut switchboard_core::Account)| {
            let mut snapshot = base.clone();
            edit(&mut snapshot.accounts[0]);
            credentials_key(&store, &snapshot)
        };
        assert_ne!(key, changed(&|a| a.id = "other".into()), "id");
        assert_ne!(key, changed(&|a| a.provider = Provider::Codex), "provider");
        assert_ne!(key, changed(&|a| a.kind = AuthKind::ApiKey), "kind");
        assert_ne!(key, changed(&|a| a.enabled = !a.enabled), "enabled");
        assert_ne!(
            key,
            changed(&|a| a.external_identity = Some(identity("synthetic-b"))),
            "identity"
        );
        assert_ne!(key, changed(&|a| a.external_identity = None), "no identity");
        let work = tempfile::tempdir().unwrap();
        store
            .save_project(None, "App", &[work.path().canonicalize().unwrap()], &[])
            .unwrap();
        assert_eq!(
            key,
            credentials_key(&store, &store.snapshot().unwrap()),
            "a project save"
        );
    }
    /// N1 round 3 (planted defect D16): one refresh token held by copies of two different
    /// accounts names nobody; once the snapshot says they are copies of one account, the first
    /// copy holds it. A holder map kept across an identity change would still say nobody.
    #[test]
    fn the_token_holders_follow_each_accounts_identity() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-a", "default")]);
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "other".into(),
                expired("synthetic-a"),
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let a = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(
            state.holder(&store, &snapshot, "synthetic-a-refresh"),
            None,
            "two different accounts hold it"
        );
        let mut copies = snapshot.clone();
        for account in &mut copies.accounts {
            account.external_identity = Some(identity("synthetic-a"));
        }
        let first = copies.accounts[0].id.clone();
        assert!(copies.accounts.iter().any(|x| x.id == a));
        assert_eq!(
            state.holder(&store, &copies, "synthetic-a-refresh"),
            Some(first),
            "copies of one account: the first holds it"
        );
    }
    /// N1 round 1 (seam tier): the map of which account holds a refresh token was rebuilt,
    /// reading every Claude account's credential from the vault, whenever `Store::changes`
    /// moved, which every project save does.
    #[test]
    fn a_project_save_does_not_rebuild_the_token_holders() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault::default());
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let a = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(
            state.holder(&store, &snapshot, "synthetic-a-refresh"),
            Some(a.clone())
        );
        let reads = vault.gets.load(Ordering::SeqCst);
        let work = tempfile::tempdir().unwrap();
        let mut folders = Vec::new();
        for n in 0..5 {
            let folder = work.path().join(format!("repo-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            folders.push(folder.canonicalize().unwrap());
            store
                .save_project(Some("app"), "App", &folders, &[])
                .or_else(|_| store.save_project(None, "App", &folders, &[]))
                .unwrap();
            let snapshot = store.snapshot().unwrap();
            assert_eq!(
                state.holder(&store, &snapshot, "synthetic-a-refresh"),
                Some(a.clone())
            );
        }
        assert_eq!(vault.gets.load(Ordering::SeqCst), reads);
        // A credential written is seen.
        let mut fresh = expired("synthetic-a");
        fresh.refresh_token = Some("synthetic-a-next".into());
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                fresh,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        assert_eq!(state.holder(&store, &snapshot, "synthetic-a-refresh"), None);
        assert_eq!(
            state.holder(&store, &snapshot, "synthetic-a-next"),
            Some(a.clone())
        );
    }
    /// PRD-01 (iteration 2026-10-24): the SB-182 cache was keyed on `Store::changes`, which
    /// every project save moves too; a folder joining its group (about 740 a day) sent the next
    /// `recovery` back to the Keychain, and it still took 5 to 30 s. Only a credential write
    /// may cost a read now, and a new sign-in still invalidates the answer.
    #[test]
    fn a_folder_joining_its_group_does_not_cost_a_vault_read() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(CountingVault::default());
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-a"),
                Some(identity("synthetic-a")),
            )
            .unwrap();
        let a = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        remember_dead(&state, &a, "synthetic-a-refresh");
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        let reads = vault.gets.load(Ordering::SeqCst);
        let work = tempfile::tempdir().unwrap();
        let mut folders = Vec::new();
        for n in 0..5 {
            let folder = work.path().join(format!("repo-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            folders.push(folder.canonicalize().unwrap());
            let before = store.changes();
            store
                .save_project(Some("app"), "App", &folders, &[])
                .or_else(|_| store.save_project(None, "App", &folders, &[]))
                .unwrap();
            assert!(
                store.changes() > before,
                "a project save is a backup change"
            );
            assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
        }
        assert_eq!(
            vault.gets.load(Ordering::SeqCst),
            reads,
            "a project save is not a credential change"
        );
        // A new sign-in is still seen at once.
        let mut fresh = expired("synthetic-a");
        fresh.refresh_token = Some("signed-in-again".into());
        store
            .upsert(
                "synthetic-a".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                fresh,
                Some(identity("synthetic-a")),
            )
            .unwrap();
        assert!(state.sign_in_required(&store).is_empty());
    }
    /// A vault whose next read returns what it held, then waits until released: a write
    /// lands between the read and the reader's return.
    #[derive(Default)]
    struct GatedVault {
        inner: switchboard_core::MemoryVault,
        armed: std::sync::atomic::AtomicBool,
        read: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
        release: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    }
    impl GatedVault {
        fn arm(&self) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
            let (read_tx, read_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            *self.read.lock().unwrap() = Some(read_tx);
            *self.release.lock().unwrap() = Some(release_rx);
            self.armed.store(true, Ordering::SeqCst);
            (read_rx, release_tx)
        }
    }
    impl switchboard_core::Vault for GatedVault {
        fn get(&self, id: &str) -> Result<Credential, String> {
            let value = self.inner.get(id);
            if self.armed.swap(false, Ordering::SeqCst) {
                self.read.lock().unwrap().take().unwrap().send(()).unwrap();
                self.release.lock().unwrap().take().unwrap().recv().unwrap();
            }
            value
        }
        fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
            self.inner.put(id, value)
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.inner.delete(id)
        }
    }
    /// PRD-01 (adversarial round): a credential stored while `sign_in_required` reads the vault
    /// may make that one answer old, never the next: a sign-in is seen and a lineage put back
    /// is not hidden.
    #[test]
    fn a_credential_stored_during_the_read_never_sticks_in_the_answer() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(GatedVault::default());
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        let put = |token: &str| {
            let mut c = expired("synthetic-a");
            c.refresh_token = Some(token.into());
            store
                .upsert(
                    "synthetic-a".into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    "default".into(),
                    c,
                    Some(identity("synthetic-a")),
                )
                .unwrap();
        };
        put("synthetic-a-refresh");
        let a = id_of(&store, "synthetic-a", "default");
        let state = RefreshState::default();
        remember_dead(&state, &a, "synthetic-a-refresh");
        // A sign-in lands while the rejected credential is being read.
        let (read, release) = vault.arm();
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| state.sign_in_required(&store));
            read.recv_timeout(std::time::Duration::from_secs(10))
                .expect("the reader went to the vault");
            put("signed-in-again");
            release.send(()).unwrap();
            assert_eq!(reader.join().unwrap(), vec![a.clone()], "read before it");
        });
        assert!(
            state.sign_in_required(&store).is_empty(),
            "the sign-in is seen by the next answer"
        );
        // The rejected lineage put back while the new one is being read is not hidden. A write
        // first, so the reader goes to the vault.
        put("signed-in-again");
        let (read, release) = vault.arm();
        std::thread::scope(|scope| {
            let reader = scope.spawn(|| state.sign_in_required(&store));
            read.recv_timeout(std::time::Duration::from_secs(10))
                .expect("the reader went to the vault");
            put("synthetic-a-refresh");
            release.send(()).unwrap();
            assert!(reader.join().unwrap().is_empty(), "read before it");
        });
        assert_eq!(state.sign_in_required(&store), vec![a.clone()]);
    }
    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }
    #[tokio::test]
    async fn a_renewed_token_that_cannot_be_stored_is_kept_and_adopted_later() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"fresh","expires_in":28800,"refresh_token":"fresh-r"}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(FlakyVault {
            inner: Default::default(),
            refuse: Default::default(),
        });
        let store = Store::open(root.path().to_owned(), vault.clone()).unwrap();
        store
            .upsert(
                "b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                expired("synthetic-b"),
                Some(identity("synthetic-b")),
            )
            .unwrap();
        let id = store.snapshot().unwrap().accounts[0].id.clone();
        let state = RefreshState::at(url);
        vault.refuse.store(true, Ordering::SeqCst);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::Transient
        );
        assert!(
            state.sign_in_required(&store).is_empty(),
            "a storage hiccup is not an ended sign-in"
        );
        assert!(has_stash(&state, "synthetic-b-refresh"));
        // While it cannot be stored, no second grant spends anything.
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, true).await,
            Outcome::Transient
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        vault.refuse.store(false, Ordering::SeqCst);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &id, false).await,
            Outcome::Refreshed
        );
        assert_eq!(
            store
                .stored_credential(&id)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("fresh-r")
        );
        assert!(!has_stash(&state, &id));
    }
    #[tokio::test]
    async fn a_token_issued_for_another_account_is_a_dead_lineage_here() {
        let (url, _) = endpoint((
            200,
            json!({"access_token":"x","expires_in":60,"account":{"uuid":"someone-else"}}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::SignInRequired
        );
        assert_eq!(
            store.stored_credential(&b).unwrap().access_token,
            "synthetic-b-token",
            "nothing foreign was stored"
        );
    }
    #[tokio::test]
    async fn a_renewal_issued_to_another_saved_account_reaches_that_account() {
        let (url, _) = endpoint((
            200,
            json!({"access_token":"next-access","expires_in":28800,"refresh_token":"next-refresh","account":{"uuid":"synthetic-c"}}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        // synthetic-c rightfully holds the lineage synthetic-b holds by mistake.
        let mut shared = expired("synthetic-c");
        shared.refresh_token = Some("synthetic-b-refresh".into());
        store
            .upsert(
                "synthetic-c".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "work".into(),
                shared,
                Some(identity("synthetic-c")),
            )
            .unwrap();
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        let c = id_of(&store, "synthetic-c", "work");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::SignInRequired
        );
        assert_eq!(
            store
                .stored_credential(&c)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("next-refresh"),
            "the owner keeps its lineage"
        );
        assert!(state.is_dead(&store, &b));
        assert!(!state.is_dead(&store, &c));
    }
    #[tokio::test]
    async fn a_grant_whose_caller_gives_up_still_keeps_the_successor() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = Router::new().route(
            "/token",
            post(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    Json(json!({"access_token":"late-access","expires_in":28800,"refresh_token":"late-refresh"}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(format!("http://{address}/token"));
        let b = id_of(&store, "synthetic-b", "default");
        // The caller's deadline passes while the provider is already rotating the token.
        assert!(tokio::time::timeout(
            Duration::from_millis(50),
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false)
        )
        .await
        .is_err());
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(has_stash(&state, "synthetic-b-refresh"));
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no second grant");
        assert_eq!(
            store
                .stored_credential(&b)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("late-refresh")
        );
    }
    #[tokio::test]
    async fn the_same_login_in_another_organization_is_another_account() {
        use axum::routing::get;
        let app = Router::new().route(
            "/profile",
            get(|| async {
                Json(
                    json!({"account":{"uuid":"synthetic-a"},"organization":{"uuid":"another-org"}}),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-c", "default")]);
        let state = RefreshState::default();
        state.set_profile_endpoint(format!("http://{address}/profile"));
        let live = signed_in(Provider::Claude).unwrap();
        learn_live_owner(&store, SIGNED_IN_A, &state).await;
        assert_eq!(
            lineage(&store, &state, &live.identity, &live.credential),
            Lineage::Foreign
        );
        assert_eq!(
            filable(&store, &state, &live.identity, &live.credential).unwrap_err(),
            FOREIGN_LIVE
        );
    }
    #[tokio::test]
    async fn an_unanswered_owner_question_waits_a_minute() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let app = Router::new().route(
            "/profile",
            axum::routing::get(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                async { axum::http::StatusCode::UNAUTHORIZED }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-c", "default")]);
        let state = RefreshState::default();
        state.set_profile_endpoint(format!("http://{address}/profile"));
        learn_live_owner(&store, SIGNED_IN_A, &state).await;
        learn_live_owner(&store, SIGNED_IN_A, &state).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let live = signed_in(Provider::Claude).unwrap();
        assert_eq!(
            filable(&store, &state, &live.identity, &live.credential).unwrap_err(),
            UNCONFIRMED_LIVE
        );
    }
    #[tokio::test]
    async fn the_provider_names_the_owner_of_an_unknown_live_lineage() {
        use axum::routing::get;
        let app = Router::new().route(
            "/profile",
            get(|| async { Json(json!({"account":{"uuid":"synthetic-b","email":"b@example.invalid"},"organization":{"uuid":"synthetic-org"}})) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-c", "default")]);
        let state = RefreshState::default();
        state.set_profile_endpoint(format!("http://{address}/profile"));
        let live = signed_in(Provider::Claude).unwrap();
        assert_eq!(
            lineage(&store, &state, &live.identity, &live.credential),
            Lineage::Unresolved
        );
        learn_live_owner(&store, SIGNED_IN_A, &state).await;
        assert_eq!(
            lineage(&store, &state, &live.identity, &live.credential),
            Lineage::Foreign
        );
    }
    #[tokio::test]
    async fn a_running_claude_swap_renews_its_accounts_and_switchboard_follows_it() {
        let (url, calls) = endpoint((200, json!({"access_token":"x","expires_in":60}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[("synthetic-b", "default"), ("synthetic-b", "work")],
        );
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        // Claude Swap holds b with a later generation than Switchboard's copies.
        let mut newer = expired("synthetic-b");
        newer.access_token = "swap-renewed".into();
        newer.refresh_token = Some("swap-renewed-r".into());
        newer.expires_at = Some(now() + 28800);
        let profile = crate::external::CapturedProfile {
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            credential: newer,
            identity: identity("synthetic-b"),
            label: "b".into(),
        };
        assert_eq!(
            follow_claude_swap(&store, &state, SwapView::Profiles(vec![profile], vec![])),
            2,
            "both pools follow"
        );
        assert_eq!(
            store.stored_credential(&b).unwrap().access_token,
            "swap-renewed"
        );
        // While it runs, Switchboard never spends that lineage itself.
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::NotNeeded
        );
        // A pass that cannot read its files (it was writing them) keeps what it holds.
        follow_claude_swap(&store, &state, SwapView::Unreadable);
        assert_eq!(swap_held(&state), 1);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::NotNeeded
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // Claude Swap stops: renewal is Switchboard's again.
        follow_claude_swap(&store, &state, SwapView::NotRunning);
        assert_eq!(swap_held(&state), 0);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn a_successor_issued_to_a_saved_account_with_an_older_lineage_still_reaches_it() {
        let (url, _) = endpoint((
            200,
            json!({"access_token":"c-g3","expires_in":28800,"refresh_token":"c-g3-r","account":{"uuid":"synthetic-c"}}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        // b holds c's token by mistake; c's own copy holds an older lineage of c's.
        let store = store_with(
            root.path(),
            &[("synthetic-b", "default"), ("synthetic-c", "work")],
        );
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        let c = id_of(&store, "synthetic-c", "work");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::SignInRequired
        );
        assert_eq!(
            store
                .stored_credential(&c)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("c-g3-r"),
            "c's newest sign-in is kept, not dropped"
        );
    }
    /// SB-179 (design note owner-transaction-2026-10-23, T-2): the grant leaves the owner
    /// transaction. While it is on the network the transaction is free, a native switch to the
    /// account is refused, and a new sign-in stored meanwhile stays: the successor goes only
    /// where the spent token still is.
    #[tokio::test]
    async fn a_grant_runs_outside_the_transaction_and_a_sign_in_stored_meanwhile_stands() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                tokio::time::sleep(Duration::from_millis(400)).await;
                // The endpoint names whose token it issued, as Anthropic's does.
                Json(json!({"access_token":"late","expires_in":28800,"refresh_token":"late-r","account":{"uuid":"synthetic-b"}}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = Arc::new(RefreshState::at(format!("http://{address}/token")));
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let b = id_of(&store, "synthetic-b", "default");
        let renewal = {
            let (store, state, lock, b) = (store.clone(), state.clone(), lock.clone(), b.clone());
            tokio::spawn(async move {
                ensure_fresh_with(
                    &store,
                    SIGNED_IN_A,
                    &state,
                    &b,
                    false,
                    Some(&lock),
                    "renewal",
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(state.in_flight(&store, &b));
        // The transaction is free during the grant: a control write takes it at once.
        let held = lock
            .try_lock()
            .expect("the grant holds the owner transaction");
        assert_eq!(
            crate::activate_native(&store, &b, None, SIGNED_IN_A, Some(&state)).unwrap_err(),
            RENEWING
        );
        // The person signs b in again meanwhile.
        let mut new = expired("synthetic-b");
        new.access_token = "signed-in-again".into();
        new.refresh_token = Some("signed-in-again-r".into());
        // Earlier than the successor's, as a sign-in made before the grant answered is.
        new.expires_at = Some(now() + 28_000);
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                new,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        drop(held);
        assert_eq!(renewal.await.unwrap(), Outcome::NotNeeded);
        let stored = store.stored_credential(&b).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("signed-in-again-r"));
        assert!(!has_stash(&state, "synthetic-b-refresh"));
    }
    /// SB-179 unit review: a grant the provider rejects while a new sign-in is stored for the
    /// account says nothing about that sign-in; the account does not need another.
    #[tokio::test]
    async fn a_rejected_grant_does_not_mark_a_sign_in_stored_meanwhile() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                tokio::time::sleep(Duration::from_millis(400)).await;
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant"})),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = Arc::new(RefreshState::at(format!("http://{address}/token")));
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let b = id_of(&store, "synthetic-b", "default");
        let renewal = {
            let (store, state, lock, b) = (store.clone(), state.clone(), lock.clone(), b.clone());
            tokio::spawn(async move {
                ensure_fresh_with(
                    &store,
                    SIGNED_IN_A,
                    &state,
                    &b,
                    false,
                    Some(&lock),
                    "renewal",
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut new = expired("synthetic-b");
        new.refresh_token = Some("signed-in-again-r".into());
        new.expires_at = Some(now() + 28_000);
        store
            .upsert(
                "synthetic-b".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                new,
                Some(identity("synthetic-b")),
            )
            .unwrap();
        assert_eq!(renewal.await.unwrap(), Outcome::NotNeeded);
        assert!(state.sign_in_required(&store).is_empty());
    }
    /// SB-179: a successor kept in memory, because its grant ended while the drain held the
    /// transaction, is stored by the drain.
    #[tokio::test]
    async fn the_drain_stores_a_successor_kept_in_memory() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at("http://127.0.0.1:9/token".into());
        let b = id_of(&store, "synthetic-b", "default");
        let mut refreshed = expired("synthetic-b");
        refreshed.access_token = "kept".into();
        refreshed.refresh_token = Some("kept-r".into());
        refreshed.expires_at = Some(now() + 28_800);
        state.stash.lock().unwrap().insert(
            "synthetic-b-refresh".into(),
            Kept {
                refreshed,
                owner: None,
            },
        );
        assert_eq!(adopt_all_kept(&store, &state), 1);
        let stored = store.stored_credential(&b).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("kept-r"));
        assert_eq!(adopt_all_kept(&store, &state), 0);
    }
    #[tokio::test]
    async fn an_account_whose_renewal_is_in_flight_is_not_switched_to() {
        let app = Router::new().route(
            "/token",
            post(|| async {
                tokio::time::sleep(Duration::from_millis(400)).await;
                Json(json!({"access_token":"late","expires_in":28800,"refresh_token":"late-r"}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(format!("http://{address}/token"));
        let b = id_of(&store, "synthetic-b", "default");
        // The caller gives up; the grant keeps spending b's token.
        let _ = tokio::time::timeout(
            Duration::from_millis(50),
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false),
        )
        .await;
        assert!(state.in_flight(&store, &b));
        assert_eq!(
            crate::activate_native(&store, &b, None, SIGNED_IN_A, Some(&state)).unwrap_err(),
            RENEWING
        );
        // A second grant on the same token is never made meanwhile: a caller under the
        // transaction (a launch) waits for this one and takes its successor (SB-179).
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::Refreshed
        );
        assert!(!state.in_flight(&store, &b));
        let stored = store.stored_credential(&b).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("late-r"));
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::NotNeeded
        );
    }
    #[tokio::test]
    async fn the_offline_cli_never_spends_a_refresh_token() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"x","expires_in":28800,"refresh_token":"y"}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::offline(root.path().join("renewal-state.json"));
        state.set_endpoint(url);
        let b = id_of(&store, "synthetic-b", "default");
        assert_ne!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            store
                .stored_credential(&b)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-b-refresh")
        );
    }
    #[tokio::test]
    async fn the_offline_cli_still_refuses_a_lineage_the_owner_saw_rejected() {
        let (url, _) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let journal = root.path().join("renewal-state.json");
        let owner = RefreshState::at(url);
        owner.remember_in(journal.clone());
        let b = id_of(&store, "synthetic-b", "default");
        ensure_fresh(&store, SIGNED_IN_A, &owner, &b, false).await;
        assert!(RefreshState::offline(journal).is_dead(&store, &b));
    }
    fn swap_with_newer_b() -> Result<crate::external::ImportBatch, String> {
        let mut credential = expired("synthetic-b");
        credential.access_token = "swap-access".into();
        credential.refresh_token = Some("swap-renewed".into());
        credential.expires_at = Some(now() + 28800);
        Ok(crate::external::ImportBatch {
            profiles: vec![crate::external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential,
                identity: identity("synthetic-b"),
                label: "b".into(),
            }],
            failed: 0,
            failed_emails: vec![],
            skipped: 0,
        })
    }
    fn swap_rewriting_b() -> Result<crate::external::ImportBatch, String> {
        Ok(crate::external::ImportBatch {
            profiles: vec![],
            failed: 1,
            failed_emails: vec!["synthetic-b@example.invalid".into()],
            skipped: 0,
        })
    }
    #[test]
    fn switching_to_an_account_claude_swap_renews_takes_its_newest_generation_first() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::default();
        state.set_swap_activity(true, false);
        let b = id_of(&store, "synthetic-b", "default");
        let fresh = NativeSources {
            swap: swap_with_newer_b,
            ..SIGNED_IN_A
        };
        // Not held by Swap: nothing is read.
        catch_up_with_claude_swap(&store, &state, fresh, &b).unwrap();
        assert_eq!(
            store
                .stored_credential(&b)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-b-refresh")
        );
        state.hold_for_test("synthetic-b");
        catch_up_with_claude_swap(&store, &state, fresh, &b).unwrap();
        assert_eq!(
            store
                .stored_credential(&b)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("swap-renewed")
        );
        // Swap is rewriting b's own row, or its files cannot be read: refused, not guessed.
        let rewriting = NativeSources {
            swap: swap_rewriting_b,
            ..SIGNED_IN_A
        };
        assert_eq!(
            catch_up_with_claude_swap(&store, &state, rewriting, &b).unwrap_err(),
            SWAP_BUSY
        );
        assert_eq!(
            catch_up_with_claude_swap(&store, &state, SIGNED_IN_A, &b).unwrap_err(),
            SWAP_BUSY
        );
    }
    #[test]
    fn claude_swaps_files_are_read_when_they_can_hold_something_new() {
        let sig = Some((3, 100, 7));
        let moved = Some((3, 120, 9));
        // While it runs: every pass.
        assert!(swap_read_due(true, Some(true), sig, sig));
        // Switchboard just started: once, whatever Swap is doing.
        assert!(swap_read_due(false, None, sig, None));
        // It just stopped: its last renewals are in the files.
        assert!(swap_read_due(false, Some(true), sig, sig));
        // Stopped and untouched: nothing new to read.
        assert!(!swap_read_due(false, Some(false), sig, sig));
        // Stopped, but its files changed (a cswap command ran): read.
        assert!(swap_read_due(false, Some(false), moved, sig));
        // No Claude Swap folder at all: never.
        assert!(!swap_read_due(false, None, None, None));
    }
    #[tokio::test]
    async fn a_claude_swap_row_that_could_not_be_read_stays_held() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"x","expires_in":28800,"refresh_token":"y"}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        state.hold_for_test("synthetic-b");
        // Swap is rewriting b's row this pass: it is missing from the profiles read.
        follow_claude_swap(
            &store,
            &state,
            SwapView::Profiles(vec![], vec!["synthetic-b@example.invalid".into()]),
        );
        assert_eq!(swap_held(&state), 1);
        let b = id_of(&store, "synthetic-b", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, true).await,
            Outcome::NotNeeded
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no second renewer");
        // A pass that reads every row and no longer lists it releases it.
        follow_claude_swap(&store, &state, SwapView::Profiles(vec![], vec![]));
        assert_eq!(swap_held(&state), 0);
    }
    #[test]
    fn a_switch_after_claude_swap_stopped_takes_its_generation_and_holds_nothing() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::default();
        // Held at the monitor's last look; Swap has stopped since.
        state.hold_for_test("synthetic-b");
        state.set_swap_activity(false, false);
        let b = id_of(&store, "synthetic-b", "default");
        let native = NativeSources {
            swap: swap_with_newer_b,
            ..SIGNED_IN_A
        };
        catch_up_with_claude_swap(&store, &state, native, &b).unwrap();
        assert_eq!(
            store
                .stored_credential(&b)
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("swap-renewed")
        );
        assert_eq!(
            swap_held(&state),
            0,
            "a stopped Swap renews nothing: Switchboard does"
        );
    }
    #[test]
    fn only_the_unreadable_rows_account_stays_held() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[("synthetic-b", "default"), ("synthetic-c", "default")],
        );
        let state = RefreshState::default();
        state.hold_for_test("synthetic-b");
        state.hold_for_test("synthetic-c");
        // c's row is broken for good (another email), b was removed from Swap: neither is held
        // forever — only an account whose own row failed this pass stays held.
        follow_claude_swap(
            &store,
            &state,
            SwapView::Profiles(vec![], vec!["someone-else@example.invalid".into()]),
        );
        assert_eq!(swap_held(&state), 0);
        state.hold_for_test("synthetic-b");
        follow_claude_swap(
            &store,
            &state,
            SwapView::Profiles(vec![], vec!["SYNTHETIC-B@example.invalid".into()]),
        );
        assert_eq!(swap_held(&state), 1, "matched case-insensitively");
    }
    #[test]
    fn switching_to_an_account_swap_no_longer_lists_is_not_refused() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::default();
        state.hold_for_test("synthetic-b");
        let b = id_of(&store, "synthetic-b", "default");
        fn swap_without_b() -> Result<crate::external::ImportBatch, String> {
            Ok(crate::external::ImportBatch {
                profiles: vec![],
                failed: 1,
                failed_emails: vec!["someone-else@example.invalid".into()],
                skipped: 0,
            })
        }
        let native = NativeSources {
            swap: swap_without_b,
            ..SIGNED_IN_A
        };
        catch_up_with_claude_swap(&store, &state, native, &b).unwrap();
        assert_eq!(swap_held(&state), 0, "released: Swap no longer has it");
    }
    #[test]
    fn a_stopped_claude_swap_still_hands_over_what_it_renewed_last() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[
                ("synthetic-b", "default"),
                ("synthetic-b", "work"),
                ("synthetic-c", "default"),
            ],
        );
        let state = RefreshState::default();
        state.hold_for_test("synthetic-b");
        let profile =
            |account: &str, refresh: &str, expires: i64| crate::external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential: {
                    let mut c = expired(account);
                    c.access_token = format!("{refresh}-access");
                    c.refresh_token = Some(refresh.into());
                    c.expires_at = Some(expires);
                    c
                },
                identity: identity(account),
                label: account.into(),
            };
        let updated = follow_claude_swap(
            &store,
            &state,
            SwapView::Stopped(vec![
                // Renewed by Swap just before it stopped: newer than Switchboard's copies.
                profile("synthetic-b", "swap-last", now() + 28800),
                // Older than what Switchboard holds: never taken.
                profile("synthetic-c", "swap-old", EXPIRED - 1),
            ]),
        );
        assert_eq!(updated, 2, "both pools of synthetic-b");
        assert_eq!(swap_held(&state), 0, "nothing is held once it stopped");
        for pool in ["default", "work"] {
            let c = store
                .stored_credential(&id_of(&store, "synthetic-b", pool))
                .unwrap();
            assert_eq!(c.refresh_token.as_deref(), Some("swap-last"));
        }
        let c = store
            .stored_credential(&id_of(&store, "synthetic-c", "default"))
            .unwrap();
        assert_eq!(c.refresh_token.as_deref(), Some("synthetic-c-refresh"));
    }
    #[tokio::test]
    async fn a_dead_copy_comes_back_when_claude_swap_holds_its_successor() {
        let (url, _) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        // Swap renewed b and stopped; Switchboard's grant on the spent token was rejected.
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::SignInRequired
        );
        let mut newer = expired("synthetic-b");
        newer.refresh_token = Some("swap-successor".into());
        newer.expires_at = Some(now() + 28800);
        follow_claude_swap(
            &store,
            &state,
            SwapView::Stopped(vec![crate::external::CapturedProfile {
                provider: Provider::Claude,
                kind: AuthKind::OAuth,
                credential: newer,
                identity: identity("synthetic-b"),
                label: "b".into(),
            }]),
        );
        assert!(
            !state.is_dead(&store, &b),
            "the successor replaces the spent token"
        );
        assert!(state.sign_in_required(&store).is_empty());
    }
    #[tokio::test]
    async fn a_rejected_lineage_is_remembered_across_restarts() {
        let (url, calls) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let b = id_of(&store, "synthetic-b", "default");
        let journal = root.path().join("renewal-state.json");
        let state = RefreshState::at(url.clone());
        state.remember_in(journal.clone());
        ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await;
        let text = std::fs::read_to_string(&journal).unwrap();
        assert!(
            !text.contains("synthetic-b-refresh"),
            "fingerprints only, never the token"
        );
        // A new process loads it and spends no further grant.
        let restarted = RefreshState::at(url);
        restarted.remember_in(journal);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &restarted, &b, true).await,
            Outcome::SignInRequired
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn a_renewed_live_item_keeps_every_other_key() {
        let live = serde_json::to_vec(&json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r","expiresAt":1,"subscriptionType":"max"},"mcpOAuth":{"keep":true}})).unwrap();
        let mut refreshed = credential("synthetic-a");
        refreshed.access_token = "new".into();
        refreshed.refresh_token = Some("new-r".into());
        refreshed.expires_at = Some(5000);
        let item: Value =
            serde_json::from_slice(&renewed_item(&live, &refreshed).unwrap()).unwrap();
        assert_eq!(item["claudeAiOauth"]["accessToken"], "new");
        assert_eq!(item["claudeAiOauth"]["refreshToken"], "new-r");
        assert_eq!(item["claudeAiOauth"]["expiresAt"], 5_000_000);
        assert_eq!(item["claudeAiOauth"]["subscriptionType"], "max");
        assert_eq!(item["mcpOAuth"], json!({"keep":true}));
        assert!(renewed_item(b"not json", &refreshed).is_none());
    }
    #[tokio::test]
    async fn the_account_in_use_is_left_to_claude_code_until_it_is_idle() {
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-a", "default")]);
        let state = RefreshState::default();
        // signed_in's live token is far from expiry: Claude Code renews it, not Switchboard.
        assert_eq!(
            renew_idle_live(&store, SIGNED_IN_A, &state).await,
            Outcome::NotNeeded
        );
    }
    thread_local! {
        static LIVE: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
        static WRITE_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    /// Claude Code's live item on this test's thread (`#[tokio::test]` is single-threaded).
    struct FakeLive;
    impl crate::external::LiveItem for FakeLive {
        fn read(&self) -> Result<Option<Vec<u8>>, String> {
            Ok(LIVE.with(|l| l.borrow().clone()))
        }
        fn write(&self, auth: &[u8]) -> Result<(), String> {
            if WRITE_FAILS.with(|f| f.get()) {
                return Err("Keychain write failed.".into());
            }
            LIVE.with(|l| *l.borrow_mut() = Some(auth.to_vec()));
            Ok(())
        }
    }
    fn fake_live() -> Result<Box<dyn crate::external::LiveItem>, String> {
        Ok(Box::new(FakeLive))
    }
    /// Claude Code is signed in as synthetic-a and left its access token expired.
    fn idle_a(provider: Provider) -> Result<crate::external::CapturedProfile, String> {
        let mut live = signed_in(provider)?;
        live.credential.expires_at = Some(EXPIRED);
        Ok(live)
    }
    const IDLE_A: NativeSources = NativeSources {
        current: idle_a,
        activate: activates,
        live: fake_live,
        swap: crate::no_swap,
        ..crate::UNAVAILABLE
    };
    fn idle_setup(item: Value) -> (tempfile::TempDir, Arc<Store>) {
        LIVE.with(|l| *l.borrow_mut() = Some(serde_json::to_vec(&item).unwrap()));
        WRITE_FAILS.with(|f| f.set(false));
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[("synthetic-a", "default"), ("synthetic-a", "work")],
        );
        (root, store)
    }
    fn live_item() -> Value {
        LIVE.with(|l| serde_json::from_slice(l.borrow().as_deref().unwrap()).unwrap())
    }
    const IDLE_ITEM: fn() -> Value = || json!({"claudeAiOauth":{"accessToken":"synthetic-a-token","refreshToken":"synthetic-a-refresh","expiresAt":EXPIRED*1000},"mcpOAuth":{"keep":true}});
    #[tokio::test]
    async fn an_idle_live_account_is_renewed_in_claude_code_and_every_copy() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"fresh-access","expires_in":28800,"refresh_token":"fresh-refresh","account":{"uuid":"synthetic-a"}}),
        ))
        .await;
        let (_root, store) = idle_setup(IDLE_ITEM());
        let state = RefreshState::at(url);
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let item = live_item();
        assert_eq!(item["claudeAiOauth"]["accessToken"], "fresh-access");
        assert_eq!(item["claudeAiOauth"]["refreshToken"], "fresh-refresh");
        assert_eq!(item["mcpOAuth"], json!({"keep":true}), "other keys kept");
        for pool in ["default", "work"] {
            let c = store
                .stored_credential(&id_of(&store, "synthetic-a", pool))
                .unwrap();
            assert_eq!(c.refresh_token.as_deref(), Some("fresh-refresh"));
        }
    }
    #[tokio::test]
    async fn an_idle_renewal_issued_to_another_account_stays_with_claude_code_only() {
        let (url, _) = endpoint((
            200,
            json!({"access_token":"other-access","expires_in":28800,"refresh_token":"other-refresh","account":{"uuid":"someone-else"}}),
        ))
        .await;
        let (_root, store) = idle_setup(IDLE_ITEM());
        let state = RefreshState::at(url);
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::SignInRequired
        );
        // The spent token's successor reached Claude Code, so it stays signed in.
        assert_eq!(
            live_item()["claudeAiOauth"]["refreshToken"],
            "other-refresh"
        );
        let a = id_of(&store, "synthetic-a", "default");
        assert_eq!(
            store.stored_credential(&a).unwrap().access_token,
            "synthetic-a-token",
            "nothing foreign was filed under synthetic-a"
        );
        // synthetic-a's copies hold the spent token: they need a new sign-in.
        assert!(state.is_dead(&store, &a));
        assert!(state.is_dead(&store, &id_of(&store, "synthetic-a", "work")));
        let mut renewed = idle_a(Provider::Claude).unwrap();
        renewed.credential.refresh_token = Some("other-refresh".into());
        assert_eq!(
            lineage(&store, &state, &renewed.identity, &renewed.credential),
            Lineage::Foreign
        );
    }
    #[tokio::test]
    async fn an_idle_renewal_that_cannot_be_written_is_stashed() {
        let (url, calls) = endpoint((
            200,
            json!({"access_token":"fresh-access","expires_in":28800,"refresh_token":"fresh-refresh"}),
        ))
        .await;
        let (_root, store) = idle_setup(IDLE_ITEM());
        WRITE_FAILS.with(|f| f.set(true));
        let state = RefreshState::at(url);
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::Transient
        );
        assert!(has_stash(&state, "synthetic-a-refresh"));
        // SB-101: already on disk, so a quit or crash now loses nothing.
        for pool in ["default", "work"] {
            let c = store
                .stored_credential(&id_of(&store, "synthetic-a", pool))
                .unwrap();
            assert_eq!(c.refresh_token.as_deref(), Some("fresh-refresh"), "{pool}");
        }
        // The next pass writes the kept successor before anything else — no second grant —
        // and every copy holding the spent token gets it, not just the first.
        WRITE_FAILS.with(|f| f.set(false));
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::Refreshed
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            live_item()["claudeAiOauth"]["refreshToken"],
            "fresh-refresh"
        );
        for pool in ["default", "work"] {
            let c = store
                .stored_credential(&id_of(&store, "synthetic-a", pool))
                .unwrap();
            assert_eq!(c.refresh_token.as_deref(), Some("fresh-refresh"));
        }
        assert!(!has_stash(&state, "synthetic-a-refresh"));
    }
    #[tokio::test]
    async fn an_idle_live_item_that_changed_is_left_alone() {
        let (url, calls) = endpoint((200, json!({"access_token":"x","expires_in":60}))).await;
        // Claude Code renewed it between the read and the lock.
        let (_root, store) = idle_setup(
            json!({"claudeAiOauth":{"accessToken":"newer","refreshToken":"newer-r","expiresAt":EXPIRED*1000}}),
        );
        let state = RefreshState::at(url);
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::NotNeeded
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn an_idle_live_lineage_rejected_by_the_provider_is_dead() {
        let (url, _) = endpoint((400, json!({"error":"invalid_grant"}))).await;
        let (_root, store) = idle_setup(IDLE_ITEM());
        let state = RefreshState::at(url);
        assert_eq!(
            renew_idle_live(&store, IDLE_A, &state).await,
            Outcome::SignInRequired
        );
        assert!(state.is_dead(&store, &id_of(&store, "synthetic-a", "work")));
    }
    #[tokio::test]
    async fn a_refused_client_holds_every_renewal_without_blaming_an_account() {
        let (url, calls) = endpoint((
            401,
            json!({"error":"invalid_client","error_description":"Client authentication failed"}),
        ))
        .await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(
            root.path(),
            &[("synthetic-b", "default"), ("synthetic-c", "default")],
        );
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        let c = id_of(&store, "synthetic-c", "default");
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await,
            Outcome::Transient
        );
        assert!(state.renewal_blocked());
        // No account is marked dead, and no other account spends a grant while held.
        assert!(!state.is_dead(&store, &b));
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &c, false).await,
            Outcome::Transient
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(state.sign_in_required(&store).is_empty());
        // A grant that gets through clears the hold.
        let (url, _) = endpoint((
            200,
            json!({"access_token":"fresh","expires_in":28800,"refresh_token":"fresh-r"}),
        ))
        .await;
        state.set_endpoint(url);
        state.client_refused(false);
        assert_eq!(
            ensure_fresh(&store, SIGNED_IN_A, &state, &c, false).await,
            Outcome::Refreshed
        );
        assert!(!state.renewal_blocked());
    }
    #[tokio::test]
    async fn a_refused_client_after_the_hold_is_tried_once_more() {
        let (url, calls) = endpoint((400, json!({"error":"invalid_client"}))).await;
        let root = tempfile::tempdir().unwrap();
        let store = store_with(root.path(), &[("synthetic-b", "default")]);
        let state = RefreshState::at(url);
        let b = id_of(&store, "synthetic-b", "default");
        ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await;
        // An hour later the hold lapses and exactly one grant tests the client again.
        *state.client_rejected.lock().unwrap() = Some(now() - CLIENT_HOLD_SECONDS);
        assert!(!state.renewal_blocked());
        state.backoff.lock().unwrap().clear();
        ensure_fresh(&store, SIGNED_IN_A, &state, &b, false).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(state.renewal_blocked());
    }
    #[test]
    fn malformed_grant_responses_are_rejected() {
        let old = expired("synthetic-b");
        assert!(apply(&old, &json!({"expires_in":60}), 10).is_none());
        assert!(apply(&old, &json!({"access_token":"x","expires_in":0}), 10).is_none());
        assert!(apply(
            &old,
            &json!({"access_token":"x","expires_in":60,"refresh_token":7}),
            10
        )
        .is_none());
        let kept = apply(&old, &json!({"access_token":"x","expires_in":60}), 10).unwrap();
        assert_eq!(
            kept.refresh_token, old.refresh_token,
            "no rotation returned"
        );
        assert_eq!(kept.expires_at, Some(70));
    }
}
