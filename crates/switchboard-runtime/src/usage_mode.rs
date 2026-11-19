//! SB-518 (DEC-0032): the person's choice of how quota figures are kept current, kept in
//! `<data>/usage-mode` (0600, one word): `smart` (absent, the default), `timer` (the fixed
//! cadence of 0.7.120) or `on_demand` (no background check). Read at the owner's start and by each
//! offline command; set with `switchboard usage-mode` or MCP `switchboard_usage_mode`.
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::UsageMode;

pub const FILE: &str = "usage-mode";
pub const UNKNOWN: &str = "Choose smart, timer or on_demand.";
pub const UNWRITTEN: &str =
    "The usage check mode could not be saved. Check Switchboard's data folder, then retry.";

/// The saved mode; absent or unreadable reads as `smart`.
pub fn read(root: &Path) -> UsageMode {
    switchboard_core::private_fs::read_private(&root.join(FILE), 64)
        .ok()
        .and_then(|bytes| UsageMode::parse(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

pub fn status(root: &Path) -> Value {
    let mode = read(root);
    json!({
        "usage_mode": mode.as_str(),
        "modes": ["smart", "timer", "on_demand"],
    })
}

/// Saves `mode` (0600, by rename) and answers the new status.
pub fn set(root: &Path, mode: &str) -> Result<Value, String> {
    let mode = UsageMode::parse(mode).ok_or(UNKNOWN)?;
    switchboard_core::private_fs::private_write(
        &root.join(FILE),
        format!("{}\n", mode.as_str()).as_bytes(),
    )
    .map_err(|_| UNWRITTEN.to_string())?;
    crate::oplog::event(
        "usage_mode",
        &[("mode", crate::oplog::Field::Code(mode.as_str()))],
    );
    Ok(status(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_is_smart_until_the_person_chooses_and_survives_a_restart() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()), UsageMode::Smart);
        assert_eq!(status(root.path())["usage_mode"], "smart");
        assert_eq!(
            set(root.path(), "on-demand").unwrap()["usage_mode"],
            "on_demand"
        );
        assert_eq!(read(root.path()), UsageMode::OnDemand);
        assert_eq!(set(root.path(), "never").unwrap_err(), UNKNOWN);
        assert_eq!(
            read(root.path()),
            UsageMode::OnDemand,
            "a refusal changes nothing"
        );
        set(root.path(), "timer").unwrap();
        assert_eq!(read(root.path()), UsageMode::Timer);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
