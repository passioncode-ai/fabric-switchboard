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

/// H-1c: a session nobody watches and that neither wrote nor read for this long is closed
/// (`reason: idle`), as Fabric Dashboards' consoles do (LC-18).
pub const DEFAULT_IDLE: Duration = Duration::from_secs(30 * 60);
/// H-1c: how long an ended session's output stays readable on disk.
const OUTPUT_KEPT: Duration = Duration::from_secs(24 * 3600);

/// The output a viewer that attaches late is shown first, as Fabric Dashboards' console keeps.
pub const SCROLLBACK: usize = 1 << 20;
const MAX_FRAME: usize = 1 << 20;
const MAX_VIEWERS: usize = 8;
/// A viewer that cannot take output this long is dropped, so it never holds the session up.
const VIEWER_WRITE: Duration = Duration::from_secs(2);
const MAX_DESCRIPTOR: u64 = 4096;

/// The frame protocol's version, in both `HELLO`s: a viewer refuses a host that speaks another.
pub const PROTOCOL: u64 = 1;

pub const NOT_FOUND: &str = "No session with this launch id is open here.";
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
}

fn nonzero(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n).unwrap_or(std::num::NonZeroU16::MIN)
}

struct Session {
    /// The session's process group: its process leads it (setsid).
    pid: i32,
    root: PathBuf,
    launch_id: String,
    socket: PathBuf,
    master: Mutex<File>,
    inner: Mutex<Inner>,
}

impl Session {
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
        {
            let mut inner = self.lock();
            if inner.exit.is_some() {
                return;
            }
            inner.reason.get_or_insert(reason);
        }
        unsafe { libc::kill(-self.pid, libc::SIGHUP) };
        let session = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            if session.lock().exit.is_none() {
                unsafe { libc::kill(-session.pid, libc::SIGTERM) };
            }
        });
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
    let slave_out = slave
        .try_clone()
        .map_err(|_| "No terminal could be opened.")?;
    let slave_err = slave
        .try_clone()
        .map_err(|_| "No terminal could be opened.")?;
    let mut command = Command::new(start.program);
    command
        .stdin(Stdio::from(slave))
        .stdout(Stdio::from(slave_out))
        .stderr(Stdio::from(slave_err));
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
    let mut child = command.spawn().map_err(|_| {
        let _ = std::fs::remove_file(&socket);
        "The session could not start.".to_string()
    })?;
    drop(command);

    let reader = File::from(
        master
            .try_clone()
            .map_err(|_| "No terminal could be opened.")?,
    );
    let session = Arc::new(Session {
        pid: child.id() as i32,
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
        }),
    });
    write_descriptor(start.root, start.launch_id, &socket, 0);
    prune_outputs(start.root);
    if let Some(idle) = start.idle {
        let session = session.clone();
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
                    session.end("idle");
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
                    Ok(0) => break,
                    Ok(n) => session.output(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    // EIO once every holder of the terminal's other end is gone.
                    Err(_) => break,
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

    let status = child
        .wait()
        .map_err(|_| "The session could not be watched.")?;
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
    match read_frame(&mut stream) {
        Ok((HELLO, ack)) => {
            let ack: Value = serde_json::from_slice(&ack).unwrap_or(Value::Null);
            if ack["protocol"].as_u64() != Some(PROTOCOL) {
                return Err(NOT_FOUND.into());
            }
            Ok(Attached {
                stream,
                viewers: ack["viewers"].as_u64().unwrap_or(1),
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

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("launch.command");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn until(stream: &mut UnixStream, needle: &str) -> (String, Option<i32>) {
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

        let mut first = attach(&root, &id, 24, 80).unwrap();
        assert_eq!(first.viewers, 1);
        until(&mut first.stream, "ready");
        write_frame(&mut first.stream, INPUT, b"one\n").unwrap();
        until(&mut first.stream, "got one");
        write_frame(&mut first.stream, DETACH, b"").unwrap();
        drop(first);

        // A viewer that detached left the session running; a new one gets what came before.
        let mut second = attach(&root, &id, 30, 100).unwrap();
        let (replay, _) = until(&mut second.stream, "got one");
        assert!(replay.contains("ready"));
        let mut third = attach(&root, &id, 30, 100).unwrap();
        assert_eq!(third.viewers, 2);
        write_frame(&mut third.stream, INPUT, b"two\n").unwrap();
        assert_eq!(until(&mut second.stream, "").1, Some(7));
        assert_eq!(until(&mut third.stream, "").1, Some(7));
        assert_eq!(
            host.join().unwrap().unwrap(),
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

    fn start_host(
        root: &Path,
        id: &str,
        program: &Path,
        idle: Option<Duration>,
    ) -> std::thread::JoinHandle<Result<Ended, String>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let (root, id, program) = (root.to_owned(), id.to_owned(), program.to_owned());
        let host = std::thread::spawn(move || {
            serve(
                Start {
                    root: &root,
                    launch_id: &id,
                    program: &program,
                    rows: 24,
                    cols: 80,
                    idle,
                },
                move || tx.send(()).unwrap(),
            )
        });
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        host
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
        let mut first = attach(&root, &id, 24, 80).unwrap();
        until(&mut first.stream, "FULLSCREEN");
        let mut second = attach(&root, &id, 24, 80).unwrap();
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
        assert_eq!(host.join().unwrap().unwrap().reason, "exited");
    }

    #[test]
    fn a_session_nobody_watches_and_nothing_moves_in_is_closed_as_idle() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let program = script(&root, "sleep 60");
        let id = crate::launches::new_id();
        let started = std::time::Instant::now();
        let host = start_host(&root, &id, &program, Some(Duration::from_millis(300)));
        let ended = host.join().unwrap().unwrap();
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
        let mut viewer = attach(&root, &id, 24, 80).unwrap();
        until(&mut viewer.stream, "up");
        let ended = end(&root, &id).unwrap();
        assert_eq!(ended["reason"], "ended");
        assert_eq!(
            until(&mut viewer.stream, "").1,
            Some(ended["exit_code"].as_i64().unwrap() as i32)
        );
        assert_eq!(host.join().unwrap().unwrap().reason, "ended");
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
        let mut viewer = attach(&root, &id, 24, 80).unwrap();
        until(&mut viewer.stream, "line60");
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
        host.join().unwrap().unwrap();
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
