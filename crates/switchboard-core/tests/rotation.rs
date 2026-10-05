use std::sync::Arc;
use switchboard_core::{
    AuthKind, Credential, ExternalIdentity, MemoryVault, Provider, RotationPolicy, Store, Usage,
    UsageWindow, Vault,
};
use tempfile::TempDir;
fn setup() -> (TempDir, Arc<MemoryVault>, Store) {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    (root, vault, store)
}
fn clock() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 1000
}
fn identity(org: &str) -> ExternalIdentity {
    ExternalIdentity {
        account_id: Some("synthetic-user".into()),
        organization_id: Some(org.into()),
        email: Some("synthetic@example.invalid".into()),
    }
}
fn credential(secret: &str, expires_at: i64) -> Credential {
    Credential {
        access_token: secret.into(),
        refresh_token: Some("synthetic-refresh".into()),
        id_token: None,
        native_context: Some(
            serde_json::json!({"auth":{"claudeAiOauth":{"accessToken":secret}},"oauth_account":{"accountUuid":"synthetic-user"}}),
        ),
        expires_at: Some(expires_at),
        account_id: Some("synthetic-user".into()),
    }
}
fn account(store: &Store, org: &str, now: i64) -> String {
    store
        .upsert(
            org.into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential(org, now + 9000),
            Some(identity(org)),
        )
        .unwrap()
        .id
}
fn observe(store: &Store, id: &str, percent: f64, now: i64) {
    store
        .observe(
            id,
            Usage {
                used_percent: percent,
                observed_at: now,
                resets_at: Some(now + 3600),
                source: "claude_oauth".into(),
                windows: vec![UsageWindow {
                    name: "five_hour".into(),
                    used_percent: percent,
                    resets_at: Some(now + 3600),
                }],
            },
        )
        .unwrap();
}
fn policy() -> RotationPolicy {
    RotationPolicy {
        provider: Provider::Claude,
        pool: "default".into(),
        target: "managed".into(),
        enabled: true,
        threshold_percent: 90.,
        hysteresis_percent: 10.,
        cooldown_seconds: 1800,
        max_age_seconds: 300,
        last_switched_at: None,
    }
}
#[test]
fn upsert_identity_includes_org_preserves_disabled_and_rolls_back_vault_on_disk_failure() {
    let (root, vault, store) = setup();
    let now = clock();
    let first = account(&store, "org-a", now);
    let second = account(&store, "org-b", now);
    assert_ne!(first, second);
    store.update(&first, "Off".into(), false).unwrap();
    let updated = store
        .upsert(
            "New".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("new-generation", now + 9000),
            Some(identity("org-a")),
        )
        .unwrap();
    assert_eq!(updated.id, first);
    assert!(!updated.enabled);
    assert_eq!(
        store
            .match_external(Provider::Claude, "default", &identity("org-b"))
            .unwrap()
            .unwrap()
            .id,
        second
    );
    assert!(store
        .match_external(Provider::Claude, "other", &identity("org-b"))
        .unwrap()
        .is_none());
    assert_eq!(vault.get(&first).unwrap().access_token, "new-generation");
    std::fs::remove_file(root.path().join("accounts.json")).unwrap();
    std::fs::create_dir(root.path().join("accounts.json")).unwrap();
    assert!(store
        .upsert(
            "Failure".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("rollback-generation", now + 9000),
            Some(identity("org-a"))
        )
        .is_err());
    assert_eq!(vault.get(&first).unwrap().access_token, "new-generation");
    assert_eq!(store.snapshot().unwrap().accounts.len(), 2);
}
#[test]
fn same_token_different_known_org_never_merges_and_absent_identity_falls_back() {
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = store
        .upsert(
            "A".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("same", now + 9000),
            Some(identity("org-a")),
        )
        .unwrap();
    let b = store
        .upsert(
            "B".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("same", now + 9000),
            Some(identity("org-b")),
        )
        .unwrap();
    assert_ne!(a.id, b.id);
    // Token-only capture cannot arbitrarily choose between organizations.
    assert!(store
        .upsert(
            "Unknown".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("same", now + 9000),
            None
        )
        .is_err());
    let c = store
        .upsert(
            "C".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "work".into(),
            credential("unique", now + 9000),
            None,
        )
        .unwrap();
    let d = store
        .upsert(
            "D".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "work".into(),
            credential("unique", now + 9000),
            Some(identity("org-c")),
        )
        .unwrap();
    assert_eq!(c.id, d.id);
    let e = store
        .upsert(
            "E".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "work".into(),
            credential("unique", now + 9000),
            None,
        )
        .unwrap();
    assert_eq!(e.external_identity.unwrap(), identity("org-c"));
}
#[test]
fn native_context_never_enters_metadata_and_expired_storage_is_backend_only() {
    let (root, _vault, store) = setup();
    let now = clock();
    let a = store
        .upsert(
            "A".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("synthetic-private-marker", now - 1),
            Some(identity("org-a")),
        )
        .unwrap();
    assert!(store.credential(&a.id).is_err());
    assert_eq!(
        store.stored_credential(&a.id).unwrap().access_token,
        "synthetic-private-marker"
    );
    let disk = std::fs::read_to_string(root.path().join("accounts.json")).unwrap();
    assert!(!disk.contains("synthetic-private-marker"));
    assert!(!disk.contains("native_context"));
    let mut invalid = credential("synthetic", now + 9000);
    invalid.native_context = Some(serde_json::json!({"auth":{},"oauth_account":{},"unknown":1}));
    assert!(store
        .upsert(
            "A".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            invalid,
            Some(identity("org-a"))
        )
        .is_err());
}
#[test]
fn rotation_threshold_hysteresis_cooldown_and_deterministic_candidate() {
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = account(&store, "org-a", now);
    let b = account(&store, "org-b", now);
    let c = account(&store, "org-c", now);
    observe(&store, &a, 90., now);
    observe(&store, &b, 80., now);
    observe(&store, &c, 81., now);
    let p = policy();
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "no_eligible_account"
    );
    observe(&store, &b, 79.9, now);
    observe(&store, &c, 10., now);
    assert_eq!(
        store
            .rotation_decision(&p, Some(&a), now)
            .unwrap()
            .candidate_id
            .as_deref(),
        Some(c.as_str())
    );
    observe(&store, &a, 89.9, now);
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "below_threshold"
    );
    observe(&store, &a, 99., now);
    let mut p = p;
    p.last_switched_at = Some(now - 1799);
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "cooldown"
    );
    p.last_switched_at = Some(now - 1800);
    assert!(store
        .rotation_decision(&p, Some(&a), now)
        .unwrap()
        .candidate_id
        .is_some());
    p.enabled = false;
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "disabled"
    );
}
#[test]
fn stale_failed_disabled_expired_reset_and_native_ineligible_accounts_hold() {
    let (_root, vault, store) = setup();
    let now = clock();
    let a = account(&store, "org-a", now);
    let b = account(&store, "org-b", now);
    let p = policy();
    observe(&store, &a, 99., now);
    observe(&store, &b, 10., now - 301);
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "no_eligible_account"
    );
    observe(&store, &b, 10., now);
    store.usage_health(&b, "failed", now, now + 180).unwrap();
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "no_eligible_account"
    );
    assert_eq!(
        store.snapshot().unwrap().accounts[1]
            .usage
            .as_ref()
            .unwrap()
            .used_percent,
        10.
    );
    store.usage_health(&b, "ok", now, now + 180).unwrap();
    store.update(&b, "B".into(), false).unwrap();
    assert!(store
        .rotation_decision(&p, Some(&a), now)
        .unwrap()
        .candidate_id
        .is_none());
    store.update(&b, "B".into(), true).unwrap();
    vault.put(&b, &credential("org-b", now)).unwrap();
    assert!(store
        .rotation_decision(&p, Some(&a), now)
        .unwrap()
        .candidate_id
        .is_none());
    vault.put(&b, &credential("org-b", now + 9000)).unwrap();
    assert!(store
        .rotation_decision(&p, Some(&a), now)
        .unwrap()
        .candidate_id
        .is_some());
    let mut native = p.clone();
    native.target = "claude_cli".into();
    let mut c = credential("org-b", now + 9000);
    c.native_context = None;
    vault.put(&b, &c).unwrap();
    assert!(store
        .rotation_decision(&native, Some(&a), now)
        .unwrap()
        .candidate_id
        .is_none());
    vault.put(&a, &credential("org-a", now)).unwrap();
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "current_unavailable"
    );
    vault.put(&a, &credential("org-a", now + 9000)).unwrap();
    store.usage_health(&a, "failed", now, now + 180).unwrap();
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "usage_unavailable"
    );
    assert!(store
        .usage_health(&a, "provider-secret-error", now, now + 180)
        .is_err());
    observe(&store, &a, 99., now);
    observe(&store, &b, 10., now);
    let mut long = p.clone();
    long.max_age_seconds = 9000;
    assert_eq!(
        store
            .rotation_decision(&long, Some(&a), now + 3600)
            .unwrap()
            .reason,
        "usage_unavailable"
    );
}
#[test]
fn policy_history_is_runtime_owned_persisted_and_validated() {
    let (root, vault, store) = setup();
    let now = clock();
    let mut p = policy();
    p.last_switched_at = Some(now);
    store.set_policy(p.clone()).unwrap();
    assert!(store.snapshot().unwrap().policies[0]
        .last_switched_at
        .is_none());
    store
        .mark_rotated(Provider::Claude, "default", "managed", now)
        .unwrap();
    p.last_switched_at = None;
    store.set_policy(p.clone()).unwrap();
    assert_eq!(
        store.snapshot().unwrap().policies[0].last_switched_at,
        Some(now)
    );
    assert!(store
        .mark_rotated(Provider::Claude, "default", "managed", now - 1)
        .is_err());
    p.target = "unknown".into();
    assert!(store.set_policy(p.clone()).is_err());
    p.target = "claude_cli".into();
    p.provider = Provider::Codex;
    assert!(store.set_policy(p.clone()).is_err());
    p = policy();
    p.hysteresis_percent = p.threshold_percent;
    assert!(store.set_policy(p).is_err());
    drop(store);
    let store = Store::open(root.path().into(), vault).unwrap();
    assert_eq!(
        store.snapshot().unwrap().policies[0].last_switched_at,
        Some(now)
    );
}
#[test]
fn v1_defaults_migrate_on_write_and_unknown_fields_fail_closed() {
    let (root, vault, store) = setup();
    let now = clock();
    account(&store, "org-a", now);
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    disk["schema_version"] = 1.into();
    disk["snapshot"].as_object_mut().unwrap().remove("policies");
    let a = &mut disk["snapshot"]["accounts"][0];
    a.as_object_mut().unwrap().remove("external_identity");
    a.as_object_mut().unwrap().remove("usage_health");
    std::fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.policies.is_empty());
    assert!(snapshot.accounts[0].external_identity.is_none());
    store.set_policy(policy()).unwrap();
    drop(store);
    let mut disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk["schema_version"], 2);
    disk["snapshot"]["unknown_new_field"] = true.into();
    std::fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(Store::open(root.path().into(), vault).is_err());
}
#[test]
fn recapturing_same_generation_preserves_quota_and_late_old_response_cannot_overwrite_new_generation(
) {
    let (_root, _vault, store) = setup();
    let now = clock();
    let id = account(&store, "org-a", now);
    let old = store.stored_credential(&id).unwrap();
    observe(&store, &id, 42., now);
    store
        .upsert(
            "Same".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            old.clone(),
            Some(identity("org-a")),
        )
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().accounts[0]
            .usage
            .as_ref()
            .unwrap()
            .used_percent,
        42.
    );
    store
        .upsert(
            "New".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            credential("new", now + 9000),
            Some(identity("org-a")),
        )
        .unwrap();
    assert!(store.snapshot().unwrap().accounts[0].usage.is_none());
    let usage = Usage {
        used_percent: 99.,
        observed_at: now,
        windows: vec![],
        resets_at: None,
        source: "claude_oauth".into(),
    };
    assert!(store.observe_credential(&id, &old, usage.clone()).is_err());
    assert!(store
        .usage_health_credential(&id, &old, "failed", now, now + 1800)
        .is_err());
    assert!(store.snapshot().unwrap().accounts[0].usage_health.is_none());
    let current = store.stored_credential(&id).unwrap();
    store
        .usage_health_credential(&id, &current, "failed", now, now + 180)
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().accounts[0]
            .usage_health
            .as_ref()
            .unwrap()
            .status,
        "failed"
    );
    store.observe_credential(&id, &current, usage).unwrap();
    assert_eq!(
        store.snapshot().unwrap().accounts[0]
            .usage
            .as_ref()
            .unwrap()
            .used_percent,
        99.
    );
}

