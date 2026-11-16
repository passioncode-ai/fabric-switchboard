//! SB-396 wave C: what a session waits on (wake-ups, monitors, cron jobs, a hold another tool
//! sets). It lives in fabric-sessions-core (SB-397 D-2); a hold logs through this app's `oplog`.
use serde_json::Value;
use std::path::Path;

pub use fabric_sessions_core::monitors::*;

/// SB-399: `switchboard monitor hold` (see `fabric_sessions_core::monitors::hold`).
pub fn hold(
    root: &Path,
    session: &str,
    until: i64,
    now: i64,
    reason: Option<&str>,
) -> Result<Value, String> {
    crate::oplog::bridge();
    fabric_sessions_core::monitors::hold(root, session, until, now, reason)
}
