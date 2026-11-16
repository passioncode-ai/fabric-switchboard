//! SB-393 (operator 2026-11-16: «мониторинг ошибок … по всем проектам … чтобы мы реагировали на
//! уровне passion код организации когда у наших клиентов случаются проблемы с агентами»): one
//! health report, `agent-health/1` (proposed to the estate in
//! `docs/research/2026-11-16-agent-health-estate.md`), that a watcher on this machine (Fabric
//! Dashboards, Observatory) reads every few minutes and turns into episodes. Counts, kinds and
//! codes only (the contract's DEC-0157): never an account label, a folder, a session id or text.
//!
//! Each check is `{id, area, state, count, fix}`: `state` is `ok`, `warn`, `fail` or `unknown`
//! (never read as ok), `fix` names the tool that repairs it. The report's `state` is the worst
//! check. A check reads only what this Switchboard already keeps: Needs attention (sign-ins,
//! ended seats, failing usage checks, lost moves, ceiling questions, stuck, duplicate and gone
//! sessions), `bugs.jsonl` (crashes found at the next start) and `error-counts.json`.
use serde_json::{json, Value};

pub const SCHEMA: &str = "agent-health/1";
/// A crash within this many seconds fails the check; an older one in the week warns.
const CRASH_FAIL_SECONDS: i64 = 86_400;
const WEEK: i64 = 7 * 86_400;
/// Errors counted today (UTC) at or above this warn: one bad call is not an incident, a storm is.
const ERRORS_WARN: u64 = 50;

pub struct Inputs<'a> {
    pub attention: &'a Value,
    pub bugs: &'a Value,
    pub owner_running: bool,
    pub version: &'a str,
    pub now: i64,
}

fn rank(state: &str) -> u8 {
    match state {
        "ok" => 0,
        "unknown" => 1,
        "warn" => 2,
        _ => 3,
    }
}

fn check(id: &str, area: &str, state: &str, count: u64, fix: Option<&str>) -> Value {
    json!({"id": id, "area": area, "state": state, "count": count, "fix": fix.map(|t| json!({"tool": t}))})
}

