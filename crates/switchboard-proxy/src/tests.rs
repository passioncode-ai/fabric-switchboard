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
        native_context: None,
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
async fn start(store: Arc<Store>, url: String) -> ProxyHandle {
    start_with(store, url, Timeouts::default()).await
}
async fn start_with(store: Arc<Store>, url: String, timeouts: Timeouts) -> ProxyHandle {
    ProxyHandle::start_at(store, url.clone(), url.clone(), url, timeouts, None, None)
        .await
        .unwrap()
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
    let p = start(s.clone(), url).await;
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
    let p = start(s, url).await;
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
    let p = start(s, url).await;
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
    let p = start(s.clone(), url).await;
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
async fn only_unsuccessful_requests_are_journaled_and_usage_is_kept() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let limited = Arc::new(AtomicUsize::new(0));
    let l = limited.clone();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move || {
            let l = l.clone();
            async move {
                let status = if l.fetch_add(1, Ordering::SeqCst) == 1 {
                    StatusCode::TOO_MANY_REQUESTS
                } else {
                    StatusCode::OK
                };
                (
                    status,
                    [
                        ("anthropic-ratelimit-unified-5h-utilization", "0.25"),
                        ("anthropic-ratelimit-unified-7d-utilization", "0.5"),
                    ],
                    "data: complete\n\n",
                )
            }
        }),
    ))
    .await;
    let p = start(s.clone(), url).await;
    let before = s.snapshot().unwrap().events.len();
    assert!(request(&p, "claude")
        .await
        .text()
        .await
        .unwrap()
        .contains("complete"));
    let after_success = s.snapshot().unwrap();
    // A success is the common case; journaling it would evict the events the runtime reads.
    assert_eq!(after_success.events.len(), before);
    assert_eq!(
        after_success.accounts[0]
            .usage
            .as_ref()
            .unwrap()
            .used_percent,
        50.0
    );
    let response = request(&p, "claude").await;
    assert_eq!(response.status(), 429);
    response.text().await.unwrap();
    let after_limit = s.snapshot().unwrap();
    assert_eq!(after_limit.events.len(), before + 1);
    assert_eq!(after_limit.events.last().unwrap().action, "request");
    assert_eq!(after_limit.events.last().unwrap().detail, "rate_limited");
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
    let p = start(s, url).await;
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
    let p = start(s, url).await;
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
async fn body_limit_admits_the_providers_32_mib_and_refuses_one_byte_more() {
    // Anthropic accepts 32 MB request bodies; the local limit must not be the smaller one.
    assert_eq!(BODY_LIMIT, 32 * 1024 * 1024);
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Read the raw body: axum's extractor default limit (2 MB) is not under test here.
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move |request: Request<Body>| {
            let tx = tx.clone();
            async move {
                let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
                tx.send(body.len()).unwrap();
                "ok"
            }
        }),
    ))
    .await;
    let p = start(s, url).await;
    let endpoint = format!("http://{}/claude/default/v1/messages", p.address());
    let client = reqwest::Client::new();
    let response = client
        .post(&endpoint)
        .bearer_auth(p.token())
        .body(vec![b'X'; BODY_LIMIT])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(rx.recv().await.unwrap(), BODY_LIMIT);
    let response = client
        .post(&endpoint)
        .bearer_auth(p.token())
        .body(vec![b'X'; BODY_LIMIT + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    let refusal: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        refusal,
        serde_json::json!({"error":{"type":"switchboard_error","message":"Request exceeds the body limit."}})
    );
    assert!(rx.try_recv().is_err(), "oversized body reached upstream");
    p.shutdown().await;
    up.abort();
}

