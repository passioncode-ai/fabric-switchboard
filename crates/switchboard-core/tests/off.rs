//! W1.2 (SB-418, DEC-0027): an account the person turned off stays off until the person turns
//! it on; no automatic writer clears it, and every surface can say since when.
use std::{fs, sync::Arc};
use switchboard_core::{
    off::{self, OffBy},
    AuthKind, Credential, MemoryVault, Provider, Store,
};
use tempfile::TempDir;

fn oauth(n: &str) -> Credential {
    Credential::parse(
        Provider::Claude,
        AuthKind::OAuth,
        &format!(
            r#"{{"claudeAiOauth":{{"accessToken":"a-{n}","refreshToken":"r-{n}","expiresAt":4102444800000,"scopes":["user:inference"]}}}}"#
        ),
    )
    .unwrap()
}

fn setup() -> (TempDir, Arc<MemoryVault>, Store, String) {
    let root = TempDir::new().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let store = Store::open(root.path().into(), vault.clone()).unwrap();
    let a = store
        .add(
            "one".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            oauth("1"),
        )
        .unwrap();
    (root, vault, store, a.id)
}

fn account(store: &Store, id: &str) -> switchboard_core::Account {
    store
        .snapshot()
        .unwrap()
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .unwrap()
}

#[test]
fn turning_an_account_off_records_since_when_and_only_the_person_turns_it_on() {
    let (root, _vault, store, id) = setup();
    store.select(Provider::Claude, "default", &id).unwrap();
    store.update_fields(&id, None, Some(false)).unwrap();
    let a = account(&store, &id);
    let off = a.off.unwrap();
    assert!(!a.enabled && !a.usable() && off.by == OffBy::Person && off.since > 0);
    let s = store.snapshot().unwrap();
    assert!(s.routes.is_empty(), "its selection is dropped");
    assert_eq!(off::availability(&s, &a), "off");
    assert!(s
        .events
        .iter()
        .any(|e| e.action == off::TURNED_OFF && e.account_id.as_deref() == Some(&id)));
    // Written at version 17, the off and its time on disk.
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("accounts.json")).unwrap()).unwrap();
    assert_eq!(disk["schema_version"], 17);
    assert_eq!(disk["snapshot"]["accounts"][0]["off"]["by"], "person");
    // Turned off again: the time stays. A rename leaves it off.
    store.update_fields(&id, None, Some(false)).unwrap();
    store
        .update_fields(&id, Some("renamed".into()), None)
        .unwrap();
    assert_eq!(account(&store, &id).off, Some(off));
    // Every use is refused, by name.
    assert_eq!(
        store.select(Provider::Claude, "default", &id).unwrap_err(),
        off::ACCOUNT_OFF
    );
    assert!(store.route(Provider::Claude, "default").is_err());
    // The person turns it on.
    store.update_fields(&id, None, Some(true)).unwrap();
    let a = account(&store, &id);
    assert!(a.usable() && a.off.is_none());
    assert!(store
        .snapshot()
        .unwrap()
        .events
        .iter()
        .any(|e| e.action == off::TURNED_ON));
    store.select(Provider::Claude, "default", &id).unwrap();
}

/// The leak: clearing the mark of being used on another machine turned every account on,
/// one the person had turned off before it was marked included.
#[test]
fn clearing_the_elsewhere_mark_leaves_an_account_turned_off_off() {
    let (_root, _vault, store, id) = setup();
    store.update_fields(&id, None, Some(false)).unwrap();
    let since = account(&store, &id).off_since();
    store
        .mark_elsewhere_at(&id, true, 10, Some("studio"))
        .unwrap();
    store.mark_elsewhere(&id, false, 20).unwrap();
    let a = account(&store, &id);
    assert!(!a.enabled && a.off_since() == since);
    assert!(store.snapshot().unwrap().elsewhere.is_empty());
    // Giving it to an agent clears a mark too, and leaves it off.
    store.mark_elsewhere(&id, true, 30).unwrap();
    store.assign_to_agent(&id, Some("an agent")).unwrap();
    assert!(!account(&store, &id).usable());
    // A mark on an account that is on still clears to on, as before.
    let b = store
        .add(
            "two".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            oauth("2"),
        )
        .unwrap();
    store.mark_elsewhere(&b.id, true, 10).unwrap();
    store.mark_elsewhere(&b.id, false, 20).unwrap();
    assert!(account(&store, &b.id).usable());
}

/// A call back brings a copy home off when it was off on the other machine, with its time.
#[test]
fn an_off_carried_from_another_machine_keeps_its_time_and_an_earlier_off_here_wins() {
    let (_root, _vault, store, id) = setup();
    store.turn_off_since(&id, 1_000).unwrap();
    assert_eq!(account(&store, &id).off_since(), Some(1_000));
    store.turn_off_since(&id, 2_000).unwrap();
    assert_eq!(account(&store, &id).off_since(), Some(1_000), "already off");
    // A time that is not one carries nothing.
    let (_r2, _v2, other, b) = setup();
    other.turn_off_since(&b, 0).unwrap();
    assert!(account(&other, &b).usable());
}

