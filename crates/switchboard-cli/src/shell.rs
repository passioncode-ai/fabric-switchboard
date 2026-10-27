//! `switchboard shell` (SB-183, node A-1): what the terminal hook reads.
//!
//! `shell env` runs on every change of folder in a hooked terminal, so it is answered by
//! [`env_fast_path`] before the async runtime starts, reads the folder map only (no store lock,
//! no owner), always exits 0 and never writes to the error stream: a terminal must never break
//! or wait because of Switchboard.
use clap::{Subcommand, ValueEnum};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};
use switchboard_runtime::shell::{install, map, scripts};

#[derive(Subcommand)]
pub enum ShellCmd {
    /// Whether this computer's terminals follow organization folders: the hook's state, whether
    /// this terminal has it, the folder's group, context and homes, and each start file Switchboard
    /// changed. Reads files only; safe to repeat.
    Status {
        /// The folder to read (absolute); default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// For the hook scripts: what a terminal exports in a folder. Prints `none`, or the group's
    /// pool, context, Claude home and Codex home (`-` for none), separated by tabs. Reads the
    /// folder map only; always exits 0 and never writes to the error stream.
    Env {
        /// The shell asking.
        #[arg(long, value_enum)]
        shell: Option<ShellKind>,
        /// The folder the terminal is in (absolute).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Puts the hook in this computer's terminals now, with or without an organization's group:
    /// one marked block in the start file of each shell present (zsh, bash, fish; on Windows the
    /// PowerShell profile of each edition), the file given back byte for byte by `shell off`.
    /// Writes a block you had taken out again.
    On,
    /// Takes the hook out of every start file Switchboard changed, byte for byte, and keeps it
    /// out. Terminals already open give your own settings back at their next change of folder.
    Off,
    /// The hook script for a shell, for people who keep their own start files: put
    /// `eval "$(switchboard shell init zsh)"` in `.zshrc` (bash likewise; fish:
    /// `switchboard shell init fish | source`; PowerShell:
    /// `switchboard shell init pwsh | Out-String | Invoke-Expression`). It names this CLI and
    /// the data folder.
    Init {
        /// The shell.
        #[arg(value_enum)]
        shell: InitShell,
    },
}
/// The shells `shell init` writes a hook for.
#[derive(Clone, Copy, ValueEnum)]
pub enum InitShell {
    Zsh,
    Bash,
    Fish,
    /// PowerShell: Windows PowerShell 5.1 and PowerShell 7.
    Pwsh,
}
impl InitShell {
    pub fn kind(self) -> scripts::Kind {
        match self {
            InitShell::Zsh => scripts::Kind::Zsh,
            InitShell::Bash => scripts::Kind::Bash,
            InitShell::Fish => scripts::Kind::Fish,
            InitShell::Pwsh => scripts::Kind::Pwsh,
        }
    }
}

/// `switchboard shell on|off`: saves the choice, brings the start files in line with it now and
/// answers `shell status`. Only for the app's own data folder: every data folder would write the
/// same marker into the same home. The hook names the installed CLI (a development build never
/// names itself).
pub fn set(data_dir: Option<&Path>, on: bool) -> Result<Value, String> {
    let root = match data_dir {
        Some(root) => root.to_owned(),
        None => switchboard_runtime::default_root()?,
    };
    if !root.is_absolute() {
        return Err("--data-dir must be an absolute private directory.".into());
    }
    if !install::is_default_root(&root) {
        return Err(NOT_OWN_FOLDER.into());
    }
    let cli = install::hook_cli();
    if on && cli.is_none() {
        return Err(NO_INSTALLED_CLI.into());
    }
    let host = install::Host::process(cli)
        .ok_or("Your home folder could not be found, so no start file was changed.")?;
    install::set_choice(&root, on)?;
    let report = install::apply(&root, &host, false, on);
    if on && report.no_hook {
        return Err(install::NO_PATH.into());
    }
    Ok(status(Some(&root), None))
}
/// `shell on|off` with another `--data-dir` (copy: Copylot reports in the commit).
const NOT_OWN_FOLDER: &str = "Only Switchboard's own data folder can change your terminals' start files. Run this command without --data-dir.";
/// `shell on` in a development build with no installed CLI to name.
const NO_INSTALLED_CLI: &str = "No installed switchboard command was found for the hook to run. Install Fabric Switchboard, then run this command again.";

/// `switchboard shell init <shell>`: the hook script, naming this executable and the data
/// folder.
pub fn init(data_dir: Option<&Path>, shell: InitShell) -> Result<String, String> {
    let root = match data_dir {
        Some(root) => root.to_owned(),
        None => switchboard_runtime::default_root()?,
    };
    let cli = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|_| "The path of this switchboard executable could not be read.".to_string())?;
    scripts::script(shell.kind(), &cli, &root).ok_or_else(|| {
        "The hook cannot name this executable or the data folder: a path is not absolute or not valid text."
            .into()
    })
}
#[derive(Clone, Copy, ValueEnum)]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    Pwsh,
}

