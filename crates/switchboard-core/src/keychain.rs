//! macOS credential storage policy: which Keychain namespace a build uses, who may read
//! what it writes, and the one-time move of items written by earlier versions.
//!
//! Decision record: docs/KEYCHAIN.md. In short, a signed build writes to
//! [`SHARED_SERVICE`] with an access list that trusts the app bundle and its bundled CLI by
//! designated requirement, so either executable — and later updates signed by the same
//! team — reads without a prompt. A development build uses [`DEVELOPMENT_SERVICE`] and never
//! touches the user's items. Ordinary reads never ask: a refusal becomes an error that says
//! what to do. Only the desktop app may ask, once per legacy item, while moving it.
//!
//! The policy is platform-neutral so it is tested with a fake [`Backend`]; the real backend
//! is `keychain_macos.rs`.
use std::{collections::HashSet, sync::Mutex};

/// Items written before the shared access list: readable only by the executable that wrote
/// them (and by whatever a person allowed afterwards).
pub(crate) const LEGACY_SERVICE: &str = "ai.passioncode.fabric-switchboard";
/// Items written by a team-signed build; the access list trusts the app and the bundled CLI.
pub(crate) const SHARED_SERVICE: &str = "ai.passioncode.fabric-switchboard.shared";
/// Items written by an unsigned or differently signed build. Never the user's accounts.
pub(crate) const DEVELOPMENT_SERVICE: &str = "ai.passioncode.fabric-switchboard.development";

pub(crate) const UNAVAILABLE: &str = "Native credential storage unavailable";
pub(crate) const ABSENT: &str = "Credential unavailable";
pub(crate) const LEGACY_NEEDS_APP: &str = "This account was saved by an earlier Switchboard. Open the Fabric Switchboard app once so it can move the account to shared storage, then retry.";
pub(crate) const LEGACY_DENIED: &str = "Keychain access was not allowed, so this account stays where an earlier Switchboard saved it. Restart Fabric Switchboard and choose Allow when macOS asks.";
pub(crate) const SHARED_REFUSED: &str = "Keychain did not let this copy of Switchboard read the account without asking. Unlock the login keychain, or use Fabric Switchboard from Applications, then retry.";
pub(crate) const DEVELOPMENT_REFUSED: &str = "This development build reads only accounts saved by the same build and never asks Keychain for access. Add the account again in this build, or use the signed app.";
pub(crate) const MOVE_UNVERIFIED: &str = "The account could not be moved to shared storage and stays where it was. Retry, or restart Fabric Switchboard.";
pub(crate) const LEGACY_REMOVAL_NEEDS_APP: &str = "Part of this account is stored where only the Fabric Switchboard app can remove it. Remove the account in the app.";

/// Vault messages that name a cause and a next step and carry no secret, so the store may
/// show them as they are instead of its generic storage error.
pub(crate) const ACTIONABLE: [&str; 6] = [
    LEGACY_NEEDS_APP,
    LEGACY_DENIED,
    SHARED_REFUSED,
    DEVELOPMENT_REFUSED,
    MOVE_UNVERIFIED,
    LEGACY_REMOVAL_NEEDS_APP,
];

/// Who signed the running executable, as far as Keychain trust is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Build {
    /// Valid signature anchored at Apple with the release team: shares the user's items.
    Signed,
    /// Anything else (ad hoc, unsigned, another team). Keeps to its own namespace.
    Development,
}