/// A store written before W1.2: an account off for no reason it records is the person's off
/// (the safe reading), an account used elsewhere is not.
#[test]
fn a_store_from_before_reads_an_account_off_for_no_recorded_reason_as_turned_off() {
    let (root, vault, store, id) = setup();
    let b = store
        .add(
            "two".into(),
            Provider::Claude,
            AuthKind::OAuth,
            "default".into(),
            oauth("2"),
        )
        .unwrap();
    store.mark_elsewhere(&b.id, true, 10).unwrap();
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    disk["schema_version"] = 8.into();
    let accounts = disk["snapshot"]["accounts"].as_array_mut().unwrap();
    for a in accounts.iter_mut() {
        if a["id"] == id.as_str() {
            a["enabled"] = false.into();
        }
    }
    // Its newest update in the journal is when it was turned off.
    disk["snapshot"]["events"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"at": 4242, "action": "account_updated", "account_id": id, "detail": "success"}));
    fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    let store = Store::open(root.path().into(), vault).unwrap();
    assert_eq!(account(&store, &id).off_since(), Some(4242));
    let b = account(&store, &b.id);
    assert!(
        b.off.is_none() && !b.enabled,
        "used elsewhere is not the person's off"
    );
    // Written once, at version 17.
    let disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk["schema_version"], 17);
}

/// A store that says an account is both off and on is refused: no path can publish one.
#[test]
fn a_store_that_says_off_and_on_at_once_is_refused() {
    let (root, vault, store, id) = setup();
    store.update_fields(&id, None, Some(false)).unwrap();
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    disk["snapshot"]["accounts"][0]["enabled"] = true.into();
    fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(Store::open(root.path().into(), vault).is_err());
}

/// Review round 1 of PR #821 (P1-1): the window's rename names no `enabled` when its switch was
/// not touched. A rename of an account used on another machine leaves it as it was, so clearing
/// its mark turns it on as before; it never becomes the person's off.
#[test]
fn a_rename_that_names_no_switch_leaves_an_account_used_elsewhere_as_it_was() {
    let (_root, _vault, store, id) = setup();
    store
        .mark_elsewhere_at(&id, true, 10, Some("studio"))
        .unwrap();
    store
        .update_fields(&id, Some("renamed".into()), None)
        .unwrap();
    let s = store.snapshot().unwrap();
    let a = account(&store, &id);
    assert_eq!(a.label, "renamed");
    assert!(!a.enabled && a.off.is_none());
    assert_eq!(off::availability(&s, &a), "elsewhere");
    assert!(!s.events.iter().any(|e| e.action == off::TURNED_OFF));
    store.mark_elsewhere(&id, false, 20).unwrap();
    assert!(account(&store, &id).usable());
}

/// Review round 2 of PR #821 (P1-A): turning on an account the person turned off while it is
/// marked as used on another machine takes back only their word; the mark stays, and *Use here*
/// (clearing the mark) brings it home on.
#[test]
fn turning_on_an_account_off_and_used_elsewhere_keeps_its_mark() {
    let (_root, _vault, store, id) = setup();
    store
        .mark_elsewhere_at(&id, true, 10, Some("studio"))
        .unwrap();
    store.update_fields(&id, None, Some(false)).unwrap();
    store.update_fields(&id, None, Some(true)).unwrap();
    let s = store.snapshot().unwrap();
    let a = account(&store, &id);
    assert!(a.off.is_none() && !a.enabled);
    assert!(s.elsewhere.contains_key(&id) && s.locations.contains_key(&id));
    assert_eq!(off::availability(&s, &a), "elsewhere");
    assert!(s.events.iter().any(|e| e.action == off::TURNED_ON));
    store.mark_elsewhere(&id, false, 20).unwrap();
    assert!(account(&store, &id).usable());
}

/// 2026-11-19 audit M5-03: a store with an account turned off is written at version 17, which a
/// 0.7.123 or older reader (one that knows versions up to 16) refuses. The refusal names the
/// version, never "corrupt", so the person reads that a newer Switchboard wrote it; shown here as
/// this reader refuses the version after its own. The way back is restoring a backup there.
#[test]
fn a_reader_refuses_a_store_of_a_later_version_by_its_version() {
    let (root, _vault, store, id) = setup();
    store.update_fields(&id, None, Some(false)).unwrap();
    drop(store);
    let path = root.path().join("accounts.json");
    let mut disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk["schema_version"], 17);
    disk["schema_version"] = 18.into();
    fs::write(&path, serde_json::to_vec(&disk).unwrap()).unwrap();
    let refused = Store::open(root.path().into(), Arc::new(MemoryVault::default()))
        .err()
        .expect("a later version is refused");
    assert_eq!(refused, "Unsupported account metadata version");
}
