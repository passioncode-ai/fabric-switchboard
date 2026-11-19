//! SB-510 (plan 10 §B): a watch over the owner that is not the owner.
//!
//! `switchboard watchdog --once` runs every five minutes from a system job of its own
//! (`watchdog_job`), needs no owner and takes no store lock. It checks four things and only
//! reports them, never acting on any: the owner answers (its `control.json`, its process and its
//! identity on the control port within `control::PROBE_SECONDS`, read beside `owner-run.json`);
//! no act of the duty officer or of a dispatched run's stop has stood half done (an intent line in
//! `duty.jsonl` with no outcome) for `ACT_SECONDS`; no dispatched run has been gone (its process
//! ended, no end recorded) for `GONE_SECONDS`; no session has run in two processes for
//! `DUPLICATE_SECONDS` (the last two from the owner's own `session-watch.json`, read only while the
//! owner answers). The answer is written to `<data>/watchdog.json` (0600, codes, counts, ids and
//! times, never text a session wrote) each run; *Needs attention* (reason `watchdog`), MCP
//! `switchboard_attention` and the `agent-health/1` check `watchdog` read it while it is no older
//! than `STALE_SECONDS`. It sends nothing anywhere: an agent relays what it says.
//!
//! `<data>/watchdog` (`on` or `off`, absent: on) is the person's switch; off, a run writes nothing
//! and the job is taken out.
use serde_json::{json, Value};
use std::path::Path;

pub const FILE: &str = "watchdog.json";
pub const SETTING: &str = "watchdog";
pub const VERSION: u32 = 1;
/// How often the job runs.
pub const INTERVAL_SECONDS: u64 = 300;
/// A report older than this says nothing any more (three runs missed).
pub const STALE_SECONDS: i64 = 900;
/// An intent with no outcome this long is an act left half done.
pub const ACT_SECONDS: i64 = 600;
/// A dispatched run gone this long with no end recorded.
pub const GONE_SECONDS: i64 = 1800;
/// A session in two processes this long.
pub const DUPLICATE_SECONDS: i64 = 900;
/// A dispatched run's stop intent older than this is history, not a finding.
const STOP_WINDOW: i64 = 86_400;
/// An owner that started this recently may not have published its port yet.
const STARTING_SECONDS: i64 = 120;

/// The owner ended without its drain (a crash) and nothing answers.
pub const OWNER_DOWN: &str = "owner_down";
/// The owner's process runs but it did not prove itself on its port in time.
pub const OWNER_NOT_ANSWERING: &str = "owner_not_answering";
/// The owner quit through its drain and nothing started it again.
pub const OWNER_STOPPED: &str = "owner_stopped";
/// An act of the duty officer or a dispatched run's stop left half done.
pub const ACT_UNFINISHED: &str = "act_unfinished";
/// A dispatched run gone with no end recorded.
pub const BOARD_RUN_GONE: &str = "board_run_gone";
/// One session in two processes.
pub const DUPLICATE_SESSION: &str = "duplicate_session";

pub const UNWRITTEN: &str =
    "The watchdog setting could not be saved. Check Switchboard's data folder, then retry.";

/// The owner codes: what only a reader that is not the owner can still say.
pub fn owner_code(code: &str) -> bool {
    matches!(code, OWNER_DOWN | OWNER_NOT_ANSWERING | OWNER_STOPPED)
}

/// The codes read from the owner's `session-watch.json`, which every reader of Needs attention
/// lists itself as `session_gone` and `session_duplicate` with their acts, with or without a
/// running owner (review rounds 2 and 3 of PR #852: never the same finding twice).
pub fn watch_code(code: &str) -> bool {
    matches!(code, BOARD_RUN_GONE | DUPLICATE_SESSION)
}

fn severity(code: &str) -> &'static str {
    match code {
        OWNER_DOWN | OWNER_NOT_ANSWERING => "fail",
        _ => "warn",
    }
}

