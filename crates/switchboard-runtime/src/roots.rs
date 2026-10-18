//! SB-135 (operator 2026-10-16, again 2026-10-18: «сделать главную, а остальные со звёздочкой, все
//! ворктри»): a project's folders shown as roots with the folders derived from them. A derived
//! folder is a linked worktree of a root's repository, or a clone with the same `origin`; it
//! belongs to the root's project and is listed under it, not beside it. The project still stores
//! a flat folder list; the tree is read from Git when it is asked for.
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Derived {
    pub folder: String,
    /// `worktree` or `clone`.
    pub kind: &'static str,
    pub branch: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub folder: String,
    pub branch: Option<String>,
    pub derived: Vec<Derived>,
}

/// What Git says about one folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub common: PathBuf,
    pub remote: Option<String>,
    pub main: bool,
    pub branch: Option<String>,
}

/// The checkout a folder is, if it is one (its top, not a subfolder of it).
pub(crate) fn checkout(dir: &Path) -> Option<Checkout> {
    let repo = crate::suggest::repo(dir)?;
    let head = std::fs::read_to_string(repo.gitdir.join("HEAD")).ok();
    let branch = head
        .as_deref()
        .and_then(|h| h.trim().strip_prefix("ref: refs/heads/"))
        .map(str::to_owned);
    Some(Checkout {
        common: repo.common,
        remote: repo.remote,
        main: repo.main,
        branch,
    })
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
    let derived = |i: usize, kind: &'static str| Derived {
        folder: folders[i].0.clone(),
        kind,
        branch: folders[i].1.as_ref().and_then(|c| c.branch.clone()),
    };
    let mut roots: Vec<(usize, Root)> = Vec::new();
    let mut taken = vec![false; groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        if taken[gi] {
            continue;
        }
        taken[gi] = true;
        let mut root = Root {
            folder: folders[g.root].0.clone(),
            branch: folders[g.root].1.as_ref().and_then(|c| c.branch.clone()),
            derived: g.members.iter().map(|&m| derived(m, "worktree")).collect(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn co(common: &str, remote: Option<&str>, main: bool, branch: &str) -> Option<Checkout> {
        Some(Checkout {
            common: PathBuf::from(common),
            remote: remote.map(str::to_owned),
            main,
            branch: Some(branch.to_owned()),
        })
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
}
