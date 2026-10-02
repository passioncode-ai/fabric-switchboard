//! Refresh of **inactive** Claude OAuth accounts, as Claude Swap does (PLAN-0.5 D-2).
//! The identity signed in to the ordinary Claude Code is never refreshed here: Claude Code
//! owns that lineage, and a second refresher would rotate its token away. Callers hold the
//! owner's mutation lock, so an activation cannot hand Claude Code a token mid-grant.
use crate::NativeSources;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use switchboard_core::{Account, AuthKind, Credential, Provider, Store};

pub(crate) const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
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

/// In-memory refresh bookkeeping of one owner process.
pub(crate) struct RefreshState {
    endpoint: Mutex<String>,
    /// Account id → hash of the refresh token the provider rejected.
    dead: Mutex<HashMap<String, String>>,
    /// Account id → (next attempt, current delay) after a transient failure.
    backoff: Mutex<HashMap<String, (i64, i64)>>,
    /// Identity (`account_id`) → last time it was seen signed in to the ordinary Claude Code.
    seen_active: Mutex<HashMap<String, i64>>,
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
        }
    }
    #[cfg(test)]
    pub(crate) fn set_endpoint(&self, endpoint: String) {
        *self.endpoint.lock().unwrap() = endpoint;
    }
    /// Records the identity the ordinary Claude Code uses now (from any observation).
    pub(crate) fn note_active(&self, identity: &switchboard_core::ExternalIdentity) {
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
    let endpoint = state.endpoint.lock().map(|e| e.clone()).unwrap_or_default();
    match grant(&endpoint, &credential).await {
        Ok(refreshed) => {
            if let Ok(mut backoff) = state.backoff.lock() {
                backoff.remove(id);
            }
            match store.adopt_refreshed(Provider::Claude, &refresh_token, &refreshed) {
                Ok(_) => Outcome::Refreshed,
                // The grant succeeded but the new generation could not be stored: the
                // stored token is now spent, which is exactly what a sign-in recovers.
                Err(_) => remember_dead(state, id, &refresh_token),
            }
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
fn remember_dead(state: &RefreshState, id: &str, token: &str) -> Outcome {
    if let Ok(mut dead) = state.dead.lock() {
        dead.insert(id.to_owned(), fingerprint(token));
    }
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
pub(crate) async fn grant(endpoint: &str, credential: &Credential) -> Result<Credential, Failure> {
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
    apply(credential, &value, now()).ok_or(Failure::Transient)
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
    };
    const SIGNED_OUT: NativeSources = NativeSources {
        current: signed_out,
        activate: activates,
    };
    const UNREADABLE: NativeSources = NativeSources {
        current: unreadable,
        activate: activates,
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
        ) -> Result<(), String> {
            panic!("a dead lineage reached Claude Code")
        }
        let native = NativeSources {
            current: signed_in,
            activate: never,
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
