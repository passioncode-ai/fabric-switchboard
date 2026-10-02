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
        time + 180
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
}
