use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use switchboard_core::{Account, AuthKind, Credential, MemoryVault, Provider, Store};
use tempfile::TempDir;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn setup() -> (TempDir, Store) {
    let root = TempDir::new().unwrap();
    let store = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
    (root, store)
}
fn add(store: &Store, provider: Provider, pool: &str, secret: &str) -> Account {
    store
        .add(
            format!("Synthetic {secret}"),
            provider,
            AuthKind::ApiKey,
            pool.into(),
            Credential::parse(provider, AuthKind::ApiKey, secret).unwrap(),
        )
        .unwrap()
}
fn project(root: &TempDir, relative: &str) -> PathBuf {
    let path = root.path().join("projects").join(relative);
    fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}
fn schema(root: &Path) -> u64 {
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("accounts.json")).unwrap()).unwrap();
    disk["schema_version"].as_u64().unwrap()
}

#[test]
fn saves_and_updates_one_rule_per_path_and_provider() {
    let (root, store) = setup();
    let work = add(&store, Provider::Claude, "work", "synthetic-work");
    let other = add(&store, Provider::Claude, "work", "synthetic-other");
    let path = project(&root, "alpha");
    let saved = store
        .set_rule(&path, &work.id, "managed", true, None)
        .unwrap();
    assert_eq!(saved.provider, Provider::Claude);
    assert_eq!(saved.account_id, work.id);
    let updated = store
        .set_rule(&path, &other.id, "managed", false, Some(now() + 3600))
        .unwrap();
    assert_eq!(updated.created_at, saved.created_at);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.rules.len(), 1);
    assert_eq!(snapshot.rules[0].account_id, other.id);
    assert!(!snapshot.rules[0].enabled);
    let events: Vec<_> = snapshot
        .events
        .iter()
        .filter(|e| e.action == "project_rule")
        .map(|e| e.detail.as_str())
        .collect();
    assert_eq!(events, ["saved", "paused"]);
}

#[test]
fn rejects_invalid_rules_without_changing_metadata() {
    let (root, store) = setup();
    let claude = add(&store, Provider::Claude, "work", "synthetic-claude");
    let codex = add(&store, Provider::Codex, "work", "synthetic-codex");
    let path = project(&root, "alpha");
    let before = store.snapshot().unwrap().events.len();
    for (path, id, target, expires) in [
        (
            Path::new("relative/project"),
            claude.id.as_str(),
            "managed",
            None,
        ),
        (
            path.as_path(),
            "00000000-0000-4000-8000-000000000000",
            "managed",
            None,
        ),
        (path.as_path(), claude.id.as_str(), "elsewhere", None),
        (path.as_path(), codex.id.as_str(), "claude_cli", None),
        (path.as_path(), claude.id.as_str(), "claude_cli", None),
        (
            path.as_path(),
            claude.id.as_str(),
            "managed",
            Some(now() - 10),
        ),
    ] {
        assert!(store.set_rule(path, id, target, true, expires).is_err());
    }
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.rules.is_empty());
    assert_eq!(snapshot.events.len(), before);
}

#[test]
fn resolves_the_longest_enabled_unexpired_prefix_per_provider() {
    let (root, store) = setup();
    let parent = add(&store, Provider::Claude, "work", "synthetic-parent");
    let child = add(&store, Provider::Claude, "work", "synthetic-child");
    let codex = add(&store, Provider::Codex, "default", "synthetic-codex");
    let alpha = project(&root, "alpha");
    let nested = project(&root, "alpha/service");
    let sibling = project(&root, "alphabet");
    store
        .set_rule(&alpha, &parent.id, "managed", true, None)
        .unwrap();
    store
        .set_rule(&nested, &child.id, "managed", true, None)
        .unwrap();
    store
        .set_rule(&alpha, &codex.id, "managed", true, None)
        .unwrap();
    let deep = project(&root, "alpha/service/src");
    let resolved = store.resolve_rules(&deep, now()).unwrap();
    let claude = resolved
        .iter()
        .find(|r| r.provider == Provider::Claude)
        .unwrap();
    assert_eq!(claude.effective.as_ref().unwrap().account_id, child.id);
    let codex_match = resolved
        .iter()
        .find(|r| r.provider == Provider::Codex)
        .unwrap();
    assert_eq!(codex_match.effective.as_ref().unwrap().account_id, codex.id);
    assert!(store
        .resolve_rules(&sibling, now())
        .unwrap()
        .iter()
        .all(|r| r.effective.is_none() && r.nearest.is_none()));

    store
        .set_rule(&nested, &child.id, "managed", false, None)
        .unwrap();
    let resolved = store.resolve_rules(&deep, now()).unwrap();
    let claude = resolved
        .iter()
        .find(|r| r.provider == Provider::Claude)
        .unwrap();
    assert_eq!(claude.effective.as_ref().unwrap().account_id, parent.id);
    assert_eq!(claude.nearest.as_ref().unwrap().account_id, child.id);

    store
        .set_rule(&alpha, &parent.id, "managed", true, Some(now() + 60))
        .unwrap();
    let later = store.resolve_rules(&deep, now() + 120).unwrap();
    let claude = later
        .iter()
        .find(|r| r.provider == Provider::Claude)
        .unwrap();
    assert!(claude.effective.is_none());
}

#[test]
fn removing_a_rule_or_its_account_removes_it_in_one_write() {
    let (root, store) = setup();
    let first = add(&store, Provider::Claude, "work", "synthetic-first");
    let second = add(&store, Provider::Claude, "work", "synthetic-second");
    let alpha = project(&root, "alpha");
    let beta = project(&root, "beta");
    store
        .set_rule(&alpha, &first.id, "managed", true, None)
        .unwrap();
    store
        .set_rule(&beta, &second.id, "managed", true, None)
        .unwrap();
    let removed = store.remove_rule(&alpha, Provider::Claude).unwrap();
    assert_eq!(removed.account_id, first.id);
    assert!(store.remove_rule(&alpha, Provider::Claude).is_err());
    store.remove(&second.id).unwrap();
    assert!(store.snapshot().unwrap().rules.is_empty());
}

#[test]
fn schema_three_is_written_only_while_rules_exist() {
    let (root, store) = setup();
    let account = add(&store, Provider::Claude, "work", "synthetic-schema");
    assert_eq!(schema(root.path()), 2);
    let alpha = project(&root, "alpha");
    store
        .set_rule(&alpha, &account.id, "managed", true, None)
        .unwrap();
    assert_eq!(schema(root.path()), 3);
    drop(store);
    let reopened = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(reopened.snapshot().unwrap().rules.len(), 1);
    reopened.remove_rule(&alpha, Provider::Claude).unwrap();
    assert_eq!(schema(root.path()), 2);
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert!(disk["snapshot"].get("rules").is_none());
}
