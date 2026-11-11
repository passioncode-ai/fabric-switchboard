//! SB-118: ordinary Claude Code sessions in a project's folders use the project's accounts
//! (operator 2026-10-14: «для проекта не стали эти подписки и тратятся основные»). The ordinary
//! `claude` is signed in once for every folder; a project's reservation reached only sessions
//! Switchboard launched. Claude Code reads `env` from a folder's `.claude/settings.local.json`
//! (measured: a session started in a subfolder sent `Bearer <value>` to the configured base URL),
//! so Switchboard writes there the proxy route of the project's pool and a capability for that
//! pool only (`switchboard_proxy::pool_token`). Everything else in the file is the person's and
//! stays; what Switchboard wrote is recorded in `<data>/claude-bindings.json` (0600, no token —
//! a digest), and only values still equal to what it wrote are ever removed. The file is kept
//! out of Git through the repository's own `info/exclude`.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use switchboard_core::{Provider, Snapshot};

pub(crate) const FILE: &str = "claude-bindings.json";
const SETTINGS_DIR: &str = ".claude";
const SETTINGS_FILE: &str = "settings.local.json";
const EXCLUDE_LINE: &str = ".claude/settings.local.json";
pub(crate) const BASE_URL: &str = "ANTHROPIC_BASE_URL";
const DEFAULT_POOL: &str = "default";
/// SB-331: the `ANTHROPIC_AUTH_TOKEN` a folder gets where the ordinary Claude Code holds no
/// sign-in, so Claude Code sends its requests to the proxy instead of asking for a login. A fixed
/// word, never a credential: the proxy puts the group's account in its place, as for a shell home.
pub(crate) const SIGNED_OUT_TOKEN: &str = crate::shell::homes::PLACEHOLDER;
/// Written by 0.6.37 and earlier; removed where it is still ours (SB-140): Claude Code counts it
/// as an auth source and turns off the session's claude.ai connectors.
pub(crate) const AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";
/// Where the capability goes now: a header, not an auth source (SB-140).
pub(crate) const CUSTOM_HEADERS: &str = "ANTHROPIC_CUSTOM_HEADERS";
fn header_value(token: &str) -> String {
    format!("X-Switchboard-Capability: {token}")
}
/// Whether a `ANTHROPIC_CUSTOM_HEADERS` value is the one Switchboard wrote: the current one, or
/// the one the record's digest names.
fn header_ours(value: &Value, token: &str, previous: Option<&Binding>) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    text == header_value(token)
        || previous.is_some_and(|old| {
            text.strip_prefix("X-Switchboard-Capability: ")
                .is_some_and(|t| sha256(t) == old.token_sha256)
        })
}
const MAX_ROWS: usize = 2048;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SETTINGS: u64 = 1024 * 1024;
const UNREADABLE: &str =
    "Claude Code bindings could not be read. Restore the private file before continuing.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub folder: String,
    pub pool: String,
    pub base_url: String,
    /// SHA-256 of the capability written, so "still ours" is decidable without keeping it.
    pub token_sha256: String,
    /// Switchboard created the settings file; it removes the file again if nothing else is left.
    pub created: bool,
}

/// What one sync did, for the log and `switchboard project bindings`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub bound: usize,
    pub unchanged: usize,
    pub removed: usize,
    /// Folders whose settings file could not be read as JSON or written: left untouched.
    pub skipped: Vec<String>,
    /// Folders whose `ANTHROPIC_*` values are the person's, not Switchboard's: left untouched.
    pub user_values_present: Vec<String>,
    /// SB-251: folders whose `.claude` folder or settings file belongs to another user of this
    /// computer: neither read nor written, never retried as a failure.
    pub another_user: Vec<String>,
    /// SB-183: the folders whose Claude Code route is Switchboard's after this sync (written or
    /// unchanged), for the terminal hook's folder map; never a folder whose values are the
    /// person's.
    pub bound_folders: std::collections::BTreeSet<String>,
    /// SB-133: each pool's folder capabilities, for the proxy (`folder_tokens`).
    pub tokens: std::collections::HashMap<String, Vec<String>>,
    /// SB-172: the capabilities of folders taken out of their group in the last `RELEASED_FOR`,
    /// by the pool they left: a session started there before goes on, on the default pool.
    pub released: std::collections::HashMap<String, Vec<String>>,
}

/// SB-172: how long a folder taken out of its group keeps its old capability, served on the
/// default pool: a Claude Code session reads its settings once, at start.
pub(crate) const RELEASED_FOR: i64 = 7 * 24 * 3600;
const RELEASED_FILE: &str = "released-folders.json";
const MAX_RELEASED: usize = 4096;

/// A folder taken out of its group: the pool it left and when (no capability is kept).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Released {
    pub folder: String,
    pub pool: String,
    pub at: i64,
}

