//! SB-332 (Fabric Dashboards' request `0080400f`, operator 2026-11-11 «чтобы можно было
//! коннектиться к удаленным агентам ... идти с кодом отсюда, запускать редактирование там»): the
//! other machines this Switchboard knows, and the ssh command that starts a coding session on one
//! of them in this terminal. The session there is that machine's own `switchboard launch`, so its
//! account comes from the folder's group there, as a session started at its keyboard would.

use crate::account_send::{quoted, target_valid, Program, Run, REMOTE_DEFAULT, TARGET_INVALID};
use serde_json::{json, Value};
use std::path::Path;

/// One machine: as ssh names it, the switchboard and data folder used there, when it was last
/// used from here, and why it is known (`moves`: accounts moved there; `dispatch`: runs started
/// there).
pub fn known(root: &Path) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut note =
        |machine: &str, at: i64, why: &str, program: Option<&str>, data_dir: Option<&str>| {
            if !target_valid(machine) {
                return;
            }
            match out.iter_mut().find(|t| t["machine"] == machine) {
                Some(t) => {
                    if t["last_at"].as_i64().unwrap_or(0) < at {
                        t["last_at"] = json!(at);
                    }
                    if !t["known_by"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|x| x == why))
                    {
                        if let Some(k) = t["known_by"].as_array_mut() {
                            k.push(json!(why));
                        }
                    }
                    if t["program"].is_null() && program.is_some() {
                        t["program"] = json!(program);
                    }
                    if t["data_dir"].is_null() && data_dir.is_some() {
                        t["data_dir"] = json!(data_dir);
                    }
                }
                None => out.push(json!({
                    "machine": machine, "last_at": at, "known_by": [why],
                    "program": program, "data_dir": data_dir,
                })),
            }
        };
    for s in crate::seats::recent(root, 1000).iter().rev() {
        if matches!(s.event.as_str(), "granted" | "recalled") {
            note(
                &s.machine,
                s.at,
                "moves",
                s.program.as_deref(),
                s.data_dir.as_deref(),
            );
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("remote-dispatch")) {
        for e in entries.flatten() {
            let Ok(bytes) = switchboard_core::private_fs::read_private(
                &e.path().join(".session.json"),
                64 * 1024,
            ) else {
                continue;
            };
            let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            if let Some(machine) = v["dispatch"]["target"].as_str() {
                note(
                    machine,
                    v["launched_at"].as_i64().unwrap_or(0),
                    "dispatch",
                    v["dispatch"]["program"].as_str(),
                    v["dispatch"]["data_dir"].as_str(),
                );
            }
        }
    }
    out.sort_by(|a, b| b["last_at"].as_i64().cmp(&a["last_at"].as_i64()));
    out
}

/// Whether the machine's Switchboard answers over ssh now, and its version.
pub fn probe(target: &str, program: &Program<'_>, run: &Run<'_>) -> Value {
    let answer = run(
        &[
            target.to_owned(),
            "--".to_owned(),
            crate::account_send::remote(program, &["--json", "status"]),
        ],
        None,
    )
    .ok()
    .and_then(|out| crate::account_send::data(&out));
    match answer {
        Some(d) => {
            json!({"online": d["online"], "owner_version": d["owner_version"], "answered": true})
        }
        None => json!({"answered": false}),
    }
}

