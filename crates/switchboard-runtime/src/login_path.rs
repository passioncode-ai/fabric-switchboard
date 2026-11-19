//! The person's login `PATH` (SB-455). The desktop app and `serve` start from launchd with a
//! short `PATH` (`/usr/bin:/bin`, and whatever the app's own start added), not the one the person's
//! login shell builds. A session Switchboard starts inherited that `PATH`, so a Claude Code hook
//! that runs `node` from Homebrew failed with "node: command not found". The process that builds a
//! session (the owner, or an offline `launch`) asks the login shell, non-interactively and bounded,
//! the first time it needs the answer; the owner asks once at start (`warm`), so its first session
//! does not wait.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long the login shell may take to print its `PATH`; past it the shell's process group is
/// killed and reaped and the fallback is used.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// After a failed ask, how long before the shell is asked once more (a profile that was slow at
/// login, a shell being installed); after the second failure the fallback stays for the process.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);
/// How many times one process asks at most.
const MAX_ASKS: u32 = 2;
/// The most output read from the login shell; a marker line past it is not read.
#[cfg(unix)]
const MAX_OUTPUT: u64 = 64 * 1024;
/// The prefix of the line the asked command prints, so a profile's own output (a greeting before
/// it, `.zlogout` after it) is never read as the answer.
#[cfg(unix)]
const MARKER: &str = "__SB_PATH__=";
/// The command given to `<shell> -l -c`: `/bin/sh` prints the exported `PATH` after the marker,
/// the same for zsh, bash and fish, and without `/usr/bin/printenv` (absent on NixOS).
pub const COMMAND: &str = r#"/bin/sh -c 'printf "__SB_PATH__=%s\n" "$PATH"'"#;
/// The folders a package manager puts tools in that a launchd `PATH` lacks, added after whatever
/// the login shell said (or after the process's own `PATH` when it said nothing).
pub const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// What one ask found, when, and how many asks there were.
struct Asked {
    answer: Option<Vec<PathBuf>>,
    at: Instant,
    asks: u32,
}
/// The process's answer: `None` until the first ask.
static LOGIN: Mutex<Option<Asked>> = Mutex::new(None);

/// The login shell's `PATH` folders (blocking, at most `TIMEOUT` when it asks). An answer is kept
/// for the process's life; a failure (no shell, a timeout, an answer that is not a `PATH`) is
/// asked once more after `RETRY_AFTER`, then kept. In this crate's unit tests no shell is asked:
/// the person's profile is not part of a test, which sets its own folders with `with_login`.
pub fn login_dirs() -> Option<Vec<PathBuf>> {
    #[cfg(test)]
    if let Some(dirs) = TEST_LOGIN.with(|t| t.borrow().clone()) {
        return Some(dirs);
    }
    cached(&LOGIN, Instant::now(), resolve_once)
}

/// `cache`'s answer at `now`: `ask` is called the first time, and once more when the first ask
/// failed at least `RETRY_AFTER` before `now` (at most `MAX_ASKS` asks in all).
fn cached(
    cache: &Mutex<Option<Asked>>,
    now: Instant,
    ask: impl FnOnce() -> Option<Vec<PathBuf>>,
) -> Option<Vec<PathBuf>> {
    let mut slot = cache.lock().unwrap_or_else(|e| e.into_inner());
    let asks = match &*slot {
        None => 0,
        Some(asked) if asked.answer.is_some() => return asked.answer.clone(),
        Some(asked)
            if asked.asks >= MAX_ASKS || now.saturating_duration_since(asked.at) < RETRY_AFTER =>
        {
            return None;
        }
        Some(asked) => asked.asks,
    };
    let answer = ask();
    *slot = Some(Asked {
        answer: answer.clone(),
        at: now,
        asks: asks + 1,
    });
    answer
}

