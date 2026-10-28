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
    /// PowerShell's profiles and execution policies (node A-8): read on Windows, None
    /// elsewhere (a test gives one to install into an injected Documents folder).
    pub windows: Option<super::windows::Host>,
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
        #[cfg(windows)]
        let windows = super::windows::process_host();
        #[cfg(not(windows))]
        let windows = None;
        Some(Host {
            home,
            zdotdir,
            login_shell,
            macos: cfg!(target_os = "macos"),
            uid,
            cli,
            windows,
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
    /// zsh's start file is missing and may live in another folder (`ZDOTDIR` set where this
    /// process cannot see it): never created.
    zdotdir_unknown: bool,
    /// A PowerShell profile whose execution policy keeps it from running: never written.
    policy_blocked: bool,
    /// PowerShell runs in Constrained Language Mode on this computer, where the hook cannot
    /// act: never written.
    constrained: bool,
}

/// The start files of the shells present for `host`: on Windows the PowerShell profiles only
/// (`cmd.exe` and Git Bash are not covered), elsewhere zsh, bash and fish, plus the PowerShell
/// profiles a test gives.
fn targets(host: &Host) -> Vec<Target> {
    let mut out = if cfg!(windows) {
        Vec::new()
    } else {
        posix_targets(host)
    };
    // Each edition's profile, created when absent (Windows PowerShell is part of Windows, and
    // PowerShell 7 counts only when installed or its folder exists), as both read it.
    for profile in host.windows.iter().flat_map(|w| w.profiles()) {
        out.push(Target {
            kind: scripts::Kind::Pwsh,
            path: profile.path,
            own_file: false,
            create: true,
            zdotdir_unknown: false,
            policy_blocked: profile.blocked,
            constrained: profile.constrained,
        });
    }
    out
}
fn posix_targets(host: &Host) -> Vec<Target> {
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
            zdotdir_unknown: host.zdotdir.is_none() && !exists(&zsh) && zdotdir_may_move(host),
            path: zsh,
            own_file: false,
            policy_blocked: false,
            constrained: false,
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
            zdotdir_unknown: false,
            path: bash,
            own_file: false,
            policy_blocked: false,
            constrained: false,
        });
    }
    let fish = host.home.join(".config").join("fish");
    if fish.is_dir() {
        out.push(Target {
            kind: scripts::Kind::Fish,
            path: fish.join("conf.d").join(FISH_FILE),
            own_file: true,
            create: true,
            zdotdir_unknown: false,
            policy_blocked: false,
            constrained: false,
        });
    }
    out
}

/// Whether zsh may read its start files from another folder than the home this process sees.
/// An app started from the Dock or at login does not inherit the `ZDOTDIR` a person exports in
/// `~/.zshenv`, so any `~/.zshenv` counts (it may set it, or source a file that does), and a
/// `~/.zprofile` that names `ZDOTDIR` or cannot be read.
fn zdotdir_may_move(host: &Host) -> bool {
    if std::fs::symlink_metadata(host.home.join(".zshenv")).is_ok() {
        return true;
    }
    let zprofile = host.home.join(".zprofile");
    match std::fs::symlink_metadata(&zprofile) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
        Ok(_) => std::fs::metadata(&zprofile)
            .ok()
            .filter(|m| m.is_file() && m.len() <= MAX_RC)
            .and_then(|_| std::fs::read(&zprofile).ok())
            .is_none_or(|bytes| bytes.windows(7).any(|w| w == b"ZDOTDIR")),
    }
}

/// Whether `root` is the app's own data folder (`default`, from `crate::default_root`): only
/// that one may change the person's start files, since every data folder would write the same
/// marker into the same home.
pub fn same_root(root: &Path, default: Option<&Path>) -> bool {
    let Some(default) = default else {
        return false;
    };
    root == default
        || matches!(
            (root.canonicalize(), default.canonicalize()),
            (Ok(a), Ok(b)) if a == b
        )
}
/// [`same_root`] against this process's default data folder.
pub fn is_default_root(root: &Path) -> bool {
    same_root(root, crate::default_root().ok().as_deref())
}
/// Whether `exe` is an app installed where people keep apps: on macOS inside `/Applications`
/// or `~/Applications` (never a bundle opened from a download or build folder, nor a
/// translocated copy); elsewhere any release build the app's caller calls installed.
pub fn installed_app(exe: &Path, home: Option<&Path>) -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    let under = |folder: &Path| {
        exe.strip_prefix(folder).is_ok_and(|rest| {
            rest.components()
                .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
        })
    };
    under(Path::new("/Applications")) || home.is_some_and(|h| under(&h.join("Applications")))
}
/// The CLI a hook names: [`crate::launch::agent_cli`] in a release build; a development build
/// never names itself (its file goes with the next `cargo clean`), only an installed copy found
/// the same way. None: no CLI to name.
pub fn hook_cli() -> Option<PathBuf> {
    let cli = if !cfg!(debug_assertions) {
        crate::launch::agent_cli()
    } else {
        let current = std::env::current_exe().ok()?.canonicalize().ok()?;
        crate::launch::installed_cli(&current)
    };
    cli.map(|p| plain_path(&p))
}