fn read_released(root: &Path) -> Vec<Released> {
    switchboard_core::private_fs::read_private(&root.join(RELEASED_FILE), 2 * 1024 * 1024)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Records `left` as taken out now, forgets entries older than `RELEASED_FOR` and folders that
/// are in a group again, and answers the capabilities still honoured, by pool.
pub(crate) fn released_tokens(
    root: &Path,
    left: &[(String, String)],
    want: &[(PathBuf, String)],
    session_token: &str,
    salts: &std::collections::HashMap<String, String>,
    now: i64,
) -> std::collections::HashMap<String, Vec<String>> {
    let before = read_released(root);
    let mut rows: Vec<Released> = before
        .iter()
        .filter(|r| now - r.at < RELEASED_FOR && r.at <= now + 60)
        .filter(|r| {
            !want
                .iter()
                .any(|(f, _)| f.to_str() == Some(r.folder.as_str()))
        })
        .cloned()
        .collect();
    for (folder, pool) in left {
        rows.retain(|r| !(r.folder == *folder && r.pool == *pool));
        rows.push(Released {
            folder: folder.clone(),
            pool: pool.clone(),
            at: now,
        });
    }
    if rows.len() > MAX_RELEASED {
        rows.drain(..rows.len() - MAX_RELEASED);
    }
    if rows != before {
        if let Ok(bytes) = serde_json::to_vec(&rows) {
            let _ = switchboard_core::private_fs::private_write(&root.join(RELEASED_FILE), &bytes);
        }
    }
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    for r in &rows {
        out.entry(r.pool.clone())
            .or_default()
            .push(folder_capability(session_token, &r.pool, salts, &r.folder));
    }
    out
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn read(root: &Path) -> Result<Vec<Binding>, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    let rows: Vec<Binding> = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    if rows.len() > MAX_ROWS {
        return Err(UNREADABLE.into());
    }
    Ok(rows)
}

fn write(root: &Path, rows: &[Binding]) -> Result<(), String> {
    let bytes = serde_json::to_vec(rows).map_err(|_| UNREADABLE)?;
    if bytes.len() as u64 > MAX_BYTES || rows.len() > MAX_ROWS {
        return Err(UNREADABLE.into());
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

/// The working-tree root `folder` resolves under (its nearest ancestor holding `.git`) and the
/// repository's common git folder (shared by all its worktrees).
fn git_root_and_common(folder: &Path) -> Option<(PathBuf, PathBuf)> {
    folder
        .ancestors()
        .find_map(|dir| crate::suggest::repo(dir).map(|r| (dir.to_owned(), r.common)))
}

/// The folders whose Claude Code sessions should use a pool: every folder of a project whose
/// pool has an enabled Claude account, and every worktree of those folders' repositories that
/// sits inside a project folder (a worktree is its own repository root, so Claude Code reads
/// its own settings there).
pub(crate) fn desired(snapshot: &Snapshot) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    // SB-109: with organizations defined, personal groups first, so a folder in groups of two
    // contexts is written for the personal one (the more private wins), as `project_for` says.
    let mut projects: Vec<_> = snapshot.projects.iter().collect();
    if !snapshot.organizations.is_empty() {
        projects.sort_by_key(|p| !p.organizations.is_empty());
    }
    let has_claude = |pool: &str| {
        snapshot
            .accounts
            .iter()
            .any(|a| snapshot.serves(a, pool) && a.provider == Provider::Claude && a.enabled)
    };
    // SB-325: a personal group with no Claude account of its own routes its folders to the
    // default group, as a repository in no group uses it (DEC-0007); its folders come after every
    // group's own, so a folder two groups hold keeps the group that has an account.
    let mut fallback: Vec<(PathBuf, String)> = Vec::new();
    for project in projects {
        // A group whose seats left its organization keeps its folders: the proxy refuses the seat
        // (closed), rather than the folder falling back to the ordinary login (open). SB-109.
        // A group bound to an organization never falls back to the default group either.
        let route = if has_claude(&project.pool) {
            project.pool.clone()
        } else if project.organizations.is_empty()
            && project.pool != DEFAULT_POOL
            && has_claude(DEFAULT_POOL)
        {
            DEFAULT_POOL.to_owned()
        } else {
            continue;
        };
        let out = if route == project.pool {
            &mut out
        } else {
            &mut fallback
        };
        for folder in &project.folders {
            let folder = PathBuf::from(folder);
            if folder.is_dir() {
                out.push((folder.clone(), route.clone()));
            }
            let Some(common) = crate::suggest::repo(&folder).map(|r| r.common) else {
                continue;
            };
            let Ok(entries) = std::fs::read_dir(common.join("worktrees")) else {
                continue;
            };
            for entry in entries.flatten().take(1024) {
                let Ok(text) = std::fs::read_to_string(entry.path().join("gitdir")) else {
                    continue;
                };
                let gitfile = PathBuf::from(text.trim());
                let gitfile = if gitfile.is_absolute() {
                    gitfile
                } else {
                    entry.path().join(gitfile)
                };
                let Some(worktree) = gitfile.parent().and_then(|p| p.canonicalize().ok()) else {
                    continue;
                };
                // Inside one of this project's folders; a worktree elsewhere joins through a
                // suggestion (SB-117) and is then a folder of its own.
                if snapshot
                    .project_for(&worktree)
                    .is_some_and(|p| p.pool == project.pool)
                {
                    out.push((worktree, route.clone()));
                }
            }
        }
    }
    out.extend(fallback);
    // SB-146: a folder in several groups is written for the first group with a Claude account
    // (creation order), the one `Snapshot::project_for` names.
    let mut seen = std::collections::HashSet::new();
    out.retain(|(f, _)| seen.insert(f.clone()));
    out.sort();
    out
}

fn settings_path(folder: &Path) -> PathBuf {
    folder.join(SETTINGS_DIR).join(SETTINGS_FILE)
}

/// The settings file as a JSON object; `Ok(None)` when it does not exist. A file that is not a
/// JSON object is the person's to fix: never overwritten.
fn read_settings(path: &Path) -> Result<Option<Map<String, Value>>, ()> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(m) => m,
    };
    if !meta.is_file() || meta.len() > MAX_SETTINGS {
        return Err(());
    }
    let text = std::fs::read_to_string(path).map_err(|_| ())?;
    if text.trim().is_empty() {
        return Ok(Some(Map::new()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        _ => Err(()),
    }
}

/// Atomic replace in the same folder, 0600: the file carries a capability.
fn write_settings(path: &Path, map: &Map<String, Value>) -> Result<(), ()> {
    let dir = path.parent().ok_or(())?;
    std::fs::create_dir_all(dir).map_err(|_| ())?;
    let mut text = serde_json::to_string_pretty(&Value::Object(map.clone())).map_err(|_| ())?;
    text.push('\n');
    let temporary = dir.join(format!(".settings.local.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| ())?;
        file.write_all(text.as_bytes()).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        std::fs::rename(&temporary, path).map_err(|_| ())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Keeps the file out of Git through the repository's own `info/exclude` (never a tracked
/// `.gitignore`). Claude Code ignores it the same way when it creates the file itself. A
/// pattern with a slash is anchored to the working-tree root, so a folder that is a subfolder
/// of the repository gets its path relative to that root.
fn exclude(folder: &Path) {
    let Some((root, common)) = git_root_and_common(folder) else {
        return;
    };
    let relative = folder.strip_prefix(&root).unwrap_or(Path::new(""));
    let line = if relative.as_os_str().is_empty() {
        EXCLUDE_LINE.to_owned()
    } else {
        let parts: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        format!("{}/{EXCLUDE_LINE}", parts.join("/"))
    };
    let path = common.join("info").join("exclude");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current.lines().any(|l| l.trim() == line) {
        return;
    }
    let mut text = current;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# Fabric Switchboard (SB-118): the project's Claude Code route, local only\n");
    text.push_str(&line);
    text.push('\n');
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, text);
}

/// What `apply` did with one folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Applied {
    /// The route was written; `true` when Switchboard created the settings file.
    Written(bool),
    /// The route was already there.
    Unchanged,
    /// An `ANTHROPIC_*` value is the person's, not Switchboard's: the folder is left untouched.
    UserValues,
    /// SB-251: the `.claude` folder or its settings file belongs to another user of this
    /// computer, whose own Switchboard writes it: the folder is left untouched.
    AnotherUser,
}

/// SB-251: whether the folder's settings file, the `.claude` folder that holds it or the folder
/// itself belongs to another user of this computer. One settings file serves every Claude Code
/// session in the folder, so two users' owners would take turns overwriting each other's route;
/// the user who owns the file keeps it. A path this user cannot even look at counts as another
/// user's too.
#[cfg(unix)]
fn another_users(path: &Path) -> bool {
    // SAFETY: getuid has no preconditions and cannot fail.
    owned_by_another(path, unsafe { libc::getuid() })
}
#[cfg(unix)]
fn owned_by_another(path: &Path, me: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    path.ancestors()
        .take(3)
        .any(|p| match std::fs::symlink_metadata(p) {
            Ok(meta) => meta.uid() != me,
            Err(e) => e.kind() == std::io::ErrorKind::PermissionDenied,
        })
}
#[cfg(not(unix))]
fn another_users(_path: &Path) -> bool {
    false
}

/// A value in the settings file is Switchboard's when it is the one about to be written or
/// still equals what the record says was written before.
fn is_ours(value: &Value, current: &str, previous: Option<&Binding>, base: bool) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    text == current
        || previous.is_some_and(|old| {
            if base {
                text == old.base_url
            } else {
                sha256(text) == old.token_sha256
            }
        })
}

/// `http://127.0.0.1:<port>/claude/<pool>`: a route to Switchboard's own proxy, whichever
/// folder, pool or port it was written for.
fn is_switchboard_route(value: &Value) -> bool {
    let Some(rest) = value
        .as_str()
        .and_then(|t| t.strip_prefix("http://127.0.0.1:"))
    else {
        return false;
    };
    let Some((port, pool)) = rest.split_once("/claude/") else {
        return false;
    };
    !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && !pool.is_empty()
        && !pool.contains('/')
}

/// A capability header: only Switchboard's proxy reads `X-Switchboard-Capability`.
fn is_switchboard_header(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|t| t.strip_prefix("X-Switchboard-Capability: "))
        .is_some_and(|t| !t.is_empty() && !t.contains(['\r', '\n']))
}

/// Writes the route into one folder, never over a value that is the person's.
fn apply(
    folder: &Path,
    base_url: &str,
    token: &str,
    previous: Option<&Binding>,
    known: &std::collections::HashSet<String>,
    signed_out: bool,
) -> Result<Applied, ()> {
    let path = settings_path(folder);
    if another_users(&path) {
        return Ok(Applied::AnotherUser);
    }
    let existing = read_settings(&path)?;
    let created = existing.is_none();
    let mut map = existing.unwrap_or_default();
    if let Some(env) = map.get("env") {
        let Value::Object(env) = env else {
            return Err(());
        };
        // SB-244: a folder copied with its settings file carries the values Switchboard wrote
        // for another folder. A Switchboard route and capability header are Switchboard's in any
        // folder: only its proxy reads them.
        let base_ok = env
            .get(BASE_URL)
            .is_none_or(|v| is_ours(v, base_url, previous, true) || is_switchboard_route(v));
        let token_ok = env.get(AUTH_TOKEN).is_none_or(|v| {
            is_ours(v, token, previous, false)
                || v.as_str()
                    .is_some_and(|t| t == SIGNED_OUT_TOKEN || known.contains(&sha256(t)))
        });
        let header_ok = env
            .get(CUSTOM_HEADERS)
            .is_none_or(|v| header_ours(v, token, previous) || is_switchboard_header(v));
        if !base_ok || !token_ok || !header_ok {
            return Ok(Applied::UserValues);
        }
    }
    let env = map
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(env) = env else {
        return Err(());
    };
    let header = header_value(token);
    let want_token = signed_out.then_some(SIGNED_OUT_TOKEN);
    let same = env.get(BASE_URL).and_then(Value::as_str) == Some(base_url)
        && env.get(CUSTOM_HEADERS).and_then(Value::as_str) == Some(header.as_str())
        && env.get(AUTH_TOKEN).and_then(Value::as_str) == want_token;
    exclude(folder);
    if same {
        return Ok(Applied::Unchanged);
    }
    // Checked above to be ours: the old auth-token form goes; SB-331's placeholder is written
    // only while the ordinary Claude Code holds no sign-in.
    env.remove(AUTH_TOKEN);
    if let Some(placeholder) = want_token {
        env.insert(AUTH_TOKEN.into(), Value::String(placeholder.into()));
    }
    env.insert(BASE_URL.into(), Value::String(base_url.into()));
    env.insert(CUSTOM_HEADERS.into(), Value::String(header));
    write_settings(&path, &map)?;
    Ok(Applied::Written(created))
}

/// Removes what a binding wrote, per key: each of the two keys whose value is still the one it
/// wrote goes; a key the person edited stays.
fn unapply(row: &Binding) -> Result<bool, ()> {
    let path = settings_path(Path::new(&row.folder));
    // SB-251: the file is another user's now: nothing in it is this owner's to remove.
    if another_users(&path) {
        return Ok(false);
    }
    let Some(mut map) = read_settings(&path)? else {
        return Ok(false);
    };
    let Some(Value::Object(env)) = map.get_mut("env") else {
        return Ok(false);
    };
    let mut removed = false;
    if env.get(BASE_URL).and_then(Value::as_str) == Some(row.base_url.as_str()) {
        env.remove(BASE_URL);
        removed = true;
    }
    if env
        .get(AUTH_TOKEN)
        .and_then(Value::as_str)
        .is_some_and(|t| t == SIGNED_OUT_TOKEN || sha256(t) == row.token_sha256)
    {
        env.remove(AUTH_TOKEN);
        removed = true;
    }
    if env
        .get(CUSTOM_HEADERS)
        .and_then(Value::as_str)
        .and_then(|v| v.strip_prefix("X-Switchboard-Capability: "))
        .is_some_and(|t| sha256(t) == row.token_sha256)
    {
        env.remove(CUSTOM_HEADERS);
        removed = true;
    }
    if !removed {
        return Ok(false);
    }
    if env.is_empty() {
        map.remove("env");
    }
    if map.is_empty() && row.created {
        std::fs::remove_file(&path).map_err(|_| ())?;
    } else {
        write_settings(&path, &map)?;
    }
    Ok(true)
}

/// Brings every project folder's settings in line with the projects, the pools' accounts and
/// the proxy's address. Must run under the owner transaction.
/// The unsalted sync: tests and the uninstall test's fixture.
#[cfg(test)]
pub(crate) fn sync(
    root: &Path,
    snapshot: &Snapshot,
    address: &str,
    session_token: &str,
) -> Result<Report, String> {
    sync_salted(
        root,
        snapshot,
        address,
        session_token,
        &std::collections::HashMap::new(),
        false,
    )
}
/// SB-133: the capability written into one folder: its pool's, bound to the folder's path, so a
/// folder that leaves the project (or is moved or renamed away) holds a value the proxy refuses.
pub(crate) fn folder_capability(
    session_token: &str,
    pool: &str,
    salts: &std::collections::HashMap<String, String>,
    folder: &str,
) -> String {
    let pool_capability = switchboard_proxy::pool_token_salted(
        session_token,
        pool,
        salts.get(pool).map(String::as_str),
    );
    switchboard_proxy::folder_token(&pool_capability, folder)
}
/// SB-133: every folder capability the proxy accepts now, by pool: one per folder that is part of
/// its project and exists (`desired`).
pub(crate) fn folder_tokens(
    want: &[(PathBuf, String)],
    session_token: &str,
    salts: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (folder, pool) in want {
        if let Some(text) = folder.to_str() {
            out.entry(pool.clone()).or_default().push(folder_capability(
                session_token,
                pool,
                salts,
                text,
            ));
        }
    }
    out
}
/// `sync`, with the pools' capability salts (`pool_salts`): a pool whose capability was rotated
/// gets its new capability written in every one of its folders.
pub(crate) fn sync_salted(
    root: &Path,
    snapshot: &Snapshot,
    address: &str,
    session_token: &str,
    salts: &std::collections::HashMap<String, String>,
    signed_out: bool,
) -> Result<Report, String> {
    let want = desired(snapshot);
    let mut report = Report {
        tokens: folder_tokens(&want, session_token, salts),
        ..Report::default()
    };
    let rows = read(root)?;
    // Every capability Switchboard recorded writing, in any folder (SB-244).
    let known: std::collections::HashSet<String> =
        rows.iter().map(|r| r.token_sha256.clone()).collect();
    let mut kept: Vec<Binding> = Vec::new();
    let mut left: Vec<(String, String)> = Vec::new();
    for (folder, pool) in &want {
        let Some(text) = folder.to_str() else {
            continue;
        };
        let base_url = format!("http://{address}/claude/{pool}");
        let token = folder_capability(session_token, pool, salts, text);
        let previous = rows.iter().find(|r| r.folder == text);
        // A folder moved to another pool, or a new address: the old values go first.
        if let Some(old) = previous {
            if old.pool != *pool || old.base_url != base_url {
                let _ = unapply(old);
            }
        }
        match apply(folder, &base_url, &token, previous, &known, signed_out) {
            Ok(result @ (Applied::Written(_) | Applied::Unchanged)) => {
                let created = match (result, previous) {
                    (Applied::Written(written), Some(old)) => written || old.created,
                    (Applied::Written(written), None) => written,
                    (Applied::Unchanged, Some(old)) => old.created,
                    (Applied::Unchanged, None) => false,
                    (Applied::UserValues | Applied::AnotherUser, _) => unreachable!(),
                };
                report.bound_folders.insert(text.to_owned());
                if matches!(result, Applied::Written(_)) {
                    report.bound += 1;
                } else {
                    report.unchanged += 1;
                }
                kept.push(Binding {
                    folder: text.to_owned(),
                    pool: pool.clone(),
                    base_url,
                    token_sha256: sha256(&token),
                    created,
                });
            }
            // The folder's values are the person's: never overwritten. What an earlier row of
            // ours still covers stays removable through it.
            Ok(Applied::UserValues) => {
                report.user_values_present.push(text.to_owned());
                if let Some(old) = previous {
                    kept.push(old.clone());
                }
            }
            // SB-251: another user's file; this owner keeps no row for it, so nothing is
            // retried and nothing of the other user's is ever removed.
            Ok(Applied::AnotherUser) => report.another_user.push(text.to_owned()),
            // Still ours but not writable now: kept to try again.
            Err(()) => {
                report.skipped.push(text.to_owned());
                if let Some(old) = previous {
                    kept.push(old.clone());
                }
            }
        }
    }
    for row in rows
        .iter()
        .filter(|r| !want.iter().any(|(f, _)| f.to_str() == Some(&r.folder)))
    {
        // Its sessions keep their old capability until they restart (SB-172).
        left.push((row.folder.clone(), row.pool.clone()));
        match unapply(row) {
            Ok(true) => report.removed += 1,
            Ok(false) => {}
            // Still ours but not removable now: kept to try again.
            Err(()) => {
                report.skipped.push(row.folder.clone());
                kept.push(row.clone());
            }
        }
    }
    kept.truncate(MAX_ROWS);
    write(root, &kept)?;
    report.released = released_tokens(
        root,
        &left,
        &want,
        session_token,
        salts,
        crate::monitor::now(),
    );
    Ok(report)
}

/// Removes every binding (`switchboard uninstall`). A folder whose file could not be updated
/// is an error — the rest is still done, and the record stays for a retry.
pub(crate) fn unbind_all(root: &Path) -> Result<usize, String> {
    let rows = read(root)?;
    let mut removed = 0;
    let mut failed = 0;
    for row in &rows {
        match unapply(row) {
            Ok(true) => removed += 1,
            Ok(false) => {}
            Err(()) => failed += 1,
        }
    }
    if failed > 0 {
        return Err("Could not update every bound folder.".into());
    }
    let _ = std::fs::remove_file(root.join(FILE));
    Ok(removed)
}

/// A file's modification time and length; `None` when the path is absent or unstamped.
type Stamp = Option<(std::time::SystemTime, u64)>;

/// The repository common folder of each project folder, re-resolved only when the folder's
/// `.git` modification stamp moves, so an idle pass reads no git files (LC-04) — the same
/// stamp-driven shape as the SB-117 scanner.
#[derive(Default)]
pub(crate) struct RepoCache {
    entries: std::collections::HashMap<PathBuf, (Stamp, Option<PathBuf>)>,
    /// Resolutions done so far, for the idle-budget test.
    pub(crate) reads: usize,
}

impl RepoCache {
    fn common(&mut self, folder: &Path) -> Option<PathBuf> {
        let stamp: Stamp = std::fs::symlink_metadata(folder.join(".git"))
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
        if let Some((seen, common)) = self.entries.get(folder) {
            if *seen == stamp {
                return common.clone();
            }
        }
        self.reads += 1;
        let common = crate::suggest::repo(folder).map(|r| r.common);
        self.entries
            .insert(folder.to_owned(), (stamp, common.clone()));
        common
    }
}

/// A cheap fingerprint of everything `sync` depends on, so the background runs it only when
/// something changed: projects, their pools' Claude accounts, the proxy address, and the
/// modification time of each project repository's worktree list.
pub(crate) fn key(snapshot: &Snapshot, address: &str, repos: &mut RepoCache) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    address.hash(&mut h);
    for p in &snapshot.projects {
        p.pool.hash(&mut h);
        p.folders.hash(&mut h);
        // SB-325: a personal group falls back to the default group; one bound to an
        // organization does not.
        p.organizations.hash(&mut h);
        for f in &p.folders {
            let worktrees = repos.common(Path::new(f)).map(|c| c.join("worktrees"));
            let stamp =
                worktrees.and_then(|w| std::fs::metadata(w).and_then(|m| m.modified()).ok());
            stamp.hash(&mut h);
            Path::new(f).is_dir().hash(&mut h);
        }
    }
    for a in &snapshot.accounts {
        if a.provider == Provider::Claude {
            (&a.pool, a.enabled).hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Account, AuthKind, Project};

    fn account(pool: &str) -> Account {
        Account {
            id: format!("id-{pool}"),
            label: "Synthetic".into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            external_identity: None,
            usage: None,
            usage_health: None,
        }
    }
    fn git(dir: &Path) {
        std::fs::create_dir_all(dir.join(".git/info")).unwrap();
        std::fs::write(dir.join(".git/config"), "").unwrap();
    }
    fn project(pool: &str, folders: &[&Path]) -> Project {
        Project {
            shared_accounts: Vec::new(),
            pool: pool.into(),
            name: pool.into(),
            folders: folders
                .iter()
                .map(|f| f.canonicalize().unwrap().to_string_lossy().into_owned())
                .collect(),
            created_at: 1,
            sources: vec![],
            organizations: Vec::new(),
        }
    }
    fn env(folder: &Path) -> Option<Map<String, Value>> {
        let map = read_settings(&settings_path(folder)).ok()??;
        map.get("env").and_then(Value::as_object).cloned()
    }

    #[test]
    fn a_project_folder_gets_its_pool_route_and_keeps_the_persons_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        std::fs::create_dir_all(app.join(".claude")).unwrap();
        std::fs::write(
            settings_path(&app),
            r#"{"permissions":{"allow":["Bash(ls)"]},"env":{"FOO":"bar"}}"#,
        )
        .unwrap();
        let fresh = data.join("fresh");
        git(&fresh);
        let mut s = Snapshot {
            projects: vec![project("pc", &[&app, &fresh])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!((report.bound, report.skipped.len()), (2, 0));
        let e = env(&app).unwrap();
        assert_eq!(e[BASE_URL], "http://127.0.0.1:4000/claude/pc");
        let none = std::collections::HashMap::new();
        let mine = folder_capability("session", "pc", &none, app.to_str().unwrap());
        assert_eq!(e[CUSTOM_HEADERS], header_value(&mine).as_str());
        // SB-133: each folder its own value, all of them known to the proxy for the pool.
        let theirs = folder_capability("session", "pc", &none, fresh.to_str().unwrap());
        assert_ne!(mine, theirs);
        assert_eq!(report.tokens["pc"], [mine.clone(), theirs]);
        assert_ne!(mine, switchboard_proxy::pool_token("session", "pc"));
        // SB-140: never an auth source, so the session keeps its claude.ai login and connectors.
        assert_eq!(e.get(AUTH_TOKEN), None);
        assert_eq!(e["FOO"], "bar", "the person's own env stays");
        let whole = read_settings(&settings_path(&app)).unwrap().unwrap();
        assert!(whole.contains_key("permissions"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(settings_path(&fresh))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let excluded = std::fs::read_to_string(app.join(".git/info/exclude")).unwrap();
        assert!(excluded.lines().any(|l| l == EXCLUDE_LINE));
        // Nothing changed: nothing written; the record holds no capability.
        let again = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!((again.bound, again.unchanged), (0, 2));
        let record = std::fs::read_to_string(root.path().join(FILE)).unwrap();
        assert!(!record.contains(&switchboard_proxy::pool_token("session", "pc")));
        // A new port rewrites; leaving the project removes only ours, and the file Switchboard
        // created goes with it.
        sync(root.path(), &s, "127.0.0.1:4001", "session").unwrap();
        assert_eq!(
            env(&app).unwrap()[BASE_URL],
            "http://127.0.0.1:4001/claude/pc"
        );
        s.projects.clear();
        let gone = sync(root.path(), &s, "127.0.0.1:4001", "session").unwrap();
        assert_eq!(gone.removed, 2);
        assert_eq!(env(&app).unwrap().get(BASE_URL), None);
        assert_eq!(env(&app).unwrap()["FOO"], "bar");
        assert!(
            !settings_path(&fresh).exists(),
            "a file Switchboard created is removed"
        );
    }

    #[test]
    fn a_group_without_its_own_claude_account_routes_to_the_default_group() {
        // SB-325 (Mac Studio, 2026-11-10): every group there held no account, so a plain `claude`
        // in a group folder had no route at all. A personal group falls back to the default
        // group; one bound to an organization never does.
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let (agents, org, own) = (data.join("agents"), data.join("org"), data.join("own"));
        for d in [&agents, &org, &own] {
            git(d);
        }
        let mut bound = project("bound", &[&org]);
        bound.organizations = vec!["acme".into()];
        let mut s = Snapshot {
            projects: vec![
                project("agents", &[&agents, &own]),
                bound,
                project("team", &[&own]),
            ],
            accounts: vec![account("default"), account("team")],
            ..Default::default()
        };
        let want = desired(&s);
        let pool_of = |f: &Path| want.iter().find(|(w, _)| w == f).map(|(_, p)| p.as_str());
        assert_eq!(pool_of(&agents), Some("default"));
        assert_eq!(
            pool_of(&org),
            None,
            "an organization's group never falls back"
        );
        assert_eq!(
            pool_of(&own),
            Some("team"),
            "a group's own account comes first"
        );
        // No default account: nothing to fall back to.
        s.accounts.retain(|a| a.pool != "default");
        assert_eq!(desired(&s).iter().find(|(w, _)| *w == agents), None);
    }

    #[test]
    fn a_machine_with_no_claude_sign_in_gets_a_placeholder_token_only_while_it_lasts() {
        // SB-331: with no ordinary sign-in, Claude Code says "Not logged in" before it sends a
        // request; the placeholder lets it reach the proxy. It goes once a sign-in exists, and the
        // person's own token is never touched.
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let (app, theirs) = (data.join("app"), data.join("theirs"));
        git(&app);
        git(&theirs);
        std::fs::create_dir_all(theirs.join(".claude")).unwrap();
        std::fs::write(
            settings_path(&theirs),
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"the-persons"}}"#,
        )
        .unwrap();
        let s = Snapshot {
            projects: vec![project("pc", &[&app, &theirs])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let none = std::collections::HashMap::new();
        let at = "127.0.0.1:4000";
        sync_salted(root.path(), &s, at, "session", &none, true).unwrap();
        assert_eq!(env(&app).unwrap()[AUTH_TOKEN], SIGNED_OUT_TOKEN);
        assert_eq!(env(&theirs).unwrap()[AUTH_TOKEN], "the-persons");
        let again = sync_salted(root.path(), &s, at, "session", &none, true).unwrap();
        assert_eq!(again.bound, 0, "nothing changed, nothing written");
        sync_salted(root.path(), &s, at, "session", &none, false).unwrap();
        assert_eq!(env(&app).unwrap().get(AUTH_TOKEN), None);
        sync_salted(root.path(), &s, at, "session", &none, true).unwrap();
        let empty = Snapshot::default();
        sync_salted(root.path(), &empty, at, "session", &none, true).unwrap();
        assert!(
            !settings_path(&app).exists(),
            "leaving the group takes the placeholder too"
        );
        assert_eq!(env(&theirs).unwrap()[AUTH_TOKEN], "the-persons");
    }

    #[test]
    fn values_the_person_changed_or_a_broken_file_are_never_touched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        let broken = data.join("broken");
        git(&broken);
        std::fs::create_dir_all(broken.join(".claude")).unwrap();
        std::fs::write(settings_path(&broken), "{ not json").unwrap();
        let mut s = Snapshot {
            projects: vec![project("pc", &[&app, &broken])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(
            std::fs::read_to_string(settings_path(&broken)).unwrap(),
            "{ not json"
        );
        // The person points the folder elsewhere: leaving the project leaves their value.
        let mut map = read_settings(&settings_path(&app)).unwrap().unwrap();
        map["env"][BASE_URL] = Value::String("https://example.invalid".into());
        write_settings(&settings_path(&app), &map).unwrap();
        s.projects.clear();
        sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(env(&app).unwrap()[BASE_URL], "https://example.invalid");
        // A pool without an enabled Claude account binds nothing.
        let s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![Account {
                enabled: false,
                ..account("pc")
            }],
            ..Default::default()
        };
        assert!(desired(&s).is_empty());
    }

    #[test]
    fn a_group_whose_seats_all_left_its_organization_stays_bound() {
        // SB-170: fail closed. The group's only Claude seat is the person's own now (outside the
        // group's organization); its folder keeps the group's route, so the proxy refuses the
        // seat, rather than the folder falling back to the ordinary login.
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().canonicalize().unwrap().join("app");
        git(&app);
        let acme: switchboard_core::Organization = serde_json::from_value(serde_json::json!({
            "id": "org-acme", "name": "Acme", "domains": ["example.invalid"], "created_at": 1
        }))
        .unwrap();
        let mut group = project("pc", &[&app]);
        group.organizations = vec![acme.id.clone()];
        let seat = account("pc");
        let s = Snapshot {
            projects: vec![group],
            memberships: [(
                seat.id.clone(),
                switchboard_core::organizations::PERSONAL.into(),
            )]
            .into(),
            accounts: vec![seat],
            organizations: vec![acme],
            ..Default::default()
        };
        assert_eq!(desired(&s), vec![(app.clone(), "pc".to_string())]);
    }

    #[test]
    fn a_worktree_inside_a_project_folder_is_bound_too() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        let wt = app.join(".worktrees").join("feature");
        let admin = app.join(".git/worktrees/feature");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", wt.join(".git").display()),
        )
        .unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        let s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let folders: Vec<PathBuf> = desired(&s).into_iter().map(|(f, _)| f).collect();
        assert_eq!(folders, vec![app.clone(), wt.canonicalize().unwrap()]);
        // The worktree's exclude goes to the repository's common git folder.
        let root = tempfile::tempdir().unwrap();
        sync(root.path(), &s, "127.0.0.1:4000", "t").unwrap();
        assert!(settings_path(&wt).exists());
        let excluded = std::fs::read_to_string(app.join(".git/info/exclude")).unwrap();
        assert_eq!(excluded.matches(EXCLUDE_LINE).count(), 1);
    }

    #[test]
    fn a_folder_nested_in_a_repository_gets_a_relative_exclude_line() {
        // A pattern with a slash is anchored to the working-tree root: a project folder that is
        // a subfolder of the repository needs its path relative to that root.
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let repo = data.join("repo");
        git(&repo);
        let app = repo.join("packages").join("app");
        std::fs::create_dir_all(&app).unwrap();
        let s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(report.bound, 1);
        assert!(settings_path(&app).exists());
        let excluded = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
        assert!(
            excluded
                .lines()
                .any(|l| l == "packages/app/.claude/settings.local.json"),
            "{excluded}"
        );
        assert!(!excluded.lines().any(|l| l == EXCLUDE_LINE));
    }

    #[test]
    fn a_failed_re_apply_keeps_the_record_row_and_later_syncs_own_it_again() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        let mut s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        // The settings file is unreadable at re-sync time: skipped, but the row survives, so
        // the written capability is never orphaned.
        std::fs::write(settings_path(&app), "{ not json").unwrap();
        let failed = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(failed.skipped.len(), 1);
        let rows = read(root.path()).unwrap();
        assert_eq!(rows.len(), 1, "the record row survives a failed re-apply");
        assert_eq!(rows[0].pool, "pc");
        // Restored: the folder is owned again, and leaving the project removes ours.
        std::fs::remove_file(settings_path(&app)).unwrap();
        let again = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!((again.bound, again.skipped.len()), (1, 0));
        assert!(env(&app).unwrap().contains_key(CUSTOM_HEADERS));
        s.projects.clear();
        let gone = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(gone.removed, 1);
    }

    #[test]
    fn values_that_are_not_switchboards_are_never_overwritten_but_own_values_are_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let theirs = data.join("theirs");
        git(&theirs);
        std::fs::create_dir_all(theirs.join(".claude")).unwrap();
        std::fs::write(
            settings_path(&theirs),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://person.example","ANTHROPIC_AUTH_TOKEN":"theirs"}}"#,
        )
        .unwrap();
        let mixed = data.join("mixed");
        git(&mixed);
        std::fs::create_dir_all(mixed.join(".claude")).unwrap();
        std::fs::write(
            settings_path(&mixed),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://person.example"}}"#,
        )
        .unwrap();
        let clean = data.join("clean");
        git(&clean);
        let s = Snapshot {
            projects: vec![project("pc", &[&theirs, &mixed, &clean])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(report.bound, 1, "only the clean folder is bound");
        assert_eq!(report.user_values_present.len(), 2);
        assert!(report.skipped.is_empty());
        let e = env(&theirs).unwrap();
        assert_eq!(e[BASE_URL], "https://person.example");
        assert_eq!(e[AUTH_TOKEN], "theirs");
        assert_eq!(env(&mixed).unwrap()[BASE_URL], "https://person.example");
        assert!(
            read(root.path()).unwrap().len() == 1,
            "no record for a folder never written"
        );
        // Over Switchboard's own values a re-apply replaces: a new port rewrites the route.
        let report = sync(root.path(), &s, "127.0.0.1:4001", "session").unwrap();
        assert_eq!(report.bound, 1);
        assert!(report.user_values_present.len() == 2);
        assert_eq!(
            env(&clean).unwrap()[BASE_URL],
            "http://127.0.0.1:4001/claude/pc"
        );
    }

    #[test]
    fn removal_is_per_key_a_value_the_person_edited_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        let mut s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        // The person edits only the base URL: unbind removes the token (still ours) and keeps
        // the edited key.
        let mut map = read_settings(&settings_path(&app)).unwrap().unwrap();
        map["env"][BASE_URL] = Value::String("https://person.example".into());
        write_settings(&settings_path(&app), &map).unwrap();
        s.projects.clear();
        let gone = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(gone.removed, 1);
        let e = env(&app).unwrap();
        assert_eq!(e[BASE_URL], "https://person.example");
        assert_eq!(e.get(AUTH_TOKEN), None);
        assert!(read(root.path()).unwrap().is_empty());
    }

    #[test]
    fn the_repository_is_resolved_again_only_when_its_git_stamp_moves() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let app = data.join("app");
        git(&app);
        let s = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let mut repos = RepoCache::default();
        let first = key(&s, "127.0.0.1:4000", &mut repos);
        assert_eq!(repos.reads, 1);
        assert_eq!(key(&s, "127.0.0.1:4000", &mut repos), first);
        assert_eq!(repos.reads, 1, "an unchanged `.git` is not re-read");
        // The stamp moves: resolved once more, the fingerprint stays the same.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(app.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(key(&s, "127.0.0.1:4000", &mut repos), first);
        assert_eq!(repos.reads, 2);
    }

    #[test]
    #[cfg(unix)]
    fn a_rotated_pool_gets_its_new_capability_written_over_the_old_one() {
        // 2026-10-18: a folder's capability was printed into a transcript; the rotation rewrites
        // it in every folder of the pool (the old value is still ours, never the person's).
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let app = temp.path().join("app");
        git(&app);
        let snapshot = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        sync(&root, &snapshot, "127.0.0.1:4000", "session").unwrap();
        let read = || {
            env(&app).unwrap()[CUSTOM_HEADERS]
                .as_str()
                .unwrap()
                .strip_prefix("X-Switchboard-Capability: ")
                .unwrap()
                .to_owned()
        };
        let old = read();
        let salts: std::collections::HashMap<String, String> =
            [("pc".to_owned(), "synthetic-salt".to_owned())].into();
        let report =
            sync_salted(&root, &snapshot, "127.0.0.1:4000", "session", &salts, false).unwrap();
        assert_eq!(report.bound, 1, "{report:?}");
        let new = read();
        assert_ne!(new, old);
        let canonical = app.canonicalize().unwrap();
        assert_eq!(
            new,
            folder_capability("session", "pc", &salts, canonical.to_str().unwrap())
        );
    }

    #[test]
    fn a_folder_copied_with_its_settings_file_is_rewritten_by_a_rotation() {
        // SB-244 (2026-11-03): two full clones copied with their source's settings file kept
        // the old capability through a rotation that answered `skipped: 0`, because the copied
        // values matched no record of their own folder and were taken for the person's.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let app = temp.path().join("app");
        let copy = temp.path().join("app-copy");
        let stale = temp.path().join("app-stale");
        for folder in [&app, &copy, &stale] {
            git(folder);
        }
        let first = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        sync(&root, &first, "127.0.0.1:4000", "session").unwrap();
        std::fs::create_dir_all(copy.join(SETTINGS_DIR)).unwrap();
        std::fs::copy(settings_path(&app), settings_path(&copy)).unwrap();
        // A copy from before an earlier rotation, on an older port: no record names its value.
        std::fs::create_dir_all(stale.join(SETTINGS_DIR)).unwrap();
        std::fs::write(
            settings_path(&stale),
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:3999/claude/pc","ANTHROPIC_CUSTOM_HEADERS":"X-Switchboard-Capability: from-an-earlier-salt"},"permissions":{"allow":["Bash(ls)"]}}"#,
        )
        .unwrap();
        let snapshot = Snapshot {
            projects: vec![project("pc", &[&app, &copy, &stale])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let salts: std::collections::HashMap<String, String> =
            [("pc".to_owned(), "synthetic-salt".to_owned())].into();
        let report =
            sync_salted(&root, &snapshot, "127.0.0.1:4000", "session", &salts, false).unwrap();
        assert_eq!(report.bound, 3, "{report:?}");
        assert!(report.user_values_present.is_empty(), "{report:?}");
        assert!(report.skipped.is_empty(), "{report:?}");
        for folder in [&app, &copy, &stale] {
            let canonical = folder.canonicalize().unwrap();
            let e = env(folder).unwrap();
            assert_eq!(e[BASE_URL], "http://127.0.0.1:4000/claude/pc");
            assert_eq!(
                e[CUSTOM_HEADERS],
                header_value(&folder_capability(
                    "session",
                    "pc",
                    &salts,
                    canonical.to_str().unwrap()
                ))
            );
        }
        // The rest of a copied file stays as it was.
        let kept: Value =
            serde_json::from_str(&std::fs::read_to_string(settings_path(&stale)).unwrap()).unwrap();
        assert_eq!(kept["permissions"]["allow"][0], "Bash(ls)");
    }

    #[test]
    #[cfg(unix)]
    fn a_file_folder_or_settings_folder_of_another_user_is_another_users() {
        // SB-251: the uid of the file, its `.claude` folder or the project folder decides.
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().canonicalize().unwrap();
        let path = settings_path(&folder);
        // SAFETY: getuid has no preconditions and cannot fail.
        let me = unsafe { libc::getuid() };
        assert!(
            !owned_by_another(&path, me),
            "nothing there yet, the folder is mine"
        );
        assert!(
            owned_by_another(&path, me + 1),
            "the folder is someone else's"
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{}").unwrap();
        assert!(!owned_by_another(&path, me));
        assert!(owned_by_another(&path, me + 1));
    }

    #[test]
    #[cfg(unix)]
    fn a_folder_whose_settings_this_user_cannot_see_is_left_to_its_owner() {
        // SB-251: a `.claude` folder this user cannot search is another user's (as on a shared
        // folder the other user bound first): no write, no row, no retry as a failure.
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let data = tmp.path().canonicalize().unwrap();
        let shared = data.join("shared");
        git(&shared);
        let dir = shared.join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let s = Snapshot {
            projects: vec![project("pc", &[&shared])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(root.path(), &s, "127.0.0.1:4000", "session").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        if unsafe { libc::getuid() } == 0 {
            return; // root searches any folder; the rule is about other users.
        }
        let folder = shared.to_string_lossy().into_owned();
        assert_eq!(report.another_user, vec![folder]);
        assert_eq!((report.bound, report.skipped.len()), (0, 0));
        assert!(report.bound_folders.is_empty());
        assert!(read(root.path()).unwrap().is_empty());
        assert!(!settings_path(&shared).exists());
    }

    #[test]
    fn a_route_to_somewhere_else_is_still_the_persons() {
        // SB-244 recognises only Switchboard's own route and header; anything near it stays.
        for text in [
            "http://127.0.0.1:4000/claude/pc/extra",
            "http://127.0.0.1:/claude/pc",
            "http://127.0.0.1:4000/claude/",
            "http://localhost:4000/claude/pc",
            "https://127.0.0.1:4000/claude/pc",
            "http://127.0.0.1:4000/codex/pc",
        ] {
            assert!(!is_switchboard_route(&Value::String(text.into())), "{text}");
        }
        assert!(is_switchboard_route(&Value::String(
            "http://127.0.0.1:4000/claude/pc".into()
        )));
        for text in [
            "X-Other: value",
            "X-Switchboard-Capability: ",
            "X-Switchboard-Capability: a\nX-Other: b",
        ] {
            assert!(
                !is_switchboard_header(&Value::String(text.into())),
                "{text}"
            );
        }
        assert!(!is_switchboard_header(&Value::Bool(true)));
    }

    #[test]
    #[cfg(unix)]
    fn a_folder_bound_by_an_older_version_loses_its_auth_token_for_the_header() {
        // SB-140 (2026-10-18): `ANTHROPIC_AUTH_TOKEN` made Claude Code turn off claude.ai
        // connectors in every project folder. An older binding's token goes; the person's own
        // auth token is never touched.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let app = temp.path().join("app");
        git(&app);
        let app = app.canonicalize().unwrap();
        let cap = switchboard_proxy::pool_token("session", "pc");
        let old = Binding {
            folder: app.to_string_lossy().into_owned(),
            pool: "pc".into(),
            base_url: "http://127.0.0.1:4000/claude/pc".into(),
            token_sha256: sha256(&cap),
            created: true,
        };
        write(&root, std::slice::from_ref(&old)).unwrap();
        std::fs::create_dir_all(app.join(".claude")).unwrap();
        let mut env_map = Map::new();
        env_map.insert(BASE_URL.into(), Value::String(old.base_url.clone()));
        env_map.insert(AUTH_TOKEN.into(), Value::String(cap.clone()));
        let mut settings = Map::new();
        settings.insert("env".into(), Value::Object(env_map));
        write_settings(&settings_path(&app), &settings).unwrap();
        let snapshot = Snapshot {
            projects: vec![project("pc", &[&app])],
            accounts: vec![account("pc")],
            ..Default::default()
        };
        let report = sync(&root, &snapshot, "127.0.0.1:4000", "session").unwrap();
        assert_eq!(report.bound, 1, "{report:?}");
        let e = env(&app).unwrap();
        assert_eq!(e.get(AUTH_TOKEN), None);
        let none = std::collections::HashMap::new();
        let mine = folder_capability("session", "pc", &none, app.to_str().unwrap());
        assert_eq!(e[CUSTOM_HEADERS], header_value(&mine).as_str());
        // Leaving the project removes the header as well.
        let gone = sync(&root, &Snapshot::default(), "127.0.0.1:4000", "session").unwrap();
        assert_eq!(gone.removed, 1);
        assert!(!settings_path(&app).exists());
        // SB-172: its sessions keep the capability they started with, for the pool it left.
        assert_eq!(gone.released.get("pc"), Some(&vec![mine.clone()]));
        let again = sync(&root, &Snapshot::default(), "127.0.0.1:4000", "session").unwrap();
        assert_eq!(
            again.released.get("pc"),
            Some(&vec![mine.clone()]),
            "kept, not renewed"
        );
        let file = std::fs::read_to_string(root.join(RELEASED_FILE)).unwrap();
        assert!(!file.contains(&mine), "no capability in the file");
        // Forgotten after RELEASED_FOR, and at once when the folder is in a group again.
        let later = crate::monitor::now() + RELEASED_FOR;
        assert!(released_tokens(&root, &[], &[], "session", &none, later).is_empty());
        let back = sync(&root, &snapshot, "127.0.0.1:4000", "session").unwrap();
        assert!(back.released.is_empty());
    }
}
