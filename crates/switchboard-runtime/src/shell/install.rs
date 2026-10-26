//! The start-file installer of the terminal hook (SB-183, node A-3).
//!
//! On a computer holding a group bound to an organization, the installed app's owner puts one
//! marked block into the start file of each shell present (operator decision D-3: on by
//! default, off by one command). The block reads the hook script only if it is there:
//!
//! ```text
//! # >>> Fabric Switchboard: shell hook (SB-183) >>>
//! if [ -r '<data>/shell/hook.zsh' ]; then . '<data>/shell/hook.zsh'; fi
//! # <<< Fabric Switchboard <<<
//! ```
//!
//! - zsh: `${ZDOTDIR:-$HOME}/.zshrc`, created only when zsh is the login shell; bash: on macOS the
//!   first existing of `.bash_profile`, `.bash_login`, `.profile` (else `.bash_profile`), on
//!   Linux `.bashrc`, only when it exists or bash is the login shell; fish: its own file
//!   `~/.config/fish/conf.d/fabric-switchboard.fish`, only when `~/.config/fish` exists.
//! - Nothing outside the markers changes; taking the block out gives the file back byte for byte.
//! - Never written: a start file that is a link (dotfile managers, A-Q3), another user's file, a
//!   file that is not valid text, a file whose markers were changed, a file that changed while
//!   it was being written (tried again next time).
//! - Never fought: a block the person took out is recorded `removed_by_person` and not written
//!   back until `switchboard shell on`.
//! - The choice `<data>/shell-hook`: `on` writes even without an organization, `off` takes every
//!   block out and never writes again, absent is the default (D-3).
//! - The record `<data>/shell-hook.json` (0600): `{"files": [{"shell", "path", "state", ...}]}`.
//!
//! Who calls it: the installed app's owner (at start, and when the presence of an organization's
//! group flips in a bindings pass), `switchboard shell on|off`, and `switchboard uninstall`.
//! Never a development build, `serve`, `--data-dir` or a test: their owners never get a [`Host`].
use super::{map, scripts};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const OPEN: &str = "# >>> Fabric Switchboard: shell hook (SB-183) >>>";
pub const CLOSE: &str = "# <<< Fabric Switchboard <<<";
const FISH_FILE: &str = "fabric-switchboard.fish";
const MAX_RC: u64 = 1024 * 1024;

/// The person's home as the installer sees it. Injected: a test never names the real home.
#[derive(Clone, Debug)]
pub struct Host {
    pub home: PathBuf,
    /// zsh's `ZDOTDIR`, when set.
    pub zdotdir: Option<PathBuf>,
    /// The login shell's name (`zsh`, `bash`, `fish`), when known.
    pub login_shell: Option<String>,
    /// macOS's start-file rules for bash (Terminal starts login shells).
    pub macos: bool,
    /// The person's user id: a start file someone else owns is never written. None: not
    /// checked (Windows, which has no start file here yet).
    pub uid: Option<u32>,
    /// The CLI the hook script names; None: no hook can be written.
    pub cli: Option<PathBuf>,
}
impl Host {
    /// This process's user: `HOME`, `ZDOTDIR`, the login shell (`SHELL`).
    pub fn process(cli: Option<PathBuf>) -> Option<Host> {
        let home = dirs::home_dir()?;
        let zdotdir = std::env::var_os("ZDOTDIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let login_shell = std::env::var_os("SHELL")
            .map(PathBuf::from)
            .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned));
        #[cfg(unix)]
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = Some(unsafe { libc::getuid() });
        #[cfg(not(unix))]
        let uid = None;
        Some(Host {
            home,
            zdotdir,
            login_shell,
            macos: cfg!(target_os = "macos"),
            uid,
            cli,
        })
    }
    fn login(&self, shell: &str) -> bool {
        self.login_shell.as_deref() == Some(shell)
    }
}

/// A start file the installer looks after.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    kind: scripts::Kind,
    path: PathBuf,
    /// Switchboard's own file (fish's `conf.d`): created and deleted whole.
    own_file: bool,
    /// Created when it does not exist.
    create: bool,
}

