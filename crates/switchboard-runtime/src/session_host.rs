//! SB-342 H-1: a session host, so a coding session opens in any of our apps and ends cleanly.
//!
//! One small host process per session (`switchboard session-host`) holds the session's PTY, a
//! scrollback of its last output and a Unix socket any viewer attaches to by launch id
//! (`switchboard attach`, Fabric Dashboards, Fabric). A viewer that goes away detaches; the
//! session ends only when its own process exits. The host is its own process, not a thread of
//! the owner, so an update or a restart of the app leaves every session running.
//!
//! The socket sits in `/tmp/switchboard-<uid>/`, a folder only this user may enter (0700, owned
//! by the user, never a link): that folder is the access boundary, as tmux's is. The data
//! folder's path is too long for a socket address on macOS (104 bytes), so the name is a digest
//! of the data folder and the launch id. `<data>/sessions/<launch_id>.json` (0600) says where
//! the socket is, the host's pid and how many viewers are attached.
//!
//! Frames, both ways: one kind byte, a big-endian u32 length, the payload (at most 1 MiB).
//! A viewer sends `HELLO` (JSON `{launch_id, rows?, cols?}`) first; the host answers `HELLO`
//! (JSON `{launch_id, viewers}`) or `ERROR` (JSON `{error}`) and closes, then the scrollback as
//! one `OUTPUT`, then live `OUTPUT`. A viewer sends `INPUT` (bytes), `RESIZE` (u16 rows, u16
//! cols, big-endian) and `DETACH`. When the session's process exits the host sends `EXIT` (JSON
//! `{exit_code}`) to every viewer and closes them.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const HELLO: u8 = 1;
pub const OUTPUT: u8 = 2;
pub const INPUT: u8 = 3;
pub const RESIZE: u8 = 4;
pub const EXIT: u8 = 5;
pub const ERROR: u8 = 6;
pub const DETACH: u8 = 7;
/// H-1c: a viewer asks for the raw output kept (up to 1 MiB) beyond the screen it was shown;
/// the host answers with the same kind.
pub const HISTORY: u8 = 8;
/// H-1d (Fabric Dashboards, `e3e2819c`): a viewer asks the host to end the session, as its idle
/// close does (`reason: ended`); detaching never ends it.
pub const END: u8 = 9;
/// SB-397 R1: a controller asks the host to restart its session in place: the running process
/// gets a hangup, and once it exits the host starts `program` (the session's resume script, a file
/// under the data folder) on the same terminal. Viewers stay attached and see the new process's
/// output; nobody gets `EXIT`. The host answers `RESTART` `{accepted}`. A host that does not
/// offer it (`can` in its `HELLO` answer lacks `restart`) ignores the frame.
pub const RESTART: u8 = 10;

/// H-1c: a session nobody watches and that neither wrote nor read for this long is closed
/// (`reason: idle`), as Fabric Dashboards' consoles do (LC-18). SB-396 D3: the duty officer's
/// wake-up floor (65 minutes), not 30: a session waiting on a wake-up it set itself writes nothing
/// until it wakes.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(crate::duty::WAKE_UP_FLOOR as u64);
/// H-1c: how long an ended session's output stays readable on disk.
const OUTPUT_KEPT: Duration = Duration::from_secs(24 * 3600);

/// The output a viewer that attaches late is shown first, as Fabric Dashboards' console keeps.
pub const SCROLLBACK: usize = 1 << 20;
const MAX_FRAME: usize = 1 << 20;
const MAX_VIEWERS: usize = 8;
/// A viewer that cannot take output this long is dropped, so it never holds the session up.
const VIEWER_WRITE: Duration = Duration::from_secs(2);
const MAX_DESCRIPTOR: u64 = 4096;
/// How long a viewer waits for the host's answer to its `HELLO` (SB-413).
const HELLO_WAIT: Duration = Duration::from_secs(15);

/// The frame protocol's version, in both `HELLO`s: a viewer refuses a host that speaks another.
pub const PROTOCOL: u64 = 1;

pub const NOT_FOUND: &str = crate::launches::NO_SUCH_LAUNCH;
pub const TOO_MANY_VIEWERS: &str = "This session already has eight viewers.";

/// Writes one frame.
pub fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    let mut head = [0u8; 5];
    head[0] = kind;
    head[1..].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads one frame; a frame over 1 MiB is refused before it is read.
pub fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok((head[0], payload))
}

/// The folder every session socket of this user sits in: created 0700 when absent; refused when
/// it is a link, belongs to someone else or lets others in.
pub fn socket_dir() -> Result<PathBuf, String> {
    let uid = unsafe { libc::getuid() };
    let dir = PathBuf::from(format!("/tmp/switchboard-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("The session folder in /tmp could not be made.".into()),
    }
    let meta = std::fs::symlink_metadata(&dir)
        .map_err(|_| "The session folder in /tmp could not be read.".to_string())?;
    if !meta.file_type().is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err("The session folder in /tmp is not this user's own.".into());
    }
    Ok(dir)
}

fn socket_name(root: &Path, launch_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(root.as_os_str().as_encoded_bytes());
    digest.update([0]);
    digest.update(launch_id.as_bytes());
    format!("{:x}", digest.finalize())[..16].to_owned() + ".sock"
}

/// Where the session of `launch_id` listens.
pub fn socket_path(root: &Path, launch_id: &str) -> Result<PathBuf, String> {
    Ok(socket_dir()?.join(socket_name(root, launch_id)))
}

fn descriptor_path(root: &Path, launch_id: &str) -> PathBuf {
    root.join("sessions").join(format!("{launch_id}.json"))
}

fn output_path(root: &Path, launch_id: &str) -> PathBuf {
    root.join("sessions").join(format!("{launch_id}.output"))
}

/// H-1c: where an ended hosted session's last output is kept (a day), while it is.
pub fn ended_output(root: &Path, launch_id: &str) -> Option<PathBuf> {
    if !crate::launches::valid_id(launch_id) {
        return None;
    }
    let path = output_path(root, launch_id);
    std::fs::symlink_metadata(&path)
        .is_ok_and(|m| m.file_type().is_file())
        .then_some(path)
}

