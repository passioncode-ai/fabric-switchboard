use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use switchboard_core::{Account, AuthKind, Credential, MemoryVault, Provider, Store, Usage, Vault};
use tempfile::TempDir;

fn token(value: &str) -> Credential {
    Credential::parse(Provider::Claude, AuthKind::ApiKey, value).unwrap()
}
/// Opens a store another handle of this test just dropped. Tests running beside it start this
/// binary again as a child process; a child forked at that moment shares the lock's open file
/// until it execs, so the exclusive lock can stay taken for a few milliseconds after the drop
/// (seen once on the Linux arm64 nightly, 2026-10-12).
fn reopen(root: &std::path::Path, vault: Arc<MemoryVault>) -> Store {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match Store::open(root.into(), vault.clone()) {
            Ok(store) => return store,
            Err(error) if std::time::Instant::now() >= deadline => panic!("{error}"),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}
fn setup() -> (TempDir, Arc<MemoryVault>, Store) {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    (root, vault, store)
}
fn add(store: &Store, secret: &str) -> Account {
    store
        .add(
            "Synthetic account".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token(secret),
        )
        .unwrap()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn parses_provider_formats_and_rejects_invalid_credentials() {
    let c = Credential::parse(Provider::Claude, AuthKind::OAuth, r#"{"claudeAiOauth":{"accessToken":"synthetic","refreshToken":"synthetic-refresh","expiresAt":2000000000000}}"#).unwrap();
    assert_eq!(c.expires_at, Some(2000000000));
    let c = Credential::parse(Provider::Codex, AuthKind::OAuth, r#"{"tokens":{"access_token":"synthetic","refresh_token":"synthetic-refresh","account_id":"account-claimed"}}"#).unwrap();
    assert_eq!(c.account_id.as_deref(), Some("account-claimed"));
    for bad in ["", "a\nb", "a b", "a\0b", "a\rb", "é"] {
        assert!(Credential::parse(Provider::Claude, AuthKind::ApiKey, bad).is_err());
    }
    assert!(Credential::parse(Provider::Codex, AuthKind::SetupToken, "synthetic").is_err());
    assert!(!Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        "synthetic-sensitive-invalid"
    )
    .err()
    .unwrap()
    .contains("synthetic-sensitive"));
    assert!(Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        r#"{"claudeAiOauth":{"accessToken":"synthetic","expiresAt":"later"}}"#
    )
    .is_err());
    assert!(Credential::parse(
        Provider::Codex,
        AuthKind::OAuth,
        r#"{"tokens":{"access_token":"synthetic","account_id":"bad\r\nheader"}}"#
    )
    .is_err());
    assert!(Credential::parse(Provider::Claude, AuthKind::ApiKey, &"x".repeat(65_537)).is_err());
}

#[test]
fn codex_jwt_expiry_is_a_hint_not_a_verified_identity() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let body =
        URL_SAFE_NO_PAD.encode(br#"{"exp":1700000000,"email":"unverified@example.invalid"}"#);
    let json =
        serde_json::json!({"tokens":{"access_token":format!("header.{body}.synthetic-signature")}})
            .to_string();
    let c = Credential::parse(Provider::Codex, AuthKind::OAuth, &json).unwrap();
    assert_eq!(c.expires_at, Some(1700000000));
    assert!(c.account_id.is_none());
}

#[test]
fn codex_id_token_is_preserved_in_vault_only_and_is_bounded() {
    let (_root, vault, store) = setup();
    let input = serde_json::json!({"tokens": {
        "access_token": "synthetic-access", "id_token": "synthetic.claimed.identity-token",
        "refresh_token": "synthetic-refresh", "account_id": "claimed-account"
    }})
    .to_string();
    let credential = Credential::parse(Provider::Codex, AuthKind::OAuth, &input).unwrap();
    assert_eq!(
        credential.id_token.as_deref(),
        Some("synthetic.claimed.identity-token")
    );
    let account = store
        .add(
            "Claimed account".into(),
            Provider::Codex,
            AuthKind::OAuth,
            "default".into(),
            credential,
        )
        .unwrap();
    assert_eq!(
        vault.get(&account.id).unwrap().id_token.as_deref(),
        Some("synthetic.claimed.identity-token")
    );
    let snapshot = serde_json::to_string(&store.snapshot().unwrap()).unwrap();
    assert!(!snapshot.contains("identity-token"));
    assert!(!snapshot.contains("id_token"));
    for value in [
        "bad\nheader".into(),
        "x".repeat(16 * 1024 + 1),
        String::new(),
    ] {
        let input = serde_json::json!({"tokens":{"access_token":"synthetic", "id_token":value}})
            .to_string();
        assert!(Credential::parse(Provider::Codex, AuthKind::OAuth, &input).is_err());
    }
    let legacy: Credential = serde_json::from_str(
        r#"{"access_token":"synthetic","refresh_token":null,"expires_at":null,"account_id":null}"#,
    )
    .unwrap();
    assert!(legacy.id_token.is_none());
}

#[test]
fn lifecycle_routes_are_scoped_and_survive_restart_without_secret_metadata() {
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic-secret-alpha");
    let b = add(&store, "synthetic-secret-beta");
    store.select(Provider::Claude, "default", &a.id).unwrap();
    let captured = store.route(Provider::Claude, "default").unwrap();
    store.select(Provider::Claude, "default", &b.id).unwrap();
    assert_eq!(captured.0.id, a.id);
    assert_eq!(captured.1.access_token, "synthetic-secret-alpha");
    assert_eq!(store.route(Provider::Claude, "default").unwrap().0.id, b.id);
    assert!(store.select(Provider::Codex, "default", &a.id).is_err());
    assert!(store.select(Provider::Claude, "work", &a.id).is_err());
    assert!(store.route(Provider::Claude, "work").is_err());
    assert!(store.remove(&b.id).is_err());
    store.update(&b.id, "Disabled".into(), false).unwrap();
    assert!(store.route(Provider::Claude, "default").is_err());
    assert!(store.select(Provider::Claude, "default", &b.id).is_err());
    store.remove(&b.id).unwrap();
    assert!(vault.get(&b.id).is_err());
    store.select(Provider::Claude, "default", &a.id).unwrap();
    let before = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    drop(store);
    let store = reopen(root.path(), vault);
    assert_eq!(
        before,
        serde_json::to_value(store.snapshot().unwrap()).unwrap()
    );
    let disk = fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(!disk.contains("synthetic-secret"));
    assert!(!disk.contains("access_token"));
}

#[test]
fn duplicates_are_secret_and_pool_based_and_direct_structs_are_validated() {
    let (_root, _vault, store) = setup();
    add(&store, "synthetic");
    assert!(store
        .add(
            "Different label".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic")
        )
        .is_err());
    store
        .add(
            "Different pool".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "work".into(),
            token("synthetic"),
        )
        .unwrap();
    add(&store, "synthetic-other");
    for pool in ["", "../work", "Work", "has space"] {
        assert!(store
            .add(
                "Label".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                pool.into(),
                token("synthetic-new")
            )
            .is_err());
    }
    assert!(store
        .add(
            " ".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic-new")
        )
        .is_err());
    assert!(store
        .add(
            "Label".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            Credential {
                access_token: "unsafe\nheader".into(),
                refresh_token: None,
                id_token: None,
                native_context: None,
                expires_at: None,
                account_id: None
            }
        )
        .is_err());
}

#[test]
fn expired_or_missing_credentials_never_become_selected() {
    let (_root, vault, store) = setup();
    let a = store
        .add(
            "Expired".into(),
            Provider::Codex,
            AuthKind::OAuth,
            "default".into(),
            Credential {
                access_token: "synthetic".into(),
                refresh_token: None,
                id_token: None,
                native_context: None,
                expires_at: Some(now() - 1),
                account_id: None,
            },
        )
        .unwrap();
    assert!(store.select(Provider::Codex, "default", &a.id).is_err());
    assert!(store.credential(&a.id).is_err());
    let b = add(&store, "synthetic-live");
    store.select(Provider::Claude, "default", &b.id).unwrap();
    vault.delete(&b.id).unwrap();
    assert!(store.route(Provider::Claude, "default").is_err());
    assert_eq!(store.snapshot().unwrap().accounts.len(), 2);
}

#[test]
fn lock_contends_and_releases_on_drop() {
    let (root, vault, store) = setup();
    assert!(Store::open(root.path().into(), vault.clone()).is_err());
    drop(store);
    reopen(root.path(), vault);
}

#[test]
fn corruption_unknown_schema_and_oversized_metadata_fail_closed() {
    for content in ["invalid".to_owned(), r#"{"schema_version":42,"snapshot":{"accounts":[],"routes":{},"events":[]}}"#.into(), "x".repeat(2 * 1024 * 1024 + 1), r#"{"schema_version":1,"snapshot":{"accounts":[],"routes":{"claude:default":"unknown"},"events":[]}}"#.into()] {
        let root = TempDir::new().unwrap(); fs::write(root.path().join("accounts.json"), content).unwrap();
        assert!(Store::open(root.path().into(), Arc::new(MemoryVault::default())).is_err());
    }
}

#[cfg(unix)]
#[test]
fn private_permissions_and_symlink_and_hardlink_refusal() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (root, vault, store) = setup();
    add(&store, "synthetic");
    assert_eq!(
        fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.path().join("accounts.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(root.path().join("instance.lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(store);
    let other = TempDir::new().unwrap();
    symlink(root.path(), other.path().join("linked-root")).unwrap();
    assert!(Store::open(other.path().join("linked-root"), vault.clone()).is_err());
    fs::rename(
        root.path().join("accounts.json"),
        root.path().join("safe.json"),
    )
    .unwrap();
    symlink(
        root.path().join("safe.json"),
        root.path().join("accounts.json"),
    )
    .unwrap();
    assert!(Store::open(root.path().into(), vault.clone()).is_err());
    fs::remove_file(root.path().join("accounts.json")).unwrap();
    fs::hard_link(
        root.path().join("safe.json"),
        root.path().join("accounts.json"),
    )
    .unwrap();
    assert!(Store::open(root.path().into(), vault.clone()).is_err());
    fs::remove_file(root.path().join("accounts.json")).unwrap();
    fs::remove_file(root.path().join("instance.lock")).unwrap();
    symlink(
        root.path().join("safe.json"),
        root.path().join("instance.lock"),
    )
    .unwrap();
    assert!(Store::open(root.path().into(), vault).is_err());
}

#[derive(Default)]
struct FaultVault {
    memory: MemoryVault,
    fail_put: AtomicBool,
    fail_delete: AtomicBool,
    fail_get: AtomicBool,
    written_ids: std::sync::Mutex<Vec<String>>,
}
impl Vault for FaultVault {
    fn put(&self, id: &str, c: &Credential) -> Result<(), String> {
        if self.fail_put.load(Ordering::SeqCst) {
            Err("synthetic-sensitive-error".into())
        } else {
            self.written_ids.lock().unwrap().push(id.into());
            self.memory.put(id, c)
        }
    }
    fn get(&self, id: &str) -> Result<Credential, String> {
        if self.fail_get.load(Ordering::SeqCst) {
            Err("synthetic-sensitive-error".into())
        } else {
            self.memory.get(id)
        }
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        if self.fail_delete.load(Ordering::SeqCst) {
            Err("synthetic-sensitive-error".into())
        } else {
            self.memory.delete(id)
        }
    }
}

#[test]
fn vault_failures_preserve_state_and_never_echo_secret_errors() {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(FaultVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    vault.fail_put.store(true, Ordering::SeqCst);
    let e = store
        .add(
            "Label".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic"),
        )
        .err()
        .unwrap();
    assert!(!e.contains("sensitive"));
    assert!(store.snapshot().unwrap().accounts.is_empty());
    vault.fail_put.store(false, Ordering::SeqCst);
    let a = add(&store, "synthetic");
    vault.fail_delete.store(true, Ordering::SeqCst);
    assert!(store.remove(&a.id).is_err());
    assert_eq!(store.snapshot().unwrap().accounts.len(), 1);
    vault.fail_get.store(true, Ordering::SeqCst);
    assert!(store.select(Provider::Claude, "default", &a.id).is_err());
    assert!(store.snapshot().unwrap().routes.is_empty());
}

#[test]
fn disk_failure_rolls_back_memory_and_added_secret() {
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic-existing");
    fs::remove_file(root.path().join("accounts.json")).unwrap();
    fs::create_dir(root.path().join("accounts.json")).unwrap();
    assert!(store.update(&a.id, "Changed".into(), false).is_err());
    assert!(store.snapshot().unwrap().accounts[0].enabled);
    assert!(store
        .add(
            "New".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic-new")
        )
        .is_err());
    assert_eq!(store.snapshot().unwrap().accounts.len(), 1);
    fs::remove_dir(root.path().join("accounts.json")).unwrap();
    // A second add with the same token works after rollback and recovery.
    add(&store, "synthetic-new");
    assert!(vault.get(&a.id).is_ok());
}

#[test]
fn failed_removal_publication_leaves_visible_unusable_account_and_retry_recovers() {
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic");
    fs::remove_file(root.path().join("accounts.json")).unwrap();
    fs::create_dir(root.path().join("accounts.json")).unwrap();
    assert!(store.remove(&a.id).is_err());
    assert!(vault.get(&a.id).is_err());
    assert_eq!(store.snapshot().unwrap().accounts.len(), 1);
    assert!(store.select(Provider::Claude, "default", &a.id).is_err());
    fs::remove_dir(root.path().join("accounts.json")).unwrap();
    store.remove(&a.id).unwrap();
    assert!(store.snapshot().unwrap().accounts.is_empty());
}

#[test]
fn usage_validates_bounds_and_preserves_previous_observation() {
    let (_root, _vault, store) = setup();
    let a = add(&store, "synthetic");
    let good = Usage {
        windows: vec![],
        used_percent: 0.0,
        observed_at: now(),
        resets_at: Some(now() + 500),
        source: "claude_oauth".into(),
    };
    store.observe(&a.id, good.clone()).unwrap();
    for n in [f64::NAN, f64::INFINITY, -0.1, 100.1] {
        let mut bad = good.clone();
        bad.used_percent = n;
        assert!(store.observe(&a.id, bad).is_err());
    }
    let mut bad = good.clone();
    bad.source = "synthetic-sensitive-provider-body".into();
    assert!(store.observe(&a.id, bad).is_err());
    let mut bad = good.clone();
    bad.observed_at = now() + 3600;
    assert!(store.observe(&a.id, bad).is_err());
    let mut bad = good.clone();
    bad.observed_at -= 1;
    assert!(store.observe(&a.id, bad).is_err());
    assert_eq!(
        store.snapshot().unwrap().accounts[0]
            .usage
            .as_ref()
            .unwrap()
            .used_percent,
        0.0
    );
}

#[test]
fn clock_set_back_after_observation_keeps_store_open_and_writable() {
    use switchboard_core::RotationPolicy;
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic-a");
    let b = add(&store, "synthetic-b");
    let usage = |at: i64| Usage {
        windows: vec![],
        used_percent: 40.0,
        observed_at: at,
        resets_at: Some(at + 7200),
        source: "claude_oauth".into(),
    };
    store.observe(&a.id, usage(now())).unwrap();
    store
        .set_policy(RotationPolicy {
            provider: Provider::Claude,
            pool: "default".into(),
            target: "managed".into(),
            enabled: false,
            threshold_percent: 90.0,
            hysteresis_percent: 10.0,
            cooldown_seconds: 1800,
            max_age_seconds: 300,
            last_switched_at: None,
        })
        .unwrap();
    drop(store);
    // Model the wall clock moving back one hour: every stored time is now ahead of it.
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let ahead = now() + 3600;
    let account = &mut disk["snapshot"]["accounts"][0];
    account["usage"]["observed_at"] = ahead.into();
    account["usage"]["resets_at"] = (ahead + 7200).into();
    account["usage_health"]["checked_at"] = ahead.into();
    account["usage_health"]["next_check_at"] = (ahead + 180).into();
    disk["snapshot"]["policies"][0]["last_switched_at"] = ahead.into();
    fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();

    let store = reopen(root.path(), vault);
    store.select(Provider::Claude, "default", &b.id).unwrap();
    store
        .select_with_cooldown(Provider::Claude, "default", &a.id, now())
        .unwrap();
    add(&store, "synthetic-c");
    store
        .usage_health(&a.id, "failed", now(), now() + 180)
        .unwrap();
    store.observe(&a.id, usage(now())).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.accounts[0].usage.as_ref().unwrap().observed_at <= now());
    assert!(snapshot.policies[0].last_switched_at.unwrap() <= now());
    // New input from the future is still refused.
    assert!(store.observe(&b.id, usage(now() + 3600)).is_err());
    assert!(store
        .usage_health(&b.id, "ok", now() + 3600, now() + 3780)
        .is_err());
}

#[test]
fn header_usage_merges_into_stored_windows_and_is_not_journaled() {
    use switchboard_core::UsageWindow;
    let (_root, _vault, store) = setup();
    let a = add(&store, "synthetic");
    let t = now();
    let window = |name: &str, used_percent: f64, resets_at: i64| UsageWindow {
        name: name.into(),
        used_percent,
        resets_at: Some(resets_at),
    };
    store
        .observe(
            &a.id,
            Usage {
                windows: vec![
                    window("five_hour", 20.0, t + 3600),
                    window("seven_day", 30.0, t + 86_400),
                    window("seven_day_opus", 70.0, t + 90_000),
                    window("seven_day_sonnet", 90.0, t - 5),
                ],
                used_percent: 90.0,
                observed_at: t - 10,
                resets_at: Some(t - 5),
                source: "claude_oauth".into(),
            },
        )
        .unwrap();
    let events = store.snapshot().unwrap().events.len();
    store
        .observe(
            &a.id,
            Usage {
                windows: vec![
                    window("five_hour", 50.0, t + 3000),
                    window("seven_day", 35.0, t + 86_400),
                ],
                used_percent: 50.0,
                observed_at: t,
                resets_at: Some(t + 3000),
                source: "response_headers".into(),
            },
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let usage = snapshot.accounts[0].usage.as_ref().unwrap();
    let windows: Vec<_> = usage
        .windows
        .iter()
        .map(|w| (w.name.as_str(), w.used_percent, w.resets_at))
        .collect();
    // Header values replace same-named windows; a window whose reset passed is unknown now.
    assert_eq!(
        windows,
        [
            ("five_hour", 50.0, Some(t + 3000)),
            ("seven_day", 35.0, Some(t + 86_400)),
            ("seven_day_opus", 70.0, Some(t + 90_000)),
        ]
    );
    assert_eq!(usage.used_percent, 70.0);
    assert_eq!(usage.resets_at, Some(t + 90_000));
    // The Opus value was not observed by these headers: retained history must
    // not acquire a fresh aggregate timestamp and become a rotation candidate.
    assert_eq!(usage.observed_at, t - 10);
    assert_eq!(usage.source, "response_headers");
    assert_eq!(snapshot.events.len(), events);
}

#[test]
fn events_are_allowlisted_bounded_and_persisted() {
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic");
    assert!(store
        .record("request", Some(&a.id), "synthetic-secret-provider-body")
        .is_err());
    assert!(store
        .record("synthetic-sensitive-action", None, "success")
        .is_err());
    assert!(store
        .record("request", Some("synthetic-secret-id"), "success")
        .is_err());
    for (action, detail) in [
        ("activation", "completed"),
        ("activation", "failed"),
        ("rotation", "switched"),
        ("rotation", "failed"),
    ] {
        store.record(action, Some(&a.id), detail).unwrap();
    }
    assert!(store.record("rotation", Some(&a.id), "held").is_err());
    assert!(store.record("activation", Some(&a.id), "switched").is_err());
    for _ in 0..270 {
        store.record("request", Some(&a.id), "success").unwrap();
    }
    assert_eq!(store.snapshot().unwrap().events.len(), 256);
    drop(store);
    assert_eq!(
        reopen(root.path(), vault).snapshot().unwrap().events.len(),
        256
    );
}

#[test]
fn concurrent_updates_are_serialized_without_lost_accounts() {
    let (_root, _, store) = setup();
    let store = Arc::new(store);
    let threads: Vec<_> = (0..16)
        .map(|n| {
            let store = store.clone();
            std::thread::spawn(move || add(&store, &format!("synthetic-{n}")))
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.accounts.len(), 16);
    assert_eq!(snapshot.events.len(), 16);
}

#[test]
fn add_rollback_deletes_exact_new_item_and_failed_cleanup_is_explicit() {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(FaultVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    fs::create_dir(root.path().join("accounts.json")).unwrap();
    assert!(store
        .add(
            "Label".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic")
        )
        .is_err());
    let first_id = vault.written_ids.lock().unwrap()[0].clone();
    assert!(vault.get(&first_id).is_err());
    vault.fail_delete.store(true, Ordering::SeqCst);
    let error = store
        .add(
            "Label".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            token("synthetic"),
        )
        .err()
        .unwrap();
    assert!(error.contains("cleanup requires recovery"));
    assert!(!error.contains("sensitive"));
    let second_id = vault.written_ids.lock().unwrap()[1].clone();
    assert!(vault.get(&second_id).is_ok());
    assert!(store.snapshot().unwrap().accounts.is_empty());
}

#[test]
fn simultaneous_duplicate_adds_publish_only_one_account() {
    let (_root, _, store) = setup();
    let store = Arc::new(store);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                store.add(
                    "Label".into(),
                    Provider::Claude,
                    AuthKind::ApiKey,
                    "default".into(),
                    token("synthetic-shared"),
                )
            })
        })
        .collect();
    let successes = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(Result::is_ok)
        .count();
    assert_eq!(successes, 1);
    assert_eq!(store.snapshot().unwrap().accounts.len(), 1);
}

#[test]
fn child_process_cannot_open_locked_store() {
    let (root, _vault, _store) = setup();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_lock_probe", "--nocapture"])
        .env("SWITCHBOARD_SYNTHETIC_LOCK_PROBE", root.path())
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn process_lock_probe() {
    if let Some(path) = std::env::var_os("SWITCHBOARD_SYNTHETIC_LOCK_PROBE") {
        assert!(Store::open(path.into(), Arc::new(MemoryVault::default())).is_err());
    }
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Explicit synthetic native Keychain acceptance; may require an OS access prompt"]
fn native_vault_roundtrip_uses_only_random_app_owned_item() {
    use switchboard_core::NativeVault;
    let vault = NativeVault::new();
    let id = uuid::Uuid::new_v4().to_string();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = NativeVault::new().delete(&self.0);
        }
    }
    let _cleanup = Cleanup(id.clone());
    let expected = token("synthetic-keychain-test-only");
    vault.put(&id, &expected).unwrap();
    assert_eq!(vault.get(&id).unwrap().access_token, expected.access_token);
    vault.delete(&id).unwrap();
    assert!(vault.get(&id).is_err());
    vault.delete(&id).unwrap();
}

/// SB-88: the Linux vault against a real Secret Service. `scripts/linux/check.sh` runs it in a
/// D-Bus session with an unlocked GNOME Keyring and sets SWITCHBOARD_SECRET_SERVICE_TEST=1;
/// elsewhere it reports that it did not run.
#[cfg(target_os = "linux")]
#[test]
fn linux_vault_roundtrips_through_the_secret_service_and_refuses_without_one() {
    use switchboard_core::backup::BackupKey;
    use switchboard_core::NativeVault;
    if std::env::var("SWITCHBOARD_SECRET_SERVICE_TEST").as_deref() != Ok("1") {
        eprintln!("not run: needs a Secret Service session (scripts/linux/check.sh)");
        return;
    }
    let vault = NativeVault::new();
    let id = uuid::Uuid::new_v4().to_string();
    let expected = token("synthetic-secret-service-test-only");
    vault.put(&id, &expected).unwrap();
    assert_eq!(vault.get(&id).unwrap().access_token, expected.access_token);
    // A second put replaces the item in place.
    let renewed = token("synthetic-secret-service-renewed");
    vault.put(&id, &renewed).unwrap();
    assert_eq!(vault.get(&id).unwrap().access_token, renewed.access_token);
    vault.delete(&id).unwrap();
    assert!(vault.get(&id).is_err());
    vault.delete(&id).unwrap();
    assert!(vault.get("not-a-uuid").is_err());
    // The backup key is created once and never replaced.
    let key = switchboard_core::backup::SecretServiceKey;
    let first = key.load().unwrap();
    let candidate = [7u8; 32];
    let created = key.create(&candidate).unwrap();
    assert_eq!(created, first.is_none());
    let stored = key.load().unwrap().unwrap();
    assert!(!key.create(&[9u8; 32]).unwrap(), "an existing key is kept");
    assert_eq!(key.load().unwrap().unwrap(), stored);
    // Without a session bus there is no Secret Service: storage refuses and names the fix.
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "linux_vault_refuses_without_a_session_bus",
            "--include-ignored",
            "--nocapture",
        ])
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/bus")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "run by linux_vault_roundtrips_through_the_secret_service_and_refuses_without_one with no session bus"]
fn linux_vault_refuses_without_a_session_bus() {
    use switchboard_core::NativeVault;
    let id = uuid::Uuid::new_v4().to_string();
    let error = NativeVault::new()
        .put(&id, &token("never-stored"))
        .unwrap_err();
    assert_eq!(error, switchboard_core::secret_service::UNAVAILABLE);
}

#[test]
fn a_renewal_goes_only_to_the_owner_the_token_endpoint_named() {
    use switchboard_core::ExternalIdentity;
    let (_root, _vault, store) = setup();
    let oauth = |refresh: &str| {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(
                r#"{{"claudeAiOauth":{{"accessToken":"a-{refresh}","refreshToken":"{refresh}"}}}}"#
            ),
        )
        .unwrap()
    };
    let identity = |account: &str| ExternalIdentity {
        account_id: Some(account.into()),
        organization_id: None,
        email: Some(format!("{account}@example.invalid")),
    };
    // Account A holds B's lineage by mistake; B holds it rightfully.
    let a = store
        .upsert(
            "A".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            oauth("shared"),
            Some(identity("synthetic-a")),
        )
        .unwrap();
    let b = store
        .upsert(
            "B".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "work".into(),
            oauth("shared"),
            Some(identity("synthetic-b")),
        )
        .unwrap();
    let next = oauth("next");
    let (updated, others) = store
        .adopt_refreshed_for(Provider::Claude, "shared", &next, Some("synthetic-b"))
        .unwrap();
    assert_eq!(updated, vec![b.id.clone()]);
    assert_eq!(others, vec![a.id.clone()]);
    assert_eq!(
        store
            .stored_credential(&b.id)
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("next")
    );
    assert_eq!(
        store
            .stored_credential(&a.id)
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("shared")
    );
    // Nobody holds a spent token any more: nothing updated, and no error.
    let (updated, others) = store
        .adopt_refreshed_for(Provider::Claude, "gone", &next, None)
        .unwrap();
    assert!(updated.is_empty() && others.is_empty());
    assert!(store
        .adopt_refreshed(Provider::Claude, "gone", &next)
        .is_err());
    // Without an owner every holder is updated.
    let (updated, _) = store
        .adopt_refreshed_for(Provider::Claude, "shared", &oauth("third"), None)
        .unwrap();
    assert_eq!(updated, vec![a.id]);
}

