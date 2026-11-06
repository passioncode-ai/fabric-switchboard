//! SB-176 (SB-168 D-4): settings that move to another machine without a credential. Groups travel
//! by their repositories' identities (DEC-0089 item 2), so the other machine finds its own
//! checkouts of them; organizations by name and domains; the agents' settings; rotation policies.
//! Accounts, rules and fallback chains name account ids and stay behind: the other machine signs
//! in to its own seats (SB-168 D-1). SB-270 adds the Fabric Inbox key's server and Client Id (its
//! secret stays behind), the organizations' provider teams, the terminal hook's choice and the
//! spending ceilings of the computer and its groups. Import is a preview until it is applied.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use switchboard_core::{ChainScope, RotationPolicy, Snapshot, Store};

pub const FORMAT: &str = "switchboard-settings/1";
/// A bundle is a small JSON document; anything larger is not one.
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub exported_at: i64,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub organizations: Vec<Organization>,
    #[serde(default)]
    pub agents: Option<Agents>,
    #[serde(default)]
    pub policies: Vec<RotationPolicy>,
    /// SB-270: the Fabric Inbox key's server and Client Id; its secret is never in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox: Option<Inbox>,
    /// SB-270: the terminal hook's choice, `on` or `off`, when the person made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_hook: Option<String>,
    /// SB-270: spending ceilings of the computer and of its groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ceilings: Vec<Ceiling>,
    /// SB-272: an administrator's signature over the rest of the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<crate::settings_signing::Signature>,
}

