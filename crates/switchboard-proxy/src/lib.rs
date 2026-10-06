//! Credential-injecting loopback gateway. A request owns its route snapshot until EOF.
use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::StreamExt;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use switchboard_core::{AuthKind, Provider, Store, Usage, UsageWindow};
use tokio::{
    sync::{oneshot, Semaphore},
    task::JoinHandle,
};
use uuid::Uuid;

/// The provider's own request ceiling (Anthropic: 32 MB); the proxy must not be the smaller one.
const BODY_LIMIT: usize = 32 * 1024 * 1024;
/// Client headers relayed upstream. Authentication (`authorization`, `x-api-key`), cookies,
/// proxy and account-identity headers are never on this list: the proxy injects its own.
const REQUEST_HEADERS: &[&str] = &[
    "content-type",
    "accept",
    "user-agent",
    "x-app",
    "anthropic-version",
    "anthropic-beta",
    "openai-beta",
    "x-stainless-lang",
    "x-stainless-package-version",
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
    "x-stainless-retry-count",
    "x-stainless-timeout",
];
/// Provider headers relayed back: the client's own retry and limit handling reads them.
const RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "cache-control",
    "retry-after",
    "retry-after-ms",
    "x-should-retry",
    "request-id",
    "x-request-id",
];
const RESPONSE_HEADER_PREFIXES: &[&str] = &["anthropic-ratelimit-", "x-ratelimit-"];
/// Upstream timeouts. There is deliberately no total deadline: a stream that keeps
/// delivering bytes runs to EOF however long it takes.
#[derive(Clone, Copy, Debug)]
struct Timeouts {
    connect: Duration,
    /// Longest silence: before response headers (which includes a non-streaming call's
    /// whole generation) and between body frames.
    read: Duration,
}
impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            read: Duration::from_secs(600),
        }
    }
}
#[derive(Clone)]
struct Gateway {
    store: Arc<Store>,
    token: String,
    /// The capability for third-party agents (`agent_token`): API-key accounts only.
    agent_token: String,
    address: SocketAddr,
    client: reqwest::Client,
    slots: Arc<Semaphore>,
    claude: String,
    openai: String,
    chatgpt: String,
}
pub struct ProxyHandle {
    address: SocketAddr,
    token: String,
    agent_token: String,
    stop: std::sync::Mutex<Option<oneshot::Sender<()>>>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
}
/// How long a start waits for its recorded port to be released by a previous owner.
const REBIND_ATTEMPTS: u32 = 5;
const REBIND_WAIT: Duration = Duration::from_millis(100);
/// The third-party agents' capability, derived from the session token so it survives a restart
/// with it and needs no file of its own. Knowing it never reveals the session token.
pub fn agent_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("switchboard-agents:{token}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
/// Third-party agents may use only accounts the provider sells for any client: API keys.
/// Subscription sign-ins (Claude.ai OAuth, a Claude setup token, ChatGPT) belong to the
/// provider's own client (docs/AGENTS.md → Terms).
pub const AGENT_SUBSCRIPTION_REFUSED: &str = "This pool's selected account is a subscription sign-in, which its provider allows only in Claude Code or Codex. Select an API-key account in this pool for other agents.";
fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
impl ProxyHandle {
    pub async fn start(store: Arc<Store>) -> Result<Self, String> {
        Self::start_on(store, None, None).await
    }
    /// Starts on `port` with `token` when given — the address and capability a managed session
    /// already holds survive a restart (lifecycle LC-11). A port another program holds falls
    /// back to a fresh one; the token is kept either way.
    pub async fn start_on(
        store: Arc<Store>,
        port: Option<u16>,
        token: Option<String>,
    ) -> Result<Self, String> {
        Self::start_at(
            store,
            "https://api.anthropic.com".into(),
            "https://api.openai.com".into(),
            "https://chatgpt.com".into(),
            Timeouts::default(),
            port,
            token,
        )
        .await
    }
    async fn bind(port: Option<u16>) -> Result<tokio::net::TcpListener, String> {
        if let Some(port) = port.filter(|p| *p != 0) {
            for attempt in 0..REBIND_ATTEMPTS {
                match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
                    Ok(listener) => return Ok(listener),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        if attempt + 1 < REBIND_ATTEMPTS {
                            tokio::time::sleep(REBIND_WAIT).await;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Local proxy could not start.".to_string())
    }
    async fn start_at(
        store: Arc<Store>,
        claude: String,
        openai: String,
        chatgpt: String,
        timeouts: Timeouts,
        port: Option<u16>,
        token: Option<String>,
    ) -> Result<Self, String> {
        let listener = Self::bind(port).await?;
        let address = listener
            .local_addr()
            .map_err(|_| "Local proxy address unavailable.")?;
        let token = token
            .filter(|t| valid_token(t))
            .unwrap_or_else(|| format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()));
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read)
            .build()
            .map_err(|_| "HTTP client unavailable.")?;
        let agent = agent_token(&token);
        let state = Gateway {
            store,
            token: token.clone(),
            agent_token: agent.clone(),
            address,
            client,
            slots: Arc::new(Semaphore::new(16)),
            claude,
            openai,
            chatgpt,
        };
        let router = Router::new()
            .route("/{provider}/{pool}/v1/{*operation}", post(relay))
            .route("/claude/{pool}/api/hello", get(connectivity))
            .with_state(state);
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        Ok(Self {
            address,
            token,
            agent_token: agent,
            stop: std::sync::Mutex::new(Some(stop)),
            task: std::sync::Mutex::new(Some(task)),
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    /// The capability third-party agents present (as `x-api-key` or a bearer token).
    pub fn agent_token(&self) -> &str {
        &self.agent_token
    }
    /// Stops accepting, lets requests in flight finish until `deadline`, then cuts them.
    /// True when every connection ended by itself.
    pub async fn stop(&self, deadline: Duration) -> bool {
        if let Some(stop) = self.stop.lock().ok().and_then(|mut s| s.take()) {
            let _ = stop.send(());
        }
        let Some(mut task) = self.task.lock().ok().and_then(|mut t| t.take()) else {
            return true;
        };
        match tokio::time::timeout(deadline, &mut task).await {
            Ok(_) => true,
            Err(_) => {
                task.abort();
                let _ = task.await;
                false
            }
        }
    }
    pub async fn shutdown(self) {
        self.stop(Duration::from_secs(2)).await;
    }
}
impl Drop for ProxyHandle {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.get_mut().ok().and_then(|s| s.take()) {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.get_mut().ok().and_then(|t| t.take()) {
            task.abort();
        }
    }
}
fn connection_fields(headers: &HeaderMap) -> std::collections::HashSet<String> {
    headers
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|v| v.trim().to_ascii_lowercase())
        .collect()
}
struct AuditedStream {
    inner: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>,
    >,
    store: Arc<Store>,
    id: String,
    finished: bool,
    success: bool,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl futures_util::Stream for AuditedStream {
    type Item = Result<axum::body::Bytes, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let poll = self.inner.as_mut().poll_next(cx);
        match &poll {
            // A non-success status was journaled at headers. A completed success is not
            // journaled: it is the common case, and one event per request would evict the
            // activation and rotation history from the bounded journal.
            std::task::Poll::Ready(None) => {
                self.finished = true;
            }
            std::task::Poll::Ready(Some(Err(_))) => {
                self.finished = true;
                if self.success {
                    let _ = self.store.record("proxy", Some(&self.id), "aborted");
                }
            }
            _ => {}
        }
        poll
    }
}
impl Drop for AuditedStream {
    fn drop(&mut self) {
        if !self.finished && self.success {
            let _ = self.store.record("proxy", Some(&self.id), "aborted");
        }
    }
}
fn error(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        axum::Json(serde_json::json!({"error":{"type":"switchboard_error","message":message}})),
    )
        .into_response()
}
async fn connectivity(State(g): State<Gateway>, request: Request<Body>) -> Response {
    if request.headers().contains_key("origin")
        || request.headers().get("host").and_then(|h| h.to_str().ok())
            != Some(g.address.to_string().as_str())
    {
        return error(StatusCode::FORBIDDEN, "Local native clients only.");
    }
    axum::Json(serde_json::json!({"status":"local_proxy_ready"})).into_response()
}
async fn relay(
    State(g): State<Gateway>,
    Path((provider, pool, operation)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Response {
    // Browser-origin calls, host confusion, and upgrades must not reach credential resolution.
    let host = request.headers().get("host").and_then(|h| h.to_str().ok());
    if host != Some(g.address.to_string().as_str()) || request.headers().contains_key("origin") {
        return error(StatusCode::FORBIDDEN, "Local native clients only.");
    }
    if request.headers().contains_key("upgrade") {
        return error(
            StatusCode::NOT_IMPLEMENTED,
            "Managed mode supports HTTP/SSE only.",
        );
    }
    // Claude Code and Codex present the session token as a bearer; other agents present the
    // agent token, as a bearer or as `x-api-key` (what Anthropic SDKs send for an API key).
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Bearer "))
        .unwrap_or_default()
        .as_bytes()
        .to_vec();
    let key = request
        .headers()
        .get("x-api-key")
        .map(|x| x.as_bytes().to_vec())
        .unwrap_or_default();
    // Every comparison runs, whatever matched: no timing tells which header or token was right.
    let official = bearer.ct_eq(g.token.as_bytes());
    let as_agent = bearer.ct_eq(g.agent_token.as_bytes()) | key.ct_eq(g.agent_token.as_bytes());
    let agent = bool::from(!official & as_agent);
    let official = bool::from(official);
    if !official && !agent {
        return error(StatusCode::UNAUTHORIZED, "Local capability required.");
    }
    let query = request.uri().query().map(str::to_owned);
    if (query.is_some() && !(provider == "claude" && query.as_deref() == Some("beta=true")))
        || pool.is_empty()
        || pool.len() > 32
        || !pool
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return error(StatusCode::BAD_REQUEST, "Invalid route.");
    }
    let provider = match (provider.as_str(), operation.as_str()) {
        ("claude", "messages" | "messages/count_tokens") => Provider::Claude,
        // Chat Completions for the many agents that speak only that (OpenAI API keys only).
        ("codex", "responses" | "chat/completions") => Provider::Codex,
        _ => return error(StatusCode::NOT_FOUND, "Unsupported provider operation."),
    };
    let permit = match g.slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Proxy is busy. Retry later.",
            )
        }
    };
    let (account, credential) = match g.store.route(provider, &pool) {
        Ok(route) => route,
        Err(_) => {
            return error(
                StatusCode::CONFLICT,
                "Select a usable account in this pool first.",
            )
        }
    };
    if agent && account.kind != AuthKind::ApiKey {
        return error(StatusCode::FORBIDDEN, AGENT_SUBSCRIPTION_REFUSED);
    }
    if operation == "chat/completions" && account.kind != AuthKind::ApiKey {
        return error(
            StatusCode::NOT_FOUND,
            "Chat Completions needs an OpenAI API-key account in this pool.",
        );
    }
    let (parts, body) = request.into_parts();
    let bytes =
        match tokio::time::timeout(Duration::from_secs(30), to_bytes(body, BODY_LIMIT)).await {
            Ok(Ok(b)) => b,
            Ok(Err(_)) => {
                return error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Request exceeds the body limit.",
                )
            }
            Err(_) => return error(StatusCode::REQUEST_TIMEOUT, "Request body timed out."),
        };
    let url = match (provider, account.kind) {
        (Provider::Claude, _) => format!(
            "{}/v1/{}{}",
            g.claude,
            operation,
            if query.is_some() { "?beta=true" } else { "" }
        ),
        (Provider::Codex, AuthKind::OAuth) => format!("{}/backend-api/codex/responses", g.chatgpt),
        (Provider::Codex, _) => format!("{}/v1/{}", g.openai, operation),
    };
    let mut headers = HeaderMap::new();
    let request_hop_fields = connection_fields(&parts.headers);
    for &name in REQUEST_HEADERS {
        if request_hop_fields.contains(name) {
            continue;
        }
        if let Some(value) = parts.headers.get(name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(
        "content-type",
        axum::http::HeaderValue::from_static("application/json"),
    );
    // Claude headers are settled in the map: `RequestBuilder::header` appends, and a
    // second `anthropic-beta` beside the client's would leave the provider two answers.
    if provider == Provider::Claude && account.kind == AuthKind::OAuth {
        let existing = headers
            .get("anthropic-beta")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let beta = if existing.split(',').any(|v| v.trim() == "oauth-2025-04-20") {
            existing.to_owned()
        } else if existing.is_empty() {
            "oauth-2025-04-20".into()
        } else {
            format!("{existing},oauth-2025-04-20")
        };
        let beta = axum::http::HeaderValue::from_str(&beta)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("oauth-2025-04-20"));
        headers.insert("anthropic-beta", beta);
    }
    if provider == Provider::Claude && !headers.contains_key("anthropic-version") {
        headers.insert(
            "anthropic-version",
            axum::http::HeaderValue::from_static("2023-06-01"),
        );
    }
    let mut outgoing = g.client.post(url).headers(headers).body(bytes);
    if provider == Provider::Claude && account.kind == AuthKind::ApiKey {
        outgoing = outgoing.header("x-api-key", &credential.access_token);
    } else {
        outgoing = outgoing.bearer_auth(&credential.access_token);
    }
    if provider == Provider::Codex && account.kind == AuthKind::OAuth {
        let Some(id) = credential.account_id.as_ref() else {
            return error(
                StatusCode::CONFLICT,
                "Account identity missing. Sign in again.",
            );
        };
        outgoing = outgoing.header("chatgpt-account-id", id);
    }
    let upstream = match outgoing.send().await {
        Ok(r) => r,
        Err(_) => {
            let _ = g
                .store
                .record("request", Some(&account.id), "network_error");
            return error(
                StatusCode::BAD_GATEWAY,
                "Provider connection failed. Request was not replayed.",
            );
        }
    };
    let status = upstream.status();
    if status.is_redirection() {
        return error(StatusCode::BAD_GATEWAY, "Provider redirect refused.");
    }
    let detail = match status.as_u16() {
        200..=299 => "success",
        401 | 403 => "unauthorized",
        429 => "rate_limited",
        _ => "upstream_error",
    };
    if !status.is_success() {
        let _ = g.store.record("request", Some(&account.id), detail);
    }
    if provider == Provider::Claude {
        if let Some(usage) = usage_from_headers(upstream.headers()) {
            let _ = g.store.observe_credential(&account.id, &credential, usage);
        }
    }
    let mut response = Response::builder().status(status);
    let response_hop_fields = connection_fields(upstream.headers());
    for (name, value) in upstream.headers() {
        let name = name.as_str();
        let relayed = RESPONSE_HEADERS.contains(&name)
            || RESPONSE_HEADER_PREFIXES.iter().any(|p| name.starts_with(p));
        if relayed && !response_hop_fields.contains(name) {
            response = response.header(name, value);
        }
    }
    // The permit belongs to the response stream, not just receipt of upstream headers.
    let stream =
        AuditedStream {
            inner: Box::pin(upstream.bytes_stream().map(|item| {
                item.map_err(|_| std::io::Error::other("Provider stream interrupted"))
            })),
            store: g.store.clone(),
            id: account.id,
            finished: false,
            success: status.is_success(),
            _permit: permit,
        };
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| error(StatusCode::BAD_GATEWAY, "Provider response invalid."))
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn usage_from_headers(headers: &HeaderMap) -> Option<Usage> {
    let observed_at = now();
    let mut windows = Vec::new();
    for (window, name) in [("5h", "five_hour"), ("7d", "seven_day")] {
        let header = format!("anthropic-ratelimit-unified-{window}-utilization");
        let Some(raw) = headers.get(header) else {
            continue;
        };
        let used_percent = raw.to_str().ok()?.parse::<f64>().ok()? * 100.;
        if !used_percent.is_finite() || !(0. ..=100.).contains(&used_percent) {
            return None;
        }
        let resets_at = match headers.get(format!("anthropic-ratelimit-unified-{window}-reset")) {
            Some(value) => Some(
                parse_reset(
                    &serde_json::Value::String(value.to_str().ok()?.into()),
                    observed_at,
                )
                .ok()?,
            ),
            None => None,
        };
        windows.push(UsageWindow {
            name: name.into(),
            used_percent,
            resets_at,
        });
    }
    aggregate_usage(windows, observed_at, "response_headers").ok()
}
fn aggregate_usage(
    windows: Vec<UsageWindow>,
    observed_at: i64,
    source: &str,
) -> Result<Usage, String> {
    let worst = windows
        .iter()
        .max_by(|a, b| a.used_percent.total_cmp(&b.used_percent))
        .ok_or("Provider returned no usage windows.")?;
    Ok(Usage {
        used_percent: worst.used_percent,
        resets_at: worst.resets_at,
        windows,
        observed_at,
        source: source.into(),
    })
}
fn parse_reset(value: &serde_json::Value, observed_at: i64) -> Result<i64, String> {
    let parsed = value
        .as_i64()
        .or_else(|| {
            value.as_str().and_then(|s| {
                s.parse::<i64>().ok().or_else(|| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|t| t.timestamp())
                })
            })
        })
        .ok_or(USAGE_RESET_UNSUPPORTED)?;
    if parsed < observed_at || parsed > 253_402_300_799 {
        return Err(USAGE_RESET_UNSUPPORTED.into());
    }
    Ok(parsed)
}

