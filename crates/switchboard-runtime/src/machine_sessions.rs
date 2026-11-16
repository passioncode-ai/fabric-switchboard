//! SB-353: every coding-agent session running on this machine, Switchboard's own and the others.
//!
//! The session lifecycle plan (`docs/plans/2026-11-13-session-lifecycle.md`, SL-1, SL-3, SL-8)
//! needs one answer to «what runs now». Switchboard's records know only the sessions it
//! launched; on the operator's MacBook on 2026-11-13 32 Claude Code processes ran while
//! `sessions` knew one. This reads this user's process table in-process (no `ps`, as
//! `external::claude_swap_activity` does), keeps the Claude Code and Codex processes, and joins
//! each to the launch whose home names it (`.session-pid`), with its folder, its age and when the
//! transcripts of its folder were last written. Reads only.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A session whose folder's transcripts were not written for this long reads `quiet`.
pub const QUIET_MINUTES: i64 = 30;
/// At most this many transcript files are looked at per folder.
const MAX_TRANSCRIPTS: usize = 512;

/// One agent process as the process table shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub agent: &'static str,
    pub folder: Option<String>,
    pub started_at: Option<i64>,
}

/// Which coding agent an executable is, by its path: Claude Code installs its versions under
/// `…/claude/versions/<version>` and also runs as `claude`; Codex runs as `codex`.
pub fn agent_of(executable: &str) -> Option<&'static str> {
    let name = Path::new(executable)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    // The npm install's `claude` is a link the kernel resolves to
    // `…/@anthropic-ai/claude-code/bin/claude.exe` (Fabric Dashboards, board `7e5b6d44`: 0.7.76
    // saw none of 30 such processes).
    if name == "claude"
        || name == "claude.exe"
        || executable.contains("/claude/versions/")
        || executable.contains("/@anthropic-ai/claude-code/")
    {
        Some("claude-code")
    } else if name == "codex" {
        Some("codex")
    } else {
        None
    }
}

/// Claude Code's folder name for a working directory under `projects/`: every character that is
/// not an ASCII letter or digit becomes `-`.
pub fn projects_dir_name(folder: &str) -> String {
    folder
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The newest modification time (Unix seconds) of a `.jsonl` file directly in `dir`.
fn newest_transcript(dir: &Path) -> Option<i64> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .take(MAX_TRANSCRIPTS)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .max()
}

/// Where Claude Code keeps a folder's transcripts: the person's own `~/.claude/projects` and each
/// home Switchboard launches sessions in.
fn transcript_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = dirs::home_dir()
        .map(|h| vec![h.join(".claude/projects")])
        .unwrap_or_default();
    for folder in ["homes", "runtimes"] {
        if let Ok(entries) = std::fs::read_dir(root.join(folder)) {
            roots.extend(entries.flatten().map(|e| e.path().join("projects")));
        }
    }
    roots
}

/// SB-376: the sessions a live Claude Code process holds now, by session id, with its pid and the
/// status Claude Code reports (`busy`, `idle`). Claude Code writes `sessions/<pid>.json` in its
/// config folder (the person's `~/.claude` and each home Switchboard launches in); a file whose pid
/// is no longer a Claude Code process is left over and ignored. A host reads this before it starts
/// a stopped session again: on 2026-11-14 Fabric Dashboards resumed this repository's session in a
/// second process while the first still ran in its terminal, and both worked in one worktree.
pub fn live_sessions(root: &Path) -> HashMap<String, (u32, String)> {
    session_pairs(root)
        .into_iter()
        .map(|(session, pid, status)| (session, (pid, status)))
        .collect()
}

/// SB-379: every live Claude Code process with the session id its `sessions/<pid>.json` names,
/// one pair per process: two pairs with one session id are a resumed copy of a live session.
pub fn session_pairs(root: &Path) -> Vec<(String, u32, String)> {
    agent_processes()
        .map(|procs| pairs_for(root, &procs))
        .unwrap_or_default()
}

fn pairs_for(root: &Path, procs: &[Proc]) -> Vec<(String, u32, String)> {
    let alive: std::collections::HashSet<u32> = procs
        .iter()
        .filter(|p| p.agent == "claude-code")
        .map(|p| p.pid)
        .collect();
    let mut files = Vec::new();
    for dir in session_dirs(root) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            // A session file is a few hundred bytes; anything larger is not one.
            if std::fs::metadata(&path).is_ok_and(|m| m.len() <= 64 * 1024) {
                if let Some(v) = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                {
                    files.push(v);
                }
            }
        }
    }
    pairs_from(&files, &alive)
}

