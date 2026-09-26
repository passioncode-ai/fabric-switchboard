use super::*;
use axum::{body::Bytes, routing::post};
use std::sync::atomic::{AtomicUsize, Ordering};
use switchboard_core::{Credential, MemoryVault};
use tokio::sync::{mpsc, Notify};

fn store() -> (tempfile::TempDir, Arc<Store>) {
    let root = tempfile::tempdir().unwrap();
    let store =
        Arc::new(Store::open(root.path().join("data"), Arc::new(MemoryVault::default())).unwrap());
    (root, store)
}
fn add(
    store: &Store,
    provider: Provider,
    kind: AuthKind,
    label: &str,
    token: &str,
) -> AccountForTest {
    let credential = Credential {
        access_token: token.into(),
        refresh_token: None,
        id_token: None,
        expires_at: None,
        account_id: if kind == AuthKind::OAuth {
            Some(format!("identity-{label}"))
        } else {
            None
        },
    };
    let a = store
        .add(label.into(), provider, kind, "default".into(), credential)
        .unwrap();
    AccountForTest { id: a.id }
}
struct AccountForTest {
    id: String,
}
async fn fixture(router: Router) -> (String, JoinHandle<()>) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = l.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    (format!("http://{address}"), task)
}
async fn request(proxy: &ProxyHandle, provider: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!(
            "http://{}/{provider}/default/v1/{}",
            proxy.address(),
            if provider == "claude" {
                "messages"
            } else {
                "responses"
            }
        ))
        .bearer_auth(proxy.token())
        .body("{}")
        .send()
        .await
        .unwrap()
}
#[tokio::test]
async fn auth_is_injected_and_route_changes_only_next_request() {
    let (_root, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic-a");
    let b = add(&s, Provider::Claude, AuthKind::ApiKey, "b", "synthetic-b");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel::<HeaderMap>();
    let release = Arc::new(Notify::new());
    let release2 = release.clone();
    let router = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let tx = tx.clone();
            let release = release2.clone();
            async move {
                let first = headers.get("x-api-key").unwrap() == "synthetic-a";
                tx.send(headers).unwrap();
                let stream = futures_util::stream::unfold(0, move |step| {
                    let release = release.clone();
                    async move {
                        match step {
                            0 => Some((
                                Ok::<_, std::io::Error>(Bytes::from_static(b"data: first\n\n")),
                                1,
                            )),
                            1 => {
                                if first {
                                    release.notified().await;
                                }
                                Some((Ok(Bytes::from_static(b"data: complete\n\n")), 2))
                            }
                            _ => None,
                        }
                    }
                });
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        }),
    );
    let (url, up) = fixture(router).await;
    let p = ProxyHandle::start_at(s.clone(), url.clone(), url.clone(), url)
        .await
        .unwrap();
    let mut first = request(&p, "claude").await;
    let headers = rx.recv().await.unwrap();
    assert_eq!(headers["x-api-key"], "synthetic-a");
    assert!(!headers.contains_key("authorization"));
    assert!(first.chunk().await.unwrap().is_some());
    s.select(Provider::Claude, "default", &b.id).unwrap();
    let second = request(&p, "claude").await;
    assert_eq!(rx.recv().await.unwrap()["x-api-key"], "synthetic-b");
    assert!(second.text().await.unwrap().contains("complete"));
    release.notify_one();
    assert!(first.text().await.unwrap().contains("complete"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn codex_replaces_both_headers_and_strips_cookies() {
    let (_r, s) = store();
    let a = add(
        &s,
        Provider::Codex,
        AuthKind::OAuth,
        "work",
        "synthetic-oauth",
    );
    s.select(Provider::Codex, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (url, up) = fixture(Router::new().route(
        "/backend-api/codex/responses",
        post(move |h: HeaderMap| {
            let tx = tx.clone();
            async move {
                tx.send(h).unwrap();
                "ok"
            }
        }),
    ))
    .await;
    let p = ProxyHandle::start_at(s, url.clone(), url.clone(), url)
        .await
        .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{}/codex/default/v1/responses", p.address()))
        .bearer_auth(p.token())
        .header("chatgpt-account-id", "wrong")
        .header("cookie", "private-cookie")
        .header("x-api-key", "wrong")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let h = rx.recv().await.unwrap();
    assert_eq!(h["authorization"], "Bearer synthetic-oauth");
    assert_eq!(h["chatgpt-account-id"], "identity-work");
    assert!(!h.contains_key("cookie"));
    assert!(!h.contains_key("x-api-key"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn unauthorized_origin_and_unknown_paths_never_contact_upstream() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let (url, up) = fixture(Router::new().fallback(move || {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            "bad"
        }
    }))
    .await;
    let p = ProxyHandle::start_at(s, url.clone(), url.clone(), url)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let endpoint = format!("http://{}/claude/default/v1/messages", p.address());
    assert_eq!(
        client
            .post(&endpoint)
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(p.token())
            .header("origin", "https://evil.invalid")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(p.token())
            .header("host", "evil.invalid")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("http://{}/claude/default/v1/unknown", p.address()))
            .bearer_auth(p.token())
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(p.token())
            .header("upgrade", "websocket")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        501
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn rejection_is_never_replayed_and_events_contain_no_secrets() {
    let (_r, s) = store();
    let a = add(
        &s,
        Provider::Claude,
        AuthKind::ApiKey,
        "a",
        "SYNTHETIC_SECRET_NEVER_LOG",
    );
    let _b = add(
        &s,
        Provider::Claude,
        AuthKind::ApiKey,
        "b",
        "synthetic-other",
    );
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                (StatusCode::TOO_MANY_REQUESTS, "provider error body")
            }
        }),
    ))
    .await;
    let p = ProxyHandle::start_at(s.clone(), url.clone(), url.clone(), url)
        .await
        .unwrap();
    assert_eq!(request(&p, "claude").await.status(), 429);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let json = serde_json::to_string(&s.snapshot().unwrap()).unwrap();
    assert!(!json.contains("SYNTHETIC_SECRET"));
    assert!(!json.contains("provider error body"));
    assert_eq!(s.snapshot().unwrap().routes["claude:default"], a.id);
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn redirects_do_not_exfiltrate_credentials() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let (sink, sink_task) = fixture(Router::new().fallback(move || {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            "stolen"
        }
    }))
    .await;
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move || {
            let sink = sink.clone();
            async move {
                Response::builder()
                    .status(307)
                    .header("location", sink)
                    .body(Body::empty())
                    .unwrap()
            }
        }),
    ))
    .await;
    let p = ProxyHandle::start_at(s, url.clone(), url.clone(), url)
        .await
        .unwrap();
    assert_eq!(request(&p, "claude").await.status(), 502);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    p.shutdown().await;
    up.abort();
    sink_task.abort();
}
#[test]
fn usage_missing_is_not_zero() {
    assert!(parse_usage(Provider::Codex, &serde_json::json!({})).is_err());
    assert!(parse_usage(
        Provider::Claude,
        &serde_json::json!({"five_hour":{"utilization":-1}})
    )
    .is_err());
    assert_eq!(parse_usage(Provider::Codex,&serde_json::json!({"rate_limit":{"primary_window":{"used_percent":0},"secondary_window":{"used_percent":64}}})).unwrap().used_percent,64.);
}