/// `path` as a script names it: without the `\\?\` prefix a canonical Windows path carries
/// (`\\?\UNC\server\share` becomes `\\server\share`), which PowerShell's own path
/// handling does not read as a plain file system path.
pub(crate) fn plain_path(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{rest}"));
    }
    PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(text))
}

/// What the installer did to one start file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Written,
    RemovedByPerson,
    SkippedSymlink,
    SkippedOwner,
    /// zsh's start file was not created: zsh may read it from another folder (`ZDOTDIR` set in
    /// `~/.zshenv` or `~/.zprofile`, which an app started from the Dock does not see).
    SkippedZdotdirUnknown,
    Unreadable,
    /// A PowerShell profile whose execution policy (`Restricted` or `AllSigned`) keeps it, or
    /// the unsigned hook, from running: nothing written, the policy never changed (A-Q4).
    PolicyBlocked,
    /// PowerShell runs in Constrained Language Mode on this computer (Windows Defender
    /// Application Control, or `__PSLockdownPolicy`), where the hook cannot act: nothing
    /// written, the mode never changed (SB-225).
    ConstrainedLanguage,
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
    /// The installer created the file: taking the block out deletes it when nothing else is
    /// left in it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub created: bool,
    #[serde(default)]
    pub at: i64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub files: Vec<Entry>,
    /// The first-time notice the window has not acknowledged yet: kept across restarts, dropped
    /// when the hook is no longer wanted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice_pending: Option<Notice>,
}
/// What the window's first-time notice names: the organization and its group whose presence
/// wrote the first block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub organization: String,
    pub group: String,
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

/// Saves the first-time notice for the window, while a block is written (the owner, after the
/// run that wrote the first one).
pub fn set_notice(root: &Path, notice: Notice) {
    let _lock = run_lock(root);
    let mut record = read_record(root);
    if record.files.iter().any(|e| e.state == State::Written) {
        record.notice_pending = Some(notice);
        write_record(root, &record);
    }
}
/// The first-time notice the window has not acknowledged.
pub fn pending_notice(root: &Path) -> Option<Notice> {
    read_record(root).notice_pending
}
/// The window showed the notice: it is not shown again.
pub fn acknowledge_notice(root: &Path) {
    let _lock = run_lock(root);
    let mut record = read_record(root);
    if record.notice_pending.take().is_some() {
        write_record(root, &record);
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
/// The window's switch in an app that is not the installed one (a development build, `serve`,
/// another data folder): nothing changes.
pub const NOT_INSTALLED: &str = "Only the installed app changes terminal settings.";
/// `shell on` when the hook cannot name the CLI or the data folder.
pub const NO_PATH: &str =
    "The hook cannot name this executable or the data folder: a path is not absolute or not valid text.";

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
        scripts::Kind::Pwsh => ps_line(hook),
    };
    Some(format!("{OPEN}\n{line}\n{CLOSE}\n"))
}

/// The PowerShell block's line. Windows PowerShell 5.1 reads a profile without a byte order
/// mark in the system's code page, so the line stays ASCII: a hook path that is not ASCII (a
/// user name in another script) is carried as base64 of its UTF-8 and read inside a script
/// block, which leaves no variable behind (the hook keeps everything in global variables).
fn ps_line(hook: &str) -> String {
    if hook.is_ascii() {
        let q = scripts::ps_quote(hook);
        return format!("if (Test-Path -LiteralPath {q}) {{ . {q} }}");
    }
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(hook.as_bytes());
    format!(
        "& {{ $f = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')); if (Test-Path -LiteralPath $f) {{ . $f }} }}"
    )
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

/// How a start file's text is stored. Windows PowerShell 5.1 writes UTF-16LE with a byte order
/// mark (`>` and `Out-File`), so a profile may be either; it is written back as it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Utf16Le,
}
impl Encoding {
    fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Encoding::Utf8 => text.as_bytes().to_vec(),
            Encoding::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        }
    }
}
/// A start file's bytes as text. A PowerShell profile (`profile`) is UTF-8 (a byte order mark
/// kept as `U+FEFF`) or UTF-16LE when it starts with that byte order mark (the mark kept the same
/// way, so the bytes come back exactly); None for anything else, and for text holding a NUL:
/// UTF-16 without a byte order mark reads as valid UTF-8 full of NULs, and a block written into it
/// would garble it. Any other start file (zsh, bash, fish) is UTF-8, a NUL included, as before
/// SB-225: a block written into one earlier still comes out.
fn decode(bytes: &[u8], profile: bool) -> Option<(String, Encoding)> {
    if !profile {
        return String::from_utf8(bytes.to_vec())
            .ok()
            .map(|text| (text, Encoding::Utf8));
    }
    if bytes.starts_with(b"\xff\xfe") {
        let (pairs, rest) = bytes.as_chunks::<2>();
        if !rest.is_empty() {
            return None;
        }
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        let text = String::from_utf16(&units).ok()?;
        return (!text.contains('\0')).then_some((text, Encoding::Utf16Le));
    }
    let text = String::from_utf8(bytes.to_vec()).ok()?;
    (!text.contains('\0')).then_some((text, Encoding::Utf8))
}

