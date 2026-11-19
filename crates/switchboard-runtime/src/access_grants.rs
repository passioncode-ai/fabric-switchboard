//! SB-573 (fabric-agent-contract DEC-0187, AG-1 to AG-7): who holds the access this Switchboard
//! issued, answered as one `access-grants/1` document so Fabric Dashboards joins it with every
//! other product's by holder.
//!
//! Switchboard's grants are of two kinds:
//! - a seat moved, handed over or delegated to another machine (`seats.jsonl`): the holder is
//!   that machine, named by an opaque reference derived from its name as ssh knows it, never the
//!   name itself; a call back revokes it;
//! - an account given to an agent (`Snapshot::assigned_to`, and from SB-573 `assignments.jsonl`,
//!   so a grant taken back stays listable): the holder is the agent's service key when the name
//!   the person gave is one, otherwise `unresolved`, with that name kept out when it may be a
//!   person's.
//!
//! Every grant opens one account (`subject.kind: account`), named by its id here and its
//! provider. Switchboard meters no money per grant, so no grant carries `limitUsd` or
//! `spentDayUsd` (AG-4: both absent is "not metered"). No value of a credential, no label, no
//! e-mail and no ssh name reaches the answer.
use crate::seats;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::Path;
use switchboard_core::{Provider, Snapshot};

pub const PROTOCOL: &str = "access-grants/1";
/// Switchboard's service key (`<id>.<instance>`): the issuer of every grant listed.
pub const ISSUER: &str = "fabric-switchboard.default";
/// DEC-0036: the calling agent of an act Switchboard did not record it for. A valid service key
/// that names no agent, so a host never joins it with a real one.
pub const UNRECORDED_CALLER: &str = "unrecorded-caller.unknown";
/// AG-1: at most this many grants a page.
pub const MAX_LIMIT: usize = 500;
/// SB-573: one line per account given to an agent or taken back.
pub const ASSIGNMENTS: &str = "assignments.jsonl";
/// The newest lines kept, as `seats.jsonl`.
const KEEP: usize = 1000;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// The label of an `unresolved` holder whose name, as the person wrote it, may be a person's.
pub const HIDDEN_LABEL: &str = "agent named in words that may name a person (hidden)";
/// Refusals of the input.
pub const HOLDER_INVALID: &str =
    "holder and subject are a service key <id>.<instance>, such as frame-agent.default.";
pub const CURSOR_INVALID: &str = "That cursor is not one this answer gave. Ask without it.";
pub const LIMIT_INVALID: &str = "limit is a whole number from 1 to 500.";
/// One write of `assignments.jsonl` at a time.
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `$defs/listInput` of `access-grants.schema.json`: every member optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInput {
    /// Only grants this agent holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// Only grants that open this agent; a grant without a subject opens the issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Revoked and expired grants too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl ListInput {
    /// The input as the schema takes it; the first fault is answered.
    pub fn check(&self) -> Result<(), String> {
        for key in [&self.holder, &self.subject].into_iter().flatten() {
            if !service_key_valid(key) {
                return Err(HOLDER_INVALID.into());
            }
        }
        if self.limit.is_some_and(|n| n == 0 || n > MAX_LIMIT) {
            return Err(LIMIT_INVALID.into());
        }
        if let Some(cursor) = &self.cursor {
            offset_of(cursor).ok_or_else(|| CURSOR_INVALID.to_owned())?;
        }
        Ok(())
    }
}

