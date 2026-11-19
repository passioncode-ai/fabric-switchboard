//! SB-366 (WF-4, DEC-0017), SB-550 (DEC-0033): what this Switchboard knows of the machines its
//! accounts live on, read in the background. An account moved to another machine is in that
//! machine's group and turned off here (DEC-0013), so before SB-366 its row read "Not checked while
//! disabled" and its copy there was read only when a person asked (`machines --live`).
//!
//! The owner asks each machine's own Switchboard over ssh every minute (SB-550; every 15 minutes
//! before), one machine at a time, on a thread of its own (never under the store's lock, never on
//! the async worker), with one call, `remote-status`: its accounts, its sessions as ids and codes
//! (never a folder, a label or text a session wrote) and its own health report. A Switchboard there
//! older than that call answers it as unknown; it is then read with the older calls (`accounts
//! list`, then `sessions --rows`) and asked the one call again an hour later. Sessions are never
//! contacted directly: only that machine's Switchboard is asked.
//!
//! The answers are kept in `remote-usage.json` (0600: account ids, percentages, session and launch
//! ids, codes, counts and times; no label, no folder, no credential). A failed read keeps the last
//! good one with its age and a reason code; three in a row read that machine every five minutes,
//! and Needs attention names it (`machine_unreachable`). The numbers are shown, never used to
//! route or switch here (DEC-0013 stands). The leases of seats delegated there (SB-403) are
//! extended at most every 15 minutes, as before.

use crate::account_send::{
    data, remote, Program, Run, REMOTE_DATA_DIR_INVALID, REMOTE_INVALID, TARGET_INVALID,
    UNREACHABLE,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use switchboard_core::Snapshot;

pub const FILE: &str = "remote-usage.json";
/// How often a machine is read while its reads succeed (SB-550, DEC-0033: every minute).
pub const EVERY_SECONDS: i64 = 60;
/// After this many failures in a row a machine is read less often, and Needs attention names it.
pub const BACKOFF_AFTER: u32 = 3;
/// How often a machine that stopped answering is tried.
pub const BACKOFF_SECONDS: i64 = 5 * 60;
/// How often the leases of seats delegated to a machine are extended (DEC-0025's cadence).
pub const LEASE_EVERY_SECONDS: i64 = 15 * 60;
/// After a Switchboard there answered `remote-status` as unknown, it is asked again this late.
pub const STATUS_RETRY_SECONDS: i64 = 3600;
/// Review of PR #861 (P2-3): a Switchboard there that does not know `remote-status` costs two
/// calls (one through a restricted entry), so it is read at the old cadence until it is updated.
pub const OLDER_EVERY_SECONDS: i64 = 15 * 60;
/// Review of PR #861 (P2-2): a lease extension that failed is tried again this late, not at every
/// read.
pub const LEASE_RETRY_SECONDS: i64 = 5 * 60;
/// A Switchboard there refused the read with a sentence of its own (not ssh, not an unknown call).
pub const READ_REFUSED: &str = "The other machine's Switchboard refused the read.";
/// Each ssh call gives up after this.
pub const CALL_SECONDS: u64 = 20;
/// The one call that answers a machine's accounts, sessions and health.
pub const STATUS_WORDS: [&str; 2] = ["--json", "remote-status"];
/// What `remote-status` answers (`schema`).
pub const STATUS_SCHEMA: &str = "remote-status/1";
/// Session rows kept per machine (the rows of the ended group are only counted).
pub const MAX_ROWS: usize = 100;
/// The rows that need a person on that machine: a second copy, a board run gone quiet. A gone
/// run is history there (its own health reads `sessions_gone` as ok), so it is not one.
pub const TROUBLE: [&str; 2] = ["duplicate", "stuck"];
/// The file is a few tens of kilobytes; anything larger is not ours.
const MAX_BYTES: u64 = 1024 * 1024;

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
    /// SB-550: that machine's sessions at the last read, or none when it could not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<SessionsRead>,
    /// SB-550: that machine's own health report at the last read, or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthRead>,
    /// SB-550: which call answered the last read: `status` (one call) or `accounts` (an older
    /// Switchboard there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    /// SB-550: when the Switchboard there last answered `remote-status` as unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_unknown_at: Option<i64>,
    /// SB-550: when the leases of the seats delegated there were last tried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leases_at: Option<i64>,
    /// Review of PR #861 (P2-2): that try left a seat unextended; tried again after
    /// `LEASE_RETRY_SECONDS`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lease_failed: bool,
    /// Review of PR #861 (P2-1): the machine answers through a restricted entry (`ssh-serve`) that
    /// does not know `remote-status`, so `sessions --rows`, which it refuses, is not asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restricted: bool,
    /// Review of PR #861: since when its health has failed, and since when sessions there have
    /// needed a person, as first seen by a read that worked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions_since: Option<i64>,
}

/// One account here as its copy there answered.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountRead {
    /// The account's id here.
    pub id: String,
    #[serde(default)]
    pub used_percent: Option<f64>,
    /// SB-550: when that machine last observed its usage, by its own check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<i64>,
    /// Its usage check there: `ok`, `rejected`, `rate_limited` and so on.
    #[serde(default)]
    pub usage_status: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// SB-550: a machine's sessions as its Switchboard grouped them, ids and codes only.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionsRead {
    /// Rows per group (`needs_you`, `running`, `waiting`, `ended`).
    #[serde(default)]
    pub counts: BTreeMap<String, u64>,
    /// The rows of every group but `ended`, at most `MAX_ROWS`.
    #[serde(default)]
    pub rows: Vec<SessionRead>,
    /// What that machine could not read for its rows, as codes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded: Vec<String>,
}

