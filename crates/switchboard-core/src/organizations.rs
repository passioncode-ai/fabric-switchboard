//! Organizations (SB-158): which accounts belong to which organization, decided on this machine.
//!
//! An organization is a name, its e-mail domains and the provider organizations (a Claude Team,
//! a ChatGPT workspace) known to be its own. An account belongs to one by the person's word, by
//! its provider organization, or by the domain of its address, in that order; with no
//! organization defined nothing of this applies. Nothing here reads the network.
use crate::{append_event, now, uuid_valid, Account, Provider, Snapshot, Store, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};

pub const MAX_ORGANIZATIONS: usize = 32;
const MAX_DOMAINS: usize = 16;
const MAX_PROVIDER_ORGANIZATIONS: usize = 64;
/// The word a person's assignment stores when they keep an account personal.
pub const PERSONAL: &str = "personal";

pub const ORGANIZATION_NOT_FOUND: &str = "Organization not found.";
pub const ORGANIZATION_NAME_TAKEN: &str = "Another organization already has this name.";
pub const DOMAIN_TAKEN: &str = "Another organization already has this domain.";
pub const DOMAIN_INVALID: &str = "A domain is a name such as example.com.";
pub const ORGANIZATION_NAME_INVALID: &str = "Name the organization in 1 to 80 characters.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,
    /// `claude:<organization uuid>` or `codex:<workspace id>`: learned when the person assigns
    /// an account, so every other seat of that team follows it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_organizations: Vec<String>,
    pub created_at: i64,
}

/// Why an account belongs where it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Person,
    Provider,
    Domain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    /// The organization's id, or none: personal.
    pub organization: Option<String>,
    /// None when no rule answered (no organization, or nothing matched).
    pub by: Option<By>,
}

/// `example.com`: lowercase labels of letters, digits and hyphens, at least one dot.
pub fn domain_normal(domain: &str) -> Option<String> {
    let d = domain
        .trim()
        .trim_start_matches('@')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let ok = !d.is_empty()
        && d.len() <= 253
        && d.contains('.')
        && d.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    ok.then_some(d)
}

fn name_valid(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.chars().count() <= 80 && !n.chars().any(char::is_control)
}

/// The account's provider organization as stored on its organization.
pub fn provider_organization(account: &Account) -> Option<String> {
    let identity = account.external_identity.as_ref()?;
    let (prefix, id) = match account.provider {
        Provider::Claude => ("claude", identity.organization_id.as_deref()?),
        // A ChatGPT sign-in's account id is its workspace: shared by a team's seats.
        Provider::Codex => ("codex", identity.account_id.as_deref()?),
    };
    Some(format!("{prefix}:{id}"))
}

/// The domain of the account's address: its identity's e-mail, else a label that is one.
pub fn account_domain(account: &Account) -> Option<String> {
    let email = account
        .external_identity
        .as_ref()
        .and_then(|i| i.email.as_deref())
        .unwrap_or(&account.label);
    let (local, domain) = email.rsplit_once('@')?;
    if local.is_empty() || local.contains(char::is_whitespace) {
        return None;
    }
    domain_normal(domain)
}

impl Snapshot {
    /// Where the account belongs (SB-158): the person's assignment, then its provider
    /// organization, then its address's domain.
    pub fn attribution(&self, account: &Account) -> Attribution {
        if let Some(choice) = self.memberships.get(&account.id) {
            let organization = (choice != PERSONAL)
                .then(|| choice.clone())
                .filter(|id| self.organizations.iter().any(|o| &o.id == id));
            if organization.is_some() || choice == PERSONAL {
                return Attribution {
                    organization,
                    by: Some(By::Person),
                };
            }
        }
        if let Some(key) = provider_organization(account) {
            if let Some(o) = self
                .organizations
                .iter()
                .find(|o| o.provider_organizations.contains(&key))
            {
                return Attribution {
                    organization: Some(o.id.clone()),
                    by: Some(By::Provider),
                };
            }
        }
        if let Some(domain) = account_domain(account) {
            if let Some(o) = self
                .organizations
                .iter()
                .find(|o| o.domains.contains(&domain))
            {
                return Attribution {
                    organization: Some(o.id.clone()),
                    by: Some(By::Domain),
                };
            }
        }
        Attribution {
            organization: None,
            by: None,
        }
    }
    pub fn organization(&self, id: &str) -> Option<&Organization> {
        self.organizations.iter().find(|o| o.id == id)
    }
}

