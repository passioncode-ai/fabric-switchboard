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
//!
//! SB-421 (DEC-0029): the one exception is an account the person named (`users::Users`): its
//! connection is let in and marked with its uid (`Conn::user`), so the relay serves it only with
//! that account's own capability, for its one group.
use axum::serve::Listener;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{atomic::AtomicU64, Arc},
};

/// The uid that owns the TCP socket bound to `peer` and connected to `local`, when it can be read.
/// SB-421 review (P2-1): a socket counts only when both its addresses and both its ports are the
/// connection's: another account may bind `127.0.0.2:<proxy port>` or reuse a source port on
/// another address, and a port alone would name it. Two sockets that both match, and disagree, are
/// nobody's.
pub fn owner(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    owner_on(local, peer)
}

/// An address as a socket table writes it, IPv4 inside IPv6 (`::ffff:a.b.c.d`) read as IPv4, so
/// the two spellings of one connection compare equal.
fn plain(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

/// What a socket table says about one connection's peer.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// Exactly one socket matches, owned by this uid (or several that agree).
    Owner(u32),
    /// Matching sockets disagree: nobody's.
    Ambiguous,
    /// No socket matches.
    Absent,
}
#[cfg(any(target_os = "macos", test))]
impl Seen {
    fn and(self, uid: u32) -> Self {
        match self {
            Seen::Absent => Seen::Owner(uid),
            Seen::Owner(u) if u == uid => self,
            _ => Seen::Ambiguous,
        }
    }
}

#[cfg(target_os = "macos")]
fn owner_on(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    // SB-330: macOS 27 lists only the caller's own sockets in `pcblist64` (read on the Mac Studio,
    // 27.2: two records, the listener and the accepted socket), so every client read as nobody's
    // and the proxy closed each one. A peer the list does not name is looked for among this
    // account's own processes, whose sockets `proc_pidfdinfo` reads; another account's processes
    // cannot be read without privileges, so they stay nobody's and are refused, as before.
    match owner_in(&pcblist()?, local, peer) {
        Seen::Owner(uid) => Some(uid),
        Seen::Ambiguous => None,
        Seen::Absent => own_processes_hold(local, peer).then(own_uid).flatten(),
    }
}

/// SB-330: offsets inside `struct socket_fdinfo` (`sys/proc_info.h`: `proc_fileinfo`, then
/// `socket_info`), as `PROC_PIDFDSOCKETINFO` answers it: the TCP socket's foreign and local
/// ports, each in network order at the start of an `int` of `in_sockinfo`. Read on macOS 26.6
/// and 27.2 (792-byte answers); a live test reads a socket of another process back. The module is
/// built for tests on every OS, Windows included, where `libc` is not a dependency, so its C type
/// comes from `std::ffi` (the same `i32` as `libc::c_int` on macOS; SB-406 item 7).
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod fdinfo {
    pub const FPORT: usize = 264;
    pub const LPORT: usize = 268;
    /// `insi_vflag` (`INI_IPV4` 1, `INI_IPV6` 2) and the two 16-byte address unions, an IPv4
    /// address in their last 4 bytes; read from the SDK with `offsetof` on macOS 27.2.
    pub const VFLAG: usize = 288;
    pub const FADDR: usize = 296;
    pub const LADDR: usize = 312;
    pub const MIN_LEN: usize = 328;
    pub const PROC_PIDFDSOCKETINFO: std::ffi::c_int = 3;
    pub const PROC_UID_ONLY: u32 = 4;
}

/// SB-330: whether a socket of one of this account's processes has `peer` as its local
/// address and port and `local` as its foreign ones (the peer's end of a connection to us).
#[cfg(target_os = "macos")]
fn own_processes_hold(local: SocketAddr, peer: SocketAddr) -> bool {
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
        .any(|pid| process_holds(pid, local, peer))
}

#[cfg(target_os = "macos")]
fn process_holds(pid: libc::pid_t, local: SocketAddr, peer: SocketAddr) -> bool {
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
            got as usize >= fdinfo::MIN_LEN && ports_in(&info, local, peer)
        })
}

