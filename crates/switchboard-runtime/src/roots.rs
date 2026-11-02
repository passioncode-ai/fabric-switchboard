//! SB-135 (operator 2026-10-16, again 2026-10-18: «сделать главную, а остальные со звёздочкой, все
//! ворктри»): a project's folders shown as roots with the folders derived from them. A derived
//! folder is a linked worktree of a root's repository, or a clone with the same `origin`; it
//! belongs to the root's project and is listed under it, not beside it. The project still stores
//! a flat folder list; the tree is read from Git when it is asked for. A module checkout (a Git
//! submodule) goes under the repository that holds it, as `module`.
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Derived {
    pub folder: String,
    /// `worktree`, `clone` or `module` (a submodule of the root's repository).
    pub kind: &'static str,
    pub branch: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub folder: String,
    pub branch: Option<String>,
    pub derived: Vec<Derived>,
    /// SB-146: the `origin` as `host/owner/repo`, so two roots of one repository read as one
    /// project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// SB-155: the project's name as the person sees it in their folders: the main checkout's
    /// folder name (a worktree's own folder never names the project), else the folder's own.
    pub name: String,
}

/// SB-155: the name of the project a folder belongs to: its own folder name for a main checkout
/// or a folder outside Git; for a worktree, the folder of its main checkout (the parent of the
/// common `.git`), or a bare repository's folder without its `.git`.
pub(crate) fn project_name(folder: &str, checkout: Option<&Checkout>) -> String {
    let leaf = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let own = || leaf(Path::new(folder)).unwrap_or_else(|| folder.to_owned());
    match checkout {
        Some(c) if !c.main => {
            if c.common.file_name().is_some_and(|n| n == ".git") {
                c.common.parent().and_then(leaf).unwrap_or_else(own)
            } else {
                leaf(&c.common)
                    .map(|n| n.trim_end_matches(".git").to_owned())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(own)
            }
        }
        _ => own(),
    }
}

/// What Git says about one folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub common: PathBuf,
    pub remote: Option<String>,
    pub main: bool,
    pub branch: Option<String>,
    /// For a module checkout (a submodule), the common Git folder of the repository holding it.
    pub module_of: Option<PathBuf>,
}

/// The checkout a folder is, if it is one (its top, not a subfolder of it).
pub(crate) fn checkout(dir: &Path) -> Option<Checkout> {
    let repo = crate::suggest::repo(dir)?;
    let head = std::fs::read_to_string(repo.gitdir.join("HEAD")).ok();
    let branch = head
        .as_deref()
        .and_then(|h| h.trim().strip_prefix("ref: refs/heads/"))
        .map(str::to_owned);
    let module_of = superproject(&repo.common);
    Some(Checkout {
        common: repo.common,
        remote: repo.remote,
        main: repo.main,
        branch,
        module_of,
    })
}
/// The common Git folder of the repository a submodule's Git folder belongs to: Git keeps a
/// submodule's folder at `<git folder>/modules/<name>` (a name may hold slashes, and a module of a
/// linked worktree sits under that worktree's own Git folder). None for any other folder.
fn superproject(common: &Path) -> Option<PathBuf> {
    let mut at = common.parent();
    while let Some(dir) = at {
        if dir.file_name().is_some_and(|n| n == "modules") {
            let gitdir = dir.parent()?;
            if gitdir.join("HEAD").is_file() && gitdir.join("config").is_file() {
                return Some(match std::fs::read_to_string(gitdir.join("commondir")) {
                    Ok(text) => {
                        let path = PathBuf::from(text.trim());
                        let path = if path.is_absolute() {
                            path
                        } else {
                            gitdir.join(path)
                        };
                        path.canonicalize().unwrap_or(path)
                    }
                    Err(_) => gitdir.to_path_buf(),
                });
            }
        }
        at = dir.parent();
    }
    None
}

/// A project's folders as roots and their derived folders, in the order the roots first appear.
pub fn tree(folders: &[String]) -> Vec<Root> {
    let found: Vec<(String, Option<Checkout>)> = folders
        .iter()
        .map(|f| (f.clone(), checkout(Path::new(f))))
        .collect();
    group(&found)
}

