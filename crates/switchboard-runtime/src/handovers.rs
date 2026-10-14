//! Hand-overs for another app's console (SB-108, contract FD-62 with Fabric Dashboards). With
//! "Open hand-overs in Fabric" on, the automatic fallback offers the workflow and prepares the
//! continuing session as `switchboard continue --in-place` would, but starts nothing: it records a
//! pending hand-over here. Fabric polls `switchboard handovers --json` and runs
//! `switchboard handovers run <id>` on a console PTY; a hand-over runs once (taking it exclusively
//! creates a permanent receipt before reading it). One nobody takes within `CONSOLE_WAIT` seconds is opened in
//! Terminal by the background, as before, so a closed Fabric never strands an offer.
//!
//! Records live in `<data>/handovers/<id>.json` (0600): ids, a label, a folder and a script path —
//! never a token or the prompt text. The setting is `<data>/fabric-handovers` (absent = off).
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const DIR: &str = "handovers";
const SETTING: &str = "fabric-handovers";
/// How long a console has to take a hand-over before Terminal opens it.
pub(crate) const CONSOLE_WAIT: i64 = 120;
/// At most this many hand-overs wait at once; an older one is opened in Terminal first.
const MAX_WAITING: usize = 16;
const MAX_RECORD: u64 = 16 * 1024;

pub const NOT_FOUND: &str = "That hand-over is not waiting any more: another console or Terminal took it, or its offer lapsed.";
pub const OFF: &str = "Open hand-overs in Fabric is off. Turn it on with switchboard handovers set on, or continue without --to-console.";
pub const BAD_ID: &str = "Not a hand-over id: it looks like ho- and 12 hex digits.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handover {
    pub id: String,
    pub workflow: String,
    /// The catalog id of the agent that continues it (`claude-code`, `codex`, `kimi-code`,
    /// `hermes`).
    pub agent: String,
    pub account_label: String,
    /// The workflow's checkout the session runs in.
    pub dir: PathBuf,
    /// The prepared session script, inside Switchboard's private data.
    pub script: PathBuf,
    /// The engine's offer deadline (RFC 3339), as it answered.
    pub offered_until: String,
    pub created_at: i64,
}

pub fn id_valid(id: &str) -> bool {
    id.len() == 15
        && id.starts_with("ho-")
        && id[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn new_id() -> String {
    format!("ho-{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

pub fn enabled(root: &Path) -> bool {
    fs::symlink_metadata(root.join(SETTING)).is_ok_and(|m| m.file_type().is_file())
}

pub fn set_enabled(root: &Path, on: bool) -> Result<(), String> {
    let path = root.join(SETTING);
    if on {
        switchboard_core::private_fs::private_write(&path, b"on")
    } else {
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Switchboard could not change the hand-over setting.".into()),
        }
    }
}

fn dir(root: &Path) -> PathBuf {
    root.join(DIR)
}

/// Records a hand-over a console may take.
pub(crate) fn record(root: &Path, handover: &Handover) -> Result<(), String> {
    if !id_valid(&handover.id) {
        return Err(BAD_ID.into());
    }
    let folder = dir(root);
    crate::launch::private_dir(&folder)?;
    if fs::symlink_metadata(folder.join(format!("{}.taken", handover.id))).is_ok() {
        return Err(NOT_FOUND.into());
    }
    // Crash receipts count too: never grow an unbounded directory of ambiguous takes.
    if fs::read_dir(&folder)
        .map_err(|_| NOT_FOUND)?
        .take(256)
        .count()
        >= 256
    {
        return Err("Switchboard could not record the hand-over.".into());
    }
    let bytes = serde_json::to_vec(handover)
        .map_err(|_| "Switchboard could not record the hand-over.".to_string())?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err("Switchboard could not record the hand-over.".into());
    }
    switchboard_core::private_fs::private_write(
        &folder.join(format!("{}.json", handover.id)),
        &bytes,
    )
}

fn read(path: &Path) -> Option<Handover> {
    let bytes = switchboard_core::private_fs::read_private(path, MAX_RECORD).ok()?;
    serde_json::from_slice::<Handover>(&bytes)
        .ok()
        .filter(|h| id_valid(&h.id) && path.file_stem().and_then(|s| s.to_str()) == Some(&h.id))
}

/// The hand-overs waiting for a console, oldest first. A record that cannot be read is left for
/// `sweep` to remove.
pub fn pending(root: &Path) -> Vec<Handover> {
    let Ok(entries) = fs::read_dir(dir(root)) else {
        return Vec::new();
    };
    let mut found: Vec<Handover> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| fs::symlink_metadata(e.path().with_extension("taken")).is_err())
        .filter_map(|e| read(&e.path()))
        .collect();
    found.sort_by_key(|h| (h.created_at, h.id.clone()));
    found
}

/// Takes a hand-over once, retaining an exclusive receipt even after a crash.
pub fn take(root: &Path, id: &str) -> Result<Handover, String> {
    take_at(root, id, crate::monitor::now())
}

fn take_at(root: &Path, id: &str, now: i64) -> Result<Handover, String> {
    if !id_valid(id) {
        return Err(BAD_ID.into());
    }
    let folder = dir(root);
    let from = folder.join(format!("{id}.json"));
    let to = folder.join(format!("{id}.taken"));
    if !fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_file()) {
        return Err(NOT_FOUND.into());
    }
    // Windows can report success for concurrent renames of the same open source.
    // CREATE_NEW/O_EXCL chooses one taker; never unlink this deduplication receipt.
    let receipt =
        switchboard_core::private_fs::create_new(&to).map_err(|_| NOT_FOUND.to_string())?;
    receipt.sync_all().map_err(|_| NOT_FOUND.to_string())?;
    let handover = read_taken(&from, id);
    let _ = fs::remove_file(&from);
    handover
        .filter(|h| offer_live(h, now))
        .ok_or_else(|| NOT_FOUND.to_string())
}

