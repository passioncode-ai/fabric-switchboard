//! SB-273: whose process is on the other end of a loopback connection. The proxy is reachable
//! from every account of the machine (a port on 127.0.0.1 is not per user), so a capability that
//! leaked to another account's process must not be enough: the proxy closes a connection whose
//! peer socket belongs to another uid before it reads a byte of it (the studio-spaces port norm,
//! point 2 b, board thread `1fe66e6e`).
//!
//! macOS: `net.inet.tcp.pcblist64` lists every TCP socket with its owner's uid, readable without
//! privileges (`netstat` reads the same list); macOS 27 lists only the caller's own, so a peer it
//! does not name is looked for among this account's processes (`proc_pidfdinfo`, SB-330). Linux: `/proc/net/tcp` and `/proc/net/tcp6`.
//! Windows (SB-274): `GetExtendedTcpTable` names the peer socket's process, whose token user is
//! compared by SID with this process's; a process this one may not open counts as another user's.
//! An owner that cannot be read (the peer closed already, or the table could not be read) is
//! refused too, as the norm asks.
use axum::serve::Listener;
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
};

/// The uid that owns the TCP socket bound to `peer` and connected to `local`, when it can be read.
pub fn owner(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    owner_on(local.port(), peer.port())
}

#[cfg(target_os = "macos")]
fn owner_on(local_port: u16, peer_port: u16) -> Option<u32> {
    // SB-330: macOS 27 lists only the caller's own sockets in `pcblist64` (read on the Mac Studio,
    // 27.2: two records, the listener and the accepted socket), so every client read as nobody's
    // and the proxy closed each one. A peer the list does not name is looked for among this
    // account's own processes, whose sockets `proc_pidfdinfo` reads; another account's processes
    // cannot be read without privileges, so they stay nobody's and are refused, as before.
    owner_in(&pcblist()?, local_port, peer_port).or_else(|| {
        own_processes_hold(local_port, peer_port)
            .then(own_uid)
            .flatten()
    })
}

/// SB-330: offsets inside `struct socket_fdinfo` (`sys/proc_info.h`: `proc_fileinfo`, then
/// `socket_info`), as `PROC_PIDFDSOCKETINFO` answers it: the TCP socket's foreign and local
/// ports, each in network order at the start of an `int` of `in_sockinfo`. Read on macOS 26.6
/// and 27.2 (792-byte answers); a live test reads a socket of another process back.
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod fdinfo {
    pub const FPORT: usize = 264;
    pub const LPORT: usize = 268;
    pub const MIN_LEN: usize = 272;
    pub const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
    pub const PROC_UID_ONLY: u32 = 4;
}

/// SB-330: whether a socket of one of this account's processes has `peer_port` as its local
/// port and `local_port` as its foreign one (the peer's end of a connection to us).
#[cfg(target_os = "macos")]
fn own_processes_hold(local_port: u16, peer_port: u16) -> bool {
    let Some(uid) = own_uid() else {
        return false;
    };
    let mut pids = vec![0 as libc::pid_t; 8192];
    let bytes = std::mem::size_of_val(pids.as_slice()) as libc::c_int;
    // SAFETY: `pids` holds `bytes` writable bytes; the call writes at most that many and answers
    // how many it wrote.
    let n =
        unsafe { libc::proc_listpids(fdinfo::PROC_UID_ONLY, uid, pids.as_mut_ptr().cast(), bytes) };
    if n <= 0 {
        return false;
    }
    pids.truncate(n as usize / std::mem::size_of::<libc::pid_t>());
    let me = std::process::id() as libc::pid_t;
    // The newest processes first: a client usually started just before it connected.
    pids.sort_unstable_by(|a, b| b.cmp(a));
    pids.into_iter()
        .filter(|&pid| pid > 0 && pid != me)
        .any(|pid| process_holds(pid, local_port, peer_port))
}

