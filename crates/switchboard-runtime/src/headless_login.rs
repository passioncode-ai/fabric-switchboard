//! SB-256 N1: a Claude sign-in the owner runs itself, with no Terminal, so a sign-in autopilot
//! can finish its browser step (`docs/research/signin-autopilot-2026-11-05.md`).
//!
//! `claude auth login` (Claude Code 2.1.299) opens the URL `BROWSER` names and also prints a
//! manual URL whose `redirect_uri` is `https://platform.claude.com/oauth/code/callback`, then reads
//! the code that page shows from its input (`Paste code here if prompted >`). Here the CLI is the
//! owner's child: `BROWSER` points at a script that only records the URL, its output goes to a
//! private log in the sign-in's home, its input is a pipe the owner keeps, and the code is written
//! to that pipe. On exit 0 the home gets `.completed` like a Terminal sign-in, so capture and
//! `FinishLogin` are unchanged. Nothing here reads, logs or returns a credential: the log holds
//! what the CLI prints (no token), and the code goes to the CLI only.
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Mutex;

/// What the CLI prints, kept in the sign-in's home (0600).
pub(crate) const LOG: &str = "login.log";
/// Marks a home whose sign-in runs here rather than in Terminal.
pub(crate) const MARKER: &str = ".headless";
/// The script `BROWSER` names: it appends the URL the CLI would open to `URLS`.
const BROWSER: &str = "browser.sh";
const URLS: &str = ".browser-urls";
/// The page the manual URL sends the code to.
const MANUAL_REDIRECT: &str =
    "redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback";
/// A pasted code is printable ASCII without spaces, at most this long.
const MAX_CODE: usize = 512;

pub(crate) const NOT_WAITING: &str = "This sign-in is not waiting for a code. Start it again.";
pub(crate) const BAD_CODE: &str =
    "That is not a sign-in code. Copy the code the sign-in page shows.";

struct Running {
    stdin: Option<ChildStdin>,
    pid: u32,
}

static RUNNING: Mutex<Option<HashMap<PathBuf, Running>>> = Mutex::new(None);