pub(crate) fn offer_live(h: &Handover, now: i64) -> bool {
    time::OffsetDateTime::parse(
        &h.offered_until,
        &time::format_description::well_known::Rfc3339,
    )
    .is_ok_and(|deadline| deadline.unix_timestamp() > now)
}

fn read_taken(path: &Path, id: &str) -> Option<Handover> {
    let bytes = switchboard_core::private_fs::read_private(path, MAX_RECORD).ok()?;
    serde_json::from_slice::<Handover>(&bytes)
        .ok()
        .filter(|h| h.id == id)
}

/// The background's look (every pass, only while the folder exists): a hand-over no console
/// took within `CONSOLE_WAIT` seconds, or one beyond `MAX_WAITING`, is taken and opened with
/// `start` (Terminal); an unreadable or expired pending record is removed. Returns how many
/// were opened. All `.taken` receipts remain until uninstall or explicit reconciliation.
pub(crate) fn sweep(root: &Path, now: i64, start: fn(&Path) -> Result<(), String>) -> usize {
    let folder = dir(root);
    let Ok(entries) = fs::read_dir(&folder) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let readable = path.extension().is_some_and(|x| x == "json") && read(&path).is_some();
        // A .taken file is an ambiguous launch receipt. Never delete it while a taker
        // may still be reading it; it is not eligible for replay.
        if (!readable && path.extension().is_some_and(|x| x == "json"))
            || read(&path).is_some_and(|h| !offer_live(&h, now))
        {
            let _ = fs::remove_file(&path);
        }
    }
    let waiting = pending(root);
    let overflow = waiting.len().saturating_sub(MAX_WAITING);
    let mut opened = 0;
    for (n, handover) in waiting.iter().enumerate() {
        if n >= overflow && now.saturating_sub(handover.created_at) < CONSOLE_WAIT {
            continue;
        }
        if let Ok(taken) = take_at(root, &handover.id, now) {
            let outcome = if start(&taken.script).is_ok() {
                opened += 1;
                "terminal"
            } else {
                "terminal_failed"
            };
            crate::oplog::event(
                "handover",
                &[("outcome", crate::oplog::Field::Code(outcome))],
            );
        }
    }
    opened
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn handover(id: &str, created_at: i64) -> Handover {
        Handover {
            id: id.into(),
            workflow: "wf_0123456789abcdef".into(),
            agent: "kimi-code".into(),
            account_label: "Work".into(),
            dir: "/work/repo".into(),
            script: format!("/data/kimi/sessions/x/{id}.command").into(),
            offered_until: "2099-10-13T01:00:00Z".into(),
            created_at,
        }
    }

    #[test]
    fn a_hand_over_waits_for_a_console_and_is_taken_once() {
        let root = tempfile::tempdir().unwrap();
        assert!(pending(root.path()).is_empty());
        let a = handover("ho-0123456789ab", 100);
        record(root.path(), &a).unwrap();
        assert_eq!(pending(root.path()), vec![a.clone()]);
        assert_eq!(take(root.path(), &a.id).unwrap(), a);
        assert_eq!(
            take(root.path(), &a.id).unwrap_err(),
            NOT_FOUND,
            "runs once"
        );
        assert!(pending(root.path()).is_empty());
        assert_eq!(take(root.path(), "../../etc/passwd").unwrap_err(), BAD_ID);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            record(root.path(), &handover("ho-00000000000f", 1)).unwrap();
            let mode = fs::metadata(root.path().join("handovers/ho-00000000000f.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    static STARTED: AtomicUsize = AtomicUsize::new(0);
    fn counted(_: &Path) -> Result<(), String> {
        STARTED.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[test]
    fn a_hand_over_no_console_takes_opens_in_terminal_after_two_minutes() {
        let root = tempfile::tempdir().unwrap();
        record(root.path(), &handover("ho-000000000001", 1_000)).unwrap();
        record(root.path(), &handover("ho-000000000002", 1_100)).unwrap();
        fs::write(
            root.path().join("handovers/ho-00000000000b.json"),
            b"{not json",
        )
        .unwrap();
        let before = STARTED.load(Ordering::SeqCst);
        // Within the wait nothing opens; the unreadable record goes.
        assert_eq!(sweep(root.path(), 1_000 + CONSOLE_WAIT - 1, counted), 0);
        assert!(!root.path().join("handovers/ho-00000000000b.json").exists());
        assert_eq!(pending(root.path()).len(), 2);
        // The first one's wait is over: Terminal opens it, once.
        assert_eq!(sweep(root.path(), 1_000 + CONSOLE_WAIT, counted), 1);
        assert_eq!(STARTED.load(Ordering::SeqCst) - before, 1);
        assert_eq!(pending(root.path())[0].id, "ho-000000000002");
        assert_eq!(sweep(root.path(), 1_000 + CONSOLE_WAIT, counted), 0);
    }

    #[test]
    fn the_setting_is_off_until_turned_on() {
        let root = tempfile::tempdir().unwrap();
        assert!(!enabled(root.path()));
        set_enabled(root.path(), true).unwrap();
        assert!(enabled(root.path()));
        set_enabled(root.path(), false).unwrap();
        set_enabled(root.path(), false).unwrap();
        assert!(!enabled(root.path()));
        assert!(id_valid(&new_id()));
        assert!(!id_valid("ho-0123456789AB") && !id_valid("hx-0123456789ab"));
    }

    #[test]
    fn an_expired_or_invalid_offer_never_opens_a_session() {
        let root = tempfile::tempdir().unwrap();
        let mut expired = handover("ho-000000000001", 100);
        expired.offered_until = "1970-01-01T00:02:00Z".into();
        record(root.path(), &expired).unwrap();
        assert!(take_at(root.path(), &expired.id, 120).is_err());
        expired.id = "ho-000000000002".into();
        expired.offered_until = "invalid".into();
        record(root.path(), &expired).unwrap();
        fn never(_: &Path) -> Result<(), String> {
            panic!("expired offer launched")
        }
        assert_eq!(sweep(root.path(), 1000, never), 0);
        assert!(pending(root.path()).is_empty());
    }

    #[test]
    fn two_consoles_cannot_take_the_same_handover() {
        let root = tempfile::tempdir().unwrap();
        let h = handover("ho-000000000001", 100);
        record(root.path(), &h).unwrap();
        let results = std::thread::scope(|scope| {
            let a = scope.spawn(|| take_at(root.path(), &h.id, 101));
            let b = scope.spawn(|| take_at(root.path(), &h.id, 101));
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(root
            .path()
            .join(format!("handovers/{}.taken", h.id))
            .is_file());
        assert!(
            record(root.path(), &h).is_err(),
            "a receipt prevents republication"
        );
    }

    #[test]
    fn a_crash_after_claim_never_reoffers_or_replays_the_pending_record() {
        let root = tempfile::tempdir().unwrap();
        let h = handover("ho-000000000001", 100);
        record(root.path(), &h).unwrap();
        let receipt = root.path().join(format!("handovers/{}.taken", h.id));
        switchboard_core::private_fs::create_new(&receipt).unwrap();
        assert!(pending(root.path()).is_empty());
        assert!(take_at(root.path(), &h.id, 101).is_err());
        fn never(_: &Path) -> Result<(), String> {
            panic!("an ambiguous claim was replayed")
        }
        assert_eq!(sweep(root.path(), 1000, never), 0);
        assert!(receipt.is_file());
    }
}
