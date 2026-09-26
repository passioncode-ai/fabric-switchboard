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