#[test]
fn native_target_excludes_api_keys_and_scope_crossings_even_with_better_quota() {
    let (_root, _vault, store) = setup();
    let now = clock();
    let current = account(&store, "org-current", now);
    observe(&store, &current, 99., now);
    let api = store
        .add(
            "API".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            Credential::parse(Provider::Claude, AuthKind::ApiKey, "synthetic-api").unwrap(),
        )
        .unwrap();
    observe(&store, &api.id, 1., now);
    let other_pool = store
        .upsert(
            "Elsewhere".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "work".into(),
            credential("synthetic-other", now + 9000),
            Some(identity("org-other")),
        )
        .unwrap();
    observe(&store, &other_pool.id, 0., now);
    let mut p = policy();
    assert_eq!(
        store
            .rotation_decision(&p, Some(&current), now)
            .unwrap()
            .candidate_id,
        Some(api.id)
    );
    p.target = "claude_cli".into();
    assert_eq!(
        store
            .rotation_decision(&p, Some(&current), now)
            .unwrap()
            .reason,
        "no_eligible_account"
    );
    assert_eq!(
        store
            .rotation_decision(&p, Some(&other_pool.id), now)
            .unwrap()
            .reason,
        "current_unavailable"
    );
    let value = serde_json::to_value(&p).unwrap();
    let mut value = value.as_object().unwrap().clone();
    value.remove("enabled");
    assert!(
        !serde_json::from_value::<RotationPolicy>(value.into())
            .unwrap()
            .enabled
    );
}