pub(crate) fn validate(s: &Snapshot) -> Result<(), String> {
    if s.organizations.len() > MAX_ORGANIZATIONS || s.memberships.len() > MAX_ACCOUNTS {
        return Err("Invalid metadata bounds".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    let mut domains = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    for o in &s.organizations {
        if !uuid_valid(&o.id)
            || !ids.insert(o.id.as_str())
            || !name_valid(&o.name)
            || !names.insert(o.name.trim().to_lowercase())
            || o.domains.len() > MAX_DOMAINS
            || o.provider_organizations.len() > MAX_PROVIDER_ORGANIZATIONS
            || o.created_at <= 0
            || o.domains
                .iter()
                .any(|d| domain_normal(d).as_deref() != Some(d.as_str()) || !domains.insert(d))
            || o.provider_organizations.iter().any(|k| {
                k.len() > 300
                    || !(k.starts_with("claude:") || k.starts_with("codex:"))
                    || k.chars().any(char::is_control)
                    || !keys.insert(k)
            })
        {
            return Err("Invalid organization metadata".into());
        }
    }
    for (account, choice) in &s.memberships {
        if !uuid_valid(account) || (choice != PERSONAL && !ids.contains(choice.as_str())) {
            return Err("Invalid organization metadata".into());
        }
    }
    Ok(())
}

fn domains_from(list: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for d in list {
        let d = domain_normal(d).ok_or(DOMAIN_INVALID)?;
        if !out.contains(&d) {
            out.push(d);
        }
    }
    if out.len() > MAX_DOMAINS {
        return Err("An organization has at most 16 domains.".into());
    }
    Ok(out)
}

fn check_unique(
    s: &Snapshot,
    except: Option<&str>,
    name: &str,
    domains: &[String],
) -> Result<(), String> {
    for o in s
        .organizations
        .iter()
        .filter(|o| Some(o.id.as_str()) != except)
    {
        if o.name.trim().eq_ignore_ascii_case(name.trim()) {
            return Err(ORGANIZATION_NAME_TAKEN.into());
        }
        if o.domains.iter().any(|d| domains.contains(d)) {
            return Err(DOMAIN_TAKEN.into());
        }
    }
    Ok(())
}

impl Store {
    pub fn add_organization(&self, name: &str, domains: &[String]) -> Result<Organization, String> {
        if !name_valid(name) {
            return Err(ORGANIZATION_NAME_INVALID.into());
        }
        let domains = domains_from(domains)?;
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if candidate.organizations.len() >= MAX_ORGANIZATIONS {
            return Err("At most 32 organizations.".into());
        }
        check_unique(&candidate, None, name, &domains)?;
        let organization = Organization {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
            domains,
            provider_organizations: Vec::new(),
            created_at: now(),
        };
        candidate.organizations.push(organization.clone());
        append_event(&mut candidate, "organization", None, "added");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(organization)
    }
    /// Renames it, replaces its domains, or both; what is left out stays.
    pub fn edit_organization(
        &self,
        id: &str,
        name: Option<&str>,
        domains: Option<&[String]>,
    ) -> Result<Organization, String> {
        if name.is_none() && domains.is_none() {
            return Err(crate::NOTHING_TO_UPDATE.into());
        }
        if name.is_some_and(|n| !name_valid(n)) {
            return Err(ORGANIZATION_NAME_INVALID.into());
        }
        let domains = domains.map(domains_from).transpose()?;
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let current = candidate
            .organization(id)
            .ok_or(ORGANIZATION_NOT_FOUND)?
            .clone();
        let name = name.map(|n| n.trim().to_owned()).unwrap_or(current.name);
        let domains = domains.unwrap_or(current.domains);
        check_unique(&candidate, Some(id), &name, &domains)?;
        let o = candidate
            .organizations
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(ORGANIZATION_NOT_FOUND)?;
        o.name = name;
        o.domains = domains;
        let organization = o.clone();
        append_event(&mut candidate, "organization", None, "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(organization)
    }
    /// Removes it; accounts assigned to it fall back to the other rules.
    pub fn remove_organization(&self, id: &str) -> Result<Organization, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let index = candidate
            .organizations
            .iter()
            .position(|o| o.id == id)
            .ok_or(ORGANIZATION_NOT_FOUND)?;
        let organization = candidate.organizations.remove(index);
        candidate.memberships.retain(|_, choice| choice != id);
        append_event(&mut candidate, "organization", None, "removed");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(organization)
    }
    /// The person's word on one account: an organization's id, [`PERSONAL`], or none to let the
    /// rules decide again. Assigning to an organization also records the account's provider
    /// organization there (and nowhere else), so the team's other seats follow; keeping it
    /// personal takes that provider organization off every organization.
    pub fn assign_organization(
        &self,
        account: &str,
        choice: Option<&str>,
    ) -> Result<Attribution, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let a = candidate
            .accounts
            .iter()
            .find(|a| a.id == account)
            .ok_or("Account not found")?
            .clone();
        let key = provider_organization(&a);
        match choice {
            None => {
                candidate.memberships.remove(account);
            }
            Some(PERSONAL) => {
                candidate
                    .memberships
                    .insert(account.into(), PERSONAL.into());
                if let Some(key) = &key {
                    for o in &mut candidate.organizations {
                        o.provider_organizations.retain(|k| k != key);
                    }
                }
            }
            Some(id) => {
                if candidate.organization(id).is_none() {
                    return Err(ORGANIZATION_NOT_FOUND.into());
                }
                candidate.memberships.insert(account.into(), id.into());
                if let Some(key) = &key {
                    for o in &mut candidate.organizations {
                        o.provider_organizations.retain(|k| k != key);
                        if o.id == id && o.provider_organizations.len() < MAX_PROVIDER_ORGANIZATIONS
                        {
                            o.provider_organizations.push(key.clone());
                        }
                    }
                }
            }
        }
        append_event(&mut candidate, "organization", Some(account), "assigned");
        let attribution = candidate.attribution(&a);
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(attribution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthKind, ExternalIdentity};

    fn account(id: &str, provider: Provider, email: &str, org: Option<&str>) -> Account {
        Account {
            id: id.into(),
            label: email.into(),
            provider,
            kind: AuthKind::OAuth,
            pool: "default".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: Some(ExternalIdentity {
                account_id: org.map(str::to_owned),
                organization_id: org.map(str::to_owned),
                email: Some(email.into()),
            }),
            usage_health: None,
        }
    }
    fn org(id: &str, name: &str, domains: &[&str]) -> Organization {
        Organization {
            id: id.into(),
            name: name.into(),
            domains: domains.iter().map(|d| (*d).into()).collect(),
            provider_organizations: Vec::new(),
            created_at: 1,
        }
    }
    const A: &str = "00000000-0000-4000-8000-00000000000a";
    const B: &str = "00000000-0000-4000-8000-00000000000b";
    const O1: &str = "00000000-0000-4000-8000-000000000001";
    const O2: &str = "00000000-0000-4000-8000-000000000002";

    #[test]
    fn without_organizations_every_account_is_personal_and_unexplained() {
        let s = Snapshot::default();
        let a = account(A, Provider::Claude, "me@example.com", Some("team"));
        assert_eq!(
            s.attribution(&a),
            Attribution {
                organization: None,
                by: None
            }
        );
    }

    #[test]
    fn the_persons_word_wins_then_the_provider_organization_then_the_domain() {
        let mut s = Snapshot {
            organizations: vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])],
            ..Default::default()
        };
        s.organizations[1].provider_organizations = vec!["claude:team-b".into()];
        // Domain.
        let a = account(A, Provider::Claude, "dev@acme.example", Some("team-x"));
        assert_eq!(
            s.attribution(&a),
            Attribution {
                organization: Some(O1.into()),
                by: Some(By::Domain)
            }
        );
        // Provider organization beats the domain.
        let b = account(B, Provider::Claude, "dev@acme.example", Some("team-b"));
        assert_eq!(
            s.attribution(&b),
            Attribution {
                organization: Some(O2.into()),
                by: Some(By::Provider)
            }
        );
        // The person's word beats both, "personal" included.
        s.memberships.insert(B.into(), PERSONAL.into());
        assert_eq!(
            s.attribution(&b),
            Attribution {
                organization: None,
                by: Some(By::Person)
            }
        );
        s.memberships.insert(A.into(), O2.into());
        assert_eq!(
            s.attribution(&a),
            Attribution {
                organization: Some(O2.into()),
                by: Some(By::Person)
            }
        );
    }

    #[test]
    fn a_codex_workspace_is_its_provider_organization() {
        let a = account(A, Provider::Codex, "x@gmail.com", Some("ws-1"));
        assert_eq!(provider_organization(&a).as_deref(), Some("codex:ws-1"));
        let mut c = account(B, Provider::Claude, "x@gmail.com", None);
        assert_eq!(provider_organization(&c), None);
        c.external_identity = None;
        c.label = "Work".into();
        assert_eq!(account_domain(&c), None);
    }

    #[test]
    fn domains_are_normalised_and_checked() {
        assert_eq!(
            domain_normal(" @Acme.Example. ").as_deref(),
            Some("acme.example")
        );
        for bad in [
            "",
            "acme",
            "-a.example",
            "a .example",
            "a..example",
            "a/b.example",
        ] {
            assert_eq!(domain_normal(bad), None, "{bad}");
        }
    }

    #[test]
    fn validation_refuses_shared_domains_unknown_assignments_and_duplicate_names() {
        let mut s = Snapshot {
            organizations: vec![
                org(O1, "Acme", &["acme.example"]),
                org(O2, "Beta", &["beta.example"]),
            ],
            ..Default::default()
        };
        assert!(validate(&s).is_ok());
        s.organizations[1].domains = vec!["acme.example".into()];
        assert!(validate(&s).is_err());
        s.organizations[1].domains.clear();
        s.organizations[1].name = "acme".into();
        assert!(validate(&s).is_err());
        s.organizations[1].name = "Beta".into();
        s.memberships
            .insert(A.into(), "00000000-0000-4000-8000-0000000000ff".into());
        assert!(validate(&s).is_err());
        s.memberships.insert(A.into(), PERSONAL.into());
        assert!(validate(&s).is_ok());
    }
}
