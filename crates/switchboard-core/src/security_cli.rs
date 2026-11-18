//! Generic-password access through Apple's stable `/usr/bin/security`, the way Claude Code
//! and Claude Swap reach the login Keychain. An item created by this binary trusts it, so
//! every later read or update is silent — for the desktop app, the CLI and every rebuild.
//! Secrets never enter argv, stderr, logs or errors: a write is one `security -i` command line
//! on stdin, the value as hex, or quoted text when hex would not fit the line. A value that
//! fits neither is refused ([`TOO_LARGE`]); it is never passed as an argument, where every
//! local user could read it (SB-420).
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SECURITY: &str = "/usr/bin/security";
const LIMIT: usize = 64 * 1024;
/// `security -i` reads each command with a 4096-byte fgets(); 64 bytes of headroom.
const STDIN_LINE_LIMIT: usize = 4096 - 64;
const TIMEOUT: Duration = Duration::from_secs(5);
const NOT_FOUND: i32 = 44;
const DUPLICATE: i32 = 45;
pub const UNAVAILABLE: &str = "Keychain unavailable. Unlock it and allow access, then retry.";
/// A value too large for one `security -i` line in either form. Nothing was written.
pub const TOO_LARGE: &str = "The Keychain item is too large to write safely. Nothing was changed.";

#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    Stored,
    /// `update == false` and the item already exists; nothing was changed.
    Exists,
}

/// The secret value of one item, or None when it does not exist.
pub fn find(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    names_valid(service, account)?;
    let (code, mut bytes) = run(
        Command::new(SECURITY).args(["find-generic-password", "-a", account, "-w", "-s", service]),
        None,
        TIMEOUT,
    )?;
    match code {
        0 => {
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            Ok(Some(bytes))
        }
        NOT_FOUND => Ok(None),
        _ => Err(UNAVAILABLE.into()),
    }
}

/// Attribute-only lookup: nothing is decrypted, so it cannot show a consent dialog.
pub fn exists(service: &str, account: &str) -> Result<bool, String> {
    names_valid(service, account)?;
    let (code, _) = run(
        Command::new(SECURITY).args(["find-generic-password", "-a", account, "-s", service]),
        None,
        TIMEOUT,
    )?;
    match code {
        0 => Ok(true),
        NOT_FOUND => Ok(false),
        _ => Err(UNAVAILABLE.into()),
    }
}

pub fn add(service: &str, account: &str, secret: &[u8], update: bool) -> Result<Added, String> {
    add_via(SECURITY, None, service, account, secret, update)
}

/// [`add`] through `program` (the security tool; a fake one in tests) into `keychain` (None:
/// the default keychain). The only argument is `-i`: the value travels on stdin.
pub(crate) fn add_via(
    program: &str,
    keychain: Option<&Path>,
    service: &str,
    account: &str,
    secret: &[u8],
    update: bool,
) -> Result<Added, String> {
    names_valid(service, account)?;
    if secret.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    if secret.len() > LIMIT {
        return Err(TOO_LARGE.into());
    }
    let line = add_line(service, account, secret, update, keychain)?;
    let (code, _) = run(
        Command::new(program).args(ADD_ARGS),
        Some(line.as_bytes()),
        TIMEOUT,
    )?;
    match code {
        0 => Ok(Added::Stored),
        DUPLICATE if !update => Ok(Added::Exists),
        _ => Err(UNAVAILABLE.into()),
    }
}

/// Deleting an absent item succeeds, so removal is retry-safe.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    names_valid(service, account)?;
    let (code, _) = run(
        Command::new(SECURITY).args(["delete-generic-password", "-a", account, "-s", service]),
        None,
        TIMEOUT,
    )?;
    match code {
        0 | NOT_FOUND => Ok(()),
        _ => Err(UNAVAILABLE.into()),
    }
}

/// Item names come from Switchboard or Claude Code, never from a prompt; a control
/// character would split the `security -i` line into a second command.
fn names_valid(service: &str, account: &str) -> Result<(), String> {
    for name in [service, account] {
        if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
            return Err(UNAVAILABLE.into());
        }
    }
    Ok(())
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// SB-420: whether a write of `secret` grown by `headroom` printable bytes would still fit one
/// `security -i` line, asked before an act that cannot be undone (a renewal spends its refresh
/// token before it writes). Runs nothing.
pub fn fits(service: &str, account: &str, secret: &[u8], headroom: usize) -> bool {
    let mut probe = secret.to_vec();
    probe.extend(std::iter::repeat_n(b'x', headroom));
    names_valid(service, account).is_ok()
        && probe.len() <= LIMIT
        && add_line(service, account, &probe, true, None).is_ok()
}

/// Every argument a write passes to the security tool: none carries the value.
pub(crate) const ADD_ARGS: [&str; 1] = ["-i"];

