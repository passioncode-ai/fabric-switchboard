//! SB-421 (DEC-0029): named users of this computer the proxy serves. On a multi-user host (an
//! organization's Linux VM) Switchboard runs as one account and agents run under accounts of
//! their own. The proxy closes every connection from another account (SB-273); the person can
//! name one account, by uid, with the one group it may use. A connection from that account is
//! then let in, and its requests are served only with a capability derived for that account and
//! that group (`user_token`); nothing else it presents is accepted: not the session token, not
//! the agents' key, not a group's or a folder's capability.
//!
//! The list is `<data>/proxy-users.json` (0600): uids, group ids, folders, when, by which
//! surface, the settings file Switchboard wrote for the account, and the salt that makes its
//! capability (a salt is no secret: the capability needs the session token too). The proxy reads
//! it again whenever the file changed (`Users::current`): before it admits a named connection,
//! after each change the owner saves (`ProxyHandle::reload_users`) and every [`REFRESH`] while
//! anyone is listed. A request reads the list as last read (`Users::find_cached`), so allowing,
//! disallowing and rotating through Switchboard take effect at once, a change made by hand within
//! [`REFRESH`], with no owner restart.
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The list's file in the data folder.
pub const FILE: &str = "proxy-users.json";
const MAX_BYTES: u64 = 256 * 1024;
/// At most this many named users.
pub const MAX_USERS: usize = 256;

/// One account of this computer the proxy serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedUser {
    pub uid: u32,
    /// The account's name when it was added, for people reading the list; the uid decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The one group (pool id) whose accounts serve it.
    pub pool: String,
    /// Absolute folders it works in, for statistics.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Part of its capability: a new salt makes the old capability worthless.
    pub salt: String,
    pub added_at: i64,
    /// The surface that added or last changed it: `cli`, `mcp` or `window`.
    pub added_by: String,
    /// The Claude Code settings file Switchboard wrote for it, rewritten on rotation and when
    /// the proxy's port or token changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<String>,
    /// SB-421 review (P1-2): SHA-256 of the bytes Switchboard last wrote there. A file at that
    /// path is replaced only while it still holds exactly them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_sha256: Option<String>,
    /// Security round 2 (P2-2): when the group it was allowed for was made (`Project::
    /// created_at`; None for the default group). A group deleted since, or a new one that took
    /// its id, serves it nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_created_at: Option<i64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u8,
    users: Vec<NamedUser>,
}

pub const UNREADABLE: &str = "The list of other users the proxy serves could not be read. Restore the private file proxy-users.json in Switchboard's data folder, or remove it to allow nobody.";
pub const UNWRITTEN: &str =
    "The list of other users the proxy serves could not be saved. Check Switchboard's data folder, then retry.";

fn valid_pool(pool: &str) -> bool {
    !pool.is_empty()
        && pool.len() <= 32
        && pool
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
fn valid(user: &NamedUser) -> bool {
    user.uid != 0
        && valid_pool(&user.pool)
        && user.salt.len() == 64
        && user.salt.bytes().all(|b| b.is_ascii_hexdigit())
        && user.folders.iter().all(|f| Path::new(f).is_absolute())
}

/// The list, or an error when the file exists and cannot be read. Absent: nobody.
pub fn load(root: &Path) -> Result<Vec<NamedUser>, String> {
    let path = root.join(FILE);
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(Vec::new());
    }
    let bytes = switchboard_core::private_fs::read_private(&path, MAX_BYTES)
        .map_err(|_| UNREADABLE.to_string())?;
    let file: File = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE.to_string())?;
    if file.version != 1 {
        return Err(UNREADABLE.into());
    }
    Ok(file.users.into_iter().filter(valid).collect())
}

/// Saves the list (0600, by rename); an empty list removes the file.
pub fn save(root: &Path, users: &[NamedUser]) -> Result<(), String> {
    let path = root.join(FILE);
    if users.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(UNWRITTEN.into()),
        };
    }
    let file = File {
        version: 1,
        users: users.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|_| UNWRITTEN.to_string())?;
    switchboard_core::private_fs::private_write(&path, &bytes).map_err(|_| UNWRITTEN.into())
}

/// The capability of one named account for its group: the session token, the group, the uid and
/// the account's salt. Knowing it reveals neither the session token nor any other capability,
/// and it is worthless for another group, another account or after a new salt.
pub fn user_token(token: &str, pool: &str, uid: u32, salt: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("switchboard-user:{pool}:{uid}:{salt}:{token}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh salt: two random UUIDs, hashed.
pub fn new_salt() -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// What tells a changed file from the one read: its length, modification time and, on Unix, its
/// inode (every save is a rename, so a new file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    inode: u64,
}
fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::symlink_metadata(path).ok()?;
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let inode = 0;
    Some(Stamp {
        len: m.len(),
        modified: m.modified().ok(),
        inode,
    })
}

