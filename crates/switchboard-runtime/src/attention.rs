//! SB-365 (WF-3.1, DEC-0015): what needs a person, in one list. A view built from what the
//! owner already knows (the snapshot, the sign-in verdicts, `seats.jsonl`, `spend.json`), never a
//! group: an account listed here stays in its group, and nothing moves when it is listed or when
//! it leaves the list. The window, the CLI (`switchboard attention`) and MCP
//! (`switchboard_attention`) read the same answer.
//!
//! Each item carries a fixed English sentence (no label, no secret, no machine name), so the
//! window can word it in the person's language, and the one action that fixes it: the MCP tool
//! an agent calls and the window control a person presses.
use crate::seats::Seat;
use crate::spend::Question;
use serde_json::{json, Value};
use switchboard_core::{ChainScope, Snapshot};

/// A usage check that has not succeeded for this long on an enabled account needs a look.
pub const USAGE_FAILING_SECONDS: i64 = 3600;

pub const SIGN_IN: &str = "This account's sign-in has ended. Sign in to it again.";
pub const SEAT_ENDED: &str =
    "The organization's seat for this account ended. Sign in to it again, or ask the organization for a seat.";
pub const RENEWAL_REFUSED: &str =
    "The provider refused to renew this sign-in. Sign in to it again.";
pub const USAGE_FAILING: &str =
    "Usage checks for this account have failed for over an hour. Check it again.";
pub const MOVE_UNKNOWN: &str =
    "A move to another machine lost its answer. Look at that machine, then call the account back or remove its copy there.";
pub const REFUSED_THERE: &str =
    "The provider refused this account on the machine it lives on. Sign in to it there, or call it back.";
pub const CEILING_MACHINE: &str =
    "This computer's daily spending ceiling was reached. Keep it or double it.";
pub const CEILING_GROUP: &str =
    "A group's daily spending ceiling was reached. Keep it or double it.";
pub const CEILING_TASK: &str = "A task's spending ceiling was reached. Keep it or double it.";
/// SB-401 (DEC-0023 D-18): a Claude API key from before the pay-per-token group, outside it.
pub const KEY_OUTSIDE_DETAIL: &str =
    "This Claude API key is outside the pay-per-token group, so no daily ceiling holds it. Move it there.";
/// SB-379: one Claude Code session runs in two processes (a resumed copy of a live session).
pub const SESSION_DUPLICATE: &str =
    "One session runs in two processes. Close the quiet copy so they do not work over each other.";
/// SB-379: a board run whose folder has not been written for 30 minutes.
pub const SESSION_STUCK: &str =
    "A board run has been quiet for 30 minutes. Look at it, or stop it.";
/// SB-407: a board run whose process ended with no end recorded.
pub const SESSION_GONE: &str =
    "A board run's process is gone, but its end was not recorded. Stop the run to record it.";

/// SB-509: a launch whose process is gone left a session with its history here; it can be
/// continued as itself. Offered once per session (`session_watch::offer`).
pub const SESSION_CONTINUABLE: &str =
    "A session's process ended without its end being recorded. Its history is here: continue it as the same session.";
/// SB-509: the proxy's token changed (its port was taken), and sessions started before still send
/// the old one, which is refused.
pub const TOKEN_ROTATED: &str =
    "The proxy's token changed when its port was taken, so sessions started before it are refused. Restart them.";

/// What the list is built from; every field is already in memory or one small file read.
pub struct Inputs<'a> {
    pub snapshot: &'a Snapshot,
    /// Accounts whose stored sign-in the provider rejected (`sign_in_known`).
    pub sign_in: &'a [String],
    /// Of those, the ones whose lineage the token endpoint refused (`invalid_grant`).
    pub renewal_refused: &'a [String],
    /// `seats.jsonl`, oldest first.
    pub seats: &'a [Seat],
    /// `spend.json`'s questions, pruned as its readers prune them.
    pub questions: &'a [Question],
    /// SB-366: the last background read of the accounts on other machines.
    pub remote: &'a crate::remote_usage::State,
    /// SB-379: the one-minute watch's last findings (`session_watch::last`).
    pub watch: &'a [Value],
    /// SB-412, SB-417: the dispatches refused at the session limit that still wait
    /// (`dispatch_refusals::waiting`), or none.
    pub refusals: Option<&'a Value>,
    /// SB-420: Claude Code Keychain items Switchboard could not write, too large
    /// (`item_size::read`).
    pub too_large: &'a [crate::item_size::Entry],
    /// SB-509: the sessions left on the proxy's old token (`token_rotation::sessions`), or none.
    pub rotated: Option<&'a Value>,
    pub now: i64,
}

/// SB-550: a machine whose Switchboard did not answer three reads in a row (a minute apart).
pub const MACHINE_UNREACHABLE: &str =
    "Another machine's Switchboard has not answered three reads in a row. Check that the machine is on and reachable over ssh.";
