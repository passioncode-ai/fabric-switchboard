//! Switches a test sets on the `switchboard` binary it runs. Each is read only in a debug build,
//! so a release build behaves the same whatever its environment holds.

/// `SWITCHBOARD_TEST_NO_PROCESSES=1`: the process table reads as empty, so an answer built from
/// the data folder alone does not depend on the coding sessions running on the machine that runs
/// the test (the golden answers of SB-397 W1.0, `crates/switchboard-cli/tests/golden.rs`).
pub(crate) fn no_processes() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("SWITCHBOARD_TEST_NO_PROCESSES").is_some_and(|v| v == "1")
}