/// `$defs/serviceKey`: `^[a-z][a-z0-9-]{1,62}\.[a-z][a-z0-9-]{0,31}$`.
pub fn service_key_valid(key: &str) -> bool {
    let Some((id, instance)) = key.split_once('.') else {
        return false;
    };
    let part = |s: &str, min: usize, max: usize| {
        let mut chars = s.chars();
        chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && (min..=max).contains(&(s.len() - 1))
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    part(id, 1, 62) && part(instance, 0, 31)
}

/// `$defs/opaqueRef`: `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`.
pub fn opaque_ref_valid(r: &str) -> bool {
    let mut chars = r.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && r.len() <= 128
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// A page's cursor: the number of grants before it.
fn cursor_at(offset: usize) -> String {
    format!("offset:{offset:08}")
}
fn offset_of(cursor: &str) -> Option<usize> {
    let digits = cursor.strip_prefix("offset:")?;
    if digits.len() < 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// One line of `assignments.jsonl`: an account given to an agent (`assigned`, with the name the
/// person gave, as `Snapshot::assigned_to` holds it) or taken back (`taken_back`). Ids, times and
/// that name only, never a credential.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub at: i64,
    pub event: String,
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// SB-573: records that `account` was given to `to`, or taken back (`None`). The file keeps the
/// newest `KEEP` lines (0600, rewritten whole by rename).
pub fn record_assignment(
    root: &Path,
    at: i64,
    account: &str,
    to: Option<&str>,
) -> Result<(), String> {
    let _write = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let mut lines = assignments(root);
    lines.push(Assignment {
        at,
        event: if to.is_some() {
            "assigned"
        } else {
            "taken_back"
        }
        .into(),
        account: account.into(),
        to: to.map(str::to_owned),
    });
    let skip = lines.len().saturating_sub(KEEP);
    let mut text = String::new();
    for line in &lines[skip..] {
        text.push_str(&serde_json::to_string(line).map_err(|_| "Assignment record unavailable.")?);
        text.push('\n');
    }
    switchboard_core::private_fs::private_write(&root.join(ASSIGNMENTS), text.as_bytes())
}

/// The lines of `assignments.jsonl`, oldest first; an unreadable file reads as none.
pub fn assignments(root: &Path) -> Vec<Assignment> {
    let Ok(bytes) = switchboard_core::private_fs::read_private(&root.join(ASSIGNMENTS), MAX_BYTES)
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The provider whose account a grant opens, as the contract names providers.
fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "anthropic",
        Provider::Codex => "openai",
    }
}

/// AG-2: a machine named by an opaque reference: `m-` and the first 8 bytes of the SHA-256 of
/// its name as its machine group names it (or as ssh named it), lowercased. Stable across
/// answers and aliases; it carries no `@`, user or host.
pub fn machine_ref(snapshot: &Snapshot, machine: &str) -> String {
    use sha2::{Digest, Sha256};
    let name = snapshot
        .machines
        .iter()
        .find(|m| m.named(machine))
        .map(|m| m.name.as_str())
        .unwrap_or(machine)
        .to_ascii_lowercase();
    let digest = Sha256::digest(name.as_bytes());
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("m-{hex}")
}

/// The words of an agent's or service's name: a name holding one reads as an agent's, not a
/// person's.
const AGENT_WORDS: &[&str] = &[
    "agent",
    "agents",
    "bot",
    "service",
    "server",
    "runner",
    "worker",
    "daemon",
    "pipeline",
    "assistant",
    "gateway",
    "foundry",
    "copylot",
    "switchboard",
    "fabric",
    "observatory",
    "dashboards",
    "publisher",
    "hermes",
    "codex",
    "claude",
    "kimi",
    "opencode",
    "goose",
    "ci",
];

/// AG-2: the holder of an account given to an agent by the name the person wrote. A service key
/// is the agent; any other name is `unresolved`, shown as written only when it reads as an
/// agent's name, never with an `@` (DL-22).
pub fn agent_holder(name: &str) -> Value {
    let name = name.trim();
    if service_key_valid(name) {
        return json!({"kind": "agent", "agent": name});
    }
    let lower = name.to_lowercase();
    let agent_like = !name.contains('@')
        && lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| AGENT_WORDS.contains(&w));
    let label: String = if agent_like && !name.is_empty() {
        name.chars().take(120).collect()
    } else {
        HIDDEN_LABEL.into()
    };
    json!({"kind": "unresolved", "label": label})
}

/// AG-5: an act Switchboard recorded. The operator's words recorded with it make it the
/// operator's; otherwise an agent's (`seats.jsonl`'s own `ordered_by`). No act records which
/// agent called, so `agent` is [`UNRECORDED_CALLER`] (DEC-0036).
fn act(at: i64, operator: bool) -> Map<String, Value> {
    let mut a = Map::new();
    a.insert("at".into(), json!(crate::rfc3339(at)));
    a.insert(
        "by".into(),
        json!(if operator { "operator" } else { "agent" }),
    );
    a.insert("via".into(), json!("agent"));
    a.insert("agent".into(), json!(UNRECORDED_CALLER));
    a
}
fn revocation(at: i64, operator: bool, reason: &str) -> Value {
    let mut a = act(at, operator);
    a.insert("reason".into(), json!(reason));
    Value::Object(a)
}

/// One grant while it is built: the record, the time it was issued and whether it is still open.
struct Building {
    record: Map<String, Value>,
    issued_at: i64,
    expires_at: Option<i64>,
    revoked: bool,
    /// The account and, for a seat, the machine reference: what a call back matches.
    account: String,
    machine: Option<String>,
}

/// The `access-grants/1` answer at `now` for `input` (already [`ListInput::check`]ed).
pub fn list(root: &Path, snapshot: &Snapshot, input: &ListInput, now: i64) -> Value {
    let reads = crate::remote_usage::read(root);
    let mut degraded: Vec<String> = Vec::new();
    let mut grants = seat_grants(root, snapshot, &reads, now, &mut degraded);
    grants.extend(assignment_grants(root, snapshot, &mut degraded));
    let mut kept: Vec<(i64, String, Value)> = grants
        .into_iter()
        .filter(|g| input.include_revoked || (!g.revoked && g.expires_at.is_none_or(|t| t > now)))
        .filter(|g| {
            input.holder.as_deref().is_none_or(|h| {
                g.record["holder"]["kind"] == "agent" && g.record["holder"]["agent"] == h
            })
        })
        // Every grant here opens an account, none opens an agent or the issuer itself.
        .filter(|_| input.subject.is_none())
        .map(|g| {
            let id = g.record["id"].as_str().unwrap_or_default().to_owned();
            (g.issued_at, id, Value::Object(g.record))
        })
        .collect();
    kept.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    let start = input.cursor.as_deref().and_then(offset_of).unwrap_or(0);
    let limit = input.limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT);
    let total = kept.len();
    let page: Vec<Value> = kept
        .into_iter()
        .skip(start)
        .take(limit)
        .map(|g| g.2)
        .collect();
    let mut answer = json!({
        "protocol": PROTOCOL,
        "issuer": ISSUER,
        "generatedAt": crate::rfc3339(now),
        "grants": page,
    });
    if start + limit < total {
        answer["nextCursor"] = json!(cursor_at(start + limit));
    }
    degraded.dedup();
    if !degraded.is_empty() {
        degraded.truncate(16);
        answer["degraded"] = json!(degraded
            .into_iter()
            .map(|d| d.chars().take(200).collect::<String>())
            .collect::<Vec<_>>());
    }
    answer
}

