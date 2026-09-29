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
        self.publish(&mut state, candidate)
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
        policy.validate()?;
        if now <= 0 {
            return Err("Invalid rotation time".into());
        }
        if !policy.enabled {
            return Ok(hold("disabled"));
        }
        if policy
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
        if !self.rotation_credential_eligible(current, policy, now) {
            return Ok(hold("current_unavailable"));
        }
        let Some(usage) = fresh_usage(current, policy, now) else {
            return Ok(hold("usage_unavailable"));
        };
        if usage < policy.threshold_percent {
            return Ok(hold("below_threshold"));
        }
        let candidate = state
            .accounts
            .iter()
            .filter(|a| {
                a.id != current.id && a.provider == policy.provider && a.pool == policy.pool
            })
            .filter_map(|a| fresh_usage(a, policy, now).map(|used| (a, used)))
            .filter(|(a, used)| {
                *used < policy.threshold_percent - policy.hysteresis_percent
                    && self.rotation_credential_eligible(a, policy, now)
            })
            .min_by(|(a, x), (b, y)| {
                x.total_cmp(y)
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
        Ok(match candidate {
            Some((a, _)) => RotationDecision {
                candidate_id: Some(a.id.clone()),
                reason: "threshold_reached".into(),
            },
            None => hold("no_eligible_account"),
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
        credential.validate(account.provider, account.kind).is_ok()
            && credential.expires_at.is_none_or(|t| t > now)
            && (policy.target != "claude_cli" || credential.native_context.is_some())
    }
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
        // A reset crossing needs a fresh provider observation, never a guessed zero.
        || usage.resets_at.is_some_and(|t| t <= now)
        || usage.windows.iter().any(|w| w.resets_at.is_some_and(|t| t <= now))
    {
        return None;
    }
    Some(usage.used_percent)
}
