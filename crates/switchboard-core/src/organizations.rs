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
/// A request or a selection on a seat that left the group's context after it was placed.
pub const SEAT_OUTSIDE: &str = "This account belongs to another organization than this group. Move it, or change the group's organization.";

/// Saving a group with an account of another context (SB-109).
pub const SEAT_CHOSEN: &str = "An account chosen for this group belongs to another organization. Choose accounts of the group's organization, or change it.";
/// Changing a group's organization while it holds an account of another context (SB-109).
pub const SEATS_ELSEWHERE: &str =
    "This group still holds accounts that belong elsewhere. Move them out first.";

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
    /// SB-159: its organization on PassionCode.ai (Vesta's `orgId`), when it was read from there;
    /// its name and domains then follow Vesta's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vesta_id: Option<String>,
}

/// One of the person's organizations as PassionCode.ai answers it (`GET /v1/me/organizations`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VestaOrganization {
    pub id: String,
    pub name: String,
    pub domains: Vec<String>,
}

/// What one read from PassionCode.ai changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct VestaApplied {
    /// Organizations added from PassionCode.ai.
    pub added: usize,
    /// Local organizations linked to one of PassionCode.ai's (same name, or a shared domain).
    pub linked: usize,
    /// Linked organizations whose name or domains changed.
    pub updated: usize,
    /// Organizations the person no longer belongs to on PassionCode.ai: kept, unlinked.
    pub released: usize,
}

