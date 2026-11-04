//! SB-248 (operator, 2026-11-04: «убедиться, что есть логирование ошибок. и мы можем корректно
//! видеть, если появляются какие-то проблемы у наших клиентов»): the errors a person's Switchboard
//! hits are counted for the anonymous report, under SB-246's switch.
//!
//! Nothing new is collected: an error is a line of the operations log (`oplog`) this module
//! classifies as one, and the log holds fixed codes, numbers and flags only, by construction. A
//! panic adds the file stem of the code that panicked. Counts are kept per UTC day, class (the log
//! event, or `panic`) and code in `<data>/error-counts.json` (0600), and a day is sent once it is
//! over, as one `app_error` event per class and code (sshlg-growth's answer of 2026-11-04 to
//! fabric-inbox: string `class` and `code`, numeric `count`, `day`), the largest first.
use crate::oplog::Field;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub(crate) const FILE: &str = "error-counts.json";
/// At most this many class and code pairs are kept for one day; the rarest go first.
const KEYS_PER_DAY: usize = 64;
/// At most this many `app_error` events leave for one day.
pub(crate) const EVENTS_PER_DAY: usize = 30;
/// A day older than this is dropped unsent (the queue keeps events a week, `MAX_AGE_SECONDS`).
const KEEP_DAYS: i64 = 7;

/// Log events that are an error whatever their fields say.
const ERROR_EVENTS: &[&str] = &[
    "analytics_state_unwritable",
    "control_refused",
    "control_slow",
    "dispatch_timeout",
    "external_file_refused",
    "hard_exit",
    "transaction_held",
    "usage_holds_unreadable",
    "usage_holds_unsaved",
    "vault_slow",
];
/// Field codes that mean something failed, in any event.
const FAILURES: &[&str] = &[
    "bundle_unknown",
    "check_failed",
    "config_unreadable",
    "download_failed",
    "failed",
    "install_failed",
    "panicked",
    "signature_failed",
    "unreadable",
    "unsaved",
    "unwritable",
    "unwritten",
];
/// `usage_check`'s outcomes that are failures of the check itself; a limit (`rate_limited`), an
/// account the provider no longer has and a kind of account with no usage are answers.
const USAGE_FAILURES: &[&str] = &[
    "http_error",
    "interrupted",
    "no_windows",
    "other",
    "rejected",
    "too_large",
    "unavailable",
    "unreachable",
    "vault",
];

