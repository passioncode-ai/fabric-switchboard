//! SB-272: an administrator's settings file can be signed, so a new member's computer knows the
//! file is the one the organization's administrator wrote. The key is an Ed25519 key of this
//! computer's own: its private half is a vault item of Switchboard's (never printed, never in a
//! file), and `settings signer` prints its public half for the organization to publish. A member
//! imports with `--signer <that key>`, and an import whose file is unsigned, signed by another
//! key or changed after signing is refused. Without `--signer` the preview still says whether the
//! file is signed and by which key; a file changed after signing is refused either way.

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// Beside the store: the vault item that holds the private key and the public key, never the
/// private one.
const FILE: &str = "settings-signing.json";
/// Refusals; `person-errors.json` gives each its sentence.
pub const SIGNING_UNREADABLE: &str = "Settings signing key unreadable";
pub const SIGNER_INVALID: &str = "Settings signer invalid";
pub const NOT_SIGNED_BY: &str = "Settings file not signed by that key";
pub const SIGNATURE_INVALID: &str = "Settings file signature invalid";
/// One key creation at a time in this process, so a vault item is never orphaned.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    /// The vault item holding the PKCS#8 private key, in base64.
    item: String,
    /// The public key, base64 (32 bytes).
    public_key: String,
    created_at: i64,
}

/// A file's signature: the signer's public key and the signature, both base64.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub key: String,
    pub sig: String,
}

fn read_link(root: &Path) -> Result<Option<Link>, String> {
    let file = root.join(FILE);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    let bytes = switchboard_core::private_fs::read_private(&file, 16 * 1024)
        .map_err(|_| SIGNING_UNREADABLE)?;
    let link: Link = serde_json::from_slice(&bytes).map_err(|_| SIGNING_UNREADABLE)?;
    if public_key(&link.public_key).is_none() {
        return Err(SIGNING_UNREADABLE.into());
    }
    Ok(Some(link))
}

/// The 32 bytes of an Ed25519 public key written in base64, when it is one.
pub fn public_key(text: &str) -> Option<[u8; 32]> {
    STANDARD.decode(text.trim()).ok()?.try_into().ok()
}

/// The vault item of the signing key, for `uninstall`.
pub(crate) fn vault_item(root: &Path) -> Option<String> {
    read_link(root).ok().flatten().map(|l| l.item)
}

/// This computer's signing key, made on first use: the private half into the vault, then the
/// file. A file that cannot be written leaves no new vault item behind.
fn key_pair(
    root: &Path,
    store: &switchboard_core::Store,
) -> Result<(Ed25519KeyPair, String), String> {
    let _turn = TURN.lock().map_err(|_| SIGNING_UNREADABLE)?;
    if let Some(link) = read_link(root)? {
        let pkcs8 = STANDARD
            .decode(store.secret(&link.item)?)
            .map_err(|_| SIGNING_UNREADABLE)?;
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| SIGNING_UNREADABLE)?;
        if STANDARD.encode(pair.public_key().as_ref()) != link.public_key {
            return Err(SIGNING_UNREADABLE.into());
        }
        return Ok((pair, link.public_key));
    }
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| SIGNING_UNREADABLE)?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| SIGNING_UNREADABLE)?;
    let public = STANDARD.encode(pair.public_key().as_ref());
    let item = store.put_secret(None, &STANDARD.encode(pkcs8.as_ref()))?;
    let link = Link {
        item: item.clone(),
        public_key: public.clone(),
        created_at: crate::monitor::now(),
    };
    let written = serde_json::to_vec(&link)
        .map_err(|_| SIGNING_UNREADABLE.to_string())
        .and_then(|bytes| switchboard_core::private_fs::private_write(&root.join(FILE), &bytes));
    if let Err(error) = written {
        let _ = store.delete_secret(&item);
        return Err(error);
    }
    Ok((pair, public))
}

/// The public key the organization publishes, made on first use.
pub fn signer(root: &Path, store: &switchboard_core::Store) -> Result<Value, String> {
    let (_, public) = key_pair(root, store)?;
    Ok(json!({"public_key": public}))
}

/// Signs `message` (a bundle's bytes without its signature) with this computer's key.
pub(crate) fn sign(
    root: &Path,
    store: &switchboard_core::Store,
    message: &[u8],
) -> Result<Signature, String> {
    let (pair, public) = key_pair(root, store)?;
    Ok(Signature {
        key: public,
        sig: STANDARD.encode(pair.sign(message).as_ref()),
    })
}

