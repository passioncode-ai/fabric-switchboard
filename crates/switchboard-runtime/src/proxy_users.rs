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
pub const SELF_USER: &str = "That is the account Switchboard runs as: its sessions are served already.";
pub const ROOT_USER: &str = "Switchboard never serves the root account's processes.";
pub const NO_GROUP: &str = "Name the group whose accounts this user may use.";
pub const UNKNOWN_GROUP: &str = "No group has that id or name.";
pub const NOT_LISTED: &str = "That account is not on the list of users the proxy serves.";
pub const RELATIVE_FOLDER: &str = "Give each folder as an absolute path.";
pub const RELATIVE_SETTINGS: &str = "Give the settings file as an absolute path.";
pub const TOO_MANY: &str = "The proxy serves at most 256 other users.";
pub const NO_PROXY: &str = "Switchboard's proxy has not started on this computer yet, so the settings file cannot name its address. Start Switchboard (switchboard serve), then retry.";
pub const SETTINGS_PARENT: &str = "The settings file's folder does not exist. Create it, then retry.";
pub const SETTINGS_NOT_OURS: &str = "A file Switchboard did not write is at that path (a link, a folder or another account's file). Remove it or name another path.";
pub const SETTINGS_UNWRITTEN: &str = "The settings file could not be written there. Check the folder's permissions, then retry.";
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
            libc::getpwnam_r(name.as_ptr(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found)
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
    /// 0o640 when the file's group is the account's own, else 0o600.
    pub mode: u32,
    pub shared_with_group: bool,
}