fn vesta_id_valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_graphic())
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
    /// Whether `account` may serve the group that reserves `pool` (SB-109): its attribution
    /// equals the group's organization, personal included. Always while no organization exists,
    /// and for a pool no group reserves (`default`).
    pub fn context_admits(&self, pool: &str, account: &Account) -> bool {
        if self.organizations.is_empty() {
            return true;
        }
        self.project_of_pool(pool)
            .is_none_or(|p| p.organization == self.attribution(account).organization)
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
            || o.vesta_id.as_deref().is_some_and(|v| !vesta_id_valid(v))
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
    if s.projects.iter().any(|p| {
        p.organization
            .as_ref()
            .is_some_and(|o| !ids.contains(o.as_str()))
    }) {
        return Err("Invalid organization metadata".into());
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
            vesta_id: None,
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
        // Its groups become personal (SB-109).
        for p in &mut candidate.projects {
            if p.organization.as_deref() == Some(id) {
                p.organization = None;
            }
        }
        append_event(&mut candidate, "organization", None, "removed");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(organization)
    }
    /// SB-159: brings the person's PassionCode.ai organizations in. A linked one takes Vesta's
    /// name and domains (a name or domain another organization here holds is left as it was);
    /// an unlinked one of the same name or with a shared domain is linked rather than duplicated,
    /// keeping its accounts; the rest are added. One linked here that Vesta no longer lists is
    /// kept as a local organization.
    pub fn apply_vesta(&self, read: &[VestaOrganization]) -> Result<VestaApplied, String> {
        // Names compare as `validate` compares them: trimmed, lowercased.
        let same = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let mut applied = VestaApplied::default();
        let mut seen: Vec<String> = Vec::new();
        for v in read.iter().take(MAX_ORGANIZATIONS) {
            if !vesta_id_valid(&v.id) || !name_valid(&v.name) || seen.contains(&v.id) {
                continue;
            }
            seen.push(v.id.clone());
            let mut domains: Vec<String> = Vec::new();
            for d in v.domains.iter().filter_map(|d| domain_normal(d)) {
                if !domains.contains(&d) {
                    domains.push(d);
                }
            }
            let at = candidate
                .organizations
                .iter()
                .position(|o| o.vesta_id.as_deref() == Some(v.id.as_str()))
                .or_else(|| {
                    candidate.organizations.iter().position(|o| {
                        o.vesta_id.is_none()
                            && (same(&o.name, &v.name)
                                || o.domains.iter().any(|d| domains.contains(d)))
                    })
                });
            let name_free = |list: &[Organization], own: Option<&str>, name: &str| {
                !list
                    .iter()
                    .any(|o| Some(o.id.as_str()) != own && same(&o.name, name))
            };
            let i = match at {
                Some(i) => i,
                None => {
                    // A new one needs a name nobody here holds: Vesta's, else Vesta's with the
                    // start of its id, cut to fit.
                    let base: String = v.name.trim().chars().take(68).collect();
                    let name = if name_free(&candidate.organizations, None, &v.name) {
                        v.name.trim().to_owned()
                    } else {
                        format!("{} ({})", base.trim_end(), &v.id[..v.id.len().min(8)])
                    };
                    if candidate.organizations.len() >= MAX_ORGANIZATIONS
                        || !name_valid(&name)
                        || !name_free(&candidate.organizations, None, &name)
                    {
                        continue;
                    }
                    candidate.organizations.push(Organization {
                        id: uuid::Uuid::new_v4().to_string(),
                        name,
                        domains: Vec::new(),
                        provider_organizations: Vec::new(),
                        created_at: now(),
                        vesta_id: Some(v.id.clone()),
                    });
                    applied.added += 1;
                    candidate.organizations.len() - 1
                }
            };
            let own = candidate.organizations[i].id.clone();
            let rename = at.is_some() && name_free(&candidate.organizations, Some(&own), &v.name);
            // Domains add up: one typed here stays (its accounts with it), Vesta's join it, and a
            // domain another organization here holds is left out.
            let held: Vec<String> = candidate
                .organizations
                .iter()
                .filter(|o| o.id != own)
                .flat_map(|o| o.domains.clone())
                .collect();
            let o = &mut candidate.organizations[i];
            let before = (o.name.clone(), o.domains.clone(), o.vesta_id.clone());
            if rename {
                o.name = v.name.trim().to_owned();
            }
            for d in domains {
                if !held.contains(&d) && !o.domains.contains(&d) && o.domains.len() < MAX_DOMAINS {
                    o.domains.push(d);
                }
            }
            if at.is_some() {
                if o.vesta_id.is_none() {
                    o.vesta_id = Some(v.id.clone());
                    applied.linked += 1;
                } else if (o.name.clone(), o.domains.clone(), o.vesta_id.clone()) != before {
                    applied.updated += 1;
                }
            }
        }
        for o in &mut candidate.organizations {
            if o.vesta_id.as_ref().is_some_and(|v| !seen.contains(v)) {
                o.vesta_id = None;
                applied.released += 1;
            }
        }
        if applied != VestaApplied::default() {
            append_event(&mut candidate, "organization", None, "updated");
            self.publish(&mut state, candidate)?;
            self.changed();
        }
        Ok(applied)
    }
    /// SB-159: forgets every link to PassionCode.ai; the organizations stay as local ones.
    pub fn release_vesta(&self) -> Result<(), String> {
        let mut state = self.lock()?;
        if state.organizations.iter().all(|o| o.vesta_id.is_none()) {
            return Ok(());
        }
        let mut candidate = state.clone();
        for o in &mut candidate.organizations {
            o.vesta_id = None;
        }
        append_event(&mut candidate, "organization", None, "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// Puts an organization back from a backup, unless this install holds one of the same id,
    /// name or domain. Provider organizations another one already holds are left out.
    pub fn restore_organization(&self, mut organization: Organization) -> Result<(), String> {
        organization.vesta_id = None;
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if candidate.organization(&organization.id).is_some()
            || candidate.organizations.len() >= MAX_ORGANIZATIONS
        {
            return Err("Organization already here.".into());
        }
        check_unique(&candidate, None, &organization.name, &organization.domains)?;
        let held: Vec<String> = candidate
            .organizations
            .iter()
            .flat_map(|o| o.provider_organizations.clone())
            .collect();
        organization
            .provider_organizations
            .retain(|k| !held.contains(k));
        candidate.organizations.push(organization);
        append_event(&mut candidate, "organization", None, "added");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// The person's word on one account from a backup, where this install has none.
    pub fn restore_membership(&self, account: &str, choice: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if candidate.memberships.contains_key(account)
            || !candidate.accounts.iter().any(|a| a.id == account)
            || (choice != PERSONAL && candidate.organization(choice).is_none())
        {
            return Err("Nothing to restore.".into());
        }
        candidate.memberships.insert(account.into(), choice.into());
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
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
            vesta_id: None,
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