/// The start files of the shells present for `host`. None on Windows yet (PowerShell is node
/// A-8; `cmd.exe` and Git Bash are not covered).
fn targets(host: &Host) -> Vec<Target> {
    if cfg!(windows) {
        return vec![];
    }
    let exists = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    let mut out = Vec::new();
    let zsh = host
        .zdotdir
        .clone()
        .unwrap_or_else(|| host.home.clone())
        .join(".zshrc");
    if exists(&zsh) || host.login("zsh") {
        out.push(Target {
            kind: scripts::Kind::Zsh,
            create: host.login("zsh"),
            path: zsh,
            own_file: false,
        });
    }
    let bash = if host.macos {
        [".bash_profile", ".bash_login", ".profile"]
            .iter()
            .map(|n| host.home.join(n))
            .find(|p| exists(p))
            .unwrap_or_else(|| host.home.join(".bash_profile"))
    } else {
        host.home.join(".bashrc")
    };
    if exists(&bash) || host.login("bash") {
        out.push(Target {
            kind: scripts::Kind::Bash,
            create: host.login("bash"),
            path: bash,
            own_file: false,
        });
    }
    let fish = host.home.join(".config").join("fish");
    if fish.is_dir() {
        out.push(Target {
            kind: scripts::Kind::Fish,
            path: fish.join("conf.d").join(FISH_FILE),
            own_file: true,
            create: true,
        });
    }
    out
}

/// What the installer did to one start file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Written,
    RemovedByPerson,
    SkippedSymlink,
    SkippedOwner,
    Unreadable,
}

/// One start file in the record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub shell: String,
    pub path: String,
    pub state: State,
    /// The installer added a line break before the block (the file did not end with one), so
    /// taking the block out takes that break too.
    #[serde(default)]
    pub newline_added: bool,
    #[serde(default)]
    pub at: i64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub files: Vec<Entry>,
}

/// What one run did, for the log and the window's first-time notice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Whether the hook is wanted now (choice, else the organizations' presence).
    pub wanted: bool,
    /// Start files that hold the block after this run.
    pub written: usize,
    /// Blocks taken out in this run.
    pub removed: usize,
    /// Start files left alone (link, another user's, unreadable, taken out by the person).
    pub skipped: usize,
    /// Start files that changed while being written: tried again next time.
    pub changed: usize,
    /// No block had ever been written before, and one is now: the window says so once.
    pub first_written: bool,
    /// The hook could not be written (no CLI, or a path that is not text): no block either.
    pub no_hook: bool,
}

