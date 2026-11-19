//! SB-353: every coding-agent session running on this machine, Switchboard's own and the others.
//!
//! The session lifecycle plan (`docs/plans/2026-11-13-session-lifecycle.md`, SL-1, SL-3, SL-8)
//! needs one answer to «what runs now». Switchboard's records know only the sessions it
//! launched; on the operator's MacBook on 2026-11-13 32 Claude Code processes ran while
//! `sessions` knew one. This reads this user's process table in-process (no `ps`, as
//! `external::claude_swap_activity` does), keeps the Claude Code and Codex processes, and joins
//! each to the launch whose home names it (`.session-pid`), with its folder, its age and when the
//! transcripts of its folder were last written. Reads only.
//!
//! SB-397 D-2: moved from switchboard-runtime; the homes an app launches sessions in, and the
//! launch each one's marker names, are `Accounts`' answers (none without Switchboard).

use crate::accounts::Accounts;
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
/// home an app launches sessions in.
fn transcript_roots(accounts: &dyn Accounts) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = dirs::home_dir()
        .map(|h| vec![h.join(".claude/projects")])
        .unwrap_or_default();
    roots.extend(accounts.launched_homes().iter().map(|h| h.join("projects")));
    roots
}

/// SB-376: the sessions a live Claude Code process holds now, by session id, with its pid and the
/// status Claude Code reports (`busy`, `idle`). Claude Code writes `sessions/<pid>.json` in its
/// config folder (the person's `~/.claude` and each home Switchboard launches in); a file whose pid
/// is no longer a Claude Code process is left over and ignored. A host reads this before it starts
/// a stopped session again: on 2026-11-14 Fabric Dashboards resumed this repository's session in a
/// second process while the first still ran in its terminal, and both worked in one worktree.
pub fn live_sessions(accounts: &dyn Accounts) -> HashMap<String, (u32, String)> {
    session_pairs(accounts)
        .into_iter()
        .map(|(session, pid, status)| (session, (pid, status)))
        .collect()
}

/// SB-379: every live Claude Code process with the session id its `sessions/<pid>.json` names,
/// one pair per process: two pairs with one session id are a resumed copy of a live session.
pub fn session_pairs(accounts: &dyn Accounts) -> Vec<(String, u32, String)> {
    agent_processes()
        .map(|procs| pairs_for(accounts, &procs))
        .unwrap_or_default()
}

fn pairs_for(accounts: &dyn Accounts, procs: &[Proc]) -> Vec<(String, u32, String)> {
    let started: HashMap<u32, Option<i64>> = procs
        .iter()
        .filter(|p| p.agent == "claude-code")
        .map(|p| (p.pid, p.started_at))
        .collect();
    let alive: std::collections::HashSet<u32> = started.keys().copied().collect();
    let mut files = Vec::new();
    for dir in session_dirs(accounts) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            // A session file is a few hundred bytes; anything larger is not one.
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if meta.len() > 64 * 1024 {
                continue;
            }
            if let Some(v) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            {
                // SB-450 (review round 3 of fabric-switchboard #836): a file last written before
                // the process now holding its pid started is an older process's (a pid reused,
                // after a restart above all), never that process's session.
                let written = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
                let begun = pid.and_then(|p| started.get(&p).copied().flatten());
                if written_before_start(written, begun) {
                    continue;
                }
                files.push(v);
            }
        }
    }
    pairs_from(&files, &alive)
}

/// How much earlier than its process's start a session file may read as written (clock rounding).
const SESSION_FILE_MARGIN: i64 = 60;