/// The class and code a log line is counted under, or none when it is not an error.
pub(crate) fn classify(
    event: &'static str,
    fields: &[(&'static str, Field)],
) -> Option<(&'static str, &'static str)> {
    let mut codes = fields.iter().filter_map(|(_, f)| match f {
        Field::Code(code) => Some(*code),
        _ => None,
    });
    let code = match event {
        "usage_check" => codes.find(|c| USAGE_FAILURES.contains(c)),
        "update_download" => codes.find(|c| *c == "partial" || FAILURES.contains(c)),
        _ if ERROR_EVENTS.contains(&event) => Some(codes.next().unwrap_or("logged")),
        _ => codes.find(|c| FAILURES.contains(c)),
    }?;
    (valid(event) && valid(code)).then_some((event, code))
}

/// Lower case letters, digits and `_`, a letter first, at most 48: growth's `where` names a
/// string property's value this way, and nothing else may leave.
pub(crate) fn valid(code: &str) -> bool {
    let b = code.as_bytes();
    !b.is_empty()
        && b.len() <= 48
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// A panic's code: the stem of the source file that panicked (`monitor` for
/// `crates/switchboard-runtime/src/monitor.rs`), `unknown` when it has none that is valid.
fn panic_code(file: &str) -> String {
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .replace('-', "_");
    if valid(&stem) {
        stem
    } else {
        "unknown".into()
    }
}

type Key = (i64, String, String);
type Pending = BTreeMap<Key, u64>;
/// Errors counted since the last save, by day, class and code.
#[cfg(not(test))]
static PENDING: Mutex<Pending> = Mutex::new(BTreeMap::new());
#[cfg(test)]
thread_local! {
    /// Tests share one process: each test thread counts on its own.
    static PENDING: std::cell::RefCell<Pending> = const { std::cell::RefCell::new(BTreeMap::new()) };
}
/// Runs `f` on the counts; `wait` false gives up (none) rather than wait for the lock.
fn with_pending<R>(wait: bool, f: impl FnOnce(&mut Pending) -> R) -> Option<R> {
    #[cfg(not(test))]
    {
        let mut pending = if wait {
            PENDING.lock().ok()?
        } else {
            PENDING.try_lock().ok()?
        };
        Some(f(&mut pending))
    }
    #[cfg(test)]
    {
        let _ = wait;
        PENDING.with(|p| Some(f(&mut p.borrow_mut())))
    }
}
/// One writer of the file at a time (a background pass, a panic).
static FILE_LOCK: Mutex<()> = Mutex::new(());
/// The folder and day the file was last looked at for days that are over.
static CHECKED: Mutex<Option<(PathBuf, i64)>> = Mutex::new(None);
/// The data folder of the owner whose panics are counted.
static ROOT: OnceLock<PathBuf> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Tests share one process: only a test that asks counts its own log lines, so no other test
    /// sends an `app_error` it did not expect.
    pub(crate) static COUNTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Counts one log line when it is an error. Called by every `oplog::event`.
pub(crate) fn note(event: &'static str, fields: &[(&'static str, Field)]) {
    #[cfg(test)]
    if !COUNTING.with(|c| c.get()) {
        return;
    }
    if let Some((class, code)) = classify(event, fields) {
        add(day(now()), class.to_owned(), code.to_owned());
    }
}

pub(crate) fn add(day: i64, class: String, code: String) {
    with_pending(true, |p| *p.entry((day, class, code)).or_default() += 1);
}

/// The owner (the desktop app, `switchboard serve`): counts its panics too, keeping the hook in
/// place before it (the panic is still reported as before). Once per process.
pub fn install_panic_counter(root: &Path) {
    if ROOT.set(root.to_owned()).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let code = info
            .location()
            .map(|l| panic_code(l.file()))
            .unwrap_or_else(|| "unknown".into());
        // A panic while the counts are locked loses this one count, never the process.
        with_pending(false, |p| {
            *p.entry((day(now()), "panic".into(), code)).or_default() += 1
        });
        if let Some(root) = ROOT.get() {
            save(root, false);
        }
        previous(info);
    }));
}

#[derive(Default, Serialize, Deserialize)]
struct Counts {
    /// UTC day (days since 1970-01-01) → class → code → count.
    days: BTreeMap<i64, BTreeMap<String, BTreeMap<String, u64>>>,
}

fn read(path: &Path) -> Counts {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Adds what was counted since the last save to the file; nothing is read or written when
/// nothing was counted. `wait` false (a panic) gives up rather than wait for a lock.
fn save(root: &Path, wait: bool) -> bool {
    let _file = if wait {
        FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    } else {
        match FILE_LOCK.try_lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        }
    };
    let taken = with_pending(wait, std::mem::take).unwrap_or_default();
    if taken.is_empty() {
        return false;
    }
    let path = root.join(FILE);
    let mut counts = read(&path);
    for ((day, class, code), n) in taken {
        *counts
            .days
            .entry(day)
            .or_default()
            .entry(class)
            .or_default()
            .entry(code)
            .or_default() += n;
    }
    for codes in counts.days.values_mut() {
        cap(codes);
    }
    write(&path, &counts);
    true
}

/// Keeps the `KEYS_PER_DAY` largest pairs of one day.
fn cap(day: &mut BTreeMap<String, BTreeMap<String, u64>>) {
    let mut all: Vec<(u64, String, String)> = day
        .iter()
        .flat_map(|(class, codes)| {
            codes
                .iter()
                .map(move |(code, n)| (*n, class.clone(), code.clone()))
        })
        .collect();
    if all.len() <= KEYS_PER_DAY {
        return;
    }
    all.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| (&a.1, &a.2).cmp(&(&b.1, &b.2))));
    day.clear();
    for (n, class, code) in all.into_iter().take(KEYS_PER_DAY) {
        day.entry(class).or_default().insert(code, n);
    }
}

