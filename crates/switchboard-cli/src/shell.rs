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
use switchboard_runtime::shell::map;

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