pub fn record_path(root: &Path) -> PathBuf {
    root.join(map::RECORD)
}
pub fn read_record(root: &Path) -> Record {
    switchboard_core::private_fs::read_private(&record_path(root), map::MAX_BYTES)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
fn write_record(root: &Path, record: &Record) {
    if read_record(root) == *record {
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(record) {
        // A record that cannot be saved is written again on the next run; the start files are
        // already right.
        let _ = switchboard_core::private_fs::private_write(&record_path(root), &bytes);
    }
}

/// Saves the person's choice: `on` or `off`.
pub fn set_choice(root: &Path, on: bool) -> Result<(), String> {
    crate::launch::private_write(
        &root.join(map::CHOICE),
        if on { b"on\n" } else { b"off\n" },
        false,
    )
    .map_err(|_| CHOICE_UNSAVED.to_owned())
}
/// The one sentence a failed choice answers with.
pub const CHOICE_UNSAVED: &str =
    "The terminal hook's choice could not be saved in Switchboard's data folder.";

/// The block for a start file of `kind` reading `hook`; None when the hook's path is not text.
fn block(kind: scripts::Kind, hook: &Path) -> Option<String> {
    let hook = hook.to_str()?;
    let line = match kind {
        scripts::Kind::Fish => {
            let q = scripts::fish_quote(hook);
            format!("if test -r {q}; source {q}; end")
        }
        scripts::Kind::Zsh | scripts::Kind::Bash => {
            let q = scripts::sh_quote(hook);
            format!("if [ -r {q} ]; then . {q}; fi")
        }
    };
    Some(format!("{OPEN}\n{line}\n{CLOSE}\n"))
}

/// Where the block is in a start file's text.
#[derive(Debug, PartialEq, Eq)]
enum Found {
    None,
    /// Byte range of the block, from the start of its opening line to the end of its closing
    /// line (its line break included when there is one).
    Block(std::ops::Range<usize>),
    /// An opening line without a closing one, or two blocks: the person's to sort out.
    Broken,
}
fn find(text: &str) -> Found {
    let mut at = 0;
    let mut open: Option<usize> = None;
    let mut found: Option<std::ops::Range<usize>> = None;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == OPEN {
            if open.is_some() || found.is_some() {
                return Found::Broken;
            }
            open = Some(at);
        } else if bare == CLOSE {
            match open.take() {
                Some(start) => found = Some(start..at + line.len()),
                None => return Found::Broken,
            }
        }
        at += line.len();
    }
    match (open, found) {
        (Some(_), _) => Found::Broken,
        (None, Some(range)) => Found::Block(range),
        (None, None) => Found::None,
    }
}

/// The start file as read: its text and the stamp the write compares against.
struct Read {
    text: String,
    stamp: Option<(u64, Option<std::time::SystemTime>)>,
    mode: Option<u32>,
}
/// Reads a start file the installer may change, or says why it may not.
fn read_start(path: &Path, host: &Host) -> Result<Option<Read>, State> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(State::Unreadable),
        Ok(meta) => meta,
    };
    if meta.file_type().is_symlink() {
        return Err(State::SkippedSymlink);
    }
    if !meta.is_file() || meta.len() > MAX_RC {
        return Err(State::Unreadable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if host.uid.is_some_and(|uid| uid != meta.uid()) {
            return Err(State::SkippedOwner);
        }
    }
    #[cfg(not(unix))]
    let _ = host;
    let bytes = std::fs::read(path).map_err(|_| State::Unreadable)?;
    let text = String::from_utf8(bytes).map_err(|_| State::Unreadable)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = None;
    Ok(Some(Read {
        text,
        stamp: Some((meta.len(), meta.modified().ok())),
        mode,
    }))
}

/// How a write ended.
#[derive(Debug, PartialEq, Eq)]
enum Wrote {
    Done,
    /// The file changed (or appeared) between the read and the rename: left as it is.
    Changed,
    Failed,
}
/// Replaces `path` with `text` through a file in the same folder renamed over it, keeping its
/// mode, only when the file still has the stamp it was read with (`None`: it did not exist).
fn write_start(
    path: &Path,
    text: &str,
    read: Option<&Read>,
    before_rename: &mut dyn FnMut(&Path),
) -> Wrote {
    let Some(dir) = path.parent() else {
        return Wrote::Failed;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return Wrote::Failed;
    }
    let temporary = dir.join(format!(
        ".{}.switchboard-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("start"),
        uuid::Uuid::new_v4()
    ));
    let written = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(read.and_then(|r| r.mode).unwrap_or(0o644));
        }
        let mut file = options.open(&temporary)?;
        #[cfg(unix)]
        if let Some(mode) = read.and_then(|r| r.mode) {
            // The umask may have narrowed the create mode: the file keeps its own.
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all()
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Wrote::Failed;
    }
    before_rename(path);
    let now = std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.len(), m.modified().ok()));
    if now != read.and_then(|r| r.stamp) {
        let _ = std::fs::remove_file(&temporary);
        return Wrote::Changed;
    }
    if std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Wrote::Failed;
    }
    Wrote::Done
}

/// `text` with the block in `range` taken out; with `newline_added`, the line break the
/// installer put before a block at the end of the file goes too.
fn without(text: &str, range: std::ops::Range<usize>, newline_added: bool) -> String {
    let mut before = &text[..range.start];
    let after = &text[range.end..];
    if newline_added && after.is_empty() && before.ends_with('\n') {
        before = &before[..before.len() - 1];
    }
    format!("{before}{after}")
}