#[cfg(target_os = "macos")]
fn process_holds(pid: libc::pid_t, local_port: u16, peer_port: u16) -> bool {
    let size = std::mem::size_of::<libc::proc_fdinfo>();
    let mut fds = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0
        };
        4096
    ];
    // SAFETY: `fds` holds that many records; the call writes at most its size in bytes.
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            (fds.len() * size) as libc::c_int,
        )
    };
    if n <= 0 {
        return false;
    }
    fds.truncate(n as usize / size);
    let mut info = [0u8; 1024];
    fds.iter()
        .filter(|f| f.proc_fdtype as libc::c_int == libc::PROX_FDTYPE_SOCKET)
        .any(|f| {
            // SAFETY: `info` holds 1024 writable bytes; the call writes at most that many.
            let got = unsafe {
                libc::proc_pidfdinfo(
                    pid,
                    f.proc_fd,
                    fdinfo::PROC_PIDFDSOCKETINFO,
                    info.as_mut_ptr().cast(),
                    info.len() as libc::c_int,
                )
            };
            got as usize >= fdinfo::MIN_LEN && ports_in(&info, local_port, peer_port)
        })
}

/// SB-330: whether one `socket_fdinfo` answer is the peer's end: its local port is `peer_port`
/// and its foreign one `local_port`. Our accepted socket is the mirror image and never matches.
#[cfg(any(target_os = "macos", test))]
fn ports_in(info: &[u8], local_port: u16, peer_port: u16) -> bool {
    let port = |at: usize| {
        info.get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    port(fdinfo::LPORT) == Some(peer_port) && port(fdinfo::FPORT) == Some(local_port)
}

#[cfg(target_os = "linux")]
fn owner_on(local_port: u16, peer_port: u16) -> Option<u32> {
    ["/proc/net/tcp", "/proc/net/tcp6"].iter().find_map(|path| {
        let table = std::fs::read_to_string(path).ok()?;
        owner_in_proc(&table, local_port, peer_port)
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn owner_on(_local_port: u16, _peer_port: u16) -> Option<u32> {
    None
}

/// Who this process runs as, for the peer to be compared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Me {
    /// Nothing is checked: a system where the peer's owner cannot be read.
    Unchecked,
    /// This uid (macOS, Linux).
    Uid(u32),
    /// This process's token user (Windows).
    TokenUser,
}

/// Who this process runs as on this system.
pub fn me() -> Me {
    #[cfg(windows)]
    {
        Me::TokenUser
    }
    #[cfg(not(windows))]
    {
        own_uid().map_or(Me::Unchecked, Me::Uid)
    }
}

/// Whether the peer of the connection from `peer` to `local` runs as `me`. Blocking: it reads the
/// system's socket table.
pub fn same_user(me: Me, local: SocketAddr, peer: SocketAddr) -> bool {
    match me {
        Me::Unchecked => true,
        Me::Uid(uid) => admits(uid, owner(local, peer)),
        Me::TokenUser => token_user_matches(local.port(), peer.port()),
    }
}

#[cfg(windows)]
fn token_user_matches(local_port: u16, peer_port: u16) -> bool {
    windows::same_user(local_port, peer_port)
}
#[cfg(not(windows))]
fn token_user_matches(_local_port: u16, _peer_port: u16) -> bool {
    false
}

/// This process's uid where the peer's owner can be read (macOS, Linux); `None` elsewhere.
pub fn own_uid() -> Option<u32> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // SAFETY: getuid has no preconditions and cannot fail.
        Some(unsafe { libc::getuid() })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Offsets inside one `struct xtcpcb64` record of `net.inet.tcp.pcblist64`
/// (`netinet/tcp_var.h`, `netinet/in_pcb.h`, `sys/socketvar.h`; packed to 4 bytes): the record's
/// length, then the embedded `xinpcb64` from byte 4, whose foreign and local ports (network order)
/// are at 16 and 18 and whose socket's `so_uid` is at 248. A test reads a live connection back,
/// so a kernel that changed the layout fails the gate rather than refusing nobody.
#[cfg(any(target_os = "macos", test))]
mod layout {
    pub const HEADER: usize = 24;
    pub const FPORT: usize = 4 + 16;
    pub const LPORT: usize = 4 + 18;
    pub const UID: usize = 4 + 248;
}

#[cfg(target_os = "macos")]
fn pcblist() -> Option<Vec<u8>> {
    const NAME: &std::ffi::CStr = c"net.inet.tcp.pcblist64";
    // The list grows between the size query and the read; a few tries with room to spare.
    for _ in 0..3 {
        let mut len: libc::size_t = 0;
        // SAFETY: a size query: no output buffer, `len` receives the size.
        let rc = unsafe {
            libc::sysctlbyname(
                NAME.as_ptr(),
                std::ptr::null_mut(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 || len == 0 {
            return None;
        }
        len += len / 8 + 4096;
        let mut buf = vec![0u8; len];
        // SAFETY: `buf` holds `len` writable bytes; the kernel writes at most `len` and sets it.
        let rc = unsafe {
            libc::sysctlbyname(
                NAME.as_ptr(),
                buf.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 {
            buf.truncate(len);
            return Some(buf);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOMEM) {
            return None;
        }
    }
    None
}

/// Walks a `pcblist64` answer: a `xinpgen` header, `xtcpcb64` records each led by its length,
/// and a closing `xinpgen`. The peer's socket has the peer's port as its local port and ours as
/// its foreign one; our accepted socket is the mirror image and never matches.
#[cfg(any(target_os = "macos", test))]
fn owner_in(list: &[u8], local_port: u16, peer_port: u16) -> Option<u32> {
    let u32_at = |at: usize| -> Option<u32> {
        list.get(at..at + 4)
            .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    };
    let port_at = |at: usize| -> Option<u16> {
        list.get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let mut at = usize::try_from(u32_at(0)?).ok()?;
    while at < list.len() {
        let len = usize::try_from(u32_at(at)?).ok()?;
        if len <= layout::HEADER || at + len > list.len() {
            break;
        }
        if port_at(at + layout::LPORT)? == peer_port && port_at(at + layout::FPORT)? == local_port {
            return u32_at(at + layout::UID);
        }
        at += len;
    }
    None
}

/// One `/proc/net/tcp` table: `sl local rem st tx:rx tr:tm retrnsmt uid timeout inode ...`,
/// addresses as `HEXADDR:HEXPORT`. The client's own row names its uid; a row whose inode is 0
/// belongs to a client that already closed (its uid then reads 0), so its owner is unknown and
/// is never taken from anything else (contract CT-254, `v0.1.0+2026-11-07.3`, server side).
#[cfg(any(target_os = "linux", test))]
fn owner_in_proc(table: &str, local_port: u16, peer_port: u16) -> Option<u32> {
    let port =
        |field: &str| -> Option<u16> { u16::from_str_radix(field.rsplit_once(':')?.1, 16).ok() };
    table
        .lines()
        .skip(1)
        .find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return None;
            }
            if port(fields[1])? != peer_port || port(fields[2])? != local_port {
                return None;
            }
            // Found the client's row: an answer either way, never a later row's.
            Some((fields[9] != "0").then(|| fields[7].parse().ok()).flatten())
        })
        .flatten()
}

/// Only a peer read as this account is served; another account's, or one whose owner could not be
/// read, is not (the port norm: an owner that cannot be established is a refusal).
fn admits(uid: u32, found: Option<u32>) -> bool {
    found == Some(uid)
}

/// SB-274: the IPv4 TCP table of `GetExtendedTcpTable` (`MIB_TCPTABLE_OWNER_PID`): a row count,
/// then rows of six `u32`s (state, local address, local port, remote address, remote port, owning
/// pid), each port in network order in its low 16 bits. Answers the pid of the peer's socket, whose
/// local port is the peer's and whose remote port is ours.
#[cfg(any(windows, test))]
fn pid_in(table: &[u8], local_port: u16, peer_port: u16) -> Option<u32> {
    let u32_at = |at: usize| -> Option<u32> {
        table
            .get(at..at + 4)
            .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    };
    let port = |value: u32| u16::from_be(value as u16);
    let rows = usize::try_from(u32_at(0)?).ok()?;
    (0..rows.min(table.len().saturating_sub(4) / 24)).find_map(|row| {
        let at = 4 + row * 24;
        (port(u32_at(at + 8)?) == peer_port && port(u32_at(at + 16)?) == local_port)
            .then(|| u32_at(at + 20))
            .flatten()
    })
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, NO_ERROR},
        NetworkManagement::IpHelper::{GetExtendedTcpTable, TCP_TABLE_OWNER_PID_CONNECTIONS},
        Security::{EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    const AF_INET: u32 = 2;

    /// The connected IPv4 TCP sockets with their owning pids, as `GetExtendedTcpTable` writes them.
    fn table() -> Option<Vec<u8>> {
        let mut size: u32 = 0;
        for _ in 0..4 {
            // u64 words keep the table aligned for the kernel's writes.
            let mut words = vec![0u64; (size as usize).div_ceil(8).max(1)];
            let mut len = u32::try_from(words.len() * 8).ok()?;
            // SAFETY: `words` holds `len` writable bytes; the call writes at most `len` and, when
            // that is too few, sets `len` to the size it needs.
            let rc = unsafe {
                GetExtendedTcpTable(
                    words.as_mut_ptr().cast(),
                    &mut len,
                    0,
                    AF_INET,
                    TCP_TABLE_OWNER_PID_CONNECTIONS,
                    0,
                )
            };
            if rc == NO_ERROR {
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).collect();
                return Some(bytes[..(len as usize).min(bytes.len())].to_vec());
            }
            if rc != ERROR_INSUFFICIENT_BUFFER {
                return None;
            }
            // The table grows between calls; leave room.
            size = len + len / 8 + 1024;
        }
        None
    }

    /// An open handle, closed when dropped.
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the handle was opened by this module and is closed once.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    /// The `TOKEN_USER` of `process` (its SID points inside the returned buffer).
    fn token_user(process: HANDLE) -> Option<Vec<u64>> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: `token` receives a handle this function owns from here on.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = Handle(token);
        let mut len: u32 = 0;
        // SAFETY: a size query: no buffer, `len` receives the size.
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut len) };
        if len == 0 {
            return None;
        }
        let mut words = vec![0u64; (len as usize).div_ceil(8)];
        let capacity = u32::try_from(words.len() * 8).ok()?;
        // SAFETY: `words` holds `capacity` writable bytes, aligned for `TOKEN_USER`.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                words.as_mut_ptr().cast(),
                capacity,
                &mut len,
            )
        };
        (ok != 0).then_some(words)
    }

    /// Whether the process that owns the peer's socket runs as this process's user.
    pub(super) fn same_user(local_port: u16, peer_port: u16) -> bool {
        let Some(pid) = table().and_then(|t| super::pid_in(&t, local_port, peer_port)) else {
            return false;
        };
        if pid == std::process::id() {
            return true;
        }
        // SAFETY: the pseudo-handle of this process needs no closing.
        let Some(mine) = token_user(unsafe { GetCurrentProcess() }) else {
            return false;
        };
        // SAFETY: a handle this function owns from here on; null when the process may not be
        // opened, which counts as another user's.
        let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
        if process.0.is_null() {
            return false;
        }
        let Some(theirs) = token_user(process.0) else {
            return false;
        };
        // SAFETY: both buffers start with a `TOKEN_USER` the kernel wrote, whose SID points inside
        // the same buffer, alive for this call.
        unsafe {
            let a = &*(mine.as_ptr() as *const TOKEN_USER);
            let b = &*(theirs.as_ptr() as *const TOKEN_USER);
            EqualSid(a.User.Sid, b.User.Sid) != 0
        }
    }
}

