//! SB-141 part 2: the contract's folder attribution (fabric-agent-contract DEC-0089, rule
//! FAC-SEM-121, `src/folder-attribution.ts`), ported row for row of its AT table and checked against
//! its 135 attribution and 28 record vectors (`tests/fixtures/folder-attribution.json`, pinned by
//! `scripts/sync_attribution_vectors.py`). It reads no file system: a caller gathers the snapshot.
//! Like the reference it answers any JSON value and never panics; malformed input is an answer.
//!
//! The repository identity is `suggest::normalize` (part 1, the origin vectors).

use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

const MAX_PATH: usize = 4096;
const MAX_LIST: usize = 10_000;

/// The answer, the AT row that decided it, and the Project folder a product records after it.
pub struct Explained {
    pub answer: Value,
    pub row: u8,
    pub record: Option<Value>,
}

/// `attributeFolder(snapshot)`.
pub fn attribute_folder(snapshot: &Value) -> Value {
    explain_folder(snapshot).answer
}

fn within(value: &str, max: usize) -> bool {
    value.len() <= max || value.chars().count() <= max
}
fn id_ok(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && within(s, 256))
}
/// An origin member: absent, `null`, or a string of at most 4096 characters.
fn origin_ok(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => within(s, MAX_PATH),
        Some(_) => false,
    }
}
fn own<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().and_then(|o| o.get(key))
}
fn list(value: Option<&Value>) -> Option<&Vec<Value>> {
    value
        .and_then(Value::as_array)
        .filter(|a| a.len() <= MAX_LIST)
}
/// An optional list: absent is empty, anything but a list is malformed.
fn optional_list(value: Option<&Value>) -> Option<Vec<Value>> {
    match value {
        None => Some(Vec::new()),
        some => list(some).cloned(),
    }
}
fn ascii_lower(value: &str) -> String {
    value.to_ascii_lowercase()
}
fn trim_slashes(value: &str, leading: bool) -> &str {
    let value = value.trim_end_matches('/');
    if leading {
        value.trim_start_matches('/')
    } else {
        value
    }
}

static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:/|[A-Z]:/)(?:[^/]+(?:/[^/]+)*)?$").unwrap());
static DRIVE_ROOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z]:/$").unwrap());
static DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z]:$").unwrap());

/// A path: absolute, `/`-separated (`C:/...` on Windows), no empty, `.` or `..` segment, no trailing `/`.
pub fn is_path(value: &str) -> bool {
    within(value, MAX_PATH)
        && PATH.is_match(value)
        && !value.split('/').any(|s| s == "." || s == "..")
}
fn path_of(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| is_path(s))
}
fn is_root(path: &str) -> bool {
    path == "/" || DRIVE_ROOT.is_match(path)
}
/// Whether `path` is `base` or lies under it, segment by segment.
pub fn under(path: &str, base: &str) -> bool {
    if path == base {
        return true;
    }
    if is_root(base) {
        path.starts_with(base)
    } else {
        path.len() > base.len() && path.starts_with(base) && path.as_bytes()[base.len()] == b'/'
    }
}
fn parent_of(path: &str) -> String {
    let cut = path.rfind('/').unwrap_or(0);
    let parent = &path[..cut];
    if parent.is_empty() {
        "/".into()
    } else if DRIVE.is_match(parent) {
        format!("{parent}/")
    } else {
        parent.into()
    }
}
fn relative_to(path: &str, top: &str) -> String {
    if path == top {
        String::new()
    } else if is_root(top) {
        path[top.len()..].into()
    } else {
        path[top.len() + 1..].into()
    }
}
fn rel_under(rel: &str, base: &str) -> bool {
    base.is_empty()
        || rel == base
        || (rel.len() > base.len() && rel.starts_with(base) && rel.as_bytes()[base.len()] == b'/')
}
fn in_tool_folder(path: &str, home: &str) -> bool {
    if path == home || !under(path, home) {
        return false;
    }
    let rest = path[home.len()..].trim_start_matches('/');
    let first = rest.split('/').next().unwrap_or("");
    first.starts_with('.') || first == "Library" || first == "AppData"
}
fn normalize_origin(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| within(s, MAX_PATH))
        .and_then(crate::suggest::normalize)
}
/// The owner of a repository identity: every segment but the last.
pub fn owner_of(repository: &str) -> &str {
    repository
        .rfind('/')
        .map_or(repository, |i| &repository[..i])
}

