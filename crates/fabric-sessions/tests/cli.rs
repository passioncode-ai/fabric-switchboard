//! The binary names itself and refuses what it does not know yet.
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fabric-sessions"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn it_names_its_own_version_and_refuses_an_unknown_argument() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("fabric-sessions {}", env!("CARGO_PKG_VERSION"))
    );
    let out = run(&["--help"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage: fabric-sessions"));
    let out = run(&["serve"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}
