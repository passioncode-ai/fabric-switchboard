//! SB-342 H-1: `switchboard attach`, `launch --hosted` and the hidden `session-host` command.
//! The host and its protocol are `switchboard_runtime::session_host`.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use switchboard_runtime::session_host::{self as host, Start};

/// The key that detaches a viewer without ending the session: Ctrl-], as telnet's escape.
pub const DETACH_KEY: u8 = 0x1d;

fn size() -> (u16, u16) {
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let ok = unsafe {
        libc::ioctl(
            std::io::stdout().as_raw_fd(),
            libc::TIOCGWINSZ as _,
            &mut size,
        )
    } == 0;
    if ok && size.ws_row > 0 && size.ws_col > 0 {
        (size.ws_row, size.ws_col)
    } else {
        (24, 80)
    }
}

/// Puts the terminal in raw mode for as long as it lives, and back as it was when dropped.
struct Raw(Option<libc::termios>);
impl Raw {
    fn enter() -> Self {
        let fd = std::io::stdin().as_raw_fd();
        let mut saved = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Raw(None);
        }
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) };
        Raw(Some(saved))
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        if let Some(saved) = self.0 {
            unsafe { libc::tcsetattr(std::io::stdin().as_raw_fd(), libc::TCSANOW, &saved) };
        }
    }
}

/// How an attach ended.
pub enum Ended {
    /// The session's process exited with this status.
    Exited(i32),
    /// The viewer detached; the session goes on.
    Detached,
}

/// Shows the session of `launch_id` in this terminal until it exits or the person detaches.
pub fn attach(root: &Path, launch_id: &str) -> Result<Ended, String> {
    use std::io::IsTerminal;
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Attaching needs a terminal on stdin and stdout.".into());
    }
    let (rows, cols) = size();
    let attached = host::attach(root, launch_id, rows, cols)?;
    let mut reader = attached.stream;
    let writer = reader
        .try_clone()
        .map_err(|_| host::NOT_FOUND.to_string())?;
    let writer = Arc::new(std::sync::Mutex::new(writer));
    let raw = Raw::enter();
    let done = Arc::new(AtomicBool::new(false));
    let detached = Arc::new(AtomicBool::new(false));
    {
        // Keys go to the session; Ctrl-] detaches.
        let (writer, done, detached) = (writer.clone(), done.clone(), detached.clone());
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stdin.read(&mut buf) {
                if n == 0 || done.load(Ordering::Relaxed) {
                    break;
                }
                let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(at) = buf[..n].iter().position(|b| *b == DETACH_KEY) {
                    let _ = host::write_frame(&mut *w, host::INPUT, &buf[..at]);
                    detached.store(true, Ordering::Relaxed);
                    let _ = host::write_frame(&mut *w, host::DETACH, b"");
                    let _ = w.shutdown(std::net::Shutdown::Both);
                    break;
                }
                if host::write_frame(&mut *w, host::INPUT, &buf[..n]).is_err() {
                    break;
                }
            }
        });
    }
    {
        // A resized window resizes the session's terminal.
        let (writer, done) = (writer.clone(), done.clone());
        std::thread::spawn(move || {
            let mut last = (rows, cols);
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                let now = size();
                if now != last {
                    last = now;
                    let mut frame = Vec::with_capacity(4);
                    frame.extend(now.0.to_be_bytes());
                    frame.extend(now.1.to_be_bytes());
                    let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
                    if host::write_frame(&mut *w, host::RESIZE, &frame).is_err() {
                        break;
                    }
                }
            }
        });
    }
    let mut stdout = std::io::stdout();
    let ended = loop {
        match host::read_frame(&mut reader) {
            Ok((host::OUTPUT, bytes)) => {
                let _ = stdout.write_all(&bytes);
                let _ = stdout.flush();
            }
            Ok((host::EXIT, body)) => {
                let code = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|v| v["exit_code"].as_i64())
                    .unwrap_or(1);
                break Ended::Exited(code as i32);
            }
            Ok(_) => {}
            Err(_) if detached.load(Ordering::Relaxed) => break Ended::Detached,
            // The host went away without an exit: its session is gone.
            Err(_) => break Ended::Exited(1),
        }
    };
    done.store(true, Ordering::Relaxed);
    drop(raw);
    if let Ended::Detached = ended {
        eprintln!("\r\nDetached. The session goes on: switchboard attach {launch_id}");
    }
    Ok(ended)
}

/// Starts the host of `launch_id` as its own process (its own session, nothing on its stdio),
/// so it outlives this terminal, and waits until it can be attached.
pub fn spawn(root: &Path, launch_id: &str, program: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let (rows, cols) = size();
    let exe = std::env::current_exe().map_err(|_| "Switchboard could not find itself.")?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("--data-dir")
        .arg(root)
        .args(["session-host", "--launch-id", launch_id, "--program"])
        .arg(program)
        .args(["--rows", &rows.to_string(), "--cols", &cols.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "The session host could not start.")?;
    for _ in 0..100 {
        if host::attachable(root, launch_id).is_some() {
            return Ok(());
        }
        if let Ok(Some(_)) = child.try_wait() {
            return Err("The session host stopped before the session started.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("The session host did not answer within ten seconds.".into())
}

/// The hidden `session-host` command: runs the session until it exits, then records its end.
pub fn run(
    root: &Path,
    launch_id: &str,
    program: &Path,
    rows: u16,
    cols: u16,
    idle_minutes: u64,
) -> Result<host::Ended, String> {
    host::serve(
        Start {
            root,
            launch_id,
            program,
            rows,
            cols,
            idle: (idle_minutes > 0).then(|| Duration::from_secs(idle_minutes * 60)),
        },
        || {},
    )
}

/// What `launch --hosted` and `attach` answer once the terminal is the person's again.
pub fn answer(launch_id: &str, ended: &Ended) -> Value {
    match ended {
        Ended::Exited(code) => json!({"launch_id": launch_id, "ended": true, "exit_code": code}),
        Ended::Detached => json!({"launch_id": launch_id, "ended": false, "detached": true}),
    }
}
