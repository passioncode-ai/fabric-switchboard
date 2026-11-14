//! SB-366 (WF-4, DEC-0017): the usage of this Switchboard's accounts that live on other machines,
//! read in the background. An account moved to another machine is in that machine's group and
//! turned off here (DEC-0013), so before this its row read "Not checked while disabled" and its
//! copy there was read only when a person asked (`machines --live`). The owner now asks each
//! machine's Switchboard for its account list over ssh every 15 minutes, one machine at a time,
//! on a thread of its own (never under the store's lock, never on the async worker), and keeps
//! the numbers in `remote-usage.json` (0600: account ids, percentages, codes and times; no label,
//! no credential). A failed read keeps the last good one with its age and a reason code; three in
//! a row back the machine off to an hour. The numbers are shown, never used to route or switch
//! here (DEC-0013 stands).

use crate::account_send::{
    data, remote, Program, Run, REMOTE_DATA_DIR_INVALID, REMOTE_INVALID, TARGET_INVALID,
    UNREACHABLE,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use switchboard_core::Snapshot;

pub const FILE: &str = "remote-usage.json";
/// How often a machine is read while its reads succeed.
pub const EVERY_SECONDS: i64 = 15 * 60;
/// After this many failures in a row a machine is read once an hour.
pub const BACKOFF_AFTER: u32 = 3;
pub const BACKOFF_SECONDS: i64 = 3600;
/// Each ssh call gives up after this.
pub const CALL_SECONDS: u64 = 20;
/// The file is a few kilobytes; anything larger is not ours.
const MAX_BYTES: u64 = 256 * 1024;

/// One read of one machine, as kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MachineRead {
    /// When the last read was tried.
    pub tried_at: i64,
    /// When the last read succeeded; the accounts below are from it.
    #[serde(default)]
    pub as_of: Option<i64>,
    /// Why the last try failed (`unreachable`, `refused`, `unreadable`), or none when it worked.
    #[serde(default)]
    pub reason: Option<String>,
    /// Failures in a row.
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub accounts: Vec<AccountRead>,
}

/// One account here as its copy there answered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountRead {
    /// The account's id here.
    pub id: String,
    #[serde(default)]
    pub used_percent: Option<f64>,
    /// Its usage check there: `ok`, `rejected`, `rate_limited` and so on.
    #[serde(default)]
    pub usage_status: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// The file: each machine group by its name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub machines: BTreeMap<String, MachineRead>,
}

pub fn read(root: &Path) -> State {
    switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn write(root: &Path, state: &State) -> bool {
    serde_json::to_vec(state).is_ok_and(|bytes| {
        switchboard_core::private_fs::private_write(&root.join(FILE), &bytes).is_ok()
    })
}

/// Whether `machine` is due for a read at `now`.
pub fn due(state: &State, machine: &str, now: i64) -> bool {
    match state.machines.get(machine) {
        None => true,
        Some(r) => {
            let wait = if r.failures >= BACKOFF_AFTER {
                BACKOFF_SECONDS
            } else {
                EVERY_SECONDS
            };
            now < r.tried_at || now - r.tried_at >= wait
        }
    }
}

/// The code a failed read is kept with.
pub fn reason_of(error: &str) -> &'static str {
    if error == UNREACHABLE || error.contains("ssh") {
        "unreachable"
    } else if [TARGET_INVALID, REMOTE_INVALID, REMOTE_DATA_DIR_INVALID].contains(&error) {
        "refused"
    } else {
        "unreadable"
    }
}

/// Records one read of `machine`: on success its accounts here matched to their copies there by
/// label and provider, as `machines --live` matches them.
pub fn record(
    state: &mut State,
    snapshot: &Snapshot,
    machine: &str,
    result: Result<Vec<Value>, String>,
    now: i64,
) {
    let entry = state.machines.entry(machine.to_owned()).or_default();
    entry.tried_at = now;
    match result {
        Ok(there) => {
            let pool = snapshot
                .machines
                .iter()
                .find(|m| m.name == machine)
                .map(|m| m.pool.clone());
            entry.accounts = snapshot
                .accounts
                .iter()
                .filter(|a| pool.as_deref() == Some(a.pool.as_str()))
                .filter_map(|a| {
                    let copy = there.iter().find(|t| {
                        t["label"].as_str() == Some(a.label.as_str())
                            && t["provider"].as_str() == Some(a.provider.as_str())
                    })?;
                    Some(AccountRead {
                        id: a.id.clone(),
                        used_percent: copy["usage"]["used_percent"].as_f64(),
                        usage_status: copy["usage_health"]["status"].as_str().map(str::to_owned),
                        enabled: copy["enabled"].as_bool(),
                    })
                })
                .collect();
            entry.as_of = Some(now);
            entry.reason = None;
            entry.failures = 0;
        }
        Err(error) => {
            entry.reason = Some(reason_of(&error).to_owned());
            entry.failures = entry.failures.saturating_add(1);
        }
    }
}

