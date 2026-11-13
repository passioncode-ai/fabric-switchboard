//! SB-355 (DEC-0013): machine groups at the runtime. Which machine an ssh name means, the
//! accounts already marked elsewhere placed in their machine's group once, and each machine's
//! live state read over ssh on request. Nothing here runs under the store's lock: `ssh -G` and
//! the reads of other machines run before or beside the store's own short writes.

use crate::account_send::{target_valid, Program, Run};
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::{is_machine_pool, MachineRef, Snapshot, Store};

/// The record of the one pass that placed the accounts already marked elsewhere (0600): when
/// it ran, and the account ids and machine names it placed or could not place. No label, no
/// location text.
pub const ADOPT_FILE: &str = "machine-groups.json";
/// A live read of one machine gives up after this, per ssh call.
pub const LIVE_SECONDS: u64 = 20;
/// Refusal: a move to the machine the account already lives on.
pub const ALREADY_THERE: &str = "This account already lives on that machine.";
/// No account to place.
pub const NOTHING_TO_PLACE: &str = "No account marked as used elsewhere is outside a machine group.";

/// One machine as this computer knows it: its names (the newest first) and `ssh -G` identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Known {
    pub names: Vec<String>,
    pub identity: Option<String>,
}
impl Known {
    fn named(&self, name: &str) -> bool {
        self.names.iter().any(|n| n.eq_ignore_ascii_case(name))
    }
    fn reference(&self, target: &str) -> MachineRef {
        MachineRef {
            target: target.to_owned(),
            identity: self.identity.clone(),
            aliases: self
                .names
                .iter()
                .filter(|n| !n.eq_ignore_ascii_case(target))
                .cloned()
                .collect(),
        }
    }
}

/// Every machine known here: the machine groups, then the machines moves and remote runs named
/// (`targets`), merged by `ssh -G` identity (`resolve`).
pub fn known(root: &Path, snapshot: &Snapshot, resolve: &dyn Fn(&str) -> Option<String>) -> Vec<Known> {
    let mut out: Vec<Known> = snapshot
        .machines
        .iter()
        .map(|m| Known {
            names: m.names().into_iter().map(str::to_owned).collect(),
            identity: m.identity.clone(),
        })
        .collect();
    for t in crate::targets::known(root) {
        let Some(name) = t["machine"].as_str() else {
            continue;
        };
        if out.iter().any(|k| k.named(name)) {
            continue;
        }
        let identity = resolve(name);
        match out
            .iter_mut()
            .find(|k| identity.is_some() && k.identity == identity)
        {
            Some(k) => k.names.push(name.to_owned()),
            None => out.push(Known {
                names: vec![name.to_owned()],
                identity,
            }),
        }
    }
    out
}

/// The machine `target` names, for a move there: its identity and the other names known for it.
pub fn reference(
    root: &Path,
    snapshot: &Snapshot,
    target: &str,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> MachineRef {
    let identity = resolve(target);
    known(root, snapshot, resolve)
        .into_iter()
        .find(|k| k.named(target) || (identity.is_some() && k.identity == identity))
        .map(|k| {
            let mut r = k.reference(target);
            if r.identity.is_none() {
                r.identity = identity.clone();
            }
            r
        })
        .unwrap_or(MachineRef {
            target: target.to_owned(),
            identity,
            aliases: Vec::new(),
        })
}

/// A location's first words as an ssh name: up to a comma, colon, semicolon or bracket, spaces
/// as hyphens, lowercase ("Studio sshlg, группа agents: …" reads `studio-sshlg`).
pub fn location_words(location: &str) -> String {
    let head = location
        .split([',', ':', ';', '(', '['])
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    head.split_whitespace().collect::<Vec<_>>().join("-")
}

/// One account the pass places, and how its machine was told.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub account: String,
    pub machine: MachineRef,
    /// `move` (the move recorded in `seats.jsonl`), `location` (its location names a known
    /// machine) or `location_words` (its location's first words do).
    pub by: &'static str,
}
/// What the pass would do: the accounts it places, and those it cannot (never guessed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub placements: Vec<Placement>,
    pub unplaced: Vec<String>,
}