#[test]
fn managed_route_and_cooldown_publish_together_or_neither() {
    let (root, vault, store) = setup();
    let time = clock();
    let a = account(&store, "org-a", time);
    let b = account(&store, "org-b", time);
    // No policy: selecting remains valid and does not invent settings.
    store
        .select_with_cooldown(Provider::Claude, "default", &a, time)
        .unwrap();
    assert!(store.snapshot().unwrap().policies.is_empty());
    store.set_policy(policy()).unwrap();
    store
        .select_with_cooldown(Provider::Claude, "default", &a, time)
        .unwrap();
    let before = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    let metadata = root.path().join("accounts.json");
    let backup = root.path().join("before.json");
    std::fs::rename(&metadata, &backup).unwrap();
    std::fs::create_dir(&metadata).unwrap();
    assert!(store
        .select_with_cooldown(Provider::Claude, "default", &b, time + 1)
        .is_err());
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        before
    );
    std::fs::remove_dir(&metadata).unwrap();
    std::fs::rename(&backup, &metadata).unwrap();
    drop(store);
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        before
    );
    // Credential validation fails before either field is changed, too.
    vault.delete(&b).unwrap();
    assert!(store
        .select_with_cooldown(Provider::Claude, "default", &b, time + 1)
        .is_err());
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        before
    );
    vault.put(&b, &credential("org-b", time + 9000)).unwrap();
    assert!(store
        .select_with_cooldown(Provider::Claude, "default", &b, time - 1)
        .is_err());
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        before
    );
    store
        .select_with_cooldown(Provider::Claude, "default", &b, time + 1)
        .unwrap();
    drop(store);
    let store = Store::open(root.path().into(), vault).unwrap();
    let after = store.snapshot().unwrap();
    assert_eq!(after.routes["claude:default"], b);
    assert_eq!(after.policies[0].last_switched_at, Some(time + 1));
    assert_eq!(
        after.events.len(),
        before["events"].as_array().unwrap().len() + 1
    );
    assert_eq!(after.events.last().unwrap().action, "account_selected");
}