fn with_running<R>(f: impl FnOnce(&mut HashMap<PathBuf, Running>) -> R) -> R {
    let mut guard = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// Whether this home's CLI is still running under this owner.
pub(crate) fn running(home: &Path) -> bool {
    with_running(|r| r.contains_key(home))
}

/// Starts `program auth login` in `home` as this owner's child (its own process group), with
/// `env` set, `remove` unset and `BROWSER` recording the URL. A thread reaps it and writes
/// `.completed` when it exits 0.
pub(crate) fn start(
    home: &Path,
    program: &Path,
    env: &std::collections::BTreeMap<String, String>,
    remove: &[&str],
) -> Result<(), String> {
    let urls = home.join(URLS);
    let quoted = urls.to_string_lossy().replace('\'', "'\\''");
    let script = format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{quoted}'\n");
    crate::launch::private_write(&home.join(BROWSER), script.as_bytes(), true)?;
    crate::launch::private_write(&home.join(MARKER), b"claude", false)?;
    let log = {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(home.join(LOG))
            .map_err(|_| "Sign-in could not start.".to_string())?
    };
    let mut command = Command::new(program);
    command
        .args(["auth", "login"])
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(
            log.try_clone()
                .map_err(|_| "Sign-in could not start.".to_string())?,
        )
        .stderr(log);
    for name in remove {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command.env("BROWSER", home.join(BROWSER));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Sign-in could not start.".to_string())?;
    let stdin = child.stdin.take();
    let pid = child.id();
    let key = home.to_path_buf();
    with_running(|r| r.insert(key.clone(), Running { stdin, pid }));
    std::thread::Builder::new()
        .name("switchboard-headless-login".into())
        .spawn(move || {
            let ok = child.wait().is_ok_and(|s| s.success());
            if ok {
                let _ = crate::launch::private_write(&key.join(".completed"), b"complete", false);
            }
            with_running(|r| r.remove(&key));
        })
        .map_err(|_| "Sign-in could not start.".to_string())?;
    Ok(())
}

/// The manual sign-in URL the CLI printed, once it has: the one sending its code to
/// `platform.claude.com`, on a Claude host. `None` until it is printed.
pub(crate) fn manual_url(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(LOG)).ok()?;
    manual_url_in(&text)
}

/// Every URL in an OSC 8 hyperlink or plain on a line, the first that is a Claude sign-in URL
/// with the manual redirect.
pub(crate) fn manual_url_in(text: &str) -> Option<String> {
    let mut found = Vec::new();
    // OSC 8: ESC ] 8 ; ; URL (BEL | ESC \).
    let mut rest = text;
    while let Some(at) = rest.find("\u{1b}]8;;") {
        rest = &rest[at + 5..];
        let end = rest.find(['\u{7}', '\u{1b}']).unwrap_or(rest.len());
        found.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    for word in text.split(|c: char| c.is_whitespace() || c.is_control()) {
        if word.starts_with("https://") {
            found.push(word.to_owned());
        }
    }
    found.into_iter().find(|url| is_manual_url(url))
}

fn is_manual_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    matches!(host, "claude.com" | "claude.ai")
        && rest.contains("/oauth/authorize?")
        && url.contains(MANUAL_REDIRECT)
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Writes the code the sign-in page showed to the CLI's input. The code is checked for shape
/// only and never stored or logged.
pub(crate) fn submit_code(home: &Path, code: &str) -> Result<(), String> {
    let code = code.trim();
    if code.is_empty() || code.len() > MAX_CODE || !code.chars().all(|c| c.is_ascii_graphic()) {
        return Err(BAD_CODE.into());
    }
    with_running(|r| {
        let stdin = r
            .get_mut(home)
            .and_then(|run| run.stdin.as_mut())
            .ok_or_else(|| NOT_WAITING.to_string())?;
        stdin
            .write_all(format!("{code}\n").as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|_| NOT_WAITING.to_string())
    })
}

/// Ends this home's CLI (its whole process group): SIGTERM, then SIGKILL after `wait`.
pub(crate) fn stop(home: &Path, wait: std::time::Duration) {
    let Some(pid) = with_running(|r| {
        r.get_mut(home).map(|run| {
            run.stdin = None;
            run.pid
        })
    }) else {
        return;
    };
    #[cfg(unix)]
    {
        let group = pid as libc::pid_t;
        // SAFETY: signals only the process group this owner started for this home.
        unsafe { libc::killpg(group, libc::SIGTERM) };
        let deadline = std::time::Instant::now() + wait;
        while running(home) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if running(home) {
            // SAFETY: as above.
            unsafe { libc::killpg(group, libc::SIGKILL) };
        }
    }
    #[cfg(not(unix))]
    let _ = (pid, wait);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const MANUAL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=c&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=user%3Ainference&code_challenge=x&code_challenge_method=S256&state=s";
    const LOCAL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=c&response_type=code&redirect_uri=http%3A%2F%2Flocalhost%3A61464%2Fcallback&state=s";

    /// A stand-in for `claude auth login`: calls `$BROWSER`, prints the manual URL as Claude
    /// Code 2.1.299 does, reads one line and exits 0 only for the code `good#s`.
    fn fake_cli(dir: &Path) -> PathBuf {
        let path = dir.join("fake-claude");
        let script = format!(
            "#!/bin/sh\n\"$BROWSER\" '{LOCAL}'\nprintf 'Opening browser to sign in…\\n'\nprintf 'If the browser didn'\"'\"'t open, visit: \\033]8;;%s\\007%s\\033]8;;\\007\\n' '{MANUAL}' '{MANUAL}'\nprintf 'Paste code here if prompted > '\nread code\n[ \"$code\" = 'good#s' ] && exit 0\necho 'Login failed'\nexit 1\n"
        );
        std::fs::write(&path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn until(what: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if what() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn the_manual_url_is_the_one_that_sends_the_code_to_platform_claude_com() {
        let printed = format!(
            "If the browser didn't open, visit: \u{1b}]8;;{MANUAL}\u{7}{MANUAL}\u{1b}]8;;\u{7}\n"
        );
        assert_eq!(manual_url_in(&printed).as_deref(), Some(MANUAL));
        assert_eq!(manual_url_in(&format!("visit: {LOCAL}\n")), None);
        let elsewhere = MANUAL.replace("https://claude.com/", "https://claude.com.evil.example/");
        assert_eq!(manual_url_in(&format!("visit: {elsewhere}\n")), None);
    }

    #[test]
    fn a_pasted_code_completes_the_sign_in_and_the_browser_url_is_only_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let program = fake_cli(dir.path());
        start(&home, &program, &Default::default(), &[]).unwrap();
        assert!(until(|| manual_url(&home).is_some()));
        assert_eq!(manual_url(&home).as_deref(), Some(MANUAL));
        assert!(until(
            || std::fs::read_to_string(home.join(URLS)).is_ok_and(|t| t.trim() == LOCAL)
        ));
        assert_eq!(submit_code(&home, "has space"), Err(BAD_CODE.into()));
        submit_code(&home, " good#s\n").unwrap();
        assert!(until(|| !running(&home)));
        assert_eq!(
            std::fs::read_to_string(home.join(".completed")).unwrap(),
            "complete"
        );
        assert_eq!(submit_code(&home, "good#s"), Err(NOT_WAITING.into()));
    }

    #[test]
    fn a_refused_code_ends_the_sign_in_without_completing_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        start(&home, &fake_cli(dir.path()), &Default::default(), &[]).unwrap();
        assert!(until(|| manual_url(&home).is_some()));
        submit_code(&home, "wrong#s").unwrap();
        assert!(until(|| !running(&home)));
        assert!(!home.join(".completed").exists());
    }

    #[test]
    fn stopping_ends_the_cli_and_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        start(&home, &fake_cli(dir.path()), &Default::default(), &[]).unwrap();
        assert!(until(|| manual_url(&home).is_some()));
        stop(&home, Duration::from_secs(3));
        assert!(until(|| !running(&home)));
        assert!(!home.join(".completed").exists());
    }
}