/// SB-450: whether a session file written at `written` predates the process that started at
/// `started` (by more than `SESSION_FILE_MARGIN`), so that process did not write it. Unknown
/// times keep the file. Pure.
pub fn written_before_start(written: Option<i64>, started: Option<i64>) -> bool {
    written
        .zip(started)
        .is_some_and(|(w, s)| w + SESSION_FILE_MARGIN < s)
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

/// SB-384: the live Claude Code and Codex processes of this user, as the process table shows them.
pub fn processes() -> Vec<Proc> {
    agent_processes().unwrap_or_default()
}

/// SB-384: every session file (`sessions/<pid>.json`) with the config folder it lies in: the
/// person's `~/.claude`, each home Switchboard launches in and the organizations' shell homes.
pub fn session_files(accounts: &dyn Accounts) -> Vec<(PathBuf, Value)> {
    let mut found = Vec::new();
    for dir in session_dirs(accounts) {
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
                // SB-450 (review round 4 of fabric-switchboard #836): a file written before the
                // process now holding its pid started (a leftover from before a restart) is not
                // that process's session; the duty officer, the pool and the launch records
                // would read an unrelated process as it.
                let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
                if pid
                    .is_some_and(|pid| written_before_start(modified(&path), process_started(pid)))
                {
                    continue;
                }
                found.push((config.clone(), v));
            }
        }
    }
    found
}