/// SB-550: one session row there: its id, group and why, the agent, its ids and since when.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionRead {
    pub id: String,
    pub group: String,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
}

/// SB-550: a machine's `agent-health/1` report, kept to its state and each check's state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HealthRead {
    pub state: String,
    #[serde(default)]
    pub checks: BTreeMap<String, String>,
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

/// A code another machine sent: a lowercase word of letters, digits, `_` and `-`, at most 40.
fn code(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    (!s.is_empty()
        && s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'))
    .then(|| s.to_owned())
}
/// An id another machine sent: letters, digits and `:_.-`, at most 160.
fn id(v: &Value) -> Option<String> {
    let s = v
        .as_str()
        .map(str::to_owned)
        .or_else(|| v.as_i64().map(|n| n.to_string()))?;
    (!s.is_empty()
        && s.len() <= 160
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-')))
    .then_some(s)
}

impl SessionsRead {
    /// What another machine answered as its sessions (`sessions_digest`), checked field by
    /// field: a row with a field that is not an id or a code is dropped, never kept as text.
    pub fn from_answer(answer: &Value) -> Option<SessionsRead> {
        let counts: BTreeMap<String, u64> = answer["counts"]
            .as_object()?
            .iter()
            .filter_map(|(k, v)| {
                let k = code(&json!(k))?;
                Some((k, v.as_u64()?))
            })
            .collect();
        let rows = answer["rows"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some(SessionRead {
                            id: id(&r["id"])?,
                            group: code(&r["group"])?,
                            why: code(&r["why"])?,
                            agent: code(&r["agent"]),
                            launch_id: id(&r["launch_id"]),
                            session_id: id(&r["session_id"]),
                            since: r["since"].as_i64(),
                        })
                    })
                    .filter(|r| r.group != "ended")
                    .take(MAX_ROWS)
                    .collect()
            })
            .unwrap_or_default();
        let degraded = answer["degraded"]
            .as_array()
            .map(|d| d.iter().filter_map(code).collect())
            .unwrap_or_default();
        Some(SessionsRead {
            counts,
            rows,
            degraded,
        })
    }
}

impl HealthRead {
    /// Another machine's `agent-health/1` report, kept to its state and each check's state.
    pub fn from_answer(answer: &Value) -> Option<HealthRead> {
        if answer["schema"] != crate::health::SCHEMA {
            return None;
        }
        let checks = answer["checks"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(|c| Some((code(&c["id"])?, code(&c["state"])?)))
                    .collect()
            })
            .unwrap_or_default();
        Some(HealthRead {
            state: code(&answer["state"]).unwrap_or_else(|| "unknown".into()),
            checks,
        })
    }
}

impl MachineRead {
    /// Whether this machine stopped answering: Needs attention names it.
    pub fn unreachable(&self) -> bool {
        self.failures >= BACKOFF_AFTER
    }
    /// The session rows there that need a person (a second copy, a board run gone quiet), from
    /// the last read that worked; kept through one or two failed reads, so the item does not come
    /// and go, and dropped once the machine stopped answering (`machine_unreachable` speaks).
    pub fn troubled_sessions(&self) -> Vec<&SessionRead> {
        if self.unreachable() {
            return Vec::new();
        }
        self.sessions
            .iter()
            .flat_map(|s| s.rows.iter())
            .filter(|r| r.group == "needs_you" && TROUBLE.contains(&r.why.as_str()))
            .collect()
    }
    /// The health checks there that fail, and its owner when it is not running, from the last
    /// read that worked, kept as `troubled_sessions` keeps its rows. A warning (an account to sign
    /// in to, an error storm) is that machine's own Needs attention.
    pub fn failing_checks(&self) -> Vec<String> {
        if self.unreachable() {
            return Vec::new();
        }
        self.health
            .iter()
            .flat_map(|h| h.checks.iter())
            .filter(|(id, state)| *state == "fail" || (*id == "owner" && *state != "ok"))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Whether `machine` is due for a read at `now`.
pub fn due(state: &State, machine: &str, now: i64) -> bool {
    match state.machines.get(machine) {
        None => true,
        Some(r) => {
            let older = r
                .status_unknown_at
                .is_some_and(|at| now >= at && now - at < STATUS_RETRY_SECONDS);
            let wait = if r.failures >= BACKOFF_AFTER {
                BACKOFF_SECONDS
            } else if older {
                OLDER_EVERY_SECONDS
            } else {
                EVERY_SECONDS
            };
            now < r.tried_at || now - r.tried_at >= wait
        }
    }
}

/// What one read asks: the one call, and with the older calls whether `sessions --rows` too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ask {
    pub status: bool,
    pub sessions: bool,
}

/// What `machine` is asked this time: the one call unless its Switchboard answered it as unknown
/// within the hour, and `sessions --rows` unless it answers through a restricted entry.
fn ask(state: &State, machine: &str, now: i64) -> Ask {
    let r = state.machines.get(machine);
    Ask {
        status: r
            .and_then(|r| r.status_unknown_at)
            .is_none_or(|at| now < at || now - at >= STATUS_RETRY_SECONDS),
        sessions: !r.is_some_and(|r| r.restricted),
    }
}

/// Whether the leases of the seats delegated to `machine` are due for an extension.
fn leases_due(state: &State, machine: &str, now: i64) -> bool {
    state
        .machines
        .get(machine)
        .and_then(|r| r.leases_at.map(|at| (at, r.lease_failed)))
        .is_none_or(|(at, failed)| {
            let wait = if failed {
                LEASE_RETRY_SECONDS
            } else {
                LEASE_EVERY_SECONDS
            };
            now < at || now - at >= wait
        })
}

/// The code a failed read is kept with.
pub fn reason_of(error: &str) -> &'static str {
    // A refused target or path is named first: its sentence may itself mention ssh.
    if [TARGET_INVALID, REMOTE_INVALID, REMOTE_DATA_DIR_INVALID, READ_REFUSED].contains(&error) {
        "refused"
    } else if error == UNREACHABLE || error.contains("ssh") {
        "unreachable"
    } else {
        "unreadable"
    }
}

