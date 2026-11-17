//! SB-378: one cap on the coding sessions this computer runs at once. The pool lives in
//! fabric-sessions-core (SB-397 D-2); here it reads Switchboard's data folder and homes.
//! SB-417: a board start the pool admits clears the waiting board starts it refused before
//! (`dispatch_refusals::admitted`).
use serde_json::Value;
use std::path::Path;

use fabric_sessions_core::capacity as core;
pub use fabric_sessions_core::capacity::{
    cap, decide, default_board_share, read, set, set_board, used, view, Count, Pool, Start,
    AT_CAPACITY, BOARD_INVALID, CAP_INVALID, FILE, MAX_CAP, NOT_SAVED, UNREADABLE,
};

/// `switchboard capacity`: the cap, the board's share, what counts against them and what is
/// free.
pub fn status(root: &Path) -> Result<Value, String> {
    core::status(root, &crate::session_accounts::Homes::at(root))
}
/// The gate `launch --hosted` calls before a start. Without a cap nothing is read.
pub fn admit(root: &Path, start: Start) -> Result<(), String> {
    match full(root, start) {
        Some(_) => Err(AT_CAPACITY.into()),
        None => Ok(()),
    }
}
/// The pool's view when it refuses `start`, else none. A board start it admits ends the wait of
/// the board starts it refused before.
pub fn full(root: &Path, start: Start) -> Option<Value> {
    let refused = core::full(root, &crate::session_accounts::Homes::at(root), start);
    if refused.is_none() && start == Start::Board {
        crate::dispatch_refusals::admitted(root, crate::monitor::now());
    }
    refused
}
/// The pool's count and view of a scan already made (the session watch's, the tree's).
pub fn view_of(root: &Path, scan: &Value, now: i64) -> (Pool, Count, Value) {
    core::view_of(root, &crate::session_accounts::Homes::at(root), scan, now)
}
