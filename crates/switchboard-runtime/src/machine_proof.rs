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
//!
//! SB-560 (W11.1, spec S2.2): the proof is typed ([`Proof`]): `pinned`, `matched`, `unproven`,
//! `key_changed`, `refused`. An act on the other machine (a move, a call back, a dispatched run,
//! settings carried there, a session ended there) runs only on `pinned` or `matched`; a read may
//! run `unproven`, and a read never pins a key, so a read-only caller cannot decide which key
//! a name is trusted with. A parseable answer whose signature does not hold is `refused`, pinned
//! or not. A key that changed is kept in `machine-key-changes.json` (0600: the machine, the key it
//! answered with and when) until the person trusts a key for that machine or it matches again;
//! Needs attention lists it as `machine_key_changed`. Every refusal travels to the caller as its
//! own sentence ([`refusal`]), never read as "did not answer over ssh".
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
/// SB-560: an act needs a machine that proved a key trusted here; this one proved none.
pub const PROOF_REQUIRED: &str = "That machine has not proved it is the one Switchboard trusts for it, so nothing was sent. Update Switchboard there to 0.7.104 or later, then retry.";
/// SB-560 (M2-10): an answer to the challenge whose signature does not hold, pinned or not.
pub const PROOF_INVALID: &str = "That machine answered the challenge with a signature that does not hold, so nothing was sent. Check what that ssh name reaches.";
pub const CHANGES_FILE: &str = "machine-key-changes.json";

/// SB-560 (spec S2.2): what one proof of another machine found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof {
    /// Its first proof: its key is trusted from now on.
    Pinned,
    /// Signed by the key trusted for it.
    Matched,
    /// Nothing proved, nothing pinned: an older Switchboard, no answer, or a read that does not
    /// pin. Reads only.
    Unproven,
    /// Signed by another key than the one trusted for it.
    KeyChanged,
    /// A pinned machine that proved nothing, or a signature that does not hold.
    Refused,
}
impl Proof {
    pub fn as_str(self) -> &'static str {
        match self {
            Proof::Pinned => "pinned",
            Proof::Matched => "matched",
            Proof::Unproven => "unproven",
            Proof::KeyChanged => "key_changed",
            Proof::Refused => "refused",
        }
    }
    /// An act on the other machine runs only after one of these.
    pub fn trusted(self) -> bool {
        matches!(self, Proof::Pinned | Proof::Matched)
    }
}

/// SB-560: what a guarded call needs of the proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Reads: may run `unproven`, and never pin a key.
    Read,
    /// Acts (a credential, a request, settings, a session ended there): `pinned` or `matched`.
    Act,
}

/// Whether `error` is one of the proof's own refusals, which a caller passes on unchanged
/// instead of reading it as "did not answer".
pub fn refusal(error: &str) -> bool {
    [KEY_CHANGED, PROOF_MISSING, PROOF_REQUIRED, PROOF_INVALID, KEYS_UNREADABLE].contains(&error)
}

/// The refusal in a runner's answer, when the guard refused before anything ran.
pub fn refused<T>(answer: &Result<T, String>) -> Option<String> {
    answer.as_ref().err().filter(|e| refusal(e)).cloned()
}

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
    /// No answer, or a refusal of the challenge.
    Failed,
    /// An answer whose signature does not hold for this challenge (M2-10).
    Invalid,
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
        Answer::Invalid
    }
}

/// One machine whose answer was signed by another key than the one trusted for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyChange {
    pub machine: String,
    /// The key it answered with (public), to trust if it is still the person's machine.
    pub public_key: String,
    /// When a proof first saw it; kept across later proofs that see it again.
    pub since: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Changes {
    machines: BTreeMap<String, KeyChange>,
}