/// Whether the person turned the watchdog off. Unreadable reads as on.
pub fn is_off(root: &Path) -> bool {
    switchboard_core::private_fs::read_private(&root.join(SETTING), 64)
        .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).trim() == "off")
}

/// Saves the switch (0600, by rename).
pub fn set(root: &Path, on: bool) -> Result<(), String> {
    switchboard_core::private_fs::private_write(
        &root.join(SETTING),
        if on { b"on\n" } else { b"off\n" },
    )
    .map_err(|_| UNWRITTEN.to_string())?;
    crate::oplog::event("watchdog", &[("on", crate::oplog::Field::Flag(on))]);
    Ok(())
}

/// The owner as the probe and its run record show it: `answers`, `starting`, `stopped`, `down`
/// or `not_answering`. Pure, for tests.
pub fn owner_state(
    probe: &crate::control::Probe,
    run: Option<&crate::bugs::Run>,
    alive: impl Fn(u32) -> bool,
    boot: Option<i64>,
    now: i64,
) -> &'static str {
    use crate::control::Probe;
    // A run that started before this boot cannot still run, whatever process holds its pid now.
    let alive =
        |run: &crate::bugs::Run| boot.is_none_or(|boot| run.started_at >= boot) && alive(run.pid);
    match probe {
        Probe::Answers { .. } => "answers",
        Probe::NoAnswer | Probe::Unreadable => "not_answering",
        Probe::NoDescriptor | Probe::OwnerGone | Probe::Refused => match run {
            Some(run) if !run.clean && alive(run) => {
                if now - run.started_at < STARTING_SECONDS {
                    "starting"
                } else {
                    "not_answering"
                }
            }
            Some(run) if !run.clean => "down",
            _ => "stopped",
        },
    }
}

/// The acts of `duty.jsonl` left half done for at least `ACT_SECONDS`. The duty officer's own by
/// its own rule (`duty::interrupted`: the newest intent per session with no outcome that settles
/// it, under an hour old, which is as long as its next check finishes a cut act), so a warning
/// never outlives what the owner can still settle. A dispatched run's stop by `launch_id`: an
/// intent with no outcome line under a day old whose run is still open (`run_open`: its record
/// has no end and its process runs), since the dispatch pass records some ends without a journal
/// line. Pure, for tests.
pub fn unfinished_acts(lines: &[Value], now: i64, run_open: impl Fn(&str) -> bool) -> Vec<Value> {
    let mut open: Vec<Value> = crate::duty::interrupted(lines, now)
        .into_iter()
        .filter(|o| o["at"].as_i64().is_some_and(|at| now - at >= ACT_SECONDS))
        .collect();
    let mut stops: Vec<&Value> = Vec::new();
    for line in lines
        .iter()
        .filter(|l| l["action"] == crate::dispatch::JOURNAL_ACTION)
    {
        stops.retain(|o| o["launch_id"] != line["launch_id"]);
        if line["phase"] == "intent" {
            stops.push(line);
        }
    }
    open.extend(
        stops
            .into_iter()
            .filter(|o| {
                o["at"]
                    .as_i64()
                    .is_some_and(|at| now - at >= ACT_SECONDS && now - at < STOP_WINDOW)
            })
            .filter(|o| o["launch_id"].as_str().is_some_and(&run_open))
            .cloned(),
    );
    open
}

/// SB-510: whether a dispatched run's stop is still open: its record has no end and its home's
/// process runs (unknown counts as running, as the session watch reads it).
fn run_open(root: &Path, id: &str) -> bool {
    if !crate::launches::valid_id(id) {
        return false;
    }
    let Some(record) = crate::launches::get(root, id) else {
        return false;
    };
    record["ended"].is_null()
        && crate::launches::own_home(root, &record)
            .is_some_and(|home| crate::launch::session_running(&home).unwrap_or(true))
}