/// Lifecycle LC-08: a quota check that finds nothing new moves only timestamps. Those stay in
/// memory until something real changes, the store is flushed, or it closes.
#[test]
fn an_unchanged_quota_check_is_not_written_until_something_changes() {
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic");
    let metadata = || fs::read(root.path().join("accounts.json")).unwrap();
    let resets_at = now() + 7200;
    let usage = |at: i64, used: f64| Usage {
        windows: vec![],
        used_percent: used,
        observed_at: at,
        resets_at: Some(resets_at),
        source: "claude_oauth".into(),
    };
    let t = now() - 100;
    store.observe(&a.id, usage(t, 40.0)).unwrap();
    let writes = store.metadata_writes();
    let on_disk = metadata();
    for step in 1..=10 {
        store.observe(&a.id, usage(t + step, 40.0)).unwrap();
    }
    assert_eq!(store.metadata_writes(), writes, "only timestamps moved");
    assert_eq!(metadata(), on_disk);
    let usage_of = |s: &Store| s.snapshot().unwrap().accounts[0].usage.clone().unwrap();
    assert_eq!(
        usage_of(&store).observed_at,
        t + 10,
        "memory has the newest"
    );
    // A real change is written at once, carrying the pending timestamps with it.
    store.observe(&a.id, usage(t + 20, 41.0)).unwrap();
    assert_eq!(store.metadata_writes(), writes + 1);
    // A failure with the status unchanged is a timestamp too; a new status is written.
    store
        .usage_health(&a.id, "failed", t + 30, t + 210)
        .unwrap();
    assert_eq!(store.metadata_writes(), writes + 2, "ok → failed");
    store
        .usage_health(&a.id, "failed", t + 40, t + 400)
        .unwrap();
    assert_eq!(store.metadata_writes(), writes + 2, "failed → failed");
    // Flush writes what is pending once, and only once.
    store.flush().unwrap();
    assert_eq!(store.metadata_writes(), writes + 3);
    store.flush().unwrap();
    assert_eq!(store.metadata_writes(), writes + 3);
    // Closing the store writes what is still pending.
    store
        .usage_health(&a.id, "failed", t + 50, t + 500)
        .unwrap();
    assert_eq!(store.metadata_writes(), writes + 3);
    drop(store);
    let reopened = reopen(root.path(), vault);
    let health = reopened.snapshot().unwrap().accounts[0]
        .usage_health
        .clone()
        .unwrap();
    assert_eq!((health.checked_at, health.next_check_at), (t + 50, t + 500));
}

