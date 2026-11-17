//! What the core asks the app that manages coding accounts (SB-397 D-7). Each question has an
//! answer without one (`NoAccounts`): a machine with Fabric Sessions and no Switchboard reads the
//! person's own Claude Code folder and nothing else. Switchboard answers with the homes it
//! launches sessions in; further questions (a resume's environment, a stop's readiness, labels,
//! the permission choice) join as the modules that ask them move here.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub trait Accounts: Send + Sync {
    /// The config folders the app launches sessions in (each a `CLAUDE_CONFIG_DIR` with its own
    /// `projects/` and `sessions/`). None: no folder besides the person's own.
    fn launched_homes(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Config folders a terminal of the app's uses (its organizations' shell homes): sessions
    /// live there, transcripts are read from the launched homes only. None: none.
    fn shell_homes(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// The launch each launched home's marker names, by the pid the marker holds. None: no
    /// session is the app's (every one reads unmanaged).
    fn launch_markers(&self) -> HashMap<u32, String> {
        HashMap::new()
    }
    /// SB-417: the board runs live now, by launch id: of `live` (the launches the scan's processes
    /// name), those the app started for Fabric's board, and any other board run it watches that
    /// has not ended. The session pool lets these run up to the board's share. None: no session
    /// is the board's.
    fn board_launches(&self, _live: &[String]) -> HashSet<String> {
        HashSet::new()
    }
    /// D-7: opens a script that resumes a session in a window of its own. The app decides how
    /// (Switchboard runs it through a session host it can reopen later). None: a plain terminal
    /// window (`terminal::open`).
    fn open_terminal(&self, script: &Path) -> Result<(), String> {
        crate::terminal::open(script, None, &[])
    }
    /// D-7: the arguments a resumed Claude Code session takes to skip its permission prompts,
    /// as the person chose in the app. None: no such argument.
    fn resume_flags(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

/// No account manager on this machine: every answer is the empty one.
pub struct NoAccounts;
impl Accounts for NoAccounts {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_an_account_manager_every_answer_is_empty() {
        let none: &dyn Accounts = &NoAccounts;
        assert!(none.launched_homes().is_empty());
        assert!(none.shell_homes().is_empty());
        assert!(none.launch_markers().is_empty());
        assert!(none.resume_flags().is_empty());
        assert!(none.board_launches(&["L-1".into()]).is_empty());
    }
}