/// Whether this process may show a Keychain consent dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consent {
    /// CLI, MCP server, `serve`: never ask; return an actionable error instead.
    Never,
    /// The desktop app: may ask once per legacy item while moving it, and once to remove a
    /// legacy copy when a person removes the account.
    Desktop,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Read {
    Found(Vec<u8>),
    Absent,
    /// Keychain would have to ask (or was told no). With `ask: false` nothing was shown.
    Refused,
    Failed,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Create {
    Created,
    Exists,
    Failed,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    Removed,
    Absent,
    Refused,
    Failed,
}

/// The Keychain operations the policy needs. `ask` is the only way a dialog can appear.
pub(crate) trait Backend: Send + Sync {
    fn read(&self, service: &str, account: &str, ask: bool) -> Read;
    /// Adds a new item whose access list trusts this build's executables. Never replaces.
    fn create(&self, service: &str, account: &str, data: &[u8]) -> Create;
    /// Replaces the data of an existing item and keeps its access list.
    fn update(&self, service: &str, account: &str, data: &[u8]) -> bool;
    fn remove(&self, service: &str, account: &str, ask: bool) -> Removal;
}

pub(crate) struct Policy<B> {
    backend: B,
    build: Build,
    consent: Consent,
    /// Legacy items a person declined in this process: not asked about again until restart.
    declined: Mutex<HashSet<String>>,
}

impl<B: Backend> Policy<B> {
    pub(crate) fn new(backend: B, build: Build, consent: Consent) -> Self {
        Self {
            backend,
            build,
            consent,
            declined: Mutex::new(HashSet::new()),
        }
    }
    fn service(&self) -> &'static str {
        match self.build {
            Build::Signed => SHARED_SERVICE,
            Build::Development => DEVELOPMENT_SERVICE,
        }
    }
    fn may_ask(&self) -> bool {
        self.build == Build::Signed && self.consent == Consent::Desktop
    }

    pub(crate) fn get(&self, id: &str) -> Result<Vec<u8>, String> {
        match self.backend.read(self.service(), id, false) {
            Read::Found(data) => Ok(data),
            Read::Failed => Err(UNAVAILABLE.into()),
            Read::Refused => Err(match self.build {
                Build::Signed => SHARED_REFUSED.into(),
                Build::Development => DEVELOPMENT_REFUSED.into(),
            }),
            Read::Absent => match self.build {
                Build::Signed => self.move_legacy(id),
                Build::Development => Err(ABSENT.into()),
            },
        }
    }

    /// Copy a legacy item into the shared namespace, read the copy back, and only then
    /// remove the original. Asks at most once, and only from the desktop app.
    fn move_legacy(&self, id: &str) -> Result<Vec<u8>, String> {
        let legacy = match self.backend.read(LEGACY_SERVICE, id, false) {
            Read::Found(data) => data,
            Read::Absent => return Err(ABSENT.into()),
            Read::Failed => return Err(UNAVAILABLE.into()),
            Read::Refused if !self.may_ask() => return Err(LEGACY_NEEDS_APP.into()),
            Read::Refused => {
                if self.declined.lock().map_err(|_| UNAVAILABLE)?.contains(id) {
                    return Err(LEGACY_DENIED.into());
                }
                match self.backend.read(LEGACY_SERVICE, id, true) {
                    Read::Found(data) => data,
                    Read::Absent => return Err(ABSENT.into()),
                    Read::Failed => return Err(UNAVAILABLE.into()),
                    Read::Refused => {
                        self.declined
                            .lock()
                            .map_err(|_| UNAVAILABLE)?
                            .insert(id.into());
                        return Err(LEGACY_DENIED.into());
                    }
                }
            }
        };
        let created = match self.backend.create(SHARED_SERVICE, id, &legacy) {
            Create::Created => true,
            // Another process moved it, or wrote a newer value, first: the shared copy wins.
            Create::Exists => false,
            Create::Failed => return Err(MOVE_UNVERIFIED.into()),
        };
        let shared = match self.backend.read(SHARED_SERVICE, id, false) {
            Read::Found(data) => data,
            _ => {
                if created {
                    let _ = self.backend.remove(SHARED_SERVICE, id, false);
                }
                return Err(MOVE_UNVERIFIED.into());
            }
        };
        if created && shared != legacy {
            let _ = self.backend.remove(SHARED_SERVICE, id, false);
            return Err(MOVE_UNVERIFIED.into());
        }
        // The shared copy is verified; the original is now redundant. Never a second dialog:
        // if Keychain wants consent to delete it, the stale copy stays and is never read
        // again (shared is read first) until the account is removed in the app.
        let _ = self.backend.remove(LEGACY_SERVICE, id, false);
        Ok(shared)
    }

    pub(crate) fn put(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let service = self.service();
        match self.backend.create(service, id, data) {
            Create::Created => {}
            Create::Exists => {
                if !self.backend.update(service, id, data) {
                    return Err(UNAVAILABLE.into());
                }
            }
            Create::Failed => return Err(UNAVAILABLE.into()),
        }
        if self.build == Build::Signed {
            // A newer value supersedes any legacy copy; removing it is best effort and silent.
            let _ = self.backend.remove(LEGACY_SERVICE, id, false);
        }
        Ok(())
    }

    pub(crate) fn delete(&self, id: &str) -> Result<(), String> {
        match self.backend.remove(self.service(), id, false) {
            Removal::Removed | Removal::Absent => {}
            Removal::Refused => {
                return Err(match self.build {
                    Build::Signed => SHARED_REFUSED.into(),
                    Build::Development => DEVELOPMENT_REFUSED.into(),
                })
            }
            Removal::Failed => return Err(UNAVAILABLE.into()),
        }
        if self.build == Build::Development {
            return Ok(());
        }
        match self.backend.remove(LEGACY_SERVICE, id, false) {
            Removal::Removed | Removal::Absent => Ok(()),
            Removal::Failed => Err(UNAVAILABLE.into()),
            Removal::Refused if !self.may_ask() => Err(LEGACY_REMOVAL_NEEDS_APP.into()),
            // A person asked to remove the account: one dialog for the legacy copy is the
            // consent to that removal.
            Removal::Refused => match self.backend.remove(LEGACY_SERVICE, id, true) {
                Removal::Removed | Removal::Absent => Ok(()),
                Removal::Refused => Err(LEGACY_DENIED.into()),
                Removal::Failed => Err(UNAVAILABLE.into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    /// Executables as Keychain sees them: by designated requirement.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum Exe {
        App,
        Cli,
        /// The same app, a later version signed by the same team.
        AppUpdate,
        DevBuild,
        DevRebuild,
        OldDevApp,
    }
    impl Exe {
        /// Same designated requirement ⇒ Keychain treats them as one application.
        fn requirement(self) -> Exe {
            match self {
                Exe::AppUpdate => Exe::App,
                other => other,
            }
        }
    }
    struct Item {
        data: Vec<u8>,
        trusted: Vec<Exe>,
    }
    #[derive(Default)]
    struct Shared {
        items: HashMap<(String, String), Item>,
        dialogs: usize,
        /// What a person answers to a dialog.
        allow: bool,
        always_allow: bool,
        fail_create: bool,
        corrupt_create: bool,
    }
    /// One executable's view of a Keychain shared by every executable in a test.
    #[derive(Clone)]
    struct Fake {
        caller: Exe,
        /// What `create` puts in the access list; the real backend resolves the bundle.
        trust: Vec<Exe>,
        state: Arc<Mutex<Shared>>,
    }
    impl Fake {
        fn new(state: &Arc<Mutex<Shared>>, caller: Exe, trust: &[Exe]) -> Self {
            Self {
                caller,
                trust: trust.to_vec(),
                state: state.clone(),
            }
        }
        fn trusted(&self, item: &Item) -> bool {
            item.trusted
                .iter()
                .any(|t| t.requirement() == self.caller.requirement())
        }
    }
    impl Backend for Fake {
        fn read(&self, service: &str, account: &str, ask: bool) -> Read {
            let mut s = self.state.lock().unwrap();
            let key = (service.to_owned(), account.to_owned());
            let (allow, always) = (s.allow, s.always_allow);
            let Some(item) = s.items.get(&key) else {
                return Read::Absent;
            };
            if self.trusted(item) {
                return Read::Found(item.data.clone());
            }
            if !ask {
                return Read::Refused;
            }
            s.dialogs += 1;
            if !allow {
                return Read::Refused;
            }
            let item = s.items.get_mut(&key).unwrap();
            if always {
                item.trusted.push(self.caller);
            }
            Read::Found(item.data.clone())
        }
        fn create(&self, service: &str, account: &str, data: &[u8]) -> Create {
            let mut s = self.state.lock().unwrap();
            if s.fail_create {
                return Create::Failed;
            }
            let key = (service.to_owned(), account.to_owned());
            if s.items.contains_key(&key) {
                return Create::Exists;
            }
            let mut data = data.to_vec();
            if s.corrupt_create {
                data.push(b'!');
            }
            s.items.insert(
                key,
                Item {
                    data,
                    trusted: self.trust.clone(),
                },
            );
            Create::Created
        }
        fn update(&self, service: &str, account: &str, data: &[u8]) -> bool {
            let mut s = self.state.lock().unwrap();
            match s.items.get_mut(&(service.to_owned(), account.to_owned())) {
                Some(item) => {
                    item.data = data.to_vec();
                    true
                }
                None => false,
            }
        }
        fn remove(&self, service: &str, account: &str, ask: bool) -> Removal {
            let mut s = self.state.lock().unwrap();
            let key = (service.to_owned(), account.to_owned());
            let allow = s.allow;
            let Some(item) = s.items.get(&key) else {
                return Removal::Absent;
            };
            if !self.trusted(item) {
                if !ask {
                    return Removal::Refused;
                }
                s.dialogs += 1;
                if !allow {
                    return Removal::Refused;
                }
            }
            s.items.remove(&key);
            Removal::Removed
        }
    }

    const ID: &str = "8b1f7c56-4d0e-4c39-9d3a-2a7e0f6f3a10";
    const SIGNED_TRUST: [Exe; 2] = [Exe::App, Exe::Cli];

    fn world() -> Arc<Mutex<Shared>> {
        Arc::new(Mutex::new(Shared {
            allow: true,
            ..Shared::default()
        }))
    }
    fn signed(state: &Arc<Mutex<Shared>>, caller: Exe, consent: Consent) -> Policy<Fake> {
        Policy::new(
            Fake::new(state, caller, &SIGNED_TRUST),
            Build::Signed,
            consent,
        )
    }
    fn legacy(state: &Arc<Mutex<Shared>>, data: &[u8], trusted: &[Exe]) {
        state.lock().unwrap().items.insert(
            (LEGACY_SERVICE.into(), ID.into()),
            Item {
                data: data.to_vec(),
                trusted: trusted.to_vec(),
            },
        );
    }
    fn has(state: &Arc<Mutex<Shared>>, service: &str) -> bool {
        state
            .lock()
            .unwrap()
            .items
            .contains_key(&(service.to_owned(), ID.to_owned()))
    }
    fn dialogs(state: &Arc<Mutex<Shared>>) -> usize {
        state.lock().unwrap().dialogs
    }

    #[test]
    fn app_writes_and_the_bundled_cli_reads_without_a_dialog() {
        let state = world();
        signed(&state, Exe::App, Consent::Desktop)
            .put(ID, b"fixture-only")
            .unwrap();
        let cli = signed(&state, Exe::Cli, Consent::Never);
        assert_eq!(cli.get(ID).unwrap(), b"fixture-only");
        cli.put(ID, b"fixture-two").unwrap();
        let update = signed(&state, Exe::AppUpdate, Consent::Desktop);
        assert_eq!(update.get(ID).unwrap(), b"fixture-two");
        assert_eq!(dialogs(&state), 0);
        assert!(!has(&state, LEGACY_SERVICE));
    }

    #[test]
    fn legacy_item_moves_with_one_dialog_and_never_asks_again() {
        let state = world();
        legacy(&state, b"fixture-only", &[Exe::OldDevApp]);
        let app = signed(&state, Exe::App, Consent::Desktop);
        assert_eq!(app.get(ID).unwrap(), b"fixture-only");
        assert_eq!(dialogs(&state), 1, "exactly one consent for the move");
        assert!(has(&state, SHARED_SERVICE));
        // "Allow" (not "Always Allow") does not cover the delete; the stale copy stays and
        // is not asked about.
        assert!(has(&state, LEGACY_SERVICE));
        assert_eq!(app.get(ID).unwrap(), b"fixture-only");
        let cli = signed(&state, Exe::Cli, Consent::Never);
        assert_eq!(cli.get(ID).unwrap(), b"fixture-only");
        assert_eq!(dialogs(&state), 1, "no dialog after the move");
    }

    #[test]
    fn always_allow_lets_the_move_delete_the_original() {
        let state = world();
        state.lock().unwrap().always_allow = true;
        legacy(&state, b"fixture-only", &[Exe::DevBuild]);
        let app = signed(&state, Exe::App, Consent::Desktop);
        assert_eq!(app.get(ID).unwrap(), b"fixture-only");
        assert_eq!(dialogs(&state), 1);
        assert!(!has(&state, LEGACY_SERVICE));
        assert!(has(&state, SHARED_SERVICE));
    }

    #[test]
    fn legacy_item_already_trusting_the_app_moves_silently() {
        // The 0.3.2 release wrote items trusting the app's designated requirement; the
        // installed app matches it, so the move needs no dialog at all.
        let state = world();
        legacy(&state, b"fixture-only", &[Exe::DevBuild, Exe::App]);
        let app = signed(&state, Exe::AppUpdate, Consent::Desktop);
        assert_eq!(app.get(ID).unwrap(), b"fixture-only");
        assert_eq!(dialogs(&state), 0);
        assert!(!has(&state, LEGACY_SERVICE));
        assert_eq!(
            signed(&state, Exe::Cli, Consent::Never).get(ID).unwrap(),
            b"fixture-only"
        );
    }

    #[test]
    fn cli_never_asks_and_names_the_app_as_the_way_out() {
        let state = world();
        legacy(&state, b"fixture-only", &[Exe::App]);
        let cli = signed(&state, Exe::Cli, Consent::Never);
        for _ in 0..20 {
            assert_eq!(cli.get(ID).unwrap_err(), LEGACY_NEEDS_APP);
        }
        assert_eq!(dialogs(&state), 0);
        assert!(has(&state, LEGACY_SERVICE));
        assert!(!has(&state, SHARED_SERVICE));
    }

    #[test]
    fn a_declined_dialog_is_not_repeated_in_the_same_process() {
        let state = world();
        state.lock().unwrap().allow = false;
        legacy(&state, b"fixture-only", &[Exe::DevBuild]);
        let app = signed(&state, Exe::App, Consent::Desktop);
        for _ in 0..20 {
            assert_eq!(app.get(ID).unwrap_err(), LEGACY_DENIED);
        }
        assert_eq!(dialogs(&state), 1);
        assert!(
            has(&state, LEGACY_SERVICE),
            "declining never loses the item"
        );
    }

    #[test]
    fn failed_or_unverified_copy_keeps_the_original() {
        for corrupt in [false, true] {
            let state = world();
            {
                let mut s = state.lock().unwrap();
                s.fail_create = !corrupt;
                s.corrupt_create = corrupt;
            }
            legacy(&state, b"fixture-only", &[Exe::App]);
            let app = signed(&state, Exe::App, Consent::Desktop);
            assert_eq!(app.get(ID).unwrap_err(), MOVE_UNVERIFIED);
            assert!(
                has(&state, LEGACY_SERVICE),
                "original kept (corrupt={corrupt})"
            );
            assert!(
                !has(&state, SHARED_SERVICE),
                "bad copy removed (corrupt={corrupt})"
            );
            let mut s = state.lock().unwrap();
            s.fail_create = false;
            s.corrupt_create = false;
            drop(s);
            assert_eq!(app.get(ID).unwrap(), b"fixture-only", "retry succeeds");
        }
    }

    #[test]
    fn a_shared_copy_written_first_wins_over_the_legacy_one() {
        let state = world();
        legacy(&state, b"fixture-old", &[Exe::App]);
        state.lock().unwrap().items.insert(
            (SHARED_SERVICE.into(), ID.into()),
            Item {
                data: b"fixture-new".to_vec(),
                trusted: SIGNED_TRUST.to_vec(),
            },
        );
        let app = signed(&state, Exe::App, Consent::Desktop);
        assert_eq!(app.get(ID).unwrap(), b"fixture-new");
        assert_eq!(dialogs(&state), 0);
    }

    #[test]
    fn development_builds_never_touch_the_users_items() {
        let state = world();
        legacy(&state, b"fixture-only", &[Exe::DevBuild]);
        let dev = Policy::new(
            Fake::new(&state, Exe::DevBuild, &[Exe::DevBuild]),
            Build::Development,
            Consent::Desktop,
        );
        assert_eq!(dev.get(ID).unwrap_err(), ABSENT);
        dev.put(ID, b"fixture-dev").unwrap();
        assert!(has(&state, DEVELOPMENT_SERVICE));
        let rebuilt = Policy::new(
            Fake::new(&state, Exe::DevRebuild, &[Exe::DevRebuild]),
            Build::Development,
            Consent::Desktop,
        );
        assert_eq!(rebuilt.get(ID).unwrap_err(), DEVELOPMENT_REFUSED);
        dev.delete(ID).unwrap();
        assert!(!has(&state, DEVELOPMENT_SERVICE));
        assert!(has(&state, LEGACY_SERVICE));
        assert!(!has(&state, SHARED_SERVICE));
        assert_eq!(dialogs(&state), 0);
    }

    #[test]
    fn delete_removes_both_copies_and_asks_only_in_the_app() {
        let state = world();
        legacy(&state, b"fixture-only", &[Exe::DevBuild]);
        signed(&state, Exe::App, Consent::Desktop)
            .put(ID, b"fixture-two")
            .unwrap();
        assert!(has(&state, LEGACY_SERVICE), "silent removal was refused");
        let cli = signed(&state, Exe::Cli, Consent::Never);
        assert_eq!(cli.delete(ID).unwrap_err(), LEGACY_REMOVAL_NEEDS_APP);
        assert_eq!(dialogs(&state), 0);
        signed(&state, Exe::App, Consent::Desktop)
            .delete(ID)
            .unwrap();
        assert_eq!(dialogs(&state), 1);
        assert!(!has(&state, LEGACY_SERVICE));
        assert!(!has(&state, SHARED_SERVICE));
        // Retry-safe.
        cli.delete(ID).unwrap();
    }

    #[test]
    fn absent_everywhere_is_reported_as_absent() {
        let state = world();
        let app = signed(&state, Exe::App, Consent::Desktop);
        assert_eq!(app.get(ID).unwrap_err(), ABSENT);
        app.delete(ID).unwrap();
        assert_eq!(dialogs(&state), 0);
    }
}
