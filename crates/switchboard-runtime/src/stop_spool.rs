//! SB-397 W0.4: a session's limit stop reported while no owner runs is kept, not lost.
//!
//! Claude Code's `StopFailure` hook runs `switchboard recovery hook`; registering the stop needs
//! the owner (the limits it saw and the account a session used live in its memory). Before this,
//! a stop reported while the app quit or updated was refused and the session waited for nobody.
//! Now the hook writes one file per session in `<data>/stop-spool/` (0600: the session id, its
//! folder, whether it is a billing stop and when; never text the session wrote), and the owner's
//! next pass registers each one as if the hook had just run, then removes it. A file older than
//! a day is dropped: its limit window has passed and the session was resumed or ended by then.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DIR: &str = "stop-spool";
/// How long a spooled stop is worth registering.
pub const KEEP_SECONDS: i64 = 86_400;
/// At most this many waiting stops; past it the newest report is not kept (LC-12).
pub const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spooled {
    pub session: String,
    pub directory: PathBuf,
    pub billing: bool,
    pub at: i64,
}

/// Keeps one stop for the next owner. The newest report of a session replaces an older one.
pub fn spool(root: &Path, stop: &Spooled) -> Result<(), String> {
    if !crate::launches::valid_id(&stop.session) || !stop.directory.is_absolute() {
        return Err("No saved account matches the stopped session.".into());
    }
    let dir = root.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|_| "The stop could not be kept for later.")?;
    let file = dir.join(format!("{}.json", stop.session));
    let waiting = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    if waiting >= MAX_FILES && !file.exists() {
        return Err("The stop could not be kept for later.".into());
    }
    let bytes = serde_json::to_vec(stop).map_err(|_| "The stop could not be kept for later.")?;
    switchboard_core::private_fs::private_write(&file, &bytes)
        .map_err(|_| "The stop could not be kept for later.".into())
}

/// Every stop waiting, oldest first, with its file. A stop stays until `done` removes it, after the
/// owner registered it (or refused it for good): a quit between the two registers it again, as
/// `again`, never loses it. A file that is not a stop, or is older than `KEEP_SECONDS`, is removed
/// and not returned.
pub fn waiting(root: &Path, now: i64) -> Vec<(PathBuf, Spooled)> {
    let Ok(entries) = std::fs::read_dir(root.join(DIR)) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, Spooled)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stop = switchboard_core::private_fs::read_private(&path, MAX_BYTES)
                .ok()
                .and_then(|b| serde_json::from_slice::<Spooled>(&b).ok())
                .filter(|s| now - s.at < KEEP_SECONDS);
            if stop.is_none() {
                let _ = std::fs::remove_file(&path);
            }
            stop.map(|s| (path, s))
        })
        .collect();
    out.sort_by_key(|(_, s)| s.at);
    out
}

/// Removes a stop the owner handled.
pub fn done(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(session: &str, at: i64) -> Spooled {
        Spooled {
            session: session.into(),
            directory: PathBuf::from("/w/a"),
            billing: false,
            at,
        }
    }

    #[test]
    fn a_stop_is_kept_once_per_session_until_it_was_handled() {
        let root = tempfile::tempdir().unwrap();
        spool(root.path(), &stop("S1", 100)).unwrap();
        spool(root.path(), &stop("S2", 50)).unwrap();
        spool(root.path(), &stop("S1", 120)).unwrap();
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(root.path().join(DIR).join("S1.json"))
                    .unwrap()
                    .permissions()
            ) & 0o777,
            0o600
        );
        let waiting: Vec<_> = waiting(root.path(), 200);
        let stops: Vec<_> = waiting.iter().map(|(_, s)| s.clone()).collect();
        assert_eq!(stops, vec![stop("S2", 50), stop("S1", 120)]);
        assert_eq!(
            super::waiting(root.path(), 200).len(),
            2,
            "a stop stays until it was handled"
        );
        for (path, _) in &waiting {
            done(path);
        }
        assert!(
            super::waiting(root.path(), 200).is_empty(),
            "each stop is handed over once"
        );
    }

    #[test]
    fn an_old_or_broken_report_is_dropped_and_a_bad_one_is_never_kept() {
        let root = tempfile::tempdir().unwrap();
        spool(root.path(), &stop("S1", 0)).unwrap();
        std::fs::write(root.path().join(DIR).join("junk.json"), b"{").unwrap();
        assert!(waiting(root.path(), KEEP_SECONDS).is_empty());
        assert_eq!(std::fs::read_dir(root.path().join(DIR)).unwrap().count(), 0);
        assert!(spool(root.path(), &stop("../x", 1)).is_err());
        let mut relative = stop("S1", 1);
        relative.directory = PathBuf::from("w");
        assert!(spool(root.path(), &relative).is_err());
    }
}
