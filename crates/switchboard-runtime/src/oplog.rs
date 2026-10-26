//! A bounded, rotating log of lifecycle events and refusal codes (lifecycle rule LC-12): owner
//! start and stop, the drain outcome, the proxy port, and each change of state of a credential
//! source the background watches (locked, refused, absent). It answers "did it prompt, and why"
//! from evidence instead of from memory.
//!
//! Values never enter it, by construction: an event and every field are `&'static str` codes,
//! integers or booleans, so a credential, a path, an email or a provider's message cannot be passed in.
//! The file is `switchboard.log` in `~/Library/Logs/Fabric Switchboard/` (macOS), mode 0600,
//! rotated by size: five files of at most 5 MB each.
use serde_json::{Map, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub const FILE: &str = "switchboard.log";
const CAP_BYTES: u64 = 5 * 1024 * 1024;
const KEEP_FILES: usize = 5;

/// One field of an event: a fixed code, a number or a yes/no. There is deliberately no string
/// variant that takes an owned or borrowed runtime value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Code(&'static str),
    Number(i64),
    Flag(bool),
}

pub struct Log {
    dir: PathBuf,
    cap: u64,
    keep: usize,
    write: Mutex<()>,
}

impl Log {
    /// The product's log: `SWITCHBOARD_LOG_DIR` (absolute) when set, otherwise the platform's
    /// per-user log folder.
    pub fn default_location() -> Option<Self> {
        default_dir().map(|dir| Self::at(dir, CAP_BYTES, KEEP_FILES))
    }
    pub fn at(dir: PathBuf, cap: u64, keep: usize) -> Self {
        Self {
            dir,
            cap: cap.max(256),
            keep: keep.max(1),
            write: Mutex::new(()),
        }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    /// Appends one line. Logging never fails the caller: an unwritable folder drops the line.
    pub fn event(&self, event: &'static str, fields: &[(&'static str, Field)]) {
        let _ = self.append(&line(event, fields, unix_now()));
    }
    fn append(&self, line: &str) -> Result<(), String> {
        let _guard = self.write.lock().unwrap_or_else(|p| p.into_inner());
        switchboard_core::private_fs::private_dir(&self.dir)?;
        let path = self.dir.join(FILE);
        let size = fs::symlink_metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size + line.len() as u64 > self.cap {
            self.rotate()?;
        }
        let mut file = append_options()
            .open(&path)
            .map_err(|_| "Log unavailable".to_string())?;
        file.write_all(line.as_bytes())
            .map_err(|_| "Log unavailable".to_string())
    }
    /// `switchboard.log` → `.1` → … → `.{keep-1}`; the oldest is removed.
    fn rotate(&self) -> Result<(), String> {
        let numbered = |n: usize| self.dir.join(format!("{FILE}.{n}"));
        let _ = fs::remove_file(numbered(self.keep - 1));
        for n in (1..self.keep - 1).rev() {
            let from = numbered(n);
            if from.exists() {
                fs::rename(&from, numbered(n + 1)).map_err(|_| "Log rotation failed")?;
            }
        }
        if self.keep == 1 {
            return fs::remove_file(self.dir.join(FILE)).map_err(|_| "Log rotation failed".into());
        }
        fs::rename(self.dir.join(FILE), numbered(1)).map_err(|_| "Log rotation failed".into())
    }
}

#[cfg(unix)]
fn append_options() -> fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    options
}
#[cfg(not(unix))]
fn append_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    options
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn line(event: &'static str, fields: &[(&'static str, Field)], at: i64) -> String {
    let mut object = Map::new();
    object.insert(
        "at".into(),
        crate::rfc3339(at).map(Value::String).unwrap_or(Value::Null),
    );
    object.insert("event".into(), Value::String(event.into()));
    for (key, field) in fields {
        // The two keys above are fixed; a field never overwrites them.
        if *key == "at" || *key == "event" {
            continue;
        }
        object.insert(
            (*key).into(),
            match field {
                Field::Code(code) => Value::String((*code).into()),
                Field::Number(number) => Value::from(*number),
                Field::Flag(flag) => Value::from(*flag),
            },
        );
    }
    let mut text = Value::Object(object).to_string();
    text.push('\n');
    text
}

/// The log of an owner over `root` (SB-112): the per-user log for the installed app's data folder
/// or when `SWITCHBOARD_LOG_DIR` names one, else `<root>/logs`, so a test or a second
/// `switchboard serve --data-dir` never writes into the installed app's log.
pub fn for_root(root: &Path, default_root: Option<&Path>) -> Option<Log> {
    let same = |a: &Path, b: &Path| a == b || a.canonicalize().ok() == b.canonicalize().ok();
    if std::env::var_os("SWITCHBOARD_LOG_DIR").is_some()
        || default_root.is_some_and(|d| same(d, root))
    {
        return Log::default_location();
    }
    Some(Log::at(root.join("logs"), CAP_BYTES, KEEP_FILES))
}