/// A listener that hands axum only connections whose peer is this process's own account, counting
/// the ones it closed (another account's, or one whose owner could not be read).
pub struct OwnUser {
    pub inner: tokio::net::TcpListener,
    pub me: Me,
    pub refused: Arc<AtomicU64>,
}

impl Listener for OwnUser {
    type Io = tokio::net::TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (io, peer) = Listener::accept(&mut self.inner).await;
            let me = self.me;
            if me == Me::Unchecked {
                return (io, peer);
            }
            let Ok(local) = io.local_addr() else {
                return (io, peer);
            };
            let same = tokio::task::spawn_blocking(move || same_user(me, local, peer))
                .await
                .unwrap_or(false);
            if same {
                return (io, peer);
            }
            self.refused
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            drop(io);
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(lport: u16, fport: u16, uid: u32) -> Vec<u8> {
        let mut r = vec![0u8; 472];
        r[0..4].copy_from_slice(&472u32.to_ne_bytes());
        r[layout::FPORT..layout::FPORT + 2].copy_from_slice(&fport.to_be_bytes());
        r[layout::LPORT..layout::LPORT + 2].copy_from_slice(&lport.to_be_bytes());
        r[layout::UID..layout::UID + 4].copy_from_slice(&uid.to_ne_bytes());
        r
    }

