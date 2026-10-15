//! SB-117: a project finds its new folders itself (operator request 2026-10-14). A folder is
//! the project's when it is a git worktree of a project repository, a clone with the same
//! `origin`, or a repository of one of the project's sources (`github.com/<owner>`). SB-121
//! (operator, later the same day): such a folder joins its project at once, without a question,
//! and the person gets one notification. `<data>/project-suggestions.json` (0600) remembers every
//! folder it found: `added` (a folder the person later removes from the project is not added
//! again), `pending` (adding it failed — the person answers, as before) or `dismissed` (declined,
//! never offered again). Only git
//! metadata (`.git`, `commondir`, the `origin` url) is read; no file of the repository is.
//! SB-131 (operator, 2026-10-15): a repository an agent of the project changes joins it too
//! (`worked`, found by `activity`), nothing is ever asked, and a project folder deleted from disk
//! leaves its project by itself (`gone`: remembered only for the one notification, so a folder
//! that comes back is found again).
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};
use switchboard_core::Snapshot;

pub(crate) const FILE: &str = "project-suggestions.json";
/// Open questions at once; a found folder beyond this waits for the next pass.
const MAX_PENDING: usize = 256;
/// Declined folders remembered. Past this many the oldest declined folder may be offered again.
const MAX_DISMISSED: usize = 2048;
const MAX_ROWS: usize = MAX_PENDING + MAX_DISMISSED;
/// A folder path longer than this is never suggested (projects allow 1024 bytes).
const MAX_FOLDER: usize = 1024;
const MAX_LABEL: usize = 300;
/// 2304 rows of at most ~1.3 KiB each fit with room to spare; `write` refuses beyond it.
const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Repositories without an `origin` yet, watched for one (D-3c): a new repository usually gets
/// its remote a little after `git init`.
const MAX_UNSETTLED: usize = 512;
const SMALL: u64 = 4096;
const CONFIG_LIMIT: u64 = 64 * 1024;
/// Entries read from one parent folder; a larger folder is read up to this many.
const MAX_ENTRIES: usize = 4096;
/// A new folder without `.git` yet (a clone in progress) is looked at again for this long.
const UNSURE_SECONDS: i64 = 600;
const UNREADABLE: &str =
    "Project suggestions could not be read. Restore the private file before continuing.";
const NOT_FOUND: &str = "No such suggestion. Read the suggestions again.";
const NOT_OURS: &str = "No project claims this folder: it is not a worktree, a clone or a source repository of exactly one project.";
const WAITING: &str = "This folder already waits for an answer in Switchboard.";
const DECLINED: &str = "This folder was declined for its project and is not offered again.";
const IN_PROJECT: &str = "This folder is already part of a project.";
const FULL: &str = "Too many folders wait for an answer. Answer some in Switchboard first.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Worktree,
    SameRepository,
    Ecosystem,
    /// SB-131: an agent working in the project changed files or committed in it.
    Worked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Pending,
    Dismissed,
    /// Added to its project by Switchboard (SB-121); `first_seen` is when.
    Added,
    /// Taken out of its project by Switchboard because the folder was deleted (SB-131);
    /// `first_seen` is when. Found again like a new folder if it comes back.
    Gone,
    /// A repository agents of two projects work in (SB-131): never added by itself again.
    Shared,
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
    if snapshot.project_for(folder).is_some() || contains_project(snapshot, folder) {
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

/// A folder around another project's folder could never join (projects never nest).
fn contains_project(snapshot: &Snapshot, folder: &Path) -> bool {
    snapshot
        .projects
        .iter()
        .flat_map(|p| p.folders.iter())
        .any(|f| Path::new(f).starts_with(folder))
}

/// Folders whose listing changes all day for reasons of their own: the home folder (Claude
/// Code rewrites `~/.claude.json` in place) and a filesystem root. A project folder directly in
/// them is still offered its worktrees, and agents' hooks still report new folders.
fn too_busy_to_watch(dir: &Path) -> bool {
    dir.parent().is_none()
        || ["HOME", "USERPROFILE"]
            .iter()
            .filter_map(std::env::var_os)
            .any(|home| {
                let home = Path::new(&home);
                home == dir || home.canonicalize().is_ok_and(|h| h == dir)
            })
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
    // Never write what `read` would refuse.
    if bytes.len() as u64 > MAX_BYTES || rows.len() > MAX_ROWS {
        return Err(FULL.into());
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

/// What `record` did with a folder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    New(Suggestion),
    Waiting,
    Declined,
    InProject,
    /// No project claims it. `settled: false`: a repository without an `origin` yet.
    NotOurs {
        settled: bool,
    },
    Full,
}
impl Outcome {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Outcome::New(_) => "suggested",
            Outcome::Waiting => "waiting",
            Outcome::Declined => "declined",
            Outcome::InProject => "in_project",
            Outcome::NotOurs { .. } => "not_ours",
            Outcome::Full => "full",
        }
    }
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Outcome::New(_) => "",
            Outcome::Waiting => WAITING,
            Outcome::Declined => DECLINED,
            Outcome::InProject => IN_PROJECT,
            Outcome::NotOurs { .. } => NOT_OURS,
            Outcome::Full => FULL,
        }
    }
}

