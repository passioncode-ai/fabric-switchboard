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

/// The output a viewer that attaches late is shown first, as Fabric Dashboards' console keeps.
pub const SCROLLBACK: usize = 1 << 20;
const MAX_FRAME: usize = 1 << 20;
const MAX_VIEWERS: usize = 8;
/// A viewer that cannot take output this long is dropped, so it never holds the session up.
const VIEWER_WRITE: Duration = Duration::from_secs(2);
const MAX_DESCRIPTOR: u64 = 4096;

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
    viewers: Vec<(u64, UnixStream)>,
    next: u64,
    exit: Option<i32>,
}

struct Session {
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
    fn resize(&self, rows: u16, cols: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let master = self.master.lock().unwrap_or_else(|p| p.into_inner());
        unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) };
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
            &mut size,
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
}

/// Runs a session on a PTY of its own and serves its socket until the session's process exits;
/// returns its exit status (128 + the signal for one a signal ended). `ready` runs once the
/// socket listens, so whoever started the host can attach.
pub fn serve(start: Start<'_>, ready: impl FnOnce()) -> Result<i32, String> {
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
        root: start.root.to_owned(),
        launch_id: start.launch_id.to_owned(),
        socket: socket.clone(),
        master: Mutex::new(File::from(master)),
        inner: Mutex::new(Inner {
            scrollback: VecDeque::new(),
            viewers: Vec::new(),
            next: 0,
            exit: None,
        }),
    });
    write_descriptor(start.root, start.launch_id, &socket, 0);

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
    // The output the process wrote just before it ended reaches the viewers before its exit;
    // a child it left holding the terminal does not hold the end up past a second.
    let _ = drained_rx.recv_timeout(Duration::from_secs(1));
    let viewers = {
        let mut inner = session.lock();
        inner.exit = Some(code);
        std::mem::take(&mut inner.viewers)
    };
    let exit = json!({"exit_code": code}).to_string();
    for (_, mut stream) in viewers {
        let _ = write_frame(&mut stream, EXIT, exit.as_bytes());
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_file(descriptor_path(start.root, start.launch_id));
    Ok(code)
}

fn refuse(stream: &mut UnixStream, error: &str) {
    let _ = write_frame(
        stream,
        ERROR,
        json!({"error": error}).to_string().as_bytes(),
    );
}

fn viewer(session: &Session, mut stream: UnixStream) {
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
        let ack = json!({"launch_id": session.launch_id, "viewers": inner.viewers.len() + 1});
        let (a, b) = inner.scrollback.as_slices();
        let replay: Vec<u8> = [a, b].concat();
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
        session.resize(rows.clamp(1, 1000) as u16, cols.clamp(1, 1000) as u16);
    }
    let _ = stream.set_read_timeout(None);
    loop {
        match read_frame(&mut stream) {
            Ok((INPUT, bytes)) => {
                let mut master = session.master.lock().unwrap_or_else(|p| p.into_inner());
                if master.write_all(&bytes).is_err() {
                    break;
                }
            }
            Ok((RESIZE, size)) if size.len() == 4 => {
                let rows = u16::from_be_bytes([size[0], size[1]]);
                let cols = u16::from_be_bytes([size[2], size[3]]);
                if rows > 0 && cols > 0 {
                    session.resize(rows, cols);
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
}

/// Attaches to the session of `launch_id`: connects, says hello with the viewer's size and
/// reads the answer. The scrollback follows as the first `OUTPUT`.
pub fn attach(root: &Path, launch_id: &str, rows: u16, cols: u16) -> Result<Attached, String> {
    if !crate::launches::valid_id(launch_id) {
        return Err(NOT_FOUND.into());
    }
    let mut stream =
        UnixStream::connect(socket_path(root, launch_id)?).map_err(|_| NOT_FOUND.to_string())?;
    let hello = json!({"launch_id": launch_id, "rows": rows, "cols": cols});
    write_frame(&mut stream, HELLO, hello.to_string().as_bytes())
        .map_err(|_| NOT_FOUND.to_string())?;
    match read_frame(&mut stream) {
        Ok((HELLO, ack)) => {
            let ack: Value = serde_json::from_slice(&ack).unwrap_or(Value::Null);
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
        let (tx, rx) = std::sync::mpsc::channel();
        let host = {
            let (root, id, program) = (root.clone(), id.clone(), program.clone());
            std::thread::spawn(move || {
                serve(
                    Start {
                        root: &root,
                        launch_id: &id,
                        program: &program,
                        rows: 24,
                        cols: 80,
                    },
                    move || tx.send(()).unwrap(),
                )
            })
        };
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
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
        assert_eq!(host.join().unwrap().unwrap(), 7);

        // Ended: nothing to attach, no socket and no descriptor left behind.
        assert!(attachable(&root, &id).is_none());
        assert!(attach(&root, &id, 24, 80).is_err());
        assert!(!socket_path(&root, &id).unwrap().exists());
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
