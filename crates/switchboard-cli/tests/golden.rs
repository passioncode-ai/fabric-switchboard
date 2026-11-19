//! SB-397 W1.0: the golden answers of the session commands, recorded before the session code
//! moves into its own crates (W1.2 to W1.4), so the move is proved byte for byte. Each command runs
//! against one synthetic data folder with `HOME` in a scratch folder and the process table read as
//! empty (`SWITCHBOARD_TEST_NO_PROCESSES`, honoured in debug builds only), so an answer depends on
//! the records alone, never on the sessions running on the machine that runs the test. Times, pids
//! and the scratch paths are replaced by placeholders before the comparison.
//!
//! Regenerate after a deliberate change of an answer: `SWITCHBOARD_GOLDEN_UPDATE=1 cargo test -p
//! switchboard-cli --test golden`, then read the diff of `tests/golden/` before committing it.
#![cfg(unix)]
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};

/// The keys whose values are a time, a process or this machine's own: never compared.
const VOLATILE: &[&str] = &[
    "at",
    "since",
    "age",
    "live",
    "machine",
    "pid",
    "pids",
    "kept_pids",
    "closed_pid",
];

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// The synthetic data folder: one stop on a limit waiting and one resumed, two sessions the duty
/// officer closed (one with its intent line), a session a host ended, a Fabric hold and a cap.
fn seed(root: &Path, home: &Path) {
    let now = now();
    let folder = home.join("work/app");
    std::fs::create_dir_all(&folder).unwrap();
    let folder = folder.to_string_lossy().into_owned();
    let stop = |session: &str, state: &str, at: i64| {
        json!({
            "event": format!("cc-{session}-1"), "session": session, "account": "acct-1",
            "provider": "claude", "pool": "default", "directory": folder, "stopped_at": at,
            "state": state, "selected_account": null
        })
    };
    switchboard_core::private_fs::private_write(
        &root.join("recovery.json"),
        &serde_json::to_vec(&json!([
            stop("stop-waiting", "waiting", now - 600),
            stop("stop-resumed", "resumed", now - 900)
        ]))
        .unwrap(),
    )
    .unwrap();
    let config = home.join(".claude").to_string_lossy().into_owned();
    let journal = [
        json!({"at": now - 4000, "phase": "intent", "action": "close", "reason": "idle",
            "session": "closed-idle", "folder": folder, "pid": 999_991, "host": "terminal",
            "config": config, "prompt": null}),
        json!({"at": now - 3999, "action": "close", "reason": "idle", "session": "closed-idle",
            "folder": folder, "pid": 999_991, "host": "terminal", "outcome": "closed"}),
        json!({"at": now - 3000, "session": "closed-dup", "pid": 999_992, "cwd": folder,
            "reason": "duplicate", "by": "person", "outcome": "closed"}),
    ];
    let text: String = journal.iter().map(|l| format!("{l}\n")).collect();
    switchboard_core::private_fs::private_write(&root.join("duty.jsonl"), text.as_bytes()).unwrap();
    for (session, reason, by, at) in [
        ("closed-idle", "idle", "duty", now - 3999),
        ("host-ended", "host_idle", "host", now - 2000),
    ] {
        switchboard_runtime::resumable::record(
            root,
            json!({"session": session, "config": config, "cwd": folder, "reason": reason,
                "at": at, "by": by}),
        );
    }
    switchboard_runtime::monitors::hold(
        root,
        "held-by-fabric",
        now + 3600,
        now,
        Some("fabric_claim"),
    )
    .unwrap();
    switchboard_runtime::capacity::set(root, Some(12)).unwrap();
    // SB-411: a session resumed as itself (no home of its own) that has ended.
    let launches = root.join("launches");
    switchboard_core::private_fs::private_dir(&launches).unwrap();
    switchboard_core::private_fs::private_write(
        &launches.join("golden-resume-ended.json"),
        &serde_json::to_vec(&json!({
            "launch_id": "golden-resume-ended", "provider": "claude", "mode": "resume",
            "working_directory": folder, "in_place": true, "hosted": true,
            "launched_at": now - 500, "provider_session_id": "resumed-session",
            "home_dir": null, "home_path": null, "launched_by": {"surface": "dashboards"},
            "ended": {"kind": "exited", "at": now - 100, "exit_code": 0}, "ended_at": now - 100,
        }))
        .unwrap(),
    )
    .unwrap();
}

/// Replaces each volatile value, RFC 3339 times and the scratch paths, recursively.
fn normal(value: Value, scratch: &str) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let volatile = VOLATILE.contains(&k.as_str()) || k.ends_with("_at");
                    let v = if volatile && !v.is_null() {
                        json!(format!("<{k}>"))
                    } else {
                        normal(v, scratch)
                    };
                    (k, v)
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| normal(v, scratch)).collect())
        }
        Value::String(s) => {
            let s = s.replace(scratch, "<scratch>");
            let rfc3339 = s.len() >= 20
                && s.as_bytes()[4] == b'-'
                && s.as_bytes()[10] == b'T'
                && s.ends_with('Z');
            Value::String(if rfc3339 { "<time>".into() } else { s })
        }
        other => other,
    }
}

