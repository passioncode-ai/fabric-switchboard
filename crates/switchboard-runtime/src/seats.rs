//! SB-292 (S-2 of `docs/research/seats-and-levels-2026-11-08.md`): one line per seat this
//! machine granted to another machine, and who ordered it. A grant is a sign-in moved over ssh
//! (`account_send`) or a hand-over record written (`account_handover`); the order is the
//! operator's quoted words (`spend::Decision`) when the call carried them, else an agent's.
//! Ids, the other machine as ssh named it, the group and times only: never a label, a token or a
//! path.
use crate::spend::Decision;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "seats.jsonl";
/// The newest lines kept; older ones are dropped on the next write.
const KEEP: usize = 200;
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Seat {
    pub at: i64,
    /// `granted` (a sign-in moved there), `handed_over` (a record for the receiver to sign in) or
    /// `recalled` (the sign-in taken back here).
    pub event: String,
    pub account: String,
    /// The other machine as ssh knows it, or the receiver a hand-over names.
    pub machine: String,
    pub pool: String,
    /// SB-292 (S-3): the account's id on the other machine, for a later `recall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_account: Option<String>,
    /// `operator` when `decision` carries the operator's words, else `agent`.
    pub ordered_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
}

pub(crate) fn of(
    at: i64,
    event: &str,
    account: &str,
    machine: &str,
    pool: &str,
    decision: Option<&Decision>,
) -> Seat {
    Seat {
        at,
        event: event.into(),
        account: account.into(),
        machine: machine.chars().take(200).collect(),
        pool: pool.into(),
        remote_account: None,
        ordered_by: if decision.is_some() {
            "operator"
        } else {
            "agent"
        }
        .into(),
        decision: decision.cloned(),
    }
}

/// SB-292 (S-3): the other machine's id of the account this machine last moved to `machine`.
pub fn remote_account(root: &Path, account: &str, machine: &str) -> Option<String> {
    recent(root, KEEP)
        .into_iter()
        .rev()
        .find(|s| s.event == "granted" && s.account == account && s.machine == machine)
        .and_then(|s| s.remote_account)
}

/// The newest `n` lines, oldest first; a missing or unreadable file reads as none, and a line
/// that does not parse is skipped.
pub fn recent(root: &Path, n: usize) -> Vec<Seat> {
    let Ok(bytes) = switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES) else {
        return Vec::new();
    };
    let seats: Vec<Seat> = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    seats[seats.len().saturating_sub(n)..].to_vec()
}

/// Appends one line, keeping the newest `KEEP`; the file is rewritten whole (0600, by rename).
pub(crate) fn append(root: &Path, seat: &Seat) -> Result<(), String> {
    let mut seats = recent(root, KEEP - 1);
    seats.push(seat.clone());
    let mut text = String::new();
    for s in &seats {
        text.push_str(&serde_json::to_string(s).map_err(|_| "Seat record unavailable.")?);
        text.push('\n');
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_names_who_ordered_it_and_only_the_newest_lines_stay() {
        let root = tempfile::tempdir().unwrap();
        let words = Decision {
            by: "operator".into(),
            instruction: "дай место участнику".into(),
            at: "2026-11-08T01:00:00Z".into(),
            source: "session 0000".into(),
        };
        let granted = of(5, "granted", "acct", "member@vm", "acme", Some(&words));
        append(root.path(), &granted).unwrap();
        let read = recent(root.path(), 10);
        assert_eq!(read, [granted]);
        assert_eq!(read[0].ordered_by, "operator");
        let by_agent = of(6, "handed_over", "acct", "member@vm", "acme", None);
        assert_eq!(by_agent.ordered_by, "agent");
        let line = serde_json::to_string(&by_agent).unwrap();
        assert!(!line.contains("decision"), "{line}");
        for at in 0..KEEP as i64 + 5 {
            append(root.path(), &of(at, "granted", "a", "m", "p", None)).unwrap();
        }
        let kept = recent(root.path(), KEEP + 10);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept.last().unwrap().at, KEEP as i64 + 4);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
