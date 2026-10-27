use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};
use switchboard_core::MemoryVault;
use switchboard_runtime::Owner;
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_switchboard"))
}
#[test]
fn help_and_invalid_arguments_do_not_create_storage_or_echo_values() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-created");
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!root.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("--secret-stdin"));
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .args(["--json", "--secret", "fixture-never-echo"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-never-echo"));
    assert!(!root.exists());
}
#[test]
fn a_command_group_named_alone_prints_its_help_or_a_json_error() {
    // PRD-07 (iteration 2026-10-24): `switchboard accounts` printed only "Invalid arguments".
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-created");
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .arg("accounts")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: switchboard accounts"));
    for args in [["--json", "accounts"], ["accounts", "--json"]] {
        let output = binary()
            .arg("--data-dir")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["ok"], false);
        assert_eq!(
            error["error"],
            "Name a command. Run switchboard accounts --help."
        );
    }
    assert!(!root.exists());
}
#[test]
fn hand_overs_are_listed_and_set_offline_and_run_only_in_a_terminal() {
    // SB-108 / FD-62: Fabric polls `switchboard handovers --json`; the setting is off until set.
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let listed = run(&["handovers"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(json["data"]["enabled"], false);
    assert_eq!(json["data"]["handovers"], serde_json::json!([]));
    let set: serde_json::Value =
        serde_json::from_slice(&run(&["handovers", "set", "on"]).stdout).unwrap();
    assert_eq!(set["data"]["enabled"], true);
    let refused = run(&["handovers", "run", "ho-0123456789ab"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("needs a terminal"));
    let set: serde_json::Value =
        serde_json::from_slice(&run(&["handovers", "set", "off"]).stdout).unwrap();
    assert_eq!(set["data"]["enabled"], false);
}
#[test]
fn project_suggestions_work_from_the_cli_and_the_hook_never_fails() {
    // SB-117/SB-121: a clone of a project repository joins its project at once; one the person
    // then removes, or one declined while it waited, is never added again.
    let tmp = tempfile::tempdir().unwrap();
    // Not the system's temporary folder: folders there never join a project (SB-121).
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let work = work.path().canonicalize().unwrap();
    let git = |dir: &std::path::Path, remote: &str| {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            format!("[remote \"origin\"]\n\turl = {remote}\n"),
        )
        .unwrap();
    };
    let run = |args: &[&str], stdin: Option<&str>| {
        let mut child = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        if let Some(text) = stdin {
            let mut input = input;
            input.write_all(text.as_bytes()).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        let ok = output.status.success();
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
        (
            ok,
            json,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let main = work.join("app");
    git(&main, "git@github.com:org/app.git");
    let (ok, _, err) = run(
        &[
            "project",
            "save",
            "--name",
            "App",
            "--folder",
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(ok, "{err}");
    let copy = work.join("app-review");
    git(&copy, "https://github.com/org/app");
    let (ok, json, err) = run(
        &["project", "suggest", "--folder", copy.to_str().unwrap()],
        None,
    );
    assert!(ok, "{err}");
    assert_eq!(json["data"]["added"], true);
    assert_eq!(json["data"]["suggestion"]["reason"], "same_repository");
    assert_eq!(
        json["data"]["project"]["folders"].as_array().unwrap().len(),
        2
    );
    let (_, listed, _) = run(&["project", "suggestions"], None);
    assert_eq!(
        listed["data"]["suggestions"],
        serde_json::json!([]),
        "nothing waits"
    );
    assert_eq!(
        listed["data"]["added"][0]["project"], "App",
        "for the one notification"
    );
    // The person removes it from the project: it is not added back.
    let (ok, _, err) = run(
        &[
            "project",
            "save",
            "--pool",
            "app",
            "--name",
            "App",
            "--folder",
            main.to_str().unwrap(),
        ],
        None,
    );
    assert!(ok, "{err}");
    let (_, again, _) = run(
        &["project", "suggest", "--folder", copy.to_str().unwrap()],
        None,
    );
    assert_eq!(again["data"]["added"], serde_json::Value::Null);
    assert_eq!(again["data"]["reason"], "declined");
    // A new repository of a source, declined: never suggested again.
    let (ok, _, err) = run(
        &[
            "project",
            "source",
            "--pool",
            "app",
            "--add",
            "github.com/org",
        ],
        None,
    );
    assert!(ok, "{err}");
    let api = work.join("api");
    git(&api, "git@github.com:org/api.git");
    let hook =
        serde_json::json!({"session_id": "s", "cwd": api, "hook_event_name": "SessionStart"});
    // The hook's output joins the session's context: one line, only when something was found.
    let hooked = |input: &str| {
        let mut child = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(["project", "suggest", "--from-hook"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let said = hooked(&hook.to_string());
    assert!(said.contains("added to the group \"App\""), "{said}");
    assert_eq!(said.lines().count(), 1);
    let (_, listed, _) = run(&["project", "suggestions"], None);
    // The clone the person removed is declined now; the new repository is the one added.
    let added = listed["data"]["added"].as_array().unwrap();
    assert_eq!(added.len(), 1, "{listed}");
    assert_eq!(added[0]["reason"], "ecosystem");
    let (_, again, _) = run(
        &["project", "suggest", "--folder", api.to_str().unwrap()],
        None,
    );
    assert_eq!(again["data"]["suggested"], false);
    assert_eq!(again["data"]["reason"], "in_project");
    // The hook never fails a session, and says nothing when nothing was found.
    assert_eq!(hooked("not json"), "");
    assert_eq!(hooked(&hook.to_string()), "", "already in the project");
}
#[test]
fn an_unassigned_repository_stays_in_the_default_group_through_the_session_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let work = work.path().canonicalize().unwrap();
    let run = |args: &[&str]| {
        let output = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
        (
            output.status.success(),
            json,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let hooked = |input: &str| {
        let mut child = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(["project", "suggest", "--from-hook"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let repo = work.join("solo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git/config"), "").unwrap();
    let hook =
        serde_json::json!({"session_id": "s", "cwd": repo, "hook_event_name": "SessionStart"});

    // SB-146: a repository in no group stays in the default group; the hook says nothing.
    assert_eq!(hooked(&hook.to_string()), "");
    let (ok, listed, err) = run(&["project", "list"]);
    assert!(ok, "{err}");
    assert_eq!(
        listed["data"]["projects"].as_array().unwrap().len(),
        0,
        "{listed}"
    );

    let plain_hook =
        serde_json::json!({"session_id": "s", "cwd": "/", "hook_event_name": "SessionStart"});
    assert_eq!(hooked(&plain_hook.to_string()), "");
    let (_, listed, _) = run(&["project", "list"]);
    assert_eq!(listed["data"]["projects"].as_array().unwrap().len(), 0);
}

#[test]
#[cfg(unix)]
fn the_session_start_hook_gives_up_quickly_when_the_owner_does_not_answer() {
    // 2026-10-20: the owner held `ProjectSuggest` behind its transaction and the hook ran into
    // Claude Code's 10 s timeout at 42 of 44 session starts. An owner that accepts the
    // connection and never answers must cost the session at most the hook's own deadline,
    // print nothing and exit 0.
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    // Accept and hold every connection without a byte in reply, as a busy owner does.
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream);
        }
    });
    let control = tmp.path().join("control.json");
    std::fs::write(
        &control,
        serde_json::json!({
            "protocol": 1,
            "address": address.to_string(),
            "token": "a".repeat(64),
            "pid": std::process::id(),
        })
        .to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o600)).unwrap();
    let hook = serde_json::json!({
        "session_id": "s",
        "cwd": tmp.path(),
        "hook_event_name": "SessionStart",
    });
    let started = std::time::Instant::now();
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["project", "suggest", "--from-hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(hook.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let elapsed = started.elapsed();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    // The deadline is 1.5 s; the plugin's hook timeout is 3 s. Without the deadline the
    // client waited its 30 s control timeout.
    assert!(
        elapsed < std::time::Duration::from_millis(2900),
        "the hook took {elapsed:?}"
    );
    assert!(
        elapsed >= std::time::Duration::from_millis(1400),
        "the hook gave up before its deadline: {elapsed:?}"
    );
}

#[test]
fn the_session_start_hook_waits_for_no_input_it_was_not_given() {
    // A host that never closes the hook's stdin must not hold the session either.
    let tmp = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["project", "suggest", "--from-hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    // Bounded here too: before SB-149 this child waited for its input forever.
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > std::time::Duration::from_secs(10) {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("the hook still waited for its input after 10 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let elapsed = started.elapsed();
    let output = child.wait_with_output().unwrap();
    drop(stdin);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(
        elapsed < std::time::Duration::from_millis(2900),
        "the hook took {elapsed:?}"
    );
}

#[test]
fn the_plugin_bounds_its_session_start_hook() {
    let hooks: serde_json::Value = serde_json::from_str(include_str!(
        "../../../plugins/switchboard/hooks/hooks.json"
    ))
    .unwrap();
    let session_start = &hooks["hooks"]["SessionStart"][0]["hooks"][0];
    assert!(session_start["command"]
        .as_str()
        .unwrap()
        .contains("project suggest --from-hook"));
    assert_eq!(session_start["timeout"], 3);
}

#[test]
fn recovery_hook_never_fails_the_session() {
    // SB-115: Claude Code's StopFailure hook runs `switchboard recovery hook`; whatever the
    // input or the owner's state, it exits 0 and says why nothing was registered.
    let tmp = tempfile::tempdir().unwrap();
    let hook = |input: &str| {
        let mut child = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(["--json", "recovery", "hook"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        (
            json["data"]["registered"].clone(),
            json["data"]["reason"].clone(),
        )
    };
    let dir = tmp.path().to_string_lossy().into_owned();
    assert_eq!(hook("not json").1, "malformed_input");
    let overloaded = serde_json::json!({"session_id": "s-1", "cwd": dir, "error": "overloaded"});
    assert_eq!(hook(&overloaded.to_string()).1, "not_a_limit");
    let no_session = serde_json::json!({"cwd": dir, "error": "rate_limit"});
    assert_eq!(hook(&no_session.to_string()).1, "malformed_input");
    // No owner runs in this data folder: refused, still exit 0, nothing written.
    let limit = serde_json::json!({"session_id": "s-1", "cwd": dir, "error": "rate_limit"});
    let (registered, reason) = hook(&limit.to_string());
    assert_eq!(
        (registered, reason),
        (serde_json::json!(false), serde_json::json!("refused"))
    );
    assert!(!tmp.path().join("recovery.json").exists());
}
#[test]
fn the_stop_hook_after_a_turn_is_silent_and_never_fails() {
    // SB-204: the same command runs on `Stop` after every finished turn. Claude Code reads a
    // Stop hook's stdout as its decision, so the answer is an empty object, with or without a
    // stop to settle and with no owner running.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_string_lossy().into_owned();
    let stop = serde_json::json!({"session_id": "s-1", "cwd": dir, "hook_event_name": "Stop",
        "stop_hook_active": false});
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["recovery", "hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stop.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer, serde_json::json!({}));
    assert!(!tmp.path().join("recovery.json").exists());
}
#[test]
fn a_stop_input_past_the_hook_limit_answers_no_decision() {
    // SB-210: a `Stop` input over 8 MiB could not be parsed and the hook printed
    // `malformed_input` where Claude Code reads a decision; it now prints `{}`.
    let tmp = tempfile::tempdir().unwrap();
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["recovery", "hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let message = "x".repeat(8 * 1024 * 1024 + 16);
        let input = serde_json::json!({"session_id": "s-1", "hook_event_name": "Stop",
            "last_assistant_message": message});
        // The hook stops reading at its limit; the rest of the write may fail.
        let _ = stdin.write_all(input.to_string().as_bytes());
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(output.status.success());
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer, serde_json::json!({}));
}
#[test]
fn a_long_last_message_still_settles_the_stop_with_no_owner_running() {
    // Adversarial round 2: Claude Code puts the turn's whole last message in the Stop input; a
    // 64 KiB bound cut it and the stop stayed waiting. With no owner the CLI settles it itself.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let stop = serde_json::json!([{"event": "cc-s-1-100", "session": "s-1", "account": "a-1",
        "provider": "claude", "pool": "default", "directory": dir,
        "stopped_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_secs() - 60,
        "state": "waiting", "selected_account": null}]);
    let file = tmp.path().join("recovery.json");
    std::fs::write(&file, stop.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let input = serde_json::json!({"session_id": "s-1", "cwd": dir, "hook_event_name": "Stop",
        "last_assistant_message": "x".repeat(100 * 1024)});
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["recovery", "hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer, serde_json::json!({}));
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(rows[0]["state"], "resumed", "{rows}");
}
#[test]
fn the_plugin_settles_a_stop_from_the_stop_hook() {
    let hooks: serde_json::Value = serde_json::from_str(include_str!(
        "../../../plugins/switchboard/hooks/hooks.json"
    ))
    .unwrap();
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "switchboard recovery hook 2>/dev/null || true"
    );
}
#[test]
fn continuing_in_place_without_a_terminal_is_refused_before_anything_is_offered() {
    // SB-108: an offer nobody can accept is never made; no owner is even asked.
    let tmp = tempfile::tempdir().unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args([
            "--json",
            "continue",
            "wf_0123456789abcdef",
            "--account",
            "hermes",
            "--dir",
        ])
        .arg(tmp.path())
        .arg("--in-place")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("needs a terminal"));
    assert!(
        std::fs::read_dir(tmp.path()).unwrap().next().is_none(),
        "nothing written"
    );
}
#[test]
fn offline_list_and_status_are_json_and_launch_requires_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["--json", "accounts", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["accounts"], serde_json::json!([]));
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["--json", "login", "finish", "missing"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("switchboard serve"));
}
/// SB-201: a catalog agent launched through the proxy records the route's provider (`claude`)
/// and its own id as `agent`; the session list names the agent, whole, not the route.
#[test]
fn the_session_list_names_a_proxy_agent_and_not_its_route() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["sessions", "--all"]).status.success());
    let home = tmp.path().join("runtimes/agent-deepseek-harness-default");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join(".session.json"),
        serde_json::json!({"account_id": "acct", "provider": "claude",
            "agent": "deepseek-harness", "pool": "default", "context": "personal",
            "mode": "managed", "working_directory": "/w", "in_place": false,
            "launched_at": 1, "continues": null})
        .to_string(),
    )
    .unwrap();
    let output = run(&["sessions", "--all"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|l| l.contains("/w")).unwrap_or_default();
    assert!(
        line.contains(" deepseek-harness ") && !line.contains(" claude "),
        "{text}"
    );
    let output = run(&["--json", "sessions", "--all"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let session = &json["data"]["sessions"][0];
    assert_eq!(session["provider"], "claude");
    assert_eq!(session["agent"], "deepseek-harness");
}
#[test]
fn an_openrouter_launch_runs_without_an_owner_and_needs_a_saved_key() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    // Discovery must not depend on Hermes being installed on the developer's machine.
    // No key is saved, so this placeholder must never be executed.
    let executables = tempfile::tempdir().unwrap();
    let hermes = executables.path().join(if cfg!(windows) {
        "hermes.exe"
    } else {
        "hermes"
    });
    std::fs::write(&hermes, b"synthetic discovery fixture, never executed").unwrap();
    let launch = |extra: &[&str]| {
        binary()
            .env("PATH", executables.path())
            .arg("--data-dir")
            .arg(tmp.path())
            .args(["agents", "launch", "hermes", "--dir"])
            .arg(folder.path())
            .args(extra)
            .output()
            .unwrap()
    };
    // No proxy is involved, so no owner is needed: the refusal names the missing key.
    let output = launch(&["--openrouter", "--model", "moonshotai/kimi-k2"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No OpenRouter key is saved"), "{stderr}");
    // A model belongs to an OpenRouter launch, and a pool to a proxy launch.
    assert_eq!(launch(&["--model", "a/b"]).status.code(), Some(2));
    assert_eq!(
        launch(&["--openrouter", "--pool", "agents"]).status.code(),
        Some(2)
    );
    // Agents whose sessions never read the launch's environment are refused by name.
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["agents", "launch", "openclaw", "--openrouter", "--dir"])
        .arg(folder.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot launch on the OpenRouter key"));
}
#[cfg(not(windows))]
#[test]
fn agents_are_enabled_disabled_and_made_default_offline() {
    // SB-120: `agents status | enable | disable | default` work without an owner; a disabled
    // agent cannot stay the default, and only an agent Switchboard can hand work to can be one.
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".opencode/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("opencode");
    std::fs::write(&program, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        binary()
            .env("HOME", home.path())
            .env("PATH", "/nonexistent")
            .arg("--data-dir")
            .arg(tmp.path())
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let data = |args: &[&str]| {
        let output = run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["data"].clone()
    };
    let status = data(&["agents", "status"]);
    assert_eq!(status["default"], serde_json::Value::Null);
    let opencode = status["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "opencode")
        .unwrap()
        .clone();
    assert_eq!(opencode["installed"], true);
    assert_eq!(opencode["switchable"], true);
    assert_eq!(opencode["enabled"], true);
    assert_eq!(
        status["recommended_models"][0]["id"],
        "xiaomi/mimo-v2.6-pro"
    );
    assert_eq!(
        data(&["agents", "default", "opencode"])["default"],
        "opencode"
    );
    assert_eq!(
        data(&["agents", "status"])["default"],
        "opencode",
        "the default persists"
    );
    // Disabling the default agent returns the machine to Auto.
    let off = data(&["agents", "disable", "opencode"]);
    assert_eq!(off["default"], serde_json::Value::Null);
    let refused = run(&["agents", "default", "opencode"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("disabled for taking over work"));
    data(&["agents", "enable", "opencode"]);
    assert_eq!(
        data(&["agents", "default", "opencode"])["default"],
        "opencode"
    );
    assert_eq!(
        data(&["agents", "default", "auto"])["default"],
        serde_json::Value::Null
    );
    // An agent that can only use the tools, and a name the catalog lacks, are refused.
    let refused = run(&["agents", "default", "goose"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("cannot be the default"));
    let refused = run(&["agents", "enable", "no-such-agent"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("No such agent"));
    // The settings file is private.
    let mode = std::fs::metadata(tmp.path().join("agents-settings.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
#[test]
fn kimi_accounts_list_offline_and_refuse_unknown_ids_without_touching_the_ordinary_home() {
    let tmp = tempfile::tempdir().unwrap();
    let ordinary = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(args)
            .env("KIMI_CODE_HOME", ordinary.path().join("absent"))
            .output()
            .unwrap()
    };
    let output = run(&["--json", "kimi", "list"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["accounts"], serde_json::json!([]));
    assert_eq!(json["data"]["current"], serde_json::Value::Null);
    let output = run(&["kimi", "finish", "not-an-id"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no longer waiting"));
    let output = run(&["kimi", "remove", "00000000-0000-4000-8000-000000000001"]);
    assert!(output.status.success());
    assert_eq!(
        run(&["kimi", "login", "--region", "moon"]).status.code(),
        Some(1)
    );
    assert!(!ordinary.path().join("absent").exists());
}
#[test]
fn oversized_or_malformed_secret_stdin_is_not_echoed() {
    let tmp = tempfile::tempdir().unwrap();
    for input in [b"fixture-secret-not-valid-json".to_vec(), vec![b'x'; 65537]] {
        let mut child = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args([
                "--json",
                "accounts",
                "add",
                "--provider",
                "codex",
                "--kind",
                "oauth",
                "--label",
                "Test",
                "--secret-stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-secret"));
        assert!(output.stderr.len() < 512);
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn separate_cli_process_mutates_live_owner_without_touching_native_vault() {
    let tmp = tempfile::tempdir().unwrap();
    let owner = Owner::start(tmp.path().to_owned(), Arc::new(MemoryVault::default()))
        .await
        .unwrap();
    let mut child = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args([
            "--json",
            "accounts",
            "add",
            "--provider",
            "claude",
            "--kind",
            "api-key",
            "--label",
            "CLI fixture",
            "--pool",
            "team",
            "--secret-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fixture-control-test-secret")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("fixture-control-test-secret"));
    let added: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = added["data"]["id"].as_str().unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args([
            "accounts",
            "select",
            id,
            "--provider",
            "claude",
            "--pool",
            "team",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        owner.runtime.store.snapshot().unwrap().routes["claude:team"],
        id
    );
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["accounts", "remove", id])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(owner.runtime.store.snapshot().unwrap().accounts.len(), 1);
}

#[test]
fn rotation_settings_persist_offline_and_report_stopped_monitor() {
    let tmp = tempfile::tempdir().unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args([
            "--json",
            "rotation",
            "set",
            "--provider",
            "claude",
            "--enabled",
            "true",
            "--threshold",
            "90",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args(["--json", "rotation", "status"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["policies"][0]["enabled"], true);
    assert_eq!(value["data"]["monitor"]["running"], false);
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .args([
            "--json",
            "rotation",
            "set",
            "--provider",
            "codex",
            "--target",
            "claude-cli",
            "--enabled",
            "true",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

fn rotation(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = binary()
        .arg("--data-dir")
        .arg(root)
        .args(["--json", "rotation"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn rotation_set_changes_only_the_given_flags() {
    let tmp = tempfile::tempdir().unwrap();
    rotation(
        tmp.path(),
        &[
            "set",
            "--provider",
            "claude",
            "--enabled",
            "true",
            "--threshold",
            "80",
            "--hysteresis",
            "5",
            "--cooldown",
            "600",
            "--max-age",
            "120",
        ],
    );
    rotation(
        tmp.path(),
        &["set", "--provider", "claude", "--enabled", "false"],
    );
    rotation(
        tmp.path(),
        &["set", "--provider", "claude", "--cooldown", "900"],
    );
    rotation(
        tmp.path(),
        &["set", "--provider", "codex", "--enabled", "true"],
    );
    let status = rotation(tmp.path(), &["status"]);
    let policies = status["data"]["policies"].as_array().unwrap();
    let claude = policies.iter().find(|p| p["provider"] == "claude").unwrap();
    assert_eq!(claude["enabled"], false);
    assert_eq!(claude["threshold_percent"], 80.0);
    assert_eq!(claude["hysteresis_percent"], 5.0);
    assert_eq!(claude["cooldown_seconds"], 900);
    assert_eq!(claude["max_age_seconds"], 120);
    let codex = policies.iter().find(|p| p["provider"] == "codex").unwrap();
    assert_eq!(codex["enabled"], true);
    assert_eq!(codex["threshold_percent"], 90.0);
    assert_eq!(codex["hysteresis_percent"], 10.0);
    assert_eq!(codex["cooldown_seconds"], 1800);
    assert_eq!(codex["max_age_seconds"], 300);
}
#[test]
fn human_usage_and_events_print_utc_times_and_health() {
    use switchboard_core::{AuthKind, Credential, Provider, Store, Usage};
    let tmp = tempfile::tempdir().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    {
        let store = Store::open(tmp.path().to_owned(), Arc::new(MemoryVault::default())).unwrap();
        let account = store
            .add(
                "Fixture".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "fixture-only").unwrap(),
            )
            .unwrap();
        store
            .observe(
                &account.id,
                Usage {
                    windows: vec![],
                    used_percent: 42.0,
                    observed_at: 1_700_000_000,
                    resets_at: None,
                    source: "provider".into(),
                },
            )
            .unwrap();
        store
            .usage_health(&account.id, "failed", now, now + 180)
            .unwrap();
    }
    let run = |args: &[&str]| {
        let output = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    let usage = run(&["usage"]);
    assert!(usage.contains("42.0% used"), "{usage}");
    assert!(usage.contains("observed 2023-11-14T22:13:20Z"), "{usage}");
    assert!(usage.contains("stale"), "{usage}");
    assert!(usage.contains("health failed"), "{usage}");
    assert!(!usage.contains("Unix seconds"), "{usage}");
    let events = run(&["events"]);
    assert!(
        events
            .lines()
            .all(|line| line.contains('T') && line.contains("Z  ")),
        "{events}"
    );
    let json: serde_json::Value = serde_json::from_str(&run(&["--json", "events"])).unwrap();
    assert!(json["data"][0]["at"].is_i64());
}
#[test]
fn offline_backup_never_writes_and_a_scratch_folder_restores_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("backups");
    let output = binary()
        .env("SWITCHBOARD_BACKUP_DIR", &folder)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--json", "backup", "now"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!([output.stdout.clone(), output.stderr.clone()]
        .concat()
        .escape_ascii()
        .to_string()
        .contains("Backups are written by the desktop app or switchboard serve."));
    assert!(!folder.exists(), "nothing was written");
    let output = binary()
        .env("SWITCHBOARD_BACKUP_DIR", &folder)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--json", "backup", "restore", "../accounts.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!([output.stdout.clone(), output.stderr.clone()]
        .concat()
        .escape_ascii()
        .to_string()
        .contains("Backups belong to the default data folder; this data folder has none."));
    // A scratch folder lists none either, even with a backup folder named in the environment
    // (SB-61); a restore's file name is checked in switchboard-core (`another_key_or_a_tampered_file_is_refused`).
    let listed = binary()
        .env("SWITCHBOARD_BACKUP_DIR", &folder)
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["--json", "backup", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert!(listed["data"]["directory"].is_null(), "{listed}");
}

/// Lifecycle LC-01's check for `switchboard serve`: SIGTERM and SIGINT each end an idle owner
/// inside the deadline, with its descriptor removed and the stop logged as codes. The store is
/// empty, so nothing of the real Claude Code or Codex sign-in is read.
#[cfg(unix)]
#[test]
fn serve_stops_on_sigterm_and_sigint_within_the_deadline() {
    use std::time::{Duration, Instant};
    for signal in ["-TERM", "-INT"] {
        let tmp = tempfile::tempdir().unwrap();
        let logs = tmp.path().join("logs");
        let data = tmp.path().join("data");
        let mut child = binary()
            .arg("--data-dir")
            .arg(&data)
            .args(["--json", "serve"])
            .env("SWITCHBOARD_LOG_DIR", &logs)
            .env("SWITCHBOARD_BACKUP_DIR", tmp.path().join("backups"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let descriptor = data.join("control.json");
        let started = Instant::now();
        while !descriptor.exists() {
            assert!(started.elapsed() < Duration::from_secs(20), "serve started");
            std::thread::sleep(Duration::from_millis(50));
        }
        let status = Command::new("/bin/kill")
            .args([signal, &child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
        let asked = Instant::now();
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            assert!(
                asked.elapsed() < Duration::from_secs(5),
                "{signal}: an idle owner exits within five seconds"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(exit.success(), "{signal}: graceful, not killed: {exit:?}");
        assert!(!descriptor.exists(), "{signal}: descriptor removed");
        let mut out = String::new();
        std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut out).unwrap();
        assert!(out.contains("\"stopped\""), "{out}");
        let log = std::fs::read_to_string(logs.join("switchboard.log")).unwrap();
        assert!(log.contains("\"owner_started\""), "{log}");
        assert!(log.contains("\"owner_stopped\""), "{log}");
        assert!(log.contains("\"drained\""), "{log}");
    }
}

#[test]
fn onboard_prints_the_guide_and_reads_the_setup_without_creating_storage() {
    // SB-93: the guide is the repository's docs/AGENT-ONBOARDING.md; the status reads only.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-created");
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .arg("onboard")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("../../../docs/AGENT-ONBOARDING.md")
    );
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .args([
            "--json", "onboard", "--status", "--host", "codex", "--folder",
        ])
        .arg(tmp.path())
        .env("HOME", &home)
        // Windows reads the home from USERPROFILE (SB-103).
        .env("USERPROFILE", &home)
        .env_remove("CODEX_HOME")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let steps = answer["data"]["steps"].as_array().unwrap();
    let states: Vec<(&str, &str)> = steps
        .iter()
        .map(|s| (s["id"].as_str().unwrap(), s["state"].as_str().unwrap()))
        .collect();
    assert_eq!(
        states,
        [
            ("owner", "needs_person"),
            ("accounts", "needs_person"),
            ("project", "optional"),
            ("mcp", "todo"),
            ("proving_call", "todo")
        ]
    );
    assert!(steps[1]["next"]["command"]
        .as_str()
        .unwrap()
        .ends_with("login begin --provider codex"));
    assert!(steps[3]["next"]["command"]
        .as_str()
        .unwrap()
        .starts_with("codex mcp add switchboard -- "));
    assert!(!root.exists(), "reading the setup creates no storage");
    let output = binary()
        .arg("--data-dir")
        .arg(&root)
        .args(["onboard", "--status", "--folder", "relative"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
#[test]
fn onboard_lists_an_organizations_group_seat_and_hook() {
    // SB-202 (B-3): with an organization, `onboard --status` reads its group, its seats, the
    // terminal hook (`shell status`) and the recorded contexts after the five steps.
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = work.path().canonicalize().unwrap().join("acme");
    std::fs::create_dir_all(&folder).unwrap();
    let run = |args: &[&str]| {
        let output = binary()
            .arg("--data-dir")
            .arg(tmp.path())
            .arg("--json")
            .args(args)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env_remove("CODEX_HOME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let folder_text = folder.to_str().unwrap();
    let onboard = || {
        run(&[
            "onboard",
            "--status",
            "--host",
            "claude",
            "--folder",
            folder_text,
        ])
    };
    let ids = |answer: &serde_json::Value| -> Vec<String> {
        answer["data"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let saved = run(&[
        "project",
        "save",
        "--name",
        "Acme work",
        "--folder",
        folder_text,
    ]);
    let pool = saved["data"]["pool"].as_str().unwrap().to_owned();
    // A group and no organization: today's five steps.
    assert_eq!(ids(&onboard()).len(), 5);
    run(&["org", "add", "Acme", "--domain", "acme.com"]);
    run(&["project", "edit", "--pool", &pool, "--organization", "Acme"]);
    let answer = onboard();
    assert_eq!(
        ids(&answer),
        [
            "owner",
            "accounts",
            "project",
            "mcp",
            "proving_call",
            "organization",
            "org_group",
            "seat",
            "hook",
            "launch"
        ]
    );
    let step = |id: &str| {
        answer["data"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(step("org_group")["state"], "done");
    assert_eq!(step("org_group")["organization"]["name"], "Acme");
    let seat = step("seat");
    assert_eq!(seat["state"], "needs_person");
    assert!(seat["next"]["command"]
        .as_str()
        .unwrap()
        .ends_with(&format!("login begin --provider claude --pool {pool}")));
    // No owner has written the folder map: the hook reads as not on yet, from `shell status`.
    let hook = step("hook");
    assert_eq!(hook["state"], "optional");
    assert!(hook["next"]["command"]
        .as_str()
        .unwrap()
        .ends_with("shell status"));
    assert_eq!(step("launch")["state"], "optional");
    assert_eq!(answer["data"]["complete"], false);
}
#[test]
fn project_bindings_are_listed_offline() {
    // SB-118 REQ-5: `switchboard project bindings` lists the bound folders; the record's
    // digest stays in the record.
    let tmp = tempfile::tempdir().unwrap();
    let rows = serde_json::json!([{
        "folder": "/work/app",
        "pool": "pc",
        "base_url": "http://127.0.0.1:4000/claude/pc",
        "token_sha256": "00",
        "created": true
    }]);
    switchboard_core::private_fs::private_write(
        &tmp.path().join("claude-bindings.json"),
        &serde_json::to_vec(&rows).unwrap(),
    )
    .unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(tmp.path())
        .arg("--json")
        .args(["project", "bindings"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["bindings"][0]["folder"], "/work/app");
    assert_eq!(
        json["data"]["bindings"][0]["base_url"],
        "http://127.0.0.1:4000/claude/pc"
    );
    assert!(json["data"]["bindings"][0].get("token_sha256").is_none());
}
/// SB-212: a synthetic launch record, 0600 on Unix as a launch writes it. On Windows it keeps
/// the access list it inherits, which history still reads (and narrows), as after a copied folder.
fn plant_launch(root: &std::path::Path, id: &str, extra: serde_json::Value) {
    let dir = root.join("launches");
    std::fs::create_dir_all(&dir).unwrap();
    let mut record = serde_json::json!({"launch_id": id, "account_id": "a-1",
        "provider": "claude", "pool": "default", "launched_at": 100,
        "home_path": "runtimes/claude-default",
        "launched_by": {"surface": "cli", "caller_launch_id": null, "caller_project": null,
            "caller_program": "zsh", "by": null, "parent": null}});
    for (k, v) in extra.as_object().unwrap() {
        record[k] = v.clone();
    }
    let file = dir.join(format!("{id}.json"));
    std::fs::write(&file, record.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn read_launch(root: &std::path::Path, id: &str) -> serde_json::Value {
    serde_json::from_slice(
        &std::fs::read(root.join("launches").join(format!("{id}.json"))).unwrap(),
    )
    .unwrap()
}
/// Runs a hook command with `input` on stdin and, when given, the session's launch id.
fn hook_run(root: &std::path::Path, args: &[&str], input: &[u8], launch: Option<&str>) -> String {
    let mut command = binary();
    command
        .arg("--data-dir")
        .arg(root)
        .args(args)
        .env_remove("SWITCHBOARD_LAUNCH_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(id) = launch {
        command.env("SWITCHBOARD_LAUNCH_ID", id);
    }
    let mut child = command.spawn().unwrap();
    // A host may close the pipe early; the hook still ends well.
    let _ = child.stdin.take().unwrap().write_all(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
#[test]
fn a_launched_sessions_hooks_record_its_transcript_turns_and_stop_and_nothing_else_does() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let id = "5b7f2c1e-0000-4000-8000-000000000001";
    plant_launch(root, id, serde_json::json!({}));
    let dir = root.to_string_lossy().into_owned();
    // Absolute on this platform, as Claude Code reports it: `C:\...` on Windows, where a path
    // without a drive (`/synthetic/...`) is not absolute and is rightly not recorded.
    let transcript = root.join("projects").join("x").join("0f3c.jsonl");
    let transcript = transcript.to_string_lossy().into_owned();
    let start = serde_json::json!({"session_id": "0f3c", "transcript_path": transcript,
        "cwd": dir, "hook_event_name": "SessionStart"});
    let suggest = ["project", "suggest", "--from-hook"];
    // Without the variable: an ordinary session, nothing is written.
    hook_run(root, &suggest, start.to_string().as_bytes(), None);
    assert!(read_launch(root, id)["transcript"].is_null());
    hook_run(root, &suggest, start.to_string().as_bytes(), Some(id));
    let record = read_launch(root, id);
    assert_eq!(record["provider_session_id"], "0f3c");
    assert_eq!(record["transcript"], transcript);
    let stop = serde_json::json!({"session_id": "0f3c", "cwd": dir, "hook_event_name": "Stop"});
    for _ in 0..2 {
        let said = hook_run(
            root,
            &["recovery", "hook"],
            stop.to_string().as_bytes(),
            Some(id),
        );
        assert_eq!(
            said.trim(),
            "{}",
            "a Stop hook's answer stays an empty object"
        );
    }
    assert_eq!(read_launch(root, id)["turns"], 2);
    let limit = serde_json::json!({"session_id": "0f3c", "cwd": dir, "error": "rate_limit",
        "hook_event_name": "StopFailure"});
    hook_run(
        root,
        &["--json", "recovery", "hook"],
        limit.to_string().as_bytes(),
        Some(id),
    );
    assert_eq!(read_launch(root, id)["ended"], "limit");
    // Garbage, an oversized input and a planted id never fail a session or write anything.
    let before = read_launch(root, id);
    hook_run(root, &suggest, b"not json", Some(id));
    hook_run(
        root,
        &["recovery", "hook"],
        &vec![b'x'; 9 * 1024 * 1024],
        Some(id),
    );
    hook_run(
        root,
        &["recovery", "hook"],
        stop.to_string().as_bytes(),
        Some("../../escape"),
    );
    assert_eq!(read_launch(root, id), before);
    assert!(!root.join("escape.json").exists());
}
#[test]
fn the_history_and_one_launch_with_its_chain_read_from_the_command_line() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let (a, b, c) = (
        "5b7f2c1e-0000-4000-8000-00000000000a",
        "5b7f2c1e-0000-4000-8000-00000000000b",
        "5b7f2c1e-0000-4000-8000-00000000000c",
    );
    plant_launch(root, a, serde_json::json!({"launched_at": 100}));
    plant_launch(
        root,
        b,
        serde_json::json!({"launched_at": 200, "launched_by": {"surface": "cli",
            "caller_launch_id": a, "by": "voc-learn"}}),
    );
    plant_launch(
        root,
        c,
        serde_json::json!({"launched_at": 300, "launched_by": {"surface": "cli",
            "caller_launch_id": b}}),
    );
    let run = |args: &[&str]| {
        let output = binary()
            .arg("--data-dir")
            .arg(root)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let listed: serde_json::Value =
        serde_json::from_str(&run(&["--json", "sessions", "--history"])).unwrap();
    let ids: Vec<&str> = listed["data"]["launches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["launch_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [c, b, a], "newest first");
    let text = run(&["sessions", "--history"]);
    assert!(text.contains("cli:voc-learn"), "{text}");
    let shown: serde_json::Value =
        serde_json::from_str(&run(&["--json", "sessions", "show", b])).unwrap();
    assert_eq!(shown["data"]["launch"]["launch_id"], b);
    assert_eq!(shown["data"]["up"][0]["launch_id"], a);
    assert_eq!(shown["data"]["down"][0]["launch_id"], c);
    let text = run(&["sessions", "show", b]);
    assert!(
        text.contains("Launched by:") && text.contains("Then:"),
        "{text}"
    );
}
#[test]
fn a_launcher_name_or_parent_that_is_not_one_is_refused_before_anything_is_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-created");
    for (flag, value) in [
        ("--by", "two words"),
        ("--by", "x".repeat(65).as_str()),
        ("--parent", "../escape"),
    ] {
        let output = binary()
            .arg("--data-dir")
            .arg(&root)
            .args(["launch", "acct", "--working-directory", "/tmp", flag, value])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{flag} {value}");
        let said = String::from_utf8_lossy(&output.stderr);
        assert!(
            said.contains("letters, digits") || said.contains("as the parent"),
            "{said}"
        );
        assert!(!root.exists());
    }
}