/// Answers `switchboard shell env ...` from the raw arguments (without the program name), or
/// None when they name another command or ask for help, for the ordinary parser to take.
pub fn env_fast_path(args: &[OsString]) -> Option<ExitCode> {
    let request = parse_env(args)?;
    // Nothing reaches the error stream, not even a panic's message.
    std::panic::set_hook(Box::new(|_| {}));
    let line = std::panic::catch_unwind(|| match request {
        Some((data_dir, dir)) => env_answer(data_dir.as_deref(), dir.as_deref()),
        None => map::NONE.to_owned(),
    })
    .unwrap_or_else(|_| map::NONE.to_owned());
    print_line(&line);
    Some(ExitCode::SUCCESS)
}

/// The answer for a data folder (None: the app's own) and a folder; `none` for anything that
/// cannot be read.
pub fn env_answer(data_dir: Option<&Path>, dir: Option<&Path>) -> String {
    let root = match data_dir {
        Some(root) => Some(root.to_owned()),
        None => switchboard_runtime::default_root().ok(),
    };
    match (root, dir) {
        (Some(root), Some(dir)) if root.is_absolute() && dir.is_absolute() => {
            map::env_line(&root, dir)
        }
        _ => map::NONE.to_owned(),
    }
}
pub fn print_line(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}").and_then(|()| out.flush());
}

/// `Some(Some((data dir, folder)))` for a well-formed `shell env`, `Some(None)` for a malformed
/// one (answered `none`), None for anything else.
#[allow(clippy::type_complexity)]
fn parse_env(args: &[OsString]) -> Option<Option<(Option<PathBuf>, Option<PathBuf>)>> {
    let mut positional: Vec<&str> = Vec::new();
    let mut data_dir: Option<PathBuf> = None;
    let mut dir: Option<PathBuf> = None;
    let mut bad = false;
    let mut args = args.iter();
    let is_env = |positional: &[&str]| positional.len() >= 2;
    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            if is_env(&positional) {
                bad = true;
                continue;
            }
            return None;
        };
        if matches!(text, "-h" | "--help" | "-V" | "--version") {
            return None;
        }
        // `--name value` or `--name=value`.
        let mut value = |name: &str| -> Option<Option<OsString>> {
            if text == name {
                return Some(args.next().cloned());
            }
            text.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|v| Some(OsString::from(v)))
        };
        if text == "--json" {
            continue;
        }
        if let Some(found) = value("--data-dir") {
            match found {
                Some(v) => data_dir = Some(PathBuf::from(v)),
                None => bad = true,
            }
            continue;
        }
        if text.starts_with('-') {
            if !is_env(&positional) {
                // An option of another command.
                return None;
            }
            if let Some(found) = value("--dir") {
                match found {
                    Some(v) => dir = Some(PathBuf::from(v)),
                    None => bad = true,
                }
            } else if let Some(found) = value("--shell") {
                bad |= !found
                    .and_then(|v| v.into_string().ok())
                    .is_some_and(|v| ShellKind::from_str(&v, false).is_ok());
            } else {
                bad = true;
            }
            continue;
        }
        match positional.len() {
            0 if text != "shell" => return None,
            1 if text != "env" => return None,
            0 | 1 => positional.push(text),
            _ => bad = true,
        }
    }
    if !is_env(&positional) {
        return None;
    }
    Some((!bad).then_some((data_dir, dir)))
}

