//! SB-412 (board finding `1ddf0300`): a dispatch refused `at_capacity` was silent. Each
//! such refusal is counted in `<data>/dispatch-refusals.json` (0600: refusal times of the last
//! day, the newest board request's id and when a slot freed since; never text or a folder), so
//! *Needs attention* lists one `dispatch_refused` item while the last hour holds any, and
//! `switchboard capacity` reports the last hour. The file is written at a refusal and, by the
//! one-minute session watch, once when the pool has room again after one; never on a timer.
//!
//! SB-417: the board starts refused since the last one the pool admitted are a wait, however long
//! it lasts: `waiting` counts them with the first refusal's time (`waiting_since`), and the next
//! board start the pool admits (`admitted`) ends it. `switchboard capacity` reports it as
//! `refused_waiting` and Needs attention lists it, with no one-hour cutoff; `refused_last_hour`
//! stays for callers that read it.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;

pub const FILE: &str = "dispatch-refusals.json";
const MAX_BYTES: u64 = 64 * 1024;
/// Refusal times are kept a day; the attention item and the capacity report read the last hour.
const KEEP_SECONDS: i64 = 24 * 3600;
const HOUR: i64 = 3600;
const MAX_TIMES: usize = 2000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct State {
    /// Unix seconds of each refusal, oldest first.
    #[serde(default)]
    refused: Vec<i64>,
    /// The newest refused request's id (a board request's UUID or `inbox`).
    #[serde(default)]
    newest_request: Option<String>,
    /// When the session watch first saw room in the pool after the newest refusal.
    #[serde(default)]
    freed_at: Option<i64>,
    /// SB-417: refusals since the last board start the pool admitted.
    #[serde(default)]
    waiting: u64,
    /// SB-417: the first of them.
    #[serde(default)]
    waiting_since: Option<i64>,
}

static LOCK: Mutex<()> = Mutex::new(());
/// Review round 1, P1-C: the file's writers are the owner (a refusal, the watch, an admitted
/// dispatch) and the CLI (an admitted hosted start Fabric asked for), two processes: each read,
/// change and write holds this lock as well as `LOCK`.
pub const LOCK_FILE: &str = "dispatch-refusals.lock";

/// Both locks, held for one read, change and write. A file lock that cannot be taken still lets
/// the write happen, as `monitors` and `resumable` do.
struct Held {
    _mutex: std::sync::MutexGuard<'static, ()>,
    _file: Option<std::fs::File>,
}
fn held(root: &Path) -> Held {
    let mutex = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(LOCK_FILE))
        .ok()
        .and_then(|file| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            file.lock().ok().map(|()| file)
        });
    Held {
        _mutex: mutex,
        _file: file,
    }
}

