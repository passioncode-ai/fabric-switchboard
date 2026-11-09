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

#[test]
fn a_groups_organization_is_written_while_it_has_one() {
    // SB-109: a group's organization survives a reopen and leaves the file when the group is
    // personal again. SB-285: a store holding an organization is schema 11, which a 0.7.34 or
    // older reader refuses by name (version 6, a group's organization, is folded into it);
    // without one it drops back to 4.
    let (root, store) = setup();
    let alpha = project(&root, "alpha");
    let group = store
        .save_project(None, "Alpha", std::slice::from_ref(&alpha), &[])
        .unwrap();
    assert_eq!(schema(root.path()), 4);
    let acme = store.add_organization("Acme", &[]).unwrap();
    assert_eq!(schema(root.path()), 11);
    store
        .set_project_organization(&group.pool, Some(&acme.id))
        .unwrap();
    assert_eq!(schema(root.path()), 11);
    drop(store);
    let reopened = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().projects[0].primary_organization(),
        Some(acme.id.as_str())
    );
    reopened
        .set_project_organization(&group.pool, None)
        .unwrap();
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert!(disk["snapshot"]["projects"][0]
        .get("organization")
        .is_none());
    assert_eq!(schema(root.path()), 11);
    reopened.remove_organization(&acme.id).unwrap();
    assert_eq!(schema(root.path()), 4);
}

#[test]
fn a_group_of_several_organizations_keeps_their_order() {
    // SB-282: one organization is still `organization`, several are `organizations`. SB-285:
    // with an organization the store is schema 11 (it folds in 6 and 9).
    let (root, store) = setup();
    let alpha = project(&root, "alpha");
    let group = store
        .save_project(None, "Alpha", std::slice::from_ref(&alpha), &[])
        .unwrap();
    let acme = store.add_organization("Acme", &[]).unwrap();
    let beta = store.add_organization("Beta", &[]).unwrap();
    let saved = store
        .set_project_organizations(
            &group.pool,
            vec![
                beta.id.clone(),
                "personal".into(),
                acme.id.clone(),
                beta.id.clone(),
            ],
        )
        .unwrap();
    assert_eq!(saved.organizations, vec![beta.id.clone(), acme.id.clone()]);
    assert_eq!(schema(root.path()), 11);
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    let written = &disk["snapshot"]["projects"][0];
    assert!(written.get("organization").is_none(), "{written}");
    assert_eq!(
        written["organizations"],
        serde_json::json!([beta.id, acme.id])
    );
    drop(store);
    let reopened = Store::open(root.path().into(), Arc::new(MemoryVault::default())).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().projects[0].organizations,
        vec![beta.id.clone(), acme.id.clone()]
    );
    // An unknown organization is refused and changes nothing.
    assert!(reopened
        .set_project_organizations(&group.pool, vec!["no-such".into()])
        .is_err());
    // Removing one organization leaves the group in the other: one again, as `organization`.
    reopened.remove_organization(&beta.id).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().projects[0].organizations,
        vec![acme.id.clone()]
    );
    assert_eq!(schema(root.path()), 11);
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert_eq!(
        disk["snapshot"]["projects"][0]["organization"],
        acme.id.as_str()
    );
}

#[test]
fn two_groups_of_one_organization_both_belong_to_it() {
    // SB-282: an organization may have several groups.
    let (root, store) = setup();
    let alpha = project(&root, "alpha");
    let beta = project(&root, "beta");
    let acme = store.add_organization("Acme", &[]).unwrap();
    for (name, folder) in [("Alpha", &alpha), ("Beta", &beta)] {
        let group = store
            .save_project(None, name, std::slice::from_ref(folder), &[])
            .unwrap();
        store
            .set_project_organization(&group.pool, Some(&acme.id))
            .unwrap();
    }
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.organization_groups(&acme.id).len(), 2);
    assert_eq!(schema(root.path()), 11);
}

#[test]
fn a_source_may_be_a_group_path() {
    // SB-141 (DEC-0089 `ownerOf`): GitLab's `host/group/sub` is an owner as GitHub's `host/owner` is.
    let (root, store) = setup();
    let alpha = project(&root, "alpha");
    let group = store
        .save_project(None, "Alpha", std::slice::from_ref(&alpha), &[])
        .unwrap();
    let saved = store
        .set_project_sources(
            &group.pool,
            &[
                "github.com/Example".into(),
                "gitlab.example.com/group/sub/".into(),
            ],
        )
        .unwrap();
    assert_eq!(
        saved.sources,
        vec![
            "github.com/example".to_string(),
            "gitlab.example.com/group/sub".to_string()
        ]
    );
    for bad in [
        "github.com",
        "example/owner",
        "github.com//x",
        "github.com/a b",
    ] {
        assert!(
            store
                .set_project_sources(&group.pool, &[bad.into()])
                .is_err(),
            "{bad}"
        );
    }
}

