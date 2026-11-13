//! Optional project rules: a project folder names the account its sessions should use.
use crate::{append_event, now, AuthKind, Provider, Snapshot, Store, Vault, MAX_ACCOUNTS};
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

/// A repository owner as `host/owner`, lowercase: `github.com/passioncode-ai` (SB-117).
/// `host/owner`, or a group path `host/group/sub` (DEC-0089 `ownerOf`, SB-141): a repository's
/// identity without its last segment.
pub fn source_valid(source: &str) -> bool {
    let parts: Vec<&str> = source.split('/').collect();
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
    };
    source.len() <= 300
        && (2..=8).contains(&parts.len())
        && parts[0].contains('.')
        && parts.iter().all(|p| ok(p))
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
                if state.project_of_pool(&account.pool).is_some() {
                    return Err(crate::PROJECT_NOT_NATIVE.into());
                }
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
        self.changed();
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
        self.changed();
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

/// A project: one or more folders (repositories) and the pool of accounts reserved for them
/// (operator request 2026-10-05). Its accounts serve only sessions started inside its folders;
/// inside its folders, sessions of a provider it has accounts for use only those accounts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ProjectWire", into = "ProjectWire")]
pub struct Project {
    /// The pool the project's accounts are in; also the project's stable id.
    pub pool: String,
    pub name: String,
    pub folders: Vec<String>,
    pub created_at: i64,
    /// Repository owners whose new repositories Switchboard suggests for this project
    /// (`github.com/passioncode-ai`, SB-117). Set by a person, never guessed.
    pub sources: Vec<String>,
    /// The organizations whose accounts this group runs on (SB-109, SB-282: none, one or
    /// several, in the person's order); empty is personal. The first is the group's own context
    /// where no account decides (a terminal in its folder, its saved key). Only read while at
    /// least one organization exists.
    pub organizations: Vec<String>,
    /// SB-282: accounts whose home is another pool that serve this group too, in the person's
    /// order. Their credential and quota record stay one; only their home pool holds them.
    pub shared_accounts: Vec<String>,
}
/// Most organizations one group is bound to (SB-282).
pub const MAX_GROUP_ORGANIZATIONS: usize = 16;
/// A group as `accounts.json` holds it: one organization as `organization`, which every
/// reader since 0.6.65 knows, several as `organizations` (schema version 9, SB-282).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectWire {
    pool: String,
    name: String,
    folders: Vec<String>,
    created_at: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    organizations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    shared_accounts: Vec<String>,
}
impl From<ProjectWire> for Project {
    fn from(w: ProjectWire) -> Self {
        let mut organizations: Vec<String> = w.organization.into_iter().collect();
        for o in w.organizations {
            if !organizations.contains(&o) {
                organizations.push(o);
            }
        }
        Project {
            pool: w.pool,
            name: w.name,
            folders: w.folders,
            created_at: w.created_at,
            sources: w.sources,
            organizations,
            shared_accounts: w.shared_accounts,
        }
    }
}
impl From<Project> for ProjectWire {
    fn from(p: Project) -> Self {
        let (organization, organizations) = if p.organizations.len() == 1 {
            (p.organizations.into_iter().next(), Vec::new())
        } else {
            (None, p.organizations)
        };
        ProjectWire {
            pool: p.pool,
            name: p.name,
            folders: p.folders,
            created_at: p.created_at,
            sources: p.sources,
            organization,
            organizations,
            shared_accounts: p.shared_accounts,
        }
    }
}
impl Project {
    /// The group's own context where no account decides (SB-282): its first organization;
    /// none is personal.
    pub fn primary_organization(&self) -> Option<&str> {
        self.organizations.first().map(String::as_str)
    }
    /// Whether the group is bound to `organization`.
    pub fn bound_to(&self, organization: &str) -> bool {
        self.organizations.iter().any(|o| o == organization)
    }
    /// The group's contexts in order (SB-282): personal (none) for a personal group, else its
    /// organizations.
    pub fn contexts(&self) -> Vec<Option<&str>> {
        if self.organizations.is_empty() {
            vec![None]
        } else {
            self.organizations
                .iter()
                .map(|o| Some(o.as_str()))
                .collect()
        }
    }
    /// Whether an account so attributed may serve the group (SB-109, SB-282): their contexts
    /// meet, personal included.
    pub fn admits(&self, attribution: &crate::organizations::Attribution) -> bool {
        self.contexts().into_iter().any(|c| attribution.holds(c))
    }
}
const MAX_PROJECTS: usize = 64;
const MAX_SOURCES: usize = 16;
/// Folders across all projects; a project itself has no folder limit (operator 2026-10-14,
/// SB-117). 1024 paths × `MAX_PATH` keeps the metadata worst case at 1 MiB of the 2 MiB file.
const MAX_ALL_FOLDERS: usize = 1024;