fn read(root: &Path) -> State {
    switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(root: &Path, state: &State) {
    if let Ok(bytes) = serde_json::to_vec(state) {
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// A request id kept as written only when it is plain: letters, digits and hyphens, at most 64.
fn plain(request: &str) -> Option<String> {
    ((1..=64).contains(&request.len())
        && request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    .then(|| request.to_owned())
}

/// Counts one `at_capacity` refusal of `request` at `now`.
pub fn record(root: &Path, request: Option<&str>, now: i64) {
    let _held = held(root);
    let mut state = read(root);
    state.refused.retain(|t| now - t < KEEP_SECONDS);
    state.refused.push(now);
    let excess = state.refused.len().saturating_sub(MAX_TIMES);
    state.refused.drain(..excess);
    state.newest_request = request.and_then(plain);
    state.freed_at = None;
    state.waiting = state.waiting.saturating_add(1);
    state.waiting_since.get_or_insert(now);
    write(root, &state);
    crate::oplog::event(
        "dispatch_refused",
        &[(
            "reason",
            crate::oplog::Field::Code(crate::dispatch::AT_CAPACITY),
        )],
    );
}

/// The one-minute watch: after a refusal that still waits, or one of the last hour, the first
/// scan whose pool would admit a board start (a read table with room, or the board under its
/// share) is recorded as `freed_at`. Reads nothing without a refusal file.
pub fn observe(root: &Path, scan: &Value, now: i64) {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return;
    }
    let _held = held(root);
    let mut state = read(root);
    let Some(last) = state.refused.last().copied().or(state.waiting_since) else {
        return;
    };
    if state.freed_at.is_some() || (state.waiting == 0 && now - last >= HOUR) {
        return;
    }
    let read_table = crate::session_watch::table_read(scan);
    let (pool, count, _) = crate::capacity::view_of(root, scan, now);
    if read_table && crate::capacity::decide(&pool, &count, crate::capacity::Start::Board).is_ok() {
        state.freed_at = Some(now);
        write(root, &state);
    }
}

/// SB-417: a board start the pool admitted was recorded as running at `now` (a dispatch watched,
/// a hosted start Fabric asked for recorded): the board starts refused before no longer wait. Reads nothing without a refusal file, writes nothing while none waits.
pub fn admitted(root: &Path, now: i64) {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return;
    }
    let _held = held(root);
    let mut state = read(root);
    if state.waiting == 0 && state.waiting_since.is_none() {
        return;
    }
    state.waiting = 0;
    state.waiting_since = None;
    state.freed_at.get_or_insert(now);
    write(root, &state);
}

/// SB-417: the board starts refused since the last one the pool admitted: `{count, since,
/// last_at, newest_request, freed, freed_at}`, with no time cutoff; none while none waits.
pub fn waiting(root: &Path) -> Option<Value> {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return None;
    }
    waiting_of(&read(root))
}

fn waiting_of(state: &State) -> Option<Value> {
    if state.waiting == 0 {
        return None;
    }
    let last = state.refused.last().copied().or(state.waiting_since);
    Some(json!({
        "count": state.waiting,
        "since": state.waiting_since,
        "last_at": last,
        "newest_request": state.newest_request,
        "freed": state.freed_at.is_some_and(|f| last.is_some_and(|l| f >= l)),
        "freed_at": state.freed_at,
    }))
}

/// The last hour's refusals: `{count, last_at, newest_request, freed, freed_at, per_hour}`
/// (`per_hour` the last day's counts by hour, newest first), or none when the last hour holds
/// none.
pub fn last_hour(root: &Path, now: i64) -> Option<Value> {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return None;
    }
    summary(&read(root), now)
}

