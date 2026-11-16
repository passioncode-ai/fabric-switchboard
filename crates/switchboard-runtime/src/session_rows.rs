//! SB-395 (operator 2026-11-16, relayed by fabric-dashboards: «сессиям, короче, на каждом уровне
//! нужно видеть сессии»): one list of this computer's coding sessions in four groups, the order
//! Fabric Dashboards' Sessions mode uses: needs you, running, waiting, ended. Each row says what
//! it is in fields, never text a session wrote, and which of the acts Switchboard already has
//! apply to it (`close_copy`, `end`, `attach`). The window's Sessions page and `sessions --rows`
//! read the same answer; another machine's comes from its own Switchboard (`--target`).
//!
//! Sources, all already kept: the session tree (`session_watch::tree`: live processes, launches of
//! the last day, the watch's findings), the stops on a limit (`recovery::status`) and the hosted
//! sessions a viewer can open (`session_host`). A source that could not be read is named in
//! `degraded` and its rows are missing, never guessed.
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// No switchboard command next to this app to attach with.
pub const NO_CLI: &str = "The switchboard command was not found next to this app, so the session cannot be opened here. Reinstall Switchboard, then retry.";
/// A session host runs on macOS and Linux.
pub const HOST_UNSUPPORTED: &str = "A session host runs on macOS and Linux for now.";

pub const GROUPS: [&str; 4] = ["needs_you", "running", "waiting", "ended"];

/// What the rows are built from; each is the answer its own command gives.
pub struct Sources<'a> {
    pub tree: &'a Value,
    /// `recovery` (`Null` when it could not be read).
    pub recovery: &'a Value,
    /// Account id → its label, for every account this computer knows.
    pub labels: &'a HashMap<String, String>,
    /// Launch ids of hosted sessions a viewer can open now.
    pub attachable: &'a HashSet<String>,
    /// SB-396: the ends Switchboard recorded (`resumable::recent`), newest first.
    pub resumable: &'a [Value],
}

fn label(sources: &Sources, account: &Value) -> Value {
    account
        .as_str()
        .and_then(|id| sources.labels.get(id))
        .map_or(Value::Null, |l| json!(l))
}

/// The coding agent a launch record or a stop names by its provider: one field across rows.
fn agent_of(provider: &Value) -> Value {
    match provider.as_str() {
        Some("claude") => json!("claude-code"),
        Some(other) => json!(other),
        None => Value::Null,
    }
}

fn row(group: &str, why: &str, id: String, fields: Value, acts: Vec<&str>) -> Value {
    let mut row = json!({"id": id, "group": group, "why": why, "acts": acts});
    if let (Some(row), Some(fields)) = (row.as_object_mut(), fields.as_object()) {
        for (k, v) in fields {
            row.insert(k.clone(), v.clone());
        }
    }
    row
}

