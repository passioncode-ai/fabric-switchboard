//! SB-117: a project offers its new folders itself (operator request 2026-10-14). A folder is
//! the project's when it is a git worktree of a project repository, a clone with the same
//! `origin`, or a repository of one of the project's sources (`github.com/<owner>`). Nothing
//! joins without a person: suggestions wait in `<data>/project-suggestions.json` (0600) until
//! they are added or dismissed, and a dismissed folder is never suggested again. Only git
//! metadata (`.git`, `commondir`, the `origin` url) is read; no file of the repository is.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};
use switchboard_core::Snapshot;

pub(crate) const FILE: &str = "project-suggestions.json";
const MAX_ROWS: usize = 256;
const MAX_BYTES: u64 = 512 * 1024;
const SMALL: u64 = 4096;
const CONFIG_LIMIT: u64 = 64 * 1024;
/// Entries read from one parent folder; a larger folder is read up to this many.
const MAX_ENTRIES: usize = 4096;
/// A new folder without `.git` yet (a clone in progress) is looked at again for this long.
const UNSURE_SECONDS: i64 = 600;
const UNREADABLE: &str =
    "Project suggestions could not be read. Restore the private file before continuing.";
const NOT_FOUND: &str = "No such suggestion. Read the suggestions again.";
const NOT_A_REPOSITORY: &str = "This folder is not a git repository of any project.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Worktree,
    SameRepository,
    Ecosystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pending,
    Dismissed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suggestion {
    pub folder: String,
    pub pool: String,
    pub reason: Reason,
    /// `host/owner/repo` for a worktree or a clone, `host/owner` for a source.
    pub repository: String,
    pub first_seen: i64,
    pub state: State,
}

fn read_small(path: &Path, limit: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > limit {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// What identifies a repository: its common git folder (shared by all its worktrees) and its
/// `origin`, normalized to `host/owner/repo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Repo {
    pub common: PathBuf,
    pub remote: Option<String>,
}

pub(crate) fn repo(dir: &Path) -> Option<Repo> {
    let dotgit = dir.join(".git");
    let meta = std::fs::symlink_metadata(&dotgit).ok()?;
    let gitdir = if meta.is_dir() {
        dotgit
    } else if meta.is_file() {
        let text = read_small(&dotgit, SMALL)?;
        let line = text.lines().next()?.strip_prefix("gitdir:")?.trim();
        let path = PathBuf::from(line);
        if path.is_absolute() {
            path
        } else {
            dir.join(path)
        }
    } else {
        return None;
    };
    let common = match read_small(&gitdir.join("commondir"), SMALL) {
        Some(text) => {
            let path = PathBuf::from(text.trim());
            if path.is_absolute() {
                path
            } else {
                gitdir.join(path)
            }
        }
        None => gitdir,
    };
    let common = common.canonicalize().ok()?;
    let remote = read_small(&common.join("config"), CONFIG_LIMIT)
        .and_then(|c| origin_url(&c))
        .and_then(|u| normalize(&u));
    Some(Repo { common, remote })
}

fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
            continue;
        }
        if in_origin {
            if let Some(value) = line
                .strip_prefix("url")
                .map(str::trim_start)
                .and_then(|r| r.strip_prefix('='))
            {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

/// `git@github.com:Owner/Repo.git`, `https://user@github.com/owner/repo` and
/// `ssh://git@github.com:22/owner/repo.git` all become `github.com/owner/repo`. A local path,
/// or a path that is not owner/repo, is not a remote this matches on.
pub(crate) fn normalize(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        (authority, path)
    } else {
        let (left, path) = url.split_once(':')?;
        if left.contains('/') || left.len() < 2 {
            return None;
        }
        (left, path)
    };
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next()?.to_ascii_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if host.is_empty() || !host.contains('.') || owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!(
        "{host}/{}/{}",
        owner.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    ))
}

fn owner_of(remote: &str) -> &str {
    remote.rsplit_once('/').map_or(remote, |(owner, _)| owner)
}

