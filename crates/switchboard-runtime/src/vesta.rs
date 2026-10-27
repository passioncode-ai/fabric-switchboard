//! SB-159 (SCN-071): the person's organizations from PassionCode.ai (Vesta, `org.passioncode.ai`).
//!
//! Connecting is a browser sign-in with an RFC 8252 loopback redirect and PKCE (S256): Switchboard
//! listens on `http://127.0.0.1:<port>/callback`, opens the consent page in the system browser and
//! exchanges the code for a token that reads `GET /v1/me/organizations` only. The refresh token
//! lives in the OS vault (`Store::put_secret`), rotated on every use; `vesta.json` (0600) holds the
//! vault item, the server and when it was read, never a token. Reading happens at owner start and
//! at most every 6 hours; a failed read keeps the last organizations and says which step is next.
//! Without a connection nothing here runs and nothing leaves the computer.
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest as _;
use std::path::{Path, PathBuf};
use std::time::Duration;
use switchboard_core::{organizations::VestaOrganization, Store};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const BASE: &str = "https://org.passioncode.ai";
const CLIENT: &str = "switchboard";
const SCOPE: &str = "me.organizations";
const FILE: &str = "vesta.json";
/// How often the organizations are read again.
pub(crate) const EVERY: i64 = 6 * 3600;
/// How long the browser has to answer.
const WAIT: Duration = Duration::from_secs(300);
const REQUEST: Duration = Duration::from_secs(15);
const MAX_HEAD: usize = 8 * 1024;
/// One read, connect or disconnect at a time in this process: a refresh token is single use, and
/// Vesta revokes the whole grant when a rotated one is presented again.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub const VESTA_DENIED: &str = "PassionCode.ai was not connected: access was denied.";
pub const VESTA_TIMEOUT: &str =
    "PassionCode.ai did not answer in the browser within 5 minutes. Nothing changed.";
pub const VESTA_CANCELLED: &str = "Connecting PassionCode.ai was cancelled. Nothing changed.";
pub const VESTA_FAILED: &str = "PassionCode.ai could not be connected. Try again.";
pub const VESTA_NOT_CONNECTED: &str = "PassionCode.ai is not connected.";
pub const VESTA_BUSY: &str = "PassionCode.ai is already waiting for your browser.";
const VESTA_UNREADABLE: &str =
    "The PassionCode.ai connection could not be read. Disconnect and connect again.";

/// Where a connection stands; the window and the CLI turn it into a sentence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The last read succeeded.
    Ok,
    /// The grant ended, was revoked or was refused: connect again.
    SignInAgain,
    /// The person has not signed in through their identity provider lately: sign in to
    /// PassionCode.ai in the browser, then read again; the tokens are kept.
    ProviderSignIn,
    /// PassionCode.ai could not be reached; the last organizations stay.
    Unreachable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    /// The vault item holding the refresh token.
    item: String,
    base: String,
    connected_at: i64,
    #[serde(default)]
    read_at: Option<i64>,
    #[serde(default)]
    checked_at: Option<i64>,
    state: State,
    #[serde(default)]
    organizations: usize,
    /// SB-160 (B-7, SCN-079): organizations a read no longer listed, by local id with the time
    /// it first did; never a name. One linked again, or removed by the person, leaves the list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    released: Vec<Released>,
}

/// An organization the person left on PassionCode.ai: kept here, marked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Released {
    /// The local organization's id.
    organization: String,
    at: i64,
}