/// Brings every start file in line with the choice: written when the choice is `on`, or absent
/// while `organizations` holds; taken out otherwise. `explicit` (the person's own `shell on`)
/// writes again a block the person had taken out.
pub fn apply(root: &Path, host: &Host, organizations: bool, explicit: bool) -> Report {
    apply_with(root, host, organizations, explicit, &mut |_| {})
}
/// [`apply`], calling `before_rename` with each start file's path just before its rename (a
/// test changes the file there).
fn apply_with(
    root: &Path,
    host: &Host,
    organizations: bool,
    explicit: bool,
    before_rename: &mut dyn FnMut(&Path),
) -> Report {
    let wanted = match map::choice(root) {
        map::Choice::On => true,
        map::Choice::Off => false,
        map::Choice::Default => organizations,
    };
    let before = read_record(root);
    let mut report = Report {
        wanted,
        ..Report::default()
    };
    // Nothing ever written and nothing wanted: not even a record.
    if !wanted && before.files.is_empty() {
        return report;
    }
    let now = crate::monitor::now();
    let mut record = Record::default();
    let targets = targets(host);
    if wanted {
        for target in &targets {
            let entry = install_one(
                root,
                host,
                target,
                &before,
                explicit,
                before_rename,
                &mut report,
            );
            if let Some(entry) = entry {
                record.files.push(Entry { at: now, ..entry });
            }
        }
        report.first_written =
            report.written > 0 && !before.files.iter().any(|e| e.state == State::Written);
    } else {
        let removal = remove_from(host, &targets, &before, before_rename);
        report.removed = removal.removed;
        report.changed = removal.changed;
        report.skipped = removal.kept.len();
        record.files = removal.kept;
    }
    // Entries of start files that are no longer targets keep what they say.
    for old in &before.files {
        if !record.files.iter().any(|e| e.path == old.path)
            && !targets
                .iter()
                .any(|t| t.path.to_str() == Some(old.path.as_str()))
        {
            record.files.push(old.clone());
        }
    }
    write_record(root, &record);
    report
}

/// Puts the block into one start file. The entry to record, or None for a file that does not
/// exist and is not created (or changed while being written: left for the next run).
fn install_one(
    root: &Path,
    host: &Host,
    target: &Target,
    before: &Record,
    explicit: bool,
    before_rename: &mut dyn FnMut(&Path),
    report: &mut Report,
) -> Option<Entry> {
    let path_text = target.path.to_str()?.to_owned();
    let entry = |state: State, newline_added: bool| Entry {
        shell: target.kind.name().into(),
        path: path_text.clone(),
        state,
        newline_added,
        at: 0,
    };
    let previous = before.files.iter().find(|e| e.path == path_text);
    let hook = scripts::hook_path(root, target.kind);
    let (Some(cli), Some(block)) = (host.cli.as_deref(), block(target.kind, &hook)) else {
        report.no_hook = true;
        return previous.cloned();
    };
    if previous.is_some_and(|e| e.state == State::RemovedByPerson) && !explicit {
        report.skipped += 1;
        return previous.cloned();
    }
    let read = match read_start(&target.path, host) {
        Ok(read) => read,
        Err(state) => {
            report.skipped += 1;
            return Some(entry(state, false));
        }
    };
    let (next, newline_added) = match &read {
        None if !target.create => return None,
        None => (block.clone(), false),
        Some(read) => match find(&read.text) {
            Found::Broken => {
                report.skipped += 1;
                return Some(entry(State::Unreadable, false));
            }
            Found::Block(range) => {
                let mut next = read.text.clone();
                next.replace_range(range, &block);
                (next, previous.is_some_and(|e| e.newline_added))
            }
            // Written before and gone now: the person took it out (or deleted the file).
            Found::None if !explicit && previous.is_some_and(|e| e.state == State::Written) => {
                report.skipped += 1;
                return Some(entry(State::RemovedByPerson, false));
            }
            Found::None if target.own_file && !read.text.is_empty() => {
                // Switchboard's own file holds something else: the person's now.
                report.skipped += 1;
                return Some(entry(State::RemovedByPerson, false));
            }
            Found::None => {
                let gap = !read.text.is_empty() && !read.text.ends_with('\n');
                (
                    format!("{}{}{block}", read.text, if gap { "\n" } else { "" }),
                    gap,
                )
            }
        },
    };
    // A start file deleted by the person after Switchboard wrote it: theirs to want back.
    if read.is_none() && !explicit && previous.is_some_and(|e| e.state == State::Written) {
        report.skipped += 1;
        return Some(entry(State::RemovedByPerson, false));
    }
    if scripts::write(root, target.kind, cli).is_err() {
        report.no_hook = true;
        return previous.cloned();
    }
    if read.as_ref().is_some_and(|r| r.text == next) {
        report.written += 1;
        return Some(entry(State::Written, newline_added));
    }
    match write_start(&target.path, &next, read.as_ref(), before_rename) {
        Wrote::Done => {
            report.written += 1;
            Some(entry(State::Written, newline_added))
        }
        Wrote::Changed => {
            report.changed += 1;
            previous.cloned()
        }
        Wrote::Failed => {
            report.skipped += 1;
            Some(entry(State::Unreadable, false))
        }
    }
}