#[test]
fn quota_windows_keep_each_reset_and_aggregate_the_limiting_window() {
    let observed_at = 1_700_000_000;
    let claude = parse_usage_at(
        Provider::Claude,
        &serde_json::json!({
            "five_hour":{"utilization":20,"resets_at":"2023-11-14T23:13:20.123Z"},
            "seven_day":{"utilization":92,"resets_at":"2023-11-21T22:13:20+00:00"},
            "seven_day_sonnet":{"utilization":0,"resets_at":1700007200},
            "seven_day_opus":null
        }),
        observed_at,
    )
    .unwrap();
    assert_eq!(claude.windows.len(), 3);
    assert_eq!(claude.used_percent, 92.);
    assert_eq!(claude.windows[0].resets_at, Some(observed_at + 3600));
    assert_eq!(claude.resets_at, Some(observed_at + 604800));
    let codex = parse_usage_at(
        Provider::Codex,
        &serde_json::json!({"rate_limit":{
            "primary_window":{"used_percent":80,"reset_after_seconds":500},
            "secondary_window":{"used_percent":10,"reset_at":1700008000,"reset_after_seconds":8001}
        }}),
        observed_at,
    )
    .unwrap();
    assert_eq!(codex.windows.len(), 2);
    assert_eq!(codex.windows[0].resets_at, Some(observed_at + 500));
    assert_eq!(codex.windows[1].resets_at, Some(1700008000));
    assert_eq!(codex.resets_at, Some(observed_at + 500));
}
#[test]
fn malformed_or_partial_windows_never_turn_into_invented_available_quota() {
    let at = 1_700_000_000;
    for value in [
        serde_json::json!({"five_hour":{"utilization":20,"resets_at":"sensitive-invalid-body"}}),
        serde_json::json!({"five_hour":{"utilization":20,"resets_at":1699999999}}),
        serde_json::json!({"five_hour":{"utilization":20},"seven_day":{}}),
        serde_json::json!({"five_hour":null}),
        serde_json::json!({"five_hour":{"utilization":101}}),
    ] {
        let error = parse_usage_at(Provider::Claude, &value, at).unwrap_err();
        assert!(!error.contains("sensitive"));
    }
    for reset in [
        serde_json::json!(-1),
        serde_json::json!(i64::MAX),
        serde_json::json!("later"),
    ] {
        assert!(parse_usage_at(Provider::Codex,&serde_json::json!({"rate_limit":{"primary_window":{"used_percent":1,"reset_after_seconds":reset}}}),at).is_err());
    }
}
#[test]
fn headers_allow_one_window_but_reject_malformed_present_window() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-ratelimit-unified-5h-utilization",
        "0.12".parse().unwrap(),
    );
    headers.insert(
        "anthropic-ratelimit-unified-5h-reset",
        (now() + 3600).to_string().parse().unwrap(),
    );
    let usage = usage_from_headers(&headers).unwrap();
    assert_eq!(usage.windows.len(), 1);
    assert_eq!(usage.used_percent, 12.);
    headers.insert(
        "anthropic-ratelimit-unified-7d-utilization",
        "invalid".parse().unwrap(),
    );
    assert!(usage_from_headers(&headers).is_none());
}
#[tokio::test]
async fn successful_requests_never_push_other_events_out_of_the_journal() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    // Limit attribution reads these after a restart; 300 successes exceed the 256-event ring.
    s.record("activation", Some(&a.id), "completed").unwrap();
    s.record("rotation", Some(&a.id), "switched").unwrap();
    let (url, up) = fixture(Router::new().route("/v1/messages", post(|| async { "ok" }))).await;
    let p = start(s.clone(), url).await;
    let before = s.snapshot().unwrap().events;
    for _ in 0..300 {
        let response = request(&p, "claude").await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), "ok");
    }
    let after = s.snapshot().unwrap().events;
    assert_eq!(after.len(), before.len());
    for (action, detail) in [("activation", "completed"), ("rotation", "switched")] {
        assert!(after.iter().any(|e| e.action == action
            && e.detail == detail
            && e.account_id.as_ref() == Some(&a.id)));
    }
    assert!(!after.iter().any(|e| e.action == "request"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn every_unsuccessful_outcome_is_still_journaled_once() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    // Popped from the end: 401, 403, 429, 500.
    let statuses = Arc::new(std::sync::Mutex::new(vec![500u16, 429, 403, 401]));
    let next = statuses.clone();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move || {
            let next = next.clone();
            async move {
                let status = next.lock().unwrap().pop().unwrap();
                (StatusCode::from_u16(status).unwrap(), "provider body")
            }
        }),
    ))
    .await;
    let p = start(s.clone(), url).await;
    for _ in 0..4 {
        request(&p, "claude").await.text().await.unwrap();
    }
    p.shutdown().await;
    up.abort();
    // A closed port: the connection itself fails.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let p = start(s.clone(), dead).await;
    assert_eq!(request(&p, "claude").await.status(), 502);
    p.shutdown().await;
    let details: Vec<_> = s
        .snapshot()
        .unwrap()
        .events
        .into_iter()
        .filter(|e| e.action == "request")
        .map(|e| (e.detail, e.account_id))
        .collect();
    let id = Some(a.id.clone());
    assert_eq!(
        details,
        [
            "unauthorized",
            "unauthorized",
            "rate_limited",
            "upstream_error",
            "network_error"
        ]
        .map(|d| (d.to_string(), id.clone()))
    );
}
fn timeouts(read: Duration) -> Timeouts {
    Timeouts {
        connect: Duration::from_secs(2),
        read,
    }
}
#[tokio::test]
async fn a_stream_that_keeps_moving_outlives_the_read_timeout_many_times_over() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let read = Duration::from_millis(500);
    // 30 frames 60 ms apart: about 1.8 s in total, never more than 60 ms idle.
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(|| async {
            let stream = futures_util::stream::unfold(0, |step| async move {
                if step == 30 {
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(60)).await;
                Some((
                    Ok::<_, std::io::Error>(Bytes::from(format!("data: {step}\n\n"))),
                    step + 1,
                ))
            });
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(stream))
                .unwrap()
        }),
    ))
    .await;
    let p = start_with(s.clone(), url, timeouts(read)).await;
    let started = std::time::Instant::now();
    let body = request(&p, "claude").await.text().await.unwrap();
    assert!(started.elapsed() > read * 3, "stream ended too early");
    assert!(body.contains("data: 29\n\n"), "stream was cut: {body:?}");
    assert!(!s
        .snapshot()
        .unwrap()
        .events
        .iter()
        .any(|e| e.action == "proxy" && e.detail == "aborted"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn a_stream_idle_past_the_read_timeout_is_cut_and_journaled_as_aborted() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    // The first frame arrives at once, the second only after 5 s.
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(|| async {
            let stream = futures_util::stream::unfold(0, |step| async move {
                match step {
                    0 => Some((
                        Ok::<_, std::io::Error>(Bytes::from_static(b"data: first\n\n")),
                        1,
                    )),
                    1 => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        Some((Ok(Bytes::from_static(b"data: late\n\n")), 2))
                    }
                    _ => None,
                }
            });
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(stream))
                .unwrap()
        }),
    ))
    .await;
    let p = start_with(s.clone(), url, timeouts(Duration::from_millis(300))).await;
    let started = std::time::Instant::now();
    let mut response = request(&p, "claude").await;
    assert_eq!(response.status(), 200);
    let mut received = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        received.extend_from_slice(&chunk);
    }
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "idle stream was not cut"
    );
    let text = String::from_utf8(received).unwrap();
    assert!(text.contains("first") && !text.contains("late"));
    let aborted = || {
        s.snapshot()
            .unwrap()
            .events
            .iter()
            .any(|e| e.action == "proxy" && e.detail == "aborted")
    };
    // The relay stream journals the abort when it sees the upstream error.
    for _ in 0..50 {
        if aborted() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(aborted());
    p.shutdown().await;
    up.abort();
}
#[test]
fn default_timeouts_bound_connect_and_idle_but_not_the_whole_stream() {
    let t = Timeouts::default();
    assert_eq!(t.connect, Duration::from_secs(10));
    // Also bounds the wait for response headers: a non-streaming call may take 10 minutes.
    assert_eq!(t.read, Duration::from_secs(600));
}
#[tokio::test]
async fn claude_code_headers_reach_upstream_and_limit_headers_come_back() {
    let (_r, s) = store();
    let a = add(
        &s,
        Provider::Claude,
        AuthKind::OAuth,
        "work",
        "synthetic-oauth",
    );
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel::<HeaderMap>();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let tx = tx.clone();
            async move {
                tx.send(headers).unwrap();
                Response::builder()
                    .status(429)
                    .header("content-type", "application/json")
                    .header("retry-after", "17")
                    .header("retry-after-ms", "17000")
                    .header("x-should-retry", "true")
                    .header("anthropic-ratelimit-unified-status", "rejected")
                    .header("anthropic-ratelimit-unified-5h-utilization", "1.0")
                    .header("anthropic-ratelimit-unified-5h-reset", "4102444800")
                    .header(
                        "anthropic-ratelimit-unified-representative-claim",
                        "five_hour",
                    )
                    .header("anthropic-ratelimit-requests-remaining", "0")
                    .header("set-cookie", "upstream-session=private")
                    .header("x-envoy-upstream-service-time", "12")
                    .body(Body::from("{}"))
                    .unwrap()
            }
        }),
    ))
    .await;
    let p = start(s, url).await;
    let response = reqwest::Client::new()
        .post(format!(
            "http://{}/claude/default/v1/messages?beta=true",
            p.address()
        ))
        .bearer_auth(p.token())
        .header("user-agent", "claude-cli/9.9.9 (external, cli)")
        .header("x-app", "cli")
        .header("anthropic-version", "2023-06-01")
        .header(
            "anthropic-beta",
            "claude-code-20250219,interleaved-thinking-2025-05-14",
        )
        .header("x-stainless-retry-count", "0")
        .header("x-api-key", "client-supplied-key")
        .header("cookie", "client-cookie")
        .header("proxy-authorization", "Basic client-proxy")
        .body("{}")
        .send()
        .await
        .unwrap();
    let h = rx.recv().await.unwrap();
    assert_eq!(h["user-agent"], "claude-cli/9.9.9 (external, cli)");
    assert_eq!(h["x-app"], "cli");
    assert_eq!(h["anthropic-version"], "2023-06-01");
    assert_eq!(h.get_all("anthropic-version").iter().count(), 1);
    assert_eq!(
        h["anthropic-beta"],
        "claude-code-20250219,interleaved-thinking-2025-05-14,oauth-2025-04-20"
    );
    assert_eq!(h.get_all("anthropic-beta").iter().count(), 1);
    assert_eq!(h["x-stainless-retry-count"], "0");
    // The account's bearer replaces the local capability; nothing the client authenticated
    // with reaches the provider under any header name.
    assert_eq!(h["authorization"], "Bearer synthetic-oauth");
    assert_eq!(h.get_all("authorization").iter().count(), 1);
    for name in ["x-api-key", "cookie", "proxy-authorization"] {
        assert!(!h.contains_key(name), "{name} was forwarded");
    }
    for (name, value) in &h {
        let value = value.to_str().unwrap_or_default();
        assert!(!value.contains(p.token()), "local token leaked in {name}");
        assert!(
            !value.contains("client-"),
            "client credential leaked in {name}"
        );
    }
    assert_eq!(response.status(), 429);
    let r = response.headers();
    assert_eq!(r["retry-after"], "17");
    assert_eq!(r["retry-after-ms"], "17000");
    assert_eq!(r["x-should-retry"], "true");
    assert_eq!(r["anthropic-ratelimit-unified-status"], "rejected");
    assert_eq!(r["anthropic-ratelimit-unified-5h-utilization"], "1.0");
    assert_eq!(r["anthropic-ratelimit-unified-5h-reset"], "4102444800");
    assert_eq!(
        r["anthropic-ratelimit-unified-representative-claim"],
        "five_hour"
    );
    assert_eq!(r["anthropic-ratelimit-requests-remaining"], "0");
    assert!(!r.contains_key("set-cookie"));
    assert!(!r.contains_key("x-envoy-upstream-service-time"));
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn connection_listed_headers_are_not_relayed_in_either_direction() {
    let (_r, s) = store();
    let a = add(&s, Provider::Claude, AuthKind::ApiKey, "a", "synthetic");
    s.select(Provider::Claude, "default", &a.id).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel::<HeaderMap>();
    let (url, up) = fixture(Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let tx = tx.clone();
            async move {
                tx.send(headers).unwrap();
                Response::builder()
                    .header(
                        "connection",
                        "anthropic-ratelimit-unified-status, x-should-retry",
                    )
                    .header("anthropic-ratelimit-unified-status", "allowed")
                    .header("anthropic-ratelimit-unified-reset", "4102444800")
                    .header("x-should-retry", "false")
                    .body(Body::from("ok"))
                    .unwrap()
            }
        }),
    ))
    .await;
    let p = start(s, url).await;
    let response = reqwest::Client::new()
        .post(format!("http://{}/claude/default/v1/messages", p.address()))
        .bearer_auth(p.token())
        .header("connection", "user-agent, x-app")
        .header("user-agent", "hop-only")
        .header("x-app", "hop-only")
        .body("{}")
        .send()
        .await
        .unwrap();
    let h = rx.recv().await.unwrap();
    assert!(!h.contains_key("x-app"));
    assert!(!h.contains_key("user-agent"));
    let r = response.headers();
    assert!(!r.contains_key("anthropic-ratelimit-unified-status"));
    assert!(!r.contains_key("x-should-retry"));
    assert_eq!(r["anthropic-ratelimit-unified-reset"], "4102444800");
    p.shutdown().await;
    up.abort();
}
#[tokio::test]
async fn usage_checks_report_the_providers_wait_and_never_its_body() {
    use axum::routing::get;
    let router = Router::new()
        .route(
            "/limited/api/oauth/usage",
            get(|| async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", "1200")],
                    "upstream detail that must not leak",
                )
            }),
        )
        .route(
            "/dated/api/oauth/usage",
            get(|| async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")],
                    "",
                )
            }),
        )
        .route(
            "/rejected/api/oauth/usage",
            get(|| async { StatusCode::UNAUTHORIZED }),
        )
        .route(
            "/ok/api/oauth/usage",
            get(|| async {
                axum::Json(serde_json::json!({"five_hour":{"utilization":42.0,"resets_at":null}}))
            }),
        );
    let (base, _task) = fixture(router).await;
    let (_root, store) = store();
    let a = add(
        &store,
        Provider::Claude,
        AuthKind::OAuth,
        "a",
        "synthetic-token",
    );
    let probe = |path: &str| {
        let origin = format!("{base}/{path}");
        let store = store.clone();
        let id = a.id.clone();
        async move { probe_usage_from(store, id, (&origin, &origin)).await }
    };
    assert_eq!(
        probe("limited").await.unwrap_err(),
        ProbeFailure {
            message: USAGE_RATE_LIMITED.into(),
            rate_limited: Some(Some(1200))
        }
    );
    assert_eq!(probe("dated").await.unwrap_err().rate_limited, Some(None));
    let rejected = probe("rejected").await.unwrap_err();
    assert_eq!(rejected.message, REJECTED);
    assert_eq!(rejected.rate_limited, None);
    assert_eq!(probe("ok").await.unwrap().used_percent, 42.0);
}
