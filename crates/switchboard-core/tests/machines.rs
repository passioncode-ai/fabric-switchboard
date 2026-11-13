//! SB-355 (DEC-0013): an account on another machine lives in that machine's group, and in no
//! group here.
use std::sync::Arc;
use switchboard_core::{
    machines, AuthKind, Credential, MachineRef, MemoryVault, Provider, RotationPolicy, Store,
};
use tempfile::TempDir;

fn oauth(refresh: &str) -> Credential {
    Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        &format!(
            r#"{{"claudeAiOauth":{{"accessToken":"a-{refresh}","refreshToken":"{refresh}"}}}}"#
        ),
    )
    .unwrap()
}
fn setup() -> (TempDir, Arc<MemoryVault>, Store) {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    (root, vault, store)
}
fn save(store: &Store, label: &str) -> String {
    store
        .upsert(
            format!("{label}@example.com"),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            oauth(label),
            None,
        )
        .unwrap()
        .id
}
fn studio() -> MachineRef {
    MachineRef {
        target: "sshlg@100.69.20.19".into(),
        identity: Some("sshlg@100.69.20.19:22".into()),
        aliases: vec!["studio-sshlg".into()],
    }
}
fn schema(root: &std::path::Path) -> u64 {
    let disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("accounts.json")).unwrap()).unwrap();
    disk["schema_version"].as_u64().unwrap()
}

#[test]
fn an_account_on_another_machine_leaves_every_group_here() {
    let (root, _vault, store) = setup();
    let (moving, staying, other) = (
        save(&store, "moving"),
        save(&store, "staying"),
        save(&store, "other"),
    );
    let folder = root.path().join("agents");
    std::fs::create_dir_all(&folder).unwrap();
    let group = store
        .save_project(
            None,
            "Agents",
            std::slice::from_ref(&folder),
            &[moving.clone(), staying.clone()],
        )
        .unwrap();
    // `default` borrows it, and it is the group's selection.
    assert!(store.set_shared("default", &moving, true).unwrap());
    store
        .select(Provider::Claude, &group.pool, &moving)
        .unwrap();
    assert_eq!(schema(root.path()), 13);

    store
        .mark_elsewhere_at(&moving, true, 100, Some("sshlg@100.69.20.19"))
        .unwrap();
    let machine = store.join_machine(&moving, &studio(), 100).unwrap();
    assert_eq!(machine.pool, "machine_studio-sshlg");
    assert_eq!(machine.name, "studio-sshlg");

    let s = store.snapshot().unwrap();
    let a = s.accounts.iter().find(|a| a.id == moving).unwrap();
    assert_eq!(a.pool, machine.pool, "its home is the machine's group");
    assert!(!a.enabled && s.elsewhere.contains_key(&moving));
    assert_eq!(
        s.pools_of(a),
        vec![machine.pool.as_str()],
        "in no group here"
    );
    assert!(s.default_shared.is_empty(), "the loan to default ended");
    assert!(
        !s.routes.values().any(|v| v == &moving),
        "no selection here"
    );
    assert_eq!(s.returns_to.get(&moving), Some(&group.pool));
    assert_eq!(
        s.machine_of(&moving).map(|m| m.name.as_str()),
        Some("studio-sshlg")
    );
    assert!(s
        .events
        .iter()
        .any(|e| e.action == "machine" && e.detail == "joined"));
    assert_eq!(schema(root.path()), 14);
    // Joining again under another name of the same machine changes nothing.
    let again = MachineRef {
        target: "studio-sshlg".into(),
        identity: None,
        aliases: vec![],
    };
    assert_eq!(
        store.join_machine(&moving, &again, 101).unwrap().pool,
        machine.pool
    );
    assert_eq!(store.snapshot().unwrap().machines.len(), 1);

    // Nothing here takes it back in by itself.
    let refused = store.save_project(
        Some(&group.pool),
        "Agents",
        std::slice::from_ref(&folder),
        &[moving.clone()],
    );
    assert_eq!(refused.unwrap_err(), machines::ON_ANOTHER_MACHINE);
    assert_eq!(
        store.return_to_default(&moving).unwrap_err(),
        machines::ON_ANOTHER_MACHINE
    );
    assert!(store
        .select(Provider::Claude, &machine.pool, &moving)
        .is_err());
    assert!(
        !store.set_shared(&group.pool, &moving, true).unwrap(),
        "never lent"
    );
    let saved = store.upsert(
        "x".into(),
        Provider::Claude,
        AuthKind::OAuth,
        machine.pool.clone(),
        oauth("x"),
        None,
    );
    assert_eq!(saved.unwrap_err(), machines::MACHINE_POOL_REFUSED);
    let mut policy = switchboard_core::default_managed_policy(Provider::Claude, &machine.pool);
    policy.enabled = true;
    assert_eq!(
        store.set_policy(policy).unwrap_err(),
        machines::MACHINE_NOT_ROTATED
    );
    store.ensure_default_policies().unwrap();
    let s = store.snapshot().unwrap();
    assert!(!s
        .policies
        .iter()
        .any(|p: &RotationPolicy| p.pool == machine.pool));
    let _ = other;

    // It survives a reopen.
    drop(store);
    let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(store.snapshot().unwrap().machines, [machine]);
}

