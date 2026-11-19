//! SB-421 (DEC-0029): other accounts of this computer the proxy serves (`proxy allow-user`,
//! `proxy disallow-user`, `proxy users`, `proxy rotate-user`; MCP `switchboard_proxy_users`;
//! Settings in the window). The list is `switchboard_proxy::users` (`<data>/proxy-users.json`,
//! 0600); this module resolves an account's name, checks the group, writes the account's Claude
//! Code settings file without ever printing the capability, and rewrites recorded settings files
//! when the proxy's port or token changes.
//!
//! macOS and Linux only: on Windows the proxy cannot name the account that opened a connection
//! by a uid (SB-274 compares it with its own), so every call is refused there.
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::Store;
use switchboard_proxy::users::{self, NamedUser};

pub const UNSUPPORTED: &str = "Serving other users of this computer works on macOS and Linux only. On Windows Switchboard's proxy serves only the account it runs as.";
pub const NO_USER: &str = "Name the account: its user name or uid.";
pub const UNKNOWN_USER: &str = "No account of this computer has that name or uid.";
pub const SELF_USER: &str =
    "That is the account Switchboard runs as: its sessions are served already.";
pub const ROOT_USER: &str = "Switchboard never serves the root account's processes.";
pub const NO_GROUP: &str = "Name the group whose accounts this user may use.";
pub const UNKNOWN_GROUP: &str = "No group has that id or name.";
pub const NOT_LISTED: &str = "That account is not on the list of users the proxy serves.";
pub const RELATIVE_FOLDER: &str = "Give each folder as an absolute path.";
pub const RELATIVE_SETTINGS: &str = "Give the settings file as an absolute path.";
pub const TOO_MANY: &str = "The proxy serves at most 256 other users.";
pub const NO_PROXY: &str = "Switchboard's proxy has not started on this computer yet, so the settings file cannot name its address. Start Switchboard (switchboard serve), then retry.";
pub const SETTINGS_PARENT: &str =
    "The settings file's folder does not exist. Create it, then retry.";
pub const SETTINGS_NOT_OURS: &str = "A file Switchboard did not write is at that path (a link, a folder or another account's file). Remove it or name another path.";
pub const SETTINGS_FOLDER_UNSAFE: &str = "A folder on the way to the settings file is a link, or an account other than root and the one Switchboard runs as may write to it. Name a path whose folders only those two can change.";
pub const SETTINGS_CHANGED: &str = "The file at that path is not the one Switchboard wrote there last, or it changed since. Remove it or name another path.";
pub const UID_REUSED: &str = "The account this uid was allowed for is gone or has another name now. Disallow it, then allow the account again.";
pub const SETTINGS_UNWRITTEN: &str =
    "The settings file could not be written there. Check the folder's permissions, then retry.";
pub const UNKNOWN_ACTION: &str = "Unknown action: use list, allow, disallow or rotate.";

/// One account of this computer, as the password database names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub uid: u32,
    pub name: String,
    /// Its primary group.
    pub gid: u32,
    pub home: String,
}

/// The account named `who`: a user name, or a uid in decimal.
#[cfg(unix)]
pub fn account(who: &str) -> Option<Account> {
    let who = who.trim();
    if who.is_empty() || who.len() > 256 {
        return None;
    }
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 16 * 1024];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let rc = if let Ok(uid) = who.parse::<u32>() {
        // SAFETY: every pointer is to a live buffer of the stated size; `found` is null or `pwd`.
        unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found) }
    } else {
        let name = std::ffi::CString::new(who).ok()?;
        // SAFETY: as above; `name` is a NUL-terminated copy.
        unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                &mut pwd,
                buf.as_mut_ptr(),
                buf.len(),
                &mut found,
            )
        }
    };
    if rc != 0 || found.is_null() {
        return None;
    }
    let text = |p: *const libc::c_char| {
        if p.is_null() {
            String::new()
        } else {
            // SAFETY: the passwd entry's strings point into `buf`, NUL-terminated.
            unsafe { std::ffi::CStr::from_ptr(p) }
                .to_string_lossy()
                .into_owned()
        }
    };
    Some(Account {
        uid: pwd.pw_uid,
        name: text(pwd.pw_name),
        gid: pwd.pw_gid,
        home: text(pwd.pw_dir),
    })
}
#[cfg(not(unix))]
pub fn account(_who: &str) -> Option<Account> {
    None
}

/// This process's uid.
fn own_uid() -> Option<u32> {
    switchboard_proxy::peer::own_uid()
}

/// Quotes a path for a POSIX shell line.
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The settings a named account's Claude Code reads: the proxy's address for its group and its
/// own capability as the bearer, so `claude -p` runs without a sign-in of its own.
fn settings_bytes(port: u16, pool: &str, capability: &str) -> Vec<u8> {
    let mut env = serde_json::Map::new();
    env.insert(
        crate::bindings::BASE_URL.into(),
        json!(format!("http://127.0.0.1:{port}/claude/{pool}")),
    );
    env.insert(crate::bindings::AUTH_TOKEN.into(), json!(capability));
    let value = json!({ "env": env });
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// What writing a settings file did.
#[derive(Debug, PartialEq, Eq)]
pub struct Written {
    /// 0o640 when the file's group is the account's own private group, else 0o600.
    pub mode: u32,
    pub shared_with_group: bool,
    /// SHA-256 of the bytes written, recorded so the next write replaces only this file.
    pub sha256: String,
}

#[cfg_attr(not(unix), allow(dead_code))]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The last OS error number.
#[cfg(unix)]
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// A folder may hold a settings file only when root or this account owns it and nobody else may
/// write in it (a sticky folder such as `/tmp` excepted: there nobody may rename or remove what
/// another account owns).
#[cfg(unix)]
fn folder_is_safe(fd: std::os::fd::RawFd, me: u32) -> Result<(), String> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is an open descriptor this module owns; `st` is a live buffer.
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Err(SETTINGS_UNWRITTEN.into());
    }
    let mode = u32::from(st.st_mode);
    let owner_ok = st.st_uid == 0 || st.st_uid == me;
    let sticky = mode & u32::from(libc::S_ISVTX) != 0;
    // Group write counts only when the group is this account's own private group (a umask of
    // 002 makes every new folder group-writable on many Linux systems).
    let group_write = mode & 0o020 != 0 && !own_private_group(st.st_gid, me);
    let others_write = (mode & 0o002 != 0 || group_write) && !sticky;
    if (mode & u32::from(libc::S_IFMT)) != u32::from(libc::S_IFDIR) || !owner_ok || others_write {
        return Err(SETTINGS_FOLDER_UNSAFE.into());
    }
    Ok(())
}