/// What one read of a machine answered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Remote {
    /// Its account list, as `accounts list` or `remote-status` answers it.
    pub accounts: Vec<Value>,
    /// Its sessions (`sessions_digest`), or none.
    pub sessions: Option<Value>,
    /// Its health report, or none.
    pub health: Option<Value>,
    /// `status` or `accounts`.
    pub call: &'static str,
    /// The Switchboard there answered `remote-status` as unknown this time.
    pub status_unknown: bool,
    /// That answer came from a restricted entry (`ssh-serve`).
    pub restricted: bool,
}

/// Records one read of `machine`: on success its accounts here matched to their copies there by
/// label and provider, as `machines --live` matches them, with its sessions and health.
pub fn record(
    state: &mut State,
    snapshot: &Snapshot,
    machine: &str,
    result: Result<Remote, String>,
    now: i64,
) {
    let entry = state.machines.entry(machine.to_owned()).or_default();
    entry.tried_at = now;
    match result {
        Ok(remote) => {
            let pool = snapshot
                .machines
                .iter()
                .find(|m| m.name == machine)
                .map(|m| m.pool.clone());
            let there = &remote.accounts;
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
                        observed_at: copy["usage"]["observed_at"].as_i64(),
                        usage_status: copy["usage_health"]["status"].as_str().map(str::to_owned),
                        enabled: copy["enabled"].as_bool(),
                    })
                })
                .collect();
            entry.sessions = remote.sessions.as_ref().and_then(SessionsRead::from_answer);
            entry.health = remote.health.as_ref().and_then(HealthRead::from_answer);
            entry.call = Some(remote.call.to_owned());
            if remote.status_unknown {
                entry.status_unknown_at = Some(now);
                entry.restricted = remote.restricted;
            } else if remote.call == "status" {
                entry.status_unknown_at = None;
                entry.restricted = false;
            }
            entry.as_of = Some(now);
            entry.reason = None;
            entry.failures = 0;
            // When each condition started, as first seen by a read that worked.
            entry.health_since = (!entry.failing_checks().is_empty())
                .then(|| entry.health_since.unwrap_or(now));
            entry.sessions_since = (!entry.troubled_sessions().is_empty())
                .then(|| entry.sessions_since.unwrap_or(now));
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
    account_list(answer)
}

fn account_list(answer: Value) -> Result<Vec<Value>, String> {
    match answer {
        Value::Array(list) => Ok(list),
        Value::Object(mut o) => match o.remove("accounts") {
            Some(Value::Array(list)) => Ok(list),
            _ => Err("The other machine's account list could not be read.".into()),
        },
        _ => Err("The other machine's account list could not be read.".into()),
    }
}

/// Whether a refusal says the Switchboard there does not know `remote-status`: an older one
/// answers "Invalid arguments", an older restricted entry (`ssh-serve`) that it answers only for
/// delegated seats.
fn unknown_call(error: &str) -> bool {
    error.contains("Invalid arguments. Run switchboard --help.") || restricted(error)
}
fn restricted(error: &str) -> bool {
    error.contains("only for delegated seats")
}

/// SB-550: one read of a machine. With `ask.status`, the one call `remote-status`; a Switchboard
/// there that does not know it is read with the older calls in the same read: `accounts list`,
/// then `sessions --rows` unless it answers through a restricted entry, which refuses it (seen in
/// this answer, or `ask.sessions` false from an earlier one).
pub fn read_remote(
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
    ask: Ask,
) -> Result<Remote, String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let mut status_unknown = false;
    let mut restricted_entry = !ask.sessions;
    if ask.status {
        match run(
            &[
                target.to_owned(),
                "--".to_owned(),
                remote(program, &STATUS_WORDS),
            ],
            None,
        ) {
            Ok(out) => {
                let answer = data(&out).ok_or_else(|| UNREACHABLE.to_owned())?;
                let accounts = account_list(answer["accounts"].clone())?;
                let object = |v: &Value| v.is_object().then(|| v.clone());
                return Ok(Remote {
                    accounts,
                    sessions: object(&answer["sessions"]),
                    health: object(&answer["health"]),
                    call: "status",
                    status_unknown: false,
                    restricted: false,
                });
            }
            Err(error) if unknown_call(&error) => {
                status_unknown = true;
                restricted_entry = restricted(&error);
            }
            // A refusal in the Switchboard's own words (`{"ok":false,...}`) is not a lost ssh.
            Err(error) if error.trim_start().starts_with('{') => return Err(READ_REFUSED.into()),
            Err(_) => return Err(UNREACHABLE.into()),
        }
    }
    let accounts = read_accounts(target, program, run)?;
    let sessions = if restricted_entry {
        None
    } else {
        run(
            &[
                target.to_owned(),
                "--".to_owned(),
                remote(program, &["--json", "sessions", "--rows"]),
            ],
            None,
        )
        .ok()
        .and_then(|out| data(&out))
        .filter(|answer| answer["rows"].is_array())
        .map(|answer| sessions_digest(&answer, None))
    };
    Ok(Remote {
        accounts,
        sessions,
        health: None,
        call: "accounts",
        status_unknown,
        restricted: restricted_entry,
    })
}

