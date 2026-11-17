//! SB-378 (operator 2026-11-14, relayed by Fabric on the board, request `52f5a962`): one cap on
//! the coding sessions this computer runs at once. `<data>/capacity.json` (0600) holds the cap
//! the person set (`switchboard capacity set N`); absent, there is no cap. `switchboard dispatch`
//! and `launch --hosted` refuse a start at the cap with `AT_CAPACITY`, so Fabric's board, Fabric
//! Dashboards' console and the CLI share one gate; a launcher takes the refusal as a wait and asks
//! again. A start by the person in a Terminal or the window is never gated.
//!
//! SB-417 (operator 2026-11-17: «the session manager must start work when it arrives»): measured
//! that day, a cap of 20 with 34 sessions counted, most of them the person's, refused every board
//! start for as long as they stayed. Two changes:
//! - **A board share.** `capacity.json` may hold `board`, the number of board runs (a dispatch, or
//!   a hosted launch Fabric starts) that may run whatever the interactive sessions hold; absent,
//!   `max(2, cap / 4)`. A board start is admitted while the pool has room *or* the board's own
//!   live runs are under the share.
//! - **Counting per session.** A Claude Code process with its own session file counts unless its
//!   own session is quiet (its transcript and its status unchanged for `QUIET_MINUTES`, outside a
//!   turn), waits on a limit stop, or waits on a wake-up, a monitor, a cron job or a hold
//!   (`monitors`). Another session writing in the same folder no longer keeps it counted. A
//!   process with no session file (Codex, an older Claude Code) is counted by its folder, as
//!   before. The facts are the duty officer's own readers (`machine_sessions::session_files`,
//!   `session_last_write`, `monitors::holds`, `stops::live_stop_sessions`), not a second reader.
//!
//! SB-397 D-2: moved from switchboard-runtime; `root` is the data folder of the app that keeps the
//! cap, and the sessions counted are the scan's, with that app's homes (`Accounts`).
use crate::accounts::Accounts;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const FILE: &str = "capacity.json";
const MAX_BYTES: u64 = 4096;
/// The highest cap a person can set.
pub const MAX_CAP: u32 = 500;
/// The smallest board share when the person set none.
pub const MIN_BOARD_SHARE: u32 = 2;

/// `launch --hosted`'s refusal at the cap; `dispatch` answers `dispatch::AT_CAPACITY` instead.
pub const AT_CAPACITY: &str = "At capacity: this computer runs as many coding sessions as its limit allows. The start waits until one ends.";
pub const CAP_INVALID: &str = "Give the session limit as a whole number from 1 to 500.";
pub const BOARD_INVALID: &str =
    "Give the board's share as a whole number from 0 to the session limit, with a limit set.";
pub const CLEAR_INVALID: &str =
    "Clearing removes the session limit and the board's share together: give no limit or share with it.";
pub const UNREADABLE: &str =
    "The session limit (capacity.json) could not be read. Set it again, or clear it.";
pub const NOT_SAVED: &str = "The session limit could not be saved.";

/// What `capacity.json` holds. No `deny_unknown_fields`: a field a later version adds never makes
/// the file unreadable. `board` is written only when the person set it, so a file without a
/// board share stays `{"cap":N}`, which every earlier version reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pool {
    pub cap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<u32>,
}

/// The board share when the person set none: a quarter of the cap, at least two.
pub fn default_board_share(cap: u32) -> u32 {
    (cap / 4).max(MIN_BOARD_SHARE)
}

impl Pool {
    /// How many board runs may run whatever the interactive sessions hold; none without a cap.
    pub fn board_share(&self) -> Option<u32> {
        self.cap
            .map(|cap| self.board.unwrap_or_else(|| default_board_share(cap)))
    }
}

/// Who asks for a start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// A hosted console a person opens (`launch --hosted` from the CLI or Fabric Dashboards).
    Interactive,
    /// A run for Fabric's board: `dispatch`, or a hosted launch Fabric starts (`--surface fabric`).
    Board,
}

