//! SB-508 (plan 10 §B): every live Claude Code session this computer saw, so any of them can be
//! continued as itself after a reboot or a crash.
//!
//! A session the person started in a terminal or an editor has no resumable line, launch record
//! or limit stop (`resumable::locate`), so before this ledger nothing said where to resume it. The
//! one-minute session watch (`session_watch::tick`) now keeps `<data>/live-sessions.json` (0600):
//! for each session id seen live, its Claude Code config folder, its working folder, the terminal
//! host it ran under, its last process and when it was first and last seen. A session's
//! `last_seen` is refreshed at most once an hour (`REFRESH_SECONDS`), so the file is written only
//! when a session appears, changes process or folder, an hour passes, or an entry ages out: an
//! entry is kept `resumable::WINDOW_SECONDS` (7 days) after it was last seen, at most `KEEP` of
//! them. Ids and paths only: never a transcript line, a prompt or a credential.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const FILE: &str = "live-sessions.json";
pub const VERSION: u32 = 1;
/// Sessions kept, the most recently seen first.
pub const KEEP: usize = 500;
/// How often a live session's `last_seen` is written again.
pub const REFRESH_SECONDS: i64 = 3600;
/// A ledger larger than this is not read (500 entries take about 150 KiB).
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The Claude Code config folder holding the session's file and transcript.
    pub config: String,
    /// The folder the session works in.
    pub cwd: String,
    /// The terminal host it ran under (`duty::host_of`: terminal, iterm, orca, dashboards, other).
    pub host: String,
    /// The last process seen holding it.
    pub pid: u32,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub version: u32,
    /// By session id.
    pub sessions: BTreeMap<String, Entry>,
}

/// One live session as its file and the process table show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    pub session: String,
    pub config: PathBuf,
    pub cwd: String,
    pub pid: u32,
}

/// The sessions whose file (`sessions/<pid>.json` in `config`) names a live Claude Code process.
/// A file naming a dead pid, an id that is not a session id or a folder that is not absolute is
/// left out. Pure, for tests.
pub fn observed(files: &[(PathBuf, Value)], alive: &HashSet<u32>) -> Vec<Observed> {
    files
        .iter()
        .filter_map(|(config, file)| {
            let pid = u32::try_from(file["pid"].as_u64()?).ok()?;
            let session = file["sessionId"].as_str()?;
            let cwd = file["cwd"].as_str()?;
            (alive.contains(&pid)
                && crate::launches::valid_id(session)
                && Path::new(cwd).is_absolute())
            .then(|| Observed {
                session: session.to_owned(),
                config: config.clone(),
                cwd: cwd.to_owned(),
                pid,
            })
        })
        .collect()
}