#[test]
fn pending_timestamps_are_written_once_they_are_older_than_the_flush_interval() {
    let (root, _vault, store) = setup();
    let a = add(&store, "synthetic");
    let usage = |at: i64| Usage {
        windows: vec![],
        used_percent: 10.0,
        observed_at: at,
        resets_at: None,
        source: "claude_oauth".into(),
    };
    let t = now() - 100;
    store.observe(&a.id, usage(t)).unwrap();
    store.observe(&a.id, usage(t + 1)).unwrap();
    let writes = store.metadata_writes();
    store.flush_if_older(now(), 900).unwrap();
    assert_eq!(
        store.metadata_writes(),
        writes,
        "pending for less than the interval"
    );
    store.flush_if_older(now() + 901, 900).unwrap();
    assert_eq!(store.metadata_writes(), writes + 1);
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert_eq!(
        disk["snapshot"]["accounts"][0]["usage"]["observed_at"],
        t + 1
    );
}

#[test]
fn an_update_changes_only_what_it_names() {
    // SB-68: `accounts update --label` or `--enabled` alone keeps the other field.
    let (_root, _vault, store) = setup();
    let a = add(&store, "synthetic-secret-alpha");
    let label = |store: &Store| store.snapshot().unwrap().accounts[0].label.clone();
    let enabled = |store: &Store| store.snapshot().unwrap().accounts[0].enabled;
    store
        .update_fields(&a.id, Some("Renamed".into()), None)
        .unwrap();
    assert_eq!((label(&store), enabled(&store)), ("Renamed".into(), true));
    store.update_fields(&a.id, None, Some(false)).unwrap();
    assert_eq!((label(&store), enabled(&store)), ("Renamed".into(), false));
    assert_eq!(
        store.update_fields(&a.id, None, None).unwrap_err(),
        switchboard_core::NOTHING_TO_UPDATE
    );
    assert!(store.update_fields(&a.id, Some(" ".into()), None).is_err());
    assert_eq!(label(&store), "Renamed");
}