fn read_changes(root: &Path) -> Changes {
    switchboard_core::private_fs::read_private(&root.join(CHANGES_FILE), 64 * 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Records or clears `machine`'s changed key; a failed write is logged and never refuses a proof.
fn note_change(root: &Path, machine: &str, key: Option<&str>, now: i64) {
    let file = root.join(CHANGES_FILE);
    let mut changes = read_changes(root);
    let changed = match key {
        Some(key) => {
            let since = changes
                .machines
                .get(machine)
                .filter(|c| c.public_key == key)
                .map_or(now, |c| c.since);
            let entry = KeyChange {
                machine: machine.to_owned(),
                public_key: key.to_owned(),
                since,
            };
            changes.machines.insert(machine.to_owned(), entry.clone()) != Some(entry)
        }
        None => changes.machines.remove(machine).is_some(),
    };
    if !changed {
        return;
    }
    let written = if changes.machines.is_empty() {
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(()),
            _ => Ok(()),
        }
    } else {
        serde_json::to_vec_pretty(&changes)
            .map_err(|_| ())
            .and_then(|bytes| {
                switchboard_core::private_fs::private_write(&file, &bytes).map_err(|_| ())
            })
    };
    if written.is_err() {
        crate::oplog::event(
            "machine_key_change",
            &[("result", crate::oplog::Field::Code("unwritten"))],
        );
    }
}

/// SB-560: the machines whose last proof answered with another key than the one trusted for
/// them, for Needs attention (`machine_key_changed`). Public keys and times only.
pub fn key_changes(root: &Path) -> Vec<KeyChange> {
    read_changes(root).machines.into_values().collect()
}

/// Proves `target` against its pin. `Act` pins its key on the first proof; `Read` never pins.
/// `Err` only for a name that is not one or keys that cannot be read; every other outcome is a
/// [`Proof`].
pub fn verify(
    root: &Path,
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
    now: i64,
    need: Need,
) -> Result<Proof, String> {
    Ok(verify_answer(root, target, program, run, now, need)?.0)
}

/// What the other machine's answer to the challenge was, beside the proof: a refusal says
/// "did not answer" when it never answered, and "does not hold" when its signature failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    Nothing,
    Answer,
    Invalid,
}

/// [`verify`], with what the other machine's answer was.
fn verify_answer(
    root: &Path,
    target: &str,
    program: &Program<'_>,
    run: &Run<'_>,
    now: i64,
    need: Need,
) -> Result<(Proof, Seen), String> {
    if !crate::account_send::target_valid(target) {
        return Err(TARGET_INVALID.into());
    }
    let mut pins = read(root)?;
    let machine = name(target);
    let pinned = pins.machines.get(&machine).map(|p| p.public_key.clone());
    let answer = challenge(target, program, run);
    let seen = match answer {
        Answer::Failed => Seen::Nothing,
        Answer::Invalid => Seen::Invalid,
        _ => Seen::Answer,
    };
    let proof = match (answer, pinned) {
        (Answer::Proved(key), Some(pin)) if key == pin => Proof::Matched,
        (Answer::Proved(key), Some(_)) => {
            note_change(root, &machine, Some(&key), now);
            return Ok((Proof::KeyChanged, seen));
        }
        (Answer::Proved(_), None) if need == Need::Read => Proof::Unproven,
        (Answer::Proved(key), None) => {
            pins.machines.insert(
                machine.clone(),
                Pin {
                    public_key: key,
                    pinned_at: now,
                    by: "first-proof".into(),
                },
            );
            write(root, &pins)?;
            Proof::Pinned
        }
        (Answer::Invalid, _) => Proof::Refused,
        (Answer::Unsupported | Answer::Failed, Some(_)) => Proof::Refused,
        // An older Switchboard, or no answer: the call itself fails the same way when unreachable.
        (Answer::Unsupported | Answer::Failed, None) => Proof::Unproven,
    };
    if proof.trusted() {
        note_change(root, &machine, None, now);
    }
    Ok((proof, seen))
}

/// The sentence a guarded call is refused with, or none when it may run.
fn gate(proof: Proof, seen: Seen, need: Need) -> Option<&'static str> {
    match proof {
        Proof::Pinned | Proof::Matched => None,
        Proof::KeyChanged => Some(KEY_CHANGED),
        Proof::Refused if seen == Seen::Invalid => Some(PROOF_INVALID),
        Proof::Refused => Some(PROOF_MISSING),
        Proof::Unproven if need == Need::Read => None,
        // Never answered at all: the caller's own "did not answer" stays true.
        Proof::Unproven if seen == Seen::Nothing => Some(crate::account_send::UNREACHABLE),
        Proof::Unproven => Some(PROOF_REQUIRED),
    }
}

/// A runner for one target that proves the machine before its first command and refuses every
/// command after a failed proof. Each operation makes its own, so one proof covers one operation.
pub struct Guard<'a> {
    root: std::path::PathBuf,
    target: String,
    path: String,
    data_dir: Option<String>,
    need: Need,
    inner: &'a Run<'a>,
    proved: OnceLock<Result<(Proof, Seen), String>>,
}