/// The grouping itself, on what Git said (`checkout`): the folders of one repository go under its
/// main checkout (else the first of them listed); then repositories with the same `origin` go
/// under the first whose root is a main checkout, as clones. A folder that is no checkout is a
/// root of its own.
pub(crate) fn group(folders: &[(String, Option<Checkout>)]) -> Vec<Root> {
    // One group per repository, in order of first appearance.
    struct Group {
        root: usize,
        members: Vec<usize>,
        remote: Option<String>,
        main: bool,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut alone: Vec<usize> = Vec::new();
    for (i, (_, checkout)) in folders.iter().enumerate() {
        let Some(c) = checkout else {
            alone.push(i);
            continue;
        };
        match groups
            .iter_mut()
            .find(|g| folders[g.root].1.as_ref().map(|x| &x.common) == Some(&c.common))
        {
            Some(g) => {
                g.members.push(i);
                if c.main && !g.main {
                    g.members.push(g.root);
                    g.members.retain(|&m| m != i);
                    g.root = i;
                    g.main = true;
                }
            }
            None => groups.push(Group {
                root: i,
                members: Vec::new(),
                remote: c.remote.clone(),
                main: c.main,
            }),
        }
    }
    // A repository that is a module of another listed repository goes under that one's root.
    let module_of: Vec<Option<usize>> = groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let outer = folders[g.root].1.as_ref()?.module_of.as_ref()?;
            groups.iter().enumerate().position(|(oi, o)| {
                oi != gi && folders[o.root].1.as_ref().map(|c| &c.common) == Some(outer)
            })
        })
        .collect();
    let derived = |i: usize, kind: &'static str| Derived {
        folder: folders[i].0.clone(),
        kind,
        branch: folders[i].1.as_ref().and_then(|c| c.branch.clone()),
    };
    let mut roots: Vec<(usize, Root)> = Vec::new();
    let mut taken = vec![false; groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        if taken[gi] || module_of[gi].is_some() {
            continue;
        }
        taken[gi] = true;
        let mut root = Root {
            folder: folders[g.root].0.clone(),
            branch: folders[g.root].1.as_ref().and_then(|c| c.branch.clone()),
            derived: g.members.iter().map(|&m| derived(m, "worktree")).collect(),
            remote: g.remote.clone(),
            name: project_name(&folders[g.root].0, folders[g.root].1.as_ref()),
        };
        if let Some(remote) = &g.remote {
            // Clones of the same repository: only under a main checkout, and only once.
            for (oi, other) in groups.iter().enumerate().skip(gi + 1) {
                if taken[oi] || other.remote.as_ref() != Some(remote) || !g.main {
                    continue;
                }
                taken[oi] = true;
                root.derived.push(derived(other.root, "clone"));
                root.derived
                    .extend(other.members.iter().map(|&m| derived(m, "worktree")));
            }
        }
        for (mi, module) in groups.iter().enumerate() {
            if module_of[mi] == Some(gi) {
                root.derived.push(derived(module.root, "module"));
                root.derived
                    .extend(module.members.iter().map(|&m| derived(m, "module")));
            }
        }
        // A root keeps the place of its repository's first listed folder.
        let first = g
            .members
            .iter()
            .copied()
            .chain([g.root])
            .min()
            .unwrap_or(g.root);
        roots.push((first, root));
    }
    roots.extend(alone.into_iter().map(|i| {
        let root = Root {
            folder: folders[i].0.clone(),
            branch: None,
            derived: Vec::new(),
            remote: None,
            name: project_name(&folders[i].0, None),
        };
        (i, root)
    }));
    roots.sort_by_key(|(first, _)| *first);
    roots.into_iter().map(|(_, root)| root).collect()
}

/// The folders under `root` in `roots` (derived ones), for taking a root out with its tree.
pub fn derived_of<'a>(roots: &'a [Root], root: &str) -> Vec<&'a str> {
    roots
        .iter()
        .find(|r| r.folder == root)
        .map(|r| r.derived.iter().map(|d| d.folder.as_str()).collect())
        .unwrap_or_default()
}

