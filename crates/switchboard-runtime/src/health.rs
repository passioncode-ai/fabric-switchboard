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
//! sessions, and since SB-550 the machines its accounts live on), `bugs.jsonl` (crashes found at the next start) and `error-counts.json`.
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
    /// SB-510: what the watchdog found (`watchdog::watched`): no check where no job watches,
    /// `unknown` only when an installed job's report is stale.
    pub watchdog: &'a crate::watchdog::Watched,
    pub owner_running: bool,
    pub version: &'a str,
    pub now: i64,
}

/// The contract's order (DEC-0179 HL-2): ok < unknown < warn < fail. A value that is no state
/// has no rank, so the worst of a list holding one is `unknown`, never `fail` or `ok`.
fn rank(state: &str) -> Option<u8> {
    match state {
        "ok" => Some(0),
        "unknown" => Some(1),
        "warn" => Some(2),
        "fail" => Some(3),
        _ => None,
    }
}

/// The worst of a list of states (HL-2): `unknown` for an empty list or a value that is no state.
pub fn worst<'a>(states: impl IntoIterator<Item = &'a str>) -> &'static str {
    const NAMES: [&str; 4] = ["ok", "unknown", "warn", "fail"];
    let mut top: Option<u8> = None;
    for s in states {
        let Some(r) = rank(s) else { return "unknown" };
        top = Some(top.map_or(r, |t| t.max(r)));
    }
    top.map_or("unknown", |r| NAMES[usize::from(r)])
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
        // SB-407: a stuck board run is stopped with `dispatch stop`.
        (stuck > 0).then_some("switchboard_dispatch_stop"),
    ));
    let duplicate = count(&["session_duplicate"]);
    checks.push(check(
        "sessions_duplicate",
        "sessions",
        if duplicate > 0 { "warn" } else { "ok" },
        duplicate,
        (duplicate > 0).then_some("switchboard_session_close_copy"),
    ));
    // A board run whose process ended with no end recorded (SB-407): history, not an incident;
    // stopping it records the end.
    let gone = count(&["session_gone"]);
    checks.push(check(
        "sessions_gone",
        "sessions",
        "ok",
        gone,
        (gone > 0).then_some("switchboard_dispatch_stop"),
    ));
    // SB-550: the machines this computer's accounts live on, read every minute: one that stopped
    // answering, a failing health check there, or sessions there that need a person.
    let machines = count(&[
        "machine_key_changed",
        "machine_unreachable",
        "machine_health",
        "machine_sessions",
    ]);
    // The area is the contract's closed list (HL-3): the other machines are reached over ssh,
    // `connection` (review round 3 of PR #861).
    checks.push(check(
        "machines",
        "connection",
        if machines > 0 { "warn" } else { "ok" },
        machines,
        (machines > 0).then_some("switchboard_machines"),
    ));
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
    // SB-510: what the watchdog, outside the owner, found: the owner stopped or silent, an act
    // left half done, a board run gone or a session in two processes for too long. Where no job
    // watches there is no check: an absent watchdog is not an unknown state of this Switchboard.
    use crate::watchdog::Watched;
    match inputs.watchdog {
        Watched::NotWatched => {}
        Watched::Stale => checks.push(check("watchdog", "lifecycle", "unknown", 0, None)),
        Watched::Current(findings) => {
            let state = if findings.is_empty() {
                "ok"
            } else if findings.iter().any(|f| f["severity"] == "fail") {
                "fail"
            } else {
                "warn"
            };
            let n = findings.len() as u64;
            checks.push(check(
                "watchdog",
                "lifecycle",
                state,
                n,
                (n > 0).then_some("switchboard_watchdog"),
            ));
        }
    }
    let state = worst(checks.iter().map(|c| c["state"].as_str().unwrap_or("")));
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
            watchdog: &crate::watchdog::Watched::NotWatched,
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

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/agent-health.json")).unwrap()
    }

    #[test]
    fn the_worst_state_is_the_contracts_worst() {
        let file = vectors();
        let cases = file["worst"].as_array().unwrap();
        assert_eq!(cases.len(), 8, "the pinned vectors changed size");
        for case in cases {
            let states = case["input"].as_array().unwrap();
            let got = worst(states.iter().map(|s| s.as_str().unwrap()));
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }

    /// What the contract's reader requires of a report (HL-1, HL-2, FAC-SEM-181), checked on our
    /// own: the fields and no other, a slug product, whole-second `at`, every count present,
    /// each check id once, `fix` null or one tool, and the report's state the worst check's.
    #[test]
    fn our_report_reads_as_valid_by_the_contracts_rules() {
        let slug = regex::Regex::new("^[a-z][a-z0-9-]{1,62}$").unwrap();
        for (attention, bugs, owner) in [
            (
                json!({"items": []}),
                json!({"crashes": [], "counting": true, "errors": []}),
                true,
            ),
            (
                json!({"items": [{"reason": "sign_in"}]}),
                json!({"crashes": [], "counting": false}),
                false,
            ),
        ] {
            let r = run(attention, bugs, owner);
            let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
            assert_eq!(keys.len(), 6, "{keys:?}");
            for k in ["schema", "product", "version", "at", "state", "checks"] {
                assert!(keys.contains(&k), "{k}");
            }
            assert!(slug.is_match(r["product"].as_str().unwrap()));
            assert!(r["at"].is_i64());
            let checks = r["checks"].as_array().unwrap();
            let mut ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), checks.len(), "a check id twice");
            for c in checks {
                assert_eq!(c.as_object().unwrap().len(), 5, "{c}");
                assert!(c["count"].is_u64(), "{c}");
                assert!(rank(c["state"].as_str().unwrap()).is_some(), "{c}");
                assert!(
                    c["fix"].is_null() || c["fix"].as_object().unwrap().len() == 1,
                    "{c}"
                );
            }
            let states = checks.iter().map(|c| c["state"].as_str().unwrap());
            assert_eq!(r["state"], worst(states));
        }
    }

    /// Review round 3 of PR #861: every area the report emits is one of the contract's closed
    /// list (`agent-health.schema.json`, HL-3), read from the vendored fixture; an area outside it
    /// makes every report fail the schema and readers count Switchboard as unknown (HL-5).
    #[test]
    fn every_area_the_report_emits_is_in_the_contracts_list() {
        let areas: Vec<String> = vectors()["areas"]
            .as_array()
            .expect("the fixture carries the schema's area list")
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect();
        assert!(areas.len() >= 9, "{areas:?}");
        let attention = json!({"items": [
            {"reason": "machine_unreachable"}, {"reason": "sign_in"}, {"reason": "session_stuck"}]});
        for counting in [true, false] {
            let r = run(
                attention.clone(),
                json!({"crashes": [], "counting": counting, "errors": []}),
                true,
            );
            for c in r["checks"].as_array().unwrap() {
                let area = c["area"].as_str().unwrap();
                assert!(
                    areas.iter().any(|a| a == area),
                    "{} uses area {area}",
                    c["id"]
                );
            }
        }
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
            {"reason": "machine_unreachable", "label": "studio-machine"},
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
        assert_eq!(state_of(&r, "machines"), "warn");
        assert_eq!(state_of(&r, "crashes"), "fail");
        assert_eq!(state_of(&r, "errors_today"), "warn");
        assert_eq!(r["state"], "fail");
        // Counts and kinds only: no label, account, folder or text leaves.
        let text = r.to_string();
        for planted in ["a@b.c", "/secret/path", "\"x\"", "studio-machine"] {
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

    #[test]
    fn the_watchdogs_check_is_absent_where_no_job_watches_and_unknown_only_when_stale() {
        use crate::watchdog::Watched;
        let now = 10 * 86_400 + 100;
        let with = |watchdog: Watched| {
            report(&Inputs {
                attention: &json!({"items": []}),
                bugs: &json!({"crashes": [], "counting": true, "errors": []}),
                watchdog: &watchdog,
                owner_running: true,
                version: "0.7.121",
                now,
            })
        };
        let has = |r: &Value| {
            r["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "watchdog")
        };
        // No job here (off, Windows, serve, a development build): no check, and the report is ok.
        let none = with(Watched::NotWatched);
        assert!(!has(&none));
        assert_eq!(none["state"], "ok");
        assert_eq!(state_of(&with(Watched::Current(vec![])), "watchdog"), "ok");
        assert_eq!(state_of(&with(Watched::Stale), "watchdog"), "unknown");
        let acts = vec![json!({"code": "act_unfinished", "severity": "warn", "ids": ["S1"]})];
        assert_eq!(state_of(&with(Watched::Current(acts)), "watchdog"), "warn");
        let down = vec![
            json!({"code": "owner_down", "severity": "fail", "ids": []}),
            json!({"code": "act_unfinished", "severity": "warn", "ids": ["S1"]}),
        ];
        let r = with(Watched::Current(down));
        assert_eq!(state_of(&r, "watchdog"), "fail");
        assert_eq!(r["state"], "fail");
        // Counts and codes only: a session id the report holds never leaves.
        assert!(!r.to_string().contains("S1"));
    }
}