#[tokio::test]
async fn claude_beta_query_is_forwarded_and_connection_fields_are_removed() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move |request: Request<Body>| {
            let tx = tx.clone();
            async move {
                tx.send((request.uri().to_string(), request.headers().clone()))
                    .unwrap();
                Response::builder()
                    .header("connection", "retry-after")
                    .header("retry-after", "42")
                    .body(Body::from("ok"))
                    .unwrap()
            }
        }),
    ))
    .await;
    let p = ProxyHandle::start_at(s, url.clone(), url.clone(), url)
        .await
        .unwrap();
    let endpoint = format!(
        "http://{}/claude/default/v1/messages?beta=true",
        p.address()
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(p.token())
        .header("connection", "anthropic-beta")
        .header("anthropic-beta", "client-hop-only")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!response.headers().contains_key("retry-after"));
    let (uri, h) = rx.recv().await.unwrap();
    assert_eq!(uri, "/v1/messages?beta=true");
    assert!(!h.contains_key("anthropic-beta"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn incomplete_stream_is_aborted_instead_of_success() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    let permit = Arc::new(Semaphore::new(1)).acquire_owned().await.unwrap();
    let stream = AuditedStream {
        inner: Box::pin(futures_util::stream::pending()),
        store: s.clone(),
        id: a.id.clone(),
        finished: false,
        success: true,
        _permit: permit,
    };
    drop(stream);
    let snap = s.snapshot().unwrap();
    assert!(snap
        .events
        .iter()
        .any(|e| e.action == "proxy" && e.detail == "aborted"));
    assert!(!snap
        .events
        .iter()
        .any(|e| e.action == "request" && e.detail == "success"));
}
#[tokio::test]
async fn oversized_body_is_rejected_without_upstream_request() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let (url, up) = fixture(Router::new().fallback(move || {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            "bad"
        }
    }))
    .await;
    let p = ProxyHandle::start_at(s, url.clone(), url.clone(), url)
        .await
        .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{}/claude/default/v1/messages", p.address()))
        .bearer_auth(p.token())
        .body(vec![b'X'; BODY_LIMIT + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    p.shutdown().await;
    up.abort();
}