/// SB-550: a session rows answer (`sessions --rows`) cut to what another machine may keep: per
/// row its id, group, why, agent, launch and session ids and since when; never its folder, its
/// account's label, its group or its task. With `pool`, only the rows of that group (a restricted
/// entry answers only for the group its key line names). Every group is counted; the rows of the
/// ended group are not sent.
pub fn sessions_digest(answer: &Value, pool: Option<&str>) -> Value {
    let rows: Vec<&Value> = answer["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| pool.is_none_or(|p| r["pool"].as_str() == Some(p)))
                .collect()
        })
        .unwrap_or_default();
    let mut counts = serde_json::Map::new();
    for g in crate::session_rows::GROUPS {
        counts.insert(
            g.into(),
            json!(rows.iter().filter(|r| r["group"] == g).count()),
        );
    }
    let kept: Vec<Value> = rows
        .iter()
        .filter(|r| r["group"] != "ended")
        .take(MAX_ROWS)
        .map(|r| {
            json!({"id": r["id"], "group": r["group"], "why": r["why"], "agent": r["agent"],
                "launch_id": r["launch_id"], "session_id": r["session_id"], "since": r["since"]})
        })
        .collect();
    let degraded: Vec<Value> = answer["degraded"]
        .as_array()
        .map(|d| d.iter().filter(|v| code(v).is_some()).cloned().collect())
        .unwrap_or_default();
    json!({"counts": counts, "rows": kept, "degraded": degraded})
}