fn summary(state: &State, now: i64) -> Option<Value> {
    let recent: Vec<i64> = state
        .refused
        .iter()
        .copied()
        .filter(|t| now - t < HOUR && *t <= now)
        .collect();
    let last = *recent.last()?;
    let mut per_hour: std::collections::BTreeMap<i64, u64> = Default::default();
    for t in state.refused.iter().filter(|t| now - **t < KEEP_SECONDS) {
        *per_hour.entry(t - t.rem_euclid(HOUR)).or_default() += 1;
    }
    let per_hour: Vec<Value> = per_hour
        .into_iter()
        .rev()
        .map(|(hour, count)| json!({"hour": hour, "count": count}))
        .collect();
    Some(json!({
        "count": recent.len(),
        "last_at": last,
        "newest_request": state.newest_request,
        "freed": state.freed_at.is_some_and(|f| f >= last),
        "freed_at": state.freed_at,
        "per_hour": per_hour,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_is_counted_and_only_the_last_hour_is_reported() {
        let root = tempfile::tempdir().unwrap();
        assert!(last_hour(root.path(), 10_000).is_none(), "no file, nothing");
        record(
            root.path(),
            Some("11111111-2222-4333-8444-555555555555"),
            3_000,
        );
        record(root.path(), Some("../not plain"), 9_000);
        record(
            root.path(),
            Some("22222222-2222-4333-8444-555555555555"),
            9_500,
        );
        let report = last_hour(root.path(), 10_000).unwrap();
        assert_eq!(report["count"], 2, "the one at 3000 is older than an hour");
        assert_eq!(report["last_at"], 9_500);
        assert_eq!(
            report["newest_request"],
            "22222222-2222-4333-8444-555555555555"
        );
        assert_eq!(report["freed"], false);
        assert_eq!(
            report["per_hour"],
            json!([{"hour": 7_200, "count": 2}, {"hour": 0, "count": 1}])
        );
        // A refusal more than an hour ago lists nothing.
        assert!(last_hour(root.path(), 9_500 + HOUR).is_none());
        // A request id that is not plain is never kept.
        record(root.path(), Some("../x"), 9_600);
        assert!(last_hour(root.path(), 9_700).unwrap()["newest_request"].is_null());
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.path().join(FILE))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
    }

    #[test]
    fn room_in_the_pool_after_a_refusal_is_recorded_once_and_a_new_refusal_clears_it() {
        let root = tempfile::tempdir().unwrap();
        // SB-417: a share of 0, so only room in the pool admits a board start.
        crate::capacity::set_pool(root.path(), Some(1), crate::capacity::Share::Set(0)).unwrap();
        let pool = |working: usize, degraded: bool| {
            let rows: Vec<Value> = (0..working).map(|_| json!({"quiet": false})).collect();
            let degraded = if degraded {
                json!([{"source": "process_table", "reason": "x"}])
            } else {
                json!([])
            };
            json!({"sessions": rows, "total": working, "degraded": degraded})
        };
        // Nothing refused: nothing written.
        observe(root.path(), &pool(0, false), 100);
        assert!(!root.path().join(FILE).exists());
        record(root.path(), Some("inbox"), 100);
        // Still full, or a table not read: not freed.
        observe(root.path(), &pool(1, false), 160);
        observe(root.path(), &pool(0, true), 170);
        assert_eq!(last_hour(root.path(), 200).unwrap()["freed"], false);
        observe(root.path(), &pool(0, false), 220);
        let report = last_hour(root.path(), 230).unwrap();
        assert_eq!(
            (report["freed"].clone(), report["freed_at"].clone()),
            (json!(true), json!(220))
        );
        // A later read keeps the first time; a new refusal clears it.
        observe(root.path(), &pool(0, false), 280);
        assert_eq!(last_hour(root.path(), 290).unwrap()["freed_at"], 220);
        record(root.path(), Some("inbox"), 300);
        assert_eq!(last_hour(root.path(), 310).unwrap()["freed"], false);
    }

    #[test]
    fn refused_board_starts_wait_past_the_hour_until_the_pool_admits_one() {
        // SB-417: the wait has no one-hour cutoff; the next board start admitted ends it.
        let root = tempfile::tempdir().unwrap();
        assert!(waiting(root.path()).is_none(), "no file, no wait");
        admitted(root.path(), 50);
        assert!(
            !root.path().join(FILE).exists(),
            "nothing written without a refusal"
        );
        record(
            root.path(),
            Some("11111111-2222-4333-8444-555555555555"),
            100,
        );
        record(root.path(), Some("inbox"), 200);
        let now = 200 + 5 * HOUR;
        assert!(
            last_hour(root.path(), now).is_none(),
            "the hour is long over"
        );
        let wait = waiting(root.path()).unwrap();
        assert_eq!(
            (
                wait["count"].clone(),
                wait["since"].clone(),
                wait["last_at"].clone()
            ),
            (json!(2), json!(100), json!(200))
        );
        assert_eq!(wait["newest_request"], "inbox");
        assert_eq!(wait["freed"], false);
        // The watch still notes room for a wait older than the hour (no cap: room).
        let empty = json!({"sessions": [], "total": 0, "degraded": []});
        observe(root.path(), &empty, now);
        assert_eq!(waiting(root.path()).unwrap()["freed_at"], now);
        // A board start the pool admits ends the wait.
        admitted(root.path(), now + 10);
        assert!(waiting(root.path()).is_none());
        // A refusal after it starts a new wait from its own time.
        record(root.path(), None, now + 20);
        let wait = waiting(root.path()).unwrap();
        assert_eq!(
            (wait["count"].clone(), wait["since"].clone()),
            (json!(1), json!(now + 20))
        );
        assert_eq!(wait["freed"], false);
    }
}
