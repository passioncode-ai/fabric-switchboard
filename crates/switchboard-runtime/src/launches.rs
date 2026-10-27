//! SB-212: one record per launch, who launched it, what it did and where the work went.
//!
//! A launch home is reused (`runtimes/<provider>-<pool>`, `homes/<account>`), so its
//! `.session.json` names only the latest launch. Every launch that writes one also writes
//! `<data>/launches/<launch_id>.json` (0600): the same fields plus `launched_by` and the home it
//! ran in. The session's own hooks add what it did (`provider_session_id`, `transcript`,
//! `last_turn_at`, `turns`, `ended`), and a later launch that took the work over sets
//! `continued_as`. Records are kept 90 days, like the transcripts they point to. Ids, folders,
//! times and names only: never a credential, a prompt or a command line.
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    future::Future,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};

/// The folder of launch records, inside the data folder.
pub const DIR: &str = "launches";
/// The variable every launched session carries: its own launch id. A session that launches
/// another passes it on, so the new launch names its caller with no help.
pub const ENV: &str = "SWITCHBOARD_LAUNCH_ID";
/// How long a record is kept after its last write: the transcripts' own retention.
pub const KEEP_SECONDS: i64 = 90 * 86_400;
/// How often the owner's background pass looks for expired records.
const PRUNE_EVERY: i64 = 3_600;
/// A record is a few hundred bytes; anything larger is not one of ours.
const MAX_RECORD: u64 = 64 * 1024;
/// At most this many records a history answer lists, newest first (`total` counts them all).
pub const HISTORY_LIMIT: usize = 500;
/// How far a chain is followed in each direction.
const MAX_CHAIN: usize = 64;
pub const BY_INVALID: &str =
    "Name the launcher with up to 64 letters, digits, dots, hyphens or underscores.";
pub const PARENT_INVALID: &str =
    "Give a launch id from switchboard sessions --history as the parent.";
pub const LAUNCH_NOT_FOUND: &str =
    "No launch has that id. List them with switchboard sessions --history.";
pub const NOT_A_LAUNCH: &str = "Only a launch can name who launched it.";

/// Where a launch was asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    /// The Switchboard window (the app's own process).
    #[default]
    Window,
    /// The `switchboard` command, or a client of the control listener.
    Cli,
    /// `switchboard mcp`.
    Mcp,
    /// The automatic fallback (SB-73) or its hand-over.
    Fallback,
    /// A host resuming a session that stopped on a limit (SB-114).
    Recovery,
    /// Fabric Dashboards' console, which says so with `--surface dashboards`.
    Dashboards,
}

/// Who launched a session. A request may carry `caller_folder`; the owner turns it into
/// `caller_project` (the group's pool, or none) and never stores the folder.
/// Unknown fields are ignored, not refused: a later command may add one, and refusing it would
/// make a launch fail in the update window (N-7).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LaunchedBy {
    pub surface: Surface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_launch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_folder: Option<PathBuf>,
}

impl LaunchedBy {
    pub fn surface(surface: Surface) -> Self {
        Self {
            surface,
            ..Self::default()
        }
    }
    /// What the calling process knows about itself: its own launch id when Switchboard launched
    /// it (`SWITCHBOARD_LAUNCH_ID`), the folder it runs in and its parent program's name.
    pub fn from_caller(surface: Surface, by: Option<String>, parent: Option<String>) -> Self {
        Self {
            surface,
            caller_launch_id: std::env::var(ENV).ok().filter(|id| valid_id(id)),
            caller_project: None,
            caller_program: caller_program(),
            by,
            parent,
            caller_folder: std::env::current_dir().ok(),
        }
    }
    /// The record's `launched_by`: every key present, `null` when unknown.
    fn stored(&self) -> Value {
        json!({
            "surface": self.surface,
            "caller_launch_id": self.caller_launch_id,
            "caller_project": self.caller_project,
            "caller_program": self.caller_program,
            "by": self.by,
            "parent": self.parent,
        })
    }
}

