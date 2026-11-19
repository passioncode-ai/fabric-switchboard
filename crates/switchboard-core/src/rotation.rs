use crate::{ahead, pool_valid, Account, AuthKind, Provider, Store, Vault};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationPolicy {
    pub provider: Provider,
    pub pool: String,
    pub target: String,
    #[serde(default)]
    pub enabled: bool,
    pub threshold_percent: f64,
    pub hysteresis_percent: f64,
    pub cooldown_seconds: i64,
    pub max_age_seconds: i64,
    #[serde(default)]
    pub last_switched_at: Option<i64>,
}
impl RotationPolicy {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !pool_valid(&self.pool)
            || !matches!(self.target.as_str(), "managed" | "claude_cli")
            || (self.target == "claude_cli" && self.provider != Provider::Claude)
            || !self.threshold_percent.is_finite()
            || !(0.0..=100.0).contains(&self.threshold_percent)
            || self.threshold_percent == 0.0
            || !self.hysteresis_percent.is_finite()
            || self.hysteresis_percent < 0.0
            || self.hysteresis_percent >= self.threshold_percent
            || !(0..=604_800).contains(&self.cooldown_seconds)
            || !(1..=86_400).contains(&self.max_age_seconds)
            || self.last_switched_at.is_some_and(|t| t <= 0)
        {
            return Err("Invalid rotation policy".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationDecision {
    pub candidate_id: Option<String>,
    pub reason: String,
    /// DEC-0022 (SB-403): the candidate holds none of the group's organizations (taken only
    /// when no account of them had room), so the pass's decision says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
}
fn hold(reason: &str) -> RotationDecision {
    RotationDecision {
        candidate_id: None,
        reason: reason.into(),
        outside: false,
    }
}
/// The figures a managed policy starts with when nobody saved one (the initial UI/CLI settings).
pub fn default_managed_policy(provider: Provider, pool: &str) -> RotationPolicy {
    RotationPolicy {
        provider,
        pool: pool.to_owned(),
        target: "managed".into(),
        enabled: true,
        threshold_percent: 90.0,
        hysteresis_percent: 10.0,
        cooldown_seconds: 1800,
        max_age_seconds: 300,
        last_switched_at: None,
    }
}
/// SB-310 (operator, 2026-11-08): the (provider, group) pairs that have accounts serving them and
/// no saved managed policy, each once, in a stable order.
pub(crate) fn missing_managed_policies(s: &crate::Snapshot) -> Vec<(Provider, String)> {
    let mut out: Vec<(Provider, String)> = Vec::new();
    let pools: std::collections::BTreeSet<&str> = s
        .accounts
        .iter()
        .map(|a| a.pool.as_str())
        .chain(s.projects.iter().map(|p| p.pool.as_str()))
        // SB-355: a machine group never switches accounts here.
        .filter(|pool| !crate::machines::is_machine_pool(pool))
        .collect();
    for pool in pools {
        for a in s.pool_accounts(pool) {
            let saved = s
                .policies
                .iter()
                .any(|p| p.provider == a.provider && p.pool == pool && p.target == "managed");
            if !saved && !out.iter().any(|(p, q)| *p == a.provider && q == pool) {
                out.push((a.provider, pool.to_owned()));
            }
        }
    }
    out
}
impl Store {
    /// SB-310 (operator, 2026-11-08): automatic switching of managed sessions is on by default.
    /// Every provider and group with accounts and no saved managed policy gets an enabled one
    /// with the initial settings; a saved policy, switched off included, is never touched, so
    /// the person's *off* stands. Reads the published state first and writes only when a pair
    /// is missing. Returns how many were added.
    pub fn ensure_default_policies(&self) -> Result<usize, String> {
        if missing_managed_policies(&self.snapshot()?).is_empty() {
            return Ok(0);
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let mut added = 0;
        for (provider, pool) in missing_managed_policies(&candidate) {
            if candidate.policies.len() >= crate::MAX_ACCOUNTS {
                break;
            }
            candidate
                .policies
                .push(default_managed_policy(provider, &pool));
            added += 1;
        }
        if added > 0 {
            self.publish(&mut state, candidate)?;
            self.changed();
        }
        Ok(added)
    }
    pub fn set_policy(&self, mut policy: RotationPolicy) -> Result<(), String> {
        policy.validate()?;
        if crate::machines::is_machine_pool(&policy.pool) {
            return Err(crate::machines::MACHINE_NOT_ROTATED.into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        // The ordinary Claude Code serves every folder; a project's accounts serve only its own.
        if policy.enabled
            && policy.target == "claude_cli"
            && candidate.project_of_pool(&policy.pool).is_some()
        {
            return Err(crate::PROJECT_NOT_NATIVE.into());
        }
        if let Some(old) = candidate.policies.iter_mut().find(|p| {
            p.provider == policy.provider && p.pool == policy.pool && p.target == policy.target
        }) {
            // Settings cannot erase or forge the runtime's cooldown history.
            policy.last_switched_at = old.last_switched_at;
            *old = policy;
        } else {
            policy.last_switched_at = None;
            candidate.policies.push(policy);
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    pub fn mark_rotated(
        &self,
        provider: Provider,
        pool: &str,
        target: &str,
        now: i64,
    ) -> Result<(), String> {
        if now <= 0 {
            return Err("Invalid rotation time".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let p = candidate
            .policies
            .iter_mut()
            .find(|p| p.provider == provider && p.pool == pool && p.target == target)
            .ok_or("Rotation policy not found")?;
        if p.last_switched_at.is_some_and(|t| t > now && !ahead(t)) {
            return Err("Rotation time precedes the last switch".into());
        }
        p.last_switched_at = Some(now);
        self.publish(&mut state, candidate)
    }
    /// Pure selection: the caller applies a successful switch and then marks its cooldown.
    /// Unknown/failed/stale current usage holds; no failover or provider request is implicit.
    pub fn rotation_decision(
        &self,
        policy: &RotationPolicy,
        current_id: Option<&str>,
        now: i64,
    ) -> Result<RotationDecision, String> {
        self.rotation_decision_with(policy, current_id, now, &Default::default())
    }
    /// As `rotation_decision`, with the accounts a provider limit error has stopped (0.5).
    /// A limited current account moves at once — the error is evidence the quota figures
    /// cannot show, such as a seat's spend limit — and the cooldown does not hold it there.
    /// A limited account is never a candidate.
    pub fn rotation_decision_with(
        &self,
        policy: &RotationPolicy,
        current_id: Option<&str>,
        now: i64,
        limited: &std::collections::HashSet<String>,
    ) -> Result<RotationDecision, String> {
        self.decide(policy, current_id, now, limited)
    }
    /// SB-518 (DEC-0032): the candidate a switch would take if its last known figure, of any age,
    /// were current, when no candidate has a figure as recent as the policy allows: the caller checks it fresh before the pass decides, and `rotation_decision_with`
    /// then switches to it only on that fresh figure. None when the decision already has a
    /// candidate known fresh, holds for another reason, or the account in use is held by a
    /// limit (that switch takes the last known figure, SB-98, and waits for nothing).
    pub fn rotation_candidate_to_confirm(
        &self,
        policy: &RotationPolicy,
        current_id: Option<&str>,
        now: i64,
        limited: &std::collections::HashSet<String>,
    ) -> Result<Option<String>, String> {
        if current_id.is_none_or(|id| limited.contains(id)) || !policy.enabled {
            return Ok(None);
        }
        // From the metadata first: an account in use below its threshold, or with no current
        // figure, switches nowhere, and no credential is read to learn so (LC-04).
        let over = {
            let state = self.lock()?;
            state
                .accounts
                .iter()
                .find(|a| Some(a.id.as_str()) == current_id)
                .and_then(|a| {
                    let age = policy.max_age_seconds.max(CURRENT_MAX_AGE_FLOOR_SECONDS);
                    fresh_usage(a, age, now).or_else(|| self.header_figure(a, age, now))
                })
                .is_some_and(|used| used >= policy.threshold_percent)
        };
        if !over {
            return Ok(None);
        }
        // Only where the strict decision found nobody with a current figure: a candidate it
        // would take, a cooldown or an unusable account in use needs no check.
        let strict = self.rotation_decision_with(policy, current_id, now, limited)?;
        if strict.candidate_id.is_some() || strict.reason != "no_eligible_account" {
            return Ok(None);
        }
        let current = current_id.unwrap_or_default();
        let current_kind = {
            let state = self.lock()?;
            match state.accounts.iter().find(|a| a.id == current) {
                Some(a) => a.kind,
                None => return Ok(None),
            }
        };
        // SB-518 (round 1): ranked on the last figure known at any age; the figure only orders,
        // the fresh check decides. Never measured reads as 99 (last), a failed or paced account
        // after every healthy one, and an account last known full is not checked.
        let paced = self.paced.lock().map(|p| p.clone()).unwrap_or_default();
        let ranked: Vec<(bool, u8, f64, i64, String)> = {
            let state = self.lock()?;
            state
                .accounts
                .iter()
                .filter(|a| {
                    a.id != current
                        && a.provider == policy.provider
                        && state.chooses(a, &policy.pool)
                        && !limited.contains(&a.id)
                        && !state.is_assigned(&a.id)
                        && a.kind == current_kind
                        && fresh_usage(a, policy.max_age_seconds, now).is_none()
                })
                .filter_map(|a| {
                    let used = last_known_usage(a, now);
                    let blind = a.usage_health.as_ref().is_none_or(|h| h.status != "ok")
                        || paced.contains(&a.id);
                    // Usage only grows until a reset: an account last known at or past the
                    // decision's headroom, none of whose windows has reset since, could not be
                    // taken whatever a fresh check found, so it is not checked.
                    let past_headroom = used
                        >= policy.threshold_percent - policy.hysteresis_percent
                        && no_window_reset(a, now);
                    (used < 100.0 && !past_headroom).then(|| {
                        (
                            !state.context_admits(&policy.pool, a),
                            u8::from(blind),
                            used,
                            a.created_at,
                            a.id.clone(),
                        )
                    })
                })
                .collect()
        };
        let mut ranked = ranked;
        ranked.sort_by(|x, y| {
            x.0.cmp(&y.0)
                .then_with(|| x.1.cmp(&y.1))
                .then_with(|| x.2.total_cmp(&y.2))
                .then_with(|| x.3.cmp(&y.3))
                .then_with(|| x.4.cmp(&y.4))
        });
        // The credential is read only for the one to check, and the next if it is unusable.
        let state_accounts = self.snapshot()?.accounts;
        Ok(ranked.into_iter().map(|r| r.4).find(|id| {
            state_accounts
                .iter()
                .find(|a| a.id == *id)
                .is_some_and(|a| self.rotation_credential_eligible(a, policy, now))
        }))
    }
    fn decide(
        &self,
        policy: &RotationPolicy,
        current_id: Option<&str>,
        now: i64,
        limited: &std::collections::HashSet<String>,
    ) -> Result<RotationDecision, String> {
        policy.validate()?;
        if now <= 0 {
            return Err("Invalid rotation time".into());
        }
        if !policy.enabled {
            return Ok(hold("disabled"));
        }
        let current_limited = current_id.is_some_and(|id| limited.contains(id));
        if !current_limited
            && policy
                .last_switched_at
                .is_some_and(|t| now < t || now - t < policy.cooldown_seconds)
        {
            return Ok(hold("cooldown"));
        }
        let state = self.lock()?;
        let outside = |a: &crate::Account| !state.context_admits(&policy.pool, a);
        let current = current_id.and_then(|id| {
            state.accounts.iter().find(|a| {
                a.id == id && a.provider == policy.provider && state.serves(a, &policy.pool)
            })
        });
        let Some(current) = current else {
            return Ok(hold("current_unavailable"));
        };
        if !current_limited {
            if !self.rotation_credential_eligible(current, policy, now) {
                return Ok(hold("current_unavailable"));
            }
            // SB-518: the account in use is held to the policy's age, but never to less than
            // the real interval between its checks (`CURRENT_MAX_AGE_FLOOR_SECONDS`).
            let current_age = policy.max_age_seconds.max(CURRENT_MAX_AGE_FLOOR_SECONDS);
            let header = self.header_figure(current, current_age, now);
            let Some(usage) = fresh_usage(current, current_age, now)
                .or_else(|| header.filter(|used| *used >= policy.threshold_percent))
            else {
                // Headers show the five-hour and seven-day figures below the threshold while
                // the endpoint's per-model windows age: nothing is unavailable, the endpoint's
                // next answer is awaited (at most `PASSIVE_CEILING_SECONDS` after its last).
                return Ok(hold(if header.is_some() {
                    "stale_usage"
                } else {
                    "usage_unavailable"
                }));
            };
            if usage < policy.threshold_percent {
                return Ok(hold("below_threshold"));
            }
        }
        let candidate = state
            .accounts
            .iter()
            .filter(|a| {
                a.id != current.id
                    && a.provider == policy.provider
                    && state.chooses(a, &policy.pool)
                    && !limited.contains(&a.id)
                    && !state.is_assigned(&a.id)
                    // SB-217 (the architect's judged audit): the kind of account the sessions
                    // run on now, so a subscription never moves onto a pay-per-token key that no
                    // spending ceiling gates, and a key's caller never onto a subscription.
                    && a.kind == current.kind
            })
            .filter_map(|a| fresh_usage(a, policy.max_age_seconds, now).map(|used| (a, used)))
            .filter(|(a, used)| {
                // An account in use that fails every request is worse than any account with
                // room left; otherwise the usual headroom applies.
                let limit = if current_limited {
                    100.0
                } else {
                    policy.threshold_percent - policy.hysteresis_percent
                };
                *used < limit && self.rotation_credential_eligible(a, policy, now)
            })
            // DEC-0022 (SB-403): every account the group holds serves it; one of the group's own
            // organizations is preferred over one placed there from another.
            .min_by(|(a, x), (b, y)| {
                outside(a)
                    .cmp(&outside(b))
                    .then_with(|| x.total_cmp(y))
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
        // SB-98: an account in use that fails every request must not keep an agent waiting for
        // fresh figures. Without a candidate known fresh, take the one whose last known usage
        // leaves room — a window whose reset has passed counts as empty — and one never measured
        // last; a wrong guess costs one refused request, which then holds that account too.
        let candidate = candidate.or_else(|| {
            if !current_limited {
                return None;
            }
            state
                .accounts
                .iter()
                .filter(|a| {
                    a.id != current.id
                        && a.provider == policy.provider
                        && state.chooses(a, &policy.pool)
                        && !limited.contains(&a.id)
                        && !state.is_assigned(&a.id)
                        && a.kind == current.kind
                        && self.rotation_credential_eligible(a, policy, now)
                })
                .filter_map(|a| {
                    let used = last_known_usage(a, now);
                    (used < 100.0).then_some((a, used))
                })
                .min_by(|(a, x), (b, y)| {
                    outside(a)
                        .cmp(&outside(b))
                        .then_with(|| x.total_cmp(y))
                        .then_with(|| a.created_at.cmp(&b.created_at))
                        .then_with(|| a.id.cmp(&b.id))
                })
        });
        // SB-377 (operator 2026-11-14): with no account that has room, a group whose account
        // is at its limit waits on the account that will be free first, not on the one it has:
        // the one whose limit resets earliest, and only when that is before the current one's.
        let candidate = candidate.or_else(|| {
            if !current_limited {
                return None;
            }
            let current_free = free_at(current, now).unwrap_or(i64::MAX);
            state
                .accounts
                .iter()
                .filter(|a| {
                    a.id != current.id
                        && a.provider == policy.provider
                        && state.chooses(a, &policy.pool)
                        && !state.is_assigned(&a.id)
                        && a.kind == current.kind
                        && self.rotation_credential_eligible(a, policy, now)
                })
                // Only a known reset to come: an account held with no known reset is not a wait,
                // and one with room was the steps above.
                .filter_map(|a| free_at(a, now).map(|t| (a, t)))
                .filter(|(_, t)| *t > now && *t < current_free)
                .min_by(|(a, x), (b, y)| {
                    outside(a)
                        .cmp(&outside(b))
                        .then_with(|| x.cmp(y))
                        .then_with(|| a.id.cmp(&b.id))
                })
                .map(|(a, t)| (a, t as f64))
        });
        Ok(match candidate {
            Some((a, _)) => RotationDecision {
                outside: outside(a),
                candidate_id: Some(a.id.clone()),
                reason: if current_limited {
                    "limit_reached"
                } else {
                    "threshold_reached"
                }
                .into(),
            },
            None => hold(if current_limited {
                "limit_no_eligible_account"
            } else {
                "no_eligible_account"
            }),
        })
    }
    /// SB-518 (DEC-0032): the five-hour and seven-day figures of the account in use as its
    /// last complete response headers gave them, when those came within `max_age` and nothing
    /// since says otherwise: the higher of the two. Used only to learn that the account in use
    /// reached its threshold while the endpoint's per-model windows are older (a header never
    /// freshens those); below the threshold such a figure decides nothing, and without one the
    /// usage is unknown, never zero.
    fn header_figure(&self, account: &Account, max_age: i64, now: i64) -> Option<f64> {
        let seen = self.header_seen_at(&account.id)?;
        let usage = account.usage.as_ref()?;
        let health = account.usage_health.as_ref()?;
        if health.status != "ok" || seen > now || now - seen > max_age || health.checked_at < seen {
            return None;
        }
        let windows: Vec<&crate::UsageWindow> = crate::HEADER_WINDOWS
            .iter()
            .map(|name| usage.windows.iter().find(|w| w.name == *name))
            .collect::<Option<_>>()?;
        if windows
            .iter()
            .any(|w| w.resets_at.is_some_and(|t| t <= now))
        {
            return None;
        }
        windows.iter().map(|w| w.used_percent).reduce(f64::max)
    }
    fn rotation_credential_eligible(
        &self,
        account: &Account,
        policy: &RotationPolicy,
        now: i64,
    ) -> bool {
        // SB-352 wave 2: native rotation of the ordinary Claude Code takes only an account at
        // home in its pool, never one a group lent to `default`.
        if !account.enabled
            || (policy.target == "claude_cli"
                && (account.provider != Provider::Claude
                    || account.pool != policy.pool
                    || account.kind != AuthKind::OAuth
                    || account
                        .external_identity
                        .as_ref()
                        .is_none_or(|i| i.account_id.is_none())))
        {
            return false;
        }
        let Ok(credential) = self.vault.get(&account.id) else {
            return false;
        };
        // Managed requests present the access token itself. Claude Code renews an expired
        // one from its refresh token, so native rotation needs only a live lineage.
        let live = credential.expires_at.is_none_or(|t| t > now)
            || (policy.target == "claude_cli" && credential.refresh_token.is_some());
        credential.validate(account.provider, account.kind).is_ok()
            && live
            && (policy.target != "claude_cli" || credential.native_context.is_some())
    }
}
/// What a request moved by a failover can run on (SB-102): the same kind of account as the one
/// it left, a subscription when that one is gone and the request needs no key (SB-217) — a subscription session never moves onto a pay-per-token key, nor a key's caller
/// (an agent) onto a subscription — and, for an operation only an API key serves (Chat
/// Completions), an API key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fits {
    /// The kind of the account the request leaves; None when that account is gone.
    pub kind: Option<AuthKind>,
    pub api_key_only: bool,
}
impl Fits {
    pub fn admits(&self, account: &Account) -> bool {
        // SB-217: with the account it leaves gone, a request lands on a pay-per-token key only
        // when it needs one; otherwise a subscription, which no spending ceiling has to gate.
        let kind = self.kind.unwrap_or(if self.api_key_only {
            AuthKind::ApiKey
        } else {
            AuthKind::OAuth
        });
        account.kind == kind && (!self.api_key_only || account.kind == AuthKind::ApiKey)
    }
}
impl Store {
    /// The pool's best account to move a request to (SB-102), without the account in use having
    /// to be found: enabled, credential usable, not held, `fits` it; measured lately and below
    /// full first (least used), then by last known usage (`last_known_usage`), never one still
    /// known full. Oldest, then smallest id, on a tie.
    pub(crate) fn failover_candidate(
        &self,
        policy: &RotationPolicy,
        held: &std::collections::HashSet<String>,
        fits: Fits,
        now: i64,
    ) -> Result<Option<String>, String> {
        let paced = self.paced.lock().map(|p| p.clone()).unwrap_or_default();
        let state = self.lock()?;
        let outside = |a: &crate::Account| !state.context_admits(&policy.pool, a);
        Ok(state
            .accounts
            .iter()
            .filter(|a| {
                a.provider == policy.provider
                    && state.chooses(a, &policy.pool)
                    && !held.contains(&a.id)
                    && !state.is_assigned(&a.id)
                    && fits.admits(a)
                    && self.rotation_credential_eligible(a, policy, now)
            })
            .filter_map(|a| match fresh_usage(a, policy.max_age_seconds, now) {
                Some(used) if used < 100.0 => Some((a, 0u8, used)),
                Some(_) => None,
                None => {
                    let used = last_known_usage(a, now);
                    // WF-5.2d (C4): an account whose last check failed, or whose checks the
                    // provider refused lately (SB-106), is a blind guess: it comes last, after
                    // every account last seen healthy, so the one replay SB-98 allows is not
                    // spent on it while a better one is left.
                    let blind = a.usage_health.as_ref().is_none_or(|h| h.status != "ok")
                        || paced.contains(&a.id);
                    (used < 100.0).then_some((a, if blind { 2u8 } else { 1u8 }, used))
                }
            })
            // DEC-0022 (SB-403): the group's own organizations first, as rotation prefers them.
            .min_by(|(a, tier_a, x), (b, tier_b, y)| {
                outside(a)
                    .cmp(&outside(b))
                    .then_with(|| tier_a.cmp(tier_b))
                    .then_with(|| x.total_cmp(y))
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            })
            .map(|(a, _, _)| a.id.clone()))
    }
}
/// SB-237: what the proxy's failovers did since the runtime last looked, counts only. When a
/// failover found nothing, each other account of the pool is counted once under the first
/// rule `failover_candidate` refused it by, so a group that stops at once says why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FailoverTally {
    /// Requests moved to another account of their pool.
    pub moved: u64,
    /// Requests for which no account of the pool was left.
    pub exhausted: u64,
    /// Of those, pools with no other account of the provider at all.
    pub alone: u64,
    /// Accounts left out: held by a limit, refused just now, or a seat the provider ended.
    pub held: u64,
    /// Accounts of another kind than the request's (subscription and API key never mix).
    pub kind: u64,
    /// DEC-0022 (SB-403): of `moved`, requests moved to an account that holds none of the
    /// group's organizations (taken only when none of theirs had room). Was `context`, the
    /// accounts left out for another organization, before nothing was left out for it.
    pub outside: u64,
    /// Accounts disabled, given to an agent (SB-322), or whose stored credential is missing,
    /// invalid or expired.
    pub credential: u64,
    /// Accounts measured or last known full.
    pub full: u64,
}
impl FailoverTally {
    pub fn is_empty(&self) -> bool {
        *self == FailoverTally::default()
    }
}
impl Store {
    /// SB-237: `failover_candidate` found nothing: each other account of the pool, counted
    /// under the first of its rules that left it out.
    pub(crate) fn failover_misses(
        &self,
        policy: &RotationPolicy,
        held: &std::collections::HashSet<String>,
        fits: Fits,
        now: i64,
        tally: &mut FailoverTally,
    ) -> Result<(), String> {
        let state = self.lock()?;
        let mut others = 0;
        for a in state
            .accounts
            .iter()
            .filter(|a| a.provider == policy.provider && state.serves(a, &policy.pool))
        {
            others += 1;
            let slot = if held.contains(&a.id) {
                &mut tally.held
            } else if !fits.admits(a) {
                &mut tally.kind
            } else if state.is_assigned(&a.id) || !self.rotation_credential_eligible(a, policy, now)
            {
                &mut tally.credential
            } else {
                &mut tally.full
            };
            *slot += 1;
        }
        // The account the request left is one of them, and always held.
        if others <= 1 {
            tally.alone += 1;
        }
        Ok(())
    }
}
impl Store {
    /// The account a switch or a failover would most likely take next, for a check brought
    /// forward (SB-106): `failover_candidate`'s order — measured lately and below full first,
    /// least used, then by last known usage — over enabled OAuth accounts whose last check
    /// succeeded, not held. It reads no credential: an account whose credential turns out dead
    /// fails its check and backs off as any other.
    pub(crate) fn next_to_check(
        &self,
        policy: &RotationPolicy,
        held: &std::collections::HashSet<String>,
        now: i64,
    ) -> Result<Option<String>, String> {
        let state = self.lock()?;
        let outside = |a: &crate::Account| !state.context_admits(&policy.pool, a);
        Ok(state
            .accounts
            .iter()
            .filter(|a| {
                a.provider == policy.provider
                    && state.serves(a, &policy.pool)
                    && a.enabled
                    && !state.is_assigned(&a.id)
                    && a.kind == AuthKind::OAuth
                    && !held.contains(&a.id)
                    && a.usage_health.as_ref().is_some_and(|h| h.status == "ok")
            })
            .filter_map(|a| match fresh_usage(a, policy.max_age_seconds, now) {
                Some(used) if used < 100.0 => Some((a, 0u8, used)),
                Some(_) => None,
                None => {
                    let used = last_known_usage(a, now);
                    (used < 100.0).then_some((a, 1u8, used))
                }
            })
            // DEC-0022 (SB-403): the group's own organizations first, as rotation prefers them.
            .min_by(|(a, tier_a, x), (b, tier_b, y)| {
                outside(a)
                    .cmp(&outside(b))
                    .then_with(|| tier_a.cmp(tier_b))
                    .then_with(|| x.total_cmp(y))
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            })
            .map(|(a, _, _)| a.id.clone()))
    }
}

/// SB-377: when an account will have room again on what is known of it: now when it has room
/// (last known usage under 100 %, a window whose reset passed counting as empty), else the latest
/// reset of its full windows; none when that is not known.
pub fn free_at(account: &Account, now: i64) -> Option<i64> {
    if last_known_usage(account, now) < 100.0 {
        return Some(now);
    }
    let usage = account.usage.as_ref()?;
    let full: Vec<Option<i64>> = usage
        .windows
        .iter()
        .filter(|w| !w.is_feature() && w.used_percent >= 100.0)
        .map(|w| w.resets_at)
        .collect();
    if full.is_empty() {
        return usage.resets_at.filter(|t| *t > now);
    }
    full.into_iter()
        .collect::<Option<Vec<i64>>>()?
        .into_iter()
        .max()
}

/// SB-518: the account has a figure and none of its account windows (nor the aggregate, without
/// windows) has reset since it was taken.
fn no_window_reset(account: &Account, now: i64) -> bool {
    let Some(usage) = account.usage.as_ref() else {
        return false;
    };
    let windows: Vec<_> = usage.windows.iter().filter(|w| !w.is_feature()).collect();
    if windows.is_empty() {
        return usage.resets_at.is_some_and(|t| t > now);
    }
    windows.iter().all(|w| w.resets_at.is_some_and(|t| t > now))
}
/// The best guess at an account's usage when nothing fresh is known (SB-98): its last
/// account-wide figure, 0 when every window it names has since reset, and 99 (last in line,
/// still below a full account) when it was never measured.
fn last_known_usage(account: &Account, now: i64) -> f64 {
    let Some(usage) = account.usage.as_ref() else {
        return 99.0;
    };
    let windows: Vec<_> = usage.windows.iter().filter(|w| !w.is_feature()).collect();
    let all_reset = if windows.is_empty() {
        usage.resets_at.is_some_and(|t| t <= now)
    } else {
        windows
            .iter()
            .all(|w| w.resets_at.is_some_and(|t| t <= now))
    };
    if all_reset {
        return 0.0;
    }
    usage.account_used_percent().unwrap_or(99.0)
}
/// SB-518: the account in use is checked every `CHECK_ACTIVE_SECONDS` (± 15 %), on the 30-second
/// pass, under the provider's budget: a policy age below this would hold every switch until a
/// refusal forced one.
pub const CURRENT_MAX_AGE_FLOOR_SECONDS: i64 = 300;
fn fresh_usage(account: &Account, max_age: i64, now: i64) -> Option<f64> {
    let usage = account.usage.as_ref()?;
    let health = account.usage_health.as_ref()?;
    // A partial response header set cannot establish available subscription capacity:
    // the missing weekly window may already be exhausted. JSON endpoints may legitimately
    // return null optional windows, so this requirement is specific to response headers.
    if usage.source == "response_headers"
        && !["five_hour", "seven_day"]
            .iter()
            .all(|name| usage.windows.iter().any(|w| w.name == *name))
    {
        return None;
    }
    if health.status != "ok" || health.checked_at < usage.observed_at || health.checked_at > now
        || usage.observed_at > now || now - usage.observed_at > max_age
        || now - health.checked_at > max_age
        // A reset crossing needs a fresh provider observation, never a guessed zero. A feature
        // window's reset says nothing about the account (SB-40); with windows stored, the
        // aggregate's reset is one of theirs.
        || (usage.windows.is_empty() && usage.resets_at.is_some_and(|t| t <= now))
        || usage
            .windows
            .iter()
            .any(|w| !w.is_feature() && w.resets_at.is_some_and(|t| t <= now))
    {
        return None;
    }
    // A feature limit does not stand for the account (SB-40); unknown account capacity is
    // never eligible.
    usage.account_used_percent()
}
