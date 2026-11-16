//! Fabric Sessions' core (SB-397): the code that owns coding sessions apart from every app. In
//! W1 the session modules move here from `switchboard-runtime`, one seam at a time, and Switchboard
//! uses them through this crate; nothing here may depend on a `switchboard-*` crate (D-2), so the
//! service runs on a machine without Switchboard. Plan: `docs/plans/2026-11-16-session-service/`.

/// What the core asks the app that manages coding accounts (D-7).
pub mod accounts;
/// One cap on the coding sessions this computer runs at once.
pub mod capacity;
/// Talking to a command line: shell quoting, a JSON answer's data.
pub mod cli;
/// The core's operations events, sent on to the app's log.
pub mod events;
/// Identifiers a session record may carry.
pub mod ids;
/// Every coding-agent session running on this machine.
pub mod machine_sessions;
/// What a session waits on (wake-ups, monitors, cron jobs, another tool's hold).
pub mod monitors;
/// The person's own Claude Code and Codex folders.
pub mod person;
/// Private storage: owner-only files and folders, whole-file replacement (from switchboard-core).
pub mod private_fs;
/// This computer's sessions as one list in four groups (SB-395).
pub mod session_rows;
/// Limit stops kept while no owner runs.
pub mod stop_spool;
/// Switches a test sets on the binary it runs (debug builds only).
pub mod test_seams;
#[cfg(windows)]
pub mod windows;

/// This crate's version, which Fabric Sessions reports as its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The wall clock in Unix seconds (D-2: Switchboard's `monitor::now`, moved as `clock::now`).
pub mod clock {
    /// Seconds since the Unix epoch; 0 for a clock set before it.
    pub fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_reads_seconds_after_2026() {
        assert!(super::clock::now() > 1_767_225_600, "2026-01-01 or later");
    }

    #[test]
    fn the_version_is_the_crates_own() {
        assert_eq!(super::VERSION, env!("CARGO_PKG_VERSION"));
        assert!(
            !super::VERSION.starts_with("0.7."),
            "not Switchboard's version"
        );
    }
}
