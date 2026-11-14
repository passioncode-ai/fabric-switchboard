//! SB-364 (WF-2.4): the accounts given to an agent, each with where it is, the last launch that
//! ran on it and when it was last active. The window's *Given to agents* section reads it, and
//! the MCP tool `switchboard_accounts` carries the same object as each such row's `assigned`.
//! Ids, pools, folders and times only.
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, path::Path};

/// The newest launch record of each account (by `launched_at`), every state, ended included.
fn newest_launches(root: &Path) -> BTreeMap<String, Value> {
    let mut newest: BTreeMap<String, Value> = BTreeMap::new();
    for record in crate::launches::records(root) {
        let Some(id) = record["account_id"].as_str().map(str::to_owned) else {
            continue;
        };
        let at = record["launched_at"].as_i64().unwrap_or(0);
        if newest
            .get(&id)
            .is_none_or(|kept| kept["launched_at"].as_i64().unwrap_or(0) < at)
        {
            newest.insert(id, record);
        }
    }
    newest
}

/// One account's `assigned` view from the snapshot (as `Operation::Snapshot` answers it) and its
/// newest launch record, if any.
fn view(snapshot: &Value, account: &Value, agent: &str, launch: Option<&Value>) -> Value {
    let id = account["id"].as_str().unwrap_or_default();
    // The machine group it sits in, else where the person said it works.
    let machine = snapshot["machines"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["pool"] == account["pool"])
        .map(|m| m["name"].clone())
        .or_else(|| snapshot["locations"].get(id).cloned())
        .unwrap_or(Value::Null);
    let last_launch = launch.map(|r| {
        json!({
            "pool": r["pool"].as_str(),
            "folder": r["working_directory"].as_str(),
            "at": r["launched_at"].as_i64().unwrap_or(0),
        })
    });
    let last_activity = [
        launch.and_then(|r| r["launched_at"].as_i64()),
        launch.and_then(|r| r["last_turn_at"].as_i64()),
        launch.and_then(|r| r["ended_at"].as_i64()),
        account["usage"]["observed_at"].as_i64(),
    ]
    .into_iter()
    .flatten()
    .max();
    json!({
        "account_id": id,
        "agent": agent,
        "machine": machine,
        "last_launch": last_launch,
        "last_activity": last_activity,
    })
}

/// The `assigned` object of every account given to an agent, by account id, read from the
/// snapshot and the launch records in `root`. Empty, and no record read, when none is given.
pub fn assigned(root: &Path, snapshot: &Value) -> Map<String, Value> {
    let given = snapshot["assigned_to"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if given.is_empty() {
        return Map::new();
    }
    let launches = newest_launches(root);
    snapshot["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|account| {
            let id = account["id"].as_str()?;
            let agent = given.get(id)?.as_str()?;
            Some((
                id.to_owned(),
                view(snapshot, account, agent, launches.get(id)),
            ))
        })
        .collect()
}

/// `given_accounts`: every account given to an agent, in the snapshot's order.
pub fn accounts(root: &Path, snapshot: &Value) -> Value {
    let accounts: Vec<Value> = assigned(root, snapshot).into_values().collect();
    let order = |v: &Value| {
        snapshot["accounts"]
            .as_array()
            .and_then(|list| list.iter().position(|a| a["id"] == v["account_id"]))
            .unwrap_or(usize::MAX)
    };
    let mut accounts = accounts;
    accounts.sort_by_key(order);
    json!({ "accounts": accounts })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "00000000-0000-4000-8000-00000000000a";
    const B: &str = "00000000-0000-4000-8000-00000000000b";
    const C: &str = "00000000-0000-4000-8000-00000000000c";

    fn write_record(root: &Path, id: &str, record: Value) {
        let dir = root.join(crate::launches::DIR);
        std::fs::create_dir_all(&dir).unwrap();
        switchboard_core::private_fs::private_write(
            &dir.join(format!("{id}.json")),
            record.to_string().as_bytes(),
        )
        .unwrap();
    }

    fn snapshot() -> Value {
        json!({
            "accounts": [
                {"id": A, "pool": "default", "usage": {"observed_at": 500}},
                {"id": B, "pool": "default", "usage": null},
                {"id": C, "pool": "default", "usage": {"observed_at": 900}},
            ],
            "assigned_to": {B: "nightly-bot", A: "ad-manager"},
            "machines": [],
            "locations": {},
        })
    }

    #[test]
    fn nothing_given_reads_no_record() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        s["assigned_to"] = json!({});
        assert_eq!(accounts(tmp.path(), &s), json!({"accounts": []}));
        assert!(assigned(tmp.path(), &s).is_empty());
    }

    #[test]
    fn a_given_account_names_its_newest_launch_ended_included_and_its_last_activity() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_record(
            root,
            "11111111-1111-4111-8111-111111111111",
            json!({"launch_id": "11111111-1111-4111-8111-111111111111", "account_id": A,
                   "pool": "default", "working_directory": "/work/old", "launched_at": 100}),
        );
        write_record(
            root,
            "22222222-2222-4222-8222-222222222222",
            json!({"launch_id": "22222222-2222-4222-8222-222222222222", "account_id": A,
                   "pool": "ads", "working_directory": "/work/ads", "launched_at": 300,
                   "last_turn_at": 420, "ended": "exited", "ended_at": 450}),
        );
        // Another account's launch, newer, is not A's.
        write_record(
            root,
            "33333333-3333-4333-8333-333333333333",
            json!({"launch_id": "33333333-3333-4333-8333-333333333333", "account_id": C,
                   "pool": "default", "working_directory": "/work/c", "launched_at": 999}),
        );
        let answer = accounts(root, &snapshot());
        assert_eq!(
            answer,
            json!({"accounts": [
                {"account_id": A, "agent": "ad-manager", "machine": null,
                 "last_launch": {"pool": "ads", "folder": "/work/ads", "at": 300},
                 // The usage observation (500) is newer than the launch's end (450).
                 "last_activity": 500},
                {"account_id": B, "agent": "nightly-bot", "machine": null,
                 "last_launch": null, "last_activity": null},
            ]})
        );
    }

    #[test]
    fn the_machine_is_its_machine_group_else_its_location() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        s["accounts"][0]["pool"] = json!("machine_studio");
        s["machines"] = json!([{"pool": "machine_studio", "name": "studio", "created_at": 1}]);
        s["locations"] = json!({B: "Mac mini, Anna"});
        let by_id = assigned(tmp.path(), &s);
        assert_eq!(by_id[A]["machine"], "studio");
        assert_eq!(by_id[B]["machine"], "Mac mini, Anna");
        assert_eq!(by_id[A]["last_activity"], 500);
    }
}