/// The pool as `capacity.json` keeps it; empty when no file holds one.
pub fn read(root: &Path) -> Result<Pool, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Pool::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes = crate::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    let saved: Pool = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    let cap = saved.cap.filter(|c| (1..=MAX_CAP).contains(c));
    Ok(Pool {
        cap,
        board: saved.board.filter(|b| cap.is_some_and(|c| *b <= c)),
    })
}

/// The cap the person set; none when no file holds one.
pub fn cap(root: &Path) -> Result<Option<u32>, String> {
    Ok(read(root)?.cap)
}

/// What a change does with the board's share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Share {
    /// Keeps the share the person set, clamped to the new cap.
    Keep,
    /// Back to the default share (a quarter of the cap, at least 2).
    Default,
    /// This many board runs, from 0 to the cap.
    Set(u32),
}

/// Sets the cap, keeping a share the person set (clamped to the cap), or clears the pool with
/// `None`. A file without a share is `{"cap":N}`, as every earlier version wrote and reads it.
pub fn set(root: &Path, cap: Option<u32>) -> Result<(), String> {
    match cap {
        Some(_) => set_pool(root, cap, Share::Keep).map(|_| ()),
        None => save(root, Pool::default()),
    }
}

/// SB-417: changes the cap (`None` keeps the cap already set) and the board's share. Answers
/// whether a kept share was larger than the new cap and so was clamped to it.
pub fn set_pool(root: &Path, cap: Option<u32>, share: Share) -> Result<bool, String> {
    if cap.is_some_and(|c| !(1..=MAX_CAP).contains(&c)) {
        return Err(CAP_INVALID.into());
    }
    let saved = read(root).unwrap_or_default();
    let cap = cap.or(saved.cap);
    let mut clamped = false;
    let board = match share {
        Share::Keep => saved.board.zip(cap).map(|(b, c)| {
            clamped = b > c;
            b.min(c)
        }),
        Share::Default => None,
        Share::Set(board) => {
            if !cap.is_some_and(|c| board <= c) {
                return Err(BOARD_INVALID.into());
            }
            Some(board)
        }
    };
    if cap.is_none() {
        return Ok(false);
    }
    save(root, Pool { cap, board })?;
    Ok(clamped)
}

fn save(root: &Path, pool: Pool) -> Result<(), String> {
    if pool.cap.is_some_and(|c| !(1..=MAX_CAP).contains(&c)) {
        return Err(CAP_INVALID.into());
    }
    if pool
        .board
        .is_some_and(|b| !pool.cap.is_some_and(|c| b <= c))
    {
        return Err(BOARD_INVALID.into());
    }
    let bytes = serde_json::to_vec(&pool).map_err(|_| NOT_SAVED)?;
    crate::private_fs::private_write(&root.join(FILE), &bytes).map_err(|_| NOT_SAVED.into())
}

/// What the pool reads about one Claude Code process's own session (`sessions/<pid>.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Own {
    pub session: String,
    /// `busy`, `shell`, `waiting`, `idle` as Claude Code writes it.
    pub status: String,
    /// The newer of its own transcript's last write and its last status change, Unix seconds.
    pub activity: Option<i64>,
}

/// The facts a count reads beside the scan.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// Each Claude Code process's own session, by pid.
    pub own: HashMap<u32, Own>,
    /// Sessions waiting on a limit stop.
    pub stopped: HashSet<String>,
    /// Sessions waiting on a wait of their own (a wake-up, a monitor, a cron job) or a hold.
    pub waiting: HashSet<String>,
    /// Board runs live now, by launch id (`Accounts::board_launches`).
    pub board: HashSet<String>,
    /// Admitted starts not yet recorded as running: board starts and others.
    pub pending_board: usize,
    pub pending_other: usize,
}

/// Why a live process counts against the pool or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counted {
    Working,
    Quiet,
    /// On a limit stop: it holds no load until an account takes it again.
    LimitStop,
    /// On a wait of its own or a hold: it holds no load until the wait ends.
    Waits,
}

