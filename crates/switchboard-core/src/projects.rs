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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// The pool the project's accounts are in; also the project's stable id.
    pub pool: String,
    pub name: String,
    pub folders: Vec<String>,
    pub created_at: i64,
}
const MAX_PROJECTS: usize = 64;
const MAX_FOLDERS: usize = 16;

/// `a` inside `b`, by path components: `/a/b` is inside `/a`, `/ab` is not.
fn inside(a: &str, b: &str) -> bool {
    Path::new(a).starts_with(Path::new(b))
}

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
            || p.folders.len() > MAX_FOLDERS
            || p.folders.iter().any(|f| !path_valid(f))
            || p.created_at <= 0
        {
            return Err("Invalid project metadata".into());
        }
        for f in &p.folders {
            all.push((p.pool.as_str(), f.as_str()));
        }
    }
    // A folder belongs to one project: no folder of one project inside another's.
    for (i, (pool_a, a)) in all.iter().enumerate() {
        for (pool_b, b) in &all[i + 1..] {
            if pool_a != pool_b && (inside(a, b) || inside(b, a)) {
                return Err("Invalid project metadata".into());
            }
        }
    }
    Ok(())
}

impl Snapshot {
    /// The project whose folder contains `path`, if any.
    pub fn project_for(&self, path: &Path) -> Option<&Project> {
        self.projects
            .iter()
            .find(|p| p.folders.iter().any(|f| path.starts_with(Path::new(f))))
    }
    /// The project that reserves `pool`, if any.
    pub fn project_of_pool(&self, pool: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.pool == pool)
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
        let name = name.trim();
        if !crate::label_valid(name) {
            return Err("Name the project.".into());
        }
        if folders.is_empty() || folders.len() > MAX_FOLDERS {
            return Err("Add between one and sixteen project folders.".into());
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
        for other in candidate.projects.iter().filter(|p| p.pool != pool) {
            for f in &folder_list {
                if other.folders.iter().any(|g| inside(f, g) || inside(g, f)) {
                    return Err("A folder is already part of another project.".into());
                }
            }
        }
        for id in accounts {
            if !candidate.accounts.iter().any(|a| &a.id == id) {
                return Err("Account not found".into());
            }
        }
        // Membership: listed accounts in, the project's others out to `default`.
        let moves: Vec<(String, String)> = candidate
            .accounts
            .iter()
            .filter_map(|a| {
                if accounts.contains(&a.id) && a.pool != pool {
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
        for (id, to) in &moves {
            let old = {
                let a = candidate.accounts.iter_mut().find(|a| &a.id == id).unwrap();
                let old = format!("{}:{}", a.provider.as_str(), a.pool);
                a.pool = to.clone();
                old
            };
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
        let project = Project {
            pool: pool.clone(),
            name: name.to_owned(),
            folders: folder_list,
            created_at,
        };
        match existing {
            Some(i) => candidate.projects[i] = project.clone(),
            None => candidate.projects.push(project.clone()),
        }
        candidate.projects.sort_by(|a, b| a.name.cmp(&b.name));
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
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(project)
    }
    /// Puts a project back from a backup as it was: its accounts are already in its pool. A
    /// project whose pool or folders this install already uses is left out.
    pub fn restore_project(&self, project: Project) -> Result<(), String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
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
    /// Removes a project. Its accounts stay in the pool, which becomes an ordinary pool.
    pub fn remove_project(&self, pool: &str) -> Result<Project, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        let index = candidate
            .projects
            .iter()
            .position(|p| p.pool == pool)
            .ok_or("Project not found.")?;
        let project = candidate.projects.remove(index);
        crate::chains::prune(&mut candidate);
        append_event(&mut candidate, "project", None, "removed");
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(project)
    }
}
