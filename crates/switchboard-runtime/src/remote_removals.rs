//! SB-560 (W11.1, M3-03): a call back took a sign-in back, but the copy on the other machine
//! could not be removed. That copy is switched off there (a delegated seat carries
//! `surrendered_at`, which keeps it off), yet it still holds the sign-in. The removal is tried
//! again from the owner's background pass, every 15 minutes, through the machine proof, until it
//! is removed or 30 days have passed.
//!
//! `remote-removals.json` (0600, at most 64 KiB): per pending removal the account's id here, the
//! machine as ssh knows it, the account's id there, the key it was delegated under (a public
//! key), the other machine's program and data folder, and times. Never a credential.
use crate::account_send::{Program, Run};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const FILE: &str = "remote-removals.json";
/// How often one removal is tried again.
pub const RETRY_SECONDS: i64 = 15 * 60;
/// After this long a removal is given up (the copy there was surrendered; its sign-in was renewed
/// here since, so the copy there is spent).
pub const GIVE_UP_SECONDS: i64 = 30 * 86_400;
/// At most this many removals are tried in one pass.
pub const PER_PASS: usize = 4;
const MAX_PENDING: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub account: String,
    pub machine: String,
    pub remote_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_key: Option<String>,
    pub program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    pub since: i64,
    #[serde(default)]
    pub tries: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_try: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Removals {
    removals: Vec<Pending>,
}

