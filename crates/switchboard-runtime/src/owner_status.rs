//! SB-576 (Fabric Dashboards' board request `31261095`, item 2): whether an owner answers on this
//! data folder, its pid, version and how it runs, read without ever starting one and without the
//! store's lock.
//!
//! `switchboard --json status` and MCP `switchboard_status` ask the owner its `Status` through
//! `control.json`. When it answers, its own answer says who it is (`owner_pid`, `owner_version`,
//! `owner_kind`, `owner_supervisor`, `owner_started_at`). When it does not, nothing is opened
//! offline: the control port is asked to prove itself (the identity exchange only, as the
//! watchdog's probe, SB-510) and `owner-run.json` (SB-379) is read, so the answer says whether an
//! owner runs and does not answer, is starting, is gone or was never there. Nothing here writes a
//! file, takes a lock or starts a process.
use crate::{bugs, control, Operation};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// How long `status` waits for an owner that did not answer its request to prove itself.
pub const PROBE_SECONDS: u64 = 2;

/// What `status` found when the owner did not answer its request.
#[derive(Clone, Debug)]
pub struct Seen {
    /// The identity exchange at the port `control.json` names.
    pub probe: control::Probe,
    /// The pid `control.json` names, when it names one.
    pub descriptor_pid: Option<u32>,
    /// The owner's last run as `owner-run.json` records it.
    pub run: Option<bugs::Run>,
    /// Whether that run's process still runs.
    pub run_alive: bool,
    /// Why the request itself failed, when it did (`None`: no descriptor, or nothing listened).
    pub request_error: Option<String>,
}

/// The `owner` block from the owner's own `Status` answer. An owner older than SB-576 names no
/// pid: the one `control.json` names stands in.
pub fn answered(answer: &Value, descriptor_pid: Option<u32>) -> Value {
    let pid = answer["owner_pid"]
        .as_u64()
        .or(descriptor_pid.map(u64::from));
    json!({
        "state": "answers",
        "reason": null,
        "pid": pid,
        "version": answer["owner_version"],
        "kind": answer["owner_kind"],
        "supervisor": answer["owner_supervisor"],
        "started_at": answer["owner_started_at"],
        "last_run": null,
    })
}

/// Why a request the owner did not answer failed, as a code.
fn request_code(error: &str) -> &'static str {
    if control::is_timeout(error) {
        "timeout"
    } else if error == control::CONTROL_BUSY {
        "busy"
    } else if error == control::CONTROL_OWNER_OLDER || error == control::CONTROL_OWNER_NEWER {
        "version_skew"
    } else {
        "refused_request"
    }
}

/// The `owner` block when the owner did not answer the request. `state`:
/// - `answers`: it proved itself at its port, but did not answer `Status` (`reason`: `busy`,
///   `timeout`, `version_skew`, `refused_request`);
/// - `not_answering`: its process runs and it does not answer (`reason`: `no_answer`, `refused`,
///   `unreadable` for a `control.json` no owner of this version writes, `starting` for a run
///   that published no port yet);
/// - `gone`: `control.json` names a process that ended (`reason`: `process_ended`);
/// - `absent`: no `control.json` and no running owner.
pub fn unanswered(seen: &Seen) -> Value {
    let last_run = seen.run.as_ref().map(|r| {
        json!({"pid": r.pid, "version": r.version, "started_at": r.started_at, "clean": r.clean})
    });
    let block =
        |state: &str, reason: Option<&str>, pid: Option<u32>, version: Value, started: Value| {
            json!({
                "state": state, "reason": reason, "pid": pid, "version": version, "kind": null,
                "supervisor": null, "started_at": started, "last_run": last_run,
            })
        };
    // The run file's version and start, when it is the process `control.json` names.
    let run_of = |pid: Option<u32>| {
        seen.run
            .as_ref()
            .filter(|r| Some(r.pid) == pid)
            .map(|r| (json!(r.version), json!(r.started_at)))
            .unwrap_or((Value::Null, Value::Null))
    };
    match &seen.probe {
        control::Probe::Answers { version } => {
            let (run_version, started) = run_of(seen.descriptor_pid);
            let version = version.as_ref().map_or(run_version, |v| json!(v));
            let reason = seen
                .request_error
                .as_deref()
                .map_or("refused_request", request_code);
            block(
                "answers",
                Some(reason),
                seen.descriptor_pid,
                version,
                started,
            )
        }
        control::Probe::NoAnswer | control::Probe::Refused => {
            let (version, started) = run_of(seen.descriptor_pid);
            let reason = if seen.probe == control::Probe::Refused {
                "refused"
            } else {
                "no_answer"
            };
            block(
                "not_answering",
                Some(reason),
                seen.descriptor_pid,
                version,
                started,
            )
        }
        control::Probe::Unreadable => block(
            "not_answering",
            Some("unreadable"),
            None,
            Value::Null,
            Value::Null,
        ),
        control::Probe::OwnerGone => block(
            "gone",
            Some("process_ended"),
            seen.descriptor_pid,
            Value::Null,
            Value::Null,
        ),
        control::Probe::NoDescriptor => match seen.run.as_ref() {
            // An owner that took the store and has not published its port yet (or never will).
            Some(run) if !run.clean && seen.run_alive => block(
                "not_answering",
                Some("starting"),
                Some(run.pid),
                json!(run.version),
                json!(run.started_at),
            ),
            _ => block("absent", None, None, Value::Null, Value::Null),
        },
    }
}