/// The provider refused the token itself (expired or revoked), as opposed to a quota or
/// network failure. A refresh may recover it; callers compare against this exact text.
pub const REJECTED: &str = "Provider rejected the credential. Sign in again.";

/// A failed quota check: a sanitized message, and for a provider 429 how long it asked to wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeFailure {
    pub message: String,
    /// `Some` only for 429: `Some(seconds)` from `Retry-After` (delay-seconds or an HTTP-date,
    /// measured at receipt; a past date is `0`), `Some(None)` when it is absent or malformed.
    pub rate_limited: Option<Option<i64>>,
}
impl From<String> for ProbeFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            rate_limited: None,
        }
    }
}
impl From<&str> for ProbeFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
pub const USAGE_RATE_LIMITED: &str =
    "Usage checks are rate limited by the provider. Switchboard waits before the next one.";
const USAGE_UNAVAILABLE: &str = "Usage check unavailable.";
const USAGE_UNREACHABLE: &str = "Usage check could not reach provider.";
const USAGE_HTTP: &str = "Usage unavailable. Check the account and retry later.";
const USAGE_INTERRUPTED: &str = "Usage response interrupted.";
const USAGE_TOO_LARGE: &str = "Usage response too large.";
const USAGE_UNSUPPORTED: &str = "Usage response unsupported.";
const USAGE_RESET_UNSUPPORTED: &str = "Usage reset unsupported.";
impl ProbeFailure {
    /// A fixed code for the operations log (LC-12): which way the check failed, never the
    /// provider's text or a token. A message this crate does not produce is `other`.
    pub fn kind(&self) -> &'static str {
        match self.message.as_str() {
            REJECTED => "rejected",
            USAGE_RATE_LIMITED => "rate_limited",
            USAGE_UNREACHABLE => "unreachable",
            USAGE_HTTP => "http_error",
            USAGE_INTERRUPTED => "interrupted",
            USAGE_TOO_LARGE => "too_large",
            USAGE_UNSUPPORTED | USAGE_RESET_UNSUPPORTED => "unsupported",
            USAGE_UNAVAILABLE => "unavailable",
            _ => "other",
        }
    }
}
const USAGE_ORIGINS: (&str, &str) = ("https://api.anthropic.com", "https://chatgpt.com");