/// The ledger after this look, or `None` when nothing needs writing. `host` is asked only for a
/// session new to the ledger or on a new process. Pure, for tests.
pub fn merge(
    previous: &Ledger,
    seen: &[Observed],
    host: impl Fn(u32) -> String,
    now: i64,
) -> Option<Ledger> {
    let mut next = Ledger {
        version: VERSION,
        sessions: previous.sessions.clone(),
    };
    // One session in two processes (a duplicate) is one entry: the process the ledger already
    // names when it is still one of them, else the lowest pid, so it is not rewritten each minute.
    let mut one: BTreeMap<&str, &Observed> = BTreeMap::new();
    for s in seen {
        let kept_pid = previous.sessions.get(&s.session).map(|e| e.pid);
        one.entry(s.session.as_str())
            .and_modify(|chosen| {
                if kept_pid != Some(chosen.pid) && (kept_pid == Some(s.pid) || s.pid < chosen.pid) {
                    *chosen = s;
                }
            })
            .or_insert(s);
    }
    // `last_seen` of every live session is refreshed together, once any is an hour old (or in
    // the future after a clock set back), so the refresh is one write an hour, not one a session.
    let refresh = one.values().any(|s| {
        previous
            .sessions
            .get(&s.session)
            .is_some_and(|e| now - e.last_seen >= REFRESH_SECONDS || e.last_seen > now)
    });
    for s in one.into_values() {
        let config = s.config.to_string_lossy().into_owned();
        let entry = match previous.sessions.get(&s.session) {
            Some(old) if old.pid == s.pid && old.config == config && old.cwd == s.cwd => {
                let mut kept = old.clone();
                if refresh {
                    kept.last_seen = now;
                }
                kept
            }
            Some(old) => Entry {
                config,
                cwd: s.cwd.clone(),
                host: if old.pid == s.pid {
                    old.host.clone()
                } else {
                    host(s.pid)
                },
                pid: s.pid,
                first_seen: old.first_seen.min(now),
                last_seen: now,
            },
            None => Entry {
                config,
                cwd: s.cwd.clone(),
                host: host(s.pid),
                pid: s.pid,
                first_seen: now,
                last_seen: now,
            },
        };
        next.sessions.insert(s.session.clone(), entry);
    }
    let since = now - crate::resumable::WINDOW_SECONDS;
    next.sessions.retain(|_, e| e.last_seen >= since);
    if next.sessions.len() > KEEP {
        let mut by_age: Vec<(i64, String)> = next
            .sessions
            .iter()
            .map(|(id, e)| (e.last_seen, id.clone()))
            .collect();
        by_age.sort();
        by_age.reverse();
        let keep: HashSet<String> = by_age.into_iter().take(KEEP).map(|(_, id)| id).collect();
        next.sessions.retain(|id, _| keep.contains(id));
    }
    (next != *previous).then_some(next)
}

/// The kept ledger; an absent, unreadable or foreign one reads as empty.
pub fn read(root: &Path) -> Ledger {
    switchboard_core::private_fs::read_private(&root.join(FILE), MAX_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Ledger>(&bytes).ok())
        .filter(|l| l.version == VERSION)
        .unwrap_or_default()
}

fn write(root: &Path, ledger: &Ledger) {
    if let Ok(bytes) = serde_json::to_vec(ledger) {
        let _ = switchboard_core::private_fs::private_write(&root.join(FILE), &bytes);
    }
}

/// The watch's step: the live sessions of this look (`alive`: the live Claude Code pids the
/// watch's scan read) merged into the ledger, written only when it changed. With the person's
/// own sign-in off limits (SB-403), only sessions in Switchboard's own homes are kept, as the
/// duty officer keeps only those.
pub fn observe(root: &Path, alive: &HashSet<u32>, now: i64) {
    let mut seen = observed(&crate::machine_sessions::session_files(root), alive);
    if crate::own_sign_in::is_off(root) {
        seen.retain(|s| s.config.starts_with(root));
    }
    let previous = read(root);
    let host =
        |pid: u32| crate::duty::host_of(&crate::machine_sessions::ancestor_paths(pid)).to_owned();
    if let Some(next) = merge(&previous, &seen, host, now) {
        write(root, &next);
    }
}

/// SB-508: how long after its last sighting the Sessions page lists a session that ended.
pub const ENDED_LISTED_SECONDS: i64 = 86_400;