#[cfg(test)]
thread_local! {
    static CHECK_TRANSIENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Temporary and cache folders (an agent's scratch worktree, a tool's cache clone) are never a
/// project's: they come and go, and adding them by themselves (SB-121) would fill a project with
/// folders that vanish.
pub(crate) fn transient(folder: &Path) -> bool {
    // Unit tests build their repositories in the temporary folder; one test turns this back on.
    #[cfg(test)]
    if !CHECK_TRANSIENT.with(std::cell::Cell::get) {
        return false;
    }
    let mut roots: Vec<PathBuf> = vec![
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/private/var/folders"),
        PathBuf::from("/var/tmp"),
    ];
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        roots.extend([
            home.join(".cache"),
            home.join("Library/Caches"),
            home.join("AppData/Local/Temp"),
        ]);
    }
    roots
        .iter()
        .any(|r| folder.starts_with(r) || r.canonicalize().is_ok_and(|c| folder.starts_with(c)))
}

/// A tool's own folder (SB-131): `~/Library`, `AppData`, or a hidden folder directly in the home
/// folder (`~/.claude`, `~/.codex`, a plugin cache). An agent editing its settings or a plugin's
/// clone there is not working on a project, so such a folder never joins by work. A worktree of
/// a project repository a tool keeps there (`~/.codex/worktrees/…`) still joins as a worktree.
pub(crate) fn tool_folder(folder: &Path) -> bool {
    #[cfg(test)]
    if !CHECK_TRANSIENT.with(std::cell::Cell::get) {
        return false;
    }
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return false;
    };
    let home = PathBuf::from(home);
    std::iter::once(home.clone())
        .chain(home.canonicalize().ok())
        .any(|base| {
            folder.strip_prefix(&base).is_ok_and(|rest| {
                rest.components().next().is_some_and(|c| {
                    let name = c.as_os_str().to_string_lossy();
                    name.starts_with('.') || name == "Library" || name == "AppData"
                })
            })
        })
}

/// The repository a path is in (SB-131): the nearest folder at or above it with a `.git` entry,
/// never the home folder or a filesystem root. The path itself may be gone (a deleted file):
/// the walk starts at its nearest existing ancestor.
pub(crate) fn repo_root(path: &Path) -> Option<PathBuf> {
    let home: Vec<PathBuf> = ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|h| {
            let h = PathBuf::from(h);
            let c = h.canonicalize().ok();
            std::iter::once(h).chain(c)
        })
        .collect();
    let mut at = Some(path);
    // Bounded: a path deeper than this is not a working tree anyone opens.
    for _ in 0..64 {
        let dir = at?;
        if dir.parent().is_none() || home.iter().any(|h| h == dir) {
            return None;
        }
        if dir.join(".git").symlink_metadata().is_ok() {
            return Some(dir.to_owned());
        }
        at = dir.parent();
    }
    None
}

/// Records a folder if it is a project's and not already suggested, added or declined.
pub(crate) fn record(
    root: &Path,
    snapshot: &Snapshot,
    repos: &[(String, Repo)],
    folder: &Path,
    now: i64,
) -> Result<Outcome, String> {
    let Ok(folder) = folder.canonicalize() else {
        return Ok(Outcome::NotOurs { settled: true });
    };
    let Some(text) = folder
        .to_str()
        .filter(|t| t.len() <= MAX_FOLDER)
        .map(str::to_owned)
    else {
        return Ok(Outcome::NotOurs { settled: true });
    };
    if snapshot.project_for(&folder).is_some() {
        return Ok(Outcome::InProject);
    }
    if transient(&folder) {
        return Ok(Outcome::NotOurs { settled: true });
    }
    // Decided before the list is read: most folders a pass looks at are no project's.
    let Some((pool, reason, repository)) = classify(snapshot, repos, &folder) else {
        let settled = repo(&folder).is_none_or(|r| r.remote.is_some());
        return Ok(Outcome::NotOurs { settled });
    };
    record_row(root, text, pool, reason, repository, now)
}

