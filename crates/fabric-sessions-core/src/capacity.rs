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

/// Sets the cap with the default board share, or clears the pool with `None`. The file is then
/// `{"cap":N}`, as every earlier version wrote and reads it.
pub fn set(root: &Path, cap: Option<u32>) -> Result<(), String> {
    save(root, Pool { cap, board: None })
}

/// SB-417: saves the pool with a board share: `board` from 0 to the cap. `cap: None` keeps the
/// cap already set.
pub fn set_board(root: &Path, cap: Option<u32>, board: u32) -> Result<(), String> {
    let cap = match cap {
        Some(cap) => Some(cap),
        None => read(root)?.cap,
    };
    if cap.is_some_and(|c| !(1..=MAX_CAP).contains(&c)) {
        return Err(CAP_INVALID.into());
    }
    if !cap.is_some_and(|c| board <= c) {
        return Err(BOARD_INVALID.into());
    }
    save(
        root,
        Pool {
            cap,
            board: Some(board),
        },
    )
}

fn save(root: &Path, pool: Pool) -> Result<(), String> {
    if pool.cap.is_some_and(|c| !(1..=MAX_CAP).contains(&c)) {
        return Err(CAP_INVALID.into());
    }
    if pool.board.is_some_and(|b| !pool.cap.is_some_and(|c| b <= c)) {
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
    if facts.stopped.contains(&own.session) {
        return Counted::LimitStop;
    }
    if facts.waiting.contains(&own.session) {
        return Counted::Waits;
    }
    // In a turn (the model or a command of its own runs): working, however long it is silent.
    if matches!(own.status.as_str(), "busy" | "shell") {
        return Counted::Working;
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
    pub total: usize,
    /// Whether the process table was read; an unread table admits every start.
    pub read: bool,
}

/// Counts a scan with its facts.
pub fn count(scan: &Value, facts: &Facts, now: i64) -> Count {
    let mut c = Count {
        board_used: facts.board.len(),
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
    if rows.is_empty() {
        return Facts {
            board: accounts.board_launches(&[]),
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
    facts
}

/// The count and view of one scan already made, with the facts read now.
pub fn view_of(root: &Path, accounts: &dyn Accounts, scan: &Value, now: i64) -> (Pool, Count, Value) {
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

/// The gate `launch --hosted` calls before a start. Without a cap nothing is read.
pub fn admit(root: &Path, accounts: &dyn Accounts, start: Start) -> Result<(), String> {
    match full(root, accounts, start) {
        Some(_) => Err(AT_CAPACITY.into()),
        None => Ok(()),
    }
}

/// The pool's view when it refuses `start`, else none; `dispatch` answers it with its refusal.
/// Without a cap nothing is read; a file that cannot be read is no cap (`status` names it).
pub fn full(root: &Path, accounts: &dyn Accounts, start: Start) -> Option<Value> {
    let pool = read(root).ok()?;
    pool.cap?;
    let scan = crate::machine_sessions::scan(accounts);
    let now = crate::clock::now();
    let count = count(&scan, &facts(root, accounts, &scan, now), now);
    (!admits(&pool, &count, start)).then(|| view(&pool, &count, &scan))
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
        for (cap, share) in [(1, 2), (4, 2), (8, 2), (11, 2), (12, 3), (20, 5), (40, 10), (500, 125)] {
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
            row(7, "claude-code"),
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
        facts.stopped.insert("s-stop".into());
        facts.waiting.insert("s-wait".into());
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
            ],
            "its own quiet, recent, in a turn, on a limit stop, on a wait, Codex by its folder, no session file by its folder"
        );
        let c = count(&rows, &facts, now);
        assert_eq!(
            (c.used, c.quiet, c.limit_stops, c.waits, c.total),
            (4, 1, 1, 1, 7)
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
        set_board(root.path(), None, 6).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(20), Some(6)));
        assert_eq!(
            set_board(root.path(), None, 21).unwrap_err(),
            BOARD_INVALID
        );
        set_board(root.path(), Some(30), 0).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(30), Some(0)));
        // A cap set alone goes back to the default share.
        set_board(root.path(), Some(30), 10).unwrap();
        set(root.path(), Some(12)).unwrap();
        assert_eq!(read(root.path()).unwrap(), pool(Some(12), None));
        set(root.path(), None).unwrap();
        assert_eq!(cap(root.path()).unwrap(), None);
        assert_eq!(
            set_board(root.path(), None, 2).unwrap_err(),
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
}
