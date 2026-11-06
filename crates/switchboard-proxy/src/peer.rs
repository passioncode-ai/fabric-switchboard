//! SB-273: whose process is on the other end of a loopback connection. The proxy is reachable
//! from every account of the machine (a port on 127.0.0.1 is not per user), so a capability that
//! leaked to another account's process must not be enough: the proxy closes a connection whose
//! peer socket belongs to another uid before it reads a byte of it (the studio-spaces port norm,
//! point 2 b, board thread `1fe66e6e`).
//!
//! macOS: `net.inet.tcp.pcblist64` lists every TCP socket with its owner's uid, readable without
//! privileges (`netstat` reads the same list). Linux: `/proc/net/tcp` and `/proc/net/tcp6`.
//! On those two an owner that cannot be read (the peer closed already, or the list could not be
//! read) is refused too, as the norm asks. Elsewhere (Windows, SB-274) no owner is read and the
//! capability alone guards the connection.
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
    owner_in(&pcblist()?, local_port, peer_port)
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

/// This process's uid where the peer's owner can be read (macOS, Linux); `None` elsewhere, where
/// the listener checks nothing.
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

/// One `/proc/net/tcp` table: `sl local rem st tx:rx tr:tm retrnsmt uid ...`, addresses as
/// `HEXADDR:HEXPORT`.
#[cfg(any(target_os = "linux", test))]
fn owner_in_proc(table: &str, local_port: u16, peer_port: u16) -> Option<u32> {
    let port =
        |field: &str| -> Option<u16> { u16::from_str_radix(field.rsplit_once(':')?.1, 16).ok() };
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 8 {
            return None;
        }
        (port(fields[1])? == peer_port && port(fields[2])? == local_port)
            .then(|| fields[7].parse().ok())
            .flatten()
    })
}

/// Only a peer read as this account is served; another account's, or one whose owner could not be
/// read, is not (the port norm: an owner that cannot be established is a refusal).
fn admits(uid: u32, found: Option<u32>) -> bool {
    found == Some(uid)
}

/// A listener that hands axum only connections whose peer is this process's own account, counting
/// the ones it closed (another account's, or one whose owner could not be read).
pub struct OwnUser {
    pub inner: tokio::net::TcpListener,
    pub uid: Option<u32>,
    pub refused: Arc<AtomicU64>,
}

impl Listener for OwnUser {
    type Io = tokio::net::TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (io, peer) = Listener::accept(&mut self.inner).await;
            let Some(uid) = self.uid else {
                return (io, peer);
            };
            let Ok(local) = io.local_addr() else {
                return (io, peer);
            };
            let found = tokio::task::spawn_blocking(move || owner(local, peer))
                .await
                .ok()
                .flatten();
            if admits(uid, found) {
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
    fn a_proc_net_tcp_line_names_its_uid() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0FA0 0100007F:15B3 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: 0100007F:15B3 0100007F:0FA0 01 00000000:00000000 00:00000000 00000000  1001        0 2 1\n";
        assert_eq!(owner_in_proc(table, 4000, 5555), Some(1001));
        assert_eq!(owner_in_proc(table, 5555, 4000), Some(1000));
        assert_eq!(owner_in_proc(table, 4000, 1), None);
        assert_eq!(owner_in_proc("header only\n", 4000, 5555), None);
    }

    /// The live check of the layout: a connection this process opens is owned by its own uid.
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
            uid: own_uid().map(|u| u.wrapping_add(1)),
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
            uid: own_uid(),
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
