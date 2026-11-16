//! SB-396 wave C (LC-6, MON-1 to MON-3; operator 2026-11-16: «Лайфсайкл должен понимать, что
//! сессия находится в режиме мониторинга … чтобы не кикать сессии, которые остаются в режиме
//! мониторинга … если … непонятно, когда этот мониторинг придет, значит, надо, чтобы сессия
//! умерла»): a session that finished its turn and waits on something it set up itself.
//!
//! Claude Code writes nothing a reader outside the session can see about a pending wake-up, a
//! `Monitor` or a cron job (`~/.claude/sessions/<pid>.json` reads `idle` throughout; the lifecycle
//! audit, section 2). The plugin's `PreToolUse` hook on `ScheduleWakeup`, `Monitor`, `CronCreate`
//! and `CronDelete`, and its `SessionEnd` hook, run `switchboard monitor hook`, which keeps one
//! file per session in `<data>/monitors/` (0600: kinds, times and a count, never a prompt or a
//! command): a wake-up or a monitor with an honest end (`until`), and how many cron jobs it holds,
//! which have none.
//!
//! SB-397 D-2: moved from switchboard-runtime; a hold's event goes to the app's log (`events`).
//!
//! - A session with a wait whose end is known is never closed before that end plus `GRACE`
//!   (MON-2): the duty officer, a duplicate close and the session host all ask `holds`.
//! - A session holding a cron job (no known end) is kept for `OPEN_SECONDS` after its last turn,
//!   then may be closed like any finished session (MON-3); its close is recorded as resumable, so
//!   *Continue* brings back the same session (wave B).
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const DIR: &str = "monitors";
/// After a wait's end, how long its session is still let be (it may be taking the turn it woke for;
/// the clock may have run while the Mac slept).
pub const GRACE: i64 = 600;
/// How long a session holding a cron job is kept after its last turn (MON-3).
pub const OPEN_SECONDS: i64 = 6 * 3600;
/// `ScheduleWakeup` clamps to [60, 3600] s; `Monitor` to at most an hour.
const MAX_WAIT: i64 = 3600;
const MAX_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
// No `deny_unknown_fields`: a session host of an older version reads these files too (hosts
// outlive an update), so a field a newer version adds must never make the whole file unreadable.
#[serde(default)]
pub struct Waits {
    /// The end of the wake-up or monitor that ends last, Unix seconds.
    pub until: Option<i64>,
    /// Cron jobs created and not deleted (they have no end).
    pub crons: u32,
    /// When the hook last wrote, Unix seconds.
    pub at: i64,
    /// SB-399 (Fabric's request `bcb9ca3e`): the session holds live work another tool tracks (a
    /// Fabric task's claim) until this time; nothing closes it before. Set by `monitor hold`.
    pub hold_until: Option<i64>,
}

fn path(root: &Path, session: &str) -> Option<PathBuf> {
    crate::ids::valid_id(session).then(|| root.join(DIR).join(format!("{session}.json")))
}

pub fn read(root: &Path, session: &str) -> Waits {
    path(root, session)
        .and_then(|p| crate::private_fs::read_private(&p, MAX_BYTES).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// What one hook input changes, as a pure step: the waits after it, or None to forget the session.
pub fn apply(mut waits: Waits, hook: &Value, now: i64) -> Option<Waits> {
    let event = hook["hook_event_name"].as_str().unwrap_or("");
    if event == "SessionEnd" {
        return None;
    }
    let input = &hook["tool_input"];
    let wait = |seconds: Option<i64>| seconds.map(|s| now + s.clamp(1, MAX_WAIT));
    let until = match hook["tool_name"].as_str() {
        Some("ScheduleWakeup") => wait(input["delaySeconds"].as_i64().or(Some(MAX_WAIT))),
        Some("Monitor") => wait(Some(input["timeout_ms"].as_i64().unwrap_or(300_000) / 1000)),
        Some("CronCreate") => {
            waits.crons = waits.crons.saturating_add(1);
            None
        }
        Some("CronDelete") => {
            waits.crons = waits.crons.saturating_sub(1);
            None
        }
        _ => None,
    };
    if let Some(until) = until {
        waits.until = Some(
            waits
                .until
                .filter(|u| *u > now)
                .map_or(until, |u| u.max(until)),
        );
    }
    waits.at = now;
    Some(waits)
}

/// `switchboard monitor hook`: records what the hook input says. Never fails a tool call: a
/// session id it cannot use, or a file it cannot write, records nothing.
/// Held while a session's file is read, changed and written: a hook and a hold (two processes)
/// never lose each other's change (review of 2026-11-16, nit 8). A lock that cannot be taken
/// still lets the write happen, as `resumable` does.
fn writing(root: &Path) -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(root.join(DIR));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(DIR).join(".lock"))
        .ok()?;
    file.lock().ok().map(|()| file)
}