// ---- secret shape (the contract's `src/secret-shape.ts`, what `recordOrigin` reads) --------

static JWT_SHAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"eyJ[A-Za-z0-9_-]{2,}\.[A-Za-z0-9_-]{2,}\.[A-Za-z0-9_-]*").unwrap()
});
static SECRET_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^bearer\s.+|sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|xox[abposr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|glpat-[A-Za-z0-9_-]{20,})").unwrap()
});
// `\bbearer\s+(?=[...]*\d)[...]{12,}`: Rust's regex has no look-ahead, so the token run is
// captured whole and its digit checked.
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbearer\s+([A-Za-z0-9._~+/-]+)").unwrap());
static LONG_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/=_-]{40,}").unwrap());
static PATH_LIKE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:[/~.]|[A-Za-z]:[\\/]|[a-z][a-z0-9+.-]*://)").unwrap());
static EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.[A-Za-z0-9]{1,8}$").unwrap());
static WORD_SEGMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{2,}$").unwrap());

fn long_secret(value: &str) -> bool {
    LONG_RUN.find_iter(value).any(|m| {
        let run = m.as_str();
        run.bytes().any(|b| b.is_ascii_lowercase())
            && run.bytes().any(|b| b.is_ascii_uppercase())
            && run.bytes().any(|b| b.is_ascii_digit())
    })
}
fn bearer_inside(value: &str) -> bool {
    BEARER.captures_iter(value).any(|c| {
        let token = &c[1];
        token.len() >= 12 && token.bytes().any(|b| b.is_ascii_digit())
    })
}
fn path_like(piece: &str) -> bool {
    PATH_LIKE.is_match(piece)
        || (piece.contains(['/', '\\'])
            && (EXTENSION.is_match(piece)
                || piece.split(['/', '\\']).any(|s| WORD_SEGMENT.is_match(s))))
}
fn secret_argument(value: &str) -> bool {
    JWT_SHAPE.is_match(value)
        || SECRET_VALUE.is_match(value)
        || bearer_inside(value)
        || value
            .split(|c: char| c.is_whitespace() || c == '=')
            .any(|piece| {
                if path_like(piece) {
                    piece.split(['/', '\\']).any(long_secret)
                } else {
                    long_secret(piece)
                }
            })
}
/// `secretUrl` for what `record_origin` builds: no query, no fragment and no password are left in
/// it, so the argument check and a password in a URL's user information are what remain to ask.
fn secret_url(value: &str) -> bool {
    if secret_argument(value) {
        return true;
    }
    let Some(i) = value.find("://") else {
        return false;
    };
    let rest = &value[i + 3..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority
        .rsplit_once('@')
        .is_some_and(|(info, _)| info.split_once(':').is_some_and(|(_, p)| !p.is_empty()))
}

static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z][A-Za-z0-9+.-]*)://").unwrap());
static SCP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:([^@/\s]+)@)?([^:/\s]+):(\S+)$").unwrap());
static LOCAL_DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]:[\\/]").unwrap());

fn cut_query(text: &str) -> &str {
    text.split(['?', '#']).next().unwrap_or("")
}
/// The origin a record keeps: the raw remote without what can carry a credential; `None` when what
/// is left still has a secret's shape, or names another repository than the raw one.
pub fn record_origin(origin: Option<&str>) -> Option<String> {
    let origin = origin?;
    let mut value = origin.trim().to_owned();
    let user_of = |info: &str| info.split(':').next().unwrap_or("").to_owned();
    if let Some(scheme) = SCHEME.captures(&value) {
        crate::suggest::normalize(&value)?;
        let prefix = scheme[0].to_owned();
        let http = scheme[1].to_ascii_lowercase().contains("http");
        let rest = cut_query(&value[prefix.len()..]).to_owned();
        let slash = rest.find('/')?;
        let authority = &rest[..slash];
        let at = authority.rfind('@');
        let user = match at {
            Some(at) if !http => user_of(&authority[..at]),
            _ => String::new(),
        };
        let host = at.map_or(authority, |at| &authority[at + 1..]);
        value = format!(
            "{prefix}{}{host}{}",
            if user.is_empty() {
                String::new()
            } else {
                format!("{user}@")
            },
            &rest[slash..]
        );
    } else if !(value.starts_with('/') || LOCAL_DRIVE.is_match(&value) || value.starts_with('.')) {
        let scp = SCP.captures(&value)?;
        crate::suggest::normalize(&value)?;
        let host = &scp[2];
        let host = host.rfind('@').map_or(host, |i| &host[i + 1..]);
        let user = scp
            .get(1)
            .map(|u| format!("{}@", user_of(u.as_str())))
            .unwrap_or_default();
        value = format!("{user}{host}:{}", cut_query(&scp[3]));
    }
    if secret_url(&value) || crate::suggest::normalize(&value) != crate::suggest::normalize(origin)
    {
        return None;
    }
    Some(value)
}

