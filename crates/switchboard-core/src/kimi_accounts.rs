//! Kimi Code subscription accounts (SB-81, packet XA-02 A-5..A-7). Kimi's refresh tokens rotate, so
//! a Kimi credential exists in exactly one place: the account's own `KIMI_CODE_HOME`, a folder
//! Switchboard owns under its data folder (`kimi/<id>/`), written only by the official `kimi`
//! binary. The store keeps metadata only — never a token, never a copy in the vault or a backup.
use crate::{append_event, label_valid, now, uuid_valid, Snapshot, Store};
use serde::{Deserialize, Serialize};

/// The two Kimi Code regions `kimi login --region` knows.
pub const REGIONS: &[&str] = &["mainland-cn", "global"];
pub const REGION_INVALID: &str = "Region is mainland-cn (kimi.com) or global (kimi.ai).";
pub const NO_KIMI_ACCOUNT: &str =
    "Kimi Code account not found. Refresh the list and choose another.";
const MAX_PROFILE: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KimiAccount {
    pub id: String,
    pub label: String,
    /// `mainland-cn` or `global`: which Kimi service the home signs in to.
    pub region: String,
    pub added_at: i64,
    /// Kimi's nickname for the account, from `GET /me` at sign-in (never an e-mail or phone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// The membership tier (`user_level_name`, for example Allegretto).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
}

/// A profile field as shown: control characters dropped, at most 80 characters.
pub fn profile_text(text: &str) -> Option<String> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_PROFILE)
        .collect();
    let clean = clean.trim().to_owned();
    (!clean.is_empty()).then_some(clean)
}

fn profile_valid(field: &Option<String>) -> bool {
    field
        .as_deref()
        .is_none_or(|t| profile_text(t).as_deref() == Some(t))
}

pub(crate) fn validate_kimi_accounts(s: &Snapshot) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for k in &s.kimi_accounts {
        if !uuid_valid(&k.id)
            || !ids.insert(&k.id)
            || s.accounts.iter().any(|a| a.id == k.id)
            || s.agent_keys.iter().any(|a| a.id == k.id)
            || !label_valid(&k.label)
            || !REGIONS.contains(&k.region.as_str())
            || k.added_at <= 0
            || !profile_valid(&k.nickname)
            || !profile_valid(&k.tier)
        {
            return Err("Invalid Kimi Code account metadata".into());
        }
    }
    Ok(())
}

impl Store {
    /// Records a signed-in Kimi Code home under `id` (the folder `kimi/<id>/` the sign-in used).
    pub fn add_kimi_account(
        &self,
        id: &str,
        label: &str,
        region: &str,
        nickname: Option<String>,
        tier: Option<String>,
    ) -> Result<KimiAccount, String> {
        if !REGIONS.contains(&region) {
            return Err(REGION_INVALID.into());
        }
        if !label_valid(label) {
            return Err("Enter a label of up to 80 characters.".into());
        }
        let entry = KimiAccount {
            id: id.to_owned(),
            label: label.trim().to_owned(),
            region: region.to_owned(),
            added_at: now(),
            nickname: nickname.as_deref().and_then(profile_text),
            tier: tier.as_deref().and_then(profile_text),
        };
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if candidate.kimi_accounts.iter().any(|k| k.id == id) {
            return Err("This Kimi Code account is already saved.".into());
        }
        candidate.kimi_accounts.push(entry.clone());
        append_event(&mut candidate, "kimi_account", None, "added");
        crate::validate_snapshot(&candidate)?;
        self.publish(&mut state, candidate)?;
        Ok(entry)
    }

    /// Renames an account or refreshes its profile; `None` keeps a field.
    pub fn update_kimi_account(
        &self,
        id: &str,
        label: Option<&str>,
        nickname: Option<String>,
        tier: Option<String>,
    ) -> Result<KimiAccount, String> {
        if label.is_some_and(|l| !label_valid(l)) {
            return Err("Enter a label of up to 80 characters.".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let entry = candidate
            .kimi_accounts
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or(NO_KIMI_ACCOUNT)?;
        if let Some(label) = label {
            entry.label = label.trim().to_owned();
        }
        if let Some(n) = nickname.as_deref().and_then(profile_text) {
            entry.nickname = Some(n);
        }
        if let Some(t) = tier.as_deref().and_then(profile_text) {
            entry.tier = Some(t);
        }
        let entry = entry.clone();
        if state.kimi_accounts.contains(&entry) {
            return Ok(entry);
        }
        crate::validate_snapshot(&candidate)?;
        self.publish(&mut state, candidate)?;
        Ok(entry)
    }

    pub fn kimi_account(&self, id: &str) -> Result<KimiAccount, String> {
        self.snapshot()?
            .kimi_accounts
            .into_iter()
            .find(|k| k.id == id)
            .ok_or_else(|| NO_KIMI_ACCOUNT.into())
    }

    /// Forgets an account; the caller removes its home afterwards (a leftover home is unreachable
    /// and harmless, an entry without its home would not be).
    pub fn remove_kimi_account(&self, id: &str) -> Result<bool, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let Some(i) = candidate.kimi_accounts.iter().position(|k| k.id == id) else {
            return Ok(false);
        };
        candidate.kimi_accounts.remove(i);
        append_event(&mut candidate, "kimi_account", None, "removed");
        self.publish(&mut state, candidate)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_text_drops_controls_and_bounds_length() {
        assert_eq!(
            profile_text("Alle\u{7}gretto ").as_deref(),
            Some("Allegretto")
        );
        assert_eq!(profile_text("\u{1b}\n"), None);
        assert_eq!(profile_text(&"x".repeat(200)).unwrap().len(), 80);
    }
}
