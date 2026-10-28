//! `switchboard shell env` and `switchboard shell status` (SB-183, node A-1): they read the
//! folder map with no store lock and no owner, answer within the hook's budget and never break
//! a terminal.
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use switchboard_runtime::shell::map::{self, Map, Row};
use switchboard_runtime::shell::scripts;

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
    let dir = acme.clone();
    // The first call pays for loading the binary from disk; a terminal's hook runs a warm one.
    assert_eq!(line(&env(root, &dir)), expected(root));
    // Below the bound folder Claude Code would not read the route (measured): a group with
    // only Claude has nothing to export there.
    assert_eq!(line(&env(root, &acme.join("app"))), "none\n");
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
    // What starting this binary costs before it does anything: the fastest of seven warm
    // `--version` calls. On Windows creating a process alone takes about 20 ms (26.3 ms at best
    // for `shell env` on the arm64 runner of nightly run 40627045056), so there the budget is
    // the command's own cost above that start.
    let mut starts: Vec<Duration> = (0..7)
        .map(|_| {
            let started = Instant::now();
            let output = binary().arg("--version").output().unwrap();
            let elapsed = started.elapsed();
            assert!(output.status.success());
            elapsed
        })
        .collect();
    starts.sort();
    // The command's own cost is the fastest warm call: the others add only what the machine's
    // scheduler does to any process (a busy build machine, a slower CI runner).
    let budget = Duration::from_millis(25);
    let own = if cfg!(windows) {
        times[0].saturating_sub(starts[0])
    } else {
        times[0]
    };
    assert!(
        own <= budget,
        "shell env took {:?} at best, a bare start {:?} (all: {times:?}; starts: {starts:?})",
        times[0],
        starts[0]
    );
    drop(lock);
}