/// What a file's signature says: `unsigned`, `signed` by `signer`, or `invalid` (a key or
/// signature that is not one, or a file changed after it was signed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub state: &'static str,
    pub signer: Option<String>,
    /// With `--signer`: whether the file is signed by exactly that key.
    pub expected: Option<bool>,
}

/// Checks `signature` over `message`, and against `expected` when the person named a key.
pub(crate) fn verify(
    message: &[u8],
    signature: Option<&Signature>,
    expected: Option<&str>,
) -> Verdict {
    let state = match signature {
        None => "unsigned",
        Some(s) => {
            let valid = public_key(&s.key)
                .zip(STANDARD.decode(s.sig.trim()).ok())
                .is_some_and(|(key, sig)| {
                    UnparsedPublicKey::new(&ED25519, key)
                        .verify(message, &sig)
                        .is_ok()
                });
            if valid {
                "signed"
            } else {
                "invalid"
            }
        }
    };
    let signer = signature.map(|s| s.key.trim().to_owned());
    let expected = expected.map(|key| state == "signed" && signer.as_deref() == Some(key.trim()));
    Verdict {
        state,
        signer,
        expected,
    }
}

/// The refusal an import with this verdict meets, if any: a changed file always; with `--signer`,
/// anything but a file signed by that key.
pub(crate) fn refusal(verdict: &Verdict) -> Option<&'static str> {
    if verdict.state == "invalid" {
        return Some(SIGNATURE_INVALID);
    }
    (verdict.expected == Some(false)).then_some(NOT_SIGNED_BY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_store(root: &Path) -> switchboard_core::Store {
        switchboard_core::Store::open(
            root.to_path_buf(),
            std::sync::Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap()
    }

    #[test]
    fn a_key_is_made_once_kept_in_the_vault_and_only_its_public_half_is_shown() {
        let root = tempfile::tempdir().unwrap();
        let store = open_store(root.path());
        let first = signer(root.path(), &store).unwrap();
        let again = signer(root.path(), &store).unwrap();
        assert_eq!(first, again, "the same key on every call");
        let public = first["public_key"].as_str().unwrap();
        assert!(public_key(public).is_some());
        let file = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        let item = vault_item(root.path()).unwrap();
        let private = store.secret(&item).unwrap();
        assert!(
            !file.contains(&private),
            "the private key never lands in the file"
        );
        assert!(!first.to_string().contains(&private));
    }

    #[test]
    fn signed_unsigned_changed_and_other_keys_read_as_they_are() {
        let root = tempfile::tempdir().unwrap();
        let store = open_store(root.path());
        let message = br#"{"format":"switchboard-settings/1"}"#;
        let signature = sign(root.path(), &store, message).unwrap();
        let key = signature.key.clone();

        let signed = verify(message, Some(&signature), None);
        assert_eq!(
            (signed.state, signed.signer.as_deref()),
            ("signed", Some(key.as_str()))
        );
        assert_eq!(refusal(&signed), None);
        let named = verify(message, Some(&signature), Some(&key));
        assert_eq!((named.expected, refusal(&named)), (Some(true), None));

        let unsigned = verify(message, None, None);
        assert_eq!((unsigned.state, refusal(&unsigned)), ("unsigned", None));
        let wanted = verify(message, None, Some(&key));
        assert_eq!(refusal(&wanted), Some(NOT_SIGNED_BY));

        let changed = verify(
            br#"{"format":"switchboard-settings/1","x":1}"#,
            Some(&signature),
            None,
        );
        assert_eq!(
            (changed.state, refusal(&changed)),
            ("invalid", Some(SIGNATURE_INVALID))
        );

        let other_root = tempfile::tempdir().unwrap();
        let other_store = open_store(other_root.path());
        let other = signer(other_root.path(), &other_store).unwrap();
        let by_other = verify(message, Some(&signature), other["public_key"].as_str());
        assert_eq!(
            (by_other.expected, refusal(&by_other)),
            (Some(false), Some(NOT_SIGNED_BY))
        );

        let forged = Signature {
            key: other["public_key"].as_str().unwrap().to_owned(),
            sig: signature.sig.clone(),
        };
        assert_eq!(verify(message, Some(&forged), None).state, "invalid");
        let garbage = Signature {
            key: "not base64".into(),
            sig: "x".into(),
        };
        assert_eq!(verify(message, Some(&garbage), None).state, "invalid");
    }
}
