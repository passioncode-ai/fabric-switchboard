//! SB-378 (operator 2026-11-14, relayed by Fabric on the board, request `52f5a962`): one cap on
//! the coding sessions this computer runs at once. `<data>/capacity.json` (0600) holds the cap
//! the person set (`switchboard capacity set N`); absent, there is no cap. A session counts while
//! it is a live Claude Code or Codex process of this user (`machine_sessions::scan`, managed or
//! not) whose folder's transcripts were written within `machine_sessions::QUIET_MINUTES`: a quiet
//! session holds a terminal and almost no load. `switchboard dispatch` and `launch --hosted`
//! refuse a start at the cap with `AT_CAPACITY`, so Fabric's board, Fabric Dashboards' console
//! and the CLI share one gate; a launcher takes the refusal as a wait and asks again.
//!
//! SB-397 D-2: moved from switchboard-runtime; `root` is the data folder of the app that keeps the
//! cap, and the sessions counted are the scan's, with that app's homes (`Accounts`).
use crate::accounts::Accounts;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub const FILE: &str = "capacity.json";
const MAX_BYTES: u64 = 4096;
/// The highest cap a person can set.
pub const MAX_CAP: u32 = 500;

/// `launch --hosted`'s refusal at the cap; `dispatch` answers `dispatch::AT_CAPACITY` instead.
pub const AT_CAPACITY: &str = "At capacity: this computer runs as many coding sessions as its limit allows. The start waits until one ends.";
pub const CAP_INVALID: &str = "Give the session limit as a whole number from 1 to 500.";
pub const UNREADABLE: &str =
    "The session limit (capacity.json) could not be read. Set it again, or clear it.";
pub const NOT_SAVED: &str = "The session limit could not be saved.";

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    cap: Option<u32>,
}

/// The cap the person set; none when no file holds one.
pub fn cap(root: &Path) -> Result<Option<u32>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes = crate::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    let saved: Saved = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    Ok(saved.cap.filter(|c| (1..=MAX_CAP).contains(c)))
}

/// Sets the cap, or clears it with `None`.
pub fn set(root: &Path, cap: Option<u32>) -> Result<(), String> {
    if cap.is_some_and(|c| !(1..=MAX_CAP).contains(&c)) {
        return Err(CAP_INVALID.into());
    }
    let bytes = serde_json::to_vec(&Saved { cap }).map_err(|_| NOT_SAVED)?;
    crate::private_fs::private_write(&root.join(FILE), &bytes).map_err(|_| NOT_SAVED.into())
}

/// The sessions a scan counts against the cap: those not quiet.
pub fn used(scan: &Value) -> usize {
    scan["sessions"]
        .as_array()
        .map(|rows| rows.iter().filter(|r| r["quiet"] != true).count())
        .unwrap_or(0)
}

/// The pool as a launcher reads it, from a scan already made.
pub fn view(cap: Option<u32>, scan: &Value) -> Value {
    let used = used(scan);
    json!({
        "cap": cap,
        "used": used,
        "free": cap.map(|c| (c as usize).saturating_sub(used)),
        "at_capacity": cap.is_some_and(|c| used >= c as usize),
        "total": scan["total"],
        "quiet": scan["quiet"],
        "counted": "live Claude Code and Codex processes whose folder was written within the last 30 minutes",
        "quiet_after_minutes": crate::machine_sessions::QUIET_MINUTES,
        "observed_at": scan["observed_at"],
        "degraded": scan["degraded"],
    })
}

/// `switchboard capacity`: the cap, what counts against it and what is free.
pub fn status(root: &Path, accounts: &dyn Accounts) -> Result<Value, String> {
    let cap = cap(root)?;
    Ok(view(cap, &crate::machine_sessions::scan(accounts)))
}

/// Whether one more session may start. A scan whose process table could not be read admits
/// the start: an unread table is not a full pool, and the refusal would stop every launcher.
pub fn decide(cap: Option<u32>, scan: &Value) -> Result<(), String> {
    let Some(cap) = cap else {
        return Ok(());
    };
    let unread = scan["degraded"].as_array().is_some_and(|d| !d.is_empty());
    if !unread && used(scan) >= cap as usize {
        return Err(AT_CAPACITY.into());
    }
    Ok(())
}

/// The gate `launch --hosted` calls before a start. Without a cap nothing is read.
pub fn admit(root: &Path, accounts: &dyn Accounts) -> Result<(), String> {
    match full(root, accounts) {
        Some(_) => Err(AT_CAPACITY.into()),
        None => Ok(()),
    }
}

/// The pool's view when it is full, else none; `dispatch` answers it with its refusal. Without
/// a cap nothing is read; a file that cannot be read is no cap (`status` names it).
pub fn full(root: &Path, accounts: &dyn Accounts) -> Option<Value> {
    let cap = cap(root).ok().flatten()?;
    let scan = crate::machine_sessions::scan(accounts);
    decide(Some(cap), &scan)
        .err()
        .map(|_| view(Some(cap), &scan))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(quiet: &[bool]) -> Value {
        let rows: Vec<Value> = quiet.iter().map(|q| json!({"quiet": q})).collect();
        json!({"sessions": rows, "total": quiet.len(), "degraded": []})
    }

    #[test]
    fn without_a_cap_every_start_is_admitted_and_nothing_counts_as_full() {
        assert!(decide(None, &scan(&[false; 40])).is_ok());
        assert_eq!(view(None, &scan(&[false; 3]))["at_capacity"], false);
        assert_eq!(view(None, &scan(&[false; 3]))["free"], Value::Null);
    }

    #[test]
    fn a_quiet_session_does_not_count_and_the_cap_refuses_the_start_that_would_pass_it() {
        // Two working sessions and one quiet one under a cap of 3: one slot free.
        let pool = scan(&[false, true, false]);
        assert_eq!(used(&pool), 2);
        assert!(decide(Some(3), &pool).is_ok());
        assert_eq!(view(Some(3), &pool)["free"], 1);
        // A third working session fills it: the next start waits.
        let full = scan(&[false, true, false, false]);
        assert_eq!(decide(Some(3), &full).unwrap_err(), AT_CAPACITY);
        assert_eq!(view(Some(3), &full)["at_capacity"], true);
        assert_eq!(view(Some(3), &full)["free"], 0);
    }

    #[test]
    fn an_unread_process_table_admits_the_start() {
        let mut unread = scan(&[]);
        unread["degraded"] = json!([{"source": "process_table", "reason": "ps_failed"}]);
        assert!(decide(Some(1), &unread).is_ok());
    }

    #[test]
    fn the_cap_is_kept_privately_cleared_and_refused_out_of_range() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(cap(root.path()).unwrap(), None);
        set(root.path(), Some(20)).unwrap();
        assert_eq!(cap(root.path()).unwrap(), Some(20));
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
        set(root.path(), None).unwrap();
        assert_eq!(cap(root.path()).unwrap(), None);
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert_eq!(cap(root.path()).unwrap_err(), UNREADABLE);
    }
}