/// Brings `released` up to date after a read: an organization the read unlinked joins it with
/// `at`, one linked again or no longer here leaves it. Answers how many joined.
fn mark_released(
    released: &mut Vec<Released>,
    linked_before: &[String],
    organizations: &[switchboard_core::organizations::Organization],
    at: i64,
) -> usize {
    released.retain(|r| {
        organizations
            .iter()
            .any(|o| o.id == r.organization && o.vesta_id.is_none())
    });
    let mut joined = 0;
    for o in organizations {
        if o.vesta_id.is_none()
            && linked_before.contains(&o.id)
            && !released.iter().any(|r| r.organization == o.id)
        {
            released.push(Released {
                organization: o.id.clone(),
                at,
            });
            joined += 1;
        }
    }
    joined
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}
fn read_link(root: &Path) -> Result<Option<Link>, String> {
    let file = path(root);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    let bytes = switchboard_core::private_fs::read_private(&file, 16 * 1024)
        .map_err(|_| VESTA_UNREADABLE)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| VESTA_UNREADABLE.into())
}
fn write_link(root: &Path, link: &Link) -> Result<(), String> {
    let bytes = serde_json::to_vec(link).map_err(|_| VESTA_FAILED)?;
    switchboard_core::private_fs::private_write(&path(root), &bytes)
}

/// The verifier (64 unreserved characters) and its S256 challenge.
pub(crate) fn pkce() -> (String, String) {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Opens `url` in the person's default browser and returns at once.
pub fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg(url);
        c
    };
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "The browser could not be opened.")?;
    // Reaped in the background: the opener ends in a moment, the browser is not its child.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn page(title: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>Fabric Switchboard</title>\
         <body style=\"font:16px system-ui;margin:3em\"><p>{title}</p></body>"
    )
}

/// Reads one request head from `stream` and answers it; the code when it is the callback.
async fn one_request(
    stream: &mut tokio::net::TcpStream,
    state: &str,
    base: &str,
) -> Option<Result<String, String>> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < MAX_HEAD {
            match stream.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&buf[..n]),
            }
        }
    })
    .await;
    let text = String::from_utf8_lossy(&head);
    let target = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("GET "))
        .and_then(|l| l.split(' ').next())
        .unwrap_or("");
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok();
    let (status, body, outcome) = match url.filter(|_| read.is_ok()) {
        Some(url) if url.path() == "/callback" => {
            let get = |k: &str| {
                url.query_pairs()
                    .find(|(name, _)| name == k)
                    .map(|(_, v)| v.into_owned())
            };
            if get("state").as_deref() != Some(state) {
                // Not this sign-in's: ignored, the wait goes on.
                (
                    "400 Bad Request",
                    page("This link is not for this sign-in."),
                    None,
                )
            } else if get("iss").is_none_or(|iss| iss.trim_end_matches('/') != base) {
                (
                    "400 Bad Request",
                    page("This answer did not come from PassionCode.ai."),
                    Some(Err(VESTA_FAILED.into())),
                )
            } else if let Some(error) = get("error") {
                let message = if error == "access_denied" {
                    VESTA_DENIED
                } else {
                    VESTA_FAILED
                };
                (
                    "200 OK",
                    page("Switchboard was not connected. You can close this tab."),
                    Some(Err(message.into())),
                )
            } else if let Some(code) = get("code").filter(|c| !c.is_empty() && c.len() <= 512) {
                (
                    "200 OK",
                    page("Switchboard is connected. You can close this tab."),
                    Some(Ok(code)),
                )
            } else {
                (
                    "400 Bad Request",
                    page("Switchboard was not connected. You can close this tab."),
                    Some(Err(VESTA_FAILED.into())),
                )
            }
        }
        _ => ("404 Not Found", page("Not found."), None),
    };
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
    outcome
}

async fn callback(listener: &TcpListener, state: &str, base: &str) -> Result<String, String> {
    loop {
        let (mut stream, peer) = listener.accept().await.map_err(|_| VESTA_FAILED)?;
        if !peer.ip().is_loopback() {
            continue;
        }
        if let Some(outcome) = one_request(&mut stream, state, base).await {
            return outcome;
        }
    }
}

struct Tokens {
    access: String,
    refresh: String,
}
enum TokenError {
    /// The code, the grant or the refresh token was refused.
    Grant,
    Unreachable,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| VESTA_FAILED.into())
}

