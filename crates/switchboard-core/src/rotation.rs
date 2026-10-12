use crate::{ahead, pool_valid, Account, AuthKind, Provider, Store};
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
}
fn hold(reason: &str) -> RotationDecision {
    RotationDecision {
        candidate_id: None,
        reason: reason.into(),
    }
}
impl Store {
    pub fn set_policy(&self, mut policy: RotationPolicy) -> Result<(), String> {
        policy.validate()?;
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
        let current = current_id.and_then(|id| {
            state
                .accounts
                .iter()
                .find(|a| a.id == id && a.provider == policy.provider && a.pool == policy.pool)
        });
        let Some(current) = current else {
            return Ok(hold("current_unavailable"));
        };
        if !current_limited {
            if !self.rotation_credential_eligible(current, policy, now) {
                return Ok(hold("current_unavailable"));
            }
            let Some(usage) = fresh_usage(current, policy, now) else {
                return Ok(hold("usage_unavailable"));
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
                    && a.pool == policy.pool
                    && !limited.contains(&a.id)
            })
            .filter_map(|a| fresh_usage(a, policy, now).map(|used| (a, used)))
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
            .min_by(|(a, x), (b, y)| {
                x.total_cmp(y)
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
                        && a.pool == policy.pool
                        && !limited.contains(&a.id)
                        && self.rotation_credential_eligible(a, policy, now)
                })
                .filter_map(|a| {
                    let used = last_known_usage(a, now);
                    (used < 100.0).then_some((a, used))
                })
                .min_by(|(a, x), (b, y)| {
                    x.total_cmp(y)
                        .then_with(|| a.created_at.cmp(&b.created_at))
                        .then_with(|| a.id.cmp(&b.id))
                })
        });
        Ok(match candidate {
            Some((a, _)) => RotationDecision {
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
    fn rotation_credential_eligible(
        &self,
        account: &Account,
        policy: &RotationPolicy,
        now: i64,
    ) -> bool {
        if !account.enabled
            || (policy.target == "claude_cli"
                && (account.provider != Provider::Claude
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
/// it left — a subscription session never moves onto a pay-per-token key, nor a key's caller
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
        self.kind.is_none_or(|kind| account.kind == kind)
            && (!self.api_key_only || account.kind == AuthKind::ApiKey)
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
        let state = self.lock()?;
        Ok(state
            .accounts
            .iter()
            .filter(|a| {
                a.provider == policy.provider
                    && a.pool == policy.pool
                    && !held.contains(&a.id)
                    && fits.admits(a)
                    && self.rotation_credential_eligible(a, policy, now)
            })
            .filter_map(|a| match fresh_usage(a, policy, now) {
                Some(used) if used < 100.0 => Some((a, 0u8, used)),
                Some(_) => None,
                None => {
                    let used = last_known_usage(a, now);
                    (used < 100.0).then_some((a, 1u8, used))
                }
            })
            .min_by(|(a, tier_a, x), (b, tier_b, y)| {
                tier_a
                    .cmp(tier_b)
                    .then_with(|| x.total_cmp(y))
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            })
            .map(|(a, _, _)| a.id.clone()))
    }
}
/// The best guess at an account's usage when nothing fresh is known (SB-98): its last
/// account-wide figure, 0 when every window it names has since reset, and 99 — last in line,
/// still below a full account — when it was never measured.
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
fn fresh_usage(account: &Account, policy: &RotationPolicy, now: i64) -> Option<f64> {
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
        || usage.observed_at > now || now - usage.observed_at > policy.max_age_seconds
        || now - health.checked_at > policy.max_age_seconds
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