/// Removes ended sessions' output older than a day.
fn prune_outputs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root.join("sessions")) else {
        return;
    };
    for entry in entries.flatten().take(4096) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "output")
            && entry.metadata().is_ok_and(|m| {
                m.is_file()
                    && m.modified()
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > OUTPUT_KEPT)
            })
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn write_descriptor(root: &Path, launch_id: &str, socket: &Path, viewers: usize) {
    let dir = root.join("sessions");
    if switchboard_core::private_fs::private_dir(&dir).is_err() {
        return;
    }
    let body = json!({
        "launch_id": launch_id,
        "pid": std::process::id(),
        "socket": socket,
        "viewers": viewers,
    });
    let _ = switchboard_core::private_fs::private_write(
        &descriptor_path(root, launch_id),
        body.to_string().as_bytes(),
    );
}

fn pid_alive(pid: i64) -> bool {
    pid > 0 && pid <= i32::MAX as i64 && unsafe { libc::kill(pid as i32, 0) } == 0
}

/// SB-400: every launch whose host runs now, with the host's pid. A hosted continue has no
/// home of its own, so the session list, the watch and the rows find it here (R4 review).
pub fn live_hosts(root: &Path) -> Vec<(String, u32)> {
    let Ok(entries) = std::fs::read_dir(root.join("sessions")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?.to_owned();
            if !crate::launches::valid_id(&id) {
                return None;
            }
            let bytes =
                switchboard_core::private_fs::read_private(&e.path(), MAX_DESCRIPTOR).ok()?;
            let pid = serde_json::from_slice::<Value>(&bytes).ok()?["pid"].as_i64()?;
            pid_alive(pid).then_some((id, u32::try_from(pid).ok()?))
        })
        .collect()
}

/// What the session list says about a launch's host: `{attachable: true, viewers}` while its
/// host runs; None when it has none (not hosted, or ended).
pub fn attachable(root: &Path, launch_id: &str) -> Option<Value> {
    if !crate::launches::valid_id(launch_id) {
        return None;
    }
    let bytes = switchboard_core::private_fs::read_private(
        &descriptor_path(root, launch_id),
        MAX_DESCRIPTOR,
    )
    .ok()?;
    let d: Value = serde_json::from_slice(&bytes).ok()?;
    if !pid_alive(d["pid"].as_i64()?) {
        return None;
    }
    Some(json!({"attachable": true, "viewers": d["viewers"].as_u64().unwrap_or(0)}))
}

struct Inner {
    scrollback: VecDeque<u8>,
    /// H-1c: the screen as a terminal shows it now, so a viewer that attaches gets the screen
    /// (the alternate one included), not a replay of raw output.
    screen: vt100::Parser,
    viewers: Vec<(u64, UnixStream)>,
    next: u64,
    exit: Option<i32>,
    last_activity: std::time::Instant,
    /// Set when the host itself ended the session (`idle`).
    reason: Option<&'static str>,
    /// SB-397 R1: the program to start once the running process exits (a restart in place).
    restart: Option<PathBuf>,
    /// SB-397 R1: the last process exited with no restart waiting; the session is ending. Only
    /// then does the output reader stop: an end of output it read before (a restart in place
    /// closes the terminal's other end for a moment) is not the session's end (SB-413).
    finished: bool,
}

fn nonzero(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n).unwrap_or(std::num::NonZeroU16::MIN)
}

struct Session {
    /// The session's process group: its process leads it (setsid). A restart in place starts a
    /// new process, so it changes.
    pid: std::sync::atomic::AtomicI32,
    root: PathBuf,
    launch_id: String,
    socket: PathBuf,
    master: Mutex<File>,
    inner: Mutex<Inner>,
}

impl Session {
    fn pid(&self) -> i32 {
        self.pid.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn output(&self, bytes: &[u8]) {
        let mut inner = self.lock();
        inner.screen.process(bytes);
        inner.last_activity = std::time::Instant::now();
        inner.scrollback.extend(bytes);
        let excess = inner.scrollback.len().saturating_sub(SCROLLBACK);
        inner.scrollback.drain(..excess);
        let before = inner.viewers.len();
        inner
            .viewers
            .retain_mut(|(_, stream)| write_frame(stream, OUTPUT, bytes).is_ok());
        if inner.viewers.len() != before {
            write_descriptor(
                &self.root,
                &self.launch_id,
                &self.socket,
                inner.viewers.len(),
            );
        }
    }
    /// The size of the last viewer that sent one; the others are told it (`RESIZE`).
    fn resize(&self, rows: u16, cols: u16, from: Option<u64>) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        {
            let master = self.master.lock().unwrap_or_else(|p| p.into_inner());
            unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) };
        }
        let mut inner = self.lock();
        inner.screen.set_size(nonzero(rows), nonzero(cols));
        let mut frame = Vec::with_capacity(4);
        frame.extend(rows.to_be_bytes());
        frame.extend(cols.to_be_bytes());
        inner.viewers.retain_mut(|(id, stream)| {
            Some(*id) == from || write_frame(stream, RESIZE, &frame).is_ok()
        });
    }
    /// Ends the session as `reason`: a hangup to its process group, a `SIGTERM` 5 s later if it
    /// is still running. Its exit then reaches every viewer as usual.
    fn end(self: &Arc<Self>, reason: &'static str) {
        // The pid is read under the lock that marks the end, so a restart that starts the next
        // process under the same lock is either seen here (its pid) or sees the end (and starts
        // nothing).
        let pid = {
            let mut inner = self.lock();
            if inner.exit.is_some() {
                return;
            }
            inner.reason.get_or_insert(reason);
            // SB-397 R1: an end wins over a restart that waits.
            inner.restart = None;
            self.pid()
        };
        unsafe { libc::kill(-pid, libc::SIGHUP) };
        let session = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            if session.lock().exit.is_none() && session.pid() == pid {
                unsafe { libc::kill(-pid, libc::SIGTERM) };
            }
        });
    }
    /// SB-397 R1: ends the running process as `end` does, and has the host start `program` on
    /// the same terminal once it exits. Refused when the session already ended or a restart waits.
    fn restart(self: &Arc<Self>, program: PathBuf) -> bool {
        let pid = {
            let mut inner = self.lock();
            if inner.exit.is_some()
                || inner.finished
                || inner.reason.is_some()
                || inner.restart.is_some()
            {
                return false;
            }
            inner.restart = Some(program);
            self.pid()
        };
        unsafe { libc::kill(-pid, libc::SIGHUP) };
        let session = self.clone();
        std::thread::spawn(move || {
            for (wait, signal) in [(5, libc::SIGTERM), (5, libc::SIGKILL)] {
                std::thread::sleep(Duration::from_secs(wait));
                if session.pid() != pid {
                    return;
                }
                unsafe { libc::kill(-pid, signal) };
            }
        });
        true
    }
    /// What a viewer that attaches is shown first: the screen, entering the alternate screen
    /// when the session is on it.
    fn snapshot(inner: &Inner) -> Vec<u8> {
        let screen = inner.screen.screen();
        let mut out = Vec::new();
        if screen.alternate_screen() {
            out.extend(b"\x1b[?1049h");
        }
        out.extend(screen.state_formatted().into_bytes());
        out
    }
}

