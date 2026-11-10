//! SB-265 (SCN-085): an organization's GitHub repositories, through the person's own GitHub CLI
//! (`gh`). Switchboard stores no GitHub token: what the person can see is GitHub's and their sign-in
//! there (design `docs/research/organization-wizard-2026-11-05.md`). Three reads and one write:
//! the owners `gh` can see, one owner's repositories matched with this computer, and one clone into
//! a folder the person chose. `gh` runs with fixed arguments, never through a shell, in its own
//! process group, with a deadline.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Refusals; `person-errors.json` gives each its sentence.
pub const GH_FAILED: &str = "GitHub did not answer";
pub const GH_OWNER_INVALID: &str = "GitHub owner invalid";
pub const CLONE_EXISTS: &str = "Clone folder exists";
pub const CLONE_FAILED: &str = "Clone failed";
pub const CLONE_INTO_INVALID: &str = "Clone folder invalid";

const READ_TIMEOUT: Duration = Duration::from_secs(30);
const CLONE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
/// How many repositories one owner's list reads at most.
const LIST_LIMIT: &str = "1000";

/// What one run of `gh` ended with.
enum Run {
    Done(String),
    Failed,
}

/// Runs `gh` with fixed arguments and a deadline; stdout only, bounded. `PATH` for the child holds
/// `gh`'s own folder and the usual ones, so the `git` it calls is found from the desktop app too.
/// Starts `command`, again a few times while the program is busy being written (`ETXTBSY` on
/// Linux): a `gh` being updated, or in tests a script just written while another thread forked
/// and held its descriptor (SB-321: the v0.7.44 nightly read "GitHub did not answer").
fn spawn_settled(command: &mut Command) -> Option<std::process::Child> {
    for attempt in 0..5 {
        match command.spawn() {
            Ok(child) => return Some(child),
            Err(e) if busy(&e) && attempt < 4 => {
                std::thread::sleep(Duration::from_millis(50 << attempt));
            }
            Err(_) => return None,
        }
    }
    None
}
fn busy(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    return error.raw_os_error() == Some(libc::ETXTBSY);
    #[cfg(not(unix))]
    {
        let _ = error;
        false
    }
}
fn run(program: &Path, args: &[&str], timeout: Duration) -> Run {
    let mut path: Vec<PathBuf> = program
        .parent()
        .into_iter()
        .map(Path::to_path_buf)
        .collect();
    if let Some(git) = crate::launch::find_program("git") {
        path.extend(git.parent().map(Path::to_path_buf));
    }
    if let Some(current) = std::env::var_os("PATH") {
        path.extend(std::env::split_paths(&current));
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // `gh` asks nothing and opens no browser or pager.
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PAGER", "cat");
    if let Ok(joined) = std::env::join_paths(path) {
        command.env("PATH", joined);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let Some(mut child) = spawn_settled(&mut command) else {
        return Run::Failed;
    };
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout {
            let _ = stdout.take(MAX_OUTPUT as u64 + 1).read_to_end(&mut out);
        }
        out
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                #[cfg(unix)]
                if let Ok(group) = libc::pid_t::try_from(child.id()) {
                    // SAFETY: killpg only signals the group this call created.
                    unsafe {
                        libc::killpg(group, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    match status {
        Some(status) if status.success() && out.len() <= MAX_OUTPUT => {
            String::from_utf8(out).map_or(Run::Failed, Run::Done)
        }
        _ => Run::Failed,
    }
}

/// A GitHub login or organization name: letters, digits and single hyphens inside, at most 39.
pub fn owner_valid(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
/// A repository name as GitHub allows it, never `.` or `..`.
fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The owners `gh` can see: `{available, reason, owners: [{login, kind}]}`, the person's own login
/// first, then their organizations by name. `reason` is `missing` (no `gh`) or `signed_out`.
pub fn owners() -> Result<Value, String> {
    owners_with(crate::launch::find_program("gh").as_deref())
}
fn owners_with(program: Option<&Path>) -> Result<Value, String> {
    let Some(program) = program else {
        return Ok(json!({"available": false, "reason": "missing", "owners": []}));
    };
    let Run::Done(login) = run(program, &["api", "user", "--jq", ".login"], READ_TIMEOUT) else {
        return match run(program, &["auth", "status"], READ_TIMEOUT) {
            Run::Failed => Ok(json!({"available": false, "reason": "signed_out", "owners": []})),
            Run::Done(_) => Err(GH_FAILED.into()),
        };
    };
    let login = login.trim();
    if !owner_valid(login) {
        return Err(GH_FAILED.into());
    }
    let Run::Done(orgs) = run(
        program,
        &["api", "user/orgs", "--paginate", "--jq", ".[].login"],
        READ_TIMEOUT,
    ) else {
        return Err(GH_FAILED.into());
    };
    let mut names: Vec<&str> = orgs
        .lines()
        .map(str::trim)
        .filter(|o| owner_valid(o))
        .collect();
    names.sort_by_key(|o| o.to_lowercase());
    names.dedup();
    let mut owners = vec![json!({"login": login, "kind": "user"})];
    owners.extend(
        names
            .into_iter()
            .filter(|o| !o.eq_ignore_ascii_case(login))
            .map(|o| json!({"login": o, "kind": "organization"})),
    );
    Ok(json!({"available": true, "reason": null, "owners": owners}))
}

/// One owner's repositories (`gh repo list`, at most 1000), each with its folder on this computer
/// when a checkout's `origin` is it (`github.com/<owner>/<name>`, any case), and where clones go:
/// the folder holding most of that owner's checkouts, else `~/DATA`, else the home folder.
pub fn repositories(owner: &str, snapshot: &switchboard_core::Snapshot) -> Result<Value, String> {
    let program = crate::launch::find_program("gh").ok_or(GH_FAILED)?;
    let mut under: Vec<PathBuf> = crate::roots::default_under().into_iter().collect();
    for project in &snapshot.projects {
        for folder in &project.folders {
            if let Some(parent) = Path::new(folder).parent().filter(|p| p.is_dir()) {
                if !under.iter().any(|u| parent.starts_with(u)) {
                    under.push(parent.to_path_buf());
                }
            }
        }
    }
    let map = crate::roots::disk_map(&under, snapshot);
    let checkouts: Vec<(String, String)> = map
        .repositories
        .iter()
        .filter_map(|f| Some((f.remote.clone()?, f.folder.clone())))
        .collect();
    repositories_with(&program, owner, &checkouts, crate::roots::default_under())
}
fn repositories_with(
    program: &Path,
    owner: &str,
    checkouts: &[(String, String)],
    fallback: Option<PathBuf>,
) -> Result<Value, String> {
    if !owner_valid(owner) {
        return Err(GH_OWNER_INVALID.into());
    }
    let Run::Done(text) = run(
        program,
        &[
            "repo",
            "list",
            owner,
            "--limit",
            LIST_LIMIT,
            "--json",
            "name,url,isArchived,isFork",
        ],
        READ_TIMEOUT,
    ) else {
        return Err(GH_FAILED.into());
    };
    let listed: Vec<Value> = serde_json::from_str(&text).map_err(|_| GH_FAILED)?;
    let prefix = format!("github.com/{}/", owner.to_lowercase());
    let on_disk: BTreeMap<String, &str> = checkouts
        .iter()
        .filter_map(|(remote, folder)| {
            let name = remote.to_lowercase().strip_prefix(&prefix)?.to_owned();
            Some((name, folder.as_str()))
        })
        .collect();
    let mut parents: BTreeMap<PathBuf, usize> = BTreeMap::new();
    let repositories: Vec<Value> = listed
        .iter()
        .filter_map(|r| {
            let name = r.get("name")?.as_str().filter(|n| name_valid(n))?;
            let folder = on_disk.get(&name.to_lowercase()).copied();
            if let Some(parent) = folder.and_then(|f| Path::new(f).parent()) {
                *parents.entry(parent.to_path_buf()).or_default() += 1;
            }
            Some(json!({
                "name": name,
                "url": r.get("url").and_then(Value::as_str),
                "archived": r.get("isArchived").and_then(Value::as_bool).unwrap_or(false),
                "fork": r.get("isFork").and_then(Value::as_bool).unwrap_or(false),
                "folder": folder,
            }))
        })
        .collect();
    let clone_into = parents
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(p, _)| p.clone())
        .or(fallback);
    Ok(json!({"owner": owner, "clone_into": clone_into, "repositories": repositories}))
}

/// Clones `owner/name` into `into/name` with `gh repo clone`. It is written into a hidden
/// `.<name>.switchboard-partial` beside it and renamed when complete, so a clone cut off never
/// looks like the repository; a folder of that name already there is never written into.
pub fn clone(owner: &str, name: &str, into: &Path) -> Result<Value, String> {
    let program = crate::launch::find_program("gh").ok_or(GH_FAILED)?;
    clone_with(&program, owner, name, into, CLONE_TIMEOUT)
}
fn clone_with(
    program: &Path,
    owner: &str,
    name: &str,
    into: &Path,
    timeout: Duration,
) -> Result<Value, String> {
    if !owner_valid(owner) || !name_valid(name) {
        return Err(GH_OWNER_INVALID.into());
    }
    if !into.is_absolute() || !into.is_dir() {
        return Err(CLONE_INTO_INVALID.into());
    }
    let target = into.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(CLONE_EXISTS.into());
    }
    let partial = into.join(format!(".{name}.switchboard-partial"));
    // Only this name of Switchboard's own: a clone an earlier run left unfinished.
    if std::fs::symlink_metadata(&partial).is_ok() {
        std::fs::remove_dir_all(&partial).map_err(|_| CLONE_FAILED)?;
    }
    let repo = format!("{owner}/{name}");
    let Some(partial_text) = partial.to_str() else {
        return Err(CLONE_INTO_INVALID.into());
    };
    let finished = matches!(
        run(program, &["repo", "clone", &repo, partial_text], timeout),
        Run::Done(_)
    ) && partial.join(".git").exists();
    if !finished {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(CLONE_FAILED.into());
    }
    if std::fs::symlink_metadata(&target).is_ok() {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(CLONE_EXISTS.into());
    }
    std::fs::rename(&partial, &target).map_err(|_| CLONE_FAILED)?;
    Ok(json!({"folder": target}))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A stand-in for `gh`: answers the reads from fixtures and makes a clone a folder with `.git`.
    /// `state` is `ok`, `signed_out` or `broken`.
    fn fake(dir: &Path, state: &str) -> PathBuf {
        let script = dir.join("gh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
state={state}
case "$1 $2" in
  "api user")
    [ "$state" = ok ] || exit 1; echo me ;;
  "api user/orgs")
    printf 'zeta\nAcme\nme\nbad name\n' ;;
  "auth status")
    [ "$state" = signed_out ] && exit 1; exit 0 ;;
  "repo list")
    [ "$3" = acme ] || exit 1
    echo '[{{"name":"app","url":"https://github.com/acme/app","isArchived":false,"isFork":false}},{{"name":"old","url":"https://github.com/acme/old","isArchived":true,"isFork":false}},{{"name":"site","url":"https://github.com/acme/site","isArchived":false,"isFork":true}},{{"name":"..","url":"x"}}]' ;;
  "repo clone")
    [ "$3" = acme/broken ] && {{ mkdir -p "$4"; exit 1; }}
    mkdir -p "$4/.git" ;;
  *) exit 2 ;;
esac
"#
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn a_program_busy_being_written_is_started_again_and_nothing_else_is() {
        assert!(busy(&std::io::Error::from_raw_os_error(libc::ETXTBSY)));
        assert!(!busy(&std::io::Error::from_raw_os_error(libc::ENOENT)));
        // A program that is not there fails at once, without the retries' waits.
        let started = std::time::Instant::now();
        let mut command = Command::new("/nonexistent/switchboard-gh");
        assert!(spawn_settled(&mut command).is_none());
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn owners_are_the_persons_login_then_their_organizations() {
        let dir = tempfile::tempdir().unwrap();
        let view = owners_with(Some(&fake(dir.path(), "ok"))).unwrap();
        assert_eq!(view["available"], true);
        assert_eq!(
            view["owners"],
            json!([
                {"login": "me", "kind": "user"},
                {"login": "Acme", "kind": "organization"},
                {"login": "zeta", "kind": "organization"}
            ])
        );
        assert_eq!(
            owners_with(None).unwrap(),
            json!({"available": false, "reason": "missing", "owners": []})
        );
        let signed_out = owners_with(Some(&fake(dir.path(), "signed_out"))).unwrap();
        assert_eq!(signed_out["reason"], "signed_out");
        assert_eq!(
            owners_with(Some(&fake(dir.path(), "broken"))),
            Err(GH_FAILED.to_string())
        );
    }

    #[test]
    fn repositories_are_matched_with_this_computer_and_clones_go_where_most_are() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path(), "ok");
        let checkouts = vec![
            (
                "github.com/acme/app".to_owned(),
                "/work/acme/app".to_owned(),
            ),
            (
                "github.com/ACME/Site".to_owned(),
                "/work/acme/site".to_owned(),
            ),
            (
                "github.com/other/app".to_owned(),
                "/elsewhere/app".to_owned(),
            ),
        ];
        let view =
            repositories_with(&gh, "acme", &checkouts, Some(PathBuf::from("/home/data"))).unwrap();
        assert_eq!(view["clone_into"], "/work/acme");
        let rows = view["repositories"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "a name GitHub cannot have is left out");
        assert_eq!(rows[0]["folder"], "/work/acme/app");
        assert_eq!(
            (&rows[1]["archived"], &rows[1]["folder"]),
            (&json!(true), &Value::Null)
        );
        assert_eq!(
            (&rows[2]["fork"], &rows[2]["folder"]),
            (&json!(true), &json!("/work/acme/site"))
        );
        // Nothing of the owner on disk: the fallback folder.
        let none = repositories_with(&gh, "acme", &[], Some(PathBuf::from("/home/data"))).unwrap();
        assert_eq!(none["clone_into"], "/home/data");
        assert_eq!(
            repositories_with(&gh, "-bad", &[], None),
            Err(GH_OWNER_INVALID.to_string())
        );
        assert_eq!(
            repositories_with(&gh, "zeta", &[], None),
            Err(GH_FAILED.to_string())
        );
    }

    #[test]
    fn a_clone_appears_only_when_complete_and_never_over_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake(dir.path(), "ok");
        let into = tempfile::tempdir().unwrap();
        let into = into.path().canonicalize().unwrap();
        let done = clone_with(&gh, "acme", "app", &into, READ_TIMEOUT).unwrap();
        assert_eq!(done["folder"], json!(into.join("app")));
        assert!(into.join("app/.git").is_dir());
        // A folder of that name is never written into.
        assert_eq!(
            clone_with(&gh, "acme", "app", &into, READ_TIMEOUT),
            Err(CLONE_EXISTS.to_string())
        );
        // A failed clone leaves nothing behind, partial included.
        assert_eq!(
            clone_with(&gh, "acme", "broken", &into, READ_TIMEOUT),
            Err(CLONE_FAILED.to_string())
        );
        assert!(!into.join("broken").exists());
        assert!(!into.join(".broken.switchboard-partial").exists());
        // A partial an earlier run left is cleared and the clone completes.
        std::fs::create_dir_all(into.join(".site.switchboard-partial/junk")).unwrap();
        clone_with(&gh, "acme", "site", &into, READ_TIMEOUT).unwrap();
        assert!(into.join("site/.git").is_dir());
        assert!(!into.join(".site.switchboard-partial").exists());
        // Names and folders are checked before anything runs.
        assert_eq!(
            clone_with(&gh, "acme", "..", &into, READ_TIMEOUT),
            Err(GH_OWNER_INVALID.to_string())
        );
        assert_eq!(
            clone_with(&gh, "acme", "x", Path::new("relative"), READ_TIMEOUT),
            Err(CLONE_INTO_INVALID.to_string())
        );
    }
}
