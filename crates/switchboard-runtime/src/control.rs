//! Private, capability-authenticated CLI control channel, separate from inference.
use crate::{oplog, Operation, Runtime};
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;
use switchboard_core::private_fs;
use tokio::{sync::Semaphore, task::JoinHandle};
use uuid::Uuid;

const FILE: &str = "control.json";
const REQUEST_LIMIT: usize = 128 * 1024;
const RESPONSE_LIMIT: usize = 4 * 1024 * 1024;
/// How long a connect to the owner may take before it counts as unanswered. A live
/// owner accepts at once; a port nobody listens on is refused at once on macOS but
/// only after about two seconds of SYN retries on Windows, and a refusal is the
/// fallback a stale descriptor must reach, so the bound sits well past that.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    protocol: u8,
    address: SocketAddr,
    token: String,
    /// The owner's process: a client treats a descriptor whose owner is gone as stale
    /// (lifecycle LC-01). Absent in descriptors written before 0.5.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum Reply {
    Ok { value: Value },
    Error { message: String },
}
#[derive(Clone)]
struct Control {
    runtime: Arc<Runtime>,
    address: SocketAddr,
    token: String,
    slots: Arc<Semaphore>,
}
pub struct ControlHandle {
    task: Option<JoinHandle<()>>,
    descriptor: PathBuf,
}
impl ControlHandle {
    pub async fn start(runtime: Arc<Runtime>) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Control listener unavailable.")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Control address unavailable.")?;
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let descriptor = runtime.root.join(FILE);
        private_fs::private_write(
            &descriptor,
            &serde_json::to_vec(&Descriptor {
                protocol: 1,
                address,
                token: token.clone(),
                pid: Some(std::process::id()),
            })
            .map_err(|_| "Control metadata unavailable.")?,
        )?;
        let state = Control {
            runtime,
            address,
            token,
            slots: Arc::new(Semaphore::new(16)),
        };
        let router = Router::new()
            .route("/v1/control", post(handle))
            .route("/v1/hello", get(hello))
            .with_state(state);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            task: Some(task),
            descriptor,
        })
    }
    /// Stops answering and removes the descriptor; returns once the listener — and the runtime
    /// it holds — is gone, so the store's lock is released with it (lifecycle LC-01).
    pub async fn close(mut self) {
        let _ = std::fs::remove_file(&self.descriptor);
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for ControlHandle {
    fn drop(&mut self) {
        // Store remains locked by the server's Runtime until abort is processed.
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.descriptor);
    }
}
/// Whether the process that wrote a descriptor still runs. Unknown counts as running, so the
/// descriptor is proved as before.
fn owner_alive(pid: Option<u32>) -> bool {
    #[cfg(unix)]
    {
        let Some(pid) = pid.and_then(|p| libc::pid_t::try_from(p).ok()) else {
            return true;
        };
        if pid <= 0 {
            return true;
        }
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}
fn proof(token: &str, nonce: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(token.as_bytes()).expect("HMAC supports every key length");
    mac.update(b"switchboard-control-v1:");
    mac.update(nonce.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    proof: String,
}
async fn hello(State(state): State<Control>, request: Request<Body>) -> Response {
    let headers = request.headers();
    if request.uri().query().is_some()
        || headers.contains_key("origin")
        || headers.contains_key("upgrade")
        || headers.get_all("host").iter().count() != 1
        || headers.get("host").and_then(|h| h.to_str().ok())
            != Some(state.address.to_string().as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let nonce = headers
        .get("x-switchboard-challenge")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    if nonce.len() != 64 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut response = Json(Hello {
        proof: proof(&state.token, nonce),
    })
    .into_response();
    // In a header, not the body: a client of an earlier version refuses unknown body fields.
    response.headers_mut().insert(
        VERSION_HEADER,
        axum::http::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );
    response
}
/// The owner's version, sent with every identity answer so a client can tell a version skew
/// (an update installed while an earlier owner still runs) from a refusal.
const VERSION_HEADER: &str = "x-switchboard-version";
pub const CONTROL_OWNER_OLDER: &str = "The running Switchboard is an earlier version than this command and does not know it. Restart Switchboard to finish its update (Restart to update in its menu, or quit and open it), then try again. No offline operation was attempted.";
pub const CONTROL_OWNER_NEWER: &str = "This switchboard command is an earlier version than the running Switchboard. Use the command bundled with the app (Agents → Link switchboard), then try again. No offline operation was attempted.";
pub const CONTROL_BUSY: &str =
    "Switchboard is busy with other requests. Try again in a moment. No operation was attempted.";
/// Every refusal is logged by code (LC-12): which check refused, never the request.
fn refuse(status: StatusCode, reason: &'static str) -> Response {
    oplog::event(
        "control_refused",
        &[
            ("status", oplog::Field::Number(i64::from(status.as_u16()))),
            ("reason", oplog::Field::Code(reason)),
        ],
    );
    status.into_response()
}
/// `major.minor.patch` of a version string; anything else is not compared.
fn version_triple(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let triple = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(triple)
}
/// The message for a request the owner refused as malformed (400): a version skew when the two
/// versions differ, else the plain refusal. An owner that sends no version predates 0.6.11.
fn malformed_refusal(owner: Option<&str>) -> &'static str {
    let mine = version_triple(env!("CARGO_PKG_VERSION"));
    let Some(owner) = owner else {
        return CONTROL_OWNER_OLDER;
    };
    match (version_triple(owner), mine) {
        (Some(theirs), Some(mine)) if theirs < mine => CONTROL_OWNER_OLDER,
        (Some(theirs), Some(mine)) if theirs > mine => CONTROL_OWNER_NEWER,
        _ => "Control request refused. No offline operation was attempted.",
    }
}
async fn handle(State(state): State<Control>, request: Request<Body>) -> Response {
    let headers = request.headers();
    if request.uri().query().is_some()
        || headers.contains_key("origin")
        || headers.contains_key("upgrade")
        || headers.get_all("host").iter().count() != 1
        || headers.get("host").and_then(|h| h.to_str().ok())
            != Some(state.address.to_string().as_str())
    {
        return refuse(StatusCode::FORBIDDEN, "unsafe_request");
    }
    let expected = format!("Bearer {}", state.token);
    let authorized = headers.get_all("authorization").iter().count() == 1
        && headers
            .get("authorization")
            .is_some_and(|h| bool::from(h.as_bytes().ct_eq(expected.as_bytes())));
    if !authorized {
        return refuse(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let Ok(_permit) = state.slots.clone().try_acquire_owned() else {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "busy");
    };
    let bytes = match tokio::time::timeout(
        Duration::from_secs(10),
        to_bytes(request.into_body(), REQUEST_LIMIT),
    )
    .await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return refuse(StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
        Err(_) => return refuse(StatusCode::REQUEST_TIMEOUT, "body_timeout"),
    };
    let operation = match serde_json::from_slice::<Operation>(&bytes) {
        Ok(op) => op,
        // Most often a client newer than this owner (an update installed, not yet restarted).
        Err(_) => return refuse(StatusCode::BAD_REQUEST, "unknown_operation"),
    };
    let reply = match state.runtime.execute(operation).await {
        Ok(value) => Reply::Ok { value },
        Err(message) => Reply::Error { message },
    };
    Json(reply).into_response()
}

/// None means no descriptor or connection refused before a request could be sent.
/// Authentication/protocol/timeouts never trigger a second, offline mutation.
/// The caller must acquire Store's exclusive lock before any offline operation.
pub async fn request(root: &Path, operation: &Operation) -> Result<Option<Value>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Control metadata unavailable.".into()),
        Ok(_) => {}
    }
    let descriptor: Descriptor =
        serde_json::from_slice(&private_fs::read_private_strict(&path, 4096)?)
            .map_err(|_| "Invalid control metadata. Do not bypass a running owner.".to_string())?;
    if descriptor.protocol != 1
        || descriptor.address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
        || descriptor.address.port() == 0
        || descriptor.token.len() != 64
        || !descriptor.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Unsafe control metadata. Expected a private loopback capability.".into());
    }
    // The owner that wrote it is gone: nothing answers for it, whatever holds the port now.
    // The offline path that follows still takes the store's exclusive lock.
    if !owner_alive(descriptor.pid) {
        return Ok(None);
    }
    let stream = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect(descriptor.address),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => return Ok(None),
        _ => return Err("Control connection did not complete. No operation was attempted.".into()),
    };
    match tokio::time::timeout(
        Duration::from_secs(30),
        request_on_connection(stream, descriptor, operation),
    )
    .await
    {
        Ok(result) => result.map(Some),
        Err(_) => Err("Control request timed out. Check state before retrying a mutation.".into()),
    }
}