struct Removal {
    removed: usize,
    changed: usize,
    /// Entries still worth keeping: files that still hold a block Switchboard could not take
    /// out, and blocks the person took out.
    kept: Vec<Entry>,
}
/// Takes the block out of every target and every start file the record names.
fn remove_from(
    host: &Host,
    targets: &[Target],
    before: &Record,
    before_rename: &mut dyn FnMut(&Path),
) -> Removal {
    let mut out = Removal {
        removed: 0,
        changed: 0,
        kept: Vec::new(),
    };
    let mut paths: Vec<(PathBuf, bool, String)> = targets
        .iter()
        .map(|t| (t.path.clone(), t.own_file, t.kind.name().to_owned()))
        .collect();
    for e in &before.files {
        if !paths
            .iter()
            .any(|(p, _, _)| p.to_str() == Some(e.path.as_str()))
        {
            let own = Path::new(&e.path).file_name().and_then(|n| n.to_str()) == Some(FISH_FILE);
            paths.push((PathBuf::from(&e.path), own, e.shell.clone()));
        }
    }
    for (path, own_file, shell) in paths {
        let previous = before.files.iter().find(|e| Path::new(&e.path) == path);
        let keep = |state: State| Entry {
            shell: shell.clone(),
            path: path.to_string_lossy().into_owned(),
            state,
            newline_added: false,
            at: crate::monitor::now(),
        };
        if previous.is_some_and(|e| e.state == State::RemovedByPerson) {
            out.kept.extend(previous.cloned());
            continue;
        }
        let read = match read_start(&path, host) {
            Ok(Some(read)) => read,
            Ok(None) => continue,
            // A link or another user's file was never written: nothing to take out.
            Err(State::Unreadable) if previous.is_some_and(|e| e.state == State::Written) => {
                out.kept.push(keep(State::Unreadable));
                continue;
            }
            Err(_) => continue,
        };
        let range = match find(&read.text) {
            Found::None => continue,
            Found::Broken => {
                out.kept.push(keep(State::Unreadable));
                continue;
            }
            Found::Block(range) => range,
        };
        let newline_added = previous.is_some_and(|e| e.newline_added);
        let next = without(&read.text, range, newline_added);
        let result = if own_file && next.is_empty() {
            before_rename(&path);
            let now = std::fs::symlink_metadata(&path)
                .ok()
                .map(|m| (m.len(), m.modified().ok()));
            if now != read.stamp {
                Wrote::Changed
            } else if std::fs::remove_file(&path).is_ok() {
                Wrote::Done
            } else {
                Wrote::Failed
            }
        } else {
            write_start(&path, &next, Some(&read), before_rename)
        };
        match result {
            Wrote::Done => out.removed += 1,
            Wrote::Changed => {
                out.changed += 1;
                out.kept.extend(previous.cloned());
            }
            Wrote::Failed => out.kept.push(keep(State::Unreadable)),
        }
    }
    out
}

