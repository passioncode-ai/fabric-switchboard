//! SB-412 (board finding `1ddf0300`): a dispatch refused `at_capacity` was silent. Each
//! such refusal is counted in `<data>/dispatch-refusals.json` (0600: refusal times of the last
//! day, the newest board request's id and when a slot freed since; never text or a folder), so
//! *Needs attention* lists one `dispatch_refused` item while the last hour holds any, and
//! `switchboard capacity` reports the last hour. The file is written at a refusal and, by the
//! one-minute session watch, once when the pool has room again after one; never on a timer.
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
}

static LOCK: Mutex<()> = Mutex::new(());

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
    let _held = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut state = read(root);
    state.refused.retain(|t| now - t < KEEP_SECONDS);
    state.refused.push(now);
    let excess = state.refused.len().saturating_sub(MAX_TIMES);
    state.refused.drain(..excess);
    state.newest_request = request.and_then(plain);
    state.freed_at = None;
    write(root, &state);
    crate::oplog::event(
        "dispatch_refused",
        &[(
            "reason",
            crate::oplog::Field::Code(crate::dispatch::AT_CAPACITY),
        )],
    );
}

/// The one-minute watch: after a refusal of the last hour, the first scan whose pool has room
/// (a read table under the cap) is recorded as `freed_at`. Reads nothing without a refusal file.
pub fn observe(root: &Path, scan: &Value, now: i64) {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return;
    }
    let _held = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut state = read(root);
    let Some(last) = state.refused.last().copied() else {
        return;
    };
    if state.freed_at.is_some() || now - last >= HOUR {
        return;
    }
    let cap = crate::capacity::cap(root).ok().flatten();
    let read_table = crate::session_watch::table_read(scan);
    if read_table && crate::capacity::decide(cap, scan).is_ok() {
        state.freed_at = Some(now);
        write(root, &state);
    }
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
        crate::capacity::set(root.path(), Some(1)).unwrap();
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
}