/// `switchboard shell status`.
pub fn status(data_dir: Option<&Path>, dir: Option<&Path>) -> Value {
    let root = match data_dir {
        Some(root) => Some(root.to_owned()),
        None => switchboard_runtime::default_root().ok(),
    };
    let current = dir
        .map(Path::to_owned)
        .or_else(|| std::env::current_dir().ok());
    map::status(root.as_deref(), current.as_deref(), &|name| {
        std::env::var(name).ok()
    })
}

/// The human form of `shell status`. Draft copy (SCN-074, SCN-075); it goes through copywriting
/// and Copylot in wave 4.
pub fn status_text(value: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    let mut lines = Vec::new();
    lines.push(match value["hook"].as_str() {
        Some("on") => "Terminal hook: on.".to_owned(),
        Some("off") => "Terminal hook: off. New terminals keep your own settings.".to_owned(),
        Some("default_on") => {
            "Terminal hook: on (default): new terminals in organization folders work in their context."
                .to_owned()
        }
        _ => "Terminal hook: off (default): no group belongs to an organization on this computer."
            .to_owned(),
    });
    let terminal = &value["terminal"];
    lines.push(
        if terminal["launched_session"].as_bool() == Some(true) {
            "This terminal: a session Switchboard launched; the hook does nothing here."
        } else if terminal["hook_loaded"].as_bool() == Some(true) {
            "This terminal: the hook is loaded."
        } else {
            "This terminal: the hook is not loaded. Open a new terminal to load it."
        }
        .to_owned(),
    );
    if let Some(folder) = value["folder"].as_str() {
        lines.push(format!("Folder: {folder}"));
    }
    let here = &value["here"];
    if here.is_null() {
        lines.push("Group: none here. This terminal keeps your own settings.".to_owned());
    } else {
        lines.push(format!(
            "Group: {} (context {})",
            text(&here["group"]),
            text(&here["context"])
        ));
        lines.push(match here["claude_home"].as_str() {
            Some(home) => format!("claude: {home}"),
            None => "claude: your own home (this folder's Claude Code is not bound to the group)"
                .to_owned(),
        });
        lines.push(match here["codex_home"].as_str() {
            Some(home) => format!("codex: {home}"),
            None => "codex: your own sign-in (this group has no Codex account)".to_owned(),
        });
    }
    match value["map"].as_str() {
        Some("ok") | Some("missing") => {}
        Some("unknown_version") => {
            lines.push("Folder map: written by another version; nothing applies.".to_owned())
        }
        _ => lines.push("Folder map: could not be read; nothing applies.".to_owned()),
    }
    for file in value["files"].as_array().into_iter().flatten() {
        let shell = text(&file["shell"]);
        let state = match file["state"].as_str() {
            Some("written") => "added".to_owned(),
            Some("skipped_symlink") => format!(
                "a link, so Switchboard did not change it. Add this yourself: switchboard shell init {shell}"
            ),
            Some("skipped_owner") => "another user's file, not changed".to_owned(),
            Some("skipped_zdotdir_unknown") => {
                "not created: zsh may read its start file from another folder (ZDOTDIR). Add this to the start file zsh reads: eval \"$(switchboard shell init zsh)\"".to_owned()
            }
            Some("unreadable") => "could not be read, not changed".to_owned(),
            Some("removed_by_person") => {
                "you took the block out; switchboard shell on adds it again".to_owned()
            }
            Some("policy_blocked") => {
                "PowerShell's execution policy stops profiles, so the hook is inactive".to_owned()
            }
            Some(other) => other.to_owned(),
            None => String::new(),
        };
        lines.push(format!("{shell}: {}, {state}", text(&file["path"])));
    }
    lines.iter().map(|l| format!("{l}\n")).collect()
}
