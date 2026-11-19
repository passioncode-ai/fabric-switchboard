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
//! it again whenever the file changed (`Users::current`), so allowing, disallowing and rotating
//! take effect at the next connection and the next request, with no owner restart.
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

/// SB-442: what one named account used: requests served, and the list-price cost of its Claude
/// answers in micro-dollars (unknown prices count nothing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub requests: u64,
    pub cost_micros: u64,
}
impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.requests = self.requests.saturating_add(other.requests);
        self.cost_micros = self.cost_micros.saturating_add(other.cost_micros);
    }
}

/// SB-442: each named account's use per UTC day, `<data>/proxy-user-usage.json` (0600): `{day:
/// {uid: Usage}}`, counts only, the newest 90 days kept.
pub const USAGE_FILE: &str = "proxy-user-usage.json";
const USAGE_DAYS: usize = 90;
pub type UsageBook = std::collections::BTreeMap<String, std::collections::BTreeMap<u32, Usage>>;

/// The book of every day kept; an absent or unreadable file reads as empty.
pub fn usage(root: &Path) -> UsageBook {
    switchboard_core::private_fs::read_private(&root.join(USAGE_FILE), 4 * 1024 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Adds `new` to `day` and keeps the newest days.
pub fn add_usage(
    root: &Path,
    day: &str,
    new: &std::collections::BTreeMap<u32, Usage>,
) -> Result<(), String> {
    if new.is_empty() {
        return Ok(());
    }
    let mut book = usage(root);
    let today = book.entry(day.to_owned()).or_default();
    for (uid, u) in new {
        today.entry(*uid).or_default().add(*u);
    }
    while book.len() > USAGE_DAYS {
        let oldest = book.keys().next().cloned().unwrap_or_default();
        book.remove(&oldest);
    }
    let bytes = serde_json::to_vec(&book).map_err(|_| UNWRITTEN.to_string())?;
    switchboard_core::private_fs::private_write(&root.join(USAGE_FILE), &bytes)
        .map_err(|_| UNWRITTEN.into())
}

/// One account's use on `day`, and from `from` to `day` (UTC days `YYYY-MM-DD` compare as text).
pub fn usage_of(book: &UsageBook, uid: u32, day: &str, from: &str) -> (Usage, Usage) {
    let mut today = Usage::default();
    let mut window = Usage::default();
    for (d, users) in book.range(from.to_owned()..=day.to_owned()) {
        if let Some(u) = users.get(&uid) {
            window.add(*u);
            if d == day {
                today.add(*u);
            }
        }
    }
    (today, window)
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
}
impl Users {
    pub fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_owned()),
            cache: Mutex::new(None),
        }
    }
    /// A list that is always empty: a system where another account's connection is never let in.
    pub fn none() -> Self {
        Self {
            root: None,
            cache: Mutex::new(None),
        }
    }
    pub fn current(&self) -> Arc<Vec<NamedUser>> {
        let Some(root) = &self.root else {
            return Arc::default();
        };
        let now = stamp(&root.join(FILE));
        let Ok(mut cache) = self.cache.lock() else {
            return Arc::default();
        };
        if let Some((seen, users)) = cache.as_ref() {
            if *seen == now {
                return users.clone();
            }
        }
        let users = Arc::new(match now {
            None => Vec::new(),
            Some(_) => load(root).unwrap_or_default(),
        });
        *cache = Some((now, users.clone()));
        users
    }
    /// The named account with this uid, if listed now.
    pub fn find(&self, uid: u32) -> Option<NamedUser> {
        self.current().iter().find(|u| u.uid == uid).cloned()
    }
}

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

    /// SB-442: per-uid use adds up per UTC day, holds counts only and keeps 90 days.
    #[test]
    fn use_per_uid_adds_up_per_day_and_keeps_ninety_days() {
        let root = tempfile::tempdir().unwrap();
        let one = |requests, cost_micros| [(1001u32, Usage { requests, cost_micros })].into();
        add_usage(root.path(), "2026-11-17", &one(2, 300)).unwrap();
        add_usage(root.path(), "2026-11-18", &one(1, 50)).unwrap();
        add_usage(root.path(), "2026-11-18", &one(3, 0)).unwrap();
        let book = usage(root.path());
        let (today, window) = usage_of(&book, 1001, "2026-11-18", "2026-10-20");
        assert_eq!(today, Usage { requests: 4, cost_micros: 50 });
        assert_eq!(window, Usage { requests: 6, cost_micros: 350 });
        assert_eq!(usage_of(&book, 1002, "2026-11-18", "2026-10-20").1, Usage::default());
        for d in 0..95u32 {
            let day = format!("2027-{:02}-{:02}", d / 28 + 1, d % 28 + 1);
            add_usage(root.path(), &day, &one(1, 1)).unwrap();
        }
        assert_eq!(usage(root.path()).len(), 90);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(USAGE_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