/// Records a repository an agent of the project `pool` changed (SB-131), with the same rules as
/// a found folder: never a temporary folder, a folder in a project or one around a project's.
pub(crate) fn record_worked(
    root: &Path,
    snapshot: &Snapshot,
    folder: &Path,
    pool: &str,
    now: i64,
) -> Result<Outcome, String> {
    let Ok(folder) = folder.canonicalize() else {
        return Ok(Outcome::NotOurs { settled: true });
    };
    let Some(text) = folder
        .to_str()
        .filter(|t| t.len() <= MAX_FOLDER)
        .map(str::to_owned)
    else {
        return Ok(Outcome::NotOurs { settled: true });
    };
    if snapshot.project_for(&folder).is_some() {
        return Ok(Outcome::InProject);
    }
    if transient(&folder)
        || tool_folder(&folder)
        || contains_project(snapshot, &folder)
        || !snapshot.projects.iter().any(|p| p.pool == pool)
    {
        return Ok(Outcome::NotOurs { settled: true });
    }
    let repository = repo(&folder)
        .and_then(|r| r.remote)
        .unwrap_or_else(|| text.clone());
    record_row(root, text, pool.to_owned(), Reason::Worked, repository, now)
}

fn record_row(
    root: &Path,
    text: String,
    pool: String,
    reason: Reason,
    repository: String,
    now: i64,
) -> Result<Outcome, String> {
    let mut rows = read(root)?;
    if let Some(i) = rows.iter().position(|r| r.folder == text) {
        match rows[i].state {
            State::Pending => return Ok(Outcome::Waiting),
            // Added once and since removed from the project by the person: their choice stands.
            State::Dismissed | State::Added | State::Shared => return Ok(Outcome::Declined),
            // Deleted and back again: a new folder like any other.
            State::Gone => {
                rows.remove(i);
            }
        }
    }
    // A label is shown, never parsed: bounded so a row stays small.
    let repository: String = repository.chars().take(MAX_LABEL).collect();
    if rows.iter().filter(|r| r.state == State::Pending).count() >= MAX_PENDING {
        return Ok(Outcome::Full);
    }
    let remembered = rows.iter().filter(|r| r.state != State::Pending).count();
    if remembered >= MAX_DISMISSED {
        if let Some(i) = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.state != State::Pending)
            .min_by_key(|(_, r)| r.first_seen)
            .map(|(i, _)| i)
        {
            rows.remove(i);
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
    Ok(Outcome::New(suggestion))
}

/// The open suggestions: still pending, the folder still there and still in no project.
pub(crate) fn pending(root: &Path, snapshot: &Snapshot) -> Result<Vec<Suggestion>, String> {
    Ok(read(root)?
        .into_iter()
        .filter(|r| {
            r.state == State::Pending
                // A row an older version recorded for a scratch folder is never offered or added.
                && !transient(Path::new(&r.folder))
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

/// Marks a folder added to its project (SB-121): remembered, so a folder the person removes from
/// the project afterwards is not added again. `now` is when it was added.
pub(crate) fn mark_added(root: &Path, folder: &str, now: i64) -> Result<(), String> {
    let mut rows = read(root)?;
    let row = rows
        .iter_mut()
        .find(|r| r.folder == folder)
        .ok_or(NOT_FOUND)?;
    row.state = State::Added;
    row.first_seen = now;
    write(root, &rows)
}

/// Folders the person took out of a project (SB-121): remembered as declined, so they are never
/// added back by themselves. A folder already remembered keeps its row.
pub(crate) fn remember_removed(
    root: &Path,
    pool: &str,
    folders: &[String],
    now: i64,
) -> Result<(), String> {
    let mut rows = read(root)?;
    for folder in folders {
        match rows.iter_mut().find(|r| &r.folder == folder) {
            Some(row) => {
                row.state = State::Dismissed;
                // Newest, so the cap below never drops the folder just declined.
                row.first_seen = now;
            }
            None => rows.push(Suggestion {
                folder: folder.clone(),
                pool: pool.to_owned(),
                reason: Reason::SameRepository,
                repository: String::new(),
                first_seen: now,
                state: State::Dismissed,
            }),
        }
    }
    // Over the cap the oldest remembered rows go first.
    while rows.len() > MAX_ROWS {
        let Some(i) = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.state != State::Pending)
            .min_by_key(|(_, r)| r.first_seen)
            .map(|(i, _)| i)
        else {
            break;
        };
        rows.remove(i);
    }
    write(root, &rows)
}

/// Folders added since `since`, newest last, for the one notification each gets.
pub(crate) fn added_since(root: &Path, since: i64) -> Result<Vec<Suggestion>, String> {
    let mut rows: Vec<Suggestion> = read(root)?
        .into_iter()
        .filter(|r| r.state == State::Added && r.first_seen >= since)
        .collect();
    rows.sort_by_key(|r| r.first_seen);
    Ok(rows)
}

/// Folders Switchboard took out of their project because they were deleted (SB-131), for the
/// one notification each gets. A folder already remembered as declined stays declined.
pub(crate) fn mark_gone(
    root: &Path,
    pool: &str,
    folders: &[String],
    now: i64,
) -> Result<(), String> {
    let mut rows = read(root)?;
    for folder in folders {
        match rows.iter_mut().find(|r| &r.folder == folder) {
            Some(row) if row.state == State::Dismissed => {}
            Some(row) => {
                row.state = State::Gone;
                row.pool = pool.to_owned();
                row.first_seen = now;
            }
            None => rows.push(Suggestion {
                folder: folder.clone(),
                pool: pool.to_owned(),
                reason: Reason::SameRepository,
                repository: String::new(),
                first_seen: now,
                state: State::Gone,
            }),
        }
    }
    while rows.len() > MAX_ROWS {
        let Some(i) = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.state != State::Pending)
            .min_by_key(|(_, r)| r.first_seen)
            .map(|(i, _)| i)
        else {
            break;
        };
        rows.remove(i);
    }
    write(root, &rows)
}

/// Remembers a repository two projects' agents work in (SB-131): it is never added by itself
/// again. A folder the person declined stays declined.
pub(crate) fn mark_shared(root: &Path, folder: &str, now: i64) -> Result<(), String> {
    let mut rows = read(root)?;
    match rows.iter_mut().find(|r| r.folder == folder) {
        Some(row) if row.state == State::Dismissed => return Ok(()),
        Some(row) => {
            row.state = State::Shared;
            row.first_seen = now;
        }
        None => {
            if rows.len() >= MAX_ROWS {
                if let Some(i) = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.state != State::Pending)
                    .min_by_key(|(_, r)| r.first_seen)
                    .map(|(i, _)| i)
                {
                    rows.remove(i);
                }
            }
            rows.push(Suggestion {
                folder: folder.to_owned(),
                pool: String::new(),
                reason: Reason::Worked,
                repository: String::new(),
                first_seen: now,
                state: State::Shared,
            });
        }
    }
    write(root, &rows)
}

/// Folders taken out of their project since `since` because they were deleted, newest last.
pub(crate) fn gone_since(root: &Path, since: i64) -> Result<Vec<Suggestion>, String> {
    let mut rows: Vec<Suggestion> = read(root)?
        .into_iter()
        .filter(|r| r.state == State::Gone && r.first_seen >= since)
        .collect();
    rows.sort_by_key(|r| r.first_seen);
    Ok(rows)
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

/// The background half (D-4, D-5): looks at the parent folder of every project folder and at
/// every project repository's worktree list, re-reading each only when its modification time
/// changed. What it has settled is remembered until the projects change; a repository without
/// an `origin` yet is looked at again when its git config changes.
#[derive(Default)]
pub(crate) struct Scanner {
    key: u64,
    stamps: HashMap<PathBuf, SystemTime>,
    seen: HashSet<PathBuf>,
    unsure: HashMap<PathBuf, i64>,
    unsettled: HashMap<PathBuf, Option<SystemTime>>,
    /// Repositories whose recording failed (a full list, an unreadable file): offered again.
    retry: HashSet<PathBuf>,
    /// Worktree lists with an unreadable entry, read once more before settling.
    incomplete: HashSet<PathBuf>,
    /// Directories read by the last pass, for the idle-budget test.
    pub(crate) reads: usize,
}

pub(crate) fn projects_key(snapshot: &Snapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in &snapshot.projects {
        p.pool.hash(&mut h);
        p.folders.hash(&mut h);
        p.sources.hash(&mut h);
    }
    h.finish()
}

fn config_stamp(folder: &Path) -> Option<SystemTime> {
    let r = repo(folder)?;
    std::fs::metadata(r.common.join("config"))
        .and_then(|m| m.modified())
        .ok()
}

impl Scanner {
    /// Both halves at once (tests; the owner runs them apart, `settle` under its transaction).
    #[cfg(test)]
    pub(crate) fn pass(&mut self, root: &Path, snapshot: &Snapshot, now: i64) -> Vec<Suggestion> {
        let candidates = self.collect(snapshot, now);
        self.settle(root, snapshot, candidates, now)
    }
    /// Lists what changed; writes nothing, so it runs outside the owner transaction.
    pub(crate) fn collect(&mut self, snapshot: &Snapshot, now: i64) -> Vec<PathBuf> {
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
            if let Some(parent) = folder.parent().filter(|p| !too_busy_to_watch(p)) {
                watched.push((parent.to_owned(), false));
            }
            // The repository's own worktree list, also when the project folder is a worktree.
            if let Some(r) = repo(folder) {
                let worktrees = r.common.join("worktrees");
                if worktrees.is_dir() {
                    watched.push((worktrees, true));
                }
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
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            self.reads += 1;
            let mut complete = !self.incomplete.contains(&dir);
            for entry in entries.flatten().take(MAX_ENTRIES) {
                let path = if is_worktrees {
                    // `.git/worktrees/<name>/gitdir` holds `<worktree>/.git`, absolute or
                    // relative to that admin folder.
                    let Some(text) = read_small(&entry.path().join("gitdir"), SMALL) else {
                        // Being written now, or broken for good: one more read, then settled.
                        if self.incomplete.insert(dir.clone()) {
                            complete = false;
                        }
                        continue;
                    };
                    let gitfile = PathBuf::from(text.trim());
                    let gitfile = if gitfile.is_absolute() {
                        gitfile
                    } else {
                        entry.path().join(gitfile)
                    };
                    // A worktree deleted without `git worktree prune` leaves its entry: skip it.
                    match gitfile.parent().and_then(|p| p.canonicalize().ok()) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                        continue;
                    }
                    entry.path()
                };
                if self.seen.contains(&path) || self.unsure.contains_key(&path) {
                    continue;
                }
                if path.join(".git").exists() {
                    candidates.push(path);
                } else {
                    // Maybe a clone still arriving: watched for its `.git` a while (below).
                    self.unsure.insert(path, now);
                }
            }
            // A folder read only in part is read again next pass.
            if complete {
                self.incomplete.remove(&dir);
                self.stamps.insert(dir, modified);
            }
        }
        // A folder without `.git` yet: a candidate once it has one; after the window, settled.
        let mut arrived = Vec::new();
        let mut expired = Vec::new();
        for (path, first) in &self.unsure {
            if path.join(".git").exists() {
                arrived.push(path.clone());
            } else if now - *first > UNSURE_SECONDS || !path.is_dir() {
                expired.push(path.clone());
            }
        }
        for path in arrived.iter().chain(expired.iter()) {
            self.unsure.remove(path);
        }
        self.seen.extend(expired);
        candidates.extend(arrived);
        // A repository without an `origin`: looked at again once its config changes; one
        // that is gone is forgotten.
        self.unsettled.retain(|path, _| path.join(".git").exists());
        let changed: Vec<PathBuf> = self
            .unsettled
            .iter()
            .filter(|(path, stamp)| config_stamp(path) != **stamp)
            .map(|(path, _)| path.clone())
            .collect();
        candidates.extend(changed);
        candidates.extend(self.retry.drain());
        candidates.sort();
        candidates.dedup();
        candidates
    }
    /// Records what `collect` found; the owner runs it under its transaction.
    pub(crate) fn settle(
        &mut self,
        root: &Path,
        snapshot: &Snapshot,
        candidates: Vec<PathBuf>,
        now: i64,
    ) -> Vec<Suggestion> {
        if candidates.is_empty() {
            return vec![];
        }
        let repos = project_repos(snapshot);
        let mut found = Vec::new();
        for path in candidates {
            if !path.join(".git").exists() {
                self.unsettled.remove(&path);
                continue;
            }
            match record(root, snapshot, &repos, &path, now) {
                Ok(Outcome::New(s)) => {
                    self.unsettled.remove(&path);
                    self.seen.insert(path);
                    found.push(s);
                }
                Ok(Outcome::NotOurs { settled: false }) => {
                    if self.unsettled.len() < MAX_UNSETTLED || self.unsettled.contains_key(&path) {
                        let stamp = config_stamp(&path);
                        self.unsettled.insert(path.clone(), stamp);
                    }
                    self.seen.insert(path);
                }
                // A full list or an unreadable file: tried again next pass.
                Ok(Outcome::Full) | Err(_) => {
                    if self.retry.len() < MAX_PENDING {
                        self.retry.insert(path);
                    }
                }
                Ok(_) => {
                    self.unsettled.remove(&path);
                    self.seen.insert(path);
                }
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
        // A folder around another project's folder (a monorepo root) can never join.
        let mono = data.join("mono");
        git(&mono, Some("git@github.com:org/mono.git"));
        let pkg = mono.join("pkg");
        git(&pkg, Some("git@github.com:org/pkg.git"));
        let monos = snapshot(&[("one", &[&a], &["github.com/org"]), ("two", &[&pkg], &[])]);
        let repos = project_repos(&monos);
        assert_eq!(
            classify(&monos, &repos, &mono),
            None,
            "contains another project"
        );
    }

    fn new(o: Outcome) -> Suggestion {
        match o {
            Outcome::New(s) => s,
            other => panic!("expected a new suggestion, got {other:?}"),
        }
    }

    #[test]
    fn temporary_and_cache_folders_never_join_a_project() {
        CHECK_TRANSIENT.with(|c| c.set(true));
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("app");
        git(&main, Some("git@github.com:org/app.git"));
        let s = snapshot(&[("p", &[&main], &[])]);
        let repos = project_repos(&s);
        let scratch = data.join("scratchpad-wt");
        git(&scratch, Some("git@github.com:org/app.git"));
        assert_eq!(
            record(root.path(), &s, &repos, &scratch, 1).unwrap(),
            Outcome::NotOurs { settled: true }
        );
        #[cfg(unix)]
        assert!(transient(Path::new(
            "/private/tmp/claude-501/x/scratchpad/wt"
        )));
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap();
        assert!(transient(&home.join(".cache").join("tool").join("clone")));
        assert!(!transient(&home.join("DATA").join("app-2")));
        // SB-131: a tool's own folder never joins by work, but a worktree a tool keeps there
        // (`~/.codex/worktrees/…`) is not temporary: it still joins as a worktree.
        assert!(!transient(&home.join(".codex").join("worktrees").join("x")));
        assert!(tool_folder(
            &home.join(".codex").join("worktrees").join("x")
        ));
        assert!(tool_folder(
            &home.join(".claude").join("plugins").join("cache")
        ));
        assert!(tool_folder(
            &home.join("Library").join("Application Support")
        ));
        assert!(!tool_folder(&home.join("DATA").join("app-2")));
        // A question an older version left for a scratch folder is never offered or added.
        CHECK_TRANSIENT.with(|c| c.set(false));
        new(record(root.path(), &s, &repos, &scratch, 2).unwrap());
        assert_eq!(pending(root.path(), &s).unwrap().len(), 1);
        CHECK_TRANSIENT.with(|c| c.set(true));
        assert!(pending(root.path(), &s).unwrap().is_empty());
        CHECK_TRANSIENT.with(|c| c.set(false));
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
        let first = new(record(root.path(), &s, &repos, &copy, 10).unwrap());
        assert_eq!(first.state, State::Pending);
        assert_eq!(
            record(root.path(), &s, &repos, &copy, 11).unwrap(),
            Outcome::Waiting
        );
        assert_eq!(pending(root.path(), &s).unwrap().len(), 1);
        dismiss(root.path(), &first.folder).unwrap();
        assert!(pending(root.path(), &s).unwrap().is_empty());
        assert_eq!(
            record(root.path(), &s, &repos, &copy, 12).unwrap(),
            Outcome::Declined
        );
        assert_eq!(
            record(root.path(), &s, &repos, &main, 12).unwrap(),
            Outcome::InProject
        );
        let plain = data.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(
            record(root.path(), &s, &repos, &plain, 12).unwrap(),
            Outcome::NotOurs { settled: true }
        );
        let fresh = data.join("fresh");
        git(&fresh, None);
        assert_eq!(
            record(root.path(), &s, &repos, &fresh, 12).unwrap(),
            Outcome::NotOurs { settled: false },
            "a repository without origin may still become one"
        );
        let other = data.join("app-3");
        git(&other, Some("git@github.com:org/app.git"));
        let second = new(record(root.path(), &s, &repos, &other, 13).unwrap());
        assert_eq!(find_pending(root.path(), &second.folder).unwrap(), second);
        mark_added(root.path(), &second.folder, 14).unwrap();
        assert_eq!(added_since(root.path(), 14).unwrap().len(), 1);
        assert!(added_since(root.path(), 15).unwrap().is_empty());
        assert!(find_pending(root.path(), &second.folder).is_err());
        // Added, then removed from the project by the person: never added again.
        assert_eq!(
            record(root.path(), &s, &repos, &other, 15).unwrap(),
            Outcome::Declined
        );
        // Once the folder is in a project the suggestion is not shown.
        let third = data.join("app-4");
        git(&third, Some("git@github.com:org/app.git"));
        new(record(root.path(), &s, &repos, &third, 14).unwrap());
        let joined = snapshot(&[("p", &[&main, &third], &[])]);
        assert!(pending(root.path(), &joined).unwrap().is_empty());
        switchboard_core::private_fs::private_write(&root.path().join(FILE), b"broken").unwrap();
        assert!(
            read(root.path()).is_err(),
            "corrupt storage is not an empty list"
        );
    }

    #[test]
    fn a_worked_repository_is_recorded_once_and_a_deleted_one_is_found_again() {
        // SB-131: work recorded for the session's project; a folder taken out because it was
        // deleted is a new folder when it comes back, unlike one the person took out.
        let data = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let (app, lib) = (work.path().join("app"), work.path().join("lib"));
        git(&app, Some("git@github.com:org/app.git"));
        git(&lib, None);
        let app_folder = app.canonicalize().unwrap();
        let s = snapshot(&[("app", &[&app_folder], &[])]);
        let first = record_worked(data.path(), &s, &lib, "app", 1).unwrap();
        let Outcome::New(row) = first else {
            panic!("{first:?}")
        };
        assert_eq!((row.reason, row.state), (Reason::Worked, State::Pending));
        assert_eq!(row.repository, row.folder, "no origin: the folder names it");
        assert_eq!(
            record_worked(data.path(), &s, &lib, "app", 2).unwrap(),
            Outcome::Waiting
        );
        assert_eq!(
            record_worked(data.path(), &s, &app, "app", 2).unwrap(),
            Outcome::InProject
        );
        assert!(matches!(
            record_worked(data.path(), &s, &lib, "nobody", 2).unwrap(),
            Outcome::Waiting | Outcome::NotOurs { .. }
        ));
        mark_gone(data.path(), "app", std::slice::from_ref(&row.folder), 3).unwrap();
        assert_eq!(gone_since(data.path(), 3).unwrap().len(), 1);
        assert!(matches!(
            record_worked(data.path(), &s, &lib, "app", 4).unwrap(),
            Outcome::New(_)
        ));
        remember_removed(data.path(), "app", std::slice::from_ref(&row.folder), 5).unwrap();
        mark_gone(data.path(), "app", std::slice::from_ref(&row.folder), 6).unwrap();
        assert_eq!(
            record_worked(data.path(), &s, &lib, "app", 7).unwrap(),
            Outcome::Declined,
            "declined by the person stays declined"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_repository_is_the_nearest_folder_with_git_and_never_the_home_folder() {
        let work = tempfile::tempdir().unwrap();
        let repo = work.path().join("app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        assert_eq!(repo_root(&repo.join("src/deep/new.rs")), Some(repo.clone()));
        assert_eq!(repo_root(&repo), Some(repo.clone()));
        assert_eq!(repo_root(&work.path().join("plain/file")), None);
    }

    #[test]
    fn the_list_never_grows_past_what_it_can_read_back() {
        let root = tempfile::tempdir().unwrap();
        let long = format!("/{}", "x".repeat(MAX_FOLDER - 10));
        let rows: Vec<Suggestion> = (0..MAX_ROWS)
            .map(|i| Suggestion {
                folder: format!("{long}{i:05}"),
                pool: "p".into(),
                reason: Reason::Ecosystem,
                repository: "github.com/org".into(),
                first_seen: i as i64,
                state: if i < MAX_PENDING {
                    State::Pending
                } else {
                    State::Dismissed
                },
            })
            .collect();
        write(root.path(), &rows).unwrap();
        assert_eq!(read(root.path()).unwrap().len(), MAX_ROWS);
        let mut more = rows.clone();
        more.push(rows[0].clone());
        assert!(
            write(root.path(), &more).is_err(),
            "never more than read accepts"
        );
    }

    #[test]
    fn a_failed_recording_is_retried_and_gone_or_plain_folders_cost_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("app");
        git(&main, Some("git@github.com:org/app.git"));
        let notes = data.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        let s = snapshot(&[("p", &[&main], &["github.com/org"])]);
        let mut scan = Scanner::default();
        assert!(scan.pass(root.path(), &s, 100).is_empty());
        // The list is unreadable while a new repository appears; once restored, it is offered.
        switchboard_core::private_fs::private_write(&root.path().join(FILE), b"broken").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let api = data.join("api");
        git(&api, Some("git@github.com:org/api.git"));
        assert!(scan.pass(root.path(), &s, 130).is_empty());
        std::fs::remove_file(root.path().join(FILE)).unwrap();
        assert_eq!(
            scan.pass(root.path(), &s, 160).len(),
            1,
            "retried without a parent change"
        );
        // A repository without an origin that is deleted is forgotten, not looked at forever.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let scratch = data.join("scratch");
        git(&scratch, None);
        assert!(scan.pass(root.path(), &s, 190).is_empty());
        std::fs::remove_dir_all(&scratch).unwrap();
        assert!(scan.collect(&s, 220).is_empty(), "gone: no candidate");
        assert!(scan.collect(&s, 250).is_empty());
        // A plain folder, once its window passed, is not listed again when the parent changes.
        assert!(scan.collect(&s, 100 + UNSURE_SECONDS + 1).is_empty());
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::create_dir_all(data.join("other")).unwrap();
        let listed = scan.collect(&s, 100 + UNSURE_SECONDS + 2);
        assert!(!listed.contains(&notes), "{listed:?}");
    }

    #[test]
    fn a_repository_that_gets_its_origin_later_and_relative_worktrees_are_found() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let main = data.join("app");
        git(&main, Some("git@github.com:org/app.git"));
        std::fs::create_dir_all(main.join(".git/worktrees")).unwrap();
        let s = snapshot(&[("p", &[&main], &["github.com/org"])]);
        let mut scan = Scanner::default();
        assert!(scan.pass(root.path(), &s, 100).is_empty());
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let service = data.join("new-service");
        git(&service, None);
        assert!(scan.pass(root.path(), &s, 130).is_empty(), "no origin yet");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(
            service.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:org/new-service.git\n",
        )
        .unwrap();
        let found = scan.pass(root.path(), &s, 160);
        assert_eq!(
            found.iter().map(|f| f.reason).collect::<Vec<_>>(),
            vec![Reason::Ecosystem]
        );
        // A worktree whose admin entry points at it by a relative path (git 2.48+).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let wt = data.join("app-rel");
        let admin = main.join(".git/worktrees/app-rel");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();
        std::fs::write(admin.join("gitdir"), "../../../../app-rel/.git\n").unwrap();
        std::fs::write(wt.join(".git"), "gitdir: ../app/.git/worktrees/app-rel\n").unwrap();
        let found = scan.pass(root.path(), &s, 190);
        assert_eq!(
            found.iter().map(|f| f.reason).collect::<Vec<_>>(),
            vec![Reason::Worktree]
        );
        // A project whose only folder is a worktree still watches its repository's worktrees.
        let only_wt = snapshot(&[("q", &[&wt], &[])]);
        let mut scan = Scanner::default();
        let root2 = tempfile::tempdir().unwrap();
        assert!(scan.pass(root2.path(), &only_wt, 200).iter().any(|f| {
            std::path::Path::new(&f.folder)
                .file_name()
                .and_then(|n| n.to_str())
                == Some("app")
        }));
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let elsewhere = data.join("deep").join("app-third");
        worktree(&main, &elsewhere, "app-third");
        let found = scan.pass(root2.path(), &only_wt, 230);
        assert!(found
            .iter()
            .any(|f| f.folder.ends_with("app-third") && f.reason == Reason::Worktree));
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
