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
#[test]
fn an_openrouter_launch_runs_without_an_owner_and_needs_a_saved_key() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let launch = |extra: &[&str]| {
        binary()
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
