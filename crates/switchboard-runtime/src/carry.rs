//! SB-137 (Fabric board finding 2087ac21, 2026-10-16) and SB-130: a session Switchboard launches
//! in a home of its own (`CLAUDE_CONFIG_DIR`, Pi's `PI_CODING_AGENT_DIR`) lost the operator's
//! global instructions, settings, hooks, plugins and skills, and a Claude Code continuation sat
//! on the first-run and folder-trust dialogs nobody answers in a console. The operator's own
//! files are carried over as links, so they stay current; settings are merged in without anything
//! that signs in or looks like a secret. Nothing here reads or writes a credential.
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// What a Claude Code home links from the operator's (`~/.claude`, or their `CLAUDE_CONFIG_DIR`).
pub(crate) const CLAUDE_LINKS: &[&str] = &[
    "CLAUDE.md",
    "skills",
    "commands",
    "agents",
    "output-styles",
    "plugins",
    "hooks",
];
/// What a Pi home links from the operator's (`~/.pi/agent`).
pub(crate) const PI_LINKS: &[&str] = &["AGENTS.md", "skills", "extensions", "prompts", "themes"];
/// Settings that sign in or run a helper that hands out credentials: never carried.
const CLAUDE_AUTH_KEYS: &[&str] = &[
    "env",
    "apiKeyHelper",
    "awsAuthRefresh",
    "awsCredentialExport",
    "otelHeadersHelper",
    "forceLoginMethod",
    "forceLoginOrgUUID",
];

/// The operator's Claude Code user folder, as their own sessions see it (never one of
/// Switchboard's homes, SB-183).
pub(crate) fn claude_user_dir() -> Option<PathBuf> {
    claude_user_dir_from(&crate::person::Sources::process())
}
pub(crate) fn claude_user_dir_from(sources: &crate::person::Sources) -> Option<PathBuf> {
    crate::person::claude_dir_from(sources)
}
/// The operator's Pi folder (`PI_CODING_AGENT_DIR`, else `~/.pi/agent`), never one of
/// Switchboard's homes (SB-183).
pub(crate) fn pi_user_dir() -> Option<PathBuf> {
    pi_user_dir_from(&crate::person::Sources::process())
}
pub(crate) fn pi_user_dir_from(sources: &crate::person::Sources) -> Option<PathBuf> {
    crate::person::pi_dir_from(sources)
}

/// Links each of `names` that exists in `source` into `home`. A link Switchboard made before is
/// renewed; a real file or folder in `home` is never replaced. Returns how many links point there.
pub(crate) fn link_into(home: &Path, source: &Path, names: &[&str]) -> usize {
    if same_dir(home, source) {
        return 0;
    }
    let mut linked = 0;
    for name in names {
        let from = source.join(name);
        if std::fs::symlink_metadata(&from).is_err() {
            continue;
        }
        let to = home.join(name);
        match std::fs::symlink_metadata(&to) {
            Ok(m) if m.file_type().is_symlink() => {
                if std::fs::read_link(&to).ok().as_deref() == Some(from.as_path()) {
                    linked += 1;
                    continue;
                }
                if std::fs::remove_file(&to).is_err() {
                    continue;
                }
            }
            Ok(_) => continue,
            Err(_) => {}
        }
        if symlink(&from, &to).is_ok() {
            linked += 1;
        }
    }
    linked
}

/// [`link_into`] for an organization's shell home (SB-226). Elsewhere exactly [`link_into`]. On
/// Windows, where a symbolic link needs a privilege a person seldom holds, a folder is linked by
/// a directory junction (any local drive) and a file by a hard link (same volume only), neither
/// of which needs it or an administrator: a junction always shows the folder as it is now; a hard
/// link is the same file, so an edit in place is seen at once. Each hard link is recorded in
/// `<home>/.switchboard-links` with the file's identity (volume serial and file index), and only a
/// home file that still has that identity is Switchboard's: it is linked again when the person's
/// file was replaced (an editor renames a new file over the old one; the homes' fingerprint
/// starts the pass when the file's time moves) and removed when the person's file is gone. A home
/// file with another identity (a real file, or one a session replaced by rename) is never replaced
/// or removed, and its name leaves the record.
pub(crate) fn link_into_shell_home(home: &Path, source: &Path, names: &[&str]) -> usize {
    #[cfg(windows)]
    return windows_links::link_into(home, source, names);
    #[cfg(not(windows))]
    link_into(home, source, names)
}

