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
pub(crate) const AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";
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
    for project in &snapshot.projects {
        let claude = snapshot
            .accounts
            .iter()
            .any(|a| a.pool == project.pool && a.provider == Provider::Claude && a.enabled);
        if !claude {
            continue;
        }
        for folder in &project.folders {
            let folder = PathBuf::from(folder);
            if folder.is_dir() {
                out.push((folder.clone(), project.pool.clone()));
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
                    out.push((worktree, project.pool.clone()));
                }
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
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

/// Writes the route into one folder, never over a value that is the person's.
fn apply(
    folder: &Path,
    base_url: &str,
    token: &str,
    previous: Option<&Binding>,
) -> Result<Applied, ()> {
    let path = settings_path(folder);
    let existing = read_settings(&path)?;
    let created = existing.is_none();
    let mut map = existing.unwrap_or_default();
    if let Some(env) = map.get("env") {
        let Value::Object(env) = env else {
            return Err(());
        };
        let base_ok = env
            .get(BASE_URL)
            .is_none_or(|v| is_ours(v, base_url, previous, true));
        let token_ok = env
            .get(AUTH_TOKEN)
            .is_none_or(|v| is_ours(v, token, previous, false));
        if !base_ok || !token_ok {
            return Ok(Applied::UserValues);
        }
    }
    let env = map
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(env) = env else {
        return Err(());
    };
    let same = env.get(BASE_URL).and_then(Value::as_str) == Some(base_url)
        && env.get(AUTH_TOKEN).and_then(Value::as_str) == Some(token);
    exclude(folder);
    if same {
        return Ok(Applied::Unchanged);
    }
    env.insert(BASE_URL.into(), Value::String(base_url.into()));
    env.insert(AUTH_TOKEN.into(), Value::String(token.into()));
    write_settings(&path, &map)?;
    Ok(Applied::Written(created))
}

/// Removes what a binding wrote, per key: each of the two keys whose value is still the one it
/// wrote goes; a key the person edited stays.
fn unapply(row: &Binding) -> Result<bool, ()> {
    let path = settings_path(Path::new(&row.folder));
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
        .is_some_and(|t| sha256(t) == row.token_sha256)
    {
        env.remove(AUTH_TOKEN);
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
pub(crate) fn sync(
    root: &Path,
    snapshot: &Snapshot,
    address: &str,
    session_token: &str,
) -> Result<Report, String> {
    let rows = read(root)?;
    let want = desired(snapshot);
    let mut report = Report::default();
    let mut kept: Vec<Binding> = Vec::new();
    for (folder, pool) in &want {
        let Some(text) = folder.to_str() else {
            continue;
        };
        let base_url = format!("http://{address}/claude/{pool}");
        let token = switchboard_proxy::pool_token(session_token, pool);
        let previous = rows.iter().find(|r| r.folder == text);
        // A folder moved to another pool, or a new address: the old values go first.
        if let Some(old) = previous {
            if old.pool != *pool || old.base_url != base_url {
                let _ = unapply(old);
            }
        }
        match apply(folder, &base_url, &token, previous) {
            Ok(result @ (Applied::Written(_) | Applied::Unchanged)) => {
                let created = match (result, previous) {
                    (Applied::Written(written), Some(old)) => written || old.created,
                    (Applied::Written(written), None) => written,
                    (Applied::Unchanged, Some(old)) => old.created,
                    (Applied::Unchanged, None) => false,
                    (Applied::UserValues, _) => unreachable!(),
                };
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
            pool: pool.into(),
            name: pool.into(),
            folders: folders
                .iter()
                .map(|f| f.canonicalize().unwrap().to_string_lossy().into_owned())
                .collect(),
            created_at: 1,
            sources: vec![],
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
        assert_eq!(
            e[AUTH_TOKEN],
            switchboard_proxy::pool_token("session", "pc").as_str()
        );
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
        assert!(env(&app).unwrap().contains_key(AUTH_TOKEN));
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
}