/// Manual quota check. Fixed destinations only, redirects disabled, no raw error text.
pub async fn probe_usage(store: Arc<Store>, id: String) -> Result<Usage, String> {
    probe_usage_from(store, id, USAGE_ORIGINS, None)
        .await
        .map_err(|f| f.message)
}
/// The quota check with the provider's own wait on 429 (`Retry-After`), for the scheduler,
/// sent with exactly `credential` — the generation the caller binds the answer to.
pub async fn probe_usage_detailed(
    store: Arc<Store>,
    id: String,
    credential: switchboard_core::Credential,
) -> Result<Usage, ProbeFailure> {
    probe_usage_from(store, id, USAGE_ORIGINS, Some(credential)).await
}
/// The quota check against a synthetic upstream, for tests of crates that depend on this one.
#[cfg(feature = "synthetic-origins")]
pub async fn probe_usage_detailed_at(
    store: Arc<Store>,
    id: String,
    credential: switchboard_core::Credential,
    origins: (&str, &str),
) -> Result<Usage, ProbeFailure> {
    probe_usage_from(store, id, origins, Some(credential)).await
}
/// The wait a 429 asked for, in seconds. An HTTP-date is measured against the response's own
/// `Date` when it has a valid one — the provider's clock, so a local clock running ahead cannot
/// shorten the wait — else against `received_at`.
fn retry_after_seconds(headers: &HeaderMap, received_at: i64) -> Option<i64> {
    let sent_at = headers
        .get("date")
        .and_then(|v| v.to_str().ok())
        .and_then(http_date)
        .unwrap_or(received_at);
    retry_after_at(headers.get("retry-after")?.to_str().ok()?, sent_at)
}
/// An HTTP-date in the IMF-fixdate, RFC 850 or asctime form a recipient must accept
/// (RFC 9110 §5.6.7), as Unix seconds. A weekday that contradicts the date is no date.
fn http_date(value: &str) -> Option<i64> {
    let value = value.trim();
    [
        "%a, %d %b %Y %H:%M:%S GMT",
        "%A, %d-%b-%y %H:%M:%S GMT",
        "%a %b %e %H:%M:%S %Y",
    ]
    .iter()
    .find_map(|format| chrono::NaiveDateTime::parse_from_str(value, format).ok())
    .map(|t| t.and_utc().timestamp())
}
/// `Retry-After` (RFC 9110 §10.2.3) as seconds from `received_at`: delay-seconds, or an
/// HTTP-date (`http_date`). A date already past is zero; a value too large for `i64`
/// saturates; a sign, a fraction, another zone or a weekday that does not match its date is no
/// value at all.
pub fn retry_after_at(value: &str, received_at: i64) -> Option<i64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return Some(value.parse().unwrap_or(i64::MAX));
    }
    Some(http_date(value)?.saturating_sub(received_at).max(0))
}
async fn probe_usage_from(
    store: Arc<Store>,
    id: String,
    origins: (&str, &str),
    credential: Option<switchboard_core::Credential>,
) -> Result<Usage, ProbeFailure> {
    let snapshot = store.snapshot()?;
    let account = snapshot
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found.")?;
    if account.kind != AuthKind::OAuth {
        return Err("Usage unavailable for this credential type.".into());
    }
    let credential = match credential {
        Some(credential) => credential,
        None => store.stored_credential(&id)?,
    };
    if !account.enabled {
        return Err("Account is disabled".into());
    }
    if credential.expires_at.is_some_and(|t| t <= now()) {
        return Err(REJECTED.into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| USAGE_UNAVAILABLE)?;
    let url = match account.provider {
        Provider::Claude => format!("{}/api/oauth/usage", origins.0),
        Provider::Codex => format!("{}/backend-api/wham/usage", origins.1),
    };
    let mut request = client.get(url).bearer_auth(&credential.access_token);
    if account.provider == Provider::Claude {
        request = request.header("anthropic-beta", "oauth-2025-04-20");
    }
    if let Some(account_id) = credential.account_id.as_ref() {
        if account.provider == Provider::Codex {
            request = request.header("chatgpt-account-id", account_id);
        }
    }
    let response = request.send().await.map_err(|_| USAGE_UNREACHABLE)?;
    // Only 401 speaks about the token itself; a 403 can be a plan or region refusal.
    if response.status().as_u16() == 401 {
        return Err(REJECTED.into());
    }
    if response.status().as_u16() == 429 {
        return Err(ProbeFailure {
            message: USAGE_RATE_LIMITED.into(),
            rate_limited: Some(retry_after_seconds(response.headers(), now())),
        });
    }
    if !response.status().is_success() {
        return Err(USAGE_HTTP.into());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| USAGE_INTERRUPTED)?;
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(USAGE_TOO_LARGE.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| USAGE_UNSUPPORTED)?;
    let usage = parse_usage(account.provider, &value)?;
    store.observe_credential(&id, &credential, usage.clone())?;
    Ok(usage)
}
/// One window's use and reset. A malformed window is an unsupported response.
fn parse_window(
    provider: Provider,
    window: &serde_json::Value,
    key: &str,
    observed_at: i64,
) -> Result<(f64, Option<i64>), String> {
    let used_percent = window
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .ok_or(USAGE_UNSUPPORTED)?;
    if !used_percent.is_finite() || !(0. ..=100.).contains(&used_percent) {
        return Err(USAGE_UNSUPPORTED.into());
    }
    let reset_key = if provider == Provider::Claude {
        "resets_at"
    } else {
        "reset_at"
    };
    let resets_at = if let Some(reset) = window.get(reset_key).filter(|v| !v.is_null()) {
        Some(parse_reset(reset, observed_at)?)
    } else if provider == Provider::Codex {
        match window.get("reset_after_seconds").filter(|v| !v.is_null()) {
            Some(reset) => {
                let seconds = reset
                    .as_i64()
                    .filter(|n| *n >= 0)
                    .ok_or(USAGE_RESET_UNSUPPORTED)?;
                Some(
                    observed_at
                        .checked_add(seconds)
                        .filter(|t| *t <= 253_402_300_799)
                        .ok_or(USAGE_RESET_UNSUPPORTED)?,
                )
            }
            None => None,
        }
    } else {
        None
    };
    Ok((used_percent, resets_at))
}

/// At most this many feature windows; fewer when the main windows and account-wide blockers
/// leave less of the store's 16.
const MAX_FEATURE_WINDOWS: usize = 12;

/// The Codex dimensions beyond the primary and secondary windows, read as the official client
/// reads them (openai/codex@afb436d, `rate_limit_snapshots_from_payload`; SB-40). Account-wide
/// blockers become ordinary windows; each metered feature's windows are named
/// `feature_<feature>_primary|secondary` and do not stand for the account
/// (`Usage::account_used_percent`). Credits are a purchase measure and are not kept. An unknown
/// reached-type or a malformed feature entry is skipped, never guessed.
fn codex_limits(value: &serde_json::Value, observed_at: i64, windows: &mut Vec<UsageWindow>) {
    let block = |name: &str, windows: &mut Vec<UsageWindow>| {
        if !windows.iter().any(|w| w.name == name) {
            windows.push(UsageWindow {
                name: name.into(),
                used_percent: 100.0,
                resets_at: None,
            });
        }
    };
    // The member's individual spend control: a measured percentage when detailed.
    if let Some(spend) = value.get("spend_control").filter(|v| v.is_object()) {
        let reached = spend.get("reached") == Some(&serde_json::Value::Bool(true));
        // The wire's `used_percent` is an unbounded integer: an overspend reads above 100 and is
        // clamped, never dropped; its reset is read on its own, so a bad reset loses only itself.
        let detailed = spend
            .get("individual_limit")
            .filter(|v| v.is_object())
            .and_then(|limit| {
                let used = limit
                    .get("used_percent")?
                    .as_f64()
                    .filter(|p| p.is_finite())?;
                let resets_at = parse_window(
                    Provider::Codex,
                    &serde_json::json!({"used_percent": 0, "reset_at": limit.get("reset_at"),
                        "reset_after_seconds": limit.get("reset_after_seconds")}),
                    "used_percent",
                    observed_at,
                )
                .ok()
                .and_then(|(_, reset)| reset);
                Some((used.clamp(0.0, 100.0), resets_at))
            });
        match detailed {
            Some((used_percent, resets_at)) => windows.push(UsageWindow {
                name: "spend_limit".into(),
                used_percent: if reached { 100.0 } else { used_percent },
                resets_at,
            }),
            None if reached => block("spend_limit", windows),
            None => {}
        }
    }
    let reached = value
        .pointer("/rate_limit_reached_type/type")
        .and_then(serde_json::Value::as_str);
    match reached {
        Some("workspace_owner_credits_depleted" | "workspace_member_credits_depleted") => {
            block("workspace_credits", windows)
        }
        Some("workspace_owner_usage_limit_reached" | "workspace_member_usage_limit_reached") => {
            block("workspace_usage_limit", windows)
        }
        _ => {}
    }
    let flag = |pointer: &str, expected: bool| {
        value.pointer(pointer).and_then(serde_json::Value::as_bool) == Some(expected)
    };
    let main_full = windows
        .iter()
        .any(|w| matches!(w.name.as_str(), "primary" | "secondary") && w.used_percent >= 100.0);
    if !main_full
        && (flag("/rate_limit/limit_reached", true)
            || flag("/rate_limit/allowed", false)
            || reached == Some("rate_limit_reached"))
    {
        block("limit_reached", windows);
    }
    let Some(features) = value
        .get("additional_rate_limits")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    // Whatever room the main windows and blockers left in the store's 16 (never 17, which would
    // make the whole observation invalid).
    let room = MAX_FEATURE_WINDOWS.min(16usize.saturating_sub(windows.len()));
    let mut scoped: Vec<UsageWindow> = Vec::new();
    for feature in features {
        let Some(id) = feature
            .get("metered_feature")
            .and_then(serde_json::Value::as_str)
            .map(feature_id)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        for (slot, pointer) in [
            ("primary", "/rate_limit/primary_window"),
            ("secondary", "/rate_limit/secondary_window"),
        ] {
            if scoped.len() == room {
                break;
            }
            let name = format!("{}{id}_{slot}", switchboard_core::FEATURE_WINDOW_PREFIX);
            let Some(window) = feature.pointer(pointer).filter(|v| v.is_object()) else {
                continue;
            };
            let Ok((used_percent, resets_at)) =
                parse_window(Provider::Codex, window, "used_percent", observed_at)
            else {
                continue;
            };
            if scoped.iter().any(|w| w.name == name) {
                continue;
            }
            scoped.push(UsageWindow {
                name,
                used_percent,
                resets_at,
            });
        }
    }
    // A stable order, so a provider listing its features differently is not a quota change
    // (core `same_quota` compares windows in order; LC-08).
    scoped.sort_by(|a, b| a.name.cmp(&b.name));
    windows.extend(scoped);
}

/// A provider feature id as a window-name fragment: lowercase `[a-z0-9_]`, at most 40 bytes, so
/// `feature_<id>_secondary` fits the store's 64-byte window names.
fn feature_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

pub fn parse_usage(provider: Provider, value: &serde_json::Value) -> Result<Usage, String> {
    parse_usage_at(provider, value, now())
}
/// Clock injection keeps reset parsing and quota tests independent of wall time.
pub fn parse_usage_at(
    provider: Provider,
    value: &serde_json::Value,
    observed_at: i64,
) -> Result<Usage, String> {
    if observed_at <= 0 {
        return Err("Usage observation time invalid.".into());
    }
    let definitions: &[(&str, &str, &str)] = match provider {
        Provider::Codex => &[
            ("primary", "/rate_limit/primary_window", "used_percent"),
            ("secondary", "/rate_limit/secondary_window", "used_percent"),
        ],
        Provider::Claude => &[
            ("five_hour", "/five_hour", "utilization"),
            ("seven_day", "/seven_day", "utilization"),
            ("seven_day_sonnet", "/seven_day_sonnet", "utilization"),
            ("seven_day_opus", "/seven_day_opus", "utilization"),
            (
                "seven_day_oauth_apps",
                "/seven_day_oauth_apps",
                "utilization",
            ),
        ],
    };
    let mut windows = Vec::new();
    for &(name, path, key) in definitions {
        let Some(window) = value.pointer(path).filter(|v| !v.is_null()) else {
            continue;
        };
        let (used_percent, resets_at) = parse_window(provider, window, key, observed_at)?;
        windows.push(UsageWindow {
            name: name.into(),
            used_percent,
            resets_at,
        });
    }
    if provider == Provider::Codex {
        codex_limits(value, observed_at, &mut windows);
    }
    aggregate_usage(
        windows,
        observed_at,
        match provider {
            Provider::Claude => "claude_oauth",
            Provider::Codex => "codex_oauth",
        },
    )
}

#[cfg(test)]
mod tests;
