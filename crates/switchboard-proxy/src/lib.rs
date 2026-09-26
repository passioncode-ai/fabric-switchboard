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

const BODY_LIMIT: usize = 16 * 1024 * 1024;
#[derive(Clone)]
struct Gateway {
    store: Arc<Store>,
    token: String,
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
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}
impl ProxyHandle {
    pub async fn start(store: Arc<Store>) -> Result<Self, String> {
        Self::start_at(
            store,
            "https://api.anthropic.com".into(),
            "https://api.openai.com".into(),
            "https://chatgpt.com".into(),
        )
        .await
    }
    async fn start_at(
        store: Arc<Store>,
        claude: String,
        openai: String,
        chatgpt: String,
    ) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Local proxy could not start.")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Local proxy address unavailable.")?;
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|_| "HTTP client unavailable.")?;
        let state = Gateway {
            store,
            token: token.clone(),
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
            stop: Some(stop),
            task,
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if tokio::time::timeout(Duration::from_secs(2), &mut self.task)
            .await
            .is_err()
        {
            self.task.abort();
        }
    }
}
impl Drop for ProxyHandle {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.abort();
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
            std::task::Poll::Ready(None) => {
                self.finished = true;
                let _ = self.store.record("proxy", Some(&self.id), "completed");
                if self.success {
                    let _ = self.store.record("request", Some(&self.id), "success");
                }
            }
            std::task::Poll::Ready(Some(Err(_))) => {
                self.finished = true;
                let _ = self.store.record("proxy", Some(&self.id), "aborted");
            }
            _ => {}
        }
        poll
    }
}
impl Drop for AuditedStream {
    fn drop(&mut self) {
        if !self.finished {
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
    let expected = format!("Bearer {}", g.token);
    let supplied = request
        .headers()
        .get("authorization")
        .map(|x| x.as_bytes())
        .unwrap_or_default();
    if !bool::from(supplied.ct_eq(expected.as_bytes())) {
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
        ("codex", "responses") => Provider::Codex,
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
        (Provider::Codex, _) => format!("{}/v1/responses", g.openai),
    };
    let mut headers = HeaderMap::new();
    let request_hop_fields = connection_fields(&parts.headers);
    for name in [
        "content-type",
        "accept",
        "anthropic-version",
        "anthropic-beta",
        "openai-beta",
        "x-stainless-lang",
        "x-stainless-package-version",
        "x-stainless-os",
        "x-stainless-arch",
        "x-stainless-runtime",
        "x-stainless-runtime-version",
    ] {
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
    let mut outgoing = g.client.post(url).headers(headers).body(bytes);
    if provider == Provider::Claude && account.kind == AuthKind::ApiKey {
        outgoing = outgoing.header("x-api-key", &credential.access_token);
    } else {
        outgoing = outgoing.bearer_auth(&credential.access_token);
        if provider == Provider::Claude {
            let existing = if request_hop_fields.contains("anthropic-beta") {
                None
            } else {
                parts.headers.get("anthropic-beta")
            }
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
            let beta = if existing.split(',').any(|v| v.trim() == "oauth-2025-04-20") {
                existing.to_owned()
            } else if existing.is_empty() {
                "oauth-2025-04-20".into()
            } else {
                format!("{existing},oauth-2025-04-20")
            };
            outgoing = outgoing.header("anthropic-beta", beta);
        }
    }
    if provider == Provider::Claude
        && (!parts.headers.contains_key("anthropic-version")
            || request_hop_fields.contains("anthropic-version"))
    {
        outgoing = outgoing.header("anthropic-version", "2023-06-01");
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
    if status.is_success() {
        let _ = g.store.record("proxy", Some(&account.id), "started");
    } else {
        let _ = g.store.record("request", Some(&account.id), detail);
    }
    if provider == Provider::Claude {
        if let Some(usage) = usage_from_headers(upstream.headers()) {
            let _ = g.store.observe_credential(&account.id, &credential, usage);
        }
    }
    let mut response = Response::builder().status(status);
    let response_hop_fields = connection_fields(upstream.headers());
    for name in [
        "content-type",
        "cache-control",
        "retry-after",
        "request-id",
        "x-request-id",
    ] {
        if response_hop_fields.contains(name) {
            continue;
        }
        if let Some(value) = upstream.headers().get(name) {
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
        .ok_or("Usage reset unsupported.")?;
    if parsed < observed_at || parsed > 253_402_300_799 {
        return Err("Usage reset unsupported.".into());
    }
    Ok(parsed)
}

/// Manual quota check. Fixed destinations only, redirects disabled, no raw error text.
pub async fn probe_usage(store: Arc<Store>, id: String) -> Result<Usage, String> {
    let snapshot = store.snapshot()?;
    let account = snapshot
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found.")?;
    if account.kind != AuthKind::OAuth {
        return Err("Usage unavailable for this credential type.".into());
    }
    let credential = store.credential(&id)?;
    if credential.expires_at.is_some_and(|t| t <= now()) {
        return Err("Provider rejected the credential. Sign in again.".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Usage check unavailable.")?;
    let url = match account.provider {
        Provider::Claude => "https://api.anthropic.com/api/oauth/usage",
        Provider::Codex => "https://chatgpt.com/backend-api/wham/usage",
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
    let response = request
        .send()
        .await
        .map_err(|_| "Usage check could not reach provider.")?;
    if !response.status().is_success() {
        return Err("Usage unavailable. Check the account and retry later.".into());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Usage response interrupted.")?;
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("Usage response too large.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Usage response unsupported.")?;
    let usage = parse_usage(account.provider, &value)?;
    store.observe_credential(&id, &credential, usage.clone())?;
    Ok(usage)
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
        let used_percent = window
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .ok_or("Usage response unsupported.")?;
        if !used_percent.is_finite() || !(0. ..=100.).contains(&used_percent) {
            return Err("Usage response unsupported.".into());
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
                        .ok_or("Usage reset unsupported.")?;
                    Some(
                        observed_at
                            .checked_add(seconds)
                            .filter(|t| *t <= 253_402_300_799)
                            .ok_or("Usage reset unsupported.")?,
                    )
                }
                None => None,
            }
        } else {
            None
        };
        windows.push(UsageWindow {
            name: name.into(),
            used_percent,
            resets_at,
        });
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
