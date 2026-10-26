//! `switchboard shell env` and `switchboard shell status` (SB-183, node A-1): they read the
//! folder map with no store lock and no owner, answer within the hook's budget and never break
//! a terminal.
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use switchboard_runtime::shell::map::{self, Map, Row};

fn binary() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_switchboard"));
    // The test's own terminal may carry a hook or a launched session.
    command
        .env_remove("SWITCHBOARD_SHELL_HOOK")
        .env_remove("SWITCHBOARD_SESSION")
        .stdin(Stdio::null());
    command
}
fn private_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}
/// A group folder (canonical, so the answer does not depend on links in the temp path).
fn work() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let acme = tmp.path().canonicalize().unwrap().join("acme");
    std::fs::create_dir_all(acme.join("app")).unwrap();
    (tmp, acme)
}
fn write_map(root: &Path, folder: &Path) {
    map::write(
        root,
        &Map {
            version: map::VERSION,
            rows: vec![Row {
                folder: folder.to_string_lossy().into_owned(),
                pool: "acme".into(),
                context: "org:o-acme".into(),
                claude: true,
                codex: false,
            }],
        },
    )
    .unwrap();
}
fn env(root: &Path, dir: &Path) -> Output {
    binary()
        .arg("--data-dir")
        .arg(root)
        .args(["shell", "env", "--shell", "zsh", "--dir"])
        .arg(dir)
        .output()
        .unwrap()
}
fn line(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn expected(root: &Path) -> String {
    format!(
        "acme\torg:o-acme\t{}\t-\n",
        root.join("shell").join("claude-acme").display()
    )
}

#[test]
fn shell_env_answers_without_the_owner_or_the_store_lock_within_25_ms() {
    let data = private_dir();
    let root = data.path();
    let (_work, acme) = work();
    write_map(root, &acme);
    // The store is locked, as a running owner holds it ...
    let lock = std::fs::File::create(root.join("instance.lock")).unwrap();
    lock.lock().unwrap();
    // ... and the owner's control port accepts and never answers, as a stalled owner did for
    // up to 38 s on 0.6.82.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream);
        }
    });
    let control = root.join("control.json");
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let dir = acme.join("app");
    // The first call pays for loading the binary from disk; a terminal's hook runs a warm one.
    assert_eq!(line(&env(root, &dir)), expected(root));
    let mut times: Vec<Duration> = (0..7)
        .map(|_| {
            let started = Instant::now();
            let output = env(root, &dir);
            let elapsed = started.elapsed();
            assert!(output.status.success());
            assert!(output.stderr.is_empty());
            assert_eq!(line(&output), expected(root));
            elapsed
        })
        .collect();
    times.sort();
    // The command's own cost is the fastest warm call: the others add only what the machine's
    // scheduler does to any process (a busy build machine, a slower CI runner).
    assert!(
        times[0] <= Duration::from_millis(25),
        "shell env took {:?} at best (all: {times:?})",
        times[0]
    );
    drop(lock);
}

