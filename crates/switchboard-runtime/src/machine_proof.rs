//! SB-357: prove the other machine is the one a move trusted. Before this, Switchboard trusted
//! whatever ssh reached without a prompt and the JSON its `switchboard` printed: a host that
//! changed hands, or an ssh name pointed at another computer, was trusted the same.
//!
//! Every call that acts on another machine (a move, a call back, a remote read or check, settings
//! carried there, a dispatched run) first asks it to sign a fresh random challenge with its own
//! settings signing key (SB-272, an Ed25519 key whose private half never leaves its vault):
//! `switchboard --json machines prove --nonce <64 hex>`. The first answer pins that key here under
//! the machine's ssh name (`machine-keys.json`, 0600, public keys and times only); every later
//! answer must be signed by the same key, or the call is refused before anything else is sent.
//! A machine whose Switchboard is older than the challenge is let through while it has no pin
//! (nothing is pinned), and refused once it has one, so a changed machine cannot hide behind an
//! old answer. The person accepts a new key with `switchboard machines trust <machine> --key
//! <key>`, the key printed by `switchboard settings signer` on that machine.
use crate::account_send::{data, remote, Program, Run, TARGET_INVALID};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

pub const FILE: &str = "machine-keys.json";
/// Signed before the challenge, so this signature can never be read as a settings file's.
const DOMAIN: &[u8] = b"switchboard-machine-proof/1\n";

pub const KEY_CHANGED: &str = "That machine answered with another key than the one Switchboard trusted for it, so nothing was sent. If it is still your machine, run switchboard settings signer there and trust its key with switchboard machines trust.";
pub const PROOF_MISSING: &str = "That machine did not prove it is the one Switchboard trusted for it, so nothing was sent. Update Switchboard there and make sure it can open its vault, then retry.";
pub const NONCE_INVALID: &str = "The challenge is not 64 hexadecimal characters.";
pub const KEY_INVALID: &str = "That is not a machine's key. Give the key switchboard settings signer printed on that machine.";
pub const KEYS_UNREADABLE: &str = "The machine keys Switchboard trusts could not be read or saved. Check its data folder, then retry.";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pins {
    /// The machine's ssh name (lowercase) → the key it proved with.
    machines: BTreeMap<String, Pin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    public_key: String,
    pinned_at: i64,
    /// `first-proof` when its first answer pinned it, `person` when the person trusted it.
    by: String,
}

fn read(root: &Path) -> Result<Pins, String> {
    let file = root.join(FILE);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(Pins::default());
    }
    let bytes = switchboard_core::private_fs::read_private(&file, 256 * 1024)
        .map_err(|_| KEYS_UNREADABLE)?;
    serde_json::from_slice(&bytes).map_err(|_| KEYS_UNREADABLE.into())
}

fn write(root: &Path, pins: &Pins) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(pins).map_err(|_| KEYS_UNREADABLE)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| KEYS_UNREADABLE.into())
}