/// The one `security -i` line a write sends on stdin: the value as hex (`-X`), or, when that
/// would not fit the tool's line buffer, as quoted text (`-w`, printable ASCII only, half the
/// size). Refused with [`TOO_LARGE`] when neither fits: a longer line would be cut, and the
/// rest read as a second command. Kept apart from spawning so tests can prove where the value
/// travels without touching a Keychain.
pub(crate) fn add_line(
    service: &str,
    account: &str,
    secret: &[u8],
    update: bool,
    keychain: Option<&Path>,
) -> Result<String, String> {
    let update_flag = if update { "-U " } else { "" };
    let target = match keychain {
        Some(path) => format!(" {}", quote(path.to_str().ok_or(UNAVAILABLE)?)),
        None => String::new(),
    };
    let head = format!(
        "add-generic-password {update_flag}-a {} -s {}",
        quote(account),
        quote(service)
    );
    let hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();
    let line = format!("{head} -X {hex}{target}\n");
    if line.len() <= STDIN_LINE_LIMIT {
        return Ok(line);
    }
    // Printable ASCII only: quotes and backslashes are escaped (`quote`), which the tool's
    // parser reads back to the same bytes (measured with a throwaway keychain in
    // `keychain_macos`), and a control byte could end the line early.
    if secret.iter().all(|b| (0x20..=0x7e).contains(b)) {
        let text = std::str::from_utf8(secret).map_err(|_| UNAVAILABLE)?;
        let line = format!("{head} -w {}{target}\n", quote(text));
        if line.len() <= STDIN_LINE_LIMIT {
            return Ok(line);
        }
    }
    Err(TOO_LARGE.into())
}