#[test]
fn shell_env_says_none_when_off_missing_unknown_version_or_outside() {
    let data = private_dir();
    let root = data.path();
    let (work, acme) = work();
    let dir = acme.join("app");
    // No map yet: the owner has not written one.
    assert_eq!(line(&env(root, &dir)), "none\n");
    write_map(root, &acme);
    assert_eq!(line(&env(root, &dir)), expected(root));
    // Outside every organization's group.
    assert_eq!(line(&env(root, work.path())), "none\n");
    assert_eq!(
        line(&env(root, &acme.with_file_name("acme-other"))),
        "none\n"
    );
    // Turned off: the person's choice wins over the map.
    std::fs::write(root.join("shell-hook"), "off\n").unwrap();
    assert_eq!(line(&env(root, &dir)), "none\n");
    std::fs::write(root.join("shell-hook"), "on\n").unwrap();
    assert_eq!(line(&env(root, &dir)), expected(root));
    std::fs::remove_file(root.join("shell-hook")).unwrap();
    // A map of an unknown version, a broken one and an oversized one.
    let path = root.join("shell").join("map.json");
    let rows = serde_json::json!([{
        "folder": acme, "pool": "acme", "context": "org:o-acme", "claude": true, "codex": false
    }]);
    std::fs::write(
        &path,
        serde_json::json!({"version": 2, "rows": rows}).to_string(),
    )
    .unwrap();
    assert_eq!(line(&env(root, &dir)), "none\n");
    std::fs::write(&path, "{\"version\": 1, \"rows\": [").unwrap();
    assert_eq!(line(&env(root, &dir)), "none\n");
    let mut big = serde_json::json!({"version": 1, "rows": rows}).to_string();
    big.push_str(&" ".repeat(1024 * 1024));
    std::fs::write(&path, big).unwrap();
    assert_eq!(line(&env(root, &dir)), "none\n");
    // Back to a good map: answered again (the file is read on every call, nothing is cached).
    write_map(root, &acme);
    assert_eq!(line(&env(root, &dir)), expected(root));
}

#[test]
fn shell_env_never_writes_to_stderr_and_exits_zero() {
    let data = private_dir();
    let root = data.path();
    let (_work, acme) = work();
    write_map(root, &acme);
    let dir = acme.join("app");
    let dir = dir.to_str().unwrap();
    let missing = data.path().join("not-created");
    let missing = missing.to_str().unwrap();
    let root = root.to_str().unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec!["--data-dir", root, "shell", "env", "--shell", "zsh"],
        vec![
            "--data-dir",
            root,
            "shell",
            "env",
            "--shell",
            "zsh",
            "--dir",
            "relative/app",
        ],
        vec![
            "--data-dir",
            root,
            "shell",
            "env",
            "--shell",
            "tcsh",
            "--dir",
            dir,
        ],
        vec![
            "--data-dir",
            root,
            "shell",
            "env",
            "--dir",
            dir,
            "--unknown",
        ],
        vec!["--data-dir", root, "shell", "env", "--dir", dir, "extra"],
        vec!["--data-dir", root, "shell", "env", "--dir"],
        vec!["--data-dir", "relative", "shell", "env", "--dir", dir],
        vec![
            "--data-dir",
            missing,
            "shell",
            "env",
            "--shell",
            "bash",
            "--dir",
            dir,
        ],
        vec![
            "--data-dir",
            root,
            "shell",
            "env",
            "--dir",
            "/no/such/folder",
        ],
    ];
    for args in &cases {
        let output = binary().args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {:?}", output.stderr);
        assert_eq!(line(&output), "none\n", "{args:?}");
    }
    assert!(!Path::new(missing).exists(), "nothing is created");
    // The same answers with the global flags elsewhere, `=` forms and --json.
    for args in [
        vec![
            "shell",
            "env",
            "--data-dir",
            root,
            "--dir",
            dir,
            "--shell",
            "fish",
        ],
        vec![
            "--json",
            "shell",
            "env",
            &format!("--data-dir={root}"),
            &format!("--dir={dir}"),
        ],
        vec![
            "shell",
            "--data-dir",
            root,
            "env",
            "--shell=pwsh",
            "--dir",
            dir,
        ],
    ] {
        let output = binary().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        assert!(output.stderr.is_empty(), "{args:?}");
        assert_eq!(line(&output), expected(Path::new(root)), "{args:?}");
    }
    // A map that is a folder, and a data folder that is a file.
    std::fs::remove_file(Path::new(root).join("shell").join("map.json")).unwrap();
    std::fs::create_dir(Path::new(root).join("shell").join("map.json")).unwrap();
    let output = binary()
        .args(["--data-dir", root, "shell", "env", "--dir", dir])
        .output()
        .unwrap();
    assert_eq!(
        (output.status.code(), line(&output).as_str()),
        (Some(0), "none\n")
    );
    assert!(output.stderr.is_empty());
    let file = data.path().join("a-file");
    std::fs::write(&file, "x").unwrap();
    let output = binary()
        .args([
            "--data-dir",
            file.to_str().unwrap(),
            "shell",
            "env",
            "--dir",
            dir,
        ])
        .output()
        .unwrap();
    assert_eq!(
        (output.status.code(), line(&output).as_str()),
        (Some(0), "none\n")
    );
    assert!(output.stderr.is_empty());
    // Help is still help.
    let output = binary().args(["shell", "env", "--help"]).output().unwrap();
    assert!(output.status.success());
    assert!(line(&output).contains("--dir"));
}