fn open_pty(rows: u16, cols: u16) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut size = libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let r = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            // A raw pointer: macOS declares it `*mut`, Linux `*const`.
            std::ptr::addr_of_mut!(size),
        )
    };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((master, slave))
}

/// The terminal's other end opened again by its name, for the next process of a restart.
fn reopen_slave(session: &Session) -> io::Result<OwnedFd> {
    static NAME: Mutex<()> = Mutex::new(());
    let name = {
        let _one = NAME.lock().unwrap_or_else(|p| p.into_inner());
        let master = session.master.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: `ptsname` returns a static buffer; the lock keeps it this thread's.
        let ptr = unsafe { libc::ptsname(master.as_raw_fd()) };
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        unsafe { std::ffi::CStr::from_ptr(ptr) }.to_owned()
    };
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Starts `program` as the leader of a new session whose controlling terminal is `slave`.
fn spawn_on(program: &Path, slave: &OwnedFd) -> io::Result<std::process::Child> {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave.try_clone()?));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()
}

/// What a host starts.
pub struct Start<'a> {
    pub root: &'a Path,
    pub launch_id: &'a str,
    pub program: &'a Path,
    pub rows: u16,
    pub cols: u16,
    /// Close the session after this long with no viewer, no input and no output; None never.
    pub idle: Option<Duration>,
}

/// How a hosted session ended.
#[derive(Debug, PartialEq, Eq)]
pub struct Ended {
    /// Its exit status, 128 + the signal for one a signal ended.
    pub exit_code: i32,
    /// `exited`, `signal`, or `idle` when the host closed it.
    pub reason: &'static str,
}