/// The person's file under one name, as a shell home's hard-link pass sees it (SB-226).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source<Id> {
    /// Not there (deleted, or never made).
    Missing,
    /// A file, with its identity.
    File(Id),
}
/// What a shell home holds under that name.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomeItem<Id> {
    Missing,
    /// A symbolic link, with the identity of the file it leads to (None: it leads nowhere).
    Symlink(Option<Id>),
    /// A file, with its identity.
    File(Id),
    /// A folder, or a file whose identity cannot be read now.
    Other,
}
/// What the pass does with one name.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The home's file is the person's file: counted, recorded with its identity.
    Same,
    /// A symbolic link to the person's file: counted, the record left as it is.
    SameLink,
    /// Nothing in the home: hard-link the person's file and record it.
    Link,
    /// Switchboard's link to a file the person replaced, or a symbolic link elsewhere: removed
    /// and linked again.
    Relink,
    /// Switchboard's link to a file the person deleted: removed, the name dropped.
    Remove,
    /// Not Switchboard's (a real file, a folder, a file a session replaced): kept on disk, the
    /// name dropped from the record.
    Leave,
    /// Nothing to do now; the record left as it is.
    Wait,
}

/// The hard-link pass's decision for one name (SB-226). `recorded` is the identity the record
/// keeps for the name: None when it does not name it, `Some(None)` for a line written without
/// one. Only a home file whose identity is the recorded one is Switchboard's to replace or remove.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn file_step<Id: PartialEq>(
    source: &Source<Id>,
    home: &HomeItem<Id>,
    recorded: Option<Option<&Id>>,
) -> Step {
    let ours = |id: &Id| matches!(recorded, Some(Some(r)) if r == id);
    match (source, home) {
        (Source::File(s), HomeItem::File(h)) if h == s => Step::Same,
        (Source::File(s), HomeItem::Symlink(Some(h))) if h == s => Step::SameLink,
        (Source::File(_), HomeItem::Missing) => Step::Link,
        (Source::File(_), HomeItem::Symlink(_)) => Step::Relink,
        (Source::File(_), HomeItem::File(h)) if ours(h) => Step::Relink,
        (Source::Missing, HomeItem::File(h)) if ours(h) => Step::Remove,
        (_, HomeItem::File(_)) => Step::Leave,
        (Source::Missing, HomeItem::Missing) if recorded.is_some() => Step::Leave,
        (Source::File(_), HomeItem::Other) if recorded.is_some() => Step::Wait,
        (Source::File(_), HomeItem::Other) => Step::Leave,
        (Source::Missing, _) => Step::Wait,
    }
}

/// The text of a junction's target for a local folder: `X:\a\b`, with `/` read as `\`, empty and
/// `.` steps dropped and `..` taken back. None for anything a junction cannot name (a share, a
/// relative path, a `..` above the drive): no junction is made then, never a broken one.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn junction_target(text: &str) -> Option<String> {
    let text = text.replace('/', "\\");
    let bytes = text.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
        return None;
    }
    let mut steps: Vec<&str> = Vec::new();
    for step in text[3..].split('\\') {
        match step {
            "" | "." => {}
            ".." => {
                steps.pop()?;
            }
            step => steps.push(step),
        }
    }
    Some(format!("{}\\{}", &text[..2], steps.join("\\")))
}

/// The record of a shell home's hard links: one `name` per line, then a tab and the linked file's
/// identity as `volume:index` in hexadecimal. A line without one (an earlier record) names the
/// file without its identity.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_record(text: &str) -> std::collections::BTreeMap<String, Option<(u32, u64)>> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once('\t') {
            Some((name, id)) => {
                let id = id.split_once(':').and_then(|(volume, index)| {
                    Some((
                        u32::from_str_radix(volume, 16).ok()?,
                        u64::from_str_radix(index, 16).ok()?,
                    ))
                });
                (name.to_owned(), id)
            }
            None => (line.to_owned(), None),
        })
        .collect()
}
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn render_record(
    record: &std::collections::BTreeMap<String, Option<(u32, u64)>>,
) -> String {
    record
        .iter()
        .map(|(name, id)| match id {
            Some((volume, index)) => format!("{name}\t{volume:x}:{index:x}\n"),
            None => format!("{name}\n"),
        })
        .collect()
}

#[cfg(windows)]
mod windows_links {
    use super::{file_step, HomeItem, Source, Step};
    use std::collections::BTreeMap;
    use std::path::Path;

    /// The names a shell home holds as hard links Switchboard made, with each file's identity.
    pub(super) const RECORD: &str = ".switchboard-links";

    type Id = (u32, u64);

    pub(super) fn link_into(home: &Path, source: &Path, names: &[&str]) -> usize {
        if super::same_dir(home, source) {
            return 0;
        }
        let record_path = home.join(RECORD);
        let before = std::fs::read_to_string(&record_path)
            .map(|text| super::parse_record(&text))
            .unwrap_or_default();
        let mut record = before.clone();
        let mut linked = 0;
        for name in names {
            let from = source.join(name);
            let to = home.join(name);
            // The person's item as it is now, through any link of theirs.
            let meta = match std::fs::metadata(&from) {
                Ok(meta) => Some(meta),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                // Unreadable now: nothing changes.
                Err(_) => continue,
            };
            let done = match meta {
                Some(meta) if meta.is_dir() => folder(&from, &to),
                Some(_) => match file_id(&from) {
                    Some(id) => file(Source::File(id), &from, &to, name, &mut record),
                    None => false,
                },
                None => file(Source::Missing, &from, &to, name, &mut record),
            };
            if done {
                linked += 1;
            }
        }
        if record != before {
            // Kept as the next pass's guide only: a record that cannot be written leaves the
            // links, and a hard link it does not name stays as it is.
            let text = super::render_record(&record);
            let _ = crate::launch::private_write(&record_path, text.as_bytes(), false);
        }
        linked
    }

