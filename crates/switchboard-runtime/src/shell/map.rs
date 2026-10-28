//! The folder map of the terminal hook (SB-183, node A-1): `<data>/shell/map.json`.
//!
//! The owner's bindings pass writes it (node A-4) for the folders of groups bound to an
//! organization; `switchboard shell env` reads it on every change of folder in a hooked
//! terminal. The reader takes no store lock and asks no owner: owner answers stalled up to 38 s
//! on 0.6.82, and a terminal must never wait on Switchboard or break because of it. Every
//! failure reads as "nothing applies here".
//!
//! Schema, version 1 (0600, at most 1 MiB):
//! `{"version": 1, "rows": [{"folder", "pool", "context", "claude", "codex"}]}`: a group's
//! folder (absolute), its pool, its context (`org:<id>`), whether the folder's Claude Code is
//! bound to the group (`claude-bindings.json` holds it) and whether the pool has an enabled Codex
//! account. Folders, pools and organization ids only: never a capability or a token.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The map's schema version; a map of another version reads as missing.
pub const VERSION: u32 = 1;
/// The largest map the reader takes.
pub const MAX_BYTES: u64 = 1024 * 1024;
/// `<data>/shell`: the shell homes and the map.
pub const DIR: &str = "shell";
const FILE: &str = "map.json";
/// `<data>/shell-hook`: the person's choice, `on` or `off`; absent means the default (D-3).
pub const CHOICE: &str = "shell-hook";
/// `<data>/shell-hook.json`: what the installer did to each start file (node A-3 writes it).
pub const RECORD: &str = "shell-hook.json";
/// Exported (as `1`) by a loaded hook, so `shell status` can say whether this terminal has it.
pub const HOOK_LOADED: &str = "SWITCHBOARD_SHELL_HOOK";
/// The answer of `shell env` when nothing applies.
pub const NONE: &str = "none";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    pub version: u32,
    pub rows: Vec<Row>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// A group's folder, absolute; its subfolders and worktrees belong to it too.
    pub folder: String,
    /// The group's pool: names its shell homes.
    pub pool: String,
    /// `org:<id>`, exported as `SWITCHBOARD_CONTEXT`.
    pub context: String,
    /// The folder's Claude Code is bound to the group: the Claude shell home applies.
    pub claude: bool,
    /// The pool has an enabled Codex account: the Codex shell home applies.
    pub codex: bool,
}

pub fn path(root: &Path) -> PathBuf {
    root.join(DIR).join(FILE)
}
/// `<data>/shell/claude-<pool>`.
pub fn claude_home(root: &Path, pool: &str) -> PathBuf {
    root.join(DIR).join(format!("claude-{pool}"))
}
/// `<data>/shell/codex-<pool>`.
pub fn codex_home(root: &Path, pool: &str) -> PathBuf {
    root.join(DIR).join(format!("codex-{pool}"))
}

/// Writes the map (0600, replaced in one rename). For the owner's bindings pass, which writes
/// one row per folder, in canonical form, for the group `Snapshot::project_for` chooses there
/// when that group is bound to an organization. Two rows naming the same folder are refused.
pub fn write(root: &Path, map: &Map) -> Result<(), String> {
    let mut folders = std::collections::BTreeSet::new();
    for row in &map.rows {
        // Compared as the reader compares folders: on Windows `C:\A` and `c:\a\` are one.
        let folder = key(Path::new(&row.folder))
            .unwrap_or_else(|| vec![std::ffi::OsString::from(&row.folder)]);
        if !folders.insert(folder) {
            return Err("Two rows of the terminal folder map name the same folder.".into());
        }
    }
    // Encoding plain strings and booleans cannot fail; storage's own sentence if it ever did.
    let bytes = serde_json::to_vec(map).map_err(|_| "Private account storage unavailable")?;
    switchboard_core::private_fs::private_write(&path(root), &bytes)
}

/// What reading the map found.
#[derive(Debug, PartialEq, Eq)]
pub enum Loaded {
    Read(Map),
    Missing,
    Unreadable,
    UnknownVersion,
}
impl Loaded {
    pub fn code(&self) -> &'static str {
        match self {
            Loaded::Read(_) => "ok",
            Loaded::Missing => "missing",
            Loaded::Unreadable => "unreadable",
            Loaded::UnknownVersion => "unknown_version",
        }
    }
}