    fn list(records: &[Vec<u8>]) -> Vec<u8> {
        let mut head = vec![0u8; layout::HEADER];
        head[0..4].copy_from_slice(&(layout::HEADER as u32).to_ne_bytes());
        let mut out = head.clone();
        for r in records {
            out.extend_from_slice(r);
        }
        out.extend_from_slice(&head);
        out
    }

    #[test]
    fn the_peers_socket_is_found_by_its_ports_and_ours_never_matches() {
        let l = list(&[
            record(4000, 5555, 0),
            record(5555, 4000, 777),
            record(4001, 5555, 0),
        ]);
        assert_eq!(owner_in(&l, 4000, 5555), Some(777));
        assert_eq!(owner_in(&l, 4000, 5556), None);
        assert_eq!(owner_in(&list(&[]), 4000, 5555), None);
        // A truncated record ends the walk instead of reading past the buffer.
        let mut cut = list(&[record(5555, 4000, 777)]);
        cut.truncate(100);
        assert_eq!(owner_in(&cut, 4000, 5555), None);
        assert_eq!(owner_in(&[], 4000, 5555), None);
    }

    #[test]
    fn only_a_peer_read_as_this_account_is_served() {
        assert!(admits(501, Some(501)));
        assert!(!admits(501, Some(502)));
        assert!(
            !admits(501, None),
            "an owner that cannot be read is refused"
        );
    }