/// SB-330: whether one `socket_fdinfo` answer is the peer's end: its local address and port are
/// `peer`'s and its foreign ones `local`'s (SB-421 review P2-1: addresses too). Our accepted socket
/// is the mirror image and never matches.
#[cfg(any(target_os = "macos", test))]
fn ports_in(info: &[u8], local: SocketAddr, peer: SocketAddr) -> bool {
    let port = |at: usize| {
        info.get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let vflag = info.get(fdinfo::VFLAG).copied().unwrap_or(0);
    port(fdinfo::LPORT) == Some(peer.port())
        && port(fdinfo::FPORT) == Some(local.port())
        && union_address(info, fdinfo::LADDR, vflag) == Some(plain(peer.ip()))
        && union_address(info, fdinfo::FADDR, vflag) == Some(plain(local.ip()))
}

/// An address from a 16-byte `in4in6_addr`/`in6_addr` union at `at`: IPv4 (flag 1) in its last 4
/// bytes, else IPv6 (flag 2).
#[cfg(any(target_os = "macos", test))]
fn union_address(bytes: &[u8], at: usize, vflag: u8) -> Option<IpAddr> {
    let raw: [u8; 16] = bytes.get(at..at + 16)?.try_into().ok()?;
    if vflag & 1 != 0 {
        let v4: [u8; 4] = raw[12..16].try_into().ok()?;
        Some(IpAddr::from(v4))
    } else if vflag & 2 != 0 {
        Some(plain(IpAddr::from(raw)))
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn owner_on(local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    // Both tables, read as one: a connection listed in both with two owners is nobody's.
    // Security round 2 (P3): `/proc/self/net` first, which a systemd unit hardened with
    // `ProcSubset=pid` still shows (`/proc/net` is hidden there); the same network namespace.
    let mut tables = String::new();
    for name in ["tcp", "tcp6"] {
        let read = std::fs::read_to_string(format!("/proc/self/net/{name}"))
            .or_else(|_| std::fs::read_to_string(format!("/proc/net/{name}")));
        if let Ok(table) = read {
            tables.push_str(&table);
        }
    }
    owner_in_proc(&tables, local, peer)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn owner_on(_local: SocketAddr, _peer: SocketAddr) -> Option<u32> {
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
        Me::TokenUser => token_user_matches(local, peer),
    }
}

#[cfg(windows)]
fn token_user_matches(local: SocketAddr, peer: SocketAddr) -> bool {
    windows::same_user(local, peer)
}
#[cfg(not(windows))]
fn token_user_matches(_local: SocketAddr, _peer: SocketAddr) -> bool {
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
    /// SB-421 review (P2-1): `inp_vflag` and the foreign and local address unions, read from the
    /// SDK with `offsetof` on macOS 27.2 (96, 100, 116 from the record's start).
    pub const VFLAG: usize = 96;
    pub const FADDR: usize = 100;
    pub const LADDR: usize = 116;
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
/// and a closing `xinpgen`. The peer's socket has the peer's address and port as its local ones
/// and ours as its foreign ones; our accepted socket is the mirror image and never matches. Every
/// matching record is read: two that disagree are nobody's.
#[cfg(any(target_os = "macos", test))]
fn owner_in(list: &[u8], local: SocketAddr, peer: SocketAddr) -> Seen {
    let u32_at = |at: usize| -> Option<u32> {
        list.get(at..at + 4)
            .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    };
    let port_at = |at: usize| -> Option<u16> {
        list.get(at..at + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let mut seen = Seen::Absent;
    let Some(mut at) = u32_at(0).and_then(|n| usize::try_from(n).ok()) else {
        return seen;
    };
    while at < list.len() {
        let Some(len) = u32_at(at).and_then(|n| usize::try_from(n).ok()) else {
            break;
        };
        if len <= layout::HEADER || at + len > list.len() {
            break;
        }
        let record = &list[at..at + len];
        let vflag = record.get(layout::VFLAG).copied().unwrap_or(0);
        let matches = port_at(at + layout::LPORT) == Some(peer.port())
            && port_at(at + layout::FPORT) == Some(local.port())
            && union_address(record, layout::LADDR, vflag) == Some(plain(peer.ip()))
            && union_address(record, layout::FADDR, vflag) == Some(plain(local.ip()));
        if matches {
            seen = match u32_at(at + layout::UID) {
                Some(uid) => seen.and(uid),
                None => Seen::Ambiguous,
            };
        }
        at += len;
    }
    seen
}

/// One or more `/proc/net/tcp` and `/proc/net/tcp6` tables: `sl local rem st tx:rx tr:tm retrnsmt
/// uid timeout inode ...`, addresses as `HEXADDR:HEXPORT` (an IPv4 address as one host-order word,
/// an IPv6 address as four). The peer's row has the peer's address and port as its local ones and
/// ours as its remote ones (SB-421 review P2-1: addresses too, not ports alone). A matching row
/// whose inode is 0 belongs to a client that already closed (its uid then reads 0): its owner is
/// unknown and is never taken from anything else (contract CT-254, `v0.1.0+2026-11-07.3`, server
/// side); two live rows that disagree are nobody's.
#[cfg(any(target_os = "linux", test))]
fn owner_in_proc(table: &str, local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    let endpoint = |field: &str| -> Option<(IpAddr, u16)> {
        let (addr, port) = field.rsplit_once(':')?;
        let port = u16::from_str_radix(port, 16).ok()?;
        let word = |hex: &str| u32::from_str_radix(hex, 16).ok().map(u32::to_ne_bytes);
        let ip = match addr.len() {
            8 => IpAddr::from(word(addr)?),
            32 => {
                let mut bytes = [0u8; 16];
                for i in 0..4 {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&word(&addr[i * 8..i * 8 + 8])?);
                }
                IpAddr::from(bytes)
            }
            _ => return None,
        };
        Some((plain(ip), port))
    };
    let want_local = (plain(peer.ip()), peer.port());
    let want_remote = (plain(local.ip()), local.port());
    let mut found: Option<u32> = None;
    for line in table.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 || !fields[0].ends_with(':') {
            continue; // a header line, or one cut short
        }
        if endpoint(fields[1]) != Some(want_local) || endpoint(fields[2]) != Some(want_remote) {
            continue;
        }
        if fields[9] == "0" {
            return None;
        }
        let uid: u32 = fields[7].parse().ok()?;
        match found {
            Some(other) if other != uid => return None,
            _ => found = Some(uid),
        }
    }
    found
}

/// Only a peer read as this account is served; another account's, or one whose owner could not be
/// read, is not (the port norm: an owner that cannot be established is a refusal).
fn admits(uid: u32, found: Option<u32>) -> bool {
    found == Some(uid)
}

/// SB-274: the IPv4 TCP table of `GetExtendedTcpTable` (`MIB_TCPTABLE_OWNER_PID`): a row count,
/// then rows of six `u32`s (state, local address, local port, remote address, remote port, owning
/// pid), each address in network order and each port in network order in its low 16 bits. Answers
/// the pid of the peer's socket, whose local address and port are the peer's and whose remote ones
/// are ours (SB-421 review P2-1: addresses too); two rows that disagree name nobody.
#[cfg(any(windows, test))]
fn pid_in(table: &[u8], local: SocketAddr, peer: SocketAddr) -> Option<u32> {
    let u32_at = |at: usize| -> Option<u32> {
        table
            .get(at..at + 4)
            .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    };
    let ip_at = |at: usize| -> Option<IpAddr> {
        let b: [u8; 4] = table.get(at..at + 4)?.try_into().ok()?;
        Some(IpAddr::from(b))
    };
    let port = |value: u32| u16::from_be(value as u16);
    let rows = usize::try_from(u32_at(0)?).ok()?;
    let mut found: Option<u32> = None;
    for row in 0..rows.min(table.len().saturating_sub(4) / 24) {
        let at = 4 + row * 24;
        let matches = port(u32_at(at + 8)?) == peer.port()
            && port(u32_at(at + 16)?) == local.port()
            && ip_at(at + 4) == Some(plain(peer.ip()))
            && ip_at(at + 12) == Some(plain(local.ip()));
        if matches {
            let pid = u32_at(at + 20)?;
            match found {
                Some(other) if other != pid => return None,
                _ => found = Some(pid),
            }
        }
    }
    found
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
    pub(super) fn same_user(local: std::net::SocketAddr, peer: std::net::SocketAddr) -> bool {
        let Some(pid) = table().and_then(|t| super::pid_in(&t, local, peer)) else {
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

/// One accepted connection: its peer's address, and the named account it came from (SB-421), or
/// `None` for this process's own account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conn {
    pub addr: SocketAddr,
    pub user: Option<u32>,
}
impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, OwnUser>> for Conn {
    fn connect_info(stream: axum::serve::IncomingStream<'_, OwnUser>) -> Self {
        *stream.remote_addr()
    }
}

/// Who a connection's peer is, as the listener decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// This process's own account.
    Own,
    /// An account the person named (SB-421).
    Named(u32),
    /// Another account, or one that could not be read: closed.
    Refused,
}

/// Decides a connection: this account's, a named account's, or refused. Blocking: it reads the
/// system's socket table and, for another account, the named users' list.
pub fn admit(
    me: Me,
    local: SocketAddr,
    peer: SocketAddr,
    users: &crate::users::Users,
) -> Admission {
    match me {
        Me::Unchecked => Admission::Own,
        Me::TokenUser => {
            // SB-421: never on Windows, where the peer is compared by SID, never by a uid.
            if token_user_matches(local, peer) {
                Admission::Own
            } else {
                Admission::Refused
            }
        }
        Me::Uid(uid) => classify(uid, owner(local, peer), users),
    }
}

/// `admit` for an accepted socket whose own address may not have been readable: SB-421 review
/// (P2-5), such a connection admits nobody, never this account.
fn admit_at(
    me: Me,
    local: std::io::Result<SocketAddr>,
    peer: SocketAddr,
    users: &crate::users::Users,
) -> Admission {
    match local {
        Ok(local) => admit(me, local, peer, users),
        Err(_) => Admission::Refused,
    }
}

/// SB-421: this account's peer is its own; another account's only when it is named now; one
/// whose owner could not be read never.
fn classify(uid: u32, found: Option<u32>, users: &crate::users::Users) -> Admission {
    match found {
        _ if admits(uid, found) => Admission::Own,
        Some(other) if other != 0 => match users.find(other) {
            // SB-421 review (P2-4): a uid listed under a name is that account only while the
            // system still names the uid so; a uid given to a new account is refused.
            Some(user)
                if user
                    .name
                    .as_deref()
                    .is_none_or(|name| account_name(other).as_deref() == Some(name)) =>
            {
                Admission::Named(other)
            }
            _ => Admission::Refused,
        },
        _ => Admission::Refused,
    }
}

/// The name the password database gives `uid` now (macOS, Linux).
#[cfg(unix)]
pub fn account_name(uid: u32) -> Option<String> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 16 * 1024];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: every pointer is to a live buffer of the stated size; `found` is null or `pwd`,
    // whose strings point into `buf`.
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found) };
    if rc != 0 || found.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    // SAFETY: as above.
    let name = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
    Some(name.to_string_lossy().into_owned())
}
#[cfg(not(unix))]
pub fn account_name(_uid: u32) -> Option<String> {
    None
}

/// A listener that hands axum only connections whose peer is this process's own account, or an
/// account the person named (SB-421), counting the ones it closed (another account's, or one
/// whose owner could not be read).
pub struct OwnUser {
    pub inner: tokio::net::TcpListener,
    pub me: Me,
    pub refused: Arc<AtomicU64>,
    /// SB-421: the named accounts whose connections are let in.
    pub users: Arc<crate::users::Users>,
}

impl Listener for OwnUser {
    type Io = tokio::net::TcpStream;
    type Addr = Conn;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (io, peer) = Listener::accept(&mut self.inner).await;
            let own = Conn {
                addr: peer,
                user: None,
            };
            let me = self.me;
            if me == Me::Unchecked {
                return (io, own);
            }
            let local = io.local_addr();
            let users = self.users.clone();
            let admission = tokio::task::spawn_blocking(move || admit_at(me, local, peer, &users))
                .await
                .unwrap_or(Admission::Refused);
            match admission {
                Admission::Own => return (io, own),
                Admission::Named(uid) => {
                    return (
                        io,
                        Conn {
                            addr: peer,
                            user: Some(uid),
                        },
                    )
                }
                Admission::Refused => {
                    self.refused
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    drop(io);
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner
            .local_addr()
            .map(|addr| Conn { addr, user: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 127.0.0.1 and a port.
    fn at(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    fn record(lport: u16, fport: u16, uid: u32) -> Vec<u8> {
        record_at(at(lport), at(fport), uid)
    }
    fn record_at(local: SocketAddr, foreign: SocketAddr, uid: u32) -> Vec<u8> {
        let mut r = vec![0u8; 472];
        r[0..4].copy_from_slice(&472u32.to_ne_bytes());
        r[layout::FPORT..layout::FPORT + 2].copy_from_slice(&foreign.port().to_be_bytes());
        r[layout::LPORT..layout::LPORT + 2].copy_from_slice(&local.port().to_be_bytes());
        r[layout::VFLAG] = 1;
        let v4 = |a: SocketAddr| match a.ip() {
            IpAddr::V4(v4) => v4.octets(),
            IpAddr::V6(_) => unreachable!(),
        };
        r[layout::FADDR + 12..layout::FADDR + 16].copy_from_slice(&v4(foreign));
        r[layout::LADDR + 12..layout::LADDR + 16].copy_from_slice(&v4(local));
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
        assert_eq!(owner_in(&l, at(4000), at(5555)), Seen::Owner(777));
        assert_eq!(owner_in(&l, at(4000), at(5556)), Seen::Absent);
        assert_eq!(owner_in(&list(&[]), at(4000), at(5555)), Seen::Absent);
        // A truncated record ends the walk instead of reading past the buffer.
        let mut cut = list(&[record(5555, 4000, 777)]);
        cut.truncate(100);
        assert_eq!(owner_in(&cut, at(4000), at(5555)), Seen::Absent);
        assert_eq!(owner_in(&[], at(4000), at(5555)), Seen::Absent);
    }

    /// SB-421 review (P2-1): another account bound to 127.0.0.2 with the same ports is not the
    /// peer, and two matching sockets that disagree name nobody.
    #[test]
    fn a_pcb_record_matches_by_both_addresses_and_ports_and_disagreement_is_nobodys() {
        let other = SocketAddr::from(([127, 0, 0, 2], 5555));
        let l = list(&[record_at(other, at(4000), 666), record(5555, 4000, 777)]);
        assert_eq!(owner_in(&l, at(4000), at(5555)), Seen::Owner(777));
        let only_other = list(&[record_at(other, at(4000), 666)]);
        assert_eq!(owner_in(&only_other, at(4000), at(5555)), Seen::Absent);
        let to_other = list(&[record_at(
            at(5555),
            SocketAddr::from(([127, 0, 0, 2], 4000)),
            666,
        )]);
        assert_eq!(owner_in(&to_other, at(4000), at(5555)), Seen::Absent);
        let twice = list(&[record(5555, 4000, 777), record(5555, 4000, 666)]);
        assert_eq!(owner_in(&twice, at(4000), at(5555)), Seen::Ambiguous);
        let agree = list(&[record(5555, 4000, 777), record(5555, 4000, 777)]);
        assert_eq!(owner_in(&agree, at(4000), at(5555)), Seen::Owner(777));
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
        assert_eq!(pid_in(&table, at(4000), at(5555)), Some(4242));
        assert_eq!(pid_in(&table, at(4000), at(1)), None);
        // SB-421 review (P2-1): the peer's address must match too.
        let elsewhere = SocketAddr::from(([127, 0, 0, 2], 5555));
        assert_eq!(pid_in(&table, at(4000), elsewhere), None);
        // A count larger than the table never reads past it.
        let mut short = table.clone();
        short[0..4].copy_from_slice(&99u32.to_ne_bytes());
        short.truncate(4 + 24);
        assert_eq!(pid_in(&short, at(4000), at(5555)), None);
        assert_eq!(pid_in(&[], at(4000), at(5555)), None);
    }

    #[test]
    fn a_proc_net_tcp_line_names_its_uid() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0FA0 0100007F:15B3 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: 0100007F:15B3 0100007F:0FA0 01 00000000:00000000 00:00000000 00000000  1001        0 2 1\n";
        assert_eq!(owner_in_proc(table, at(4000), at(5555)), Some(1001));
        assert_eq!(owner_in_proc(table, at(5555), at(4000)), Some(1000));
        assert_eq!(owner_in_proc(table, at(4000), at(1)), None);
        assert_eq!(owner_in_proc("header only\n", at(4000), at(5555)), None);
    }

    /// SB-421 review (P2-1): on Linux another account may bind 127.0.0.2 with the proxy's port
    /// numbers, or reuse a source port on another address; only the whole address pair counts,
    /// IPv4 in IPv6 reads as IPv4, and two live rows that disagree name nobody.
    #[test]
    fn a_proc_net_tcp_row_matches_by_both_addresses_and_disagreement_is_nobodys() {
        let head = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
        let row = |local: &str, remote: &str, uid: u32, inode: u32| {
            format!("   0: {local} {remote} 01 00000000:00000000 00:00000000 00000000  {uid}        0 {inode} 1\n")
        };
        // 127.0.0.2:5555 -> 127.0.0.1:4000 is someone else's connection to another address pair.
        let other = format!("{head}{}", row("0200007F:15B3", "0100007F:0FA0", 1002, 7));
        assert_eq!(owner_in_proc(&other, at(4000), at(5555)), None);
        let both = format!("{other}{}", row("0100007F:15B3", "0100007F:0FA0", 1001, 8));
        assert_eq!(owner_in_proc(&both, at(4000), at(5555)), Some(1001));
        // 127.0.0.1:5555 -> 127.0.0.2:4000: a connection to another listener on the same port.
        let elsewhere = format!("{head}{}", row("0100007F:15B3", "0200007F:0FA0", 1002, 9));
        assert_eq!(owner_in_proc(&elsewhere, at(4000), at(5555)), None);
        // The same connection in tcp6 as ::ffff:127.0.0.1.
        let mapped = "0000000000000000FFFF00000100007F";
        let six = format!(
            "{head}{}",
            row(
                &format!("{mapped}:15B3"),
                &format!("{mapped}:0FA0"),
                1003,
                10
            )
        );
        assert_eq!(owner_in_proc(&six, at(4000), at(5555)), Some(1003));
        // Two live rows for one address pair that disagree.
        let twice = format!("{both}{}", row("0100007F:15B3", "0100007F:0FA0", 1004, 11));
        assert_eq!(owner_in_proc(&twice, at(4000), at(5555)), None);
    }

    /// CT-254 (server side): a client row with inode 0 is a client that already closed; its
    /// uid reads 0, and its owner is unknown, never root and never another row's.
    #[test]
    fn a_closed_clients_row_names_no_owner() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0FA0 0100007F:15B3 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: 0100007F:15B3 0100007F:0FA0 06 00000000:00000000 00:00000000 00000000     0        0 0 1\n   2: 0100007F:15B3 0100007F:0FA0 01 00000000:00000000 00:00000000 00000000  1001        0 3 1\n";
        assert_eq!(owner_in_proc(table, at(4000), at(5555)), None);
        assert!(!admits(0, owner_in_proc(table, at(4000), at(5555))));
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
        info[fdinfo::VFLAG] = 1;
        info[fdinfo::FADDR + 12..fdinfo::FADDR + 16].copy_from_slice(&[127, 0, 0, 1]);
        info[fdinfo::LADDR + 12..fdinfo::LADDR + 16].copy_from_slice(&[127, 0, 0, 1]);
        assert!(ports_in(&info, at(4000), at(5555)));
        assert!(
            !ports_in(&info, at(5555), at(4000)),
            "our accepted socket is the mirror image"
        );
        assert!(!ports_in(&info[..100], at(4000), at(5555)));
        // SB-421 review (P2-1): the same ports on another address are not the peer's end.
        info[fdinfo::LADDR + 12..fdinfo::LADDR + 16].copy_from_slice(&[127, 0, 0, 2]);
        assert!(!ports_in(&info, at(4000), at(5555)));
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
            (owner(local, peer), own_processes_hold(local, peer))
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
            users: Arc::new(crate::users::Users::none()),
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
            users: Arc::new(crate::users::Users::none()),
        };
        let client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (_, peer) = tokio::time::timeout(std::time::Duration::from_secs(5), own.accept())
            .await
            .unwrap();
        assert_eq!(peer.addr, client.local_addr().unwrap());
        assert_eq!(peer.user, None);
        assert_eq!(refused.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// SB-421: another account's connection is let in only while that account is named, and is
    /// marked with its uid; unnamed, it is closed as before.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn a_named_accounts_connection_is_let_in_and_marked_with_its_uid() {
        let root = tempfile::tempdir().unwrap();
        let own = own_uid().unwrap();
        let refused = Arc::new(AtomicU64::new(0));
        let users = Arc::new(crate::users::Users::new(root.path()));
        let inner = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = inner.local_addr().unwrap();
        // This process plays the service account `own + 1`; its own connections come from `own`.
        let mut listener = OwnUser {
            inner,
            me: Me::Uid(own.wrapping_add(1)),
            refused: refused.clone(),
            users: users.clone(),
        };
        let _unnamed = tokio::net::TcpStream::connect(address).await.unwrap();
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(500), listener.accept()).await;
        assert!(accepted.is_err(), "an account nobody named is closed");
        assert_eq!(refused.load(std::sync::atomic::Ordering::Relaxed), 1);

        crate::users::save(
            root.path(),
            &[crate::users::NamedUser {
                uid: own,
                name: None,
                pool: "team".into(),
                folders: Vec::new(),
                salt: crate::users::new_salt(),
                added_at: 1,
                added_by: "cli".into(),
                settings: None,
                settings_sha256: None,
                group_created_at: None,
            }],
        )
        .unwrap();
        let client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (_, conn) = tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
            .await
            .unwrap();
        assert_eq!(conn.addr, client.local_addr().unwrap());
        assert_eq!(conn.user, Some(own));

        crate::users::save(root.path(), &[]).unwrap();
        let _after = tokio::net::TcpStream::connect(address).await.unwrap();
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(500), listener.accept()).await;
        assert!(accepted.is_err(), "a disallowed account is closed at once");
        assert_eq!(refused.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn only_a_named_account_or_this_one_is_admitted() {
        let root = tempfile::tempdir().unwrap();
        let users = crate::users::Users::new(root.path());
        assert_eq!(classify(501, Some(501), &users), Admission::Own);
        assert_eq!(classify(501, Some(1001), &users), Admission::Refused);
        assert_eq!(classify(501, None, &users), Admission::Refused);
        crate::users::save(
            root.path(),
            &[crate::users::NamedUser {
                uid: 1001,
                name: None,
                pool: "team".into(),
                folders: Vec::new(),
                salt: crate::users::new_salt(),
                added_at: 1,
                added_by: "cli".into(),
                settings: None,
                settings_sha256: None,
                group_created_at: None,
            }],
        )
        .unwrap();
        assert_eq!(classify(501, Some(1001), &users), Admission::Named(1001));
        assert_eq!(classify(501, Some(1002), &users), Admission::Refused);
        assert_eq!(
            classify(501, Some(0), &users),
            Admission::Refused,
            "root never"
        );
        assert_eq!(classify(501, None, &users), Admission::Refused);
    }

    /// SB-421 review (P2-5): an accepted socket whose own address cannot be read admits nobody.
    #[test]
    fn a_socket_whose_address_cannot_be_read_admits_nobody() {
        let users = crate::users::Users::none();
        let err = || Err(std::io::Error::other("gone"));
        assert_eq!(
            admit_at(Me::Uid(501), err(), at(5555), &users),
            Admission::Refused
        );
        assert_eq!(
            admit_at(Me::TokenUser, err(), at(5555), &users),
            Admission::Refused
        );
    }

    /// SB-421 review (P2-4): a uid listed under a name is admitted only while the system still
    /// gives the uid that name.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_listed_uid_is_admitted_only_under_its_recorded_name() {
        let root = tempfile::tempdir().unwrap();
        let users = crate::users::Users::new(root.path());
        let own = own_uid().unwrap();
        let named = |name: Option<String>| crate::users::NamedUser {
            uid: own,
            name,
            pool: "team".into(),
            folders: Vec::new(),
            salt: crate::users::new_salt(),
            added_at: 1,
            added_by: "cli".into(),
            settings: None,
            settings_sha256: None,
            group_created_at: None,
        };
        let me = own.wrapping_add(1);
        crate::users::save(root.path(), &[named(account_name(own))]).unwrap();
        assert_eq!(classify(me, Some(own), &users), Admission::Named(own));
        crate::users::save(root.path(), &[named(Some("gone-account-sb421".into()))]).unwrap();
        assert_eq!(classify(me, Some(own), &users), Admission::Refused);
    }

    /// SB-421 review (P2-1), the live check of the record layout: this process's own connection is
    /// found in `pcblist64` by both addresses and ports, and nowhere else.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn this_processs_own_connection_is_found_in_the_pcb_list_by_its_address_pair() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let _client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (server, peer) = listener.accept().await.unwrap();
        let local = server.local_addr().unwrap();
        let (found, other) = tokio::task::spawn_blocking(move || {
            let list = pcblist().unwrap();
            let elsewhere = SocketAddr::from(([127, 0, 0, 2], peer.port()));
            (
                owner_in(&list, local, peer),
                owner_in(&list, local, elsewhere),
            )
        })
        .await
        .unwrap();
        assert_eq!(found, Seen::Owner(own_uid().unwrap()));
        assert_eq!(other, Seen::Absent);
    }
}