/// Fails exactly the `fail_at`-th credential write (1-based); every other call works.
struct NthPutFails {
    memory: MemoryVault,
    puts: std::sync::atomic::AtomicUsize,
    fail_at: usize,
}
impl Vault for NthPutFails {
    fn put(&self, id: &str, c: &Credential) -> Result<(), String> {
        if self.puts.fetch_add(1, Ordering::SeqCst) + 1 == self.fail_at {
            return Err("synthetic-sensitive-error".into());
        }
        self.memory.put(id, c)
    }
    fn get(&self, id: &str) -> Result<Credential, String> {
        self.memory.get(id)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.memory.delete(id)
    }
}

#[test]
fn a_sign_in_renews_every_saved_row_of_its_login_or_none() {
    // SB-62/SB-78: one login saved in two pools; signing in again renews both in one
    // transaction, and a failed write leaves both on the old credential.
    use switchboard_core::ExternalIdentity;
    let oauth = |refresh: &str| {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(
                r#"{{"claudeAiOauth":{{"accessToken":"a-{refresh}","refreshToken":"{refresh}"}}}}"#
            ),
        )
        .unwrap()
    };
    let identity = ExternalIdentity {
        account_id: Some("synthetic-a".into()),
        organization_id: None,
        email: Some("synthetic-a@example.invalid".into()),
    };
    let root = TempDir::new().unwrap();
    // Writes 1 and 2 save the two rows; write 4 is the second row of the renewal.
    let vault = Arc::new(NthPutFails {
        memory: MemoryVault::default(),
        puts: Default::default(),
        fail_at: 4,
    });
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    let save = |pool: &str| {
        store
            .upsert(
                "A".into(),
                Provider::Claude,
                AuthKind::OAuth,
                pool.into(),
                oauth("old"),
                Some(identity.clone()),
            )
            .unwrap()
    };
    let (home, work) = (save("default"), save("work"));
    let rows = vec![
        (home.id.clone(), None),
        (work.id.clone(), Some("Work".into())),
    ];
    let refresh = |id: &str| store.stored_credential(id).unwrap().refresh_token;
    assert!(store
        .renew_rows(Provider::Claude, &rows, &oauth("new"), &identity)
        .is_err());
    assert_eq!(refresh(&home.id).as_deref(), Some("old"), "rolled back");
    assert_eq!(refresh(&work.id).as_deref(), Some("old"));
    assert!(store
        .snapshot()
        .unwrap()
        .accounts
        .iter()
        .all(|a| a.label == "A"));
    // The next attempt goes through: both rows, one renamed.
    let renewed = store
        .renew_rows(Provider::Claude, &rows, &oauth("new"), &identity)
        .unwrap();
    assert_eq!(renewed.len(), 2);
    assert_eq!(refresh(&home.id).as_deref(), Some("new"));
    assert_eq!(refresh(&work.id).as_deref(), Some("new"));
    let labels: Vec<String> = store
        .snapshot()
        .unwrap()
        .accounts
        .iter()
        .map(|a| a.label.clone())
        .collect();
    assert_eq!(labels, vec!["A".to_string(), "Work".to_string()]);
    // Another login's credential is refused for these rows.
    let other = ExternalIdentity {
        account_id: Some("synthetic-b".into()),
        organization_id: None,
        email: None,
    };
    assert!(store
        .renew_rows(Provider::Claude, &rows, &oauth("b"), &other)
        .is_err());
    assert_eq!(refresh(&home.id).as_deref(), Some("new"));
}