pub fn report(inputs: &Inputs) -> Value {
    let items = inputs.attention["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let count = |reasons: &[&str]| {
        items
            .iter()
            .filter(|i| reasons.contains(&i["reason"].as_str().unwrap_or("")))
            .count() as u64
    };
    let mut checks = vec![check(
        "owner",
        "lifecycle",
        if inputs.owner_running { "ok" } else { "warn" },
        u64::from(!inputs.owner_running),
        (!inputs.owner_running).then_some("switchboard_app"),
    )];
    // Accounts a person must sign in to again, or whose seat ended: work in their group stops.
    let sign_in = count(&["sign_in", "seat_ended", "renewal_refused"]);
    checks.push(check(
        "sign_in",
        "accounts",
        if sign_in > 0 { "warn" } else { "ok" },
        sign_in,
        (sign_in > 0).then_some("switchboard_login"),
    ));
    let other_accounts = count(&["usage_failing", "move_lost", "ceiling_question"]);
    checks.push(check(
        "accounts",
        "accounts",
        if other_accounts > 0 { "warn" } else { "ok" },
        other_accounts,
        (other_accounts > 0).then_some("switchboard_attention"),
    ));
    let stuck = count(&["session_stuck"]);
    checks.push(check(
        "sessions_stuck",
        "sessions",
        if stuck > 0 { "warn" } else { "ok" },
        stuck,
        (stuck > 0).then_some("switchboard_duty"),
    ));
    let duplicate = count(&["session_duplicate"]);
    checks.push(check(
        "sessions_duplicate",
        "sessions",
        if duplicate > 0 { "warn" } else { "ok" },
        duplicate,
        (duplicate > 0).then_some("switchboard_session_close_copy"),
    ));
    // A session whose process ended with no end recorded: history, not an incident.
    let gone = count(&["session_gone"]);
    checks.push(check("sessions_gone", "sessions", "ok", gone, None));
    let crashes: Vec<i64> = inputs.bugs["crashes"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["at"].as_i64()).collect())
        .unwrap_or_default();
    let recent = crashes
        .iter()
        .filter(|at| inputs.now - **at <= CRASH_FAIL_SECONDS)
        .count();
    let week = crashes
        .iter()
        .filter(|at| inputs.now - **at <= WEEK)
        .count();
    checks.push(check(
        "crashes",
        "crash",
        if recent > 0 {
            "fail"
        } else if week > 0 {
            "warn"
        } else {
            "ok"
        },
        week as u64,
        (week > 0).then_some("switchboard_bugs"),
    ));
    let today = inputs.now.div_euclid(86_400);
    let errors = if inputs.bugs["counting"] == true {
        let n: u64 = inputs.bugs["errors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|e| e["day"].as_i64() == Some(today))
                    .filter_map(|e| e["count"].as_u64())
                    .sum()
            })
            .unwrap_or(0);
        check(
            "errors_today",
            "errors",
            if n >= ERRORS_WARN { "warn" } else { "ok" },
            n,
            (n >= ERRORS_WARN).then_some("switchboard_bugs"),
        )
    } else {
        // Usage counts are off: nothing is counted, which is not the same as no errors.
        check("errors_today", "errors", "unknown", 0, None)
    };
    checks.push(errors);
    let state = checks
        .iter()
        .map(|c| c["state"].as_str().unwrap_or("unknown"))
        .max_by_key(|s| rank(s))
        .unwrap_or("unknown")
        .to_owned();
    json!({
        "schema": SCHEMA,
        "product": "fabric-switchboard",
        "version": inputs.version,
        "at": inputs.now,
        "state": state,
        "checks": checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(attention: Value, bugs: Value, owner: bool) -> Value {
        report(&Inputs {
            attention: &attention,
            bugs: &bugs,
            owner_running: owner,
            version: "0.7.102",
            now: 10 * 86_400 + 100,
        })
    }

    fn state_of<'a>(r: &'a Value, id: &str) -> &'a str {
        r["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["state"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn a_quiet_machine_is_ok_and_unknown_errors_are_never_ok() {
        let r = run(
            json!({"items": []}),
            json!({"crashes": [], "counting": true, "errors": []}),
            true,
        );
        assert_eq!(r["schema"], SCHEMA);
        assert_eq!(r["state"], "ok");
        let off = run(
            json!({"items": []}),
            json!({"crashes": [], "counting": false}),
            true,
        );
        assert_eq!(state_of(&off, "errors_today"), "unknown");
        assert_eq!(off["state"], "unknown");
    }

    #[test]
    fn needs_attention_crashes_and_an_error_storm_set_the_worst_state() {
        let attention = json!({"items": [
            {"reason": "sign_in", "label": "a@b.c", "account_id": "x"},
            {"reason": "session_stuck", "folder": "/secret/path"},
            {"reason": "session_duplicate"},
            {"reason": "session_gone"},
        ]});
        let now = 10 * 86_400 + 100;
        let bugs = json!({"crashes": [{"at": now - 3600}], "counting": true,
            "errors": [{"day": 10, "count": 60}, {"day": 9, "count": 500}]});
        let r = run(attention, bugs, false);
        assert_eq!(state_of(&r, "owner"), "warn");
        assert_eq!(state_of(&r, "sign_in"), "warn");
        assert_eq!(state_of(&r, "sessions_stuck"), "warn");
        assert_eq!(state_of(&r, "sessions_duplicate"), "warn");
        assert_eq!(state_of(&r, "sessions_gone"), "ok");
        assert_eq!(state_of(&r, "crashes"), "fail");
        assert_eq!(state_of(&r, "errors_today"), "warn");
        assert_eq!(r["state"], "fail");
        // Counts and kinds only: no label, account, folder or text leaves.
        let text = r.to_string();
        for planted in ["a@b.c", "/secret/path", "\"x\""] {
            assert!(!text.contains(planted), "{planted} leaked");
        }
        // A crash older than a day but within the week only warns.
        let old = run(
            json!({"items": []}),
            json!({"crashes": [{"at": now - 3 * 86_400}], "counting": true, "errors": []}),
            true,
        );
        assert_eq!(state_of(&old, "crashes"), "warn");
    }
}