#[test]
fn shell_env_says_none_when_off_missing_unknown_version_or_outside() {
    let data = private_dir();
    let root = data.path();
    let (work, acme) = work();
    let dir = acme.clone();
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
    let dir = acme.clone();
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
    let value = status(&acme, true);
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
    let value = status(&acme, true);
    assert_eq!(value["hook"], "off");
    assert!(value["here"].is_null());
    // The human form names the group, the homes and each start file.
    std::fs::remove_file(root.join("shell-hook")).unwrap();
    let output = binary()
        .arg("--data-dir")
        .arg(root)
        .args(["shell", "status", "--dir"])
        .arg(&acme)
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

/// SB-183 node A-2: the script `shell init` prints, loaded by a real zsh and bash, asks this
/// very executable on every change of folder and moves the variables into the group's homes
/// and back.
#[cfg(unix)]
#[test]
fn shell_init_prints_a_hook_that_follows_the_folder_map_with_the_real_cli() {
    let data = private_dir();
    let root = data.path().canonicalize().unwrap();
    let (_work, acme) = work();
    write_map(&root, &acme);
    let elsewhere = acme.parent().unwrap().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    for (shell, prompt) in [("zsh", ":"), ("bash", "eval \"$PROMPT_COMMAND\"")] {
        let path = format!("/bin/{shell}");
        if !Path::new(&path).exists() {
            eprintln!("skipped: {shell} is not installed on this machine");
            continue;
        }
        let printed = binary()
            .arg("--data-dir")
            .arg(&root)
            .args(["shell", "init", shell])
            .output()
            .unwrap();
        assert!(printed.status.success());
        let script = String::from_utf8(printed.stdout).unwrap();
        assert!(
            script.contains(env!("CARGO_BIN_EXE_switchboard")) || script.contains("switchboard")
        );
        let hook = root.join(format!("hook-test.{shell}"));
        std::fs::write(&hook, &script).unwrap();
        let body = format!(
            ". '{}'\ncd '{}'\n{prompt}\necho \"in=${{CLAUDE_CONFIG_DIR-unset}} ctx=${{SWITCHBOARD_CONTEXT-unset}}\"\ncd '{}'\n{prompt}\necho \"out=${{CLAUDE_CONFIG_DIR-unset}} ctx=${{SWITCHBOARD_CONTEXT-unset}}\"",
            hook.display(),
            acme.display(),
            elsewhere.display()
        );
        let mut command = Command::new(&path);
        if shell == "zsh" {
            command.args(["-f", "-c", &body]);
        } else {
            command.args(["--noprofile", "--norc", "-c", &body]);
        }
        let out = command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CONFIG_DIR", "/own home")
            .current_dir(&elsewhere)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains(&format!(
                "in={} ctx=org:o-acme",
                root.join("shell").join("claude-acme").display()
            )),
            "{shell}: {text}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("out=/own home ctx=unset"), "{shell}: {text}");
        assert!(
            out.stderr.is_empty(),
            "{shell}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // --json answers the script as data.
    let json = binary()
        .arg("--data-dir")
        .arg(&root)
        .args(["--json", "shell", "init", "fish"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["data"]["shell"], "fish");
    assert!(value["data"]["script"]
        .as_str()
        .unwrap()
        .contains("--on-variable PWD"));
}

/// The app's own data folder under `home` (`dirs::data_dir()` follows `HOME`).
#[cfg(unix)]
fn default_root_in(home: &Path) -> std::path::PathBuf {
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        home.join(".local/share")
    };
    let root = base.join(switchboard_runtime::APP_ID);
    std::fs::create_dir_all(&root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    root
}
/// A temporary home with an installed CLI in `~/.local/bin` (a stand-in: never run here).
#[cfg(unix)]
fn temporary_home() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    let installed = home.join(".local/bin/switchboard");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::write(&installed, "#!/bin/sh\necho none\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
    (tmp, home, installed)
}
/// The CLI in `home`, as a person's terminal there would run it.
#[cfg(unix)]
fn in_home(home: &Path) -> Command {
    let mut command = binary();
    command
        .env("HOME", home)
        .env("SHELL", "/bin/zsh")
        .env("PATH", "/usr/bin:/bin")
        .env_remove("ZDOTDIR")
        .env_remove("XDG_DATA_HOME");
    command
}

/// SB-183 node A-3: `shell on` and `shell off` edit the start files of the home they are given
/// (here a temporary HOME, with the app's own data folder in it), and `shell off` gives each one
/// back byte for byte. A development build names the installed CLI, never itself.
#[cfg(unix)]
#[test]
fn shell_on_and_off_write_and_take_out_the_block_in_the_given_home_only() {
    let (_tmp, home, installed) = temporary_home();
    let root = default_root_in(&home);
    let original = "export MINE=1\nalias ll='ls -l'";
    std::fs::write(home.join(".zshrc"), original).unwrap();
    std::fs::create_dir_all(home.join(".config/fish")).unwrap();
    let run = |args: &[&str]| in_home(&home).args(args).output().unwrap();
    let on = run(&["--json", "shell", "on"]);
    assert!(
        on.status.success(),
        "{}",
        String::from_utf8_lossy(&on.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&on.stdout).unwrap();
    assert_eq!(value["data"]["hook"], "on");
    let files = value["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{value}");
    assert!(files.iter().all(|f| f["state"] == "written"));
    let zshrc = std::fs::read_to_string(home.join(".zshrc")).unwrap();
    assert!(zshrc.starts_with(original));
    assert!(zshrc.contains("# >>> Fabric Switchboard: shell hook (SB-183) >>>"));
    let hook = std::fs::read_to_string(root.join("shell").join("hook.zsh")).unwrap();
    assert!(
        hook.contains(&format!("_switchboard_cli='{}'", installed.display())),
        "the installed CLI, not this development build: {hook}"
    );
    assert!(!hook.contains(env!("CARGO_BIN_EXE_switchboard")));
    let fish = home.join(".config/fish/conf.d/fabric-switchboard.fish");
    assert!(fish.is_file());
    // The human answer names each file.
    let text = run(&["shell", "status"]);
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("Terminal hook: on."), "{text}");
    assert!(text.contains(".zshrc, added"), "{text}");
    let off = run(&["shell", "off"]);
    assert!(off.status.success());
    assert_eq!(
        std::fs::read(home.join(".zshrc")).unwrap(),
        original.as_bytes()
    );
    assert!(!fish.exists());
    assert_eq!(
        std::fs::read_to_string(root.join("shell-hook")).unwrap(),
        "off\n"
    );
    assert!(String::from_utf8(off.stdout)
        .unwrap()
        .contains("Terminal hook: off."));
}

/// SB-183 fix cycle 1: every data folder would write into the same home under one marker, so
/// only the app's own data folder changes start files. Another `--data-dir` is refused with a
/// sentence and never takes out the block the app's folder wrote.
#[cfg(unix)]
#[test]
fn another_data_folder_never_touches_the_start_files() {
    let (_tmp, home, _installed) = temporary_home();
    let root = default_root_in(&home);
    std::fs::write(home.join(".zshrc"), "own\n").unwrap();
    let on = in_home(&home).args(["shell", "on"]).output().unwrap();
    assert!(
        on.status.success(),
        "{}",
        String::from_utf8_lossy(&on.stderr)
    );
    let zshrc = std::fs::read(home.join(".zshrc")).unwrap();
    let record = std::fs::read(root.join("shell-hook.json")).unwrap();
    assert!(String::from_utf8_lossy(&zshrc).contains("Fabric Switchboard: shell hook"));
    let other = private_dir();
    for args in [["shell", "off"], ["shell", "on"]] {
        let out = in_home(&home)
            .arg("--data-dir")
            .arg(other.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?} was refused");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("Only Switchboard's own data folder"),
            "{said}"
        );
        assert!(!other.path().join("shell-hook").exists(), "no choice saved");
    }
    // `uninstall` of another data folder (Linux: on macOS it would also remove the real
    // Keychain's orphan item, so the runtime's `uninstall::tests` cover it there).
    #[cfg(target_os = "linux")]
    {
        let out = in_home(&home)
            .arg("--data-dir")
            .arg(other.path())
            .args(["uninstall", "--yes"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(std::fs::read(home.join(".zshrc")).unwrap(), zshrc);
    assert_eq!(std::fs::read(root.join("shell-hook.json")).unwrap(), record);
}

/// SB-183 node A-8: `shell init pwsh` prints the PowerShell hook, naming this executable and the
/// data folder, and `shell env --shell pwsh` answers it as it answers the other shells.
#[test]
fn shell_init_pwsh_prints_the_powershell_hook_naming_this_cli() {
    let data = private_dir();
    let root = data.path().canonicalize().unwrap();
    let (_work, acme) = work();
    write_map(&root, &acme);
    let printed = binary()
        .arg("--data-dir")
        .arg(&root)
        .args(["shell", "init", "pwsh"])
        .output()
        .unwrap();
    assert!(printed.status.success());
    let script = String::from_utf8(printed.stdout).unwrap();
    let exe = Path::new(env!("CARGO_BIN_EXE_switchboard"))
        .canonicalize()
        .unwrap();
    // The paths as base64 of their UTF-8, decoded as the script runs (SB-225): Windows
    // PowerShell 5.1 reads a command's output in the console's code page, so what it reads must
    // be ASCII.
    // Decoding it through PowerShell is `windows::tests::the_ps1_transition_matches_the_posix_one`.
    let encoded = |p: &Path| scripts::ps_base64(&p.to_string_lossy());
    assert!(
        script.contains(&format!("$global:_switchboard_cli = {}", encoded(&exe))),
        "{script}"
    );
    assert!(script.contains(&format!("$global:_switchboard_data = {}", encoded(&root))));
    assert!(script.is_ascii(), "{script}");
    assert!(script.contains("function global:prompt"));
    // Printed for `| Out-String | Invoke-Expression`: no byte order mark in front.
    assert!(script.starts_with("# Fabric Switchboard: shell hook for PowerShell"));
    let answer = binary()
        .arg("--data-dir")
        .arg(&root)
        .args(["shell", "env", "--shell", "pwsh", "--dir"])
        .arg(&acme)
        .output()
        .unwrap();
    assert!(answer.status.success());
    assert!(answer.stderr.is_empty());
    let line = String::from_utf8(answer.stdout).unwrap();
    assert!(line.starts_with("acme\torg:"), "{line}");
}