/// Runs a session on a PTY of its own and serves its socket until the session's process exits;
/// returns its exit status (128 + the signal for one a signal ended). `ready` runs once the
/// socket listens, so whoever started the host can attach.
pub fn serve(start: Start<'_>, ready: impl FnOnce()) -> Result<Ended, String> {
    if !crate::launches::valid_id(start.launch_id) {
        return Err(NOT_FOUND.into());
    }
    let socket = socket_path(start.root, start.launch_id)?;
    // A socket left by a host that is gone is replaced; a live one is never taken over.
    if UnixStream::connect(&socket).is_ok() {
        return Err("A session with this launch id is already open.".into());
    }
    let _ = std::fs::remove_file(&socket);
    let listener =
        UnixListener::bind(&socket).map_err(|_| "The session socket could not be made.")?;
    let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));

    let (master, slave) =
        open_pty(start.rows, start.cols).map_err(|_| "No terminal could be opened.")?;
    // `openpty`'s size argument is not honoured on every macOS: set it on the terminal itself.
    let size = libc::winsize {
        ws_row: start.rows.max(1),
        ws_col: start.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) };
    let mut child = spawn_on(start.program, &slave).map_err(|_| {
        let _ = std::fs::remove_file(&socket);
        "The session could not start.".to_string()
    })?;
    // The host keeps no end of the terminal but the master: the session's end reads as EIO.
    drop(slave);

    let reader = File::from(
        master
            .try_clone()
            .map_err(|_| "No terminal could be opened.")?,
    );
    let session = Arc::new(Session {
        pid: std::sync::atomic::AtomicI32::new(child.id() as i32),
        root: start.root.to_owned(),
        launch_id: start.launch_id.to_owned(),
        socket: socket.clone(),
        master: Mutex::new(File::from(master)),
        inner: Mutex::new(Inner {
            scrollback: VecDeque::new(),
            screen: vt100::Parser::new(nonzero(start.rows), nonzero(start.cols), 0),
            viewers: Vec::new(),
            next: 0,
            exit: None,
            last_activity: std::time::Instant::now(),
            reason: None,
            restart: None,
            finished: false,
        }),
    });
    write_descriptor(start.root, start.launch_id, &socket, 0);
    prune_outputs(start.root);
    if let Some(idle) = start.idle {
        let session = session.clone();
        let (root, launch) = (start.root.to_owned(), start.launch_id.to_owned());
        std::thread::spawn(move || {
            let check = (idle / 4).clamp(Duration::from_millis(100), Duration::from_secs(30));
            loop {
                std::thread::sleep(check);
                let inner = session.lock();
                if inner.exit.is_some() {
                    return;
                }
                if inner.viewers.is_empty() && inner.last_activity.elapsed() >= idle {
                    drop(inner);
                    // SB-396 D3: a session stopped on a limit waits for an account; its host
                    // continues it, so its window is not closed under it.
                    let id = crate::launches::session_of(&root, &launch);
                    let now = crate::monitor::now();
                    // SB-396 wave C: nor one waiting on a wake-up, a monitor or a cron job of its own.
                    let stopped = id.as_deref().is_some_and(|s| {
                        crate::recovery::session_has_live_stop(&root, s, now)
                            || crate::monitors::holds(
                                &crate::monitors::read(&root, s),
                                now,
                                now - idle.as_secs() as i64,
                            )
                    });
                    if stopped {
                        continue;
                    }
                    session.end("idle");
                    // SB-396 D5: the session can be continued as itself later.
                    if let Some(id) = id {
                        crate::resumable::record(
                            &root,
                            serde_json::json!({"session": id, "launch_id": launch,
                                "reason": "host_idle", "at": crate::monitor::now(), "by": "host"}),
                        );
                    }
                    return;
                }
            }
        });
    }

    let (drained_tx, drained_rx) = std::sync::mpsc::channel::<()>();
    {
        let session = session.clone();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => session.output(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    // End of output or EIO once every holder of the terminal's other end is gone.
                    // It ends the reader only once the host says the session is finished: during
                    // a restart in place the other end comes back with the next process, and an
                    // EIO read just before the next process started must not end the reader after
                    // it started (SB-413: on Linux the reader waited for the session's lock while
                    // the next process was spawned under it, then read the restart as over and
                    // stopped, and the viewers saw nothing of the next process).
                    _ => {
                        #[cfg(test)]
                        tests::pause_reader(&session.launch_id);
                        if session.lock().finished {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            let _ = drained_tx.send(());
        });
    }
    {
        let session = session.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let session = session.clone();
                std::thread::spawn(move || viewer(&session, stream));
            }
        });
    }
    ready();

    let status = loop {
        let status = child.wait().map_err(|_| {
            session.lock().finished = true;
            "The session could not be watched."
        })?;
        // SB-397 R1: a restart in place starts the session's next process on the same terminal;
        // the viewers stay. One that cannot start ends the session as it would have.
        let program = {
            let mut inner = session.lock();
            let program = inner.restart.take();
            inner.finished = program.is_none();
            program
        };
        let Some(program) = program else {
            break status;
        };
        // The script that led the session may exit before the agent it ran: the next process
        // starts only once the whole old process group is gone, never beside it (no two copies).
        let old = session.pid();
        for n in 0..100 {
            if unsafe { libc::kill(-old, 0) } != 0 {
                break;
            }
            if n == 99 {
                unsafe { libc::kill(-old, libc::SIGKILL) };
                std::thread::sleep(Duration::from_millis(200));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // An end (a person's *End*, the idle close) that came while the old process was going
        // wins over the restart: nothing starts after it. The check, the start and the new pid
        // happen under one lock, which `end` also takes (review of 2026-11-16, defect 2).
        // macOS revokes every open end of the terminal when its session leader exits: the next
        // process gets the terminal opened again by name. Opened before the session's lock, so a
        // viewer's write held on the terminal never keeps an end waiting behind it.
        let slave = reopen_slave(&session);
        let started = {
            let mut inner = session.lock();
            if inner.reason.is_some() {
                inner.finished = true;
                None
            } else {
                let next = slave.and_then(|slave| spawn_on(&program, &slave));
                if let Ok(next) = &next {
                    session
                        .pid
                        .store(next.id() as i32, std::sync::atomic::Ordering::SeqCst);
                } else {
                    inner.finished = true;
                }
                Some(next)
            }
        };
        match started {
            Some(Ok(next)) => {
                session.output(b"\r\n[Switchboard restarted this session]\r\n");
                child = next;
            }
            _ => break status,
        }
    };
    use std::os::unix::process::ExitStatusExt;
    let code = status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1);
    let signal = status.signal();
    // The output the process wrote just before it ended reaches the viewers before its exit;
    // a child it left holding the terminal does not hold the end up past a second.
    let _ = drained_rx.recv_timeout(Duration::from_secs(1));
    let (viewers, reason, kept) = {
        let mut inner = session.lock();
        inner.exit = Some(code);
        let reason = inner
            .reason
            .unwrap_or(if signal.is_some() { "signal" } else { "exited" });
        let kept: Vec<u8> = inner.scrollback.iter().copied().collect();
        (std::mem::take(&mut inner.viewers), reason, kept)
    };
    let mut exit = json!({"exit_code": code, "reason": reason});
    if let Some(signal) = signal {
        exit["signal"] = signal.into();
    }
    let exit = exit.to_string();
    for (_, mut stream) in viewers {
        let _ = write_frame(&mut stream, EXIT, exit.as_bytes());
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_file(descriptor_path(start.root, start.launch_id));
    let _ = switchboard_core::private_fs::private_write(
        &output_path(start.root, start.launch_id),
        &kept,
    );
    Ok(Ended {
        exit_code: code,
        reason,
    })
}

fn refuse(stream: &mut UnixStream, error: &str) {
    let _ = write_frame(
        stream,
        ERROR,
        json!({"error": error}).to_string().as_bytes(),
    );
}

/// The user at the other end of `stream`: the socket folder already keeps everyone else out, and
/// this is checked anyway (Fabric Dashboards' ask on `22ccd485`).
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let fd = stream.as_raw_fd();
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let r = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (r == 0).then_some(cred.uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        (unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } == 0).then_some(uid)
    }
}