/// The kept watch's findings of `kind` (with `dispatch` for a gone run) that have stood at least
/// `seconds`: since their first sighting, else since the kept file was written.
fn standing(watch: &Value, kind: &str, seconds: i64, now: i64) -> Vec<Value> {
    let written = watch["at"].as_i64();
    watch["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["kind"] == kind)
        .filter(|f| kind != crate::session_watch::GONE || f["dispatch"] == true)
        .filter(|f| {
            f[crate::session_watch::FIRST_SEEN]
                .as_i64()
                .or(written)
                .is_some_and(|first| now - first >= seconds)
        })
        .cloned()
        .collect()
}

fn finding(code: &str, ids: Vec<Value>, first_seen: i64) -> Value {
    json!({"code": code, "severity": severity(code), "count": ids.len().max(1),
        "ids": ids, "first_seen": first_seen})
}

/// What one run finds. `previous` is the last report: an owner finding keeps the time it was
/// first seen. Pure, for tests.
pub fn assess(
    owner: &str,
    acts: &[Value],
    watch: &Value,
    previous: Option<&Value>,
    now: i64,
) -> Vec<Value> {
    let first = |code: &str| {
        previous
            .and_then(|p| p["findings"].as_array())
            .into_iter()
            .flatten()
            .find(|f| f["code"] == code)
            .and_then(|f| f["first_seen"].as_i64())
            .filter(|t| *t <= now)
            .unwrap_or(now)
    };
    let mut out = Vec::new();
    let owner_finding = match owner {
        "down" => Some(OWNER_DOWN),
        "not_answering" => Some(OWNER_NOT_ANSWERING),
        "stopped" => Some(OWNER_STOPPED),
        _ => None,
    };
    if let Some(code) = owner_finding {
        let mut f = finding(code, Vec::new(), first(code));
        // Review round 2 of PR #852: one 5 s miss of a background job on a busy machine is not an
        // outage: `owner_not_answering` fails only when the previous run, still current, missed too.
        if code == OWNER_NOT_ANSWERING {
            let missed_before = previous.is_some_and(|p| {
                p["checked_at"]
                    .as_i64()
                    .is_some_and(|at| at <= now && now - at <= STALE_SECONDS)
                    && p["findings"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|f| f["code"] == OWNER_NOT_ANSWERING))
            });
            if !missed_before {
                f["severity"] = "warn".into();
            }
        }
        out.push(f);
    }
    if !acts.is_empty() {
        let ids = acts
            .iter()
            .map(|a| {
                if a["session"].is_string() {
                    a["session"].clone()
                } else {
                    a["launch_id"].clone()
                }
            })
            .collect();
        let oldest = acts.iter().filter_map(|a| a["at"].as_i64()).min();
        out.push(finding(ACT_UNFINISHED, ids, oldest.unwrap_or(now)));
    }
    // The watch is the owner's: read only while the owner answers, when it is current.
    if owner == "answers" {
        for (kind, code, seconds, id) in [
            (
                crate::session_watch::GONE,
                BOARD_RUN_GONE,
                GONE_SECONDS,
                "launch_id",
            ),
            (
                crate::session_watch::DUPLICATE,
                DUPLICATE_SESSION,
                DUPLICATE_SECONDS,
                "session_id",
            ),
        ] {
            let found = standing(watch, kind, seconds, now);
            if !found.is_empty() {
                let oldest = found
                    .iter()
                    .filter_map(|f| f[crate::session_watch::FIRST_SEEN].as_i64())
                    .min()
                    .or(watch["at"].as_i64())
                    .unwrap_or(now);
                let ids = found.iter().map(|f| f[id].clone()).collect();
                out.push(finding(code, ids, oldest));
            }
        }
    }
    out
}

/// The last report, whatever its age.
pub fn read(root: &Path) -> Option<Value> {
    switchboard_core::private_fs::read_private(&root.join(FILE), 256 * 1024)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v["version"] == VERSION)
}