/// SB-508: the ledger's sessions seen within `ENDED_LISTED_SECONDS`, as the Sessions page's
/// resumable lines (`reason: seen_live`), newest first. The rows drop every session still live
/// (`last_seen` is refreshed only hourly) and every one already listed, so only the ended stay.
pub fn ended_rows(root: &Path, now: i64) -> Vec<Value> {
    let ledger = read(root);
    let mut rows: Vec<(i64, Value)> = ledger
        .sessions
        .iter()
        .filter(|(_, e)| {
            let age = now - e.last_seen;
            (0..=ENDED_LISTED_SECONDS).contains(&age)
        })
        .map(|(id, e)| {
            (
                e.last_seen,
                serde_json::json!({"session": id, "config": e.config, "cwd": e.cwd,
                    "reason": "seen_live", "at": e.last_seen, "by": "ledger"}),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter().map(|(_, v)| v).collect()
}

/// Where `session` can be resumed from (its config folder and working folder), when it was seen
/// live in the last `resumable::WINDOW_SECONDS`.
pub fn locate(root: &Path, session: &str, now: i64) -> Option<(PathBuf, String)> {
    let ledger = read(root);
    let entry = ledger.sessions.get(session)?;
    (entry.last_seen >= now - crate::resumable::WINDOW_SECONDS)
        .then(|| (PathBuf::from(&entry.config), entry.cwd.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const S1: &str = "11111111-1111-4111-8111-111111111111";
    const S2: &str = "22222222-2222-4222-8222-222222222222";

    fn file(session: &str, pid: u32, cwd: &str) -> Value {
        json!({"pid": pid, "sessionId": session, "cwd": cwd, "status": "busy"})
    }

    fn seen(session: &str, pid: u32, cwd: &str) -> Observed {
        Observed {
            session: session.into(),
            config: PathBuf::from("/h/.claude"),
            cwd: cwd.into(),
            pid,
        }
    }

    #[test]
    fn only_a_live_process_with_a_session_id_and_an_absolute_folder_is_observed() {
        let config = PathBuf::from("/h/.claude");
        // An absolute folder on this platform (a drive letter on Windows).
        let work = std::env::temp_dir().join("work");
        let folder = |name: &str| work.join(name).to_string_lossy().into_owned();
        let files = vec![
            (config.clone(), file(S1, 10, &folder("a"))),
            (config.clone(), file(S2, 11, &folder("b"))),
            (config.clone(), file("../escape", 12, &folder("c"))),
            (config.clone(), file(S2, 13, "relative")),
            (config.clone(), json!({"pid": 14, "cwd": folder("d")})),
        ];
        let alive: HashSet<u32> = [10, 12, 13, 14].into_iter().collect();
        assert_eq!(observed(&files, &alive), vec![seen(S1, 10, &folder("a"))]);
    }

    #[test]
    fn a_session_is_written_when_it_appears_and_again_only_when_it_changes_or_an_hour_passes() {
        let asked = std::cell::Cell::new(0);
        let host = |_: u32| {
            asked.set(asked.get() + 1);
            "terminal".to_owned()
        };
        let first = merge(&Ledger::default(), &[seen(S1, 10, "/work/a")], host, 1000).unwrap();
        let entry = &first.sessions[S1];
        assert_eq!(
            (
                entry.host.as_str(),
                entry.first_seen,
                entry.last_seen,
                entry.pid
            ),
            ("terminal", 1000, 1000, 10)
        );
        assert_eq!(asked.get(), 1);
        // The next minutes change nothing: no write, and the host is not asked again.
        assert!(merge(&first, &[seen(S1, 10, "/work/a")], host, 1060).is_none());
        assert!(
            merge(&first, &[], host, 2000).is_none(),
            "gone: kept as it was"
        );
        assert_eq!(asked.get(), 1);
        // An hour on, `last_seen` is written again.
        let later = merge(&first, &[seen(S1, 10, "/work/a")], host, 1000 + 3600).unwrap();
        assert_eq!(later.sessions[S1].last_seen, 4600);
        assert_eq!(later.sessions[S1].first_seen, 1000);
        // Resumed in another process: the new pid and its host, the first sighting kept.
        let moved = merge(&first, &[seen(S1, 20, "/work/a")], host, 1200).unwrap();
        assert_eq!(
            (moved.sessions[S1].pid, moved.sessions[S1].first_seen),
            (20, 1000)
        );
        assert_eq!(asked.get(), 2);
        // A clock set back never leaves `last_seen` in the future.
        let back = merge(&first, &[seen(S1, 10, "/work/a")], host, 900).unwrap();
        assert_eq!(back.sessions[S1].last_seen, 900);
        // One session in two processes keeps the process the ledger names: no write each minute.
        let twice = [seen(S1, 9, "/work/a"), seen(S1, 10, "/work/a")];
        assert!(merge(&first, &twice, host, 1120).is_none());
        let fresh = merge(&Ledger::default(), &twice, host, 1000).unwrap();
        assert_eq!(fresh.sessions[S1].pid, 9, "else the lowest pid");
        // Two sessions refresh together once one of them is an hour old: one write, not two.
        let both = merge(&first, &[seen(S2, 20, "/work/b")], host, 2000).unwrap();
        let live = [seen(S1, 10, "/work/a"), seen(S2, 20, "/work/b")];
        let hour = merge(&both, &live, host, 1000 + 3600).unwrap();
        assert_eq!(
            (hour.sessions[S1].last_seen, hour.sessions[S2].last_seen),
            (4600, 4600)
        );
        assert!(merge(&hour, &live, host, 2000 + 3600).is_none());
    }

    #[test]
    fn an_entry_ages_out_after_a_week_and_the_ledger_keeps_the_newest_five_hundred() {
        let host = |_: u32| "other".to_owned();
        let first = merge(&Ledger::default(), &[seen(S1, 10, "/w")], host, 0).unwrap();
        let week = crate::resumable::WINDOW_SECONDS;
        assert!(merge(&first, &[], host, week).is_none(), "still within");
        assert!(merge(&first, &[], host, week + 1)
            .unwrap()
            .sessions
            .is_empty());
        let mut big = Ledger {
            version: VERSION,
            ..Ledger::default()
        };
        for n in 0..(KEEP as i64 + 10) {
            big.sessions.insert(
                format!("s-{n:04}"),
                Entry {
                    config: "/h".into(),
                    cwd: "/w".into(),
                    host: "other".into(),
                    pid: 1,
                    first_seen: n,
                    last_seen: n,
                },
            );
        }
        let kept = merge(&big, &[], host, 1000).unwrap();
        assert_eq!(kept.sessions.len(), KEEP);
        assert!(!kept.sessions.contains_key("s-0000"), "the oldest goes");
        assert!(kept.sessions.contains_key("s-0509"));
    }

    #[test]
    fn the_ledger_is_private_and_locates_a_session_seen_in_the_last_week() {
        let root = tempfile::tempdir().unwrap();
        let ledger = merge(
            &Ledger::default(),
            &[seen(S1, 10, "/work/a")],
            |_| "iterm".to_owned(),
            1000,
        )
        .unwrap();
        write(root.path(), &ledger);
        assert_eq!(read(root.path()), ledger);
        assert_eq!(
            locate(root.path(), S1, 2000),
            Some((PathBuf::from("/h/.claude"), "/work/a".to_owned()))
        );
        assert_eq!(locate(root.path(), S2, 2000), None);
        assert_eq!(
            locate(root.path(), S1, 1000 + crate::resumable::WINDOW_SECONDS + 1),
            None
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.path().join(FILE))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
        // No transcript text and no credential: ids, paths, a host, a pid and times only.
        let text = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let mut keys: Vec<&str> = v["sessions"][S1]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["config", "cwd", "first_seen", "host", "last_seen", "pid"]
        );
        // The Sessions page's lines: seen within a day, newest first.
        let rows = ended_rows(root.path(), 2000);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (
                rows[0]["session"].clone(),
                rows[0]["reason"].clone(),
                rows[0]["cwd"].clone()
            ),
            (json!(S1), json!("seen_live"), json!("/work/a"))
        );
        assert!(ended_rows(root.path(), 1000 + ENDED_LISTED_SECONDS + 1).is_empty());
        // A broken file reads as empty and is replaced by the next write.
        std::fs::write(root.path().join(FILE), b"{").unwrap();
        assert_eq!(read(root.path()), Ledger::default());
    }
}