#[test]
fn an_agent_key_lives_in_the_vault_and_its_metadata_names_only_the_service() {
    // SB-79: the value never reaches the metadata file; a replacement keeps the vault item.
    let (root, vault, store) = setup();
    assert!(store.agent_key("openrouter").unwrap().is_none());
    assert_eq!(
        store
            .set_agent_key("openrouter", "not-a-key", None)
            .unwrap_err(),
        switchboard_core::agent_keys::KEY_INVALID
    );
    assert_eq!(
        store
            .set_agent_key(
                "openrouter",
                "sk-or-v1-synthetic-one",
                Some("no slash".into())
            )
            .unwrap_err(),
        switchboard_core::agent_keys::MODEL_INVALID
    );
    let saved = store
        .set_agent_key(
            "openrouter",
            " sk-or-v1-synthetic-one\n",
            Some("moonshotai/kimi-k2".into()),
        )
        .unwrap();
    assert_eq!(saved.model.as_deref(), Some("moonshotai/kimi-k2"));
    assert_eq!(
        store.agent_key_value("openrouter").unwrap(),
        "sk-or-v1-synthetic-one"
    );
    let metadata = fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(
        !metadata.contains("sk-or-v1"),
        "no value in the metadata file"
    );
    assert!(metadata.contains("openrouter"));
    // Replaced: same vault item, model kept when none is given.
    let again = store
        .set_agent_key("openrouter", "sk-or-v1-synthetic-two", None)
        .unwrap();
    assert_eq!(again.id, saved.id);
    assert_eq!(again.model.as_deref(), Some("moonshotai/kimi-k2"));
    assert_eq!(
        store.agent_key_value("openrouter").unwrap(),
        "sk-or-v1-synthetic-two"
    );
    store
        .set_agent_key_model("openrouter", "anthropic/claude-sonnet-4.5")
        .unwrap();
    assert_eq!(
        store
            .agent_key("openrouter")
            .unwrap()
            .unwrap()
            .model
            .as_deref(),
        Some("anthropic/claude-sonnet-4.5")
    );
    // Survives a restart of the store.
    drop(store);
    let reopened = reopen(root.path(), vault.clone());
    assert_eq!(
        reopened.agent_key_value("openrouter").unwrap(),
        "sk-or-v1-synthetic-two"
    );
    assert!(reopened.remove_agent_key("openrouter").unwrap());
    assert!(vault.get(&saved.id).is_err(), "the vault item goes too");
    assert!(!reopened.remove_agent_key("openrouter").unwrap());
    assert_eq!(
        reopened.agent_key_value("openrouter").unwrap_err(),
        switchboard_core::agent_keys::NO_AGENT_KEY
    );
}