/// The pure part of [`session_pairs`]: one pair per session file whose pid is a live process.
pub fn pairs_from(
    files: &[Value],
    alive: &std::collections::HashSet<u32>,
) -> Vec<(String, u32, String)> {
    files
        .iter()
        .filter_map(|f| {
            let pid = u32::try_from(f["pid"].as_u64()?).ok()?;
            let session = f["sessionId"].as_str()?;
            alive.contains(&pid).then(|| {
                let status = f["status"].as_str().unwrap_or("unknown").to_owned();
                (session.to_owned(), pid, status)
            })
        })
        .collect()
}

/// The pure part of [`live_sessions`]: session files whose pid is a live Claude Code process.
pub fn live_from(
    files: &[Value],
    alive: &std::collections::HashSet<u32>,
) -> HashMap<String, (u32, String)> {
    files
        .iter()
        .filter_map(|f| {
            let pid = u32::try_from(f["pid"].as_u64()?).ok()?;
            let session = f["sessionId"].as_str()?;
            alive.contains(&pid).then(|| {
                let status = f["status"].as_str().unwrap_or("unknown").to_owned();
                (session.to_owned(), (pid, status))
            })
        })
        .collect()
}

/// Claude Code's `sessions` folders: the person's own and each home Switchboard launches in.
/// SB-384: the live Claude Code and Codex processes of this user, as the process table shows them.
pub(crate) fn processes() -> Vec<Proc> {
    agent_processes().unwrap_or_default()
}

/// SB-384: every session file (`sessions/<pid>.json`) with the config folder it lies in: the
/// person's `~/.claude`, each home Switchboard launches in and the organizations' shell homes.
pub(crate) fn session_files(root: &Path) -> Vec<(PathBuf, Value)> {
    let mut found = Vec::new();
    for dir in session_dirs(root) {
        let Some(config) = dir.parent().map(Path::to_path_buf) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().and_then(|x| x.to_str()) != Some("json")
                || !std::fs::metadata(&path).is_ok_and(|m| m.len() <= 64 * 1024)
            {
                continue;
            }
            if let Some(v) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            {
                found.push((config.clone(), v));
            }
        }
    }
    found
}

/// SB-384: the newest write (Unix seconds) of one Claude Code session's transcript and of the
/// files under its own folder (its subagents' transcripts), in `config/projects/<folder>/`.
pub(crate) fn session_last_write(config: &Path, cwd: &str, session: &str) -> Option<i64> {
    let dir = config.join("projects").join(projects_dir_name(cwd));
    let stamp = |p: &Path| {
        std::fs::metadata(p)
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    };
    let mut newest = stamp(&dir.join(format!("{session}.jsonl")));
    let own = dir.join(session);
    for sub in [own.clone(), own.join("subagents")] {
        if let Ok(entries) = std::fs::read_dir(&sub) {
            for t in entries
                .flatten()
                .take(MAX_TRANSCRIPTS)
                .filter_map(|e| stamp(&e.path()))
            {
                newest = Some(newest.map_or(t, |n| n.max(t)));
            }
        }
    }
    newest
}

/// SB-384: the executables of a process's ancestors, nearest first, at most eight.
#[cfg(target_os = "macos")]
pub(crate) fn ancestor_paths(pid: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = pid as i32;
    let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for _ in 0..8 {
        // The short info, as `ps` reads it: the full one is refused for a process of another
        // user, and an interactive Terminal session's parent is `/usr/bin/login`, owned by root,
        // so the walk stopped there and never reached the terminal's host.
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdshortinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as i32;
        // SAFETY: a plain read of one process's info into a buffer of the size passed.
        let got = unsafe {
            libc::proc_pidinfo(
                current,
                libc::PROC_PIDT_SHORTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if got != size {
            break;
        }
        // SAFETY: proc_pidinfo filled the whole struct (checked above).
        let ppid = unsafe { info.assume_init() }.pbsi_ppid as i32;
        if ppid <= 1 {
            break;
        }
        // SAFETY: as above, into `path`'s own length.
        let n = unsafe { libc::proc_pidpath(ppid, path.as_mut_ptr().cast(), path.len() as u32) };
        if n > 0 {
            out.push(String::from_utf8_lossy(&path[..n as usize]).into_owned());
        }
        current = ppid;
    }
    out
}
#[cfg(target_os = "linux")]
pub(crate) fn ancestor_paths(pid: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = pid;
    for _ in 0..8 {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{current}/stat")) else {
            break;
        };
        // The command sits in parentheses and may hold spaces: the fields follow the last `)`.
        let Some(ppid) = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(1))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        if let Ok(exe) = std::fs::read_link(format!("/proc/{ppid}/exe")) {
            out.push(exe.to_string_lossy().into_owned());
        }
        current = ppid;
    }
    out
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn ancestor_paths(_: u32) -> Vec<String> {
    Vec::new()
}

fn session_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = dirs::home_dir()
        .map(|h| vec![h.join(".claude/sessions")])
        .unwrap_or_default();
    // SB-384: the organizations' shell homes too (the terminal hook's `CLAUDE_CONFIG_DIR`).
    for folder in ["homes", "runtimes", crate::shell::map::DIR] {
        if let Ok(entries) = std::fs::read_dir(root.join(folder)) {
            dirs.extend(entries.flatten().map(|e| e.path().join("sessions")));
        }
    }
    dirs
}

