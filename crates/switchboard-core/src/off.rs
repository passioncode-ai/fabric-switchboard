//! W1.2 (SB-418, DEC-0027): an account the person turned off, and since when.
//!
//! `Account::enabled` stays the one effective bit every path reads, and it is written by the
//! person and by four automatic processes (used on another machine, a delegated lease that ran
//! out, a seat called back, a copy given back). The person's word is kept apart from it, in
//! `Account::off` (`KimiAccount::off` for a Kimi Code account): only the person sets it and only
//! the person clears it (`accounts update --enabled`, `switchboard_account_update`, the window's
//! switch; `kimi off|on`, `switchboard_kimi_update` for Kimi Code). While it is set the account is
//! off (`validate_snapshot` refuses a store that says otherwise), so no automatic writer that
//! turns `enabled` back on can turn on an account the person turned off: each asks
//! [`Account::usable`] or keeps `enabled` false while `off` is set.
use crate::{Account, Snapshot};
use serde::{Deserialize, Serialize};

/// Who turned an account off. Only the person today; a later cause (an organization's
/// administrator) gets its own value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffBy {
    Person,
}

/// The person's word that an account is off, and since when (Unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnedOff {
    pub since: i64,
    pub by: OffBy,
}

impl TurnedOff {
    pub fn by_person(since: i64) -> Self {
        Self {
            since,
            by: OffBy::Person,
        }
    }
}

/// The journal's action when the person turns an account off or on (detail `person`).
pub const TURNED_OFF: &str = "account_turned_off";
pub const TURNED_ON: &str = "account_turned_on";

/// The refusal of every path that would use an account the person turned off.
pub const ACCOUNT_OFF: &str = "This account is turned off. Turn it on to use it again.";
/// The same for a Kimi Code account.
pub const KIMI_ACCOUNT_OFF: &str =
    "This Kimi Code account is turned off. Turn it on to use it again.";

impl Account {
    /// The one check every path asks before it uses an account for anything: on, and not
    /// turned off by the person. (`enabled` is false whenever `off` is set; asking both keeps a
    /// path right even if a writer forgot the rule.)
    pub fn usable(&self) -> bool {
        self.enabled && self.off.is_none()
    }
    /// Since when the person turned it off, if they did.
    pub fn off_since(&self) -> Option<i64> {
        self.off.map(|o| o.since)
    }
}

/// Why an account is not usable here, as every surface words it: `on`, `off` (the person turned
/// it off), `elsewhere` (used on another machine), `lease_ended` (a delegated seat whose lease
/// ran out), `called_back` (a delegated seat its sender took back), or `disabled` (off for
/// no recorded reason; none after the migration below).
pub fn availability(s: &Snapshot, a: &Account) -> &'static str {
    if a.off.is_some() {
        "off"
    } else if s.elsewhere.contains_key(&a.id) {
        "elsewhere"
    } else if s.delegated.get(&a.id).is_some_and(|d| d.ended_at.is_some()) {
        "lease_ended"
    } else if !a.enabled && seat_called_back(s, &a.id) {
        "called_back"
    } else if a.enabled {
        "on"
    } else {
        "disabled"
    }
}

/// W1.2 review round 2 of PR #821 (P1-A): whether account `id` is off for an automatic reason
/// besides the person's word: used on another machine, in a machine group, delegated out, or a
/// seat delegated here whose lease ran out or that its sender called back. Turning such an account
/// on clears only the person's word and leaves it where it is.
pub(crate) fn held_away(s: &Snapshot, id: &str) -> bool {
    s.elsewhere.contains_key(id)
        || s.delegated_out.contains_key(id)
        || s.accounts
            .iter()
            .any(|a| a.id == id && crate::is_machine_pool(&a.pool))
        || s.delegated.get(id).is_some_and(|d| d.ended_at.is_some())
        || seat_called_back(s, id)
}

/// A seat another machine delegated here that its sender called back: its record says so
/// (`Delegation::surrendered_at`, written since SB-418; 0.7.124 and older write none), or, for a record written before, the newest
/// journal entry about it is its call back while the journal still holds one.
fn seat_called_back(s: &Snapshot, id: &str) -> bool {
    s.delegated.get(id).is_some_and(|d| {
        d.surrendered_at.is_some()
            || s.events
                .iter()
                .rev()
                .find(|e| e.action == "delegation" && e.account_id.as_deref() == Some(id))
                .is_some_and(|e| e.detail == "surrendered")
    })
}

/// Before W1.2 the person's off and the automatic ones shared `enabled`. A store written then
/// reads every account that is off for no automatic reason the store records (used elsewhere,
/// a delegated lease that ran out, a delegated seat called back) as turned off by the person:
/// that is the safe reading, since no automatic process may turn such an account on again. Its
/// time is the account's newest `account_updated` journal entry (turning off wrote one), else
/// `now` (the journal keeps the newest 256 entries, `MAX_EVENTS`). Answers whether anything changed.
pub(crate) fn migrate(s: &mut Snapshot, now: i64) -> bool {
    let mut changed = false;
    let reasons: Vec<(String, bool)> = s
        .accounts
        .iter()
        .map(|a| {
            let automatic = s.elsewhere.contains_key(&a.id)
                || s.delegated.get(&a.id).is_some_and(|d| d.ended_at.is_some())
                || seat_called_back(s, &a.id);
            (a.id.clone(), !a.enabled && a.off.is_none() && !automatic)
        })
        .collect();
    for (id, person) in reasons {
        if !person {
            continue;
        }
        let since = s
            .events
            .iter()
            .rev()
            .find(|e| e.action == "account_updated" && e.account_id.as_deref() == Some(&id))
            .map_or(now, |e| e.at);
        if let Some(a) = s.accounts.iter_mut().find(|a| a.id == id) {
            a.off = Some(TurnedOff::by_person(since.max(1)));
            changed = true;
        }
    }
    changed
}

/// Turns account `id` off by the person's word in `s` (a turn off of an account already off
/// keeps its time), dropping its selections. Answers whether it was on before.
pub(crate) fn turn_off(s: &mut Snapshot, id: &str, since: i64) -> Result<bool, String> {
    let a = s
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or(crate::ACCOUNT_NOT_FOUND)?;
    let was_on = a.off.is_none();
    if was_on {
        a.off = Some(TurnedOff::by_person(since.max(1)));
    }
    a.enabled = false;
    s.routes.retain(|_, value| value != id);
    Ok(was_on)
}
