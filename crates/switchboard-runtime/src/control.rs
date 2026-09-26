//! Private, capability-authenticated CLI control channel, separate from inference.
use crate::{Operation, Runtime};
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
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
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    protocol: u8,
    address: SocketAddr,
    token: String,
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
    task: JoinHandle<()>,
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
        Ok(Self { task, descriptor })
    }
}
impl Drop for ControlHandle {
    fn drop(&mut self) {
        // Store remains locked by the server's Runtime until abort is processed.
        self.task.abort();
        let _ = std::fs::remove_file(&self.descriptor);
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
    Json(Hello {
        proof: proof(&state.token, nonce),
    })
    .into_response()
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
        return StatusCode::FORBIDDEN.into_response();
    }
    let expected = format!("Bearer {}", state.token);
    let authorized = headers.get_all("authorization").iter().count() == 1
        && headers
            .get("authorization")
            .is_some_and(|h| bool::from(h.as_bytes().ct_eq(expected.as_bytes())));
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(_permit) = state.slots.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let bytes = match tokio::time::timeout(
        Duration::from_secs(10),
        to_bytes(request.into_body(), REQUEST_LIMIT),
    )
    .await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => return StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    let operation = match serde_json::from_slice::<Operation>(&bytes) {
        Ok(op) => op,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::symlink_metadata(&path)
            .map_err(|_| "Control metadata unavailable.")?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err("Control capability permissions are unsafe. Restart its owner to rotate the capability.".into());
        }
    }
    let descriptor: Descriptor = serde_json::from_slice(&private_fs::read_private(&path, 4096)?)
        .map_err(|_| "Invalid control metadata. Do not bypass a running owner.".to_string())?;
    if descriptor.protocol != 1
        || descriptor.address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
        || descriptor.address.port() == 0
        || descriptor.token.len() != 64
        || !descriptor.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Unsafe control metadata. Expected a private loopback capability.".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Control client unavailable.")?;
    // A stale private descriptor can name a port that another service reused.
    // Authenticate the listener before sending either bearer or credential bytes.
    let nonce = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut hello = match client
        .get(format!("http://{}/v1/hello", descriptor.address))
        .header("x-switchboard-challenge", &nonce)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) if e.is_connect() => return Ok(None),
        Err(_) => {
            return Err(
                "Control identity check did not complete. No operation was attempted.".into(),
            )
        }
    };
    if !hello.status().is_success() {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    let mut hello_bytes = Vec::new();
    while let Some(chunk) = hello
        .chunk()
        .await
        .map_err(|_| "Control identity response interrupted.")?
    {
        if hello_bytes.len() + chunk.len() > 256 {
            return Err("Invalid control identity response.".into());
        }
        hello_bytes.extend_from_slice(&chunk);
    }
    let hello: Hello =
        serde_json::from_slice(&hello_bytes).map_err(|_| "Invalid control identity response.")?;
    if !bool::from(
        hello
            .proof
            .as_bytes()
            .ct_eq(proof(&descriptor.token, &nonce).as_bytes()),
    ) {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    let mut response = match client
        .post(format!("http://{}/v1/control", descriptor.address))
        .bearer_auth(descriptor.token)
        .json(operation)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return Err(
                "Control request did not complete. Check state before retrying a mutation.".into(),
            )
        }
    };
    if response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN {
        return Err("Control authentication refused. No offline operation was attempted.".into());
    }
    if !response.status().is_success() {
        return Err("Control request refused. No offline operation was attempted.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Control response interrupted. Check state before retrying.")?
    {
        if bytes.len() + chunk.len() > RESPONSE_LIMIT {
            return Err("Control response exceeds size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    match serde_json::from_slice::<Reply>(&bytes)
        .map_err(|_| "Invalid control response. Check state before retrying.")?
    {
        Reply::Ok { value } => Ok(Some(value)),
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