/// One machine's account list: a single `accounts list` call over ssh.
pub fn read_accounts(
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
) -> Result<Vec<Value>, String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let answer = run(
        &[
            target.to_owned(),
            "--".to_owned(),
            remote(program, &["--json", "accounts", "list"]),
        ],
        None,
    )
    .ok()
    .and_then(|out| data(&out))
    .ok_or_else(|| UNREACHABLE.to_owned())?;
    match answer {
        Value::Array(list) => Ok(list),
        Value::Object(mut o) => match o.remove("accounts") {
            Some(Value::Array(list)) => Ok(list),
            _ => Err("The other machine's account list could not be read.".into()),
        },
        _ => Err("The other machine's account list could not be read.".into()),
    }
}

/// Reads every machine group that is due, one after another, and keeps the answers. `read_one`
/// is the ssh read (a fake in tests). Answers how many machines were read and how many failed.
pub fn pass(
    root: &Path,
    snapshot: &Snapshot,
    now: i64,
    read_one: &dyn Fn(&str, &Program<'_>) -> Result<Vec<Value>, String>,
) -> (usize, usize) {
    let mut state = read(root);
    let mut tried = 0;
    let mut failed = 0;
    for machine in &snapshot.machines {
        if !due(&state, &machine.name, now) {
            continue;
        }
        let (path, data_dir) = crate::targets::program_for(root, &machine.name, None, None);
        let program = Program {
            path: &path,
            data_dir: data_dir.as_deref(),
        };
        let result = read_one(&machine.name, &program);
        tried += 1;
        if result.is_err() {
            failed += 1;
        }
        record(&mut state, snapshot, &machine.name, result, now);
    }
    // A machine group that is gone takes its reads with it.
    let names: Vec<&str> = snapshot.machines.iter().map(|m| m.name.as_str()).collect();
    state
        .machines
        .retain(|name, _| names.contains(&name.as_str()));
    if tried > 0 || state != read(root) {
        write(root, &state);
    }
    (tried, failed)
}

/// Set while a background read runs, so a pass never starts a second one.
static BUSY: AtomicBool = AtomicBool::new(false);

/// From the owner's background pass: when a machine group is due, reads them on a thread of its
/// own and returns at once. Nothing happens on a computer without machine groups.
pub fn tick(root: &Path, snapshot: &Snapshot, now: i64) {
    if snapshot.machines.is_empty() {
        return;
    }
    let state = read(root);
    if !snapshot.machines.iter().any(|m| due(&state, &m.name, now)) {
        return;
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let root = root.to_owned();
    let snapshot = snapshot.clone();
    let spawned = std::thread::Builder::new()
        .name("remote-usage".into())
        .spawn(move || {
            let (tried, failed) = pass(&root, &snapshot, now, &|target, program| {
                let run: &Run<'_> = &|args, stdin| {
                    crate::account_send::ssh_runner_for(
                        args,
                        stdin,
                        std::time::Duration::from_secs(CALL_SECONDS),
                    )
                };
                read_accounts(target, program, run)
            });
            crate::oplog::event(
                "remote_usage",
                &[
                    ("read", crate::oplog::Field::Number(tried as i64)),
                    ("failed", crate::oplog::Field::Number(failed as i64)),
                ],
            );
            BUSY.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::Release);
    }
}

/// What a row of an account on another machine shows: the last read of its copy there, with the
/// machine's last try and reason. None when no read of its machine was ever tried.
pub fn view(state: &State, machine: &str, account: &str) -> Option<Value> {
    let r = state.machines.get(machine)?;
    let copy = r.accounts.iter().find(|a| a.id == account);
    Some(serde_json::json!({
        "used_percent": copy.and_then(|c| c.used_percent),
        "usage_status": copy.and_then(|c| c.usage_status.clone()),
        "enabled": copy.and_then(|c| c.enabled),
        "as_of": r.as_of,
        "tried_at": r.tried_at,
        "reason": r.reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use switchboard_core::{Account, AuthKind, Machine, Provider};

    fn account(id: &str, label: &str, pool: &str) -> Account {
        Account {
            id: id.into(),
            label: label.into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: false,
            created_at: 1,
            identity: None,
            external_identity: None,
            usage: None,
            usage_health: None,
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            accounts: vec![
                account("a1", "work@", "machine_studio"),
                account("a2", "spare@", "machine_studio"),
                account("h1", "here@", "default"),
            ],
            machines: vec![Machine {
                pool: "machine_studio".into(),
                name: "studio".into(),
                identity: None,
                aliases: vec![],
                created_at: 1,
            }],
            ..Snapshot::default()
        }
    }
    fn there() -> Vec<Value> {
        vec![
            json!({"label": "work@", "provider": "claude", "enabled": true,
                   "usage": {"used_percent": 42.0}, "usage_health": {"status": "ok"}}),
            json!({"label": "spare@", "provider": "claude", "enabled": true,
                   "usage": {"used_percent": null}, "usage_health": {"status": "rejected"}}),
            json!({"label": "here@", "provider": "claude", "enabled": true,
                   "usage": {"used_percent": 99.0}}),
        ]
    }

    #[test]
    fn a_read_keeps_each_copy_there_and_a_failure_keeps_the_last_good_one() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        assert_eq!(pass(root.path(), &s, 1000, &|_, _| Ok(there())), (1, 0));
        let state = read(root.path());
        let studio = &state.machines["studio"];
        assert_eq!((studio.as_of, studio.failures), (Some(1000), 0));
        // Only the accounts of that machine's group, matched by label and provider.
        assert_eq!(studio.accounts.len(), 2);
        assert_eq!(studio.accounts[0].used_percent, Some(42.0));
        assert_eq!(studio.accounts[1].usage_status.as_deref(), Some("rejected"));
        // Not due again for 15 minutes: nothing is read.
        assert_eq!(
            pass(root.path(), &s, 1000 + 60, &|_, _| panic!("read")),
            (0, 0)
        );
        // A failure keeps the numbers with their time and says why.
        let later = 1000 + EVERY_SECONDS;
        assert_eq!(
            pass(root.path(), &s, later, &|_, _| Err(UNREACHABLE.into())),
            (1, 1)
        );
        let studio = read(root.path()).machines["studio"].clone();
        assert_eq!(studio.as_of, Some(1000));
        assert_eq!(studio.reason.as_deref(), Some("unreachable"));
        assert_eq!(studio.accounts[0].used_percent, Some(42.0));
        let shown = view(&read(root.path()), "studio", "a1").unwrap();
        assert_eq!(shown["used_percent"], 42.0);
        assert_eq!(shown["as_of"], 1000);
        assert_eq!(shown["reason"], "unreachable");
    }

    #[test]
    fn three_failures_in_a_row_read_a_machine_once_an_hour_and_a_gone_group_is_forgotten() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let mut now = 1000;
        for _ in 0..BACKOFF_AFTER {
            pass(root.path(), &s, now, &|_, _| Err(UNREACHABLE.into()));
            now += EVERY_SECONDS;
        }
        assert_eq!(read(root.path()).machines["studio"].failures, BACKOFF_AFTER);
        assert!(!due(&read(root.path()), "studio", now));
        let last = read(root.path()).machines["studio"].tried_at;
        assert!(due(&read(root.path()), "studio", last + BACKOFF_SECONDS));
        // One success resets the count.
        pass(root.path(), &s, last + BACKOFF_SECONDS, &|_, _| Ok(there()));
        assert_eq!(read(root.path()).machines["studio"].failures, 0);
        // The machine group is forgotten: so are its reads.
        let mut none = s.clone();
        none.machines.clear();
        pass(root.path(), &none, now + 10 * BACKOFF_SECONDS, &|_, _| {
            panic!("read")
        });
        assert!(read(root.path()).machines.is_empty());
    }

    #[test]
    fn a_failure_is_kept_as_a_code_never_as_text() {
        assert_eq!(reason_of(UNREACHABLE), "unreachable");
        assert_eq!(reason_of(TARGET_INVALID), "refused");
        assert_eq!(reason_of(REMOTE_INVALID), "refused");
        assert_eq!(
            reason_of("The other machine's account list could not be read."),
            "unreadable"
        );
    }

    #[test]
    fn nothing_starts_on_a_computer_without_machine_groups() {
        let root = tempfile::tempdir().unwrap();
        tick(root.path(), &Snapshot::default(), 1000);
        assert!(!root.path().join(FILE).exists());
        assert!(!BUSY.load(Ordering::Acquire));
    }
}
