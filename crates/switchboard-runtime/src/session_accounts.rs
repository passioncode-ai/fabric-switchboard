//! Switchboard's answers to Fabric Sessions' questions (SB-397 D-7, `fabric_sessions_core::accounts`):
//! the homes it launches sessions in (`homes/`, `runtimes/`), its organizations' shell homes and
//! the launch each launched home's marker (`.session-pid`) names.
use fabric_sessions_core::accounts::Accounts;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Switchboard's data folder, asked as an account manager.
pub struct Homes<'a> {
    root: &'a Path,
}
impl<'a> Homes<'a> {
    pub fn at(root: &'a Path) -> Self {
        Homes { root }
    }
    fn children(&self, folder: &str) -> Vec<PathBuf> {
        std::fs::read_dir(self.root.join(folder))
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }
}
impl Accounts for Homes<'_> {
    fn launched_homes(&self) -> Vec<PathBuf> {
        let mut homes = self.children("homes");
        homes.extend(self.children("runtimes"));
        homes
    }
    fn shell_homes(&self) -> Vec<PathBuf> {
        self.children(crate::shell::map::DIR)
    }
    fn launch_markers(&self) -> HashMap<u32, String> {
        let mut found = HashMap::new();
        for home in self.launched_homes() {
            let Some(pid) = std::fs::read_to_string(home.join(".session-pid"))
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok())
            else {
                continue;
            };
            let launch = std::fs::read(home.join(crate::launch::SESSION_RECORD))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|r| r["launch_id"].as_str().map(str::to_owned));
            if let Some(launch) = launch {
                found.insert(pid, launch);
            }
        }
        // SB-400: a session host's own pid names its launch, so a hosted continue (no home of
        // its own) is found through its process's ancestors (R4 review).
        #[cfg(unix)]
        for (launch, pid) in crate::session_host::live_hosts(self.root) {
            found.entry(pid).or_insert(launch);
        }
        found
    }
    fn open_terminal(&self, script: &Path) -> Result<(), String> {
        crate::launch::start_terminal(script)
    }
    fn resume_flags(&self) -> Vec<&'static str> {
        crate::launch::bypass_args(self.root, switchboard_core::Provider::Claude, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_homes_switchboard_launches_in_and_their_markers_are_answered() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("homes/acct-1");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(root.path().join("runtimes/claude-default")).unwrap();
        std::fs::create_dir_all(root.path().join(crate::shell::map::DIR).join("claude-org"))
            .unwrap();
        std::fs::write(home.join(".session-pid"), "4242\n").unwrap();
        std::fs::write(
            home.join(crate::launch::SESSION_RECORD),
            r#"{"launch_id": "L-1"}"#,
        )
        .unwrap();
        let homes = Homes::at(root.path());
        let mut launched = homes.launched_homes();
        launched.sort();
        assert_eq!(launched.len(), 2, "{launched:?}");
        assert_eq!(homes.shell_homes().len(), 1);
        assert_eq!(
            homes.launch_markers(),
            HashMap::from([(4242, "L-1".to_string())])
        );
    }
}