#[test]
fn a_failed_agent_key_save_leaves_nothing_behind() {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(FaultVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    vault.fail_put.store(true, Ordering::SeqCst);
    assert!(store
        .set_agent_key("openrouter", "sk-or-v1-synthetic", None)
        .is_err());
    assert!(store.agent_key("openrouter").unwrap().is_none());
}

#[test]
fn an_agent_starting_in_a_project_folder_gets_that_projects_key_and_others_the_machines() {
    // SB-144 (operator 2026-10-19): Hermes in project A runs on A's key, in project B on B's,
    // anywhere else on the machine's key; a deleted project takes its key with it.
    let (root, _vault, store) = setup();
    let work = tempfile::tempdir().unwrap();
    let (a, b, plain) = (
        work.path().join("a"),
        work.path().join("b"),
        work.path().join("plain"),
    );
    for d in [&a, &b, &plain] {
        fs::create_dir_all(d).unwrap();
    }
    let alpha = store
        .save_project(None, "Alpha", std::slice::from_ref(&a), &[])
        .unwrap();
    store
        .save_project(None, "Beta", std::slice::from_ref(&b), &[])
        .unwrap();
    assert_eq!(
        store
            .set_agent_key_for("openrouter", Some("nowhere"), "sk-or-v1-x", None)
            .unwrap_err(),
        switchboard_core::agent_keys::KEY_POOL_INVALID
    );
    store
        .set_agent_key("openrouter", "sk-or-v1-machine", None)
        .unwrap();
    store
        .set_agent_key_for(
            "openrouter",
            Some(&alpha.pool),
            "sk-or-v1-alpha",
            Some("moonshotai/kimi-k2".into()),
        )
        .unwrap();
    let value = |dir: &std::path::Path| store.agent_key_value_in("openrouter", Some(dir)).unwrap();
    assert_eq!(value(&a.join("src")), "sk-or-v1-alpha");
    assert_eq!(
        value(&b),
        "sk-or-v1-machine",
        "a project without its own key"
    );
    assert_eq!(value(&plain), "sk-or-v1-machine");
    assert_eq!(
        store.agent_key_value("openrouter").unwrap(),
        "sk-or-v1-machine",
        "the machine's key is unchanged"
    );
    let metadata = fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(
        !metadata.contains("sk-or-v1"),
        "no value in the metadata file"
    );
    store.remove_project(&alpha.pool).unwrap();
    assert!(store
        .agent_key_for("openrouter", Some(&alpha.pool))
        .unwrap()
        .is_none());
    assert_eq!(value(&a), "sk-or-v1-machine");
}

#[test]
fn assigning_one_seat_brings_its_team_and_removals_clean_up_assignments() {
    // SB-158: the person's word on one Claude Team seat records the team on the organization,
    // so another seat of that team follows; removing the organization or the account leaves no
    // dangling assignment.
    use switchboard_core::{By, ExternalIdentity};
    let (root, vault, store) = setup();
    let oauth = |refresh: &str| {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(
                r#"{{"claudeAiOauth":{{"accessToken":"a-{refresh}","refreshToken":"{refresh}"}}}}"#
            ),
        )
        .unwrap()
    };
    let seat = |label: &str, refresh: &str| {
        store
            .upsert(
                label.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                oauth(refresh),
                Some(ExternalIdentity {
                    account_id: Some(format!("synthetic-{refresh}")),
                    organization_id: Some("synthetic-team".into()),
                    email: Some(format!("{refresh}@gmail.invalid")),
                }),
            )
            .unwrap()
    };
    let a = seat("A", "a");
    let b = seat("B", "b");
    let acme = store
        .add_organization(" Acme ", &["Acme.example".into()])
        .unwrap();
    assert_eq!(acme.name, "Acme");
    assert_eq!(acme.domains, vec!["acme.example".to_string()]);
    assert!(
        store.add_organization("acme", &[]).is_err(),
        "names are unique"
    );
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.attribution(&b).organization, None);

    let by_person = store.assign_organization(&a.id, Some(&acme.id)).unwrap();
    assert_eq!(by_person.by, Some(By::Person));
    let snapshot = store.snapshot().unwrap();
    let follows = snapshot.attribution(&b);
    assert_eq!(follows.organization.as_deref(), Some(acme.id.as_str()));
    assert_eq!(follows.by, Some(By::Provider));

    // The organization survives a reopen.
    drop(store);
    let store = reopen(root.path(), vault);
    assert_eq!(store.snapshot().unwrap().organizations.len(), 1);

    store.remove(&b.id).unwrap();
    assert!(!store.snapshot().unwrap().memberships.contains_key(&b.id));
    store.remove_organization(&acme.id).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.memberships.is_empty());
    assert_eq!(snapshot.attribution(&a).organization, None);
}

#[test]
fn a_groups_organization_keeps_its_folders_on_its_own_seats() {
    // SB-109 (SCN-070): with an organization defined, a group takes only accounts of its own
    // context (its organization, or personal), at placement and at run time; without one,
    // nothing is refused.
    use switchboard_core::ExternalIdentity;
    let (root, _vault, store) = setup();
    let oauth = |refresh: &str| {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(
                r#"{{"claudeAiOauth":{{"accessToken":"a-{refresh}","refreshToken":"{refresh}"}}}}"#
            ),
        )
        .unwrap()
    };
    let seat = |email: &str, refresh: &str| {
        store
            .upsert(
                email.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                oauth(refresh),
                Some(ExternalIdentity {
                    account_id: Some(format!("synthetic-{refresh}")),
                    organization_id: None,
                    email: Some(email.into()),
                }),
            )
            .unwrap()
    };
    let work = seat("dev@acme.example", "w");
    let home = seat("me@home.invalid", "h");
    let alpha_dir = root.path().join("alpha");
    let beta_dir = root.path().join("beta");
    std::fs::create_dir_all(&alpha_dir).unwrap();
    std::fs::create_dir_all(&beta_dir).unwrap();
    // No organization: any account joins any group.
    let alpha = store
        .save_project(
            None,
            "Alpha",
            std::slice::from_ref(&alpha_dir),
            std::slice::from_ref(&work.id),
        )
        .unwrap();
    assert_eq!(alpha.organization, None);
    let acme = store
        .add_organization("Acme", &["acme.example".into()])
        .unwrap();
    // The personal group now holds Acme's seat: run time refuses it, until the group is Acme's.
    let snapshot = store.snapshot().unwrap();
    assert!(!snapshot.context_admits(
        &alpha.pool,
        &snapshot
            .accounts
            .iter()
            .find(|a| a.id == work.id)
            .unwrap()
            .clone()
    ));
    assert!(
        snapshot.context_admits("default", &work),
        "default is not a group"
    );
    // Making it Acme's is accepted; making a personal group take Acme's seat is refused.
    let alpha = store
        .set_project_organization(&alpha.pool, Some(&acme.id))
        .unwrap();
    assert_eq!(alpha.organization.as_deref(), Some(acme.id.as_str()));
    let err = store
        .save_project(
            None,
            "Beta",
            std::slice::from_ref(&beta_dir),
            &[work.id.clone(), home.id.clone()],
        )
        .unwrap_err();
    assert!(err.contains("belongs to another organization"), "{err}");
    // Acme's group refuses the personal seat; saving keeps the group's organization.
    let err = store
        .save_project(
            Some(&alpha.pool),
            "Alpha",
            std::slice::from_ref(&alpha_dir),
            &[work.id.clone(), home.id.clone()],
        )
        .unwrap_err();
    assert!(err.contains("belongs to another organization"), "{err}");
    let kept = store
        .save_project(
            Some(&alpha.pool),
            "Alpha",
            std::slice::from_ref(&alpha_dir),
            std::slice::from_ref(&work.id),
        )
        .unwrap();
    assert_eq!(kept.organization.as_deref(), Some(acme.id.as_str()));
    // A personal group with a personal seat; turning it into Acme's is refused while it holds it.
    let beta = store
        .save_project(
            None,
            "Beta",
            std::slice::from_ref(&beta_dir),
            std::slice::from_ref(&home.id),
        )
        .unwrap();
    let err = store
        .set_project_organization(&beta.pool, Some(&acme.id))
        .unwrap_err();
    assert!(err.contains("elsewhere. Move"), "{err}");
    assert!(store
        .set_project_organization(&beta.pool, Some("no-such"))
        .is_err());
    // Selection and the route refuse a seat that moved to another context after placement.
    store
        .select(Provider::Claude, &alpha.pool, &work.id)
        .unwrap();
    store
        .assign_organization(&work.id, Some("personal"))
        .unwrap();
    assert!(store
        .route(Provider::Claude, &alpha.pool)
        .err()
        .unwrap()
        .contains("belongs to another organization"));
    assert!(store
        .select(Provider::Claude, &alpha.pool, &work.id)
        .is_err());
    store.assign_organization(&work.id, None).unwrap();
    assert!(store.route(Provider::Claude, &alpha.pool).is_ok());
    // Removing the organization turns its groups personal.
    store.remove_organization(&acme.id).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot.project_of_pool(&alpha.pool).unwrap().organization,
        None
    );
}