fn cli(root: &Path, home: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(root)
        .arg("--json")
        .args(args)
        .env("HOME", home)
        .env("SWITCHBOARD_TEST_NO_PROCESSES", "1")
        .env_remove("SWITCHBOARD_SESSION")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .unwrap();
    let stdout: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    json!({"exit": out.status.code(), "stdout": stdout,
        "stderr": serde_json::from_str::<Value>(&stderr).unwrap_or(Value::String(stderr))})
}

/// One MCP session: `tools/list` filtered to the session tools, then each call's answer.
fn mcp(root: &Path, home: &Path, calls: &[(&str, Value)]) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(root)
        .arg("mcp")
        .current_dir(home)
        .env("HOME", home)
        .env("SWITCHBOARD_TEST_NO_PROCESSES", "1")
        .env_remove("SWITCHBOARD_SESSION")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut send = |id: i64, method: &str, params: Value| -> Value {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(input, "{line}").unwrap();
        input.flush().unwrap();
        let mut answer = String::new();
        output.read_line(&mut answer).unwrap();
        serde_json::from_str(&answer).unwrap()
    };
    send(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "golden", "version": "1"}}),
    );
    let names: Vec<&str> = calls.iter().map(|(name, _)| *name).collect();
    let tools: Vec<Value> = send(2, "tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| names.contains(&t["name"].as_str().unwrap_or("")))
        .cloned()
        .collect();
    let mut answers = Vec::new();
    for (i, (name, arguments)) in calls.iter().enumerate() {
        let answer = send(
            3 + i as i64,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
        let result = &answer["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        answers.push(json!({
            "tool": name, "arguments": arguments, "is_error": result["isError"],
            "answer": serde_json::from_str::<Value>(text).unwrap_or(Value::String(text.into())),
        }));
    }
    drop(input);
    let _ = child.wait();
    json!({"tools": tools, "calls": answers})
}

fn compare(name: &str, answer: Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.json"));
    let text = serde_json::to_string_pretty(&answer).unwrap() + "\n";
    if std::env::var_os("SWITCHBOARD_GOLDEN_UPDATE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run with SWITCHBOARD_GOLDEN_UPDATE=1",
            path.display()
        )
    });
    assert!(
        golden == text,
        "{name}: the answer moved from {}\n--- golden\n{golden}\n--- now\n{text}",
        path.display()
    );
}

#[test]
fn the_session_commands_answer_as_recorded() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("data");
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    seed(&root, &home);
    let label = scratch.path().to_string_lossy().into_owned();
    // macOS gives the scratch folder as /var/... and resolves it to /private/var/...
    let canonical = scratch
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let tidy = |v: Value| {
        let v = normal(v, &canonical);
        normal(v, &label)
    };
    let cli_answers = json!({
        "sessions": cli(&root, &home, &["sessions"]),
        "sessions --all": cli(&root, &home, &["sessions", "--all"]),
        "sessions --rows": cli(&root, &home, &["sessions", "--rows"]),
        "sessions --tree": cli(&root, &home, &["sessions", "--tree"]),
        "duty": cli(&root, &home, &["duty"]),
        "capacity": cli(&root, &home, &["capacity"]),
        "recovery": cli(&root, &home, &["recovery"]),
        // SB-423 (FD-316): only the open stops; only the named launches.
        "recovery --open": cli(&root, &home, &["recovery", "--open"]),
        "sessions --history --launch": cli(
            &root,
            &home,
            &["sessions", "--history", "--launch", "golden-resume-ended", "--launch", "no-such-launch"],
        ),
        "sessions continue (unknown)": cli(&root, &home, &["sessions", "continue", "no-such-session"]),
        "sessions restart status (none)": cli(&root, &home, &["sessions", "restart", "status"]),
        "monitor hold (past)": cli(&root, &home, &["monitor", "hold", "--session", "s", "--until", "1"]),
    });
    compare("sessions-cli", tidy(cli_answers));
    let mcp_answers = mcp(
        &root,
        &home,
        &[
            ("switchboard_sessions", json!({"all": true})),
            ("switchboard_sessions", json!({"rows": true})),
            ("switchboard_sessions", json!({"tree": true})),
            ("switchboard_duty", json!({})),
            ("switchboard_capacity", json!({})),
            ("switchboard_recovery", json!({})),
            ("switchboard_recovery", json!({"open": true})),
            (
                "switchboard_sessions",
                json!({"history": true, "launch_ids": ["golden-resume-ended", "no-such-launch"]}),
            ),
            (
                "switchboard_sessions",
                json!({"launch_ids": ["golden-resume-ended"]}),
            ),
            (
                "switchboard_session_continue",
                json!({"session_id": "no-such-session"}),
            ),
            ("switchboard_sessions_restart_status", json!({})),
        ],
    );
    compare("sessions-mcp", tidy(mcp_answers));
}