/// One group's folder inside another group's (SB-146 keeps a shared repository legal).
pub const FOLDER_NESTED: &str = "A folder sits inside a folder of another group. Add the repository itself to that group instead.";
/// `a` inside `b`, by path components: `/a/b` is inside `/a`, `/ab` is not.
fn inside(a: &str, b: &str) -> bool {
    Path::new(a).starts_with(Path::new(b))
}

/// The organizations a person named for a group, in their order, each once: `personal` and
/// blanks are dropped, an unknown one is refused (SB-282).
fn organizations_checked(snapshot: &Snapshot, list: Vec<String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for id in list {
        let id = id.trim();
        if id.is_empty() || id == crate::organizations::PERSONAL || out.iter().any(|o| o == id) {
            continue;
        }
        if snapshot.organization(id).is_none() {
            return Err(crate::organizations::ORGANIZATION_NOT_FOUND.into());
        }
        out.push(id.to_owned());
    }
    if out.len() > MAX_GROUP_ORGANIZATIONS {
        return Err(GROUP_ORGANIZATIONS_TOO_MANY.into());
    }
    Ok(out)
}
/// SB-282: a route to an account the pool no longer shares (taken out, or its group removed) is
/// dropped, as a moved account's route in its old pool is.
/// SB-313: a group shares only accounts at home in another group. An account whose own group
/// went away (deleted, or the account moved to `default`) left in another group's list was taken
/// as a move by that group's next save (its route in `default` went with it), and kept the group's
/// later saves refused while the ordinary CLI was signed in to it. Answers whether one was removed.
pub(crate) fn stale_shares_prune(s: &mut Snapshot) -> bool {
    let homes: std::collections::HashMap<String, String> = s
        .accounts
        .iter()
        .map(|a| (a.id.clone(), a.pool.clone()))
        .collect();
    let groups: Vec<String> = s.projects.iter().map(|p| p.pool.clone()).collect();
    let mut removed = false;
    for p in s.projects.iter_mut() {
        let before = p.shared_accounts.len();
        p.shared_accounts.retain(|id| {
            homes
                .get(id)
                .is_some_and(|home| home != &p.pool && groups.contains(home))
        });
        removed |= p.shared_accounts.len() != before;
    }
    removed
}
fn shared_routes_prune(s: &mut Snapshot) {
    let keep: Vec<String> = s
        .routes
        .iter()
        .filter(|(key, id)| {
            s.accounts.iter().any(|a| {
                &&a.id == id
                    && s.pools_of(a)
                        .iter()
                        .any(|p| **key == format!("{}:{}", a.provider.as_str(), p))
            })
        })
        .map(|(k, _)| k.clone())
        .collect();
    s.routes.retain(|k, _| keep.contains(k));
}
/// Refusal: more organizations than one group holds (SB-282).
pub const GROUP_ORGANIZATIONS_TOO_MANY: &str = "A group can belong to at most 16 organizations.";

/// The pool a launch on the OpenRouter key writes in its record (SB-240): it names the key, not a
/// group, so a new group never takes it (a group named "OpenRouter" becomes `openrouter-2`), as
/// it never takes `default`.
pub const KEY_POOL: &str = "openrouter";
/// A pool name from a project name: lowercase letters, digits and hyphens.
pub fn pool_from_name(name: &str) -> String {
    let mut pool = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            pool.push(c.to_ascii_lowercase());
        } else if !pool.ends_with('-') && !pool.is_empty() {
            pool.push('-');
        }
    }
    let pool: String = pool.trim_end_matches('-').chars().take(24).collect();
    if pool.is_empty() {
        "project".into()
    } else {
        pool
    }
}