fn write(path: &Path, counts: &Counts) {
    if counts.days.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Ok(body) = serde_json::to_vec(counts) {
        let _ = switchboard_core::private_fs::private_write(path, &body);
    }
}

/// One day's report: `(day, class, code, count)`, the largest first.
pub(crate) type Report = Vec<(i64, String, String, u64)>;

/// Every pass: saves what was counted, then takes the days before `today` out of the file, at
/// most `EVENTS_PER_DAY` pairs each, the largest first; a day older than a week goes unsent. The
/// file is read only when something was counted or a past day waits in it.
pub(crate) fn take_due(root: &Path, today: i64) -> Report {
    let saved = save(root, true);
    let _file = FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // The idle budget: with nothing counted, the file is looked at once a day.
    {
        let mut checked = CHECKED.lock().unwrap_or_else(|p| p.into_inner());
        let this = Some((root.to_owned(), today));
        if !saved && *checked == this {
            return Vec::new();
        }
        *checked = this;
    }
    let path = root.join(FILE);
    if !path.exists() {
        return Vec::new();
    }
    let mut counts = read(&path);
    if counts.days.keys().all(|d| *d >= today) {
        return Vec::new();
    }
    let kept = counts.days.split_off(&today);
    let mut due = Vec::new();
    for (day, classes) in std::mem::take(&mut counts.days) {
        if day < today - KEEP_DAYS {
            continue;
        }
        let mut pairs: Vec<(i64, String, String, u64)> = classes
            .into_iter()
            .flat_map(|(class, codes)| {
                codes
                    .into_iter()
                    .map(move |(code, n)| (day, class.clone(), code, n))
            })
            .collect();
        pairs.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| (&a.1, &a.2).cmp(&(&b.1, &b.2))));
        due.extend(pairs.into_iter().take(EVENTS_PER_DAY));
    }
    counts.days = kept;
    write(&path, &counts);
    due
}

/// A report that could not be queued goes back to the counts, to be saved on the next pass.
pub(crate) fn restore(report: Report) {
    with_pending(true, |p| {
        for (day, class, code, n) in report {
            *p.entry((day, class, code)).or_default() += n;
        }
    });
}

/// Switchboard's switch was turned off: what was counted is forgotten, on disk too.
pub(crate) fn discard(root: &Path) {
    let _file = FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    with_pending(true, |p| p.clear());
    let _ = std::fs::remove_file(root.join(FILE));
}

/// While the switch is off nothing is counted for long: each pass forgets what the last one saw.
pub(crate) fn forget_pending() {
    with_pending(true, |p| p.clear());
}

/// `YYYY-MM-DD` of a UTC day number.
pub(crate) fn date(day: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(day * 86_400)
        .map(|t| {
            let d = t.date();
            format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
        })
        .unwrap_or_default()
}

