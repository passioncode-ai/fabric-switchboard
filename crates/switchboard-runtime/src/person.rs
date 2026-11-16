//! The person's own Claude Code and Codex folders (SB-183, node A-5). The rules live in
//! fabric-sessions-core (SB-397 D-2); here they read this process with Switchboard's data folder,
//! whose homes are never the person's.
use std::path::PathBuf;

pub use fabric_sessions_core::person::{
    claude_dir_from, codex_dir_from, inside, pi_dir_from, Sources,
};

/// This process's environment and home, with Switchboard's data folder.
pub(crate) fn process() -> Sources<'static> {
    Sources::process(crate::default_root().ok())
}
/// The person's Claude Code folder: their `CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    claude_dir_from(&process())
}
/// The person's Codex folder: their `CODEX_HOME`, else `~/.codex`.
pub fn codex_dir() -> Option<PathBuf> {
    codex_dir_from(&process())
}
/// The `CLAUDE_CONFIG_DIR` the person set themselves, if any (None: Claude Code uses `~/.claude`
/// and `~/.claude.json`).
pub fn claude_config() -> Option<PathBuf> {
    fabric_sessions_core::person::own("CLAUDE_CONFIG_DIR", &process())
}
/// The `CODEX_HOME` the person set themselves, if any (None: Codex uses `~/.codex`).
pub fn codex_home() -> Option<PathBuf> {
    fabric_sessions_core::person::own("CODEX_HOME", &process())
}
