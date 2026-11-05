//! SB-250 (the operator, 2026-11-04, relayed by Fabric: «Вся эта система должна логироваться.
//! Должен быть анализ логов раз в час и раз в день»; Fabric ADR-0188, fabric-agent-contract
//! DEC-0153 `docs/specification/ops-digest.md`): Switchboard reads its own operations log.
//!
//! Every line the log takes (`oplog::event`) is counted by its event name and outcome in the UTC
//! hour it happened: `failed` when `errors::classify` reads it as an error (SB-248), `unknown`
//! when it says its outcome is not known, `ok` otherwise. Each closed hour the owner ran through,
//! an empty one included, folds into one `ops.digest.hourly@1` line of
//! `<data>/digests/hourly-YYYY-MM.jsonl`; each UTC day the day's hourly lines fold into one
//! `ops.digest.daily@1` line of `daily-YYYY-MM.jsonl` (0600, the folder 0700). Counts only: the
//! log's codes are fixed `&'static str`, so no value reaches a digest. Failures of an operation that
//! rose against the hour before (at least 10, and at least twice the hour before's, which counts as
//! one when it had none) are logged as `failures_rose`. Nothing leaves the machine.
use crate::oplog::Field;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path};

pub const DIR: &str = "digests";
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
/// OD-4: a rise is at least this many failures in the hour…
const RISE_MIN_FAILED: u64 = 10;
/// …and at least this many times the hour before's (none counts as one).
const RISE_FACTOR: u64 = 2;
/// Monthly files older than this many months are removed.
const KEEP_MONTHS: i64 = 3;
/// Codes that say the outcome is not known.
const UNKNOWN: &[&str] = &["unknown", "outcome_unknown", "deferred"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Counts {
    pub ok: u64,
    pub failed: u64,
    pub unknown: u64,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.ok += other.ok;
        self.failed += other.failed;
        self.unknown += other.unknown;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpCount {
    pub op: String,
    pub ok: u64,
    pub failed: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Risen {
    pub op: String,
    pub failed: u64,
    pub previous: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Hourly {
    pub schema: String,
    pub from: String,
    pub to: String,
    pub ops: Vec<OpCount>,
    pub total: Counts,
    pub risen: Vec<Risen>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DailyRisen {
    pub op: String,
    pub hours: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Daily {
    pub schema: String,
    pub day: String,
    pub hours: u64,
    pub ops: Vec<OpCount>,
    pub total: Counts,
    pub risen: Vec<DailyRisen>,
}

/// One hour's counts by operation.
type Hour = BTreeMap<&'static str, Counts>;

#[derive(Default)]
struct State {
    /// Start of the hour `counts` belongs to; none before the first line.
    hour: Option<i64>,
    counts: Hour,
    /// Hours that closed while lines were counted, waiting for the next pass to write them.
    closed: Vec<(i64, Hour)>,
    /// The last hour written and its digest, for the next hour's rise and to fill empty hours.
    written: Option<(i64, Hourly)>,
    /// This owner's first pass: an hour before it is a gap, never an empty hour.
    first: Option<i64>,
    /// OD-5: the days that closed with hourly lines and no daily one were folded at this
    /// owner's first pass.
    caught_up: bool,
}

#[cfg(not(test))]
static STATE: std::sync::Mutex<Option<State>> = std::sync::Mutex::new(None);
#[cfg(test)]
thread_local! {
    /// Tests share one process: each test thread keeps its own digest, and only a test that asks
    /// counts its log lines.
    static STATE: std::cell::RefCell<Option<State>> = const { std::cell::RefCell::new(None) };
    pub(crate) static COUNTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    #[cfg(not(test))]
    {
        let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
        f(guard.get_or_insert_with(State::default))
    }
    #[cfg(test)]
    {
        STATE.with(|s| f(s.borrow_mut().get_or_insert_with(State::default)))
    }
}

/// The outcome a log line counts under (OD-3).
pub(crate) fn outcome(event: &'static str, fields: &[(&'static str, Field)]) -> &'static str {
    if crate::errors::classify(event, fields).is_some() {
        return "failed";
    }
    let unknown = fields
        .iter()
        .any(|(_, f)| matches!(f, Field::Code(c) if UNKNOWN.contains(c)));
    if unknown {
        "unknown"
    } else {
        "ok"
    }
}

/// Counts one log line in its hour. Called by every `oplog::event`.
pub(crate) fn note(event: &'static str, fields: &[(&'static str, Field)]) {
    #[cfg(test)]
    if !COUNTING.with(|c| c.get()) {
        return;
    }
    count_at(event, outcome(event, fields), crate::errors::now());
}

fn count_at(op: &'static str, outcome: &str, at: i64) {
    if !crate::errors::valid(op) {
        return;
    }
    let hour = at.div_euclid(HOUR) * HOUR;
    with_state(|s| {
        if s.hour != Some(hour) {
            if let Some(previous) = s.hour.filter(|h| *h < hour) {
                let counts = std::mem::take(&mut s.counts);
                s.closed.push((previous, counts));
            }
            if s.hour.is_none_or(|h| h < hour) {
                s.hour = Some(hour);
            }
        }
        let c = s.counts.entry(op).or_default();
        match outcome {
            "ok" => c.ok += 1,
            "failed" => c.failed += 1,
            _ => c.unknown += 1,
        }
    });
}

/// Every background pass: writes each hour that closed since the last pass, an empty one the
/// owner ran through included (OD-1), and the daily digest of each day those hours finished.
/// The owner's first pass marks its start: an hour before it is a gap, never an empty hour.
/// Nothing is read or written on a pass inside an hour already accounted for.
pub(crate) fn tick(root: &Path, now: i64) {
    #[cfg(test)]
    if !COUNTING.with(|c| c.get()) {
        return;
    }
    let current = now.div_euclid(HOUR) * HOUR;
    let (due, previous, catch_up) = with_state(|s| {
        let first = s.first.get_or_insert(now).div_euclid(HOUR) * HOUR;
        // The hour of the lines counted so far closes when the clock has left it.
        if s.hour.is_some_and(|h| h < current) {
            let counts = std::mem::take(&mut s.counts);
            s.closed.push((s.hour.unwrap_or(current), counts));
            s.hour = Some(current);
        }
        let mut closed: BTreeMap<i64, Hour> = std::mem::take(&mut s.closed).into_iter().collect();
        // Empty hours this owner ran through after the last one written.
        let mut hour = s
            .written
            .as_ref()
            .map(|(h, _)| h + HOUR)
            .unwrap_or(first)
            .max(first);
        while hour < current {
            closed.entry(hour).or_default();
            hour += HOUR;
        }
        let written = s.written.as_ref().map(|(h, _)| *h);
        closed.retain(|h, _| written.is_none_or(|w| *h > w));
        let catch_up = !std::mem::replace(&mut s.caught_up, true);
        (closed, s.written.clone(), catch_up)
    });
    if catch_up {
        fold_missed_days(root, now);
    }
    if due.is_empty() {
        return;
    }
    let mut previous = previous;
    let mut days = Vec::new();
    for (hour, counts) in due {
        let prior = previous
            .as_ref()
            .filter(|(h, _)| *h + HOUR == hour)
            .map(|(_, d)| d);
        let digest = fold_hour(hour, &counts, prior);
        for r in &digest.risen {
            if let Some(op) = counts.keys().find(|k| **k == r.op) {
                crate::oplog::event(
                    "failures_rose",
                    &[
                        ("op", Field::Code(op)),
                        ("failed", Field::Number(r.failed as i64)),
                        ("previous", Field::Number(r.previous as i64)),
                    ],
                );
            }
        }
        append(
            &root.join(DIR),
            &format!("hourly-{}.jsonl", month(hour)),
            &digest,
        );
        let day = hour.div_euclid(DAY);
        if previous
            .as_ref()
            .is_some_and(|(h, _)| h.div_euclid(DAY) < day)
        {
            days.push(day - 1);
        }
        previous = Some((hour, digest));
    }
    // A day is over once an hour of the next one was written.
    for day in days {
        daily(root, day);
    }
    with_state(|s| s.written = previous);
    prune(&root.join(DIR), now);
}

/// OD-1, OD-3, OD-4: one hour's counts folded, with its rises against the hour before when that
/// digest is the hour just before (a gap compares with nothing).
pub(crate) fn fold_hour(hour: i64, counts: &Hour, previous: Option<&Hourly>) -> Hourly {
    let prev: BTreeMap<&str, u64> = previous
        .map(|p| p.ops.iter().map(|o| (o.op.as_str(), o.failed)).collect())
        .unwrap_or_default();
    let mut total = Counts::default();
    let ops: Vec<OpCount> = counts
        .iter()
        .map(|(op, c)| {
            total.add(*c);
            OpCount {
                op: (*op).to_owned(),
                ok: c.ok,
                failed: c.failed,
                unknown: c.unknown,
            }
        })
        .collect();
    let risen = ops
        .iter()
        .filter_map(|o| {
            let before = prev.get(o.op.as_str()).copied().unwrap_or(0);
            (o.failed >= RISE_MIN_FAILED && o.failed >= RISE_FACTOR * before.max(1)).then(|| {
                Risen {
                    op: o.op.clone(),
                    failed: o.failed,
                    previous: before,
                }
            })
        })
        .collect();
    Hourly {
        schema: "ops.digest.hourly@1".into(),
        from: stamp(hour),
        to: stamp(hour + HOUR),
        ops,
        total,
        risen,
    }
}

/// OD-5: the day's hourly lines, each hour folded once (the first line of an hour wins).
pub(crate) fn fold_day(day: i64, hourly: &[Hourly]) -> Option<Daily> {
    let date = crate::errors::date(day);
    let mut seen = std::collections::BTreeSet::new();
    let mut ops: BTreeMap<String, Counts> = BTreeMap::new();
    let mut rose: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for h in hourly.iter().filter(|h| h.from.starts_with(&date)) {
        if !seen.insert(h.from.clone()) {
            continue;
        }
        for o in &h.ops {
            ops.entry(o.op.clone()).or_default().add(Counts {
                ok: o.ok,
                failed: o.failed,
                unknown: o.unknown,
            });
        }
        for r in &h.risen {
            let x = rose.entry(r.op.clone()).or_default();
            x.0 += 1;
            x.1 += r.failed;
        }
    }
    if seen.is_empty() {
        return None;
    }
    let mut total = Counts::default();
    let ops = ops
        .into_iter()
        .map(|(op, c)| {
            total.add(c);
            OpCount {
                op,
                ok: c.ok,
                failed: c.failed,
                unknown: c.unknown,
            }
        })
        .collect();
    let mut risen: Vec<DailyRisen> = rose
        .into_iter()
        .map(|(op, (hours, failed))| DailyRisen { op, hours, failed })
        .collect();
    risen.sort_by(|a, b| b.failed.cmp(&a.failed).then_with(|| a.op.cmp(&b.op)));
    Some(Daily {
        schema: "ops.digest.daily@1".into(),
        day: date,
        hours: seen.len() as u64,
        ops,
        total,
        risen,
    })
}

/// How far back the first pass looks for a day left without its daily digest (OD-5).
const CATCH_UP_DAYS: i64 = 40;

/// OD-5: once per owner, every closed day of the last `CATCH_UP_DAYS` that has hourly lines and
/// no daily one is folded, so a day the owner was not running at midnight (a crash, a quit, a
/// computer asleep) still gets its daily digest. Reads the hourly files of those months once.
fn fold_missed_days(root: &Path, now: i64) {
    let dir = root.join(DIR);
    let today = now.div_euclid(DAY);
    let first = today - CATCH_UP_DAYS;
    let mut months: Vec<String> = (first..today).map(|d| month(d * DAY)).collect();
    months.dedup();
    let mut dates = std::collections::BTreeSet::new();
    for m in &months {
        for h in read::<Hourly>(&dir.join(format!("hourly-{m}.jsonl"))) {
            if let Some(date) = h.from.get(..10) {
                dates.insert(date.to_owned());
            }
        }
    }
    for day in first..today {
        if dates.contains(&crate::errors::date(day)) {
            daily(root, day);
        }
    }
}

/// Writes the daily digest of `day` unless its month's file already holds one for it.
fn daily(root: &Path, day: i64) {
    let dir = root.join(DIR);
    let start = day * DAY;
    let file = format!("daily-{}.jsonl", month(start));
    let date = crate::errors::date(day);
    if read::<Daily>(&dir.join(&file))
        .iter()
        .any(|d| d.day == date)
    {
        return;
    }
    let hourly = read::<Hourly>(&dir.join(format!("hourly-{}.jsonl", month(start))));
    if let Some(digest) = fold_day(day, &hourly) {
        append(&dir, &file, &digest);
    }
}

/// The lines of one file a reader can read (OD-6: one that does not parse is skipped).
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Appends one line, the folder 0700 and the file 0600. A digest that cannot be written is lost.
fn append<T: Serialize>(dir: &Path, file: &str, digest: &T) {
    let Ok(mut line) = serde_json::to_string(digest) else {
        return;
    };
    line.push('\n');
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut f) = options.open(dir.join(file)) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Removes monthly files older than `KEEP_MONTHS` months.
fn prune(dir: &Path, now: i64) {
    let Some(cutoff) = month_index(&month(now)).map(|m| m - KEEP_MONTHS) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".jsonl").and_then(|n| {
            n.strip_prefix("hourly-")
                .or_else(|| n.strip_prefix("daily-"))
        }) else {
            continue;
        };
        if month_index(stem).is_some_and(|m| m < cutoff) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn month_index(yyyy_mm: &str) -> Option<i64> {
    let (y, m) = yyyy_mm.split_once('-')?;
    Some(y.parse::<i64>().ok()? * 12 + m.parse::<i64>().ok()? - 1)
}

/// `YYYY-MM` of a time.
fn month(at: i64) -> String {
    crate::errors::date(at.div_euclid(DAY))[..7].to_owned()
}

/// `YYYY-MM-DDTHH:00:00Z` of a whole hour.
fn stamp(hour: i64) -> String {
    format!(
        "{}T{:02}:00:00Z",
        crate::errors::date(hour.div_euclid(DAY)),
        hour.rem_euclid(DAY) / HOUR
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const DAY0: i64 = 20_000 * DAY;

    fn hour_of(counts: &[(&'static str, Counts)]) -> Hour {
        counts.iter().copied().collect()
    }

    #[test]
    fn an_hour_folds_by_operation_with_totals_and_its_window() {
        let counts = hour_of(&[
            (
                "usage_check",
                Counts {
                    ok: 3,
                    failed: 2,
                    unknown: 0,
                },
            ),
            (
                "analytics_flush",
                Counts {
                    ok: 1,
                    failed: 0,
                    unknown: 1,
                },
            ),
        ]);
        let d = fold_hour(DAY0 + 5 * HOUR, &counts, None);
        assert_eq!(d.schema, "ops.digest.hourly@1");
        assert_eq!(
            (d.from.as_str(), d.to.as_str()),
            ("2024-10-04T05:00:00Z", "2024-10-04T06:00:00Z")
        );
        assert_eq!(d.ops[0].op, "analytics_flush", "by name");
        assert_eq!(
            d.total,
            Counts {
                ok: 4,
                failed: 2,
                unknown: 1
            }
        );
        assert!(d.risen.is_empty());
        let json = serde_json::to_value(&d).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["from", "ops", "risen", "schema", "to", "total"]);
    }

    #[test]
    fn a_rise_is_ten_failures_and_twice_the_hour_before_and_a_gap_compares_with_none() {
        let failing = |n| {
            hour_of(&[(
                "usage_check",
                Counts {
                    ok: 0,
                    failed: n,
                    unknown: 0,
                },
            )])
        };
        let before = |n| fold_hour(DAY0, &failing(n), None);
        let h = DAY0 + HOUR;
        assert_eq!(
            fold_hour(h, &failing(10), Some(&before(0))).risen.len(),
            1,
            "10 after 0"
        );
        assert_eq!(
            fold_hour(h, &failing(10), Some(&before(5))).risen.len(),
            1,
            "10 after 5"
        );
        assert!(
            fold_hour(h, &failing(10), Some(&before(6)))
                .risen
                .is_empty(),
            "10 after 6"
        );
        assert!(
            fold_hour(h, &failing(9), None).risen.is_empty(),
            "9 is never one"
        );
        let risen = fold_hour(h, &failing(12), Some(&before(5))).risen;
        assert_eq!(
            risen,
            vec![Risen {
                op: "usage_check".into(),
                failed: 12,
                previous: 5
            }]
        );
    }

    #[test]
    fn a_day_folds_each_hour_once_and_sums_its_rises() {
        let failing = |n| {
            hour_of(&[(
                "usage_check",
                Counts {
                    ok: 1,
                    failed: n,
                    unknown: 0,
                },
            )])
        };
        let a = fold_hour(DAY0, &failing(10), None);
        let b = fold_hour(DAY0 + HOUR, &failing(30), Some(&a));
        let other_day = fold_hour(DAY0 + DAY, &failing(1), None);
        let d = fold_day(20_000, &[a.clone(), a.clone(), b, other_day]).unwrap();
        assert_eq!((d.day.as_str(), d.hours), ("2024-10-04", 2));
        assert_eq!(
            d.total,
            Counts {
                ok: 2,
                failed: 40,
                unknown: 0
            }
        );
        assert_eq!(
            d.risen,
            vec![DailyRisen {
                op: "usage_check".into(),
                hours: 2,
                failed: 40
            }]
        );
        assert!(fold_day(19_999, &[a]).is_none());
    }

    #[test]
    fn the_log_counts_by_outcome_and_each_hour_the_owner_ran_through_is_written_once() {
        let root = tempfile::tempdir().unwrap();
        COUNTING.with(|c| c.set(true));
        let start = DAY0 + 22 * HOUR + 600;
        count_at("usage_check", "failed", start);
        count_at("usage_check", "ok", start + 10);
        count_at("analytics_flush", "unknown", start + 20);
        // Inside the hour: nothing is written.
        tick(root.path(), start + 30);
        assert!(!root.path().join(DIR).exists());
        // Three hours later (one counted, two empty), across midnight.
        tick(root.path(), DAY0 + DAY + HOUR + 5);
        tick(root.path(), DAY0 + DAY + HOUR + 35);
        COUNTING.with(|c| c.set(false));
        let hourly = read::<Hourly>(&root.path().join(DIR).join("hourly-2024-10.jsonl"));
        let froms: Vec<&str> = hourly.iter().map(|h| h.from.as_str()).collect();
        assert_eq!(
            froms,
            [
                "2024-10-04T22:00:00Z",
                "2024-10-04T23:00:00Z",
                "2024-10-05T00:00:00Z"
            ]
        );
        assert_eq!(
            hourly[0].total,
            Counts {
                ok: 1,
                failed: 1,
                unknown: 1
            }
        );
        assert!(hourly[1].ops.is_empty() && hourly[1].total == Counts::default());
        let daily = read::<Daily>(&root.path().join(DIR).join("daily-2024-10.jsonl"));
        assert_eq!(
            daily.len(),
            1,
            "the day before is folded once its next hour is written"
        );
        assert_eq!((daily[0].day.as_str(), daily[0].hours), ("2024-10-04", 2));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |p: std::path::PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(root.path().join(DIR)), 0o700);
            assert_eq!(
                mode(root.path().join(DIR).join("hourly-2024-10.jsonl")),
                0o600
            );
        }
    }

    #[test]
    fn a_day_the_owner_was_not_running_at_midnight_is_folded_at_its_next_start() {
        // OD-5 (DEC-0153 6bb6ce04): an owner that ended before midnight (here 2024-10-04 23:00)
        // left that day's hours and no daily line; the next owner folds it at its first pass,
        // once, and never folds today.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        for hour in [DAY0 + 21 * HOUR, DAY0 + 22 * HOUR, DAY0 + DAY + 2 * HOUR] {
            let digest = fold_hour(hour, &hour_of(&[]), None);
            append(&dir, &format!("hourly-{}.jsonl", month(hour)), &digest);
        }
        COUNTING.with(|c| c.set(true));
        tick(root.path(), DAY0 + DAY + 3 * HOUR + 60);
        tick(root.path(), DAY0 + DAY + 3 * HOUR + 90);
        COUNTING.with(|c| c.set(false));
        let daily = read::<Daily>(&dir.join("daily-2024-10.jsonl"));
        let days: Vec<(&str, u64)> = daily.iter().map(|d| (d.day.as_str(), d.hours)).collect();
        assert_eq!(days, [("2024-10-04", 2)]);
    }

    #[test]
    fn an_error_line_counts_as_failed_and_an_unknown_one_as_unknown() {
        let code = Field::Code;
        assert_eq!(
            outcome("launch_record", &[("result", code("failed"))]),
            "failed"
        );
        assert_eq!(
            outcome("analytics_flush", &[("outcome", code("deferred"))]),
            "unknown"
        );
        assert_eq!(outcome("owner_started", &[("outcome", code("ok"))]), "ok");
    }

    #[test]
    fn files_older_than_three_months_go() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "hourly-2024-06.jsonl",
            "daily-2024-07.jsonl",
            "hourly-2024-09.jsonl",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        prune(&dir, DAY0);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["daily-2024-07.jsonl", "hourly-2024-09.jsonl", "notes.txt"]
        );
    }
}