#[cfg(test)]
thread_local! {
    static TEST_LOGIN: std::cell::RefCell<Option<Vec<PathBuf>>> = const { std::cell::RefCell::new(None) };
}
/// Runs `f` with `dirs` as this thread's login `PATH`.
#[cfg(test)]
pub(crate) fn with_login<R>(dirs: Vec<PathBuf>, f: impl FnOnce() -> R) -> R {
    TEST_LOGIN.with(|t| *t.borrow_mut() = Some(dirs));
    let result = f();
    TEST_LOGIN.with(|t| *t.borrow_mut() = None);
    result
}

/// Asks the login shell now, off the caller's path, so the first session does not wait for it.
/// The owner calls it once at start; a call after the first ask does nothing.
pub fn warm() {
    if LOGIN.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("login-path".into())
        .spawn(|| {
            let started = Instant::now();
            let dirs = login_dirs();
            let dirs = dirs.as_deref();
            crate::oplog::event(
                "login_path",
                &[
                    (
                        "source",
                        crate::oplog::Field::Code(if dirs.is_some() {
                            "shell"
                        } else {
                            "fallback"
                        }),
                    ),
                    (
                        "dirs",
                        crate::oplog::Field::Number(dirs.map_or(0, |d| d.len() as i64)),
                    ),
                    (
                        "millis",
                        crate::oplog::Field::Number(started.elapsed().as_millis() as i64),
                    ),
                ],
            );
        });
}

#[cfg(all(unix, not(test)))]
fn resolve_once() -> Option<Vec<PathBuf>> {
    resolve(&login_shell()?, TIMEOUT)
}
#[cfg(any(not(unix), test))]
fn resolve_once() -> Option<Vec<PathBuf>> {
    None
}

/// The person's login shell: `$SHELL` when it names an executable by an absolute path, else the
/// account's shell from the password database.
#[cfg(unix)]
pub fn login_shell() -> Option<PathBuf> {
    let usable = |p: &Path| p.is_absolute() && p.is_file();
    if let Some(shell) = std::env::var_os("SHELL").map(PathBuf::from) {
        if usable(&shell) {
            return Some(shell);
        }
    }
    // SAFETY: getpwuid returns null or static storage valid until the next passwd call; the
    // field is copied out at once.
    let pw = unsafe { libc::getpwuid(libc::geteuid()) };
    if pw.is_null() {
        return None;
    }
    // SAFETY: `pw` was checked non-null and points at the passwd record getpwuid filled.
    let shell = unsafe { (*pw).pw_shell };
    if shell.is_null() {
        return None;
    }
    let shell = PathBuf::from(
        // SAFETY: a non-null pw_shell is a NUL-terminated string in the same record.
        unsafe { std::ffi::CStr::from_ptr(shell) }
            .to_string_lossy()
            .into_owned(),
    );
    usable(&shell).then_some(shell)
}
#[cfg(not(unix))]
pub fn login_shell() -> Option<PathBuf> {
    None
}

/// What the reader thread hands over: a marker line's value, or the end of the output.
#[cfg(unix)]
enum Out {
    Line(String),
    End,
}