/// The findings a reader shows now: from a report no older than `STALE_SECONDS`, without the
/// watch's own codes (`watch_code`) and, when the reader is the running owner, without the owner
/// codes (it answers, whatever an earlier run saw). `None`: the watchdog is off, never ran, or its
/// report is stale, which is not the same as nothing found.
pub fn current(root: &Path, now: i64, owner_running: bool) -> Option<Vec<Value>> {
    if is_off(root) {
        return None;
    }
    let report = read(root)?;
    let at = report["checked_at"].as_i64()?;
    if now - at > STALE_SECONDS || at > now + 60 {
        return None;
    }
    Some(
        report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| {
                let code = f["code"].as_str().unwrap_or_default();
                // The watch's own codes always: Needs attention reads `session-watch.json` itself,
                // offline too (review round 3 of PR #852).
                !(watch_code(code) || (owner_running && owner_code(code)))
            })
            .cloned()
            .collect(),
    )
}

/// What the health report's `watchdog` check reads (SB-510 review round 1).
#[derive(Debug, Clone, PartialEq)]
pub enum Watched {
    /// No job runs here (off, never installed, Windows, `serve`, a development build, another data
    /// folder) and no current report: no check, since nothing is expected to watch.
    NotWatched,
    /// The job is installed and on, and its report is missing or older than `STALE_SECONDS`.
    Stale,
    /// A current report's findings, as `current` answers them.
    Current(Vec<Value>),
}

/// `Watched` for a reader: a current report wherever it came from; else `Stale` only when the
/// job is installed (`job_installed`) and the watchdog is on.
pub fn watched(root: &Path, now: i64, owner_running: bool, job_installed: bool) -> Watched {
    match current(root, now, owner_running) {
        Some(findings) => Watched::Current(findings),
        None if job_installed && !is_off(root) => Watched::Stale,
        None => Watched::NotWatched,
    }
}

/// One run: the owner probed, the journal and the watch read, the report written. Off, or over a
/// folder no owner ever ran in, nothing is written.
pub async fn once(root: &Path, now: i64) -> Value {
    if is_off(root) {
        return json!({"checked": false, "reason": "off"});
    }
    let run = crate::bugs::last_run(root);
    if run.is_none() && std::fs::symlink_metadata(root.join("control.json")).is_err() {
        return json!({"checked": false, "reason": "no_owner_record"});
    }
    let probe = crate::control::probe(
        root,
        std::time::Duration::from_secs(crate::control::PROBE_SECONDS),
    )
    .await;
    let owner = owner_state(
        &probe,
        run.as_ref(),
        crate::bugs::alive,
        crate::boot_resume::boot(),
        now,
    );
    let acts = unfinished_acts(&crate::duty::journal(root), now, |id| run_open(root, id));
    let watch: Value = switchboard_core::private_fs::read_private(
        &root.join(crate::session_watch::FILE),
        4 * 1024 * 1024,
    )
    .ok()
    .and_then(|b| serde_json::from_slice(&b).ok())
    .unwrap_or(Value::Null);
    let previous = read(root);
    let findings = assess(owner, &acts, &watch, previous.as_ref(), now);
    let version = match &probe {
        crate::control::Probe::Answers { version } => json!(version),
        _ => Value::Null,
    };
    let report = json!({
        "version": VERSION,
        "checked_at": now,
        "checked_by": env!("CARGO_PKG_VERSION"),
        "owner": owner,
        "owner_version": version,
        "findings": findings,
    });
    let written = serde_json::to_vec(&report)
        .ok()
        .is_some_and(|b| switchboard_core::private_fs::private_write(&root.join(FILE), &b).is_ok());
    let mut answer = report;
    answer["checked"] = true.into();
    answer["written"] = written.into();
    answer
}

