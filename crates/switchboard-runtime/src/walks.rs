//! SB-82 (D-3, [design](../../../docs/research/runner-routes-2026-11-02.md)): one record per walk of
//! a fallback chain, in the shape of fabric-agent-contract's `runner-route-event` (DEC-0029). Each
//! candidate the walk passed over is named once, in order, with the contract's closed probe result
//! and Switchboard's own reason as `detail`; a walk that selected a candidate names it. The basis is
//! `host-order`: a chain is Switchboard's own order, not a contract route revision (Fabric ADR-0125
//! decision 1 says the same of its fallback order). Ids only: agent, workflow and account ids,
//! never a label, a credential or a path. The oplog keeps its code-only `fallback` event.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "fallback-walks.jsonl";
/// The newest walks kept; older lines are dropped on the next write.
const KEEP: usize = 200;
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Passed {
    pub agent: String,
    pub runner_kind: String,
    pub probe: String,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub agent: String,
    pub runner_kind: String,
    pub account_id: String,
    pub spawned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Walk {
    pub at: i64,
    /// `runner-selected` or `runner-exhausted`.
    pub event: String,
    pub basis: String,
    pub workflow: String,
    pub unattended: bool,
    pub passed_over: Vec<Passed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<Selected>,
}

/// The contract's probe result for one of Switchboard's reasons a candidate was passed over.
pub fn probe(reason: &str) -> &'static str {
    match reason {
        "agent_not_automatic_yet" => "not-admitted",
        "not_installed" => "not-installed",
        "sign_in_required" => "not-connected",
        "spend_unknown" | "ceilings_unreadable" | "quota_unknown" => "quota-unknown",
        "launch_failed" => "spawn-failed",
        // Disabled, no usable account, a spend ceiling or a key at its limit, a refused offer:
        // the candidate cannot serve this request.
        _ => "refused",
    }
}

/// One walk's record from what the hand-over did: `passed` in walk order, `selected` the agent
/// and account an offer was made for.
pub(crate) fn of(
    at: i64,
    workflow: &str,
    passed: &[(String, &'static str)],
    selected: Option<(String, String)>,
) -> Walk {
    let kind = |agent: &str| crate::agent_catalog::contract_id(agent).to_owned();
    Walk {
        at,
        event: if selected.is_some() {
            "runner-selected"
        } else {
            "runner-exhausted"
        }
        .into(),
        basis: "host-order".into(),
        workflow: workflow.chars().take(64).collect(),
        unattended: true,
        passed_over: passed
            .iter()
            .map(|(agent, reason)| Passed {
                agent: agent.chars().take(64).collect(),
                runner_kind: kind(agent),
                probe: probe(reason).into(),
                detail: (*reason).into(),
            })
            .collect(),
        selected: selected.map(|(agent, account_id)| Selected {
            runner_kind: kind(&agent),
            agent,
            account_id,
            spawned: true,
        }),
    }
}

/// The newest `n` walks, oldest first; a missing or unreadable file reads as none, and a line
/// that does not parse is skipped.
pub fn recent(root: &Path, n: usize) -> Vec<Walk> {
    let Ok(bytes) = switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES) else {
        return Vec::new();
    };
    let walks: Vec<Walk> = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    walks[walks.len().saturating_sub(n)..].to_vec()
}

/// Appends one walk, keeping the newest `KEEP`; the file is rewritten whole (0600, by rename).
pub(crate) fn append(root: &Path, walk: &Walk) -> Result<(), String> {
    let mut walks = recent(root, KEEP - 1);
    walks.push(walk.clone());
    let mut text = String::new();
    for w in &walks {
        text.push_str(&serde_json::to_string(w).map_err(|_| "Walk record unavailable.")?);
        text.push('\n');
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_names_each_passed_over_candidate_once_in_order_with_its_probe() {
        let root = tempfile::tempdir().unwrap();
        let passed = vec![
            ("claude-code".to_owned(), "no_usable_account"),
            ("cursor-cli".to_owned(), "agent_not_automatic_yet"),
            ("hermes".to_owned(), "spend_unknown"),
        ];
        let walk = of(
            5,
            "wf_0123456789abcdef",
            &passed,
            Some(("codex".into(), "acct".into())),
        );
        append(root.path(), &walk).unwrap();
        let read = recent(root.path(), 20);
        assert_eq!(read, [walk]);
        let w = &read[0];
        assert_eq!(w.event, "runner-selected");
        assert_eq!(w.basis, "host-order");
        let probes: Vec<(&str, &str, &str)> = w
            .passed_over
            .iter()
            .map(|p| (p.agent.as_str(), p.runner_kind.as_str(), p.probe.as_str()))
            .collect();
        assert_eq!(
            probes,
            [
                ("claude-code", "claude-code", "refused"),
                ("cursor-cli", "cursor-agent", "not-admitted"),
                ("hermes", "hermes", "quota-unknown"),
            ]
        );
        assert_eq!(w.selected.as_ref().unwrap().runner_kind, "codex");
        // An exhausted walk names no selection.
        let exhausted = of(6, "wf_0123456789abcdef", &passed, None);
        assert_eq!(exhausted.event, "runner-exhausted");
        assert!(exhausted.selected.is_none());
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

    #[test]
    fn only_the_newest_walks_are_kept() {
        let root = tempfile::tempdir().unwrap();
        for at in 0..(KEEP as i64 + 5) {
            append(root.path(), &of(at, "wf_0123456789abcdef", &[], None)).unwrap();
        }
        let all = recent(root.path(), usize::MAX);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all.first().unwrap().at, 5);
        assert_eq!(recent(root.path(), 3).last().unwrap().at, KEEP as i64 + 4);
    }
}
