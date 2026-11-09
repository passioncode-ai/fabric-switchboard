//! Organizations (SB-158): which accounts belong to which organization, decided on this machine.
//!
//! An organization is a name, its e-mail domains and the provider organizations (a Claude Team,
//! a ChatGPT workspace) known to be its own. An account belongs to one by the person's word or by
//! its provider organization, in that order; with no organization defined nothing of this
//! applies. SB-285 (operator 2026-11-06): the domain of an account's address only suggests an
//! organization, which the person confirms by placing the account there (its *Organization…*
//! choice, or putting it in a group of that organization); one organization may use several
//! domains. Nothing here reads the network.
use crate::{append_event, now, uuid_valid, Account, Provider, Snapshot, Store, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};

pub const MAX_ORGANIZATIONS: usize = 32;
const MAX_DOMAINS: usize = 16;
const MAX_PROVIDER_ORGANIZATIONS: usize = 64;
/// The word a person's assignment stores when they keep an account personal.
/// SB-282: in a list of an account's contexts, the first one left to the rules.
pub const AUTO: &str = "auto";
pub const PERSONAL: &str = "personal";

pub const ORGANIZATION_NOT_FOUND: &str = "Organization not found.";
pub const ORGANIZATION_NAME_TAKEN: &str = "Another organization already has this name.";
pub const DOMAIN_TAKEN: &str = "Another organization already has this domain.";
pub const DOMAIN_INVALID: &str = "A domain is a name such as example.com.";
pub const ORGANIZATION_NAME_INVALID: &str = "Name the organization in 1 to 80 characters.";
/// A request or a selection on a seat that left the group's context after it was placed.
pub const SEAT_OUTSIDE: &str = "This account belongs to another organization than this group. Move it, or change the group's organization.";

/// SB-313: an organization chosen for an account (and its provider team) that a group it serves
/// would then refuse.
pub const ASSIGN_OUTSIDE: &str = "A group this account or its provider team serves would then refuse it. Move it out of that group first, or change the group's organization.";
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

