//! SB-373 (WF-11.1): the activity journal read a page at a time, by kind and account. The
//! window's Activity page, `switchboard events` and the MCP tool `switchboard_events` all page
//! the same way, through [`page`], so the three never differ.
//!
//! Every event has one class, from its `action` ([`class_of`]); an event whose `detail` says
//! something failed ([`is_failure`]) is also an error. The filter `errors` takes those of every
//! class, and the class filters keep their failed events too: a failed switch is still a switch.
use serde_json::{json, Value};

/// The kinds a filter takes, in the order the window shows them.
pub const KINDS: [&str; 6] = ["switches", "sign_ins", "groups", "moves", "errors", "other"];
/// How many events a page holds when the caller names no limit.
pub const DEFAULT_LIMIT: usize = 50;
/// The most a page holds. The journal keeps 256, so this is every event.
pub const MAX_LIMIT: usize = 500;
/// Refusal: a kind that is not one of [`KINDS`].
pub const KIND_INVALID: &str = "kind is one of switches, sign_ins, groups, moves, errors or other.";

/// The class of an event, from its action. Every action the journal accepts (`event_valid`) is
/// named here; one added later and not named reads as `other` until it is.
pub fn class_of(action: &str) -> &'static str {
    match action {
        // Which account serves next: chosen, rotated, failed over, activated in the ordinary
        // CLI, or work handed to the next agent of a fallback chain.
        "account_selected" | "rotation" | "activation" | "fallback" => "switches",
        // A sign-in made or an account added by one (Claude, Codex, Kimi Code).
        "login" | "account_added" | "kimi_account" => "sign_ins",
        // What a group holds: its folders and rules, and accounts lent between groups.
        "project" | "project_rule" | "balancing" => "groups",
        // An account leaving this machine's use: to another machine, back from it, or given
        // to an agent.
        "machine" | "assignment" => "moves",
        _ => "other",
    }
}

/// Whether an event's detail says something failed.
pub fn is_failure(detail: &str) -> bool {
    matches!(
        detail,
        "failed"
            | "unavailable"
            | "unauthorized"
            | "rate_limited"
            | "upstream_error"
            | "network_error"
            | "aborted"
            | "connection_failed"
            | "timeout"
            | "rejected"
            | "4xx"
            | "5xx"
    )
}

fn matches_kind(event: &Value, kind: &str) -> bool {
    let action = event["action"].as_str().unwrap_or_default();
    let detail = event["detail"].as_str().unwrap_or_default();
    if kind == "errors" {
        is_failure(detail)
    } else {
        class_of(action) == kind
    }
}

/// What a page asks for. `before` is a Unix second, exclusive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub kind: Option<String>,
    pub account_id: Option<String>,
    pub before: Option<i64>,
    pub limit: Option<usize>,
}