/// The repositories of every project folder, read once per call.
pub(crate) fn project_repos(snapshot: &Snapshot) -> Vec<(String, Repo)> {
    snapshot
        .projects
        .iter()
        .flat_map(|p| p.folders.iter().map(move |f| (p.pool.clone(), f)))
        .filter_map(|(pool, f)| repo(Path::new(f)).map(|r| (pool, r)))
        .collect()
}

/// Which project a folder belongs to, and why: the strongest match wins (worktree, then clone,
/// then source); two projects matching equally strongly is no answer (D-7).
pub(crate) fn classify(
    snapshot: &Snapshot,
    repos: &[(String, Repo)],
    folder: &Path,
) -> Option<(String, Reason, String)> {
    if snapshot.project_for(folder).is_some() {
        return None;
    }
    let candidate = repo(folder)?;
    let mut best: Option<(u8, String, Reason, String)> = None;
    let mut tie = false;
    for project in &snapshot.projects {
        let mut here: Option<(u8, Reason, String)> = None;
        for (_, r) in repos.iter().filter(|(pool, _)| pool == &project.pool) {
            let label = || {
                r.remote
                    .clone()
                    .unwrap_or_else(|| r.common.to_string_lossy().into_owned())
            };
            let found = if r.common == candidate.common {
                Some((3, Reason::Worktree, label()))
            } else if r.remote.is_some() && r.remote == candidate.remote {
                Some((2, Reason::SameRepository, label()))
            } else {
                None
            };
            if let Some(f) = found {
                if here.as_ref().is_none_or(|h| f.0 > h.0) {
                    here = Some(f);
                }
            }
        }
        if here.is_none() {
            if let Some(remote) = &candidate.remote {
                let owner = owner_of(remote);
                if project.sources.iter().any(|s| s == owner) {
                    here = Some((1, Reason::Ecosystem, owner.to_owned()));
                }
            }
        }
        let Some((rank, reason, label)) = here else {
            continue;
        };
        match &best {
            Some((top, pool, ..)) if *top == rank && pool != &project.pool => tie = true,
            Some((top, ..)) if *top >= rank => {}
            _ => {
                best = Some((rank, project.pool.clone(), reason, label));
                tie = false;
            }
        }
    }
    if tie {
        return None;
    }
    best.map(|(_, pool, reason, label)| (pool, reason, label))
}

pub(crate) fn read(root: &Path) -> Result<Vec<Suggestion>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    let rows: Vec<Suggestion> = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    if rows.len() > MAX_ROWS {
        return Err(UNREADABLE.into());
    }
    Ok(rows)
}

fn write(root: &Path, rows: &[Suggestion]) -> Result<(), String> {
    let bytes = serde_json::to_vec(rows).map_err(|_| UNREADABLE)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

/// Records a folder if it is a project's and not already suggested, added or dismissed.
/// `Ok(Some)` is a new suggestion; `Ok(None)` means nothing new.
pub(crate) fn record(
    root: &Path,
    snapshot: &Snapshot,
    repos: &[(String, Repo)],
    folder: &Path,
    now: i64,
) -> Result<Option<Suggestion>, String> {
    let Ok(folder) = folder.canonicalize() else {
        return Ok(None);
    };
    let Some((pool, reason, repository)) = classify(snapshot, repos, &folder) else {
        return Ok(None);
    };
    let Some(text) = folder.to_str().map(str::to_owned) else {
        return Ok(None);
    };
    let mut rows = read(root)?;
    if rows.iter().any(|r| r.folder == text) {
        return Ok(None);
    }
    if rows.len() >= MAX_ROWS {
        // Make room by forgetting the oldest declined folder; never drop an open question.
        match rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.state == State::Dismissed)
            .min_by_key(|(_, r)| r.first_seen)
            .map(|(i, _)| i)
        {
            Some(i) => {
                rows.remove(i);
            }
            None => return Ok(None),
        }
    }
    let suggestion = Suggestion {
        folder: text,
        pool,
        reason,
        repository,
        first_seen: now,
        state: State::Pending,
    };
    rows.push(suggestion.clone());
    write(root, &rows)?;
    Ok(Some(suggestion))
}