fn name(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

fn message(nonce: &[u8]) -> Vec<u8> {
    [DOMAIN, nonce].concat()
}

fn nonce_bytes(hex: &str) -> Option<Vec<u8>> {
    (hex.len() == 64)
        .then(|| {
            (0..32)
                .map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

/// The other side: signs the challenge with this computer's settings signing key.
pub fn prove(root: &Path, store: &switchboard_core::Store, nonce: &str) -> Result<Value, String> {
    let bytes = nonce_bytes(nonce).ok_or(NONCE_INVALID)?;
    let signature = crate::settings_signing::sign(root, store, &message(&bytes))?;
    Ok(json!({"public_key": signature.key, "signature": signature.sig}))
}

/// What one challenge found, before the pin is consulted.
enum Answer {
    /// Signed by this key.
    Proved(String),
    /// A Switchboard that does not know the challenge (older than 0.7.104).
    Unsupported,
    /// No answer, a refusal or a signature that does not hold.
    Failed,
}

fn challenge(target: &str, program: &Program<'_>, run: &Run<'_>) -> Answer {
    let mut nonce = [0u8; 32];
    if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce).is_err() {
        return Answer::Failed;
    }
    let hex: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let args = ["--json", "machines", "prove", "--nonce", hex.as_str()];
    let output = match run(
        &[target.to_owned(), "--".to_owned(), remote(program, &args)],
        None,
    ) {
        Ok(out) | Err(out) => out,
    };
    let Some(answer) = data(&output) else {
        // An older Switchboard answers its generic refusal for a command it does not know.
        let unknown = output
            .lines()
            .any(|l| l.contains("Invalid arguments. Run switchboard --help."));
        return if unknown {
            Answer::Unsupported
        } else {
            Answer::Failed
        };
    };
    let key = answer["public_key"].as_str().unwrap_or_default().trim();
    let holds = crate::settings_signing::public_key(key)
        .zip(
            STANDARD
                .decode(answer["signature"].as_str().unwrap_or_default().trim())
                .ok(),
        )
        .is_some_and(|(public, sig)| {
            UnparsedPublicKey::new(&ED25519, public)
                .verify(&message(&nonce), &sig)
                .is_ok()
        });
    if holds {
        Answer::Proved(key.to_owned())
    } else {
        Answer::Failed
    }
}

/// Proves `target` against its pin, pinning its key on the first proof. `Ok` names what happened:
/// `pinned`, `matched` or `unproven` (an older Switchboard, nothing pinned yet).
pub fn verify(
    root: &Path,
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
    now: i64,
) -> Result<&'static str, String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    let mut pins = read(root)?;
    let pinned = pins
        .machines
        .get(&name(target))
        .map(|p| p.public_key.clone());
    match (challenge(target, program, run), pinned) {
        (Answer::Proved(key), Some(pin)) if key == pin => Ok("matched"),
        (Answer::Proved(_), Some(_)) => Err(KEY_CHANGED.into()),
        (Answer::Proved(key), None) => {
            pins.machines.insert(
                name(target),
                Pin {
                    public_key: key,
                    pinned_at: now,
                    by: "first-proof".into(),
                },
            );
            write(root, &pins)?;
            Ok("pinned")
        }
        (Answer::Unsupported, None) => Ok("unproven"),
        (Answer::Unsupported | Answer::Failed, Some(_)) => Err(PROOF_MISSING.into()),
        // Unreachable or refused: the call itself would fail the same way; say so as before.
        (Answer::Failed, None) => Ok("unproven"),
    }
}

/// A runner for one target that proves the machine before its first command and refuses every
/// command after a failed proof. Each operation makes its own, so one proof covers one operation.
pub struct Guard<'a> {
    root: std::path::PathBuf,
    target: String,
    path: String,
    data_dir: Option<String>,
    inner: &'a Run<'a>,
    proved: OnceLock<Result<&'static str, String>>,
}

impl<'a> Guard<'a> {
    pub fn new(root: &Path, target: &str, program: &Program<'_>, inner: &'a Run<'a>) -> Self {
        Guard {
            root: root.to_owned(),
            target: target.to_owned(),
            path: program.path.to_owned(),
            data_dir: program.data_dir.map(str::to_owned),
            inner,
            proved: OnceLock::new(),
        }
    }

    /// What the proof found, running it now if no command ran yet.
    pub fn proof(&self) -> Result<&'static str, String> {
        self.proved
            .get_or_init(|| {
                let program = Program {
                    path: &self.path,
                    data_dir: self.data_dir.as_deref(),
                };
                verify(
                    &self.root,
                    &self.target,
                    &program,
                    self.inner,
                    crate::monitor::now(),
                )
            })
            .clone()
    }

    pub fn run(&self, args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
        self.proof()?;
        (self.inner)(args, stdin)
    }
}