#[test]
fn one_native_cli_target_cannot_have_competing_enabled_pools_even_on_disk() {
    let (root, vault, store) = setup();
    let mut first = policy();
    first.target = "claude_cli".into();
    store.set_policy(first.clone()).unwrap();
    let mut second = first.clone();
    second.pool = "work".into();
    assert_eq!(
        store.set_policy(second.clone()).unwrap_err(),
        "Disable the existing Claude CLI rotation policy before enabling another pool."
    );
    // Disabled settings can coexist; explicitly handing ownership to another pool works.
    second.enabled = false;
    store.set_policy(second.clone()).unwrap();
    first.enabled = false;
    store.set_policy(first.clone()).unwrap();
    second.enabled = true;
    store.set_policy(second.clone()).unwrap();
    // Managed policy is a separate target, not a competing native policy.
    store.set_policy(policy()).unwrap();
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    disk["snapshot"]["policies"][0]["enabled"] = true.into();
    std::fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(Store::open(root.path().into(), vault).is_err());
}

#[test]
fn partial_headers_never_freshen_an_unobserved_low_quota_window() {
    // Both a missing weekly window and a missing model-specific window must keep
    // their original freshness, even when inference keeps reporting other windows.
    for omitted in ["seven_day", "seven_day_opus"] {
        let (_root, _vault, store) = setup();
        let time = clock();
        let current = account(&store, "org-current", time);
        let candidate = account(&store, "org-candidate", time);
        let window = |name: &str| UsageWindow {
            name: name.into(),
            used_percent: 10.,
            resets_at: Some(time + 7200),
        };
        let mut original = vec![window("five_hour"), window("seven_day")];
        if omitted == "seven_day_opus" {
            original.push(window(omitted));
        }
        store
            .observe(
                &candidate,
                Usage {
                    used_percent: 10.,
                    observed_at: time,
                    resets_at: Some(time + 7200),
                    source: "claude_oauth".into(),
                    windows: original,
                },
            )
            .unwrap();
        for elapsed in [301, 302] {
            let at = time + elapsed;
            observe(&store, &current, 99., at);
            let mut reported = vec![window("five_hour")];
            if omitted == "seven_day_opus" {
                reported.push(window("seven_day"));
            }
            store
                .observe(
                    &candidate,
                    Usage {
                        used_percent: 10.,
                        observed_at: at,
                        resets_at: Some(time + 7200),
                        source: "response_headers".into(),
                        windows: reported,
                    },
                )
                .unwrap();
            assert_eq!(
                store
                    .rotation_decision(&policy(), Some(&current), at)
                    .unwrap()
                    .reason,
                "no_eligible_account",
                "omitted {omitted} at {elapsed}"
            );
            let saved = store
                .snapshot()
                .unwrap()
                .accounts
                .into_iter()
                .find(|account| account.id == candidate)
                .unwrap();
            assert_eq!(saved.usage.unwrap().observed_at, time);
            assert_eq!(saved.usage_health.unwrap().checked_at, at);
        }
        // A failed check must not erase ordering of the preceding successful
        // response, whose aggregate age still includes the historical window.
        store
            .usage_health(&candidate, "failed", time + 303, time + 483)
            .unwrap();
        // The conservative aggregate age is not the latest response time:
        // a delayed response must still not overwrite the newer header values.
        assert!(store
            .observe(
                &candidate,
                Usage {
                    used_percent: 0.,
                    observed_at: time + 301,
                    resets_at: Some(time + 7200),
                    source: "response_headers".into(),
                    windows: vec![UsageWindow {
                        name: "five_hour".into(),
                        used_percent: 0.,
                        resets_at: Some(time + 7200),
                    }],
                },
            )
            .is_err());
        // An authoritative endpoint response replaces the historical windows,
        // including a model window that the endpoint no longer reports.
        let at = time + 303;
        observe(&store, &current, 99., at);
        store
            .observe(
                &candidate,
                Usage {
                    used_percent: 10.,
                    observed_at: at,
                    resets_at: Some(time + 7200),
                    source: "claude_oauth".into(),
                    windows: vec![window("five_hour"), window("seven_day")],
                },
            )
            .unwrap();
        assert_eq!(
            store
                .rotation_decision(&policy(), Some(&current), at)
                .unwrap()
                .candidate_id,
            Some(candidate)
        );
    }
}

