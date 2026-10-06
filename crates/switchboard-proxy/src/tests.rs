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
                    [("retry-after", "Fri, 31 Dec 9999 23:59:59 GMT")],
                    "",
                )
            }),
        )
        .route(
            "/past/api/oauth/usage",
            get(|| async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", "Sun, 06 Nov 1994 08:49:37 GMT")],
                    "",
                )
            }),
        )
        .route(
            "/garbled/api/oauth/usage",
            get(|| async { (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "soon")], "") }),
        )
        .route(
            "/rejected/api/oauth/usage",
            get(|| async { StatusCode::UNAUTHORIZED }),
        )
        .route(
            "/broken/api/oauth/usage",
            get(|| async {
                (
                    StatusCode::BAD_GATEWAY,
                    "upstream detail that must not leak",
                )
            }),
        )
        .route("/garbage/api/oauth/usage", get(|| async { "not json" }))
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
        async move { probe_usage_from(store, id, (&origin, &origin), None).await }
    };
    assert_eq!(
        probe("limited").await.unwrap_err(),
        ProbeFailure {
            message: USAGE_RATE_LIMITED.into(),
            rate_limited: Some(Some(1200))
        }
    );
    // An HTTP-date is a deadline too (RFC 9110 §10.2.3), not a missing value.
    let dated = probe("dated")
        .await
        .unwrap_err()
        .rate_limited
        .unwrap()
        .unwrap();
    assert!(dated > 0, "a future date is a positive wait, got {dated}");
    assert_eq!(probe("past").await.unwrap_err().rate_limited, Some(Some(0)));
    assert_eq!(probe("garbled").await.unwrap_err().rate_limited, Some(None));
    let rejected = probe("rejected").await.unwrap_err();
    assert_eq!(rejected.message, REJECTED);
    assert_eq!(rejected.rate_limited, None);
    assert_eq!(probe("ok").await.unwrap().used_percent, 42.0);
    // The operations log's code for each way a check fails (LC-12): fixed, never the body.
    assert_eq!(probe("limited").await.unwrap_err().kind(), "rate_limited");
    assert_eq!(rejected.kind(), "rejected");
    assert_eq!(probe("broken").await.unwrap_err().kind(), "http_error");
    assert_eq!(probe("garbage").await.unwrap_err().kind(), "unsupported");
    let unreachable = probe_usage_from(
        store.clone(),
        a.id.clone(),
        ("http://127.0.0.1:9", "http://127.0.0.1:9"),
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(unreachable.kind(), "unreachable");
    assert_eq!(ProbeFailure::from("Account not found.").kind(), "other");
}

/// RFC 9110 §10.2.3 and §5.6.7: delay-seconds and the three HTTP-date forms, measured against
/// the receipt time; nothing a provider sends can wrap, go negative or parse as a sign.
#[test]
fn retry_after_reads_seconds_and_every_http_date_form() {
    // 1994-11-06 08:49:37 UTC, the RFC's own example instant.
    let example = 784_111_777;
    let received = example - 120;
    assert_eq!(retry_after_at("120", received), Some(120));
    assert_eq!(retry_after_at(" 7 ", received), Some(7));
    assert_eq!(retry_after_at("0", received), Some(0));
    assert_eq!(
        retry_after_at("Sun, 06 Nov 1994 08:49:37 GMT", received),
        Some(120)
    );
    assert_eq!(
        retry_after_at("Sunday, 06-Nov-94 08:49:37 GMT", received),
        Some(120)
    );
    assert_eq!(
        retry_after_at("Sun Nov  6 08:49:37 1994", received),
        Some(120)
    );
    // A date already past asks for no wait at all; the caller applies its floor.
    assert_eq!(
        retry_after_at("Sun, 06 Nov 1994 08:49:37 GMT", example + 5),
        Some(0)
    );
    // Overflow saturates instead of wrapping into a past or negative deadline.
    assert_eq!(
        retry_after_at("99999999999999999999999999", received),
        Some(i64::MAX)
    );
    assert_eq!(
        retry_after_at("Fri, 31 Dec 9999 23:59:59 GMT", received),
        Some(253_402_300_799 - received)
    );
    for bad in [
        "",
        "-5",
        "+5",
        "1.5",
        "5s",
        "soon",
        "Sun, 06 Nov 1994 08:49:37 PST",
        "Mon, 06 Nov 1994 08:49:37 GMT",
    ] {
        assert_eq!(
            retry_after_at(bad, received),
            None,
            "{bad:?} is not a Retry-After"
        );
    }
}