/// One row of the scan, read with the facts.
pub fn classify(row: &Value, facts: &Facts, now: i64) -> Counted {
    let own = row["pid"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .and_then(|pid| facts.own.get(&pid));
    let Some(own) = own else {
        // No session file (Codex, a Claude Code that writes none): by its folder, as before.
        return if row["quiet"] == true {
            Counted::Quiet
        } else {
            Counted::Working
        };
    };
    // In a turn (the model or a command of its own runs): working, however long it is silent,
    // and whatever it waits on besides (a busy session that armed a monitor, or one a Fabric claim
    // holds, still works; review round 1, P1-A).
    if matches!(own.status.as_str(), "busy" | "shell") {
        return Counted::Working;
    }
    if facts.stopped.contains(&own.session) {
        return Counted::LimitStop;
    }
    if facts.waiting.contains(&own.session) {
        return Counted::Waits;
    }
    match own.activity {
        Some(at) if now - at >= crate::machine_sessions::QUIET_MINUTES * 60 => Counted::Quiet,
        Some(_) => Counted::Working,
        None if row["quiet"] == true => Counted::Quiet,
        None => Counted::Working,
    }
}

/// The pool's numbers from one scan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Count {
    pub used: usize,
    pub quiet: usize,
    pub limit_stops: usize,
    pub waits: usize,
    pub board_used: usize,
    /// Starts the pool admitted that have not been recorded as running yet (`board-pending.json`).
    pub pending: usize,
    pub total: usize,
    /// Whether the process table was read; an unread table admits every start.
    pub read: bool,
}

/// Counts a scan with its facts.
pub fn count(scan: &Value, facts: &Facts, now: i64) -> Count {
    let mut c = Count {
        board_used: facts.board.len() + facts.pending_board,
        used: facts.pending_board + facts.pending_other,
        pending: facts.pending_board + facts.pending_other,
        read: !scan["degraded"].as_array().is_some_and(|d| !d.is_empty()),
        ..Count::default()
    };
    for row in scan["sessions"].as_array().into_iter().flatten() {
        c.total += 1;
        match classify(row, facts, now) {
            Counted::Working => c.used += 1,
            Counted::Quiet => c.quiet += 1,
            Counted::LimitStop => c.limit_stops += 1,
            Counted::Waits => c.waits += 1,
        }
    }
    c
}

/// The sessions a scan counts against the cap without facts: those not quiet by their folder.
pub fn used(scan: &Value) -> usize {
    count(scan, &Facts::default(), 0).used
}

/// Whether a start may begin. An unread process table admits: an unread table is not a full
/// pool, and the refusal would stop every launcher. A board start is admitted while the pool
/// has room or the board's own runs are under its share.
pub fn admits(pool: &Pool, count: &Count, start: Start) -> bool {
    let Some(cap) = pool.cap else {
        return true;
    };
    if !count.read || count.used < cap as usize {
        return true;
    }
    start == Start::Board
        && pool
            .board_share()
            .is_some_and(|share| count.board_used < share as usize)
}

/// `admits` as a refusal.
pub fn decide(pool: &Pool, count: &Count, start: Start) -> Result<(), String> {
    if admits(pool, count, start) {
        Ok(())
    } else {
        Err(AT_CAPACITY.into())
    }
}

/// The pool as a launcher reads it.
pub fn view(pool: &Pool, count: &Count, scan: &Value) -> Value {
    let cap = pool.cap;
    let share = pool.board_share();
    json!({
        "cap": cap,
        "board_share": share,
        "board_share_set": pool.board.is_some(),
        "used": count.used,
        "free": cap.map(|c| (c as usize).saturating_sub(count.used)),
        "at_capacity": cap.is_some_and(|c| count.used >= c as usize),
        "board_used": count.board_used,
        "board_free": share.map(|s| (s as usize).saturating_sub(count.board_used)),
        "board_admitted": admits(pool, count, Start::Board),
        "total": count.total,
        "quiet": count.quiet,
        "on_limit_stop": count.limit_stops,
        "on_waits": count.waits,
        "pending": count.pending,
        "counted": "live Claude Code and Codex sessions doing work: a session quiet for 30 minutes outside a turn, on a limit stop or on a wait of its own is not counted",
        "quiet_after_minutes": crate::machine_sessions::QUIET_MINUTES,
        "observed_at": scan["observed_at"],
        "degraded": scan["degraded"],
    })
}