/// SB-272: the bytes a signature covers: the bundle as Switchboard writes it, without its
/// signature.
pub fn signed_message(bundle: &Bundle) -> Vec<u8> {
    let mut unsigned = bundle.clone();
    unsigned.signature = None;
    serde_json::to_vec(&unsigned).unwrap_or_default()
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Inbox {
    pub origin: String,
    pub client_id: String,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Ceiling {
    /// The group's pool in the file; none for the computer's own ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub cents: u32,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub pool: String,
    /// The organization's name, when the group runs on one.
    #[serde(default)]
    pub organization: Option<String>,
    /// SB-282: the names of its further organizations, beyond `organization`. Written only
    /// when the group has several, which a 0.7.20 importer refuses as an unknown field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_organizations: Vec<String>,
    /// Repository identities (`host/owner/name`) of the group's checkouts.
    #[serde(default)]
    pub repositories: Vec<String>,
    /// Folders outside Git or without an `origin`, by path: they match only at the same path.
    #[serde(default)]
    pub plain_folders: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub name: String,
    #[serde(default)]
    pub domains: Vec<String>,
    /// SB-270: its provider teams (`claude:<uuid>`, `codex:<workspace>`), so the team's seats
    /// signed in on the other computer follow it there too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_organizations: Vec<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Agents {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub disabled: Vec<String>,
}

/// This machine's settings as a bundle.
pub fn export(root: &Path, snapshot: &Snapshot, now: i64) -> Bundle {
    let org_name = |id: &str| snapshot.organization(id).map(|o| o.name.clone());
    let groups = snapshot
        .projects
        .iter()
        .map(|p| {
            let mut repositories = Vec::new();
            let mut plain_folders = Vec::new();
            for root in crate::roots::tree(&p.folders) {
                // SB-281: an agent's data folder under its project travels as a plain folder.
                plain_folders.extend(
                    root.derived
                        .iter()
                        .filter(|d| d.kind == "data")
                        .map(|d| d.folder.clone()),
                );
                match root.remote {
                    Some(remote) if !repositories.contains(&remote) => repositories.push(remote),
                    Some(_) => {}
                    None => plain_folders.push(root.folder),
                }
            }
            Group {
                name: p.name.clone(),
                pool: p.pool.clone(),
                organization: p.primary_organization().and_then(org_name),
                also_organizations: p
                    .organizations
                    .iter()
                    .skip(1)
                    .filter_map(|o| org_name(o))
                    .collect(),
                repositories,
                plain_folders,
                sources: p.sources.clone(),
            }
        })
        .collect();
    let agents = crate::agent_settings::read(root).ok().map(|s| Agents {
        default: s.default,
        disabled: s.disabled,
    });
    Bundle {
        format: FORMAT.into(),
        exported_at: now,
        groups,
        organizations: snapshot
            .organizations
            .iter()
            .map(|o| Organization {
                name: o.name.clone(),
                domains: o.domains.clone(),
                provider_organizations: o.provider_organizations.clone(),
            })
            .collect(),
        agents,
        policies: snapshot
            .policies
            .iter()
            .map(|p| RotationPolicy {
                last_switched_at: None,
                ..p.clone()
            })
            .collect(),
        inbox: crate::inbox::carried(root).map(|(origin, client_id)| Inbox { origin, client_id }),
        shell_hook: match crate::shell::map::choice(root) {
            crate::shell::map::Choice::On => Some("on".into()),
            crate::shell::map::Choice::Off => Some("off".into()),
            crate::shell::map::Choice::Default => None,
        },
        ceilings: crate::spend::read(root)
            .map(|file| {
                file.ceilings
                    .iter()
                    .filter_map(|c| match &c.scope {
                        ChainScope::Machine => Some(Ceiling {
                            group: None,
                            cents: c.cents,
                        }),
                        ChainScope::Project { pool } => Some(Ceiling {
                            group: Some(pool.clone()),
                            cents: c.cents,
                        }),
                        ChainScope::Workflow { .. } => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        signature: None,
    }
}

/// Reads a bundle the person gives; refuses anything that is not one.
pub fn parse(bytes: &[u8]) -> Result<Bundle, String> {
    if bytes.len() > MAX_BYTES {
        return Err(NOT_A_BUNDLE.into());
    }
    let bundle: Bundle = serde_json::from_slice(bytes).map_err(|_| NOT_A_BUNDLE)?;
    if bundle.format != FORMAT {
        return Err(NOT_A_BUNDLE.into());
    }
    Ok(bundle)
}
pub const NOT_A_BUNDLE: &str = "This is not a Switchboard settings file (switchboard-settings/1).";

/// One group of the bundle as it would land here.
#[derive(Serialize, Debug, PartialEq)]
pub struct PlannedGroup {
    pub name: String,
    pub pool: String,
    /// A group with this pool exists here: folders are added to it, nothing else changes.
    pub exists: bool,
    pub organization: Option<String>,
    /// SB-282: its further organizations, by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_organizations: Vec<String>,
    /// Folders found here for the group's repositories and plain folders, not yet in it.
    pub folders: Vec<String>,
    /// Repositories and plain folders with no checkout here.
    pub missing: Vec<String>,
    /// SB-268: the missing repositories on GitHub (`github.com/<owner>/<name>`), which an import
    /// with `clone` clones into its folder.
    #[serde(default)]
    pub clonable: Vec<String>,
    /// SB-267: owners whose new repositories join the group by themselves (`github.com/<owner>`).
    #[serde(default)]
    pub sources: Vec<String>,
}
#[derive(Serialize, Debug, PartialEq)]
pub struct Plan {
    pub groups: Vec<PlannedGroup>,
    /// Organizations to add here (by name; one that exists here is kept as it is).
    pub organizations: Vec<String>,
    pub agents: bool,
    pub policies: usize,
    /// SB-270: provider teams the file gives its organizations, not yet held here.
    pub provider_organizations: usize,
    /// SB-270: the Fabric Inbox key's server, and whether a key for it is saved here.
    pub inbox: Option<PlannedInbox>,
    /// SB-270: the terminal hook's choice, set only where this computer has none.
    pub shell_hook: Option<PlannedChoice>,
    /// SB-270: each spending ceiling and what import does with it.
    pub ceilings: Vec<PlannedCeiling>,
}
#[derive(Serialize, Debug, PartialEq)]
pub struct PlannedInbox {
    pub origin: String,
    /// `saved` (a key for this server is saved here), `other` (one for another server is), `none`.
    pub here: &'static str,
    /// What the person runs to save the key here; the secret is typed, never carried.
    pub command: Option<String>,
}
#[derive(Serialize, Debug, PartialEq)]
pub struct PlannedChoice {
    pub choice: String,
    /// `set` when this computer has no choice yet, else `kept` (its own choice stays).
    pub action: &'static str,
}
#[derive(Serialize, Debug, PartialEq)]
pub struct PlannedCeiling {
    /// The group's name, or none for the computer's own ceiling.
    pub group: Option<String>,
    pub cents: u32,
    /// `set` (none here), `lower` (here it is higher), `kept` (here it is lower or equal: import
    /// never raises one) or `needs_key` (the group has no key of its own here yet).
    pub action: &'static str,
}

/// What importing `bundle` would do here, finding checkouts under `under`. Reads Git files, writes
/// nothing.
pub fn plan(root: &Path, snapshot: &Snapshot, bundle: &Bundle, under: &[PathBuf]) -> Plan {
    let map = crate::roots::disk_map(under, snapshot);
    let groups = bundle
        .groups
        .iter()
        .map(|g| {
            let here = snapshot.projects.iter().find(|p| p.pool == g.pool);
            let held = |folder: &str| here.is_some_and(|p| p.folders.iter().any(|f| f == folder));
            let mut folders = Vec::new();
            let mut missing = Vec::new();
            for identity in &g.repositories {
                let found: Vec<&str> = map
                    .repositories
                    .iter()
                    .filter(|r| r.remote.as_deref() == Some(identity.as_str()))
                    .map(|r| r.folder.as_str())
                    .collect();
                if found.is_empty() {
                    missing.push(identity.clone());
                }
                for folder in found {
                    if !held(folder) && !folders.iter().any(|f| f == folder) {
                        folders.push(folder.to_owned());
                    }
                }
            }
            for plain in &g.plain_folders {
                let path = Path::new(plain);
                match path.canonicalize() {
                    Ok(real) if real.is_dir() => {
                        let real = real.to_string_lossy().into_owned();
                        if !held(&real) && !folders.contains(&real) {
                            folders.push(real);
                        }
                    }
                    _ => missing.push(plain.clone()),
                }
            }
            let clonable = missing
                .iter()
                .filter(|m| github_repository(m).is_some())
                .cloned()
                .collect();
            PlannedGroup {
                name: g.name.clone(),
                pool: g.pool.clone(),
                exists: here.is_some(),
                organization: g.organization.clone(),
                also_organizations: g.also_organizations.clone(),
                folders,
                missing,
                clonable,
                sources: g.sources.clone(),
            }
        })
        .collect();
    let organizations = bundle
        .organizations
        .iter()
        .filter(|o| {
            !snapshot
                .organizations
                .iter()
                .any(|h| h.name.trim().eq_ignore_ascii_case(o.name.trim()))
        })
        .map(|o| o.name.clone())
        .collect();
    let held: Vec<&String> = snapshot
        .organizations
        .iter()
        .flat_map(|o| &o.provider_organizations)
        .collect();
    let provider_organizations = bundle
        .organizations
        .iter()
        .flat_map(|o| &o.provider_organizations)
        .filter(|k| !held.contains(k))
        .count();
    let inbox = bundle.inbox.as_ref().and_then(|i| {
        let origin = crate::inbox::origin(&i.origin)?;
        if !crate::inbox::client_id_valid(&i.client_id) {
            return None;
        }
        let here = match crate::inbox::carried(root) {
            Some((saved, _)) if saved == origin => "saved",
            Some(_) => "other",
            None => "none",
        };
        let command = (here != "saved").then(|| {
            format!(
                "switchboard inbox set --origin {origin} --client-id {}",
                i.client_id
            )
        });
        Some(PlannedInbox {
            origin,
            here,
            command,
        })
    });
    let shell_hook = bundle
        .shell_hook
        .as_deref()
        .filter(|c| matches!(*c, "on" | "off"))
        .map(|choice| PlannedChoice {
            choice: choice.to_owned(),
            action: match crate::shell::map::choice(root) {
                crate::shell::map::Choice::Default => "set",
                _ => "kept",
            },
        });
    let file = crate::spend::read(root).unwrap_or_default();
    let ceilings = bundle
        .ceilings
        .iter()
        .filter(|c| (crate::spend::MIN_CENTS..=crate::spend::MAX_CENTS).contains(&c.cents))
        .filter_map(|c| {
            let (name, scope) = match &c.group {
                None => (None, Some(ChainScope::Machine)),
                Some(pool) => {
                    let name = bundle.groups.iter().find(|g| &g.pool == pool)?.name.clone();
                    let here = snapshot.projects.iter().find(|p| &p.pool == pool);
                    (
                        Some(name),
                        here.map(|p| ChainScope::Project {
                            pool: p.pool.clone(),
                        }),
                    )
                }
            };
            let action = match &scope {
                Some(ChainScope::Project { pool })
                    if !crate::spend::group_has_own_key(snapshot, pool) =>
                {
                    "needs_key"
                }
                None => "needs_key",
                Some(scope) => match file.ceilings.iter().find(|h| &h.scope == scope) {
                    None => "set",
                    Some(h) if h.cents > c.cents => "lower",
                    Some(_) => "kept",
                },
            };
            Some(PlannedCeiling {
                group: name,
                cents: c.cents,
                action,
            })
        })
        .collect();
    Plan {
        groups,
        organizations,
        agents: bundle.agents.is_some(),
        policies: bundle.policies.len(),
        provider_organizations,
        inbox,
        shell_hook,
        ceilings,
    }
}

/// Applies `plan` (made from `bundle` here). Organizations first, then groups (a new one empty of
/// accounts: the person gives it this machine's seats), the agents' settings and the rotation
/// policies; a policy for the ordinary Claude Code lands switched off, so an import never starts
/// changing this machine's own sign-in. Each part that is refused is named; the rest stays.
pub fn apply(store: &Store, root: &Path, bundle: &Bundle, plan: &Plan) -> Value {
    apply_with(store, root, bundle, plan, None, &|_, _, _| {
        Err(crate::github::GH_FAILED.to_string())
    })
}

/// A repository identity on GitHub as `(owner, name)`.
fn github_repository(identity: &str) -> Option<(&str, &str)> {
    let rest = identity.strip_prefix("github.com/")?;
    let (owner, name) = rest.split_once('/')?;
    (crate::github::owner_valid(owner) && !name.is_empty() && !name.contains('/'))
        .then_some((owner, name))
}

/// Clones one repository (`owner`, `name`) into a folder; answers the checkout's folder.
pub type Cloner<'a> = dyn Fn(&str, &str, &Path) -> Result<PathBuf, String> + 'a;

/// [`apply`], cloning each group's [`PlannedGroup::clonable`] repositories into `clone_into` first
/// (SB-268) with `clone`, so a new member's Mac with no checkout yet gets its organization's
/// groups. A clone that fails is named; the rest goes on.
pub fn apply_with(
    store: &Store,
    root: &Path,
    bundle: &Bundle,
    plan: &Plan,
    clone_into: Option<&Path>,
    clone: &Cloner,
) -> Value {
    let mut refused: Vec<String> = Vec::new();
    let mut cloned = 0;
    for o in &bundle.organizations {
        if plan.organizations.contains(&o.name)
            && store.add_organization(&o.name, &o.domains).is_err()
        {
            refused.push(format!("organization {}", o.name));
        }
    }
    let snapshot = match store.snapshot() {
        Ok(s) => s,
        Err(e) => return json!({"applied": false, "error": e}),
    };
    let org_id = |name: &str| {
        snapshot
            .organizations
            .iter()
            .find(|o| o.name.trim().eq_ignore_ascii_case(name.trim()))
            .map(|o| o.id.clone())
    };
    let mut groups = 0;
    // SB-267: where each of the bundle's pools landed here, so its policies follow.
    let mut pools: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for g in &plan.groups {
        let mut planned = g.folders.clone();
        if let Some(into) = clone_into {
            for identity in &g.clonable {
                let Some((owner, name)) = github_repository(identity) else {
                    continue;
                };
                match clone(owner, name, into) {
                    Ok(folder) => {
                        cloned += 1;
                        planned.push(folder.to_string_lossy().into_owned());
                    }
                    Err(_) => refused.push(format!("clone {identity}")),
                }
            }
        }
        if planned.is_empty() {
            continue;
        }
        let here = snapshot.projects.iter().find(|p| p.pool == g.pool);
        let result = match here {
            Some(p) => {
                let mut folders: Vec<PathBuf> = p.folders.iter().map(PathBuf::from).collect();
                folders.extend(planned.iter().map(PathBuf::from));
                let accounts: Vec<String> = snapshot
                    .accounts
                    .iter()
                    .filter(|a| snapshot.serves(a, &p.pool))
                    .map(|a| a.id.clone())
                    .collect();
                store.save_project_in(Some(&p.pool), &p.name, &folders, &accounts, None)
            }
            None => {
                let folders: Vec<PathBuf> = planned.iter().map(PathBuf::from).collect();
                let orgs: Vec<String> = g
                    .organization
                    .iter()
                    .chain(&g.also_organizations)
                    .filter_map(|name| org_id(name))
                    .collect();
                // A new group takes its pool from its name here (`pool_from_name`).
                store.save_project_with(None, &g.name, &folders, &[], Some(orgs), true)
            }
        };
        match result {
            Ok(project) => {
                groups += 1;
                pools.insert(g.pool.clone(), project.pool.clone());
                if !g.sources.is_empty() {
                    let mut sources = project.sources.clone();
                    for source in &g.sources {
                        if !sources.contains(source) {
                            sources.push(source.clone());
                        }
                    }
                    if store.set_project_sources(&project.pool, &sources).is_err() {
                        refused.push(format!("sources of {}", g.name));
                    }
                }
            }
            Err(_) => refused.push(format!("group {}", g.name)),
        }
    }
    if let Some(agents) = &bundle.agents {
        let mut ok = crate::agent_settings::set_default(root, agents.default.as_deref()).is_ok();
        for id in &agents.disabled {
            ok &= crate::agent_settings::set_enabled(root, id, false).is_ok();
        }
        if !ok {
            refused.push("agents".into());
        }
    }
    let mut policies = 0;
    let here: Vec<String> = store
        .snapshot()
        .map(|s| s.projects.into_iter().map(|p| p.pool).collect())
        .unwrap_or_default();
    for p in &bundle.policies {
        let mut p = p.clone();
        p.last_switched_at = None;
        // SB-267: a policy follows its group to the pool it landed in; one whose group is not
        // here is left out rather than naming a pool that does not exist.
        if let Some(pool) = pools.get(&p.pool) {
            p.pool = pool.clone();
        } else if p.pool != "default" && !here.contains(&p.pool) {
            refused.push(format!("a rotation policy of {}", p.pool));
            continue;
        }
        if p.target == "claude_cli" {
            p.enabled = false;
        }
        match store.set_policy(p) {
            Ok(()) => policies += 1,
            Err(_) => refused.push("a rotation policy".into()),
        }
    }
    // SB-270: the organizations' provider teams, so their seats signed in here follow them.
    let mut provider_organizations = 0;
    if let Ok(current) = store.snapshot() {
        for o in &bundle.organizations {
            if o.provider_organizations.is_empty() {
                continue;
            }
            let Some(id) = current
                .organizations
                .iter()
                .find(|h| h.name.trim().eq_ignore_ascii_case(o.name.trim()))
                .map(|h| h.id.clone())
            else {
                continue;
            };
            match store.learn_provider_organizations(&id, &o.provider_organizations) {
                Ok(added) => provider_organizations += added,
                Err(_) => refused.push(format!("provider teams of {}", o.name)),
            }
        }
    }
    // SB-270: the terminal hook's choice, only where this computer has none; the installed app
    // brings the start files in line with it at its next start.
    let mut shell_hook = None;
    if let Some(planned) = plan.shell_hook.as_ref().filter(|p| p.action == "set") {
        match crate::shell::install::set_choice(root, planned.choice == "on") {
            Ok(()) => shell_hook = Some(planned.choice.clone()),
            Err(_) => refused.push("terminal hook".into()),
        }
    }
    // SB-270: spending ceilings, only ever set where there is none or lowered: import never
    // raises one (the CLI's rule, C-Q1). A group's needs its own key here first.
    let mut ceilings = 0;
    if let Ok(current) = store.snapshot() {
        let file = crate::spend::read(root).unwrap_or_default();
        for c in &bundle.ceilings {
            let (label, scope) = match &c.group {
                None => ("this computer".to_owned(), ChainScope::Machine),
                Some(pool) => {
                    let Some(group) = bundle.groups.iter().find(|g| &g.pool == pool) else {
                        continue;
                    };
                    let landed = pools.get(pool).cloned().or_else(|| {
                        current
                            .projects
                            .iter()
                            .any(|p| &p.pool == pool)
                            .then(|| pool.clone())
                    });
                    let Some(landed) = landed else {
                        refused.push(format!("ceiling of {}", group.name));
                        continue;
                    };
                    (group.name.clone(), ChainScope::Project { pool: landed })
                }
            };
            if file
                .ceilings
                .iter()
                .any(|h| h.scope == scope && h.cents <= c.cents)
            {
                continue;
            }
            match crate::spend::set_ceiling(
                root,
                &current,
                scope,
                Some(f64::from(c.cents) / 100.0),
                crate::spend::Surface::Cli,
                crate::monitor::now(),
            ) {
                Ok(_) => ceilings += 1,
                Err(e) if e == crate::spend::NO_OWN_KEY => {
                    refused.push(format!("ceiling of {label} (it needs its own key here)"))
                }
                Err(_) => refused.push(format!("ceiling of {label}")),
            }
        }
    }
    let inbox = plan.inbox.as_ref().and_then(|i| i.command.clone());
    json!({
        "applied": true,
        "groups": groups,
        "cloned": cloned,
        "policies": policies,
        "provider_organizations": provider_organizations,
        "shell_hook": shell_hook,
        "ceilings": ceilings,
        "inbox_command": inbox,
        "refused": refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_store(root: &Path) -> Store {
        Store::open(
            root.to_path_buf(),
            std::sync::Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap()
    }

    #[test]
    fn a_bundle_round_trips_and_names_no_account() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let app = work.join("app");
        std::fs::create_dir_all(app.join(".git")).unwrap();
        std::fs::write(
            app.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:o/app.git\n",
        )
        .unwrap();
        let notes = work.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        // SB-281: the app's data folder sits under it in the tree and still travels.
        let app_data = work.join("app-data");
        std::fs::create_dir_all(&app_data).unwrap();
        let store = open_store(root.path());
        let acme = store
            .add_organization("Acme", &["acme.example".into()])
            .unwrap();
        store
            .save_project_in(
                None,
                "Alpha",
                &[app.clone(), notes.clone(), app_data.clone()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        let bundle = export(root.path(), &store.snapshot().unwrap(), 5);
        assert_eq!(bundle.groups[0].repositories, vec!["github.com/o/app"]);
        assert_eq!(bundle.groups[0].organization.as_deref(), Some("Acme"));
        assert_eq!(
            bundle.groups[0].plain_folders,
            vec![
                app_data.to_string_lossy().into_owned(),
                notes.to_string_lossy().into_owned()
            ]
        );
        let text = serde_json::to_vec(&bundle).unwrap();
        assert_eq!(
            serde_json::to_value(parse(&text).unwrap()).unwrap(),
            serde_json::to_value(&bundle).unwrap()
        );
        assert!(!String::from_utf8_lossy(&text).contains("synthetic"));
        assert_eq!(parse(b"{}").unwrap_err(), NOT_A_BUNDLE);
        assert_eq!(
            parse(br#"{"format":"other/1","exported_at":1}"#).unwrap_err(),
            NOT_A_BUNDLE
        );

        // Another machine: its checkout of the repository sits elsewhere.
        let other = tempfile::tempdir().unwrap();
        let there = tempfile::tempdir().unwrap();
        let there = there.path().canonicalize().unwrap();
        let clone = there.join("org").join("app");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        std::fs::write(
            clone.join(".git/config"),
            "[remote \"origin\"]\n\turl = https://github.com/o/app\n",
        )
        .unwrap();
        let fresh = open_store(other.path());
        let preview = plan(
            other.path(),
            &fresh.snapshot().unwrap(),
            &bundle,
            std::slice::from_ref(&there),
        );
        assert_eq!(preview.organizations, vec!["Acme"]);
        let group = &preview.groups[0];
        assert!(!group.exists);
        assert_eq!(group.folders[0], clone.to_string_lossy());
        // The plain folder exists on this test machine at its path, so it is found too.
        assert!(group.missing.is_empty(), "{:?}", group.missing);
        // Nothing is written by a preview.
        assert!(fresh.snapshot().unwrap().projects.is_empty());
        let done = apply(&fresh, other.path(), &bundle, &preview);
        assert_eq!(done["groups"], 1, "{done}");
        let snap = fresh.snapshot().unwrap();
        assert_eq!(snap.projects[0].name, "Alpha");
        assert!(!snap.projects[0].organizations.is_empty());
        assert_eq!(snap.organizations[0].name, "Acme");
    }

    /// A checkout folder as `gh repo clone` leaves it, with its `origin`.
    fn checkout(folder: &Path, remote: &str) {
        std::fs::create_dir_all(folder.join(".git")).unwrap();
        std::fs::write(
            folder.join(".git/config"),
            format!("[remote \"origin\"]\n\turl = {remote}\n"),
        )
        .unwrap();
    }

    #[test]
    fn an_import_keeps_a_groups_sources_and_its_policies_follow_its_pool() {
        // SB-267: the GitHub owner a group takes new repositories from travels, and a policy of a
        // group whose pool is taken here by another name lands on the group's new pool.
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        checkout(&work.join("app"), "https://github.com/acme/app");
        let policy = |pool: &str| RotationPolicy {
            provider: switchboard_core::Provider::Claude,
            pool: pool.into(),
            target: "managed".into(),
            enabled: true,
            threshold_percent: 90.0,
            hysteresis_percent: 5.0,
            cooldown_seconds: 300,
            max_age_seconds: 600,
            last_switched_at: None,
        };
        let bundle = Bundle {
            format: FORMAT.into(),
            exported_at: 1,
            groups: vec![Group {
                also_organizations: Vec::new(),
                name: "Acme".into(),
                pool: "acme-old".into(),
                organization: None,
                repositories: vec!["github.com/acme/app".into()],
                plain_folders: vec![],
                sources: vec!["github.com/acme".into()],
            }],
            organizations: vec![],
            agents: None,
            policies: vec![policy("acme-old"), policy("elsewhere")],
            inbox: None,
            shell_hook: None,
            ceilings: vec![],
            signature: None,
        };
        let root = tempfile::tempdir().unwrap();
        let store = open_store(root.path());
        let preview = plan(
            root.path(),
            &store.snapshot().unwrap(),
            &bundle,
            std::slice::from_ref(&work),
        );
        assert_eq!(preview.groups[0].sources, vec!["github.com/acme"]);
        let done = apply(&store, root.path(), &bundle, &preview);
        let snap = store.snapshot().unwrap();
        let group = &snap.projects[0];
        assert_eq!(group.sources, vec!["github.com/acme"]);
        assert_eq!(done["policies"], 1, "{done}");
        assert!(
            snap.policies
                .iter()
                .any(|p| p.pool == group.pool && p.enabled),
            "the policy follows the group to {}",
            group.pool
        );
        assert!(!snap
            .policies
            .iter()
            .any(|p| p.pool == "acme-old" || p.pool == "elsewhere"));
        assert_eq!(done["refused"], json!(["a rotation policy of elsewhere"]));
    }

    #[test]
    fn a_new_members_mac_clones_the_groups_repositories_and_gets_the_group() {
        // SB-268: nothing is checked out yet; the plan names what can be cloned, and an import
        // that clones makes the organization's group from the clones.
        let bundle = Bundle {
            format: FORMAT.into(),
            exported_at: 1,
            groups: vec![Group {
                also_organizations: Vec::new(),
                name: "Acme".into(),
                pool: "acme".into(),
                organization: Some("Acme".into()),
                repositories: vec![
                    "github.com/acme/app".into(),
                    "github.com/acme/broken".into(),
                    "gitlab.com/acme/other".into(),
                ],
                plain_folders: vec![],
                sources: vec!["github.com/acme".into()],
            }],
            organizations: vec![Organization {
                name: "Acme".into(),
                domains: vec!["acme.example".into()],
                provider_organizations: vec![],
            }],
            agents: None,
            policies: vec![],
            inbox: None,
            shell_hook: None,
            ceilings: vec![],
            signature: None,
        };
        let root = tempfile::tempdir().unwrap();
        let into = tempfile::tempdir().unwrap();
        let into = into.path().canonicalize().unwrap();
        let store = open_store(root.path());
        let preview = plan(
            root.path(),
            &store.snapshot().unwrap(),
            &bundle,
            std::slice::from_ref(&into),
        );
        let group = &preview.groups[0];
        assert!(group.folders.is_empty());
        assert_eq!(
            group.clonable,
            vec!["github.com/acme/app", "github.com/acme/broken"]
        );
        // Without cloning, nothing is made: a group needs a folder.
        let fresh_root = tempfile::tempdir().unwrap();
        let fresh = open_store(fresh_root.path());
        let none = apply(&fresh, fresh_root.path(), &bundle, &preview);
        assert_eq!(none["groups"], 0, "{none}");
        // With cloning: each clone becomes a folder of the group; a failed one is named.
        let clone = |owner: &str, name: &str, into: &Path| {
            if name == "broken" {
                return Err(crate::github::CLONE_FAILED.to_string());
            }
            let folder = into.join(name);
            checkout(&folder, &format!("https://github.com/{owner}/{name}"));
            Ok(folder)
        };
        let done = apply_with(&store, root.path(), &bundle, &preview, Some(&into), &clone);
        assert_eq!(
            (&done["groups"], &done["cloned"]),
            (&json!(1), &json!(1)),
            "{done}"
        );
        assert_eq!(done["refused"], json!(["clone github.com/acme/broken"]));
        let snap = store.snapshot().unwrap();
        let made = &snap.projects[0];
        assert_eq!(
            made.folders,
            vec![into.join("app").to_string_lossy().into_owned()]
        );
        assert_eq!(made.sources, vec!["github.com/acme"]);
        assert!(!made.organizations.is_empty());
    }

    #[test]
    fn the_inbox_server_teams_hook_and_ceilings_travel_and_import_never_raises_a_ceiling() {
        // SB-270: one computer carries what the person set up; the other gets it without the
        // Inbox secret, keeps its own terminal choice and never ends up with a higher ceiling.
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        checkout(&work.join("app"), "https://github.com/acme/app");
        let old_root = tempfile::tempdir().unwrap();
        let old = open_store(old_root.path());
        let acme = old.add_organization("Acme", &[]).unwrap();
        assert_eq!(
            old.learn_provider_organizations(&acme.id, &["claude:team-1".into(), "bad".into()])
                .unwrap(),
            1
        );
        let alpha = old
            .save_project_in(
                None,
                "Alpha",
                &[work.join("app")],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        old.set_agent_key_for(
            "openrouter",
            Some(&alpha.pool),
            "sk-or-v1-synthetic-a",
            None,
        )
        .unwrap();
        crate::inbox::set(
            old_root.path(),
            &old,
            "https://inbox.example.com",
            "client-id-synthetic",
            "secret-synthetic-never-carried",
        )
        .unwrap();
        crate::shell::install::set_choice(old_root.path(), false).unwrap();
        let snap = old.snapshot().unwrap();
        for (scope, usd) in [
            (ChainScope::Machine, 5.0),
            (
                ChainScope::Project {
                    pool: alpha.pool.clone(),
                },
                3.0,
            ),
        ] {
            crate::spend::set_ceiling(
                old_root.path(),
                &snap,
                scope,
                Some(usd),
                crate::spend::Surface::Window,
                1,
            )
            .unwrap();
        }
        let bundle = export(old_root.path(), &old.snapshot().unwrap(), 2);
        let text = String::from_utf8(serde_json::to_vec(&bundle).unwrap()).unwrap();
        assert!(!text.contains("secret-synthetic"), "{text}");
        assert!(!text.contains("sk-or-v1"), "{text}");
        let bundle = parse(text.as_bytes()).unwrap();
        assert_eq!(
            bundle.inbox,
            Some(Inbox {
                origin: "https://inbox.example.com".into(),
                client_id: "client-id-synthetic".into(),
            })
        );
        assert_eq!(
            bundle.organizations[0].provider_organizations,
            vec!["claude:team-1"]
        );
        assert_eq!(bundle.shell_hook.as_deref(), Some("off"));
        assert_eq!(
            bundle.ceilings,
            vec![
                Ceiling {
                    group: None,
                    cents: 500
                },
                Ceiling {
                    group: Some(alpha.pool.clone()),
                    cents: 300
                },
            ]
        );

        // The other computer: a lower ceiling of its own, its own terminal choice, no key yet.
        let new_root = tempfile::tempdir().unwrap();
        let new = open_store(new_root.path());
        crate::spend::set_ceiling(
            new_root.path(),
            &new.snapshot().unwrap(),
            ChainScope::Machine,
            Some(2.0),
            crate::spend::Surface::Window,
            1,
        )
        .unwrap();
        crate::shell::install::set_choice(new_root.path(), true).unwrap();
        let preview = plan(
            new_root.path(),
            &new.snapshot().unwrap(),
            &bundle,
            std::slice::from_ref(&work),
        );
        assert_eq!(preview.provider_organizations, 1);
        let inbox = preview.inbox.as_ref().unwrap();
        assert_eq!(
            (inbox.origin.as_str(), inbox.here),
            ("https://inbox.example.com", "none")
        );
        assert_eq!(
            inbox.command.as_deref(),
            Some("switchboard inbox set --origin https://inbox.example.com --client-id client-id-synthetic")
        );
        assert_eq!(
            preview.shell_hook,
            Some(PlannedChoice {
                choice: "off".into(),
                action: "kept"
            })
        );
        assert_eq!(
            preview.ceilings,
            vec![
                PlannedCeiling {
                    group: None,
                    cents: 500,
                    action: "kept"
                },
                PlannedCeiling {
                    group: Some("Alpha".into()),
                    cents: 300,
                    action: "needs_key"
                },
            ]
        );
        let done = apply(&new, new_root.path(), &bundle, &preview);
        assert_eq!(done["provider_organizations"], 1, "{done}");
        assert_eq!(done["shell_hook"], Value::Null, "{done}");
        assert_eq!(done["ceilings"], 0, "{done}");
        assert_eq!(done["inbox_command"], json!(inbox.command), "{done}");
        assert_eq!(
            done["refused"],
            json!(["ceiling of Alpha (it needs its own key here)"]),
        );
        // Nothing of the Inbox key is saved without its secret; the hook and ceiling stay.
        assert_eq!(crate::inbox::carried(new_root.path()), None);
        assert_eq!(
            crate::shell::map::choice(new_root.path()),
            crate::shell::map::Choice::On
        );
        let file = crate::spend::read(new_root.path()).unwrap();
        assert_eq!(file.ceilings.len(), 1);
        assert_eq!(file.ceilings[0].cents, 200);
        let snap = new.snapshot().unwrap();
        assert_eq!(
            snap.organizations[0].provider_organizations,
            vec!["claude:team-1"]
        );

        // Once the group has its key, a second import sets its ceiling; a choice-less computer
        // takes the hook's choice; a lower file ceiling lowers this one.
        let pool = snap.projects[0].pool.clone();
        new.set_agent_key_for("openrouter", Some(&pool), "sk-or-v1-synthetic-b", None)
            .unwrap();
        std::fs::remove_file(new_root.path().join(crate::shell::map::CHOICE)).unwrap();
        let mut lower = bundle.clone();
        lower.ceilings[0].cents = 100;
        let preview = plan(
            new_root.path(),
            &new.snapshot().unwrap(),
            &lower,
            std::slice::from_ref(&work),
        );
        assert_eq!(
            preview
                .ceilings
                .iter()
                .map(|c| c.action)
                .collect::<Vec<_>>(),
            vec!["lower", "set"]
        );
        let done = apply(&new, new_root.path(), &lower, &preview);
        assert_eq!(
            (&done["ceilings"], &done["shell_hook"]),
            (&json!(2), &json!("off")),
            "{done}"
        );
        let file = crate::spend::read(new_root.path()).unwrap();
        let cents = |scope: &ChainScope| {
            file.ceilings
                .iter()
                .find(|c| &c.scope == scope)
                .map(|c| c.cents)
        };
        assert_eq!(cents(&ChainScope::Machine), Some(100));
        assert_eq!(cents(&ChainScope::Project { pool }), Some(300));
        assert_eq!(
            crate::shell::map::choice(new_root.path()),
            crate::shell::map::Choice::Off
        );
    }

    #[test]
    fn a_signed_file_reads_as_signed_after_the_trip_and_a_changed_one_does_not() {
        // SB-272: what `settings export --sign` writes and `settings import` reads back.
        let root = tempfile::tempdir().unwrap();
        let store = open_store(root.path());
        store
            .add_organization("Acme", &["acme.example".into()])
            .unwrap();
        let mut bundle = export(root.path(), &store.snapshot().unwrap(), 7);
        let message = signed_message(&bundle);
        bundle.signature =
            Some(crate::settings_signing::sign(root.path(), &store, &message).unwrap());
        let key = bundle.signature.as_ref().unwrap().key.clone();
        // The file as the person carries it: JSON, read back by another computer.
        let text = serde_json::to_vec_pretty(&bundle).unwrap();
        let read = parse(&text).unwrap();
        let check = |b: &Bundle, signer: Option<&str>| {
            crate::settings_signing::verify(&signed_message(b), b.signature.as_ref(), signer)
        };
        let verdict = check(&read, Some(&key));
        assert_eq!((verdict.state, verdict.expected), ("signed", Some(true)));
        let mut changed = read.clone();
        changed.organizations[0].domains = vec!["evil.example".into()];
        assert_eq!(check(&changed, None).state, "invalid");
        let mut unsigned = read;
        unsigned.signature = None;
        assert_eq!(check(&unsigned, None).state, "unsigned");
        assert_eq!(
            crate::settings_signing::refusal(&check(&unsigned, Some(&key))),
            Some(crate::settings_signing::NOT_SIGNED_BY)
        );
    }
}
