//! SB-420: a Claude Code Keychain item Switchboard could not write because it is too large to
//! go through the security tool without its value on the command line. Each refusal is kept in
//! `<data>/keychain-too-large.json` (0600, which item, during what and when; never a value or a
//! size; since SB-549 the version that recorded it), cleared by the next write of that item that
//! succeeds, and dropped at the start of an owner of another version. `switchboard status` (MCP
//! `switchboard_status`) and Needs attention read it, so the state is seen, not only logged.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

pub const FILE: &str = "keychain-too-large.json";
/// The ordinary Claude Code's item: written by a switch (activation, rotation) or a renewal.
pub const ORDINARY: &str = "ordinary";
/// A launched home's item: the MCP sign-ins copied into it at a launch.
pub const LAUNCH: &str = "launch";
pub const DURING_SWITCH: &str = "switch";
pub const DURING_RENEWAL: &str = "renewal";
pub const DURING_LAUNCH_COPY: &str = "launch_copy";
/// The Needs attention sentence: names the item and the fix, never a value.
pub const DETAIL: &str =
    "Claude Code's sign-in is too large to write to the Keychain safely, so Switchboard could not update it. Sign out of MCP servers you no longer use in Claude Code.";
const READ_LIMIT: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub item: String,
    pub during: String,
    pub at: i64,
    /// The item's Keychain stamp when a renewal was refused (attributes only, nothing
    /// decrypted): while it has not moved, the renewal is not tried again (the idle budget).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<String>,
    /// SB-549: the version that recorded it. A later version may write the item (SB-542 leaves
    /// out MCP stubs), so its owner drops the older ones at start ([`forget_older`]); a refusal
    /// that still holds is recorded again by the next act.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    items: Vec<Entry>,
}

/// Whether a write's error is the refusal of a too large item.
pub fn is_too_large(error: &str) -> bool {
    #[cfg(unix)]
    if error == switchboard_core::security_cli::TOO_LARGE {
        return true;
    }
    error == crate::external::CREDENTIAL_TOO_LARGE
}

/// The refusals kept, oldest first; an absent or unreadable file holds none.
pub fn read(root: &Path) -> Vec<Entry> {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(root.join(FILE)) else {
        return vec![];
    };
    let mut text = String::new();
    if file.take(READ_LIMIT).read_to_string(&mut text).is_err() {
        return vec![];
    }
    serde_json::from_str::<File>(&text)
        .map(|f| f.items)
        .unwrap_or_default()
}

/// One read-modify-write of the file at a time among the owner's threads (a switch, a
/// renewal and a launch's copy can meet).
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Records that `item` was refused `during` an act at `at` (one entry per item). The same
/// refusal again writes and logs nothing: a renewal asks every pass (the idle budget).
pub fn record(root: &Path, item: &str, during: &str, at: i64) {
    record_at_stamp(root, item, during, at, None);
}

/// [`record`] with the item's Keychain stamp at the refusal ([`renewal_held`] reads it).
pub fn record_at_stamp(root: &Path, item: &str, during: &str, at: i64, stamp: Option<String>) {
    let _held = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut items = read(root);
    if items.iter().any(|e| {
        e.item == item
            && e.during == during
            && e.stamp == stamp
            && e.version.as_deref() == Some(env!("CARGO_PKG_VERSION"))
    }) {
        return;
    }
    items.retain(|e| e.item != item);
    items.push(Entry {
        item: item.into(),
        during: during.into(),
        at,
        stamp,
        version: Some(env!("CARGO_PKG_VERSION").into()),
    });
    write(root, items);
    crate::oplog::event(
        "keychain_too_large",
        &[
            ("item", crate::oplog::Field::Code(code(item))),
            ("during", crate::oplog::Field::Code(code(during))),
        ],
    );
}

/// A write of `item` succeeded: its refusal, if any, is over. Writes nothing when none is kept.
pub fn clear(root: &Path, item: &str) {
    let _held = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut items = read(root);
    let before = items.len();
    items.retain(|e| e.item != item);
    if items.len() == before {
        return;
    }
    write(root, items);
}

/// SB-549: drops the refusals another version recorded, at the owner's start: the app updated
/// since may write the item now (0.7.121 left the operator's two refusals of 0.7.120 in Needs
/// attention until the next switch and the next launch). Answers how many it dropped.
pub fn forget_older(root: &Path, current: &str) -> usize {
    let _held = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut items = read(root);
    let before = items.len();
    items.retain(|e| e.version.as_deref() == Some(current));
    let dropped = before - items.len();
    if dropped > 0 {
        write(root, items);
        crate::oplog::event(
            "keychain_too_large_reset",
            &[("dropped", crate::oplog::Field::Number(dropped as i64))],
        );
    }
    dropped
}