// ---- the snapshot ----------------------------------------------------------------------------

struct Folder {
    path: String,
    common: Option<String>,
    origin: Option<Value>,
    repository: std::cell::OnceCell<Option<String>>,
    rel: String,
}
impl Folder {
    fn repository(&self) -> Option<&str> {
        self.repository
            .get_or_init(|| normalize_origin(self.origin.as_ref()))
            .as_deref()
    }
}
struct Project {
    id: String,
    folders: Vec<Folder>,
    sources: Vec<String>,
}

fn projects_of(value: Option<&Value>) -> Option<Vec<Project>> {
    let raw = list(value)?;
    let mut projects = Vec::with_capacity(raw.len());
    for entry in raw {
        let id = id_ok(own(entry, "id"))?;
        let folders = list(own(entry, "folders"))?;
        let sources: Vec<Value> = optional_list(own(entry, "sources"))?;
        let mut kept_sources = Vec::with_capacity(sources.len());
        for source in &sources {
            let s = source
                .as_str()
                .filter(|s| !s.is_empty() && within(s, MAX_PATH))?;
            kept_sources.push(trim_slashes(&ascii_lower(s), false).to_owned());
        }
        let mut parsed = Vec::with_capacity(folders.len());
        for folder in folders {
            let path = path_of(own(folder, "path"))?;
            let common = own(folder, "commonDir");
            let common = match common {
                None | Some(Value::Null) => None,
                Some(v) => Some(path_of(Some(v))?.to_owned()),
            };
            let origin = own(folder, "origin");
            if !origin_ok(origin) {
                return None;
            }
            let rel = match own(folder, "toplevel") {
                None | Some(Value::Null) => String::new(),
                Some(v) => {
                    let top = path_of(Some(v))?;
                    if !under(path, top) {
                        return None;
                    }
                    relative_to(path, top)
                }
            };
            parsed.push(Folder {
                path: path.to_owned(),
                common,
                origin: origin.cloned(),
                repository: Default::default(),
                rel,
            });
        }
        projects.push(Project {
            id: id.to_owned(),
            folders: parsed,
            sources: kept_sources,
        });
    }
    let distinct: HashSet<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    (distinct.len() == projects.len()).then_some(projects)
}

/// The distinct ids, in the snapshot's order.
fn ids(projects: &[&Project]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    projects
        .iter()
        .filter(|p| seen.insert(p.id.clone()))
        .map(|p| p.id.clone())
        .collect()
}
fn decide(matches: &[&Project], rule: &str) -> Option<Value> {
    let found = ids(matches);
    match found.as_slice() {
        [] => None,
        [one] => Some(json!({"outcome": "attached", "project": one, "rule": rule})),
        many => Some(json!({"outcome": "ambiguous", "rule": rule, "projects": many})),
    }
}
/// The Projects whose folder in this repository holds the place `rel`, nearest first.
fn nearest_in<'a>(
    projects: &'a [Project],
    matches: &dyn Fn(&Folder) -> bool,
    rel: &str,
) -> (Vec<&'a Project>, String) {
    let mut nearest: isize = -1;
    let mut place = String::new();
    let mut owners: Vec<&Project> = Vec::new();
    for project in projects {
        for entry in &project.folders {
            if !matches(entry) || !rel_under(rel, &entry.rel) {
                continue;
            }
            let len = entry.rel.len() as isize;
            if len > nearest {
                nearest = len;
                place = entry.rel.clone();
                owners = vec![project];
            } else if len == nearest {
                owners.push(project);
            }
        }
    }
    (owners, place)
}