/// SB-313: an account whose own group goes away (it moves to `default`, or the group is deleted)
/// is no longer shared anywhere. Left in another group's list, that group's next save took it as
/// a move and dropped its route in `default`.
#[test]
fn an_account_whose_own_group_goes_away_is_no_longer_shared_and_never_moves_by_itself() {
    let (root, store) = setup();
    let x = add(&store, Provider::Claude, "default", "synthetic-x");
    let y = add(&store, Provider::Claude, "default", "synthetic-y");
    let z = add(&store, Provider::Claude, "default", "synthetic-z");
    let g = project(&root, "g");
    let h = project(&root, "h");
    let k = project(&root, "k");
    let home = store
        .save_project(
            None,
            "Home",
            std::slice::from_ref(&g),
            std::slice::from_ref(&x.id),
        )
        .unwrap();
    let kept = store
        .save_project(
            None,
            "Kept",
            std::slice::from_ref(&k),
            std::slice::from_ref(&z.id),
        )
        .unwrap();
    let shares = store
        .save_project(
            None,
            "Shares",
            std::slice::from_ref(&h),
            &[y.id.clone(), x.id.clone(), z.id.clone()],
        )
        .unwrap();
    assert_eq!(shares.shared_accounts, [x.id.clone(), z.id.clone()]);
    // x goes back to default: its own group lets it go, and the share goes with it.
    store
        .save_project(Some(&home.pool), "Home", std::slice::from_ref(&g), &[])
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let group = |pool: &str| {
        snapshot
            .projects
            .iter()
            .find(|p| p.pool == pool)
            .unwrap()
            .clone()
    };
    assert_eq!(
        group(&shares.pool).shared_accounts,
        std::slice::from_ref(&z.id)
    );
    store.select(Provider::Claude, "default", &x.id).unwrap();
    // The sharing group's next save (a folder the background adds) leaves x where it is.
    let more = project(&root, "h-worktree");
    let listed: Vec<String> = [y.id.clone(), z.id.clone()].into();
    store
        .save_project(Some(&shares.pool), "Shares", &[h.clone(), more], &listed)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let x_now = snapshot.accounts.iter().find(|a| a.id == x.id).unwrap();
    assert_eq!(x_now.pool, "default");
    assert_eq!(snapshot.routes.get("claude:default"), Some(&x.id));
    // z's own group is deleted: no group shares it any more.
    store.remove_project(&kept.pool).unwrap();
    let snapshot = store.snapshot().unwrap();
    let group = |pool: &str| {
        snapshot
            .projects
            .iter()
            .find(|p| p.pool == pool)
            .unwrap()
            .clone()
    };
    assert!(group(&shares.pool).shared_accounts.is_empty());
}

/// SB-313: placing an account moves its provider team with it. A teammate a group admitted
/// before and would refuse after keeps the choice from being made; the account itself follows
/// the person's word, and a group that had it selected moves to another account it admits.
#[test]
fn a_teammate_a_group_would_refuse_keeps_the_word_from_being_given() {
    use switchboard_core::organizations::{ASSIGN_OUTSIDE, PERSONAL};
    use switchboard_core::ExternalIdentity;
    let (root, store) = setup();
    let acme = store.add_organization("Acme", &[]).unwrap();
    let seat = |label: &str, token: &str| {
        store
            .upsert(
                label.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential::parse(
                    Provider::Claude,
                    AuthKind::OAuth,
                    &format!(r#"{{"claudeAiOauth":{{"accessToken":"a-{token}","refreshToken":"r-{token}"}}}}"#),
                )
                .unwrap(),
                Some(ExternalIdentity {
                    account_id: Some(format!("synthetic-{token}")),
                    organization_id: Some("synthetic-team".into()),
                    email: None,
                }),
            )
            .unwrap()
    };
    let lead = seat("Lead", "lead");
    let mate = seat("Mate", "mate");
    // The lead's word places the team in Acme; the mate follows the team into Acme's group.
    store.assign_organization(&lead.id, Some(&acme.id)).unwrap();
    let folder = project(&root, "acme");
    store
        .save_project_with(
            None,
            "Acme work",
            std::slice::from_ref(&folder),
            std::slice::from_ref(&mate.id),
            Some(vec![acme.id.clone()]),
            true,
        )
        .unwrap();
    assert_eq!(
        store
            .assign_organization(&lead.id, Some(PERSONAL))
            .unwrap_err(),
        ASSIGN_OUTSIDE
    );
    // The account itself may leave: its group moves to another account it admits.
    let c = add(&store, Provider::Claude, "default", "synthetic-c");
    let d = add(&store, Provider::Claude, "default", "synthetic-d");
    for id in [&c.id, &d.id] {
        store.assign_organization(id, Some(&acme.id)).unwrap();
    }
    let other = project(&root, "acme-two");
    let group = store
        .save_project_with(
            None,
            "Acme two",
            std::slice::from_ref(&other),
            &[c.id.clone(), d.id.clone()],
            Some(vec![acme.id.clone()]),
            true,
        )
        .unwrap();
    store.select(Provider::Claude, &group.pool, &c.id).unwrap();
    store.assign_organization(&c.id, Some(PERSONAL)).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot.routes.get(&format!("claude:{}", group.pool)),
        Some(&d.id)
    );
}

