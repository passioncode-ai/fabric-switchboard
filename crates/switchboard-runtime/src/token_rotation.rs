//! SB-509: the sessions left on the proxy's old token. When the recorded proxy port is taken at
//! start, the proxy starts on another port with a new token (SB-269), and every capability derived
//! from the old one is refused: a session started before keeps the old token in its environment
//! and gets 401 until it starts again. The owner records when that happened in
//! `<data>/proxy-rotated.json` (0600, one time); Needs attention lists the live sessions started
//! before it that run through the proxy, with a restart wave as the fix. Nothing restarts by
//! itself. The file is removed once no such session is left, so a computer that never rotated
//! reads nothing.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

pub const FILE: &str = "proxy-rotated.json";

/// Records that the proxy's token changed at `now`.
pub fn record(root: &Path, now: i64) {
    let _ = switchboard_core::private_fs::private_write(
        &root.join(FILE),
        json!({"at": now}).to_string().as_bytes(),
    );
}

/// When the proxy's token last changed; none when it never did (or the file is gone).
pub fn rotated_at(root: &Path) -> Option<i64> {
    switchboard_core::private_fs::read_private(&root.join(FILE), 4096)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["at"].as_i64())
}

/// The live sessions still on the old token: a Claude Code or Codex process started before `at`
/// that runs through the proxy, launched in managed mode (`modes` by launch id) or working in one
/// of the groups' folders (their route). Pure, for tests.
pub fn stale(
    rows: &[Value],
    modes: &HashMap<String, String>,
    folders: &[String],
    at: i64,
) -> Vec<Value> {
    rows.iter()
        .filter(|r| matches!(r["agent"].as_str(), Some("claude-code" | "codex")))
        .filter(|r| r["started_at"].as_i64().is_some_and(|t| t < at))
        .filter(|r| {
            let managed = r["launch_id"]
                .as_str()
                .and_then(|id| modes.get(id))
                .is_some_and(|m| m == "managed");
            let routed = r["folder"].as_str().is_some_and(|f| {
                folders.iter().any(|g| Path::new(f).starts_with(g))
            });
            managed || routed
        })
        .map(|r| json!({"pid": r["pid"], "agent": r["agent"], "folder": r["folder"], "launch_id": r["launch_id"]}))
        .collect()
}

/// The sessions on the old token now, `{at, sessions}`; none when the token never changed or no
/// such session is left (then the file goes, once the process table was read).
pub fn sessions(root: &Path, snapshot: &switchboard_core::Snapshot) -> Option<Value> {
    let at = rotated_at(root)?;
    let scan = crate::machine_sessions::scan(root);
    let rows: Vec<Value> = scan["sessions"].as_array().cloned().unwrap_or_default();
    let modes: HashMap<String, String> = rows
        .iter()
        .filter_map(|r| r["launch_id"].as_str())
        .filter_map(|id| {
            let mode = crate::launches::get(root, id)?["mode"].as_str()?.to_owned();
            Some((id.to_owned(), mode))
        })
        .collect();
    let folders: Vec<String> = snapshot
        .projects
        .iter()
        .flat_map(|p| p.folders.iter().cloned())
        .collect();
    let left = stale(&rows, &modes, &folders, at);
    if left.is_empty() {
        if crate::session_watch::table_read(&scan) {
            let _ = std::fs::remove_file(root.join(FILE));
        }
        return None;
    }
    Some(json!({"at": at, "sessions": left}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_proxy_sessions_started_before_the_rotation_are_listed() {
        let rows = vec![
            json!({"pid": 1, "agent": "claude-code", "folder": "/w/a", "started_at": 50, "launch_id": "L-1"}),
            json!({"pid": 2, "agent": "claude-code", "folder": "/g/app/sub", "started_at": 60}),
            json!({"pid": 3, "agent": "codex", "folder": "/elsewhere", "started_at": 70}),
            json!({"pid": 4, "agent": "claude-code", "folder": "/w/b", "started_at": 80, "launch_id": "L-4"}),
            json!({"pid": 5, "agent": "claude-code", "folder": "/g/app", "started_at": 150}),
            json!({"pid": 6, "agent": "kimi-code", "folder": "/g/app", "started_at": 10}),
        ];
        let modes: HashMap<String, String> = [
            ("L-1".to_owned(), "managed".to_owned()),
            ("L-4".to_owned(), "isolated".to_owned()),
        ]
        .into();
        let folders = vec!["/g/app".to_owned()];
        let pids: Vec<u64> = stale(&rows, &modes, &folders, 100)
            .iter()
            .map(|r| r["pid"].as_u64().unwrap())
            .collect();
        // A managed launch and a session in a group's folder; not an isolated launch, one outside
        // every group, one started after the rotation or another agent.
        assert_eq!(pids, [1, 2]);
    }

    #[test]
    fn a_computer_whose_token_never_changed_reads_nothing_and_the_time_is_kept() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(rotated_at(root.path()), None);
        record(root.path(), 1234);
        assert_eq!(rotated_at(root.path()), Some(1234));
    }
}