/// Why an account belongs where it does. SB-285: an address's domain is no reason any more; it
/// only suggests ([`Snapshot::domain_suggestion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Person,
    Provider,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    /// The organization's id, or none: personal.
    pub organization: Option<String>,
    /// None when no rule answered (no organization, or nothing matched).
    pub by: Option<By>,
    /// SB-282: the organizations the person said the account also works for, beyond
    /// `organization` (the first context, personal included), in the person's order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
}
impl Attribution {
    /// Whether the account holds `context` (none: personal): its first context or one of
    /// `also` (SB-282).
    pub fn holds(&self, context: Option<&str>) -> bool {
        match context {
            None => self.organization.is_none(),
            Some(o) => self.organization.as_deref() == Some(o) || self.also.iter().any(|x| x == o),
        }
    }
    /// The account's contexts in order: the first (none: personal), then `also`.
    pub fn contexts(&self) -> Vec<Option<&str>> {
        std::iter::once(self.organization.as_deref())
            .chain(self.also.iter().map(|o| Some(o.as_str())))
            .collect()
    }
    /// Every organization the account belongs to, its first one first.
    pub fn organizations(&self) -> Vec<&str> {
        self.contexts().into_iter().flatten().collect()
    }
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
    /// organization; else personal. SB-285: its address's domain decides nothing.
    pub fn attribution(&self, account: &Account) -> Attribution {
        let mut attribution = self.first_attribution(account);
        if let Some(list) = self.also_organizations.get(&account.id) {
            for o in list {
                if attribution.organization.as_deref() != Some(o.as_str())
                    && !attribution.also.contains(o)
                    && self.organizations.iter().any(|x| &x.id == o)
                {
                    attribution.also.push(o.clone());
                }
            }
        }
        attribution
    }
    fn first_attribution(&self, account: &Account) -> Attribution {
        if let Some(choice) = self.memberships.get(&account.id) {
            let organization = (choice != PERSONAL)
                .then(|| choice.clone())
                .filter(|id| self.organizations.iter().any(|o| &o.id == id));
            if organization.is_some() || choice == PERSONAL {
                return Attribution {
                    organization,
                    by: Some(By::Person),
                    also: Vec::new(),
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
                    also: Vec::new(),
                };
            }
        }
        Attribution {
            organization: None,
            by: None,
            also: Vec::new(),
        }
    }
    /// SB-285: the organization one of whose domains holds the account's address. A suggestion
    /// only: it places nothing by itself.
    pub fn domain_organization(&self, account: &Account) -> Option<&Organization> {
        let domain = account_domain(account)?;
        self.organizations
            .iter()
            .find(|o| o.domains.contains(&domain))
    }
    /// SB-285: the organization the account's address suggests, for the person to confirm. None
    /// when no organization holds its domain, when the person already said where it belongs
    /// (personal included), or when it already holds that organization.
    pub fn domain_suggestion(&self, account: &Account) -> Option<&Organization> {
        let organization = self.domain_organization(account)?;
        let attribution = self.attribution(account);
        (attribution.by != Some(By::Person) && !attribution.holds(Some(&organization.id)))
            .then_some(organization)
    }
    /// SB-285: the organization the person confirms by placing `account` in a group of
    /// `organizations`: the one its address suggests, when nothing else decides its context (no
    /// word from the person, no provider team) and the group would not take it otherwise.
    fn confirmed_by_placement(
        &self,
        account: &Account,
        organizations: &[String],
    ) -> Option<String> {
        let attribution = self.attribution(account);
        if attribution.by.is_some() || organizations.iter().any(|o| attribution.holds(Some(o))) {
            return None;
        }
        let organization = self.domain_organization(account)?;
        organizations
            .contains(&organization.id)
            .then(|| organization.id.clone())
    }
    /// SB-109, SB-285: whether a group of `organizations` (none: personal) takes `account` when
    /// the person places it there: it fits already, or placing it confirms what its address
    /// suggests. Always while no organization exists.
    pub fn placement_admits(&self, organizations: &[String], account: &Account) -> bool {
        if self.organizations.is_empty() {
            return true;
        }
        let group = crate::Project {
            organizations: organizations.to_vec(),
            ..Default::default()
        };
        group.admits(&self.attribution(account))
            || self
                .confirmed_by_placement(account, organizations)
                .is_some()
    }
    /// SB-285: records, as the person's word, the organization placing `account_id` in the group
    /// of `pool`, bound to `organizations`, confirms (see [`Snapshot::placement_admits`]): as its
    /// organization, or, when another group it serves would then refuse it (a personal group it
    /// stays at home in while a group of the organization shares it), as one it also works for
    /// (SB-282). True when it recorded one.
    pub(crate) fn confirm_placement(
        &mut self,
        account_id: &str,
        pool: &str,
        organizations: &[String],
    ) -> bool {
        let Some(account) = self.accounts.iter().find(|a| a.id == account_id) else {
            return false;
        };
        let Some(organization) = self.confirmed_by_placement(account, organizations) else {
            return false;
        };
        let current = self.attribution(account);
        let first = Attribution {
            organization: Some(organization.clone()),
            by: Some(By::Person),
            also: current.also,
        };
        let others_agree = self
            .projects
            .iter()
            .filter(|p| p.pool != pool && self.serves(account, &p.pool))
            .all(|p| p.admits(&first));
        if others_agree {
            self.memberships.insert(account_id.into(), organization);
        } else {
            let also = self
                .also_organizations
                .entry(account_id.into())
                .or_default();
            if !also.contains(&organization) {
                also.push(organization);
            }
        }
        append_event(self, "organization", Some(account_id), "assigned");
        true
    }
    /// SB-285: the placements the retired domain rule made, `(account, organization, also)`: an
    /// account with no word from the person and no provider team, whose address is on an
    /// organization's domain. Where every group it serves takes it as that organization's, it
    /// becomes its own (`also` false). Where a group of another context also holds it (the domain
    /// arrived after the person placed it there, as on 2026-11-08), that group's word stands, and
    /// the organization is one it also works for only when one of its groups needs it to (a seat
    /// shared into the organization's group, SB-282), as [`Snapshot::confirm_placement`] does.
    pub(crate) fn domain_placements(&self) -> Vec<(String, String, bool)> {
        self.accounts
            .iter()
            .filter(|a| !self.memberships.contains_key(&a.id))
            .filter_map(|a| {
                let attribution = self.attribution(a);
                if attribution.by.is_some() {
                    return None;
                }
                let organization = self.domain_organization(a)?;
                let placed = Attribution {
                    organization: Some(organization.id.clone()),
                    by: Some(By::Person),
                    also: attribution.also.clone(),
                };
                let mut serving = self.projects.iter().filter(|p| self.serves(a, &p.pool));
                if serving.clone().all(|p| p.admits(&placed)) {
                    return Some((a.id.clone(), organization.id.clone(), false));
                }
                serving
                    .any(|p| !p.admits(&attribution) && p.admits(&placed))
                    .then(|| (a.id.clone(), organization.id.clone(), true))
            })
            .collect()
    }
    /// SB-285: writes one placement [`Snapshot::domain_placements`] found.
    fn apply_domain_placement(&mut self, account: &str, organization: &str, also: bool) {
        if also {
            let list = self.also_organizations.entry(account.into()).or_default();
            if !list.iter().any(|o| o == organization) {
                list.push(organization.into());
            }
        } else {
            self.memberships.insert(account.into(), organization.into());
        }
        append_event(self, "organization", Some(account), "assigned");
    }
    /// SB-285: once per store, the move off the domain rule: every placement it made that the
    /// account's groups agree with ([`Snapshot::domain_placements`]) becomes the person's word,
    /// so nothing running changes, and the store is marked so a later open never freezes a
    /// placement this version did not make. A store with no organization needs nothing. True
    /// when it changed the snapshot.
    pub(crate) fn retire_domain_rule(&mut self) -> bool {
        if self.domain_rule_retired || self.organizations.is_empty() {
            return false;
        }
        for (account, organization, also) in self.domain_placements() {
            self.apply_domain_placement(&account, &organization, also);
        }
        self.domain_rule_retired = true;
        true
    }
    pub fn organization(&self, id: &str) -> Option<&Organization> {
        self.organizations.iter().find(|o| o.id == id)
    }
    /// SB-110: the context a session of `account` in `folder` runs in, in the contract's form
    /// (`personal`, `org:<id>`; fabric-agent-contract `contexts.md`): `context_for` with the
    /// account's own organization and its pool.
    pub fn session_context(&self, account: &Account, folder: Option<&std::path::Path>) -> String {
        let attribution = self.attribution(account);
        self.context_of(&attribution.contexts(), Some(&account.pool), folder)
    }
    /// SB-282: the context of a session of `account` serving `pool` (it may serve several
    /// groups), in `folder`.
    pub fn serving_context(
        &self,
        account: &Account,
        pool: &str,
        folder: Option<&std::path::Path>,
    ) -> String {
        let attribution = self.attribution(account);
        self.context_of(&attribution.contexts(), Some(pool), folder)
    }
    /// SB-110, SB-201: the one context rule for any session. `own` is the organization of
    /// whatever pays for it (an account, a key; none is personal), `pool` the pool it serves,
    /// `folder` where it works. The group of the pool decides, else the group holding the folder,
    /// else `own` (a folder in no group is a session without a project, CX-26); when a group and
    /// `own` disagree, the more private wins. `personal` while no organization exists. A group
    /// of several organizations (SB-282) runs in the one `own` names when it is among them.
    pub fn context_for(
        &self,
        own: Option<&str>,
        pool: Option<&str>,
        folder: Option<&std::path::Path>,
    ) -> String {
        self.context_of(&[own], pool, folder)
    }
    /// [`Snapshot::context_for`] with every context the payer holds, its first first (SB-282):
    /// the group's first context the payer also holds, else personal; with no group, the
    /// payer's first.
    pub fn context_of(
        &self,
        own: &[Option<&str>],
        pool: Option<&str>,
        folder: Option<&std::path::Path>,
    ) -> String {
        if self.organizations.is_empty() {
            return PERSONAL.into();
        }
        let group = pool
            .and_then(|p| self.project_of_pool(p))
            .or_else(|| folder.and_then(|f| self.project_for(f)));
        let context = match group {
            Some(group) => group
                .contexts()
                .into_iter()
                .find(|c| own.contains(c))
                .flatten(),
            None => own.first().copied().flatten(),
        };
        context.map_or_else(|| PERSONAL.into(), |id| format!("org:{id}"))
    }
    /// SB-201: the context of a session that runs on no account, on a saved key of `key_pool`
    /// (a group's own key; none: the machine's key, or no key at all as Kimi Code): the key's
    /// group's organization is its own (SB-282: its first, of several). The machine's key
    /// belongs to no organization, so in an organization's folder it reads personal.
    pub fn key_context(&self, key_pool: Option<&str>, folder: Option<&std::path::Path>) -> String {
        let own = key_pool
            .and_then(|p| self.project_of_pool(p))
            .and_then(|p| p.primary_organization());
        self.context_for(own, key_pool, folder)
    }
    /// Whether `account` may serve the group that reserves `pool` (SB-109): its attribution is
    /// one of the group's organizations (SB-282), or personal for a personal group. Always while
    /// no organization exists, and for a pool no group reserves (`default`).
    pub fn context_admits(&self, pool: &str, account: &Account) -> bool {
        if self.organizations.is_empty() {
            return true;
        }
        self.project_of_pool(pool)
            .is_none_or(|p| p.admits(&self.attribution(account)))
    }
    /// SB-202, SB-282: the groups bound to `organization`, in the store's order (by name); an
    /// organization may have several, and a group may be shared with other organizations.
    pub fn organization_groups(&self, organization: &str) -> Vec<&crate::Project> {
        self.projects
            .iter()
            .filter(|p| p.bound_to(organization))
            .collect()
    }
    /// SB-202: the one group bound to `organization`; none when it has no group or several.
    /// What offers a single destination (moving a new seat there) asks; the rest read
    /// [`Snapshot::organization_groups`].
    pub fn organization_group(&self, organization: &str) -> Option<&crate::Project> {
        match self.organization_groups(organization).as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }
    /// SB-202, SB-282: the accounts of `organization` in any of its groups, whatever their
    /// state; empty while it has no group.
    pub fn organization_seats(&self, organization: &str) -> Vec<&Account> {
        let groups = self.organization_groups(organization);
        self.accounts
            .iter()
            .filter(|a| {
                groups.iter().any(|g| self.serves(a, &g.pool))
                    && self.attribution(a).holds(Some(organization))
            })
            .collect()
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
    if s.projects
        .iter()
        .any(|p| p.organizations.iter().any(|o| !ids.contains(o.as_str())))
    {
        return Err("Invalid organization metadata".into());
    }
    for (account, choice) in &s.memberships {
        if !uuid_valid(account) || (choice != PERSONAL && !ids.contains(choice.as_str())) {
            return Err("Invalid organization metadata".into());
        }
    }
    if s.also_organizations.len() > MAX_ACCOUNTS {
        return Err("Invalid metadata bounds".into());
    }
    for (account, list) in &s.also_organizations {
        if !uuid_valid(account)
            || list.is_empty()
            || list.len() > MAX_ORGANIZATIONS
            || list
                .iter()
                .enumerate()
                .any(|(i, o)| !ids.contains(o.as_str()) || list[..i].contains(o))
        {
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
        for list in candidate.also_organizations.values_mut() {
            list.retain(|o| o != id);
        }
        candidate
            .also_organizations
            .retain(|_, list| !list.is_empty());
        // Its groups leave it; a group it was the only organization of becomes personal (SB-109).
        for p in &mut candidate.projects {
            p.organizations.retain(|o| o != id);
        }
        append_event(&mut candidate, "organization", None, "removed");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(organization)
    }
    /// SB-159: brings the person's PassionCode.ai organizations in. A linked one takes Vesta's
    /// name and domains (a name or domain another organization here holds is left as it was);
    /// an unlinked one of the same name or with a shared domain is linked rather than duplicated,
    /// keeping its accounts; the rest are added. One linked here that a well-formed answer no
    /// longer lists is kept as a local organization (SB-227: an empty list, or a row whose id
    /// cannot be read, releases nothing).
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
        // SB-227: only a well-formed list proves the person left: one that names something and
        // whose every row has an id that can be read. An organization is released only when no
        // row of it names its link, whether or not that row was taken above (a name too long,
        // a row past the 32nd).
        let well_formed = !read.is_empty() && read.iter().all(|v| vesta_id_valid(&v.id));
        if well_formed {
            for o in &mut candidate.organizations {
                if o.vesta_id
                    .as_ref()
                    .is_some_and(|v| !read.iter().any(|r| &r.id == v))
                {
                    o.vesta_id = None;
                    applied.released += 1;
                }
            }
        }
        if applied != VestaApplied::default() {
            append_event(&mut candidate, "organization", None, "updated");
            self.publish(&mut state, candidate)?;
            self.changed();
        }
        Ok(applied)
    }
    /// SB-285: a backup taken before the domain rule was retired carries no word for the
    /// accounts the domain placed. Restoring one records those placements for the accounts it
    /// added (`ids`), as the move off the rule did for this install
    /// ([`Snapshot::domain_placements`]). Answers how many it recorded.
    pub fn freeze_domain_placements(
        &self,
        ids: &std::collections::BTreeSet<String>,
    ) -> Result<usize, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let placements: Vec<(String, String, bool)> = candidate
            .domain_placements()
            .into_iter()
            .filter(|(account, _, _)| ids.contains(account))
            .collect();
        if placements.is_empty() {
            return Ok(0);
        }
        for (account, organization, also) in &placements {
            candidate.apply_domain_placement(account, organization, *also);
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(placements.len())
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
    /// SB-285: a restore saves its accounts before it puts back the person's word on them, and a
    /// save into an organization's group confirms the address's suggestion (`SaveRequest::plan`).
    /// For the accounts the restore itself added, that confirmation is not the person's: it is
    /// dropped here so the backup's word, or the move off the domain rule for an older backup,
    /// decides. True when something was dropped.
    pub fn forget_restored_placements(
        &self,
        ids: &std::collections::BTreeSet<String>,
    ) -> Result<bool, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let before = (
            candidate.memberships.len(),
            candidate.also_organizations.len(),
        );
        candidate.memberships.retain(|k, _| !ids.contains(k));
        candidate.also_organizations.retain(|k, _| !ids.contains(k));
        if before
            == (
                candidate.memberships.len(),
                candidate.also_organizations.len(),
            )
        {
            return Ok(false);
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(true)
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
    /// SB-282: the organizations an account also works for, from a backup, where this install
    /// has none for it.
    pub fn restore_also_organizations(&self, account: &str, list: &[String]) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let mut kept: Vec<String> = Vec::new();
        for o in list {
            if candidate.organization(o).is_some() && !kept.contains(o) {
                kept.push(o.clone());
            }
        }
        if kept.is_empty()
            || candidate.also_organizations.contains_key(account)
            || !candidate.accounts.iter().any(|a| a.id == account)
        {
            return Err("Nothing to restore.".into());
        }
        candidate.also_organizations.insert(account.into(), kept);
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// SB-270: provider organizations (`claude:<uuid>`, `codex:<workspace>`) a settings file
    /// carried for this organization, so the team's seats signed in here follow it. One another
    /// organization already holds stays with that one; the rest are added up to the limit.
    /// Answers how many were added.
    pub fn learn_provider_organizations(&self, id: &str, keys: &[String]) -> Result<usize, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if candidate.organization(id).is_none() {
            return Err(ORGANIZATION_NOT_FOUND.into());
        }
        let held: Vec<String> = candidate
            .organizations
            .iter()
            .flat_map(|o| o.provider_organizations.clone())
            .collect();
        let mut added = 0;
        if let Some(o) = candidate.organizations.iter_mut().find(|o| o.id == id) {
            for key in keys {
                let valid = key.len() <= 300
                    && (key.starts_with("claude:") || key.starts_with("codex:"))
                    && !key.chars().any(char::is_control);
                if valid
                    && !held.contains(key)
                    && !o.provider_organizations.contains(key)
                    && o.provider_organizations.len() < MAX_PROVIDER_ORGANIZATIONS
                {
                    o.provider_organizations.push(key.clone());
                    added += 1;
                }
            }
        }
        if added == 0 {
            return Ok(0);
        }
        append_event(&mut candidate, "organization", None, "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(added)
    }
    /// The person's word on one account: an organization's id, [`PERSONAL`], or none to let the
    /// rules decide again. Assigning to an organization also records the account's provider
    /// organization there (and nowhere else), so the team's other seats follow; keeping it
    /// personal takes that provider organization off every organization. The organizations it
    /// also works for (SB-282) are kept; none drops them too.
    pub fn assign_organization(
        &self,
        account: &str,
        choice: Option<&str>,
    ) -> Result<Attribution, String> {
        let also = match choice {
            None => Some(Vec::new()),
            Some(_) => None,
        };
        self.assign(account, choice, also)
    }
    /// SB-282: the person's word on every context of one account, in order: the first as
    /// [`Store::assign_organization`] takes it (an organization, [`PERSONAL`], or `auto` to let
    /// the rules decide it), the rest organizations it also works for. An empty list lets the
    /// rules decide again and drops the rest.
    pub fn assign_organizations(
        &self,
        account: &str,
        choices: &[String],
    ) -> Result<Attribution, String> {
        let Some((first, rest)) = choices.split_first() else {
            return self.assign(account, None, Some(Vec::new()));
        };
        let first = first.trim();
        let mut also: Vec<String> = Vec::new();
        for o in rest.iter().map(|o| o.trim()) {
            if o.is_empty() || o == PERSONAL || o == first || also.iter().any(|x| x == o) {
                continue;
            }
            also.push(o.to_owned());
        }
        let first = (first != AUTO).then_some(first);
        self.assign(account, first, Some(also))
    }
    fn assign(
        &self,
        account: &str,
        choice: Option<&str>,
        also: Option<Vec<String>>,
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
        if let Some(also) = also {
            if also.iter().any(|o| candidate.organization(o).is_none()) {
                return Err(ORGANIZATION_NOT_FOUND.into());
            }
            if also.is_empty() {
                candidate.also_organizations.remove(account);
            } else {
                candidate.also_organizations.insert(account.into(), also);
            }
        }
        // SB-313: placing an account moves its provider team with it, and nothing checked the
        // groups they serve: an account a group admitted before and refuses now was left there,
        // every request in that group's folders refused. Such a choice is refused instead; an
        // account a group refused already is not counted (this choice may be its fix).
        let serving = |s: &crate::Snapshot, x: &crate::Account| -> Vec<Vec<String>> {
            s.projects
                .iter()
                .filter(|p| p.pool == x.pool || p.shared_accounts.contains(&x.id))
                .map(|p| p.organizations.clone())
                .collect()
        };
        for x in &candidate.accounts {
            let Some(before) = state.accounts.iter().find(|b| b.id == x.id) else {
                continue;
            };
            if serving(&candidate, x).iter().any(|organizations| {
                state.placement_admits(organizations, before)
                    && !candidate.placement_admits(organizations, x)
            }) {
                return Err(ASSIGN_OUTSIDE.into());
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

    /// SB-110 H-1: a session's context, as the design note's rule says.
    #[test]
    fn a_sessions_context_follows_its_group_else_its_account_and_the_more_private_wins() {
        use crate::Project;
        let group = |pool: &str, folder: &str, organization: Option<&str>| Project {
            shared_accounts: Vec::new(),
            pool: pool.into(),
            name: pool.into(),
            folders: vec![folder.into()],
            created_at: 1,
            sources: Vec::new(),
            organizations: organization.map(str::to_owned).into_iter().collect(),
        };
        let mut s = Snapshot::default();
        let mut acme = account(A, Provider::Claude, "dev@acme.example", None);
        assert_eq!(s.session_context(&acme, None), "personal");
        s.organizations = vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])];
        s.projects = vec![
            group("acme-pool", "/work/acme", Some(O1)),
            group("own-pool", "/work/own", None),
            group("beta-pool", "/work/beta", Some(O2)),
        ];
        // SB-285: its domain only suggests Acme; the person placed it there.
        assert_eq!(s.session_context(&acme, None), "personal");
        s.memberships.insert(A.into(), O1.into());
        let org1 = format!("org:{O1}");
        // No group: the account's own organization, also with no folder (CX-26).
        assert_eq!(s.session_context(&acme, None), org1);
        assert_eq!(s.session_context(&acme, Some("/elsewhere".as_ref())), org1);
        // The folder's group agrees.
        assert_eq!(
            s.session_context(&acme, Some("/work/acme/app".as_ref())),
            org1
        );
        // The folder's group is personal or another organization's: the more private wins.
        assert_eq!(
            s.session_context(&acme, Some("/work/own".as_ref())),
            "personal"
        );
        assert_eq!(
            s.session_context(&acme, Some("/work/beta".as_ref())),
            "personal"
        );
        // The account's pool belongs to a group: that group decides, wherever the folder is.
        acme.pool = "acme-pool".into();
        assert_eq!(s.session_context(&acme, Some("/elsewhere".as_ref())), org1);
        // A personal account is personal everywhere.
        let me = account(B, Provider::Claude, "me@example.com", None);
        assert_eq!(
            s.session_context(&me, Some("/work/acme".as_ref())),
            "personal"
        );
    }

    /// SB-201: a session that runs on no account (Kimi Code, an agent on a saved OpenRouter key)
    /// follows the same rule with its key's group as its own: the group's key gives the group's
    /// context, the machine's key or no key gives the folder's group only when that agrees, so
    /// an organization's folder on a personal key reads personal.
    #[test]
    fn a_session_without_an_account_takes_its_keys_group_else_personal() {
        use crate::Project;
        let group = |pool: &str, folder: &str, organization: Option<&str>| Project {
            shared_accounts: Vec::new(),
            pool: pool.into(),
            name: pool.into(),
            folders: vec![folder.into()],
            created_at: 1,
            sources: Vec::new(),
            organizations: organization.map(str::to_owned).into_iter().collect(),
        };
        let mut s = Snapshot::default();
        let acme = Some("/work/acme".as_ref());
        assert_eq!(s.context_for(None, None, acme), "personal");
        assert_eq!(s.key_context(Some("acme-pool"), acme), "personal");
        s.organizations = vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])];
        s.projects = vec![
            group("acme-pool", "/work/acme", Some(O1)),
            group("own-pool", "/work/own", None),
        ];
        let org1 = format!("org:{O1}");
        // The group's own key: that group's context, wherever the folder is.
        assert_eq!(s.key_context(Some("acme-pool"), acme), org1);
        assert_eq!(s.key_context(Some("acme-pool"), None), org1);
        assert_eq!(s.key_context(Some("own-pool"), acme), "personal");
        // The machine's key, or no key at all (Kimi Code): personal, also in the
        // organization's folder (the more private wins, A-Q6).
        assert_eq!(s.key_context(None, acme), "personal");
        assert_eq!(s.key_context(None, Some("/work/own".as_ref())), "personal");
        assert_eq!(s.key_context(None, None), "personal");
        assert_eq!(s.context_for(None, None, acme), "personal");
        // A pool no group reserves decides nothing: the folder's group does.
        assert_eq!(s.context_for(Some(O1), Some("default"), acme), org1);
        assert_eq!(s.context_for(Some(O1), None, None), org1);
        assert_eq!(s.context_for(Some(O2), None, acme), "personal");
    }