/// A dated `Retry-After` is measured on the provider's clock when the response carries `Date`:
/// a local clock three hours ahead cannot turn a two-hour wait into none.
#[test]
fn a_dated_retry_after_is_measured_on_the_providers_clock() {
    let sent = 784_111_777; // Sun, 06 Nov 1994 08:49:37 GMT
    let mut headers = HeaderMap::new();
    headers.insert("date", "Sun, 06 Nov 1994 08:49:37 GMT".parse().unwrap());
    headers.insert(
        "retry-after",
        "Sun, 06 Nov 1994 10:49:37 GMT".parse().unwrap(),
    );
    assert_eq!(retry_after_seconds(&headers, sent + 3 * 3600), Some(7200));
    // Without a usable Date the receipt time is all there is.
    headers.insert("date", "yesterday".parse().unwrap());
    assert_eq!(retry_after_seconds(&headers, sent + 3600), Some(3600));
    headers.remove("date");
    assert_eq!(retry_after_seconds(&headers, sent), Some(7200));
    // Delay-seconds never depend on either clock.
    headers.insert("retry-after", "30".parse().unwrap());
    assert_eq!(retry_after_seconds(&headers, sent + 99_999), Some(30));
}

/// SB-40: the payload of the official Codex client's own test
/// (`usage_payload_maps_primary_and_additional_rate_limits`, openai/codex@afb436d), as JSON.
fn official_codex_payload() -> serde_json::Value {
    serde_json::json!({
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": 42, "limit_window_seconds": 300, "reset_after_seconds": 0, "reset_at": 2_000_000_123},
            "secondary_window": {"used_percent": 84, "limit_window_seconds": 3600, "reset_after_seconds": 0, "reset_at": 2_000_000_456}
        },
        "additional_rate_limits": [{
            "limit_name": "codex_other", "metered_feature": "codex_other",
            "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 70, "limit_window_seconds": 900, "reset_after_seconds": 0, "reset_at": 2_000_000_789}}
        }],
        "credits": {"has_credits": true, "unlimited": false, "balance": "9.99"},
        "spend_control": {"reached": false, "individual_limit": {
            "source": null, "limit": "25000", "used": "8000", "remaining": "17000",
            "used_percent": 32, "remaining_percent": 68, "reset_after_seconds": 3600, "reset_at": 2_000_000_789}},
        "rate_limit_reached_type": {"type": "workspace_member_credits_depleted"}
    })
}
fn window<'a>(usage: &'a Usage, name: &str) -> Option<&'a UsageWindow> {
    usage.windows.iter().find(|w| w.name == name)
}

#[test]
fn codex_usage_keeps_every_dimension_the_official_client_reads() {
    let at = 2_000_000_000;
    let usage = parse_usage_at(Provider::Codex, &official_codex_payload(), at).unwrap();
    assert_eq!(window(&usage, "primary").unwrap().used_percent, 42.0);
    assert_eq!(
        window(&usage, "primary").unwrap().resets_at,
        Some(2_000_000_123)
    );
    assert_eq!(window(&usage, "secondary").unwrap().used_percent, 84.0);
    let feature = window(&usage, "feature_codex_other_primary").unwrap();
    assert_eq!(
        (feature.used_percent, feature.resets_at),
        (70.0, Some(2_000_000_789))
    );
    let spend = window(&usage, "spend_limit").unwrap();
    assert_eq!(
        (spend.used_percent, spend.resets_at),
        (32.0, Some(2_000_000_789))
    );
    // The workspace has run out of credits: no seat of it can work, whatever the windows say.
    let credits = window(&usage, "workspace_credits").unwrap();
    assert_eq!((credits.used_percent, credits.resets_at), (100.0, None));
    assert_eq!(usage.account_used_percent(), Some(100.0));
    // The credit balance is a purchase measure, not a limit, and is not stored.
    assert!(!serde_json::to_string(&usage).unwrap().contains("9.99"));
}

#[test]
fn a_feature_limit_is_scoped_and_does_not_stand_for_the_account() {
    let at = 2_000_000_000;
    let mut payload = official_codex_payload();
    payload["additional_rate_limits"][0]["rate_limit"]["primary_window"]["used_percent"] =
        100.into();
    payload["spend_control"] = serde_json::Value::Null;
    payload["rate_limit_reached_type"] = serde_json::Value::Null;
    let usage = parse_usage_at(Provider::Codex, &payload, at).unwrap();
    // Stored aggregate stays the maximum over every window (older builds read it).
    assert_eq!(usage.used_percent, 100.0);
    // The account's own capacity is the primary and secondary windows.
    assert_eq!(usage.account_used_percent(), Some(84.0));
}