async fn token(base: &str, form: &[(&str, &str)]) -> Result<Tokens, TokenError> {
    #[derive(Deserialize)]
    struct Answer {
        access_token: String,
        refresh_token: String,
    }
    let client = client().map_err(|_| TokenError::Unreachable)?;
    let response = client
        .post(format!("{base}/v1/app-tokens/token"))
        .form(form)
        .send()
        .await
        .map_err(|_| TokenError::Unreachable)?;
    let status = response.status();
    if status.is_success() {
        let answer: Answer = response.json().await.map_err(|_| TokenError::Unreachable)?;
        if answer.access_token.is_empty() || answer.refresh_token.is_empty() {
            return Err(TokenError::Unreachable);
        }
        return Ok(Tokens {
            access: answer.access_token,
            refresh: answer.refresh_token,
        });
    }
    if status == reqwest::StatusCode::BAD_REQUEST || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(TokenError::Grant);
    }
    Err(TokenError::Unreachable)
}

/// Connects: the browser sign-in, the exchange, then the first read. `open` is handed the
/// consent URL (the system browser in the app; a synthetic browser in tests).
pub async fn connect(
    root: &Path,
    store: &Store,
    base: &str,
    open: impl FnOnce(&str) -> Result<(), String>,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<Value, String> {
    let base = base.trim_end_matches('/');
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| VESTA_FAILED)?;
    let port = listener.local_addr().map_err(|_| VESTA_FAILED)?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let (verifier, challenge) = pkce();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let url = reqwest::Url::parse_with_params(
        &format!("{base}/auth/app"),
        &[
            ("client_id", CLIENT),
            ("redirect_uri", redirect.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            ("scope", SCOPE),
        ],
    )
    .map_err(|_| VESTA_FAILED)?;
    open(url.as_str())?;
    let code = tokio::select! {
        code = callback(&listener, &state, base) => code?,
        _ = tokio::time::sleep(WAIT) => return Err(VESTA_TIMEOUT.into()),
        _ = cancel => return Err(VESTA_CANCELLED.into()),
    };
    drop(listener);
    let _turn = TURN.lock().await;
    let tokens = token(
        base,
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", CLIENT),
        ],
    )
    .await
    .map_err(|_| VESTA_FAILED)?;
    let previous = read_link(root).ok().flatten();
    let item = store.put_secret(previous.as_ref().map(|l| l.item.as_str()), &tokens.refresh)?;
    let mut link = Link {
        item,
        base: base.to_owned(),
        connected_at: now(),
        read_at: None,
        checked_at: None,
        state: State::Ok,
        organizations: 0,
        // Connecting again is not leaving or rejoining: what earlier reads found stays.
        released: previous.map(|l| l.released).unwrap_or_default(),
    };
    write_link(root, &link)?;
    read_organizations(root, store, &mut link, &tokens.access).await
}