/// Whether `gid` is the private group of the account `me`: named as the account, with no member
/// but it.
#[cfg(unix)]
fn own_private_group(gid: u32, me: u32) -> bool {
    let Some(name) = switchboard_proxy::peer::account_name(me) else {
        return false;
    };
    group_entry(gid).is_some_and(|(group, members)| group == name && members.iter().all(|m| *m == name))
}

/// SB-421 review (P1-2): opens `parent` one folder at a time from `/`, never through a link, and
/// refuses it when any folder on the way is a link or writable by an account other than root and
/// this one. The file is then written through this handle, so no folder can be swapped under it.
#[cfg(unix)]
fn open_safe_folder(parent: &Path, me: u32) -> Result<std::os::fd::OwnedFd, String> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
    // SAFETY: a NUL-terminated literal; the descriptor is owned from here on.
    let raw = unsafe { libc::open(c"/".as_ptr(), flags) };
    if raw < 0 {
        return Err(SETTINGS_UNWRITTEN.into());
    }
    let mut folder = unsafe { OwnedFd::from_raw_fd(raw) };
    folder_is_safe(folder.as_raw_fd(), me)?;
    for part in parent.components() {
        let name = match part {
            std::path::Component::RootDir => continue,
            std::path::Component::Normal(name) => name,
            _ => return Err(RELATIVE_SETTINGS.into()),
        };
        let name = std::ffi::CString::new(name.as_bytes()).map_err(|_| SETTINGS_PARENT)?;
        // SAFETY: `folder` is open; `name` is NUL-terminated.
        let raw =
            unsafe { libc::openat(folder.as_raw_fd(), name.as_ptr(), flags | libc::O_NOFOLLOW) };
        if raw < 0 {
            let error = errno();
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: as above; `st` is a live buffer.
            let exists = unsafe {
                libc::fstatat(
                    folder.as_raw_fd(),
                    name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } == 0;
            let link = exists && (st.st_mode & libc::S_IFMT) == libc::S_IFLNK;
            return Err(if link || error == libc::ELOOP {
                SETTINGS_FOLDER_UNSAFE
            } else if error == libc::ENOENT || error == libc::ENOTDIR {
                SETTINGS_PARENT
            } else {
                SETTINGS_UNWRITTEN
            }
            .into());
        }
        folder = unsafe { OwnedFd::from_raw_fd(raw) };
        folder_is_safe(folder.as_raw_fd(), me)?;
    }
    Ok(folder)
}