/// One page of `events` (the journal as it is stored, oldest first): newest first, at most
/// `limit` (default 50, at most 500, at least 1). A page never splits one second: the events
/// of the second the page ends in all come with it, so `next_before` (that second, exclusive)
/// loses none; it is null when nothing older matches. `kinds` names every kind present in the
/// whole journal, whatever the filter, `errors` included when an event failed.
pub fn page(events: &Value, filter: &Filter) -> Result<Value, String> {
    if let Some(kind) = filter.kind.as_deref() {
        if !KINDS.contains(&kind) {
            return Err(KIND_INVALID.into());
        }
    }
    let all: Vec<&Value> = events.as_array().into_iter().flatten().collect();
    let kinds: Vec<&str> = KINDS
        .iter()
        .copied()
        .filter(|k| all.iter().any(|e| matches_kind(e, k)))
        .collect();
    let at = |e: &Value| e["at"].as_i64().unwrap_or(0);
    let mut matching: Vec<&Value> = all
        .into_iter()
        .filter(|e| filter.kind.as_deref().is_none_or(|k| matches_kind(e, k)))
        .filter(|e| {
            filter
                .account_id
                .as_deref()
                .is_none_or(|id| e["account_id"].as_str() == Some(id))
        })
        .filter(|e| filter.before.is_none_or(|b| at(e) < b))
        .collect();
    // Newest first; events of one second keep the journal's order, newest first.
    matching.reverse();
    matching.sort_by_key(|e| std::cmp::Reverse(at(e)));
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut end = limit.min(matching.len());
    if end > 0 {
        let last = at(matching[end - 1]);
        while end < matching.len() && at(matching[end]) == last {
            end += 1;
        }
    }
    let next_before = (end < matching.len()).then(|| at(matching[end - 1]));
    let page: Vec<Value> = matching[..end].iter().map(|e| (*e).clone()).collect();
    Ok(json!({"events": page, "next_before": next_before, "kinds": kinds}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(at: i64, action: &str, id: Option<&str>, detail: &str) -> Value {
        json!({"at": at, "action": action, "account_id": id, "detail": detail})
    }

    /// Every action and detail the journal accepts, as `event_valid` names them.
    const VOCABULARY: &[(&str, &[&str])] = &[
        ("assignment", &["assigned", "cleared", "override"]),
        ("balancing", &["lent", "returned"]),
        ("machine", &["joined", "left"]),
        ("account_added", &["success"]),
        ("account_updated", &["success"]),
        ("account_removed", &["success"]),
        ("account_selected", &["success"]),
        ("usage", &["observed", "unavailable", "failed"]),
        (
            "request",
            &[
                "success",
                "unauthorized",
                "rate_limited",
                "upstream_error",
                "network_error",
            ],
        ),
        (
            "proxy",
            &[
                "started",
                "stopped",
                "completed",
                "aborted",
                "connection_failed",
                "timeout",
                "rejected",
                "2xx",
                "3xx",
                "4xx",
                "5xx",
            ],
        ),
        ("project_rule", &["saved", "paused", "removed", "applied"]),
        ("project", &["created", "updated", "removed", "sources"]),
        ("fallback_chain", &["set", "cleared"]),
        ("fallback", &["offered", "failed"]),
        ("agent_key", &["saved", "removed"]),
        ("kimi_account", &["added", "removed"]),
        ("organization", &["added", "updated", "removed", "assigned"]),
        ("activation", &["completed", "failed"]),
        (
            "rotation",
            &[
                "switched",
                "failed",
                "switched_on_limit",
                "failover",
                "unusable",
            ],
        ),
        (
            "launch",
            &[
                "started",
                "completed",
                "failed",
                "cancelled",
                "isolated",
                "managed",
            ],
        ),
        (
            "login",
            &[
                "started",
                "completed",
                "failed",
                "cancelled",
                "isolated",
                "managed",
            ],
        ),
    ];

    #[test]
    fn every_action_the_journal_accepts_has_one_class() {
        let expected = [
            ("assignment", "moves"),
            ("balancing", "groups"),
            ("machine", "moves"),
            ("account_added", "sign_ins"),
            ("account_updated", "other"),
            ("account_removed", "other"),
            ("account_selected", "switches"),
            ("usage", "other"),
            ("request", "other"),
            ("proxy", "other"),
            ("project_rule", "groups"),
            ("project", "groups"),
            ("fallback_chain", "other"),
            ("fallback", "switches"),
            ("agent_key", "other"),
            ("kimi_account", "sign_ins"),
            ("organization", "other"),
            ("activation", "switches"),
            ("rotation", "switches"),
            ("launch", "other"),
            ("login", "sign_ins"),
        ];
        assert_eq!(expected.len(), VOCABULARY.len());
        for (action, class) in expected {
            assert_eq!(class_of(action), class, "{action}");
            let details = VOCABULARY.iter().find(|(a, _)| *a == action).unwrap().1;
            for detail in details {
                assert!(crate::event_valid(action, detail), "{action} {detail}");
            }
        }
        assert_eq!(class_of("something_new"), "other");
    }

    #[test]
    fn failures_are_errors_and_stay_in_their_class() {
        for detail in ["failed", "unavailable", "rate_limited", "5xx", "timeout"] {
            assert!(is_failure(detail), "{detail}");
        }
        for detail in [
            "success",
            "switched",
            "unusable",
            "cancelled",
            "2xx",
            "observed",
        ] {
            assert!(!is_failure(detail), "{detail}");
        }
        let journal = json!([
            event(10, "rotation", None, "failed"),
            event(11, "rotation", None, "switched"),
            event(12, "usage", None, "observed"),
        ]);
        let only = |kind: &str| {
            page(
                &journal,
                &Filter {
                    kind: Some(kind.into()),
                    ..Filter::default()
                },
            )
            .unwrap()["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["at"].as_i64().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(only("errors"), vec![10]);
        assert_eq!(only("switches"), vec![11, 10]);
        assert_eq!(only("other"), vec![12]);
        assert_eq!(only("moves"), Vec::<i64>::new());
    }

    #[test]
    fn a_page_is_newest_first_and_names_every_kind_in_the_journal() {
        let a = "00000000-0000-4000-8000-000000000001";
        let b = "00000000-0000-4000-8000-000000000002";
        let journal = json!([
            event(100, "login", Some(a), "completed"),
            event(200, "project", None, "created"),
            event(300, "account_selected", Some(b), "success"),
            event(400, "usage", Some(a), "failed"),
        ]);
        let all = page(&journal, &Filter::default()).unwrap();
        let at: Vec<i64> = all["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["at"].as_i64().unwrap())
            .collect();
        assert_eq!(at, vec![400, 300, 200, 100]);
        assert_eq!(all["next_before"], Value::Null);
        assert_eq!(
            all["kinds"],
            json!(["switches", "sign_ins", "groups", "errors", "other"])
        );
        // The same event objects the journal holds, nothing added.
        assert_eq!(all["events"][0], journal[3]);
        let mine = page(
            &journal,
            &Filter {
                account_id: Some(a.into()),
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!(mine["events"].as_array().unwrap().len(), 2);
        // `kinds` describes the whole journal, not the filtered page.
        assert_eq!(mine["kinds"], all["kinds"]);
        assert_eq!(
            page(
                &journal,
                &Filter {
                    kind: Some("logins".into()),
                    ..Filter::default()
                }
            )
            .unwrap_err(),
            KIND_INVALID
        );
    }

    #[test]
    fn paging_with_before_reaches_every_event_once_and_never_splits_a_second() {
        // 7 events, three of them in second 50.
        let journal = json!([
            event(10, "usage", None, "observed"),
            event(20, "usage", None, "observed"),
            event(50, "usage", None, "observed"),
            event(50, "rotation", None, "switched"),
            event(50, "project", None, "updated"),
            event(60, "usage", None, "observed"),
            event(70, "usage", None, "observed"),
        ]);
        let mut seen = 0;
        let mut before = None;
        let mut pages = Vec::new();
        loop {
            let p = page(
                &journal,
                &Filter {
                    before,
                    limit: Some(3),
                    ..Filter::default()
                },
            )
            .unwrap();
            let n = p["events"].as_array().unwrap().len();
            pages.push(n);
            seen += n;
            match p["next_before"].as_i64() {
                Some(b) => before = Some(b),
                None => break,
            }
        }
        // 70, 60, 50, and the other two of second 50 with them; then 20, 10.
        assert_eq!(pages, vec![5, 2]);
        assert_eq!(seen, 7);
        // Within one second, the newest stored first.
        let first = page(&journal, &Filter::default()).unwrap();
        assert_eq!(first["events"][2]["action"], "project");
        assert_eq!(first["events"][4]["action"], "usage");
        // The limit is clamped, never refused.
        let one = page(
            &journal,
            &Filter {
                limit: Some(0),
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!(one["events"].as_array().unwrap().len(), 1);
        assert_eq!(one["next_before"], 70);
        // An empty journal is an empty page.
        let empty = page(&json!([]), &Filter::default()).unwrap();
        assert_eq!(
            empty,
            json!({"events": [], "next_before": null, "kinds": []})
        );
    }
}