/// The `owner` block for a `Status` request already made: the owner's own answer, else what the
/// port and `owner-run.json` say. Never starts an owner, never opens the store.
pub async fn owner_of(root: &Path, request: &Result<Option<Value>, String>) -> Value {
    if let Ok(Some(answer)) = request {
        return answered(answer, control::descriptor_pid(root));
    }
    let probe = control::probe(root, Duration::from_secs(PROBE_SECONDS)).await;
    let run = bugs::last_run(root);
    let run_alive = run.as_ref().is_some_and(|r| bugs::alive(r.pid));
    unanswered(&Seen {
        probe,
        descriptor_pid: control::descriptor_pid(root),
        run,
        run_alive,
        request_error: request.as_ref().err().cloned(),
    })
}

/// `switchboard --json status`: the owner's own `Status` with `owner`, or, when it did not answer
/// that request, the same fields read from files only (no proxy address) with `owner` saying why.
/// `online` is true exactly when an owner answers: one that proved itself at its port and was busy
/// is online, so a caller never starts a second one beside it.
pub async fn status(root: &Path) -> Value {
    let request = control::request(root, &Operation::Status).await;
    let owner = owner_of(root, &request).await;
    match request {
        Ok(Some(mut answer)) => {
            answer["owner"] = owner;
            answer
        }
        _ => {
            let online = owner["state"] == "answers";
            json!({
                "proxy_address": null,
                "platform": std::env::consts::OS,
                "live_mode": if online { "Next request · HTTP/SSE" } else { "Offline" },
                "owner_version": owner["version"],
                "owner_kind": null,
                "owner_copy": null,
                "owner_updates_copy": null,
                "online": online,
                "vault": switchboard_core::vault_kind(root).ok(),
                "keychain_too_large": crate::item_size::status(root),
                "owner": owner,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pid: u32, clean: bool) -> bugs::Run {
        bugs::Run {
            pid,
            started_at: 1_000,
            version: "0.7.120".into(),
            clean,
        }
    }
    fn seen(probe: control::Probe) -> Seen {
        Seen {
            probe,
            descriptor_pid: Some(77),
            run: Some(run(77, false)),
            run_alive: true,
            request_error: None,
        }
    }

    #[test]
    fn an_owner_that_answers_names_itself() {
        let answer = json!({"owner_pid": 41, "owner_version": "0.7.125", "owner_kind": "serve",
            "owner_supervisor": "launchd", "owner_started_at": 900});
        let owner = answered(&answer, Some(99));
        assert_eq!(owner["state"], "answers");
        assert_eq!(owner["pid"], 41);
        assert_eq!(owner["version"], "0.7.125");
        assert_eq!(owner["kind"], "serve");
        assert_eq!(owner["supervisor"], "launchd");
        assert_eq!(owner["started_at"], 900);
        // An owner before SB-576 names no pid: control.json's stands in.
        let older = answered(
            &json!({"owner_version": "0.7.100", "owner_kind": "app"}),
            Some(99),
        );
        assert_eq!(older["pid"], 99);
        assert!(older["supervisor"].is_null());
    }

    #[test]
    fn an_owner_that_proves_itself_but_does_not_answer_says_why() {
        let mut busy = seen(control::Probe::Answers {
            version: Some("0.7.125".into()),
        });
        busy.request_error = Some(control::CONTROL_BUSY.into());
        let owner = unanswered(&busy);
        assert_eq!(owner["state"], "answers");
        assert_eq!(owner["reason"], "busy");
        assert_eq!(owner["pid"], 77);
        assert_eq!(owner["version"], "0.7.125");
        busy.request_error = Some(control::CONTROL_TIMED_OUT.into());
        assert_eq!(unanswered(&busy)["reason"], "timeout");
        busy.request_error = Some(control::CONTROL_OWNER_OLDER.into());
        assert_eq!(unanswered(&busy)["reason"], "version_skew");
    }

    #[test]
    fn a_running_owner_that_does_not_answer_is_not_answering() {
        let owner = unanswered(&seen(control::Probe::NoAnswer));
        assert_eq!(owner["state"], "not_answering");
        assert_eq!(owner["reason"], "no_answer");
        assert_eq!(owner["pid"], 77);
        assert_eq!(owner["version"], "0.7.120", "from the run file of that pid");
        assert_eq!(
            unanswered(&seen(control::Probe::Refused))["reason"],
            "refused"
        );
        let unreadable = unanswered(&seen(control::Probe::Unreadable));
        assert_eq!(unreadable["reason"], "unreadable");
        assert!(unreadable["pid"].is_null());
    }

    #[test]
    fn a_descriptor_whose_process_ended_is_gone() {
        let mut s = seen(control::Probe::OwnerGone);
        s.run_alive = false;
        let owner = unanswered(&s);
        assert_eq!(owner["state"], "gone");
        assert_eq!(owner["reason"], "process_ended");
        assert_eq!(owner["pid"], 77);
        assert_eq!(owner["last_run"]["clean"], false);
    }

    #[test]
    fn without_a_descriptor_a_running_unclean_run_is_starting_and_anything_else_absent() {
        let starting = unanswered(&seen(control::Probe::NoDescriptor));
        assert_eq!(starting["state"], "not_answering");
        assert_eq!(starting["reason"], "starting");
        assert_eq!(starting["pid"], 77);
        assert_eq!(starting["started_at"], 1_000);
        let mut ended = seen(control::Probe::NoDescriptor);
        ended.run = Some(run(77, true));
        let absent = unanswered(&ended);
        assert_eq!(absent["state"], "absent");
        assert!(absent["pid"].is_null());
        assert_eq!(absent["last_run"]["clean"], true);
        let mut crashed = seen(control::Probe::NoDescriptor);
        crashed.run_alive = false;
        assert_eq!(unanswered(&crashed)["state"], "absent");
        let never = Seen {
            probe: control::Probe::NoDescriptor,
            descriptor_pid: None,
            run: None,
            run_alive: false,
            request_error: None,
        };
        let owner = unanswered(&never);
        assert_eq!(owner["state"], "absent");
        assert!(owner["last_run"].is_null());
    }

    /// A `control.json` whose process ended: `gone`, offline, and still nothing is opened.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_stale_descriptor_reads_gone_and_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // A pid far above any real one: no process has it.
        let descriptor = json!({"protocol": 1, "address": "127.0.0.1:9", "token": "a".repeat(64),
            "pid": 999_999_999u32});
        switchboard_core::private_fs::private_write(
            &root.join("control.json"),
            &serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let answer = status(root).await;
        assert_eq!(answer["online"], false);
        assert_eq!(answer["live_mode"], "Offline");
        assert_eq!(answer["owner"]["state"], "gone");
        assert_eq!(answer["owner"]["pid"], 999_999_999u32);
        assert!(!root.join("instance.lock").exists(), "status took the lock");
    }

    /// A data folder with no owner: the answer comes from files, creates nothing and takes no
    /// lock (no `instance.lock`, no folder made).
    #[tokio::test]
    async fn an_empty_data_folder_answers_absent_and_creates_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let answer = status(&root).await;
        assert_eq!(answer["online"], false);
        assert_eq!(answer["owner"]["state"], "absent");
        assert!(!root.exists(), "status made the data folder");
        std::fs::create_dir_all(&root).unwrap();
        let answer = status(&root).await;
        assert_eq!(answer["owner"]["state"], "absent");
        assert!(!root.join("instance.lock").exists(), "status took the lock");
    }
}
