//! SB-250 (the operator, 2026-11-04, relayed by Fabric: «Вся эта система должна логироваться.
//! Должен быть анализ логов раз в час и раз в день»; Fabric ADR-0188, fabric-agent-contract
//! DEC-0153 `docs/specification/ops-digest.md`): Switchboard reads its own operations log.
//!
//! Every line the log takes (`oplog::event`) is counted by its event name and outcome in the UTC
//! hour it happened: `failed` when `errors::classify` reads it as an error (SB-248), `unknown`
//! when it says its outcome is not known, `ok` otherwise. Each closed hour the owner saw itself
//! running in (SB-257: a pass or a counted line), an empty one included, folds into one `ops.digest.hourly@1` line of
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
/// SB-257, DEC-0153 OD-1: the hour an owner stops in, kept for the next owner to write.
const OPEN: &str = "open.json";
/// A line counts at most this many operations; the rest count under `OTHER` (DEC-0153). OD-3
/// bounds the UTC day, not only the line: at most `MAX_OPS - 1` names a day, in its hourly lines
/// and in its daily one (SB-275).
const MAX_OPS: usize = 4096;
const OTHER: &str = "other_operations";
/// Codes that say the outcome is not known.
const UNKNOWN: &[&str] = &["unknown", "outcome_unknown", "deferred"];
/// SB-255, OD-3: answers that say an operation was not done because of setup, policy or the
/// provider's own wait, not that it tried and failed: counted `unknown`, so an unfinished setup
/// never reads as a rise in failures (OD-4).
const NOT_DONE: &[&str] = &[
    "absent",
    "declined",
    "denied",
    "held",
    "locked",
    "no_account",
    "rate_limited",
    "refused",
    "skipped",
    "superseded",
];

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
    /// SB-257, OD-1: the hours this owner saw itself running in (a pass or a counted line); an
    /// hour the computer slept through is never written as an empty one.
    seen: std::collections::BTreeSet<i64>,
    /// SB-275, OD-3: the UTC day `day_ops` belongs to, and the operations named in it so far.
    day: Option<i64>,
    day_ops: std::collections::BTreeSet<&'static str>,
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
        .any(|(_, f)| matches!(f, Field::Code(c) if UNKNOWN.contains(c) || NOT_DONE.contains(c)));
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
        let day = at.div_euclid(DAY) * DAY;
        if s.day != Some(day) {
            s.day = Some(day);
            s.day_ops.clear();
        }
        // A name the day already counts stays named; a new one only while the day has room.
        let op = if s.day_ops.contains(op) || s.day_ops.len() < MAX_OPS - 1 {
            s.day_ops.insert(op);
            op
        } else {
            OTHER
        };
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
    let starting = with_state(|s| s.first.is_none());
    if starting {
        resume(root, current);
    }
    let (due, previous, catch_up) = with_state(|s| {
        s.first.get_or_insert(now);
        s.seen.insert(current);
        // The hour of the lines counted so far closes when the clock has left it.
        if s.hour.is_some_and(|h| h < current) {
            let counts = std::mem::take(&mut s.counts);
            s.closed.push((s.hour.unwrap_or(current), counts));
            s.hour = Some(current);
        }
        let mut closed: BTreeMap<i64, Hour> = std::mem::take(&mut s.closed).into_iter().collect();
        // Empty hours this owner saw itself running in (SB-257: never one it slept through).
        for hour in s.seen.range(..current) {
            closed.entry(*hour).or_default();
        }
        s.seen.retain(|h| *h >= current);
        let written = s.written.as_ref().map(|(h, _)| *h);
        closed.retain(|h, _| written.is_none_or(|w| *h > w));
        let catch_up = !std::mem::replace(&mut s.caught_up, true);
        (closed, s.written.clone(), catch_up)
    });
    if due.is_empty() {
        if catch_up {
            fold_missed_days(root, now);
        }
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
    // DEC-0153 OD-1 and OD-5 (6eb61183): the hours kept from the last owner are written before
    // the missed days fold, or a day's daily line would lose its last hour for good.
    if catch_up {
        fold_missed_days(root, now);
    }
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
    // A day out of range names no date, and every line starts with the empty one.
    if date.is_empty() {
        return None;
    }
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
    // SB-275, OD-3: the day names at most `MAX_OPS - 1` operations even when its hourly lines came
    // from owners that each counted their own; the rest fold into `OTHER`.
    let named = ops.keys().filter(|op| op.as_str() != OTHER).count();
    if named > MAX_OPS - 1 {
        let extra: Vec<String> = ops
            .keys()
            .filter(|op| op.as_str() != OTHER)
            .skip(MAX_OPS - 1)
            .cloned()
            .collect();
        let mut folded = Counts::default();
        for op in extra {
            if let Some(c) = ops.remove(&op) {
                folded.add(c);
            }
        }
        ops.entry(OTHER.to_owned()).or_default().add(folded);
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

/// SB-257, OD-1: at the drain, the hours counted and not yet written (the hour the owner stops
/// in among them) go to `open.json` for the next owner; an owner that crashes leaves none, and
/// never a partial line.
pub(crate) fn save_open(root: &Path) {
    #[cfg(test)]
    if !COUNTING.with(|c| c.get()) {
        return;
    }
    let hours: Vec<OpenHour> = with_state(|s| {
        let mut hours: Vec<OpenHour> = s.closed.iter().map(|(h, c)| OpenHour::of(*h, c)).collect();
        if let Some(h) = s.hour {
            if !s.counts.is_empty() || s.seen.contains(&h) {
                hours.push(OpenHour::of(h, &s.counts));
            }
        }
        let written = s.written.as_ref().map(|(h, _)| *h);
        hours.retain(|o| written.is_none_or(|w| o.hour > w));
        hours
    });
    if hours.is_empty() {
        return;
    }
    let dir = root.join(DIR);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string(&hours) {
        let temporary = dir.join(format!("{OPEN}.tmp"));
        let written = {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(&temporary)
                .and_then(|mut f| f.write_all(text.as_bytes()))
        };
        if written.is_ok() {
            let _ = std::fs::rename(&temporary, dir.join(OPEN));
        } else {
            let _ = std::fs::remove_file(&temporary);
        }
    }
}

/// One hour's counts as the drain keeps them.
#[derive(Serialize, Deserialize)]
struct OpenHour {
    hour: i64,
    ops: BTreeMap<String, Counts>,
}
impl OpenHour {
    fn of(hour: i64, counts: &Hour) -> Self {
        OpenHour {
            hour,
            ops: counts.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
        }
    }
}

/// SB-257, OD-1, at this owner's first pass: `previous` is the last hourly line already written
/// (this month's or the one before), and the hours the last owner kept at its drain come back:
/// one before this hour is written once closed, this hour's counts go on. Hours already written
/// are never written again; `open.json` is read once and removed.
fn resume(root: &Path, current: i64) {
    let dir = root.join(DIR);
    let last = [month(current), month(current - 32 * DAY)]
        .iter()
        .flat_map(|m| read::<Hourly>(&dir.join(format!("hourly-{m}.jsonl"))))
        .filter_map(|h| Some((hour_from_stamp(&h.from)?, h)))
        .filter(|(h, _)| *h < current)
        .max_by_key(|(h, _)| *h);
    let open: Vec<OpenHour> = std::fs::read_to_string(dir.join(OPEN))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let _ = std::fs::remove_file(dir.join(OPEN));
    with_state(|s| {
        // Lines counted before this first pass in an hour that has already closed.
        if let Some(h) = s.hour.filter(|h| *h < current) {
            let counts = std::mem::take(&mut s.counts);
            s.closed.push((h, counts));
            s.hour = None;
        }
        if s.written.is_none() {
            s.written = last;
        }
        let written = s.written.as_ref().map(|(h, _)| *h);
        for o in open {
            if written.is_some_and(|w| o.hour <= w) || o.hour > current {
                continue;
            }
            let target: &mut Hour = if o.hour == current {
                s.hour = Some(current);
                &mut s.counts
            } else {
                match s.closed.iter().position(|(h, _)| *h == o.hour) {
                    Some(i) => &mut s.closed[i].1,
                    None => {
                        s.closed.push((o.hour, Hour::new()));
                        &mut s.closed.last_mut().expect("just pushed").1
                    }
                }
            };
            for (op, c) in o.ops {
                if !crate::errors::valid(&op) && op != OTHER {
                    continue;
                }
                // The names are the log's fixed codes; a bounded few are kept for the process.
                let op: &'static str = Box::leak(op.into_boxed_str());
                target.entry(op).or_default().add(c);
            }
        }
    });
}

/// The start of the hour `YYYY-MM-DDTHH:00:00Z` names.
fn hour_from_stamp(stamp: &str) -> Option<i64> {
    let y: i64 = stamp.get(0..4)?.parse().ok()?;
    let m: i64 = stamp.get(5..7)?.parse().ok()?;
    let d: i64 = stamp.get(8..10)?.parse().ok()?;
    let h: i64 = stamp.get(11..13)?.parse().ok()?;
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * DAY + h * HOUR)
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
        // Two empty hours the owner was seen in, across midnight, then the next hour.
        tick(root.path(), DAY0 + 23 * HOUR + 5);
        tick(root.path(), DAY0 + DAY + 5);
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

    /// A new owner process: the digest's memory starts empty.
    fn restart() {
        STATE.with(|s| *s.borrow_mut() = None);
    }

    #[test]
    fn hours_the_computer_slept_through_are_never_written() {
        // SB-257, OD-1: a pass at 10:00, the computer asleep, the next pass at 15:00.
        let root = tempfile::tempdir().unwrap();
        COUNTING.with(|c| c.set(true));
        restart();
        count_at("usage_check", "ok", DAY0 + 10 * HOUR + 60);
        tick(root.path(), DAY0 + 10 * HOUR + 90);
        tick(root.path(), DAY0 + 15 * HOUR + 30);
        COUNTING.with(|c| c.set(false));
        let hourly = read::<Hourly>(&root.path().join(DIR).join("hourly-2024-10.jsonl"));
        let froms: Vec<&str> = hourly.iter().map(|h| h.from.as_str()).collect();
        assert_eq!(froms, ["2024-10-04T10:00:00Z"]);
    }

    #[test]
    fn the_hour_an_owner_stops_in_is_written_by_the_next_one_once() {
        // SB-257, OD-1: counted at 10:20, stopped at 10:40 (kept), the next owner starts at
        // 12:05: 10:00 is written with its counts and `open.json` is gone.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        COUNTING.with(|c| c.set(true));
        restart();
        tick(root.path(), DAY0 + 10 * HOUR + 30);
        count_at("usage_check", "failed", DAY0 + 10 * HOUR + 1200);
        save_open(root.path());
        assert!(dir.join(OPEN).exists());
        restart();
        tick(root.path(), DAY0 + 12 * HOUR + 300);
        tick(root.path(), DAY0 + 12 * HOUR + 330);
        COUNTING.with(|c| c.set(false));
        assert!(!dir.join(OPEN).exists());
        let hourly = read::<Hourly>(&dir.join("hourly-2024-10.jsonl"));
        assert_eq!(hourly.len(), 1);
        assert_eq!(hourly[0].from, "2024-10-04T10:00:00Z");
        assert_eq!(hourly[0].total.failed, 1);
    }

    #[test]
    fn the_stopping_hour_is_written_before_its_day_folds() {
        // DEC-0153 round 5: an owner ran on day D from 20:00 to 23:30 and stopped; the next one
        // starts on D+1 at 09:00. The daily line of D holds 20:00 to 23:00, four hours.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        COUNTING.with(|c| c.set(true));
        restart();
        for h in 20..24 {
            tick(root.path(), DAY0 + h * HOUR + 60);
        }
        count_at("usage_check", "ok", DAY0 + 23 * HOUR + 1800);
        save_open(root.path());
        restart();
        tick(root.path(), DAY0 + DAY + 9 * HOUR + 60);
        COUNTING.with(|c| c.set(false));
        let daily = read::<Daily>(&dir.join("daily-2024-10.jsonl"));
        let days: Vec<(&str, u64)> = daily.iter().map(|d| (d.day.as_str(), d.hours)).collect();
        assert_eq!(days, [("2024-10-04", 4)]);
        assert_eq!(daily[0].total.ok, 1);
    }

    #[test]
    fn a_restart_inside_the_hour_goes_on_counting_it_and_writes_it_once() {
        let root = tempfile::tempdir().unwrap();
        COUNTING.with(|c| c.set(true));
        restart();
        tick(root.path(), DAY0 + 10 * HOUR + 30);
        count_at("usage_check", "ok", DAY0 + 10 * HOUR + 600);
        save_open(root.path());
        restart();
        count_at("usage_check", "ok", DAY0 + 10 * HOUR + 900);
        tick(root.path(), DAY0 + 10 * HOUR + 930);
        tick(root.path(), DAY0 + 11 * HOUR + 30);
        COUNTING.with(|c| c.set(false));
        let hourly = read::<Hourly>(&root.path().join(DIR).join("hourly-2024-10.jsonl"));
        assert_eq!(hourly.len(), 1);
        assert_eq!(hourly[0].total.ok, 2);
    }

    #[test]
    fn after_a_restart_the_next_hour_compares_with_the_last_line_written() {
        // SB-257, OD-1: after a restart the hour compares with the line written before it, not
        // with nothing. 10:00 had 8 failures; the next owner counts 20 at 11:00: a rise against 8.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        let mut before = Hour::new();
        before.insert(
            "usage_check",
            Counts {
                ok: 0,
                failed: 8,
                unknown: 0,
            },
        );
        append(
            &dir,
            "hourly-2024-10.jsonl",
            &fold_hour(DAY0 + 10 * HOUR, &before, None),
        );
        COUNTING.with(|c| c.set(true));
        restart();
        tick(root.path(), DAY0 + 11 * HOUR + 5);
        for i in 0..20 {
            count_at("usage_check", "failed", DAY0 + 11 * HOUR + 60 + i);
        }
        tick(root.path(), DAY0 + 12 * HOUR + 5);
        COUNTING.with(|c| c.set(false));
        let hourly = read::<Hourly>(&dir.join("hourly-2024-10.jsonl"));
        assert_eq!(hourly.len(), 2);
        assert_eq!(hourly[1].risen.len(), 1, "{:?}", hourly[1].risen);
        assert_eq!(hourly[1].risen[0].previous, 8);
    }

    #[test]
    fn past_four_thousand_and_ninety_six_operations_the_rest_count_as_one() {
        COUNTING.with(|c| c.set(true));
        restart();
        let at = DAY0 + 10 * HOUR;
        for i in 0..MAX_OPS + 10 {
            let op: &'static str = Box::leak(format!("op_{i}").into_boxed_str());
            count_at(op, "ok", at);
        }
        let (len, other) = with_state(|s| (s.counts.len(), s.counts.get(OTHER).map(|c| c.ok)));
        COUNTING.with(|c| c.set(false));
        restart();
        assert_eq!(len, MAX_OPS);
        assert_eq!(other, Some(11));
    }

    #[test]
    fn a_day_names_at_most_four_thousand_and_ninety_five_operations() {
        // SB-275, OD-3: the bound is the UTC day's, across its hours, and the next day starts over.
        COUNTING.with(|c| c.set(true));
        restart();
        let name = |i: usize| -> &'static str { Box::leak(format!("op_{i}").into_boxed_str()) };
        for i in 0..3000 {
            count_at(name(i), "ok", DAY0 + 10 * HOUR);
        }
        for i in 3000..5000 {
            count_at(name(i), "ok", DAY0 + 11 * HOUR);
        }
        // A name the day already counts stays named in a later hour.
        count_at(name(7), "ok", DAY0 + 11 * HOUR);
        let (named, other) = with_state(|s| {
            (
                s.counts.keys().filter(|op| **op != OTHER).count(),
                s.counts.get(OTHER).map(|c| c.ok),
            )
        });
        assert_eq!(
            (named, other),
            (MAX_OPS - 1 - 3000 + 1, Some(5000 - (MAX_OPS - 1) as u64))
        );
        count_at(name(4500), "ok", DAY0 + DAY);
        let next = with_state(|s| s.counts.contains_key(name(4500)));
        COUNTING.with(|c| c.set(false));
        restart();
        assert!(next, "the next day names its operations again");

        // Hourly lines that together name more than the day may (two owners, say) fold too.
        let hour = |h: i64, from: usize, to: usize| {
            let counts: Hour = (from..to)
                .map(|i| {
                    (
                        name(i),
                        Counts {
                            ok: 1,
                            ..Counts::default()
                        },
                    )
                })
                .collect();
            fold_hour(DAY0 + h * HOUR, &counts, None)
        };
        let day = fold_day(DAY0 / DAY, &[hour(1, 0, 3000), hour(2, 3000, 6000)]).unwrap();
        assert_eq!(day.ops.len(), MAX_OPS);
        assert!(day
            .ops
            .iter()
            .any(|o| o.op == OTHER && o.ok == (6000 - (MAX_OPS - 1)) as u64));
        assert_eq!(day.total.ok, 6000);
    }

    #[test]
    fn a_stamp_reads_back_as_its_hour() {
        for hour in [DAY0, DAY0 + 23 * HOUR, 0, 19_000 * DAY + 5 * HOUR] {
            assert_eq!(hour_from_stamp(&stamp(hour)), Some(hour));
        }
    }

    #[test]
    fn a_setup_or_policy_answer_counts_as_unknown_and_never_as_done() {
        // SB-255: the published map in CONTRACTS, "Operations digests".
        let code = Field::Code;
        for (event, field, value) in [
            ("recovery_hook", "result", "refused"),
            ("recovery_hook", "result", "no_account"),
            ("usage_check", "outcome", "rate_limited"),
            ("usage_check", "outcome", "superseded"),
            ("credential_source", "state", "refused"),
            ("credential_source", "state", "absent"),
            ("update_restart", "outcome", "refused"),
        ] {
            assert_eq!(
                outcome(event, &[(field, code(value))]),
                "unknown",
                "{event} {value}"
            );
        }
        // Still failed: an error SB-248 counts; still done: the operation's own success.
        assert_eq!(
            outcome("usage_check", &[("outcome", code("unreachable"))]),
            "failed"
        );
        assert_eq!(
            outcome("update_check", &[("outcome", code("check_failed"))]),
            "failed"
        );
        for (event, value) in [
            ("recovery_hook", "again"),
            ("update_check", "current"),
            ("analytics_flush", "sent"),
        ] {
            assert_eq!(
                outcome(event, &[("outcome", code(value))]),
                "ok",
                "{event} {value}"
            );
        }
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

/// SB-276: the contract's own vectors for `foldHour` and `foldDay` (DEC-0153), vendored from
/// passioncode-ai/fabric-agent-contract tag `v0.1.0+2026-11-06.1`,
/// `fixtures/vectors/ops-digest.json` (sha256 `ed027cec…89bb52`), never edited here. Times are
/// compared as hours: the vectors write `toISOString`'s fraction, Switchboard writes none, and
/// the schema takes both.
#[cfg(test)]
mod contract_vectors {
    use super::*;
    use serde_json::{json, Value};

    const VECTORS: &str = include_str!("vectors/ops-digest.json");

    /// A digest as JSON with each `from` and `to` replaced by the hour it names.
    fn hours_of(mut v: Value) -> Value {
        for key in ["from", "to"] {
            if let Some(s) = v.get(key).and_then(Value::as_str) {
                v[key] = json!(hour_from_stamp(s));
            }
        }
        v
    }

    #[test]
    fn the_contracts_hour_and_day_vectors_fold_the_same() {
        let vectors: Value = serde_json::from_str(VECTORS).unwrap();
        let hours = vectors["hour"].as_array().unwrap();
        assert_eq!(hours.len(), 11);
        for v in hours {
            let name = v["name"].as_str().unwrap();
            let input = &v["input"];
            let hour = hour_from_stamp(input["from"].as_str().unwrap()).unwrap();
            let mut counts = Hour::new();
            for c in input["counts"].as_array().unwrap() {
                let op: &'static str =
                    Box::leak(c["op"].as_str().unwrap().to_owned().into_boxed_str());
                counts.entry(op).or_default().add(Counts {
                    ok: c["ok"].as_u64().unwrap(),
                    failed: c["failed"].as_u64().unwrap(),
                    unknown: c["unknown"].as_u64().unwrap(),
                });
            }
            let previous: Option<Hourly> =
                serde_json::from_value(input["previous"].clone()).unwrap();
            // As the writer does: only the hour just before is a baseline (OD-4).
            let previous = previous.filter(|p| hour_from_stamp(&p.from) == Some(hour - HOUR));
            let got = serde_json::to_value(fold_hour(hour, &counts, previous.as_ref())).unwrap();
            assert_eq!(hours_of(got), hours_of(v["expected"].clone()), "{name}");
        }
        let days = vectors["day"].as_array().unwrap();
        assert_eq!(days.len(), 4);
        for v in days {
            let name = v["name"].as_str().unwrap();
            let input = &v["input"];
            let day = input["day"].as_str().unwrap();
            let start = hour_from_stamp(&format!("{day}T00:00:00Z")).unwrap();
            let hourly: Vec<Hourly> = serde_json::from_value(input["hourly"].clone()).unwrap();
            let got = serde_json::to_value(fold_day(start / DAY, &hourly)).unwrap();
            assert_eq!(got, v["expected"], "{name}");
        }
    }
}