/// Writes `bytes` at `path` as this account's file, through a handle on its folder: a new file
/// renamed over the old one, mode 0640 with the group `share_gid` when given and this account may
/// set it, else 0600. SB-421 review (P1-2): the folders on the way must be safe
/// (`open_safe_folder`), and a file already at `path` is replaced only when it is this account's
/// regular file with one link holding exactly the bytes recorded at the last write (`expected`);
/// anything else is refused, so a rewrite can never land on another file of this account.
#[cfg(unix)]
pub fn write_settings(
    path: &Path,
    bytes: &[u8],
    share_gid: Option<u32>,
    expected: Option<&str>,
) -> Result<Written, String> {
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    if !path.is_absolute() {
        return Err(RELATIVE_SETTINGS.into());
    }
    let parent = path.parent().ok_or(SETTINGS_PARENT)?;
    let name = path.file_name().ok_or(RELATIVE_SETTINGS)?;
    let me = own_uid().ok_or(SETTINGS_UNWRITTEN)?;
    let folder = open_safe_folder(parent, me)?;
    let dir = folder.as_raw_fd();
    let c_name = std::ffi::CString::new(name.as_bytes()).map_err(|_| RELATIVE_SETTINGS)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is open; `c_name` is NUL-terminated; `st` is a live buffer.
    let present =
        unsafe { libc::fstatat(dir, c_name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } == 0;
    if present {
        if (st.st_mode & libc::S_IFMT) != libc::S_IFREG || st.st_uid != me || st.st_nlink != 1 {
            return Err(SETTINGS_NOT_OURS.into());
        }
        let Some(expected) = expected else {
            return Err(SETTINGS_CHANGED.into());
        };
        // SAFETY: as above; the descriptor is owned by `file` from here on.
        let raw = unsafe {
            libc::openat(
                dir,
                c_name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if raw < 0 {
            return Err(SETTINGS_NOT_OURS.into());
        }
        let file = unsafe { std::fs::File::from_raw_fd(raw) };
        let mut held = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut held)
            .map_err(|_| SETTINGS_NOT_OURS.to_string())?;
        if sha256_hex(&held) != expected {
            return Err(SETTINGS_CHANGED.into());
        }
    }
    let temporary = format!(
        ".{}.switchboard-{}.tmp",
        name.to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    );
    let c_temp = std::ffi::CString::new(temporary).map_err(|_| SETTINGS_UNWRITTEN)?;
    // SAFETY: as above; O_EXCL and O_NOFOLLOW: a new file, never through a link.
    let raw = unsafe {
        libc::openat(
            dir,
            c_temp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if raw < 0 {
        return Err(SETTINGS_UNWRITTEN.into());
    }
    let result = (|| {
        let mut file = unsafe { std::fs::File::from_raw_fd(raw) };
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| SETTINGS_UNWRITTEN.to_string())?;
        // SAFETY: the descriptor is this function's open file; -1 leaves the owner unchanged.
        let grouped = share_gid.is_some_and(|gid| unsafe {
            libc::fchown(file.as_raw_fd(), u32::MAX as libc::uid_t, gid as libc::gid_t)
        } == 0);
        let mode: u32 = if grouped { 0o640 } else { 0o600 };
        // SAFETY: as above.
        if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(SETTINGS_UNWRITTEN.to_string());
        }
        drop(file);
        // SAFETY: both names are NUL-terminated and relative to the same open folder.
        if unsafe { libc::renameat(dir, c_temp.as_ptr(), dir, c_name.as_ptr()) } != 0 {
            return Err(SETTINGS_UNWRITTEN.to_string());
        }
        // SAFETY: `dir` is open; a failed folder sync only loses durability.
        unsafe { libc::fsync(dir) };
        Ok(Written {
            mode,
            shared_with_group: grouped,
            sha256: sha256_hex(bytes),
        })
    })();
    if result.is_err() {
        // SAFETY: as above; the temporary is ours.
        unsafe { libc::unlinkat(dir, c_temp.as_ptr(), 0) };
    }
    result
}
#[cfg(not(unix))]
pub fn write_settings(
    _path: &Path,
    _bytes: &[u8],
    _share_gid: Option<u32>,
    _expected: Option<&str>,
) -> Result<Written, String> {
    Err(UNSUPPORTED.into())
}

/// The members of group `gid` and its name, as the group database lists them.
#[cfg(unix)]
fn group_entry(gid: u32) -> Option<(String, Vec<String>)> {
    let mut grp: libc::group = unsafe { std::mem::zeroed() };
    // 8-byte words: the member list the call writes into the buffer is an array of pointers.
    let mut buf = vec![0u64; 8 * 1024];
    let mut found: *mut libc::group = std::ptr::null_mut();
    // SAFETY: every pointer is to a live buffer of the stated size; `found` is null or `grp`.
    let rc = unsafe {
        libc::getgrgid_r(
            gid,
            &mut grp,
            buf.as_mut_ptr().cast(),
            buf.len() * 8,
            &mut found,
        )
    };
    if rc != 0 || found.is_null() || grp.gr_name.is_null() {
        return None;
    }
    let text = |p: *const libc::c_char| {
        // SAFETY: the entry's strings point into `buf`, NUL-terminated.
        unsafe { std::ffi::CStr::from_ptr(p) }
            .to_string_lossy()
            .into_owned()
    };
    let mut members = Vec::new();
    if !grp.gr_mem.is_null() {
        for i in 0.. {
            // SAFETY: `gr_mem` is a NULL-terminated array of strings in `buf`.
            let member = unsafe { grp.gr_mem.add(i).read_unaligned() };
            if member.is_null() || i > 4096 {
                break;
            }
            members.push(text(member));
        }
    }
    Some((text(grp.gr_name), members))
}
#[cfg(not(unix))]
fn group_entry(_gid: u32) -> Option<(String, Vec<String>)> {
    None
}

/// SB-421 review (P2-2, P2-3): the group a settings file may be given to: the account's primary
/// group only when it is its own private group (named as the account, with no member but the
/// account and this service account). An account that could not be looked up gets none.
fn private_group(account: Option<&Account>) -> Option<u32> {
    let account = account?;
    let (name, members) = group_entry(account.gid)?;
    let me = own_uid().and_then(switchboard_proxy::peer::account_name);
    let private = name == account.name
        && members
            .iter()
            .all(|m| *m == account.name || Some(m) == me.as_ref());
    private.then_some(account.gid)
}

/// The proxy's port and session token: the running owner's, else the recorded ones.
fn proxy_address(runtime: Option<&crate::Runtime>, root: &Path) -> Option<(u16, String)> {
    match runtime {
        Some(r) => Some((r.proxy.address().port(), r.proxy.token().to_owned())),
        None => crate::load_proxy_record(root).map(|r| (r.port, r.token)),
    }
}

/// Writes `user`'s settings file at `path` and answers what was done, never the capability, with
/// the digest of the bytes written. `expected` is the digest recorded at the last write there.
fn deliver(
    user: &NamedUser,
    account: Option<&Account>,
    path: &Path,
    proxy: Option<&(u16, String)>,
    expected: Option<&str>,
) -> Result<(Value, String), String> {
    let (port, token) = proxy.ok_or(NO_PROXY)?;
    let capability = users::user_token(token, &user.pool, user.uid, &user.salt);
    let written = write_settings(
        path,
        &settings_bytes(*port, &user.pool, &capability),
        private_group(account),
        expected,
    )?;
    let shown = path.display().to_string();
    let mut answer = json!({
        "path": shown,
        "mode": format!("{:04o}", written.mode),
        "readable_by_group": written.shared_with_group,
    });
    if let Some(a) = account.filter(|a| !written.shared_with_group && !a.home.is_empty()) {
        // SB-421 review (P1-1): the file stays the service account's (0600); a person with sudo
        // copies it into the user's own folder, written AS that user (`sudo -u`), so nothing runs
        // as root under a path the user controls. The line carries no secret.
        let folder = format!("{}/.claude", a.home.trim_end_matches('/'));
        let destination = format!("{folder}/switchboard-settings.json");
        let inner = format!(
            "umask 077; mkdir -p {} && cat > {}",
            sh_quote(&folder),
            sh_quote(&destination)
        );
        answer["install_command"] = json!(format!(
            "sudo cat {} | sudo -u {} sh -c {}",
            sh_quote(&shown),
            sh_quote(&a.name),
            sh_quote(&inner)
        ));
        answer["installed_path"] = json!(destination);
    }
    Ok((answer, written.sha256))
}

/// SB-442: the account's use today and over the last 30 UTC days, from the day file.
fn usage_view(root: &Path, uid: u32) -> Value {
    let now = crate::monitor::now();
    let day = switchboard_proxy::stats::day_of(now);
    let from = switchboard_proxy::stats::day_of(now - 29 * 86_400);
    let (today, window) = users::usage_of(&users::usage(root), uid, &day, &from);
    json!({
        "today": {"requests": today.requests, "cost_micros": today.cost_micros},
        "last_30_days": {"requests": window.requests, "cost_micros": window.cost_micros},
    })
}

/// The answer's view of one listed account: never the salt.
fn view(user: &NamedUser, store: &Store) -> Value {
    let group_name = store.snapshot().ok().and_then(|s| {
        s.projects
            .iter()
            .find(|p| p.pool == user.pool)
            .map(|p| p.name.clone())
    });
    json!({
        "uid": user.uid,
        "name": user.name,
        "group": user.pool,
        "group_name": group_name,
        "folders": user.folders,
        "added_at": user.added_at,
        "added_by": user.added_by,
        "settings": user.settings,
    })
}

/// The group named `group`: its pool id, or its name; `default` is the default group.
fn group_pool(store: &Store, group: &str) -> Result<String, String> {
    let group = group.trim();
    if group.is_empty() {
        return Err(NO_GROUP.into());
    }
    if group == "default" {
        return Ok("default".into());
    }
    let snapshot = store.snapshot()?;
    snapshot
        .projects
        .iter()
        .find(|p| p.pool == group)
        .or_else(|| snapshot.projects.iter().find(|p| p.name == group))
        .map(|p| p.pool.clone())
        .ok_or_else(|| UNKNOWN_GROUP.into())
}

fn normal_folder(folder: &str) -> Result<String, String> {
    let path = Path::new(folder.trim());
    if !path.is_absolute() {
        return Err(RELATIVE_FOLDER.into());
    }
    let text = path.display().to_string();
    let trimmed = text.trim_end_matches('/');
    Ok(if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.into()
    })
}

/// The listed entry `who` names: by the password database first, else by the uid or the name
/// recorded (an account removed from the computer can still be taken off the list).
fn listed_index(list: &[NamedUser], who: &str) -> Option<usize> {
    let uid = account(who)
        .map(|a| a.uid)
        .or_else(|| who.trim().parse().ok());
    list.iter().position(|u| Some(u.uid) == uid).or_else(|| {
        list.iter()
            .position(|u| u.name.as_deref() == Some(who.trim()))
    })
}

fn surface(by: Option<&str>) -> String {
    match by {
        Some(s @ ("cli" | "mcp" | "window")) => s.into(),
        _ => "cli".into(),
    }
}

/// `Operation::ProxyUsers`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute(
    store: &Store,
    root: &Path,
    runtime: Option<&crate::Runtime>,
    action: Option<&str>,
    who: Option<&str>,
    group: Option<&str>,
    folders: &[String],
    write_to: Option<&str>,
    by: Option<&str>,
) -> Result<Value, String> {
    if cfg!(windows) {
        return Err(UNSUPPORTED.into());
    }
    let mut list = users::load(root)?;
    match action.unwrap_or("list") {
        "list" => Ok(json!({
            "users": list
                .iter()
                .map(|u| {
                    let mut v = view(u, store);
                    v["usage"] = usage_view(root, u.uid);
                    v
                })
                .collect::<Vec<_>>(),
        })),
        "allow" => {
            let who = who.filter(|w| !w.trim().is_empty()).ok_or(NO_USER)?;
            let account = account(who).ok_or(UNKNOWN_USER)?;
            if Some(account.uid) == own_uid() {
                return Err(SELF_USER.into());
            }
            if account.uid == 0 {
                return Err(ROOT_USER.into());
            }
            let pool = group_pool(store, group.unwrap_or_default())?;
            let mut folders = folders
                .iter()
                .map(|f| normal_folder(f))
                .collect::<Result<Vec<_>, _>>()?;
            folders.dedup();
            let settings = write_to
                .map(|p| {
                    let path = Path::new(p.trim());
                    if path.is_absolute() {
                        Ok(path.display().to_string())
                    } else {
                        Err(RELATIVE_SETTINGS.to_string())
                    }
                })
                .transpose()?;
            let existing = list.iter().position(|u| u.uid == account.uid);
            // SB-421 review (P2-4): a uid listed under another name is a reused uid.
            if let Some(i) = existing {
                if list[i].name.as_deref().is_some_and(|n| n != account.name) {
                    return Err(UID_REUSED.into());
                }
            }
            let before = existing.map(|i| list[i].clone());
            // SB-421 review (P2-6): a new group rewrites the file written before, so its address
            // names the group the user is now allowed for.
            let target = settings.clone().or_else(|| {
                before
                    .as_ref()
                    .filter(|b| b.pool != pool)
                    .and_then(|b| b.settings.clone())
            });
            let mut entry = NamedUser {
                uid: account.uid,
                name: Some(account.name.clone()),
                pool: pool.clone(),
                folders,
                salt: existing
                    .map(|i| list[i].salt.clone())
                    .unwrap_or_else(users::new_salt),
                added_at: crate::monitor::now(),
                added_by: surface(by),
                settings: before.as_ref().and_then(|b| b.settings.clone()),
                settings_sha256: before.as_ref().and_then(|b| b.settings_sha256.clone()),
            };
            if existing.is_none() && list.len() >= users::MAX_USERS {
                return Err(TOO_MANY.into());
            }
            // The file first: a settings file the proxy would refuse is never left behind. A
            // file is replaced only where Switchboard wrote it last, with the bytes it wrote.
            let delivered = match &target {
                Some(path) => {
                    let expected = (entry.settings.as_deref() == Some(path.as_str()))
                        .then(|| entry.settings_sha256.clone())
                        .flatten();
                    let (answer, sha) = deliver(
                        &entry,
                        Some(&account),
                        Path::new(path),
                        proxy_address(runtime, root).as_ref(),
                        expected.as_deref(),
                    )?;
                    entry.settings = Some(path.clone());
                    entry.settings_sha256 = Some(sha);
                    Some(answer)
                }
                None => None,
            };
            match existing {
                Some(i) => list[i] = entry.clone(),
                None => list.push(entry.clone()),
            }
            users::save(root, &list)?;
            crate::oplog::event(
                "proxy_user",
                &[
                    ("action", crate::oplog::Field::Code("allow")),
                    ("uid", crate::oplog::Field::Number(account.uid.into())),
                    ("updated", crate::oplog::Field::Flag(existing.is_some())),
                ],
            );
            Ok(json!({"user": view(&entry, store), "settings_file": delivered}))
        }
        "disallow" => {
            let who = who.filter(|w| !w.trim().is_empty()).ok_or(NO_USER)?;
            let index = listed_index(&list, who).ok_or(NOT_LISTED)?;
            let removed = list.remove(index);
            users::save(root, &list)?;
            crate::oplog::event(
                "proxy_user",
                &[
                    ("action", crate::oplog::Field::Code("disallow")),
                    ("uid", crate::oplog::Field::Number(removed.uid.into())),
                ],
            );
            Ok(json!({"removed": view(&removed, store)}))
        }
        "rotate" => {
            let who = who.filter(|w| !w.trim().is_empty()).ok_or(NO_USER)?;
            let index = listed_index(&list, who).ok_or(NOT_LISTED)?;
            let account_now = account(&list[index].uid.to_string());
            // SB-421 review (P2-4): never a new capability for whoever holds a reused uid now.
            if !same_account(&list[index], account_now.as_ref()) {
                return Err(UID_REUSED.into());
            }
            let target = match write_to {
                Some(p) => {
                    let path = Path::new(p.trim());
                    if !path.is_absolute() {
                        return Err(RELATIVE_SETTINGS.into());
                    }
                    Some(path.display().to_string())
                }
                None => list[index].settings.clone(),
            };
            list[index].salt = users::new_salt();
            let entry = list[index].clone();
            // Saved first: the old capability is worthless from here on, whatever the file does.
            users::save(root, &list)?;
            crate::oplog::event(
                "proxy_user",
                &[
                    ("action", crate::oplog::Field::Code("rotate")),
                    ("uid", crate::oplog::Field::Number(entry.uid.into())),
                ],
            );
            let delivered = match &target {
                Some(path) => {
                    let expected = (entry.settings.as_deref() == Some(path.as_str()))
                        .then(|| entry.settings_sha256.clone())
                        .flatten();
                    Some(deliver(
                        &entry,
                        account_now.as_ref(),
                        Path::new(path),
                        proxy_address(runtime, root).as_ref(),
                        expected.as_deref(),
                    ))
                }
                None => None,
            };
            let (settings_file, settings_error) = match delivered {
                Some(Ok((answer, sha))) => {
                    list[index].settings = target.clone();
                    list[index].settings_sha256 = Some(sha);
                    users::save(root, &list)?;
                    (Some(answer), None)
                }
                Some(Err(e)) => (None, Some(e)),
                None => (None, None),
            };
            Ok(json!({
                "user": view(&entry, store),
                "rotated": true,
                "settings_file": settings_file,
                "settings_error": settings_error,
            }))
        }
        _ => Err(UNKNOWN_ACTION.into()),
    }
}

