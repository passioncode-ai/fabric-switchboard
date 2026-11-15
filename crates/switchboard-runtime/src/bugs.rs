//! W3 part 2 (SB-379, plan `docs/plans/2026-11-15-quality-waves` W3): crashes found at the next
//! start, and one local list of what went wrong.
//!
//! The owner (the app, `switchboard serve`) of the real data folder writes `owner-run.json`
//! (0600: its pid, start time and version, `clean: false`) when it starts and sets `clean: true`
//! when its drain ends (or its hard exit fires). A start that finds the previous run not clean
//! and its process gone records a crash: the previous run's start and version, and, on macOS,
//! the exception type and signal of the system's crash report for that process (`.ips`, only
//! those two codes are read, never a stack, a path or a message). The crash is logged as
//! `owner_crashed` (counted in the daily `app_error` like every error) and appended to
//! `bugs.jsonl` (0600, the newest 200 lines). `switchboard bugs`, MCP `switchboard_bugs` and the
//! window read `list`: those crashes and the error counts of the last seven days when usage counts
//! are on. Nothing here is a text a person or a provider wrote.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const RUN_FILE: &str = "owner-run.json";
pub const FILE: &str = "bugs.jsonl";
/// Lines kept in `bugs.jsonl`.
const KEEP: usize = 200;
/// Crashes `list` answers.
const LIST: usize = 20;
/// Days of error counts `list` answers.
const DAYS: i64 = 7;
/// The process names of the owner in a macOS crash report: the app and `switchboard serve`.
const PROCESSES: &[&str] = &["fabric-switchboard", "switchboard", "Fabric Switchboard"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub pid: u32,
    pub started_at: i64,
    pub version: String,
    pub clean: bool,
}

/// The data folder of the owner that wrote this process's run file, for the hard exit.
static ROOT: OnceLock<PathBuf> = OnceLock::new();

