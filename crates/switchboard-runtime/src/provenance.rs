//! SB-47 (Fabric COM-11, issue #36): which account serves a provider's group, since when and why,
//! and which session it serves, for Fabric's project responder and its replacement flow. The
//! shape is the one Fabric accepted on board thread `82241ad5` (2026-11-05): read through
//! `switchboard --json provenance`, accounts by their handle only (`account_ref`, never the UUID),
//! no credential, and no generation of Fabric's (a run takes its slot with `com.enroll`).
//!
//! Everything here is read from what Switchboard already keeps: the group's route, its journal
//! (`account_selected`, and `rotation` with the cause since SB-47), the limit evidence the owner
//! holds and, for one launch, its record.
use serde_json::{json, Value};
use switchboard_core::{Provider, Snapshot};

/// `rotation` journal details and the cause each names; a selection with none after it was the
/// person's (or a rule's) own choice.
fn cause_of(detail: &str) -> &'static str {
    match detail {
        "switched_on_limit" => "limit",
        "failover" | "unusable" => "failover",
        _ => "rotation",
    }
}

/// A hash of the provider's identity of the account, so a reader sees when the account behind a
/// handle changed without learning who it is; none when Switchboard holds no identity for it.
fn identity_revision(snapshot: &Snapshot, id: &str) -> Value {
    snapshot
        .accounts
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.external_identity.as_ref())
        .and_then(|i| serde_json::to_vec(i).ok())
        .map(|bytes| {
            let digest = <sha2::Sha256 as sha2::Digest>::digest(&bytes);
            json!(digest
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect::<String>())
        })
        .unwrap_or(Value::Null)
}

/// The provenance of `provider`'s group `pool` (`default` when none), or of one launch's account
/// when `launch` is its record. `limited` is the owner's limit evidence (`Limits::report`), empty
/// offline.
pub(crate) fn build(
    snapshot: &Snapshot,
    provider: Option<Provider>,
    pool: Option<&str>,
    launch: Option<&Value>,
    limited: &[Value],
) -> Result<Value, String> {
    let (provider, pool, account_id) = match launch {
        Some(record) => {
            let provider = record["provider"]
                .as_str()
                .and_then(|p| serde_json::from_value::<Provider>(json!(p)).ok())
                .or(provider)
                .ok_or(UNKNOWN_PROVIDER)?;
            let pool = record["pool"].as_str().unwrap_or("default").to_owned();
            let account = record["account_id"].as_str().map(str::to_owned);
            (provider, pool, account)
        }
        None => {
            let provider = provider.ok_or(UNKNOWN_PROVIDER)?;
            let pool = pool.unwrap_or("default").to_owned();
            let account = snapshot
                .routes
                .get(&format!("{}:{}", provider.as_str(), pool))
                .cloned();
            (provider, pool, account)
        }
    };
    let in_group = |id: &str| {
        snapshot
            .accounts
            .iter()
            .any(|a| a.id == id && a.provider == provider && a.pool == pool)
    };
    // The journal is oldest first and bounded: a choice older than it reads as unknown.
    let selections: Vec<(usize, &switchboard_core::Event)> = snapshot
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.action == "account_selected")
        .filter(|(_, e)| e.account_id.as_deref().is_some_and(in_group))
        .collect();
    let last = account_id.as_deref().and_then(|id| {
        selections
            .iter()
            .rev()
            .find(|(_, e)| e.account_id.as_deref() == Some(id))
    });
    let (changed_at, cause) = match last {
        Some((at, event)) => {
            // The `rotation` line written right after the selection, for the same account.
            let rotation = snapshot.events[at + 1..].iter().find(|e| {
                e.action == "rotation" && e.account_id == event.account_id && e.detail != "failed"
            });
            let cause = rotation.map_or("manual", |r| cause_of(&r.detail));
            (json!(event.at), json!(cause))
        }
        None => (Value::Null, json!("unknown")),
    };
    let replaced = last.and_then(|(at, event)| {
        selections
            .iter()
            .rev()
            .find(|(i, e)| i < at && e.account_id != event.account_id)
            .and_then(|(_, e)| e.account_id.clone())
    });
    let replaced = replaced.map(|id| {
        let limit = limited
            .iter()
            .find(|l| l["account_id"].as_str() == Some(id.as_str()))
            .map(|l| json!({"kind": l["kind"], "resets_at": l["resets_at"], "confidence": l["confidence"]}))
            .unwrap_or(Value::Null);
        json!({"account_ref": crate::continuation::account_ref(&id), "limit": limit})
    });
    let session = launch.map(|record| {
        let dispatch = &record["launched_by"]["dispatch"];
        json!({
            "launch_id": record["launch_id"],
            "dispatch_id": dispatch["id"],
            "workflow": dispatch["workflow"].as_str().map(Value::from).unwrap_or_else(|| record["continues"].clone()),
        })
    });
    Ok(json!({
        "provider": provider.as_str(),
        "pool": pool,
        "account_ref": account_id.as_deref().map(crate::continuation::account_ref),
        "identity_revision": account_id.as_deref().map_or(Value::Null, |id| identity_revision(snapshot, id)),
        "mode": if launch.is_some() { Value::Null } else { json!("managed") },
        "changed_at": changed_at,
        "cause": cause,
        "replaced": replaced,
        "session": session,
    }))
}