/// A launch id: a UUID, or what an earlier writer used, within 64 safe characters.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
/// A runner's name for itself (`--by`).
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
/// An identifier a provider gave its session (`session_id` in a hook's input).
fn valid_session(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A program's name from what the system reports for it: the first word, without its folder,
/// in safe characters. Never an argument: whatever follows the first space is dropped.
pub fn program_name(raw: &str) -> Option<String> {
    let first = raw.split_whitespace().next()?;
    let base = first.rsplit(['/', '\\']).next()?;
    let kept: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(64)
        .collect();
    (!kept.is_empty()).then_some(kept)
}
/// The name of the program that started this process (the shell, an agent, a console), read
/// from the kernel: the name only, never its command line.
pub fn caller_program() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `getppid` cannot fail; `proc_name` writes at most `buffer.len()` bytes.
        let parent = unsafe { libc::getppid() };
        let mut buffer = [0u8; 256];
        let written = unsafe {
            libc::proc_name(
                parent,
                buffer.as_mut_ptr().cast::<libc::c_void>(),
                buffer.len() as u32,
            )
        };
        if written <= 0 {
            return None;
        }
        program_name(&String::from_utf8_lossy(&buffer[..written as usize]))
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `getppid` cannot fail.
        let parent = unsafe { libc::getppid() };
        fs::read_to_string(format!("/proc/{parent}/comm"))
            .ok()
            .and_then(|name| program_name(&name))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// The owner's checks on a request's `launched_by`: `by` and `parent` are refused when
/// malformed, a malformed caller id is dropped, and the caller's folder becomes its group.
pub(crate) fn resolve(
    snapshot: &switchboard_core::Snapshot,
    mut by: LaunchedBy,
) -> Result<LaunchedBy, String> {
    if by.by.as_deref().is_some_and(|b| !valid_name(b)) {
        return Err(BY_INVALID.into());
    }
    if by.parent.as_deref().is_some_and(|p| !valid_id(p)) {
        return Err(PARENT_INVALID.into());
    }
    by.caller_launch_id = by.caller_launch_id.filter(|id| valid_id(id));
    by.caller_program = by.caller_program.as_deref().and_then(program_name);
    by.caller_project = by
        .caller_folder
        .take()
        .filter(|f| f.is_absolute())
        .and_then(|f| f.canonicalize().ok())
        .and_then(|f| snapshot.project_for(&f).map(|p| p.pool.clone()));
    Ok(by)
}

struct Scope {
    by: LaunchedBy,
    /// The launch this operation recorded and the data folder its record is in.
    recorded: Arc<Mutex<Option<(String, PathBuf)>>>,
}
tokio::task_local! {
    static SCOPE: Scope;
}
/// The launch the running operation was asked for by; the window when nothing says otherwise.
fn current_by() -> LaunchedBy {
    SCOPE.try_with(|s| s.by.clone()).unwrap_or_default()
}
/// Whether an outer operation already collects the launch id (`ContinueToConsole` runs a
/// `Continue` inside itself).
pub(crate) fn capturing() -> bool {
    SCOPE.try_with(|_| ()).is_ok()
}
/// The launch id the running operation's launch recorded so far, if any.
pub(crate) fn recorded() -> Option<String> {
    SCOPE
        .try_with(|s| {
            s.recorded
                .lock()
                .ok()
                .and_then(|id| id.as_ref().map(|(id, _)| id.clone()))
        })
        .ok()
        .flatten()
}
/// Runs a launch operation with `by` (or the enclosing operation's) as who asked for it, and
/// adds the launch id its record got to its answer.
pub(crate) async fn scoped<F>(by: Option<LaunchedBy>, operation: F) -> Result<Value, String>
where
    F: Future<Output = Result<Value, String>>,
{
    let by = by.unwrap_or_else(current_by);
    let recorded = Arc::new(Mutex::new(None));
    let result = SCOPE
        .scope(
            Scope {
                by,
                recorded: recorded.clone(),
            },
            operation,
        )
        .await;
    let id = recorded.lock().ok().and_then(|mut id| id.take());
    // N-1: a launch that failed after its record was written never started; nothing continues
    // as it, so a later launch that starts takes the link.
    let id = match (&result, id) {
        (Err(_), Some((id, root))) => {
            not_started(&root, &id, crate::monitor::now());
            None
        }
        (_, id) => id.map(|(id, _)| id),
    };
    result.map(|mut value| {
        if let (Some(id), Some(answer)) = (id, value.as_object_mut()) {
            answer.insert("launch_id".into(), id.into());
        }
        value
    })
}

static WRITES: Mutex<()> = Mutex::new(());
fn writing() -> std::sync::MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(|p| p.into_inner())
}
fn path(root: &Path, id: &str) -> PathBuf {
    root.join(DIR).join(format!("{id}.json"))
}
fn read(root: &Path, id: &str) -> Option<Value> {
    if !valid_id(id) {
        return None;
    }
    // Not `read_private_strict`, which is for capabilities: a record holds none, and a strict
    // read drops one whose access list was widened (a copied folder, a Windows file that
    // inherited its folder's list) from the history; this read narrows it again.
    switchboard_core::private_fs::read_private(&path(root, id), MAX_RECORD)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
}
fn write(root: &Path, id: &str, record: &Value) -> Result<(), String> {
    switchboard_core::private_fs::private_dir(&root.join(DIR))?;
    switchboard_core::private_fs::private_write(&path(root, id), record.to_string().as_bytes())
}
fn update(root: &Path, id: &str, change: impl FnOnce(&mut Value) -> bool) -> bool {
    let Some(mut record) = read(root, id) else {
        return false;
    };
    change(&mut record) && write(root, id, &record).is_ok()
}
#[cfg(test)]
thread_local! {
    /// How many full scans of the records this thread made (SB-219).
    static SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// Every record, by id. Only `<id>.json` regular files are read.
fn all(root: &Path) -> BTreeMap<String, Value> {
    #[cfg(test)]
    SCANS.with(|n| n.set(n.get() + 1));
    let mut found = BTreeMap::new();
    let Ok(entries) = fs::read_dir(root.join(DIR)) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
            continue;
        };
        if let Some(record) = read(root, id) {
            found.insert(id.to_owned(), record);
        }
    }
    found
}