/// `switchboard watchdog` and MCP `switchboard_watchdog` without a change: the switch, the job on
/// this host, and the last report with whether it is current.
pub fn status(root: &Path, host: Option<&crate::watchdog_job::Host>, now: i64) -> Value {
    let report = read(root);
    let fresh = report
        .as_ref()
        .and_then(|r| r["checked_at"].as_i64())
        .is_some_and(|at| now - at <= STALE_SECONDS && at <= now + 60);
    json!({
        "on": !is_off(root),
        "job": host.map_or("unknown", crate::watchdog_job::state),
        "interval_seconds": INTERVAL_SECONDS,
        "report": report,
        "current": fresh,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bugs::Run;
    use crate::control::Probe;
    use crate::session_watch::{DUPLICATE, FIRST_SEEN, GONE};

    fn run(clean: bool, started_at: i64) -> Run {
        Run {
            pid: 77,
            started_at,
            version: "0.7.121".into(),
            clean,
        }
    }

    #[test]
    fn the_owner_is_answering_starting_stopped_down_or_silent() {
        let now = 10_000;
        let answers = Probe::Answers { version: None };
        assert_eq!(owner_state(&answers, None, |_| false, None, now), "answers");
        assert_eq!(
            owner_state(&Probe::NoAnswer, Some(&run(false, 0)), |_| true, None, now),
            "not_answering"
        );
        // Quit through its drain: stopped. Ended without it: down (a crash).
        for probe in [Probe::NoDescriptor, Probe::OwnerGone, Probe::Refused] {
            assert_eq!(
                owner_state(&probe, Some(&run(true, 0)), |_| false, None, now),
                "stopped"
            );
            assert_eq!(
                owner_state(&probe, Some(&run(false, 0)), |_| false, None, now),
                "down"
            );
        }
        // Its process runs and has published no port: starting for two minutes, then silent.
        let alive = Some(run(false, now - 30));
        assert_eq!(
            owner_state(&Probe::NoDescriptor, alive.as_ref(), |_| true, None, now),
            "starting"
        );
        let long = Some(run(false, now - 600));
        assert_eq!(
            owner_state(&Probe::NoDescriptor, long.as_ref(), |_| true, None, now),
            "not_answering"
        );
        assert_eq!(
            owner_state(&Probe::NoDescriptor, None, |_| true, None, now),
            "stopped"
        );
        // A pid another process holds after a reboot: a run started before this boot is down.
        let before_boot = Some(run(false, now - 600));
        assert_eq!(
            owner_state(
                &Probe::NoDescriptor,
                before_boot.as_ref(),
                |_| true,
                Some(now - 300),
                now
            ),
            "down"
        );
        assert_eq!(
            owner_state(
                &Probe::NoDescriptor,
                before_boot.as_ref(),
                |_| true,
                Some(now - 900),
                now
            ),
            "not_answering"
        );
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(
            crate::boot_resume::boot().is_some_and(|b| b > 0 && b <= crate::session_watch::now())
        );
    }

    #[test]
    fn an_intent_with_no_outcome_for_ten_minutes_is_an_act_left_half_done() {
        let now = 100_000;
        let intent = |session: &str, at: i64| json!({"at": at, "phase": "intent", "action": "close", "session": session, "pid": 1});
        let outcome = |session: &str| json!({"at": now - 3000, "action": "close", "outcome": "closed", "session": session, "pid": 1});
        let stop = |id: &str, at: i64, phase: bool| {
            let mut l =
                json!({"at": at, "action": crate::dispatch::JOURNAL_ACTION, "launch_id": id});
            if phase {
                l["phase"] = "intent".into();
            } else {
                l["outcome"] = "stopped".into();
            }
            l
        };
        let lines = vec![
            intent("done", now - 3000),
            outcome("done"),
            intent("cut", now - 1200),
            intent("young", now - 120),
            // Past the hour the duty officer still finishes a cut act: nothing settles it now.
            intent("past_the_hour", now - 5400),
            intent("ancient", now - 2 * 86_400),
            stop("L-done", now - 2000, true),
            stop("L-done", now - 1990, false),
            stop("L-cut", now - 900, true),
            // The dispatch pass recorded its end without a journal line.
            stop("L-ended", now - 900, true),
        ];
        let open = unfinished_acts(&lines, now, |id| id != "L-ended");
        let names: Vec<&str> = open
            .iter()
            .map(|o| o["session"].as_str().or(o["launch_id"].as_str()).unwrap())
            .collect();
        assert_eq!(names, ["cut", "L-cut"]);
        // A newer intent of the same session stands for the older one.
        let again = vec![intent("s", now - 5000), intent("s", now - 100)];
        assert!(unfinished_acts(&again, now, |_| true).is_empty());
    }

    #[test]
    fn the_owners_watch_counts_only_findings_that_have_stood_long_enough() {
        let now = 100_000;
        let watch = json!({"at": now - 60, "findings": [
            {"kind": GONE, "launch_id": "L-old", "dispatch": true, FIRST_SEEN: now - 2000},
            {"kind": GONE, "launch_id": "L-new", "dispatch": true, FIRST_SEEN: now - 600},
            {"kind": GONE, "launch_id": "L-cli", "dispatch": false, FIRST_SEEN: now - 9000},
            {"kind": DUPLICATE, "session_id": "s-old", FIRST_SEEN: now - 1000},
            {"kind": DUPLICATE, "session_id": "s-new", FIRST_SEEN: now - 100},
            {"kind": "stuck", "launch_id": "L-stuck", FIRST_SEEN: now - 9000},
        ]});
        let found = assess("answers", &[], &watch, None, now);
        let codes: Vec<(&str, Value)> = found
            .iter()
            .map(|f| (f["code"].as_str().unwrap(), f["ids"].clone()))
            .collect();
        assert_eq!(
            codes,
            [
                (BOARD_RUN_GONE, json!(["L-old"])),
                (DUPLICATE_SESSION, json!(["s-old"]))
            ]
        );
        assert_eq!(found[0]["first_seen"], now - 2000);
        assert_eq!(found[0]["severity"], "warn");
        // A kept watch from an older owner (no first sighting) counts from when it was written.
        let old = json!({"at": now - 3600, "findings": [{"kind": DUPLICATE, "session_id": "s"}]});
        assert_eq!(assess("answers", &[], &old, None, now).len(), 1);
        // The owner does not answer: its watch is not current, only the owner is reported.
        let down = assess("down", &[], &watch, None, now);
        assert_eq!(down.len(), 1);
        assert_eq!(
            (down[0]["code"].clone(), down[0]["severity"].clone()),
            (json!(OWNER_DOWN), json!("fail"))
        );
        // Review round 2: one miss warns, two runs in a row fail.
        let once = assess("not_answering", &[], &Value::Null, None, now);
        assert_eq!(once[0]["severity"], "warn");
        let before = json!({"checked_at": now - 300, "findings": once});
        let twice = assess("not_answering", &[], &Value::Null, Some(&before), now);
        assert_eq!(twice[0]["severity"], "fail");
        let stale = json!({"checked_at": now - 3600, "findings": once});
        assert_eq!(
            assess("not_answering", &[], &Value::Null, Some(&stale), now)[0]["severity"],
            "warn",
            "a miss an hour ago is not the run before"
        );
        // The owner's finding keeps the time it was first seen.
        let earlier = json!({"findings": [{"code": OWNER_DOWN, "first_seen": now - 900}]});
        let kept = assess("down", &[], &Value::Null, Some(&earlier), now);
        assert_eq!(kept[0]["first_seen"], now - 900);
        assert!(assess("starting", &[], &Value::Null, None, now).is_empty());
        assert_eq!(
            assess("stopped", &[], &Value::Null, None, now)[0]["severity"],
            "warn"
        );
    }

    #[test]
    fn a_running_owner_never_reads_back_what_it_already_lists_itself() {
        // Review round 2 of PR #852: a gone board run and a duplicate come from the owner's own
        // watch, which Needs attention already lists with their acts.
        let root = tempfile::tempdir().unwrap();
        let now = 100_000;
        let findings = [OWNER_DOWN, ACT_UNFINISHED, BOARD_RUN_GONE, DUPLICATE_SESSION]
            .map(|code| json!({"code": code, "severity": "warn", "count": 1, "ids": [], "first_seen": now}));
        let report = json!({"version": VERSION, "checked_at": now, "findings": findings});
        std::fs::write(root.path().join(FILE), report.to_string()).unwrap();
        let codes = |running| -> Vec<String> {
            current(root.path(), now, running)
                .unwrap()
                .iter()
                .map(|f| f["code"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(codes(true), [ACT_UNFINISHED]);
        // Offline the owner's state is said, its watch's findings still are not.
        assert_eq!(codes(false), [OWNER_DOWN, ACT_UNFINISHED]);
    }

    #[tokio::test]
    async fn a_run_writes_a_private_report_that_readers_take_while_it_is_current() {
        let root = tempfile::tempdir().unwrap();
        let now = crate::session_watch::now();
        // No owner ever ran here: nothing to watch, nothing written.
        assert_eq!(once(root.path(), now).await["reason"], "no_owner_record");
        assert!(read(root.path()).is_none());
        // An owner that quit through its drain, and an act its duty officer left half done.
        let record = serde_json::to_vec(&run(true, now - 3600)).unwrap();
        std::fs::write(root.path().join(crate::bugs::RUN_FILE), record).unwrap();
        std::fs::write(
            root.path().join(crate::duty::JOURNAL),
            format!(
                "{}\n",
                json!({"at": now - 700, "phase": "intent", "action": "close", "session": "S1", "pid": 9})
            ),
        )
        .unwrap();
        let report = once(root.path(), now).await;
        assert_eq!(report["owner"], "stopped");
        assert_eq!(report["written"], true);
        let codes: Vec<&str> = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, [OWNER_STOPPED, ACT_UNFINISHED]);
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.path().join(FILE))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
        // An offline reader sees both; the owner itself leaves its own code out.
        assert_eq!(current(root.path(), now, false).unwrap().len(), 2);
        let as_owner = current(root.path(), now, true).unwrap();
        assert_eq!(as_owner.len(), 1);
        assert_eq!(as_owner[0]["code"], ACT_UNFINISHED);
        // Stale after fifteen minutes: no answer, never "nothing found".
        let later = now + STALE_SECONDS + 1;
        assert!(current(root.path(), later, false).is_none());
        // The health check: unknown only where an installed job's report went stale.
        assert!(
            matches!(watched(root.path(), now, false, false), Watched::Current(f) if f.len() == 2)
        );
        assert_eq!(watched(root.path(), later, false, true), Watched::Stale);
        assert_eq!(
            watched(root.path(), later, false, false),
            Watched::NotWatched
        );
        let state = status(root.path(), None, now);
        assert_eq!(
            (
                state["on"].clone(),
                state["current"].clone(),
                state["job"].clone()
            ),
            (json!(true), json!(true), json!("unknown"))
        );
        // Off: a run writes nothing and readers have no answer.
        set(root.path(), false).unwrap();
        assert!(is_off(root.path()));
        assert_eq!(once(root.path(), now + 300).await["reason"], "off");
        assert!(current(root.path(), now, false).is_none());
        assert_eq!(
            watched(root.path(), later, false, true),
            Watched::NotWatched
        );
        assert_eq!(read(root.path()).unwrap()["checked_at"], now);
        set(root.path(), true).unwrap();
        assert!(!is_off(root.path()));
    }
}