/// The open suggestions: still pending, the folder still there and still in no project.
pub(crate) fn pending(root: &Path, snapshot: &Snapshot) -> Result<Vec<Suggestion>, String> {
    Ok(read(root)?
        .into_iter()
        .filter(|r| {
            r.state == State::Pending
                && snapshot.projects.iter().any(|p| p.pool == r.pool)
                && snapshot.project_for(Path::new(&r.folder)).is_none()
                && Path::new(&r.folder).is_dir()
        })
        .collect())
}

/// The pending suggestion for `folder`, for an Add.
pub(crate) fn find_pending(root: &Path, folder: &str) -> Result<Suggestion, String> {
    read(root)?
        .into_iter()
        .find(|r| r.folder == folder && r.state == State::Pending)
        .ok_or_else(|| NOT_FOUND.into())
}

/// Removes an added suggestion; the folder is now in its project.
pub(crate) fn forget(root: &Path, folder: &str) -> Result<(), String> {
    let mut rows = read(root)?;
    rows.retain(|r| r.folder != folder);
    write(root, &rows)
}

/// Declines a suggestion for good.
pub(crate) fn dismiss(root: &Path, folder: &str) -> Result<(), String> {
    let mut rows = read(root)?;
    let row = rows
        .iter_mut()
        .find(|r| r.folder == folder)
        .ok_or(NOT_FOUND)?;
    row.state = State::Dismissed;
    write(root, &rows)
}

/// For `switchboard project suggest`: a folder that is no project's says so.
pub(crate) fn explain_none() -> &'static str {
    NOT_A_REPOSITORY
}

/// The background half (D-4, D-5): looks at the parent folder of every project folder and at
/// every project repository's worktree list, re-reading each only when its modification time
/// changed. What it has looked at is remembered until the projects change.
#[derive(Default)]
pub(crate) struct Scanner {
    key: u64,
    stamps: HashMap<PathBuf, SystemTime>,
    seen: HashSet<PathBuf>,
    unsure: HashMap<PathBuf, i64>,
    /// Directories read by the last pass, for the idle-budget test.
    pub(crate) reads: usize,
}

fn projects_key(snapshot: &Snapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in &snapshot.projects {
        p.pool.hash(&mut h);
        p.folders.hash(&mut h);
        p.sources.hash(&mut h);
    }
    h.finish()
}