    /// A folder: a junction to `from`, renewed when it leads elsewhere.
    fn folder(from: &Path, to: &Path) -> bool {
        match std::fs::symlink_metadata(to) {
            // A junction or a link: ours to renew.
            Ok(m) if m.file_type().is_symlink() => {
                // Compared as the junction names it, so a `/` or `..` in the person's path does
                // not renew it at every pass.
                let wanted = crate::shell::install::plain_path(from)
                    .to_str()
                    .and_then(super::junction_target);
                if std::fs::read_link(to).is_ok_and(|target| {
                    wanted
                        .as_deref()
                        .is_some_and(|w| same_path(&target, Path::new(w)))
                }) {
                    return true;
                }
                if std::fs::remove_dir(to).is_err() && std::fs::remove_file(to).is_err() {
                    return false;
                }
            }
            Ok(_) => return false,
            Err(_) => {}
        }
        junction(from, to).is_ok()
    }

    /// A file, by [`file_step`]: a hard link to `from`, renewed or removed only while the home's
    /// file is still the one the record names.
    fn file(
        source: Source<Id>,
        from: &Path,
        to: &Path,
        name: &str,
        record: &mut BTreeMap<String, Option<Id>>,
    ) -> bool {
        let home = match std::fs::symlink_metadata(to) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HomeItem::Missing,
            Err(_) => HomeItem::Other,
            Ok(m) if m.file_type().is_symlink() => HomeItem::Symlink(file_id(to)),
            Ok(m) if m.is_file() => file_id(to).map_or(HomeItem::Other, HomeItem::File),
            Ok(_) => HomeItem::Other,
        };
        let recorded = record.get(name).map(Option::as_ref);
        match file_step(&source, &home, recorded) {
            Step::Same => {
                if let Source::File(id) = source {
                    record.insert(name.to_owned(), Some(id));
                }
                true
            }
            Step::SameLink => true,
            Step::Link => link(source, from, to, name, record),
            Step::Relink => {
                std::fs::remove_file(to).is_ok() && link(source, from, to, name, record)
            }
            Step::Remove => {
                if std::fs::remove_file(to).is_ok() {
                    record.remove(name);
                }
                false
            }
            Step::Leave => {
                record.remove(name);
                false
            }
            Step::Wait => false,
        }
    }

    fn link(
        source: Source<Id>,
        from: &Path,
        to: &Path,
        name: &str,
        record: &mut BTreeMap<String, Option<Id>>,
    ) -> bool {
        if std::fs::hard_link(from, to).is_ok() {
            if let Source::File(id) = source {
                record.insert(name.to_owned(), Some(id));
            }
            true
        } else {
            // Another volume, or a file system without hard links: nothing carried.
            record.remove(name);
            false
        }
    }

    /// Whether two paths name the same place as Windows compares them (`\\?\` dropped, case
    /// folded).
    fn same_path(a: &Path, b: &Path) -> bool {
        let key = |p: &Path| crate::shell::map::windows_key(&p.to_string_lossy());
        key(a).is_some_and(|k| Some(k) == key(b))
    }

    /// The volume and file index of a file: equal for two names of one file.
    pub(super) fn file_id(path: &Path) -> Option<Id> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: a zeroed BY_HANDLE_FILE_INFORMATION is a valid value of plain integers.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the handle is open for the call and `info` lives across it.
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
        (ok != 0).then(|| {
            (
                info.dwVolumeSerialNumber,
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            )
        })
    }

    /// Creates `link`, a directory junction to the local folder `target` (a mount point reparse
    /// point, as `mklink /J` makes): no privilege needed. Not for a network share, which a
    /// junction cannot name ([`super::junction_target`]).
    pub(super) fn junction(target: &Path, link: &Path) -> std::io::Result<()> {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;
        const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
        const MAX_DATA: usize = 16 * 1024 - 8;
        let unsupported = || std::io::Error::other("a junction names a local folder only");
        let plain = crate::shell::install::plain_path(target);
        // A `/` in the person's path would make a junction that leads nowhere: the target is
        // written as Windows names it, or no junction is made.
        let text = plain
            .to_str()
            .and_then(super::junction_target)
            .ok_or_else(unsupported)?;
        let substitute: Vec<u16> = format!(r"\??\{text}").encode_utf16().collect();
        let print: Vec<u16> = text.encode_utf16().collect();
        let data = 8 + (substitute.len() + 1 + print.len() + 1) * 2;
        if data > MAX_DATA {
            return Err(unsupported());
        }
        let mut buffer: Vec<u8> = Vec::with_capacity(8 + data);
        buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        for value in [
            data,
            0,
            0,
            substitute.len() * 2,
            (substitute.len() + 1) * 2,
            print.len() * 2,
        ] {
            buffer.extend((value as u16).to_le_bytes());
        }
        for unit in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
            buffer.extend(unit.to_le_bytes());
        }
        std::fs::create_dir(link)?;
        let made = (|| {
            let dir = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(link)?;
            let mut returned = 0u32;
            // SAFETY: the handle is open for the call; the buffer holds `buffer.len()` bytes and
            // no output is asked for.
            let ok = unsafe {
                DeviceIoControl(
                    dir.as_raw_handle() as _,
                    FSCTL_SET_REPARSE_POINT,
                    buffer.as_ptr().cast(),
                    buffer.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })();
        if made.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        made
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}
/// Windows needs a privilege for symbolic links; nothing is linked there yet.
#[cfg(not(unix))]
fn symlink(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("links are not made on this platform"))
}

