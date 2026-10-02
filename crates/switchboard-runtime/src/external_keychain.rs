//! External CLI items through Apple's stable `/usr/bin/security`, as Claude Code and
//! Claude Swap do: the executable that created an item reads and updates it silently.
//! The bounded runner, quoting and stdin transport live in `switchboard_core::security_cli`.
use switchboard_core::security_cli;

pub fn read(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    security_cli::find(service, account)
}
/// Creates or updates the item; the secret travels on stdin.
pub fn write(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    security_cli::add(service, account, secret, true).map(|_| ())
}
/// Absent counts as removed.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    security_cli::delete(service, account)
}