#[test]
fn a_folder_in_groups_of_two_contexts_runs_in_the_personal_one() {
    // SB-109 (SCN-070, RM-28 CX-26): the more private group wins for a shared folder.
    let (root, _vault, store) = setup();
    let shared = root.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let acme = store.add_organization("Acme", &[]).unwrap();
    let first = store
        .save_project(None, "Acme work", std::slice::from_ref(&shared), &[])
        .unwrap();
    store
        .set_project_organization(&first.pool, Some(&acme.id))
        .unwrap();
    let second = store
        .save_project(None, "Zed personal", std::slice::from_ref(&shared), &[])
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.project_for(&shared).unwrap().pool, second.pool);
}

#[test]
fn organizations_from_passioncode_are_linked_kept_current_and_released() {
    // SB-159 (SCN-071): a typed organization of the same name or a shared domain is linked, not
    // duplicated; a new one is added; a rename and a new domain arrive; one the person left
    // stays local, unmarked; a secret lives only in the vault.
    use switchboard_core::organizations::VestaOrganization;
    let (_root, vault, store) = setup();
    let typed = store
        .add_organization("acme", &["acme.example".into()])
        .unwrap();
    let vesta = |id: &str, name: &str, domains: &[&str]| VestaOrganization {
        id: id.into(),
        name: name.into(),
        domains: domains.iter().map(|d| (*d).into()).collect(),
    };
    let applied = store
        .apply_vesta(&[
            vesta("v-acme", "Acme Inc", &["acme.example", "acme.dev"]),
            vesta("v-beta", "Beta", &[]),
        ])
        .unwrap();
    assert_eq!((applied.linked, applied.added), (1, 1));
    let snap = store.snapshot().unwrap();
    let acme = snap.organization(&typed.id).unwrap();
    assert_eq!(acme.vesta_id.as_deref(), Some("v-acme"));
    assert_eq!(acme.name, "Acme Inc");
    assert_eq!(
        acme.domains,
        vec!["acme.example".to_string(), "acme.dev".to_string()]
    );
    assert!(snap
        .organizations
        .iter()
        .any(|o| o.name == "Beta" && o.vesta_id.as_deref() == Some("v-beta")));
    // The person left Beta: it stays, no longer linked.
    let applied = store
        .apply_vesta(&[vesta("v-acme", "Acme", &["acme.example"])])
        .unwrap();
    assert_eq!(applied.released, 1);
    let snap = store.snapshot().unwrap();
    assert_eq!(snap.organization(&typed.id).unwrap().name, "Acme");
    assert!(snap
        .organizations
        .iter()
        .any(|o| o.name == "Beta" && o.vesta_id.is_none()));
    // Domains add up: one Vesta no longer lists stays until the person edits it.
    assert_eq!(
        snap.organization(&typed.id).unwrap().domains,
        vec!["acme.example".to_string(), "acme.dev".to_string()]
    );
    // What `validate` would refuse never gets in: a long name differing only in non-ASCII case
    // links to the one typed here; another Vesta organization of a linked one's name gets a
    // suffixed name that still fits 80 characters; the same domain twice is kept once.
    let long: String = "Ж".repeat(80);
    let typed_long = store.add_organization(&long, &[]).unwrap();
    let applied = store
        .apply_vesta(&[
            vesta("v-acme", "Acme", &["acme.example"]),
            vesta(
                "v-long",
                &long.to_lowercase(),
                &["dup.example", "DUP.example"],
            ),
            vesta("v-long-2", &long, &[]),
        ])
        .unwrap();
    assert_eq!((applied.linked, applied.added), (1, 1));
    let snap = store.snapshot().unwrap();
    let linked = snap.organization(&typed_long.id).unwrap();
    assert_eq!(linked.vesta_id.as_deref(), Some("v-long"));
    assert_eq!(linked.domains, vec!["dup.example".to_string()]);
    let added = snap
        .organizations
        .iter()
        .find(|o| o.vesta_id.as_deref() == Some("v-long-2"))
        .unwrap();
    assert!(
        added.name.chars().count() <= 80 && added.name.ends_with("(v-long-2)"),
        "{}",
        added.name
    );
    store.release_vesta().unwrap();
    assert!(store
        .snapshot()
        .unwrap()
        .organizations
        .iter()
        .all(|o| o.vesta_id.is_none()));
    // A secret: in the vault under its item, never in the metadata.
    let item = store.put_secret(None, "vapr_synthetic_refresh").unwrap();
    assert_eq!(store.secret(&item).unwrap(), "vapr_synthetic_refresh");
    let item2 = store
        .put_secret(Some(&item), "vapr_synthetic_next")
        .unwrap();
    assert_eq!(item, item2);
    assert_eq!(store.secret(&item).unwrap(), "vapr_synthetic_next");
    store.delete_secret(&item).unwrap();
    assert!(store.secret(&item).is_err());
    let _ = vault;
}

#[test]
fn an_account_used_on_another_machine_is_never_chosen_here() {
    // SB-175 (SB-168 D-2): a renewal here would spend the other machine's sign-in; marking the
    // account disables it, clears its selection and refuses a switch with its own reason.
    let (root, vault, store) = setup();
    let a = add(&store, "synthetic-secret-alpha");
    store.select(Provider::Claude, "default", &a.id).unwrap();
    store.mark_elsewhere(&a.id, true, 1_000).unwrap();
    let snap = store.snapshot().unwrap();
    assert!(!snap.accounts[0].enabled);
    assert_eq!(snap.elsewhere.get(&a.id), Some(&1_000));
    assert!(snap.routes.is_empty(), "its selection is cleared");
    assert_eq!(
        store
            .select(Provider::Claude, "default", &a.id)
            .unwrap_err(),
        switchboard_core::ELSEWHERE
    );
    // The mark survives a restart, under schema version 8.
    drop(store);
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert_eq!(disk["schema_version"], 8);
    let store = reopen(root.path(), vault);
    assert!(store.snapshot().unwrap().elsewhere.contains_key(&a.id));
    // Enabling it says it is used here: the mark goes.
    store.update_fields(&a.id, None, Some(true)).unwrap();
    let snap = store.snapshot().unwrap();
    assert!(snap.accounts[0].enabled && snap.elsewhere.is_empty());
    store.mark_elsewhere(&a.id, true, 2_000).unwrap();
    store.mark_elsewhere(&a.id, false, 2_001).unwrap();
    let snap = store.snapshot().unwrap();
    assert!(snap.accounts[0].enabled && snap.elsewhere.is_empty());
    assert!(store.mark_elsewhere("missing", true, 1).is_err());
}

/// A vault whose reads wait until released, like a Keychain read held by `/usr/bin/security`.
struct HeldVault {
    inner: MemoryVault,
    hold: std::sync::Mutex<bool>,
    released: std::sync::Condvar,
    reading: std::sync::mpsc::SyncSender<()>,
}
impl switchboard_core::Vault for HeldVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        let _ = self.reading.try_send(());
        let mut held = self.hold.lock().unwrap();
        while *held {
            held = self.released.wait(held).unwrap();
        }
        drop(held);
        self.inner.get(id)
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        self.inner.put(id, value)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.inner.delete(id)
    }
}

#[test]
fn a_slow_credential_read_does_not_hold_the_metadata() {
    let root = TempDir::new().unwrap();
    let (reading, read_started) = std::sync::mpsc::sync_channel(1);
    let vault = Arc::new(HeldVault {
        inner: MemoryVault::default(),
        hold: std::sync::Mutex::new(false),
        released: std::sync::Condvar::new(),
        reading,
    });
    let store = Arc::new(Store::open(root.path().into(), vault.clone()).unwrap());
    let account = add(&store, "synthetic-held");
    *vault.hold.lock().unwrap() = true;
    while read_started.try_recv().is_ok() {}
    let reader = {
        let store = store.clone();
        let id = account.id.clone();
        std::thread::spawn(move || store.stored_credential(&id))
    };
    read_started
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    // The read is in the vault now. Metadata answers at once (SB-185); before, this waited for it.
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let store = store.clone();
        std::thread::spawn(move || {
            let _ = tx.send(store.snapshot().map(|s| s.accounts.len()));
        });
    }
    let answered = rx.recv_timeout(std::time::Duration::from_secs(2));
    *vault.hold.lock().unwrap() = false;
    vault.released.notify_all();
    assert_eq!(answered.expect("snapshot waited for a vault read"), Ok(1));
    assert!(reader.join().unwrap().is_ok());
    let _ = store.take_slowest_read();
    assert_eq!(store.take_slowest_read(), 0);
}