fn read(root: &Path) -> Removals {
    switchboard_core::private_fs::read_private(&root.join(FILE), 64 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(root: &Path, removals: &Removals) -> bool {
    let file = root.join(FILE);
    if removals.removals.is_empty() {
        return match std::fs::remove_file(&file) {
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            Ok(()) => true,
        };
    }
    serde_json::to_vec_pretty(removals)
        .ok()
        .is_some_and(|bytes| switchboard_core::private_fs::private_write(&file, &bytes).is_ok())
}

/// Records a removal to try again; the same copy (machine and id there) is recorded once.
pub fn add(root: &Path, pending: Pending) {
    let mut removals = read(root);
    removals
        .removals
        .retain(|p| !(p.machine == pending.machine && p.remote_id == pending.remote_id));
    removals.removals.push(pending);
    if removals.removals.len() > MAX_PENDING {
        let over = removals.removals.len() - MAX_PENDING;
        removals.removals.drain(..over);
    }
    if !write(root, &removals) {
        crate::oplog::event(
            "remote_removal",
            &[("result", crate::oplog::Field::Code("unwritten"))],
        );
    }
}

/// The removals waiting, oldest first.
pub fn pending(root: &Path) -> Vec<Pending> {
    read(root).removals
}

fn due(p: &Pending, now: i64) -> bool {
    p.last_try.is_none_or(|t| now >= t + RETRY_SECONDS)
}

/// Tries the due removals (at most `PER_PASS`): `remove` answers whether the copy there is gone.
/// A removal past `GIVE_UP_SECONDS` is dropped and logged. Answers (removed, failed, given up).
pub fn retry(root: &Path, now: i64, remove: &dyn Fn(&Pending) -> bool) -> (usize, usize, usize) {
    let mut removals = read(root);
    if removals.removals.is_empty() {
        return (0, 0, 0);
    }
    let before = removals.clone();
    let given_up = removals.removals.len();
    removals
        .removals
        .retain(|p| now < p.since + GIVE_UP_SECONDS);
    let given_up = given_up - removals.removals.len();
    let (mut removed, mut failed) = (0, 0);
    let mut kept = Vec::new();
    let mut tried = 0;
    for mut p in std::mem::take(&mut removals.removals) {
        if tried < PER_PASS && due(&p, now) {
            tried += 1;
            if remove(&p) {
                removed += 1;
                continue;
            }
            failed += 1;
            p.tries = p.tries.saturating_add(1);
            p.last_try = Some(now);
        }
        kept.push(p);
    }
    removals.removals = kept;
    if removals != before {
        write(root, &removals);
    }
    if removed + failed + given_up > 0 {
        crate::oplog::event(
            "remote_removal",
            &[
                ("removed", crate::oplog::Field::Number(removed as i64)),
                ("failed", crate::oplog::Field::Number(failed as i64)),
                ("given_up", crate::oplog::Field::Number(given_up as i64)),
            ],
        );
    }
    (removed, failed, given_up)
}

/// One removal there, through the runner given: the copy is removed under its key and the
/// account list no longer names it.
pub fn remove_there(p: &Pending, run: &Run<'_>) -> bool {
    let program = Program {
        path: &p.program,
        data_dir: p.data_dir.as_deref(),
    };
    let taken = crate::account_send::Taken {
        remote_id: p.remote_id.clone(),
        token: None,
        kept: None,
        lease_ended: false,
        sender_key: p.sender_key.clone(),
    };
    crate::account_send::recall_finish(&p.machine, &taken, &program, run)["proved"] == true
}

static BUSY: AtomicBool = AtomicBool::new(false);

/// From the owner's background pass: when a removal is due, tries it on a thread of its own,
/// through the machine proof (an act). Nothing is read while no file exists.
pub fn tick(root: &Path, now: i64) {
    if std::fs::symlink_metadata(root.join(FILE)).is_err() {
        return;
    }
    if !pending(root).iter().any(|p| due(p, now)) {
        return;
    }
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let root = root.to_owned();
    let spawned = std::thread::Builder::new()
        .name("remote-removals".into())
        .spawn(move || {
            retry(&root, now, &|p| {
                let program = Program {
                    path: &p.program,
                    data_dir: p.data_dir.as_deref(),
                };
                let ssh = crate::machine_proof::ssh(&root, &p.machine, &program);
                remove_there(p, &|a, i| ssh.run(a, i))
            });
            BUSY.store(false, Ordering::Release);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn pending(remote_id: &str, since: i64) -> Pending {
        Pending {
            account: "a".into(),
            machine: "teammate".into(),
            remote_id: remote_id.into(),
            sender_key: Some("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=".into()),
            program: "/x/switchboard".into(),
            data_dir: None,
            since,
            tries: 0,
            last_try: None,
        }
    }

    #[test]
    fn a_removal_is_tried_every_15_minutes_until_it_is_gone_or_30_days_old() {
        let root = tempfile::tempdir().unwrap();
        add(root.path(), pending("r-1", 1_000));
        add(root.path(), pending("r-1", 1_000));
        assert_eq!(super::pending(root.path()).len(), 1, "one copy once");
        let calls = Mutex::new(0);
        let refuse = |_: &Pending| {
            *calls.lock().unwrap() += 1;
            false
        };
        assert_eq!(retry(root.path(), 2_000, &refuse), (0, 1, 0));
        assert_eq!(retry(root.path(), 2_100, &refuse), (0, 0, 0), "not due yet");
        assert_eq!(*calls.lock().unwrap(), 1);
        let left = super::pending(root.path());
        assert_eq!((left[0].tries, left[0].last_try), (1, Some(2_000)));
        assert_eq!(
            retry(root.path(), 2_000 + RETRY_SECONDS, &|_| true),
            (1, 0, 0)
        );
        assert!(
            !root.path().join(FILE).exists(),
            "the file goes with the last one"
        );

        add(root.path(), pending("r-2", 1_000));
        assert_eq!(
            retry(root.path(), 1_000 + GIVE_UP_SECONDS, &|_| unreachable!()),
            (0, 0, 1)
        );
        assert!(super::pending(root.path()).is_empty());
    }

    #[test]
    fn a_pass_tries_at_most_four_and_the_file_holds_no_secret() {
        let root = tempfile::tempdir().unwrap();
        for n in 0..6 {
            add(root.path(), pending(&format!("r-{n}"), 1_000));
        }
        assert_eq!(retry(root.path(), 2_000, &|_| false), (0, PER_PASS, 0));
        let text = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        assert!(!text.contains("token") && !text.contains("refresh"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions();
            assert_eq!(mode.mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn the_removal_names_the_key_and_proves_the_copy_is_gone() {
        let sent = Mutex::new(Vec::<String>::new());
        let run = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            let command = args.last().cloned().unwrap_or_default();
            sent.lock().unwrap().push(command.clone());
            if command.contains("'remove'") {
                return Ok(json!({"ok": true, "data": {"removed": true}}).to_string());
            }
            Ok(json!({"ok": true, "data": {"accounts": [{"id": "other"}]}}).to_string())
        };
        assert!(remove_there(&pending("r-1", 1), &run));
        let sent = sent.lock().unwrap();
        assert!(sent[0].contains("'r-1'") && sent[0].contains("'--sender-key'"));
        assert!(sent[1].contains("'list'"));
    }
}