/// SB-550: that machine's Switchboard reports a failing health check, or that it is not running.
pub const MACHINE_HEALTH: &str =
    "Another machine's Switchboard reports a failing health check. Open that machine to see which.";
/// SB-550: sessions on that machine need a person: a second copy, a quiet or gone board run.
pub const MACHINE_SESSIONS: &str =
    "Sessions on another machine need a look: a second copy, or a board run that is quiet or gone.";

/// SB-412: dispatches refused at this computer's session limit. SB-417: listed while they wait
/// (since the pool last admitted a board start), with no one-hour cutoff.
pub const DISPATCH_REFUSED: &str =
    "Board runs are waiting: this computer's session limit refused them, and none has started since.";

/// Reasons in the order they are listed.
const ORDER: [&str; 18] = [
    "keychain_too_large",
    "token_rotated",
    "session_duplicate",
    "session_stuck",
    "session_gone",
    "session_continuable",
    "dispatch_refused",
    "sign_in",
    "seat_ended",
    "renewal_refused",
    "move_unknown",
    "refused_there",
    "machine_unreachable",
    "machine_health",
    "machine_sessions",
    "key_outside_tokens",
    "usage_failing",
    "ceiling_question",
];

/// The list: `{count, items}`, items ordered by reason, then label.
pub fn build(inputs: &Inputs) -> Value {
    let snapshot = inputs.snapshot;
    let mut items: Vec<(usize, String, Value)> = Vec::new();
    let mut push = |reason: &str, label: String, item: Value| {
        let rank = ORDER
            .iter()
            .position(|r| *r == reason)
            .unwrap_or(ORDER.len());
        items.push((rank, label, item));
    };
    for account in &snapshot.accounts {
        let id = &account.id;
        let item = |reason: &str, detail: &str, since: Option<i64>, tool: &str, window: &str| {
            json!({
                "account_id": id,
                "label": account.label,
                "provider": account.provider.as_str(),
                "pool": account.pool,
                "reason": reason,
                "detail": detail,
                "since": since,
                "action": {"tool": tool, "window": window},
            })
        };
        if inputs.sign_in.iter().any(|s| s == id) {
            let (reason, detail) = if snapshot.attribution(account).organization.is_some() {
                ("seat_ended", SEAT_ENDED)
            } else if inputs.renewal_refused.iter().any(|s| s == id) {
                ("renewal_refused", RENEWAL_REFUSED)
            } else {
                ("sign_in", SIGN_IN)
            };
            push(
                reason,
                account.label.clone(),
                item(reason, detail, None, "switchboard_login", "sign_in"),
            );
            continue;
        }
        if snapshot.elsewhere.contains_key(id) {
            // Only the newest of a move, a call back or a lost answer speaks for the account.
            let last = inputs.seats.iter().rev().find(|s| {
                &s.account == id && matches!(s.event.as_str(), "granted" | "recalled" | "unknown")
            });
            if let Some(seat) = last.filter(|s| s.event == "unknown") {
                push(
                    "move_unknown",
                    account.label.clone(),
                    item(
                        "move_unknown",
                        MOVE_UNKNOWN,
                        Some(seat.at),
                        "switchboard_remote",
                        "open_machines",
                    ),
                );
            }
            // SB-366 (WF-3, `refused_there`): the last read of its copy there says the provider
            // refused it; never checked here, so its usage cannot be failing here.
            let machine = snapshot.machines.iter().find(|m| m.pool == account.pool);
            let copy = machine
                .and_then(|m| inputs.remote.machines.get(&m.name))
                .and_then(|r| {
                    r.accounts
                        .iter()
                        .find(|a| &a.id == id)
                        .map(|a| (a, r.as_of))
                });
            if let Some((_, as_of)) =
                copy.filter(|(c, _)| c.usage_status.as_deref() == Some("rejected"))
            {
                push(
                    "refused_there",
                    account.label.clone(),
                    item(
                        "refused_there",
                        REFUSED_THERE,
                        as_of,
                        "switchboard_remote",
                        "open_machines",
                    ),
                );
            }
            continue;
        }
        if !account.enabled {
            continue;
        }
        // SB-401 D-18: a key from before, outside the pay-per-token group: nothing selects or
        // launches it, and a route that already names it keeps serving with no ceiling.
        if snapshot.key_outside_tokens(account) {
            let mut listed = item(
                "key_outside_tokens",
                KEY_OUTSIDE_DETAIL,
                None,
                "switchboard_account_move",
                "move_tokens",
            );
            // The pool to move it to, as the group's own id (`tokens` also names it).
            listed["action"]["pool"] = json!(snapshot.tokens_pool());
            push("key_outside_tokens", account.label.clone(), listed);
        }
        let Some(health) = account.usage_health.as_ref() else {
            continue;
        };
        let since = account
            .usage
            .as_ref()
            .map_or(account.created_at, |u| u.observed_at);
        if health.status == "failed"
            && health.checked_at > since
            && inputs.now - since > USAGE_FAILING_SECONDS
        {
            push(
                "usage_failing",
                account.label.clone(),
                item(
                    "usage_failing",
                    USAGE_FAILING,
                    Some(since),
                    "switchboard_usage",
                    "check_usage",
                ),
            );
        }
    }
    for question in inputs.questions.iter().filter(|q| q.answer.is_none()) {
        let (detail, pool) = match &question.scope {
            ChainScope::Machine => (CEILING_MACHINE, None),
            ChainScope::Project { pool } => (CEILING_GROUP, Some(pool.clone())),
            ChainScope::Workflow { .. } => (CEILING_TASK, None),
        };
        push(
            "ceiling_question",
            String::new(),
            json!({
                "account_id": null,
                "label": null,
                "provider": null,
                "pool": pool,
                "reason": "ceiling_question",
                "detail": detail,
                "since": question.reached_at,
                "action": {"tool": "switchboard_ceiling_answer", "window": "answer_ceiling"},
            }),
        );
    }
    for finding in inputs.watch {
        let (reason, detail) = match finding["kind"].as_str() {
            Some(crate::session_watch::DUPLICATE) => ("session_duplicate", SESSION_DUPLICATE),
            Some(crate::session_watch::STUCK) => ("session_stuck", SESSION_STUCK),
            // SB-407: only a dispatched run that is gone needs a person; any other launch that
            // is gone is history, shown in the session tree.
            Some(crate::session_watch::GONE) if finding["dispatch"] == true => {
                ("session_gone", SESSION_GONE)
            }
            // SB-509: a gone launch whose session can be continued as itself, offered once.
            Some(crate::session_watch::GONE) if finding["continuable"] == true => {
                ("session_continuable", SESSION_CONTINUABLE)
            }
            _ => continue,
        };
        let label = finding["folder"]
            .as_str()
            .and_then(|f| std::path::Path::new(f).file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .or_else(|| finding["session_id"].as_str().map(str::to_owned))
            .unwrap_or_default();
        push(
            reason,
            label.clone(),
            json!({
                "account_id": null,
                "label": label,
                "provider": null,
                "pool": null,
                "reason": reason,
                "detail": detail,
                "since": null,
                "pids": finding["pids"],
                "launch_id": finding["launch_id"],
                "session_id": finding["session_id"],
                // SB-389: the copies and the one offered (`quiet`, `older`, or null: every copy
                // is answering).
                "copies": finding["copies"],
                "close": finding["close"],
                // SB-388: a duplicate is fixed in one press: its quiet copy is closed. SB-407: a
                // stuck or gone board run is stopped (`dispatch stop`), which records its end.
                "action": match (reason, finding["launch_id"].as_str()) {
                    ("session_duplicate", _) => {
                        json!({"tool": "switchboard_session_close_copy", "window": "close_copy"})
                    }
                    // Review round 1 (P1): a gone run's act only records its end, and refuses a
                    // run whose process turns out to run (`if_gone`).
                    ("session_gone", Some(id)) if finding["dispatch"] == true => json!({
                        "tool": "switchboard_dispatch_stop", "window": "dispatch_stop",
                        "if_gone": true,
                        "command": format!("switchboard dispatch stop --launch-id {id} --if-gone"),
                    }),
                    // SB-509: continued only when the person (or an agent on their word) asks.
                    ("session_continuable", _) => json!({
                        "tool": "switchboard_session_continue", "window": "continue_session",
                        "command": format!(
                            "switchboard sessions continue {}",
                            finding["session_id"].as_str().unwrap_or_default()
                        ),
                    }),
                    (_, Some(id)) if finding["dispatch"] == true => json!({
                        "tool": "switchboard_dispatch_stop", "window": "dispatch_stop",
                        "command": format!("switchboard dispatch stop --launch-id {id}"),
                    }),
                    _ => json!({"tool": "switchboard_sessions", "window": "open_sessions"}),
                },
            }),
        );
    }
    // SB-550: each machine group's Switchboard, read every minute: one that stopped answering,
    // and from a read that worked, its failing health checks and its sessions that need a person.
    // Each item names the machine as its label; the sentence names none.
    for machine in &snapshot.machines {
        let Some(read) = inputs.remote.machines.get(&machine.name) else {
            continue;
        };
        let item = |reason: &str, detail: &str, since: Option<i64>, window: &str| {
            json!({
                "account_id": null, "label": machine.name, "provider": null, "pool": null,
                "machine": machine.name, "machine_pool": machine.pool,
                "reason": reason, "detail": detail, "since": since,
                "action": {"tool": "switchboard_machines", "window": window},
            })
        };
        if read.unreachable() {
            let mut listed = item(
                "machine_unreachable",
                MACHINE_UNREACHABLE,
                read.as_of,
                "open_machines",
            );
            listed["failures"] = json!(read.failures);
            listed["code"] = json!(read.reason);
            push("machine_unreachable", machine.name.clone(), listed);
            continue;
        }
        let failing = read.failing_checks();
        if !failing.is_empty() {
            let mut listed = item("machine_health", MACHINE_HEALTH, read.as_of, "open_machines");
            listed["checks"] = json!(failing);
            push("machine_health", machine.name.clone(), listed);
        }
        let troubled = read.troubled_sessions();
        if !troubled.is_empty() {
            let since = troubled.iter().filter_map(|s| s.since).min();
            let mut listed = item("machine_sessions", MACHINE_SESSIONS, since, "open_sessions");
            listed["count"] = json!(troubled.len());
            let mut kinds: Vec<&str> = troubled.iter().map(|s| s.why.as_str()).collect();
            kinds.sort_unstable();
            kinds.dedup();
            listed["kinds"] = json!(kinds);
            listed["session_ids"] =
                json!(troubled.iter().filter_map(|s| s.session_id.clone()).collect::<Vec<_>>());
            push("machine_sessions", machine.name.clone(), listed);
        }
    }
    // SB-509: one item for the sessions on the proxy's old token, with a restart wave as the fix
    // (never a restart by itself).
    if let Some(rotated) = inputs.rotated {
        let sessions = rotated["sessions"].as_array().cloned().unwrap_or_default();
        push(
            "token_rotated",
            String::new(),
            json!({
                "account_id": null, "label": null, "provider": null, "pool": null,
                "reason": "token_rotated", "detail": TOKEN_ROTATED,
                "since": rotated["at"],
                "count": sessions.len(),
                "sessions": sessions,
                "action": {"tool": "switchboard_sessions_restart_plan", "window": "restart_wave",
                    "command": "switchboard sessions restart plan"},
            }),
        );
    }
    // SB-412: one item for the waiting refusals, however many. SB-417: `since` is the first of
    // them, `last_at` the newest.
    if let Some(refused) = inputs.refusals {
        push(
            "dispatch_refused",
            String::new(),
            json!({
                "account_id": null, "label": null, "provider": null, "pool": null,
                "reason": "dispatch_refused", "detail": DISPATCH_REFUSED,
                "since": refused["since"],
                "last_at": refused["last_at"],
                "count": refused["count"],
                "newest_request": refused["newest_request"],
                "freed": refused["freed"],
                "freed_at": refused["freed_at"],
                "action": {"tool": "switchboard_capacity", "window": "open_capacity"},
            }),
        );
    }
    // SB-420: one item per Claude Code item refused as too large; the fix is in Claude Code, so
    // the window offers no button (`none`).
    for entry in inputs.too_large {
        push(
            "keychain_too_large",
            entry.item.clone(),
            json!({
                "account_id": null, "label": null, "provider": "claude", "pool": null,
                "reason": "keychain_too_large", "detail": crate::item_size::DETAIL,
                "since": entry.at,
                "item": entry.item,
                "during": entry.during,
                "action": {"tool": "switchboard_status", "window": "none"},
            }),
        );
    }
    items.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    let items: Vec<Value> = items.into_iter().map(|(_, _, item)| item).collect();
    json!({"count": items.len(), "items": items})
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Account, AuthKind, Provider, Usage, UsageHealth};

    fn account(id: &str, label: &str, pool: &str) -> Account {
        Account {
            id: id.into(),
            label: label.into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1_000,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        }
    }
    fn seat(at: i64, event: &str, account: &str) -> Seat {
        crate::seats::of(at, event, account, "studio.local", "default", None)
    }
    fn question(scope: ChainScope, answered: bool) -> Question {
        Question {
            id: "q1".into(),
            scope,
            cents: 500,
            spent: 5.2,
            reached_at: 9_000,
            answer: answered.then_some(crate::spend::Answer::Keep),
            answered_at: answered.then_some(9_100),
            answered_by: answered.then_some(crate::spend::Surface::Window),
        }
    }
    fn run(
        snapshot: &Snapshot,
        sign_in: &[&str],
        refused: &[&str],
        seats: &[Seat],
        questions: &[Question],
    ) -> Value {
        let sign_in: Vec<String> = sign_in.iter().map(|s| (*s).to_owned()).collect();
        let refused: Vec<String> = refused.iter().map(|s| (*s).to_owned()).collect();
        build(&Inputs {
            snapshot,
            sign_in: &sign_in,
            renewal_refused: &refused,
            seats,
            questions,
            remote: &crate::remote_usage::State::default(),
            watch: &[],
            refusals: None,
            too_large: &[],
            rotated: None,
            now: 10_000,
        })
    }
    #[test]
    fn a_copy_of_a_live_session_and_a_quiet_board_run_come_first() {
        let snapshot = Snapshot::default();
        let watch = vec![
            json!({"kind": "stuck", "launch_id": "L-1", "pids": [7], "folder": "/work/alpha", "dispatch": true}),
            json!({"kind": "duplicate", "session_id": "s-1", "pids": [3, 4]}),
            json!({"kind": "gone", "launch_id": "L-2"}),
            json!({"kind": "gone", "launch_id": "L-3", "folder": "/work/beta", "dispatch": true}),
        ];
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &crate::remote_usage::State::default(),
            watch: &watch,
            refusals: None,
            too_large: &[],
            rotated: None,
            now: 10_000,
        });
        // A launch that is gone is shown in the session tree, not here, unless it is a board run
        // (SB-407): then it is stopped, as a stuck one is, by `dispatch stop`.
        assert_eq!(
            reasons(&answer),
            ["session_duplicate", "session_stuck", "session_gone"]
        );
        assert_eq!(answer["items"][0]["pids"], json!([3, 4]));
        assert_eq!(answer["items"][1]["label"], "alpha");
        assert_eq!(
            answer["items"][1]["action"],
            json!({"tool": "switchboard_dispatch_stop", "window": "dispatch_stop",
                "command": "switchboard dispatch stop --launch-id L-1"})
        );
        assert_eq!(answer["items"][1]["launch_id"], "L-1");
        assert_eq!(answer["items"][2]["label"], "beta");
        assert_eq!(answer["items"][2]["detail"], SESSION_GONE);
        assert_eq!(
            answer["items"][2]["action"],
            json!({"tool": "switchboard_dispatch_stop", "window": "dispatch_stop", "if_gone": true,
                "command": "switchboard dispatch stop --launch-id L-3 --if-gone"})
        );
        assert_eq!(answer["items"][0]["session_id"], "s-1");
        assert_eq!(answer["items"][0]["action"]["window"], "close_copy");
        assert_eq!(
            answer["items"][0]["action"]["tool"],
            "switchboard_session_close_copy"
        );
    }
    #[test]
    fn a_gone_session_with_its_history_offers_continue_and_a_rotated_token_offers_a_restart() {
        // SB-509: both are offered, never done by themselves.
        let snapshot = Snapshot::default();
        let watch = vec![
            json!({"kind": "gone", "launch_id": "L-2", "folder": "/work/gamma",
                "session_id": "s-2", "continuable": true}),
            json!({"kind": "gone", "launch_id": "L-3", "session_id": "s-3"}),
        ];
        let rotated = json!({"at": 9_000, "sessions": [
            {"pid": 7, "agent": "claude-code", "folder": "/work/a", "launch_id": "L-7"}]});
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &crate::remote_usage::State::default(),
            watch: &watch,
            refusals: None,
            too_large: &[],
            rotated: Some(&rotated),
            now: 10_000,
        });
        assert_eq!(reasons(&answer), ["token_rotated", "session_continuable"]);
        let restart = &answer["items"][0];
        assert_eq!(restart["detail"], TOKEN_ROTATED);
        assert_eq!(restart["count"], 1);
        assert_eq!(restart["since"], 9_000);
        assert_eq!(
            restart["action"]["tool"],
            "switchboard_sessions_restart_plan"
        );
        assert_eq!(restart["action"]["window"], "restart_wave");
        let resume = &answer["items"][1];
        assert_eq!(resume["label"], "gamma");
        assert_eq!(resume["session_id"], "s-2");
        assert_eq!(
            resume["action"],
            json!({"tool": "switchboard_session_continue", "window": "continue_session",
                "command": "switchboard sessions continue s-2"})
        );
    }
    /// SB-420: a Claude Code item too large to write is one item each, listed first, naming
    /// what refused it and when; it offers no button, the fix is in Claude Code.
    #[test]
    fn a_keychain_item_too_large_to_write_is_listed_first() {
        let snapshot = Snapshot::default();
        let too_large = [crate::item_size::Entry {
            item: crate::item_size::ORDINARY.into(),
            during: crate::item_size::DURING_RENEWAL.into(),
            at: 9_000,
            stamp: Some("1:2".into()),
        }];
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &crate::remote_usage::State::default(),
            watch: &[json!({"kind": "duplicate", "session_id": "s-1", "pids": [3, 4]})],
            refusals: None,
            too_large: &too_large,
            rotated: None,
            now: 10_000,
        });
        assert_eq!(
            reasons(&answer),
            ["keychain_too_large", "session_duplicate"]
        );
        let item = &answer["items"][0];
        assert_eq!(item["detail"], crate::item_size::DETAIL);
        assert_eq!(item["item"], "ordinary");
        assert_eq!(item["during"], "renewal");
        assert_eq!(item["since"], 9_000);
        assert_eq!(item["action"]["window"], "none");
    }
    #[test]
    fn dispatches_refused_at_the_session_limit_are_one_item_with_their_count() {
        // SB-412: the hour's refusals, however many, are one item that opens the session limit.
        let snapshot = Snapshot::default();
        let refused = json!({"count": 3, "since": 2_000, "last_at": 9_900, "freed": true,
            "freed_at": 9_950, "newest_request": "11111111-2222-4333-8444-555555555555"});
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &crate::remote_usage::State::default(),
            watch: &[json!({"kind": "duplicate", "session_id": "s-1", "pids": [3, 4]})],
            refusals: Some(&refused),
            too_large: &[],
            rotated: None,
            now: 10_000,
        });
        assert_eq!(reasons(&answer), ["session_duplicate", "dispatch_refused"]);
        let item = &answer["items"][1];
        assert_eq!(item["count"], 3);
        assert_eq!(item["since"], 2_000, "the first refusal that still waits");
        assert_eq!(item["last_at"], 9_900);
        assert_eq!(item["freed"], true);
        assert_eq!(
            item["newest_request"],
            "11111111-2222-4333-8444-555555555555"
        );
        assert_eq!(item["detail"], DISPATCH_REFUSED);
        assert_eq!(
            item["action"],
            json!({"tool": "switchboard_capacity", "window": "open_capacity"})
        );
    }
    fn reasons(answer: &Value) -> Vec<String> {
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["reason"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn nothing_to_attend_is_an_empty_list_with_a_count() {
        let snapshot = Snapshot {
            accounts: vec![account("a", "Work", "default")],
            ..Default::default()
        };
        assert_eq!(
            run(&snapshot, &[], &[], &[], &[]),
            json!({"count": 0, "items": []})
        );
    }

    #[test]
    fn a_sign_in_carries_the_account_its_group_and_the_tool_that_fixes_it() {
        let snapshot = Snapshot {
            accounts: vec![account("a", "Work", "team")],
            ..Default::default()
        };
        let answer = run(&snapshot, &["a"], &[], &[], &[]);
        assert_eq!(
            answer,
            json!({"count": 1, "items": [{
                "account_id": "a", "label": "Work", "provider": "claude", "pool": "team",
                "reason": "sign_in", "detail": SIGN_IN, "since": null,
                "action": {"tool": "switchboard_login", "window": "sign_in"},
            }]})
        );
    }

    #[test]
    fn a_refused_renewal_and_an_organizations_seat_are_told_apart() {
        let mut snapshot = Snapshot {
            accounts: vec![account("a", "A", "default"), account("b", "B", "default")],
            ..Default::default()
        };
        snapshot.organizations.push(
            serde_json::from_value(json!({"id": "acme", "name": "Acme", "created_at": 1})).unwrap(),
        );
        snapshot.memberships.insert("b".into(), "acme".into());
        let answer = run(&snapshot, &["a", "b"], &["a", "b"], &[], &[]);
        assert_eq!(reasons(&answer), ["seat_ended", "renewal_refused"]);
        assert_eq!(answer["items"][0]["account_id"], "b");
        assert_eq!(answer["items"][1]["detail"], RENEWAL_REFUSED);
        // A refused lineage no longer in the sign-in list (replaced since) is not listed.
        assert_eq!(run(&snapshot, &[], &["a"], &[], &[])["count"], 0);
    }

    #[test]
    fn usage_failing_for_over_an_hour_on_an_enabled_account_is_listed() {
        let failing = |status: &str, observed: i64, enabled: bool| {
            let mut a = account("a", "A", "default");
            a.enabled = enabled;
            a.usage = Some(Usage {
                windows: vec![],
                used_percent: 10.0,
                observed_at: observed,
                resets_at: None,
                source: "fixture".into(),
            });
            a.usage_health = Some(UsageHealth {
                status: status.into(),
                checked_at: 9_900,
                next_check_at: 10_500,
            });
            Snapshot {
                accounts: vec![a],
                ..Default::default()
            }
        };
        let listed = run(&failing("failed", 6_000, true), &[], &[], &[], &[]);
        assert_eq!(reasons(&listed), ["usage_failing"]);
        assert_eq!(listed["items"][0]["since"], 6_000);
        assert_eq!(
            listed["items"][0]["action"],
            json!({"tool": "switchboard_usage", "window": "check_usage"})
        );
        // Under an hour, a status other than failed, or a disabled account: nothing.
        assert_eq!(
            run(&failing("failed", 9_500, true), &[], &[], &[], &[])["count"],
            0
        );
        assert_eq!(
            run(&failing("unavailable", 6_000, true), &[], &[], &[], &[])["count"],
            0
        );
        assert_eq!(
            run(&failing("failed", 6_000, false), &[], &[], &[], &[])["count"],
            0
        );
        // An account that needs a sign-in is listed once, for the sign-in.
        assert_eq!(
            reasons(&run(&failing("failed", 6_000, true), &["a"], &[], &[], &[])),
            ["sign_in"]
        );
    }

    #[test]
    fn a_copy_the_provider_refused_on_its_machine_is_listed_from_the_last_read() {
        // SB-366 (WF-3's `refused_there`): read in the background, never checked here.
        use crate::remote_usage::{AccountRead, MachineRead, State};
        let mut snapshot = Snapshot {
            accounts: vec![
                account("a", "A", "machine_studio"),
                account("b", "B", "machine_studio"),
            ],
            machines: vec![switchboard_core::Machine {
                pool: "machine_studio".into(),
                name: "studio".into(),
                identity: None,
                aliases: vec![],
                created_at: 1,
            }],
            ..Default::default()
        };
        snapshot.elsewhere.insert("a".into(), 7_000);
        snapshot.elsewhere.insert("b".into(), 7_000);
        let copy = |id: &str, status: &str| AccountRead {
            id: id.into(),
            used_percent: None,
            observed_at: None,
            usage_status: Some(status.into()),
            enabled: Some(true),
        };
        let mut remote = State::default();
        remote.machines.insert(
            "studio".into(),
            MachineRead {
                tried_at: 9_000,
                as_of: Some(9_000),
                reason: None,
                failures: 0,
                accounts: vec![copy("a", "rejected"), copy("b", "ok")],
                ..MachineRead::default()
            },
        );
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &remote,
            watch: &[],
            refusals: None,
            too_large: &[],
            rotated: None,
            now: 10_000,
        });
        assert_eq!(reasons(&answer), ["refused_there"]);
        assert_eq!(answer["items"][0]["account_id"], "a");
        assert_eq!(answer["items"][0]["since"], 9_000);
        assert_eq!(answer["items"][0]["detail"], REFUSED_THERE);
        assert_eq!(answer["items"][0]["action"]["window"], "open_machines");
    }

    /// SB-550: a machine that stopped answering, and from a read that worked its failing health
    /// checks and the sessions there that need a person, each one item naming the machine; the
    /// sentence names none, and a stale read names nothing but the silence.
    #[test]
    fn a_silent_machine_its_failing_health_and_its_troubled_sessions_are_listed() {
        use crate::remote_usage::{HealthRead, MachineRead, SessionRead, SessionsRead, State};
        let machine = |pool: &str, name: &str| switchboard_core::Machine {
            pool: pool.into(),
            name: name.into(),
            identity: None,
            aliases: vec![],
            created_at: 1,
        };
        let snapshot = Snapshot {
            machines: vec![
                machine("machine_a", "alpha"),
                machine("machine_b", "beta"),
                machine("machine_c", "gamma"),
            ],
            ..Default::default()
        };
        let row = |why: &str, group: &str, session: &str, since: i64| SessionRead {
            id: format!("pid:{since}"),
            group: group.into(),
            why: why.into(),
            agent: Some("claude-code".into()),
            launch_id: None,
            session_id: Some(session.into()),
            since: Some(since),
        };
        let healthy = MachineRead {
            tried_at: 9_900,
            as_of: Some(9_900),
            sessions: Some(SessionsRead {
                rows: vec![
                    row("stuck", "needs_you", "s-1", 9_000),
                    row("gone", "needs_you", "s-2", 8_000),
                    row("working", "running", "s-3", 9_500),
                    row("ready_to_continue", "needs_you", "s-4", 9_600),
                ],
                ..Default::default()
            }),
            health: Some(HealthRead {
                state: "fail".into(),
                checks: [("crashes", "fail"), ("sign_in", "warn"), ("owner", "ok")]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
            }),
            ..Default::default()
        };
        let mut remote = State::default();
        remote.machines.insert("alpha".into(), healthy.clone());
        remote.machines.insert(
            "beta".into(),
            MachineRead {
                tried_at: 9_950,
                as_of: Some(7_000),
                reason: Some("unreachable".into()),
                failures: crate::remote_usage::BACKOFF_AFTER,
                ..healthy.clone()
            },
        );
        // Failed once only: its last read is stale, so nothing is named.
        remote.machines.insert(
            "gamma".into(),
            MachineRead {
                reason: Some("unreachable".into()),
                failures: 1,
                ..healthy
            },
        );
        let answer = build(&Inputs {
            snapshot: &snapshot,
            sign_in: &[],
            renewal_refused: &[],
            seats: &[],
            questions: &[],
            remote: &remote,
            watch: &[],
            refusals: None,
            too_large: &[],
            rotated: None,
            now: 10_000,
        });
        assert_eq!(
            reasons(&answer),
            ["machine_unreachable", "machine_health", "machine_sessions"]
        );
        let items = answer["items"].as_array().unwrap();
        assert_eq!(items[0]["label"], "beta");
        assert_eq!(items[0]["since"], 7_000);
        assert_eq!(items[0]["detail"], MACHINE_UNREACHABLE);
        assert_eq!(items[0]["action"]["window"], "open_machines");
        assert_eq!(items[1]["label"], "alpha");
        assert_eq!(items[1]["checks"], json!(["crashes"]));
        assert_eq!(items[2]["count"], 2);
        assert_eq!(items[2]["kinds"], json!(["gone", "stuck"]));
        assert_eq!(items[2]["since"], 8_000);
        assert_eq!(items[2]["action"]["window"], "open_sessions");
        for item in items {
            assert!(!item["detail"].as_str().unwrap().contains("alpha"));
            assert_eq!(item["action"]["tool"], "switchboard_machines");
        }
    }

    #[test]
    fn a_move_whose_answer_was_lost_is_listed_until_it_is_called_back() {
        let mut snapshot = Snapshot {
            accounts: vec![account("a", "A", "default")],
            ..Default::default()
        };
        snapshot.elsewhere.insert("a".into(), 7_000);
        let lost = [seat(7_000, "unknown", "a")];
        let answer = run(&snapshot, &[], &[], &lost, &[]);
        assert_eq!(reasons(&answer), ["move_unknown"]);
        assert_eq!(answer["items"][0]["since"], 7_000);
        assert_eq!(answer["items"][0]["action"]["tool"], "switchboard_remote");
        // Called back (or moved again) since: the newest line speaks.
        let settled = [seat(7_000, "unknown", "a"), seat(8_000, "recalled", "a")];
        assert_eq!(run(&snapshot, &[], &[], &settled, &[])["count"], 0);
        // No longer marked as used elsewhere (the person took it back): nothing.
        snapshot.elsewhere.clear();
        assert_eq!(run(&snapshot, &[], &[], &lost, &[])["count"], 0);
    }

    #[test]
    fn an_open_ceiling_question_is_listed_and_an_answered_one_is_not() {
        let snapshot = Snapshot::default();
        let group = question(
            ChainScope::Project {
                pool: "team".into(),
            },
            false,
        );
        let answer = run(&snapshot, &[], &[], &[], &[group]);
        assert_eq!(
            answer["items"][0],
            json!({
                "account_id": null, "label": null, "provider": null, "pool": "team",
                "reason": "ceiling_question", "detail": CEILING_GROUP, "since": 9_000,
                "action": {"tool": "switchboard_ceiling_answer", "window": "answer_ceiling"},
            })
        );
        let answered = question(ChainScope::Machine, true);
        assert_eq!(run(&snapshot, &[], &[], &[], &[answered])["count"], 0);
    }

    #[test]
    fn a_claude_key_outside_the_pay_per_token_group_is_listed_with_the_move() {
        let mut snapshot = Snapshot::default();
        let mut key = account("k", "Old key", "default");
        key.kind = AuthKind::ApiKey;
        let mut codex = account("c", "Codex key", "default");
        codex.provider = Provider::Codex;
        codex.kind = AuthKind::ApiKey;
        snapshot.accounts = vec![key, codex];
        snapshot.projects = vec![switchboard_core::Project::tokens_group("tokens".into(), 0)];
        let answer = run(&snapshot, &[], &[], &[], &[]);
        assert_eq!(reasons(&answer), ["key_outside_tokens"]);
        assert_eq!(
            answer["items"][0]["action"],
            json!({"tool": "switchboard_account_move", "window": "move_tokens", "pool": "tokens"})
        );
        assert_eq!(answer["items"][0]["detail"], KEY_OUTSIDE_DETAIL);
    }

    #[test]
    fn items_are_ordered_by_reason_then_label_and_carry_no_dash() {
        let mut snapshot = Snapshot {
            accounts: vec![
                account("z", "Zed", "default"),
                account("y", "Amy", "default"),
            ],
            ..Default::default()
        };
        snapshot.elsewhere.insert("z".into(), 7_000);
        let answer = run(
            &snapshot,
            &["y"],
            &[],
            &[seat(7_000, "unknown", "z")],
            &[question(ChainScope::Machine, false)],
        );
        assert_eq!(
            reasons(&answer),
            ["sign_in", "move_unknown", "ceiling_question"]
        );
        for sentence in [
            SIGN_IN,
            SEAT_ENDED,
            RENEWAL_REFUSED,
            USAGE_FAILING,
            MOVE_UNKNOWN,
            CEILING_MACHINE,
            CEILING_GROUP,
            CEILING_TASK,
            KEY_OUTSIDE_DETAIL,
            SESSION_DUPLICATE,
            SESSION_STUCK,
            SESSION_GONE,
            SESSION_CONTINUABLE,
            TOKEN_ROTATED,
            DISPATCH_REFUSED,
        ] {
            assert!(
                !sentence.contains('\u{2014}') && !sentence.contains(" - "),
                "{sentence}"
            );
        }
    }
}