fn viewer(session: &Arc<Session>, mut stream: UnixStream) {
    if peer_uid(&stream) != Some(unsafe { libc::getuid() }) {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(VIEWER_WRITE));
    let Ok((HELLO, hello)) = read_frame(&mut stream) else {
        return;
    };
    let hello: Value = serde_json::from_slice(&hello).unwrap_or(Value::Null);
    if hello["launch_id"].as_str() != Some(session.launch_id.as_str()) {
        return refuse(&mut stream, NOT_FOUND);
    }
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let id = {
        let mut inner = session.lock();
        if let Some(code) = inner.exit {
            let _ = write_frame(
                &mut stream,
                EXIT,
                json!({"exit_code": code}).to_string().as_bytes(),
            );
            return;
        }
        if inner.viewers.len() >= MAX_VIEWERS {
            drop(inner);
            return refuse(&mut stream, TOO_MANY_VIEWERS);
        }
        let ack = json!({
            "protocol": PROTOCOL,
            "launch_id": session.launch_id,
            "viewers": inner.viewers.len() + 1,
            // SB-397 R1: what this host does beyond protocol 1's frames.
            "can": ["restart"],
        });
        let replay = Session::snapshot(&inner);
        if write_frame(&mut stream, HELLO, ack.to_string().as_bytes()).is_err()
            || write_frame(&mut stream, OUTPUT, &replay).is_err()
        {
            return;
        }
        let id = inner.next;
        inner.next += 1;
        inner.viewers.push((id, writer));
        write_descriptor(
            &session.root,
            &session.launch_id,
            &session.socket,
            inner.viewers.len(),
        );
        id
    };
    if let (Some(rows), Some(cols)) = (hello["rows"].as_u64(), hello["cols"].as_u64()) {
        session.resize(
            rows.clamp(1, 1000) as u16,
            cols.clamp(1, 1000) as u16,
            Some(id),
        );
    }
    let _ = stream.set_read_timeout(None);
    loop {
        match read_frame(&mut stream) {
            Ok((INPUT, bytes)) => {
                session.lock().last_activity = std::time::Instant::now();
                let mut master = session.master.lock().unwrap_or_else(|p| p.into_inner());
                if master.write_all(&bytes).is_err() {
                    break;
                }
            }
            Ok((RESIZE, size)) if size.len() == 4 => {
                let rows = u16::from_be_bytes([size[0], size[1]]);
                let cols = u16::from_be_bytes([size[2], size[3]]);
                if rows > 0 && cols > 0 {
                    session.resize(rows, cols, Some(id));
                }
            }
            Ok((HISTORY, _)) => {
                let kept = {
                    let inner = session.lock();
                    let (a, b) = inner.scrollback.as_slices();
                    [a, b].concat()
                };
                // The viewer's own stream is written by the output thread too: take its lock.
                let mut inner = session.lock();
                if let Some((_, w)) = inner.viewers.iter_mut().find(|(v, _)| *v == id) {
                    if write_frame(w, HISTORY, &kept).is_err() {
                        break;
                    }
                }
            }
            Ok((END, _)) => session.end("ended"),
            Ok((RESTART, body)) => {
                let program = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|v| v["program"].as_str().map(PathBuf::from))
                    .filter(|p| restart_program_allowed(&session.root, p));
                let accepted = program.is_some_and(|p| session.restart(p));
                let mut inner = session.lock();
                if let Some((_, w)) = inner.viewers.iter_mut().find(|(v, _)| *v == id) {
                    let answer = json!({"accepted": accepted}).to_string();
                    if write_frame(w, RESTART, answer.as_bytes()).is_err() {
                        break;
                    }
                }
            }
            Ok((DETACH, _)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    let mut inner = session.lock();
    let before = inner.viewers.len();
    inner.viewers.retain(|(v, _)| *v != id);
    if inner.viewers.len() != before && inner.exit.is_none() {
        write_descriptor(
            &session.root,
            &session.launch_id,
            &session.socket,
            inner.viewers.len(),
        );
    }
}

/// A viewer's connection, attached and past its `HELLO`.
#[derive(Debug)]
pub struct Attached {
    pub stream: UnixStream,
    pub viewers: u64,
    /// What the host offers beyond protocol 1 (`restart`, SB-397 R1).
    pub can: Vec<String>,
}

/// Attaches to the session of `launch_id`: connects, says hello with the viewer's size and
/// reads the answer. The scrollback follows as the first `OUTPUT`.
pub fn attach(root: &Path, launch_id: &str, rows: u16, cols: u16) -> Result<Attached, String> {
    connect(root, launch_id, Some((rows, cols)))
}

/// H-1d: ends the hosted session of `launch_id` and waits (up to 15 s) for its exit:
/// `{launch_id, exit_code, reason}`. Connects as a viewer with no size, so the session's size
/// stays the one its viewers set.
pub fn end(root: &Path, launch_id: &str) -> Result<Value, String> {
    let attached = connect(root, launch_id, None)?;
    let mut stream = attached.stream;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    write_frame(&mut stream, END, b"").map_err(|_| NOT_FOUND.to_string())?;
    loop {
        match read_frame(&mut stream) {
            Ok((EXIT, body)) => {
                let exit: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                return Ok(json!({
                    "launch_id": launch_id,
                    "exit_code": exit["exit_code"],
                    "reason": exit["reason"],
                }));
            }
            Ok(_) => continue,
            Err(_) => return Err(END_UNCONFIRMED.into()),
        }
    }
}
pub const END_UNCONFIRMED: &str =
    "The session was asked to end, and its end was not seen within 15 seconds.";

/// The folder a restart's program must be in: the scripts Switchboard writes for restarts.
pub const RESTART_DIR: &str = "restarts";

/// SB-397 R1: only a regular file Switchboard wrote under `<data>/restarts/`, owned by this user,
/// may be started by a restart: a viewer cannot have the host run anything else.
fn restart_program_allowed(root: &Path, program: &Path) -> bool {
    let Ok(dir) = root.join(RESTART_DIR).canonicalize() else {
        return false;
    };
    let Ok(path) = program.canonicalize() else {
        return false;
    };
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    use std::os::unix::fs::MetadataExt;
    path.parent() == Some(dir.as_path())
        && meta.is_file()
        && meta.uid() == unsafe { libc::getuid() }
        && meta.mode() & 0o022 == 0
        && meta.mode() & 0o100 != 0
}

/// SB-397 R1: whether the host of `launch_id` restarts its session in place (its `HELLO` answer
/// lists `restart`). Attaches with no size, so the session's size stays its viewers', and detaches.
pub fn offers_restart(root: &Path, launch_id: &str) -> bool {
    let Ok(mut attached) = connect(root, launch_id, None) else {
        return false;
    };
    let _ = write_frame(&mut attached.stream, DETACH, b"");
    attached.can.iter().any(|c| c == "restart")
}

/// How a host answered a restart request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    Accepted,
    /// It cannot (it predates restarts, or a restart already waits).
    Refused,
    /// The request went out and no answer came back (a timeout or a closed socket): it may have
    /// been taken, so it is neither a refusal nor an acceptance (R3 review).
    Unknown,
}

/// SB-397 R1: asks the host of `launch_id` to restart its session in place with `program`.
/// `Err` when no host answers before the request is written.
pub fn restart(root: &Path, launch_id: &str, program: &Path) -> Result<Restart, String> {
    let attached = connect(root, launch_id, None)?;
    if !attached.can.iter().any(|c| c == "restart") {
        return Ok(Restart::Refused);
    }
    let mut stream = attached.stream;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let body = json!({"program": program}).to_string();
    write_frame(&mut stream, RESTART, body.as_bytes()).map_err(|_| NOT_FOUND.to_string())?;
    loop {
        match read_frame(&mut stream) {
            Ok((RESTART, body)) => {
                let answer: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let _ = write_frame(&mut stream, DETACH, b"");
                return Ok(if answer["accepted"] == true {
                    Restart::Accepted
                } else {
                    Restart::Refused
                });
            }
            Ok(_) => continue,
            Err(_) => return Ok(Restart::Unknown),
        }
    }
}

fn connect(root: &Path, launch_id: &str, size: Option<(u16, u16)>) -> Result<Attached, String> {
    if !crate::launches::valid_id(launch_id) {
        return Err(NOT_FOUND.into());
    }
    let mut stream =
        UnixStream::connect(socket_path(root, launch_id)?).map_err(|_| NOT_FOUND.to_string())?;
    let mut hello = json!({"protocol": PROTOCOL, "launch_id": launch_id});
    if let Some((rows, cols)) = size {
        hello["rows"] = rows.into();
        hello["cols"] = cols.into();
    }
    write_frame(&mut stream, HELLO, hello.to_string().as_bytes())
        .map_err(|_| NOT_FOUND.to_string())?;
    // A host that takes the connection and never answers is not waited for forever (SB-413);
    // the stream is handed on with no timeout, as before.
    let _ = stream.set_read_timeout(Some(HELLO_WAIT));
    let answer = read_frame(&mut stream);
    let _ = stream.set_read_timeout(None);
    match answer {
        Ok((HELLO, ack)) => {
            let ack: Value = serde_json::from_slice(&ack).unwrap_or(Value::Null);
            if ack["protocol"].as_u64() != Some(PROTOCOL) {
                return Err(NOT_FOUND.into());
            }
            Ok(Attached {
                stream,
                viewers: ack["viewers"].as_u64().unwrap_or(1),
                can: ack["can"]
                    .as_array()
                    .map(|c| {
                        c.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        }
        Ok((ERROR, body)) => Err(serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| NOT_FOUND.into())),
        Ok((EXIT, _)) => Err("The session has ended.".into()),
        _ => Err(NOT_FOUND.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SB-413: the launch id whose output reader pauses after an end of output, and for how
    /// long, so a test can make the reader see a restart's EIO late, as a loaded machine does.
    static READER_PAUSE: Mutex<Option<(String, Duration)>> = Mutex::new(None);

    pub(super) fn pause_reader(launch_id: &str) {
        let pause = READER_PAUSE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|(id, _)| id == launch_id)
            .map(|(_, d)| *d);
        if let Some(pause) = pause {
            std::thread::sleep(pause);
        }
    }

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("launch.command");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    /// How long a test waits for anything a session should show: long enough for a loaded CI
    /// runner, short enough that a stuck session fails its test instead of hanging the job
    /// (SB-413).
    const WAIT: Duration = Duration::from_secs(30);

    /// A test viewer: attached, with a read timeout so no read waits past `WAIT`.
    fn watch(root: &Path, id: &str, rows: u16, cols: u16) -> Attached {
        let attached = attach(root, id, rows, cols).unwrap();
        attached.stream.set_read_timeout(Some(WAIT)).unwrap();
        attached
    }

    /// Reads until `needle` is seen, the session's exit arrives, the stream closes or `WAIT`
    /// passes; answers what was seen and the exit code, if one came.
    fn until(stream: &mut UnixStream, needle: &str) -> (String, Option<i32>) {
        // macOS refuses the option (EINVAL) on a socket the host already shut down; reads then
        // end at once anyway.
        let _ = stream.set_read_timeout(Some(WAIT));
        let mut seen = String::new();
        loop {
            match read_frame(stream) {
                Ok((OUTPUT, bytes)) => {
                    seen.push_str(&String::from_utf8_lossy(&bytes));
                    if !needle.is_empty() && seen.contains(needle) {
                        return (seen, None);
                    }
                }
                Ok((EXIT, body)) => {
                    let v: Value = serde_json::from_slice(&body).unwrap();
                    return (seen, v["exit_code"].as_i64().map(|c| c as i32));
                }
                Ok(_) => {}
                Err(_) => return (seen, None),
            }
        }
    }

    /// `until`, failing with what was seen when `needle` did not come.
    fn saw(stream: &mut UnixStream, needle: &str) -> String {
        let (seen, exit) = until(stream, needle);
        assert!(
            seen.contains(needle),
            "waited {WAIT:?} for {needle:?}; saw {seen:?}, exit {exit:?}"
        );
        seen
    }

    /// The session's exit code, failing with what was seen when no exit came within `WAIT`.
    fn exit_of(stream: &mut UnixStream) -> i32 {
        let (seen, exit) = until(stream, "");
        exit.unwrap_or_else(|| panic!("no exit within {WAIT:?}; saw {seen:?}"))
    }

    #[test]
    fn a_frame_round_trips_and_an_oversized_one_is_refused() {
        let mut buf = Vec::new();
        write_frame(&mut buf, INPUT, b"hi").unwrap();
        assert_eq!(
            read_frame(&mut buf.as_slice()).unwrap(),
            (INPUT, b"hi".to_vec())
        );
        let mut big = vec![OUTPUT];
        big.extend(((MAX_FRAME + 1) as u32).to_be_bytes());
        assert!(read_frame(&mut big.as_slice()).is_err());
    }

    #[test]
    fn two_viewers_share_one_session_a_late_one_sees_the_scrollback_and_both_get_its_exit() {
        // SB-342 H-1: the session outlives a viewer that detaches; another app attaches by launch
        // id and sees what was printed before it came; the exit reaches every viewer.
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(
            &root,
            "echo ready; read line; echo \"got $line\"; read line; exit 7",
        );
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        assert_eq!(attachable(&root, &id).unwrap()["viewers"], 0);

        let mut first = watch(&root, &id, 24, 80);
        assert_eq!(first.viewers, 1);
        saw(&mut first.stream, "ready");
        write_frame(&mut first.stream, INPUT, b"one\n").unwrap();
        saw(&mut first.stream, "got one");
        write_frame(&mut first.stream, DETACH, b"").unwrap();
        drop(first);

        // A viewer that detached left the session running; a new one gets what came before.
        let mut second = watch(&root, &id, 30, 100);
        let replay = saw(&mut second.stream, "got one");
        assert!(replay.contains("ready"), "{replay:?}");
        let mut third = watch(&root, &id, 30, 100);
        assert_eq!(third.viewers, 2);
        write_frame(&mut third.stream, INPUT, b"two\n").unwrap();
        assert_eq!(exit_of(&mut second.stream), 7);
        assert_eq!(exit_of(&mut third.stream), 7);
        assert_eq!(
            host.join().unwrap(),
            Ended {
                exit_code: 7,
                reason: "exited"
            }
        );

        // Ended: nothing to attach, no socket and no descriptor left behind.
        assert!(attachable(&root, &id).is_none());
        assert!(attach(&root, &id, 24, 80).is_err());
        assert!(!socket_path(&root, &id).unwrap().exists());
        // H-1c: its last output stays readable for a day.
        let kept = std::fs::read(ended_output(&root, &id).unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&kept).contains("got one"));
    }

    #[test]
    fn a_restart_in_place_keeps_the_viewers_and_runs_the_next_program_on_the_same_terminal() {
        // SB-397 R1: a restart wave restarts a hosted session in its host: the viewer attached
        // before stays attached, sees the next process's output and gets no exit in between.
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(&root, "echo first; read line; exit 0");
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        let mut viewer = watch(&root, &id, 24, 80);
        assert!(viewer.can.iter().any(|c| c == "restart"));
        saw(&mut viewer.stream, "first");

        // Only a program of this user under `restarts/` is started.
        let outside = script(&root, "echo never");
        assert_eq!(restart(&root, &id, &outside), Ok(Restart::Refused));
        let dir = root.join(RESTART_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let next = dir.join("next.command");
        std::fs::write(&next, "#!/bin/sh\necho second; read line; exit 3\n").unwrap();
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(restart(&root, &id, &next), Ok(Restart::Accepted));
        let (seen, exit) = until(&mut viewer.stream, "second");
        assert!(
            seen.contains("second"),
            "the next program's output reached the viewer within {WAIT:?}; saw {seen:?}, exit {exit:?}"
        );
        assert_eq!(
            exit, None,
            "no exit reached the viewer between the two processes"
        );
        assert!(seen.contains("restarted this session"), "{seen}");
        assert!(
            attachable(&root, &id).is_some(),
            "the session is still open"
        );

        write_frame(&mut viewer.stream, INPUT, b"done\n").unwrap();
        assert_eq!(exit_of(&mut viewer.stream), 3);
        assert_eq!(host.join().unwrap().exit_code, 3);
    }

    #[test]
    fn a_restart_in_place_keeps_its_output_when_the_reader_sees_the_old_end_late() {
        // SB-413: the output reader read the old process's end (EIO) just before the next
        // process started, and acted on it only after: it took the restart as over and stopped,
        // so the viewers never saw the next process (Linux CI, run 44076842446). The pause holds
        // the reader past the next process's start, as a loaded machine did.
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(&root, "echo first; read line; exit 0");
        let id = crate::launches::new_id();
        *READER_PAUSE.lock().unwrap_or_else(|p| p.into_inner()) =
            Some((id.clone(), Duration::from_millis(600)));
        let host = start_host(&root, &id, &program, None);
        let mut viewer = watch(&root, &id, 24, 80);
        saw(&mut viewer.stream, "first");
        let dir = root.join(RESTART_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let next = dir.join("next.command");
        std::fs::write(&next, "#!/bin/sh\necho second; read line; exit 3\n").unwrap();
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(restart(&root, &id, &next), Ok(Restart::Accepted));
        saw(&mut viewer.stream, "second");
        write_frame(&mut viewer.stream, INPUT, b"done\n").unwrap();
        assert_eq!(exit_of(&mut viewer.stream), 3);
        assert_eq!(host.join().unwrap().exit_code, 3);
        *READER_PAUSE.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    #[test]
    fn an_end_wins_over_a_restart_and_a_restart_after_the_end_is_refused() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        // A session that ignores the hangup for a moment, so the end comes while the restart waits.
        let program = script(
            &root,
            "trap 'sleep 1; exit 0' HUP; echo up; while :; do sleep 1; done",
        );
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        let mut viewer = watch(&root, &id, 24, 80);
        saw(&mut viewer.stream, "up");
        let dir = root.join(RESTART_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let next = dir.join("next.command");
        std::fs::write(&next, "#!/bin/sh\necho second\n").unwrap();
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(restart(&root, &id, &next), Ok(Restart::Accepted));
        write_frame(&mut viewer.stream, END, b"").unwrap();
        let (seen, exit) = until(&mut viewer.stream, "");
        assert!(exit.is_some(), "the end reached the viewer; saw {seen:?}");
        assert!(
            !seen.contains("second"),
            "the restart did not run after the end: {seen}"
        );
        assert_eq!(host.join().unwrap().reason, "ended");
        // A script that is not executable is never started.
        let id = crate::launches::new_id();
        let program = script(&root, "echo up; read line");
        let host = start_host(&root, &id, &program, None);
        let plain = dir.join("plain.command");
        std::fs::write(&plain, "#!/bin/sh\necho x\n").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(restart(&root, &id, &plain), Ok(Restart::Refused));
        end(&root, &id).unwrap();
        host.join().unwrap();
    }

    fn start_host(root: &Path, id: &str, program: &Path, idle: Option<Duration>) -> Host {
        let (tx, rx) = std::sync::mpsc::channel();
        let (done_tx, done) = std::sync::mpsc::channel();
        let (root, id, program) = (root.to_owned(), id.to_owned(), program.to_owned());
        std::thread::spawn(move || {
            let ended = serve(
                Start {
                    root: &root,
                    launch_id: &id,
                    program: &program,
                    rows: 24,
                    cols: 80,
                    idle,
                },
                move || tx.send(()).unwrap(),
            );
            let _ = done_tx.send(ended);
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the host listens within 10 s");
        Host { done }
    }

    /// A host running on a thread of the test; `join` waits for its end at most `WAIT`, so a
    /// session that never ends fails its test instead of hanging the job (SB-413).
    struct Host {
        done: std::sync::mpsc::Receiver<Result<Ended, String>>,
    }

    impl Host {
        fn join(self) -> Result<Ended, String> {
            match self.done.recv_timeout(WAIT) {
                Ok(ended) => ended,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("the host did not end within {WAIT:?}")
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("the host's thread panicked")
                }
            }
        }
    }

    #[test]
    fn a_late_viewer_gets_the_screen_on_the_alternate_screen_and_a_resize_reaches_the_others() {
        // H-1c (Fabric Dashboards, `7972274c`): a full-screen program is shown as its screen,
        // not as a replay of its raw output; the size of the last viewer that sent one wins and
        // the other viewers are told it.
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(
            &root,
            "printf '\\033[?1049h\\033[2J\\033[HFULLSCREEN'; read line; exit 0",
        );
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        let mut first = watch(&root, &id, 24, 80);
        saw(&mut first.stream, "FULLSCREEN");
        let mut second = watch(&root, &id, 24, 80);
        let Ok((OUTPUT, screen)) = read_frame(&mut second.stream) else {
            panic!("no screen");
        };
        assert!(screen.starts_with(b"\x1b[?1049h"));
        assert!(String::from_utf8_lossy(&screen).contains("FULLSCREEN"));
        let mut size = Vec::new();
        size.extend(40u16.to_be_bytes());
        size.extend(120u16.to_be_bytes());
        write_frame(&mut second.stream, RESIZE, &size).unwrap();
        loop {
            match read_frame(&mut first.stream).unwrap() {
                // The second viewer's hello sent its own size first.
                (RESIZE, told) if told == size => break,
                _ => continue,
            }
        }
        write_frame(&mut first.stream, INPUT, b"x\n").unwrap();
        assert_eq!(host.join().unwrap().reason, "exited");
    }

    #[test]
    fn a_session_nobody_watches_and_nothing_moves_in_is_closed_as_idle() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(&root, "sleep 60");
        let id = crate::launches::new_id();
        let started = std::time::Instant::now();
        let host = start_host(&root, &id, &program, Some(Duration::from_millis(300)));
        let ended = host.join().unwrap();
        assert_eq!(ended.reason, "idle");
        assert!(started.elapsed() < Duration::from_secs(20));
        assert!(attachable(&root, &id).is_none());
    }

    #[test]
    fn end_closes_the_session_for_every_viewer_as_ended() {
        // H-1d: a viewer's END ends the session (detaching never does); every viewer gets its exit.
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(&root, "echo up; sleep 60");
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        let mut viewer = watch(&root, &id, 24, 80);
        saw(&mut viewer.stream, "up");
        let ended = end(&root, &id).unwrap();
        assert_eq!(ended["reason"], "ended");
        assert_eq!(
            exit_of(&mut viewer.stream),
            ended["exit_code"].as_i64().unwrap() as i32
        );
        assert_eq!(host.join().unwrap().reason, "ended");
        assert_eq!(end(&root, &id).unwrap_err(), NOT_FOUND);
    }

    #[test]
    fn history_answers_the_raw_output_beyond_the_screen() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(
            &root,
            "for i in $(seq 1 60); do echo line$i; done; read line",
        );
        let id = crate::launches::new_id();
        let host = start_host(&root, &id, &program, None);
        let mut viewer = watch(&root, &id, 24, 80);
        saw(&mut viewer.stream, "line60");
        write_frame(&mut viewer.stream, HISTORY, b"").unwrap();
        loop {
            match read_frame(&mut viewer.stream).unwrap() {
                (HISTORY, kept) => {
                    assert!(String::from_utf8_lossy(&kept).contains("line1\r\n"));
                    break;
                }
                _ => continue,
            }
        }
        write_frame(&mut viewer.stream, INPUT, b"x\n").unwrap();
        host.join().unwrap();
    }

    #[test]
    fn the_running_session_list_says_a_hosted_session_is_attachable() {
        // SB-346 (Fabric Dashboards): `sessions` and `sessions --all`, not only the history.
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let id = crate::launches::new_id();
        let home = root.join("homes").join("acct");
        std::fs::create_dir_all(&home).unwrap();
        switchboard_core::private_fs::private_write(
            &home.join(crate::launch::SESSION_RECORD),
            serde_json::json!({"launch_id": id, "provider": "claude"})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
        switchboard_core::private_fs::private_dir(&root.join("sessions")).unwrap();
        switchboard_core::private_fs::private_write(
            &descriptor_path(root, &id),
            serde_json::json!({"pid": std::process::id(), "viewers": 2})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
        let rows = crate::launch::sessions(root, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["attachable"], true);
        assert_eq!(rows[0]["viewers"], 2);
    }

    #[test]
    fn an_unknown_launch_id_or_a_bad_one_has_nothing_to_attach() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(
            attach(data.path(), &crate::launches::new_id(), 24, 80).unwrap_err(),
            NOT_FOUND
        );
        assert_eq!(attach(data.path(), "../x", 24, 80).unwrap_err(), NOT_FOUND);
        assert!(attachable(data.path(), "../x").is_none());
    }

    #[test]
    fn the_socket_folder_is_this_users_own() {
        let dir = socket_dir().unwrap();
        let meta = std::fs::symlink_metadata(&dir).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.mode() & 0o077, 0);
        // The digest keeps the socket's address short whatever the data folder's path.
        let long = Path::new("/a").join("b".repeat(300));
        assert!(
            socket_path(&long, &crate::launches::new_id())
                .unwrap()
                .as_os_str()
                .len()
                < 100
        );
    }
}