/// The account `id` as a grant's subject, or none when it is no longer here.
fn subject_of(snapshot: &Snapshot, id: &str) -> Option<Value> {
    let a = snapshot.accounts.iter().find(|a| a.id == id)?;
    Some(json!({"kind": "account", "provider": provider_name(a.provider), "ref": a.id}))
}

/// Whether the account `id` is still away from this machine: marked as used elsewhere, in a
/// machine's group, or delegated out.
fn away(snapshot: &Snapshot, id: &str) -> bool {
    snapshot.elsewhere.contains_key(id)
        || snapshot.delegated_out.contains_key(id)
        || snapshot
            .accounts
            .iter()
            .any(|a| a.id == id && switchboard_core::machines::is_machine_pool(&a.pool))
}

fn seat_grants(
    root: &Path,
    snapshot: &Snapshot,
    reads: &crate::remote_usage::State,
    now: i64,
    degraded: &mut Vec<String>,
) -> Vec<Building> {
    let mut out: Vec<Building> = Vec::new();
    let mut gone = 0usize;
    let mut lease_unsure = false;
    for s in seats::recent(root, KEEP) {
        let machine = machine_ref(snapshot, &s.machine);
        let operator = s.ordered_by == "operator";
        match s.event.as_str() {
            "granted" | "handed_over" | "unknown" => {
                // A retry of a move whose answer was lost: the newer line is the grant.
                let lost = |g: &Building| {
                    g.account == s.account
                        && g.machine.as_deref() == Some(machine.as_str())
                        && g.record["scopes"][0] == "seat.outcome_unknown"
                };
                out.retain(|g| g.revoked || !lost(g));
                let Some(subject) = subject_of(snapshot, &s.account) else {
                    gone += 1;
                    continue;
                };
                let scope = match (s.event.as_str(), s.lease_hours) {
                    ("granted", Some(_)) => "seat.delegated",
                    ("granted", None) => "seat",
                    ("handed_over", _) => "seat.handed_over",
                    _ => "seat.outcome_unknown",
                };
                let mut id = format!("seat:{}:{}", s.account, s.at);
                let mut n = 1;
                while out.iter().any(|g| g.record["id"] == id.as_str()) {
                    n += 1;
                    id = format!("seat:{}:{}:{n}", s.account, s.at);
                }
                let mut record = Map::new();
                record.insert("id".into(), json!(id));
                record.insert("holder".into(), json!({"kind": "machine", "ref": machine}));
                record.insert("subject".into(), subject);
                record.insert("scopes".into(), json!([scope]));
                record.insert("issued".into(), Value::Object(act(s.at, operator)));
                let expires_at = s.lease_hours.map(|hours| {
                    let name = snapshot
                        .machines
                        .iter()
                        .find(|m| m.named(&s.machine))
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| s.machine.clone());
                    let read = reads.machines.get(&name);
                    lease_unsure |= read.is_some_and(|r| r.lease_failed);
                    let extended = read.and_then(|r| r.leases_at).filter(|t| *t <= now);
                    extended.unwrap_or(s.at).max(s.at) + i64::from(hours) * 3600
                });
                if let Some(t) = expires_at {
                    record.insert("expiresAt".into(), json!(crate::rfc3339(t)));
                }
                out.push(Building {
                    record,
                    issued_at: s.at,
                    expires_at,
                    revoked: false,
                    account: s.account.clone(),
                    machine: Some(machine),
                });
            }
            "recalled" => {
                // The call back of a seat from the machine it names; an older line with another
                // name for that machine is matched by the account.
                let same: Vec<usize> = (0..out.len())
                    .filter(|&i| !out[i].revoked && out[i].account == s.account)
                    .collect();
                let exact: Vec<usize> = same
                    .iter()
                    .copied()
                    .filter(|&i| out[i].machine.as_deref() == Some(machine.as_str()))
                    .collect();
                for i in if exact.is_empty() { same } else { exact } {
                    let g = &mut out[i];
                    g.revoked = true;
                    g.record.insert(
                        "revoked".into(),
                        revocation(
                            s.at.max(g.issued_at),
                            operator,
                            "Called back to this machine.",
                        ),
                    );
                }
            }
            _ => {}
        }
    }
    // A grant still open stands only while its account is away from this machine, and only the
    // newest one of an account; one that came back without a recorded call back is left out.
    let mut seen = std::collections::HashSet::new();
    for i in (0..out.len()).rev() {
        if out[i].revoked {
            continue;
        }
        let account = out[i].account.clone();
        if !away(snapshot, &account) || !seen.insert(account) {
            out.remove(i);
            gone += 1;
        }
    }
    if gone > 0 {
        degraded.push(format!(
            "{gone} seat(s) ended with no recorded call back, or their account is gone from this machine: left out."
        ));
    }
    if lease_unsure {
        degraded.push("A lease extension failed at its last try: a delegated seat's lease there may end before expiresAt.".into());
    }
    out
}