impl<'a> Guard<'a> {
    /// A guard for an act (`Need::Act`): the machine must prove a trusted key.
    pub fn new(root: &Path, target: &str, program: &Program<'_>, inner: &'a Run<'a>) -> Self {
        Self::with(root, target, program, inner, Need::Act)
    }

    pub fn with(
        root: &Path,
        target: &str,
        program: &Program<'_>,
        inner: &'a Run<'a>,
        need: Need,
    ) -> Self {
        Guard {
            root: root.to_owned(),
            target: target.to_owned(),
            path: program.path.to_owned(),
            data_dir: program.data_dir.map(str::to_owned),
            need,
            inner,
            proved: OnceLock::new(),
        }
    }

    fn proved(&self) -> Result<(Proof, Seen), String> {
        self.proved
            .get_or_init(|| {
                let program = Program {
                    path: &self.path,
                    data_dir: self.data_dir.as_deref(),
                };
                verify_answer(
                    &self.root,
                    &self.target,
                    &program,
                    self.inner,
                    crate::monitor::now(),
                    self.need,
                )
            })
            .clone()
    }

    /// What the proof found, running it now if no command ran yet.
    pub fn proof(&self) -> Result<Proof, String> {
        self.proved().map(|(proof, _)| proof)
    }

    pub fn run(&self, args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
        let (proof, seen) = self.proved()?;
        if let Some(refusal) = gate(proof, seen, self.need) {
            return Err(refusal.into());
        }
        (self.inner)(args, stdin)
    }
}

/// The runner every act on another machine goes through: the system's ssh, behind the proof,
/// which must find a trusted key (`Need::Act`).
pub fn ssh(root: &Path, target: &str, program: &Program<'_>) -> Guard<'static> {
    Guard::new(root, target, program, &crate::account_send::ssh_runner)
}

/// SB-560: the runner of a read of another machine: proved, never pinning, `unproven` allowed.
pub fn ssh_read(root: &Path, target: &str, program: &Program<'_>) -> Guard<'static> {
    Guard::with(
        root,
        target,
        program,
        &crate::account_send::ssh_runner,
        Need::Read,
    )
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
    note_change(root, &name(machine), None, now);
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
            verify(here.path(), "Studio", &PROGRAM, &run, 100, Need::Act),
            Ok(Proof::Pinned)
        );
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &run, 200, Need::Act),
            Ok(Proof::Matched),
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
            verify(here.path(), "studio", &PROGRAM, &original, 1, Need::Act),
            Ok(Proof::Pinned)
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
            verify(here.path(), "studio", &PROGRAM, &impostor, 6, Need::Act),
            Ok(Proof::Matched)
        );
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &original, 7, Need::Act),
            Ok(Proof::KeyChanged),
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
            verify(here.path(), "laptop", &PROGRAM, &old, 1, Need::Act),
            Ok(Proof::Unproven)
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
                2,
                Need::Act
            ),
            Ok(Proof::Pinned)
        );
        assert_eq!(
            verify(here.path(), "laptop", &PROGRAM, &old, 3, Need::Act),
            Ok(Proof::Refused),
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
                1,
                Need::Act
            ),
            Ok(Proof::Pinned)
        );
        // An answer recorded from an earlier challenge, replayed: the nonce differs every time.
        let recorded = prove(there.path(), &store, &"ab".repeat(32)).unwrap();
        let replay = move |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Ok(json!({"ok": true, "data": recorded}).to_string())
        };
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &replay, 2, Need::Act),
            Ok(Proof::Refused)
        );
        let guard = Guard::new(here.path(), "studio", &PROGRAM, &replay);
        assert_eq!(
            guard.run(&["studio".into(), "--".into(), "x".into()], None),
            Err(PROOF_INVALID.into())
        );
        assert_eq!(prove(there.path(), &store, "zz"), Err(NONCE_INVALID.into()));
        assert_eq!(nonce_bytes(&"g".repeat(64)), None);
    }
}

#[cfg(test)]
mod typed {
    //! SB-560 (spec S2.2): the typed proof, what each need lets run, and the key change kept for
    //! Needs attention.
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
    fn line() -> Vec<String> {
        vec!["studio".into(), "--".into(), "'accounts' 'list'".into()]
    }

