//! SB-283: handing an account over to another machine, or to the Switchboard of a colleague,
//! without moving its credential (operator 2026-11-06; design `docs/research/handover-2026-11-06.md`).
//!
//! A renewal on one machine ends the sign-in on the other (two-machines D-2), so a credential is
//! never copied. The giving machine writes a record (`switchboard-handover/1`): who the sign-in
//! must be, the group and organizations it served there and who receives it, signed with the
//! settings signing key (SB-272), and marks the account as used elsewhere with the recipient as
//! its location (SB-284). The receiving machine checks the record and starts a sign-in for that
//! address in the group of the same pool, if it has one.

use crate::settings_signing::{self, Signature};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use switchboard_core::{AuthKind, Provider, Snapshot, Store};

pub const FORMAT: &str = "switchboard-handover/1";
/// Refusal: an API key is not signed in to, so there is nothing to start on the other side.
pub const HANDOVER_SIGN_IN_ONLY: &str =
    "Only an account you sign in to can be handed over. Give an API key through your vault.";
/// Refusal: a file that is not a hand-over record.
pub const NOT_A_HANDOVER: &str =
    "This is not a Switchboard hand-over file (switchboard-handover/1).";
/// Refusal: the record's signature does not hold, or it was not signed by the key named.
pub const HANDOVER_SIGNATURE: &str =
    "This hand-over file is not signed by the key you named, or it changed after it was signed.";

/// The group the account served on the giving machine.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupRef {
    pub pool: String,
    pub name: String,
}

/// The record that travels: no credential, only who the sign-in must be and where it goes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Handover {
    pub format: String,
    pub provider: Provider,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The provider's ids of the sign-in, so the receiver can tell it is the same one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRef>,
    /// The organizations it works for there, by name (ids differ between machines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub organizations: Vec<String>,
    /// Who receives it, in the giver's words: a machine and its user, or a colleague.
    pub to: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

/// The bytes a signature covers: the record without its signature.
fn message(record: &Handover) -> Vec<u8> {
    let mut unsigned = record.clone();
    unsigned.signature = None;
    serde_json::to_vec(&unsigned).unwrap_or_default()
}

/// Writes the record for `id`, signed with this computer's settings key, and marks the account
/// as used elsewhere with `to` as where it works. Nothing else changes; its credential stays.
pub fn hand_over(
    root: &Path,
    store: &Store,
    id: &str,
    to: &str,
    now: i64,
) -> Result<Value, String> {
    let to = to.trim();
    let snapshot = store.snapshot()?;
    let account = snapshot
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    if account.kind != AuthKind::OAuth {
        return Err(HANDOVER_SIGN_IN_ONLY.into());
    }
    // The location rule (SB-284) checks `to` before anything is signed or marked.
    if to.is_empty() || to.chars().count() > 80 || to.chars().any(char::is_control) {
        return Err(switchboard_core::LOCATION_INVALID.into());
    }
    let identity = account.external_identity.clone().unwrap_or_default();
    let mut record = Handover {
        format: FORMAT.into(),
        provider: account.provider,
        label: account.label.clone(),
        email: identity.email,
        account_id: identity.account_id,
        organization_id: identity.organization_id,
        groups: groups_of(&snapshot, account),
        organizations: snapshot
            .attribution(account)
            .organizations()
            .into_iter()
            .filter_map(|o| snapshot.organization(o).map(|o| o.name.clone()))
            .collect(),
        to: to.to_owned(),
        at: now,
        signature: None,
    };
    record.signature = Some(settings_signing::sign(root, store, &message(&record))?);
    store.mark_elsewhere_at(id, true, now, Some(to))?;
    Ok(json!({"record": record, "marked_elsewhere": true, "location": to}))
}

/// The groups the account serves on this machine (SB-282), its own first.
fn groups_of(snapshot: &Snapshot, account: &switchboard_core::Account) -> Vec<GroupRef> {
    snapshot
        .pools_of(account)
        .into_iter()
        .filter_map(|pool| snapshot.project_of_pool(pool))
        .map(|p| GroupRef {
            pool: p.pool.clone(),
            name: p.name.clone(),
        })
        .collect()
}

/// Reads a record a person passed in: the record itself, or the whole answer `hand-over`
/// printed (`{"record": ...}`).
pub fn parse(value: &Value) -> Result<Handover, String> {
    let value = value.get("record").unwrap_or(value);
    let record: Handover =
        serde_json::from_value(value.clone()).map_err(|_| NOT_A_HANDOVER.to_owned())?;
    if record.format != FORMAT {
        return Err(NOT_A_HANDOVER.into());
    }
    Ok(record)
}

