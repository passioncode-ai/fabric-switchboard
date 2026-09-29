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
