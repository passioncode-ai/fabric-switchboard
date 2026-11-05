//! SB-176 (SB-168 D-4): settings that move to another machine without a credential. Groups travel
//! by their repositories' identities (DEC-0089 item 2), so the other machine finds its own
//! checkouts of them; organizations by name and domains; the agents' settings; rotation policies.
//! Accounts, rules and fallback chains name account ids and stay behind: the other machine signs
//! in to its own seats (SB-168 D-1). Import is a preview until it is applied.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use switchboard_core::{RotationPolicy, Snapshot, Store};

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
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub pool: String,
    /// The organization's name, when the group runs on one.
    #[serde(default)]
    pub organization: Option<String>,
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
                match root.remote {
                    Some(remote) if !repositories.contains(&remote) => repositories.push(remote),
                    Some(_) => {}
                    None => plain_folders.push(root.folder),
                }
            }
            Group {
                name: p.name.clone(),
                pool: p.pool.clone(),
                organization: p.organization.as_deref().and_then(org_name),
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
}

/// What importing `bundle` would do here, finding checkouts under `under`. Reads Git files, writes
/// nothing.
pub fn plan(snapshot: &Snapshot, bundle: &Bundle, under: &[PathBuf]) -> Plan {
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
    Plan {
        groups,
        organizations,
        agents: bundle.agents.is_some(),
        policies: bundle.policies.len(),
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
                    .filter(|a| a.pool == p.pool)
                    .map(|a| a.id.clone())
                    .collect();
                store.save_project_in(Some(&p.pool), &p.name, &folders, &accounts, None)
            }
            None => {
                let folders: Vec<PathBuf> = planned.iter().map(PathBuf::from).collect();
                let org = g.organization.as_deref().and_then(org_id);
                // A new group takes its pool from its name here (`pool_from_name`).
                store.save_project_in(None, &g.name, &folders, &[], org.as_deref().map(Some))
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
    json!({"applied": true, "groups": groups, "cloned": cloned, "policies": policies, "refused": refused})
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
        let store = open_store(root.path());
        let acme = store
            .add_organization("Acme", &["acme.example".into()])
            .unwrap();
        store
            .save_project_in(
                None,
                "Alpha",
                &[app.clone(), notes.clone()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        let bundle = export(root.path(), &store.snapshot().unwrap(), 5);
        assert_eq!(bundle.groups[0].repositories, vec!["github.com/o/app"]);
        assert_eq!(bundle.groups[0].organization.as_deref(), Some("Acme"));
        assert_eq!(
            bundle.groups[0].plain_folders,
            vec![notes.to_string_lossy().into_owned()]
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
        assert!(snap.projects[0].organization.is_some());
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
        };
        let root = tempfile::tempdir().unwrap();
        let store = open_store(root.path());
        let preview = plan(
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
            }],
            agents: None,
            policies: vec![],
        };
        let root = tempfile::tempdir().unwrap();
        let into = tempfile::tempdir().unwrap();
        let into = into.path().canonicalize().unwrap();
        let store = open_store(root.path());
        let preview = plan(
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
        assert!(made.organization.is_some());
    }
}