/// SB-219: the launches that continued each workflow, `launches/by-workflow/<workflow>.json` (a
/// list of launch ids, oldest first, at most `WORKFLOW_INDEX_MAX`), so linking a continuation
/// reads those records instead of every record. A workflow with no index (records from before
/// it) is found by one full scan, which seeds its index.
const BY_WORKFLOW: &str = "by-workflow";
const WORKFLOW_INDEX_MAX: usize = 64;
fn workflow_key(workflow: &str) -> Option<&str> {
    ((1..=64).contains(&workflow.len())
        && workflow
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then_some(workflow)
}
fn index_path(root: &Path, workflow: &str) -> PathBuf {
    root.join(DIR)
        .join(BY_WORKFLOW)
        .join(format!("{workflow}.json"))
}
fn indexed(root: &Path, workflow: &str) -> Option<Vec<String>> {
    let bytes =
        switchboard_core::private_fs::read_private(&index_path(root, workflow), MAX_RECORD).ok()?;
    let ids: Vec<String> = serde_json::from_slice(&bytes).ok()?;
    Some(ids.into_iter().filter(|id| valid_id(id)).collect())
}
fn index_write(root: &Path, workflow: &str, mut ids: Vec<String>) {
    let mut seen = BTreeSet::new();
    ids.reverse();
    ids.retain(|id| seen.insert(id.clone()));
    ids.truncate(WORKFLOW_INDEX_MAX);
    ids.reverse();
    let folder = root.join(DIR).join(BY_WORKFLOW);
    if switchboard_core::private_fs::private_dir(&folder).is_ok() {
        let _ = switchboard_core::private_fs::private_write(
            &index_path(root, workflow),
            json!(ids).to_string().as_bytes(),
        );
    }
}
/// The records a back-pointer names (`continued_from`, SB-219), or every record when the record
/// has none (written before it).
fn pointing_at(root: &Path, id: &str) -> Vec<(String, Value)> {
    let from = read(root, id).and_then(|r| {
        r["continued_from"].as_array().map(|ids| {
            ids.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
    });
    match from {
        Some(ids) => ids
            .into_iter()
            .filter_map(|other| read(root, &other).map(|r| (other, r)))
            .collect(),
        None => all(root).into_iter().collect(),
    }
}

/// Writes a launch's `.session.json` in `dir` (its failure fails the launch, as before) and its
/// record in `launches/` (best effort: a launch never fails for its history), then links it to
/// the launch it continues. `record` is what `.session.json` held before SB-212.
pub(crate) fn record(root: &Path, dir: &Path, mut record: Value, id: &str) -> Result<(), String> {
    let by = current_by();
    record["launch_id"] = id.into();
    record["launched_by"] = by.stored();
    crate::launch::private_write(
        &dir.join(crate::launch::SESSION_RECORD),
        record.to_string().as_bytes(),
        false,
    )?;
    let _ = SCOPE.try_with(|s| {
        if let Ok(mut recorded) = s.recorded.lock() {
            *recorded = Some((id.to_owned(), root.to_owned()));
        }
    });
    record["home_path"] = dir
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .into();
    // SB-219: file work under the records' lock, off the async workers.
    crate::blocking::run(|| {
        let _writing = writing();
        if write(root, id, &record).is_err() {
            crate::oplog::event(
                "launch_record",
                &[("result", crate::oplog::Field::Code("failed"))],
            );
            return;
        }
        link(root, id, &record, by.parent.as_deref());
    });
    Ok(())
}
/// Where the work went: the parent a runner named, and the latest earlier launch that continued
/// the same workflow, now continue as this launch, unless something already took them over. The
/// launch names the records it took over in `continued_from` (SB-219), so undoing or moving the
/// link reads those records only; the workflow's earlier launches come from its index.
fn link(root: &Path, id: &str, record: &Value, parent: Option<&str>) {
    let continued = |r: &mut Value| {
        if !r["continued_as"].is_null() {
            return false;
        }
        r["continued_as"] = json!({"kind": "launch", "id": id});
        true
    };
    let mut from: Vec<String> = Vec::new();
    if let Some(parent) = parent.filter(|p| *p != id) {
        if update(root, parent, continued) {
            from.push(parent.to_owned());
        }
    }
    if let Some(workflow) = record["continues"].as_str() {
        let key = workflow_key(workflow);
        let (candidates, ids): (Vec<(String, Value)>, Vec<String>) =
            match key.and_then(|k| indexed(root, k)) {
                Some(ids) => (
                    ids.iter()
                        .filter_map(|other| read(root, other).map(|r| (other.clone(), r)))
                        .collect(),
                    ids,
                ),
                None => {
                    let mut same: Vec<(String, Value)> = all(root)
                        .into_iter()
                        .filter(|(_, r)| r["continues"].as_str() == Some(workflow))
                        .collect();
                    same.sort_by_key(|(_, r)| r["launched_at"].as_i64().unwrap_or(0));
                    let ids = same.iter().map(|(k, _)| k.clone()).collect();
                    (same, ids)
                }
            };
        let previous = candidates
            .into_iter()
            .filter(|(other, r)| {
                other != id
                    && r["continues"].as_str() == Some(workflow)
                    && r["continued_as"].is_null()
                    && r["ended"] != NOT_STARTED
            })
            .max_by_key(|(_, r)| r["launched_at"].as_i64().unwrap_or(0))
            .map(|(other, _)| other);
        if let Some(previous) = previous {
            if update(root, &previous, continued) {
                from.push(previous);
            }
        }
        if let Some(key) = key {
            let mut ids = ids;
            ids.push(id.to_owned());
            index_write(root, key, ids);
        }
    }
    update(root, id, |r| {
        r["continued_from"] = json!(from);
        true
    });
}
/// `ended` of a launch whose operation failed after its record was written.
pub const NOT_STARTED: &str = "not_started";
/// The launch `id` never started (its operation failed after the record was written): it says
/// so, and a launch that named it as where its work went no longer does.
fn not_started(root: &Path, id: &str, now: i64) {
    if !valid_id(id) {
        return;
    }
    crate::blocking::run(|| {
        let _writing = writing();
        update(root, id, |r| {
            r["ended"] = NOT_STARTED.into();
            r["ended_at"] = now.into();
            true
        });
        for (other, r) in pointing_at(root, id) {
            let to = &r["continued_as"];
            if to["id"].as_str() == Some(id) || to["launch_id"].as_str() == Some(id) {
                update(root, &other, |r| {
                    r["continued_as"] = Value::Null;
                    true
                });
            }
        }
    })
}
/// A hand-over left the launch `id` for a console (SB-108): the launch names it, and the launch
/// it continues now says the work went through that hand-over.
pub(crate) fn link_handover(root: &Path, id: &str, handover: &str) {
    if !valid_id(id) {
        return;
    }
    crate::blocking::run(|| {
        let _writing = writing();
        update(root, id, |r| {
            r["handover"] = handover.into();
            true
        });
        let previous = pointing_at(root, id).into_iter().find(|(_, r)| {
            r["continued_as"]["kind"] == "launch" && r["continued_as"]["id"].as_str() == Some(id)
        });
        if let Some((previous, _)) = previous {
            update(root, &previous, |r| {
                r["continued_as"] = json!({"kind": "handover", "id": handover, "launch_id": id});
                true
            });
        }
    })
}

/// What a session's own hook reports about its launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// `SessionStart`: the provider's session id and its transcript.
    Start,
    /// `Stop`: a turn finished.
    Turn,
    /// `StopFailure` on a rate or spend limit: the session stopped there.
    Limit,
}
/// Records a hook's report on launch `id`; `recorded: false` when there is no such record or
/// the report is malformed. Never an error: a hook must not fail its session. The owner calls
/// `event_from`, which also takes the start's `source`.
#[cfg(test)]
pub(crate) fn event(
    root: &Path,
    id: &str,
    kind: Event,
    session: Option<&str>,
    transcript: Option<&Path>,
    now: i64,
) -> Value {
    event_from(root, id, kind, session, transcript, None, now)
}
/// At most this many nested sessions a record names (SB-218).
const MAX_NESTED: usize = 16;
/// `SessionStart`'s `source` values that start a new session in the same process (Claude Code's
/// `/clear`, a compaction, a resume): the launch's own session, under a new id.
fn same_process(source: Option<&str>) -> bool {
    matches!(source, Some("clear" | "compact" | "resume"))
}
/// `event` with the `SessionStart` hook's `source` (SB-218). A launched session's children
/// inherit `SWITCHBOARD_LAUNCH_ID`, so a nested `claude -p` in the same home runs the same hooks
/// on the same record. A start under a session id other than the one stored, unless its `source`
/// says the launch's own process moved to a new session, is a nested session: it is named in
/// `nested_sessions` (at most `MAX_NESTED`) and never replaces the parent's session or
/// transcript, and a turn or limit of a session other than the stored one changes nothing
/// (`{"recorded": false, "nested": true}`).
pub(crate) fn event_from(
    root: &Path,
    id: &str,
    kind: Event,
    session: Option<&str>,
    transcript: Option<&Path>,
    source: Option<&str>,
    now: i64,
) -> Value {
    let session = session.filter(|s| valid_session(s));
    let transcript = transcript.filter(|t| {
        t.is_absolute()
            && t.as_os_str().len() <= 4096
            && !t.to_string_lossy().chars().any(char::is_control)
    });
    // N-5: a start names the provider's session or its transcript; any other object is not a
    // `SessionStart` input and writes nothing.
    if kind == Event::Start && session.is_none() && transcript.is_none() {
        return json!({"recorded": false});
    }
    // SB-219: one small file under the records' lock, off the async workers.
    crate::blocking::run(|| {
        let _writing = writing();
        let mut nested = false;
        let recorded = update(root, id, |r| {
            let other = |r: &Value| match (r["provider_session_id"].as_str(), session) {
                (Some(stored), Some(session)) => stored != session,
                _ => false,
            };
            if other(r) && !(kind == Event::Start && same_process(source)) {
                nested = true;
                if kind != Event::Start {
                    return false;
                }
                let session = session.unwrap_or_default();
                let list = r["nested_sessions"].as_array().cloned().unwrap_or_default();
                if list.iter().any(|s| s == session) || list.len() >= MAX_NESTED {
                    return false;
                }
                let mut list = list;
                list.push(session.into());
                r["nested_sessions"] = list.into();
                return true;
            }
            match kind {
                Event::Start => {
                    if let Some(session) = session {
                        r["provider_session_id"] = session.into();
                    }
                    if let Some(transcript) = transcript {
                        r["transcript"] = transcript.to_string_lossy().into_owned().into();
                    }
                    r["session_started_at"] = now.into();
                }
                Event::Turn => {
                    r["last_turn_at"] = now.into();
                    r["turns"] = (r["turns"].as_i64().unwrap_or(0) + 1).into();
                }
                Event::Limit => {
                    r["ended"] = "limit".into();
                    r["ended_at"] = now.into();
                }
            }
            true
        });
        if nested {
            return json!({"recorded": recorded && kind == Event::Start, "nested": true});
        }
        json!({"recorded": recorded})
    })
}

/// The home a record ran in, when it is one of Switchboard's own (never a path out of them).
fn home_of(root: &Path, record: &Value) -> Option<PathBuf> {
    let relative = Path::new(record["home_path"].as_str()?);
    let mut components = relative.components();
    let first = components.next()?;
    if !matches!(first, Component::Normal(f) if f == "homes" || f == "runtimes" || f == "kimi") {
        return None;
    }
    if !components.all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(relative))
}
/// The record with its state now: its home's own state while the home still holds this launch,
/// else `ended` (a later launch of the same home replaced it, which only happens after it ended).
fn with_state(root: &Path, mut record: Value) -> Value {
    let id = record["launch_id"].as_str().map(str::to_owned);
    let home = home_of(root, &record);
    let current = home.as_ref().and_then(|home| {
        // Read as the session list reads it (`launch::read_regular`), so the two never differ.
        switchboard_core::private_fs::read_private(
            &home.join(crate::launch::SESSION_RECORD),
            MAX_RECORD,
        )
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    });
    let (state, started) = match (&home, current) {
        (Some(home), Some(current)) if current["launch_id"].as_str() == id.as_deref() => {
            crate::launch::home_state(home)
        }
        _ => ("ended", None),
    };
    record["state"] = state.into();
    if record["started_at"].is_null() {
        record["started_at"] = started.into();
    }
    record
}
/// Every launch record, newest first, with its state now; at most `HISTORY_LIMIT`.
/// What a session list row learns from its launch's record (fabric-dashboards, board
/// `62d78b7f`, 2026-10-26): how the session ended, where its work went and its transcript, so a
/// plain session list says it without `history`.
pub(crate) const OUTCOME_FIELDS: [&str; 8] = [
    "provider_session_id",
    "transcript",
    "session_started_at",
    "last_turn_at",
    "turns",
    "ended",
    "ended_at",
    "continued_as",
];
/// Copies `OUTCOME_FIELDS` from the record of the launch a `.session.json` names onto that row;
/// a missing or unreadable record leaves the row as it was.
pub(crate) fn add_outcome(root: &Path, row: &mut Value) {
    let Some(stored) = row["launch_id"].as_str().and_then(|id| read(root, id)) else {
        return;
    };
    for field in OUTCOME_FIELDS {
        if !stored[field].is_null() {
            row[field] = stored[field].clone();
        }
    }
}
pub fn history(root: &Path) -> (Vec<Value>, usize) {
    let mut records: Vec<Value> = all(root).into_values().collect();
    records.sort_by_key(|r| std::cmp::Reverse(r["launched_at"].as_i64().unwrap_or(0)));
    let total = records.len();
    let listed = records
        .into_iter()
        .take(HISTORY_LIMIT)
        .map(|r| with_state(root, r))
        .collect();
    (listed, total)
}
fn short(record: &Value) -> Value {
    let mut out = Map::new();
    for key in [
        "launch_id",
        "account_id",
        "provider",
        "agent",
        "pool",
        "working_directory",
        "launched_at",
        "continues",
        "ended",
        "continued_as",
        "launched_by",
    ] {
        out.insert(key.into(), record[key].clone());
    }
    Value::Object(out)
}
/// One launch with its chain: `up` follows who launched it (`parent`, else `caller_launch_id`,
/// else the launch that continued as it) to the first; `down` lists the launches it started
/// and the ones that took its work over, breadth first.
pub fn show(root: &Path, id: &str) -> Result<Value, String> {
    if !valid_id(id) {
        return Err(LAUNCH_NOT_FOUND.into());
    }
    let records = all(root);
    let launch = records.get(id).cloned().ok_or(LAUNCH_NOT_FOUND)?;
    let successor = |r: &Value| -> Option<String> {
        let to = &r["continued_as"];
        to["launch_id"]
            .as_str()
            .or_else(|| {
                (to["kind"] == "launch")
                    .then(|| to["id"].as_str())
                    .flatten()
            })
            .map(str::to_owned)
    };
    let mut up = Vec::new();
    let mut seen = BTreeSet::from([id.to_owned()]);
    let mut current = launch.clone();
    while up.len() < MAX_CHAIN {
        let by = &current["launched_by"];
        let (next, via) = if let Some(p) = by["parent"].as_str() {
            (Some(p.to_owned()), "parent")
        } else if let Some(c) = by["caller_launch_id"].as_str() {
            (Some(c.to_owned()), "caller")
        } else {
            let me = current["launch_id"].as_str().unwrap_or_default().to_owned();
            (
                records
                    .iter()
                    .find(|(_, r)| successor(r).as_deref() == Some(me.as_str()))
                    .map(|(k, _)| k.clone()),
                "continued_as",
            )
        };
        let Some(next) = next.filter(|n| seen.insert(n.clone())) else {
            break;
        };
        match records.get(&next) {
            Some(r) => {
                let mut row = short(r);
                row["via"] = via.into();
                up.push(row);
                current = r.clone();
            }
            None => {
                up.push(json!({"launch_id": next, "via": via, "missing": true}));
                break;
            }
        }
    }
    let mut down = Vec::new();
    let mut queue = std::collections::VecDeque::from([(id.to_owned(), 0usize)]);
    let mut seen = BTreeSet::from([id.to_owned()]);
    while let Some((from, depth)) = queue.pop_front() {
        let Some(record) = records.get(&from) else {
            continue;
        };
        let mut next: Vec<(String, &str)> = records
            .iter()
            .filter(|(_, r)| {
                r["launched_by"]["parent"].as_str() == Some(from.as_str())
                    || r["launched_by"]["caller_launch_id"].as_str() == Some(from.as_str())
            })
            .map(|(k, _)| (k.clone(), "child"))
            .collect();
        if let Some(s) = successor(record) {
            next.push((s, "continued_as"));
        }
        for (other, via) in next {
            if down.len() >= MAX_CHAIN || !seen.insert(other.clone()) {
                continue;
            }
            let mut row = records
                .get(&other)
                .map_or_else(|| json!({"launch_id": other, "missing": true}), short);
            row["via"] = via.into();
            row["from"] = from.clone().into();
            row["depth"] = (depth + 1).into();
            down.push(row);
            queue.push_back((other, depth + 1));
        }
    }
    Ok(json!({"launch": with_state(root, launch), "up": up, "down": down}))
}

/// Removes records not written for `KEEP_SECONDS`; `<id>.json` regular files only. Returns how
/// many went.
pub fn prune(root: &Path, now: std::time::SystemTime) -> usize {
    let Ok(entries) = fs::read_dir(root.join(DIR)) else {
        return 0;
    };
    let keep = std::time::Duration::from_secs(KEEP_SECONDS as u64);
    let _writing = writing();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name
            .to_str()
            .and_then(|n| n.strip_suffix(".json"))
            .is_some_and(valid_id)
        {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > keep);
        if meta.file_type().is_file() && old && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    // SB-219: a workflow's index not written for as long goes too; it names no kept record.
    if let Ok(entries) = fs::read_dir(root.join(DIR).join(BY_WORKFLOW)) {
        for entry in entries.flatten() {
            let old = fs::symlink_metadata(entry.path()).is_ok_and(|meta| {
                meta.file_type().is_file()
                    && meta
                        .modified()
                        .ok()
                        .and_then(|t| now.duration_since(t).ok())
                        .is_some_and(|age| age > keep)
            });
            if old {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    removed
}
static PRUNED_AT: AtomicI64 = AtomicI64::new(0);
/// The background pass's prune: at the owner's first pass, then at most once an hour.
pub(crate) fn prune_due(root: &Path, now: i64) {
    let last = PRUNED_AT.load(Ordering::Relaxed);
    if now - last < PRUNE_EVERY {
        return;
    }
    PRUNED_AT.store(now, Ordering::Relaxed);
    let removed = prune(root, std::time::SystemTime::now());
    if removed > 0 {
        crate::oplog::event(
            "launch_records_pruned",
            &[("count", crate::oplog::Field::Number(removed as i64))],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, String) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let home = root.join("runtimes").join("claude-default");
        switchboard_core::private_fs::private_dir(&home).unwrap();
        let id = new_id();
        record(
            &root,
            &home,
            json!({"account_id": "a", "provider": "claude", "launched_at": 100}),
            &id,
        )
        .unwrap();
        (temp, root, id)
    }
    fn read_back(root: &Path, id: &str) -> Value {
        read(root, id).unwrap()
    }
    /// A transcript path as Claude Code reports one: absolute on this platform. `/synthetic/...`
    /// names no drive, so on Windows it is not absolute and the hooks rightly record nothing.
    fn transcript_at(name: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(
                r"C:\Users\synthetic\.claude\projects\C--work-x\{name}"
            ))
        } else {
            PathBuf::from(format!("/synthetic/home/projects/x/{name}"))
        }
    }

    #[test]
    fn the_start_and_stop_hooks_record_the_session_its_transcript_and_its_turns() {
        let (_temp, root, id) = fixture();
        let transcript = transcript_at("0f3c.jsonl");
        let transcript = transcript.as_path();
        let answer = event(
            &root,
            &id,
            Event::Start,
            Some("0f3c-session"),
            Some(transcript),
            200,
        );
        assert_eq!(answer, json!({"recorded": true}));
        event(&root, &id, Event::Turn, None, None, 210);
        event(&root, &id, Event::Turn, None, None, 220);
        let r = read_back(&root, &id);
        assert_eq!(r["provider_session_id"], "0f3c-session");
        assert_eq!(r["transcript"], transcript.to_string_lossy().as_ref());
        assert_eq!(r["session_started_at"], 200);
        assert_eq!(r["turns"], 2);
        assert_eq!(r["last_turn_at"], 220);
        assert!(r["ended"].is_null());
        event(&root, &id, Event::Limit, None, None, 230);
        let r = read_back(&root, &id);
        assert_eq!(
            (r["ended"].clone(), r["ended_at"].clone()),
            (json!("limit"), json!(230))
        );
    }
    #[test]
    fn a_plain_session_list_says_how_a_launch_ended_and_where_its_work_went() {
        let (_temp, root, id) = fixture();
        let transcript = transcript_at("0f3c.jsonl");
        let transcript = transcript.as_path();
        event(
            &root,
            &id,
            Event::Start,
            Some("0f3c-session"),
            Some(transcript),
            200,
        );
        event(&root, &id, Event::Limit, None, None, 230);
        update(&root, &id, |r| {
            r["continued_as"] = json!({"kind": "launch", "id": "next"});
            true
        });
        let listed = crate::launch::sessions(&root, true);
        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        assert_eq!(row["launch_id"], id.as_str());
        assert_eq!(row["ended"], "limit");
        assert_eq!(row["ended_at"], 230);
        assert_eq!(row["continued_as"], json!({"kind": "launch", "id": "next"}));
        assert_eq!(row["transcript"], transcript.to_string_lossy().as_ref());
        assert_eq!(row["provider_session_id"], "0f3c-session");
        // A record that is gone leaves the row as `.session.json` has it.
        fs::remove_file(path(&root, &id)).unwrap();
        let row = crate::launch::sessions(&root, true).remove(0);
        assert!(row["ended"].is_null() && row["transcript"].is_null());
    }
    /// Windows: Claude Code names its transcript with a drive (`C:\Users\...\.claude\projects\...`,
    /// also with `/` separators); a path without a drive is not absolute there and is not
    /// recorded.
    #[cfg(windows)]
    #[test]
    fn a_windows_transcript_is_recorded_and_one_without_a_drive_is_not() {
        for (spelling, kept) in [
            (r"C:\Users\x\.claude\projects\C--work-repo\0f3c.jsonl", true),
            ("C:/Users/x/.claude/projects/C--work-repo/0f3c.jsonl", true),
            (
                r"\\?\C:\Users\x\.claude\projects\C--work-repo\0f3c.jsonl",
                true,
            ),
            (r"\Users\x\.claude\projects\0f3c.jsonl", false),
            ("/synthetic/home/projects/x/0f3c.jsonl", false),
            (r"C:Users\x\0f3c.jsonl", false),
        ] {
            let (_temp, root, id) = fixture();
            event(
                &root,
                &id,
                Event::Start,
                Some("0f3c"),
                Some(Path::new(spelling)),
                5,
            );
            let r = read_back(&root, &id);
            assert_eq!(r["provider_session_id"], "0f3c", "{spelling}");
            if kept {
                assert_eq!(r["transcript"], spelling, "{spelling}");
            } else {
                assert!(r["transcript"].is_null(), "{spelling}");
            }
        }
    }
    #[test]
    fn a_malformed_report_or_an_unknown_launch_records_nothing() {
        let (_temp, root, id) = fixture();
        event(
            &root,
            &id,
            Event::Start,
            Some("has spaces; rm -rf"),
            Some(Path::new("relative/path.jsonl")),
            5,
        );
        let r = read_back(&root, &id);
        assert!(r["provider_session_id"].is_null());
        assert!(r["transcript"].is_null());
        for unknown in ["0000", "../escape", "", &"a".repeat(65)] {
            assert_eq!(
                event(&root, unknown, Event::Turn, None, None, 5),
                json!({"recorded": false}),
                "{unknown}"
            );
        }
        assert!(!root.join("escape.json").exists());
    }
    #[test]
    fn records_older_than_ninety_days_are_pruned_and_nothing_else_is() {
        let (_temp, root, id) = fixture();
        let other = new_id();
        let home = root.join("runtimes").join("claude-default");
        record(&root, &home, json!({"launched_at": 1}), &other).unwrap();
        let stray = root.join(DIR).join("notes.txt");
        fs::write(&stray, "kept").unwrap();
        let now = std::time::SystemTime::now();
        assert_eq!(prune(&root, now), 0);
        let later = now + std::time::Duration::from_secs(KEEP_SECONDS as u64 + 60);
        assert_eq!(prune(&root, later), 2);
        assert!(read(&root, &id).is_none());
        assert!(stray.exists(), "only launch records are pruned");
    }
    #[test]
    fn a_program_name_never_carries_its_arguments() {
        assert_eq!(program_name("zsh").as_deref(), Some("zsh"));
        assert_eq!(
            program_name("/usr/local/bin/node --token=synthetic-secret x").as_deref(),
            Some("node")
        );
        assert_eq!(program_name("claude\n").as_deref(), Some("claude"));
        assert_eq!(program_name("   ").as_deref(), None);
        assert_eq!(program_name("we$ird;name").as_deref(), Some("weirdname"));
        let own = caller_program();
        if let Some(own) = own {
            assert!(!own.contains(' ') && !own.contains('/'), "{own}");
        }
    }
    #[test]
    fn the_owner_checks_by_and_parent_and_turns_the_folder_into_its_group() {
        let snapshot = switchboard_core::Snapshot::default();
        let ok = LaunchedBy {
            surface: Surface::Cli,
            by: Some("voc-learn.v2_x".into()),
            parent: Some(new_id()),
            caller_launch_id: Some("not a launch id".into()),
            caller_program: Some("/bin/zsh -l".into()),
            caller_folder: Some(std::env::temp_dir()),
            caller_project: Some("forged".into()),
        };
        let resolved = resolve(&snapshot, ok).unwrap();
        assert!(resolved.caller_launch_id.is_none());
        assert!(
            resolved.caller_folder.is_none(),
            "the folder is never stored"
        );
        assert!(resolved.caller_project.is_none(), "no group holds it");
        assert_eq!(resolved.caller_program.as_deref(), Some("zsh"));
        for bad in ["", "has space", "slash/name", &"x".repeat(65)] {
            let by = LaunchedBy {
                by: Some(bad.into()),
                ..LaunchedBy::surface(Surface::Cli)
            };
            assert_eq!(resolve(&snapshot, by).unwrap_err(), BY_INVALID, "{bad}");
        }
        let by = LaunchedBy {
            parent: Some("../x".into()),
            ..LaunchedBy::surface(Surface::Cli)
        };
        assert_eq!(resolve(&snapshot, by).unwrap_err(), PARENT_INVALID);
    }
    #[test]
    fn a_hand_over_names_the_launch_it_holds_and_the_one_it_continues_points_through_it() {
        let (_temp, root, first) = fixture();
        let home = root.join("runtimes").join("opencode-continue").join("wf_x");
        switchboard_core::private_fs::private_dir(&home).unwrap();
        // Both continue the same workflow: the second takes the first's work over.
        update(&root, &first, |r| {
            r["continues"] = "wf_0123456789abcdef".into();
            true
        });
        let second = new_id();
        record(
            &root,
            &home,
            json!({"launched_at": 300, "continues": "wf_0123456789abcdef"}),
            &second,
        )
        .unwrap();
        assert_eq!(
            read_back(&root, &first)["continued_as"],
            json!({"kind": "launch", "id": second})
        );
        link_handover(&root, &second, "h-1");
        assert_eq!(read_back(&root, &second)["handover"], "h-1");
        assert_eq!(
            read_back(&root, &first)["continued_as"],
            json!({"kind": "handover", "id": "h-1", "launch_id": second})
        );
        let shown = show(&root, &first).unwrap();
        assert_eq!(shown["down"][0]["launch_id"], second.as_str());
        assert_eq!(shown["down"][0]["via"], "continued_as");
        let shown = show(&root, &second).unwrap();
        assert_eq!(shown["up"][0]["launch_id"], first.as_str());
    }
    /// N-1: a launch whose start failed never becomes where the work went. The launch it would
    /// have continued keeps `continued_as` null, the failed one says it never started and is not
    /// taken for the workflow's latest launch, and the next launch that starts takes the link.
    #[tokio::test]
    async fn a_launch_that_never_started_is_not_where_the_work_went() {
        let (_temp, root, first) = fixture();
        let workflow = "wf_0123456789abcdef";
        update(&root, &first, |r| {
            r["continues"] = workflow.into();
            true
        });
        let home = root.join("runtimes").join("claude-continue");
        switchboard_core::private_fs::private_dir(&home).unwrap();
        let failed = new_id();
        let (r, h, f) = (root.clone(), home.clone(), failed.clone());
        let answer = scoped(Some(LaunchedBy::surface(Surface::Cli)), async move {
            record(
                &r,
                &h,
                json!({"launched_at": 200, "continues": workflow}),
                &f,
            )?;
            Err::<Value, String>("Terminal could not be opened.".into())
        })
        .await;
        assert!(answer.is_err());
        assert!(read_back(&root, &first)["continued_as"].is_null());
        assert_eq!(read_back(&root, &failed)["ended"], "not_started");
        let started = new_id();
        let (r, h, s) = (root.clone(), home.clone(), started.clone());
        let answer = scoped(Some(LaunchedBy::surface(Surface::Cli)), async move {
            record(
                &r,
                &h,
                json!({"launched_at": 300, "continues": workflow}),
                &s,
            )?;
            Ok(json!({"launched": true}))
        })
        .await
        .unwrap();
        assert_eq!(answer["launch_id"], started.as_str());
        assert_eq!(
            read_back(&root, &first)["continued_as"],
            json!({"kind": "launch", "id": started})
        );
        assert!(read_back(&root, &failed)["continued_as"].is_null());
    }
    /// N-5: a JSON object that is not a `SessionStart` input (no `session_id`, no
    /// `transcript_path`) writes nothing, not even `session_started_at`.
    #[test]
    fn a_start_without_a_session_or_a_transcript_writes_nothing() {
        let (_temp, root, id) = fixture();
        let before = read_back(&root, &id);
        let answer = event(&root, &id, Event::Start, None, None, 500);
        assert_eq!(answer["recorded"], false);
        let answer = event(
            &root,
            &id,
            Event::Start,
            Some("not a session id"),
            Some(Path::new("relative/t.jsonl")),
            500,
        );
        assert_eq!(answer["recorded"], false);
        assert_eq!(read_back(&root, &id), before);
    }
    /// N-7: a later command's new `launched_by` field reaches an owner of this version and is
    /// ignored, never the version skew `deny_unknown_fields` made of it.
    #[test]
    fn a_field_this_version_does_not_know_is_ignored() {
        let by: LaunchedBy = serde_json::from_value(json!({
            "surface": "cli", "by": "runner", "a_later_field": {"x": 1}
        }))
        .unwrap();
        assert_eq!(by.by.as_deref(), Some("runner"));
        assert_eq!(by.surface, Surface::Cli);
    }
    #[test]
    fn a_planted_home_path_is_never_followed() {
        let (_temp, root, id) = fixture();
        let mut r = read_back(&root, &id);
        for planted in ["../../etc", "/etc", "elsewhere/x", "runtimes/../../x"] {
            r["home_path"] = planted.into();
            assert!(home_of(&root, &r).is_none(), "{planted}");
        }
    }
    /// A record or a home's `.session.json` whose permissions are not this user's alone (a data
    /// folder copied or restored, a Windows file that inherited its folder's access list, as the
    /// nightly's fixture did) is still history: neither holds a capability, so it is read as
    /// every other record and narrowed again, never dropped (2026-10-27 nightly, Windows).
    #[cfg(unix)]
    #[test]
    fn a_record_whose_permissions_were_widened_stays_in_the_history() {
        use std::os::unix::fs::PermissionsExt;
        let (_temp, root, id) = fixture();
        let home = root.join("runtimes").join("claude-default");
        let widened = |p: &Path| fs::set_permissions(p, fs::Permissions::from_mode(0o644)).unwrap();
        widened(&path(&root, &id));
        widened(&home.join(crate::launch::SESSION_RECORD));
        fs::write(home.join(".launch-pending"), b"").unwrap();
        let transcript = transcript_at("0f3c.jsonl");
        let transcript = transcript.as_path();
        let answer = event(
            &root,
            &id,
            Event::Start,
            Some("0f3c"),
            Some(transcript),
            200,
        );
        assert_eq!(answer, json!({"recorded": true}));
        let (listed, total) = history(&root);
        assert_eq!(total, 1);
        assert_eq!(listed[0]["launch_id"], id.as_str());
        assert_eq!(listed[0]["provider_session_id"], "0f3c");
        assert_eq!(
            listed[0]["state"], "starting",
            "the home still holds this launch, so its state is the home's"
        );
        let mode = fs::metadata(path(&root, &id)).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "narrowed again");
    }

    #[test]
    fn a_nested_session_in_the_same_home_never_overwrites_its_parent() {
        // SB-218: a `claude -p` the launched session runs inherits `SWITCHBOARD_LAUNCH_ID` and
        // its hooks report on the parent's record.
        let (_temp, root, id) = fixture();
        let parent = transcript_at("parent.jsonl");
        let parent = parent.as_path();
        let child = transcript_at("child.jsonl");
        let child = child.as_path();
        event(&root, &id, Event::Start, Some("parent"), Some(parent), 200);
        event(&root, &id, Event::Turn, Some("parent"), None, 210);
        let answer = event(&root, &id, Event::Start, Some("child"), Some(child), 220);
        assert_eq!(answer, json!({"recorded": true, "nested": true}));
        let answer = event(&root, &id, Event::Turn, Some("child"), None, 230);
        assert_eq!(answer, json!({"recorded": false, "nested": true}));
        event(&root, &id, Event::Limit, Some("child"), None, 240);
        let r = read_back(&root, &id);
        assert_eq!(r["provider_session_id"], "parent");
        assert_eq!(r["transcript"], parent.to_string_lossy().as_ref());
        assert_eq!(
            (r["turns"].clone(), r["last_turn_at"].clone()),
            (json!(1), json!(210))
        );
        assert!(r["ended"].is_null(), "a child's limit is not the parent's");
        assert_eq!(r["nested_sessions"], json!(["child"]));
        // Named once.
        event(&root, &id, Event::Start, Some("child"), Some(child), 250);
        assert_eq!(read_back(&root, &id)["nested_sessions"], json!(["child"]));
        // The parent's own `/clear` moves the record to its new session.
        let cleared = transcript_at("cleared.jsonl");
        let cleared = cleared.as_path();
        let answer = event_from(
            &root,
            &id,
            Event::Start,
            Some("cleared"),
            Some(cleared),
            Some("clear"),
            260,
        );
        assert_eq!(answer, json!({"recorded": true}));
        event(&root, &id, Event::Turn, Some("cleared"), None, 270);
        let r = read_back(&root, &id);
        assert_eq!(r["provider_session_id"], "cleared");
        assert_eq!(r["turns"], 2);
    }

    #[test]
    fn linking_a_continuation_among_thousands_of_records_reads_no_full_scan() {
        // SB-219: `link`, `link_handover` and `not_started` read every record under the records'
        // lock. With the workflow's index and the back-pointer they read a handful.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let home = root.join("runtimes").join("claude-default");
        switchboard_core::private_fs::private_dir(&home).unwrap();
        switchboard_core::private_fs::private_dir(&root.join(DIR)).unwrap();
        let workflow = "wf_0123456789abcdef";
        for n in 0..3_000 {
            let continues = if n % 10 == 0 {
                json!(workflow)
            } else {
                Value::Null
            };
            write(
                &root,
                &format!("old-{n:05}"),
                &json!({"launch_id": format!("old-{n:05}"), "launched_at": n, "continues": continues}),
            )
            .unwrap();
        }
        let launch = |n: i64| {
            let id = new_id();
            record(
                &root,
                &home,
                json!({"account_id": "a", "launched_at": 10_000 + n, "continues": workflow}),
                &id,
            )
            .unwrap();
            id
        };
        // The first launch of the workflow finds the records from before the index once.
        let first = launch(1);
        assert_eq!(
            read_back(&root, "old-02990")["continued_as"]["id"],
            first.as_str()
        );
        let scans = SCANS.with(|n| n.get());
        let started = std::time::Instant::now();
        let second = launch(2);
        link_handover(&root, &second, "ho-0123456789ab");
        not_started(&root, &second, 20_000);
        let third = launch(3);
        let took = started.elapsed();
        assert_eq!(
            SCANS.with(|n| n.get()),
            scans,
            "no full scan after the index exists"
        );
        assert!(took < std::time::Duration::from_secs(5), "took {took:?}");
        // The links are what the scans gave: the second took the first over, was handed over,
        // never started (so the first is free again), and the third took the first over.
        assert_eq!(
            read_back(&root, &first)["continued_as"]["id"],
            third.as_str()
        );
        assert_eq!(read_back(&root, &second)["ended"], NOT_STARTED);
        assert_eq!(read_back(&root, &third)["continued_from"], json!([first]));
    }
}