/// The launch each home's session marker names: marker pid to launch id.
fn launch_markers(root: &Path) -> HashMap<u32, String> {
    let mut found = HashMap::new();
    for folder in ["homes", "runtimes"] {
        let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
            continue;
        };
        for home in entries.flatten().map(|e| e.path()) {
            let Some(pid) = std::fs::read_to_string(home.join(".session-pid"))
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok())
            else {
                continue;
            };
            let launch = std::fs::read(home.join(crate::launch::SESSION_RECORD))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|r| r["launch_id"].as_str().map(str::to_owned));
            if let Some(launch) = launch {
                found.insert(pid, launch);
            }
        }
    }
    found
}

/// The answer from processes, markers and transcript times; pure, so the rules are tested
/// without a process table. A process is Switchboard's when it, its parent or its grandparent is
/// the process a launch's marker names (the launch script runs the agent as its child).
pub fn assemble(
    procs: &[Proc],
    markers: &HashMap<u32, String>,
    last_write: impl Fn(&str) -> Option<i64>,
    now: i64,
) -> Value {
    let parent: HashMap<u32, u32> = procs.iter().map(|p| (p.pid, p.ppid)).collect();
    let mut rows: Vec<Value> = procs
        .iter()
        .map(|p| {
            let mut chain = vec![p.pid, p.ppid];
            if let Some(grand) = parent.get(&p.ppid) {
                chain.push(*grand);
            }
            let launch = chain.iter().find_map(|pid| markers.get(pid)).cloned();
            let written = p.folder.as_deref().and_then(&last_write);
            let quiet_minutes = written.map(|w| ((now - w).max(0)) / 60);
            json!({
                "pid": p.pid,
                "agent": p.agent,
                "folder": p.folder,
                "started_at": p.started_at,
                "last_write_at": written,
                "quiet_minutes": quiet_minutes,
                "quiet": quiet_minutes.is_some_and(|m| m >= QUIET_MINUTES),
                "launch_id": launch,
                "managed": launch.is_some(),
            })
        })
        .collect();
    rows.sort_by_key(|r| r["started_at"].as_i64().unwrap_or(i64::MAX));
    let managed = rows.iter().filter(|r| r["managed"] == true).count();
    let quiet = rows.iter().filter(|r| r["quiet"] == true).count();
    json!({
        "sessions": rows,
        "total": procs.len(),
        "managed": managed,
        "unmanaged": procs.len() - managed,
        "quiet": quiet,
        "quiet_after_minutes": QUIET_MINUTES,
    })
}

/// Every Claude Code and Codex process of this user, joined to Switchboard's launches.
/// This user's coding agent processes, or none under the golden test's switch (SB-397 W1.0).
fn agent_processes() -> Result<Vec<Proc>, &'static str> {
    if crate::test_seams::no_processes() {
        return Ok(Vec::new());
    }
    os_agent_processes()
}

pub fn scan(root: &Path) -> Value {
    scan_procs(root, agent_processes())
}

/// SB-379: the scan and the live session pairs from one walk of the process table (the watch and
/// the tree need both; two walks each read about a thousand pid paths under load).
pub fn scan_and_pairs(root: &Path) -> (Value, Vec<(String, u32, String)>) {
    let procs = agent_processes();
    let pairs = procs
        .as_ref()
        .map(|p| pairs_for(root, p))
        .unwrap_or_default();
    (scan_procs(root, procs), pairs)
}

