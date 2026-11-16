//! What the core asks the app that manages coding accounts (SB-397 D-7). Each question has an
//! answer without one (`NoAccounts`): a machine with Fabric Sessions and no Switchboard reads the
//! person's own Claude Code folder and nothing else. Switchboard answers with the homes it
//! launches sessions in; further questions (a resume's environment, a stop's readiness, labels,
//! the permission choice) join as the modules that ask them move here.
use std::collections::HashMap;
use std::path::PathBuf;

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
    }
}
