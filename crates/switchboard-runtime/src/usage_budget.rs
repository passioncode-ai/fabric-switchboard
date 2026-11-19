//! SB-518 (DEC-0032): one budget of usage checks per provider. Every quota check reaches the
//! provider's usage endpoint from one address; before this, the background alone sent about
//! eight a minute with 32 accounts and a 429 held only the one account it answered.
//!
//! - A token bucket per provider: [`RATE_PER_MINUTE`] checks a minute, a burst of [`BURST`]. The
//!   background takes a token only when one is there; a person's check and the fresh check of a
//!   switch's candidate may run up to [`DEBT`] tokens ahead, which the background then waits out.
//! - At most [`IN_FLIGHT`] checks of one provider on the network at once.
//! - A 429 on a usage check holds the whole provider for `max(Retry-After, 900 s)` (the floor a
//!   single account's hold already has: the provider answers `retry-after: 0` while it refuses
//!   every check, anthropics/claude-code#30930), doubling on each 429 that follows up to an hour
//!   (a longer `Retry-After` is kept whole), and a success ends the doubling. The hold is kept in `usage-provider-holds.json` (0600, provider names
//!   and times only) so a restart honours it; the per-account hold stays in `usage-holds.json`.
//!
//! Times are whole seconds passed in by the caller, so the background's fake clock drives it.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};
use switchboard_core::Provider;

pub(crate) const PROVIDER_HOLDS_FILE: &str = "usage-provider-holds.json";
/// Checks a minute the bucket refills by: one token every [`TOKEN_SECONDS`].
pub(crate) const RATE_PER_MINUTE: i64 = 2;
/// Seconds of refill one check costs.
pub(crate) const TOKEN_SECONDS: i64 = 60 / RATE_PER_MINUTE;
/// Tokens the bucket holds at most.
pub(crate) const BURST: i64 = 4;
/// How far a person's check or a candidate's fresh check may run ahead of the bucket, in tokens.
pub(crate) const DEBT: i64 = 4;
/// Checks of one provider on the network at once.
pub(crate) const IN_FLIGHT: usize = 2;
/// The shortest provider hold after a 429: the per-account floor, never lower.
pub(crate) const PROVIDER_HOLD_FLOOR: i64 = crate::usage_gate::RATE_LIMIT_FLOOR;
/// The longest a repeated 429 doubles the hold to (a longer `Retry-After` is kept whole).
pub(crate) const PROVIDER_HOLD_CEILING: i64 = 3600;
/// ± this share of every computed next-check interval, so checks never fall due in step.
pub(crate) const JITTER: f64 = 0.15;
/// A switch's candidate is checked fresh at most once in this many seconds per provider, so
/// confirmations never take the whole budget from the accounts in use.
pub(crate) const CONFIRM_SPACING_SECONDS: i64 = 60;
/// Shorter schedules (a near-limit minute, a reset's half minute) are not spread.
const JITTER_FROM_SECONDS: i64 = 120;
const MAX_FILE: u64 = 16 * 1024;
/// A hold stamped further ahead of the clock than this was written under a wrong clock.
const AHEAD: i64 = 60;

/// The person-facing refusal when the bucket is spent past its debt.
pub(crate) const USAGE_PACED: &str =
    "Usage checks of this provider are paced to keep its usage endpoint answering. Try again in a minute.";

/// Who asks for a check: the background takes only a token that is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Caller {
    /// The background pass; its token was taken by the pass with its own clock.
    Background,
    /// A person (the window, the CLI, MCP): may run [`DEBT`] tokens ahead.
    Person,
    /// The fresh check of a switch's candidate: may run ahead like a person's, taken by the pass.
    Confirm,
}

