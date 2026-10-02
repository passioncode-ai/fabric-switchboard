//! Generic-password access through Apple's stable `/usr/bin/security`, the way Claude Code
//! and Claude Swap reach the login Keychain. An item created by this binary trusts it, so
//! every later read or update is silent — for the desktop app, the CLI and every rebuild.
//! Secrets never enter stderr, logs or errors; on writes they travel on stdin as hex, and in
//! argv only when the line would overflow the 4 KB `security -i` buffer (Claude Code's own
//! fallback for the same items).
use std::{
    io::{Read, Write},
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
    names_valid(service, account)?;
    if secret.is_empty() || secret.len() > LIMIT {
        return Err(UNAVAILABLE.into());
    }
    let command = AddCommand::new(service, account, secret, update);
    let (code, _) = match &command.stdin {
        Some(line) => run(
            Command::new(SECURITY).arg("-i"),
            Some(line.as_bytes()),
            TIMEOUT,
        )?,
        None => run(Command::new(SECURITY).args(&command.argv), None, TIMEOUT)?,
    };
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

/// The two shapes one write can take. Kept separate from spawning so tests can prove
/// where the secret travels without touching a Keychain.
pub(crate) struct AddCommand {
    pub(crate) stdin: Option<String>,
    pub(crate) argv: Vec<String>,
}
impl AddCommand {
    pub(crate) fn new(service: &str, account: &str, secret: &[u8], update: bool) -> Self {
        let hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();
        let update_flag = if update { "-U " } else { "" };
        let line = format!(
            "add-generic-password {update_flag}-a {} -s {} -X {hex}\n",
            quote(account),
            quote(service)
        );
        if line.len() <= STDIN_LINE_LIMIT {
            return Self {
                stdin: Some(line),
                argv: vec![],
            };
        }
        let mut argv = vec!["add-generic-password".to_string()];
        if update {
            argv.push("-U".into());
        }
        argv.extend(["-a", account, "-s", service, "-X"].map(String::from));
        argv.push(hex);
        Self { stdin: None, argv }
    }
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
    #[test]
    fn a_small_secret_travels_on_stdin_as_hex_and_never_in_argv() {
        let secret = br#"{"claudeAiOauth":{"accessToken":"fixture-only"}}"#;
        let command = AddCommand::new("Claude Code-credentials", "user", secret, true);
        let line = command.stdin.expect("fits the stdin line");
        assert!(command.argv.is_empty());
        assert!(line.starts_with(
            "add-generic-password -U -a \"user\" -s \"Claude Code-credentials\" -X 7b22"
        ));
        assert!(line.ends_with('\n'));
        assert!(!line.contains("fixture-only"));
    }
    #[test]
    fn creation_without_update_omits_the_update_flag() {
        let command = AddCommand::new("service", "v1", b"ab", false);
        assert_eq!(
            command.stdin.unwrap(),
            "add-generic-password -a \"v1\" -s \"service\" -X 6162\n"
        );
    }
    #[test]
    fn names_are_quoted_for_the_interactive_parser() {
        let command = AddCommand::new("a\"b\\c", "u", b"x", true);
        assert!(command.stdin.unwrap().contains("-s \"a\\\"b\\\\c\""));
        assert!(names_valid("line\nbreak", "u").is_err());
        assert!(names_valid("", "u").is_err());
    }
    #[test]
    fn an_oversized_secret_falls_back_to_argv_as_hex() {
        let secret = vec![b'a'; 3000];
        let command = AddCommand::new("service", "user", &secret, true);
        assert!(command.stdin.is_none());
        assert_eq!(&command.argv[..2], ["add-generic-password", "-U"]);
        assert_eq!(command.argv.last().unwrap(), &"61".repeat(3000));
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