impl Scanner {
    pub(crate) fn pass(&mut self, root: &Path, snapshot: &Snapshot, now: i64) -> Vec<Suggestion> {
        self.reads = 0;
        if snapshot.projects.is_empty() {
            return vec![];
        }
        let key = projects_key(snapshot);
        if key != self.key {
            *self = Scanner {
                key,
                ..Scanner::default()
            };
        }
        let mut watched: Vec<(PathBuf, bool)> = Vec::new();
        for folder in snapshot.projects.iter().flat_map(|p| p.folders.iter()) {
            let folder = Path::new(folder);
            if let Some(parent) = folder.parent() {
                watched.push((parent.to_owned(), false));
            }
            let worktrees = folder.join(".git").join("worktrees");
            if worktrees.is_dir() {
                watched.push((worktrees, true));
            }
        }
        watched.sort();
        watched.dedup();
        let mut candidates: Vec<PathBuf> = Vec::new();
        for (dir, is_worktrees) in watched {
            let Ok(modified) = std::fs::metadata(&dir).and_then(|m| m.modified()) else {
                continue;
            };
            if self.stamps.get(&dir) == Some(&modified) {
                continue;
            }
            self.stamps.insert(dir.clone(), modified);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            self.reads += 1;
            for entry in entries.flatten().take(MAX_ENTRIES) {
                let path = if is_worktrees {
                    // `.git/worktrees/<name>/gitdir` holds `<worktree>/.git`.
                    let Some(text) = read_small(&entry.path().join("gitdir"), SMALL) else {
                        continue;
                    };
                    let gitfile = PathBuf::from(text.trim());
                    match gitfile.parent() {
                        Some(p) => p.to_owned(),
                        None => continue,
                    }
                } else {
                    if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                        continue;
                    }
                    entry.path()
                };
                if self.seen.insert(path.clone()) {
                    candidates.push(path);
                }
            }
        }
        // A folder that has no `.git` yet may be a clone still arriving.
        self.unsure
            .retain(|_, first| now - *first <= UNSURE_SECONDS);
        let unsure: Vec<PathBuf> = self.unsure.keys().cloned().collect();
        candidates.extend(unsure);
        if candidates.is_empty() {
            return vec![];
        }
        let repos = project_repos(snapshot);
        let mut found = Vec::new();
        for path in candidates {
            if !path.join(".git").exists() {
                self.unsure.entry(path).or_insert(now);
                continue;
            }
            self.unsure.remove(&path);
            if let Ok(Some(s)) = record(root, snapshot, &repos, &path, now) {
                found.push(s);
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::Project;

    fn git(dir: &Path, remote: Option<&str>) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let config = remote.map_or(String::new(), |r| {
            format!("[core]\n\tbare = false\n[remote \"origin\"]\n\turl = {r}\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n")
        });
        std::fs::write(dir.join(".git/config"), config).unwrap();
    }
    fn worktree(main: &Path, wt: &Path, name: &str) {
        let admin = main.join(".git/worktrees").join(name);
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(wt).unwrap();
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", wt.join(".git").display()),
        )
        .unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    }
    fn snapshot(projects: &[(&str, &[&Path], &[&str])]) -> Snapshot {
        Snapshot {
            projects: projects
                .iter()
                .map(|(pool, folders, sources)| Project {
                    pool: (*pool).into(),
                    name: (*pool).into(),
                    folders: folders
                        .iter()
                        .map(|f| f.canonicalize().unwrap().to_string_lossy().into_owned())
                        .collect(),
                    created_at: 1,
                    sources: sources.iter().map(|s| (*s).into()).collect(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn remotes_normalize_to_host_owner_repo() {
        for url in [
            "git@github.com:Passioncode-AI/Fabric.git",
            "https://github.com/passioncode-ai/fabric",
            "https://user@github.com/passioncode-ai/fabric.git/",
            "ssh://git@github.com:22/passioncode-ai/fabric.git",
        ] {
            assert_eq!(
                normalize(url).as_deref(),
                Some("github.com/passioncode-ai/fabric"),
                "{url}"
            );
        }
        for url in [
            "/local/path/repo",
            "../repo",
            "C:\\work\\repo",
            "https://github.com/only-owner",
            "git@host:a/b/c",
        ] {
            assert_eq!(normalize(url), None, "{url}");
        }
    }

    #[test]
    fn worktrees_clones_and_ecosystem_repositories_are_recognised() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("fabric");
        git(&main, Some("git@github.com:passioncode-ai/fabric.git"));
        let s = snapshot(&[("passioncode", &[&main], &["github.com/passioncode-ai"])]);
        let repos = project_repos(&s);
        let wt = data.join("fabric-night");
        worktree(&main, &wt, "fabric-night");
        assert_eq!(
            classify(&s, &repos, &wt).map(|c| c.1),
            Some(Reason::Worktree)
        );
        let clone = data.join("fabric-review");
        git(&clone, Some("https://github.com/passioncode-ai/fabric"));
        assert_eq!(
            classify(&s, &repos, &clone),
            Some((
                "passioncode".into(),
                Reason::SameRepository,
                "github.com/passioncode-ai/fabric".into()
            ))
        );
        let new_repo = data.join("relay");
        git(&new_repo, Some("git@github.com:passioncode-ai/relay.git"));
        assert_eq!(
            classify(&s, &repos, &new_repo),
            Some((
                "passioncode".into(),
                Reason::Ecosystem,
                "github.com/passioncode-ai".into()
            ))
        );
    }

    #[test]
    fn other_folders_ambiguous_matches_and_project_folders_are_not() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let a = data.join("a");
        git(&a, Some("git@github.com:org/shared.git"));
        let b = data.join("b");
        git(&b, Some("git@github.com:org/shared.git"));
        let s = snapshot(&[("one", &[&a], &[]), ("two", &[&b], &[])]);
        let repos = project_repos(&s);
        let third = data.join("c");
        git(&third, Some("git@github.com:org/shared.git"));
        assert_eq!(
            classify(&s, &repos, &third),
            None,
            "two projects match equally"
        );
        let other = data.join("other");
        git(&other, Some("git@github.com:someone/else.git"));
        assert_eq!(classify(&s, &repos, &other), None, "no match");
        let plain = data.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(classify(&s, &repos, &plain), None, "not a repository");
        let inside = a.join("sub");
        git(&inside, Some("git@github.com:org/shared.git"));
        assert_eq!(classify(&s, &repos, &inside), None, "already in a project");
    }

    #[test]
    fn a_suggestion_is_recorded_once_accepted_or_dismissed_for_good() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("app");
        git(&main, Some("git@github.com:org/app.git"));
        let s = snapshot(&[("p", &[&main], &[])]);
        let repos = project_repos(&s);
        let copy = data.join("app-2");
        git(&copy, Some("git@github.com:org/app.git"));
        let first = record(root.path(), &s, &repos, &copy, 10).unwrap().unwrap();
        assert_eq!(first.state, State::Pending);
        assert_eq!(
            record(root.path(), &s, &repos, &copy, 11).unwrap(),
            None,
            "no duplicate"
        );
        assert_eq!(pending(root.path(), &s).unwrap().len(), 1);
        dismiss(root.path(), &first.folder).unwrap();
        assert!(pending(root.path(), &s).unwrap().is_empty());
        assert_eq!(
            record(root.path(), &s, &repos, &copy, 12).unwrap(),
            None,
            "dismissed stays"
        );
        let other = data.join("app-3");
        git(&other, Some("git@github.com:org/app.git"));
        let second = record(root.path(), &s, &repos, &other, 13)
            .unwrap()
            .unwrap();
        assert_eq!(find_pending(root.path(), &second.folder).unwrap(), second);
        forget(root.path(), &second.folder).unwrap();
        assert!(find_pending(root.path(), &second.folder).is_err());
        // Once the folder is in a project the suggestion is not shown.
        let third = data.join("app-4");
        git(&third, Some("git@github.com:org/app.git"));
        record(root.path(), &s, &repos, &third, 14)
            .unwrap()
            .unwrap();
        let joined = snapshot(&[("p", &[&main, &third], &[])]);
        assert!(pending(root.path(), &joined).unwrap().is_empty());
        switchboard_core::private_fs::private_write(&root.path().join(FILE), b"broken").unwrap();
        assert!(
            read(root.path()).is_err(),
            "corrupt storage is not an empty list"
        );
    }

    #[test]
    fn the_scan_finds_new_siblings_and_worktrees_and_reads_nothing_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("app");
        git(&main, Some("git@github.com:org/app.git"));
        std::fs::create_dir_all(main.join(".git/worktrees")).unwrap();
        let unrelated = data.join("notes");
        std::fs::create_dir_all(&unrelated).unwrap();
        let s = snapshot(&[("p", &[&main], &["github.com/org"])]);
        let mut scan = Scanner::default();
        assert!(scan.pass(root.path(), &s, 100).is_empty());
        assert_eq!(scan.reads, 2, "the parent and the worktree list, once");
        assert!(scan.pass(root.path(), &s, 130).is_empty());
        assert_eq!(scan.reads, 0, "nothing changed: nothing read");
        // A new sibling clone of another repository of the source.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let sibling = data.join("api");
        git(&sibling, Some("git@github.com:org/api.git"));
        let found = scan.pass(root.path(), &s, 160);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, Reason::Ecosystem);
        // A new worktree elsewhere.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let wt = data.join("elsewhere").join("app-wt");
        worktree(&main, &wt, "app-wt");
        let found = scan.pass(root.path(), &s, 190);
        assert_eq!(
            found.iter().map(|f| f.reason).collect::<Vec<_>>(),
            vec![Reason::Worktree]
        );
        // A folder that gets its `.git` a little later (a clone in progress) is still found.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let slow = data.join("slow");
        std::fs::create_dir_all(&slow).unwrap();
        assert!(scan.pass(root.path(), &s, 220).is_empty());
        git(&slow, Some("git@github.com:org/slow.git"));
        assert_eq!(scan.pass(root.path(), &s, 250).len(), 1);
    }
}
