//! Read external CLI items using Apple's stable executable, as Claude Code/Swap do.
//! No credentials in argv, stderr, logs or errors. Private vault writes stay native.
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const LIMIT: usize = 64 * 1024;
const UNAVAILABLE: &str = "Keychain unavailable. Unlock it and allow access, then retry.";

pub fn read(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    bounded(
        Command::new("/usr/bin/security").args([
            "find-generic-password",
            "-a",
            account,
            "-w",
            "-s",
            service,
        ]),
        Duration::from_secs(5),
    )
}
fn bounded(command: &mut Command, timeout: Duration) -> Result<Option<Vec<u8>>, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| UNAVAILABLE)?;
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
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(UNAVAILABLE.to_string());
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| UNAVAILABLE)?
        .map_err(|_| UNAVAILABLE)?;
    let status = status?;
    if status.code() == Some(44) {
        return Ok(None);
    }
    if !status.success() {
        return Err(UNAVAILABLE.into());
    }
    let mut bytes = bytes;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.len() > LIMIT {
        return Err(UNAVAILABLE.into());
    }
    Ok(Some(bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_keychain_distinguishes_absent_denied_and_valid_without_leaking() {
        let call = |script| {
            bounded(
                Command::new("/bin/sh").args(["-c", script]),
                Duration::from_secs(1),
            )
        };
        assert_eq!(
            call("printf 'fixture-only\\n'").unwrap(),
            Some(b"fixture-only".to_vec())
        );
        assert_eq!(call("exit 44").unwrap(), None);
        assert_eq!(
            call("printf 'secret-must-not-escape' >&2; exit 1").unwrap_err(),
            UNAVAILABLE
        );
    }
    #[test]
    fn external_keychain_deadline_kills_and_reaps_the_reader() {
        let start = Instant::now();
        let result = bounded(
            Command::new("/bin/sh").args(["-c", "exec sleep 10"]),
            Duration::from_millis(50),
        );
        assert_eq!(result.unwrap_err(), UNAVAILABLE);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn external_keychain_output_is_bounded() {
        let result = bounded(
            Command::new("/usr/bin/head").args(["-c", "70000", "/dev/zero"]),
            Duration::from_secs(1),
        );
        assert_eq!(result.unwrap_err(), UNAVAILABLE);
    }
}