/// The runner every call to another machine goes through: the system's ssh, behind the proof.
pub fn ssh(root: &Path, target: &str, program: &Program<'_>) -> Guard<'static> {
    Guard::new(root, target, program, &crate::account_send::ssh_runner)
}

/// The person accepts `key` for `machine`: the next proof must be signed by it.
pub fn trust(root: &Path, machine: &str, key: &str, now: i64) -> Result<Value, String> {
    if !crate::account_send::target_valid(machine) {
        return Err(TARGET_INVALID.into());
    }
    let key = key.trim();
    if crate::settings_signing::public_key(key).is_none() {
        return Err(KEY_INVALID.into());
    }
    let mut pins = read(root)?;
    let before = pins
        .machines
        .insert(
            name(machine),
            Pin {
                public_key: key.to_owned(),
                pinned_at: now,
                by: "person".into(),
            },
        )
        .map(|p| p.public_key);
    write(root, &pins)?;
    Ok(json!({"machine": name(machine), "public_key": key, "replaced": before}))
}

/// The keys trusted here, by machine name: public keys and times only.
pub fn list(root: &Path) -> Value {
    match read(root) {
        Ok(pins) => json!(pins
            .machines
            .iter()
            .map(|(m, p)| json!({"machine": m, "public_key": p.public_key, "pinned_at": p.pinned_at, "by": p.by}))
            .collect::<Vec<_>>()),
        Err(error) => json!({"error": error}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn open_store(root: &Path) -> switchboard_core::Store {
        switchboard_core::Store::open(
            root.to_path_buf(),
            std::sync::Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap()
    }

    const PROGRAM: Program<'static> = Program {
        path: "/x/switchboard",
        data_dir: None,
    };

    /// "ssh" into another Switchboard data folder: answers `machines prove` from that store and
    /// anything else with `ok`, counting the commands it ran.
    fn machine<'a>(
        there: &'a Path,
        store: &'a switchboard_core::Store,
        ran: &'a Mutex<Vec<String>>,
    ) -> impl Fn(&[String], Option<&[u8]>) -> Result<String, String> + 'a {
        move |args, _| {
            let command = args.last().cloned().unwrap_or_default();
            ran.lock().unwrap().push(command.clone());
            if command.contains("'prove'") {
                let nonce = command
                    .split_whitespace()
                    .last()
                    .unwrap()
                    .trim_matches('\'')
                    .to_owned();
                let answer = prove(there, store, &nonce)?;
                return Ok(json!({"ok": true, "data": answer}).to_string());
            }
            Ok(json!({"ok": true, "data": {}}).to_string())
        }
    }

    #[test]
    fn the_first_proof_pins_the_key_and_the_same_machine_matches_it() {
        let (here, there) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let store = open_store(there.path());
        let ran = Mutex::new(Vec::new());
        let run = machine(there.path(), &store, &ran);
        assert_eq!(
            verify(here.path(), "Studio", &PROGRAM, &run, 100),
            Ok("pinned")
        );
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &run, 200),
            Ok("matched"),
            "the ssh name is compared without case"
        );
        let file = std::fs::read_to_string(here.path().join(FILE)).unwrap();
        let signer = crate::settings_signing::signer(there.path(), &store).unwrap();
        assert!(file.contains(signer["public_key"].as_str().unwrap()));
        let item = crate::settings_signing::vault_item(there.path()).unwrap();
        assert!(
            !file.contains(&store.secret(&item).unwrap()),
            "only the public key is kept here"
        );
    }

    #[test]
    fn another_machine_behind_the_same_name_is_refused_until_the_person_trusts_it() {
        let here = tempfile::tempdir().unwrap();
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (first_store, second_store) = (open_store(first.path()), open_store(second.path()));
        let ran = Mutex::new(Vec::new());
        let original = machine(first.path(), &first_store, &ran);
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &original, 1),
            Ok("pinned")
        );

        // The name now reaches another computer: nothing but the challenge is sent to it.
        let ran = Mutex::new(Vec::new());
        let impostor = machine(second.path(), &second_store, &ran);
        let guard = Guard::new(here.path(), "studio", &PROGRAM, &impostor);
        assert_eq!(
            guard.run(
                &["studio".into(), "--".into(), "'accounts' 'add'".into()],
                None
            ),
            Err(KEY_CHANGED.into())
        );
        let sent = ran.lock().unwrap().clone();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].contains("'prove'"));

        let key = crate::settings_signing::signer(second.path(), &second_store).unwrap();
        let trusted = trust(
            here.path(),
            "studio",
            key["public_key"].as_str().unwrap(),
            5,
        )
        .unwrap();
        assert!(trusted["replaced"].is_string());
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &impostor, 6),
            Ok("matched")
        );
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &original, 7),
            Err(KEY_CHANGED.into()),
            "the old key is no longer trusted"
        );
        assert_eq!(
            trust(here.path(), "studio", "not a key", 8),
            Err(KEY_INVALID.into())
        );
    }

    #[test]
    fn an_older_switchboard_passes_only_while_nothing_is_pinned() {
        let here = tempfile::tempdir().unwrap();
        let old = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err(r#"{"ok":false,"error":"Invalid arguments. Run switchboard --help."}"#.into())
        };
        assert_eq!(
            verify(here.path(), "laptop", &PROGRAM, &old, 1),
            Ok("unproven")
        );
        assert!(!here.path().join(FILE).exists(), "nothing is pinned");

        let there = tempfile::tempdir().unwrap();
        let store = open_store(there.path());
        let ran = Mutex::new(Vec::new());
        assert_eq!(
            verify(
                here.path(),
                "laptop",
                &PROGRAM,
                &machine(there.path(), &store, &ran),
                2
            ),
            Ok("pinned")
        );
        assert_eq!(
            verify(here.path(), "laptop", &PROGRAM, &old, 3),
            Err(PROOF_MISSING.into()),
            "a pinned machine cannot fall back to an answer that proves nothing"
        );
    }

    #[test]
    fn a_signature_that_does_not_hold_or_a_replayed_answer_proves_nothing() {
        let here = tempfile::tempdir().unwrap();
        let there = tempfile::tempdir().unwrap();
        let store = open_store(there.path());
        let ran = Mutex::new(Vec::new());
        assert_eq!(
            verify(
                here.path(),
                "studio",
                &PROGRAM,
                &machine(there.path(), &store, &ran),
                1
            ),
            Ok("pinned")
        );
        // An answer recorded from an earlier challenge, replayed: the nonce differs every time.
        let recorded = prove(there.path(), &store, &"ab".repeat(32)).unwrap();
        let replay = move |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Ok(json!({"ok": true, "data": recorded}).to_string())
        };
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &replay, 2),
            Err(PROOF_MISSING.into())
        );
        assert_eq!(prove(there.path(), &store, "zz"), Err(NONCE_INVALID.into()));
        assert_eq!(nonce_bytes(&"g".repeat(64)), None);
    }
}