#[test]
fn coming_home_goes_to_the_group_named_else_the_one_it_left_else_default() {
    let (root, _vault, store) = setup();
    let (a, b, c) = (save(&store, "a"), save(&store, "b"), save(&store, "c"));
    let folder = root.path().join("apps");
    std::fs::create_dir_all(&folder).unwrap();
    let apps = store
        .save_project(
            None,
            "Apps",
            std::slice::from_ref(&folder),
            &[a.clone(), b.clone()],
        )
        .unwrap();
    let other_folder = root.path().join("pc");
    std::fs::create_dir_all(&other_folder).unwrap();
    let pc = store
        .save_project(
            None,
            "PC",
            std::slice::from_ref(&other_folder),
            &[c.clone()],
        )
        .unwrap();
    for id in [&a, &b, &c] {
        store.join_machine(id, &studio(), 5).unwrap();
    }
    let home = |id: &str| {
        store
            .snapshot()
            .unwrap()
            .accounts
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .pool
            .clone()
    };
    // "Use here": back to the group it left.
    store.mark_elsewhere(&a, false, 6).unwrap();
    assert_eq!(home(&a), apps.pool);
    assert!(
        store
            .snapshot()
            .unwrap()
            .accounts
            .iter()
            .find(|x| x.id == a)
            .unwrap()
            .enabled
    );
    // A group named before the call back.
    store.set_return(&b, &pc.pool).unwrap();
    assert_eq!(
        store.set_return(&a, "default").unwrap_err(),
        machines::NOT_ON_A_MACHINE
    );
    assert_eq!(
        store.set_return(&b, "nowhere").unwrap_err(),
        "Project not found."
    );
    store.update_fields(&b, None, Some(true)).unwrap();
    assert_eq!(home(&b), pc.pool);
    // The group it left is gone: the default group.
    store.remove_project(&pc.pool).unwrap();
    store.assign_to_agent(&c, Some("Agent")).unwrap();
    assert_eq!(home(&c), "default");
    let s = store.snapshot().unwrap();
    assert!(s.returns_to.is_empty());
    assert_eq!(
        s.events
            .iter()
            .filter(|e| e.action == "machine" && e.detail == "left")
            .count(),
        3
    );
    // An empty machine group stays until it is forgotten.
    assert_eq!(s.machines.len(), 1);
    store.join_machine(&a, &studio(), 7).unwrap();
    assert_eq!(
        store.forget_machine("studio-sshlg").unwrap_err(),
        machines::MACHINE_NOT_EMPTY
    );
    store.remove(&a).unwrap();
    assert_eq!(
        store.forget_machine("sshlg@100.69.20.19").unwrap().pool,
        "machine_studio-sshlg"
    );
    assert!(store.snapshot().unwrap().machines.is_empty());
    assert_eq!(
        store.forget_machine("studio-sshlg").unwrap_err(),
        machines::MACHINE_NOT_FOUND
    );
}

#[test]
fn a_store_that_breaks_the_rule_is_refused() {
    let (root, _vault, store) = setup();
    let a = save(&store, "a");
    store.join_machine(&a, &studio(), 5).unwrap();
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    // An account in a machine group that is not marked elsewhere.
    disk["snapshot"]["elsewhere"] = serde_json::json!({});
    disk["snapshot"]["locations"] = serde_json::json!({});
    std::fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(Store::open(root.path().into(), Arc::new(MemoryVault::default())).is_err());
}
