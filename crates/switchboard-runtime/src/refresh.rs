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
use switchboard_core::{Account, AuthKind, Credential, ExternalIdentity, Provider, Store};

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

/// In-memory refresh bookkeeping of one owner process.
pub(crate) struct RefreshState {
    endpoint: Mutex<String>,
    /// Account id → hash of the refresh token the provider rejected.
    dead: Mutex<HashMap<String, String>>,
    /// Account id → (next attempt, current delay) after a transient failure.
    backoff: Mutex<HashMap<String, (i64, i64)>>,
    /// Identity (`account_id`) → last time it was seen signed in to the ordinary Claude Code.
    seen_active: Mutex<HashMap<String, i64>>,
    profile_endpoint: Mutex<String>,
    /// Where rejected lineages are remembered across restarts (fingerprints only).
    journal: Mutex<Option<std::path::PathBuf>>,
    /// Identities (`account_id`) a running Claude Swap holds: it renews them, Switchboard does not.
    swap_held: Mutex<std::collections::HashSet<String>>,
    /// The refresh token a grant spent → its successor, until every holder has it. Shared with
    /// the grant task, which records the successor before anyone can drop it.
    stash: Arc<Mutex<HashMap<String, Kept>>>,
    /// Refresh-token fingerprint → the identity `/api/oauth/profile` reported for it.
    owners: Mutex<HashMap<String, ExternalIdentity>>,
    /// The lineage last asked about without an answer, and when: asked again after a minute.
    profile_tried: Mutex<Option<(String, i64)>>,
}
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
    pub(crate) fn at(endpoint: String) -> Self {
        Self {
            endpoint: Mutex::new(endpoint),
            dead: Mutex::new(HashMap::new()),
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
            stash: Arc::new(Mutex::new(HashMap::new())),
            swap_held: Mutex::new(Default::default()),
            journal: Mutex::new(None),
        }
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
    pub(crate) fn sign_in_required(&self, store: &Store) -> Vec<String> {
        let Ok(dead) = self.dead.lock() else {
            return vec![];
        };
        let mut ids: Vec<String> = dead
            .iter()
            .filter(|(id, hash)| {
                store.stored_credential(id).is_ok_and(|c| {
                    c.refresh_token.as_deref().map(fingerprint).as_deref() == Some(hash.as_str())
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }
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

/// Refreshes one account if it is inactive and due (or `force`, after a 401).
pub(crate) async fn ensure_fresh(
    store: &Store,
    native: NativeSources,
    state: &RefreshState,
    id: &str,
    force: bool,
) -> Outcome {
    let time = now();
    let Some(account) = store
        .snapshot()
        .ok()
        .and_then(|s| s.accounts.into_iter().find(|a| a.id == id))
    else {
        return Outcome::NotNeeded;
    };
    if account.provider != Provider::Claude || account.kind != AuthKind::OAuth {
        return Outcome::NotNeeded;
    }
    let Ok(credential) = store.stored_credential(id) else {
        return Outcome::NotNeeded;
    };
    // A successor that could not be stored yet goes first; the stored token is already spent.
    if let Some(outcome) = adopt_stash(store, state, id) {
        return outcome;
    }
    let Some(refresh_token) = credential.refresh_token.clone() else {
        return Outcome::NotNeeded;
    };
    // A rejected lineage is reported before anything else, due or not.
    if state.is_dead(store, id) {
        return Outcome::SignInRequired;
    }
    // Without a captured identity nothing proves this is not the live Claude Code lineage
    // in an older generation; renewing it could spend a token Claude Code still needs.
    if account
        .external_identity
        .as_ref()
        .is_none_or(|i| i.account_id.is_none())
    {
        return Outcome::NotNeeded;
    }
    // A running Claude Swap renews this lineage itself; a second renewer would spend its token.
    if account
        .external_identity
        .as_ref()
        .and_then(|i| i.account_id.as_ref())
        .is_some_and(|id| state.swap_held.lock().is_ok_and(|held| held.contains(id)))
    {
        return Outcome::NotNeeded;
    }
    if !force && !due(&credential, time) {
        return Outcome::NotNeeded;
    }
    if state
        .backoff
        .lock()
        .ok()
        .and_then(|b| b.get(id).copied())
        .is_some_and(|(next, _)| time < next)
    {
        return Outcome::NotNeeded;
    }
    if state.recently_active(&account, time) || is_active(native, state, &account, &credential) {
        return Outcome::Active;
    }
    match spend(state, &credential).await {
        Ok(kept) => {
            if let Ok(mut backoff) = state.backoff.lock() {
                backoff.remove(id);
            }
            // The token endpoint names whose token it issued: only that account's copies get
            // the successor, and a copy of another account holding it is a dead lineage here
            // (Claude Swap `autoswitch.py:836-877`).
            settled(store, state, id, &refresh_token, &kept)
        }
        Err(Failure::Permanent) => remember_dead(state, id, &refresh_token),
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
/// One refresh grant whose successor survives its caller: the grant runs in its own task and
/// records the successor before returning, so a deadline, a dropped control request or a
/// cancelled caller can never lose a token the provider has already spent.
async fn spend(state: &RefreshState, credential: &Credential) -> Result<Kept, Failure> {
    let Some(consumed) = credential.refresh_token.clone() else {
        return Err(Failure::Transient);
    };
    let endpoint = state.endpoint.lock().map(|e| e.clone()).unwrap_or_default();
    let stash = state.stash.clone();
    let credential = credential.clone();
    tokio::spawn(async move {
        let (refreshed, owner) = grant(&endpoint, &credential).await?;
        let kept = Kept { refreshed, owner };
        if let Ok(mut stash) = stash.lock() {
            stash.insert(consumed, kept.clone());
        }
        Ok(kept)
    })
    .await
    .unwrap_or(Err(Failure::Transient))
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
    let (updated, others) = store
        .adopt_refreshed_for(
            Provider::Claude,
            consumed,
            &kept.refreshed,
            kept.owner.as_deref(),
        )
        .map_err(|_| ())?;
    for other in &others {
        remember_dead(state, other, consumed);
    }
    if let Ok(mut stash) = state.stash.lock() {
        stash.remove(consumed);
    }
    Ok(updated)
}
/// What settling a successor means for the account `id` that held the spent token.
fn settled(store: &Store, state: &RefreshState, id: &str, consumed: &str, kept: &Kept) -> Outcome {
    match settle(store, state, consumed, kept) {
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
        return Err(
            if matches!(status, 400 | 401 | 403)
                && value.get("error") == Some(&json!("invalid_grant"))
            {
                Failure::Permanent
            } else {
                Failure::Transient
            },
        );
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
            Err(Failure::Transient) => return Outcome::Transient,
        },
    };
    let Some(item) = renewed_item(&before, &kept.refreshed) else {
        return Outcome::Transient;
    };
    // A failed write leaves the successor kept; the next pass writes it before anything else.
    if lock.write(&item).is_err() {
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
    Profiles(Vec<crate::external::CapturedProfile>),
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
        SwapView::Profiles(profiles) => profiles,
    };
    let mut held = std::collections::HashSet::new();
    let mut updated = 0;
    for profile in profiles {
        let Some(id) = profile.identity.account_id.clone() else {
            continue;
        };
        held.insert(id);
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
    if let Ok(mut current) = state.swap_held.lock() {
        *current = held;
    }
    updated
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

/// Decides from stored copies and the cached provider answer; never touches the network.
pub(crate) fn lineage(
    store: &Store,
    state: &RefreshState,
    identity: &ExternalIdentity,
    credential: &Credential,
) -> Lineage {
    let Some(token) = credential.refresh_token.as_deref() else {
        return Lineage::Unresolved;
    };
    let Ok(snapshot) = store.snapshot() else {
        return Lineage::Unresolved;
    };
    let same = |other: Option<&ExternalIdentity>| {
        other.is_some_and(|o| {
            o.account_id.is_some()
                && o.account_id == identity.account_id
                && o.organization_id == identity.organization_id
        })
    };
    for account in snapshot
        .accounts
        .iter()
        .filter(|a| a.provider == Provider::Claude && a.kind == AuthKind::OAuth)
    {
        if store
            .stored_credential(&account.id)
            .is_ok_and(|c| c.refresh_token.as_deref() == Some(token))
        {
            return if same(account.external_identity.as_ref()) {
                Lineage::Own
            } else {
                Lineage::Foreign
            };
        }
    }
    match state.owner(token) {
        // The same login in another organization is another account.
        Some(owner)
            if owner.account_id == identity.account_id
                && (owner.organization_id.is_none()
                    || identity.organization_id.is_none()
                    || owner.organization_id == identity.organization_id) =>
        {
            Lineage::Own
        }
        Some(_) => Lineage::Foreign,
        None => Lineage::Unresolved,
    }
}
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

/// Asks `/api/oauth/profile` once per unknown lineage of the live Claude Code sign-in, outside
/// every lock (5-second timeout). Silent on failure: the answer stays Unresolved.
pub(crate) async fn learn_live_owner(store: &Store, native: NativeSources, state: &RefreshState) {
    let Ok(live) = (native.current)(Provider::Claude) else {
        return;
    };
    let Some(token) = live.credential.refresh_token.clone() else {
        return;
    };
    if lineage(store, state, &live.identity, &live.credential) != Lineage::Unresolved {
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
    use std::sync::{atomic::AtomicUsize, atomic::Ordering, Arc};

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
    };
    const SIGNED_OUT: NativeSources = NativeSources {
        current: signed_out,
        activate: activates,
        live: crate::no_live,
    };
    const UNREADABLE: NativeSources = NativeSources {
        current: unreadable,
        activate: activates,
        live: crate::no_live,
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
            follow_claude_swap(&store, &state, SwapView::Profiles(vec![profile])),
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
