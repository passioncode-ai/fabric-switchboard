//! SB-403 S3 (DEC-0025): the person's own Claude Code and Codex sign-ins, off limits.
//!
//! On a teammate's computer Switchboard serves seats an organization delegated to it, and the
//! person keeps their own subscription in Claude Desktop and in the ordinary Claude Code. With
//! this setting off (`<data>/own-sign-in` holds `off`), Switchboard never reads or writes that
//! sign-in: the background source sync reads nothing, no account is captured or imported from it,
//! nothing is activated in it, Claude Code's live sign-in is never renewed, Claude Swap is not
//! read, and a launch carries no MCP sign-in out of the ordinary home's credential item. Claude
//! Desktop is outside Switchboard either way: no code names its app, its data folder or its
//! Keychain item (`no_code_names_claude_desktop` below).
//!
//! Absent, the file means on, as every version before this one behaved.
use crate::{external, NativeSources};
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::Provider;

pub const FILE: &str = "own-sign-in";
pub const OFF_LIMITS: &str = "Switchboard does not read or change your own Claude Code or Codex sign-in on this computer (own sign-in is off). Turn it on with switchboard own-sign-in on to use it here.";
pub const UNWRITTEN: &str =
    "The own sign-in setting could not be saved. Check Switchboard's data folder, then retry.";

/// Whether the person's own sign-in is off limits here. Unreadable reads as on: the safe side for
/// a computer that never chose (every earlier version), and a broken file is reported by `status`.
pub fn is_off(root: &Path) -> bool {
    switchboard_core::private_fs::read_private(&root.join(FILE), 64)
        .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).trim() == "off")
}

pub fn status(root: &Path) -> Value {
    let off = is_off(root);
    json!({"own_sign_in": if off { "off" } else { "on" }, "on": !off})
}

/// Saves the choice (0600, by rename) and answers the new status.
pub fn set(root: &Path, on: bool) -> Result<Value, String> {
    switchboard_core::private_fs::private_write(
        &root.join(FILE),
        if on { b"on\n" } else { b"off\n" },
    )
    .map_err(|_| UNWRITTEN.to_string())?;
    crate::oplog::event("own_sign_in", &[("on", crate::oplog::Field::Flag(on))]);
    Ok(status(root))
}

fn refused(_: Provider) -> Result<external::CapturedProfile, String> {
    Err(OFF_LIMITS.into())
}
fn no_live() -> Result<Box<dyn external::LiveItem>, String> {
    Err(OFF_LIMITS.into())
}

/// The sources with every read and write of the person's own sign-in taken out while `off`;
/// what is not a sign-in (session transcripts for limits, Codex's routing file, the shell homes'
/// carried settings) stays as it was.
pub(crate) fn mask(sources: NativeSources, off: bool) -> NativeSources {
    if !off {
        return sources;
    }
    NativeSources {
        current: refused,
        fresh: None,
        activate: |_, _, _, _| Err(OFF_LIMITS.into()),
        live: no_live,
        live_stamp: || None,
        swap: crate::no_swap,
        swap_activity: external::SwapActivity::none,
        swap_signature: || None,
        forget: || {},
        ..sources
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static READS: AtomicUsize = AtomicUsize::new(0);
    static WRITES: AtomicUsize = AtomicUsize::new(0);

    fn counted_read(_: Provider) -> Result<external::CapturedProfile, String> {
        READS.fetch_add(1, Ordering::SeqCst);
        Err("synthetic".into())
    }
    fn counted_live() -> Result<Box<dyn external::LiveItem>, String> {
        READS.fetch_add(1, Ordering::SeqCst);
        Err("synthetic".into())
    }
    fn counted_swap() -> Result<external::ImportBatch, String> {
        READS.fetch_add(1, Ordering::SeqCst);
        Err("synthetic".into())
    }
    const COUNTING: NativeSources = NativeSources {
        current: counted_read,
        fresh: Some(counted_read),
        activate: |_, _, _, _| {
            WRITES.fetch_add(1, Ordering::SeqCst);
            Err("synthetic".into())
        },
        live: counted_live,
        swap: counted_swap,
        ..crate::UNAVAILABLE
    };

    #[test]
    fn the_setting_is_on_until_the_person_turns_it_off() {
        let root = tempfile::tempdir().unwrap();
        assert!(!is_off(root.path()));
        assert_eq!(set(root.path(), false).unwrap()["own_sign_in"], "off");
        assert!(is_off(root.path()));
        assert_eq!(set(root.path(), true).unwrap()["on"], true);
        assert!(!is_off(root.path()));
    }

    #[test]
    fn off_the_sources_never_read_or_write_the_persons_sign_in() {
        let masked = mask(COUNTING, true);
        assert_eq!(
            (masked.current)(Provider::Claude).err().as_deref(),
            Some(OFF_LIMITS)
        );
        assert_eq!(
            masked.fresh(Provider::Codex).err().as_deref(),
            Some(OFF_LIMITS)
        );
        assert!((masked.live)().is_err());
        assert!((masked.swap)().is_err());
        let identity = switchboard_core::ExternalIdentity::default();
        let credential = switchboard_core::Credential::parse(
            Provider::Claude,
            switchboard_core::AuthKind::ApiKey,
            "sk-ant-api03-synthetic-key-for-a-test-only-0000000000000000",
        );
        if let Ok(credential) = credential {
            assert_eq!(
                (masked.activate)(&credential, &identity, None, &mut |_| Ok(())).unwrap_err(),
                OFF_LIMITS
            );
        }
        assert_eq!(READS.load(Ordering::SeqCst), 0);
        assert_eq!(WRITES.load(Ordering::SeqCst), 0);
        // On, the same sources are the person's own again.
        let _ = (mask(COUNTING, false).current)(Provider::Claude);
        assert_eq!(READS.load(Ordering::SeqCst), 1);
    }

    /// Claude Desktop is outside Switchboard: no source file names its bundle, its data folder or
    /// its Keychain item, so nothing here can read or change it, setting on or off. (The one
    /// mention of its app's path, in fabric-sessions-core's process table test, asserts that it
    /// is *not* taken for a coding agent.)
    #[test]
    fn no_code_names_claude_desktop() {
        let forbidden = [
            ["com.anthropic.", "claudefordesktop"].concat(),
            ["Application Support/", "Claude/"].concat(),
            ["Claude ", "Desktop-credentials"].concat(),
            ["Claude", "Desktop"].concat(),
        ];
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut stack = vec![crates.clone(), crates.join("../src-tauri/src")];
        let mut read = 0;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if path.is_dir() {
                    if name != "target" && !name.starts_with('.') {
                        stack.push(path);
                    }
                    continue;
                }
                if !name.ends_with(".rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                read += 1;
                for word in &forbidden {
                    assert!(
                        !text.contains(word.as_str()),
                        "{} names {word}",
                        path.display()
                    );
                }
            }
        }
        assert!(read > 50, "the sources were read ({read} files)");
    }
}
