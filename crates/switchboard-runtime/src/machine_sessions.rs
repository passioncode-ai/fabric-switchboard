//! SB-353: every coding-agent session running on this machine. The scan lives in
//! fabric-sessions-core (SB-397 D-2); here it answers with Switchboard's homes
//! (`session_accounts::Homes`), under the paths its callers already use.
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fabric_sessions_core::machine_sessions as core;
pub use fabric_sessions_core::machine_sessions::{
    agent_of, ancestor_paths, assemble, live_from, pairs_from, processes, projects_dir_name,
    session_last_write, Proc, QUIET_MINUTES,
};

fn homes(root: &Path) -> crate::session_accounts::Homes<'_> {
    crate::session_accounts::Homes::at(root)
}
/// SB-376: the sessions a live Claude Code process holds now, by session id.
pub fn live_sessions(root: &Path) -> HashMap<String, (u32, String)> {
    core::live_sessions(&homes(root))
}
/// SB-379: every live Claude Code process with the session id its `sessions/<pid>.json` names.
pub fn session_pairs(root: &Path) -> Vec<(String, u32, String)> {
    core::session_pairs(&homes(root))
}
/// SB-384: every session file with the config folder it lies in.
pub(crate) fn session_files(root: &Path) -> Vec<(PathBuf, Value)> {
    core::session_files(&homes(root))
}
pub fn scan(root: &Path) -> Value {
    core::scan(&homes(root))
}
/// SB-379: the scan and the live session pairs from one walk of the process table.
pub fn scan_and_pairs(root: &Path) -> (Value, Vec<(String, u32, String)>) {
    core::scan_and_pairs(&homes(root))
}