/// The facts for a scan: each Claude Code process's own session file, its transcript, its
/// waits and limit stops, and the board runs the app answers for. Reads nothing without rows.
pub fn facts(root: &Path, accounts: &dyn Accounts, scan: &Value, now: i64) -> Facts {
    let rows: Vec<&Value> = scan["sessions"].as_array().into_iter().flatten().collect();
    let (pending_board, pending_other) = pending_counts(&pending_read(root, now));
    if rows.is_empty() {
        return Facts {
            board: accounts.board_launches(&[]),
            pending_board,
            pending_other,
            ..Facts::default()
        };
    }
    let claude: HashSet<u32> = rows
        .iter()
        .filter(|r| r["agent"] == "claude-code")
        .filter_map(|r| r["pid"].as_u64().and_then(|p| u32::try_from(p).ok()))
        .collect();
    let mut facts = Facts::default();
    let mut since: HashMap<String, i64> = HashMap::new();
    let files = if claude.is_empty() {
        Vec::new()
    } else {
        crate::machine_sessions::session_files(accounts)
    };
    for (config, file) in files {
        let Some(pid) = file["pid"].as_u64().and_then(|p| u32::try_from(p).ok()) else {
            continue;
        };
        if !claude.contains(&pid) {
            continue;
        }
        let (Some(session), Some(cwd)) = (file["sessionId"].as_str(), file["cwd"].as_str()) else {
            continue;
        };
        let millis = |key: &str| file[key].as_i64().map(|m| m / 1000);
        let status_since = millis("statusUpdatedAt").or_else(|| millis("updatedAt"));
        let written = crate::machine_sessions::session_last_write(&config, cwd, session);
        let activity = match (written, status_since) {
            (Some(w), Some(s)) => Some(w.max(s)),
            (w, s) => w.or(s),
        };
        since.insert(session.to_owned(), status_since.unwrap_or(now));
        facts.own.insert(
            pid,
            Own {
                session: session.to_owned(),
                status: file["status"].as_str().unwrap_or("unknown").to_owned(),
                activity,
            },
        );
    }
    facts.stopped = crate::stops::live_stop_sessions(root, now);
    facts.waiting = since
        .iter()
        .filter(|(session, idle_since)| {
            crate::monitors::holds(&crate::monitors::read(root, session), now, **idle_since)
        })
        .map(|(session, _)| session.clone())
        .collect();
    let live: Vec<String> = rows
        .iter()
        .filter_map(|r| r["launch_id"].as_str().map(str::to_owned))
        .collect();
    facts.board = accounts.board_launches(&live);
    facts.pending_board = pending_board;
    facts.pending_other = pending_other;
    facts
}

/// The count and view of one scan already made, with the facts read now.
pub fn view_of(
    root: &Path,
    accounts: &dyn Accounts,
    scan: &Value,
    now: i64,
) -> (Pool, Count, Value) {
    let pool = read(root).unwrap_or_default();
    let count = count(scan, &facts(root, accounts, scan, now), now);
    let view = view(&pool, &count, scan);
    (pool, count, view)
}

/// `switchboard capacity`: the cap, the board's share, what counts against them and what is
/// free.
pub fn status(root: &Path, accounts: &dyn Accounts) -> Result<Value, String> {
    let pool = read(root)?;
    let scan = crate::machine_sessions::scan(accounts);
    let now = crate::clock::now();
    let count = count(&scan, &facts(root, accounts, &scan, now), now);
    Ok(view(&pool, &count, &scan))
}