pub fn hook(root: &Path, hook: &Value, now: i64) {
    let Some(session) = hook["session_id"].as_str() else {
        return;
    };
    let Some(file) = path(root, session) else {
        return;
    };
    let _held = writing(root);
    match apply(read(root, session), hook, now) {
        None => {
            let _ = std::fs::remove_file(&file);
        }
        Some(waits) => {
            let _ = std::fs::create_dir_all(root.join(DIR));
            if let Ok(bytes) = serde_json::to_vec(&waits) {
                let _ = crate::private_fs::private_write(&file, &bytes);
            }
        }
    }
}

/// Whether `session` is waiting on something it set up itself, so nothing may close it now.
/// `idle_since`: when its last turn ended.
pub fn holds(waits: &Waits, now: i64, idle_since: i64) -> bool {
    waits.hold_until.is_some_and(|u| now < u)
        || waits.until.is_some_and(|u| now < u + GRACE)
        || (waits.crons > 0 && now - idle_since < OPEN_SECONDS)
}

/// The longest a hold may run: a claim is renewed, never set once for days.
pub const MAX_HOLD: i64 = 7 * 86_400;
pub const HOLD_INVALID: &str =
    "Give the session id and a hold end within seven days from now, or 0 to release it.";
pub const HOLD_NOT_SAVED: &str = "The session's hold could not be saved.";
pub const HOLD_REASON_INVALID: &str =
    "Give the hold's reason as one short word: lowercase letters, digits and underscores.";
/// The reasons the operations log names; any other reason is logged as `other` (the log keeps
/// codes only, never text a caller wrote).
const HOLD_REASONS: &[&str] = &["fabric_claim"];

/// SB-399: `switchboard monitor hold`: the session holds live work until `until` (Unix seconds),
/// or no longer with 0. `reason` (Fabric passes `fabric_claim`) is a short slug, echoed in the
/// answer and logged as a code; the waits file does not keep it, so a session host of 0.7.109,
/// whose reader refused unknown fields, still reads the hold. Answers what is recorded.
pub fn hold(
    root: &Path,
    session: &str,
    until: i64,
    now: i64,
    reason: Option<&str>,
) -> Result<Value, String> {
    let file = path(root, session).ok_or(HOLD_INVALID)?;
    if until != 0 && (until <= now || until > now + MAX_HOLD) {
        return Err(HOLD_INVALID.into());
    }
    if let Some(r) = reason {
        let mut chars = r.chars();
        let slug = r.len() <= 40
            && chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !slug {
            return Err(HOLD_REASON_INVALID.into());
        }
    }
    let _held = writing(root);
    let mut waits = read(root, session);
    waits.hold_until = (until != 0).then_some(until);
    waits.at = now;
    std::fs::create_dir_all(root.join(DIR)).map_err(|_| HOLD_NOT_SAVED)?;
    let bytes = serde_json::to_vec(&waits).map_err(|_| HOLD_NOT_SAVED)?;
    crate::private_fs::private_write(&file, &bytes).map_err(|_| HOLD_NOT_SAVED)?;
    let code = reason.map(|r| {
        HOLD_REASONS
            .iter()
            .find(|k| **k == r)
            .copied()
            .unwrap_or("other")
    });
    crate::events::event(
        "monitor_hold",
        &[
            ("reason", crate::events::Field::Code(code.unwrap_or("none"))),
            ("released", crate::events::Field::Flag(until == 0)),
        ],
    );
    Ok(serde_json::json!({"session_id": session, "hold_until": waits.hold_until, "reason": reason}))
}