/// Runs `shell -l -c COMMAND` with nothing on stdin and stderr discarded, in a process group of
/// its own, and reads the last line that starts with `MARKER`. A reader thread hands each marker
/// line over as soon as it reads it, so a child of the profile that keeps the pipe open after the
/// shell exits holds the answer up by at most half a second (the thread is left to end when the
/// pipe closes). At `timeout` the group is killed while the shell is still unreaped, then the shell
/// is reaped; no group is signalled after that. `None` unless the shell exited with success and
/// the answer names at least one absolute folder.
#[cfg(unix)]
pub fn resolve(shell: &Path, timeout: Duration) -> Option<Vec<PathBuf>> {
    use std::io::{BufRead, Read};
    use std::process::{Command, Stdio};
    let mut command = Command::new(shell);
    command
        .args(["-l", "-c", COMMAND])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take();
    let (sent, read) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(stdout) = stdout {
            let mut lines = std::io::BufReader::new(stdout.take(MAX_OUTPUT));
            let mut line = Vec::new();
            while matches!(lines.read_until(b'\n', &mut line), Ok(n) if n > 0) {
                if let Some(value) = std::str::from_utf8(&line)
                    .ok()
                    .and_then(|text| text.trim_end().strip_prefix(MARKER))
                {
                    let _ = sent.send(Out::Line(value.to_string()));
                }
                line.clear();
            }
        }
        let _ = sent.send(Out::End);
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                if let Ok(group) = libc::pid_t::try_from(child.id()) {
                    // SAFETY: the shell leads this group and is not reaped yet, so its id still
                    // names the group this call created.
                    unsafe {
                        libc::killpg(group, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    if !status?.success() {
        return None;
    }
    // The shell exited, so its marker line, if it printed one, is in the pipe already. The reader
    // ends at once unless a child of the profile holds the pipe; then the last marker line handed
    // over within a short wait is the answer. The first line gets two seconds, so a reader thread
    // the machine's load keeps off the processor is not taken for a missing answer.
    let mut answer = None;
    let mut deadline = Instant::now() + Duration::from_secs(2);
    while let Ok(Out::Line(value)) =
        read.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        answer = Some(value);
        deadline = Instant::now() + Duration::from_millis(500);
    }
    parse(&answer?)
}
#[cfg(not(unix))]
pub fn resolve(_: &Path, _: Duration) -> Option<Vec<PathBuf>> {
    None
}

/// A marker line's value read as a `PATH`: its absolute entries (an empty or relative entry is
/// dropped, never searched from the session's folder), at least one.
#[cfg(unix)]
fn parse(value: &str) -> Option<Vec<PathBuf>> {
    let dirs: Vec<PathBuf> = std::env::split_paths(value.trim())
        .filter(|d| d.is_absolute())
        .collect();
    (!dirs.is_empty()).then_some(dirs)
}

/// The `PATH` a session gets: `first` (a program's own folder), then `login` (the login shell's),
/// then `current` (this process's), then `extra` (per-user folders), then `FALLBACK_DIRS`; each
/// folder once, in its first place.
pub fn merge(
    first: Option<&Path>,
    login: Option<&[PathBuf]>,
    current: Option<OsString>,
    extra: &[PathBuf],
) -> OsString {
    let mut dirs: Vec<PathBuf> = first.map(Path::to_path_buf).into_iter().collect();
    dirs.extend(login.unwrap_or_default().iter().cloned());
    if let Some(path) = current {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.extend(extra.iter().cloned());
    dirs.extend(FALLBACK_DIRS.iter().map(PathBuf::from));
    let mut seen = std::collections::BTreeSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    std::env::join_paths(dirs).unwrap_or_default()
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_failed_ask_is_asked_once_more_after_a_minute_and_an_answer_is_kept() {
        let cache = Mutex::new(None);
        let asks = Cell::new(0);
        let t0 = Instant::now();
        let dirs = vec![PathBuf::from("/synthetic/brew/bin")];
        let ask = |answer: Option<Vec<PathBuf>>| {
            let asks = &asks;
            move || {
                asks.set(asks.get() + 1);
                answer
            }
        };
        assert_eq!(cached(&cache, t0, ask(None)), None);
        // Within the minute the failure stands, nothing is asked.
        assert_eq!(
            cached(&cache, t0 + Duration::from_secs(30), ask(None)),
            None
        );
        assert_eq!(asks.get(), 1);
        // After it, once more; the answer is then kept.
        let later = t0 + RETRY_AFTER + Duration::from_secs(1);
        assert_eq!(
            cached(&cache, later, ask(Some(dirs.clone()))),
            Some(dirs.clone())
        );
        let much_later = later + Duration::from_secs(3600);
        assert_eq!(cached(&cache, much_later, ask(None)), Some(dirs.clone()));
        assert_eq!(asks.get(), 2);

        // Two failures: the fallback stays for the process.
        let cache = Mutex::new(None);
        asks.set(0);
        assert_eq!(cached(&cache, t0, ask(None)), None);
        assert_eq!(cached(&cache, later, ask(None)), None);
        assert_eq!(cached(&cache, much_later, ask(Some(dirs.clone()))), None);
        assert_eq!(asks.get(), 2);

        // A first answer is never asked again.
        let cache = Mutex::new(None);
        asks.set(0);
        assert_eq!(
            cached(&cache, t0, ask(Some(dirs.clone()))),
            Some(dirs.clone())
        );
        assert_eq!(cached(&cache, much_later, ask(None)), Some(dirs));
        assert_eq!(asks.get(), 1);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn shell(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake-shell");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn the_login_shell_runs_the_marker_command_and_only_the_marker_line_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let brew = dir.path().join("homebrew/bin");
        let log = dir.path().join("args");
        // A profile that greets before the answer and says goodbye after it (`.zlogout`); the
        // fake shell runs the command it was given, as a login shell does, with its own PATH.
        let fake = shell(
            dir.path(),
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\necho 'Welcome back'\nPATH='{}:/usr/bin:/bin'\nexport PATH\n/bin/sh -c \"$3\"\necho 'Goodbye /usr/bin'",
                log.display(),
                brew.display()
            ),
        );
        let dirs = resolve(&fake, TIMEOUT).unwrap();
        assert_eq!(dirs[0], brew);
        assert_eq!(
            dirs[1..],
            [PathBuf::from("/usr/bin"), PathBuf::from("/bin")]
        );
        let args = std::fs::read_to_string(log).unwrap();
        assert_eq!(args, format!("-l\n-c\n{COMMAND}\n"));
    }

    #[test]
    fn a_shell_that_hangs_is_killed_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        // A background child keeps the pipe open: the group is killed with the shell.
        let fake = shell(
            dir.path(),
            "sleep 30 &\nsleep 30\necho __SB_PATH__=/usr/bin",
        );
        let started = Instant::now();
        assert_eq!(resolve(&fake, Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_child_of_the_profile_that_holds_the_pipe_does_not_hold_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        // The shell answers and exits; a daemon its profile started keeps stdout open.
        let fake = shell(
            dir.path(),
            "echo __SB_PATH__=/usr/bin:/bin\nsleep 5 &\nexit 0",
        );
        let started = Instant::now();
        assert_eq!(
            resolve(&fake, TIMEOUT),
            Some(vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn relative_entries_are_dropped_and_the_rest_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let fake = shell(dir.path(), "echo __SB_PATH__=node_modules/.bin::/usr/bin:.");
        assert_eq!(
            resolve(&fake, TIMEOUT),
            Some(vec![PathBuf::from("/usr/bin")])
        );
    }

    #[test]
    fn a_failed_shell_or_an_answer_that_is_not_a_path_gives_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve(
                &shell(dir.path(), "echo __SB_PATH__=/usr/bin\nexit 1"),
                TIMEOUT
            ),
            None
        );
        let dir = tempfile::tempdir().unwrap();
        // A line without the marker is never the answer.
        assert_eq!(resolve(&shell(dir.path(), "echo /usr/bin"), TIMEOUT), None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve(&shell(dir.path(), "echo '__SB_PATH__=not a path'"), TIMEOUT),
            None
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(&shell(dir.path(), "true"), TIMEOUT), None);
        assert_eq!(resolve(Path::new("/nonexistent/shell"), TIMEOUT), None);
    }

    #[test]
    fn the_merged_path_puts_the_login_folders_first_and_keeps_every_folder_once() {
        let login = [
            PathBuf::from("/synthetic/brew/bin"),
            PathBuf::from("/usr/bin"),
        ];
        let merged = merge(
            Some(Path::new("/opt/agent/bin")),
            Some(&login),
            Some(OsString::from("/home/x/.cargo/bin:/usr/bin:/bin")),
            &[PathBuf::from("/home/x/.local/bin")],
        );
        assert_eq!(
            merged,
            OsString::from(
                "/opt/agent/bin:/synthetic/brew/bin:/usr/bin:/home/x/.cargo/bin:/bin:/home/x/.local/bin:/opt/homebrew/bin:/usr/local/bin"
            )
        );
        // Without a login answer the fallback still carries Homebrew's folders.
        let fallback = merge(None, None, Some(OsString::from("/usr/bin:/bin")), &[]);
        assert_eq!(
            fallback,
            OsString::from("/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
        );
    }
}