pub(crate) fn day(time: i64) -> i64 {
    time.div_euclid(86_400)
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_failures_count_and_under_codes_only() {
        let code = |c| Field::Code(c);
        assert_eq!(
            classify("update_check", &[("outcome", code("check_failed"))]),
            Some(("update_check", "check_failed"))
        );
        assert_eq!(
            classify("update_check", &[("outcome", code("current"))]),
            None
        );
        assert_eq!(
            classify("usage_check", &[("outcome", code("unreachable"))]),
            Some(("usage_check", "unreachable"))
        );
        assert_eq!(
            classify("usage_check", &[("outcome", code("rate_limited"))]),
            None
        );
        assert_eq!(
            classify("update_download", &[("outcome", code("partial"))]),
            Some(("update_download", "partial"))
        );
        assert_eq!(classify("handover", &[("outcome", code("partial"))]), None);
        assert_eq!(
            classify("transaction_held", &[("seconds", Field::Number(9))]),
            Some(("transaction_held", "logged"))
        );
        assert_eq!(
            classify("launch_record", &[("result", code("failed"))]),
            Some(("launch_record", "failed"))
        );
        assert_eq!(classify("owner_started", &[("outcome", code("ok"))]), None);
        // A code that does not read as one is never sent.
        assert_eq!(
            classify("launch_record", &[("result", code("Failed"))]),
            None
        );
    }

    #[test]
    fn a_panic_is_named_by_its_file_stem_only() {
        assert_eq!(
            panic_code("crates/switchboard-runtime/src/monitor.rs"),
            "monitor"
        );
        assert_eq!(
            panic_code("/Users/someone/.cargo/registry/src/x/tokio-1.0/src/lib.rs"),
            "lib"
        );
        assert_eq!(panic_code("src/person-errors.rs"), "person_errors");
        assert_eq!(panic_code(""), "unknown");
        assert_eq!(panic_code("src/Ünïcode.rs"), "unknown");
    }

    #[test]
    fn dates_read_as_utc_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(day(1_762_300_800)), "2025-11-05");
    }

    #[test]
    fn a_day_is_sent_once_it_is_over_the_largest_first_and_at_most_thirty() {
        let dir = tempfile::tempdir().unwrap();
        forget_pending();
        let today = 20_000;
        for i in 0..40u64 {
            for _ in 0..=i {
                add(today - 1, "update_check".into(), format!("code_{i:02}"));
            }
        }
        add(today, "usage_check".into(), "unreachable".into());
        let due = take_due(dir.path(), today);
        assert_eq!(due.len(), EVENTS_PER_DAY);
        assert_eq!(
            due[0],
            (today - 1, "update_check".into(), "code_39".into(), 40)
        );
        assert!(due.iter().all(|(d, ..)| *d == today - 1));
        // Today's count waits in the file, and nothing is sent twice.
        assert!(take_due(dir.path(), today).is_empty());
        let due = take_due(dir.path(), today + 1);
        assert_eq!(
            due,
            vec![(today, "usage_check".into(), "unreachable".into(), 1)]
        );
        assert!(!dir.path().join(FILE).exists(), "an empty file goes");
    }

    #[test]
    fn a_day_older_than_a_week_goes_unsent_and_off_forgets_everything() {
        let dir = tempfile::tempdir().unwrap();
        forget_pending();
        add(100, "hard_exit".into(), "logged".into());
        add(110, "hard_exit".into(), "logged".into());
        let due = take_due(dir.path(), 111);
        assert_eq!(due, vec![(110, "hard_exit".into(), "logged".into(), 1)]);
        add(111, "hard_exit".into(), "logged".into());
        save(dir.path(), true);
        assert!(dir.path().join(FILE).exists());
        discard(dir.path());
        assert!(!dir.path().join(FILE).exists());
        assert!(take_due(dir.path(), 200).is_empty());
    }

    #[test]
    fn an_error_line_of_the_log_is_counted_and_any_other_line_is_not() {
        let dir = tempfile::tempdir().unwrap();
        forget_pending();
        COUNTING.with(|c| c.set(true));
        crate::oplog::event("launch_record", &[("result", Field::Code("failed"))]);
        crate::oplog::event("launch_record", &[("result", Field::Code("failed"))]);
        crate::oplog::event("owner_started", &[("outcome", Field::Code("ok"))]);
        COUNTING.with(|c| c.set(false));
        let today = day(now());
        assert!(take_due(dir.path(), today).is_empty(), "today is not over");
        let due = take_due(dir.path(), today + 1);
        assert_eq!(
            due,
            vec![(today, "launch_record".into(), "failed".into(), 2)]
        );
    }

    #[test]
    fn nothing_counted_reads_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        forget_pending();
        assert!(take_due(dir.path(), 20_000).is_empty());
        assert!(!dir.path().join(FILE).exists());
    }
}