pub fn read(root: &Path) -> Loaded {
    let path = path(root);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
        Err(_) => return Loaded::Unreadable,
        Ok(_) => {}
    }
    let Ok(bytes) = switchboard_core::private_fs::read_private(&path, MAX_BYTES) else {
        return Loaded::Unreadable;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Loaded::Unreadable;
    };
    if value.get("version").and_then(Value::as_u64) != Some(u64::from(VERSION)) {
        return Loaded::UnknownVersion;
    }
    match serde_json::from_value::<Map>(value) {
        Ok(map) => Loaded::Read(map),
        Err(_) => Loaded::Unreadable,
    }
}

impl Map {
    /// The row whose folder holds `dir` (the folder itself, a subfolder, a worktree inside it),
    /// the longest folder winning. `dir` is matched with its links resolved, so a link inside a
    /// group's folder that leads out of it (`/w/acme/out -> /w/mine`) is the folder it leads to,
    /// and a link from outside that leads in is the group's; only a `dir` that cannot be
    /// resolved (it does not exist, or cannot be read) is matched as given. Whole path
    /// components are compared, so `/w/acme-other` is not inside `/w/acme`. Rows that are not
    /// valid are skipped, and a `dir` with a `..` step is never matched (`/w/acme/../mine` is
    /// not inside `/w/acme`). Of two rows of the same length, the earlier one wins. On Windows
    /// folders compare as Windows names them ([`windows_key`]): `\\?\C:\w\acme` (what
    /// `canonicalize` answers, and so how rows are written) holds `c:/W/Acme/src`.
    pub fn lookup(&self, dir: &Path) -> Option<&Row> {
        if !dir.is_absolute()
            || dir
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return None;
        }
        let target = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let target = key(&target)?;
        let mut best: Option<(usize, &Row)> = None;
        for row in self.valid_rows() {
            let Some(folder) = key(Path::new(&row.folder)) else {
                continue;
            };
            if !target.starts_with(&folder) {
                continue;
            }
            let length = folder.len();
            if best.is_none_or(|(longest, _)| length > longest) {
                best = Some((length, row));
            }
        }
        best.map(|(_, row)| row)
    }
}
/// A folder as the map compares it, one entry per path component. Elsewhere the components as
/// they are, so a folder holds another exactly when [`Path::starts_with`] says so.
#[cfg(not(windows))]
fn key(path: &Path) -> Option<Vec<std::ffi::OsString>> {
    Some(
        path.components()
            .map(|c| c.as_os_str().to_owned())
            .collect(),
    )
}
/// On Windows, [`windows_key`] of the path's text.
#[cfg(windows)]
fn key(path: &Path) -> Option<Vec<std::ffi::OsString>> {
    windows_key(&path.to_string_lossy()).map(|parts| parts.into_iter().map(Into::into).collect())
}
/// An absolute Windows folder as Windows compares it: the verbatim prefix `canonicalize` adds
/// dropped (`\\?\C:\w` is `C:\w`, `\\?\UNC\host\share` is `\\host\share`), `/` read as `\`,
/// empty and `.` steps skipped, and every part in one case (NTFS and SMB names are
/// case-insensitive). None for a path that is not absolute (no drive and root, no share) or
/// that has a `..` step. Pure text, so it is tested on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_key(text: &str) -> Option<Vec<String>> {
    let text = text.replace('/', "\\");
    let rest = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(local) = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\\.\"))
    {
        local.to_owned()
    } else {
        text
    };
    let mut parts = Vec::new();
    let steps = if let Some(unc) = rest.strip_prefix(r"\\") {
        let mut steps = unc.split('\\');
        let (host, share) = (steps.next()?, steps.next()?);
        if host.is_empty() || share.is_empty() || host == "?" || host == "." {
            return None;
        }
        parts.push(format!(r"\\{}", host.to_lowercase()));
        parts.push(share.to_lowercase());
        steps
    } else {
        let bytes = rest.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return None;
        }
        parts.push(rest[..2].to_ascii_lowercase());
        rest[3..].split('\\')
    };
    for step in steps {
        match step {
            "" | "." => {}
            ".." => return None,
            name => parts.push(name.to_lowercase()),
        }
    }
    Some(parts)
}
impl Map {
    /// The rows a reader acts on: the valid ones, in the map's order.
    pub fn valid_rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|row| row.valid())
    }
}
/// Whether the hook is on by default: the map holds a valid row, a folder of a group bound to
/// an organization. The one rule for it (A-7 fix cycle 1): the owner's installer writes the
/// default block by it and `status` (the window, MCP `switchboard_shell_hook`, `shell status`,
/// onboarding's `hook` step) reports `default_on` by it, both from the same file, so a written
/// block and a reported state never disagree. A group whose folders are all taken by another
/// group (`project_for`) or missing on disk has no row and so does not count.
pub fn organizations_mapped(loaded: &Loaded) -> bool {
    matches!(loaded, Loaded::Read(map) if map.valid_rows().next().is_some())
}
/// [`organizations_mapped`] of the map in `root`.
pub fn organizations_present(root: &Path) -> bool {
    organizations_mapped(&read(root))
}
impl Row {
    /// A row the answer can carry: an absolute folder, a pool that names a folder inside
    /// `shell/` and a context that keeps the answer one tab-separated line.
    fn valid(&self) -> bool {
        let plain = |text: &str| {
            !text.is_empty()
                && text.len() <= 256
                && !text.chars().any(|c| c.is_control() || c.is_whitespace())
        };
        Path::new(&self.folder).is_absolute()
            && !self.folder.chars().any(char::is_control)
            && plain(&self.context)
            && plain(&self.pool)
            && !self.pool.starts_with('.')
            && self
                .pool
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    }
}