/// Each account marked elsewhere and in no machine group, placed by the move recorded here that
/// was not called back, else its location naming a known machine, else its location's first
/// words naming one.
pub fn plan(root: &Path, snapshot: &Snapshot, resolve: &dyn Fn(&str) -> Option<String>) -> Plan {
    let known = known(root, snapshot, resolve);
    // A name of a known machine, or a name `ssh -G` resolves to one's identity.
    let find = |name: &str| -> Option<&Known> {
        if name.is_empty() {
            return None;
        }
        known.iter().find(|k| k.named(name)).or_else(|| {
            let identity = resolve(name).filter(|_| target_valid(name))?;
            known
                .iter()
                .find(|k| k.identity.as_deref() == Some(identity.as_str()))
        })
    };
    let mut out = Plan::default();
    for a in &snapshot.accounts {
        if !snapshot.elsewhere.contains_key(&a.id) || is_machine_pool(&a.pool) {
            continue;
        }
        let seat = crate::seats::outstanding(root, &a.id)
            .map(|s| s.machine)
            .filter(|m| target_valid(m));
        let location = snapshot.locations.get(&a.id).map(String::as_str).unwrap_or("");
        let found = if let Some(machine) = seat {
            let k = find(&machine);
            Some((
                k.map_or_else(
                    || MachineRef {
                        target: machine.clone(),
                        identity: resolve(&machine),
                        aliases: Vec::new(),
                    },
                    |k| k.reference(&machine),
                ),
                "move",
            ))
        } else if let Some(k) = find(location) {
            Some((k.reference(location), "location"))
        } else {
            find(&location_words(location)).map(|k| (k.reference(&k.names[0]), "location_words"))
        };
        match found {
            Some((machine, by)) => out.placements.push(Placement {
                account: a.id.clone(),
                machine,
                by,
            }),
            None => out.unplaced.push(a.id.clone()),
        }
    }
    out
}

/// Runs the pass: with `apply`, each placement joins its machine's group (one store write each)
/// and the record is written; without it, only what it would do. Answers both lists.
pub fn adopt(
    root: &Path,
    store: &Store,
    apply: bool,
    now: i64,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<Value, String> {
    let snapshot = store.snapshot()?;
    let plan = plan(root, &snapshot, resolve);
    let label = |id: &str| {
        snapshot
            .accounts
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.label.clone())
    };
    let mut placed = Vec::new();
    let mut failed = Vec::new();
    for p in &plan.placements {
        let mut line = json!({
            "account": p.account, "label": label(&p.account), "machine": p.machine.target,
            "by": p.by,
        });
        if apply {
            match store.join_machine(&p.account, &p.machine, now) {
                Ok(m) => {
                    line["machine"] = json!(m.name);
                    line["pool"] = json!(m.pool);
                    placed.push(line);
                }
                Err(e) => {
                    line["error"] = json!(e);
                    failed.push(line);
                }
            }
        } else {
            placed.push(line);
        }
    }
    let unplaced: Vec<Value> = plan
        .unplaced
        .iter()
        .map(|id| {
            json!({
                "account": id, "label": label(id),
                "location": snapshot.locations.get(id),
            })
        })
        .collect();
    if apply {
        let record = json!({
            "ran_at": now,
            "placed": placed.iter().map(|p| json!({"account": p["account"], "machine": p["machine"], "by": p["by"]})).collect::<Vec<_>>(),
            "failed": failed.iter().map(|p| p["account"].clone()).collect::<Vec<_>>(),
            "unplaced": plan.unplaced,
        });
        let bytes = serde_json::to_vec(&record).map_err(|_| "Machine groups record unavailable.")?;
        switchboard_core::private_fs::private_write(&root.join(ADOPT_FILE), &bytes)?;
    }
    Ok(json!({"applied": apply, "placed": placed, "failed": failed, "unplaced": unplaced}))
}