async fn read_organizations(
    root: &Path,
    store: &Store,
    link: &mut Link,
    access: &str,
) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Row {
        #[serde(rename = "orgId")]
        org_id: String,
        name: String,
        #[serde(default)]
        domains: Vec<String>,
    }
    #[derive(Deserialize)]
    struct Answer {
        organizations: Vec<Row>,
    }
    link.checked_at = Some(now());
    let response = match client()?
        .get(format!("{}/v1/me/organizations", link.base))
        .bearer_auth(access)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            link.state = State::Unreachable;
            write_link(root, link)?;
            return Ok(status_of(Some(link), None));
        }
    };
    let code = response.status();
    if code == reqwest::StatusCode::UNAUTHORIZED {
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let reason = body["error"]["code"]
            .as_str()
            .or_else(|| body["code"].as_str())
            .or_else(|| body["error"].as_str())
            .unwrap_or("");
        link.state = if reason == "idp-sign-in-required" {
            State::ProviderSignIn
        } else {
            State::SignInAgain
        };
        write_link(root, link)?;
        return Ok(status_of(Some(link), None));
    }
    let answer: Option<Answer> = if code.is_success() {
        response.json().await.ok()
    } else {
        None
    };
    let Some(answer) = answer else {
        link.state = State::Unreachable;
        write_link(root, link)?;
        return Ok(status_of(Some(link), None));
    };
    let read: Vec<VestaOrganization> = answer
        .organizations
        .into_iter()
        .map(|r| VestaOrganization {
            id: r.org_id,
            name: r.name,
            domains: r.domains,
        })
        .collect();
    let linked_before: Vec<String> = store
        .snapshot()
        .map(|snap| {
            snap.organizations
                .iter()
                .filter(|o| o.vesta_id.is_some())
                .map(|o| o.id.clone())
                .collect()
        })
        .unwrap_or_default();
    let applied = match store.apply_vesta(&read) {
        Ok(applied) => applied,
        Err(error) => {
            // Recorded as tried, so the monitor does not rotate the token every pass.
            link.state = State::Unreachable;
            write_link(root, link)?;
            return Err(error);
        }
    };
    if let Ok(snap) = store.snapshot() {
        let joined = mark_released(
            &mut link.released,
            &linked_before,
            &snap.organizations,
            link.checked_at.unwrap_or_else(now),
        );
        if joined > 0 {
            crate::oplog::event(
                "organization_released",
                &[("count", crate::oplog::Field::Number(joined as i64))],
            );
        }
    }
    link.state = State::Ok;
    link.read_at = link.checked_at;
    link.organizations = read.len();
    write_link(root, link)?;
    Ok(status_of(Some(link), Some(json!(applied))))
}

/// Reads the organizations again: a refresh (rotated, saved before it is used), then the read.
pub async fn sync(root: &Path, store: &Store) -> Result<Value, String> {
    let _turn = TURN.lock().await;
    read_now(root, store).await
}

/// The monitor's read: only while it is still due and nothing may stop it, decided after the
/// turn, so reads spawned on consecutive passes never queue up a second refresh.
pub(crate) async fn sync_if_due(
    root: &Path,
    store: &Store,
    allowed: impl Fn() -> bool,
) -> Option<Result<Value, String>> {
    let _turn = TURN.lock().await;
    if !allowed() || !due(root, now()) {
        return None;
    }
    Some(read_now(root, store).await)
}

async fn read_now(root: &Path, store: &Store) -> Result<Value, String> {
    let mut link = read_link(root)?.ok_or(VESTA_NOT_CONNECTED)?;
    link.checked_at = Some(now());
    let refresh = match store.secret(&link.item) {
        Ok(refresh) => refresh,
        Err(error) => {
            // Recorded as tried: a vault that would not answer (locked, or its item gone) is
            // not asked again on every pass.
            link.state = State::Unreachable;
            write_link(root, &link)?;
            return Err(error);
        }
    };
    let tokens = match token(
        &link.base,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", CLIENT),
        ],
    )
    .await
    {
        Ok(t) => t,
        Err(TokenError::Grant) => {
            link.state = State::SignInAgain;
            write_link(root, &link)?;
            return Ok(status_of(Some(&link), None));
        }
        Err(TokenError::Unreachable) => {
            link.state = State::Unreachable;
            write_link(root, &link)?;
            return Ok(status_of(Some(&link), None));
        }
    };
    // The old refresh token stopped working the moment this one was issued.
    if let Err(error) = store.put_secret(Some(&link.item), &tokens.refresh) {
        link.state = State::SignInAgain;
        write_link(root, &link)?;
        return Err(error);
    }
    read_organizations(root, store, &mut link, &tokens.access).await
}

/// Whether a read is due: connected, and not tried for `EVERY` (a connection waiting for the
/// person is not retried on its own).
pub(crate) fn due(root: &Path, at: i64) -> bool {
    read_link(root).ok().flatten().is_some_and(|l| {
        matches!(l.state, State::Ok | State::Unreachable)
            && l.checked_at.is_none_or(|c| at - c >= EVERY)
    })
}

