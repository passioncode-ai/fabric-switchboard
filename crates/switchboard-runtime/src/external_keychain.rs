//! External CLI items through Apple's stable `/usr/bin/security`, as Claude Code and
//! Claude Swap do: the executable that created an item reads and updates it silently.
//! The bounded runner, quoting and stdin transport live in `switchboard_core::security_cli`.
//! Each call may wait up to five seconds, so it runs as a blocking section (board SB-23).
use switchboard_core::security_cli;

pub fn read(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    crate::blocking::run(|| security_cli::find(service, account))
}
/// Creates or updates the item; the secret travels on stdin.
pub fn write(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    crate::blocking::run(|| add_compacting(service, account, secret))
}
/// SB-542: writes a Claude Code credential item; when it is too large for one `security -i`
/// line, writes it again without the MCP entries that hold no token (`mcp_homes::without_stubs`):
/// discovery stubs Claude Code writes again at its next discovery, which had grown the operator's
/// item past the line (41 of 43 entries on 2026-11-19). Every entry holding a token is kept; still
/// too large is refused with nothing written, as before.
pub(crate) fn add_compacting(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    match security_cli::add(service, account, secret, true) {
        Err(e) if e == security_cli::TOO_LARGE => {
            let Some((smaller, dropped)) = crate::mcp_homes::without_stubs(secret) else {
                return Err(e);
            };
            security_cli::add(service, account, &smaller, true)?;
            crate::oplog::event(
                "keychain_compacted",
                &[("dropped", crate::oplog::Field::Number(dropped as i64))],
            );
            Ok(())
        }
        other => other.map(|_| ()),
    }
}
/// Absent counts as removed.
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    crate::blocking::run(|| security_cli::delete(service, account))
}
