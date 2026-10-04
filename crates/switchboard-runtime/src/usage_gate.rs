//! One provider not-before for every quota check (SB-39), and one request per account at a
//! time. A provider that answers a usage check with 429 names a time before which it wants no
//! further check; every caller — the desktop, the CLI and MCP through the owner, the offline
//! CLI and the background pass — reaches `monitor::check`, which asks this gate first.
//!
//! The not-before is kept in `usage-holds.json` (0600) beside the store, not in
//! `accounts.json`: older builds read `accounts.json` with `deny_unknown_fields`, and one kept
//! for rollback must still open it while a hold exists. The file is owned by whoever holds the
//! store's lock (the owner, or the offline CLI for one command). It holds account ids, times and
//! a fingerprint of the access token a hold was earned with — never a token. An entry is
//! dropped when it expires, or when its account no longer stores that token (a new sign-in,
//! capture or renewal is a new credential generation and starts without the old wait).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
};
use switchboard_core::{Credential, Usage};

pub(crate) const HOLDS_FILE: &str = "usage-holds.json";
/// A 429 without a usable `Retry-After` — absent, malformed, zero or a date already past —
/// waits this long. Zero is the observed failure mode (anthropics/claude-code#30930): a
/// provider repeating `retry-after: 0` while refusing every check.
pub(crate) const RATE_LIMIT_FLOOR: i64 = 900;
/// The longest wait a `Retry-After` can impose. A sanity bound against a garbled header, set
/// above any provider window (a week), so it never shortens a wait a provider plausibly means.
pub(crate) const MAX_NOT_BEFORE: i64 = 7 * 86_400;
/// A hold recorded further ahead of the clock than this was written under a wrong clock.
const AHEAD: i64 = 60;
const MAX_FILE: u64 = 256 * 1024;

/// Seconds a provider 429 forbids further checks for: its own positive wait (bounded), else the
/// floor.
pub(crate) fn not_before_delay(retry_after: Option<i64>) -> i64 {
    match retry_after {
        Some(seconds) if seconds > 0 => seconds.min(MAX_NOT_BEFORE),
        _ => RATE_LIMIT_FLOOR,
    }
}