fn status_of(link: Option<&Link>, applied: Option<Value>) -> Value {
    match link {
        None => json!({"connected": false, "base": BASE}),
        Some(l) => json!({
            "connected": true, "base": l.base, "state": l.state,
            "connected_at": l.connected_at, "read_at": l.read_at, "checked_at": l.checked_at,
            "organizations": l.organizations, "applied": applied,
            "released": l.released,
        }),
    }
}

/// The vault item of the connection's token, for uninstall.
pub(crate) fn vault_item(root: &Path) -> Option<String> {
    read_link(root).ok().flatten().map(|l| l.item)
}

pub fn status(root: &Path) -> Result<Value, String> {
    Ok(status_of(read_link(root)?.as_ref(), None))
}

/// Deletes the token and forgets the connection; the organizations stay as local ones. The grant
/// itself is the person's to revoke in My data on PassionCode.ai.
pub async fn disconnect(root: &Path, store: &Store) -> Result<Value, String> {
    let _turn = TURN.lock().await;
    let link = read_link(root).ok().flatten();
    if let Some(link) = &link {
        // Deleting an item already gone succeeds; a failure is real (a locked keychain), and the
        // connection stays rather than orphaning its token.
        store.delete_secret(&link.item)?;
    }
    store.release_vesta()?;
    match std::fs::remove_file(path(root)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(VESTA_FAILED.into()),
    }
    Ok(json!({"connected": false, "disconnected": link.is_some(), "base": BASE}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State as Axum,
        routing::{get, post},
        Form, Json, Router,
    };
    use std::sync::{Arc, Mutex};
    use switchboard_core::MemoryVault;

    #[derive(Default)]
    struct Server {
        challenge: Mutex<String>,
        refresh: Mutex<String>,
        /// What the organizations read answers: 200, or 401 with this code.
        refuse: Mutex<Option<&'static str>>,
        name: Mutex<String>,
        /// Refresh tokens already rotated; presenting one again revokes the grant, as Vesta does.
        rotated: Mutex<Vec<String>>,
        revoked: std::sync::atomic::AtomicBool,
        /// The person left every organization on PassionCode.ai: the read lists none.
        left: std::sync::atomic::AtomicBool,
    }

    async fn serve(server: Arc<Server>) -> String {
        async fn token(
            Axum(s): Axum<Arc<Server>>,
            Form(f): Form<std::collections::HashMap<String, String>>,
        ) -> (axum::http::StatusCode, Json<Value>) {
            let ok = match f.get("grant_type").map(String::as_str) {
                Some("authorization_code") => {
                    let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(sha2::Sha256::digest(verifier.as_bytes()));
                    f.get("code").map(String::as_str) == Some("vapc_synthetic")
                        && challenge == *s.challenge.lock().unwrap()
                        && f.get("client_id").map(String::as_str) == Some("switchboard")
                }
                Some("refresh_token") => {
                    // Time for a second read to present the same token.
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let presented = f.get("refresh_token").cloned().unwrap_or_default();
                    if s.rotated.lock().unwrap().contains(&presented) {
                        s.revoked.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    let current = s.refresh.lock().unwrap().clone();
                    let ok = !s.revoked.load(std::sync::atomic::Ordering::SeqCst)
                        && presented == current;
                    if ok {
                        s.rotated.lock().unwrap().push(presented);
                    }
                    ok
                }
                _ => false,
            };
            if !ok {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_grant"})),
                );
            }
            let next = format!("vapr_{}", uuid::Uuid::new_v4().simple());
            *s.refresh.lock().unwrap() = next.clone();
            (
                axum::http::StatusCode::OK,
                Json(
                    json!({"access_token": "vapt_synthetic", "token_type": "Bearer",
                    "expires_in": 3600, "refresh_token": next, "scope": "me.organizations"}),
                ),
            )
        }
        async fn orgs(
            Axum(s): Axum<Arc<Server>>,
            headers: axum::http::HeaderMap,
        ) -> (axum::http::StatusCode, Json<Value>) {
            if let Some(code) = *s.refuse.lock().unwrap() {
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    Json(json!({"error": {"code": code}})),
                );
            }
            if headers.get("authorization").and_then(|v| v.to_str().ok())
                != Some("Bearer vapt_synthetic")
            {
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    Json(json!({"error": {"code": "app-token-invalid"}})),
                );
            }
            let name = s.name.lock().unwrap().clone();
            if s.left.load(std::sync::atomic::Ordering::SeqCst) {
                return (
                    axum::http::StatusCode::OK,
                    Json(json!({"organizations": []})),
                );
            }
            (
                axum::http::StatusCode::OK,
                Json(json!({"organizations": [
                    {"orgId": "v-acme", "name": name, "domains": ["acme.example"], "role": "member", "current": true}
                ]})),
            )
        }
        let app = Router::new()
            .route("/v1/app-tokens/token", post(token))
            .route("/v1/me/organizations", get(orgs))
            .with_state(server);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    /// A browser that answers the consent page with `answer` (`code=…` or `error=…`).
    fn browser(
        server: Arc<Server>,
        base: String,
        answer: &'static str,
    ) -> impl FnOnce(&str) -> Result<(), String> {
        move |url: &str| {
            let url = reqwest::Url::parse(url).unwrap();
            let get = |k: &str| {
                url.query_pairs()
                    .find(|(n, _)| n == k)
                    .unwrap()
                    .1
                    .into_owned()
            };
            assert!(url.as_str().starts_with(&format!("{base}/auth/app?")));
            assert_eq!(get("code_challenge_method"), "S256");
            *server.challenge.lock().unwrap() = get("code_challenge");
            let back = format!(
                "{}?{answer}&state={}&iss={base}",
                get("redirect_uri"),
                get("state")
            );
            tokio::spawn(async move {
                // A stray request first: not the callback, ignored.
                let _ = reqwest::get(back.replace("/callback?", "/favicon.ico?")).await;
                let _ = reqwest::get(back).await;
            });
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_browser_sign_in_reads_the_organizations_and_a_refresh_keeps_them_current() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let server = Arc::new(Server::default());
        *server.name.lock().unwrap() = "Acme".into();
        let base = serve(server.clone()).await;
        assert_eq!(status(root.path()).unwrap()["connected"], false);
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let connected = connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_synthetic"),
            cancel,
        )
        .await
        .unwrap();
        assert_eq!(connected["state"], "ok", "{connected}");
        assert_eq!(connected["organizations"], 1);
        let snap = store.snapshot().unwrap();
        assert_eq!(snap.organizations[0].vesta_id.as_deref(), Some("v-acme"));
        // The token is in the vault, never in the file.
        let file = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        assert!(!file.contains("vapr_") && !file.contains("vapt_"), "{file}");
        // A read later: the refresh rotates, a rename arrives.
        *server.name.lock().unwrap() = "Acme Inc".into();
        let read = sync(root.path(), &store).await.unwrap();
        assert_eq!(read["state"], "ok");
        assert_eq!(store.snapshot().unwrap().organizations[0].name, "Acme Inc");
        assert!(!due(root.path(), now()));
        assert!(due(root.path(), now() + EVERY));
        // The provider sign-in is stale: the tokens stay, the state says what to do.
        *server.refuse.lock().unwrap() = Some("idp-sign-in-required");
        assert_eq!(
            sync(root.path(), &store).await.unwrap()["state"],
            "provider_sign_in"
        );
        *server.refuse.lock().unwrap() = Some("app-token-invalid");
        assert_eq!(
            sync(root.path(), &store).await.unwrap()["state"],
            "sign_in_again"
        );
        assert!(!due(root.path(), now() + EVERY), "waits for the person");
        // A refresh token the server no longer accepts: connect again.
        *server.refuse.lock().unwrap() = None;
        *server.refresh.lock().unwrap() = "vapr_other".into();
        assert_eq!(
            sync(root.path(), &store).await.unwrap()["state"],
            "sign_in_again"
        );
        // Disconnect: the file goes, the organizations stay.
        let gone = disconnect(root.path(), &store).await.unwrap();
        assert_eq!(gone["disconnected"], true);
        assert_eq!(status(root.path()).unwrap()["connected"], false);
        assert_eq!(store.snapshot().unwrap().organizations.len(), 1);
        assert_eq!(
            sync(root.path(), &store).await.unwrap_err(),
            VESTA_NOT_CONNECTED
        );
    }

    // SB-160 (B-7, SCN-079): a read that no longer lists an organization records it as left, with
    // its local id and the time only; the organization stays, and rejoining clears the mark.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_organization_left_on_passioncode_is_marked_with_its_date() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let server = Arc::new(Server::default());
        *server.name.lock().unwrap() = "Acme".into();
        let base = serve(server.clone()).await;
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_synthetic"),
            cancel,
        )
        .await
        .unwrap();
        let acme = store.snapshot().unwrap().organizations[0].id.clone();
        assert!(status(root.path()).unwrap()["released"]
            .as_array()
            .unwrap()
            .is_empty());
        // The person leaves Acme on PassionCode.ai.
        server.left.store(true, std::sync::atomic::Ordering::SeqCst);
        let before = now();
        let (read, events) = crate::oplog::capture_async(sync(root.path(), &store)).await;
        let read = read.unwrap();
        assert_eq!(read["state"], "ok", "{read}");
        let released = read["released"].as_array().unwrap().clone();
        assert_eq!(released.len(), 1, "{read}");
        assert_eq!(released[0]["organization"], acme.as_str());
        let at = released[0]["at"].as_i64().unwrap();
        assert!(at >= before && at <= now() + 1, "{at}");
        assert_eq!(status(root.path()).unwrap()["released"], json!(released));
        // Nothing is removed: the organization stays, only its link goes.
        let snap = store.snapshot().unwrap();
        assert_eq!(snap.organizations.len(), 1);
        assert_eq!(snap.organizations[0].id, acme);
        assert_eq!(snap.organizations[0].vesta_id, None);
        // The file holds the id and the time, never the name.
        let file = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        assert!(!file.contains("Acme"), "{file}");
        assert_eq!(
            events
                .iter()
                .filter(|(e, _)| *e == "organization_released")
                .map(|(_, f)| f.clone())
                .collect::<Vec<_>>(),
            vec![vec![("count", crate::oplog::Field::Number(1))]]
        );
        // A later read that still lists nothing keeps the first date and logs nothing more.
        let (again, events) = crate::oplog::capture_async(sync(root.path(), &store)).await;
        assert_eq!(again.unwrap()["released"], json!(released));
        assert!(!events.iter().any(|(e, _)| *e == "organization_released"));
        // Connecting again (offered after a refused sign-in) keeps the mark and its first date.
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_synthetic"),
            cancel,
        )
        .await
        .unwrap();
        assert_eq!(status(root.path()).unwrap()["released"], json!(released));
        // Added back: linked again by its name, and the mark goes.
        server
            .left
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let back = sync(root.path(), &store).await.unwrap();
        assert!(back["released"].as_array().unwrap().is_empty(), "{back}");
        assert_eq!(
            store.snapshot().unwrap().organizations[0]
                .vesta_id
                .as_deref(),
            Some("v-acme")
        );
        // Left again and then removed by the person: the mark of a gone organization goes too.
        server.left.store(true, std::sync::atomic::Ordering::SeqCst);
        sync(root.path(), &store).await.unwrap();
        store.remove_organization(&acme).unwrap();
        let after = sync(root.path(), &store).await.unwrap();
        assert!(after["released"].as_array().unwrap().is_empty(), "{after}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_denied_or_unreachable_sign_in_changes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let server = Arc::new(Server::default());
        let base = serve(server.clone()).await;
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let denied = connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "error=access_denied"),
            cancel,
        )
        .await;
        assert_eq!(denied.unwrap_err(), VESTA_DENIED);
        assert!(!root.path().join(FILE).exists());
        // A code the server refuses (wrong verifier): nothing saved.
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let refused = connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_other"),
            cancel,
        )
        .await;
        assert_eq!(refused.unwrap_err(), VESTA_FAILED);
        assert!(!root.path().join(FILE).exists());
        // Cancelled while waiting.
        let (stop, cancel) = tokio::sync::oneshot::channel();
        let _ = stop.send(());
        let cancelled = connect(root.path(), &store, &base, |_| Ok(()), cancel).await;
        assert_eq!(cancelled.unwrap_err(), VESTA_CANCELLED);
        assert!(store.snapshot().unwrap().organizations.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_reads_at_once_spend_one_refresh_token_each() {
        // The seam review of 2026-10-22: two reads sharing one refresh token would replay a
        // rotated one, which Vesta answers by revoking the grant.
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap(),
        );
        let server = Arc::new(Server::default());
        *server.name.lock().unwrap() = "Acme".into();
        let base = serve(server.clone()).await;
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_synthetic"),
            cancel,
        )
        .await
        .unwrap();
        let reads = (0..4)
            .map(|_| {
                let (root, store) = (root.path().to_owned(), store.clone());
                tokio::spawn(async move { sync(&root, &store).await.unwrap()["state"].clone() })
            })
            .collect::<Vec<_>>();
        for read in reads {
            assert_eq!(read.await.unwrap(), "ok");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_vault_that_does_not_answer_is_asked_once_not_every_pass() {
        // Round 2 of the seam review: `uninstall --keep-data` deletes the item and keeps the file.
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let server = Arc::new(Server::default());
        *server.name.lock().unwrap() = "Acme".into();
        let base = serve(server.clone()).await;
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        connect(
            root.path(),
            &store,
            &base,
            browser(server.clone(), base.clone(), "code=vapc_synthetic"),
            cancel,
        )
        .await
        .unwrap();
        store
            .delete_secret(&vault_item(root.path()).unwrap())
            .unwrap();
        let later = now() + EVERY;
        assert!(due(root.path(), later));
        assert!(sync(root.path(), &store).await.is_err());
        assert!(
            !due(root.path(), now()),
            "tried: not again on the next pass"
        );
        assert_eq!(status(root.path()).unwrap()["state"], "unreachable");
        // The monitor's read decides after the turn, and not once the drain began.
        assert!(sync_if_due(root.path(), &store, || true).await.is_none());
        assert!(sync_if_due(root.path(), &store, || false).await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_answer_without_the_issuer_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let open = |url: &str| {
            let url = reqwest::Url::parse(url).unwrap();
            let get = |k: &str| {
                url.query_pairs()
                    .find(|(n, _)| n == k)
                    .unwrap()
                    .1
                    .into_owned()
            };
            let back = format!(
                "{}?code=vapc_synthetic&state={}",
                get("redirect_uri"),
                get("state")
            );
            tokio::spawn(async move {
                let _ = reqwest::get(back).await;
            });
            Ok(())
        };
        let refused = connect(root.path(), &store, "http://127.0.0.1:9", open, cancel).await;
        assert_eq!(refused.unwrap_err(), VESTA_FAILED);
    }

    #[test]
    fn the_challenge_is_the_s256_of_the_verifier() {
        let (verifier, challenge) = pkce();
        assert_eq!(verifier.len(), 64);
        assert!(verifier.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_eq!(
            challenge,
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(sha2::Sha256::digest(verifier.as_bytes()))
        );
        assert_ne!(pkce().0, verifier);
    }
}