/// What receiving `record` here would do: whose sign-in, into which group, and whether this
/// machine already holds that sign-in. Refused when the signature does not hold (or, with
/// `signer`, is not that key's).
pub fn plan(snapshot: &Snapshot, record: &Handover, signer: Option<&str>) -> Result<Value, String> {
    let verdict = settings_signing::verify(&message(record), record.signature.as_ref(), signer);
    if settings_signing::refusal(&verdict).is_some() {
        return Err(HANDOVER_SIGNATURE.into());
    }
    let group = record
        .groups
        .iter()
        .find_map(|g| snapshot.project_of_pool(&g.pool))
        .map(|p| json!({"pool": p.pool, "name": p.name}));
    let held = snapshot.accounts.iter().any(|a| {
        a.provider == record.provider
            && a.external_identity.as_ref().is_some_and(|i| {
                i.account_id.is_some()
                    && i.account_id == record.account_id
                    && i.organization_id == record.organization_id
            })
    });
    let organizations: Vec<Value> = record
        .organizations
        .iter()
        .map(|name| {
            json!({"name": name, "here": snapshot.organizations.iter().any(|o| o.name.eq_ignore_ascii_case(name))})
        })
        .collect();
    Ok(json!({
        "provider": record.provider, "label": record.label, "email": record.email,
        "to": record.to, "at": record.at,
        "signature": verdict.state, "signer": verdict.signer,
        "group": group, "groups": record.groups, "organizations": organizations,
        "held": held,
        "pool": group.as_ref().map_or_else(|| json!("default"), |g| g["pool"].clone()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use switchboard_core::{Credential, ExternalIdentity, MemoryVault};

    fn store(root: &Path) -> Store {
        Store::open(root.to_path_buf(), Arc::new(MemoryVault::default())).unwrap()
    }
    fn seat(store: &Store) -> switchboard_core::Account {
        store
            .upsert(
                "dev@example.com".into(),
                Provider::Claude,
                AuthKind::OAuth,
                "default".into(),
                Credential::parse(
                    Provider::Claude,
                    AuthKind::OAuth,
                    r#"{"claudeAiOauth":{"accessToken":"a-h","refreshToken":"h"}}"#,
                )
                .unwrap(),
                Some(ExternalIdentity {
                    account_id: Some("synthetic-account".into()),
                    organization_id: Some("synthetic-org".into()),
                    email: Some("dev@example.com".into()),
                }),
            )
            .unwrap()
    }

    #[test]
    fn a_handed_over_account_travels_as_a_signed_record_without_its_credential() {
        let giver_root = tempfile::tempdir().unwrap();
        let giver = store(giver_root.path());
        let account = seat(&giver);
        let folder = giver_root.path().join("app");
        std::fs::create_dir_all(&folder).unwrap();
        let group = giver
            .save_project(None, "App", &[folder], std::slice::from_ref(&account.id))
            .unwrap();
        let out = hand_over(
            giver_root.path(),
            &giver,
            &account.id,
            "Anna's MacBook",
            1_000,
        )
        .unwrap();
        let text = out["record"].to_string();
        assert!(
            !text.contains("a-h") && !text.contains("refreshToken"),
            "{text}"
        );
        assert_eq!(out["record"]["groups"][0]["pool"], group.pool.as_str());
        // The giving machine no longer uses it, and says where it went.
        let snap = giver.snapshot().unwrap();
        assert!(snap.elsewhere.contains_key(&account.id));
        assert_eq!(
            snap.locations.get(&account.id).map(String::as_str),
            Some("Anna's MacBook")
        );
        // The receiver with that group sees it signed and lands it there.
        let key = settings_signing::signer(giver_root.path(), &giver).unwrap()["public_key"]
            .as_str()
            .unwrap()
            .to_owned();
        let record = parse(&out["record"]).unwrap();
        let receiver_root = tempfile::tempdir().unwrap();
        let receiver = store(receiver_root.path());
        let theirs = receiver_root.path().join("app");
        std::fs::create_dir_all(&theirs).unwrap();
        receiver.save_project(None, "App", &[theirs], &[]).unwrap();
        let plan = plan(&receiver.snapshot().unwrap(), &record, Some(&key)).unwrap();
        assert_eq!(plan["signature"], "signed");
        assert_eq!(plan["pool"], group.pool.as_str());
        assert_eq!(plan["held"], false);
        // Another key, or a changed record: refused.
        assert_eq!(
            super::plan(
                &receiver.snapshot().unwrap(),
                &record,
                Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
            )
            .unwrap_err(),
            HANDOVER_SIGNATURE
        );
        let mut changed = record.clone();
        changed.to = "Someone else".into();
        assert_eq!(
            super::plan(&receiver.snapshot().unwrap(), &changed, None).unwrap_err(),
            HANDOVER_SIGNATURE
        );
        // Not a record at all.
        assert_eq!(
            parse(&json!({"format": "other"})).unwrap_err(),
            NOT_A_HANDOVER
        );
    }

    #[test]
    fn an_api_key_is_not_handed_over() {
        let root = tempfile::tempdir().unwrap();
        let s = store(root.path());
        let key = s
            .add(
                "Key".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "default".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "sk-ant-api03-synthetic")
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            hand_over(root.path(), &s, &key.id, "Anna", 1).unwrap_err(),
            HANDOVER_SIGN_IN_ONLY
        );
        assert!(s.snapshot().unwrap().elsewhere.is_empty());
    }
}