fn read_run(root: &Path) -> Option<Run> {
    std::fs::read(root.join(RUN_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn write_run(root: &Path, run: &Run) {
    if let Ok(bytes) = serde_json::to_vec(run) {
        let _ = switchboard_core::private_fs::private_write(&root.join(RUN_FILE), &bytes);
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    pid != 0 && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0
}
#[cfg(not(unix))]
fn alive(_: u32) -> bool {
    false
}

/// What a start learns from the previous run: `None` when it ended cleanly, never ran, or its
/// process still runs (then it is not ours to judge).
pub fn crashed(previous: Option<&Run>, alive: impl Fn(u32) -> bool) -> Option<&Run> {
    previous.filter(|run| !run.clean && !alive(run.pid))
}

/// The owner's start: records the previous run's crash, if any, then this run.
pub fn begin(root: &Path, now: i64) {
    let _ = ROOT.set(root.to_owned());
    let previous = read_run(root);
    if let Some(run) = crashed(previous.as_ref(), alive) {
        let (exception, signal, at) = report_of(run)
            .map(|(e, s, at)| (Some(e), s, Some(at)))
            .unwrap_or((None, None, None));
        let record = crash_record(run, exception.as_deref(), signal.as_deref(), at, now);
        crate::oplog::event(
            "owner_crashed",
            &[
                (
                    "exception",
                    crate::oplog::Field::Code(code_static(exception.as_deref())),
                ),
                (
                    "uptime",
                    crate::oplog::Field::Code(
                        record["uptime"].as_str().map_or("unknown", uptime_static),
                    ),
                ),
            ],
        );
        append(root, &record);
    }
    write_run(
        root,
        &Run {
            pid: std::process::id(),
            started_at: now,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            clean: false,
        },
    );
}

/// The owner's drain ended (or its hard exit fired): this run ends cleanly.
pub fn end(root: &Path) {
    if let Some(mut run) = read_run(root) {
        if run.pid == std::process::id() && !run.clean {
            run.clean = true;
            write_run(root, &run);
        }
    }
}

/// `end` for the data folder `begin` was given; the hard exit knows no folder.
pub fn end_this_run() {
    if let Some(root) = ROOT.get() {
        end(root);
    }
}

/// One crash line: the previous run, what its crash report says and how long it had run.
pub fn crash_record(
    run: &Run,
    exception: Option<&str>,
    signal: Option<&str>,
    crashed_at: Option<i64>,
    now: i64,
) -> Value {
    let uptime = crashed_at.map(|at| uptime_bucket(at - run.started_at));
    json!({
        "at": now,
        "kind": "owner_crash",
        "version": run.version,
        "started_at": run.started_at,
        "crashed_at": crashed_at,
        "uptime": uptime.unwrap_or("unknown"),
        "exception": exception,
        "signal": signal,
    })
}

/// How long a run lasted, in five buckets: no exact figure leaves.
pub fn uptime_bucket(seconds: i64) -> &'static str {
    match seconds {
        s if s < 60 => "under_1m",
        s if s < 600 => "under_10m",
        s if s < 3600 => "under_1h",
        s if s < 86_400 => "under_1d",
        _ => "over_1d",
    }
}
fn uptime_static(bucket: &str) -> &'static str {
    ["under_1m", "under_10m", "under_1h", "under_1d", "over_1d"]
        .into_iter()
        .find(|b| *b == bucket)
        .unwrap_or("unknown")
}

/// The exception types macOS names (`EXC_BAD_ACCESS`), lower-cased for the log's codes; any other
/// value reads `other`, and none `none`.
const EXCEPTIONS: &[&str] = &[
    "exc_bad_access",
    "exc_bad_instruction",
    "exc_arithmetic",
    "exc_emulation",
    "exc_software",
    "exc_breakpoint",
    "exc_syscall",
    "exc_mach_syscall",
    "exc_rpc_alert",
    "exc_crash",
    "exc_resource",
    "exc_guard",
    "exc_corpse_notify",
];
fn code_static(exception: Option<&str>) -> &'static str {
    match exception {
        None => "none",
        Some(e) => {
            let lower = e.to_ascii_lowercase();
            EXCEPTIONS
                .iter()
                .copied()
                .find(|c| *c == lower)
                .unwrap_or("other")
        }
    }
}

/// An exception type or signal as macOS writes it (`EXC_BAD_ACCESS`, `SIGABRT`): capitals,
/// digits and `_`, at most 32; anything else is not read.
fn sane(code: &str) -> Option<String> {
    let ok = !code.is_empty()
        && code.len() <= 32
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    ok.then(|| code.to_owned())
}

/// The exception type and signal of one `.ips` report, when it is the owner's: its first line is
/// a JSON header naming the process, the rest the report. Nothing else of it is kept.
pub fn parse_report(text: &str) -> Option<(String, Option<String>)> {
    let (head, body) = text.split_once('\n')?;
    let head: Value = serde_json::from_str(head).ok()?;
    let name = head["name"]
        .as_str()
        .or_else(|| head["app_name"].as_str())?;
    if !PROCESSES.contains(&name) {
        return None;
    }
    let body: Value = serde_json::from_str(body).ok()?;
    let exception = sane(body["exception"]["type"].as_str()?)?;
    let signal = body["exception"]["signal"].as_str().and_then(sane);
    Some((exception, signal))
}

/// The newest owner crash report written after `run` started (macOS only), with its time.
fn report_of(run: &Run) -> Option<(String, Option<String>, i64)> {
    // A test never reads this machine's own crash reports.
    if !cfg!(target_os = "macos") || cfg!(test) {
        return None;
    }
    let dir = dirs::home_dir()?.join("Library/Logs/DiagnosticReports");
    newest_report(&dir, run.started_at)
}

pub fn newest_report(dir: &Path, since: i64) -> Option<(String, Option<String>, i64)> {
    let mut found: Option<(String, Option<String>, i64)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".ips") || !PROCESSES.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let Some(at) = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
        else {
            continue;
        };
        if at < since || found.as_ref().is_some_and(|f| f.2 >= at) {
            continue;
        }
        // A report is a few hundred KiB at most; one that is larger is not read.
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > 4 << 20 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if let Some((exception, signal)) = parse_report(&text) {
            found = Some((exception, signal, at));
        }
    }
    found
}