    #[test]
    fn a_read_runs_unproven_and_never_pins_and_an_act_pins() {
        let (here, there) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let store = open_store(there.path());
        let ran = Mutex::new(Vec::new());
        let run = machine(there.path(), &store, &ran);
        let read = Guard::with(here.path(), "studio", &PROGRAM, &run, Need::Read);
        assert!(read.run(&line(), None).is_ok());
        assert_eq!(read.proof(), Ok(Proof::Unproven));
        assert!(!here.path().join(FILE).exists(), "a read pins nothing (M3-08)");

        let act = Guard::new(here.path(), "studio", &PROGRAM, &run);
        assert!(act.run(&line(), None).is_ok());
        assert_eq!(act.proof(), Ok(Proof::Pinned));
        let read = Guard::with(here.path(), "studio", &PROGRAM, &run, Need::Read);
        read.run(&line(), None).unwrap();
        assert_eq!(read.proof(), Ok(Proof::Matched));
    }

    #[test]
    fn an_act_needs_a_trusted_key_and_says_why_it_ran_nothing() {
        let here = tempfile::tempdir().unwrap();
        let ran = Mutex::new(Vec::new());
        let old = |args: &[String], _: Option<&[u8]>| -> Result<String, String> {
            ran.lock().unwrap().push(args.last().cloned().unwrap_or_default());
            Err(r#"{"ok":false,"error":"Invalid arguments. Run switchboard --help."}"#.into())
        };
        let act = Guard::new(here.path(), "laptop", &PROGRAM, &old);
        assert_eq!(act.run(&line(), None), Err(PROOF_REQUIRED.into()));
        assert_eq!(ran.lock().unwrap().len(), 1, "only the challenge was sent");
        let read = Guard::with(here.path(), "laptop", &PROGRAM, &old, Need::Read);
        assert!(read.run(&line(), None).is_err(), "the read itself ran and failed");
        assert_eq!(ran.lock().unwrap().len(), 3);

        let silent = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Err("ssh: connect to host laptop port 22: Connection refused".into())
        };
        let act = Guard::new(here.path(), "laptop", &PROGRAM, &silent);
        assert_eq!(
            act.run(&line(), None),
            Err(crate::account_send::UNREACHABLE.into()),
            "a machine that never answered is still said not to answer"
        );
        assert!(refusal(KEY_CHANGED) && refusal(PROOF_REQUIRED) && refusal(PROOF_INVALID));
        assert!(!refusal(crate::account_send::UNREACHABLE));
    }

    #[test]
    fn a_bad_signature_from_an_unpinned_machine_is_refused_not_unproven() {
        let here = tempfile::tempdir().unwrap();
        let forged = |_: &[String], _: Option<&[u8]>| -> Result<String, String> {
            Ok(json!({"ok": true, "data": {"public_key": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "signature": "AAAA"}}).to_string())
        };
        assert_eq!(
            verify(here.path(), "studio", &PROGRAM, &forged, 1, Need::Read),
            Ok(Proof::Refused)
        );
        let read = Guard::with(here.path(), "studio", &PROGRAM, &forged, Need::Read);
        assert_eq!(read.run(&line(), None), Err(PROOF_INVALID.into()));
    }

    #[test]
    fn a_changed_key_is_kept_for_attention_until_trusted() {
        let here = tempfile::tempdir().unwrap();
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (open_store(first.path()), open_store(second.path()));
        let ran = Mutex::new(Vec::new());
        let original = machine(first.path(), &a, &ran);
        let other = machine(second.path(), &b, &ran);
        verify(here.path(), "studio", &PROGRAM, &original, 1, Need::Act).unwrap();
        assert!(key_changes(here.path()).is_empty());

        let read = Guard::with(here.path(), "Studio", &PROGRAM, &other, Need::Read);
        assert_eq!(read.run(&line(), None), Err(KEY_CHANGED.into()), "reads too");
        verify(here.path(), "studio", &PROGRAM, &other, 9, Need::Act).unwrap();
        let changes = key_changes(here.path());
        let key = crate::settings_signing::signer(second.path(), &b).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].machine, "studio");
        assert_eq!(changes[0].public_key, key["public_key"].as_str().unwrap());
        assert_ne!(changes[0].since, 9, "the first sighting is kept");
        let mode = std::fs::metadata(here.path().join(CHANGES_FILE)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(mode.permissions().mode() & 0o777, 0o600);
        }
        #[cfg(not(unix))]
        let _ = mode;

        trust(here.path(), "studio", key["public_key"].as_str().unwrap(), 10).unwrap();
        assert!(key_changes(here.path()).is_empty());
        assert!(!here.path().join(CHANGES_FILE).exists());
    }
}