/// A credential generation as the store counts one (core `upsert`, `usage_health_credential`):
/// the access token and its expiry.
fn fingerprint(credential: &Credential) -> String {
    let mut digest = Sha256::new();
    digest.update(credential.access_token.as_bytes());
    digest.update(credential.expires_at.unwrap_or(0).to_be_bytes());
    format!("{:x}", digest.finalize())[..32].to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hold {
    not_before: i64,
    recorded_at: i64,
    /// First 128 bits of SHA-256 over the access token the 429 answered and its expiry.
    credential: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldsFile {
    version: u32,
    holds: BTreeMap<String, Hold>,
}

#[derive(Default)]
struct Holds {
    by_account: BTreeMap<String, Hold>,
    /// Bumped on every change; a write of an older state never replaces a newer one.
    version: u64,
}

#[derive(Default)]
struct Slot {
    /// Checks of this account finished so far.
    completed: AtomicU64,
    /// Held for the length of one check; keeps the last answer, with the store's change count
    /// when that check began, for callers that asked while it ran.
    turn: tokio::sync::Mutex<Option<(u64, Result<Usage, String>)>>,
}

/// A poisoned lock still holds consistent data here (every change is one insert or remove), and
/// a gate that failed open would let a caller past a provider's wait.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A hold as read from disk, made sane: never longer than the bound, never ending before it
/// began. `None` when it can never apply again.
fn sane(hold: Hold, now: i64) -> Option<Hold> {
    if hold.not_before <= hold.recorded_at {
        return None;
    }
    let not_before = hold
        .not_before
        .min(hold.recorded_at.saturating_add(MAX_NOT_BEFORE));
    // One stamped ahead of the clock is kept for a rebase; one already over is dropped.
    (not_before > now || hold.recorded_at > now.saturating_add(AHEAD))
        .then_some(Hold { not_before, ..hold })
}

/// Moves a hold recorded ahead of the clock to `now` plus its remaining length. True if moved.
fn rebase(hold: &mut Hold, now: i64) -> bool {
    if hold.recorded_at <= now.saturating_add(AHEAD) {
        return false;
    }
    let remaining = hold
        .not_before
        .saturating_sub(hold.recorded_at)
        .clamp(0, MAX_NOT_BEFORE);
    hold.recorded_at = now;
    hold.not_before = now.saturating_add(remaining);
    true
}

#[derive(Default)]
pub(crate) struct UsageGate {
    path: Mutex<Option<PathBuf>>,
    holds: Mutex<Holds>,
    /// The version last written; held while writing, so writes never overlap or go backwards.
    written: Mutex<u64>,
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    /// Synthetic usage endpoints for tests; production always uses the fixed provider origins.
    #[cfg(test)]
    pub(crate) origins: Mutex<Option<(String, String)>>,
}

impl UsageGate {
    /// The gate of an owner or of the offline CLI, with the holds recorded before that can still
    /// apply at `now`.
    pub(crate) fn kept_in(path: PathBuf, now: i64) -> Self {
        let gate = Self::default();
        *lock(&gate.path) = Some(path.clone());
        let Ok(bytes) = switchboard_core::private_fs::read_private(&path, MAX_FILE) else {
            return gate;
        };
        match serde_json::from_slice::<HoldsFile>(&bytes) {
            Ok(file) if file.version == 1 => {
                let loaded = file.holds.len();
                let kept: BTreeMap<_, _> = file
                    .holds
                    .into_iter()
                    .filter_map(|(id, hold)| sane(hold.clone(), now).map(|h| (id, h)))
                    .collect();
                let changed = kept.len() != loaded;
                let snapshot = {
                    let mut holds = lock(&gate.holds);
                    holds.by_account = kept;
                    changed.then(|| gate.changed(&mut holds))
                };
                gate.write(snapshot);
            }
            // An unreadable file cannot be trusted to shorten anything; it is rewritten at the
            // next hold. Nothing in it is a credential.
            _ => crate::oplog::event(
                "usage_holds_unreadable",
                &[("outcome", crate::oplog::Field::Code("ignored"))],
            ),
        }
        gate
    }

    /// Marks a change made under the `holds` lock; returns what to write once it is released.
    fn changed(&self, holds: &mut Holds) -> (u64, BTreeMap<String, Hold>) {
        holds.version += 1;
        (holds.version, holds.by_account.clone())
    }

    /// Writes a state taken under the `holds` lock, after that lock is released: the background
    /// never waits behind a file sync to ask whether an account is held.
    fn write(&self, snapshot: Option<(u64, BTreeMap<String, Hold>)>) {
        let Some((version, holds)) = snapshot else {
            return;
        };
        let Some(path) = lock(&self.path).clone() else {
            return;
        };
        let mut written = lock(&self.written);
        if *written >= version {
            return;
        }
        let file = HoldsFile { version: 1, holds };
        let saved = serde_json::to_vec(&file)
            .map_err(|_| String::new())
            .and_then(|bytes| switchboard_core::private_fs::private_write(&path, &bytes));
        match saved {
            Ok(()) => *written = version,
            // The hold still applies in this process; a restart may allow one check early.
            Err(_) => crate::oplog::event(
                "usage_holds_unsaved",
                &[("outcome", crate::oplog::Field::Code("memory_only"))],
            ),
        }
    }

    /// Records the provider's not-before for `id`, bound to the token it answered.
    pub(crate) fn hold(
        &self,
        id: &str,
        credential: &Credential,
        recorded_at: i64,
        not_before: i64,
    ) {
        let snapshot = {
            let mut holds = lock(&self.holds);
            holds.by_account.retain(|_, h| h.not_before > recorded_at);
            holds.by_account.insert(
                id.to_owned(),
                Hold {
                    not_before,
                    recorded_at,
                    credential: fingerprint(credential),
                },
            );
            Some(self.changed(&mut holds))
        };
        self.write(snapshot);
    }

    /// True while a hold for `id` has not passed — without reading any credential, for the
    /// background's due filter (LC-04). A hold recorded ahead of a clock that was then set back is
    /// rebased here first, so the background is never held for the size of the correction; one
    /// that passed is dropped. Writes only on such a change.
    pub(crate) fn held(&self, id: &str, now: i64) -> bool {
        let (held, snapshot) = {
            let mut holds = lock(&self.holds);
            let Some(hold) = holds.by_account.get_mut(id) else {
                return false;
            };
            let moved = rebase(hold, now);
            let held = now < hold.not_before;
            if !held {
                holds.by_account.remove(id);
            }
            (held, (moved || !held).then(|| self.changed(&mut holds)))
        };
        self.write(snapshot);
        held
    }

    /// Whether a hold is recorded for `id` at all: only then is the credential read to check it.
    pub(crate) fn has_hold(&self, id: &str) -> bool {
        lock(&self.holds).by_account.contains_key(id)
    }

    /// The not-before that applies to `credential` of `id` at `now`, if any. Drops a hold that
    /// passed or belongs to another credential generation; rebases one recorded ahead of the
    /// clock to `now` plus its remaining length, so a wrong clock neither shortens nor freezes it.
    pub(crate) fn active(&self, id: &str, credential: &Credential, now: i64) -> Option<i64> {
        let (answer, snapshot) = {
            let mut holds = lock(&self.holds);
            let hold = holds.by_account.get_mut(id)?;
            if hold.credential != fingerprint(credential) {
                holds.by_account.remove(id);
                (None, Some(self.changed(&mut holds)))
            } else {
                let moved = rebase(hold, now);
                let not_before = hold.not_before;
                if now >= not_before {
                    holds.by_account.remove(id);
                    (None, Some(self.changed(&mut holds)))
                } else {
                    (Some(not_before), moved.then(|| self.changed(&mut holds)))
                }
            }
        };
        self.write(snapshot);
        answer
    }

    /// Forgets holds and request slots of accounts the store no longer has.
    pub(crate) fn forget_missing<'a>(&self, accounts: impl IntoIterator<Item = &'a str>) {
        let present: std::collections::HashSet<&str> = accounts.into_iter().collect();
        lock(&self.slots).retain(|id, _| present.contains(id.as_str()));
        let snapshot = {
            let mut holds = lock(&self.holds);
            let before = holds.by_account.len();
            holds
                .by_account
                .retain(|id, _| present.contains(id.as_str()));
            (holds.by_account.len() != before).then(|| self.changed(&mut holds))
        };
        self.write(snapshot);
    }

    /// Runs `check` as the only request for `id`. A caller that asked while another check of the
    /// same account was running takes that check's answer instead of sending its own — unless the
    /// store changed between that check's start and the ask (`epoch`, the store's change count:
    /// a new sign-in, a renewal), which a check of the old credential cannot answer for. Nothing
    /// global is held while waiting.
    pub(crate) async fn coalesce<F, Fut>(
        &self,
        id: &str,
        epoch: u64,
        check: F,
    ) -> Result<Usage, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Usage, String>>,
    {
        let slot = lock(&self.slots).entry(id.to_owned()).or_default().clone();
        let asked = slot.completed.load(Ordering::SeqCst);
        let mut turn = slot.turn.lock().await;
        if slot.completed.load(Ordering::SeqCst) != asked {
            if let Some((began, answer)) = turn.as_ref() {
                if *began == epoch {
                    return answer.clone();
                }
            }
        }
        let answer = check().await;
        *turn = Some((epoch, answer.clone()));
        slot.completed.fetch_add(1, Ordering::SeqCst);
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::credential;

    fn file(root: &tempfile::TempDir) -> PathBuf {
        root.path().join(HOLDS_FILE)
    }

    #[test]
    fn a_provider_wait_is_kept_whole_and_a_useless_one_gets_the_floor() {
        assert_eq!(not_before_delay(None), RATE_LIMIT_FLOOR);
        assert_eq!(not_before_delay(Some(0)), RATE_LIMIT_FLOOR);
        assert_eq!(not_before_delay(Some(-3)), RATE_LIMIT_FLOOR);
        assert_eq!(not_before_delay(Some(5)), 5);
        // Longer than the six hours 0.5.1 allowed: kept, not shortened.
        assert_eq!(not_before_delay(Some(9 * 3600)), 9 * 3600);
        assert_eq!(not_before_delay(Some(i64::MAX)), MAX_NOT_BEFORE);
    }

    #[test]
    fn a_hold_survives_a_restart_and_never_stores_a_token() {
        let root = tempfile::tempdir().unwrap();
        let a = credential("synthetic-a");
        UsageGate::kept_in(file(&root), 1_000).hold("id-a", &a, 1_000, 5_000);
        let text = std::fs::read_to_string(file(&root)).unwrap();
        assert!(
            !text.contains("synthetic-a-token"),
            "the file holds a fingerprint only"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(file(&root)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let restarted = UsageGate::kept_in(file(&root), 1_001);
        assert!(restarted.held("id-a", 1_001));
        assert_eq!(restarted.active("id-a", &a, 1_001), Some(5_000));
        assert_eq!(
            restarted.active("id-a", &a, 5_000),
            None,
            "the deadline itself is free"
        );
        assert!(!restarted.has_hold("id-a"), "a passed hold is dropped");
        assert!(
            !UsageGate::kept_in(file(&root), 5_001).has_hold("id-a"),
            "and stays dropped"
        );
    }

    #[test]
    fn a_new_credential_generation_starts_without_the_old_wait() {
        let root = tempfile::tempdir().unwrap();
        let gate = UsageGate::kept_in(file(&root), 1_000);
        gate.hold("id-a", &credential("synthetic-a"), 1_000, 5_000);
        let mut renewed = credential("synthetic-a");
        renewed.access_token = "synthetic-a-renewed".into();
        assert_eq!(gate.active("id-a", &renewed, 1_001), None);
        assert!(!gate.has_hold("id-a"));
        // The same token with another expiry is a new generation to the store too.
        gate.hold("id-a", &credential("synthetic-a"), 1_000, 5_000);
        let mut extended = credential("synthetic-a");
        extended.expires_at = extended.expires_at.map(|t| t + 1);
        assert_eq!(gate.active("id-a", &extended, 1_001), None);
    }

    #[test]
    fn a_hold_written_under_a_clock_set_back_is_rebased_not_frozen() {
        let root = tempfile::tempdir().unwrap();
        let a = credential("synthetic-a");
        let gate = UsageGate::kept_in(file(&root), 100_000);
        // Recorded at 100 000 with 900 s to wait; the clock now reads 10 000.
        gate.hold("id-a", &a, 100_000, 100_900);
        assert_eq!(gate.active("id-a", &a, 10_000), Some(10_900));
        // Rebased once and written: the wait ends 900 s after the corrected clock.
        let restarted = UsageGate::kept_in(file(&root), 10_000);
        assert_eq!(restarted.active("id-a", &a, 10_899), Some(10_900));
        assert_eq!(restarted.active("id-a", &a, 10_900), None);
    }

    #[test]
    fn the_background_rebases_a_hold_without_a_credential() {
        let root = tempfile::tempdir().unwrap();
        let gate = UsageGate::kept_in(file(&root), 100_000);
        gate.hold("id-a", &credential("synthetic-a"), 100_000, 100_900);
        // The clock was set back a day: one look from the due filter moves the wait.
        assert!(gate.held("id-a", 13_600));
        assert!(UsageGate::kept_in(file(&root), 13_600).held("id-a", 14_499));
        assert!(!gate.held("id-a", 14_500));
        assert!(
            !gate.has_hold("id-a"),
            "a passed hold is dropped by the due filter too"
        );
    }

    #[test]
    fn holds_read_from_disk_are_bounded_and_expired_ones_dropped() {
        let root = tempfile::tempdir().unwrap();
        let fingerprint = fingerprint(&credential("synthetic-a"));
        let hold = |not_before: i64, recorded_at: i64| Hold {
            not_before,
            recorded_at,
            credential: fingerprint.clone(),
        };
        let holds = BTreeMap::from([
            ("expired".to_string(), hold(1_500, 1_000)),
            ("month".to_string(), hold(1_000 + 30 * 86_400, 1_000)),
            ("garbled".to_string(), hold(i64::MAX, i64::MIN)),
            ("backwards".to_string(), hold(1_000, 5_000)),
        ]);
        let bytes = serde_json::to_vec(&HoldsFile { version: 1, holds }).unwrap();
        switchboard_core::private_fs::private_write(&file(&root), &bytes).unwrap();
        let gate = UsageGate::kept_in(file(&root), 2_000);
        assert!(!gate.has_hold("expired"));
        assert!(!gate.has_hold("garbled"));
        assert!(!gate.has_hold("backwards"));
        let a = credential("synthetic-a");
        assert_eq!(
            gate.active("month", &a, 2_000),
            Some(1_000 + MAX_NOT_BEFORE)
        );
        // The cleaned set is what the file now holds.
        let text = std::fs::read_to_string(file(&root)).unwrap();
        assert!(!text.contains("expired") && !text.contains("garbled"));
    }

    #[test]
    fn an_unreadable_holds_file_is_ignored_and_replaced() {
        let root = tempfile::tempdir().unwrap();
        switchboard_core::private_fs::private_write(&file(&root), b"{not json").unwrap();
        let gate = UsageGate::kept_in(file(&root), 1_000);
        assert!(!gate.has_hold("id-a"));
        gate.hold("id-a", &credential("synthetic-a"), 1_000, 2_000);
        assert!(UsageGate::kept_in(file(&root), 1_500).held("id-a", 1_500));
    }

    #[test]
    fn a_removed_account_leaves_no_hold_behind() {
        let root = tempfile::tempdir().unwrap();
        let gate = UsageGate::kept_in(file(&root), 1_000);
        gate.hold("id-a", &credential("synthetic-a"), 1_000, 5_000);
        gate.hold("id-b", &credential("synthetic-b"), 1_000, 5_000);
        gate.forget_missing(["id-b"]);
        assert!(!gate.has_hold("id-a"));
        assert!(UsageGate::kept_in(file(&root), 1_001).has_hold("id-b"));
        assert!(!UsageGate::kept_in(file(&root), 1_001).has_hold("id-a"));
    }

    async fn race(epoch_second: u64) -> (String, String, usize) {
        use std::sync::atomic::AtomicUsize;
        let gate = Arc::new(UsageGate::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let first = {
            let (gate, calls) = (gate.clone(), calls.clone());
            tokio::spawn(async move {
                gate.coalesce("id-a", 7, || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    released.await.unwrap();
                    Err::<Usage, _>("first".to_string())
                })
                .await
            })
        };
        // The first check is running when the second caller asks.
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let second = {
            let (gate, calls) = (gate.clone(), calls.clone());
            tokio::spawn(async move {
                gate.coalesce("id-a", epoch_second, || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err::<Usage, _>("second".to_string())
                })
                .await
            })
        };
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        release.send(()).unwrap();
        let first = first.await.unwrap().unwrap_err();
        let second = second.await.unwrap().unwrap_err();
        (first, second, calls.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn a_caller_that_asked_during_a_check_takes_its_answer() {
        assert_eq!(race(7).await, ("first".into(), "first".into(), 1));
        // A caller that asks after the check finished sends its own.
        let gate = UsageGate::default();
        let _ = gate
            .coalesce("id-a", 7, || async { Err::<Usage, _>("one".to_string()) })
            .await;
        let again = gate
            .coalesce("id-a", 7, || async { Err::<Usage, _>("two".to_string()) })
            .await;
        assert_eq!(again.unwrap_err(), "two");
    }

    #[tokio::test]
    async fn a_new_sign_in_during_a_check_is_checked_on_its_own() {
        // The store changed (a new sign-in, a renewal) after the running check began: its
        // answer speaks for the old credential and is not handed on.
        assert_eq!(race(8).await, ("first".into(), "second".into(), 2));
    }
}
