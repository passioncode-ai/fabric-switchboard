//! SB-420: a Claude Code Keychain item Switchboard could not write because it is too large to
//! go through the security tool without its value on the command line. Each refusal is kept in
//! `<data>/keychain-too-large.json` (0600, which item, during what and when; never a value or a
//! size), cleared by the next write of that item that succeeds. `switchboard status` (MCP
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

/// Records that `item` was refused `during` an act at `at` (one entry per item, the newest).
pub fn record(root: &Path, item: &str, during: &str, at: i64) {
    let mut items = read(root);
    items.retain(|e| e.item != item);
    items.push(Entry {
        item: item.into(),
        during: during.into(),
        at,
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
    let mut items = read(root);
    let before = items.len();
    items.retain(|e| e.item != item);
    if items.len() == before {
        return;
    }
    write(root, items);
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
    json!(read(root))
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    at: 11
                },
                Entry {
                    item: ORDINARY.into(),
                    during: DURING_RENEWAL.into(),
                    at: 13
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
        note(root, ORDINARY, DURING_SWITCH, &Ok(()), 14);
        assert_eq!(read(root).len(), 1);
        note(root, LAUNCH, DURING_LAUNCH_COPY, &Ok(()), 15);
        assert!(read(root).is_empty());
        assert!(!root.join(FILE).exists(), "nothing kept, no file");
    }
}