/// The owner's one pass at the first start of a version with machine groups: skipped once
/// `machine-groups.json` exists. Answers how many it placed and left, or None when skipped.
pub fn adopt_once(
    root: &Path,
    store: &Store,
    now: i64,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Option<(usize, usize)> {
    if root.join(ADOPT_FILE).exists() {
        return None;
    }
    let answer = adopt(root, store, true, now, resolve).ok()?;
    let count = |key: &str| answer[key].as_array().map_or(0, Vec::len);
    Some((count("placed"), count("unplaced") + count("failed")))
}

/// Every machine group with its accounts, and the accounts marked elsewhere that no machine
/// group holds. From the store and files here; no ssh.
pub fn list(root: &Path, snapshot: &Snapshot) -> Value {
    let account = |a: &switchboard_core::Account| {
        json!({
            "id": a.id, "label": a.label, "provider": a.provider, "kind": a.kind,
            "since": snapshot.elsewhere.get(&a.id),
            "location": snapshot.locations.get(&a.id),
            "returns_to": snapshot.returns_to.get(&a.id),
        })
    };
    let machines: Vec<Value> = snapshot
        .machines
        .iter()
        .map(|m| {
            let (program, data_dir) = crate::targets::program_for(root, &m.name, None, None);
            json!({
                "pool": m.pool, "name": m.name, "identity": m.identity, "aliases": m.aliases,
                "created_at": m.created_at,
                "program": program, "data_dir": data_dir,
                "accounts": snapshot.accounts.iter().filter(|a| a.pool == m.pool).map(account).collect::<Vec<_>>(),
            })
        })
        .collect();
    let unplaced: Vec<Value> = snapshot
        .accounts
        .iter()
        .filter(|a| snapshot.elsewhere.contains_key(&a.id) && !is_machine_pool(&a.pool))
        .map(|a| {
            let mut v = account(a);
            v["pool"] = json!(a.pool);
            v
        })
        .collect();
    json!({"machines": machines, "unplaced": unplaced})
}

/// `list`, each machine with its live state read over ssh (`remote_check::status`): all
/// machines at once, each ssh call given up after `LIVE_SECONDS`. An unreachable machine reads
/// `{"reachable": false}`; its accounts are still listed from the record here.
pub fn live(root: &Path, snapshot: &Snapshot, run: &(dyn Fn(&str, &Program<'_>) -> Result<Value, String> + Sync)) -> Value {
    let mut view = list(root, snapshot);
    let Some(machines) = view["machines"].as_array_mut() else {
        return view;
    };
    let answers: Vec<Result<Value, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = machines
            .iter()
            .map(|m| {
                let name = m["name"].as_str().unwrap_or_default().to_owned();
                let program = m["program"].as_str().unwrap_or(crate::account_send::REMOTE_DEFAULT).to_owned();
                let data_dir = m["data_dir"].as_str().map(str::to_owned);
                scope.spawn(move || {
                    run(
                        &name,
                        &Program {
                            path: &program,
                            data_dir: data_dir.as_deref(),
                        },
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("ssh failed.".into())))
            .collect()
    });
    for (m, answer) in machines.iter_mut().zip(answers) {
        m["live"] = match answer {
            Ok(there) => {
                // Each account here with its copy there, matched by label and provider.
                if let Some(accounts) = m["accounts"].as_array_mut() {
                    for a in accounts {
                        let copy = there["accounts"].as_array().and_then(|list| {
                            list.iter().find(|t| t["label"] == a["label"] && t["provider"] == a["provider"])
                        });
                        a["there"] = copy.map_or(Value::Null, |t| {
                            json!({
                                "pool": t["pool"], "enabled": t["enabled"],
                                "used_percent": t["used_percent"], "usage_status": t["usage_status"],
                            })
                        });
                    }
                }
                json!({"reachable": true, "accounts": there["accounts"], "groups": there["groups"]})
            }
            Err(error) => json!({"reachable": false, "error": error}),
        };
    }
    view
}

/// The real live read: `remote_check::status` over `ssh`, each call given up after
/// `LIVE_SECONDS`.
pub fn live_status(target: &str, program: &Program<'_>) -> Result<Value, String> {
    let run: &Run<'_> = &|args, stdin| {
        crate::account_send::ssh_runner_for(
            args,
            stdin,
            std::time::Duration::from_secs(LIVE_SECONDS),
        )
    };
    crate::remote_check::status(target, program, run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use switchboard_core::{AuthKind, Credential, MemoryVault, Provider};

    fn store() -> (tempfile::TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        (root, store)
    }
    fn save(store: &Store, label: &str) -> String {
        store
            .upsert(
                label.into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential::parse(
                    Provider::Claude,
                    AuthKind::OAuth,
                    &format!(r#"{{"claudeAiOauth":{{"accessToken":"a-{label}","refreshToken":"r-{label}"}}}}"#),
                )
                .unwrap(),
                None,
            )
            .unwrap()
            .id
    }
    /// `ssh -G` here: the two names of the operator's Studio user are one identity.
    fn resolve(name: &str) -> Option<String> {
        match name {
            "studio-sshlg" | "sshlg@100.69.20.19" => Some("sshlg@100.69.20.19:22".into()),
            "studio-appvillis" => Some("appvillis@100.69.20.19:22".into()),
            "passioncode@100.69.20.19" => Some("passioncode@100.69.20.19:22".into()),
            _ => None,
        }
    }

    #[test]
    fn location_words_read_a_typed_location_as_an_ssh_name() {
        assert_eq!(
            location_words("Studio sshlg, группа agents: выключен, вход не проходит с 2026-11-09"),
            "studio-sshlg"
        );
        assert_eq!(location_words("нигде: нет записи переноса"), "нигде");
        assert_eq!(location_words("  studio-appvillis "), "studio-appvillis");
        assert_eq!(location_words(""), "");
    }

    #[test]
    fn the_accounts_already_elsewhere_are_placed_by_move_then_location_and_never_guessed() {
        let (root, store) = store();
        let ids: Vec<String> = ["by-move", "by-location", "by-words", "nowhere", "here"]
            .iter()
            .map(|l| save(&store, l))
            .collect();
        // The operator's MacBook as it stood: a move recorded to the alias, locations in four
        // spellings.
        crate::seats::append(
            root.path(),
            &crate::seats::of(10, "granted", &ids[0], "studio-sshlg", "agents", None),
        )
        .unwrap();
        crate::seats::append(
            root.path(),
            &crate::seats::of(11, "granted", "other", "passioncode@100.69.20.19", "default", None),
        )
        .unwrap();
        store.mark_elsewhere_at(&ids[0], true, 10, Some("studio-sshlg")).unwrap();
        store.mark_elsewhere_at(&ids[1], true, 10, Some("sshlg@100.69.20.19")).unwrap();
        store
            .mark_elsewhere_at(&ids[2], true, 10, Some("Studio sshlg, группа agents: выключен"))
            .unwrap();
        store
            .mark_elsewhere_at(&ids[3], true, 10, Some("нигде: нет записи переноса"))
            .unwrap();
        let dry = adopt(root.path(), &store, false, 20, &resolve).unwrap();
        assert_eq!(dry["placed"].as_array().unwrap().len(), 3);
        let by: Vec<&str> = dry["placed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["by"].as_str().unwrap())
            .collect();
        assert_eq!(by, ["move", "location", "location_words"]);
        assert_eq!(dry["unplaced"][0]["account"], ids[3].as_str());
        assert!(store.snapshot().unwrap().machines.is_empty(), "a dry run writes nothing");
        assert!(!root.path().join(ADOPT_FILE).exists());

        assert_eq!(adopt_once(root.path(), &store, 20, &resolve), Some((3, 1)));
        let s = store.snapshot().unwrap();
        assert_eq!(s.machines.len(), 1, "three spellings, one machine");
        let m = &s.machines[0];
        assert_eq!((m.name.as_str(), m.pool.as_str()), ("studio-sshlg", "machine_studio-sshlg"));
        assert!(m.named("sshlg@100.69.20.19"));
        for id in &ids[..3] {
            assert_eq!(s.machine_of(id).map(|m| m.pool.as_str()), Some("machine_studio-sshlg"));
        }
        assert!(s.machine_of(&ids[3]).is_none() && s.machine_of(&ids[4]).is_none());
        // Once: the record stands, so a second start does nothing.
        assert_eq!(adopt_once(root.path(), &store, 30, &resolve), None);
        let record: Value =
            serde_json::from_slice(&std::fs::read(root.path().join(ADOPT_FILE)).unwrap()).unwrap();
        assert_eq!(record["unplaced"][0], ids[3].as_str());
        assert!(!String::from_utf8_lossy(&std::fs::read(root.path().join(ADOPT_FILE)).unwrap()).contains("нигде"));
        // Run by hand again it places nothing twice.
        let again = adopt(root.path(), &store, true, 40, &resolve).unwrap();
        assert!(again["placed"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_list_shows_each_machine_and_a_live_read_never_blocks_on_one_that_does_not_answer() {
        let (root, store) = store();
        let a = save(&store, "a");
        let b = save(&store, "b");
        store
            .join_machine(&a, &MachineRef { target: "studio-sshlg".into(), identity: None, aliases: vec![] }, 5)
            .unwrap();
        store
            .join_machine(&b, &MachineRef { target: "passioncode@100.69.20.19".into(), identity: None, aliases: vec![] }, 5)
            .unwrap();
        let s = store.snapshot().unwrap();
        let view = list(root.path(), &s);
        assert_eq!(view["machines"].as_array().unwrap().len(), 2);
        let started = std::time::Instant::now();
        let view = live(root.path(), &s, &|target, _program| {
            if target == "studio-sshlg" {
                Ok(json!({"accounts": [{"label": "a", "provider": "claude", "pool": "agents", "enabled": true, "used_percent": 12.0}], "groups": []}))
            } else {
                std::thread::sleep(std::time::Duration::from_millis(200));
                Err(crate::account_send::UNREACHABLE.into())
            }
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let machines = view["machines"].as_array().unwrap();
        let studio = machines.iter().find(|m| m["name"] == "studio-sshlg").unwrap();
        assert_eq!(studio["live"]["reachable"], true);
        assert_eq!(studio["accounts"][0]["there"]["pool"], "agents");
        let other = machines.iter().find(|m| m["name"] == "passioncode@100.69.20.19").unwrap();
        assert_eq!(other["live"]["reachable"], false);
        assert_eq!(other["accounts"][0]["label"], "b", "still listed from the record here");
    }
}