#[test]
fn partial_headers_cannot_erase_unknown_weekly_capacity_for_rotation() {
    let (_root, _vault, store) = setup();
    let time = clock();
    let current = account(&store, "org-current", time);
    let candidate = account(&store, "org-candidate", time);
    observe(&store, &current, 99., time);
    store
        .observe(
            &candidate,
            Usage {
                used_percent: 98.,
                observed_at: time,
                resets_at: Some(time + 3600),
                source: "claude_oauth".into(),
                windows: vec![
                    UsageWindow {
                        name: "five_hour".into(),
                        used_percent: 10.,
                        resets_at: Some(time + 3600),
                    },
                    UsageWindow {
                        name: "seven_day".into(),
                        used_percent: 98.,
                        resets_at: Some(time + 7200),
                    },
                ],
            },
        )
        .unwrap();
    let partial = Usage {
        used_percent: 10.,
        observed_at: time + 1,
        resets_at: Some(time + 3600),
        source: "response_headers".into(),
        windows: vec![UsageWindow {
            name: "five_hour".into(),
            used_percent: 10.,
            resets_at: Some(time + 3600),
        }],
    };
    store.observe(&candidate, partial.clone()).unwrap();
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .accounts
            .iter()
            .find(|a| a.id == candidate)
            .unwrap()
            .usage_health
            .as_ref()
            .unwrap()
            .next_check_at,
        // Headers keep the schedule of the last endpoint check: idle, no saved policy (SB-48).
        time + switchboard_core::CHECK_IDLE_SECONDS
    );
    assert_eq!(
        store
            .rotation_decision(&policy(), Some(&current), time + 1)
            .unwrap()
            .reason,
        "no_eligible_account"
    );
    let mut complete = partial;
    complete.windows.push(UsageWindow {
        name: "seven_day".into(),
        used_percent: 5.,
        resets_at: Some(time + 7200),
    });
    store.observe(&candidate, complete).unwrap();
    assert_eq!(
        store
            .rotation_decision(&policy(), Some(&current), time + 1)
            .unwrap()
            .candidate_id,
        Some(candidate)
    );
}