/// PRD-01 (iteration 2026-10-24): `credential_changes` moves on a credential stored or removed
/// and never on a project save, which `changes` counts too.
#[test]
fn only_credential_writes_move_the_credential_count() {
    let (_root, _vault, store) = setup();
    let start = store.credential_changes();
    let account = add(&store, "synthetic-counted");
    let added = store.credential_changes();
    assert!(added > start, "a stored credential is counted");
    let work = TempDir::new().unwrap();
    let folder = work.path().canonicalize().unwrap();
    let changes = store.changes();
    store
        .save_project(None, "Synthetic group", &[folder], &[])
        .unwrap();
    assert!(
        store.changes() > changes,
        "a project save is a backup change"
    );
    assert_eq!(
        store.credential_changes(),
        added,
        "and not a credential write"
    );
    store.remove(&account.id).unwrap();
    assert!(
        store.credential_changes() > added,
        "a removed credential is counted"
    );
}

/// PRD-17 (iteration 2026-10-24): 15 quota checks in 48 h were logged `usage_check
/// outcome=other`: a check whose answer the store refused to keep (older than what it holds,
/// a credential renewed while it ran) carried the store's text and no code. Each refusal of
/// `observe_credential` now has one.
#[test]
fn each_refusal_of_a_usage_observation_has_a_log_code() {
    let (_root, _vault, store) = setup();
    let credential = Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        r#"{"claudeAiOauth":{"accessToken":"synthetic","refreshToken":"synthetic-refresh","expiresAt":2000000000000}}"#,
    )
    .unwrap();
    let a = store
        .add(
            "Synthetic".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential.clone(),
        )
        .unwrap();
    let usage = |at: i64, percent: f64| Usage {
        windows: vec![],
        used_percent: percent,
        observed_at: at,
        resets_at: Some(at + 7200),
        source: "claude_oauth".into(),
    };
    let code = |e: String| Store::usage_refusal_code(&e);
    store
        .observe_credential(&a.id, &credential, usage(now(), 10.0))
        .unwrap();
    let older = store
        .observe_credential(&a.id, &credential, usage(now() - 60, 10.0))
        .unwrap_err();
    assert_eq!(code(older), Some("superseded"));
    let invalid = store
        .observe_credential(&a.id, &credential, usage(now(), 200.0))
        .unwrap_err();
    assert_eq!(code(invalid), Some("invalid_observation"));
    let mut renewed = credential.clone();
    renewed.access_token = "synthetic-other".into();
    let changed = store
        .observe_credential(&a.id, &renewed, usage(now() + 1, 10.0))
        .unwrap_err();
    assert_eq!(code(changed), Some("credential_changed"));
    let gone = store
        .observe_credential(
            "00000000-0000-4000-8000-000000000000",
            &credential,
            usage(now() + 2, 10.0),
        )
        .unwrap_err();
    assert_eq!(code(gone), Some("account_gone"));
    assert_eq!(
        Store::usage_refusal_code("Credential storage unavailable"),
        Some("vault")
    );
    assert_eq!(
        Store::usage_refusal_code("Account store unavailable"),
        Some("store")
    );
    assert_eq!(Store::usage_refusal_code("anything else"), None);
}

/// N1 round 1 (unit tier): a quota check's answer and its failure are stored only if the
/// credential the provider answered is still the stored one, and that was read from the vault
/// under the metadata lock: every background check could stall `snapshot()` behind a slow
/// Keychain read, as SB-185 found for `stored_credential`.
#[test]
fn a_quota_checks_generation_read_does_not_hold_the_metadata() {
    for path in ["observe", "health"] {
        let root = TempDir::new().unwrap();
        let (reading, read_started) = std::sync::mpsc::sync_channel(1);
        let vault = Arc::new(HeldVault {
            inner: MemoryVault::default(),
            hold: std::sync::Mutex::new(false),
            released: std::sync::Condvar::new(),
            reading,
        });
        let store = Arc::new(Store::open(root.path().into(), vault.clone()).unwrap());
        let credential = Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            r#"{"claudeAiOauth":{"accessToken":"synthetic","refreshToken":"synthetic-refresh","expiresAt":2000000000000}}"#,
        )
        .unwrap();
        let account = store
            .add(
                "Synthetic".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                credential.clone(),
            )
            .unwrap();
        *vault.hold.lock().unwrap() = true;
        while read_started.try_recv().is_ok() {}
        let writer = {
            let store = store.clone();
            let id = account.id.clone();
            let credential = credential.clone();
            std::thread::spawn(move || {
                let at = now();
                if path == "observe" {
                    store.observe_credential(
                        &id,
                        &credential,
                        Usage {
                            windows: vec![],
                            used_percent: 10.0,
                            observed_at: at,
                            resets_at: Some(at + 7200),
                            source: "claude_oauth".into(),
                        },
                    )
                } else {
                    store.usage_health_credential(&id, &credential, "failed", at, at + 600)
                }
            })
        };
        read_started
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let store = store.clone();
            std::thread::spawn(move || {
                let _ = tx.send(store.snapshot().map(|s| s.accounts.len()));
            });
        }
        let answered = rx.recv_timeout(std::time::Duration::from_secs(2));
        *vault.hold.lock().unwrap() = false;
        vault.released.notify_all();
        assert_eq!(answered.expect(path), Ok(1), "{path}");
        assert_eq!(writer.join().unwrap(), Ok(()), "{path}");
    }
}

/// N1 round 1: a credential written between the check's vault read (now outside the lock) and
/// the store's lock is still seen: the count of credential writes moved, so the store reads the
/// credential again and refuses the answer as another credential's.
struct RacingVault {
    inner: MemoryVault,
    store: std::sync::OnceLock<std::sync::Weak<Store>>,
    replacement: std::sync::Mutex<Option<Credential>>,
}
impl switchboard_core::Vault for RacingVault {
    fn get(&self, id: &str) -> Result<Credential, String> {
        let read = self.inner.get(id);
        let replacement = self.replacement.lock().unwrap().take();
        if let Some(replacement) = replacement {
            // Another writer replaces the credential right after this read returned.
            self.inner.delete(id).unwrap();
            let store = self.store.get().unwrap().upgrade().unwrap();
            assert!(store.restore_missing_credential(id, &replacement).unwrap());
        }
        read
    }
    fn put(&self, id: &str, value: &Credential) -> Result<(), String> {
        self.inner.put(id, value)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.inner.delete(id)
    }
}
#[test]
fn a_credential_written_after_the_generation_read_is_still_seen() {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(RacingVault {
        inner: MemoryVault::default(),
        store: Default::default(),
        replacement: Default::default(),
    });
    let store = Arc::new(Store::open(root.path().into(), vault.clone()).unwrap());
    vault.store.set(Arc::downgrade(&store)).unwrap();
    let parse = |token: &str| {
        Credential::parse(
            Provider::Claude,
            AuthKind::OAuth,
            &format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}","refreshToken":"r","expiresAt":2000000000000}}}}"#),
        )
        .unwrap()
    };
    let account = store
        .add(
            "Synthetic".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            parse("old"),
        )
        .unwrap();
    *vault.replacement.lock().unwrap() = Some(parse("new"));
    let at = now();
    let refused = store
        .usage_health_credential(&account.id, &parse("old"), "failed", at, at + 600)
        .unwrap_err();
    assert_eq!(
        Store::usage_refusal_code(&refused),
        Some("credential_changed"),
        "{refused}"
    );
}