/// SB-314: an account a group already shares is not checked again on each save: one that no
/// longer fits (its organization changed by the person's word) no longer blocks a rename or a
/// folder added, and a deleted group's accounts go back to `default`.
#[test]
fn a_share_that_no_longer_fits_does_not_block_its_group_and_a_deleted_groups_accounts_go_home() {
    use switchboard_core::organizations::PERSONAL;
    let (root, store) = setup();
    let acme = store.add_organization("Acme", &[]).unwrap();
    let x = add(&store, Provider::Claude, "default", "synthetic-x");
    store.assign_organization(&x.id, Some(&acme.id)).unwrap();
    let home = project(&root, "home");
    let mine = store
        .save_project_with(
            None,
            "Home",
            std::slice::from_ref(&home),
            std::slice::from_ref(&x.id),
            Some(vec![acme.id.clone()]),
            true,
        )
        .unwrap();
    let work = project(&root, "work");
    let shares = store
        .save_project_with(
            None,
            "Work",
            std::slice::from_ref(&work),
            std::slice::from_ref(&x.id),
            Some(vec![acme.id.clone()]),
            true,
        )
        .unwrap();
    assert_eq!(shares.shared_accounts, std::slice::from_ref(&x.id));
    // The person's word takes x out of Acme: both groups refuse it now.
    store.assign_organization(&x.id, Some(PERSONAL)).unwrap();
    // The sharing group can still be renamed and given a folder.
    let more = project(&root, "work-2");
    store
        .save_project(
            Some(&shares.pool),
            "Work renamed",
            &[work.clone(), more],
            std::slice::from_ref(&x.id),
        )
        .unwrap();
    // Deleting x's own group sends x back to default; no group shares it any more.
    store.remove_project(&mine.pool).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .accounts
            .iter()
            .find(|a| a.id == x.id)
            .unwrap()
            .pool,
        "default"
    );
    assert!(snapshot
        .projects
        .iter()
        .all(|p| p.shared_accounts.is_empty()));
}

/// SB-314: a new account saved into the pool of a group of another organization is refused, as
/// a move into that group is; its own organization's group and a personal group take it.
#[test]
fn a_new_account_lands_only_in_a_group_that_admits_it() {
    let (root, store) = setup();
    let beta = store.add_organization("Beta", &[]).unwrap();
    let folder = project(&root, "beta");
    let group = store
        .save_project_with(
            None,
            "Beta work",
            std::slice::from_ref(&folder),
            &[],
            Some(vec![beta.id.clone()]),
            true,
        )
        .unwrap();
    let refused = store
        .add(
            "Synthetic personal".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            group.pool.clone(),
            Credential::parse(Provider::Claude, AuthKind::ApiKey, "synthetic-personal").unwrap(),
        )
        .unwrap_err();
    assert_eq!(refused, switchboard_core::organizations::SEAT_CHOSEN);
    assert!(store.snapshot().unwrap().accounts.is_empty());
    // A personal group takes it.
    let mine = project(&root, "mine");
    let personal = store
        .save_project(None, "Mine", std::slice::from_ref(&mine), &[])
        .unwrap();
    add(&store, Provider::Claude, &personal.pool, "synthetic-personal");
}