/// Runs one bounded `security` call: stdin written from a thread, stdout capped,
/// stderr discarded, killed and reaped at the deadline.
pub(crate) fn run(
    command: &mut Command,
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<(i32, Vec<u8>), String> {
    let mut child = command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| UNAVAILABLE)?;
    let writer = stdin.map(|bytes| {
        let mut pipe = child.stdin.take();
        let bytes = bytes.to_vec();
        std::thread::spawn(move || {
            if let Some(pipe) = pipe.as_mut() {
                let _ = pipe.write_all(&bytes);
            }
            // Dropping the pipe sends EOF, which ends `security -i`.
        })
    });
    let stdout = child.stdout.take().ok_or(UNAVAILABLE)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((LIMIT + 2) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(UNAVAILABLE.to_string());
            }
        }
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let bytes = reader
        .join()
        .map_err(|_| UNAVAILABLE)?
        .map_err(|_| UNAVAILABLE)?;
    let status = status?;
    if bytes.len() > LIMIT {
        return Err(UNAVAILABLE.into());
    }
    Ok((status.code().unwrap_or(-1), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    #[test]
    fn a_small_secret_travels_on_stdin_as_hex_and_never_in_argv() {
        let secret = br#"{"claudeAiOauth":{"accessToken":"fixture-only"}}"#;
        let line = add_line("Claude Code-credentials", "user", secret, true, None).unwrap();
        assert!(line.starts_with(
            "add-generic-password -U -a \"user\" -s \"Claude Code-credentials\" -X 7b22"
        ));
        assert!(line.ends_with('\n'));
        assert!(!line.contains("fixture-only"));
        assert_eq!(ADD_ARGS, ["-i"]);
    }
    #[test]
    fn creation_without_update_omits_the_update_flag() {
        assert_eq!(
            add_line("service", "v1", b"ab", false, None).unwrap(),
            "add-generic-password -a \"v1\" -s \"service\" -X 6162\n"
        );
    }
    #[test]
    fn names_are_quoted_for_the_interactive_parser() {
        let line = add_line("a\"b\\c", "u", b"x", true, None).unwrap();
        assert!(line.contains("-s \"a\\\"b\\\\c\""));
        assert!(names_valid("line\nbreak", "u").is_err());
        assert!(names_valid("", "u").is_err());
    }
    #[test]
    fn a_text_too_long_for_hex_goes_on_stdin_as_quoted_text() {
        // 3,000 bytes as hex overflow the line; as text, with every quote and backslash
        // escaped, they fit.
        let mut secret = br#"{"mcpOAuth":{"a\"b":"c'd\\e"},"pad":""#.to_vec();
        secret.resize(2990, b'x');
        secret.extend_from_slice(br#""}"#);
        let line = add_line("service", "user", &secret, true, None).unwrap();
        assert!(line.len() <= STDIN_LINE_LIMIT);
        assert!(line.starts_with("add-generic-password -U -a \"user\" -s \"service\" -w \""));
        assert!(line.contains(r#"{\"mcpOAuth\":{\"a\\\"b\":\"c'd\\\\e\"}"#));
        assert!(!line.contains(&hex(&secret)));
        assert_eq!(line.matches('\n').count(), 1, "one command, one line");
    }
    #[test]
    fn a_value_too_large_for_one_line_is_refused_never_put_in_argv() {
        // Text past the line, and bytes that are not printable text past the hex line.
        let text = vec![b'a'; 4100];
        assert_eq!(
            add_line("s", "u", &text, true, None).unwrap_err(),
            TOO_LARGE
        );
        let mut binary = vec![b'a'; 2100];
        binary[7] = b'\n';
        assert_eq!(
            add_line("s", "u", &binary, true, None).unwrap_err(),
            TOO_LARGE
        );
        // The largest hex value that still fits takes the hex form.
        let room = (STDIN_LINE_LIMIT - "add-generic-password -U -a \"u\" -s \"s\" -X \n".len()) / 2;
        assert!(add_line("s", "u", &vec![1u8; room], true, None)
            .unwrap()
            .contains(" -X "));
        assert_eq!(
            add_line("s", "u", &vec![1u8; room + 1], true, None).unwrap_err(),
            TOO_LARGE
        );
        // Past the 64 KiB a write takes at all: too large, never the unlock sentence.
        assert_eq!(
            add_via(
                "/nonexistent/security",
                None,
                "s",
                "u",
                &vec![b'a'; LIMIT + 1],
                true
            )
            .unwrap_err(),
            TOO_LARGE
        );
    }
    #[test]
    fn a_fit_check_counts_the_headroom_and_runs_nothing() {
        let text = vec![b'a'; 3000];
        assert!(fits("s", "u", &text, 0));
        assert!(fits("s", "u", &text, 512));
        let room = STDIN_LINE_LIMIT - "add-generic-password -U -a \"u\" -s \"s\" -w \"\"\n".len();
        assert!(fits("s", "u", &vec![b'a'; room], 0));
        assert!(!fits("s", "u", &vec![b'a'; room], 1));
        assert!(!fits("s", "u", &vec![b'a'; room - 10], 512));
        assert!(!fits("line\nbreak", "u", b"x", 0));
    }
    #[test]
    fn a_named_keychain_ends_the_line() {
        let line = add_line(
            "s",
            "u",
            b"ab",
            false,
            Some(Path::new("/tmp/k.keychain-db")),
        )
        .unwrap();
        assert_eq!(
            line,
            "add-generic-password -a \"u\" -s \"s\" -X 6162 \"/tmp/k.keychain-db\"\n"
        );
    }
    /// The spawned process itself: a stand-in for the security tool records its arguments and
    /// its stdin. Neither the value nor its hex is ever in its arguments, short or long; a value
    /// that fits no line never starts it.
    #[test]
    fn the_spawned_tool_never_sees_the_value_in_its_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("security");
        let args = dir.path().join("args");
        let input = dir.path().join("stdin");
        std::fs::write(
            &tool,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > '{}'\n",
                args.display(),
                input.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
        let tool = tool.to_str().unwrap();
        let short = b"synthetic-short-fixture".to_vec();
        let mut long = br#"{"synthetic":""#.to_vec();
        long.resize(3000, b'y');
        long.extend_from_slice(br#""}"#);
        for secret in [&short, &long] {
            let _ = std::fs::remove_file(&args);
            assert_eq!(
                add_via(tool, None, "svc", "user", secret, true).unwrap(),
                Added::Stored
            );
            let argv = std::fs::read_to_string(&args).unwrap();
            assert_eq!(argv, "-i\n");
            let text = String::from_utf8_lossy(secret).into_owned();
            assert!(!argv.contains(&text) && !argv.contains(&hex(secret)));
            let sent = std::fs::read_to_string(&input).unwrap();
            assert!(sent.contains(&hex(secret)) || sent.contains(&"y".repeat(100)));
        }
        let _ = std::fs::remove_file(&args);
        assert_eq!(
            add_via(tool, None, "svc", "user", &vec![b'z'; 5000], true).unwrap_err(),
            TOO_LARGE
        );
        assert!(!args.exists(), "a refused value never starts the tool");
    }
    #[test]
    fn runner_reports_exit_codes_stdout_and_hides_stderr() {
        let call = |script: &str| {
            run(
                Command::new("/bin/sh").args(["-c", script]),
                None,
                Duration::from_secs(2),
            )
        };
        assert_eq!(
            call("printf 'fixture-only\\n'").unwrap(),
            (0, b"fixture-only\n".to_vec())
        );
        assert_eq!(call("exit 44").unwrap().0, 44);
        assert_eq!(
            call("printf 'secret-must-not-escape' >&2; exit 1").unwrap(),
            (1, vec![])
        );
    }
    #[test]
    fn runner_passes_stdin_and_ends_it() {
        let (code, out) = run(
            Command::new("/bin/cat").arg("-"),
            Some(b"line from stdin\n"),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!((code, out), (0, b"line from stdin\n".to_vec()));
    }
    #[test]
    fn runner_deadline_kills_and_reaps() {
        let start = Instant::now();
        let result = run(
            Command::new("/bin/sh").args(["-c", "exec sleep 10"]),
            None,
            Duration::from_millis(50),
        );
        assert_eq!(result.unwrap_err(), UNAVAILABLE);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn runner_output_is_bounded() {
        let result = run(
            Command::new("/usr/bin/head").args(["-c", "70000", "/dev/zero"]),
            None,
            Duration::from_secs(2),
        );
        assert_eq!(result.unwrap_err(), UNAVAILABLE);
    }
}
