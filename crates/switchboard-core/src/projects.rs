//! Optional project rules: a project folder names the account its sessions should use.
use crate::{append_event, now, AuthKind, Provider, Snapshot, Store, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAX_PATH: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRule {
    pub path: String,
    pub provider: Provider,
    pub account_id: String,
    pub target: String,
    pub enabled: bool,
    pub created_at: i64,
    #[serde(default)]
    pub expires_at: Option<i64>,
}
impl ProjectRule {
    /// Paused and expired rules stay visible but never apply.
    pub fn in_force(&self, now: i64) -> bool {
        self.enabled && self.expires_at.is_none_or(|t| t > now)
    }
}

/// For one provider: the rule that applies, and the closest rule of any state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuleResolution {
    pub provider: Provider,
    pub effective: Option<ProjectRule>,
    pub nearest: Option<ProjectRule>,
}

fn path_valid(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.chars().any(char::is_control)
        && Path::new(path).is_absolute()
}

pub(crate) fn validate_rules(s: &Snapshot) -> Result<(), String> {
    if s.rules.len() > MAX_ACCOUNTS {
        return Err("Invalid metadata bounds".into());
    }
    let mut keys = std::collections::HashSet::new();
    for r in &s.rules {
        let account_matches = s
            .accounts
            .iter()
            .any(|a| a.id == r.account_id && a.provider == r.provider);
        if !path_valid(&r.path)
            || !account_matches
            || !matches!(r.target.as_str(), "managed" | "claude_cli")
            || (r.target == "claude_cli" && r.provider != Provider::Claude)
            || r.created_at <= 0
            || r.expires_at.is_some_and(|t| t <= r.created_at)
            || !keys.insert((r.path.as_str(), r.provider))
        {
            return Err("Invalid project rule metadata".into());
        }
    }
    Ok(())
}

impl Store {
    /// Saves the rule for `path` and the account's provider. The caller supplies a
    /// canonical directory; the store checks the shape, not the filesystem.
    pub fn set_rule(
        &self,
        path: &Path,
        account_id: &str,
        target: &str,
        enabled: bool,
        expires_at: Option<i64>,
    ) -> Result<ProjectRule, String> {
        let path = path
            .to_str()
            .filter(|p| path_valid(p))
            .ok_or("Choose an absolute project folder.")?
            .to_owned();
        let time = now();
        if expires_at.is_some_and(|t| t <= time) {
            return Err("Choose an expiry in the future.".into());
        }
        let mut state = self.lock()?;
        let account = state
            .accounts
            .iter()
            .find(|a| a.id == account_id)
            .ok_or("Account not found")?;
        match target {
            "managed" => {}
            "claude_cli" => {
                if account.provider != Provider::Claude || account.kind != AuthKind::OAuth {
                    return Err("Native activation requires a Claude OAuth profile.".into());
                }
                if account.external_identity.is_none() {
                    return Err("Capture or import this profile before native activation.".into());
                }
            }
            _ => return Err("Choose managed or claude_cli as the rule target.".into()),
        }
        let provider = account.provider;
        let mut candidate = state.clone();
        let created_at = candidate
            .rules
            .iter()
            .find(|r| r.path == path && r.provider == provider)
            .map(|r| r.created_at)
            .unwrap_or(time);
        candidate
            .rules
            .retain(|r| !(r.path == path && r.provider == provider));
        let rule = ProjectRule {
            path,
            provider,
            account_id: account_id.into(),
            target: target.into(),
            enabled,
            created_at,
            expires_at,
        };
        candidate.rules.push(rule.clone());
        candidate.rules.sort_by(|a, b| {
            (a.path.as_str(), a.provider.as_str()).cmp(&(b.path.as_str(), b.provider.as_str()))
        });
        let detail = if enabled { "saved" } else { "paused" };
        append_event(&mut candidate, "project_rule", Some(account_id), detail);
        self.publish(&mut state, candidate)?;
        Ok(rule)
    }
    pub fn remove_rule(&self, path: &Path, provider: Provider) -> Result<ProjectRule, String> {
        let path = path.to_str().ok_or("Project rule not found.")?;
        let mut state = self.lock()?;
        let rule = state
            .rules
            .iter()
            .find(|r| r.path == path && r.provider == provider)
            .cloned()
            .ok_or("Project rule not found.")?;
        let mut candidate = state.clone();
        candidate
            .rules
            .retain(|r| !(r.path == path && r.provider == provider));
        append_event(
            &mut candidate,
            "project_rule",
            Some(&rule.account_id),
            "removed",
        );
        self.publish(&mut state, candidate)?;
        Ok(rule)
    }
    /// Component-wise longest prefix; `/a` never matches `/ab`.
    pub fn resolve_rules(&self, path: &Path, now: i64) -> Result<Vec<RuleResolution>, String> {
        let state = self.lock()?;
        let mut output = Vec::new();
        for provider in [Provider::Claude, Provider::Codex] {
            let mut matches: Vec<&ProjectRule> = state
                .rules
                .iter()
                .filter(|r| r.provider == provider && path.starts_with(Path::new(&r.path)))
                .collect();
            matches.sort_by_key(|r| std::cmp::Reverse(Path::new(&r.path).components().count()));
            output.push(RuleResolution {
                provider,
                nearest: matches.first().map(|r| (*r).clone()),
                effective: matches
                    .iter()
                    .find(|r| r.in_force(now))
                    .map(|r| (*r).clone()),
            });
        }
        Ok(output)
    }
    /// Journals a rule that changed a route or the native account.
    pub fn record_rule_applied(&self, account_id: &str) -> Result<(), String> {
        self.record("project_rule", Some(account_id), "applied")
    }
}