#[test]
fn a_plan_without_main_windows_has_unknown_account_capacity() {
    let usage = parse_usage_at(
        Provider::Codex,
        &serde_json::json!({"plan_type": "plus", "rate_limit": null, "additional_rate_limits": [
            {"limit_name": "codex_other", "metered_feature": "codex_other",
             "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 10, "limit_window_seconds": 900, "reset_after_seconds": 60, "reset_at": 2_000_000_060}}}]}),
        2_000_000_000,
    )
    .unwrap();
    assert!(window(&usage, "feature_codex_other_primary").is_some());
    assert_eq!(usage.account_used_percent(), None, "missing is not zero");
    // Nothing usable at all is still no observation.
    assert!(parse_usage_at(
        Provider::Codex,
        &serde_json::json!({"plan_type": "plus", "rate_limit": null}),
        2_000_000_000
    )
    .is_err());
}

#[test]
fn spend_control_and_reached_limits_block_whatever_the_percentages() {
    let at = 2_000_000_000;
    let base = |extra: serde_json::Value| {
        let mut payload = serde_json::json!({"plan_type": "pro", "rate_limit": {"allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": 5, "limit_window_seconds": 300, "reset_after_seconds": 0, "reset_at": 2_000_000_300}}});
        for (k, v) in extra.as_object().unwrap() {
            if k == "rate_limit" {
                for (rk, rv) in v.as_object().unwrap() {
                    payload["rate_limit"][rk] = rv.clone();
                }
            } else {
                payload[k] = v.clone();
            }
        }
        parse_usage_at(Provider::Codex, &payload, at).unwrap()
    };
    // Reached spend control without details: blocked, reset unknown.
    let u = base(serde_json::json!({"spend_control": {"reached": true}}));
    assert_eq!(
        (
            window(&u, "spend_limit").unwrap().used_percent,
            window(&u, "spend_limit").unwrap().resets_at
        ),
        (100.0, None)
    );
    assert_eq!(u.account_used_percent(), Some(100.0));
    // The provider says the limit is reached while no window reads 100 %.
    for flags in [
        serde_json::json!({"rate_limit": {"limit_reached": true}}),
        serde_json::json!({"rate_limit": {"allowed": false}}),
        serde_json::json!({"rate_limit_reached_type": {"type": "rate_limit_reached"}}),
    ] {
        let u = base(flags);
        assert_eq!(window(&u, "limit_reached").unwrap().used_percent, 100.0);
        assert_eq!(u.account_used_percent(), Some(100.0));
    }
    // Workspace usage limit: an organisation block.
    let u = base(
        serde_json::json!({"rate_limit_reached_type": {"type": "workspace_owner_usage_limit_reached"}}),
    );
    assert_eq!(
        window(&u, "workspace_usage_limit").unwrap().used_percent,
        100.0
    );
    // An unknown reason, unknown fields: ignored, nothing invented.
    let u = base(
        serde_json::json!({"rate_limit_reached_type": {"type": "something_new"}, "brand_new_field": {"x": 1}}),
    );
    assert_eq!(u.windows.len(), 1);
    assert_eq!(u.account_used_percent(), Some(5.0));
    // An under-limit spend control with details is a measurement, not a block.
    let u = base(
        serde_json::json!({"spend_control": {"reached": false, "individual_limit": {"limit": "1", "used": "0", "remaining": "1",
        "used_percent": 0, "remaining_percent": 100, "reset_after_seconds": 10, "reset_at": 2_000_000_010}}}),
    );
    assert_eq!(window(&u, "spend_limit").unwrap().used_percent, 0.0);
}

#[test]
fn feature_names_are_sanitised_bounded_and_unique() {
    let at = 2_000_000_000;
    let feature = |name: &str, used: i64| {
        serde_json::json!({"limit_name": name, "metered_feature": name,
        "rate_limit": {"allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": used, "limit_window_seconds": 60, "reset_after_seconds": 0, "reset_at": 2_000_000_060},
            "secondary_window": {"used_percent": used, "limit_window_seconds": 600, "reset_after_seconds": 0, "reset_at": 2_000_000_600}}})
    };
    let mut features: Vec<_> = (0..20).map(|i| feature(&format!("f{i}"), 1)).collect();
    features.insert(0, feature("Codex Other/Model:<script>", 3));
    features.insert(1, feature("codex_other_model__script_", 4));
    features.insert(2, feature(&"x".repeat(200), 5));
    features.insert(3, feature("", 6));
    let usage = parse_usage_at(Provider::Codex, &serde_json::json!({"plan_type": "pro",
        "rate_limit": {"allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": 1, "limit_window_seconds": 300, "reset_after_seconds": 0, "reset_at": 2_000_000_300}},
        "additional_rate_limits": features}), at).unwrap();
    let names: Vec<&str> = usage
        .windows
        .iter()
        .map(|w| w.name.as_str())
        .filter(|n| n.starts_with("feature_"))
        .collect();
    assert!(names.len() <= 12, "{names:?}");
    // Sanitised and trimmed; a second feature that sanitises to the same id is dropped.
    let first = window(&usage, "feature_codex_other_model__script_primary").unwrap();
    assert_eq!(first.used_percent, 3.0);
    assert!(names.iter().all(|n| n.len() <= 64
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')));
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    assert!(usage.windows.len() <= 16);
    assert!(!serde_json::to_string(&usage).unwrap().contains("script>"));
}

/// SB-40 review F1: every blocker at once plus many features still fits the store's 16 windows,
/// and the store accepts the observation.
#[test]
fn every_blocker_with_many_features_still_fits_the_store() {
    // Real time: the store refuses an observation stamped ahead of the clock.
    let at = now();
    let features: Vec<_> = (0..8)
        .map(|i| serde_json::json!({"limit_name": format!("f{i}"), "metered_feature": format!("f{i}"),
            "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 1, "limit_window_seconds": 60, "reset_after_seconds": 60},
                "secondary_window": {"used_percent": 1, "limit_window_seconds": 600, "reset_after_seconds": 600}}}))
        .collect();
    let usage = parse_usage_at(Provider::Codex, &serde_json::json!({"plan_type": "pro",
        "rate_limit": {"allowed": false, "limit_reached": false,
            "primary_window": {"used_percent": 50, "limit_window_seconds": 300, "reset_after_seconds": 300},
            "secondary_window": {"used_percent": 50, "limit_window_seconds": 3600, "reset_after_seconds": 3600}},
        "spend_control": {"reached": true},
        "rate_limit_reached_type": {"type": "workspace_member_credits_depleted"},
        "additional_rate_limits": features}), at).unwrap();
    assert_eq!(usage.windows.len(), 16);
    for name in ["spend_limit", "workspace_credits", "limit_reached"] {
        assert!(window(&usage, name).is_some(), "{name}");
    }
    let (_root, store) = store();
    let a = add(
        &store,
        Provider::Codex,
        AuthKind::OAuth,
        "a",
        "synthetic-token",
    );
    store.observe(&a.id, usage).unwrap();
}

/// SB-40 review F5: an overspend reads above 100 % on the wire; a bad reset loses only itself.
#[test]
fn an_overspent_limit_is_clamped_and_a_bad_reset_loses_only_itself() {
    let at = 2_000_000_000;
    let parse = |limit: serde_json::Value, reached: bool| {
        parse_usage_at(Provider::Codex, &serde_json::json!({"plan_type": "pro",
            "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 5, "limit_window_seconds": 300, "reset_after_seconds": 0, "reset_at": 2_000_000_300}},
            "spend_control": {"reached": reached, "individual_limit": limit}}), at).unwrap()
    };
    let over = parse(
        serde_json::json!({"limit": "1", "used": "2", "remaining": "0", "used_percent": 180,
        "remaining_percent": 0, "reset_after_seconds": 3600, "reset_at": 2_000_003_600}),
        false,
    );
    let spend = window(&over, "spend_limit").unwrap();
    assert_eq!(
        (spend.used_percent, spend.resets_at),
        (100.0, Some(2_000_003_600))
    );
    // A past `reset_at` with no usable fallback: the block stays, its reset is unknown.
    let past = parse(
        serde_json::json!({"limit": "1", "used": "1", "remaining": "0", "used_percent": 100,
        "remaining_percent": 0, "reset_at": 5}),
        true,
    );
    let spend = window(&past, "spend_limit").unwrap();
    assert_eq!((spend.used_percent, spend.resets_at), (100.0, None));
    // Not reached, measured, bad reset: the measurement is kept without a reset.
    let measured = parse(
        serde_json::json!({"limit": "10", "used": "3", "remaining": "7", "used_percent": 30,
        "remaining_percent": 70, "reset_at": 0}),
        false,
    );
    assert_eq!(window(&measured, "spend_limit").unwrap().used_percent, 30.0);
}

/// SB-40 review F8: the same features listed in another order are the same observation.
#[test]
fn feature_order_does_not_change_the_observation() {
    let at = 2_000_000_000;
    let feature = |name: &str| {
        serde_json::json!({"limit_name": name, "metered_feature": name,
        "rate_limit": {"allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": 3, "limit_window_seconds": 60, "reset_after_seconds": 0, "reset_at": 2_000_000_060}}})
    };
    let parse = |features: Vec<serde_json::Value>| {
        parse_usage_at(Provider::Codex, &serde_json::json!({"plan_type": "pro",
            "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 1, "limit_window_seconds": 300, "reset_after_seconds": 0, "reset_at": 2_000_000_300}},
            "additional_rate_limits": features}), at).unwrap()
    };
    let one = parse(vec![feature("beta"), feature("alpha")]);
    let two = parse(vec![feature("alpha"), feature("beta")]);
    let names = |u: &Usage| u.windows.iter().map(|w| w.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&one), names(&two));
}