/// 0.5: a provider limit error moves off an account whose measured quota still looks fine —
/// a seat's individual spend limit is invisible to the quota endpoint.
#[test]
fn a_limit_error_switches_despite_spare_quota_and_skips_limited_candidates() {
    use std::collections::HashSet;
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = account(&store, "org-a", now);
    let b = account(&store, "org-b", now);
    let c = account(&store, "org-c", now);
    for (id, used) in [(&a, 20.), (&b, 5.), (&c, 30.)] {
        observe(&store, id, used, now);
    }
    let mut p = policy();
    // Without an error the 20% account stays.
    let held = store.rotation_decision(&p, Some(&a), now).unwrap();
    assert_eq!(held.reason, "below_threshold");
    // The account in use hit a limit; the best unlimited candidate is chosen, even inside cooldown.
    p.last_switched_at = Some(now - 10);
    let limited: HashSet<String> = [a.clone(), b.clone()].into();
    let decision = store
        .rotation_decision_with(&p, Some(&a), now, &limited)
        .unwrap();
    assert_eq!(decision.reason, "limit_reached");
    assert_eq!(
        decision.candidate_id.as_deref(),
        Some(c.as_str()),
        "b is limited too"
    );
    // Every other account limited as well: hold with a distinct reason.
    let all: HashSet<String> = [a.clone(), b.clone(), c.clone()].into();
    let decision = store
        .rotation_decision_with(&p, Some(&a), now, &all)
        .unwrap();
    assert_eq!(decision.reason, "limit_no_eligible_account");
    assert!(decision.candidate_id.is_none());
    // A limited candidate is never chosen by an ordinary threshold switch either.
    p.last_switched_at = None;
    observe(&store, &a, 95., now);
    let only_b: HashSet<String> = [b.clone()].into();
    let decision = store
        .rotation_decision_with(&p, Some(&a), now, &only_b)
        .unwrap();
    assert_eq!(decision.reason, "threshold_reached");
    assert_eq!(decision.candidate_id.as_deref(), Some(c.as_str()));
    // A limited account in use takes any account with room, even above the usual headroom.
    observe(&store, &c, 92., now);
    let limited: HashSet<String> = [a.clone(), b.clone()].into();
    let decision = store
        .rotation_decision_with(&p, Some(&a), now, &limited)
        .unwrap();
    assert_eq!(decision.candidate_id.as_deref(), Some(c.as_str()));
}

/// SB-40: a metered feature's limit is not the account's. It stays in the stored aggregate
/// (older builds read it conservatively) but rotation weighs the account's own windows, and an
/// observation with only feature windows has unknown account capacity — never eligible.
#[test]
fn feature_limits_neither_move_the_account_nor_make_a_candidate() {
    let (root, vault, store) = setup();
    let now = clock();
    let with = |windows: &[(&str, f64)]| Usage {
        used_percent: windows.iter().map(|w| w.1).fold(0.0, f64::max),
        observed_at: now,
        resets_at: Some(now + 3600),
        source: "codex_oauth".into(),
        windows: windows
            .iter()
            .map(|(name, used)| UsageWindow {
                name: (*name).into(),
                used_percent: *used,
                resets_at: Some(now + 3600),
            })
            .collect(),
    };
    let a = account(&store, "org-a", now);
    let b = account(&store, "org-b", now);
    let c = account(&store, "org-c", now);
    // The account in use has room; only one of its features is exhausted.
    store
        .observe(
            &a,
            with(&[("five_hour", 40.), ("feature_codex_other_primary", 100.)]),
        )
        .unwrap();
    observe(&store, &c, 10., now);
    let p = policy();
    let decision = store.rotation_decision(&p, Some(&a), now).unwrap();
    assert_eq!(decision.candidate_id, None);
    assert_ne!(decision.reason, "threshold_reached");
    // The account in use is full; the only other account with a fresh observation reports only
    // a feature window: unknown, not a candidate. The measured one is.
    observe(&store, &a, 95., now);
    store
        .observe(&b, with(&[("feature_codex_other_primary", 0.)]))
        .unwrap();
    observe(&store, &c, 99., now);
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "no_eligible_account"
    );
    observe(&store, &c, 10., now);
    assert_eq!(
        store
            .rotation_decision(&p, Some(&a), now)
            .unwrap()
            .candidate_id
            .as_deref(),
        Some(c.as_str())
    );
    // An account-wide blocker (spend control, a workspace limit) is not a feature: it counts.
    store
        .observe(&c, with(&[("five_hour", 10.), ("workspace_credits", 100.)]))
        .unwrap();
    assert_eq!(
        store.rotation_decision(&p, Some(&a), now).unwrap().reason,
        "no_eligible_account"
    );
    // A feature window whose reset has passed does not make the account's figures stale.
    let mut passed = with(&[("five_hour", 10.), ("feature_codex_other_primary", 100.)]);
    passed.windows[1].resets_at = Some(now + 1);
    store.observe(&c, passed).unwrap();
    assert_eq!(
        store
            .rotation_decision(&p, Some(&a), now + 2)
            .unwrap()
            .candidate_id
            .as_deref(),
        Some(c.as_str())
    );
    // The stored metadata still passes validation when the store is opened again.
    drop(store);
    Store::open(root.path().into(), vault).unwrap();
}

// SB-48: the quota-check cadence follows what the numbers decide.
fn account_in(store: &Store, org: &str, pool: &str, now: i64) -> String {
    store
        .upsert(
            org.into(),
            Provider::Claude,
            AuthKind::OAuth,
            pool.into(),
            credential(org, now + 9000),
            Some(identity(org)),
        )
        .unwrap()
        .id
}
fn next_check(store: &Store, id: &str) -> i64 {
    store
        .snapshot()
        .unwrap()
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .unwrap()
        .usage_health
        .unwrap()
        .next_check_at
}
fn usage_resetting(now: i64, resets: &[Option<i64>]) -> Usage {
    Usage {
        used_percent: 40.,
        observed_at: now,
        resets_at: resets.iter().flatten().copied().max(),
        source: "claude_oauth".into(),
        windows: resets
            .iter()
            .enumerate()
            .map(|(n, reset)| UsageWindow {
                name: format!("window_{n}"),
                used_percent: 40.,
                resets_at: *reset,
            })
            .collect(),
    }
}