/// The per-user log folder: `SWITCHBOARD_LOG_DIR` when it is absolute, else
/// `~/Library/Logs/Fabric Switchboard` on macOS, `%LOCALAPPDATA%\Fabric Switchboard\Logs` on
/// Windows, `$XDG_STATE_HOME/fabric-switchboard` (`~/.local/state/…`) on Linux (SB-88), and the
/// data folder's `Fabric Switchboard/logs` elsewhere.
pub fn default_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SWITCHBOARD_LOG_DIR").map(PathBuf::from) {
        return dir.is_absolute().then_some(dir);
    }
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Logs/Fabric Switchboard"))
    }
    #[cfg(windows)]
    {
        dirs::data_local_dir().map(|d| d.join("Fabric Switchboard").join("Logs"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::state_dir().map(|d| d.join("fabric-switchboard"))
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        dirs::data_dir().map(|d| d.join("Fabric Switchboard").join("logs"))
    }
}

static GLOBAL: OnceLock<Log> = OnceLock::new();
/// Installs the process's log once; only a real owner (the desktop app, `switchboard serve`)
/// installs one. Later calls keep the first.
pub fn install(log: Log) {
    let _ = GLOBAL.set(log);
}
#[cfg(test)]
type Captured = Vec<(&'static str, Vec<(&'static str, Field)>)>;
#[cfg(test)]
thread_local! {
    static CAPTURED: std::cell::RefCell<Option<Captured>> =
        const { std::cell::RefCell::new(None) };
}
/// Tests: runs `f` and answers the names of the events this thread logged meanwhile, whether
/// or not a log is installed (tests share one process, so the installed log is not theirs).
#[cfg(test)]
pub(crate) fn capture<R>(f: impl FnOnce() -> R) -> (R, Vec<&'static str>) {
    CAPTURED.with(|c| *c.borrow_mut() = Some(Vec::new()));
    let result = f();
    let events = CAPTURED.with(|c| c.borrow_mut().take().unwrap_or_default());
    (result, events.into_iter().map(|(name, _)| name).collect())
}
/// Tests: as `capture`, for a future polled on this thread (a current-thread test runtime), with
/// each event's fields.
#[cfg(test)]
pub(crate) async fn capture_async<R>(f: impl std::future::Future<Output = R>) -> (R, Captured) {
    CAPTURED.with(|c| *c.borrow_mut() = Some(Vec::new()));
    let result = f.await;
    let events = CAPTURED.with(|c| c.borrow_mut().take().unwrap_or_default());
    (result, events)
}
/// Writes to the installed log; a process without one (tests, offline commands) writes nothing.
pub fn event(event: &'static str, fields: &[(&'static str, Field)]) {
    #[cfg(test)]
    CAPTURED.with(|c| {
        if let Some(events) = c.borrow_mut().as_mut() {
            events.push((event, fields.to_vec()));
        }
    });
    if let Some(log) = GLOBAL.get() {
        log.event(event, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(dir: &Path, name: &str) -> Vec<Value> {
        fs::read_to_string(dir.join(name))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn an_owner_over_another_data_folder_logs_inside_it() {
        // SB-112: a second `serve --data-dir` never writes into the installed app's log.
        let other = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        if std::env::var_os("SWITCHBOARD_LOG_DIR").is_none() {
            let log = for_root(other.path(), Some(installed.path())).unwrap();
            assert_eq!(log.dir, other.path().join("logs"));
            let own = for_root(installed.path(), Some(installed.path()));
            assert_eq!(own.map(|l| l.dir), default_dir());
        }
    }

    #[test]
    fn an_event_is_one_json_line_of_codes_and_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let log = Log::at(tmp.path().join("logs"), CAP_BYTES, KEEP_FILES);
        log.event(
            "owner_stopped",
            &[
                ("outcome", Field::Code("drained")),
                ("millis", Field::Number(42)),
                ("event", Field::Code("must-not-overwrite")),
            ],
        );
        let written = lines(log.dir(), FILE);
        assert_eq!(written.len(), 1);
        assert_eq!(written[0]["event"], "owner_stopped");
        assert_eq!(written[0]["outcome"], "drained");
        assert_eq!(written[0]["millis"], 42);
        assert!(written[0]["at"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn a_flag_is_written_as_a_json_boolean() {
        let tmp = tempfile::tempdir().unwrap();
        let log = Log::at(tmp.path().join("logs"), CAP_BYTES, KEEP_FILES);
        log.event(
            "seat_ended",
            &[
                ("provider", Field::Code("codex")),
                ("organization", Field::Flag(true)),
            ],
        );
        let written = lines(log.dir(), FILE);
        assert_eq!(written[0]["organization"], Value::Bool(true));
        assert_eq!(written[0]["provider"], "codex");
    }

    #[cfg(unix)]
    #[test]
    fn the_log_and_its_folder_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let log = Log::at(tmp.path().join("logs"), CAP_BYTES, KEEP_FILES);
        log.event("owner_started", &[]);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&log.dir().join(FILE)), 0o600);
        assert_eq!(mode(log.dir()), 0o700);
    }

    #[test]
    fn writing_past_the_cap_rotates_and_keeps_a_bounded_number_of_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        let log = Log::at(dir.clone(), 1024, 3);
        for n in 0..200 {
            log.event("probe", &[("n", Field::Number(n))]);
        }
        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, [FILE, "switchboard.log.1", "switchboard.log.2"]);
        for name in &names {
            assert!(
                fs::metadata(dir.join(name)).unwrap().len() <= 1024,
                "{name}"
            );
        }
        // The newest line is in the live file, and nothing was lost between the live file
        // and the first rotation.
        let live = lines(&dir, FILE);
        assert_eq!(live.last().unwrap()["n"], 199);
        let previous = lines(&dir, "switchboard.log.1");
        assert_eq!(
            previous.last().unwrap()["n"].as_i64().unwrap() + 1,
            live[0]["n"].as_i64().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_log_file_is_never_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        fs::create_dir(&dir).unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        fs::write(&elsewhere, "").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join(FILE)).unwrap();
        Log::at(dir, CAP_BYTES, KEEP_FILES).event("owner_started", &[]);
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "");
    }

    #[test]
    fn a_process_without_an_installed_log_writes_nothing() {
        // Never installed in unit tests: the call is a no-op rather than a panic.
        event("owner_started", &[("outcome", Field::Code("ok"))]);
    }
}