fn scan_procs(root: &Path, procs: Result<Vec<Proc>, &'static str>) -> Value {
    // SB-375: a table that could not be read is said so, never answered as no sessions.
    let (procs, degraded) = match procs {
        Ok(procs) => (procs, vec![]),
        Err(reason) => (
            vec![],
            vec![json!({"source": "process_table", "reason": reason})],
        ),
    };
    let markers = launch_markers(root);
    let roots = transcript_roots(root);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // SB-379: each folder's newest transcript once per scan, however many sessions share it.
    let seen: std::cell::RefCell<HashMap<String, Option<i64>>> = Default::default();
    let last_write = |folder: &str| -> Option<i64> {
        if let Some(known) = seen.borrow().get(folder) {
            return *known;
        }
        let name = projects_dir_name(folder);
        let newest = roots
            .iter()
            .filter_map(|r| newest_transcript(&r.join(&name)))
            .max();
        seen.borrow_mut().insert(folder.to_owned(), newest);
        newest
    };
    let mut answer = assemble(&procs, &markers, last_write, now);
    answer["observed_at"] = now.into();
    answer["degraded"] = degraded.into();
    answer
}

#[cfg(target_os = "macos")]
fn os_agent_processes() -> Result<Vec<Proc>, &'static str> {
    const PROC_UID_ONLY: u32 = 4;
    // SAFETY: plain reads of this user's process table into buffers of the sizes passed.
    let uid = unsafe { libc::geteuid() };
    let mut pids = vec![0i32; 8192];
    let bytes = unsafe {
        libc::proc_listpids(
            PROC_UID_ONLY,
            uid,
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if bytes <= 0 {
        return Err("proc_listpids_failed");
    }
    pids.truncate(bytes as usize / std::mem::size_of::<i32>());
    let mut found = Vec::new();
    let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    for pid in pids.into_iter().filter(|p| *p > 0) {
        let n = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        if n <= 0 {
            continue;
        }
        let executable = String::from_utf8_lossy(&path[..n as usize]).into_owned();
        let Some(agent) = agent_of(&executable) else {
            continue;
        };
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        let got = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        let (ppid, started_at) = if got == size {
            let info = unsafe { info.assume_init() };
            (info.pbi_ppid, Some(info.pbi_start_tvsec as i64))
        } else {
            (0, None)
        };
        let mut vnode = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
        let vsize = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
        let got = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                vnode.as_mut_ptr().cast(),
                vsize,
            )
        };
        let folder = (got == vsize).then(|| {
            let vnode = unsafe { vnode.assume_init() };
            // `vip_path` is MAXPATHLEN bytes, which libc spells as 32 rows of 32.
            let bytes: Vec<u8> = vnode
                .pvi_cdir
                .vip_path
                .iter()
                .flatten()
                .take_while(|c| **c != 0)
                .map(|c| *c as u8)
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        });
        found.push(Proc {
            pid: pid as u32,
            ppid,
            agent,
            folder: folder.filter(|f| !f.is_empty()),
            started_at,
        });
    }
    Ok(found)
}