/// Takes every block out (`switchboard uninstall`), whatever the choice. Err: the files that
/// still hold one.
pub fn remove_all(root: &Path, host: &Host) -> Result<usize, Vec<String>> {
    let before = read_record(root);
    let removal = remove_from(host, &targets(host), &before, &mut |_| {});
    let left: Vec<String> = removal
        .kept
        .iter()
        .filter(|e| e.state != State::RemovedByPerson)
        .map(|e| e.path.clone())
        .collect();
    write_record(
        root,
        &Record {
            files: removal.kept,
        },
    );
    if left.is_empty() {
        Ok(removal.removed)
    } else {
        Err(left)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct World {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        host: Host,
    }
    fn uid() -> u32 {
        // SAFETY: getuid has no preconditions and cannot fail.
        unsafe { libc::getuid() }
    }
    /// A data folder and an injected home with a zsh login shell on macOS rules.
    fn world() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("data");
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        crate::launch::private_dir(&root).unwrap();
        let cli = base.join("bin").join("switchboard");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "#!/bin/sh\necho none\n").unwrap();
        World {
            _tmp: tmp,
            root,
            host: Host {
                home,
                zdotdir: None,
                login_shell: Some("zsh".into()),
                macos: true,
                uid: Some(uid()),
                cli: Some(cli),
            },
        }
    }
    fn zshrc(w: &World) -> PathBuf {
        w.host.home.join(".zshrc")
    }
    fn state_of(w: &World, path: &Path) -> Option<State> {
        read_record(&w.root)
            .files
            .into_iter()
            .find(|e| Path::new(&e.path) == path)
            .map(|e| e.state)
    }
    fn block_in(text: &str) -> bool {
        text.lines().any(|l| l == OPEN) && text.lines().any(|l| l == CLOSE)
    }

    #[test]
    fn the_block_goes_in_and_comes_out_leaving_the_file_byte_identical() {
        for original in [
            "export PATH=$HOME/bin:$PATH\nalias ll='ls -l'\n",
            // No final line break, CRLF lines, odd bytes of valid text.
            "setopt autocd\r\nexport X=1",
            "",
            "# ünïcödé, “quotes” and a tab\t\n\n\n",
        ] {
            let w = world();
            std::fs::write(zshrc(&w), original).unwrap();
            std::fs::set_permissions(zshrc(&w), std::fs::Permissions::from_mode(0o640)).unwrap();
            let report = set_and_apply(&w, None, true);
            assert!(report.wanted && report.written == 1, "{report:?}");
            assert!(report.first_written);
            let text = std::fs::read_to_string(zshrc(&w)).unwrap();
            assert!(text.starts_with(original), "nothing before the block moves");
            assert!(block_in(&text), "{text}");
            let hook = scripts::hook_path(&w.root, scripts::Kind::Zsh);
            assert!(text.contains(&format!(
                "if [ -r '{0}' ]; then . '{0}'; fi",
                hook.display()
            )));
            assert!(hook.is_file(), "the hook file the block reads");
            assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Written));
            let mode = std::fs::metadata(zshrc(&w)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o640, "the file's mode is kept");
            // Again: nothing changes.
            let again = apply(&w.root, &w.host, true, false);
            assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), text);
            assert!(!again.first_written);
            // Off: byte for byte what it was.
            set_choice(&w.root, false).unwrap();
            let off = apply(&w.root, &w.host, true, false);
            assert_eq!(off.removed, 1, "{off:?}");
            assert_eq!(std::fs::read(zshrc(&w)).unwrap(), original.as_bytes());
            // And off stays off, with an organization present.
            apply(&w.root, &w.host, true, false);
            assert_eq!(std::fs::read(zshrc(&w)).unwrap(), original.as_bytes());
        }
    }

    /// The choice as given (None: the default), then `apply`.
    fn set_and_apply(w: &World, choice: Option<bool>, organizations: bool) -> Report {
        if let Some(on) = choice {
            set_choice(&w.root, on).unwrap();
        }
        apply(&w.root, &w.host, organizations, false)
    }

    #[test]
    fn a_symlinked_foreign_owned_or_changed_rc_is_never_written() {
        // A link (a dotfile manager's), whatever it points to.
        let w = world();
        let target = w.host.home.join("dotfiles-zshrc");
        std::fs::write(&target, "mine\n").unwrap();
        std::os::unix::fs::symlink(&target, zshrc(&w)).unwrap();
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 0);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "mine\n");
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::SkippedSymlink));
        // Another user's file: the uid differs.
        let mut w = world();
        std::fs::write(zshrc(&w), "theirs\n").unwrap();
        w.host.uid = Some(uid() + 1);
        set_and_apply(&w, None, true);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "theirs\n");
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::SkippedOwner));
        // Not valid text.
        let w = world();
        std::fs::write(zshrc(&w), b"\xff\xfe binary\n").unwrap();
        set_and_apply(&w, None, true);
        assert_eq!(std::fs::read(zshrc(&w)).unwrap(), b"\xff\xfe binary\n");
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Unreadable));
        // Changed between the read and the rename: left as the person made it, written later.
        let w = world();
        std::fs::write(zshrc(&w), "before\n").unwrap();
        let path = zshrc(&w);
        let report = apply_with(&w.root, &w.host, true, false, &mut |p: &Path| {
            if p == path {
                std::fs::write(p, "edited by the person meanwhile, longer\n").unwrap();
            }
        });
        assert_eq!(report.changed, 1, "{report:?}");
        assert_eq!(
            std::fs::read_to_string(zshrc(&w)).unwrap(),
            "edited by the person meanwhile, longer\n"
        );
        assert_ne!(state_of(&w, &zshrc(&w)), Some(State::Written));
        let leftovers: Vec<_> = std::fs::read_dir(&w.host.home)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "no temporary file left: {leftovers:?}");
        let later = apply(&w.root, &w.host, true, false);
        assert_eq!(later.written, 1);
        assert!(std::fs::read_to_string(zshrc(&w))
            .unwrap()
            .starts_with("edited by the person meanwhile, longer\n"));
    }

    #[test]
    fn a_block_the_person_removed_is_not_written_back() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        set_and_apply(&w, None, true);
        // The person deletes the block by hand.
        std::fs::write(zshrc(&w), "own\n").unwrap();
        let report = apply(&w.root, &w.host, true, false);
        assert_eq!(report.written, 0);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::RemovedByPerson));
        // A restart, an organization coming and going: still not written back.
        apply(&w.root, &w.host, false, false);
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        // The person's own `shell on` writes it again.
        set_choice(&w.root, true).unwrap();
        let on = apply(&w.root, &w.host, false, true);
        assert_eq!(on.written, 1);
        assert!(block_in(&std::fs::read_to_string(zshrc(&w)).unwrap()));
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Written));
        // The fish file deleted by the person: the same.
        let w = world();
        std::fs::create_dir_all(w.host.home.join(".config/fish")).unwrap();
        set_and_apply(&w, None, true);
        let fish = w.host.home.join(".config/fish/conf.d").join(FISH_FILE);
        assert!(fish.is_file());
        std::fs::remove_file(&fish).unwrap();
        apply(&w.root, &w.host, true, false);
        assert!(!fish.exists());
        assert_eq!(state_of(&w, &fish), Some(State::RemovedByPerson));
    }

    #[test]
    fn default_on_only_on_an_installed_build_with_an_organization_group_and_off_wins() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        // No organization, no choice: nothing written, no record.
        let report = set_and_apply(&w, None, false);
        assert!(!report.wanted);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        assert!(!record_path(&w.root).exists());
        // An organization: written.
        assert_eq!(set_and_apply(&w, None, true).written, 1);
        // Off wins over the organization.
        set_and_apply(&w, Some(false), true);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        assert_eq!(map::choice(&w.root), map::Choice::Off);
        // On writes without one.
        assert_eq!(set_and_apply(&w, Some(true), false).written, 1);
        // No CLI to name: no hook and no block.
        let mut w = world();
        w.host.cli = None;
        let report = set_and_apply(&w, None, true);
        assert!(report.no_hook && report.written == 0, "{report:?}");
        assert!(!zshrc(&w).exists());
    }

    #[test]
    fn the_last_organization_group_leaving_removes_the_default_block() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        set_and_apply(&w, None, true);
        let report = apply(&w.root, &w.host, false, false);
        assert_eq!(report.removed, 1);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        // An explicit `on` keeps it.
        set_and_apply(&w, Some(true), true);
        apply(&w.root, &w.host, false, false);
        assert!(block_in(&std::fs::read_to_string(zshrc(&w)).unwrap()));
    }

    #[test]
    fn the_start_files_follow_the_shells_present() {
        // macOS bash: the first existing login file; zsh in ZDOTDIR; zsh created only as the
        // login shell.
        let mut w = world();
        w.host.login_shell = Some("bash".into());
        let zdot = w.host.home.join("zdot");
        std::fs::create_dir_all(&zdot).unwrap();
        w.host.zdotdir = Some(zdot.clone());
        std::fs::write(w.host.home.join(".profile"), "p\n").unwrap();
        std::fs::write(w.host.home.join(".bash_login"), "l\n").unwrap();
        apply(&w.root, &w.host, true, false);
        assert!(block_in(
            &std::fs::read_to_string(w.host.home.join(".bash_login")).unwrap()
        ));
        assert_eq!(
            std::fs::read_to_string(w.host.home.join(".profile")).unwrap(),
            "p\n"
        );
        assert!(!zdot.join(".zshrc").exists(), "zsh is not the login shell");
        assert!(!w.host.home.join(".bash_profile").exists());
        // Linux bash: `.bashrc`; zsh's existing file in ZDOTDIR is written.
        let mut w = world();
        w.host.macos = false;
        w.host.login_shell = Some("bash".into());
        std::fs::write(w.host.home.join(".bash_profile"), "x\n").unwrap();
        let zdot = w.host.home.join("zdot");
        std::fs::create_dir_all(&zdot).unwrap();
        std::fs::write(zdot.join(".zshrc"), "z\n").unwrap();
        w.host.zdotdir = Some(zdot.clone());
        apply(&w.root, &w.host, true, false);
        assert!(block_in(
            &std::fs::read_to_string(w.host.home.join(".bashrc")).unwrap()
        ));
        assert_eq!(
            std::fs::read_to_string(w.host.home.join(".bash_profile")).unwrap(),
            "x\n"
        );
        assert!(block_in(
            &std::fs::read_to_string(zdot.join(".zshrc")).unwrap()
        ));
        // Created files get the usual mode, not the data folder's.
        let mode = std::fs::metadata(w.host.home.join(".bashrc"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o600, 0o600);
    }

    #[test]
    fn a_block_with_a_changed_marker_is_the_persons_and_never_touched() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        set_and_apply(&w, None, true);
        // The closing marker edited away: neither rewritten nor taken out.
        let text = std::fs::read_to_string(zshrc(&w))
            .unwrap()
            .replace(CLOSE, "# <<< my edit <<<");
        std::fs::write(zshrc(&w), &text).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), text);
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Unreadable));
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), text);
        assert_eq!(remove_all(&w.root, &w.host).unwrap_err().len(), 1);
        // The opening marker edited: the block is no longer ours; never written back.
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        set_and_apply(&w, None, true);
        let text = std::fs::read_to_string(zshrc(&w))
            .unwrap()
            .replace(OPEN, "# >>> mine >>>");
        std::fs::write(zshrc(&w), &text).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), text);
        // The person's own lines inside the markers are replaced with Switchboard's.
        let w = world();
        set_and_apply(&w, None, true);
        let ours = std::fs::read_to_string(zshrc(&w)).unwrap();
        let tampered = ours.replace("if [ -r", "echo hi; if [ -r");
        std::fs::write(zshrc(&w), &tampered).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), ours);
    }

    #[test]
    fn remove_all_takes_every_block_out_whatever_the_choice() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        std::fs::create_dir_all(w.host.home.join(".config/fish")).unwrap();
        set_and_apply(&w, Some(true), false);
        let fish = w.host.home.join(".config/fish/conf.d").join(FISH_FILE);
        assert!(fish.is_file());
        assert_eq!(remove_all(&w.root, &w.host), Ok(2));
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        assert!(!fish.exists());
        // A fish file the person edited stays; only Switchboard's block goes.
        set_and_apply(&w, Some(true), false);
        let mut text = std::fs::read_to_string(&fish).unwrap();
        text.push_str("set -gx MINE 1\n");
        std::fs::write(&fish, &text).unwrap();
        remove_all(&w.root, &w.host).unwrap();
        assert_eq!(std::fs::read_to_string(&fish).unwrap(), "set -gx MINE 1\n");
    }

    #[test]
    fn a_data_folder_that_is_not_text_writes_no_block() {
        use std::os::unix::ffi::OsStrExt;
        let w = world();
        // macOS's file system refuses such a name, so the folder is not created: the installer
        // must refuse before it touches anything.
        let bad = w.root.join(std::ffi::OsStr::from_bytes(b"\xffdata"));
        std::fs::write(zshrc(&w), "own\n").unwrap();
        let report = apply(&bad, &w.host, true, false);
        assert!(report.no_hook);
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
    }
}