/// The stamp of the file last read (none: absent) and the list read from it.
type Cached = Option<(Option<Stamp>, Arc<Vec<NamedUser>>)>;
/// The proxy's view of the list: read again only when the file changed. A file that cannot be
/// read lets nobody in (fails closed).
pub struct Users {
    root: Option<PathBuf>,
    cache: Mutex<Cached>,
    /// Security round 2 (P3): one read of the file at a time, outside `cache`, so the request
    /// path's `find_cached` never waits on file I/O and reads land in the order they ran.
    loading: Mutex<()>,
}
impl Users {
    pub fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_owned()),
            cache: Mutex::new(None),
            loading: Mutex::new(()),
        }
    }
    /// A list that is always empty: a system where another account's connection is never let in.
    pub fn none() -> Self {
        Self {
            root: None,
            cache: Mutex::new(None),
            loading: Mutex::new(()),
        }
    }
    pub fn current(&self) -> Arc<Vec<NamedUser>> {
        let Some(root) = &self.root else {
            return Arc::default();
        };
        // Security round 3 (P3-3): `loading` guards no data, so a read that panicked leaves
        // nothing half done: go on, rather than freeze a cache that may still name a user
        // disallowed since.
        let _loading = self
            .loading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = stamp(&root.join(FILE));
        {
            let Ok(cache) = self.cache.lock() else {
                return Arc::default();
            };
            if let Some((seen, users)) = cache.as_ref() {
                if *seen == now {
                    return users.clone();
                }
            }
        }
        let users = Arc::new(match now {
            None => Vec::new(),
            Some(_) => load(root).unwrap_or_default(),
        });
        let Ok(mut cache) = self.cache.lock() else {
            return Arc::default();
        };
        *cache = Some((now, users.clone()));
        users
    }
    /// The named account with this uid, if listed now. Blocking: it may stat and read the file.
    pub fn find(&self, uid: u32) -> Option<NamedUser> {
        self.current().iter().find(|u| u.uid == uid).cloned()
    }
    /// 2026-11-19 audit M5-11: the list as last read, never touching the file, for a request's
    /// handler on the async runtime. The accept loop (`peer::OwnUser`, on a blocking thread)
    /// reads the file before it admits a named connection, the owner reads it again after each
    /// change it makes (`ProxyHandle::reload_users`), and a refresher reads it every
    /// [`REFRESH`] while anyone is listed, so a list changed by hand also applies to a kept-alive
    /// connection. Never read: nobody.
    pub fn find_cached(&self, uid: u32) -> Option<NamedUser> {
        let cache = self.cache.lock().ok()?;
        let (_, users) = cache.as_ref()?;
        users.iter().find(|u| u.uid == uid).cloned()
    }
    /// Whether the list as last read names anyone (the refresher's cue; no file read).
    pub fn any_cached(&self) -> bool {
        self.cache
            .lock()
            .ok()
            .and_then(|c| c.as_ref().map(|(_, users)| !users.is_empty()))
            .unwrap_or(false)
    }
}

/// M5-11: how often the proxy reads the list again while it names anyone (one stat, a read only
/// when the file changed), so a change made outside Switchboard reaches open connections.
pub const REFRESH: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(test)]
mod tests {
    use super::*;

    fn user(uid: u32, pool: &str) -> NamedUser {
        NamedUser {
            uid,
            name: Some(format!("agent-{uid}")),
            pool: pool.into(),
            folders: vec!["/srv/agents/a".into()],
            salt: new_salt(),
            added_at: 1,
            added_by: "cli".into(),
            settings: None,
            settings_sha256: None,
            group_created_at: None,
        }
    }

    #[test]
    fn the_list_is_private_and_holds_no_capability() {
        let root = tempfile::tempdir().unwrap();
        let token = "a".repeat(64);
        let u = user(1001, "team");
        save(root.path(), std::slice::from_ref(&u)).unwrap();
        let path = root.path().join(FILE);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&token));
        assert!(!text.contains(&user_token(&token, "team", 1001, &u.salt)));
        assert_eq!(load(root.path()).unwrap(), vec![u]);
        save(root.path(), &[]).unwrap();
        assert!(!path.exists());
        assert!(load(root.path()).unwrap().is_empty());
    }

    #[test]
    fn a_capability_is_bound_to_its_group_uid_salt_and_token() {
        let t = "b".repeat(64);
        let base = user_token(&t, "team", 1001, "s1");
        assert_eq!(base.len(), 64);
        assert_ne!(base, user_token(&t, "other", 1001, "s1"));
        assert_ne!(base, user_token(&t, "team", 1002, "s1"));
        assert_ne!(base, user_token(&t, "team", 1001, "s2"));
        assert_ne!(base, user_token(&"c".repeat(64), "team", 1001, "s1"));
        assert_ne!(base, crate::pool_token(&t, "team"));
    }

    /// Unix only: on Windows the proxy names nobody (`Users::none`), and a file's stamp there has
    /// no inode, so two saves of one length within the clock's step read as one (review round 1).
    #[cfg(unix)]
    #[test]
    fn the_view_follows_the_file_and_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let users = Users::new(root.path());
        assert!(users.find(1001).is_none());
        save(root.path(), &[user(1001, "team")]).unwrap();
        assert_eq!(users.find(1001).unwrap().pool, "team");
        save(root.path(), &[user(1002, "team")]).unwrap();
        assert!(
            users.find(1001).is_none(),
            "a disallowed uid is out at once"
        );
        assert!(users.find(1002).is_some());
        std::fs::write(root.path().join(FILE), b"not json").unwrap();
        assert!(
            users.find(1002).is_none(),
            "an unreadable list lets nobody in"
        );
        assert!(Users::none().find(1002).is_none());
    }

    #[test]
    fn root_and_malformed_rows_are_never_listed() {
        let root = tempfile::tempdir().unwrap();
        let mut rows = vec![user(0, "team"), user(1003, "Bad Pool")];
        rows.push(NamedUser {
            folders: vec!["relative".into()],
            ..user(1004, "team")
        });
        rows.push(user(1005, "team"));
        save(root.path(), &rows).unwrap();
        let listed: Vec<u32> = load(root.path()).unwrap().iter().map(|u| u.uid).collect();
        assert_eq!(listed, [1005]);
    }
}