/// SB-135 (part 2): one repository on disk and where it belongs.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub folder: String,
    /// The `origin` as `host/owner/repo`, when there is one.
    pub remote: Option<String>,
    pub branch: Option<String>,
    /// The project's pool and name; none: no project holds it.
    pub pool: Option<String>,
    pub project: Option<String>,
    /// `folder`: it is one of the project's folders; `inside`: it sits inside one, so it moves
    /// with that folder and not on its own.
    pub place: Option<&'static str>,
    /// SB-146: every group that holds it (as itself or around it), in creation order.
    pub groups: Vec<GroupRef>,
    /// SB-135: for a module checkout (a submodule), the folder of the repository holding it; it
    /// moves with that repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_of: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct GroupRef {
    pub pool: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DiskMap {
    pub under: Vec<String>,
    pub repositories: Vec<Found>,
    /// More was there than the walk looks at (`MAP_REPOSITORIES`, `MAP_FOLDERS`).
    pub truncated: bool,
}

/// How deep below each chosen folder the walk looks for repositories (`~/DATA/<org>/<repo>`).
const MAP_DEPTH: usize = 3;
/// At most this many repositories are listed, and this many folders looked at.
const MAP_REPOSITORIES: usize = 2000;
const MAP_FOLDERS: usize = 20_000;
/// Folders that hold dependencies, build output or app data, never a person's repositories.
const MAP_SKIP: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "build",
    "dist",
    "Library",
    "Pods",
    "DerivedData",
];

/// Where the map looks when no folder is chosen: `~/DATA` when it exists, else the home folder.
pub fn default_under() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    let data = home.join("DATA");
    Some(if data.is_dir() { data } else { home })
}

/// The main checkouts under `under` (a linked worktree is listed under its root on the project
/// page, not here), each followed by the module checkouts its `.gitmodules` names. The walk does
/// not go into a repository otherwise, nor into a hidden folder or `MAP_SKIP`.
pub fn disk_map(under: &[PathBuf], snapshot: &switchboard_core::Snapshot) -> DiskMap {
    let mut starts: Vec<PathBuf> = under.iter().filter_map(|u| u.canonicalize().ok()).collect();
    starts.sort();
    starts.dedup();
    // A chosen folder inside another chosen one is walked once.
    let starts: Vec<PathBuf> = starts
        .iter()
        .filter(|s| !starts.iter().any(|o| o != *s && s.starts_with(o)))
        .cloned()
        .collect();
    let mut repositories = Vec::new();
    let mut truncated = false;
    let mut visited = 0usize;
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        starts.iter().map(|s| (s.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MAP_FOLDERS || repositories.len() >= MAP_REPOSITORIES {
            truncated = true;
            break;
        }
        let dotgit = dir.join(".git");
        if let Ok(meta) = std::fs::symlink_metadata(&dotgit) {
            if meta.is_dir() {
                repositories.push(found(&dir, snapshot));
                for module in modules(&dir) {
                    if repositories.len() >= MAP_REPOSITORIES {
                        truncated = true;
                        break;
                    }
                    let mut row = found(&module, snapshot);
                    row.module_of = Some(dir.to_string_lossy().into_owned());
                    repositories.push(row);
                }
            }
            // A chosen folder is walked even when it is a repository itself (`~/DATA` with a
            // `.git` of its own, 2026-10-19: the map listed one row instead of hundreds).
            if depth > 0 {
                continue;
            }
        }
        if depth >= MAP_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && !MAP_SKIP.contains(&name.as_ref())
            })
            .map(|e| e.path())
            .collect();
        children.sort();
        queue.extend(children.into_iter().map(|c| (c, depth + 1)));
    }
    repositories.sort_by(|a, b| a.folder.cmp(&b.folder));
    DiskMap {
        under: starts
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect(),
        repositories,
        truncated,
    }
}