// Third-party agents (operator request 2026-10-05): their own capability, API-key accounts only.
#[tokio::test]
async fn agents_reach_api_key_accounts_and_never_a_subscription_sign_in() {
    let (_root, s) = store();
    let key = add(
        &s,
        Provider::Claude,
        AuthKind::ApiKey,
        "key",
        "synthetic-key",
    );
    let sub = add(
        &s,
        Provider::Claude,
        AuthKind::OAuth,
        "sub",
        "synthetic-oauth",
    );
    s.select(Provider::Claude, "default", &key.id).unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let router = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers["x-api-key"], "synthetic-key");
                "{}"
            }
        }),
    );
    let (url, up) = fixture(router).await;
    let p = start(s.clone(), url).await;
    let agent = p.agent_token().to_owned();
    assert_ne!(
        agent,
        p.token(),
        "the agent capability is not the session token"
    );
    assert_eq!(
        agent,
        agent_token(p.token()),
        "stable across restarts with the token"
    );
    let url = format!("http://{}/claude/default/v1/messages", p.address());
    let client = reqwest::Client::new();
    // As `x-api-key` (Anthropic SDKs) and as a bearer.
    let by_key = client
        .post(&url)
        .header("x-api-key", &agent)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(by_key.status(), 200);
    let by_bearer = client
        .post(&url)
        .bearer_auth(&agent)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(by_bearer.status(), 200);
    // The session token is never accepted as `x-api-key`, and nothing else gets in.
    let wrong = client
        .post(&url)
        .header("x-api-key", p.token())
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    // A subscription sign-in selected: refused for agents before any provider call.
    s.select(Provider::Claude, "default", &sub.id).unwrap();
    let refused = client
        .post(&url)
        .header("x-api-key", &agent)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 403);
    assert!(refused
        .text()
        .await
        .unwrap()
        .contains("subscription sign-in"));
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "the provider was never asked"
    );
    p.shutdown().await;
    up.abort();
}

#[tokio::test]
async fn chat_completions_reach_openai_with_an_api_key_only() {
    let (_root, s) = store();
    let key = add(
        &s,
        Provider::Codex,
        AuthKind::ApiKey,
        "key",
        "synthetic-openai",
    );
    s.select(Provider::Codex, "default", &key.id).unwrap();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer synthetic-openai");
            "{\"choices\":[]}"
        }),
    );
    let (url, up) = fixture(router).await;
    let p = start(s.clone(), url).await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/codex/default/v1/chat/completions", p.address());
    let ok = client
        .post(&url)
        .bearer_auth(p.agent_token())
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    assert!(ok.text().await.unwrap().contains("choices"));
    // A ChatGPT sign-in has no Chat Completions: refused, even for Codex's own capability.
    let sub = add(
        &s,
        Provider::Codex,
        AuthKind::OAuth,
        "sub",
        "synthetic-chatgpt",
    );
    s.select(Provider::Codex, "default", &sub.id).unwrap();
    let refused = client
        .post(&url)
        .bearer_auth(p.token())
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 404);
    p.shutdown().await;
    up.abort();
}