fn append(root: &Path, record: &Value) {
    let path = root.join(FILE);
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .map(|t| t.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    lines.push(record.to_string());
    let start = lines.len().saturating_sub(KEEP);
    let text = lines[start..].join("\n") + "\n";
    let _ = switchboard_core::private_fs::private_write(&path, text.as_bytes());
}

/// `switchboard bugs`: the newest crashes first, and the error counts of the last days when
/// usage counts are on (`counting`); a run in progress is `running`.
pub fn list(root: &Path, now: i64) -> Value {
    let mut crashes: Vec<Value> = std::fs::read_to_string(root.join(FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    crashes.reverse();
    let total = crashes.len();
    crashes.truncate(LIST);
    let today = now.div_euclid(86_400);
    let errors = crate::errors::recent_days(root, today - DAYS + 1);
    let run = read_run(root);
    json!({
        "crashes": crashes,
        "crashes_total": total,
        "errors": errors,
        "counting": root.join(crate::errors::FILE).exists(),
        "running": run.map(|r| json!({"started_at": r.started_at, "version": r.version})),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(clean: bool) -> Run {
        Run {
            pid: 4242,
            started_at: 1_000,
            version: "0.7.98".into(),
            clean,
        }
    }

    #[test]
    fn only_an_unclean_run_whose_process_is_gone_crashed() {
        assert!(crashed(Some(&run(false)), |_| false).is_some());
        assert!(crashed(Some(&run(true)), |_| false).is_none(), "drained");
        assert!(crashed(Some(&run(false)), |_| true).is_none(), "still runs");
        assert!(crashed(None, |_| false).is_none(), "first start");
    }

    #[test]
    fn a_crash_report_gives_two_codes_and_only_the_owners() {
        let head = |name: &str| format!("{{\"name\":\"{name}\",\"bug_type\":\"309\"}}\n");
        let body = "{\"exception\":{\"type\":\"EXC_BAD_ACCESS\",\"signal\":\"SIGSEGV\",\"message\":\"/Users/someone/secret\"},\"procPath\":\"/Users/someone/x\"}";
        assert_eq!(
            parse_report(&(head("fabric-switchboard") + body)),
            Some(("EXC_BAD_ACCESS".into(), Some("SIGSEGV".into())))
        );
        assert_eq!(parse_report(&(head("node") + body)), None, "not ours");
        let odd = "{\"exception\":{\"type\":\"see /Users/x\",\"signal\":\"SIGABRT\"}}";
        assert_eq!(
            parse_report(&(head("switchboard") + odd)),
            None,
            "not a code"
        );
        let record = crash_record(
            &run(false),
            Some("EXC_CRASH"),
            Some("SIGABRT"),
            Some(1_500),
            2_000,
        );
        assert_eq!(record["uptime"], "under_10m");
        assert_eq!(record["exception"], "EXC_CRASH");
        let text = record.to_string();
        assert!(!text.contains("/Users"), "no path: {text}");
        assert_eq!(code_static(Some("EXC_BAD_ACCESS")), "exc_bad_access");
        assert_eq!(code_static(Some("EXC_NEW_KIND")), "other");
        assert_eq!(code_static(None), "none");
    }

    #[test]
    fn a_start_after_a_crash_records_it_and_a_drain_marks_its_run_clean() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A run whose process is long gone (pid 0 is never alive).
        write_run(
            root,
            &Run {
                pid: 0,
                ..run(false)
            },
        );
        begin(root, 5_000);
        let answer = list(root, 5_000);
        assert_eq!(answer["crashes_total"], 1);
        assert_eq!(answer["crashes"][0]["version"], "0.7.98");
        assert_eq!(answer["crashes"][0]["started_at"], 1_000);
        let mine = read_run(root).unwrap();
        assert_eq!(mine.pid, std::process::id());
        assert!(!mine.clean);
        end(root);
        assert!(read_run(root).unwrap().clean);
        // A start after a clean run records nothing more.
        begin(root, 6_000);
        assert_eq!(list(root, 6_000)["crashes_total"], 1);
        assert!(list(root, 6_000)["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_list_keeps_the_newest_two_hundred() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(KEEP + 5) {
            append(dir.path(), &json!({"at": i, "kind": "owner_crash"}));
        }
        let answer = list(dir.path(), 0);
        assert_eq!(answer["crashes_total"], KEEP);
        assert_eq!(answer["crashes"][0]["at"], KEEP + 4, "newest first");
        assert_eq!(answer["crashes"].as_array().unwrap().len(), LIST);
    }

    #[test]
    fn the_newest_owner_report_after_the_run_started_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let report = |name: &str, process: &str, kind: &str| {
            let text = format!(
                "{{\"name\":\"{process}\"}}\n{{\"exception\":{{\"type\":\"{kind}\",\"signal\":\"SIGABRT\"}}}}"
            );
            std::fs::write(dir.path().join(name), text).unwrap();
        };
        report(
            "fabric-switchboard-2026-11-16-010203.ips",
            "fabric-switchboard",
            "EXC_CRASH",
        );
        report(
            "switchboard_runtime-abc-2026-11-16.ips",
            "switchboard_runtime-abc",
            "EXC_BAD_ACCESS",
        );
        report("node-2026-11-16.ips", "node", "EXC_GUARD");
        let found = newest_report(dir.path(), 0).unwrap();
        assert_eq!(
            (found.0.as_str(), found.1.as_deref()),
            ("EXC_CRASH", Some("SIGABRT"))
        );
        assert!(
            newest_report(dir.path(), i64::MAX).is_none(),
            "written before the run started"
        );
    }

    #[test]
    fn uptime_is_bucketed() {
        assert_eq!(uptime_bucket(5), "under_1m");
        assert_eq!(uptime_bucket(3_599), "under_1h");
        assert_eq!(uptime_bucket(200_000), "over_1d");
    }
}
