//! The person's login `PATH` (SB-455). The desktop app and `serve` start from launchd with a
//! short `PATH` (`/usr/bin:/bin`, and whatever the app's own start added), not the one the person's
//! login shell builds. A session Switchboard starts inherited that `PATH`, so a Claude Code hook
//! that runs `node` from Homebrew failed with "node: command not found". The owner asks the login
//! shell once, non-interactively and bounded, and every session it starts gets that `PATH`.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long the login shell may take to print its `PATH`; past it the shell's process group is
/// killed and reaped and the fallback is used.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// The most output read from the login shell; a profile that prints more is not a `PATH`.
const MAX_OUTPUT: usize = 64 * 1024;
/// The folders a package manager puts tools in that a launchd `PATH` lacks, added after whatever
/// the login shell said (or after the process's own `PATH` when it said nothing).
pub const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// What the login shell answered, once per process: `None` when it could not be asked (no shell,
/// a timeout, an error, an answer that is not a `PATH`).
static LOGIN: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();

/// The login shell's `PATH` folders, asked at most once per process (blocking, at most `TIMEOUT`
/// the first time). In this crate's unit tests no shell is asked: the person's profile is not
/// part of a test, which sets its own folders with `with_login`.
pub fn login_dirs() -> Option<Vec<PathBuf>> {
    #[cfg(test)]
    if let Some(dirs) = TEST_LOGIN.with(|t| t.borrow().clone()) {
        return Some(dirs);
    }
    LOGIN.get_or_init(resolve_once).clone()
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
/// The owner calls it once at start; a second call does nothing.
pub fn warm() {
    if LOGIN.get().is_some() {
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
                        crate::oplog::Field::Code(if dirs.is_some() { "shell" } else { "fallback" }),
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

/// Runs `shell -l -c '/usr/bin/printenv PATH'` with nothing on stdin and stderr discarded, in a
/// process group of its own, and reads the last line it printed (a profile may print before it).
/// `printenv` reads the exported value, so the answer is the same for zsh, bash and fish. At
/// `timeout` the group is killed and reaped. `None` unless the answer is a `PATH` of absolute
/// folders.
#[cfg(unix)]
pub fn resolve(shell: &Path, timeout: Duration) -> Option<Vec<PathBuf>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut command = Command::new(shell);
    command
        .args(["-l", "-c", "/usr/bin/printenv PATH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take();
    let (sent, read) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout {
            let _ = stdout.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut out);
        }
        let _ = sent.send(out);
    });
    let kill_group = |child: &std::process::Child| {
        if let Ok(group) = libc::pid_t::try_from(child.id()) {
            // SAFETY: killpg only signals the group this call created, called only while a member
            // of it is known to live (before the shell is reaped, or while one holds the pipe).
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                kill_group(&child);
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    // A profile that started a background process keeps the pipe open past the shell's exit: a
    // member of the group is then alive, so the group is killed and the reader ends.
    let out = match read.recv_timeout(timeout.saturating_sub(started.elapsed()).max(Duration::from_millis(100))) {
        Ok(out) => out,
        Err(_) => {
            kill_group(&child);
            read.recv().unwrap_or_default()
        }
    };
    if !status?.success() || out.len() > MAX_OUTPUT {
        return None;
    }
    parse(&String::from_utf8(out).ok()?)
}
#[cfg(not(unix))]
pub fn resolve(_: &Path, _: Duration) -> Option<Vec<PathBuf>> {
    None
}

/// The last non-empty line of `out`, read as a `PATH`: every entry absolute (empty entries
/// dropped), at least one.
fn parse(out: &str) -> Option<Vec<PathBuf>> {
    let line = out.lines().rev().map(str::trim).find(|l| !l.is_empty())?;
    let dirs: Vec<PathBuf> = std::env::split_paths(line)
        .filter(|d| !d.as_os_str().is_empty())
        .collect();
    (!dirs.is_empty() && dirs.iter().all(|d| d.is_absolute())).then_some(dirs)
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
    fn the_login_shell_is_asked_as_a_login_shell_and_its_last_line_is_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let brew = dir.path().join("homebrew/bin");
        let log = dir.path().join("args");
        // A profile that greets before the answer, as some do.
        let fake = shell(
            dir.path(),
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\necho 'Welcome back'\nprintf '%s\\n' '{}:/usr/bin:/bin'",
                log.display(),
                brew.display()
            ),
        );
        let dirs = resolve(&fake, TIMEOUT).unwrap();
        assert_eq!(dirs[0], brew);
        assert_eq!(dirs[1..], [PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        let args = std::fs::read_to_string(log).unwrap();
        assert_eq!(args, "-l\n-c\n/usr/bin/printenv PATH\n");
    }

    #[test]
    fn a_shell_that_hangs_is_killed_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        // A background child keeps the pipe open: the group is killed with the shell.
        let fake = shell(dir.path(), "sleep 30 &\nsleep 30\necho /usr/bin");
        let started = Instant::now();
        assert_eq!(resolve(&fake, Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_failed_shell_or_an_answer_that_is_not_a_path_gives_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(&shell(dir.path(), "echo /usr/bin\nexit 1"), TIMEOUT), None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(&shell(dir.path(), "echo 'not a path'"), TIMEOUT), None);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve(&shell(dir.path(), "true"), TIMEOUT), None);
        assert_eq!(resolve(Path::new("/nonexistent/shell"), TIMEOUT), None);
    }

    #[test]
    fn the_merged_path_puts_the_login_folders_first_and_keeps_every_folder_once() {
        let login = [PathBuf::from("/synthetic/brew/bin"), PathBuf::from("/usr/bin")];
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
