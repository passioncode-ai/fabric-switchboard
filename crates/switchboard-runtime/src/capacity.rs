//! SB-378: one cap on the coding sessions this computer runs at once. The pool lives in
//! fabric-sessions-core (SB-397 D-2); here it reads Switchboard's data folder and homes.
use serde_json::Value;
use std::path::Path;

use fabric_sessions_core::capacity as core;
pub use fabric_sessions_core::capacity::{
    cap, decide, set, used, view, AT_CAPACITY, CAP_INVALID, FILE, MAX_CAP, NOT_SAVED, UNREADABLE,
};

/// `switchboard capacity`: the cap, what counts against it and what is free.
pub fn status(root: &Path) -> Result<Value, String> {
    core::status(root, &crate::session_accounts::Homes::at(root))
}
/// The gate `launch --hosted` calls before a start. Without a cap nothing is read.
pub fn admit(root: &Path) -> Result<(), String> {
    core::admit(root, &crate::session_accounts::Homes::at(root))
}
/// The pool's view when it is full, else none.
pub fn full(root: &Path) -> Option<Value> {
    core::full(root, &crate::session_accounts::Homes::at(root))
}