/// SB-550: an account list cut to what another machine matches and shows: label and provider (to
/// match it to its account there), whether it is on, its used share with when it was observed,
/// and its usage check's status.
pub fn accounts_answer(accounts: &Value) -> Value {
    let list: Vec<Value> = accounts
        .as_array()
        .map(|list| {
            list.iter()
                .map(|a| {
                    json!({"label": a["label"], "provider": a["provider"], "enabled": a["enabled"],
                        "usage": {"used_percent": a["usage"]["used_percent"],
                            "observed_at": a["usage"]["observed_at"]},
                        "usage_health": {"status": a["usage_health"]["status"]}})
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(list)
}

/// SB-403: the seats this machine delegated to `machine` that are still there: accounts in that
/// machine's group whose outstanding move carries a lease, with the id there and the lease.
pub fn delegated_to(root: &Path, snapshot: &Snapshot, machine: &str) -> Vec<(String, u32, String)> {
    let Some(pool) = snapshot
        .machines
        .iter()
        .find(|m| m.name == machine)
        .map(|m| m.pool.clone())
    else {
        return Vec::new();
    };
    snapshot
        .accounts
        .iter()
        .filter(|a| a.pool == pool)
        .filter_map(|a| {
            // The store's record (review P2-5), else the move's `seats.jsonl` line.
            if let Some(o) = snapshot.delegated_out.get(&a.id) {
                return Some((o.remote_id.clone(), o.lease_hours, o.sender_key.clone()));
            }
            let seat = crate::seats::outstanding(root, &a.id)?;
            Some((seat.remote_account?, seat.lease_hours?, seat.sender_key?))
        })
        .collect()
}

/// SB-403: extends each seat delegated to `machine` to its lease from now. Answers how many were
/// extended and how many were not; a failure is logged and tried again on the next read.
pub fn extend_leases(
    root: &Path,
    snapshot: &Snapshot,
    machine: &str,
    program: &Program<'_>,
    run: &Run<'_>,
) -> (usize, usize) {
    let seats = delegated_to(root, snapshot, machine);
    if seats.is_empty() {
        return (0, 0);
    }
    let mut extended = 0;
    let mut failed = 0;
    for (remote_id, hours, key) in &seats {
        match crate::account_send::extend_lease(machine, remote_id, *hours, key, program, run) {
            Ok(_) => extended += 1,
            Err(_) => failed += 1,
        }
    }
    crate::oplog::event(
        "delegation_lease",
        &[
            ("extended", crate::oplog::Field::Number(extended as i64)),
            ("failed", crate::oplog::Field::Number(failed as i64)),
        ],
    );
    (extended, failed)
}

/// One machine's read as `read_remote` answers it (a fake in tests), asked as `Ask` says.
pub type ReadOne<'a> = dyn Fn(&str, &Program<'_>, Ask) -> Result<Remote, String> + 'a;
/// Extends the leases of the seats delegated to a machine (`extend_leases`; a fake in tests).
pub type Extend<'a> = dyn Fn(&str, &Program<'_>) -> (usize, usize) + 'a;

/// Reads every machine group that is due, one after another, and keeps the answers. A machine
/// that answered gets its delegated seats' leases extended when they are due (every 15 minutes),
/// so a seat lapses only after its lease of silence. Answers how many machines were read and how
/// many failed.
pub fn pass(
    root: &Path,
    snapshot: &Snapshot,
    now: i64,
    read_one: &ReadOne<'_>,
    extend: &Extend<'_>,
) -> (usize, usize) {
    let mut state = read(root);
    let before = state.clone();
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
        let result = read_one(&machine.name, &program, ask(&state, &machine.name, now));
        tried += 1;
        let answered = result.is_ok();
        if !answered {
            failed += 1;
        }
        let lease = answered && leases_due(&state, &machine.name, now);
        record(&mut state, snapshot, &machine.name, result, now);
        // SB-403 (DEC-0025): extended at most every 15 minutes; a failure is tried again after
        // five (review of PR #861, P2-2), never at every read.
        if lease {
            let failed = extend(&machine.name, &program).1 > 0;
            if let Some(entry) = state.machines.get_mut(&machine.name) {
                entry.leases_at = Some(now);
                entry.lease_failed = failed;
            }
        }
    }
    // A machine group that is gone takes its reads with it.
    let names: Vec<&str> = snapshot.machines.iter().map(|m| m.name.as_str()).collect();
    state
        .machines
        .retain(|name, _| names.contains(&name.as_str()));
    if tried > 0 || state != before {
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
            let run: &Run<'_> = &|args, stdin| {
                crate::account_send::ssh_runner_for(
                    args,
                    stdin,
                    std::time::Duration::from_secs(CALL_SECONDS),
                )
            };
            let (tried, failed) = pass(
                &root,
                &snapshot,
                now,
                &|target, program, ask| read_remote(target, program, run, ask),
                &|target, program| extend_leases(&root, &snapshot, target, program, run),
            );
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
    Some(json!({
        // Review of PR #861 (P2-4): whether its copy was in that machine's answer at all.
        "listed": r.as_of.is_some() && copy.is_some(),
        "used_percent": copy.and_then(|c| c.used_percent),
        "observed_at": copy.and_then(|c| c.observed_at),
        "usage_status": copy.and_then(|c| c.usage_status.clone()),
        "enabled": copy.and_then(|c| c.enabled),
        "as_of": r.as_of,
        "tried_at": r.tried_at,
        "reason": r.reason,
    }))
}

/// SB-550: a machine's last read as `machines list` shows it: its time and reason, which call
/// answered, its sessions' counts and the rows that need a person, and its health.
pub fn machine_view(r: &MachineRead) -> Value {
    let troubled: Vec<Value> = r
        .troubled_sessions()
        .iter()
        .map(|s| json!({"why": s.why, "session_id": s.session_id, "launch_id": s.launch_id, "since": s.since}))
        .collect();
    json!({
        "as_of": r.as_of, "tried_at": r.tried_at, "reason": r.reason, "failures": r.failures,
        "health_since": r.health_since, "sessions_since": r.sessions_since,
        "call": r.call,
        "sessions": r.sessions.as_ref().map(|s| json!({"counts": s.counts, "troubled": troubled, "degraded": s.degraded})),
        "health": r.health.as_ref().map(|h| json!({"state": h.state, "failing": r.failing_checks()})),
    })
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
                   "usage": {"used_percent": 42.0, "observed_at": 990}, "usage_health": {"status": "ok"}}),
            json!({"label": "spare@", "provider": "claude", "enabled": true,
                   "usage": {"used_percent": null}, "usage_health": {"status": "rejected"}}),
            json!({"label": "here@", "provider": "claude", "enabled": true,
                   "usage": {"used_percent": 99.0}}),
        ]
    }

    #[test]
    fn the_stores_record_of_a_delegated_seat_outlives_seats_jsonl() {
        // SB-403 (review P2-5): a rotated or rewritten `seats.jsonl` loses nothing.
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        s.delegated_out.insert(
            "a2".into(),
            switchboard_core::DelegatedOut {
                remote_id: "0b1edeee-4125-403c-9c12-44c6aadaeb2b".into(),
                lease_hours: 24,
                sender_key: "KEY".into(),
                sent: None,
            },
        );
        assert_eq!(
            delegated_to(root.path(), &s, "studio"),
            vec![(
                "0b1edeee-4125-403c-9c12-44c6aadaeb2b".to_owned(),
                24,
                "KEY".to_owned()
            )]
        );
    }

    #[test]
    fn a_read_extends_the_leases_of_the_seats_delegated_there_and_only_those() {
        // SB-403: a1 was delegated with a 48-hour lease, a2 moved as before (no lease).
        let root = tempfile::tempdir().unwrap();
        let mut delegated = crate::seats::of(1, "granted", "a1", "studio", "org", None);
        delegated.remote_account = Some("r1".into());
        delegated.lease_hours = Some(48);
        delegated.sender_key = Some("KEY".into());
        crate::seats::append(root.path(), &delegated).unwrap();
        let mut moved = crate::seats::of(1, "granted", "a2", "studio", "org", None);
        moved.remote_account = Some("r2".into());
        crate::seats::append(root.path(), &moved).unwrap();
        let s = snapshot();
        assert_eq!(
            delegated_to(root.path(), &s, "studio"),
            vec![("r1".to_owned(), 48, "KEY".to_owned())]
        );
        let calls = std::sync::Mutex::new(Vec::new());
        let program = Program {
            path: "/usr/local/bin/switchboard",
            data_dir: None,
        };
        let run = |args: &[String], _: Option<&[u8]>| {
            calls.lock().unwrap().push(args[2].clone());
            Ok(r#"{"ok":true,"data":{"id":"r1","lease_until":1}}"#.to_owned())
        };
        assert_eq!(
            extend_leases(root.path(), &s, "studio", &program, &run),
            (1, 0)
        );
        let calls = calls.into_inner().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].contains("'accounts' 'lease' 'r1' '--hours' '48' '--sender-key' 'KEY'"),
            "{calls:?}"
        );
        // A refusal there is counted, never fatal.
        let refuse = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err(r#"{"ok":false,"error":"no"}"#.into())
        };
        assert_eq!(
            extend_leases(root.path(), &s, "studio", &program, &refuse),
            (0, 1)
        );
    }

    fn ok(accounts: Vec<Value>) -> Result<Remote, String> {
        Ok(Remote {
            accounts,
            call: "status",
            ..Remote::default()
        })
    }
    fn no_lease(_: &str, _: &Program<'_>) -> (usize, usize) {
        (0, 0)
    }
    const FIRST: Ask = Ask {
        status: true,
        sessions: true,
    };

    #[test]
    fn a_read_keeps_each_copy_there_and_a_failure_keeps_the_last_good_one() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        assert_eq!(
            pass(root.path(), &s, 1000, &|_, _, _| ok(there()), &no_lease),
            (1, 0)
        );
        let state = read(root.path());
        let studio = &state.machines["studio"];
        assert_eq!((studio.as_of, studio.failures), (Some(1000), 0));
        // Only the accounts of that machine's group, matched by label and provider.
        assert_eq!(studio.accounts.len(), 2);
        assert_eq!(studio.accounts[0].used_percent, Some(42.0));
        assert_eq!(studio.accounts[0].observed_at, Some(990));
        assert_eq!(studio.accounts[1].usage_status.as_deref(), Some("rejected"));
        // SB-550: not due again within the minute: nothing is read.
        assert_eq!(
            pass(
                root.path(),
                &s,
                1000 + 30,
                &|_, _, _| panic!("read"),
                &no_lease
            ),
            (0, 0)
        );
        // A failure keeps the numbers with their time and says why.
        let later = 1000 + EVERY_SECONDS;
        assert_eq!(EVERY_SECONDS, 60, "DEC-0033: every minute");
        assert_eq!(
            pass(
                root.path(),
                &s,
                later,
                &|_, _, _| Err(UNREACHABLE.into()),
                &no_lease
            ),
            (1, 1)
        );
        let studio = read(root.path()).machines["studio"].clone();
        assert_eq!(studio.as_of, Some(1000));
        assert_eq!(studio.reason.as_deref(), Some("unreachable"));
        assert_eq!(studio.accounts[0].used_percent, Some(42.0));
        let shown = view(&read(root.path()), "studio", "a1").unwrap();
        assert_eq!(shown["used_percent"], 42.0);
        assert_eq!(shown["observed_at"], 990);
        assert_eq!(shown["as_of"], 1000);
        assert_eq!(shown["reason"], "unreachable");
    }

    #[test]
    fn three_failures_in_a_row_slow_the_reads_and_a_gone_group_is_forgotten() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let mut now = 1000;
        let fail = |_: &str, _: &Program<'_>, _: bool| -> Result<Remote, String> {
            Err(UNREACHABLE.into())
        };
        for _ in 0..BACKOFF_AFTER {
            pass(root.path(), &s, now, &fail, &no_lease);
            now += EVERY_SECONDS;
        }
        let state = read(root.path());
        assert_eq!(state.machines["studio"].failures, BACKOFF_AFTER);
        assert!(state.machines["studio"].unreachable());
        assert!(!due(&state, "studio", now));
        let last = state.machines["studio"].tried_at;
        assert!(due(&state, "studio", last + BACKOFF_SECONDS));
        // One success resets the count.
        pass(
            root.path(),
            &s,
            last + BACKOFF_SECONDS,
            &|_, _, _| ok(there()),
            &no_lease,
        );
        assert_eq!(read(root.path()).machines["studio"].failures, 0);
        // The machine group is forgotten: so are its reads.
        let mut none = s.clone();
        none.machines.clear();
        pass(
            root.path(),
            &none,
            now + 10 * BACKOFF_SECONDS,
            &|_, _, _| panic!("read"),
            &no_lease,
        );
        assert!(read(root.path()).machines.is_empty());
    }

    /// SB-550: the one call answers sessions and health; only ids, codes and counts are kept, and
    /// the rows that need a person and the failing checks are named.
    #[test]
    fn a_status_read_keeps_sessions_and_health_as_ids_and_codes_only() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let rows = json!({"rows": [
            {"id": "pid:41", "group": "needs_you", "why": "stuck", "agent": "claude-code",
             "launch_id": "L-1", "session_id": "s-1", "since": 900, "pool": "apps",
             "folder": "/Users/someone/secret", "account": "work@example.com"},
            {"id": "pid:42", "group": "running", "why": "working", "agent": "codex", "since": 950,
             "folder": "/Users/someone/other"},
            {"id": "launch:L-9", "group": "ended", "why": "exited", "since": 10},
            {"id": "bad id with spaces", "group": "running", "why": "working"},
            {"id": "pid:43", "group": "running", "why": "Some text a session wrote"}
        ], "degraded": ["recovery", "/a/path"]});
        let digest = sessions_digest(&rows, None);
        let text = digest.to_string();
        for planted in ["/Users/someone", "work@example.com", "apps"] {
            assert!(!text.contains(planted), "{planted} left the machine");
        }
        assert_eq!(digest["counts"]["running"], 3);
        assert_eq!(digest["counts"]["ended"], 1);
        assert_eq!(digest["degraded"], json!(["recovery"]));
        let health = json!({"schema": "agent-health/1", "state": "fail", "checks": [
            {"id": "owner", "state": "warn"}, {"id": "crashes", "state": "fail"},
            {"id": "sign_in", "state": "warn"}, {"id": "errors_today", "state": "ok"}]});
        let read_one = |_: &str, _: &Program<'_>, ask: Ask| {
            assert!(ask.status, "the one call is asked first");
            Ok(Remote {
                accounts: there(),
                sessions: Some(digest.clone()),
                health: Some(health.clone()),
                call: "status",
                ..Remote::default()
            })
        };
        pass(root.path(), &s, 1000, &read_one, &no_lease);
        let file = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        for planted in ["/Users/someone", "work@example.com", "Some text", "spaces"] {
            assert!(!file.contains(planted), "{planted} was kept");
        }
        let studio = read(root.path()).machines["studio"].clone();
        let sessions = studio.sessions.as_ref().unwrap();
        // The ended row is counted, not kept; the malformed rows are dropped.
        assert_eq!(sessions.rows.len(), 2);
        assert_eq!(sessions.counts["ended"], 1);
        let troubled = studio.troubled_sessions();
        assert_eq!(troubled.len(), 1);
        assert_eq!(troubled[0].session_id.as_deref(), Some("s-1"));
        assert_eq!(studio.failing_checks(), vec!["crashes", "owner"]);
        assert_eq!(studio.call.as_deref(), Some("status"));
        let shown = machine_view(&studio);
        assert_eq!(shown["sessions"]["troubled"][0]["why"], "stuck");
        assert_eq!(shown["health"]["state"], "fail");
        // A failed read keeps them, but names nothing as needing a person from a stale read.
        pass(
            root.path(),
            &s,
            1000 + EVERY_SECONDS,
            &|_, _, _| Err(UNREACHABLE.into()),
            &no_lease,
        );
        // Review of PR #861: one or two failed reads keep the last good items, so they do not come
        // and go; the time each condition started is kept.
        let studio = read(root.path()).machines["studio"].clone();
        assert!(studio.sessions.is_some());
        assert_eq!(studio.troubled_sessions().len(), 1);
        assert_eq!(studio.failing_checks(), vec!["crashes", "owner"]);
        assert_eq!((studio.health_since, studio.sessions_since), (Some(1000), Some(1000)));
        pass(
            root.path(),
            &s,
            1000 + 2 * EVERY_SECONDS,
            &read_one,
            &no_lease,
        );
        let studio = read(root.path()).machines["studio"].clone();
        assert_eq!(
            (studio.health_since, studio.sessions_since),
            (Some(1000), Some(1000)),
            "a later read that sees the same trouble keeps when it started"
        );
        // Once the machine stopped answering, only `machine_unreachable` speaks.
        for k in 3..6 {
            pass(
                root.path(),
                &s,
                1000 + k * BACKOFF_SECONDS,
                &|_, _, _| Err(UNREACHABLE.into()),
                &no_lease,
            );
        }
        let studio = read(root.path()).machines["studio"].clone();
        assert!(studio.unreachable());
        assert!(studio.troubled_sessions().is_empty());
        assert!(studio.failing_checks().is_empty());
        // A read without the trouble clears when it started.
        pass(
            root.path(),
            &s,
            1000 + 10 * BACKOFF_SECONDS,
            &|_, _, _| ok(there()),
            &no_lease,
        );
        let studio = read(root.path()).machines["studio"].clone();
        assert_eq!((studio.health_since, studio.sessions_since), (None, None));
        assert!(!TROUBLE.contains(&"gone"), "a gone run is history there");
        // A restricted entry answers the group's rows only.
        let only = sessions_digest(&rows, Some("apps"));
        assert_eq!(only["rows"].as_array().unwrap().len(), 1);
        assert_eq!(only["counts"]["running"], 0);
    }

    /// SB-550: an older Switchboard there does not know the one call; it is read with the older
    /// calls in the same read, and asked the one call again an hour later. An older restricted
    /// entry is not asked `sessions --rows`, which it refuses.
    #[test]
    fn an_older_switchboard_is_read_with_the_older_calls() {
        let program = Program {
            path: "/usr/local/bin/switchboard",
            data_dir: None,
        };
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let older = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            calls.lock().unwrap().push(args[2].clone());
            if args[2].contains("remote-status") {
                Err(r#"{"ok":false,"error":"Invalid arguments. Run switchboard --help."}"#.into())
            } else if args[2].contains("'sessions'") {
                Ok(json!({"ok": true, "data": {"rows": [
                    {"id": "pid:1", "group": "running", "why": "working", "folder": "/x/y"}],
                    "degraded": []}})
                .to_string())
            } else {
                Ok(json!({"ok": true, "data": {"accounts": there()}}).to_string())
            }
        };
        let remote = read_remote("studio", &program, &older, FIRST).unwrap();
        assert!(remote.status_unknown);
        assert_eq!(remote.call, "accounts");
        assert_eq!(remote.accounts.len(), 3);
        assert!(!remote
            .sessions
            .as_ref()
            .unwrap()
            .to_string()
            .contains("/x/y"));
        assert_eq!(calls.lock().unwrap().len(), 3);
        // Within the hour it is not asked the one call again.
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        // Review of PR #861 (P2-3): through `pass` and `read_remote`, an older Switchboard is read
        // at the old cadence (every 15 minutes), with the older calls, and asked the one call
        // again after an hour.
        calls.lock().unwrap().clear();
        let read_one =
            |target: &str, program: &Program<'_>, ask: Ask| read_remote(target, program, &older, ask);
        let mut now = 1000;
        for _ in 0..16 {
            pass(root.path(), &s, now, &read_one, &no_lease);
            now += EVERY_SECONDS;
        }
        let made = calls.lock().unwrap().clone();
        assert_eq!(
            made.iter().filter(|c| c.contains("remote-status")).count(),
            1,
            "{made:?}"
        );
        assert_eq!(made.len(), 1 + 2 * 2, "two reads in 16 minutes, two calls each: {made:?}");
        calls.lock().unwrap().clear();
        pass(root.path(), &s, 1000 + STATUS_RETRY_SECONDS, &read_one, &no_lease);
        assert!(calls.lock().unwrap()[0].contains("remote-status"), "asked again after the hour");
        // An older restricted entry (`ssh-serve`) refuses the one call by its own sentence.
        calls.lock().unwrap().clear();
        let entry = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            calls.lock().unwrap().push(args[2].clone());
            if args[2].contains("remote-status") {
                Err(r#"{"ok":false,"error":"This machine answers another machine only for delegated seats: moving one here, reading, extending and calling it back. Nothing was run."}"#.into())
            } else {
                Ok(json!({"ok": true, "data": {"accounts": there()}}).to_string())
            }
        };
        let remote = read_remote("studio", &program, &entry, FIRST).unwrap();
        assert!(remote.status_unknown && remote.restricted && remote.sessions.is_none());
        assert_eq!(calls.lock().unwrap().len(), 2);
        // Review of PR #861 (P2-1): the next reads know it is a restricted entry and never ask it
        // `sessions --rows`, which it refuses.
        let root = tempfile::tempdir().unwrap();
        calls.lock().unwrap().clear();
        let through_entry =
            |target: &str, program: &Program<'_>, ask: Ask| read_remote(target, program, &entry, ask);
        let mut now = 1000;
        for _ in 0..3 {
            pass(root.path(), &s, now, &through_entry, &no_lease);
            now += OLDER_EVERY_SECONDS;
        }
        let made = calls.lock().unwrap().clone();
        assert!(read(root.path()).machines["studio"].restricted);
        assert!(!made.iter().any(|c| c.contains("'sessions'")), "{made:?}");
        assert_eq!(made.len(), 1 + 3, "{made:?}");
        // A newer one answers everything in one call.
        let newer = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            assert!(args[2].contains("'remote-status'"));
            Ok(
                json!({"ok": true, "data": {"schema": STATUS_SCHEMA, "accounts": there(),
                "sessions": {"counts": {"running": 0}, "rows": []},
                "health": {"schema": "agent-health/1", "state": "ok", "checks": []}}})
                .to_string(),
            )
        };
        let remote = read_remote("studio", &program, &newer, FIRST).unwrap();
        assert_eq!((remote.call, remote.status_unknown), ("status", false));
        assert!(remote.sessions.is_some() && remote.health.is_some());
        // A machine that does not answer is unreachable, whatever the call.
        let down = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err("ssh failed.".into())
        };
        assert_eq!(
            read_remote("studio", &program, &down, FIRST).unwrap_err(),
            UNREACHABLE
        );
        // A newer one refusing in its own words is not a machine that did not answer.
        let refuses = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err(r#"{"ok":false,"error":"Switchboard could not read its own state."}"#.into())
        };
        let error = read_remote("studio", &program, &refuses, FIRST).unwrap_err();
        assert_eq!(reason_of(&error), "refused");
    }

    /// SB-550: reads every minute, but the leases of seats delegated there are extended every 15
    /// minutes as before, and again the next minute when an extension failed.
    #[test]
    fn leases_are_extended_every_fifteen_minutes_not_every_read() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let extended = std::sync::Mutex::new(Vec::new());
        let failing = std::sync::atomic::AtomicBool::new(false);
        let extend = |_: &str, _: &Program<'_>| {
            let now_failing = failing.load(Ordering::Relaxed);
            extended.lock().unwrap().push(now_failing);
            if now_failing {
                (0, 1)
            } else {
                (1, 0)
            }
        };
        let mut now = 1000;
        for _ in 0..16 {
            pass(root.path(), &s, now, &|_, _, _| ok(there()), &extend);
            now += EVERY_SECONDS;
        }
        // At the first read and fifteen minutes later.
        assert_eq!(extended.lock().unwrap().len(), 2);
        failing.store(true, Ordering::Relaxed);
        now = 1000 + 2 * LEASE_EVERY_SECONDS;
        for k in 0..5 {
            pass(
                root.path(),
                &s,
                now + k * EVERY_SECONDS,
                &|_, _, _| ok(there()),
                &extend,
            );
        }
        // Review of PR #861 (P2-2): a failed extension is not tried at every read...
        assert_eq!(extended.lock().unwrap().len(), 3, "one try in five minutes");
        // ...but again after five minutes.
        pass(
            root.path(),
            &s,
            now + LEASE_RETRY_SECONDS,
            &|_, _, _| ok(there()),
            &extend,
        );
        assert_eq!(extended.lock().unwrap().len(), 4, "a failure is tried again");
        now += LEASE_RETRY_SECONDS;
        // A machine that did not answer gets no extension call.
        let calls = extended.lock().unwrap().len();
        pass(
            root.path(),
            &s,
            now + 2 * EVERY_SECONDS,
            &|_, _, _| Err(UNREACHABLE.into()),
            &extend,
        );
        assert_eq!(extended.lock().unwrap().len(), calls);
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