/// The person's choice in `<data>/shell-hook`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    On,
    Off,
    Default,
}
pub fn choice(root: &Path) -> Choice {
    let mut text = String::new();
    let read = std::fs::File::open(root.join(CHOICE)).and_then(|file| {
        use std::io::Read;
        file.take(64).read_to_string(&mut text)
    });
    match (read, text.trim()) {
        (Ok(_), "on") => Choice::On,
        (Ok(_), "off") => Choice::Off,
        _ => Choice::Default,
    }
}

/// What applies in a folder: the group's pool, its context and the shell homes to export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub pool: String,
    pub context: String,
    pub claude_home: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
}
impl Answer {
    /// `<pool>\t<context>\t<claude home or ->\t<codex home or ->`.
    pub fn line(&self) -> String {
        let home = |h: &Option<PathBuf>| {
            h.as_ref()
                .map_or_else(|| "-".to_owned(), |p| p.to_string_lossy().into_owned())
        };
        format!(
            "{}\t{}\t{}\t{}",
            self.pool,
            self.context,
            home(&self.claude_home),
            home(&self.codex_home)
        )
    }
}

/// What applies in `dir`, or None: the hook turned off, the map missing, unreadable or of an
/// unknown version, the folder in no organization's group, or a row with neither home there
/// (the Claude home is answered in a row's own folder only, the Codex home anywhere inside it).
pub fn answer(root: &Path, dir: &Path) -> Option<Answer> {
    answer_in(root, dir, &read(root))
}
fn answer_in(root: &Path, dir: &Path, loaded: &Loaded) -> Option<Answer> {
    if choice(root) == Choice::Off {
        return None;
    }
    let Loaded::Read(map) = loaded else {
        return None;
    };
    let row = map.lookup(dir)?;
    // Claude Code reads Switchboard's route only from `.claude/settings.local.json` in the folder
    // it starts in (measured, docs/evidence/shell-home-measurement-2026-10.md): below the bound
    // folder the home's placeholder would reach the provider, so no Claude home there.
    let at_folder = key(&dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()))
        == key(Path::new(&row.folder));
    let answer = Answer {
        pool: row.pool.clone(),
        context: row.context.clone(),
        claude_home: (row.claude && at_folder).then(|| claude_home(root, &row.pool)),
        codex_home: row.codex.then(|| codex_home(root, &row.pool)),
    };
    (answer.claude_home.is_some() || answer.codex_home.is_some()).then_some(answer)
}

/// The one line `switchboard shell env` prints: `none`, or [`Answer::line`]. `none` too when
/// the data folder's path would break the line.
pub fn env_line(root: &Path, dir: &Path) -> String {
    if root.to_string_lossy().chars().any(char::is_control) {
        return NONE.to_owned();
    }
    answer(root, dir).map_or_else(|| NONE.to_owned(), |a| a.line())
}