pub(crate) const UNKNOWN_PROVIDER: &str = "Name a provider (claude or codex) or a launch id.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::save;
    use std::sync::Arc;
    use switchboard_core::{MemoryVault, Store};

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        (dir, store)
    }

    #[test]
    fn a_groups_account_by_handle_since_when_and_why() {
        let (_dir, store) = store();
        let a = save(&store, "synthetic-a", "default");
        let b = save(&store, "synthetic-b", "default");
        store.select(Provider::Claude, "default", &a.id).unwrap();
        let snapshot = store.snapshot().unwrap();
        let shown = build(&snapshot, Some(Provider::Claude), None, None, &[]).unwrap();
        assert_eq!(
            shown["account_ref"],
            crate::continuation::account_ref(&a.id)
        );
        assert_eq!(shown["cause"], "manual");
        assert_eq!(shown["mode"], "managed");
        assert!(shown["changed_at"].as_i64().is_some());
        assert!(shown["identity_revision"]
            .as_str()
            .is_some_and(|r| r.len() == 16));
        assert!(shown["replaced"].is_null() && shown["session"].is_null());
        // A switch on a limit names its cause and the account it replaced, with its evidence.
        store.select(Provider::Claude, "default", &b.id).unwrap();
        store
            .record("rotation", Some(&b.id), "switched_on_limit")
            .unwrap();
        let limited = vec![
            json!({"account_id": a.id, "kind": "five_hour", "resets_at": 1_800_000_000, "confidence": "observed", "until": 1}),
        ];
        let snapshot = store.snapshot().unwrap();
        let shown = build(
            &snapshot,
            Some(Provider::Claude),
            Some("default"),
            None,
            &limited,
        )
        .unwrap();
        assert_eq!(shown["cause"], "limit");
        assert_eq!(
            shown["replaced"]["account_ref"],
            crate::continuation::account_ref(&a.id)
        );
        assert_eq!(
            shown["replaced"]["limit"],
            json!({"kind": "five_hour", "resets_at": 1_800_000_000, "confidence": "observed"})
        );
        // A 429 failover reads as failover; no id ever leaves.
        store.select(Provider::Claude, "default", &a.id).unwrap();
        store.record("rotation", Some(&a.id), "failover").unwrap();
        let shown = build(
            &store.snapshot().unwrap(),
            Some(Provider::Claude),
            None,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(shown["cause"], "failover");
        let text = shown.to_string();
        assert!(!text.contains(&a.id) && !text.contains(&b.id), "{text}");
        assert!(!text.contains("token") && !text.contains("example.invalid"));
    }

    #[test]
    fn a_launch_names_its_dispatch_and_workflow_and_a_group_with_no_choice_is_unknown() {
        let (_dir, store) = store();
        let a = save(&store, "synthetic-a", "default");
        let record = json!({"launch_id": "l-1", "provider": "claude", "pool": "default", "account_id": a.id,
            "launched_by": {"surface": "fabric", "dispatch": {"id": "d-1", "workflow": "wf_0123456789abcdef"}}});
        let shown = build(&store.snapshot().unwrap(), None, None, Some(&record), &[]).unwrap();
        assert_eq!(
            shown["session"],
            json!({"launch_id": "l-1", "dispatch_id": "d-1", "workflow": "wf_0123456789abcdef"})
        );
        assert!(shown["mode"].is_null());
        // Never selected while the journal holds it: when and why are unknown, never guessed.
        assert_eq!(
            (shown["changed_at"].clone(), shown["cause"].clone()),
            (Value::Null, json!("unknown"))
        );
        let none = build(
            &store.snapshot().unwrap(),
            Some(Provider::Codex),
            None,
            None,
            &[],
        )
        .unwrap();
        assert!(none["account_ref"].is_null());
        assert_eq!(
            build(&store.snapshot().unwrap(), None, None, None, &[]).unwrap_err(),
            UNKNOWN_PROVIDER
        );
    }
}