/// The session's wait as a row shows it: `{until, held: true}` while another tool holds it,
/// `{until}` while a known end is ahead, `{open: true}` while a cron job holds it, none otherwise.
pub fn shown(waits: &Waits, now: i64) -> Option<Value> {
    if let Some(u) = waits.hold_until.filter(|u| now < *u) {
        return Some(serde_json::json!({"until": u, "held": true}));
    }
    if let Some(u) = waits.until.filter(|u| now < u + GRACE) {
        return Some(serde_json::json!({"until": u}));
    }
    (waits.crons > 0).then(|| serde_json::json!({"open": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_hold_takes_a_short_reason_and_a_newer_field_never_hides_the_waits() {
        let root = tempfile::tempdir().unwrap();
        let held = hold(root.path(), "S9", 500, 100, Some("fabric_claim")).unwrap();
        assert_eq!(held["reason"], "fabric_claim");
        assert_eq!(read(root.path(), "S9").hold_until, Some(500));
        for bad in ["Fabric claim", "", "x".repeat(41).as_str(), "1abc"] {
            assert_eq!(
                hold(root.path(), "S9", 500, 100, Some(bad)).unwrap_err(),
                HOLD_REASON_INVALID,
                "{bad:?}"
            );
        }
        assert_eq!(
            read(root.path(), "S9").hold_until,
            Some(500),
            "a refusal changes nothing"
        );
        // A file a newer version wrote with a field this one does not know still reads.
        let file = root.path().join(DIR).join("S8.json");
        crate::private_fs::private_write(
            &file,
            br#"{"hold_until": 700, "at": 1, "a_later_field": "x"}"#,
        )
        .unwrap();
        assert_eq!(read(root.path(), "S8").hold_until, Some(700));
    }

    fn tool(name: &str, input: Value) -> Value {
        json!({"session_id": "S1", "hook_event_name": "PreToolUse", "tool_name": name, "tool_input": input})
    }

    #[test]
    fn a_wake_up_or_a_monitor_holds_its_session_until_its_end_and_a_little_after() {
        let w = apply(
            Waits::default(),
            &tool("ScheduleWakeup", json!({"delaySeconds": 1800})),
            1000,
        )
        .unwrap();
        assert_eq!(w.until, Some(2800));
        assert!(holds(&w, 2800 + GRACE - 1, 900));
        assert!(
            !holds(&w, 2800 + GRACE, 900),
            "past its end and the grace it is a finished session again"
        );
        // A wake-up past the clamp is an hour; a monitor counts its own timeout; the later end wins.
        let w = apply(
            w,
            &tool("ScheduleWakeup", json!({"delaySeconds": 99_999})),
            1000,
        )
        .unwrap();
        assert_eq!(w.until, Some(1000 + MAX_WAIT));
        let w = apply(w, &tool("Monitor", json!({"timeout_ms": 600_000})), 1000).unwrap();
        assert_eq!(
            w.until,
            Some(1000 + MAX_WAIT),
            "an earlier end does not shorten the wait"
        );
        assert_eq!(shown(&w, 1000), Some(json!({"until": 1000 + MAX_WAIT})));
    }

    #[test]
    fn a_cron_job_holds_its_session_for_six_hours_after_its_last_turn_then_lets_it_go() {
        let w = apply(
            Waits::default(),
            &tool("CronCreate", json!({"cron": "*/5 * * * *"})),
            100,
        )
        .unwrap();
        assert_eq!(w.crons, 1);
        assert!(holds(&w, 100 + OPEN_SECONDS - 1, 100));
        assert!(
            !holds(&w, 100 + OPEN_SECONDS, 100),
            "an open-ended wait does not hold forever"
        );
        assert_eq!(shown(&w, 200), Some(json!({"open": true})));
        let w = apply(w, &tool("CronDelete", json!({"id": "c1"})), 300).unwrap();
        assert_eq!(w.crons, 0);
        assert!(!holds(&w, 400, 300));
        assert!(apply(
            w,
            &json!({"session_id": "S1", "hook_event_name": "SessionEnd"}),
            500
        )
        .is_none());
    }

    #[test]
    fn a_hold_keeps_its_session_until_its_end_and_is_released_with_zero() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            hold(root.path(), "S1", 1000 + 1800, 1000, None).unwrap()["hold_until"],
            2800
        );
        let w = read(root.path(), "S1");
        assert!(holds(&w, 2799, 0), "held however long the session was idle");
        assert!(!holds(&w, 2800, 0));
        assert_eq!(shown(&w, 2000), Some(json!({"until": 2800, "held": true})));
        hold(root.path(), "S1", 0, 1100, None).unwrap();
        assert!(!holds(&read(root.path(), "S1"), 1200, 0));
        // An end in the past, past seven days, or a session id that is not one: refused.
        assert_eq!(
            hold(root.path(), "S1", 900, 1000, None),
            Err(HOLD_INVALID.into())
        );
        assert_eq!(
            hold(root.path(), "S1", 1000 + MAX_HOLD + 1, 1000, None),
            Err(HOLD_INVALID.into())
        );
        assert_eq!(
            hold(root.path(), "../x", 2000, 1000, None),
            Err(HOLD_INVALID.into())
        );
        // A hook that comes after keeps the hold.
        hold(root.path(), "S1", 5000, 1000, None).unwrap();
        hook(root.path(), &tool("CronCreate", json!({})), 1001);
        let w = read(root.path(), "S1");
        assert_eq!((w.hold_until, w.crons), (Some(5000), 1));
    }

    #[test]
    fn the_hook_writes_one_private_file_per_session_and_forgets_it_at_the_end() {
        let root = tempfile::tempdir().unwrap();
        hook(
            root.path(),
            &tool("ScheduleWakeup", json!({"delaySeconds": 600})),
            1000,
        );
        assert_eq!(read(root.path(), "S1").until, Some(1600));
        let file = root.path().join(DIR).join("S1.json");
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(&file).unwrap().permissions()
            ) & 0o777,
            0o600
        );
        hook(
            root.path(),
            &json!({"session_id": "S1", "hook_event_name": "SessionEnd"}),
            1100,
        );
        assert!(!file.exists());
        // A session id that is not one writes nothing.
        hook(
            root.path(),
            &json!({"session_id": "../x", "hook_event_name": "PreToolUse", "tool_name": "ScheduleWakeup", "tool_input": {}}),
            1,
        );
        let files: Vec<_> = std::fs::read_dir(root.path().join(DIR))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != ".lock")
            .collect();
        assert!(files.is_empty(), "{files:?}");
    }
}