#[cfg(target_os = "linux")]
fn os_agent_processes() -> Result<Vec<Proc>, &'static str> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Err("proc_unreadable");
    };
    // SAFETY: reads this process's own effective uid.
    let uid = unsafe { libc::geteuid() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let dir = entry.path();
        use std::os::unix::fs::MetadataExt;
        let Ok(meta) = std::fs::metadata(&dir) else {
            continue;
        };
        if meta.uid() != uid {
            continue;
        }
        let Some(agent) = std::fs::read_link(dir.join("exe"))
            .ok()
            .and_then(|p| agent_of(&p.to_string_lossy()))
        else {
            continue;
        };
        let ppid = std::fs::read_to_string(dir.join("stat"))
            .ok()
            .and_then(|s| {
                let after = s.rsplit_once(')')?.1;
                after.split_whitespace().nth(1)?.parse::<u32>().ok()
            })
            .unwrap_or(0);
        let folder = std::fs::read_link(dir.join("cwd"))
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        found.push(Proc {
            pid,
            ppid,
            agent,
            folder,
            started_at: Some(meta.mtime()),
        });
    }
    Ok(found)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn os_agent_processes() -> Result<Vec<Proc>, &'static str> {
    Err("not_read_on_this_platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_and_codex_are_known_by_their_executable() {
        assert_eq!(
            agent_of("/Users/x/.local/share/claude/versions/2.1.322"),
            Some("claude-code")
        );
        assert_eq!(agent_of("/opt/homebrew/bin/claude"), Some("claude-code"));
        assert_eq!(
            agent_of("/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe"),
            Some("claude-code")
        );
        assert_eq!(agent_of("/usr/local/bin/codex"), Some("codex"));
        assert_eq!(agent_of("/usr/bin/zsh"), None);
        assert_eq!(
            agent_of("/Applications/Claude.app/Contents/MacOS/Claude"),
            None
        );
    }

    #[test]
    fn a_folder_maps_to_claude_codes_projects_name() {
        assert_eq!(
            projects_dir_name("/Users/sshlg/DATA/fabric-switchboard"),
            "-Users-sshlg-DATA-fabric-switchboard"
        );
        assert_eq!(
            projects_dir_name("/Users/a/DATA/_worktrees/x.y"),
            "-Users-a-DATA--worktrees-x-y"
        );
    }

    #[test]
    fn every_agent_process_is_listed_with_its_launch_or_as_unmanaged_and_quiet_ones_are_named() {
        let procs = vec![
            // Launched by Switchboard: the launch script (pid 100) runs the agent as its child.
            Proc {
                pid: 101,
                ppid: 100,
                agent: "claude-code",
                folder: Some("/w/a".into()),
                started_at: Some(1_000),
            },
            // Started by hand in a Terminal: no marker names it or its parents.
            Proc {
                pid: 201,
                ppid: 200,
                agent: "claude-code",
                folder: Some("/w/b".into()),
                started_at: Some(2_000),
            },
            Proc {
                pid: 301,
                ppid: 300,
                agent: "codex",
                folder: None,
                started_at: None,
            },
        ];
        let markers = HashMap::from([(100, "launch-a".to_owned())]);
        let now = 10_000;
        let written = |folder: &str| match folder {
            "/w/a" => Some(now - 60),
            "/w/b" => Some(now - 31 * 60),
            _ => None,
        };
        let answer = assemble(&procs, &markers, written, now);
        assert_eq!(answer["total"], 3);
        assert_eq!(answer["managed"], 1);
        assert_eq!(answer["unmanaged"], 2);
        assert_eq!(answer["quiet"], 1);
        let rows = answer["sessions"].as_array().unwrap();
        assert_eq!(rows[0]["pid"], 101);
        assert_eq!(rows[0]["launch_id"], "launch-a");
        assert_eq!(rows[0]["quiet"], false);
        assert_eq!(rows[1]["pid"], 201);
        assert_eq!(rows[1]["managed"], false);
        assert_eq!(rows[1]["quiet_minutes"], 31);
        assert_eq!(rows[1]["quiet"], true);
        // A process whose transcripts are unknown is never called quiet.
        assert_eq!(rows[2]["agent"], "codex");
        assert!(rows[2]["quiet_minutes"].is_null());
        assert_eq!(rows[2]["quiet"], false);
    }

    #[test]
    fn a_session_is_live_only_while_its_pid_is_a_claude_code_process() {
        // SB-376: a left-over file of an ended process never reads as a session still open.
        let files = vec![
            json!({"pid": 10, "sessionId": "s-open", "status": "idle"}),
            json!({"pid": 11, "sessionId": "s-ended", "status": "busy"}),
            json!({"pid": 12, "status": "busy"}),
            json!({"sessionId": "s-nopid"}),
        ];
        let alive = std::collections::HashSet::from([10, 12]);
        let live = live_from(&files, &alive);
        assert_eq!(live.len(), 1, "{live:?}");
        assert_eq!(live["s-open"], (10, "idle".to_string()));
    }
    #[test]
    fn the_scan_answers_on_this_machine_without_failing() {
        let data = tempfile::tempdir().unwrap();
        let answer = scan(data.path());
        assert!(answer["total"].as_u64().is_some());
        assert_eq!(
            answer["total"].as_u64().unwrap(),
            answer["managed"].as_u64().unwrap() + answer["unmanaged"].as_u64().unwrap()
        );
        // SB-375: an unread table is named, never answered as an empty machine.
        let degraded = answer["degraded"].as_array().unwrap();
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(degraded.is_empty(), "{answer}");
        } else {
            assert_eq!(degraded[0]["source"], "process_table", "{answer}");
            assert_eq!(answer["total"], 0);
        }
    }
}