/// Writes `bytes` at `path` as this account's file: a new file renamed over the old one, mode
/// 0640 with the group `gid` when this account may set it (a member of that group), else 0600.
/// A link, a folder or another account's file at `path` is never replaced.
#[cfg(unix)]
pub fn write_settings(path: &Path, bytes: &[u8], gid: u32) -> Result<Written, String> {
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    if !path.is_absolute() {
        return Err(RELATIVE_SETTINGS.into());
    }
    let parent = path.parent().ok_or(SETTINGS_PARENT)?;
    if !parent.is_dir() {
        return Err(SETTINGS_PARENT.into());
    }
    let me = own_uid().ok_or(SETTINGS_UNWRITTEN)?;
    if let Ok(m) = std::fs::symlink_metadata(path) {
        if m.file_type().is_symlink() || !m.is_file() || m.uid() != me || m.nlink() != 1 {
            return Err(SETTINGS_NOT_OURS.into());
        }
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(SETTINGS_UNWRITTEN)?;
    let temporary = parent.join(format!(".{name}.switchboard-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)
            .map_err(|_| SETTINGS_UNWRITTEN.to_string())?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| SETTINGS_UNWRITTEN.to_string())?;
        // SAFETY: the descriptor is this function's open file; -1 leaves the owner unchanged.
        let grouped = unsafe {
            libc::fchown(
                std::os::fd::AsRawFd::as_raw_fd(&f),
                u32::MAX as libc::uid_t,
                gid as libc::gid_t,
            )
        } == 0;
        let mode = if grouped { 0o640 } else { 0o600 };
        f.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|_| SETTINGS_UNWRITTEN.to_string())?;
        drop(f);
        std::fs::rename(&temporary, path).map_err(|_| SETTINGS_UNWRITTEN.to_string())?;
        Ok(Written {
            mode,
            shared_with_group: grouped,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
#[cfg(not(unix))]
pub fn write_settings(_path: &Path, _bytes: &[u8], _gid: u32) -> Result<Written, String> {
    Err(UNSUPPORTED.into())
}

/// The proxy's port and session token: the running owner's, else the recorded ones.
fn proxy_address(runtime: Option<&crate::Runtime>, root: &Path) -> Option<(u16, String)> {
    match runtime {
        Some(r) => Some((r.proxy.address().port(), r.proxy.token().to_owned())),
        None => crate::load_proxy_record(root).map(|r| (r.port, r.token)),
    }
}

/// Writes `user`'s settings file at `path` and answers what was done, never the capability.
fn deliver(
    user: &NamedUser,
    account: Option<&Account>,
    path: &Path,
    proxy: Option<&(u16, String)>,
) -> Result<Value, String> {
    let (port, token) = proxy.ok_or(NO_PROXY)?;
    let capability = users::user_token(token, &user.pool, user.uid, &user.salt);
    let gid = account.map_or(u32::MAX, |a| a.gid);
    let written = write_settings(path, &settings_bytes(*port, &user.pool, &capability), gid)?;
    let shown = path.display().to_string();
    let mut answer = json!({
        "path": shown,
        "mode": format!("{:04o}", written.mode),
        "readable_by_group": written.shared_with_group,
    });
    if !written.shared_with_group {
        // This account may not give the file to the user's group: a person with sudo copies it
        // into the user's own folder, readable by that user only.
        let name = account.map_or_else(|| user.uid.to_string(), |a| a.name.clone());
        let destination = match account {
            Some(a) if !a.home.is_empty() => format!("{}/.claude/switchboard-settings.json", a.home),
            _ => format!("{shown}.{name}"),
        };
        // GNU install makes the folder too (-D); macOS's install has no such flag.
        let make_folder = if cfg!(target_os = "linux") { " -D" } else { "" };
        answer["install_command"] = json!(format!(
            "sudo install{make_folder} -o {} -m 0600 {} {}",
            sh_quote(&name),
            sh_quote(&shown),
            sh_quote(&destination)
        ));
        answer["installed_path"] = json!(destination);
    }
    Ok(answer)
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
    Ok(if trimmed.is_empty() { "/".into() } else { trimmed.into() })
}

/// The listed entry `who` names: by the password database first, else by the uid or the name
/// recorded (an account removed from the computer can still be taken off the list).
fn listed_index(list: &[NamedUser], who: &str) -> Option<usize> {
    let uid = account(who)
        .map(|a| a.uid)
        .or_else(|| who.trim().parse().ok());
    list.iter()
        .position(|u| Some(u.uid) == uid)
        .or_else(|| list.iter().position(|u| u.name.as_deref() == Some(who.trim())))
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
            "users": list.iter().map(|u| view(u, store)).collect::<Vec<_>>(),
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
            let entry = NamedUser {
                uid: account.uid,
                name: Some(account.name.clone()),
                pool: pool.clone(),
                folders,
                salt: existing
                    .map(|i| list[i].salt.clone())
                    .unwrap_or_else(users::new_salt),
                added_at: crate::monitor::now(),
                added_by: surface(by),
                settings: settings
                    .clone()
                    .or_else(|| existing.and_then(|i| list[i].settings.clone())),
            };
            match existing {
                Some(i) => list[i] = entry.clone(),
                None if list.len() >= users::MAX_USERS => return Err(TOO_MANY.into()),
                None => list.push(entry.clone()),
            }
            // The file first: a settings file the proxy would refuse is never left behind.
            let delivered = match &settings {
                Some(path) => Some(deliver(
                    &entry,
                    Some(&account),
                    Path::new(path),
                    proxy_address(runtime, root).as_ref(),
                )?),
                None => None,
            };
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
            if let Some(p) = write_to {
                let path = Path::new(p.trim());
                if !path.is_absolute() {
                    return Err(RELATIVE_SETTINGS.into());
                }
                list[index].settings = Some(path.display().to_string());
            }
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
            let delivered = match &entry.settings {
                Some(path) => Some(deliver(
                    &entry,
                    account(&entry.uid.to_string()).as_ref(),
                    Path::new(path),
                    proxy_address(runtime, root).as_ref(),
                )),
                None => None,
            };
            let (settings_file, settings_error) = match delivered {
                Some(Ok(v)) => (Some(v), None),
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
    let Ok(list) = users::load(root) else {
        return (0, 0);
    };
    let proxy = (port, token.to_owned());
    let (mut written, mut failed) = (0, 0);
    for user in list.iter().filter(|u| u.settings.is_some()) {
        let path = user.settings.as_deref().unwrap_or_default();
        match deliver(
            user,
            account(&user.uid.to_string()).as_ref(),
            Path::new(path),
            Some(&proxy),
        ) {
            Ok(_) => written += 1,
            Err(_) => failed += 1,
        }
    }
    (written, failed)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use switchboard_core::MemoryVault;

    fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(
            Store::open(dir.path().join("data"), std::sync::Arc::new(MemoryVault::default()))
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

    #[test]
    fn a_settings_file_holds_the_address_and_is_never_world_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-a.json");
        let bytes = settings_bytes(4321, "team", "c".repeat(64).as_str());
        // A group this account cannot give the file to: 0600, and the answer says so.
        let refused = write_settings(&path, &bytes, 0).unwrap();
        assert_eq!(refused.mode, 0o600);
        assert!(!refused.shared_with_group);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            v["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:4321/claude/team"
        );
        // This account's own primary group: 0640.
        let gid = account(&own_uid().unwrap().to_string()).unwrap().gid;
        let shared = write_settings(&path, &bytes, gid).unwrap();
        assert_eq!(shared.mode, 0o640);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        // A link at the path is never followed or replaced.
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(write_settings(&link, &bytes, gid), Err(SETTINGS_NOT_OURS.into()));
        assert_eq!(
            write_settings(Path::new("relative.json"), &bytes, gid),
            Err(RELATIVE_SETTINGS.into())
        );
        assert_eq!(
            write_settings(&dir.path().join("missing/x.json"), &bytes, gid),
            Err(SETTINGS_PARENT.into())
        );
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
            serde_json::to_vec(&json!({"protocol": 1, "port": 4555, "token": token})).unwrap().as_slice(),
        )
        .unwrap();
        let settings = dir.path().join("agent.json");
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
        let mode = std::fs::metadata(root.join(users::FILE)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let written: Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(written["env"]["ANTHROPIC_AUTH_TOKEN"], capability.as_str());
        if answer["settings_file"]["readable_by_group"] == false {
            assert!(answer["settings_file"]["install_command"]
                .as_str()
                .unwrap()
                .starts_with("sudo install"));
        }
        // Rotation: a new capability, written into the recorded file.
        let rotated = execute(&store, &root, None, Some("rotate"), Some(&other.uid.to_string()), None, &[], None, Some("cli")).unwrap();
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
        assert_eq!(written["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:4777/claude/default");
        // Disallowed: off the list, and the file goes when it is empty.
        let removed = execute(&store, &root, None, Some("disallow"), Some(&other.name), None, &[], None, None).unwrap();
        assert_eq!(removed["removed"]["uid"], other.uid);
        assert!(!root.join(users::FILE).exists());
        assert_eq!(
            execute(&store, &root, None, Some("disallow"), Some(&other.name), None, &[], None, None),
            Err(NOT_LISTED.into())
        );
    }

    #[test]
    fn this_account_root_unknown_users_and_groups_and_relative_paths_are_refused() {
        let (_dir, store) = store();
        let root = store.root().to_owned();
        let me = own_uid().unwrap().to_string();
        let allow = |who: &str, group: &str, folders: &[String], settings: Option<&str>| {
            execute(&store, &root, None, Some("allow"), Some(who), Some(group), folders, settings, None)
        };
        assert_eq!(allow(&me, "default", &[], None), Err(SELF_USER.into()));
        if account("0").is_some() && own_uid() != Some(0) {
            assert_eq!(allow("0", "default", &[], None), Err(ROOT_USER.into()));
        }
        assert_eq!(allow("no-such-account-sb421", "default", &[], None), Err(UNKNOWN_USER.into()));
        if let Some(other) = another_account() {
            let name = other.name.as_str();
            assert_eq!(allow(name, "no-such-group", &[], None), Err(UNKNOWN_GROUP.into()));
            assert_eq!(allow(name, "", &[], None), Err(NO_GROUP.into()));
            assert_eq!(allow(name, "default", &["rel".into()], None), Err(RELATIVE_FOLDER.into()));
            assert_eq!(allow(name, "default", &[], Some("rel.json")), Err(RELATIVE_SETTINGS.into()));
            // Without the proxy's recorded address no settings file can be written, and
            // nothing is listed.
            assert_eq!(allow(name, "default", &[], Some("/tmp/x.json")), Err(NO_PROXY.into()));
            assert!(users::load(&root).unwrap().is_empty());
        }
        assert_eq!(
            execute(&store, &root, None, Some("bogus"), None, None, &[], None, None),
            Err(UNKNOWN_ACTION.into())
        );
        assert_eq!(
            execute(&store, &root, None, None, None, None, &[], None, None).unwrap(),
            json!({"users": []})
        );
    }
}