#[test]
fn shell_status_reads_the_map_and_the_record_without_the_owner_and_never_fails() {
    let data = private_dir();
    let root = data.path();
    let (_work, acme) = work();
    let status = |dir: &Path, hooked: bool| {
        let mut command = binary();
        if hooked {
            command.env("SWITCHBOARD_SHELL_HOOK", "1");
        }
        let output = command
            .arg("--data-dir")
            .arg(root)
            .args(["--json", "shell", "status", "--dir"])
            .arg(dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ok"], true);
        value["data"].clone()
    };
    // Nothing written yet: default, and off because no group belongs to an organization.
    let value = status(&acme, false);
    assert_eq!(value["hook"], "default_off");
    assert_eq!(value["map"], "missing");
    assert_eq!(value["terminal"]["hook_loaded"], false);
    assert!(value["here"].is_null());
    assert_eq!(value["files"], serde_json::json!([]));
    // The map and the installer's record written; the store locked by an owner.
    write_map(root, &acme);
    std::fs::write(
        root.join("shell-hook.json"),
        serde_json::json!({"version": 1, "files": [
            {"shell": "zsh", "path": "/home/p/.zshrc", "state": "written", "digest": "x"},
            {"shell": "bash", "path": "/home/p/.bash_profile", "state": "skipped_symlink"},
            {"shell": 7}
        ]})
        .to_string(),
    )
    .unwrap();
    let lock = std::fs::File::create(root.join("instance.lock")).unwrap();
    lock.lock().unwrap();
    let value = status(&acme.join("app"), true);
    assert_eq!(value["hook"], "default_on");
    assert_eq!(value["map"], "ok");
    assert_eq!(value["terminal"]["hook_loaded"], true);
    assert_eq!(value["here"]["group"], "acme");
    assert_eq!(value["here"]["context"], "org:o-acme");
    assert_eq!(
        value["here"]["claude_home"],
        root.join("shell")
            .join("claude-acme")
            .to_string_lossy()
            .as_ref()
    );
    assert!(value["here"]["codex_home"].is_null());
    assert_eq!(
        value["files"],
        serde_json::json!([
            {"shell": "zsh", "path": "/home/p/.zshrc", "state": "written"},
            {"shell": "bash", "path": "/home/p/.bash_profile", "state": "skipped_symlink"}
        ])
    );
    std::fs::write(root.join("shell-hook"), "off").unwrap();
    let value = status(&acme.join("app"), true);
    assert_eq!(value["hook"], "off");
    assert!(value["here"].is_null());
    // The human form names the group, the homes and each start file.
    std::fs::remove_file(root.join("shell-hook")).unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(root)
        .args(["shell", "status", "--dir"])
        .arg(acme.join("app"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = line(&output);
    assert!(text.contains("acme"), "{text}");
    assert!(text.contains("org:o-acme"), "{text}");
    assert!(
        text.contains("your own sign-in (this group has no Codex account)"),
        "{text}"
    );
    assert!(text.contains("/home/p/.zshrc"), "{text}");
    // A broken record and a data folder that does not exist still answer.
    std::fs::write(root.join("shell-hook.json"), "{").unwrap();
    assert_eq!(status(&acme, false)["files"], serde_json::json!([]));
    let missing = data.path().join("not-created");
    let output = binary()
        .arg("--data-dir")
        .arg(&missing)
        .args(["--json", "shell", "status"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!missing.exists());
    drop(lock);
}