/// The module checkouts of the repository at `dir`: each `path` its `.gitmodules` names that holds
/// a checkout (its `.git` a file) inside `dir`. One level: a module's own modules are not read.
fn modules(dir: &Path) -> Vec<PathBuf> {
    /// A `.gitmodules` larger than this is not read.
    const GITMODULES_LIMIT: u64 = 256 * 1024;
    let file = dir.join(".gitmodules");
    if std::fs::symlink_metadata(&file).map_or(true, |m| !m.is_file() || m.len() > GITMODULES_LIMIT)
    {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for line in text.lines() {
        let Some(path) = line
            .trim()
            .strip_prefix("path")
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('='))
            .map(str::trim)
        else {
            continue;
        };
        let relative = Path::new(path);
        // Only a path inside the repository: no `..`, no absolute path.
        if path.is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        let module = dir.join(relative);
        if std::fs::symlink_metadata(module.join(".git")).is_ok_and(|m| m.is_file())
            && !found.contains(&module)
        {
            found.push(module);
        }
    }
    found
}

fn found(dir: &Path, snapshot: &switchboard_core::Snapshot) -> Found {
    let c = checkout(dir);
    let text = dir.to_string_lossy().into_owned();
    let project = snapshot.project_for(dir);
    let place = project.map(|p| {
        if p.folders.iter().any(|f| f == &text) {
            "folder"
        } else {
            "inside"
        }
    });
    Found {
        folder: text,
        remote: c.as_ref().and_then(|c| c.remote.clone()),
        branch: c.and_then(|c| c.branch),
        pool: project.map(|p| p.pool.clone()),
        project: project.map(|p| p.name.clone()),
        place,
        groups: snapshot
            .projects_for(dir)
            .into_iter()
            .map(|p| GroupRef {
                pool: p.pool.clone(),
                name: p.name.clone(),
            })
            .collect(),
        module_of: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn co(common: &str, remote: Option<&str>, main: bool, branch: &str) -> Option<Checkout> {
        Some(Checkout {
            common: PathBuf::from(common),
            remote: remote.map(str::to_owned),
            main,
            branch: Some(branch.to_owned()),
            module_of: None,
        })
    }

    #[test]
    fn a_project_is_named_by_its_main_checkouts_folder_not_its_remote_or_a_worktree() {
        // SB-155 (operator 2026-10-20): projects read as the folders in ~/DATA.
        let main = co(
            "/d/project-observatory-open-source/.git",
            Some("github.com/o/project-observatory-dashboard"),
            true,
            "main",
        );
        assert_eq!(
            project_name("/d/project-observatory-open-source", main.as_ref()),
            "project-observatory-open-source"
        );
        let wt = co(
            "/d/asset-foundry/.git",
            Some("github.com/o/asset-foundry"),
            false,
            "w3",
        );
        assert_eq!(
            project_name("/d/_worktrees/asset-foundry-w3-fix", wt.as_ref()),
            "asset-foundry"
        );
        let bare = co("/srv/repos/tool.git", None, false, "x");
        assert_eq!(project_name("/d/_wt/tool-x", bare.as_ref()), "tool");
        assert_eq!(project_name("/d/notes", None), "notes");
        // A group holding only a worktree names it by its main checkout.
        let roots = group(&[("/d/_worktrees/asset-foundry-w3-fix".into(), wt)]);
        assert_eq!(roots[0].name, "asset-foundry");
    }

    #[test]
    fn worktrees_and_clones_go_under_their_main_checkout() {
        // The operator's example: the bot and its worktree in `.worktrees`, listed as peers.
        let bot = "/d/vitrual-phone-number-bot/.git";
        let folders = vec![
            (
                "/d/.worktrees/bot-card".to_owned(),
                co(bot, Some("gh/bot"), false, "card"),
            ),
            ("/d/notes".to_owned(), None),
            (
                "/d/vitrual-phone-number-bot".to_owned(),
                co(bot, Some("gh/bot"), true, "main"),
            ),
            (
                "/d/bot-copy".to_owned(),
                co("/d/bot-copy/.git", Some("gh/bot"), true, "dev"),
            ),
            (
                "/d/web".to_owned(),
                co("/d/web/.git", Some("gh/web"), true, "main"),
            ),
        ];
        let roots = group(&folders);
        let names: Vec<&str> = roots.iter().map(|r| r.folder.as_str()).collect();
        assert_eq!(names, ["/d/vitrual-phone-number-bot", "/d/notes", "/d/web"]);
        assert_eq!(roots[0].branch.as_deref(), Some("main"));
        assert_eq!(
            roots[0].derived,
            [
                Derived {
                    folder: "/d/.worktrees/bot-card".into(),
                    kind: "worktree",
                    branch: Some("card".into())
                },
                Derived {
                    folder: "/d/bot-copy".into(),
                    kind: "clone",
                    branch: Some("dev".into())
                },
            ]
        );
        assert!(roots[1].derived.is_empty() && roots[2].derived.is_empty());
        assert_eq!(derived_of(&roots, "/d/vitrual-phone-number-bot").len(), 2);
        assert!(derived_of(&roots, "/d/web").is_empty());
    }

    #[test]
    fn without_its_main_checkout_a_repository_keeps_its_first_folder_as_root() {
        let common = "/d/app/.git";
        let folders = vec![
            ("/d/wt-a".to_owned(), co(common, None, false, "a")),
            ("/d/wt-b".to_owned(), co(common, None, false, "b")),
            // Same origin, but neither is a main checkout: no clone is guessed.
            ("/d/other".to_owned(), co("/d/other/.git", None, false, "x")),
        ];
        let roots = group(&folders);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].folder, "/d/wt-a");
        assert_eq!(roots[0].derived.len(), 1);
        assert_eq!(roots[0].derived[0].folder, "/d/wt-b");
    }

    #[test]
    #[cfg(unix)]
    fn a_real_repository_and_its_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let main = base.join("app");
        let git = |args: &[&str], dir: &Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        std::fs::create_dir_all(&main).unwrap();
        git(&["init", "-q", "-b", "main"], &main);
        git(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.test",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
            &main,
        );
        let wt = base.join("wt");
        git(
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                wt.to_str().unwrap(),
            ],
            &main,
        );
        let text = |p: &Path| p.to_string_lossy().into_owned();
        let roots = tree(&[text(&wt), text(&main)]);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].folder, text(&main));
        assert_eq!(roots[0].branch.as_deref(), Some("main"));
        assert_eq!(roots[0].derived[0].branch.as_deref(), Some("feature"));
    }

    /// SB-135 (the part left after 0.6.45): a module checkout goes under the repository that
    /// holds it, in a project's tree and on the disk map, read from real Git.
    #[test]
    #[cfg(unix)]
    fn a_module_checkout_goes_under_the_repository_that_holds_it() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let git = |args: &[&str], dir: &Path| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@e.test"])
                .args(["-c", "protocol.file.allow=always"])
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        let (lib, app) = (base.join("lib"), base.join("org/app"));
        for dir in [&lib, &app] {
            std::fs::create_dir_all(dir).unwrap();
            git(&["init", "-q", "-b", "main"], dir);
            git(&["commit", "-q", "--allow-empty", "-m", "x"], dir);
        }
        git(
            &[
                "submodule",
                "add",
                "-q",
                lib.to_str().unwrap(),
                "vendor/lib",
            ],
            &app,
        );
        let module = app.join("vendor/lib");
        let text = |p: &Path| p.to_string_lossy().into_owned();
        // In a project's folders, listed before its repository: still under it.
        let roots = tree(&[text(&module), text(&app), text(&lib)]);
        let folders: Vec<&str> = roots.iter().map(|r| r.folder.as_str()).collect();
        assert_eq!(folders, [text(&app), text(&lib)]);
        assert_eq!(roots[0].derived.len(), 1);
        assert_eq!(roots[0].derived[0].folder, text(&module));
        assert_eq!(roots[0].derived[0].kind, "module");
        assert_eq!(roots[0].name, "app");
        // Without the repository holding it, a module is a root of its own.
        let alone = tree(&[text(&module)]);
        assert_eq!(alone.len(), 1);
        assert!(alone[0].derived.is_empty());
        // On the disk map: right after its repository, with where it belongs.
        let snapshot = switchboard_core::Snapshot {
            projects: vec![switchboard_core::Project {
                pool: "p".into(),
                name: "P".into(),
                folders: vec![text(&app)],
                created_at: 1,
                sources: vec![],
                organization: None,
            }],
            ..Default::default()
        };
        let map = disk_map(std::slice::from_ref(&base), &snapshot);
        let listed: Vec<(String, Option<&str>, Option<String>)> = map
            .repositories
            .iter()
            .map(|r| (r.folder.clone(), r.place, r.module_of.clone()))
            .collect();
        assert_eq!(
            listed,
            [
                (text(&lib), None, None),
                (text(&app), Some("folder"), None),
                (text(&module), Some("inside"), Some(text(&app))),
            ]
        );
    }

    #[test]
    fn a_gitmodules_path_outside_its_repository_is_not_followed() {
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let app = work.join("app");
        std::fs::create_dir_all(app.join(".git")).unwrap();
        std::fs::create_dir_all(work.join("other")).unwrap();
        std::fs::write(work.join("other/.git"), "gitdir: /nowhere\n").unwrap();
        std::fs::create_dir_all(app.join("real")).unwrap();
        std::fs::write(app.join("real/.git"), "gitdir: ../.git/modules/real\n").unwrap();
        std::fs::create_dir_all(app.join("plain")).unwrap();
        std::fs::write(
            app.join(".gitmodules"),
            "[submodule \"a\"]\n\tpath = ../other\n[submodule \"b\"]\n\tpath = /etc\n\
             [submodule \"c\"]\n\tpath = plain\n[submodule \"d\"]\n\tpath = real\n",
        )
        .unwrap();
        assert_eq!(modules(&app), [app.join("real")]);
    }

    #[test]
    fn the_disk_map_lists_repositories_with_their_projects_and_skips_dependencies() {
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        let repo = |d: &Path| std::fs::create_dir_all(d.join(".git")).unwrap();
        let (app, shared, inner, loose, deep) = (
            work.join("org/app"),
            work.join("shared"),
            work.join("shared/tool"),
            work.join("loose"),
            work.join("a/b/c/d"),
        );
        for d in [&app, &inner, &loose, &deep] {
            repo(d);
        }
        repo(&work.join("node_modules/pkg"));
        repo(&work.join(".hidden/x"));
        repo(&app.join("module"));
        let snapshot = switchboard_core::Snapshot {
            projects: vec![switchboard_core::Project {
                pool: "p".into(),
                name: "P".into(),
                folders: vec![
                    app.to_string_lossy().into_owned(),
                    shared.to_string_lossy().into_owned(),
                ],
                created_at: 1,
                sources: vec![],
                organization: None,
            }],
            ..Default::default()
        };
        let map = disk_map(&[work.clone(), work.join("org")], &snapshot);
        let listed: Vec<(&str, Option<&str>)> = map
            .repositories
            .iter()
            .map(|r| (r.folder.as_str(), r.place))
            .collect();
        let text = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(
            map.under,
            [text(&work)],
            "a chosen folder inside another is walked once"
        );
        assert_eq!(
            listed,
            [
                (text(&loose).as_str(), None),
                (text(&app).as_str(), Some("folder")),
                (text(&inner).as_str(), Some("inside")),
            ]
        );
        assert_eq!(map.repositories[1].pool.as_deref(), Some("p"));
        assert!(!map.truncated);
    }

    #[test]
    fn a_chosen_folder_that_is_a_repository_itself_is_still_walked() {
        let work = tempfile::tempdir().unwrap();
        let work = work.path().canonicalize().unwrap();
        for d in [work.clone(), work.join("app"), work.join("app/module")] {
            std::fs::create_dir_all(d.join(".git")).unwrap();
        }
        let map = disk_map(
            std::slice::from_ref(&work),
            &switchboard_core::Snapshot::default(),
        );
        let listed: Vec<String> = map.repositories.iter().map(|r| r.folder.clone()).collect();
        let text = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(
            listed,
            [text(&work), text(&work.join("app"))],
            "the chosen folder and the repositories in it; not into a repository below it"
        );
    }
}