pub(crate) fn validate_projects(s: &Snapshot) -> Result<(), String> {
    if s.projects.len() > MAX_PROJECTS {
        return Err("Invalid metadata bounds".into());
    }
    let mut pools = std::collections::HashSet::new();
    let mut all: Vec<(&str, &str)> = Vec::new();
    for p in &s.projects {
        if !crate::pool_valid(&p.pool)
            || !pools.insert(p.pool.as_str())
            || !crate::label_valid(&p.name)
            || p.folders.is_empty()
            || p.sources.len() > MAX_SOURCES
            || p.sources.iter().any(|s| !source_valid(s))
            || p.folders.iter().any(|f| !path_valid(f))
            || p.created_at <= 0
            || p.organizations.len() > MAX_GROUP_ORGANIZATIONS
            || p.shared_accounts.len() > MAX_ACCOUNTS
            || p.shared_accounts.iter().enumerate().any(|(i, id)| {
                p.shared_accounts[..i].contains(id)
                    || !s.accounts.iter().any(|a| &a.id == id && a.pool != p.pool)
            })
            || p.organizations
                .iter()
                .enumerate()
                .any(|(i, o)| p.organizations[..i].contains(o))
        {
            return Err("Invalid project metadata".into());
        }
        for f in &p.folders {
            all.push((p.pool.as_str(), f.as_str()));
        }
    }
    if all.len() > MAX_ALL_FOLDERS {
        return Err("Invalid project metadata".into());
    }
    // SB-146: a folder may be in several groups as itself, never inside another group's folder.
    for (i, (pool_a, a)) in all.iter().enumerate() {
        for (pool_b, b) in &all[i + 1..] {
            if pool_a != pool_b && a != b && (inside(a, b) || inside(b, a)) {
                return Err("Invalid project metadata".into());
            }
        }
    }
    Ok(())
}

impl Snapshot {
    /// SB-322: the account is given to an agent, so nothing chooses it on its own.
    pub fn is_assigned(&self, id: &str) -> bool {
        self.assigned_to.contains_key(id)
    }
    /// The group a session in `path` works in (SB-146): of the groups holding the folder, the
    /// first (in creation order) with an enabled account, else the first.
    /// With organizations defined, a personal group comes first: the more private wins (SB-109,
    /// RM-28 CX-26).
    pub fn project_for(&self, path: &Path) -> Option<&Project> {
        let mut all = self.projects_for(path);
        if !self.organizations.is_empty() {
            all.sort_by_key(|p| !p.organizations.is_empty());
        }
        all.iter()
            .find(|p| self.accounts.iter().any(|a| a.pool == p.pool && a.enabled))
            .or(all.first())
            .copied()
    }
    /// Every group whose folder contains `path` (SB-146: a repository may be in several).
    pub fn projects_for(&self, path: &Path) -> Vec<&Project> {
        self.projects
            .iter()
            .filter(|p| p.folders.iter().any(|f| path.starts_with(Path::new(f))))
            .collect()
    }
    /// The project that reserves `pool`, if any.
    pub fn project_of_pool(&self, pool: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.pool == pool)
    }
    /// Whether `account` serves `pool` (SB-282): its home pool, or a group that shares it.
    pub fn serves(&self, account: &crate::Account, pool: &str) -> bool {
        account.pool == pool
            || self
                .project_of_pool(pool)
                .is_some_and(|p| p.shared_accounts.iter().any(|id| id == &account.id))
    }
    /// The accounts that serve `pool`: its own, then those other groups share with it.
    pub fn pool_accounts(&self, pool: &str) -> Vec<&crate::Account> {
        let mut out: Vec<&crate::Account> =
            self.accounts.iter().filter(|a| a.pool == pool).collect();
        if let Some(p) = self.project_of_pool(pool) {
            for id in &p.shared_accounts {
                if let Some(a) = self.accounts.iter().find(|a| &a.id == id && a.pool != pool) {
                    out.push(a);
                }
            }
        }
        out
    }
    /// The pools an account serves: its home, then each group sharing it (SB-282).
    pub fn pools_of<'a>(&'a self, account: &'a crate::Account) -> Vec<&'a str> {
        std::iter::once(account.pool.as_str())
            .chain(
                self.projects
                    .iter()
                    .filter(|p| p.pool != account.pool && p.shared_accounts.contains(&account.id))
                    .map(|p| p.pool.as_str()),
            )
            .collect()
    }
}