    /// SB-202: the one group bound to an organization, and its seats there.
    #[test]
    fn the_only_group_of_an_organization_is_found_and_two_are_not() {
        use crate::Project;
        let group = |pool: &str, folder: &str, organization: Option<&str>| Project {
            shared_accounts: Vec::new(),
            pool: pool.into(),
            name: pool.into(),
            folders: vec![folder.into()],
            created_at: 1,
            sources: Vec::new(),
            organizations: organization.map(str::to_owned).into_iter().collect(),
        };
        let mut s = Snapshot {
            organizations: vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])],
            ..Default::default()
        };
        assert!(s.organization_group(O1).is_none());
        assert!(s.organization_seats(O1).is_empty());
        s.projects = vec![
            group("acme-pool", "/work/acme", Some(O1)),
            group("own-pool", "/work/own", None),
        ];
        let mut seat = account(A, Provider::Claude, "dev@acme.example", None);
        seat.pool = "acme-pool".into();
        let mut elsewhere = account(B, Provider::Codex, "ops@acme.example", None);
        elsewhere.pool = "own-pool".into();
        let mut personal = account(
            "00000000-0000-4000-8000-00000000000c",
            Provider::Claude,
            "me@example.com",
            None,
        );
        personal.pool = "acme-pool".into();
        s.accounts = vec![seat, elsewhere, personal];
        // SB-285: the person placed Acme's seats (their domain only suggests it).
        for id in [A, B, "00000000-0000-4000-8000-00000000000d"] {
            s.memberships.insert(id.into(), O1.into());
        }
        assert_eq!(
            s.organization_group(O1).map(|p| p.pool.as_str()),
            Some("acme-pool")
        );
        // Its seats in that group only: not one of its accounts in another group, not a
        // personal account the group still holds.
        let seats: Vec<&str> = s
            .organization_seats(O1)
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(seats, vec![A]);
        // No group, an unknown organization: none.
        assert!(s.organization_group(O2).is_none());
        assert!(s.organization_seats(O2).is_empty());
        assert!(s.organization_group("unknown").is_none());
        // Two groups bound to one organization (SB-282): no single group to offer, and its
        // seats are counted in both.
        s.projects
            .push(group("acme-two", "/work/acme-two", Some(O1)));
        let mut second = account(
            "00000000-0000-4000-8000-00000000000d",
            Provider::Claude,
            "qa@acme.example",
            None,
        );
        second.pool = "acme-two".into();
        s.accounts.push(second);
        assert!(s.organization_group(O1).is_none());
        let groups: Vec<&str> = s
            .organization_groups(O1)
            .iter()
            .map(|p| p.pool.as_str())
            .collect();
        assert_eq!(groups, vec!["acme-pool", "acme-two"]);
        let seats: Vec<&str> = s
            .organization_seats(O1)
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(seats, vec![A, "00000000-0000-4000-8000-00000000000d"]);
    }

    /// SB-282: a group shared by two organizations takes the accounts of both, runs a session
    /// in the context of the account that serves it, and its own key in its first organization.
    #[test]
    fn a_group_of_several_organizations_admits_each_and_runs_in_the_serving_accounts_context() {
        use crate::Project;
        let mut s = Snapshot {
            organizations: vec![
                org(O1, "Acme", &["acme.example"]),
                org(O2, "Beta", &["beta.example"]),
                org(
                    "00000000-0000-4000-8000-000000000003",
                    "Gamma",
                    &["gamma.example"],
                ),
            ],
            projects: vec![Project {
                shared_accounts: Vec::new(),
                pool: "shared".into(),
                name: "shared".into(),
                folders: vec!["/work/shared".into()],
                created_at: 1,
                sources: Vec::new(),
                organizations: vec![O2.into(), O1.into()],
            }],
            ..Default::default()
        };
        let mut acme = account(A, Provider::Claude, "dev@acme.example", None);
        acme.pool = "shared".into();
        let mut beta = account(B, Provider::Claude, "dev@beta.example", None);
        beta.pool = "shared".into();
        let gamma = account(
            "00000000-0000-4000-8000-00000000000c",
            Provider::Claude,
            "dev@gamma.example",
            None,
        );
        let me = account(
            "00000000-0000-4000-8000-00000000000d",
            Provider::Claude,
            "me@example.com",
            None,
        );
        s.accounts = vec![acme.clone(), beta.clone()];
        // SB-285: each was placed by the person; a domain only suggests.
        s.memberships.insert(A.into(), O1.into());
        s.memberships.insert(B.into(), O2.into());
        s.memberships.insert(
            "00000000-0000-4000-8000-00000000000c".into(),
            "00000000-0000-4000-8000-000000000003".into(),
        );
        assert!(s.context_admits("shared", &acme));
        assert!(s.context_admits("shared", &beta));
        assert!(!s.context_admits("shared", &gamma));
        assert!(!s.context_admits("shared", &me));
        assert_eq!(s.session_context(&acme, None), format!("org:{O1}"));
        assert_eq!(s.session_context(&beta, None), format!("org:{O2}"));
        let folder = Some("/work/shared/app".as_ref());
        assert_eq!(s.session_context(&gamma, folder), "personal");
        assert_eq!(s.key_context(Some("shared"), None), format!("org:{O2}"));
        // Both organizations count the group as theirs.
        assert_eq!(
            s.organization_group(O1).map(|p| p.pool.as_str()),
            Some("shared")
        );
        assert_eq!(
            s.organization_group(O2).map(|p| p.pool.as_str()),
            Some("shared")
        );
        assert_eq!(s.organization_seats(O2).len(), 1);
    }

    #[test]
    fn without_organizations_every_account_is_personal_and_unexplained() {
        let s = Snapshot::default();
        let a = account(A, Provider::Claude, "me@example.com", Some("team"));
        assert_eq!(
            s.attribution(&a),
            Attribution {
                also: Vec::new(),
                organization: None,
                by: None
            }
        );
    }

    #[test]
    fn the_persons_word_wins_then_the_provider_organization_and_a_domain_only_suggests() {
        // SB-285: an address on an organization's domain places nothing; it suggests.
        let mut s = Snapshot {
            organizations: vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])],
            ..Default::default()
        };
        s.organizations[1].provider_organizations = vec!["claude:team-b".into()];
        // Domain: personal, with Acme suggested.
        let a = account(A, Provider::Claude, "dev@acme.example", Some("team-x"));
        assert_eq!(
            s.attribution(&a),
            Attribution {
                also: Vec::new(),
                organization: None,
                by: None
            }
        );
        assert_eq!(s.domain_suggestion(&a).map(|o| o.id.as_str()), Some(O1));
        assert_eq!(s.session_context(&a, None), "personal");
        // A provider organization decides; the domain still suggests the other one.
        let b = account(B, Provider::Claude, "dev@acme.example", Some("team-b"));
        assert_eq!(
            s.attribution(&b),
            Attribution {
                also: Vec::new(),
                organization: Some(O2.into()),
                by: Some(By::Provider)
            }
        );
        assert_eq!(s.domain_suggestion(&b).map(|o| o.id.as_str()), Some(O1));
        // The person's word beats both, "personal" included, and silences the suggestion.
        s.memberships.insert(B.into(), PERSONAL.into());
        assert_eq!(
            s.attribution(&b),
            Attribution {
                also: Vec::new(),
                organization: None,
                by: Some(By::Person)
            }
        );
        assert!(s.domain_suggestion(&b).is_none());
        s.memberships.insert(A.into(), O2.into());
        assert_eq!(
            s.attribution(&a),
            Attribution {
                also: Vec::new(),
                organization: Some(O2.into()),
                by: Some(By::Person)
            }
        );
        assert!(s.domain_suggestion(&a).is_none());
        // Holding the suggested organization (as an organization it also works for) leaves
        // nothing to suggest either.
        s.memberships.remove(A);
        s.also_organizations.insert(A.into(), vec![O1.into()]);
        assert!(s.domain_suggestion(&a).is_none());
        // No domain matches: no suggestion.
        let c = account(
            "00000000-0000-4000-8000-00000000000c",
            Provider::Claude,
            "me@home.example",
            None,
        );
        assert!(s.domain_organization(&c).is_none() && s.domain_suggestion(&c).is_none());
    }

    /// SB-285: placing an account in a group of the organization its address suggests is the
    /// person confirming it; a group of another organization, a provider team or the person's
    /// word keep refusing.
    #[test]
    fn placing_an_account_in_a_group_of_its_domains_organization_confirms_it() {
        let mut s = Snapshot {
            organizations: vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])],
            ..Default::default()
        };
        s.organizations[1].provider_organizations = vec!["claude:team-b".into()];
        let dev = account(A, Provider::Claude, "dev@acme.example", None);
        let teamed = account(B, Provider::Claude, "ops@acme.example", Some("team-b"));
        s.accounts = vec![dev.clone(), teamed.clone()];
        let acme = vec![O1.to_string()];
        let beta = vec![O2.to_string()];
        assert!(s.placement_admits(&acme, &dev));
        assert!(!s.placement_admits(&beta, &dev));
        assert!(s.placement_admits(&[], &dev), "a personal group takes it");
        // A provider team decides; the domain does not move it into Acme's group.
        assert!(!s.placement_admits(&acme, &teamed));
        assert!(!s.confirm_placement(&teamed.id, "acme-pool", &acme));
        // Confirming records the person's word, once.
        assert!(!s.confirm_placement(&dev.id, "beta-pool", &beta));
        assert!(s.confirm_placement(&dev.id, "acme-pool", &acme));
        assert_eq!(s.memberships.get(A).map(String::as_str), Some(O1));
        assert_eq!(s.attribution(&dev).by, Some(By::Person));
        assert!(
            !s.confirm_placement(&dev.id, "acme-pool", &acme),
            "already Acme's"
        );
        // The person said personal: no group of an organization takes it by its domain.
        s.memberships.insert(A.into(), PERSONAL.into());
        assert!(!s.placement_admits(&acme, &dev));
        // Without organizations nothing is refused.
        let empty = Snapshot::default();
        assert!(empty.placement_admits(&acme, &dev));
        // At home in a personal group and shared into Acme's: it also works for Acme, so both
        // groups keep taking it.
        let c = "00000000-0000-4000-8000-00000000000c";
        let mut shared = account(c, Provider::Claude, "qa@acme.example", None);
        shared.pool = "own-pool".into();
        s.accounts.push(shared.clone());
        s.projects = vec![crate::Project {
            pool: "own-pool".into(),
            name: "Own".into(),
            folders: vec!["/work/own".into()],
            created_at: 1,
            ..Default::default()
        }];
        assert!(s.confirm_placement(c, "acme-pool", &acme));
        assert!(!s.memberships.contains_key(c));
        assert_eq!(s.also_organizations.get(c), Some(&acme));
        let at = s.attribution(&shared);
        assert!(at.holds(None) && at.holds(Some(O1)));
        assert!(s.context_admits("own-pool", &shared));
    }

    /// SB-285: the move off the domain rule keeps, as the person's word, each placement it made
    /// that the account's groups agree with, once; one a group of another context holds (the
    /// 2026-11-08 incident) goes back to that group's word.
    #[test]
    fn the_domain_rule_retires_once_keeping_what_the_groups_agree_with() {
        use crate::Project;
        let group = |pool: &str, organization: Option<&str>| Project {
            shared_accounts: Vec::new(),
            pool: pool.into(),
            name: pool.into(),
            folders: vec![format!("/work/{pool}")],
            created_at: 1,
            sources: Vec::new(),
            organizations: organization.map(str::to_owned).into_iter().collect(),
        };
        let id = |n: u8| format!("00000000-0000-4000-8000-0000000000{n:02x}");
        let mut in_default = account(&id(1), Provider::Claude, "a@acme.example", None);
        in_default.pool = "default".into();
        let mut in_acme = account(&id(2), Provider::Claude, "b@acme.example", None);
        in_acme.pool = "acme-pool".into();
        let mut in_personal = account(&id(3), Provider::Claude, "c@acme.example", None);
        in_personal.pool = "own-pool".into();
        let mut said_personal = account(&id(4), Provider::Claude, "d@acme.example", None);
        said_personal.pool = "default".into();
        let mut teamed = account(&id(5), Provider::Claude, "e@acme.example", Some("team-b"));
        teamed.pool = "default".into();
        let mut home = account(&id(6), Provider::Claude, "me@home.example", None);
        home.pool = "own-pool".into();
        // At home in a personal group and shared into Acme's group (SB-282).
        let mut shared = account(&id(7), Provider::Claude, "f@acme.example", None);
        shared.pool = "own-pool".into();
        let mut s = Snapshot {
            organizations: vec![org(O1, "Acme", &["acme.example"]), org(O2, "Beta", &[])],
            projects: vec![group("acme-pool", Some(O1)), group("own-pool", None)],
            accounts: vec![
                in_default.clone(),
                in_acme.clone(),
                in_personal.clone(),
                said_personal.clone(),
                teamed.clone(),
                home.clone(),
                shared.clone(),
            ],
            ..Default::default()
        };
        s.projects[0].shared_accounts = vec![shared.id.clone()];
        s.organizations[1].provider_organizations = vec!["claude:team-b".into()];
        s.memberships
            .insert(said_personal.id.clone(), PERSONAL.into());
        assert!(s.retire_domain_rule());
        assert!(s.domain_rule_retired);
        let word = |s: &Snapshot, a: &Account| s.memberships.get(&a.id).cloned();
        assert_eq!(word(&s, &in_default).as_deref(), Some(O1));
        assert_eq!(word(&s, &in_acme).as_deref(), Some(O1));
        assert_eq!(s.attribution(&in_acme).by, Some(By::Person));
        assert!(
            s.context_admits("acme-pool", &in_acme),
            "nothing running changes"
        );
        // The personal group placed it before the domain arrived: its word stands.
        assert_eq!(word(&s, &in_personal), None);
        assert!(s.context_admits("own-pool", &in_personal));
        assert_eq!(
            s.domain_suggestion(&in_personal).map(|o| o.id.as_str()),
            Some(O1)
        );
        assert_eq!(word(&s, &said_personal).as_deref(), Some(PERSONAL));
        assert_eq!(word(&s, &teamed), None, "its team decides");
        assert_eq!(word(&s, &home), None);
        // Shared into Acme's group: its personal group's word stands, and Acme is one it also
        // works for, so neither group loses it.
        assert_eq!(word(&s, &shared), None);
        assert_eq!(
            s.also_organizations.get(&shared.id),
            Some(&vec![O1.to_string()])
        );
        assert!(s.context_admits("own-pool", &shared));
        assert!(s.context_admits("acme-pool", &shared));
        // Once: a second pass changes nothing, and a store without organizations needs none.
        let before = s.clone();
        assert!(!s.retire_domain_rule());
        assert_eq!(s.memberships, before.memberships);
        assert_eq!(s.events.len(), before.events.len());
        let mut empty = Snapshot::default();
        assert!(!empty.retire_domain_rule() && !empty.domain_rule_retired);
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
