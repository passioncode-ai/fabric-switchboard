//! Credential-injecting loopback gateway. A request owns its route snapshot until EOF.
pub mod stats;
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
/// The header a project folder's Claude Code sends its pool capability in (SB-140).
pub const CAPABILITY_HEADER: &str = "x-switchboard-capability";
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
    salts: PoolSalts,
    folders: FolderTokens,
    codex_folders: CodexFolders,
    legacy_until: Arc<std::sync::atomic::AtomicI64>,
    draining: Arc<std::sync::atomic::AtomicBool>,
    stats: stats::Stats,
    stat_folders: StatFolders,
}
/// SB-147: every group folder, and each folder's own capability → that folder, so a request's
/// statistics name the folder it came from.
#[derive(Default)]
pub struct FolderMap {
    folders: Vec<String>,
    by_capability: std::collections::HashMap<String, String>,
}
pub type StatFolders = Arc<std::sync::RwLock<FolderMap>>;
/// SB-133: each pool's folder capabilities (`folder_token`), one per folder that is part of the
/// pool's project now. `None` until the owner sets them at start.
pub type FolderTokens =
    Arc<std::sync::RwLock<Option<std::collections::HashMap<String, Vec<String>>>>>;
/// SB-143: the folders of projects whose pool has a Codex account, each with its pool, for the
/// `/codex/_auto/v1` route. `None` until the owner sets them.
pub type CodexFolders = Arc<std::sync::RwLock<Option<Vec<(String, String)>>>>;
/// SB-143: the route a user-level Codex provider points at; no pool can start with `_`.
pub const AUTO_POOL: &str = "_auto";
/// Per-pool salts of the folders' capabilities (`pool_token_salted`): a pool whose capability was
/// rotated has one; the rest keep the unsalted capability.
pub type PoolSalts = Arc<std::sync::RwLock<std::collections::HashMap<String, String>>>;
pub struct ProxyHandle {
    address: SocketAddr,
    token: String,
    agent_token: String,
    salts: PoolSalts,
    folders: FolderTokens,
    codex_folders: CodexFolders,
    legacy_until: Arc<std::sync::atomic::AtomicI64>,
    draining: Arc<std::sync::atomic::AtomicBool>,
    stop: std::sync::Mutex<Option<oneshot::Sender<()>>>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
    slots: Arc<Semaphore>,
    stats: stats::Stats,
    stat_folders: StatFolders,
}
/// Requests relayed at once; each holds a slot until its response stream ends. 16 was too few for
/// a machine with two dozen agent sessions streaming at once (2026-10-18): the rest got 503.
const SLOTS: usize = 64;
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
/// A capability for one pool's Claude Code and Codex route only (SB-118): written into a project
/// folder's Claude Code settings so ordinary sessions there reach the project's accounts. It is
/// derived from the session token, so it survives a restart with it; knowing it reveals neither
/// the session token nor any other pool's capability.
pub fn pool_token(token: &str, pool: &str) -> String {
    pool_token_salted(token, pool, None)
}
/// A pool's capability after a rotation: the pool's salt is part of it, so a new salt makes the
/// old capability worthless without touching the session token or any other pool.
pub fn pool_token_salted(token: &str, pool: &str, salt: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    let input = match salt {
        Some(salt) => format!("switchboard-pool:{pool}:{salt}:{token}"),
        None => format!("switchboard-pool:{pool}:{token}"),
    };
    let digest = Sha256::digest(input.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
/// SB-133: one folder's capability: the pool's (`pool_token_salted`) bound to the folder's path.
/// A folder that leaves its project, or is moved or renamed away from it, keeps a value the proxy
/// no longer accepts, so it cannot go on spending the project's accounts.
pub fn folder_token(pool_capability: &str, folder: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("switchboard-folder:{folder}:{pool_capability}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
/// Answered while the app drains for an update restart; Claude Code and the SDKs retry a 503.
pub const RESTARTING: &str =
    "Switchboard is restarting to finish an update. Retry in a few seconds.";
/// SB-133: refused to a folder capability the proxy does not know (any longer).
pub const FOLDER_NOT_IN_PROJECT: &str = "This folder is not part of a Switchboard project now. Remove Switchboard's lines from .claude/settings.local.json, or add the folder to a project in Switchboard.";
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
        let salts: PoolSalts = Arc::default();
        let folders: FolderTokens = Arc::default();
        let codex_folders: CodexFolders = Arc::default();
        let legacy_until = Arc::new(std::sync::atomic::AtomicI64::new(i64::MAX));
        let draining = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stats: stats::Stats = Arc::default();
        let stat_folders: StatFolders = Arc::default();
        let state = Gateway {
            store,
            token: token.clone(),
            agent_token: agent.clone(),
            address,
            client,
            slots: Arc::new(Semaphore::new(SLOTS)),
            claude,
            openai,
            chatgpt,
            salts: salts.clone(),
            folders: folders.clone(),
            codex_folders: codex_folders.clone(),
            legacy_until: legacy_until.clone(),
            draining: draining.clone(),
            stats: stats.clone(),
            stat_folders: stat_folders.clone(),
        };
        let slots = state.slots.clone();
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
            salts,
            folders,
            codex_folders,
            legacy_until,
            draining,
            stop: std::sync::Mutex::new(Some(stop)),
            task: std::sync::Mutex::new(Some(task)),
            slots,
            stats,
            stat_folders,
        })
    }
    /// Managed requests in flight, streams included: zero means no managed session is waiting
    /// on an answer right now (an update may activate, LC-16).
    pub fn in_flight(&self) -> usize {
        SLOTS.saturating_sub(self.slots.available_permits())
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    /// SB-147: the group folders as `(folder, pool)`, so statistics can name a request's folder:
    /// by the folder's own capability when it sent one, else by the folder the request names.
    pub fn set_stat_folders(&self, folders: Vec<(String, String)>) {
        let salts = self.salts.read().map(|s| s.clone()).unwrap_or_default();
        let mut map = FolderMap::default();
        for (folder, pool) in folders {
            let capability =
                pool_token_salted(&self.token, &pool, salts.get(&pool).map(String::as_str));
            map.by_capability
                .insert(folder_token(&capability, &folder), folder.clone());
            map.folders.push(folder);
        }
        map.folders.sort();
        map.folders.dedup();
        if let Ok(mut current) = self.stat_folders.write() {
            *current = map;
        }
    }
    /// SB-145: the counts gathered since the last call, handed over and cleared; the owner adds
    /// them to what it keeps on disk.
    pub fn take_statistics(&self) -> stats::Tallies {
        self.stats
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default()
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    /// The pools' capability salts, set at start and after a rotation.
    pub fn set_pool_salts(&self, salts: std::collections::HashMap<String, String>) {
        if let Ok(mut current) = self.salts.write() {
            *current = salts;
        }
    }
    /// The capability a pool's folders present now.
    pub fn pool_capability(&self, pool: &str) -> String {
        let salt = self.salts.read().ok().and_then(|s| s.get(pool).cloned());
        pool_token_salted(&self.token, pool, salt.as_deref())
    }
    /// SB-143: the capability a user-level Codex provider sends to `/codex/_auto/v1`.
    pub fn auto_capability(&self) -> String {
        self.pool_capability(AUTO_POOL)
    }
    /// SB-143: the folders of projects with a Codex account and their pools, set at start and
    /// whenever projects or accounts change.
    pub fn set_codex_folders(&self, folders: Vec<(String, String)>) {
        if let Ok(mut current) = self.codex_folders.write() {
            *current = Some(folders);
        }
    }
    /// SB-133: the folder capabilities of every pool, set at start and whenever the project
    /// folders change.
    pub fn set_folder_tokens(&self, tokens: std::collections::HashMap<String, Vec<String>>) {
        if let Ok(mut current) = self.folders.write() {
            *current = Some(tokens);
        }
    }
    /// SB-133: until this Unix time a pool's own capability, which 0.6.39 and earlier wrote into
    /// every folder of the pool, is still accepted, so sessions started before the folders got
    /// their own keep working. `i64::MAX` (the start value) accepts it always; 0 never.
    pub fn set_legacy_until(&self, until: i64) {
        self.legacy_until
            .store(until, std::sync::atomic::Ordering::Relaxed);
    }
    /// While set, new requests are answered 503 with `Retry-After` and requests already relayed
    /// finish: the app is about to restart into an update that waited too long for a quiet moment.
    pub fn set_draining(&self, on: bool) {
        self.draining
            .store(on, std::sync::atomic::Ordering::Relaxed);
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
    /// SB-145: the tally this response's tokens go to, once, when it ends or is dropped.
    stats: stats::Stats,
    key: String,
    scanner: Option<stats::UsageScanner>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl AuditedStream {
    fn settle(&mut self) {
        let Some(mut scan) = self.scanner.take() else {
            return;
        };
        scan.finish();
        if scan.input_tokens + scan.output_tokens + scan.cache_read_tokens + scan.cache_write_tokens
            == 0
        {
            return;
        }
        if let Ok(mut all) = self.stats.lock() {
            let t = all.entry(std::mem::take(&mut self.key)).or_default();
            t.input_tokens += scan.input_tokens;
            t.output_tokens += scan.output_tokens;
            t.cache_read_tokens += scan.cache_read_tokens;
            t.cache_write_tokens += scan.cache_write_tokens;
        }
    }
}
/// SB-145: one request's outcome in the day's tally of its account, pool and client.
fn count(stats: &stats::Stats, key: &str, status: Option<StatusCode>) {
    if let Ok(mut all) = stats.lock() {
        let t = all.entry(key.to_owned()).or_default();
        t.requests += 1;
        match status {
            Some(s) if s.is_success() => t.ok += 1,
            Some(StatusCode::TOO_MANY_REQUESTS) => t.limited += 1,
            _ => t.failed += 1,
        }
    }
}
impl futures_util::Stream for AuditedStream {
    type Item = Result<axum::body::Bytes, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let poll = self.inner.as_mut().poll_next(cx);
        if let std::task::Poll::Ready(Some(Ok(chunk))) = &poll {
            let chunk = chunk.clone();
            if let Some(scan) = self.scanner.as_mut() {
                scan.feed(&chunk);
            }
        }
        match &poll {
            // A non-success status was journaled at headers. A completed success is not
            // journaled: it is the common case, and one event per request would evict the
            // activation and rotation history from the bounded journal.
            std::task::Poll::Ready(None) => {
                self.finished = true;
                self.settle();
            }
            std::task::Poll::Ready(Some(Err(_))) => {
                self.finished = true;
                self.settle();
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
        self.settle();
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
    if provider == "codex" && pool == AUTO_POOL {
        return Box::pin(codex_auto(g, operation, request)).await;
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
    // SB-140: a project folder's capability as a header of its own. Claude Code sends it with
    // `ANTHROPIC_CUSTOM_HEADERS`, which is not an auth source, so a session there keeps its
    // claude.ai login and connectors; its own bearer is then ignored and never forwarded.
    let header = request
        .headers()
        .get(CAPABILITY_HEADER)
        .map(|x| x.as_bytes().to_vec())
        .unwrap_or_default();
    // Every comparison runs, whatever matched: no timing tells which header or token was right.
    // SB-118: a project folder's capability counts as the session token, for its pool only.
    let salt = g.salts.read().ok().and_then(|s| s.get(&pool).cloned());
    let capability = pool_token_salted(&g.token, &pool, salt.as_deref());
    // SB-133: a folder's own capability, while the folder is part of the pool's project; the
    // pool's capability itself only inside the migration window.
    let legacy = subtle::Choice::from(u8::from(
        now() < g.legacy_until.load(std::sync::atomic::Ordering::Relaxed),
    ));
    let mut scoped =
        (bearer.ct_eq(capability.as_bytes()) | header.ct_eq(capability.as_bytes())) & legacy;
    if let Some(tokens) = g
        .folders
        .read()
        .ok()
        .and_then(|f| f.as_ref().and_then(|m| m.get(&pool).cloned()))
    {
        for token in &tokens {
            scoped |= bearer.ct_eq(token.as_bytes()) | header.ct_eq(token.as_bytes());
        }
    }
    let official = bearer.ct_eq(g.token.as_bytes()) | scoped;
    let as_agent = bearer.ct_eq(g.agent_token.as_bytes()) | key.ct_eq(g.agent_token.as_bytes());
    let agent = bool::from(!official & as_agent);
    let official = bool::from(official);
    if !official && !agent {
        // A folder that left its project still sends its old header: say what to do there.
        if !header.is_empty() {
            return error(StatusCode::UNAUTHORIZED, FOLDER_NOT_IN_PROJECT);
        }
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
    if g.draining.load(std::sync::atomic::Ordering::Relaxed) {
        let mut response = error(StatusCode::SERVICE_UNAVAILABLE, RESTARTING);
        response
            .headers_mut()
            .insert("retry-after", axum::http::HeaderValue::from_static("5"));
        return response;
    }
    let permit = match g.slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Proxy is busy. Retry later.",
            )
        }
    };
    // SB-101: a selected account whose credential expired, or that is gone, no longer answers
    // a session with 409 while the pool has another usable account.
    // SB-102: only to an account of the same kind as the broken one (any kind when it is
    // gone), and one that serves this operation.
    let api_key_only = operation == "chat/completions";
    let route = match g.store.route(provider, &pool) {
        Ok(route) => Some(route),
        Err(_) if official => {
            let fits = switchboard_core::Fits {
                kind: g.store.selected_account(provider, &pool).map(|a| a.kind),
                api_key_only,
            };
            g.store
                .failover_unusable(provider, &pool, fits, now())
                .ok()
                .flatten()
        }
        Err(_) => None,
    };
    let Some((account, credential)) = route else {
        return error(
            StatusCode::CONFLICT,
            "Select a usable account in this pool first.",
        );
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
    let client = stats::client_of(
        parts
            .headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok()),
    );
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
    // SB-147: the folder the request came from: its own capability, else the folder it names.
    let folder = g
        .stat_folders
        .read()
        .ok()
        .and_then(|map| {
            let presented = |v: &[u8]| {
                std::str::from_utf8(v)
                    .ok()
                    .and_then(|t| map.by_capability.get(t))
                    .cloned()
            };
            presented(&header)
                .or_else(|| presented(&bearer))
                .or_else(|| {
                    stats::folder_hint(&bytes)
                        .and_then(|hint| stats::deepest(&map.folders, &hint).map(str::to_owned))
                })
        })
        .unwrap_or_default();
    let tally = |id: &str| stats::key(&stats::day_of(now()), id, &pool, &client, &folder);
    let mut moved = false;
    let mut account = account;
    let mut credential = credential;
    let (upstream, journaled) = loop {
        let url = match (provider, account.kind) {
            (Provider::Claude, _) => format!(
                "{}/v1/{}{}",
                g.claude,
                operation,
                if query.is_some() { "?beta=true" } else { "" }
            ),
            (Provider::Codex, AuthKind::OAuth) => {
                format!("{}/backend-api/codex/responses", g.chatgpt)
            }
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
        // A Claude Code session signed in with claude.ai sends the OAuth beta; an API-key account
        // upstream must not get it.
        if provider == Provider::Claude && account.kind == AuthKind::ApiKey {
            if let Some(existing) = headers.get("anthropic-beta").and_then(|h| h.to_str().ok()) {
                let kept: Vec<&str> = existing
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty() && *v != "oauth-2025-04-20")
                    .collect();
                if kept.is_empty() {
                    headers.remove("anthropic-beta");
                } else if let Ok(value) = axum::http::HeaderValue::from_str(&kept.join(",")) {
                    headers.insert("anthropic-beta", value);
                }
            }
        }
        if provider == Provider::Claude && !headers.contains_key("anthropic-version") {
            headers.insert(
                "anthropic-version",
                axum::http::HeaderValue::from_static("2023-06-01"),
            );
        }
        let mut outgoing = g.client.post(url).headers(headers).body(bytes.clone());
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
                count(&g.stats, &tally(&account.id), None);
                let _ = g
                    .store
                    .record("request", Some(&account.id), "network_error");
                return error(
                    StatusCode::BAD_GATEWAY,
                    "Provider connection failed. Request was not replayed.",
                );
            }
        };
        // SB-98 (operator decision 2026-10-11): a session's request the provider refused for
        // a limit did no work, so it moves — once — to the pool's next usable account, which
        // also serves the requests after it; the agent never sees the pause. Only Claude Code
        // and Codex sessions: an agent's API-key request keeps the provider's answer.
        if upstream.status() == StatusCode::TOO_MANY_REQUESTS && official && !moved {
            let _ = g.store.record("request", Some(&account.id), "rate_limited");
            if provider == Provider::Claude {
                if let Some(usage) = usage_from_headers(upstream.headers()) {
                    let _ = g.store.observe_credential(&account.id, &credential, usage);
                }
            }
            let fits = switchboard_core::Fits {
                kind: Some(account.kind),
                api_key_only,
            };
            if let Ok(Some(next)) = g.store.failover(provider, &pool, &account.id, fits, now()) {
                count(
                    &g.stats,
                    &tally(&account.id),
                    Some(StatusCode::TOO_MANY_REQUESTS),
                );
                (account, credential) = next;
                moved = true;
                continue;
            }
            break (upstream, true);
        }
        break (upstream, false);
    };
    let status = upstream.status();
    let key = tally(&account.id);
    count(&g.stats, &key, Some(status));
    if status.is_redirection() {
        return error(StatusCode::BAD_GATEWAY, "Provider redirect refused.");
    }
    let detail = match status.as_u16() {
        200..=299 => "success",
        401 | 403 => "unauthorized",
        429 => "rate_limited",
        _ => "upstream_error",
    };
    if !status.is_success() && !journaled {
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
            stats: g.stats.clone(),
            key,
            scanner: status.is_success().then(stats::UsageScanner::default),
            _permit: permit,
        };
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| error(StatusCode::BAD_GATEWAY, "Provider response invalid."))
}
/// SB-143: a Codex session's request through the user-level provider. The request names the
/// folder the session works in (`<cwd>` in its environment context); a folder of a project whose
/// pool has a Codex account goes on as that pool's managed request, with its account and
/// fail-over; any other request goes on unchanged with the person's own sign-in.
async fn codex_auto(g: Gateway, operation: String, request: Request<Body>) -> Response {
    let header = request
        .headers()
        .get(CAPABILITY_HEADER)
        .map(|x| x.as_bytes().to_vec())
        .unwrap_or_default();
    let salt = g.salts.read().ok().and_then(|s| s.get(AUTO_POOL).cloned());
    let capability = pool_token_salted(&g.token, AUTO_POOL, salt.as_deref());
    if !bool::from(header.ct_eq(capability.as_bytes())) {
        return error(StatusCode::UNAUTHORIZED, "Local capability required.");
    }
    if operation != "responses" || request.uri().query().is_some() {
        return error(StatusCode::NOT_FOUND, "Unsupported provider operation.");
    }
    if g.draining.load(std::sync::atomic::Ordering::Relaxed) {
        let mut response = error(StatusCode::SERVICE_UNAVAILABLE, RESTARTING);
        response
            .headers_mut()
            .insert("retry-after", axum::http::HeaderValue::from_static("5"));
        return response;
    }
    let (mut parts, body) = request.into_parts();
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
    let pool = request_folder(&bytes).and_then(|folder| {
        let map = g.codex_folders.read().ok()?;
        project_pool(map.as_deref()?, &folder)
    });
    if let Some(pool) = pool {
        // The pool's managed route, as the session token: its account, its fail-over, its
        // journal. The person's own sign-in is never forwarded.
        parts.headers.remove("authorization");
        parts.headers.remove("chatgpt-account-id");
        parts.headers.remove(CAPABILITY_HEADER);
        if let Ok(value) = axum::http::HeaderValue::from_str(&format!("Bearer {}", g.token)) {
            parts.headers.insert("authorization", value);
        }
        let request = Request::from_parts(parts, Body::from(bytes));
        return Box::pin(relay(
            State(g),
            Path(("codex".to_owned(), pool, operation)),
            request,
        ))
        .await;
    }
    let permit = match g.slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Proxy is busy. Retry later.",
            )
        }
    };
    // Not a project's folder: the person's own request, to where Codex itself would send it.
    let url = if parts.headers.contains_key("chatgpt-account-id") {
        format!("{}/backend-api/codex/responses", g.chatgpt)
    } else {
        format!("{}/v1/responses", g.openai)
    };
    let hop = connection_fields(&parts.headers);
    let mut headers = HeaderMap::new();
    for (name, value) in &parts.headers {
        let n = name.as_str();
        if hop.contains(n)
            || matches!(
                n,
                "host" | "content-length" | "connection" | "origin" | CAPABILITY_HEADER
            )
        {
            continue;
        }
        headers.append(name.clone(), value.clone());
    }
    let upstream = match g.client.post(url).headers(headers).body(bytes).send().await {
        Ok(r) => r,
        Err(_) => {
            return error(
                StatusCode::BAD_GATEWAY,
                "Provider connection failed. Request was not replayed.",
            )
        }
    };
    let status = upstream.status();
    if status.is_redirection() {
        return error(StatusCode::BAD_GATEWAY, "Provider redirect refused.");
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
    let stream =
        PassStream {
            inner: Box::pin(upstream.bytes_stream().map(|item| {
                item.map_err(|_| std::io::Error::other("Provider stream interrupted"))
            })),
            _permit: permit,
        };
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| error(StatusCode::BAD_GATEWAY, "Provider response invalid."))
}
/// A passed-through response: it holds its slot until the stream ends, and records nothing (no
/// account of Switchboard's served it).
struct PassStream {
    inner: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>,
    >,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl futures_util::Stream for PassStream {
    type Item = Result<axum::body::Bytes, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
/// The last folder a Codex request names in its environment context (`<cwd>…</cwd>`), read from
/// the request's JSON text values; None when it names none.
pub fn request_folder(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    // Walked in document order, so the last match is the newest environment context.
    let mut last = None;
    let mut stack = vec![&value];
    let mut seen = 0usize;
    while let Some(v) = stack.pop() {
        seen += 1;
        if seen > 200_000 {
            break;
        }
        match v {
            serde_json::Value::String(s) => {
                if let Some(at) = s.rfind("<cwd>") {
                    let rest = &s[at + 5..];
                    if let Some(end) = rest.find("</cwd>") {
                        let folder = rest[..end].trim();
                        if folder.starts_with('/') || folder.chars().nth(1) == Some(':') {
                            last = Some(folder.to_owned());
                        }
                    }
                }
            }
            serde_json::Value::Array(items) => stack.extend(items.iter().rev()),
            serde_json::Value::Object(map) => stack.extend(map.values().rev()),
            _ => {}
        }
    }
    last
}
/// The pool of the project folder holding `folder`, the deepest one when folders nest.
pub fn project_pool(folders: &[(String, String)], folder: &str) -> Option<String> {
    let path = std::path::Path::new(folder);
    folders
        .iter()
        .filter(|(f, _)| path.starts_with(f))
        .max_by_key(|(f, _)| f.len())
        .map(|(_, pool)| pool.clone())
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
    /// The HTTP status of an answer that was neither success, 401 nor 429 (`http_error`): a number
    /// for the operations log, so a 403 and a 502 are told apart without the provider's text.
    pub status: Option<u16>,
    /// The log's code for a refusal whose message is another layer's (the store's, the vault's):
    /// its text still reaches the person; the log gets `store` or `vault` (SB-127).
    pub code: Option<&'static str>,
}
impl From<String> for ProbeFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            rate_limited: None,
            status: None,
            code: None,
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
// Refusals before any request (SB-127: logged as `other` until 2026-10-15).
const USAGE_ACCOUNT_GONE: &str = "Account not found.";
const USAGE_NOT_OAUTH: &str = "Usage unavailable for this credential type.";
const USAGE_DISABLED: &str = "Account is disabled";

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
            USAGE_ACCOUNT_GONE => "account_gone",
            USAGE_NOT_OAUTH => "not_oauth",
            USAGE_DISABLED => "disabled",
            _ => self.code.unwrap_or("other"),
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
    let snapshot = store.snapshot().map_err(|e| ProbeFailure {
        code: Some("store"),
        ..e.into()
    })?;
    let account = snapshot
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or(USAGE_ACCOUNT_GONE)?;
    if account.kind != AuthKind::OAuth {
        return Err(USAGE_NOT_OAUTH.into());
    }
    let credential = match credential {
        Some(credential) => credential,
        None => store.stored_credential(&id).map_err(|e| ProbeFailure {
            code: Some("vault"),
            ..e.into()
        })?,
    };
    if !account.enabled {
        return Err(USAGE_DISABLED.into());
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
            status: None,
            code: None,
        });
    }
    if !response.status().is_success() {
        return Err(ProbeFailure {
            status: Some(response.status().as_u16()),
            ..USAGE_HTTP.into()
        });
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
