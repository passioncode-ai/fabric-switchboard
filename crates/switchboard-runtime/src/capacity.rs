//! SB-378: one cap on the coding sessions this computer runs at once. The pool lives in
//! fabric-sessions-core (SB-397 D-2); here it reads Switchboard's data folder and homes.
//! SB-417: a start the pool admits holds a reservation until the launcher records it as running
//! (`reserve`); a board start recorded ends the wait of the board starts refused before
//! (`dispatch_refusals::admitted`, called by the launcher once the run is recorded).
use serde_json::Value;
use std::path::Path;

use fabric_sessions_core::capacity as core;
pub use fabric_sessions_core::capacity::{
    cap, decide, default_board_share, read, set, set_pool, used, view, Count, Pool, Reservation,
    Share, Start, AT_CAPACITY, BOARD_INVALID, CAP_INVALID, CLEAR_INVALID, FILE, MAX_CAP, NOT_SAVED,
    PENDING, POOL_LOCK, UNREADABLE,
};

/// `switchboard capacity`: the cap, the board's share, what counts against them and what is
/// free.
pub fn status(root: &Path) -> Result<Value, String> {
    core::status(root, &crate::session_accounts::Homes::at(root))
}
/// The gate before a hosted start or a dispatch: `Ok(None)` without a cap, `Ok(Some(_))`
/// admitted (counted until dropped), `Err(view)` refused.
pub fn reserve(root: &Path, start: Start) -> Result<Option<Reservation>, Value> {
    core::reserve(root, &crate::session_accounts::Homes::at(root), start)
}
/// Review round 2, P2-3: a reservation whose drop (a blocking file lock) runs off the async
/// worker, as every blocking section of the owner does (`blocking::run`).
pub struct Held(Option<Reservation>);
impl Drop for Held {
    fn drop(&mut self) {
        if let Some(reservation) = self.0.take() {
            crate::blocking::run(move || drop(reservation));
        }
    }
}
/// `reserve` for the owner's `dispatch`: the reservation held until it is dropped.
pub fn reserve_held(root: &Path, start: Start) -> Result<Held, Value> {
    reserve(root, start).map(Held)
}
/// `reserve` for `launch --hosted`: the refusal as the sentence the CLI prints.
pub fn admit(root: &Path, start: Start) -> Result<Option<Reservation>, String> {
    reserve(root, start).map_err(|_| AT_CAPACITY.to_owned())
}
/// The pool's count and view of a scan already made (the session watch's, the tree's).
pub fn view_of(root: &Path, scan: &Value, now: i64) -> (Pool, Count, Value) {
    core::view_of(root, &crate::session_accounts::Homes::at(root), scan, now)
}
