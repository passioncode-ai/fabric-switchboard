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
pub const CEILING_MACHINE: &str =
    "This computer's daily spending ceiling was reached. Keep it or double it.";
pub const CEILING_GROUP: &str =
    "A group's daily spending ceiling was reached. Keep it or double it.";
pub const CEILING_TASK: &str = "A task's spending ceiling was reached. Keep it or double it.";

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
    pub now: i64,
}

/// Reasons in the order they are listed.
const ORDER: [&str; 7] = [
    "sign_in",
    "seat_ended",
    "renewal_refused",
    "move_unknown",
    "refused_there",
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
            // Used on another machine: never checked here, so its usage cannot be failing here.
            continue;
        }
        if !account.enabled {
            continue;
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
            now: 10_000,
        })
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
        ] {
            assert!(
                !sentence.contains('\u{2014}') && !sentence.contains(" - "),
                "{sentence}"
            );
        }
    }
}