#[test]
fn an_idle_account_is_checked_every_ten_minutes_and_just_after_a_reset() {
    use switchboard_core::{
        next_quota_check, CHECK_ACTIVE_SECONDS, CHECK_AFTER_RESET_SECONDS, CHECK_IDLE_SECONDS,
    };
    let now = clock();
    let far = usage_resetting(now, &[Some(now + 3600), Some(now + 86_400)]);
    assert_eq!(
        next_quota_check(true, &far, now),
        now + CHECK_ACTIVE_SECONDS
    );
    assert_eq!(next_quota_check(false, &far, now), now + CHECK_IDLE_SECONDS);
    // The earliest future reset of any window pulls the idle check in.
    let soon = usage_resetting(now, &[Some(now + 86_400), Some(now + 200)]);
    assert_eq!(
        next_quota_check(false, &soon, now),
        now + 200 + CHECK_AFTER_RESET_SECONDS
    );
    // Never sooner than a minute, and a reset already passed or unknown changes nothing.
    let imminent = usage_resetting(now, &[Some(now + 5)]);
    assert_eq!(next_quota_check(false, &imminent, now), now + 60);
    let passed = usage_resetting(now, &[Some(now - 5), None]);
    assert_eq!(
        next_quota_check(false, &passed, now),
        now + CHECK_IDLE_SECONDS
    );
    // An observation stored without windows (before 0.4) uses its aggregate reset.
    let mut aggregate = usage_resetting(now, &[]);
    aggregate.resets_at = Some(now + 300);
    assert_eq!(
        next_quota_check(false, &aggregate, now),
        now + 300 + CHECK_AFTER_RESET_SECONDS
    );
}

#[test]
fn rotation_pools_routed_and_in_use_accounts_keep_the_three_minute_cadence() {
    use switchboard_core::{CHECK_ACTIVE_SECONDS, CHECK_IDLE_SECONDS};
    let (_root, _vault, store) = setup();
    let now = clock();
    let rotating = account_in(&store, "org-rotating", "default", now);
    let idle = account_in(&store, "org-idle", "idle", now);
    let routed = account_in(&store, "org-routed", "routed", now);
    let other = account_in(&store, "org-other", "routed", now);
    store.set_policy(policy()).unwrap();
    store.select(Provider::Claude, "routed", &routed).unwrap();
    store.select(Provider::Claude, "idle", &idle).unwrap();
    // The "idle" pool's route names the account, so pick another to be idle there.
    let idle_free = account_in(&store, "org-idle-free", "idle", now);
    for id in [&rotating, &idle, &routed, &other, &idle_free] {
        observe(&store, id, 40., now);
    }
    assert_eq!(next_check(&store, &rotating), now + CHECK_ACTIVE_SECONDS);
    assert_eq!(next_check(&store, &routed), now + CHECK_ACTIVE_SECONDS);
    assert_eq!(next_check(&store, &other), now + CHECK_IDLE_SECONDS);
    assert_eq!(next_check(&store, &idle_free), now + CHECK_IDLE_SECONDS);
    // The account the ordinary CLI is signed in to is active too, once the monitor says so.
    store.set_in_use([idle_free.clone()].into()).unwrap();
    observe(&store, &idle_free, 41., now + 1);
    assert_eq!(
        next_check(&store, &idle_free),
        now + 1 + CHECK_ACTIVE_SECONDS
    );
}

#[test]
fn becoming_the_account_in_use_pulls_its_check_in_without_a_write() {
    use switchboard_core::{CHECK_ACTIVE_SECONDS, CHECK_IDLE_SECONDS};
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = account_in(&store, "org-a", "idle", now);
    let b = account_in(&store, "org-b", "idle", now);
    let c = account_in(&store, "org-c", "other", now);
    store.select(Provider::Claude, "other", &c).unwrap();
    for id in [&a, &b] {
        observe(&store, id, 40., now);
    }
    // Both idle: neither is the pool's route (the first account added may be).
    let routes = store.snapshot().unwrap().routes;
    let idle_ids: Vec<&String> = [&a, &b]
        .into_iter()
        .filter(|id| !routes.values().any(|r| r == *id))
        .collect();
    assert!(!idle_ids.is_empty());
    let target = idle_ids[0].clone();
    assert_eq!(next_check(&store, &target), now + CHECK_IDLE_SECONDS);
    // The first call only learns the set: a start moves no schedule.
    store.set_in_use([target.clone()].into()).unwrap();
    assert_eq!(next_check(&store, &target), now + CHECK_IDLE_SECONDS);
    // A later switch to it does, in memory only.
    store.set_in_use(Default::default()).unwrap();
    store.flush().unwrap();
    let writes = store.metadata_writes();
    store.set_in_use([target.clone()].into()).unwrap();
    assert_eq!(next_check(&store, &target), now + CHECK_ACTIVE_SECONDS);
    assert_eq!(store.metadata_writes(), writes, "timestamps stay in memory");
    // Leaving it in use, or repeating the set, moves nothing.
    store.set_in_use([target.clone()].into()).unwrap();
    store.set_in_use(Default::default()).unwrap();
    assert_eq!(next_check(&store, &target), now + CHECK_ACTIVE_SECONDS);
}