/// A key that names a secret by its shape (`apiKey`, `token`, `password`, …).
fn secret_shaped(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "key",
        "token",
        "secret",
        "password",
        "auth",
        "credential",
        "cookie",
    ]
    .iter()
    .any(|w| k.contains(w))
}

/// The operator's settings without what signs in or looks like a secret (top-level keys only;
/// `hooks`, `permissions`, `enabledPlugins` and the like are kept whole). Absent, unreadable or
/// not an object: nothing.
pub(crate) fn carried_settings(path: &Path, drop: &[&str]) -> Map<String, Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Map::new();
    };
    let Ok(Value::Object(mut settings)) = serde_json::from_str::<Value>(&text) else {
        return Map::new();
    };
    settings.retain(|k, _| !drop.contains(&k.as_str()) && !secret_shaped(k));
    settings
}

/// A Claude Code home's `settings.json`: the operator's carried settings, then this launch's
/// own `env` (the isolated account's credential, or nothing in managed mode) on top.
pub(crate) fn claude_settings(user: Option<&Path>, env: Option<Value>) -> Value {
    let mut settings = user
        .map(|u| carried_settings(&u.join("settings.json"), CLAUDE_AUTH_KEYS))
        .unwrap_or_default();
    if let Some(env) = env {
        settings.insert("env".into(), env);
    }
    Value::Object(settings)
}

/// A Pi home's `settings.json`: the operator's, without what signs in or looks like a secret.
pub(crate) fn pi_settings(user: &Path) -> Option<Value> {
    let settings = carried_settings(&user.join("settings.json"), &[]);
    (!settings.is_empty()).then_some(Value::Object(settings))
}