impl Store {
    /// Creates or updates a project. `pool` names an existing project to update; a new project
    /// gets a pool from its name. `accounts` is the project's whole set: each listed account
    /// moves into the project's pool, and one of its accounts left out moves back to `default`.
    /// Folders are canonical directories (the caller resolves them).
    pub fn save_project(
        &self,
        pool: Option<&str>,
        name: &str,
        folders: &[std::path::PathBuf],
        accounts: &[String],
    ) -> Result<Project, String> {
        self.save_project_in(pool, name, folders, accounts, None)
    }
    /// [`Store::save_project`], also setting the group's organization when `organization` is
    /// given (`Some(None)` makes it personal); left out, a saved group keeps its own (SB-109).
    pub fn save_project_in(
        &self,
        pool: Option<&str>,
        name: &str,
        folders: &[std::path::PathBuf],
        accounts: &[String],
        organization: Option<Option<&str>>,
    ) -> Result<Project, String> {
        let organizations = organization.map(|o| o.map(str::to_owned).into_iter().collect());
        self.save_project_with(pool, name, folders, accounts, organizations, true)
    }
    /// [`Store::save_project`], also setting the group's organizations when given (SB-282: an
    /// empty list makes it personal); left out, a saved group keeps its own. With `share`, a
    /// listed account whose home is another group serves this one too and keeps its home
    /// (SB-282); without it, it moves here (`MoveAccount`). An account in no group's pool moves
    /// here either way.
    pub fn save_project_with(
        &self,
        pool: Option<&str>,
        name: &str,
        folders: &[std::path::PathBuf],
        accounts: &[String],
        organizations: Option<Vec<String>>,
        share: bool,
    ) -> Result<Project, String> {
        let name = name.trim();
        if !crate::label_valid(name) {
            return Err("Name the project.".into());
        }
        if folders.is_empty() {
            return Err("Add at least one project folder.".into());
        }
        let mut folder_list: Vec<String> = Vec::new();
        for f in folders {
            let f = f
                .to_str()
                .filter(|p| path_valid(p))
                .ok_or("Choose absolute project folders.")?
                .to_owned();
            if !folder_list.contains(&f) {
                folder_list.push(f);
            }
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let existing = pool.and_then(|p| candidate.projects.iter().position(|x| x.pool == p));
        if pool.is_some() && existing.is_none() {
            return Err("Project not found.".into());
        }
        let pool = match existing {
            Some(i) => candidate.projects[i].pool.clone(),
            None => {
                let base = pool_from_name(name);
                let taken = |p: &str| {
                    p == "default"
                        || p == KEY_POOL
                        || candidate.projects.iter().any(|x| x.pool == p)
                        || candidate
                            .accounts
                            .iter()
                            .any(|a| a.pool == p && !accounts.contains(&a.id))
                };
                let mut chosen = base.clone();
                let mut n = 2;
                while taken(&chosen) {
                    chosen = format!("{base}-{n}");
                    n += 1;
                }
                chosen
            }
        };
        let elsewhere: usize = candidate
            .projects
            .iter()
            .filter(|p| p.pool != pool)
            .map(|p| p.folders.len())
            .sum();
        if elsewhere + folder_list.len() > MAX_ALL_FOLDERS {
            return Err("Projects hold at most 1024 folders in all.".into());
        }
        for other in candidate.projects.iter().filter(|p| p.pool != pool) {
            for f in &folder_list {
                // SB-146: the same repository may be in several groups; one group's folder
                // inside another's stays ambiguous and is refused.
                if other
                    .folders
                    .iter()
                    .any(|g| g != f && (inside(f, g) || inside(g, f)))
                {
                    return Err(FOLDER_NESTED.into());
                }
            }
        }
        for id in accounts {
            if !candidate.accounts.iter().any(|a| &a.id == id) {
                return Err("Account not found".into());
            }
        }
        // SB-282: listed accounts at home in another group are shared, in the person's order.
        let shared: Vec<String> = accounts
            .iter()
            .filter(|id| {
                share
                    && candidate.accounts.iter().any(|a| {
                        &&a.id == id
                            && a.pool != pool
                            && candidate.projects.iter().any(|p| p.pool == a.pool)
                    })
            })
            .cloned()
            .collect();
        for id in &shared {
            let account = candidate.accounts.iter().find(|a| &a.id == id).unwrap();
            let clash = candidate.accounts.iter().any(|b| {
                b.id != account.id
                    && b.provider == account.provider
                    && b.pool == pool
                    && accounts.contains(&b.id)
                    && b.external_identity
                        .as_ref()
                        .zip(account.external_identity.as_ref())
                        .is_some_and(|(x, y)| x.matches(y))
            });
            if clash {
                return Err(
                    "This account is already saved in that pool. Remove the other copy first."
                        .into(),
                );
            }
        }
        // Membership: listed accounts in (or shared), the project's others out to `default`.
        let moves: Vec<(String, String)> = candidate
            .accounts
            .iter()
            .filter_map(|a| {
                if accounts.contains(&a.id) && a.pool != pool && !shared.contains(&a.id) {
                    Some((a.id.clone(), pool.clone()))
                } else if !accounts.contains(&a.id) && a.pool == pool {
                    Some((a.id.clone(), "default".to_string()))
                } else {
                    None
                }
            })
            .collect();
        for (id, to) in &moves {
            let account = candidate
                .accounts
                .iter()
                .find(|a| &a.id == id)
                .unwrap()
                .clone();
            let clash = candidate.accounts.iter().any(|b| {
                b.id != account.id
                    && b.provider == account.provider
                    && &b.pool == to
                    && !moves.iter().any(|(m, _)| m == &b.id)
                    && b.external_identity
                        .as_ref()
                        .zip(account.external_identity.as_ref())
                        .is_some_and(|(x, y)| x.matches(y))
            });
            if clash {
                return Err(
                    "This account is already saved in that pool. Remove the other copy first."
                        .into(),
                );
            }
        }
        let current =
            existing.map_or_else(Vec::new, |i| candidate.projects[i].organizations.clone());
        let organizations = match organizations {
            Some(list) => organizations_checked(&candidate, list)?,
            None => current.clone(),
        };
        // SB-109, SB-282: an account joins a group of its own context only; a group whose
        // organizations change keeps only accounts of the new ones. SB-285: placing an account
        // whose address suggests one of the group's organizations confirms it (below, once it
        // has moved).
        // SB-314: an account the group already shares is not joining; re-checking it on every save
        // let one misfit share block a rename or the background's folder add.
        let shared_before: Vec<String> =
            existing.map_or_else(Vec::new, |i| candidate.projects[i].shared_accounts.clone());
        let joining: Vec<String> = candidate
            .accounts
            .iter()
            .filter(|a| {
                accounts.contains(&a.id)
                    && (organizations != current
                        || (a.pool != pool && !shared_before.contains(&a.id)))
            })
            .map(|a| a.id.clone())
            .collect();
        for id in &joining {
            let a = candidate.accounts.iter().find(|a| &a.id == id).unwrap();
            if !candidate.placement_admits(&organizations, a) {
                return Err(candidate.seat_chosen(a));
            }
        }
        for (id, to) in &moves {
            let old = {
                let a = candidate.accounts.iter_mut().find(|a| &a.id == id).unwrap();
                let old = format!("{}:{}", a.provider.as_str(), a.pool);
                a.pool = to.clone();
                old
            };
            // A group that shared it and is now its home holds it as its own.
            for p in candidate.projects.iter_mut().filter(|p| &p.pool == to) {
                p.shared_accounts.retain(|s| s != id);
            }
            // The route it held in its old pool no longer applies there.
            if candidate.routes.get(&old) == Some(id) {
                candidate.routes.remove(&old);
            }
            // Its project rules point at folders it may no longer serve; rules stay valid data.
        }
        // A native rotation policy on the adopted pool would hand its accounts to every folder.
        for policy in candidate.policies.iter_mut() {
            if policy.pool == pool && policy.target == "claude_cli" {
                policy.enabled = false;
            }
        }
        let created_at = existing.map_or_else(now, |i| candidate.projects[i].created_at);
        let sources = existing.map_or_else(Vec::new, |i| candidate.projects[i].sources.clone());
        let project = Project {
            pool: pool.clone(),
            name: name.to_owned(),
            folders: folder_list,
            created_at,
            sources,
            organizations,
            shared_accounts: shared,
        };
        match existing {
            Some(i) => candidate.projects[i] = project.clone(),
            None => candidate.projects.push(project.clone()),
        }
        candidate.projects.sort_by(|a, b| a.name.cmp(&b.name));
        stale_shares_prune(&mut candidate);
        shared_routes_prune(&mut candidate);
        append_event(
            &mut candidate,
            "project",
            None,
            if existing.is_some() {
                "updated"
            } else {
                "created"
            },
        );
        for id in &joining {
            candidate.confirm_placement(id, &pool, &project.organizations);
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(project)
    }
    /// Binds a group to an organization, or to none (personal), SB-109. Refused while the group
    /// still holds an account of another context.
    pub fn set_project_organization(
        &self,
        pool: &str,
        organization: Option<&str>,
    ) -> Result<Project, String> {
        let organization = organization.filter(|o| *o != crate::organizations::PERSONAL);
        self.set_project_organizations(pool, organization.map(str::to_owned).into_iter().collect())
    }
    /// Binds a group to these organizations (SB-282: none is personal, several share it, the
    /// first is its own context). Refused while the group holds an account of another context.
    pub fn set_project_organizations(
        &self,
        pool: &str,
        organizations: Vec<String>,
    ) -> Result<Project, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let organizations = organizations_checked(&candidate, organizations)?;
        if candidate
            .accounts
            .iter()
            .any(|a| candidate.serves(a, pool) && !candidate.placement_admits(&organizations, a))
        {
            return Err(crate::organizations::SEATS_ELSEWHERE.into());
        }
        // SB-285: binding the group to the organization its accounts' addresses suggest
        // confirms it for each of them.
        let serving: Vec<String> = candidate
            .accounts
            .iter()
            .filter(|a| candidate.serves(a, pool))
            .map(|a| a.id.clone())
            .collect();
        for id in &serving {
            candidate.confirm_placement(id, pool, &organizations);
        }
        let project = candidate
            .projects
            .iter_mut()
            .find(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        project.organizations = organizations;
        let project = project.clone();
        append_event(&mut candidate, "project", None, "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(project)
    }
    /// SB-352 (DEC-0012): lends account `id` to the group of `pool` as a shared account
    /// (`share`), or takes it back. The account keeps its home group; only the group's
    /// `shared_accounts` changes, as `save_project_with` shares one (SB-282). A share is admitted
    /// only when the account is at home in another group a person made, the group of `pool`
    /// takes it by its organizations as they stand now (`context_admits`: never a new word on
    /// its organizations, unlike a person's placement) and holds no other copy of its sign-in.
    /// Answers whether the group changed; a share that is not admitted, or already as asked,
    /// changes nothing.
    pub fn set_shared(&self, pool: &str, id: &str, share: bool) -> Result<bool, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let account = candidate
            .accounts
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or("Account not found")?;
        let index = candidate
            .projects
            .iter()
            .position(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        let held = candidate.projects[index]
            .shared_accounts
            .contains(&account.id);
        if share {
            let clash = candidate.accounts.iter().any(|b| {
                b.id != account.id
                    && b.provider == account.provider
                    && b.pool == pool
                    && b.external_identity
                        .as_ref()
                        .zip(account.external_identity.as_ref())
                        .is_some_and(|(x, y)| x.matches(y))
            });
            if held
                || account.pool == pool
                || candidate.project_of_pool(&account.pool).is_none()
                || !candidate.context_admits(pool, &account)
                || clash
            {
                return Ok(false);
            }
            candidate.projects[index]
                .shared_accounts
                .push(account.id.clone());
        } else {
            if !held {
                return Ok(false);
            }
            candidate.projects[index]
                .shared_accounts
                .retain(|s| s != &account.id);
        }
        stale_shares_prune(&mut candidate);
        shared_routes_prune(&mut candidate);
        append_event(&mut candidate, "project", Some(id), "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(true)
    }
    /// SB-282: shares these accounts with the group of `pool` from a backup, where the group
    /// holds no share yet; an account at home there, or unknown, is left out.
    pub fn restore_shared_accounts(&self, pool: &str, ids: &[String]) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let accounts = candidate.accounts.clone();
        let project = candidate
            .projects
            .iter_mut()
            .find(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        if !project.shared_accounts.is_empty() {
            return Err("Nothing to restore.".into());
        }
        for id in ids {
            if accounts.iter().any(|a| &a.id == id && a.pool != pool)
                && !project.shared_accounts.contains(id)
            {
                project.shared_accounts.push(id.clone());
            }
        }
        if project.shared_accounts.is_empty() {
            return Err("Nothing to restore.".into());
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// SB-314: a restore puts `id` back in `pool` (`default` included) when the group of `pool`
    /// admits it by its word as it stands now ([`crate::Snapshot::context_admits`]) and that pool
    /// holds no other copy of its identity; nothing else. True when it moved. Its route in the
    /// pool it leaves goes, and shares that no longer apply are pruned.
    pub(crate) fn restore_account_pool(&self, id: &str, pool: &str) -> Result<bool, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let Some(account) = candidate.accounts.iter().find(|a| a.id == id).cloned() else {
            return Ok(false);
        };
        if account.pool == pool || !candidate.context_admits(pool, &account) {
            return Ok(false);
        }
        let clash = candidate.accounts.iter().any(|b| {
            b.id != account.id
                && b.provider == account.provider
                && b.pool == pool
                && b.external_identity
                    .as_ref()
                    .zip(account.external_identity.as_ref())
                    .is_some_and(|(x, y)| x.matches(y))
        });
        if clash {
            return Ok(false);
        }
        let old = format!("{}:{}", account.provider.as_str(), account.pool);
        if candidate.routes.get(&old).map(String::as_str) == Some(id) {
            candidate.routes.remove(&old);
        }
        if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == id) {
            a.pool = pool.into();
        }
        for p in candidate.projects.iter_mut().filter(|p| p.pool == pool) {
            p.shared_accounts.retain(|s| s != id);
        }
        stale_shares_prune(&mut candidate);
        shared_routes_prune(&mut candidate);
        append_event(&mut candidate, "account_updated", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(true)
    }
    /// SB-122: moves an account whose pool no project reserves (a deleted project leaves its
    /// accounts there) back to `default`. A project's account leaves through `save_project`.
    pub fn return_to_default(&self, id: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let account = candidate
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("Account not found")?
            .clone();
        if account.pool == "default" {
            return Ok(());
        }
        if candidate.project_of_pool(&account.pool).is_some() {
            return Err("This account is in a project. Take it out of the project instead.".into());
        }
        let clash = candidate.accounts.iter().any(|b| {
            b.id != account.id
                && b.provider == account.provider
                && b.pool == "default"
                && b.external_identity
                    .as_ref()
                    .zip(account.external_identity.as_ref())
                    .is_some_and(|(x, y)| x.matches(y))
        });
        if clash {
            return Err(
                "This account is already saved in that pool. Remove the other copy first.".into(),
            );
        }
        let old = format!("{}:{}", account.provider.as_str(), account.pool);
        if candidate.routes.get(&old).map(String::as_str) == Some(id) {
            candidate.routes.remove(&old);
        }
        if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == id) {
            a.pool = "default".into();
        }
        stale_shares_prune(&mut candidate);
        shared_routes_prune(&mut candidate);
        append_event(&mut candidate, "account_updated", Some(id), "success");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// Puts a project back from a backup as it was: its accounts are already in its pool. A
    /// project whose pool or folders this install already uses is left out.
    pub fn restore_project(&self, mut project: Project) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        // An organization this install does not hold leaves the group (personal when none is
        // left).
        project
            .organizations
            .retain(|o| candidate.organization(o).is_some());
        if candidate.projects.iter().any(|p| p.pool == project.pool) {
            return Err("Project already here.".into());
        }
        candidate.projects.push(project);
        candidate.projects.sort_by(|a, b| a.name.cmp(&b.name));
        append_event(&mut candidate, "project", None, "created");
        // Validation refuses an overlap with this install's folders or a bad record.
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// Replaces a project's sources: the repository owners whose new repositories Switchboard
    /// suggests for it (SB-117). Each is `host/owner` or a group path `host/group/sub`, lowercase.
    pub fn set_project_sources(&self, pool: &str, sources: &[String]) -> Result<Project, String> {
        let mut list: Vec<String> = Vec::new();
        for source in sources {
            let source = source.trim().trim_end_matches('/').to_ascii_lowercase();
            if !source_valid(&source) {
                return Err("Name a source as host/owner or host/group/subgroup, for example github.com/example.".into());
            }
            if !list.contains(&source) {
                list.push(source);
            }
        }
        if list.len() > MAX_SOURCES {
            return Err("A project watches at most 16 sources.".into());
        }
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let project = candidate
            .projects
            .iter_mut()
            .find(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        project.sources = list;
        let project = project.clone();
        // Journaled as "updated", a word every 0.6 build accepts (SB-117 seam review).
        append_event(&mut candidate, "project", None, "updated");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(project)
    }
    /// Removes a project. Its accounts go back to `default` (SB-314: left in a pool no folder routes
    /// to, they served nothing); one whose sign-in `default` already holds stays where it was.
    pub fn remove_project(&self, pool: &str) -> Result<Project, String> {
        // H-2: under the credential lock, so no save of this project's key lands between the
        // metadata and the deletions; the deletions run after the metadata lock is released.
        let _writes = self.credential_lock()?;
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let index = candidate
            .projects
            .iter()
            .position(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        let project = candidate.projects.remove(index);
        crate::chains::prune(&mut candidate);
        let leaving: Vec<String> = candidate
            .accounts
            .iter()
            .filter(|a| a.pool == pool)
            .map(|a| a.id.clone())
            .collect();
        for id in leaving {
            let account = candidate
                .accounts
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .clone();
            let clash = candidate.accounts.iter().any(|b| {
                b.id != account.id
                    && b.provider == account.provider
                    && b.pool == "default"
                    && b.external_identity
                        .as_ref()
                        .zip(account.external_identity.as_ref())
                        .is_some_and(|(x, y)| x.matches(y))
            });
            if clash {
                continue;
            }
            let old = format!("{}:{}", account.provider.as_str(), account.pool);
            if candidate.routes.get(&old) == Some(&id) {
                candidate.routes.remove(&old);
            }
            if let Some(a) = candidate.accounts.iter_mut().find(|a| a.id == id) {
                a.pool = "default".into();
            }
        }
        stale_shares_prune(&mut candidate);
        shared_routes_prune(&mut candidate);
        // SB-144: the project's own agent keys go with it, so a later project of the same pool
        // never inherits them; the vault items are deleted after the metadata (a leftover item is
        // unreachable).
        let keys: Vec<String> = candidate
            .agent_keys
            .iter()
            .filter(|k| k.pool.as_deref() == Some(pool))
            .map(|k| k.id.clone())
            .collect();
        candidate
            .agent_keys
            .retain(|k| k.pool.as_deref() != Some(pool));
        append_event(&mut candidate, "project", None, "removed");
        self.publish(&mut state, candidate)?;
        drop(state);
        self.changed();
        for id in keys {
            let _ = self.vault.delete(&id);
        }
        Ok(project)
    }
}