struct ConnectionTask(JoinHandle<()>);
impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn read_response(
    mut response: hyper::Response<hyper::body::Incoming>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(frame) = response.body_mut().frame().await {
        let frame =
            frame.map_err(|_| "Control response interrupted. Check state before retrying.")?;
        if let Ok(chunk) = frame.into_data() {
            if bytes.len() + chunk.len() > limit {
                return Err("Control response exceeds size limit.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
    }
    Ok(bytes)
}
async fn request_on_connection(
    stream: tokio::net::TcpStream,
    descriptor: Descriptor,
    operation: &Operation,
) -> Result<Value, String> {
    // One raw HTTP/1 connection, never a pooling client: a proved listener cannot
    // be swapped for a rebound port by an implicit reconnect before the mutation.
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|_| "Control handshake failed. No operation was attempted.")?;
    let _connection = ConnectionTask(tokio::spawn(async move {
        let _ = connection.await;
    }));
    let nonce = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let hello_request = Request::builder()
        .method("GET")
        .uri("/v1/hello")
        .header("host", descriptor.address.to_string())
        .header("x-switchboard-challenge", &nonce)
        .body(Full::new(Bytes::new()))
        .map_err(|_| "Control request unavailable.")?;
    let hello = sender
        .send_request(hello_request)
        .await
        .map_err(|_| "Control identity check did not complete. No operation was attempted.")?;
    if !hello.status().is_success() {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    let owner_version = hello
        .headers()
        .get(VERSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 32)
        .map(str::to_owned);
    if hello
        .headers()
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("close"))
        })
    {
        return Err(
            "Control listener closed its proved connection. No operation was attempted.".into(),
        );
    }
    let hello: Hello = serde_json::from_slice(&read_response(hello, 256).await?)
        .map_err(|_| "Invalid control identity response.")?;
    if !bool::from(
        hello
            .proof
            .as_bytes()
            .ct_eq(proof(&descriptor.token, &nonce).as_bytes()),
    ) {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    let bytes =
        serde_json::to_vec(operation).map_err(|_| "Control request serialization failed.")?;
    if bytes.len() > REQUEST_LIMIT {
        return Err("Control request exceeds size limit.".into());
    }
    let mutation = Request::builder()
        .method("POST")
        .uri("/v1/control")
        .header("host", descriptor.address.to_string())
        .header("authorization", format!("Bearer {}", descriptor.token))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .map_err(|_| "Control request unavailable.")?;
    // hyper sends on a connection only once it is idle again: the hello response just read leaves
    // it finishing that exchange, and a request sent before then is cancelled ("connection was not
    // ready") without leaving this process. Seen on Linux, 12 of 44 CLI-to-owner requests in a
    // loop (2026-10-09); macOS's timing hid it. Nothing has been sent yet, so this is no mutation.
    sender.ready().await.map_err(|_| {
        "Control listener closed its proved connection. No operation was attempted."
    })?;
    let response = sender
        .send_request(mutation)
        .await
        .map_err(|_| "Control request did not complete. Check state before retrying a mutation.")?;
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    if response.status() == StatusCode::BAD_REQUEST {
        return Err(malformed_refusal(owner_version.as_deref()).into());
    }
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CONTROL_BUSY.into());
    }
    if !response.status().is_success() {
        return Err("Control request refused. No offline operation was attempted.".into());
    }
    match serde_json::from_slice::<Reply>(&read_response(response, RESPONSE_LIMIT).await?)
        .map_err(|_| "Invalid control response. Check state before retrying.")?
    {
        Reply::Ok { value } => Ok(value),
        Reply::Error { message } => Err(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Owner;
    use switchboard_core::{AuthKind, MemoryVault, Provider, Store};
    async fn fixture() -> (tempfile::TempDir, Owner, Descriptor) {
        let tmp = tempfile::tempdir().unwrap();
        let owner = Owner::start(tmp.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let desc = serde_json::from_slice(&std::fs::read(tmp.path().join(FILE)).unwrap()).unwrap();
        (tmp, owner, desc)
    }
    #[tokio::test]
    async fn live_owner_accepts_mutation_and_never_returns_secret() {
        let (tmp, owner, _) = fixture().await;
        let result = request(
            tmp.path(),
            &Operation::Add {
                label: "Fixture".into(),
                provider: Provider::Claude,
                kind: AuthKind::ApiKey,
                pool: "work".into(),
                secret: "fixture-secret-only".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result["label"], "Fixture");
        assert!(!result.to_string().contains("fixture-secret-only"));
        let id = result["id"].as_str().unwrap();
        request(
            tmp.path(),
            &Operation::Select {
                provider: Provider::Claude,
                pool: "work".into(),
                id: id.into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            owner.runtime.store.snapshot().unwrap().routes["claude:work"],
            id
        );
        assert!(Store::open(tmp.path().to_owned(), Arc::new(MemoryVault::default())).is_err());
        assert!(!std::fs::read_to_string(tmp.path().join("accounts.json"))
            .unwrap()
            .contains("fixture-secret-only"));
    }
    #[tokio::test]
    async fn auth_host_origin_query_unknown_operation_and_limits_are_enforced() {
        let (_tmp, _owner, desc) = fixture().await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let url = format!("http://{}/v1/control", desc.address);
        assert_eq!(
            client.post(&url).body("{}").send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        for (header, value) in [
            ("origin", "https://evil.example"),
            ("host", "evil.example"),
            ("upgrade", "websocket"),
        ] {
            assert_eq!(
                client
                    .post(&url)
                    .bearer_auth(&desc.token)
                    .header(header, value)
                    .body("{}")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            client
                .post(format!("{url}?credential=true"))
                .bearer_auth(&desc.token)
                .body("{}")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .post(&url)
                .bearer_auth(&desc.token)
                .body(r#"{"operation":"credential","id":"x"}"#)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            client
                .post(&url)
                .bearer_auth(&desc.token)
                .body(vec![b'x'; REQUEST_LIMIT + 1])
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
    #[test]
    fn a_malformed_request_is_named_by_which_side_is_older() {
        assert_eq!(version_triple("0.6.10"), Some((0, 6, 10)));
        assert_eq!(version_triple("0.6.11-rc.1"), Some((0, 6, 11)));
        assert_eq!(version_triple("0.6"), None);
        assert_eq!(version_triple("1.2.3.4"), None);
        // An owner that sends no version predates the header, so it is the older one.
        assert_eq!(malformed_refusal(None), CONTROL_OWNER_OLDER);
        assert_eq!(malformed_refusal(Some("0.6.8")), CONTROL_OWNER_OLDER);
        assert_eq!(malformed_refusal(Some("999.0.0")), CONTROL_OWNER_NEWER);
        assert!(malformed_refusal(Some(env!("CARGO_PKG_VERSION")))
            .starts_with("Control request refused"));
        assert!(malformed_refusal(Some("garbage")).starts_with("Control request refused"));
    }
    #[tokio::test]
    async fn the_owner_names_its_version_and_an_unknown_operation_reads_as_a_skew() {
        let (_tmp, _owner, desc) = fixture().await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let hello = client
            .get(format!("http://{}/v1/hello", desc.address))
            .header("x-switchboard-challenge", "a".repeat(64))
            .send()
            .await
            .unwrap();
        assert_eq!(
            hello.headers().get(VERSION_HEADER).unwrap(),
            env!("CARGO_PKG_VERSION")
        );
        // The body keeps its one field, so a client of an earlier version still reads it.
        let body: serde_json::Value = hello.json().await.unwrap();
        assert_eq!(body.as_object().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn tampered_capability_does_not_fallback_to_offline() {
        let (tmp, _owner, mut desc) = fixture().await;
        desc.token = "0".repeat(64);
        private_fs::private_write(&tmp.path().join(FILE), &serde_json::to_vec(&desc).unwrap())
            .unwrap();
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap_err()
            .contains("authentication refused"));
        desc.address = "192.0.2.1:80".parse().unwrap();
        private_fs::private_write(&tmp.path().join(FILE), &serde_json::to_vec(&desc).unwrap())
            .unwrap();
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap_err()
            .contains("Unsafe control"));
    }
    #[tokio::test]
    async fn missing_and_stale_descriptor_allow_only_lock_guarded_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap()
            .is_none());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let desc = Descriptor {
            protocol: 1,
            address: listener.local_addr().unwrap(),
            token: "a".repeat(64),
            pid: None,
        };
        drop(listener);
        private_fs::private_write(&tmp.path().join(FILE), &serde_json::to_vec(&desc).unwrap())
            .unwrap();
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap()
            .is_none());
    }
    #[tokio::test]
    async fn reused_port_never_receives_bearer_or_secret() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        let leaked = Arc::new(AtomicBool::new(false));
        let observed = leaked.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let descriptor = Descriptor {
            protocol: 1,
            address: listener.local_addr().unwrap(),
            token: "a".repeat(64),
            pid: None,
        };
        private_fs::private_write(
            &tmp.path().join(FILE),
            &serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let router = Router::new().fallback(move |request: Request<Body>| {
            let observed = observed.clone();
            async move {
                let bearer = request.headers().contains_key("authorization");
                let body = to_bytes(request.into_body(), REQUEST_LIMIT).await.unwrap();
                observed.store(bearer || !body.is_empty(), Ordering::SeqCst);
                Json(Hello {
                    proof: "0".repeat(64),
                })
            }
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let result = request(
            tmp.path(),
            &Operation::Add {
                label: "Test".into(),
                provider: Provider::Claude,
                kind: AuthKind::ApiKey,
                pool: "default".into(),
                secret: "never-sent-secret".into(),
            },
        )
        .await;
        assert!(result.unwrap_err().contains("authentication refused"));
        assert!(!leaked.load(Ordering::SeqCst));
        task.abort();
    }
    /// Lifecycle LC-01: a descriptor left by an owner that died says so by its pid. Even with
    /// its old port reused by another program, the CLI goes offline (under the store lock)
    /// instead of refusing until the app starts again.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_descriptor_whose_owner_died_is_stale_even_when_its_port_was_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut gone = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let dead = gone.id();
        gone.wait().unwrap();
        // Another program now answers on the old port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().fallback(|| async {
            Json(Hello {
                proof: "0".repeat(64),
            })
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let write = |pid: Option<u32>| {
            private_fs::private_write(
                &tmp.path().join(FILE),
                &serde_json::to_vec(&Descriptor {
                    protocol: 1,
                    address,
                    token: "a".repeat(64),
                    pid,
                })
                .unwrap(),
            )
            .unwrap()
        };
        write(Some(dead));
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap()
            .is_none());
        // A live owner's descriptor is still proved, and a wrong proof still refused.
        write(Some(std::process::id()));
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap_err()
            .contains("authentication refused"));
        // A descriptor from before pids were recorded keeps the old behaviour.
        write(None);
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap_err()
            .contains("authentication refused"));
        task.abort();
    }
    #[tokio::test]
    async fn the_descriptor_names_the_owners_pid() {
        let (_tmp, _owner, desc) = fixture().await;
        assert_eq!(desc.pid, Some(std::process::id()));
    }
    #[tokio::test]
    async fn proved_connection_close_never_reconnects_to_send_a_secret() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        let received_mutation = Arc::new(AtomicBool::new(false));
        let observed = received_mutation.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let descriptor = Descriptor {
            protocol: 1,
            address: listener.local_addr().unwrap(),
            token: "a".repeat(64),
            pid: None,
        };
        private_fs::private_write(
            &tmp.path().join(FILE),
            &serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let token = descriptor.token.clone();
        let router = Router::new()
            .route(
                "/v1/hello",
                get(move |request: Request<Body>| {
                    let token = token.clone();
                    async move {
                        let nonce = request.headers()["x-switchboard-challenge"]
                            .to_str()
                            .unwrap();
                        (
                            [("connection", "close")],
                            Json(Hello {
                                proof: proof(&token, nonce),
                            }),
                        )
                    }
                }),
            )
            .route(
                "/v1/control",
                post(move || {
                    let observed = observed.clone();
                    async move {
                        observed.store(true, Ordering::SeqCst);
                        Json(Reply::Ok { value: Value::Null })
                    }
                }),
            );
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let result = request(
            tmp.path(),
            &Operation::Add {
                label: "Test".into(),
                provider: Provider::Claude,
                kind: AuthKind::ApiKey,
                pool: "default".into(),
                secret: "never-sent-after-close".into(),
            },
        )
        .await;
        assert!(result.unwrap_err().contains("closed its proved connection"));
        assert!(!received_mutation.load(Ordering::SeqCst));
        task.abort();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn descriptor_is_private_and_symlink_is_rejected() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let (tmp, _owner, _) = fixture().await;
        assert_eq!(
            std::fs::metadata(tmp.path().join(FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::set_permissions(
            tmp.path().join(FILE),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(request(tmp.path(), &Operation::Snapshot)
            .await
            .unwrap_err()
            .contains("permissions are unsafe"));
        std::fs::set_permissions(
            tmp.path().join(FILE),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let other = tempfile::tempdir().unwrap();
        symlink(tmp.path().join(FILE), other.path().join(FILE)).unwrap();
        assert!(request(other.path(), &Operation::Snapshot).await.is_err());
    }
}