/// `explainFolder(snapshot)`: the answer in the order of the contract's AT table.
pub fn explain_folder(snapshot: &Value) -> Explained {
    explain(snapshot).unwrap_or(Explained {
        answer: json!({"fault": "input-invalid"}),
        row: 0,
        record: None,
    })
}

fn at(row: u8, answer: Value, record: Option<Value>) -> Option<Explained> {
    Some(Explained {
        answer,
        row,
        record,
    })
}
fn refused(row: u8, reason: &str) -> Option<Explained> {
    at(row, json!({"outcome": "refused", "reason": reason}), None)
}
fn record(path: &str, common: Option<&str>, origin: Option<&str>, top: Option<&str>) -> Value {
    let mut m = Map::new();
    m.insert("path".into(), json!(path));
    m.insert("commonDir".into(), json!(common));
    m.insert("origin".into(), json!(origin));
    m.insert("toplevel".into(), json!(top));
    Value::Object(m)
}

/// `None` is AT-0, the fault.
fn explain(snapshot: &Value) -> Option<Explained> {
    let folder = path_of(own(snapshot, "folder"))?;
    let home = path_of(own(snapshot, "home"))?;
    let temp_roots = list(own(snapshot, "tempRoots"))?;
    let temp_roots: Vec<&str> = temp_roots
        .iter()
        .map(|v| path_of(Some(v)))
        .collect::<Option<_>>()?;
    let projects = projects_of(own(snapshot, "projects"))?;
    let work = optional_list(own(snapshot, "work"))?;
    let tombstones = optional_list(own(snapshot, "tombstones"))?;
    let tombstones: Vec<String> = tombstones
        .iter()
        .map(|v| path_of(Some(v)).map(str::to_owned))
        .collect::<Option<_>>()?;
    let containers = optional_list(own(snapshot, "containers"))?;
    let declared: HashSet<String> = containers
        .iter()
        .map(|v| path_of(Some(v)).map(str::to_owned))
        .collect::<Option<_>>()?;
    let git = own(snapshot, "git")?;
    let git = match git {
        Value::Null => None,
        Value::Object(_) => Some(git),
        _ => return None,
    };
    let (toplevel, common_dir, main_checkout, origin) = match git {
        None => (None, None, None, None),
        Some(g) => {
            let top = path_of(own(g, "toplevel"))?;
            let common = path_of(own(g, "commonDir"))?;
            let main = path_of(own(g, "mainCheckout"))?;
            let origin = own(g, "origin");
            if !under(folder, top) || !origin_ok(origin) {
                return None;
            }
            (Some(top), Some(common), Some(main), origin)
        }
    };
    let origin_text = origin.and_then(Value::as_str);
    let mut records: Vec<(String, String, Option<Value>)> = Vec::new();
    for r in &work {
        let project = id_ok(own(r, "project"))?;
        let repo = path_of(own(r, "commonDir"))?;
        let o = own(r, "origin");
        if !origin_ok(o) {
            return None;
        }
        records.push((project.to_owned(), repo.to_owned(), o.cloned()));
    }
    let broad = |path: &str| is_root(path) || under(home, path) || declared.contains(path);
    let cut_of = |path: &str| -> Option<&str> {
        declared
            .iter()
            .filter(|c| under(path, c))
            .max_by_key(|c| c.len())
            .map(String::as_str)
    };
    let reaches = |path: &str, cut: Option<&str>, entry: &str| {
        under(path, entry) && !(cut.is_some_and(|c| c != entry && under(c, entry)))
    };
    let kept_origin = record_origin(origin_text);
    let git_record = |path: &str, top: Option<&str>| -> Option<Value> {
        (!broad(path)).then(|| record(path, common_dir, kept_origin.as_deref(), top))
    };

    if let Some(explicit) = own(snapshot, "explicit") {
        let target = id_ok(Some(explicit)).and_then(|id| projects.iter().find(|p| p.id == id))?;
        let rec = if broad(folder) {
            None
        } else if git.is_none() {
            Some(record(folder, None, None, None))
        } else {
            git_record(folder, toplevel)
        };
        return at(
            1,
            json!({"outcome": "attached", "project": target.id, "rule": "explicit"}),
            rec,
        );
    }

    let root = main_checkout.unwrap_or(folder);
    let listed = |path: &str| {
        projects
            .iter()
            .any(|p| p.folders.iter().any(|e| e.path == path))
    };
    let loose_under_home = |path: &str| parent_of(path) == home && !listed(path);
    let by_path = |path: &str| -> Vec<&Project> {
        let cut = cut_of(path);
        let mut nearest: isize = -1;
        let mut owners: Vec<&Project> = Vec::new();
        for project in &projects {
            for entry in &project.folders {
                if !reaches(path, cut, &entry.path) || broad(&entry.path) {
                    continue;
                }
                let len = entry.path.len() as isize;
                if len > nearest {
                    nearest = len;
                    owners = vec![project];
                } else if len == nearest {
                    owners.push(project);
                }
            }
        }
        owners
    };
    let repository = git.and(normalize_origin(origin));
    let place = toplevel.map_or(String::new(), |top| relative_to(folder, top));
    let same_common = |e: &Folder| {
        git.is_some()
            && !broad(&e.path)
            && (e.common.as_deref() == common_dir || Some(e.path.as_str()) == main_checkout)
    };
    let same_identity = |e: &Folder| {
        repository.is_some() && !broad(&e.path) && e.repository() == repository.as_deref()
    };
    let same_repository = |e: &Folder| same_common(e) || same_identity(e);
    let any_folder =
        |test: &dyn Fn(&Folder) -> bool| projects.iter().any(|p| p.folders.iter().any(test));

    // AT-2
    let folder_cut = cut_of(folder);
    let mut deepest: Option<&str> = None;
    for project in &projects {
        for entry in &project.folders {
            if !broad(&entry.path)
                && reaches(folder, folder_cut, &entry.path)
                && deepest.is_none_or(|d| entry.path.len() > d.len())
            {
                deepest = Some(&entry.path);
            }
        }
    }
    let stoned = tombstones.iter().any(|stone| {
        under(folder, stone) && !deepest.is_some_and(|d| d != stone && under(d, stone))
    });
    if stoned {
        return refused(2, "tombstoned");
    }
    // AT-3
    if any_folder(&|e| same_repository(e) && reaches(folder, folder_cut, &e.path)) {
        let common = nearest_in(&projects, &same_common, &place);
        let identity = nearest_in(&projects, &same_identity, &place);
        let nearer = if common.0.is_empty() || identity.1.len() > common.1.len() {
            &identity
        } else {
            &common
        };
        if let Some(answer) = decide(&nearer.0, "folder") {
            return at(3, answer, None);
        }
    }
    if let Some(answer) = decide(&by_path(folder), "folder") {
        return at(3, answer, None);
    }
    // AT-4
    if temp_roots
        .iter()
        .any(|temp| under(folder, temp) || under(root, temp))
    {
        return refused(4, "transient");
    }
    // AT-5
    let loose_root = loose_under_home(root)
        && !any_folder(&|e| same_repository(e) && (toplevel != Some(root) || under(&e.path, root)));
    let worktree_top_loose = toplevel
        .is_some_and(|top| top != root && (loose_under_home(top) || declared.contains(top)));
    if broad(folder) || broad(root) || loose_under_home(folder) || worktree_top_loose || loose_root
    {
        return refused(5, "container");
    }
    if let (Some(top), Some(common_dir)) = (toplevel, common_dir) {
        let at_place = |found: &(Vec<&Project>, String), row: u8, rule: &str| {
            decide(&found.0, rule).map(|answer| {
                let rec = if answer["outcome"] == "attached" {
                    let path = if found.1.is_empty() {
                        top.to_owned()
                    } else if is_root(top) {
                        format!("{top}{}", found.1)
                    } else {
                        format!("{top}/{}", found.1)
                    };
                    git_record(&path, Some(top))
                } else {
                    None
                };
                Explained {
                    answer,
                    row,
                    record: rec,
                }
            })
        };
        // AT-6, AT-7
        let common = nearest_in(&projects, &same_common, &place);
        let identity = nearest_in(&projects, &same_identity, &place);
        let identity_nearer = common.0.is_empty() || identity.1.len() > common.1.len();
        let by_place = if identity_nearer {
            at_place(&identity, 7, "repository")
        } else {
            at_place(&common, 6, "worktree")
        };
        if by_place.is_some() {
            return by_place;
        }
        let whole = |row: u8, answer: Option<Value>| {
            answer.map(|answer| {
                let rec = if answer["outcome"] == "attached" {
                    git_record(top, Some(top))
                } else {
                    None
                };
                Explained {
                    answer,
                    row,
                    record: rec,
                }
            })
        };
        // AT-8
        if let Some(e) = whole(8, decide(&by_path(root), "worktree")) {
            return Some(e);
        }
        // AT-9
        if any_folder(&same_repository) {
            return refused(9, "container");
        }
        // AT-10
        if let Some(repo) = &repository {
            let owner = owner_of(repo);
            let sources: Vec<&Project> = projects
                .iter()
                .filter(|p| p.sources.iter().any(|s| s == owner))
                .collect();
            if let Some(e) = whole(10, decide(&sources, "owner")) {
                return Some(e);
            }
        }
        // AT-11
        if in_tool_folder(folder, home) || in_tool_folder(root, home) {
            return refused(11, "tool");
        }
        // AT-12
        let workers: HashSet<&str> = records
            .iter()
            .filter(|(_, common, o)| {
                common == common_dir
                    || (repository.is_some() && normalize_origin(o.as_ref()) == repository)
            })
            .map(|(p, _, _)| p.as_str())
            .collect();
        let by_work: Vec<&Project> = projects
            .iter()
            .filter(|p| workers.contains(p.id.as_str()))
            .collect();
        if let Some(e) = whole(12, decide(&by_work, "work")) {
            return Some(e);
        }
    } else if in_tool_folder(folder, home) {
        return refused(11, "tool");
    }
    // AT-13
    if tombstones.iter().any(|stone| under(root, stone)) {
        return refused(13, "tombstoned");
    }
    // AT-14
    if any_folder(&|e| under(&e.path, root)) {
        return refused(14, "container");
    }
    // AT-15
    let rec = match git {
        None => Some(record(root, None, None, None)),
        Some(_) => git_record(root, (Some(root) != common_dir).then_some(root)),
    };
    at(15, json!({"outcome": "new", "root": root}), rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/folder-attribution.json")).unwrap()
    }

    #[test]
    fn folders_attribute_as_the_contracts_vectors_say() {
        let file = vectors();
        let cases = file["attribution"].as_array().unwrap();
        assert_eq!(cases.len(), 135, "the pinned vectors changed size");
        let mut wrong = Vec::new();
        for case in cases {
            let got = attribute_folder(&case["snapshot"]);
            if got != case["expected"] {
                wrong.push(format!(
                    "{}: {} expected {}",
                    case["name"], got, case["expected"]
                ));
            }
        }
        assert!(wrong.is_empty(), "{}\n{}", wrong.len(), wrong.join("\n"));
    }

    #[test]
    fn records_are_kept_as_the_contracts_vectors_say() {
        let file = vectors();
        let cases = file["record"].as_array().unwrap();
        assert_eq!(cases.len(), 28);
        let mut wrong = Vec::new();
        for case in cases {
            let got = explain_folder(&case["snapshot"])
                .record
                .unwrap_or(Value::Null);
            if got != case["record"] {
                wrong.push(format!(
                    "{}: {} expected {}",
                    case["name"], got, case["record"]
                ));
            }
        }
        assert!(wrong.is_empty(), "{}\n{}", wrong.len(), wrong.join("\n"));
    }

    #[test]
    fn any_value_is_an_answer() {
        for value in [
            Value::Null,
            json!(1),
            json!("x"),
            json!([]),
            json!({}),
            json!({"folder": "/a", "home": "/u", "tempRoots": [], "projects": []}),
        ] {
            assert_eq!(attribute_folder(&value), json!({"fault": "input-invalid"}));
        }
    }
}