fn assignment_grants(
    root: &Path,
    snapshot: &Snapshot,
    degraded: &mut Vec<String>,
) -> Vec<Building> {
    let mut out: Vec<Building> = Vec::new();
    let mut gone = 0usize;
    let close = |out: &mut Vec<Building>, account: &str, at: i64, reason: &str| {
        for g in out
            .iter_mut()
            .filter(|g| !g.revoked && g.account == account)
        {
            g.revoked = true;
            g.record.insert(
                "revoked".into(),
                revocation(at.max(g.issued_at), false, reason),
            );
        }
    };
    for line in assignments(root) {
        match (line.event.as_str(), line.to.as_deref()) {
            ("assigned", Some(to)) => {
                close(&mut out, &line.account, line.at, "Given to another holder.");
                let Some(subject) = subject_of(snapshot, &line.account) else {
                    gone += 1;
                    continue;
                };
                // Two gives of one account in one second are two grants.
                let base = format!("assign:{}:{}", line.account, line.at);
                let mut id = base.clone();
                let mut n = 1;
                while out.iter().any(|g| g.record["id"] == id.as_str()) {
                    n += 1;
                    id = format!("{base}:{n}");
                }
                out.push(assigned(id, to, subject, line.at, &line.account));
            }
            ("taken_back", _) => close(&mut out, &line.account, line.at, "Taken back."),
            _ => {}
        }
    }
    let mut legacy = 0usize;
    for (id, to) in &snapshot.assigned_to {
        let recorded = out
            .iter()
            .any(|g| !g.revoked && g.account == *id && g.record["holder"] == agent_holder(to));
        if recorded {
            continue;
        }
        let Some(subject) = subject_of(snapshot, id) else {
            continue;
        };
        // Given before `assignments.jsonl` was kept: when is not known, so the time the account
        // was added, the earliest it can have been given; stable from one answer to the next.
        let at = snapshot
            .accounts
            .iter()
            .find(|a| a.id == *id)
            .map(|a| a.created_at)
            .unwrap_or(0);
        out.push(assigned(
            format!("assign:{id}:before-record"),
            to,
            subject,
            at,
            id,
        ));
        legacy += 1;
    }
    // An open grant whose account is no longer given to that holder ended unrecorded.
    let before = out.len();
    out.retain(|g| {
        g.revoked
            || snapshot
                .assigned_to
                .get(&g.account)
                .is_some_and(|to| g.record["holder"] == agent_holder(to))
    });
    gone += before - out.len();
    if legacy > 0 {
        degraded.push(format!(
            "{legacy} account(s) were given to an agent before Switchboard recorded it: issued.at is the earliest it can be."
        ));
    }
    if gone > 0 {
        degraded.push(format!(
            "{gone} account(s) given to an agent ended with no record, or their account is gone: left out."
        ));
    }
    out
}