/// SB-417 (review round 1, P1-B): starts the pool admitted and nobody has recorded as running yet,
/// `<data>/board-pending.json` (0600). The gate decides and writes its entry under one lock on
/// `board-pool.lock`, so two starts asked at once never both take the last slot; the entry goes
/// when its `Reservation` is dropped (after the start is recorded, or when it fails), and an entry
/// older than `PENDING_SECONDS` (a launcher that died) is dropped by the next read.
pub const PENDING: &str = "board-pending.json";
pub const POOL_LOCK: &str = "board-pool.lock";
pub const PENDING_SECONDS: i64 = 300;
const MAX_PENDING_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pending {
    id: String,
    at: i64,
    #[serde(default)]
    board: bool,
}

fn pending_read(root: &Path, now: i64) -> Vec<Pending> {
    if std::fs::symlink_metadata(root.join(PENDING)).is_err() {
        return Vec::new();
    }
    let mut rows: Vec<Pending> =
        crate::private_fs::read_private(&root.join(PENDING), MAX_PENDING_BYTES)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    rows.retain(|p| (0..PENDING_SECONDS).contains(&(now - p.at)));
    rows
}

fn pending_write(root: &Path, rows: &[Pending]) {
    let path = root.join(PENDING);
    if rows.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(rows) {
        let _ = crate::private_fs::private_write(&path, &bytes);
    }
}

fn pending_counts(rows: &[Pending]) -> (usize, usize) {
    let board = rows.iter().filter(|p| p.board).count();
    (board, rows.len() - board)
}

/// The pool's lock, held while a start is decided and its entry written. A lock that cannot be
/// taken still lets the start be decided, as `monitors` does.
fn pool_lock(root: &Path) -> Option<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(POOL_LOCK))
        .ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    file.lock().ok().map(|()| file)
}

/// A start the pool admitted, counted until it is dropped.
#[derive(Debug)]
pub struct Reservation {
    root: std::path::PathBuf,
    id: String,
    pub start: Start,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let _held = pool_lock(&self.root);
        let mut rows = pending_read(&self.root, crate::clock::now());
        let before = rows.len();
        rows.retain(|p| p.id != self.id);
        if rows.len() != before {
            pending_write(&self.root, &rows);
        }
    }
}

/// The gate before a start: `Ok(None)` without a cap (nothing is read; a file that cannot be read
/// is no cap, `status` names it), `Ok(Some(_))` admitted and counted until the reservation is
/// dropped, `Err(view)` refused. `dispatch` answers the view with its refusal.
pub fn reserve(
    root: &Path,
    accounts: &dyn Accounts,
    start: Start,
) -> Result<Option<Reservation>, Value> {
    reserve_with(root, start, |now| {
        let scan = crate::machine_sessions::scan(accounts);
        let facts = facts(root, accounts, &scan, now);
        (scan, facts)
    })
}