/// The start file as read: its text and the stamp the write compares against.
struct Read {
    text: String,
    encoding: Encoding,
    stamp: Option<(u64, Option<std::time::SystemTime>)>,
    /// Read only where permission bits exist (unix).
    #[cfg_attr(not(unix), allow(dead_code))]
    mode: Option<u32>,
}
/// Reads a start file the installer may change, or says why it may not.
/// `utf16`: a PowerShell profile, which may be UTF-16LE; no other start file is read as such.
fn read_start(path: &Path, host: &Host, utf16: bool) -> Result<Option<Read>, State> {
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
    let (text, encoding) = decode(&bytes, utf16).ok_or(State::Unreadable)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = None;
    Ok(Some(Read {
        text,
        encoding,
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
/// mode and its encoding, only when the file still has the stamp it was read with (`None`: it
/// did not exist, and is written as UTF-8).
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
        let encoding = read.map_or(Encoding::Utf8, |r| r.encoding);
        file.write_all(&encoding.encode(text))?;
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

/// An advisory `flock` on the data folder itself, held for a whole run, so the owner's run and
/// `switchboard shell on|off` or `uninstall` from another process never interleave between a
/// start file's read and its rename. The folder, not a file in it: the record is replaced by a
/// rename, which a lock on it would not survive, and a lock file would be one more thing in a
/// data folder that may never have had an organization's group (or one `uninstall` empties).
/// The store's own lock is `instance.lock`, a different file, so the two never meet.
struct RunLock(#[allow(dead_code)] Option<std::fs::File>);
/// Waits for the lock; a data folder that cannot be opened runs without it (nothing can be
/// written there either).
fn run_lock(root: &Path) -> RunLock {
    let Ok(folder) = std::fs::File::open(root) else {
        return RunLock(None);
    };
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        loop {
            // SAFETY: the descriptor is open for the call; the lock goes with the file.
            if unsafe { libc::flock(folder.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return RunLock(None);
            }
        }
    }
    RunLock(Some(folder))
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
    let _lock = run_lock(root);
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
    let mut record = Record {
        notice_pending: before.notice_pending.clone().filter(|_| wanted),
        ..Record::default()
    };
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
        created: false,
        at: 0,
    };
    let ours = |newline_added: bool, created: bool| Entry {
        created,
        ..entry(State::Written, newline_added)
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
    // The execution policy is read, never changed (A-Q4): the profile is left as it is, a block
    // in it from an earlier run included, until the policy lets it run.
    // What an earlier run did to the file (a line break added, the file created) stays in the
    // record, so `shell off` and a later relaxed policy still give it back.
    // Constrained Language Mode likewise (SB-225): the hook would load and do nothing.
    if target.policy_blocked || target.constrained {
        report.skipped += 1;
        let state = if target.constrained {
            State::ConstrainedLanguage
        } else {
            State::PolicyBlocked
        };
        return Some(Entry {
            created: previous.is_some_and(|e| e.created),
            ..entry(state, previous.is_some_and(|e| e.newline_added))
        });
    }
    let read = match read_start(&target.path, host, target.kind == scripts::Kind::Pwsh) {
        Ok(read) => read,
        Err(state) => {
            report.skipped += 1;
            return Some(entry(state, false));
        }
    };
    let (next, newline_added) = match &read {
        None if !target.create => return None,
        None if target.zdotdir_unknown => {
            report.skipped += 1;
            return Some(entry(State::SkippedZdotdirUnknown, false));
        }
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
    // Created now, or by an earlier run: given back by deleting it when only the block is left.
    let created = read.is_none() || previous.is_some_and(|e| e.created);
    if read.as_ref().is_some_and(|r| r.text == next) {
        report.written += 1;
        return Some(ours(newline_added, created));
    }
    match write_start(&target.path, &next, read.as_ref(), before_rename) {
        Wrote::Done => {
            report.written += 1;
            Some(ours(newline_added, created))
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
            created: false,
            at: crate::monitor::now(),
        };
        if previous.is_some_and(|e| e.state == State::RemovedByPerson) {
            out.kept.extend(previous.cloned());
            continue;
        }
        let read = match read_start(&path, host, shell == scripts::Kind::Pwsh.name()) {
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
        // Switchboard's own file, or a start file the installer created: deleted when nothing
        // else is left in it, so the home is as it was.
        let created = own_file || previous.is_some_and(|e| e.created);
        let result = if created && next.is_empty() {
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
    let _lock = run_lock(root);
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
            notice_pending: None,
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
                windows: None,
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
        assert_eq!(set_and_apply(&w, Some(true), true).written, 1);
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

    /// A GUI app does not see the `ZDOTDIR` a person sets in `~/.zshenv`: a `~/.zshrc` created
    /// there would never be read, so none is created and the record says why.
    #[test]
    fn no_zshrc_is_created_when_zshenv_may_move_zdotdir() {
        let w = world();
        std::fs::write(
            w.host.home.join(".zshenv"),
            "export ZDOTDIR=\"$HOME/.config/zsh\"\n",
        )
        .unwrap();
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 0, "{report:?}");
        assert!(!zshrc(&w).exists(), "no start file zsh may never read");
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::SkippedZdotdirUnknown));
        // Any `.zshenv` may set it (or source a file that does): not created either.
        let w = world();
        std::fs::write(w.host.home.join(".zshenv"), ". \"$HOME/.cargo/env\"\n").unwrap();
        assert_eq!(set_and_apply(&w, None, true).written, 0);
        assert!(!zshrc(&w).exists());
        // A `.zprofile` naming ZDOTDIR: not created.
        let w = world();
        std::fs::write(w.host.home.join(".zprofile"), "export ZDOTDIR=~/z\n").unwrap();
        assert_eq!(set_and_apply(&w, None, true).written, 0);
        assert!(!zshrc(&w).exists());
        // A `.zprofile` that does not (Homebrew's line) leaves zsh reading `~/.zshrc`: created.
        let w = world();
        std::fs::write(
            w.host.home.join(".zprofile"),
            "eval \"$(/opt/homebrew/bin/brew shellenv)\"\n",
        )
        .unwrap();
        assert_eq!(set_and_apply(&w, Some(true), true).written, 1);
        assert!(block_in(&std::fs::read_to_string(zshrc(&w)).unwrap()));
        // Once the person makes their own `~/.zshrc`, the next run writes into it.
        let w = world();
        std::fs::write(w.host.home.join(".zshenv"), "export ZDOTDIR=$HOME\n").unwrap();
        set_and_apply(&w, None, true);
        std::fs::write(zshrc(&w), "mine\n").unwrap();
        assert_eq!(apply(&w.root, &w.host, true, false).written, 1);
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Written));
        // An existing `~/.zshrc` is written whatever `.zshenv` says.
        let w = world();
        std::fs::write(w.host.home.join(".zshenv"), "export ZDOTDIR=$HOME/z\n").unwrap();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        assert_eq!(set_and_apply(&w, Some(true), true).written, 1);
    }

    /// A start file the installer created is given back too: deleted when nothing of the
    /// person's is left in it.
    #[test]
    fn a_start_file_the_installer_created_is_deleted_when_the_block_goes() {
        let mut w = world();
        w.host.login_shell = Some("bash".into());
        let profile = w.host.home.join(".bash_profile");
        assert!(!profile.exists());
        assert_eq!(set_and_apply(&w, Some(true), true).written, 1);
        assert!(block_in(&std::fs::read_to_string(&profile).unwrap()));
        assert!(read_record(&w.root).files.iter().all(|e| e.created));
        set_choice(&w.root, false).unwrap();
        let off = apply(&w.root, &w.host, true, false);
        assert_eq!(off.removed, 1, "{off:?}");
        assert!(!profile.exists(), "the file the installer created is gone");
        // One the person wrote into meanwhile keeps their lines.
        let mut w = world();
        w.host.login_shell = Some("bash".into());
        let profile = w.host.home.join(".bash_profile");
        set_and_apply(&w, None, true);
        let mut text = std::fs::read_to_string(&profile).unwrap();
        text.push_str("export MINE=1\n");
        std::fs::write(&profile, &text).unwrap();
        assert_eq!(remove_all(&w.root, &w.host), Ok(1));
        assert_eq!(
            std::fs::read_to_string(&profile).unwrap(),
            "export MINE=1\n"
        );
        // A file the person had is never deleted, even when only the block was in it.
        let mut w = world();
        w.host.login_shell = Some("bash".into());
        let profile = w.host.home.join(".bash_profile");
        std::fs::write(&profile, "").unwrap();
        set_and_apply(&w, None, true);
        assert_eq!(remove_all(&w.root, &w.host), Ok(1));
        assert_eq!(std::fs::read_to_string(&profile).unwrap(), "");
    }

    /// The owner's run and `shell on|off` never interleave: a second run waits for the first.
    #[test]
    fn a_second_run_waits_for_the_first() {
        let w = world();
        std::fs::write(zshrc(&w), "own\n").unwrap();
        let path = zshrc(&w);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let mut other: Option<std::thread::JoinHandle<()>> = None;
        let report = apply_with(&w.root, &w.host, true, false, &mut |p: &Path| {
            if p != path || other.is_some() {
                return;
            }
            // `shell off` from another process while this run is between its read and rename.
            let (root, host, done) = (w.root.clone(), w.host.clone(), done_tx.clone());
            other = Some(std::thread::spawn(move || {
                set_choice(&root, false).unwrap();
                apply(&root, &host, true, false);
                done.send(()).unwrap();
            }));
            assert!(
                done_rx
                    .recv_timeout(std::time::Duration::from_millis(500))
                    .is_err(),
                "the second run went on while the first held the start files"
            );
        });
        assert_eq!(report.written, 1, "{report:?}");
        other.unwrap().join().unwrap();
        // The `off` ran after the `on`: the file is back as it was and the record agrees.
        assert_eq!(std::fs::read_to_string(zshrc(&w)).unwrap(), "own\n");
        assert!(read_record(&w.root)
            .files
            .iter()
            .all(|e| e.state != State::Written));
    }

    #[test]
    fn only_the_apps_own_data_folder_and_an_installed_app_touch_the_home() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().canonicalize().unwrap().join("a");
        std::fs::create_dir_all(&a).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&a, &link).unwrap();
        assert!(same_root(&a, Some(&a)));
        assert!(same_root(&link, Some(&a)), "the same folder through a link");
        assert!(!same_root(&tmp.path().join("b"), Some(&a)));
        assert!(!same_root(&a, None));
        let home = Path::new("/Users/p");
        let app = |dir: &str| PathBuf::from(dir).join("Fabric Switchboard.app/Contents/MacOS/app");
        if cfg!(target_os = "macos") {
            assert!(installed_app(&app("/Applications"), Some(home)));
            assert!(installed_app(&app("/Users/p/Applications"), Some(home)));
            assert!(installed_app(&app("/Applications/Utilities"), Some(home)));
            assert!(!installed_app(&app("/Users/p/Downloads"), Some(home)));
            assert!(!installed_app(
                &app("/private/var/folders/x/T/AppTranslocation/0A1B"),
                Some(home)
            ));
            assert!(!installed_app(
                &app("/Users/p/src/target/release/bundle/macos"),
                Some(home)
            ));
            assert!(!installed_app(&app("/Users/q/Applications"), Some(home)));
            assert!(!installed_app(&app("/Users/p/Applications"), None));
        } else {
            assert!(installed_app(
                Path::new("/usr/bin/fabric-switchboard"),
                Some(home)
            ));
        }
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

    // PowerShell (node A-8): the same installer over an injected Documents folder.

    use super::super::windows::{self, Edition, Policy};
    /// A world without zsh or bash, with both PowerShell editions under `<home>/Documents`.
    fn pwsh_world(desktop: Policy, core: Policy) -> World {
        let mut w = world();
        w.host.login_shell = None;
        w.host.windows = Some(windows::Host {
            documents: w.host.home.join("Documents"),
            editions: vec![(Edition::Desktop, desktop), (Edition::Core, core)],
            constrained: windows::Lockdown::default(),
        });
        w
    }
    fn profile(w: &World, edition: Edition) -> PathBuf {
        windows::profile_path(&w.host.home.join("Documents"), edition)
    }

    #[test]
    fn a_powershell_profile_gets_the_block_and_gives_it_back_byte_identical() {
        // No final line break and CRLF lines, as Notepad leaves a profile.
        let original = "Set-Alias ll Get-ChildItem\r\n# mine, “quoted”";
        let w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let desktop = profile(&w, Edition::Desktop);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, original).unwrap();
        let report = set_and_apply(&w, None, true);
        assert!(report.wanted && report.written == 2, "{report:?}");
        assert!(report.first_written);
        let hook = scripts::hook_path(&w.root, scripts::Kind::Pwsh);
        assert_eq!(hook.file_name().unwrap(), "hook.ps1");
        let line = format!(
            "if (Test-Path -LiteralPath '{0}') {{ . '{0}' }}",
            hook.display()
        );
        let text = std::fs::read_to_string(&desktop).unwrap();
        assert_eq!(text, format!("{original}\n{OPEN}\n{line}\n{CLOSE}\n"));
        // PowerShell 7's profile did not exist: created, with its folder, holding the block.
        let core = profile(&w, Edition::Core);
        assert_eq!(
            std::fs::read_to_string(&core).unwrap(),
            format!("{OPEN}\n{line}\n{CLOSE}\n")
        );
        // The hook file, with the byte order mark Windows PowerShell 5.1 needs.
        let bytes = std::fs::read(&hook).unwrap();
        assert!(bytes.starts_with("\u{feff}# Fabric Switchboard".as_bytes()));
        for path in [&desktop, &core] {
            assert_eq!(
                state_of(&w, path),
                Some(State::Written),
                "{}",
                path.display()
            );
        }
        let record = read_record(&w.root);
        assert!(record.files.iter().all(|e| e.shell == "pwsh"), "{record:?}");
        // Again: nothing changes.
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), text);
        // Off: the person's profile byte for byte, the created one gone, its folder kept.
        set_choice(&w.root, false).unwrap();
        let off = apply(&w.root, &w.host, true, false);
        assert_eq!(off.removed, 2, "{off:?}");
        assert_eq!(std::fs::read(&desktop).unwrap(), original.as_bytes());
        assert!(!core.exists());
        assert!(core.parent().unwrap().is_dir());
    }

    #[test]
    fn a_powershell_block_the_person_removed_is_not_written_back() {
        let w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let desktop = profile(&w, Edition::Desktop);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, "$env:EDITOR = 'code'\n").unwrap();
        set_and_apply(&w, None, true);
        assert!(block_in(&std::fs::read_to_string(&desktop).unwrap()));
        // The person takes it out, and adds a line of their own.
        std::fs::write(
            &desktop,
            "$env:EDITOR = 'code'\nSet-PSReadLineOption -EditMode Emacs\n",
        )
        .unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(
            std::fs::read_to_string(&desktop).unwrap(),
            "$env:EDITOR = 'code'\nSet-PSReadLineOption -EditMode Emacs\n"
        );
        assert_eq!(state_of(&w, &desktop), Some(State::RemovedByPerson));
        // PowerShell 7's created profile is left as it is: still written.
        assert_eq!(
            state_of(&w, &profile(&w, Edition::Core)),
            Some(State::Written)
        );
        // `shell on` writes it again, after the person's lines.
        apply(&w.root, &w.host, true, true);
        let text = std::fs::read_to_string(&desktop).unwrap();
        assert!(text.starts_with("$env:EDITOR = 'code'\nSet-PSReadLineOption -EditMode Emacs\n"));
        assert!(block_in(&text));
        // A linked profile (a dotfile manager's) is never written.
        let w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let core = profile(&w, Edition::Core);
        std::fs::create_dir_all(core.parent().unwrap()).unwrap();
        let target = w.host.home.join("dotfiles-profile.ps1");
        std::fs::write(&target, "mine\n").unwrap();
        std::os::unix::fs::symlink(&target, &core).unwrap();
        set_and_apply(&w, None, true);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "mine\n");
        assert_eq!(state_of(&w, &core), Some(State::SkippedSymlink));
    }

    #[test]
    fn a_blocked_policy_leaves_the_profile_alone_and_records_policy_blocked() {
        let w = pwsh_world(Policy::Restricted, Policy::AllSigned);
        let desktop = profile(&w, Edition::Desktop);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, "# mine\n").unwrap();
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 0, "{report:?}");
        assert!(!report.first_written);
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), "# mine\n");
        assert!(!profile(&w, Edition::Core).exists(), "nothing created");
        for edition in Edition::ALL {
            assert_eq!(
                state_of(&w, &profile(&w, edition)),
                Some(State::PolicyBlocked)
            );
        }
        // What the window, MCP and `shell status` read.
        let status = map::status(Some(&w.root), None, &|_| None);
        let states: Vec<&str> = status["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["policy_blocked", "policy_blocked"], "{status}");
        // Whoever manages the computer allows local scripts: the next run writes the block.
        let mut w = w;
        w.host.windows = Some(windows::Host {
            documents: w.host.home.join("Documents"),
            editions: vec![
                (Edition::Desktop, Policy::RemoteSigned),
                (Edition::Core, Policy::AllSigned),
            ],
            constrained: windows::Lockdown::default(),
        });
        let report = apply(&w.root, &w.host, true, false);
        assert_eq!(report.written, 1, "{report:?}");
        assert!(block_in(&std::fs::read_to_string(&desktop).unwrap()));
        assert_eq!(state_of(&w, &desktop), Some(State::Written));
        assert_eq!(
            state_of(&w, &profile(&w, Edition::Core)),
            Some(State::PolicyBlocked)
        );
        // Off takes the written block out; the blocked profile was never touched.
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), "# mine\n");
        assert!(!profile(&w, Edition::Core).exists());
    }

    /// A profile written while scripts ran, then blocked by a stricter policy: the record keeps
    /// what Switchboard did to it, so `shell off` (during the block or after it is relaxed) gives
    /// back the person's bytes and deletes the profile Switchboard created.
    #[test]
    fn a_policy_that_turns_blocked_keeps_what_off_gives_back() {
        let mut w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let original = "Set-Alias ll Get-ChildItem";
        let desktop = profile(&w, Edition::Desktop);
        let core = profile(&w, Edition::Core);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, original).unwrap();
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 2, "{report:?}");
        assert!(core.is_file());
        let blocked = |w: &mut World, policy: Policy| {
            w.host.windows = Some(windows::Host {
                documents: w.host.home.join("Documents"),
                editions: vec![(Edition::Desktop, policy), (Edition::Core, policy)],
                constrained: windows::Lockdown::default(),
            });
        };
        // Blocked: both profiles keep their block untouched, recorded as policy_blocked.
        blocked(&mut w, Policy::AllSigned);
        apply(&w.root, &w.host, true, false);
        for path in [&desktop, &core] {
            assert_eq!(state_of(&w, path), Some(State::PolicyBlocked));
            assert!(block_in(&std::fs::read_to_string(path).unwrap()));
        }
        // Off while blocked: the person's bytes back, the created profile gone.
        set_choice(&w.root, false).unwrap();
        let off = apply(&w.root, &w.host, true, false);
        assert_eq!(off.removed, 2, "{off:?}");
        assert_eq!(std::fs::read(&desktop).unwrap(), original.as_bytes());
        assert!(!core.exists());

        // On again, blocked, then relaxed: the flags survive the blocked run, so off still
        // gives the same back.
        set_choice(&w.root, true).unwrap();
        blocked(&mut w, Policy::RemoteSigned);
        apply(&w.root, &w.host, true, false);
        blocked(&mut w, Policy::Restricted);
        apply(&w.root, &w.host, true, false);
        blocked(&mut w, Policy::RemoteSigned);
        apply(&w.root, &w.host, true, false);
        for path in [&desktop, &core] {
            assert_eq!(state_of(&w, path), Some(State::Written));
        }
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read(&desktop).unwrap(), original.as_bytes());
        assert!(!core.exists());
    }

    /// SB-225: Windows PowerShell 5.1 writes UTF-16LE with a byte order mark (`>`, `Out-File`).
    /// Such a profile gets the block in UTF-16LE and gives the same bytes back; one in UTF-16
    /// without the mark (valid UTF-8 full of NULs) is left alone and reads `unreadable`.
    #[test]
    fn a_utf16_profile_is_written_in_its_own_encoding_and_given_back_byte_identical() {
        let utf16 =
            |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        let w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let desktop = profile(&w, Edition::Desktop);
        let core = profile(&w, Edition::Core);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::create_dir_all(core.parent().unwrap()).unwrap();
        let original = utf16("\u{feff}Set-Alias ll Get-ChildItem\r\n# Сергей's profile\r\n");
        std::fs::write(&desktop, &original).unwrap();
        let unmarked = utf16("Set-Alias ll Get-ChildItem\r\n");
        std::fs::write(&core, &unmarked).unwrap();
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 1, "{report:?}");
        let bytes = std::fs::read(&desktop).unwrap();
        assert!(bytes.starts_with(b"\xff\xfe"), "still UTF-16LE");
        let (text, encoding) = decode(&bytes, true).unwrap();
        assert_eq!(encoding, Encoding::Utf16Le);
        assert!(text.starts_with("\u{feff}Set-Alias ll Get-ChildItem\r\n# Сергей's profile\r\n"));
        assert!(block_in(&text), "{text}");
        assert_eq!(state_of(&w, &desktop), Some(State::Written));
        // Without the mark: never written, never garbled.
        assert_eq!(std::fs::read(&core).unwrap(), unmarked);
        assert_eq!(state_of(&w, &core), Some(State::Unreadable));
        // Again: nothing changes. Off: the person's bytes back.
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read(&desktop).unwrap(), bytes);
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read(&desktop).unwrap(), original);
        assert_eq!(std::fs::read(&core).unwrap(), unmarked);
        // Odd lengths and lone surrogates are not text.
        assert_eq!(decode(b"\xff\xfea", true), None);
        assert_eq!(decode(b"\xff\xfe\x00\xd8", true), None);
        assert_eq!(
            decode("\u{feff}x".as_bytes(), true),
            Some(("\u{feff}x".to_owned(), Encoding::Utf8))
        );
        // Only a PowerShell profile is read as UTF-16; another start file is UTF-8 only.
        assert_eq!(decode(b"\xff\xfea\x00", false), None);
        assert_eq!(
            decode(b"a\x00b", false),
            Some(("a\x00b".to_owned(), Encoding::Utf8))
        );
        assert_eq!(decode(b"a\x00b", true), None);
    }

    /// Seam R2: the NUL rule is PowerShell's. A `.zshrc` holding a NUL still gets the block and
    /// gives its bytes back, as before SB-225.
    #[test]
    fn a_zsh_start_file_with_a_nul_keeps_todays_behaviour() {
        let w = world();
        let original = b"export A=1\n# \x00 left by a tool\n";
        std::fs::write(zshrc(&w), original).unwrap();
        let report = set_and_apply(&w, Some(true), true);
        assert!(report.written >= 1, "{report:?}");
        let text = std::fs::read(zshrc(&w)).unwrap();
        assert!(text.starts_with(original));
        assert!(block_in(&String::from_utf8(text).unwrap()));
        assert_eq!(state_of(&w, &zshrc(&w)), Some(State::Written));
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read(zshrc(&w)).unwrap(), original);
    }

    /// SB-225: in Constrained Language Mode the hook can do nothing, so no profile is written;
    /// a block written before stays until `shell off`, which still gives the bytes back.
    #[test]
    fn constrained_language_mode_writes_no_profile_and_says_so() {
        let mut w = pwsh_world(Policy::RemoteSigned, Policy::RemoteSigned);
        let desktop = profile(&w, Edition::Desktop);
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, "# mine\n").unwrap();
        let constrained = |w: &mut World, on: bool| {
            if let Some(host) = w.host.windows.as_mut() {
                host.constrained = windows::Lockdown::both(on);
            }
        };
        constrained(&mut w, true);
        let report = set_and_apply(&w, None, true);
        assert_eq!(report.written, 0, "{report:?}");
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), "# mine\n");
        assert!(!profile(&w, Edition::Core).exists(), "nothing created");
        let status = map::status(Some(&w.root), None, &|_| None);
        let states: Vec<&str> = status["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["state"].as_str().unwrap())
            .collect();
        assert_eq!(
            states,
            ["constrained_language", "constrained_language"],
            "{status}"
        );
        // Lifted: written; constrained again: the block stays, recorded as such; off: gone.
        constrained(&mut w, false);
        assert_eq!(apply(&w.root, &w.host, true, false).written, 2);
        constrained(&mut w, true);
        apply(&w.root, &w.host, true, false);
        assert!(block_in(&std::fs::read_to_string(&desktop).unwrap()));
        assert_eq!(state_of(&w, &desktop), Some(State::ConstrainedLanguage));
        set_choice(&w.root, false).unwrap();
        apply(&w.root, &w.host, true, false);
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), "# mine\n");
        assert!(!profile(&w, Edition::Core).exists());
        // `__PSLockdownPolicy` alone holds Windows PowerShell 5.1 only: PowerShell 7's profile is
        // written, 5.1's is not.
        if let Some(host) = w.host.windows.as_mut() {
            host.constrained = windows::lockdown(None, Some("4"));
        }
        assert_eq!(set_and_apply(&w, Some(true), true).written, 1);
        assert_eq!(std::fs::read_to_string(&desktop).unwrap(), "# mine\n");
        assert_eq!(state_of(&w, &desktop), Some(State::ConstrainedLanguage));
        let core = profile(&w, Edition::Core);
        assert!(block_in(&std::fs::read_to_string(&core).unwrap()));
        assert_eq!(state_of(&w, &core), Some(State::Written));
    }

    #[test]
    fn the_cli_a_hook_names_has_no_verbatim_prefix() {
        for (given, named) in [
            (
                r"\\?\C:\Program Files\Fabric Switchboard\switchboard.exe",
                r"C:\Program Files\Fabric Switchboard\switchboard.exe",
            ),
            (
                r"\\?\UNC\server\share\switchboard.exe",
                r"\\server\share\switchboard.exe",
            ),
            (r"C:\tools\switchboard.exe", r"C:\tools\switchboard.exe"),
            ("/usr/local/bin/switchboard", "/usr/local/bin/switchboard"),
        ] {
            assert_eq!(
                plain_path(Path::new(given)),
                PathBuf::from(named),
                "{given}"
            );
        }
    }

    #[test]
    fn a_hook_path_that_is_not_ascii_keeps_the_powershell_block_ascii() {
        let hook = Path::new("/Users/Сергей/AppData/Local/sb/shell/hook.ps1");
        let line = ps_line(hook.to_str().unwrap());
        assert!(line.is_ascii(), "{line}");
        use base64::Engine;
        let encoded = line
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, hook.to_str().unwrap().as_bytes());
        assert!(
            line.contains("if (Test-Path -LiteralPath $f) { . $f }"),
            "{line}"
        );
        // An ASCII path with a quote stays a literal, the quote doubled.
        assert_eq!(
            ps_line("C:\\Users\\o'brien\\hook.ps1"),
            "if (Test-Path -LiteralPath 'C:\\Users\\o''brien\\hook.ps1') { . 'C:\\Users\\o''brien\\hook.ps1' }"
        );
    }
}