/// The program and data folder to use on `target`: the ones named, else those the newest move or
/// run there used, else the macOS app's with its own data folder.
pub fn program_for(
    root: &Path,
    target: &str,
    program: Option<String>,
    data_dir: Option<String>,
) -> (String, Option<String>) {
    let seen = known(root).into_iter().find(|t| t["machine"] == target);
    let program = program
        .or_else(|| {
            seen.as_ref()
                .and_then(|t| t["program"].as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| REMOTE_DEFAULT.into());
    let data_dir =
        data_dir.or_else(|| seen.and_then(|t| t["data_dir"].as_str().map(str::to_owned)));
    (program, data_dir)
}

/// The arguments of `ssh` that run `<program> [--data-dir D] launch <launch_args…>` on `target`:
/// with a terminal for a person (`-t`, the session in this terminal) or without one (`-T`, a
/// headless run). Every word is quoted for the remote shell; the agent's own arguments go after
/// `--` untouched.
pub fn launch_ssh(
    target: &str,
    program: &Program<'_>,
    launch_args: &[String],
    tty: bool,
) -> Result<Vec<String>, String> {
    if !target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    program.check()?;
    let mut words: Vec<String> = vec![quoted(program.path)];
    if let Some(dir) = program.data_dir {
        words.push(quoted("--data-dir"));
        words.push(quoted(dir));
    }
    words.push(quoted("launch"));
    words.extend(launch_args.iter().map(|a| quoted(a)));
    Ok(vec![
        if tty { "-t" } else { "-T" }.to_owned(),
        "-o".to_owned(),
        "ConnectTimeout=15".to_owned(),
        target.to_owned(),
        "--".to_owned(),
        words.join(" "),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_send::DEFAULT_APP;

    #[test]
    fn machines_come_from_moves_and_runs_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let mut a = crate::seats::of(10, "granted", "acc-1", "me@studio.local", "default", None);
        a.data_dir = Some("/srv/sb".into());
        crate::seats::append(root.path(), &a).unwrap();
        crate::seats::append(
            root.path(),
            &crate::seats::of(5, "handed_over", "acc-2", "colleague", "default", None),
        )
        .unwrap();
        let run = root.path().join("remote-dispatch").join("l1");
        std::fs::create_dir_all(&run).unwrap();
        switchboard_core::private_fs::private_write(
            &run.join(".session.json"),
            br#"{"launched_at":20,"dispatch":{"target":"other@vm","program":null,"data_dir":null}}"#,
        )
        .unwrap();
        let found = known(root.path());
        let machines: Vec<&str> = found
            .iter()
            .map(|t| t["machine"].as_str().unwrap())
            .collect();
        assert_eq!(
            machines,
            vec!["other@vm", "me@studio.local"],
            "a hand-over names no machine"
        );
        assert_eq!(found[1]["data_dir"], "/srv/sb");
        assert_eq!(found[0]["known_by"], json!(["dispatch"]));
        let (program, data_dir) = program_for(root.path(), "me@studio.local", None, None);
        assert_eq!(
            (program.as_str(), data_dir.as_deref()),
            (REMOTE_DEFAULT, Some("/srv/sb"))
        );
    }

    #[test]
    fn a_remote_launch_is_one_quoted_command_and_a_bad_target_is_refused() {
        let args: Vec<String> = [
            "--provider",
            "claude",
            "--working-directory",
            "/Users/me/DATA/it's",
            "--in-place",
            "--",
            "--resume",
            "a b",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let ssh = launch_ssh("me@studio.local", &DEFAULT_APP, &args, true).unwrap();
        assert_eq!(
            &ssh[..5],
            &["-t", "-o", "ConnectTimeout=15", "me@studio.local", "--"]
        );
        assert_eq!(
            ssh[5],
            format!("'{REMOTE_DEFAULT}' 'launch' '--provider' 'claude' '--working-directory' '/Users/me/DATA/it'\\''s' '--in-place' '--' '--resume' 'a b'")
        );
        assert_eq!(
            launch_ssh("me@studio.local", &DEFAULT_APP, &args, false).unwrap()[0],
            "-T"
        );
        assert_eq!(
            launch_ssh("-oProxyCommand=x", &DEFAULT_APP, &args, true).unwrap_err(),
            TARGET_INVALID
        );
    }

    #[test]
    fn a_probe_reads_the_version_or_says_it_was_not_answered() {
        let up = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Ok(r#"{"ok":true,"data":{"online":true,"owner_version":"0.7.52"}}"#.into())
        };
        assert_eq!(
            probe("me@studio.local", &DEFAULT_APP, &up)["owner_version"],
            "0.7.52"
        );
        let down = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err("ssh failed.".into())
        };
        assert_eq!(
            probe("me@studio.local", &DEFAULT_APP, &down)["answered"],
            false
        );
    }
}