/// `reserve` with the scan and its facts read by `read`, under the pool's lock.
pub fn reserve_with(
    root: &Path,
    start: Start,
    read_pool: impl FnOnce(i64) -> (Value, Facts),
) -> Result<Option<Reservation>, Value> {
    let pool = match read(root) {
        Ok(pool) if pool.cap.is_some() => pool,
        _ => return Ok(None),
    };
    let _held = pool_lock(root);
    let now = crate::clock::now();
    let (scan, mut facts) = read_pool(now);
    // The entries read under the lock, whatever the reader saw before it was taken.
    let mut rows = pending_read(root, now);
    (facts.pending_board, facts.pending_other) = pending_counts(&rows);
    let count = count(&scan, &facts, now);
    if !admits(&pool, &count, start) {
        return Err(view(&pool, &count, &scan));
    }
    let id = uuid::Uuid::new_v4().to_string();
    rows.push(Pending {
        id: id.clone(),
        at: now,
        board: start == Start::Board,
    });
    pending_write(root, &rows);
    Ok(Some(Reservation {
        root: root.to_owned(),
        id,
        start,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(quiet: &[bool]) -> Value {
        let rows: Vec<Value> = quiet
            .iter()
            .enumerate()
            .map(|(i, q)| json!({"pid": 1000 + i, "agent": "codex", "quiet": q}))
            .collect();
        json!({"sessions": rows, "total": quiet.len(), "degraded": []})
    }
    fn pool(cap: Option<u32>, board: Option<u32>) -> Pool {
        Pool { cap, board }
    }
    fn counted(quiet: &[bool]) -> Count {
        count(&scan(quiet), &Facts::default(), 0)
    }

    #[test]
    fn without_a_cap_every_start_is_admitted_and_nothing_counts_as_full() {
        let none = pool(None, None);
        let busy = counted(&[false; 40]);
        assert!(decide(&none, &busy, Start::Interactive).is_ok());
        assert!(decide(&none, &busy, Start::Board).is_ok());
        let view = view(&none, &counted(&[false; 3]), &scan(&[false; 3]));
        assert_eq!(view["at_capacity"], false);
        assert_eq!(view["free"], Value::Null);
        assert_eq!(view["board_share"], Value::Null);
    }

    #[test]
    fn a_quiet_session_does_not_count_and_the_cap_refuses_the_start_that_would_pass_it() {
        // Two working sessions and one quiet one under a cap of 3: one slot free.
        let three = pool(Some(3), Some(0));
        let some = counted(&[false, true, false]);
        assert_eq!(used(&scan(&[false, true, false])), 2);
        assert!(decide(&three, &some, Start::Interactive).is_ok());
        assert_eq!(view(&three, &some, &Value::Null)["free"], 1);
        // A third working session fills it: the next start waits.
        let full = counted(&[false, true, false, false]);
        assert_eq!(
            decide(&three, &full, Start::Interactive).unwrap_err(),
            AT_CAPACITY
        );
        let view = view(&three, &full, &Value::Null);
        assert_eq!(view["at_capacity"], true);
        assert_eq!(view["free"], 0);
    }

    #[test]
    fn a_board_start_runs_inside_its_share_while_interactive_sessions_fill_the_pool() {
        // SB-417: the measured case of 2026-11-17: cap 20, 34 sessions working, none the board's.
        let twenty = pool(Some(20), None);
        assert_eq!(twenty.board_share(), Some(5), "a quarter of 20");
        let mut full = counted(&[false; 34]);
        assert!(decide(&twenty, &full, Start::Board).is_ok());
        assert_eq!(
            decide(&twenty, &full, Start::Interactive).unwrap_err(),
            AT_CAPACITY,
            "a hosted console a person opens still waits at the cap"
        );
        // Five board runs fill the share: the sixth waits for one of them, or for room.
        full.board_used = 5;
        assert_eq!(
            decide(&twenty, &full, Start::Board).unwrap_err(),
            AT_CAPACITY
        );
        assert_eq!(view(&twenty, &full, &Value::Null)["board_admitted"], false);
        // Room in the pool admits a board start whatever its share holds.
        let mut room = counted(&[false; 19]);
        room.board_used = 9;
        assert!(decide(&twenty, &room, Start::Board).is_ok());
        // A share of 0 the person set keeps the old rule: the cap alone.
        let mut none = counted(&[false; 20]);
        none.board_used = 0;
        assert!(decide(&pool(Some(20), Some(0)), &none, Start::Board).is_err());
    }

    #[test]
    fn the_default_board_share_is_a_quarter_of_the_cap_and_at_least_two() {
        for (cap, share) in [
            (1, 2),
            (4, 2),
            (8, 2),
            (11, 2),
            (12, 3),
            (20, 5),
            (40, 10),
            (500, 125),
        ] {
            assert_eq!(default_board_share(cap), share, "cap {cap}");
        }
        assert_eq!(pool(Some(20), Some(7)).board_share(), Some(7));
        assert_eq!(pool(None, None).board_share(), None);
    }

    #[test]
    fn an_unread_process_table_admits_the_start() {
        let mut unread = scan(&[]);
        unread["degraded"] = json!([{"source": "process_table", "reason": "ps_failed"}]);
        let c = count(&unread, &Facts::default(), 0);
        assert!(decide(&pool(Some(1), Some(0)), &c, Start::Interactive).is_ok());
    }

    #[test]
    fn each_session_counts_by_its_own_activity_waits_and_limit_stops() {
        // SB-417: five Claude Code processes in one folder that another session keeps writing
        // (each row reads `quiet: false` by its folder), and one Codex process.
        let now = 100_000;
        let row = |pid: u32, agent: &str| json!({"pid": pid, "agent": agent, "quiet": false});
        let rows = json!({"sessions": [
            row(1, "claude-code"), row(2, "claude-code"), row(3, "claude-code"),
            row(4, "claude-code"), row(5, "claude-code"), row(6, "codex"),
            row(7, "claude-code"), row(8, "claude-code"), row(9, "claude-code"),
        ], "degraded": []});
        let own = |session: &str, status: &str, minutes_ago: Option<i64>| Own {
            session: session.into(),
            status: status.into(),
            activity: minutes_ago.map(|m| now - m * 60),
        };
        let mut facts = Facts::default();
        facts.own.insert(1, own("s-idle", "idle", Some(31)));
        facts.own.insert(2, own("s-recent", "idle", Some(5)));
        facts.own.insert(3, own("s-turn", "busy", Some(90)));
        facts.own.insert(4, own("s-stop", "idle", Some(1)));
        facts.own.insert(5, own("s-wait", "idle", Some(1)));
        // Review round 1, P1-A: in a turn, a Fabric claim's hold or a monitor it armed does not
        // count it out.
        facts.own.insert(8, own("s-busy-held", "busy", Some(1)));
        facts
            .own
            .insert(9, own("s-shell-monitor", "shell", Some(45)));
        facts.stopped.insert("s-stop".into());
        facts.waiting.insert("s-wait".into());
        facts.waiting.insert("s-busy-held".into());
        facts.waiting.insert("s-shell-monitor".into());
        let kinds: Vec<Counted> = rows["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| classify(r, &facts, now))
            .collect();
        assert_eq!(
            kinds,
            [
                Counted::Quiet,
                Counted::Working,
                Counted::Working,
                Counted::LimitStop,
                Counted::Waits,
                Counted::Working,
                Counted::Working,
                Counted::Working,
                Counted::Working,
            ],
            "its own quiet, recent, in a turn, on a limit stop, on a wait, Codex by its folder, no session file by its folder, busy with a hold, in a command with a monitor"
        );
        let c = count(&rows, &facts, now);
        assert_eq!(
            (c.used, c.quiet, c.limit_stops, c.waits, c.total),
            (6, 1, 1, 1, 9)
        );
    }

    #[test]
    fn the_pool_is_kept_privately_cleared_and_refused_out_of_range() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(cap(root.path()).unwrap(), None);
        set(root.path(), Some(20)).unwrap();
        assert_eq!(cap(root.path()).unwrap(), Some(20));
        assert_eq!(
            std::fs::read_to_string(root.path().join(FILE)).unwrap(),
            r#"{"cap":20}"#,
            "without a share of its own the file is what every earlier version reads"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(set(root.path(), Some(0)).unwrap_err(), CAP_INVALID);
        assert_eq!(
            set(root.path(), Some(MAX_CAP + 1)).unwrap_err(),
            CAP_INVALID
        );
        assert_eq!(cap(root.path()).unwrap(), Some(20));
        // A share: from 0 to the cap.
        assert!(!set_pool(root.path(), None, Share::Set(6)).unwrap());
        assert_eq!(read(root.path()).unwrap(), pool(Some(20), Some(6)));
        assert_eq!(
            set_pool(root.path(), None, Share::Set(21)).unwrap_err(),
            BOARD_INVALID
        );
        set_pool(root.path(), Some(30), Share::Set(0)).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(30), Some(0)));
        // Review round 1, P2-2: a new cap keeps the share, clamped to the cap and said so.
        set_pool(root.path(), Some(30), Share::Set(10)).unwrap();
        assert!(!set_pool(root.path(), Some(12), Share::Keep).unwrap());
        assert_eq!(read(root.path()).unwrap(), pool(Some(12), Some(10)));
        assert!(
            set_pool(root.path(), Some(8), Share::Keep).unwrap(),
            "clamped"
        );
        assert_eq!(read(root.path()).unwrap(), pool(Some(8), Some(8)));
        set(root.path(), Some(9)).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(9), Some(8)));
        // Back to the default only when asked.
        set_pool(root.path(), None, Share::Default).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(9), None));
        set(root.path(), None).unwrap();
        assert_eq!(cap(root.path()).unwrap(), None);
        assert_eq!(
            set_pool(root.path(), None, Share::Set(2)).unwrap_err(),
            BOARD_INVALID,
            "no share without a cap"
        );
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert_eq!(cap(root.path()).unwrap_err(), UNREADABLE);
    }

    #[test]
    fn a_file_from_an_earlier_or_a_later_version_reads() {
        let root = tempfile::tempdir().unwrap();
        crate::private_fs::private_write(&root.path().join(FILE), br#"{"cap":20}"#).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(20), None));
        crate::private_fs::private_write(
            &root.path().join(FILE),
            br#"{"cap":20,"board":4,"later":"field"}"#,
        )
        .unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(20), Some(4)));
        // A share larger than the cap (a hand edit) reads as the default.
        crate::private_fs::private_write(&root.path().join(FILE), br#"{"cap":3,"board":9}"#)
            .unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(3), None));
    }

    #[test]
    fn a_burst_of_board_starts_takes_only_the_slots_the_share_has() {
        // Review round 1, P1-B: eight board starts asked at once while the pool is full and the
        // share has two slots: two are admitted, six refused, and a dropped reservation frees its
        // slot.
        let root = tempfile::tempdir().unwrap();
        set_pool(root.path(), Some(2), Share::Set(2)).unwrap();
        let path = root.path().to_owned();
        let full = || {
            (
                json!({"sessions": [{"pid": 1, "agent": "codex", "quiet": false}, {"pid": 2, "agent": "codex", "quiet": false}], "degraded": []}),
                Facts::default(),
            )
        };
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    reserve_with(&path, Start::Board, |_| full())
                })
            })
            .collect();
        let answers: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let admitted: Vec<Reservation> = answers
            .into_iter()
            .filter_map(|a| a.ok().flatten())
            .collect();
        assert_eq!(admitted.len(), 2, "only the share's two slots");
        let refused = reserve_with(&path, Start::Board, |_| full()).unwrap_err();
        assert_eq!(
            (refused["board_used"].clone(), refused["pending"].clone()),
            (json!(2), json!(2))
        );
        // An interactive start waits too: the pending starts count against the cap.
        assert!(reserve_with(&path, Start::Interactive, |_| full()).is_err());
        drop(admitted);
        assert!(
            !path.join(PENDING).exists(),
            "every entry gone with its reservation"
        );
        assert!(reserve_with(&path, Start::Board, |_| full())
            .unwrap()
            .is_some());
        // An entry left by a launcher that died is dropped after PENDING_SECONDS.
        let old = json!([{"id": "x", "at": crate::clock::now() - PENDING_SECONDS, "board": true},
            {"id": "y", "at": crate::clock::now() - PENDING_SECONDS, "board": true}]);
        crate::private_fs::private_write(&path.join(PENDING), old.to_string().as_bytes()).unwrap();
        assert!(reserve_with(&path, Start::Board, |_| full())
            .unwrap()
            .is_some());
        // Without a cap nothing is reserved or written.
        set(&path, None).unwrap();
        let _ = std::fs::remove_file(path.join(PENDING));
        assert!(reserve_with(&path, Start::Board, |_| full())
            .unwrap()
            .is_none());
        assert!(!path.join(PENDING).exists());
    }
}