pub fn build(sources: &Sources) -> Value {
    let tree = sources.tree;
    let findings: Vec<&Value> = tree["findings"]
        .as_array()
        .map_or(Vec::new(), |f| f.iter().collect());
    let finding = |kind: &str, key: &str, value: &Value| {
        findings
            .iter()
            .find(|f| f["kind"] == kind && !value.is_null() && f[key] == *value)
            .copied()
    };
    let launches: Vec<&Value> = tree["launches"]
        .as_array()
        .map_or(Vec::new(), |l| l.iter().collect());
    let launch = |id: &Value| {
        launches
            .iter()
            .find(|l| !id.is_null() && l["launch_id"] == *id)
            .copied()
    };
    let mut rows = Vec::new();
    // SB-396 D10: one row per session. A session still alive in its window after a limit stop is
    // its live row (waiting), never a second row for the stop.
    let live_sessions: HashSet<&str> = tree["live"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["session_id"].as_str())
        .collect();
    let stops: Vec<&Value> = sources.recovery["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["stop"]["state"] == "waiting")
        .collect();
    let stopped_live: HashMap<&str, &Value> = stops
        .iter()
        .filter_map(|s| s["stop"]["session"].as_str().map(|id| (id, *s)))
        .filter(|(id, _)| live_sessions.contains(id))
        .collect();
    let mut listed: HashSet<String> = live_sessions.iter().map(|s| (*s).to_owned()).collect();

    for live in tree["live"].as_array().into_iter().flatten() {
        let launch_id = &live["launch_id"];
        let record = launch(launch_id);
        let hosted = launch_id
            .as_str()
            .is_some_and(|id| sources.attachable.contains(id));
        let mut acts = Vec::new();
        if hosted {
            acts.extend(["attach", "end"]);
        }
        let stop = live["session_id"]
            .as_str()
            .and_then(|id| stopped_live.get(id));
        let (group, why) =
            if let Some(dup) = finding("duplicate", "session_id", &live["session_id"]) {
                // Only the copy Needs attention offers to close carries the act (SB-388, SB-389).
                if dup["close"]["pid"] == live["pid"] {
                    acts.push("close_copy");
                }
                ("needs_you", "duplicate")
            } else if finding("stuck", "launch_id", launch_id).is_some() {
                ("needs_you", "stuck")
            } else if stop.is_some() {
                ("waiting", "limit")
            } else if live["quiet"] == true {
                ("running", "quiet")
            } else {
                ("running", "working")
            };
        let context = record.and_then(|r| r["pool"].as_str()).map(str::to_owned);
        rows.push(row(
            group,
            why,
            format!("pid:{}", live["pid"]),
            json!({
                "agent": live["agent"], "folder": live["folder"], "pid": live["pid"],
                "session_id": live["session_id"], "launch_id": launch_id,
                "account": label(sources, &live["account_id"]),
                "pool": live["pool"].as_str().map(str::to_owned).or(context),
                "origin": live["surface"], "since": live["started_at"],
                "quiet_minutes": live["quiet_minutes"], "task": live["task"],
                "probe_at": stop.map_or(Value::Null, |s| s["probe_at"].clone()),
            }),
            acts,
        ));
    }

    for s in &stops {
        let stop = &s["stop"];
        let Some(session) = stop["session"].as_str() else {
            continue;
        };
        if !listed.insert(session.to_owned()) {
            continue;
        }
        let (group, why, acts) = if s["ready"] == true {
            ("needs_you", "ready_to_continue", vec!["continue"])
        } else {
            ("waiting", "limit", Vec::new())
        };
        rows.push(row(
            group,
            why,
            format!("stop:{}", stop["event"].as_str().unwrap_or_default()),
            json!({
                "agent": agent_of(&stop["provider"]), "folder": stop["directory"],
                "session_id": session, "launch_id": s["launch_id"],
                "account": label(sources, &stop["account"]), "pool": stop["pool"],
                "origin": Value::Null, "since": stop["stopped_at"], "probe_at": s["probe_at"],
            }),
            acts,
        ));
    }

    for l in &launches {
        let state = l["state"].as_str().unwrap_or("");
        // SB-396 D11: `ended` is an object `{kind, at}` or, written by older paths, a word.
        let ended = l["ended"]["kind"].as_str().or_else(|| l["ended"].as_str());
        let (group, why) = match state {
            "running" => continue,
            "gone" => ("needs_you", "gone"),
            _ => ("ended", ended.unwrap_or("ended")),
        };
        let session = l["session_id"].as_str();
        if session.is_some_and(|id| !listed.insert(id.to_owned())) {
            continue;
        }
        let acts = if session.is_some() && l["provider"] == "claude" {
            vec!["continue"]
        } else {
            Vec::new()
        };
        rows.push(row(
            group,
            why,
            format!("launch:{}", l["launch_id"].as_str().unwrap_or_default()),
            json!({
                "agent": agent_of(&l["provider"]), "folder": l["folder"], "launch_id": l["launch_id"],
                "session_id": session, "account": label(sources, &l["account_id"]), "pool": l["pool"],
                "origin": l["surface"], "since": l["ended"]["at"].as_i64().or(l["launched_at"].as_i64()),
                "task": l["task"],
            }),
            acts,
        ));
    }

    // SB-396 D5: the sessions Switchboard closed (the duty officer, an idle session host) can be
    // continued as themselves.
    for r in sources.resumable {
        let Some(session) = r["session"].as_str() else {
            continue;
        };
        if !listed.insert(session.to_owned()) {
            continue;
        }
        let why = match r["reason"].as_str() {
            Some("host_idle") => "closed_idle_window",
            Some("duplicate") => "closed_duplicate",
            Some("stuck") => "closed_stuck",
            _ => "closed_idle",
        };
        rows.push(row(
            "ended",
            why,
            format!("closed:{session}"),
            json!({
                "agent": "claude-code", "folder": r["cwd"], "session_id": session,
                "launch_id": r["launch_id"], "account": Value::Null, "pool": Value::Null,
                "origin": Value::Null, "since": r["at"],
            }),
            vec!["continue"],
        ));
    }

    let order = |g: &Value| GROUPS.iter().position(|x| g == x).unwrap_or(GROUPS.len());
    rows.sort_by(|a, b| {
        order(&a["group"])
            .cmp(&order(&b["group"]))
            .then_with(|| b["since"].as_i64().cmp(&a["since"].as_i64()))
    });
    let mut counts = serde_json::Map::new();
    for g in GROUPS {
        counts.insert(
            g.into(),
            json!(rows.iter().filter(|r| r["group"] == g).count()),
        );
    }
    let mut degraded = tree["degraded"].as_array().cloned().unwrap_or_default();
    if sources.recovery.is_null() {
        degraded.push(json!("recovery"));
    }
    json!({"rows": rows, "counts": counts, "degraded": degraded, "observed_at": tree["observed_at"]})
}

/// How another machine's rows read, as its heading says it: `read`, `unreachable`,
/// `no_switchboard`, `too_old` (a Switchboard without `sessions --rows`) or `bad_answer`.
pub fn there(output: Result<String, String>) -> Value {
    let text = match &output {
        Ok(out) | Err(out) => out.as_str(),
    };
    if let Some(answer) = crate::account_send::data(text) {
        if answer["rows"].is_array() {
            return json!({"status": "read", "rows": answer["rows"], "counts": answer["counts"], "degraded": answer["degraded"]});
        }
        return json!({"status": "bad_answer"});
    }
    let status = if text.contains("Invalid arguments. Run switchboard --help.") {
        "too_old"
    } else if text.contains("No such file or directory") || text.contains("command not found") {
        "no_switchboard"
    } else if output.is_ok() {
        "bad_answer"
    } else {
        "unreachable"
    };
    json!({"status": status})
}

/// The script a Terminal window runs to open a hosted session: this computer's Switchboard
/// attaches to it here, or through ssh on `target`. Every word is quoted; closing the window
/// leaves the session running.
pub fn attach_script(cli: &str, root: &str, launch_id: &str, target: Option<&str>) -> String {
    let q = crate::account_send::quoted;
    let there = target.map_or(String::new(), |t| format!(" --target {}", q(t)));
    format!(
        "#!/bin/sh\nexec {} --data-dir {} attach {}{}\n",
        q(cli),
        q(root),
        q(launch_id),
        there
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Value {
        json!({
            "observed_at": 1000,
            "live": [
                {"pid": 11, "agent": "claude-code", "folder": "/w/a", "session_id": "S1", "launch_id": null,
                 "quiet": false, "quiet_minutes": 1, "started_at": 900, "surface": null},
                {"pid": 12, "agent": "claude-code", "folder": "/w/a", "session_id": "S1", "launch_id": null,
                 "quiet": true, "quiet_minutes": 40, "started_at": 800, "surface": null},
                {"pid": 13, "agent": "claude-code", "folder": "/w/b", "session_id": "S2", "launch_id": "L1",
                 "account_id": "A1", "pool": "apps", "quiet": true, "quiet_minutes": 50, "started_at": 700, "surface": "fabric"},
                {"pid": 14, "agent": "codex", "folder": "/w/c", "session_id": "S3", "launch_id": "L2",
                 "account_id": "A2", "pool": "default", "quiet": false, "quiet_minutes": 0, "started_at": 950, "surface": "cli"}
            ],
            "launches": [
                {"launch_id": "L1", "state": "running", "pool": "apps", "provider": "claude", "folder": "/w/b", "launched_at": 700},
                {"launch_id": "L2", "state": "running", "pool": "default", "provider": "codex", "folder": "/w/c", "launched_at": 950},
                {"launch_id": "L3", "state": "ended", "ended": {"kind": "exited", "at": 990}, "account_id": "A1", "provider": "claude", "folder": "/w/d", "launched_at": 500},
                {"launch_id": "L4", "state": "gone", "ended": null, "provider": "claude", "folder": "/w/e", "launched_at": 100}
            ],
            "findings": [
                {"kind": "duplicate", "session_id": "S1", "pids": [11, 12], "close": {"pid": 12, "why": "quiet"}},
                {"kind": "stuck", "launch_id": "L1", "pids": [13]},
                {"kind": "gone", "launch_id": "L4"}
            ]
        })
    }

    fn recovery() -> Value {
        json!({"sessions": [
            {"ready": true, "launch_id": null, "stop": {"state": "waiting", "event": "E1", "provider": "claude", "directory": "/w/f", "session": "S9", "account": "A1", "pool": "apps", "stopped_at": 600}},
            {"ready": false, "probe_at": 2000, "stop": {"state": "waiting", "event": "E2", "provider": "claude", "directory": "/w/g", "session": "S8", "account": "A2", "pool": "default", "stopped_at": 650}},
            {"ready": false, "stop": {"state": "resumed", "event": "E3", "provider": "claude", "directory": "/w/h", "stopped_at": 300}}
        ]})
    }

    fn rows(recovery: &Value) -> Value {
        let labels = HashMap::from([
            ("A1".to_owned(), "one@x".to_owned()),
            ("A2".to_owned(), "two@x".to_owned()),
        ]);
        build(&Sources {
            tree: &tree(),
            recovery,
            labels: &labels,
            attachable: &HashSet::from(["L2".to_owned()]),
            resumable: &[],
        })
    }

    #[test]
    fn every_session_lands_in_one_group_with_the_acts_it_has() {
        let answer = rows(&recovery());
        let list = answer["rows"].as_array().unwrap();
        let by = |id: &str| list.iter().find(|r| r["id"] == id).unwrap().clone();
        assert_eq!(
            answer["counts"],
            json!({"needs_you": 5, "running": 1, "waiting": 1, "ended": 1})
        );
        // The duplicate: both copies need the person, only the offered one can be closed.
        assert_eq!(
            (by("pid:11")["why"].clone(), by("pid:11")["acts"].clone()),
            (json!("duplicate"), json!([]))
        );
        assert_eq!(by("pid:12")["acts"], json!(["close_copy"]));
        assert_eq!(
            (by("pid:13")["why"].clone(), by("pid:13")["account"].clone()),
            (json!("stuck"), json!("one@x"))
        );
        // A hosted session can be opened here and ended.
        assert_eq!(
            (by("pid:14")["group"].clone(), by("pid:14")["acts"].clone()),
            (json!("running"), json!(["attach", "end"]))
        );
        assert_eq!(
            (
                by("launch:L3")["group"].clone(),
                by("launch:L3")["why"].clone()
            ),
            (json!("ended"), json!("exited"))
        );
        assert_eq!(by("launch:L4")["why"], "gone");
        assert_eq!(
            by("launch:L3")["agent"],
            "claude-code",
            "a provider reads as its agent"
        );
        assert_eq!(
            (by("stop:E1")["group"].clone(), by("stop:E1")["why"].clone()),
            (json!("needs_you"), json!("ready_to_continue"))
        );
        assert_eq!(
            (
                by("stop:E2")["group"].clone(),
                by("stop:E2")["probe_at"].clone()
            ),
            (json!("waiting"), json!(2000))
        );
        assert!(
            list.iter().all(|r| r["id"] != "stop:E3"),
            "a resumed stop is not a session now"
        );
        let groups: Vec<&str> = list.iter().map(|r| r["group"].as_str().unwrap()).collect();
        let mut sorted = groups.clone();
        sorted.sort_by_key(|g| GROUPS.iter().position(|x| x == g));
        assert_eq!(groups, sorted, "rows come in the groups' order");
        assert!(answer["degraded"].as_array().unwrap().is_empty());
    }

    #[test]
    fn another_machines_answer_reads_as_its_heading_says() {
        let rows =
            json!({"ok": true, "data": {"rows": [], "counts": {}, "degraded": []}}).to_string();
        assert_eq!(there(Ok(rows))["status"], "read");
        let old = r#"{"ok":false,"error":"Invalid arguments. Run switchboard --help."}"#;
        assert_eq!(there(Err(old.into()))["status"], "too_old");
        assert_eq!(
            there(Err("zsh: command not found: switchboard".into()))["status"],
            "no_switchboard"
        );
        assert_eq!(
            there(Err(
                "ssh: connect to host x port 22: Operation timed out".into()
            ))["status"],
            "unreachable"
        );
        assert_eq!(there(Ok("garbage".into()))["status"], "bad_answer");
        let script = attach_script("/a b/switchboard", "/data", "L'1", Some("studio"));
        assert_eq!(script, "#!/bin/sh\nexec '/a b/switchboard' --data-dir '/data' attach 'L'\\''1' --target 'studio'\n");
    }

    #[test]
    fn one_row_per_session_and_every_ended_one_can_be_continued_as_itself() {
        let mut tree = tree();
        // S3 runs (pid 14) and has a waiting stop: one row, waiting on its limit.
        let recovery = json!({"sessions": [
            {"ready": false, "probe_at": 2000, "stop": {"state": "waiting", "event": "E7", "provider": "claude", "directory": "/w/c", "session": "S3", "account": "A2", "pool": "default", "stopped_at": 960}}
        ]});
        // An ended launch with its session id, and one whose `ended` an older path wrote as a word.
        tree["launches"].as_array_mut().unwrap().extend([
            json!({"launch_id": "L5", "state": "ended", "ended": {"kind": "exited", "at": 995}, "provider": "claude", "folder": "/w/f", "launched_at": 900, "session_id": "S5"}),
            json!({"launch_id": "L6", "state": "ended", "ended": "limit", "provider": "claude", "folder": "/w/g", "launched_at": 800, "session_id": "S6"}),
        ]);
        let ledger = [
            json!({"session": "S7", "config": "/h", "cwd": "/w/h", "reason": "idle", "at": 990, "by": "duty"}),
            json!({"session": "S5", "config": "/h", "cwd": "/w/f", "reason": "idle", "at": 980, "by": "duty"}),
        ];
        let labels = HashMap::new();
        let answer = build(&Sources {
            tree: &tree,
            recovery: &recovery,
            labels: &labels,
            attachable: &HashSet::new(),
            resumable: &ledger,
        });
        let list = answer["rows"].as_array().unwrap();
        let of = |session: &str| {
            list.iter()
                .filter(|r| r["session_id"] == session)
                .cloned()
                .collect::<Vec<_>>()
        };
        let s3 = of("S3");
        assert_eq!(s3.len(), 1, "{s3:?}");
        assert_eq!(
            (
                s3[0]["group"].clone(),
                s3[0]["why"].clone(),
                s3[0]["probe_at"].clone()
            ),
            (json!("waiting"), json!("limit"), json!(2000))
        );
        let s5 = of("S5");
        assert_eq!(
            s5.len(),
            1,
            "the launch and the ledger line are one session"
        );
        assert_eq!(s5[0]["acts"], json!(["continue"]));
        assert_eq!(of("S6")[0]["why"], "limit");
        let s7 = of("S7");
        assert_eq!(
            (
                s7[0]["group"].clone(),
                s7[0]["why"].clone(),
                s7[0]["acts"].clone()
            ),
            (json!("ended"), json!("closed_idle"), json!(["continue"]))
        );
    }

    #[test]
    fn a_recovery_that_could_not_be_read_is_named_not_guessed() {
        let answer = rows(&Value::Null);
        assert_eq!(answer["degraded"], json!(["recovery"]));
        assert_eq!(answer["counts"]["waiting"], 0);
    }
}
