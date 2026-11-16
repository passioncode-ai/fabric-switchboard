//! App-owned storage primitives. They live in fabric-sessions-core (SB-397 D-2), so Fabric
//! Sessions writes its records the same way without Switchboard; re-exported here unchanged.
pub use fabric_sessions_core::private_fs::*;