/// Records or clears `item` by the outcome of one write of it.
pub fn note(root: &Path, item: &str, during: &str, outcome: &Result<(), String>, at: i64) {
    match outcome {
        Ok(()) => clear(root, item),
        Err(error) if is_too_large(error) => record(root, item, during, at),
        Err(_) => {}
    }
}

fn write(root: &Path, items: Vec<Entry>) {
    let path = root.join(FILE);
    if items.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(&File { items }) {
        let _ = crate::launch::private_write(&path, &bytes, false);
    }
}

/// The static code for the log: only the known words, never text from the file.
fn code(word: &str) -> &'static str {
    match word {
        ORDINARY => ORDINARY,
        LAUNCH => LAUNCH,
        DURING_SWITCH => DURING_SWITCH,
        DURING_RENEWAL => DURING_RENEWAL,
        DURING_LAUNCH_COPY => DURING_LAUNCH_COPY,
        _ => "other",
    }
}

/// The answer `status` carries: `[{item, during, at}]`, empty when nothing is refused.
pub fn status(root: &Path) -> Value {
    json!(read(root)
        .into_iter()
        .map(|e| json!({"item": e.item, "during": e.during, "at": e.at}))
        .collect::<Vec<_>>())
}

/// Whether a renewal of Claude Code's item was refused as too large and the item has not been
/// written since: its Keychain stamp (`stamp`, attributes only) is the one recorded. An unknown
/// stamp is never held.
pub fn renewal_held(root: &Path, stamp: Option<&str>) -> bool {
    let Some(stamp) = stamp else {
        return false;
    };
    read(root).iter().any(|e| {
        e.item == ORDINARY && e.during == DURING_RENEWAL && e.stamp.as_deref() == Some(stamp)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_new_version_forgets_the_refusals_an_older_one_recorded() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let old = r#"{"items":[{"item":"ordinary","during":"switch","at":1},{"item":"launch","during":"launch_copy","at":2,"version":"0.7.120"}]}"#;
        std::fs::write(root.join(FILE), old).unwrap();
        record(root, ORDINARY, DURING_RENEWAL, 3);
        assert_eq!(
            read(root).len(),
            2,
            "the newest refusal of an item replaces its older one"
        );
        assert_eq!(forget_older(root, env!("CARGO_PKG_VERSION")), 1);
        assert_eq!(read(root).len(), 1);
        assert_eq!(read(root)[0].during, DURING_RENEWAL);
        assert_eq!(
            forget_older(root, env!("CARGO_PKG_VERSION")),
            0,
            "nothing more to drop"
        );
        assert_eq!(forget_older(root, "99.0.0"), 1);
        assert!(!root.join(FILE).exists(), "an empty file is removed");
    }
    #[test]
    fn a_refusal_is_kept_per_item_until_a_write_of_that_item_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        assert!(read(root).is_empty());
        let too_large: Result<(), String> = Err(crate::external::CREDENTIAL_TOO_LARGE.into());
        note(root, ORDINARY, DURING_SWITCH, &too_large, 10);
        note(root, LAUNCH, DURING_LAUNCH_COPY, &too_large, 11);
        // Another failure says nothing about the size; the newest refusal of an item replaces it.
        note(root, ORDINARY, DURING_RENEWAL, &Err("locked".into()), 12);
        note(root, ORDINARY, DURING_RENEWAL, &too_large, 13);
        assert_eq!(
            read(root),
            [
                Entry {
                    item: LAUNCH.into(),
                    during: DURING_LAUNCH_COPY.into(),
                    at: 11,
                    stamp: None,
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                },
                Entry {
                    item: ORDINARY.into(),
                    during: DURING_RENEWAL.into(),
                    at: 13,
                    stamp: None,
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                },
            ]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // The same refusal again changes nothing, not even its time.
        let stamp = std::fs::metadata(root.join(FILE))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        note(root, ORDINARY, DURING_RENEWAL, &too_large, 20);
        assert_eq!(read(root)[1].at, 13);
        assert_eq!(
            std::fs::metadata(root.join(FILE))
                .unwrap()
                .modified()
                .unwrap(),
            stamp
        );
        note(root, ORDINARY, DURING_SWITCH, &Ok(()), 14);
        assert_eq!(read(root).len(), 1);
        note(root, LAUNCH, DURING_LAUNCH_COPY, &Ok(()), 15);
        assert!(read(root).is_empty());
        assert!(!root.join(FILE).exists(), "nothing kept, no file");
    }
}