// Projects (0.6): a project reserves a pool for its folders.
#[test]
fn a_project_takes_its_accounts_into_its_pool_and_gives_back_the_ones_left_out() {
    let (root, vault, store) = setup();
    let now = clock();
    let a = account_in(&store, "org-a", "default", now);
    let b = account_in(&store, "org-b", "default", now);
    store.select(Provider::Claude, "default", &a).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = (dir.path().join("web"), dir.path().join("api"));
    let project = store
        .save_project(
            None,
            "Alpha Web",
            &[one.clone(), two.clone()],
            std::slice::from_ref(&a),
        )
        .unwrap();
    assert_eq!(project.pool, "alpha-web");
    let snap = store.snapshot().unwrap();
    let pool = |id: &str| {
        snap.accounts
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .pool
            .clone()
    };
    assert_eq!((pool(&a), pool(&b)), ("alpha-web".into(), "default".into()));
    assert!(
        !snap.routes.contains_key("claude:default"),
        "its old route no longer applies"
    );
    assert_eq!(
        snap.project_for(&two.join("src")).unwrap().pool,
        "alpha-web"
    );
    assert!(snap.project_for(dir.path()).is_none());
    // Updating the set: b in, a out (back to default).
    store
        .save_project(
            Some("alpha-web"),
            "Alpha Web",
            std::slice::from_ref(&one),
            std::slice::from_ref(&b),
        )
        .unwrap();
    let snap = store.snapshot().unwrap();
    let pool = |id: &str| {
        snap.accounts
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .pool
            .clone()
    };
    assert_eq!((pool(&a), pool(&b)), ("default".into(), "alpha-web".into()));
    assert_eq!(snap.projects[0].folders.len(), 1);
    // Survives a restart, and removing the project keeps the accounts where they are.
    drop(store);
    let store = Store::open(root.path().into(), vault).unwrap();
    assert_eq!(store.snapshot().unwrap().projects.len(), 1);
    store.remove_project("alpha-web").unwrap();
    let snap = store.snapshot().unwrap();
    assert!(snap.projects.is_empty());
    assert_eq!(
        snap.accounts.iter().find(|x| x.id == b).unwrap().pool,
        "alpha-web"
    );
}

#[test]
fn projects_never_share_a_folder_or_an_account_identity() {
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = account_in(&store, "org-a", "default", now);
    let dir = tempfile::tempdir().unwrap();
    store
        .save_project(None, "One", &[dir.path().join("repo")], &[])
        .unwrap();
    // A folder inside another project's folder, or around it, is refused.
    assert!(store
        .save_project(None, "Two", &[dir.path().join("repo/sub")], &[])
        .is_err());
    assert!(store
        .save_project(None, "Two", &[dir.path().to_owned()], &[])
        .is_err());
    // Same name twice gets its own pool.
    let again = store
        .save_project(None, "One", &[dir.path().join("other")], &[])
        .unwrap();
    assert_eq!(again.pool, "one-2");
    // The same identity already saved in the target pool cannot move in beside itself.
    let copy = account_in(&store, "org-a", "one", now);
    let err = store
        .save_project(Some("one"), "One", &[dir.path().join("repo")], &[a, copy])
        .unwrap_err();
    assert!(err.contains("already saved in that pool"), "{err}");
}

#[test]
fn a_project_pool_never_drives_the_ordinary_claude_code() {
    let (_root, _vault, store) = setup();
    let now = clock();
    let a = account_in(&store, "org-a", "alpha", now);
    let mut native = policy();
    native.pool = "alpha".into();
    native.target = "claude_cli".into();
    store.set_policy(native.clone()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    store
        .save_project(
            None,
            "alpha",
            &[dir.path().join("repo")],
            std::slice::from_ref(&a),
        )
        .unwrap();
    let snap = store.snapshot().unwrap();
    assert!(
        !snap
            .policies
            .iter()
            .any(|p| p.pool == "alpha" && p.target == "claude_cli" && p.enabled),
        "adopting the pool switched its native policy off"
    );
    assert_eq!(
        store.set_policy(native).unwrap_err(),
        switchboard_core::PROJECT_NOT_NATIVE
    );
}