fn assigned(id: String, to: &str, subject: Value, at: i64, account: &str) -> Building {
    let mut record = Map::new();
    record.insert("id".into(), json!(id));
    record.insert("holder".into(), agent_holder(to));
    record.insert("subject".into(), subject);
    record.insert("scopes".into(), json!(["account.assigned"]));
    record.insert("issued".into(), Value::Object(act(at, false)));
    Building {
        record,
        issued_at: at,
        expires_at: None,
        revoked: false,
        account: account.to_owned(),
        machine: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_usage::{MachineRead, State};
    use crate::spend::Decision;
    use switchboard_core::{Account, AuthKind};

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";
    const C: &str = "33333333-3333-4333-8333-333333333333";
    const NOW: i64 = 1_800_000_000;

    fn account(id: &str, provider: Provider, pool: &str) -> Account {
        Account {
            id: id.into(),
            label: "person.name@example.com".into(),
            provider,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1_700_000_000,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        }
    }
    fn words() -> Decision {
        Decision {
            by: "operator".into(),
            instruction: "отправь место на студию".into(),
            at: "2026-11-19T10:00:00Z".into(),
            source: "session 0000".into(),
        }
    }
    fn page(root: &Path, snapshot: &Snapshot, input: ListInput) -> Value {
        input.check().unwrap();
        list(root, snapshot, &input, NOW)
    }
    fn all() -> ListInput {
        ListInput {
            include_revoked: true,
            ..Default::default()
        }
    }
    /// FAC-SEM-185 and the closed schema's shape, as the contract's `accessGrantsRules` reads them.
    fn conforms(answer: &Value) {
        assert_eq!(answer["protocol"], PROTOCOL);
        assert!(service_key_valid(answer["issuer"].as_str().unwrap()));
        let generated = answer["generatedAt"].as_str().unwrap();
        let mut ids = std::collections::HashSet::new();
        for g in answer["grants"].as_array().unwrap() {
            let keys: Vec<&str> = g.as_object().unwrap().keys().map(String::as_str).collect();
            for k in &keys {
                assert!(
                    [
                        "id",
                        "holder",
                        "subject",
                        "scopes",
                        "issued",
                        "expiresAt",
                        "revoked"
                    ]
                    .contains(k),
                    "{k}"
                );
            }
            assert!(opaque_ref_valid(g["id"].as_str().unwrap()), "{g}");
            assert!(ids.insert(g["id"].as_str().unwrap().to_owned()));
            match g["holder"]["kind"].as_str().unwrap() {
                "machine" => assert!(opaque_ref_valid(g["holder"]["ref"].as_str().unwrap())),
                "agent" => assert!(service_key_valid(g["holder"]["agent"].as_str().unwrap())),
                "unresolved" => assert!(!g["holder"]["label"].as_str().unwrap().contains('@')),
                other => panic!("{other}"),
            }
            assert_eq!(g["subject"]["kind"], "account");
            assert!(opaque_ref_valid(g["subject"]["ref"].as_str().unwrap()));
            assert!(g.get("limitUsd").is_none() && g.get("limitPeriod").is_none());
            for name in ["issued", "revoked"] {
                let Some(act) = g.get(name) else { continue };
                assert_eq!(act["via"], "agent");
                assert!(service_key_valid(act["agent"].as_str().unwrap()));
                assert!(act["at"].as_str().unwrap() <= generated);
            }
            if let Some(r) = g.get("revoked") {
                assert!(r["at"].as_str() >= g["issued"]["at"].as_str());
                assert!(!r["reason"].as_str().unwrap().is_empty());
            }
            if let Some(e) = g.get("expiresAt") {
                assert!(e.as_str() > g["issued"]["at"].as_str());
            }
        }
        let text = answer.to_string();
        assert!(
            !text.contains("example.com") && !text.contains("studio.local"),
            "{text}"
        );
    }

    #[test]
    fn a_seat_moved_and_called_back_is_a_grant_then_a_revoked_one() {
        let root = tempfile::tempdir().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot
            .accounts
            .push(account(A, Provider::Claude, "default"));
        seats::append(
            root.path(),
            &seats::of(
                NOW - 100,
                "granted",
                A,
                "sshlg@studio.local",
                "default",
                Some(&words()),
            ),
        )
        .unwrap();
        snapshot.elsewhere.insert(A.into(), NOW - 100);
        let open = page(root.path(), &snapshot, ListInput::default());
        conforms(&open);
        let g = &open["grants"][0];
        assert_eq!(g["id"], format!("seat:{A}:{}", NOW - 100));
        assert_eq!(g["holder"]["kind"], "machine");
        assert_eq!(
            g["holder"]["ref"],
            machine_ref(&snapshot, "sshlg@studio.local")
        );
        assert_eq!(
            g["subject"],
            json!({"kind": "account", "provider": "anthropic", "ref": A})
        );
        assert_eq!(g["scopes"], json!(["seat"]));
        assert_eq!(g["issued"]["by"], "operator");
        assert_eq!(g["issued"]["agent"], UNRECORDED_CALLER);
        assert!(open.get("degraded").is_none() && open.get("nextCursor").is_none());

        seats::append(
            root.path(),
            &seats::of(
                NOW - 10,
                "recalled",
                A,
                "sshlg@studio.local",
                "default",
                None,
            ),
        )
        .unwrap();
        snapshot.elsewhere.clear();
        assert_eq!(
            page(root.path(), &snapshot, ListInput::default())["grants"],
            json!([])
        );
        let history = page(root.path(), &snapshot, all());
        conforms(&history);
        let r = &history["grants"][0]["revoked"];
        assert_eq!(r["by"], "agent");
        assert_eq!(r["reason"], "Called back to this machine.");
        assert_eq!(r["at"], json!(crate::rfc3339(NOW - 10)));
    }

    #[test]
    fn a_delegated_seat_expires_with_its_lease_and_a_lost_answer_gives_way_to_its_retry() {
        let root = tempfile::tempdir().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.machines.push(switchboard_core::machines::Machine {
            pool: "machine_teammate".into(),
            name: "teammate".into(),
            identity: None,
            aliases: vec!["sshlg@studio.local".into()],
            created_at: 1,
        });
        snapshot
            .accounts
            .push(account(B, Provider::Codex, "machine_teammate"));
        seats::append(
            root.path(),
            &seats::of(NOW - 7200, "unknown", B, "sshlg@studio.local", "acme", None),
        )
        .unwrap();
        let mut seat = seats::of(NOW - 3600, "granted", B, "teammate", "acme", None);
        seat.lease_hours = Some(48);
        seats::append(root.path(), &seat).unwrap();
        let mut reads = State::default();
        reads.machines.insert(
            "teammate".into(),
            MachineRead {
                leases_at: Some(NOW - 60),
                ..Default::default()
            },
        );
        switchboard_core::private_fs::private_write(
            &root.path().join(crate::remote_usage::FILE),
            &serde_json::to_vec(&reads).unwrap(),
        )
        .unwrap();
        let answer = page(root.path(), &snapshot, ListInput::default());
        conforms(&answer);
        let grants = answer["grants"].as_array().unwrap();
        assert_eq!(grants.len(), 1, "{answer}");
        assert_eq!(grants[0]["scopes"], json!(["seat.delegated"]));
        assert_eq!(grants[0]["subject"]["provider"], "openai");
        assert_eq!(
            grants[0]["expiresAt"],
            json!(crate::rfc3339(NOW - 60 + 48 * 3600))
        );
        // The alias and the name are one machine.
        assert_eq!(
            machine_ref(&snapshot, "sshlg@studio.local"),
            machine_ref(&snapshot, "TEAMMATE")
        );
        // Past its lease it is expired: listed only with includeRevoked.
        let later = list(
            root.path(),
            &snapshot,
            &ListInput::default(),
            NOW + 49 * 3600,
        );
        assert_eq!(later["grants"], json!([]));
        let failed = MachineRead {
            leases_at: Some(NOW - 60),
            lease_failed: true,
            ..Default::default()
        };
        reads.machines.insert("teammate".into(), failed);
        switchboard_core::private_fs::private_write(
            &root.path().join(crate::remote_usage::FILE),
            &serde_json::to_vec(&reads).unwrap(),
        )
        .unwrap();
        let unsure = page(root.path(), &snapshot, ListInput::default());
        assert!(unsure["degraded"][0].as_str().unwrap().contains("lease"));
    }

    #[test]
    fn a_seat_that_came_home_unrecorded_or_whose_account_is_gone_is_left_out_and_said() {
        let root = tempfile::tempdir().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot
            .accounts
            .push(account(A, Provider::Claude, "default"));
        seats::append(
            root.path(),
            &seats::of(NOW - 50, "granted", A, "studio.local", "default", None),
        )
        .unwrap();
        seats::append(
            root.path(),
            &seats::of(NOW - 40, "handed_over", C, "colleague", "default", None),
        )
        .unwrap();
        let answer = page(root.path(), &snapshot, all());
        conforms(&answer);
        assert_eq!(answer["grants"], json!([]));
        assert!(answer["degraded"][0]
            .as_str()
            .unwrap()
            .starts_with("2 seat(s)"));
    }

    #[test]
    fn an_account_given_to_an_agent_names_the_agent_or_hides_a_persons_name() {
        assert_eq!(
            agent_holder("frame-agent.default"),
            json!({"kind": "agent", "agent": "frame-agent.default"})
        );
        assert_eq!(
            agent_holder("Frame Agent on Studio")["label"],
            "Frame Agent on Studio"
        );
        assert_eq!(agent_holder("Copylot")["label"], "Copylot");
        for person in [
            "Ivan Petrov",
            "ivan@example.com",
            "agent ivan@example.com",
            "",
        ] {
            assert_eq!(
                agent_holder(person),
                json!({"kind": "unresolved", "label": HIDDEN_LABEL}),
                "{person}"
            );
        }
        let root = tempfile::tempdir().unwrap();
        let mut snapshot = Snapshot::default();
        for id in [A, B, C] {
            snapshot
                .accounts
                .push(account(id, Provider::Claude, "default"));
        }
        record_assignment(root.path(), NOW - 300, A, Some("frame-agent.default")).unwrap();
        record_assignment(root.path(), NOW - 200, A, None).unwrap();
        record_assignment(root.path(), NOW - 100, B, Some("Ivan Petrov")).unwrap();
        record_assignment(root.path(), NOW - 90, B, Some("asset-foundry.default")).unwrap();
        snapshot
            .assigned_to
            .insert(B.into(), "asset-foundry.default".into());
        // Given before the file was kept: the journal has no line, so the account's own time.
        snapshot.assigned_to.insert(C.into(), "Hermes".into());
        let answer = page(root.path(), &snapshot, all());
        conforms(&answer);
        let grants = answer["grants"].as_array().unwrap();
        let ids: Vec<&str> = grants.iter().map(|g| g["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            [
                format!("assign:{C}:before-record"),
                format!("assign:{A}:{}", NOW - 300),
                format!("assign:{B}:{}", NOW - 100),
                format!("assign:{B}:{}", NOW - 90),
            ]
        );
        assert_eq!(
            grants[0]["issued"]["at"],
            json!(crate::rfc3339(1_700_000_000))
        );
        assert_eq!(grants[1]["revoked"]["reason"], "Taken back.");
        assert_eq!(grants[2]["holder"]["label"], HIDDEN_LABEL);
        assert_eq!(grants[2]["revoked"]["reason"], "Given to another holder.");
        assert_eq!(grants[3]["scopes"], json!(["account.assigned"]));
        assert!(answer["degraded"][0]
            .as_str()
            .unwrap()
            .contains("before Switchboard recorded"));
        assert!(!answer.to_string().contains("Ivan"));

        let held = page(
            root.path(),
            &snapshot,
            ListInput {
                holder: Some("asset-foundry.default".into()),
                ..Default::default()
            },
        );
        assert_eq!(held["grants"].as_array().unwrap().len(), 1);
        let opened = page(
            root.path(),
            &snapshot,
            ListInput {
                subject: Some(ISSUER.into()),
                include_revoked: true,
                ..Default::default()
            },
        );
        assert_eq!(opened["grants"], json!([]));
        // An account given away and then removed here, or taken back with no line, is left out.
        snapshot.assigned_to.clear();
        let gone = page(root.path(), &snapshot, ListInput::default());
        assert_eq!(gone["grants"], json!([]));
        assert!(gone["degraded"][0]
            .as_str()
            .unwrap()
            .starts_with("1 account(s)"));
    }

    #[test]
    fn a_page_carries_a_cursor_to_the_next_and_bad_input_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let mut snapshot = Snapshot::default();
        snapshot
            .accounts
            .push(account(A, Provider::Claude, "default"));
        for at in 0..5 {
            record_assignment(
                root.path(),
                NOW - 100 + at,
                A,
                Some(&format!("agent-{at}.default")),
            )
            .unwrap();
        }
        snapshot
            .assigned_to
            .insert(A.into(), "agent-4.default".into());
        let first = page(
            root.path(),
            &snapshot,
            ListInput {
                include_revoked: true,
                limit: Some(2),
                ..Default::default()
            },
        );
        assert_eq!(first["grants"].as_array().unwrap().len(), 2);
        let cursor = first["nextCursor"].as_str().unwrap().to_owned();
        assert!(cursor.len() >= 8);
        let rest = page(
            root.path(),
            &snapshot,
            ListInput {
                include_revoked: true,
                limit: Some(2),
                cursor: Some(cursor),
                ..Default::default()
            },
        );
        assert_eq!(rest["grants"][0]["holder"]["agent"], "agent-2.default");
        let last = page(
            root.path(),
            &snapshot,
            ListInput {
                include_revoked: true,
                cursor: Some(cursor_at(4)),
                ..Default::default()
            },
        );
        assert_eq!(last["grants"].as_array().unwrap().len(), 1);
        assert!(last.get("nextCursor").is_none());

        let bad = |input: ListInput| input.check().unwrap_err();
        assert_eq!(
            bad(ListInput {
                holder: Some("Frame Agent".into()),
                ..Default::default()
            }),
            HOLDER_INVALID
        );
        assert_eq!(
            bad(ListInput {
                subject: Some("a.b".into()),
                ..Default::default()
            }),
            HOLDER_INVALID
        );
        assert_eq!(
            bad(ListInput {
                limit: Some(501),
                ..Default::default()
            }),
            LIMIT_INVALID
        );
        assert_eq!(
            bad(ListInput {
                cursor: Some("page-two-please".into()),
                ..Default::default()
            }),
            CURSOR_INVALID
        );
        // The contract's names, and nothing else.
        let input: ListInput =
            serde_json::from_value(json!({"includeRevoked": true, "limit": 3})).unwrap();
        assert!(input.include_revoked && input.limit == Some(3));
        assert!(serde_json::from_value::<ListInput>(json!({"include_revoked": true})).is_err());
    }

    #[test]
    fn service_keys_and_references_follow_the_contracts_patterns() {
        for good in [
            "fabric-switchboard.default",
            "ab.c",
            "frame-agent.cloud-2",
            UNRECORDED_CALLER,
            ISSUER,
        ] {
            assert!(service_key_valid(good), "{good}");
        }
        for bad in [
            "a.b", "Ab.c", "ab.", "ab.C", "ab", "ab.c.d", "1b.c", "ab.c d",
        ] {
            assert!(!service_key_valid(bad), "{bad}");
        }
        assert!(opaque_ref_valid("m-0123456789abcdef"));
        assert!(
            !opaque_ref_valid("sshlg@studio")
                && !opaque_ref_valid("a b")
                && !opaque_ref_valid("-a")
        );
    }

    #[test]
    fn the_assignment_record_keeps_the_newest_lines_privately() {
        let root = tempfile::tempdir().unwrap();
        for at in 0..KEEP as i64 + 3 {
            record_assignment(root.path(), at, A, None).unwrap();
        }
        let lines = assignments(root.path());
        assert_eq!(lines.len(), KEEP);
        assert_eq!(lines.last().unwrap().at, KEEP as i64 + 2);
        assert!(lines
            .iter()
            .all(|l| l.event == "taken_back" && l.to.is_none()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(ASSIGNMENTS))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