/// `switchboard shell status`: the hook's state on this computer, whether this terminal has it
/// loaded, what applies in `dir` and what the installer did to each start file. Reads files
/// only (no store lock, no owner) and always answers: a data folder that cannot be named reads
/// as an unreadable map.
///
/// `hook`: `on` or `off` (the person's choice), `default_on` (no choice, and a group bound to an
/// organization is in the map) or `default_off`. `here` is what `shell env` answers in `dir`.
/// `files` comes from the installer's record (`shell-hook.json`, node A-3):
/// `{"files": [{"shell", "path", "state"}]}`, other fields ignored, entries without the three
/// strings skipped.
pub fn status(
    root: Option<&Path>,
    dir: Option<&Path>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Value {
    let root = root.filter(|r| r.is_absolute());
    let loaded = root.map_or(Loaded::Unreadable, read);
    let choice = root.map_or(Choice::Default, choice);
    let organizations = organizations_mapped(&loaded);
    let hook = match choice {
        Choice::On => "on",
        Choice::Off => "off",
        Choice::Default if organizations => "default_on",
        Choice::Default => "default_off",
    };
    let here = root
        .zip(dir)
        .and_then(|(root, dir)| answer_in(root, dir, &loaded));
    json!({
        "hook": hook,
        "choice": match choice { Choice::On => "on", Choice::Off => "off", Choice::Default => "default" },
        "map": loaded.code(),
        "folder": dir,
        "terminal": {
            "hook_loaded": env(HOOK_LOADED).as_deref() == Some("1"),
            "launched_session": env("SWITCHBOARD_SESSION").is_some_and(|v| !v.is_empty()),
        },
        "here": here.map(|a| json!({
            "group": a.pool,
            "context": a.context,
            "claude_home": a.claude_home,
            "codex_home": a.codex_home,
        })),
        "files": root.map(record_files).unwrap_or_default(),
    })
}
fn record_files(root: &Path) -> Vec<Value> {
    let Ok(bytes) = switchboard_core::private_fs::read_private(&root.join(RECORD), MAX_BYTES)
    else {
        return vec![];
    };
    let Ok(record) = serde_json::from_slice::<Value>(&bytes) else {
        return vec![];
    };
    record["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let text = |key: &str| file.get(key).and_then(Value::as_str);
            Some(json!({"shell": text("shell")?, "path": text("path")?, "state": text("state")?}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(folder: &str, pool: &str, context: &str, claude: bool, codex: bool) -> Row {
        Row {
            folder: folder.into(),
            pool: pool.into(),
            context: context.into(),
            claude,
            codex,
        }
    }

    #[test]
    fn the_longest_folder_of_an_organization_group_answers_its_homes_and_context() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().canonicalize().unwrap();
        let acme = work.join("acme");
        let nested = acme.join("vendor").join("globex");
        let skipped = work.join("mine");
        for dir in [&nested, &skipped] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let s = |p: &Path| p.to_string_lossy().into_owned();
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![
                    row(&s(&acme), "acme", "org:o-acme", true, false),
                    row(&s(&nested), "globex", "org:o-globex", true, true),
                    // A folder whose Claude Code settings are the person's and whose pool has
                    // no Codex account: nothing to export there.
                    row(&s(&skipped), "mine", "org:o-acme", false, false),
                ],
            },
        )
        .unwrap();
        // The group's folder: the group's Claude home, no Codex home. (A subfolder of a group
        // with only Claude answers nothing: `the_claude_home_is_answered_in_the_bound_folder_itself_only`.)
        let inside = acme.join("app").join("src");
        assert_eq!(answer(root, &inside), None);
        let found = answer(root, &acme).unwrap();
        assert_eq!(found.pool, "acme");
        assert_eq!(found.context, "org:o-acme");
        assert_eq!(
            found.claude_home,
            Some(root.join("shell").join("claude-acme"))
        );
        assert_eq!(found.codex_home, None);
        assert_eq!(
            found.line(),
            format!(
                "acme\torg:o-acme\t{}\t-",
                s(&root.join("shell").join("claude-acme"))
            )
        );
        // A group folder inside another: the longer folder wins, both homes.
        let found = answer(root, &nested.join("x")).unwrap();
        assert_eq!(
            (found.pool.as_str(), found.context.as_str()),
            ("globex", "org:o-globex")
        );
        assert_eq!(
            found.codex_home,
            Some(root.join("shell").join("codex-globex"))
        );
        assert_eq!(answer(root, &nested).unwrap().pool, "globex");
        // The folder itself.
        assert_eq!(answer(root, &acme).unwrap().pool, "acme");
        // The same folder reached through a link.
        #[cfg(unix)]
        {
            let link = work.join("link-to-acme");
            std::os::unix::fs::symlink(&acme, &link).unwrap();
            assert_eq!(answer(root, &link).unwrap().pool, "acme");
        }
        // A sibling whose name starts the same is not inside; neither is the parent.
        assert_eq!(answer(root, &work.join("acme-other")), None);
        assert_eq!(answer(root, &work), None);
        // A row with neither home changes nothing.
        assert_eq!(answer(root, &skipped), None);
        assert_eq!(env_line(root, &skipped), NONE);
        // A relative folder is never matched.
        assert_eq!(answer(root, Path::new("acme")), None);
        // Rows that would break the answer line or name a path outside `shell/` are skipped
        // (written by hand: `write` takes one row per folder).
        write_raw(
            root,
            &Map {
                version: VERSION,
                rows: vec![
                    row(&s(&acme), "../escape", "org:o-acme", true, true),
                    row(&s(&acme), "acme", "org:o\tbad", true, true),
                    row("relative/acme", "acme", "org:o-acme", true, true),
                ],
            },
        );
        assert_eq!(answer(root, &inside), None);
    }

    /// Claude Code 2.1.299 reads Switchboard's route only from `.claude/settings.local.json` in
    /// the folder it starts in, git repository or not (`docs/evidence/shell-home-measurement-2026-10.md`):
    /// below the bound folder the shell home's placeholder would reach the provider. So the Claude
    /// home is answered in the bound folder itself only; the Codex home everywhere inside.
    #[test]
    fn the_claude_home_is_answered_in_the_bound_folder_itself_only() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().canonicalize().unwrap();
        let (acme, globex) = (work.join("acme"), work.join("globex"));
        for dir in [&acme.join("src"), &globex.join("src")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let s = |p: &Path| p.to_string_lossy().into_owned();
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![
                    row(&s(&acme), "acme", "org:o-acme", true, false),
                    row(&s(&globex), "globex", "org:o-globex", true, true),
                ],
            },
        )
        .unwrap();
        let claude = root.join("shell").join("claude-acme");
        assert_eq!(answer(root, &acme).unwrap().claude_home, Some(claude));
        // Below it, a Claude-only group has nothing to export: `none`.
        assert_eq!(answer(root, &acme.join("src")), None);
        assert_eq!(env_line(root, &acme.join("src")), NONE);
        // A group with Codex: its context and Codex home, `-` for Claude.
        let below = answer(root, &globex.join("src")).unwrap();
        assert_eq!(below.claude_home, None);
        assert_eq!(
            below.codex_home,
            Some(root.join("shell").join("codex-globex"))
        );
        assert_eq!(
            below.line(),
            format!(
                "globex\torg:o-globex\t-\t{}",
                s(&root.join("shell").join("codex-globex"))
            )
        );
        // The folder itself through a link is the folder itself.
        #[cfg(unix)]
        {
            let link = work.join("link-to-acme");
            std::os::unix::fs::symlink(&acme, &link).unwrap();
            assert!(answer(root, &link).unwrap().claude_home.is_some());
        }
    }

    /// A map as it lands on disk, bypassing `write`'s checks (an older or hand-edited file).
    fn write_raw(root: &Path, map: &Map) {
        switchboard_core::private_fs::private_write(&path(root), &serde_json::to_vec(map).unwrap())
            .unwrap();
    }

    #[test]
    fn the_map_holds_one_row_per_folder() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        // An absolute folder on every platform (`/w/acme` is not absolute on Windows).
        let tmp = tempfile::tempdir().unwrap();
        let acme = tmp.path().canonicalize().unwrap().join("acme");
        std::fs::create_dir_all(acme.join("src")).unwrap();
        let folder = acme.to_string_lossy().into_owned();
        let one = |pool: &str| row(&folder, pool, "org:o-acme", true, false);
        let refused = write(
            root,
            &Map {
                version: VERSION,
                rows: vec![one("acme"), one("globex")],
            },
        )
        .unwrap_err();
        assert!(refused.contains("same folder"), "{refused}");
        assert_eq!(read(root), Loaded::Missing, "nothing written");
        // The same folder spelled with a trailing separator is the same folder.
        let mut trailing = one("globex");
        trailing.folder = format!("{folder}{}", std::path::MAIN_SEPARATOR);
        assert!(write(
            root,
            &Map {
                version: VERSION,
                rows: vec![one("acme"), trailing],
            },
        )
        .is_err());
        // A map that holds two rows for one folder anyway: the first row answers, every time.
        write_raw(
            root,
            &Map {
                version: VERSION,
                rows: vec![one("acme"), one("globex"), one("initech")],
            },
        );
        assert_eq!(answer(root, &acme).unwrap().pool, "acme");
    }

    #[test]
    fn a_folder_with_a_parent_step_is_never_matched() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().canonicalize().unwrap();
        let acme = work.join("acme");
        let mine = work.join("mine");
        for dir in [&acme, &mine] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let s = |p: &Path| p.to_string_lossy().into_owned();
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![row(&s(&acme), "acme", "org:o-acme", true, true)],
            },
        )
        .unwrap();
        // `<acme>/../mine` is the person's folder, though it starts with the group's folder.
        // Spelled as text: on Windows `join("..")` on the verbatim `\\?\C:\...` that
        // `canonicalize` answers drops the step itself, and a terminal's folder is plain text.
        let sep = std::path::MAIN_SEPARATOR;
        for acme in [s(&acme), plain(&acme)] {
            let up = |rest: &str| PathBuf::from(format!("{acme}{sep}{rest}"));
            assert_eq!(answer(root, &up(&format!("..{sep}mine"))), None, "{acme}");
            assert_eq!(answer(root, &up(&format!("src{sep}.."))), None, "{acme}");
            assert_eq!(answer(root, Path::new(&acme)).unwrap().pool, "acme");
        }
    }

    /// The folder as a terminal names it: on Windows without the `\\?\` prefix
    /// `canonicalize` adds (PowerShell's `$PWD` is `C:\...`).
    fn plain(path: &Path) -> String {
        let text = path.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }

    #[test]
    fn a_windows_folder_compares_as_windows_names_it() {
        let key = |text: &str| windows_key(text).map(|parts| parts.join("|"));
        let acme = Some("c:|work|acme".to_owned());
        // The verbatim prefix, the drive's case, the names' case, `/`, a trailing separator
        // and `.` steps do not make another folder.
        for spelling in [
            r"C:\Work\Acme",
            r"\\?\C:\work\acme",
            r"c:\WORK\ACME\",
            "C:/Work/Acme",
            r"C:\Work\.\Acme",
            r"\\.\C:\Work\Acme",
        ] {
            assert_eq!(key(spelling), acme, "{spelling}");
        }
        // Shares: `\\?\UNC\host\share` is `\\host\share`.
        let share = Some(r"\\host|share|acme".to_owned());
        assert_eq!(key(r"\\Host\Share\Acme"), share);
        assert_eq!(key(r"\\?\UNC\host\share\acme"), share);
        // Not absolute, or with a `..` step: no key, so never matched.
        for refused in [
            r"\Work\Acme",
            r"C:Work\Acme",
            "acme",
            "/w/acme",
            r"C:\Work\..\Acme",
            r"\\host",
            r"\\?\pipe",
            "",
        ] {
            assert_eq!(key(refused), None, "{refused}");
        }
        // Whole names: `acme-other` is not inside `acme`.
        let inside = |dir: &str, folder: &str| {
            windows_key(dir)
                .zip(windows_key(folder))
                .is_some_and(|(d, f)| d.starts_with(&f))
        };
        assert!(inside(r"c:\work\ACME\src", r"\\?\C:\Work\acme"));
        assert!(!inside(r"C:\Work\acme-other", r"C:\Work\acme"));
        assert!(!inside(r"D:\Work\acme", r"C:\Work\acme"));
        assert!(!inside(r"C:\Work", r"C:\Work\acme"));
    }

    /// Windows: rows are written as `canonicalize` answers (`\\?\C:\...`); the terminal asks
    /// with `$PWD` (`C:\...`), in any case, with `/` too, and about folders that may not exist.
    #[cfg(windows)]
    #[test]
    fn a_windows_terminal_folder_finds_its_verbatim_row() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let acme = tmp.path().canonicalize().unwrap().join("acme");
        std::fs::create_dir_all(acme.join("src")).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        assert!(s(&acme).starts_with(r"\\?\"), "{}", s(&acme));
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![row(&s(&acme), "acme", "org:o-acme", true, true)],
            },
        )
        .unwrap();
        let plain = plain(&acme);
        for asked in [
            plain.clone(),
            plain.to_uppercase(),
            plain.replace('\\', "/"),
            format!(r"{plain}\"),
        ] {
            let found = answer(root, Path::new(&asked)).unwrap();
            assert_eq!(found.pool, "acme", "{asked}");
            assert!(found.claude_home.is_some(), "the folder itself: {asked}");
        }
        for below in [format!(r"{plain}\src"), format!(r"{plain}\gone\deeper")] {
            let found = answer(root, Path::new(&below)).unwrap();
            assert_eq!(found.claude_home, None, "{below}");
            assert!(found.codex_home.is_some(), "{below}");
        }
        assert_eq!(answer(root, Path::new(&format!("{plain}-other"))), None);
        // The same folder in another spelling is the same row.
        let again = row(&plain.to_uppercase(), "globex", "org:o-globex", true, true);
        assert!(write(
            root,
            &Map {
                version: VERSION,
                rows: vec![row(&s(&acme), "acme", "org:o-acme", true, true), again],
            },
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_a_group_folder_never_answers_its_homes() {
        // A link inside the group's folder that leads to the person's own folder is the
        // person's folder; a link from outside that leads into the group's folder is the
        // group's.
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().canonicalize().unwrap();
        let acme = work.join("acme");
        let mine = work.join("mine");
        for dir in [&acme, &mine] {
            std::fs::create_dir_all(dir.join("src")).unwrap();
        }
        let out = acme.join("out");
        std::os::unix::fs::symlink(&mine, &out).unwrap();
        let into = work.join("link");
        std::os::unix::fs::symlink(&acme, &into).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![row(&s(&acme), "acme", "org:o-acme", true, true)],
            },
        )
        .unwrap();
        assert_eq!(answer(root, &out), None);
        assert_eq!(answer(root, &out.join("src")), None);
        assert_eq!(answer(root, &into).unwrap().pool, "acme");
        assert_eq!(answer(root, &into.join("src")).unwrap().pool, "acme");
        // A folder that cannot be resolved (it does not exist) is taken as given.
        assert_eq!(answer(root, &acme.join("gone")).unwrap().pool, "acme");
    }

    #[cfg(unix)]
    #[test]
    fn a_row_stored_through_a_link_loses_to_the_resolved_folder() {
        // Rows are written in canonical form and a folder is matched with its links resolved:
        // a row that names a folder through a link never holds a folder that resolves, and
        // where the folder's resolved path has its own row, that row answers whatever the
        // rows' order.
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().canonicalize().unwrap();
        let acme = work.join("acme");
        std::fs::create_dir_all(acme.join("src")).unwrap();
        let link = work.join("link");
        std::os::unix::fs::symlink(&acme, &link).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        // Both homes, so a subfolder answers (its Codex home) and the match itself is seen.
        let linked = row(&s(&link), "linked", "org:o-linked", true, true);
        let real = row(&s(&acme), "acme", "org:o-acme", true, true);
        for rows in [
            vec![real.clone(), linked.clone()],
            vec![linked.clone(), real.clone()],
        ] {
            write(
                root,
                &Map {
                    version: VERSION,
                    rows,
                },
            )
            .unwrap();
            assert_eq!(answer(root, &link.join("src")).unwrap().pool, "acme");
            assert_eq!(answer(root, &acme.join("src")).unwrap().pool, "acme");
        }
        // Alone, the linked row answers nothing that resolves, through the link or not; only a
        // folder under the link that cannot be resolved (it does not exist) is taken as given.
        write(
            root,
            &Map {
                version: VERSION,
                rows: vec![linked],
            },
        )
        .unwrap();
        assert_eq!(answer(root, &link.join("src")), None);
        assert_eq!(answer(root, &acme.join("src")), None);
        assert_eq!(answer(root, &link.join("gone")).unwrap().pool, "linked");
    }
}