/// Marks a Claude Code home's `.claude.json` as past the first-run screens and, when `trust` names
/// a folder, that folder as trusted (SB-130: a continuation works in the folder its previous agent
/// worked in; operator rule 2026-10-17, decided 2026-10-19). A file that is not a JSON object is
/// left as it is. Returns whether it was written.
pub(crate) fn claude_state(home: &Path, trust: Option<&Path>) -> Result<bool, String> {
    let path = home.join(".claude.json");
    let mut state = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => m,
            _ => return Ok(false),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(_) => return Ok(false),
    };
    state.insert("hasCompletedOnboarding".into(), Value::Bool(true));
    if let Some(folder) = trust {
        let projects = state
            .entry("projects")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(projects) = projects {
            let entry = projects
                .entry(folder.to_string_lossy().into_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(entry) = entry {
                entry.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
            }
        }
    }
    // A plain JSON object always serialises; an empty result would mean nothing to write.
    let bytes = serde_json::to_vec(&Value::Object(state)).unwrap_or_default();
    if bytes.is_empty() {
        return Ok(false);
    }
    crate::launch::private_write(&path, &bytes, false)?;
    Ok(true)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_operators_files_are_linked_and_a_real_file_in_the_home_is_kept() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("CLAUDE.md"), "rules").unwrap();
        std::fs::create_dir(user.path().join("skills")).unwrap();
        std::fs::create_dir(user.path().join("commands")).unwrap();
        std::fs::write(home.path().join("commands"), "the home's own").unwrap();
        assert_eq!(link_into(home.path(), user.path(), CLAUDE_LINKS), 2);
        assert_eq!(
            std::fs::read_to_string(home.path().join("CLAUDE.md")).unwrap(),
            "rules",
            "linked, so it stays current"
        );
        assert!(home.path().join("skills").is_dir());
        assert_eq!(
            std::fs::read_to_string(home.path().join("commands")).unwrap(),
            "the home's own"
        );
        // Again: nothing changes, nothing breaks.
        assert_eq!(link_into(home.path(), user.path(), CLAUDE_LINKS), 2);
        // A link to an older place is renewed.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("CLAUDE.md"), "new").unwrap();
        link_into(home.path(), elsewhere.path(), CLAUDE_LINKS);
        assert_eq!(
            std::fs::read_to_string(home.path().join("CLAUDE.md")).unwrap(),
            "new"
        );
        // The operator's own folder is never linked into itself.
        assert_eq!(link_into(user.path(), user.path(), CLAUDE_LINKS), 0);
    }

    #[test]
    fn settings_keep_hooks_and_permissions_and_drop_what_signs_in() {
        let user = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("settings.json"),
            json!({
                "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "/x"}]}]},
                "permissions": {"deny": ["Bash(rm -rf /)"]},
                "enabledPlugins": {"p@m": true},
                "env": {"ANTHROPIC_API_KEY": "synthetic-not-a-key"},
                "apiKeyHelper": "/bin/helper",
                "someToken": "synthetic",
                "model": "opus"
            })
            .to_string(),
        )
        .unwrap();
        let settings = claude_settings(
            Some(user.path()),
            Some(json!({"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-launch"})),
        );
        let keys: Vec<&String> = settings.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["enabledPlugins", "env", "hooks", "model", "permissions"]
        );
        assert_eq!(
            settings["env"],
            json!({"CLAUDE_CODE_OAUTH_TOKEN": "synthetic-launch"}),
            "the launch's own env only, never the operator's"
        );
        // Managed mode: no env at all; an absent or broken file carries nothing.
        assert!(claude_settings(Some(user.path()), None)
            .get("env")
            .is_none());
        std::fs::write(user.path().join("settings.json"), "{broken").unwrap();
        assert_eq!(claude_settings(Some(user.path()), None), json!({}));
        assert_eq!(claude_settings(None, None), json!({}));
    }

    #[test]
    fn a_continuation_trusts_its_own_folder_and_nothing_else() {
        let home = tempfile::tempdir().unwrap();
        assert!(claude_state(home.path(), None).unwrap());
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert!(
            state.get("projects").is_none(),
            "an ordinary launch keeps the trust dialog"
        );
        std::fs::write(
            home.path().join(".claude.json"),
            json!({"userID": "u", "projects": {"/w/other": {"x": 1}}}).to_string(),
        )
        .unwrap();
        assert!(claude_state(home.path(), Some(Path::new("/w/app"))).unwrap());
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["userID"], "u", "what Claude Code wrote is kept");
        assert_eq!(state["projects"]["/w/app"]["hasTrustDialogAccepted"], true);
        assert_eq!(state["projects"]["/w/other"], json!({"x": 1}));
        std::fs::write(home.path().join(".claude.json"), "[1]").unwrap();
        assert!(!claude_state(home.path(), Some(Path::new("/w/app"))).unwrap());
        assert_eq!(
            std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
            "[1]",
            "not an object: left as it is"
        );
    }

    #[test]
    fn a_launch_from_a_hooked_terminal_links_from_the_persons_folder() {
        // SB-183: a terminal with the shell hook exports the group's shell home as
        // CLAUDE_CONFIG_DIR. A launch (or `serve`) started there must still carry the person's
        // own CLAUDE.md, skills and hooks, never the shell home's.
        let person = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let own = person.path().join(".claude");
        std::fs::create_dir_all(own.join("skills")).unwrap();
        std::fs::write(own.join("CLAUDE.md"), "the person's rules").unwrap();
        let shell_home = data.path().join("shell").join("claude-acme");
        std::fs::create_dir_all(&shell_home).unwrap();
        std::fs::write(shell_home.join("CLAUDE.md"), "the shell home's").unwrap();
        let env =
            |name: &str| (name == "CLAUDE_CONFIG_DIR").then(|| shell_home.clone().into_os_string());
        let sources = crate::person::Sources {
            env: &env,
            home: Some(person.path().to_owned()),
            data: Some(data.path().to_owned()),
        };
        let user = claude_user_dir_from(&sources).unwrap();
        assert_eq!(user, own);
        let launched = tempfile::tempdir().unwrap();
        assert_eq!(link_into(launched.path(), &user, CLAUDE_LINKS), 2);
        assert_eq!(
            std::fs::read_to_string(launched.path().join("CLAUDE.md")).unwrap(),
            "the person's rules"
        );
    }

    #[test]
    fn a_pi_launch_from_a_switchboard_home_links_from_the_persons_pi_folder() {
        // SB-183: a Pi session Switchboard launched exports its own home as
        // PI_CODING_AGENT_DIR; a launch started inside it carries the person's `~/.pi/agent`,
        // never that home. The person's own value elsewhere is kept.
        let person = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let launched = data.path().join("agent-home").join("pi");
        std::fs::create_dir_all(&launched).unwrap();
        let env =
            |name: &str| (name == "PI_CODING_AGENT_DIR").then(|| launched.clone().into_os_string());
        let sources = crate::person::Sources {
            env: &env,
            home: Some(person.path().to_owned()),
            data: Some(data.path().to_owned()),
        };
        assert_eq!(
            pi_user_dir_from(&sources),
            Some(person.path().join(".pi").join("agent"))
        );
        let own = |name: &str| {
            (name == "PI_CODING_AGENT_DIR").then(|| std::ffi::OsString::from("/elsewhere/pi"))
        };
        let sources = crate::person::Sources {
            env: &own,
            home: Some(person.path().to_owned()),
            data: Some(data.path().to_owned()),
        };
        assert_eq!(
            pi_user_dir_from(&sources),
            Some(PathBuf::from("/elsewhere/pi"))
        );
    }

    #[test]
    fn pi_settings_drop_what_looks_like_a_secret() {
        let user = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("settings.json"),
            json!({"skills": ["-x"], "apiKey": "synthetic", "theme": "dark"}).to_string(),
        )
        .unwrap();
        assert_eq!(
            pi_settings(user.path()),
            Some(json!({"skills": ["-x"], "theme": "dark"}))
        );
        assert_eq!(pi_settings(tempfile::tempdir().unwrap().path()), None);
    }

    /// SB-226, the decision behind the Windows hard links (seam B1, B2): only a home file that
    /// still has the identity the record keeps is Switchboard's to relink or remove; one a session
    /// replaced, or the person's own, is kept and leaves the record.
    #[test]
    fn a_shell_homes_hard_link_is_replaced_or_removed_only_while_it_is_still_switchboards() {
        use HomeItem as H;
        use Source as S;
        let ours = Some(Some(&7u64));
        // The person's file is the home's file: kept and recorded, whatever the record said.
        assert_eq!(file_step(&S::File(7), &H::File(7), None), Step::Same);
        assert_eq!(file_step(&S::File(7), &H::File(7), Some(None)), Step::Same);
        assert_eq!(
            file_step(&S::File(7), &H::Symlink(Some(7)), None),
            Step::SameLink
        );
        // Nothing in the home: linked.
        assert_eq!(file_step(&S::File(7), &H::Missing, None), Step::Link);
        assert_eq!(file_step(&S::File(7), &H::Missing, ours), Step::Link);
        // The person's editor replaced their file: the home still holds Switchboard's link to
        // the old one, so it is linked again.
        assert_eq!(file_step(&S::File(8), &H::File(7), ours), Step::Relink);
        // A symbolic link elsewhere is renewed, as on macOS.
        assert_eq!(
            file_step(&S::File(8), &H::Symlink(Some(7)), None),
            Step::Relink
        );
        assert_eq!(
            file_step(&S::File(8), &H::Symlink(None), None),
            Step::Relink
        );
        // B1: the person deleted their file: Switchboard's link goes too, never a stale copy.
        assert_eq!(file_step(&S::Missing, &H::File(7), ours), Step::Remove);
        // B2: a session replaced the home's file by rename (another identity): kept, and the
        // name leaves the record, whether the person's file moved or not.
        assert_eq!(file_step(&S::File(7), &H::File(9), ours), Step::Leave);
        assert_eq!(file_step(&S::File(8), &H::File(9), ours), Step::Leave);
        assert_eq!(file_step(&S::Missing, &H::File(9), ours), Step::Leave);
        // A real file the record never named, or named without an identity: kept.
        assert_eq!(file_step(&S::File(8), &H::File(9), None), Step::Leave);
        assert_eq!(file_step(&S::File(8), &H::File(9), Some(None)), Step::Leave);
        assert_eq!(file_step(&S::Missing, &H::File(9), Some(None)), Step::Leave);
        // Gone on both sides: the name leaves the record; nothing recorded: nothing to do.
        assert_eq!(
            file_step::<u64>(&S::Missing, &H::Missing, ours),
            Step::Leave
        );
        assert_eq!(file_step::<u64>(&S::Missing, &H::Missing, None), Step::Wait);
        // A folder, or a file whose identity cannot be read now: never touched.
        assert_eq!(file_step(&S::File(7), &H::Other, ours), Step::Wait);
        assert_eq!(file_step(&S::Missing, &H::Other, ours), Step::Wait);
        assert_eq!(file_step(&S::File(7), &H::Other, None), Step::Leave);
        assert_eq!(
            file_step(&S::Missing, &H::Symlink(Some(7)), ours),
            Step::Wait
        );
    }

    #[test]
    fn the_hard_link_record_keeps_each_files_identity_and_reads_an_earlier_one() {
        let mut record = std::collections::BTreeMap::new();
        record.insert(
            "CLAUDE.md".to_owned(),
            Some((0xA1B2_C3D4u32, 0x0001_0000_0000_002Au64)),
        );
        record.insert("AGENTS.md".to_owned(), None);
        let text = render_record(&record);
        assert_eq!(text, "AGENTS.md\nCLAUDE.md\ta1b2c3d4:100000000002a\n");
        assert_eq!(parse_record(&text), record);
        // An earlier record (names only) names its files without an identity; a line that is
        // not one reads as a name without one, never as a file to replace.
        let earlier = parse_record("CLAUDE.md\n\nhooks\tnot-an-id\n");
        assert_eq!(earlier.get("CLAUDE.md"), Some(&None));
        assert_eq!(earlier.get("hooks"), Some(&None));
        assert_eq!(earlier.len(), 2);
    }

    /// SB-226 (seam R5): a junction's target is written as Windows names it, or none is made.
    #[test]
    fn a_junction_target_is_written_as_windows_names_it_or_not_at_all() {
        for (given, named) in [
            (
                r"C:\Users\p\.claude\skills",
                Some(r"C:\Users\p\.claude\skills"),
            ),
            (
                "C:/Users/p/.claude/skills",
                Some(r"C:\Users\p\.claude\skills"),
            ),
            (
                r"C:\Users/p\.claude/skills",
                Some(r"C:\Users\p\.claude\skills"),
            ),
            (r"D:\a\\b\.\c\..\skills\", Some(r"D:\a\b\skills")),
            (r"C:\", Some(r"C:\")),
            (r"C:\..\x", None),
            (r"\\server\share\skills", None),
            ("//server/share/skills", None),
            ("relative/skills", None),
            ("C:skills", None),
            ("", None),
        ] {
            assert_eq!(junction_target(given).as_deref(), named, "{given}");
        }
    }
}

/// SB-226: the Windows links of an organization's shell home, made in temporary folders on the
/// Windows runners (no privilege, no administrator).
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn a_shell_home_on_windows_carries_folders_as_junctions_and_files_as_hard_links() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("CLAUDE.md"), "rules").unwrap();
        std::fs::create_dir_all(user.path().join("skills").join("a")).unwrap();
        std::fs::write(
            user.path().join("skills").join("a").join("SKILL.md"),
            "skill",
        )
        .unwrap();
        std::fs::create_dir(user.path().join("commands")).unwrap();
        std::fs::write(home.path().join("commands"), "the home's own").unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            2
        );
        // The folder: a junction that resolves, and shows what is added later.
        let skills = home.path().join("skills");
        assert!(std::fs::symlink_metadata(&skills)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_to_string(skills.join("a").join("SKILL.md")).unwrap(),
            "skill"
        );
        std::fs::write(user.path().join("skills").join("b.md"), "later").unwrap();
        assert_eq!(
            std::fs::read_to_string(skills.join("b.md")).unwrap(),
            "later"
        );
        // The file: the same file, so an edit in place is seen at once.
        let claude = home.path().join("CLAUDE.md");
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "rules");
        std::fs::write(user.path().join("CLAUDE.md"), "edited").unwrap();
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "edited");
        // An editor that replaces the file: the next pass links the new one.
        std::fs::write(user.path().join("CLAUDE.md.new"), "replaced").unwrap();
        std::fs::rename(
            user.path().join("CLAUDE.md.new"),
            user.path().join("CLAUDE.md"),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "edited");
        let record = home.path().join(windows_links::RECORD);
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            2
        );
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "replaced");
        // A pass with nothing new writes nothing.
        let stamp = std::fs::metadata(&record).unwrap().modified().unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            2
        );
        assert_eq!(
            std::fs::metadata(&record).unwrap().modified().unwrap(),
            stamp
        );
        // The home's own file is never replaced.
        assert_eq!(
            std::fs::read_to_string(home.path().join("commands")).unwrap(),
            "the home's own"
        );
        // A junction to another place is renewed; a person's own `CLAUDE.md` is kept.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(elsewhere.path().join("skills")).unwrap();
        std::fs::write(elsewhere.path().join("skills").join("c.md"), "moved").unwrap();
        std::fs::write(elsewhere.path().join("CLAUDE.md"), "other rules").unwrap();
        let other_home = tempfile::tempdir().unwrap();
        std::fs::write(other_home.path().join("CLAUDE.md"), "mine").unwrap();
        assert_eq!(
            link_into_shell_home(other_home.path(), user.path(), CLAUDE_LINKS),
            2,
            "skills and commands; the person's CLAUDE.md stays"
        );
        assert_eq!(
            link_into_shell_home(other_home.path(), elsewhere.path(), CLAUDE_LINKS),
            1
        );
        assert_eq!(
            std::fs::read_to_string(other_home.path().join("skills").join("c.md")).unwrap(),
            "moved"
        );
        assert_eq!(
            std::fs::read_to_string(other_home.path().join("CLAUDE.md")).unwrap(),
            "mine"
        );
        // A hard link Switchboard made follows a new source too.
        link_into_shell_home(home.path(), elsewhere.path(), CLAUDE_LINKS);
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "other rules");
        // Taking the links away never touches the person's files.
        std::fs::remove_dir(home.path().join("skills")).unwrap();
        std::fs::remove_file(&claude).unwrap();
        assert_eq!(
            std::fs::read_to_string(elsewhere.path().join("skills").join("c.md")).unwrap(),
            "moved"
        );
        assert_eq!(
            std::fs::read_to_string(elsewhere.path().join("CLAUDE.md")).unwrap(),
            "other rules"
        );
        // A home holding junctions is removed whole without following them.
        std::fs::remove_dir_all(other_home.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(elsewhere.path().join("skills").join("c.md")).unwrap(),
            "moved"
        );
        // The person's own folder is never linked into itself.
        assert_eq!(
            link_into_shell_home(user.path(), user.path(), CLAUDE_LINKS),
            0
        );
    }

    /// Seam B1: a person's file that is gone takes Switchboard's hard link with it.
    #[test]
    fn a_deleted_source_takes_its_hard_link_out_of_the_shell_home() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("CLAUDE.md"), "rules").unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            1
        );
        let claude = home.path().join("CLAUDE.md");
        let record = home.path().join(windows_links::RECORD);
        assert!(std::fs::read_to_string(&record)
            .unwrap()
            .starts_with("CLAUDE.md\t"));
        std::fs::remove_file(user.path().join("CLAUDE.md")).unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            0
        );
        assert!(!claude.exists(), "no stale copy of a deleted file");
        assert!(!std::fs::read_to_string(&record)
            .unwrap()
            .contains("CLAUDE.md"));
        // Back: linked again.
        std::fs::write(user.path().join("CLAUDE.md"), "new rules").unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            1
        );
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "new rules");
    }

    /// Seam B2: a home file a session replaced by rename is the session's: never replaced or
    /// removed, whatever happens to the person's file.
    #[test]
    fn a_home_file_a_session_replaced_is_kept() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("CLAUDE.md"), "rules").unwrap();
        link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS);
        let claude = home.path().join("CLAUDE.md");
        std::fs::write(home.path().join("CLAUDE.md.tmp"), "the session's edit").unwrap();
        std::fs::rename(home.path().join("CLAUDE.md.tmp"), &claude).unwrap();
        assert_eq!(
            link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS),
            0
        );
        assert_eq!(
            std::fs::read_to_string(&claude).unwrap(),
            "the session's edit"
        );
        let record = home.path().join(windows_links::RECORD);
        assert!(!std::fs::read_to_string(&record)
            .unwrap()
            .contains("CLAUDE.md"));
        // The person's file replaced, then deleted: the session's file stays.
        std::fs::write(user.path().join("CLAUDE.md.new"), "replaced").unwrap();
        std::fs::rename(
            user.path().join("CLAUDE.md.new"),
            user.path().join("CLAUDE.md"),
        )
        .unwrap();
        link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS);
        assert_eq!(
            std::fs::read_to_string(&claude).unwrap(),
            "the session's edit"
        );
        std::fs::remove_file(user.path().join("CLAUDE.md")).unwrap();
        link_into_shell_home(home.path(), user.path(), CLAUDE_LINKS);
        assert_eq!(
            std::fs::read_to_string(&claude).unwrap(),
            "the session's edit"
        );
    }

    /// Seam R5: a person's folder named with `/` gets a junction that resolves.
    #[test]
    fn a_source_named_with_slashes_gets_a_junction_that_resolves() {
        let user = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(user.path().join("skills")).unwrap();
        std::fs::write(user.path().join("skills").join("a.md"), "skill").unwrap();
        let slashed = std::path::PathBuf::from(user.path().to_string_lossy().replace('\\', "/"));
        assert_eq!(link_into_shell_home(home.path(), &slashed, CLAUDE_LINKS), 1);
        assert_eq!(
            std::fs::read_to_string(home.path().join("skills").join("a.md")).unwrap(),
            "skill"
        );
        // The next pass keeps the junction it made.
        let made = std::fs::read_link(home.path().join("skills")).unwrap();
        assert_eq!(link_into_shell_home(home.path(), &slashed, CLAUDE_LINKS), 1);
        assert_eq!(
            std::fs::read_link(home.path().join("skills")).unwrap(),
            made
        );
    }

    #[test]
    fn a_junction_names_local_folders_only() {
        let home = tempfile::tempdir().unwrap();
        let link = home.path().join("share");
        assert!(windows_links::junction(Path::new(r"\\server\share\skills"), &link).is_err());
        assert!(!link.exists(), "nothing left behind");
        assert!(windows_links::junction(Path::new("relative"), &link).is_err());
    }
}