/// A file's last write, Unix seconds.
fn modified(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// When `pid` started, Unix seconds, read for that one process (macOS `proc_pidinfo`, Linux
/// `/proc/<pid>`'s time, as the process table's walk reads them); None when unknown.
#[cfg(target_os = "macos")]
fn process_started(pid: u32) -> Option<i64> {
    let pid = i32::try_from(pid).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: one fixed-size read of this pid's BSD info into a zeroed buffer of its size.
    let got = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (got == size).then(|| {
        // SAFETY: filled above, its size checked.
        let info = unsafe { info.assume_init() };
        info.pbi_start_tvsec as i64
    })
}
#[cfg(target_os = "linux")]
fn process_started(pid: u32) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(format!("/proc/{pid}"))
        .ok()
        .map(|m| m.mtime())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_started(_pid: u32) -> Option<i64> {
    None
}

/// SB-384: the newest write (Unix seconds) of one Claude Code session's transcript and of the
/// files under its own folder (its subagents' transcripts), in `config/projects/<folder>/`.
pub fn session_last_write(config: &Path, cwd: &str, session: &str) -> Option<i64> {
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

/// FD-291: the newest write of a session's transcript in any config folder Claude Code may have
/// run it in (the person's `~/.claude` and each home an app launches sessions in), or `None` when
/// no folder holds it. A stop whose transcript was written after it was taken up again.
pub fn transcript_at(accounts: &dyn Accounts, cwd: &str, session: &str) -> Option<i64> {
    // Beside the transcript roots: the app's terminal homes (an organization's terminal writes
    // its transcripts there) and the folder `CLAUDE_CONFIG_DIR` names (R4 review).
    let mut configs: Vec<PathBuf> = transcript_roots(accounts)
        .iter()
        .filter_map(|projects| projects.parent().map(Path::to_path_buf))
        .collect();
    configs.extend(accounts.shell_homes());
    configs.extend(
        std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    configs
        .iter()
        .filter_map(|config| session_last_write(config, cwd, session))
        .max()
}

/// SB-384: the executables of a process's ancestors, nearest first, at most eight.
#[cfg(target_os = "macos")]
pub fn ancestor_paths(pid: u32) -> Vec<String> {
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
pub fn ancestor_paths(pid: u32) -> Vec<String> {
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
pub fn ancestor_paths(_: u32) -> Vec<String> {
    Vec::new()
}

/// Claude Code's `sessions` folders: the person's own and each home the app launches in.
fn session_dirs(accounts: &dyn Accounts) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = dirs::home_dir()
        .map(|h| vec![h.join(".claude/sessions")])
        .unwrap_or_default();
    // SB-384: the organizations' shell homes too (the terminal hook's `CLAUDE_CONFIG_DIR`).
    let homes = accounts
        .launched_homes()
        .into_iter()
        .chain(accounts.shell_homes());
    dirs.extend(homes.map(|h| h.join("sessions")));
    dirs
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

/// This user's coding agent processes, or none under the golden test's switch (SB-397 W1.0).
fn agent_processes() -> Result<Vec<Proc>, &'static str> {
    if crate::test_seams::no_processes() {
        return Ok(Vec::new());
    }
    os_agent_processes()
}

/// Every Claude Code and Codex process of this user, joined to the app's launches.
pub fn scan(accounts: &dyn Accounts) -> Value {
    scan_procs(accounts, agent_processes())
}

/// SB-379: the scan and the live session pairs from one walk of the process table (the watch and
/// the tree need both; two walks each read about a thousand pid paths under load).
pub fn scan_and_pairs(accounts: &dyn Accounts) -> (Value, Vec<(String, u32, String)>) {
    let procs = agent_processes();
    let pairs = procs
        .as_ref()
        .map(|p| pairs_for(accounts, p))
        .unwrap_or_default();
    (scan_procs(accounts, procs), pairs)
}

fn scan_procs(accounts: &dyn Accounts, procs: Result<Vec<Proc>, &'static str>) -> Value {
    // SB-375: a table that could not be read is said so, never answered as no sessions.
    let (procs, degraded) = match procs {
        Ok(procs) => (procs, vec![]),
        Err(reason) => (
            vec![],
            vec![json!({"source": "process_table", "reason": reason})],
        ),
    };
    let markers = accounts.launch_markers();
    let roots = transcript_roots(accounts);
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
    fn a_session_file_older_than_its_process_is_not_its_session() {
        // SB-450: a file from before a restart whose pid a new process holds.
        assert!(written_before_start(Some(1000), Some(2000)));
        assert!(
            !written_before_start(Some(1990), Some(2000)),
            "within the margin"
        );
        assert!(!written_before_start(Some(2100), Some(2000)));
        assert!(!written_before_start(None, Some(2000)), "unknown keeps it");
        assert!(!written_before_start(Some(1000), None));
    }

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
    fn a_transcript_is_found_in_any_home_and_its_newest_write_wins() {
        struct Home(PathBuf);
        impl Accounts for Home {
            fn launched_homes(&self) -> Vec<PathBuf> {
                vec![self.0.clone()]
            }
        }
        let home = tempfile::tempdir().unwrap();
        let accounts = Home(home.path().to_owned());
        assert_eq!(transcript_at(&accounts, "/w/f", "S-none"), None);
        let dir = home.path().join("projects").join(projects_dir_name("/w/f"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("S1.jsonl"), b"{}").unwrap();
        let at = transcript_at(&accounts, "/w/f", "S1").expect("found in the launched home");
        assert!(at > 1_700_000_000);
        // Another folder's transcript of the same id is not this session's.
        assert_eq!(transcript_at(&accounts, "/w/g", "S1"), None);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_session_file_from_before_its_process_started_is_not_listed() {
        // SB-450 (review round 4 of fabric-switchboard #836): a leftover `sessions/<pid>.json`
        // whose pid a newer process holds is not that process's session.
        struct Home(PathBuf);
        impl Accounts for Home {
            fn launched_homes(&self) -> Vec<PathBuf> {
                vec![self.0.clone()]
            }
        }
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let body = json!({"pid": pid, "sessionId": "S-fresh"}).to_string();
        std::fs::write(dir.join("fresh.json"), &body).unwrap();
        let stale = dir.join("stale.json");
        std::fs::write(
            &stale,
            json!({"pid": pid, "sessionId": "S-stale"}).to_string(),
        )
        .unwrap();
        let path = std::ffi::CString::new(stale.to_string_lossy().as_bytes()).unwrap();
        let old = [
            libc::timeval {
                tv_sec: 1_000,
                tv_usec: 0,
            },
            libc::timeval {
                tv_sec: 1_000,
                tv_usec: 0,
            },
        ];
        // SAFETY: sets the times of a file this test wrote.
        assert_eq!(unsafe { libc::utimes(path.as_ptr(), old.as_ptr()) }, 0);
        let mut ours: Vec<String> = session_files(&Home(home.path().to_owned()))
            .into_iter()
            .filter(|(config, _)| config == home.path())
            .filter_map(|(_, v)| v["sessionId"].as_str().map(str::to_owned))
            .collect();
        ours.sort();
        assert_eq!(ours, ["S-fresh"]);
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
        let answer = scan(&crate::accounts::NoAccounts);
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