/// The bucket in seconds of refill, so no rounding decides a check: a token is
/// [`TOKEN_SECONDS`] of it.
#[derive(Clone, Debug, Default)]
struct Bucket {
    credit: i64,
    at: i64,
    started: bool,
}
impl Bucket {
    fn refill(&mut self, now: i64) {
        if !self.started {
            *self = Bucket {
                credit: BURST * TOKEN_SECONDS,
                at: now,
                started: true,
            };
            return;
        }
        // A clock set back refills nothing and restarts the count from it.
        let elapsed = now.saturating_sub(self.at).max(0);
        self.credit = self
            .credit
            .saturating_add(elapsed)
            .min(BURST * TOKEN_SECONDS);
        self.at = now;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderHold {
    until: i64,
    recorded_at: i64,
    /// 429s in a row since the last success.
    strikes: u32,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldsFile {
    version: u32,
    providers: BTreeMap<String, ProviderHold>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How long a provider is held after its `strikes`-th 429 in a row.
pub(crate) fn provider_hold_seconds(retry_after: Option<i64>, strikes: u32) -> i64 {
    let doubled = PROVIDER_HOLD_FLOOR
        .saturating_mul(1i64 << strikes.saturating_sub(1).min(16))
        .min(PROVIDER_HOLD_CEILING);
    let asked = retry_after
        .filter(|s| *s > 0)
        .unwrap_or(0)
        .min(crate::usage_gate::MAX_NOT_BEFORE);
    doubled.max(asked)
}

/// When a schedule computed from `checked` to `next` falls due once spread by ± [`JITTER`]: the
/// same account and schedule always spread the same way, so a pass asked twice agrees.
pub(crate) fn jittered(id: &str, checked: i64, next: i64) -> i64 {
    let span = next.saturating_sub(checked);
    if span < JITTER_FROM_SECONDS {
        return next;
    }
    let reach = (span as f64 * JITTER) as i64;
    if reach == 0 {
        return next;
    }
    // A fixed hash (SHA-256), so the spread is the same on every build and platform.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(id.as_bytes());
    h.update(next.to_be_bytes());
    let digest = h.finalize();
    let mut eight = [0u8; 8];
    eight.copy_from_slice(&digest[..8]);
    let offset = (u64::from_be_bytes(eight) % (2 * reach as u64 + 1)) as i64 - reach;
    next.saturating_add(offset)
}

/// One budget per provider. `Default` is unbudgeted (a test's gate); an owner's and the
/// offline CLI's gate come from [`UsageBudget::kept_in`].
#[derive(Default)]
pub(crate) struct UsageBudget {
    limited: bool,
    path: Mutex<Option<PathBuf>>,
    buckets: Mutex<HashMap<Provider, Bucket>>,
    holds: Mutex<BTreeMap<String, ProviderHold>>,
    flights: Mutex<HashMap<Provider, Arc<tokio::sync::Semaphore>>>,
    /// When the last fresh check of a switch's candidate took a token, per provider.
    confirmed_at: Mutex<HashMap<Provider, i64>>,
}

impl UsageBudget {
    /// The production budget, with the provider holds recorded before that still apply.
    pub(crate) fn kept_in(path: PathBuf, now: i64) -> Self {
        let budget = Self {
            limited: true,
            ..Default::default()
        };
        if let Ok(bytes) = switchboard_core::private_fs::read_private(&path, MAX_FILE) {
            match serde_json::from_slice::<HoldsFile>(&bytes) {
                Ok(file) if file.version == 1 => {
                    let kept: BTreeMap<_, _> = file
                        .providers
                        .into_iter()
                        .filter(|(name, hold)| {
                            matches!(name.as_str(), "claude" | "codex")
                                && hold.until > hold.recorded_at
                                && (hold.until > now
                                    || hold.recorded_at > now.saturating_add(AHEAD))
                        })
                        .map(|(name, mut hold)| {
                            hold.until = hold.until.min(
                                hold.recorded_at
                                    .saturating_add(crate::usage_gate::MAX_NOT_BEFORE),
                            );
                            (name, hold)
                        })
                        .collect();
                    *lock(&budget.holds) = kept;
                }
                // Nothing in it can shorten a wait; it is rewritten at the next 429.
                _ => crate::oplog::event(
                    "usage_holds_unreadable",
                    &[("outcome", crate::oplog::Field::Code("provider"))],
                ),
            }
        }
        *lock(&budget.path) = Some(path);
        budget
    }

    #[cfg(test)]
    pub(crate) fn limited() -> Self {
        Self {
            limited: true,
            ..Default::default()
        }
    }

    fn save(&self, holds: &BTreeMap<String, ProviderHold>) {
        let Some(path) = lock(&self.path).clone() else {
            return;
        };
        let file = HoldsFile {
            version: 1,
            providers: holds.clone(),
        };
        let saved = serde_json::to_vec(&file)
            .map_err(|_| String::new())
            .and_then(|bytes| switchboard_core::private_fs::private_write(&path, &bytes));
        if saved.is_err() {
            crate::oplog::event(
                "usage_holds_unsaved",
                &[("outcome", crate::oplog::Field::Code("provider"))],
            );
        }
    }

    /// Until when the provider is held, if it is at `now`. A hold stamped ahead of a clock set
    /// back is moved to `now` plus what was left of it.
    pub(crate) fn held_until(&self, provider: Provider, now: i64) -> Option<i64> {
        if !self.limited {
            return None;
        }
        let mut holds = lock(&self.holds);
        let hold = holds.get_mut(provider.as_str())?;
        if hold.recorded_at > now.saturating_add(AHEAD) {
            let left = hold.until.saturating_sub(hold.recorded_at).max(0);
            hold.recorded_at = now;
            hold.until = now.saturating_add(left);
        }
        (now < hold.until).then_some(hold.until)
    }

    /// A 429 on a usage check of `provider`: the whole provider waits.
    pub(crate) fn refused(&self, provider: Provider, now: i64, retry_after: Option<i64>) -> i64 {
        if !self.limited {
            return now;
        }
        let snapshot = {
            let mut holds = lock(&self.holds);
            // A hold that ended more than its own length ago is history: the next 429 starts
            // again from the floor.
            // A 429 while the hold still runs (two checks were in flight together) is the
            // same refusal: it doubles nothing.
            let strikes = match holds.get(provider.as_str()) {
                Some(h) if now < h.until => h.strikes.max(1),
                other => other
                    .filter(|h| {
                        now.saturating_sub(h.until) <= h.until.saturating_sub(h.recorded_at)
                    })
                    .map_or(0, |h| h.strikes)
                    .saturating_add(1),
            };
            let until = now.saturating_add(provider_hold_seconds(retry_after, strikes));
            // Never shortens a hold already longer.
            let until = holds.get(provider.as_str()).map_or(until, |h| {
                if h.until > now {
                    h.until.max(until)
                } else {
                    until
                }
            });
            holds.insert(
                provider.as_str().to_owned(),
                ProviderHold {
                    until,
                    recorded_at: now,
                    strikes,
                },
            );
            holds.clone()
        };
        crate::oplog::event(
            "usage_provider_held",
            &[
                ("provider", crate::oplog::Field::Code(provider.as_str())),
                (
                    "seconds",
                    crate::oplog::Field::Number(
                        snapshot[provider.as_str()].until.saturating_sub(now),
                    ),
                ),
                (
                    "strikes",
                    crate::oplog::Field::Number(i64::from(snapshot[provider.as_str()].strikes)),
                ),
            ],
        );
        self.save(&snapshot);
        snapshot[provider.as_str()].until
    }

    /// A usage check of `provider` answered: the doubling starts again from the floor. Writes
    /// only when a hold was recorded.
    pub(crate) fn succeeded(&self, provider: Provider) {
        self.succeeded_since(provider, i64::MAX);
    }
    /// A usage check of `provider` that started at `started` answered. A hold recorded since it
    /// started (another check in flight beside it was refused) stays: an answer sent before that
    /// 429 says nothing about it.
    pub(crate) fn succeeded_since(&self, provider: Provider, started: i64) {
        if !self.limited {
            return;
        }
        let snapshot = {
            let mut holds = lock(&self.holds);
            match holds.get(provider.as_str()) {
                Some(hold) if hold.recorded_at < started => {}
                _ => return,
            }
            holds.remove(provider.as_str());
            holds.clone()
        };
        self.save(&snapshot);
    }

    /// Takes one token for `caller` at `now`. The background needs a whole token and a free
    /// flight; a person or a confirmation may run [`DEBT`] tokens ahead. Refused while the
    /// provider is held.
    pub(crate) fn take(&self, provider: Provider, caller: Caller, now: i64) -> Result<(), String> {
        if !self.limited {
            return Ok(());
        }
        if self.held_until(provider, now).is_some() {
            return Err(switchboard_proxy::USAGE_RATE_LIMITED.into());
        }
        if caller == Caller::Background && self.flight(provider).available_permits() == 0 {
            return Err(USAGE_PACED.into());
        }
        if caller == Caller::Confirm
            && lock(&self.confirmed_at)
                .get(&provider)
                .is_some_and(|at| now >= *at && now - at < CONFIRM_SPACING_SECONDS)
        {
            return Err(USAGE_PACED.into());
        }
        let mut buckets = lock(&self.buckets);
        let bucket = buckets.entry(provider).or_default();
        bucket.refill(now);
        let floor = if caller == Caller::Background {
            TOKEN_SECONDS
        } else {
            TOKEN_SECONDS - DEBT * TOKEN_SECONDS
        };
        if bucket.credit < floor {
            return Err(USAGE_PACED.into());
        }
        bucket.credit -= TOKEN_SECONDS;
        drop(buckets);
        if caller == Caller::Confirm {
            lock(&self.confirmed_at).insert(provider, now);
        }
        Ok(())
    }

    fn flight(&self, provider: Provider) -> Arc<tokio::sync::Semaphore> {
        lock(&self.flights)
            .entry(provider)
            .or_insert_with(|| {
                Arc::new(tokio::sync::Semaphore::new(if self.limited {
                    IN_FLIGHT
                } else {
                    tokio::sync::Semaphore::MAX_PERMITS
                }))
            })
            .clone()
    }

    /// A place on the network for one check of `provider`: the background takes one only when
    /// it is free; anyone else waits for one (inside its caller's deadline).
    pub(crate) async fn fly(
        &self,
        provider: Provider,
        caller: Caller,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
        let flight = self.flight(provider);
        if caller == Caller::Background {
            return flight.try_acquire_owned().map_err(|_| USAGE_PACED.into());
        }
        flight.acquire_owned().await.map_err(|_| USAGE_PACED.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucket_allows_its_burst_then_two_a_minute() {
        let budget = UsageBudget::limited();
        let p = Provider::Claude;
        for _ in 0..4 {
            assert!(budget.take(p, Caller::Background, 1_000).is_ok());
        }
        assert_eq!(
            budget.take(p, Caller::Background, 1_000).unwrap_err(),
            USAGE_PACED
        );
        // A token every 30 seconds.
        assert!(budget.take(p, Caller::Background, 1_029).is_err());
        assert!(budget.take(p, Caller::Background, 1_030).is_ok());
        // Another provider has its own bucket.
        assert!(budget
            .take(Provider::Codex, Caller::Background, 1_030)
            .is_ok());
        // A person runs ahead up to the debt; the background then waits it out.
        for _ in 0..4 {
            assert!(budget.take(p, Caller::Person, 1_030).is_ok());
        }
        assert!(budget.take(p, Caller::Person, 1_030).is_err());
        assert!(budget.take(p, Caller::Background, 1_030 + 120).is_err());
        assert!(budget.take(p, Caller::Background, 1_030 + 150).is_ok());
        // A switch's candidate: once a minute per provider, whatever the bucket holds.
        let confirming = UsageBudget::limited();
        assert!(confirming.take(p, Caller::Confirm, 5_000).is_ok());
        assert!(confirming.take(p, Caller::Confirm, 5_059).is_err());
        assert!(confirming.take(p, Caller::Background, 5_059).is_ok());
        assert!(confirming.take(p, Caller::Confirm, 5_060).is_ok());
        // A clock set back refills nothing.
        assert!(budget.take(p, Caller::Background, 500).is_err());
        // A test's gate is unbudgeted.
        let free = UsageBudget::default();
        for _ in 0..100 {
            assert!(free.take(p, Caller::Background, 1).is_ok());
        }
    }

    #[test]
    fn a_429_holds_the_whole_provider_and_doubles_on_repeats_up_to_an_hour() {
        // Never under the 900 s floor a single account's hold has (`retry-after: 0`).
        assert_eq!(PROVIDER_HOLD_FLOOR, 900);
        assert_eq!(provider_hold_seconds(None, 1), 900);
        assert_eq!(provider_hold_seconds(Some(0), 1), 900);
        assert_eq!(provider_hold_seconds(Some(60), 1), 900);
        assert_eq!(provider_hold_seconds(Some(1200), 1), 1200);
        assert_eq!(provider_hold_seconds(None, 2), 1800);
        assert_eq!(provider_hold_seconds(None, 3), 3600);
        assert_eq!(provider_hold_seconds(None, 40), 3600);
        // A longer wait the provider asks for is kept whole.
        assert_eq!(provider_hold_seconds(Some(7200), 1), 7200);

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(PROVIDER_HOLDS_FILE);
        let budget = UsageBudget::kept_in(path.clone(), 1_000);
        assert_eq!(budget.refused(Provider::Claude, 1_000, None), 1_900);
        // Every check of the provider waits, a person's too; another provider does not.
        assert!(budget
            .take(Provider::Claude, Caller::Person, 1_899)
            .is_err());
        assert!(budget
            .take(Provider::Codex, Caller::Background, 1_899)
            .is_ok());
        assert!(budget
            .take(Provider::Claude, Caller::Background, 1_900)
            .is_ok());
        // The next 429 doubles it; a restart honours it.
        assert_eq!(budget.refused(Provider::Claude, 1_910, Some(5)), 3_710);
        let restarted = UsageBudget::kept_in(path.clone(), 2_000);
        assert_eq!(restarted.held_until(Provider::Claude, 2_000), Some(3_710));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"claude\"") && !text.contains("token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // A success ends the doubling: the next 429 is back at the floor.
        restarted.succeeded(Provider::Claude);
        assert_eq!(restarted.held_until(Provider::Claude, 2_000), None);
        assert_eq!(restarted.refused(Provider::Claude, 4_000, None), 4_900);
        // A 429 long after the last hold ended starts again from the floor.
        let aging = UsageBudget::limited();
        assert_eq!(aging.refused(Provider::Codex, 10_000, None), 10_900);
        assert_eq!(aging.refused(Provider::Codex, 10_900, None), 12_700);
        // That hold lasted 1800 s and ended at 12 700: a 429 within 1800 s after doubles again,
        assert_eq!(aging.refused(Provider::Codex, 14_000, None), 17_600);
        // one later than its length after the end does not.
        assert_eq!(aging.refused(Provider::Codex, 17_600 + 3_601, None), 22_101);
        // Two checks in flight together: a 429 while the hold runs doubles nothing, and an
        // answer to a check that started before that 429 leaves the hold in place.
        let pair = UsageBudget::limited();
        assert_eq!(pair.refused(Provider::Claude, 30_000, None), 30_900);
        assert_eq!(pair.refused(Provider::Claude, 30_001, None), 30_901);
        pair.succeeded_since(Provider::Claude, 29_999);
        assert_eq!(pair.held_until(Provider::Claude, 30_002), Some(30_901));
        // An answer to a check that started after it ends the hold.
        pair.succeeded_since(Provider::Claude, 30_002);
        assert_eq!(pair.held_until(Provider::Claude, 30_002), None);
        // A hold over by the next start is dropped.
        assert_eq!(
            UsageBudget::kept_in(path, 4_900).held_until(Provider::Claude, 4_900),
            None
        );
    }

    #[test]
    fn jitter_spreads_a_schedule_by_at_most_fifteen_percent_and_leaves_short_ones() {
        let mut seen = std::collections::BTreeSet::new();
        for n in 0..200 {
            let id = format!("account-{n}");
            let due = jittered(&id, 1_000, 1_180);
            assert!((1_153..=1_207).contains(&due), "{due}");
            assert_eq!(due, jittered(&id, 1_000, 1_180), "the same every time");
            seen.insert(due);
        }
        assert!(seen.len() > 20, "spread, not one value");
        assert_eq!(jittered("a", 1_000, 1_060), 1_060);
        assert_eq!(jittered("a", 1_000, 1_000), 1_000);
    }

    #[tokio::test]
    async fn at_most_two_checks_of_a_provider_fly_at_once() {
        let budget = UsageBudget::limited();
        let p = Provider::Claude;
        let one = budget.fly(p, Caller::Person).await.unwrap();
        let two = budget.fly(p, Caller::Person).await.unwrap();
        assert!(budget.fly(p, Caller::Background).await.is_err());
        assert!(
            budget.take(p, Caller::Background, 1).is_err(),
            "no free flight"
        );
        drop(one);
        assert!(budget.fly(p, Caller::Background).await.is_ok());
        drop(two);
    }
}