    #[test]
    fn a_windows_tcp_row_names_the_peers_pid() {
        let row = |lport: u16, rport: u16, pid: u32| {
            let port = |p: u16| u32::from(p.to_be());
            [5, 0x0100_007f, port(lport), 0x0100_007f, port(rport), pid]
        };
        let rows = [
            row(4000, 5555, 1),
            row(5555, 4000, 4242),
            row(4001, 5555, 3),
        ];
        let mut table = (rows.len() as u32).to_ne_bytes().to_vec();
        for r in rows {
            for v in r {
                table.extend_from_slice(&v.to_ne_bytes());
            }
        }
        assert_eq!(pid_in(&table, 4000, 5555), Some(4242));
        assert_eq!(pid_in(&table, 4000, 1), None);
        // A count larger than the table never reads past it.
        let mut short = table.clone();
        short[0..4].copy_from_slice(&99u32.to_ne_bytes());
        short.truncate(4 + 24);
        assert_eq!(pid_in(&short, 4000, 5555), None);
        assert_eq!(pid_in(&[], 4000, 5555), None);
    }

    #[test]
    fn a_proc_net_tcp_line_names_its_uid() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0FA0 0100007F:15B3 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: 0100007F:15B3 0100007F:0FA0 01 00000000:00000000 00:00000000 00000000  1001        0 2 1\n";
        assert_eq!(owner_in_proc(table, 4000, 5555), Some(1001));
        assert_eq!(owner_in_proc(table, 5555, 4000), Some(1000));
        assert_eq!(owner_in_proc(table, 4000, 1), None);
        assert_eq!(owner_in_proc("header only\n", 4000, 5555), None);
    }

    /// CT-254 (server side): a client row with inode 0 is a client that already closed; its
    /// uid reads 0, and its owner is unknown, never root and never another row's.
    #[test]
    fn a_closed_clients_row_names_no_owner() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0FA0 0100007F:15B3 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: 0100007F:15B3 0100007F:0FA0 06 00000000:00000000 00:00000000 00000000     0        0 0 1\n   2: 0100007F:15B3 0100007F:0FA0 01 00000000:00000000 00:00000000 00000000  1001        0 3 1\n";
        assert_eq!(owner_in_proc(table, 4000, 5555), None);
        assert!(!admits(0, owner_in_proc(table, 4000, 5555)));
    }

    /// On every system the listener checks, a connection this process opens is its own user's.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[tokio::test]
    async fn a_connection_from_this_user_is_this_users() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let _client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (server, peer) = listener.accept().await.unwrap();
        let local = server.local_addr().unwrap();
        assert_ne!(me(), Me::Unchecked);
        let same = tokio::task::spawn_blocking(move || {
            (
                same_user(me(), local, peer),
                same_user(me(), local, SocketAddr::from(([127, 0, 0, 1], 1))),
            )
        })
        .await
        .unwrap();
        assert_eq!(
            same,
            (true, false),
            "a socket that is not there is nobody's"
        );
    }

    /// The live check of the layout: a connection this process opens is owned by its own uid.
    #[test]
    fn a_socket_fdinfo_answer_names_the_peers_end_and_never_ours() {
        let mut info = vec![0u8; 792];
        info[fdinfo::FPORT..fdinfo::FPORT + 2].copy_from_slice(&4000u16.to_be_bytes());
        info[fdinfo::LPORT..fdinfo::LPORT + 2].copy_from_slice(&5555u16.to_be_bytes());
        assert!(ports_in(&info, 4000, 5555));
        assert!(
            !ports_in(&info, 5555, 4000),
            "our accepted socket is the mirror image"
        );
        assert!(!ports_in(&info[..100], 4000, 5555));
    }

    /// SB-330, the live check: a connection another process of this account opens reads back as
    /// this account, also where `pcblist64` lists only the caller's sockets (macOS 27).
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_connection_from_another_process_of_this_account_reads_back_as_this_account() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut child = std::process::Command::new("/usr/bin/nc")
            .args(["127.0.0.1", &address.port().to_string()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let (server, peer) = listener.accept().await.unwrap();
        let local = server.local_addr().unwrap();
        let (found, held) = tokio::task::spawn_blocking(move || {
            (
                owner(local, peer),
                own_processes_hold(local.port(), peer.port()),
            )
        })
        .await
        .unwrap();
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found, own_uid());
        assert!(
            held,
            "the other process's socket is found among this account's processes"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn a_connection_from_this_account_reads_back_as_this_account() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (server, peer) = listener.accept().await.unwrap();
        assert_eq!(peer, client.local_addr().unwrap());
        let local = server.local_addr().unwrap();
        let found = tokio::task::spawn_blocking(move || owner(local, peer))
            .await
            .unwrap();
        assert_eq!(found, own_uid());
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn the_listener_serves_this_account_and_closes_a_peer_of_another() {
        let inner = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = inner.local_addr().unwrap();
        let refused = Arc::new(AtomicU64::new(0));
        // Pretending this process is another account makes its own connection a foreign one.
        let mut other = OwnUser {
            inner,
            me: Me::Uid(own_uid().unwrap().wrapping_add(1)),
            refused: refused.clone(),
        };
        let _first = tokio::net::TcpStream::connect(address).await.unwrap();
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(500), other.accept()).await;
        assert!(
            accepted.is_err(),
            "a foreign peer must never reach the router"
        );
        assert_eq!(refused.load(std::sync::atomic::Ordering::Relaxed), 1);

        let inner = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = inner.local_addr().unwrap();
        let mut own = OwnUser {
            inner,
            me: me(),
            refused: refused.clone(),
        };
        let client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (_, peer) = tokio::time::timeout(std::time::Duration::from_secs(5), own.accept())
            .await
            .unwrap();
        assert_eq!(peer, client.local_addr().unwrap());
        assert_eq!(refused.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