/// When the proxy started on another port or with another token: every recorded settings file is
/// written again with the new address and capability. Answers how many were written and how many
/// could not be (a file a person moved to the user with sudo is no longer this account's).
pub(crate) fn repoint(root: &Path, port: u16, token: &str) -> (usize, usize) {
    let Ok(mut list) = users::load(root) else {
        return (0, 0);
    };
    let proxy = (port, token.to_owned());
    let (mut written, mut failed) = (0, 0);
    for user in list.iter_mut().filter(|u| u.settings.is_some()) {
        let account = account(&user.uid.to_string());
        // SB-421 review (P2-4): a reused uid's file is left alone.
        if !same_account(user, account.as_ref()) {
            failed += 1;
            continue;
        }
        let path = user.settings.clone().unwrap_or_default();
        match deliver(
            user,
            account.as_ref(),
            Path::new(&path),
            Some(&proxy),
            user.settings_sha256.as_deref(),
        ) {
            Ok((_, sha)) => {
                user.settings_sha256 = Some(sha);
                written += 1;
            }
            Err(_) => failed += 1,
        }
    }
    if written > 0 && users::save(root, &list).is_err() {
        failed += written;
        written = 0;
    }
    (written, failed)
}

/// SB-421 review (P2-4): whether `account`, the password database's account for the listed uid
/// now, is the account it was listed for (an entry without a recorded name: any account).
fn same_account(user: &NamedUser, account: Option<&Account>) -> bool {
    match (&user.name, account) {
        (None, _) => true,
        (Some(name), Some(a)) => *name == a.name,
        (Some(_), None) => false,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use switchboard_core::MemoryVault;

    fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(
            Store::open(
                dir.path().join("data"),
                std::sync::Arc::new(MemoryVault::default()),
            )
            .unwrap(),
        );
        (dir, store)
    }

    /// Another account of this computer for the tests: any uid but this one and root, as the
    /// password database names it.
    fn another_account() -> Option<Account> {
        let me = own_uid()?;
        ["nobody", "daemon", "1", "2", "65534"]
            .iter()
            .filter_map(|w| account(w))
            .find(|a| a.uid != me && a.uid != 0)
    }

    #[test]
    fn the_password_database_names_this_account_by_name_and_uid() {
        let me = own_uid().unwrap();
        let by_uid = account(&me.to_string()).unwrap();
        assert_eq!(by_uid.uid, me);
        assert_eq!(account(&by_uid.name).unwrap().uid, me);
        assert!(account("no-such-account-sb421").is_none());
        assert!(account("").is_none());
    }

    /// A temporary folder by its real path (macOS's `/var` is a link, which a settings path
    /// may not pass through).
    fn real(dir: &tempfile::TempDir) -> std::path::PathBuf {
        std::fs::canonicalize(dir.path()).unwrap()
    }

    #[test]
    fn a_settings_file_holds_the_address_and_is_never_world_readable() {
        let dir = tempfile::tempdir().unwrap();
        let base = real(&dir);
        let path = base.join("agent-a.json");
        let bytes = settings_bytes(4321, "team", "c".repeat(64).as_str());
        // A group this account cannot give the file to: 0600, and the answer says so.
        let refused = write_settings(&path, &bytes, Some(0), None).unwrap();
        assert_eq!(refused.mode, 0o600);
        assert!(!refused.shared_with_group);
        assert_eq!(refused.sha256, sha256_hex(&bytes));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            v["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:4321/claude/team"
        );
        // This account's own primary group, with the digest of what is there: 0640.
        let gid = account(&own_uid().unwrap().to_string()).unwrap().gid;
        let shared = write_settings(&path, &bytes, Some(gid), Some(&refused.sha256)).unwrap();
        assert_eq!(shared.mode, 0o640);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        // P2-3: no group to give it to (no account looked up): 0600, never this account's group.
        let lone = write_settings(&path, &bytes, None, Some(&shared.sha256)).unwrap();
        assert_eq!(lone.mode, 0o600);
        // A link at the path is never followed or replaced.
        let link = base.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(
            write_settings(&link, &bytes, Some(gid), None),
            Err(SETTINGS_NOT_OURS.into())
        );
        assert_eq!(
            write_settings(Path::new("relative.json"), &bytes, Some(gid), None),
            Err(RELATIVE_SETTINGS.into())
        );
        assert_eq!(
            write_settings(&base.join("missing/x.json"), &bytes, Some(gid), None),
            Err(SETTINGS_PARENT.into())
        );
    }

    /// SB-421 review (P1-2): a rewrite never lands on another file of this account: an existing
    /// file is replaced only with the digest of what Switchboard wrote there, and a folder on the
    /// way that is a link, or that another account may write to, is refused.
    #[test]
    fn a_settings_file_replaces_only_its_own_bytes_and_never_passes_a_link_or_an_open_folder() {
        let dir = tempfile::tempdir().unwrap();
        let base = real(&dir);
        let bytes = settings_bytes(4321, "team", "c".repeat(64).as_str());
        // A file of this account that Switchboard did not write: never replaced.
        let theirs = base.join("authorized_keys");
        std::fs::write(&theirs, b"ssh-ed25519 AAAA keep-me\n").unwrap();
        assert_eq!(
            write_settings(&theirs, &bytes, None, None),
            Err(SETTINGS_CHANGED.into())
        );
        assert_eq!(
            write_settings(&theirs, &bytes, None, Some(&sha256_hex(b"something else"))),
            Err(SETTINGS_CHANGED.into())
        );
        assert_eq!(
            std::fs::read(&theirs).unwrap(),
            b"ssh-ed25519 AAAA keep-me\n"
        );
        // A folder swapped for a link to this account's own folder: refused.
        let home = base.join("home");
        std::fs::create_dir(&home).unwrap();
        let swapped = base.join("agent");
        std::os::unix::fs::symlink(&home, &swapped).unwrap();
        assert_eq!(
            write_settings(&swapped.join("settings.json"), &bytes, None, None),
            Err(SETTINGS_FOLDER_UNSAFE.into())
        );
        assert!(!home.join("settings.json").exists());
        // A folder anyone may write to, without the sticky bit: refused; with it: allowed.
        let open = base.join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            write_settings(&open.join("settings.json"), &bytes, None, None),
            Err(SETTINGS_FOLDER_UNSAFE.into())
        );
        let group = base.join("group");
        std::fs::create_dir(&group).unwrap();
        std::fs::set_permissions(&group, std::fs::Permissions::from_mode(0o770)).unwrap();
        // Group-writable: refused unless the group is this account's own private group.
        let gid = std::os::unix::fs::MetadataExt::gid(&std::fs::metadata(&group).unwrap());
        let in_group = write_settings(&group.join("settings.json"), &bytes, None, None);
        if own_private_group(gid, own_uid().unwrap()) {
            assert!(in_group.is_ok());
        } else {
            assert_eq!(in_group, Err(SETTINGS_FOLDER_UNSAFE.into()));
        }
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o1777)).unwrap();
        let written = write_settings(&open.join("settings.json"), &bytes, None, None).unwrap();
        assert_eq!(written.mode, 0o600);
        // Its own file again, with the recorded digest: replaced.
        assert!(write_settings(
            &open.join("settings.json"),
            &bytes,
            None,
            Some(&written.sha256)
        )
        .is_ok());
    }

    /// SB-421 review (P1-1): the hand-over line copies the file as the user, never as root into a
    /// path the user controls, and carries no capability.
    #[test]
    fn the_hand_over_line_writes_as_the_user_and_carries_no_secret() {
        let dir = tempfile::tempdir().unwrap();
        let base = real(&dir);
        let token = "e".repeat(64);
        let user = NamedUser {
            uid: 4242,
            name: Some("agent-a".into()),
            pool: "team".into(),
            folders: Vec::new(),
            salt: users::new_salt(),
            added_at: 1,
            added_by: "cli".into(),
            settings: None,
            settings_sha256: None,
        };
        let account = Account {
            uid: 4242,
            name: "agent-a".into(),
            gid: 0,
            home: "/home/agent-a".into(),
        };
        let path = base.join("agent-a.json");
        let (answer, _) = deliver(
            &user,
            Some(&account),
            &path,
            Some(&(4555, token.clone())),
            None,
        )
        .unwrap();
        let line = answer["install_command"].as_str().unwrap();
        assert!(line.starts_with("sudo cat '"), "{line}");
        assert!(line.contains("| sudo -u 'agent-a' sh -c "), "{line}");
        assert!(line.contains("umask 077; mkdir -p"), "{line}");
        assert!(!line.contains("sudo install"), "{line}");
        let capability = users::user_token(&token, "team", 4242, &user.salt);
        assert!(!answer.to_string().contains(&capability));
        assert_eq!(
            answer["installed_path"],
            "/home/agent-a/.claude/switchboard-settings.json"
        );
        // P2-3: an account not looked up gets no group and no hand-over line it cannot name.
        let (alone, _) = deliver(
            &user,
            None,
            &base.join("b.json"),
            Some(&(4555, token)),
            None,
        )
        .unwrap();
        assert_eq!(alone["mode"], "0600");
        assert!(alone.get("install_command").is_none());
    }

    #[test]
    fn allow_rotate_and_disallow_keep_the_capability_out_of_every_answer_and_file() {
        let Some(other) = another_account() else {
            return;
        };
        let (dir, store) = store();
        let root = store.root().to_owned();
        let token = "d".repeat(64);
        // The proxy's recorded address, as an offline command reads it.
        switchboard_core::private_fs::private_write(
            &root.join("proxy.json"),
            serde_json::to_vec(&json!({"protocol": 1, "port": 4555, "token": token}))
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let settings = real(&dir).join("agent.json");
        let settings_text = settings.display().to_string();
        let folder = ["/srv/agents/a/".to_owned()];
        let answer = execute(
            &store,
            &root,
            None,
            Some("allow"),
            Some(&other.name),
            Some("default"),
            &folder,
            Some(&settings_text),
            Some("mcp"),
        )
        .unwrap();
        assert_eq!(answer["user"]["uid"], other.uid);
        assert_eq!(answer["user"]["group"], "default");
        assert_eq!(answer["user"]["folders"][0], "/srv/agents/a");
        assert_eq!(answer["user"]["added_by"], "mcp");
        let listed = users::load(&root).unwrap();
        let capability = users::user_token(&token, "default", other.uid, &listed[0].salt);
        let answer_text = answer.to_string();
        assert!(!answer_text.contains(&capability) && !answer_text.contains(&token));
        assert!(!answer_text.contains(&listed[0].salt));
        let list_text = std::fs::read_to_string(root.join(users::FILE)).unwrap();
        assert!(!list_text.contains(&capability) && !list_text.contains(&token));
        let mode = std::fs::metadata(root.join(users::FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let written: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(written["env"]["ANTHROPIC_AUTH_TOKEN"], capability.as_str());
        if answer["settings_file"]["readable_by_group"] == false {
            assert!(answer["settings_file"]["install_command"]
                .as_str()
                .unwrap()
                .starts_with("sudo cat "));
        }
        // Rotation: a new capability, written into the recorded file.
        let rotated = execute(
            &store,
            &root,
            None,
            Some("rotate"),
            Some(&other.uid.to_string()),
            None,
            &[],
            None,
            Some("cli"),
        )
        .unwrap();
        assert_eq!(rotated["rotated"], true);
        let salt = users::load(&root).unwrap()[0].salt.clone();
        assert_ne!(salt, listed[0].salt);
        let fresh = users::user_token(&token, "default", other.uid, &salt);
        let written: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(written["env"]["ANTHROPIC_AUTH_TOKEN"], fresh.as_str());
        assert!(!rotated.to_string().contains(&fresh));
        // A new port and token after a restart: the file follows.
        assert_eq!(repoint(&root, 4777, &"e".repeat(64)), (1, 0));
        let written: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(
            written["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:4777/claude/default"
        );
        // Disallowed: off the list, and the file goes when it is empty.
        let removed = execute(
            &store,
            &root,
            None,
            Some("disallow"),
            Some(&other.name),
            None,
            &[],
            None,
            None,
        )
        .unwrap();
        assert_eq!(removed["removed"]["uid"], other.uid);
        assert!(!root.join(users::FILE).exists());
        assert_eq!(
            execute(
                &store,
                &root,
                None,
                Some("disallow"),
                Some(&other.name),
                None,
                &[],
                None,
                None
            ),
            Err(NOT_LISTED.into())
        );
    }

    #[test]
    fn this_account_root_unknown_users_and_groups_and_relative_paths_are_refused() {
        let (_dir, store) = store();
        let root = store.root().to_owned();
        let me = own_uid().unwrap().to_string();
        let allow = |who: &str, group: &str, folders: &[String], settings: Option<&str>| {
            execute(
                &store,
                &root,
                None,
                Some("allow"),
                Some(who),
                Some(group),
                folders,
                settings,
                None,
            )
        };
        assert_eq!(allow(&me, "default", &[], None), Err(SELF_USER.into()));
        if account("0").is_some() && own_uid() != Some(0) {
            assert_eq!(allow("0", "default", &[], None), Err(ROOT_USER.into()));
        }
        assert_eq!(
            allow("no-such-account-sb421", "default", &[], None),
            Err(UNKNOWN_USER.into())
        );
        if let Some(other) = another_account() {
            let name = other.name.as_str();
            assert_eq!(
                allow(name, "no-such-group", &[], None),
                Err(UNKNOWN_GROUP.into())
            );
            assert_eq!(allow(name, "", &[], None), Err(NO_GROUP.into()));
            assert_eq!(
                allow(name, "default", &["rel".into()], None),
                Err(RELATIVE_FOLDER.into())
            );
            assert_eq!(
                allow(name, "default", &[], Some("rel.json")),
                Err(RELATIVE_SETTINGS.into())
            );
            // Without the proxy's recorded address no settings file can be written, and
            // nothing is listed.
            assert_eq!(
                allow(name, "default", &[], Some("/tmp/x.json")),
                Err(NO_PROXY.into())
            );
            assert!(users::load(&root).unwrap().is_empty());
        }
        assert_eq!(
            execute(
                &store,
                &root,
                None,
                Some("bogus"),
                None,
                None,
                &[],
                None,
                None
            ),
            Err(UNKNOWN_ACTION.into())
        );
        assert_eq!(
            execute(&store, &root, None, None, None, None, &[], None, None).unwrap(),
            json!({"users": []})
        );
    }

    fn record_proxy(root: &Path, token: &str) {
        switchboard_core::private_fs::private_write(
            &root.join("proxy.json"),
            serde_json::to_vec(&json!({"protocol": 1, "port": 4555, "token": token}))
                .unwrap()
                .as_slice(),
        )
        .unwrap();
    }

    /// SB-421 review (P2-6): allowing a listed user for another group writes the file recorded
    /// before again, so it names the new group.
    #[test]
    fn a_new_group_rewrites_the_recorded_settings_file() {
        let Some(other) = another_account() else {
            return;
        };
        let (dir, store) = store();
        let root = store.root().to_owned();
        record_proxy(&root, &"d".repeat(64));
        let folder = real(&dir).join("team");
        std::fs::create_dir(&folder).unwrap();
        let team = store.save_project(None, "Team", &[folder], &[]).unwrap();
        let settings = real(&dir).join("agent.json");
        let text = settings.display().to_string();
        let allow = |group: &str, path: Option<&str>| {
            execute(
                &store,
                &root,
                None,
                Some("allow"),
                Some(&other.name),
                Some(group),
                &[],
                path,
                None,
            )
        };
        allow("default", Some(&text)).unwrap();
        let read =
            || -> Value { serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap() };
        assert_eq!(
            read()["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:4555/claude/default"
        );
        let moved = allow(&team.pool, None).unwrap();
        assert_eq!(moved["settings_file"]["path"], text.as_str());
        assert_eq!(
            read()["env"]["ANTHROPIC_BASE_URL"],
            format!("http://127.0.0.1:4555/claude/{}", team.pool)
        );
        assert_eq!(
            users::load(&root).unwrap()[0].settings_sha256.as_deref(),
            Some(sha256_hex(&std::fs::read(&settings).unwrap()).as_str())
        );
    }

    /// SB-421 review (P2-4): a uid listed under a name that the system now gives another account
    /// is refused for allow and rotate, and its file is not rewritten.
    #[test]
    fn a_reused_uid_is_refused_and_its_file_left_alone() {
        let Some(other) = another_account() else {
            return;
        };
        let (dir, store) = store();
        let root = store.root().to_owned();
        record_proxy(&root, &"d".repeat(64));
        let settings = real(&dir).join("agent.json");
        users::save(
            &root,
            &[NamedUser {
                uid: other.uid,
                name: Some("gone-account-sb421".into()),
                pool: "default".into(),
                folders: Vec::new(),
                salt: users::new_salt(),
                added_at: 1,
                added_by: "cli".into(),
                settings: Some(settings.display().to_string()),
                settings_sha256: None,
            }],
        )
        .unwrap();
        let run = |action: &str| {
            execute(
                &store,
                &root,
                None,
                Some(action),
                Some(&other.uid.to_string()),
                Some("default"),
                &[],
                None,
                None,
            )
        };
        assert_eq!(run("allow"), Err(UID_REUSED.into()));
        assert_eq!(run("rotate"), Err(UID_REUSED.into()));
        assert_eq!(repoint(&root, 4777, &"e".repeat(64)), (0, 1));
        assert!(!settings.exists());
        assert!(
            run("disallow").is_ok(),
            "a reused uid can always be taken off"
        );
    }
}
