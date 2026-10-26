//! Spending ceilings for paid fallback agents (SB-72, iteration 0.7.0 dossier C part C; operator
//! decision D-5 of 2026-10-25). The operator caps what paid agents may start spending: a daily
//! ceiling for this computer (every saved OpenRouter key, added up per UTC day), a daily ceiling
//! for a group on its own key, and a ceiling for one task counted on the key since its first paid
//! hand-over (C-Q2). A ceiling is a question, not a wall (fabric-workspace `knowledge/authority.md`):
//! reaching one opens «double it or keep it», answered *double* only in the window (C-Q1).
//!
//! Everything lives in `<data>/spend.json` (0600, carried in backups as a setting), never in
//! `Snapshot`, which refuses unknown fields: a 0.6.x build keeps reading `accounts.json` after a
//! downgrade. A file that cannot be read, or that was edited into something this build does not
//! accept, is an error, never "no ceilings": nothing paid is handed over until it reads again.
//!
//! This module stores, validates and decides; the hand-over gate that calls [`decide`] and writes
//! the baselines and the last hand-over is `fallback::spend_gate` (C-5).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;
use switchboard_core::{workflow_id_valid, ChainScope, Snapshot};

pub const FILE: &str = "spend.json";
/// The backup carries this file whole (`backup::setting_limit`). The largest file the count caps
/// and field rules allow (256 ceilings and 64 questions on 64-character group ids, 256 baselines,
/// extreme amounts and times) is about 104 KiB, so a write within the caps is never refused for
/// its size (`the_largest_file_the_caps_allow_fits_under_the_limit`).
pub const MAX_BYTES: u64 = 128 * 1024;
const VERSION: u32 = 1;
/// A ceiling is whole cents from $0.01 to $1,000, the estate's monthly total (authority L0).
pub const MIN_CENTS: u32 = 1;
pub const MAX_CENTS: u32 = 100_000;
/// A paid step is not handed work on a key with less than this left of its own limit: a session
/// started on an almost empty key dies at the provider after the offer, which stops the chain.
pub const MIN_REMAINING_USD: f64 = 0.50;
pub const MAX_CEILINGS: usize = 256;
/// Task ceilings are never pruned (a task's end is the engine's, not Switchboard's), so they take
/// at most this many slots: the machine's and every group's ceiling keep room of their own.
pub const MAX_TASK_CEILINGS: usize = 192;
/// Group ceilings: the slots left besides the tasks' and the machine's one.
pub const MAX_GROUP_CEILINGS: usize = MAX_CEILINGS - MAX_TASK_CEILINGS - 1;
pub const MAX_QUESTIONS: usize = 64;
pub const MAX_BASELINES: usize = 256;
const MAX_SKIPPED: usize = 8;
const DAY: i64 = 86_400;
/// Answered questions and task baselines are kept this long.
const KEEP: i64 = 30 * DAY;
/// An open question about a task is kept this long after it was asked: a task's end is not
/// something Switchboard sees, and the question must not hold a slot forever.
const TASK_QUESTION_AGE: i64 = 7 * DAY;

pub const UNREADABLE: &str = "Spending ceilings (spend.json) could not be read, so no paid agent is handed work until they are. Delete spend.json, then restore a backup to get the ceilings back, or leave it deleted to start over with no ceilings.";
/// A write that would pass `MAX_BYTES`. The count caps keep every file this build writes under
/// it, so this guards against a cap raised without its size.
pub const TOO_LARGE: &str = "The spending ceilings file is full. Clear a task ceiling in the Switchboard window, then try again.";
pub const TASK_CEILINGS_FULL: &str =
    "At most 192 task ceilings. Clear one in the Switchboard window first.";
pub const GROUP_CEILINGS_FULL: &str =
    "At most 63 group ceilings. Clear one in the Switchboard window first.";
/// Deleting a group drops its ceiling, which loosens it: only the window does that (C-Q7).
pub const GROUP_HOLDS_CEILING: &str = "This group has a spending ceiling. Delete the group in the Switchboard window, where its ceiling goes with it.";
pub const OUT_OF_RANGE: &str = "Enter an amount from $0.01 to $1,000.";
pub const NO_OWN_KEY: &str = "Give this group its own key under API keys first.";
pub const NO_GROUP: &str = "That group id names no group.";
pub const DOUBLE_IN_WINDOW: &str = "Doubling a ceiling is answered in the Switchboard window.";
pub const DOUBLE_TOO_HIGH: &str =
    "Doubling would pass $1,000, the most a ceiling may be. Set a ceiling of up to $1,000 instead.";
/// A request that names the window but did not come from the window's own process (C-Q1).
pub const WINDOW_ONLY: &str = "Only the Switchboard window answers as the window. Keep a ceiling from here, or double it in the window.";
/// C-Q9: removing or replacing a capped group's own key would move its paid steps off the key
/// its ceiling counts (or reset that count), a loosening only the window makes (C-Q1).
pub const KEY_HOLDS_CEILING: &str =
    "This group has a spending ceiling. Remove or replace its own key in the Switchboard window.";
/// C-Q9: removing or replacing a key whose spend another ceiling counts: any saved key while a
/// machine ceiling adds them up, or the key a task ceiling counts from (its baseline).
pub const KEY_COUNTED_BY_CEILING: &str =
    "A spending ceiling counts this key's spend. Remove or replace the key in the Switchboard window.";
/// C-Q8: a paid start by hand refused by the spend gate, by its reason code.
pub const STOP_KEY_EXHAUSTED: &str =
    "This OpenRouter key has less than $0.50 left of its own limit, so no paid agent starts on it.";
pub const STOP_CEILING_REACHED: &str = "A spending ceiling is reached, so no paid agent starts. Its question waits in the Switchboard window.";
pub const STOP_SPEND_UNKNOWN: &str = "A spending ceiling covers this start and its spend could not be read, so no paid agent starts.";
/// Raising or clearing a ceiling from the CLI or MCP (C-Q1): an agent may set a first ceiling or
/// lower one, never loosen one.
pub const RAISE_IN_WINDOW: &str = "Raising or clearing a ceiling is done in the Switchboard window. From here, set a first ceiling or a lower one.";
pub const NO_QUESTION: &str =
    "No open question with that id: it was answered, or its ceiling changed or was removed.";

/// One read-modify-write of the file at a time in this process (the owner, or an offline
/// command holding the store's lock).
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpendFile {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub ceilings: Vec<Ceiling>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub baselines: Vec<Baseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_handover: Option<LastHandover>,
}
fn version() -> u32 {
    VERSION
}
/// `usd` of `SetCeiling` must be present: `null` clears, a missing field is refused (serde would
/// otherwise read an absent `Option` as `None`, a clear nobody asked for).
pub(crate) fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Option::<f64>::deserialize(d)
}
/// The surface a request that names none is taken for: never the window (C-Q1).
pub(crate) fn agent_surface() -> Surface {
    Surface::Cli
}
impl Default for SpendFile {
    fn default() -> Self {
        SpendFile {
            version: VERSION,
            ceilings: Vec::new(),
            questions: Vec::new(),
            baselines: Vec::new(),
            last_handover: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ceiling {
    pub scope: ChainScope,
    pub cents: u32,
    pub set_at: i64,
    /// The value a *double* answer raised it from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doubled_from: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Keep,
    Double,
}

/// Which surface answered. Only the window is a person on this machine; the CLI and MCP are run
/// by agents too, so they may keep a ceiling but never raise one (C-Q1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Window,
    Cli,
    Mcp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub scope: ChainScope,
    /// The ceiling's value when it was reached: an answer is tied to it.
    pub cents: u32,
    /// What was counted against it, in dollars.
    pub spent: f64,
    pub reached_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<Surface>,
}

/// A task's key usage at its first paid hand-over: what its ceiling counts from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub workflow: String,
    /// `machine`, or the pool of the group whose own key the task ran on.
    pub key: String,
    /// The key's total usage then, in dollars; `null` when it could not be read at the task's
    /// first paid hand-over, so a task ceiling set later counts its spend as unknown, never as
    /// zero (C-5).
    pub usage: Option<f64>,
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub agent: String,
    pub reason: String,
}

/// What the last hand-over pass did (written by the gate, C-5; read by the chains view, C-2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LastHandover {
    pub at: i64,
    pub workflow: String,
    pub outcome: String,
    #[serde(default)]
    pub skipped: Vec<Skipped>,
}

/// Which saved OpenRouter key a paid step spends: the machine's, or a group's own (SB-144).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySlot {
    Machine,
    Group(String),
}
impl KeySlot {
    fn name(&self) -> &str {
        match self {
            KeySlot::Machine => "machine",
            KeySlot::Group(pool) => pool,
        }
    }
}

/// A paid step about to be handed work.
pub struct PaidStep<'a> {
    /// The key it spends (for an API-key Claude Code or Codex account: the key its folder's
    /// OpenRouter agents would use, which decides which group ceiling covers it).
    pub key: KeySlot,
    pub workflow: Option<&'a str>,
    /// Claude Code or Codex on an API-key account: its spend cannot be read (C-Q3).
    pub api_key_account: bool,
}

/// One key's spend as OpenRouter reports it (`agent_keys::credit`), in dollars.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyRead {
    pub limit_remaining: Option<f64>,
    pub usage: Option<f64>,
    pub usage_daily: Option<f64>,
}

/// The reads a decision needs: the step's own key, and every other saved OpenRouter key (the
/// machine ceiling adds them up). `None` is a read that failed.
pub struct Reads {
    pub key: Option<KeyRead>,
    pub others: Vec<Option<KeyRead>>,
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Run,
    /// No ceiling covers the step and its key's balance could not be read: it runs, the provider
    /// still enforces the key's own limit (log `spend_unread`).
    RunUnread,
    Skip {
        reason: &'static str,
        /// For `ceiling_reached`: the ceiling reached and what was counted against it.
        reached: Option<(ChainScope, u32, f64)>,
    },
}

/// Dollars to whole cents within the allowed range; anything else is refused.
pub fn cents(usd: f64) -> Result<u32, String> {
    if !usd.is_finite() || usd <= 0.0 {
        return Err(OUT_OF_RANGE.into());
    }
    let cents = (usd * 100.0).round();
    if cents < f64::from(MIN_CENTS) || cents > f64::from(MAX_CENTS) {
        return Err(OUT_OF_RANGE.into());
    }
    Ok(cents as u32)
}
fn usd(cents: u32) -> f64 {
    f64::from(cents) / 100.0
}

fn scope_valid(scope: &ChainScope) -> bool {
    match scope {
        ChainScope::Machine => true,
        ChainScope::Project { pool } => {
            !pool.is_empty()
                && pool.len() <= 64
                && pool
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        }
        ChainScope::Workflow { id } => workflow_id_valid(id),
    }
}
fn question_id_valid(id: &str) -> bool {
    id.len() == 18
        && id.starts_with("q_")
        && id[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn amount_valid(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
fn short_text_valid(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Every rule a file written by this build keeps; a hand edit that breaks one makes the whole
/// file unreadable, so such an edit is caught rather than read as fewer or higher ceilings. This
/// is no boundary against a process running as the operator: a hand edit that keeps every rule,
/// or deleting the file, loosens or removes ceilings, and only the next backup or the log shows
/// it (CONTRACTS, spending ceilings).
fn validate(file: &SpendFile) -> bool {
    let mut scopes = std::collections::HashSet::new();
    let ceilings_ok = file.ceilings.len() <= MAX_CEILINGS
        && file.ceilings.iter().all(|c| {
            scope_valid(&c.scope)
                && (MIN_CENTS..=MAX_CENTS).contains(&c.cents)
                && c.doubled_from
                    .is_none_or(|d| (MIN_CENTS..=MAX_CENTS).contains(&d))
                && scopes.insert(c.scope.clone())
        });
    let mut ids = std::collections::HashSet::new();
    let questions_ok = file.questions.len() <= MAX_QUESTIONS
        && file.questions.iter().all(|q| {
            question_id_valid(&q.id)
                && ids.insert(q.id.clone())
                && scope_valid(&q.scope)
                && (MIN_CENTS..=MAX_CENTS).contains(&q.cents)
                && amount_valid(q.spent)
                && (q.answer.is_some() == q.answered_at.is_some())
                && (q.answer.is_some() == q.answered_by.is_some())
        });
    let baselines_ok = file.baselines.len() <= MAX_BASELINES
        && file.baselines.iter().all(|b| {
            workflow_id_valid(&b.workflow)
                && short_text_valid(&b.key)
                && b.usage.is_none_or(amount_valid)
        });
    let last_ok = file.last_handover.as_ref().is_none_or(|h| {
        workflow_id_valid(&h.workflow)
            && short_text_valid(&h.outcome)
            && h.skipped.len() <= MAX_SKIPPED
            && h.skipped
                .iter()
                .all(|s| short_text_valid(&s.agent) && short_text_valid(&s.reason))
    });
    file.version == VERSION && ceilings_ok && questions_ok && baselines_ok && last_ok
}

pub fn read(root: &Path) -> Result<SpendFile, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SpendFile::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(m) if !m.is_file() => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    // Only an object: serde would also read a struct from a JSON array.
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| UNREADABLE)?;
    if !value.is_object() {
        return Err(UNREADABLE.into());
    }
    let file: SpendFile = serde_json::from_value(value).map_err(|_| UNREADABLE)?;
    if !validate(&file) {
        return Err(UNREADABLE.into());
    }
    Ok(file)
}

fn write(root: &Path, file: &SpendFile) -> Result<(), String> {
    if !validate(file) {
        return Err(UNREADABLE.into());
    }
    let bytes = serde_json::to_vec(file).map_err(|_| UNREADABLE)?;
    write_bytes(root, &bytes)
}
/// The last step of [`write`]: up to `MAX_BYTES` inclusive, the limit [`read`] takes, and nothing
/// written past it.
fn write_bytes(root: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), bytes)
}

/// Whether a group has its own OpenRouter key (SB-144): only such a group takes a daily ceiling.
pub fn group_has_own_key(snapshot: &Snapshot, pool: &str) -> bool {
    snapshot
        .agent_keys
        .iter()
        .any(|k| k.service == "openrouter" && k.pool.as_deref() == Some(pool))
}

/// The OpenRouter key a paid step started in `folder` spends: the choice `Store::agent_key_in`
/// makes (`agent_keys::key_in`, one function for both), for the gate's `PaidStep::key` (C-5) and
/// the chains view. `None` when no OpenRouter key is saved.
pub fn key_slot(snapshot: &Snapshot, folder: Option<&Path>) -> Option<KeySlot> {
    switchboard_core::agent_keys::key_in(snapshot, "openrouter", folder).map(|k| match &k.pool {
        Some(pool) => KeySlot::Group(pool.clone()),
        None => KeySlot::Machine,
    })
}

/// Deletes a group through `remove` under the write lock, then drops what spend.json holds for
/// it (its ceiling, its questions, the task baselines counted on its key), so a group saved again
/// under the same id inherits nothing. From the CLI or MCP a group that holds a ceiling is
/// refused (`GROUP_HOLDS_CEILING`, C-Q7): deleting it would drop the ceiling, a loosening only
/// the window makes (C-Q1). A file that cannot be read refuses every surface, since whether the
/// group holds a ceiling cannot be told. Answers the removal's answer and whether spend.json
/// changed. Should the write fail after the group is gone, the file keeps the ceiling until the
/// next write that prunes with a snapshot (`set_ceiling`, `answer`: `change` with `Some`; the
/// hand-over's writes prune with `None` and keep it), and `decide` and `view` never count it
/// meanwhile: a ceiling of a group the snapshot does not hold covers nothing.
pub fn remove_group<T>(
    root: &Path,
    pool: &str,
    by: Surface,
    remove: impl FnOnce() -> Result<T, String>,
) -> Result<(T, bool), String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let mut file = read(root)?;
    let scope = ChainScope::Project {
        pool: pool.to_owned(),
    };
    if by != Surface::Window && file.ceilings.iter().any(|c| c.scope == scope) {
        return Err(GROUP_HOLDS_CEILING.into());
    }
    let removed = remove()?;
    let before = file.clone();
    file.ceilings.retain(|c| c.scope != scope);
    file.questions.retain(|q| q.scope != scope);
    // `machine` names the machine key's baselines, never a group's.
    file.baselines
        .retain(|b| b.key != pool || pool == "machine");
    let changed = file != before;
    if changed {
        match write(root, &file) {
            Ok(()) => log("removed", &scope),
            Err(_) => log("remove_failed", &scope),
        }
    }
    Ok((removed, changed))
}

/// The refusal a paid start by hand gets for the gate's reason code (C-Q8).
pub fn refusal(reason: &str) -> &'static str {
    match reason {
        "key_exhausted" => STOP_KEY_EXHAUSTED,
        "ceiling_reached" => STOP_CEILING_REACHED,
        "ceilings_unreadable" => UNREADABLE,
        _ => STOP_SPEND_UNKNOWN,
    }
}

/// Removes or replaces a saved OpenRouter key through `change` under the write lock (C-Q9): the
/// machine's key with `pool` `None`, else that group's own. From the CLI or MCP a key whose spend a
/// ceiling counts is refused, since a removed key leaves the count (its paid steps move to another
/// key) and a new one starts it again: a group's own key while the group holds a ceiling
/// (`KEY_HOLDS_CEILING`); any key while a machine ceiling adds every saved key's daily spend up,
/// and the key a task ceiling counts from, its baseline (`KEY_COUNTED_BY_CEILING`). `replaces`
/// says whether the slot has a key now (saving a first key loosens nothing). An unreadable file
/// refuses every surface but the window, since whether a ceiling counts the key cannot be told.
pub fn guard_group_key<T>(
    root: &Path,
    pool: Option<&str>,
    replaces: bool,
    by: Surface,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    if replaces && by != Surface::Window {
        let file = read(root)?;
        if let Some(pool) = pool {
            let scope = ChainScope::Project {
                pool: pool.to_owned(),
            };
            if file.ceilings.iter().any(|c| c.scope == scope) {
                return Err(KEY_HOLDS_CEILING.into());
            }
        }
        let name = pool.unwrap_or("machine");
        let counted = file.ceilings.iter().any(|c| match &c.scope {
            ChainScope::Machine => true,
            ChainScope::Workflow { id } => file
                .baselines
                .iter()
                .any(|b| &b.workflow == id && b.key == name),
            ChainScope::Project { .. } => false,
        });
        if counted {
            return Err(KEY_COUNTED_BY_CEILING.into());
        }
    }
    change()
}

/// Drops what removed groups and time leave behind. A clock set back drops nothing: an age is
/// never negative.
///
/// With `snapshot` `None` nothing is dropped for a missing group, only for time: a hand-over's
/// writes carry the snapshot of the start of their pass, which may be older than a ceiling the
/// window set meanwhile for a new group.
pub(crate) fn prune(file: &mut SpendFile, snapshot: Option<&Snapshot>, now: i64) {
    let group_exists =
        |pool: &str| snapshot.is_none_or(|s| s.projects.iter().any(|p| p.pool == pool));
    let scope_exists = |scope: &ChainScope| match scope {
        ChainScope::Project { pool } => group_exists(pool),
        _ => true,
    };
    let older_than = |at: i64, age: i64| now > at && now - at > age;
    let day = |t: i64| t.div_euclid(DAY);
    file.ceilings.retain(|c| scope_exists(&c.scope));
    file.questions.retain(|q| {
        scope_exists(&q.scope)
            && match q.answered_at {
                Some(at) => !older_than(at, KEEP),
                // A daily ceiling's question no longer applies after the next UTC day; a task's
                // is kept 7 days.
                None => match q.scope {
                    ChainScope::Workflow { .. } => !older_than(q.reached_at, TASK_QUESTION_AGE),
                    _ => day(now) <= day(q.reached_at) + 1,
                },
            }
    });
    // A task that still has a ceiling keeps its baseline past 30 days: dropping it would count
    // the task from zero again at its next hand-over.
    let ceilings = &file.ceilings;
    let has_ceiling = |workflow: &str| {
        ceilings
            .iter()
            .any(|c| matches!(&c.scope, ChainScope::Workflow { id } if id == workflow))
    };
    file.baselines.retain(|b| {
        (b.key == "machine" || group_exists(&b.key))
            && (!older_than(b.at, KEEP) || has_ceiling(&b.workflow))
    });
}

fn log(action: &'static str, scope: &ChainScope) {
    crate::oplog::event(
        "ceiling",
        &[
            ("action", crate::oplog::Field::Code(action)),
            (
                "scope",
                crate::oplog::Field::Code(match scope {
                    ChainScope::Machine => "machine",
                    ChainScope::Project { .. } => "project",
                    ChainScope::Workflow { .. } => "workflow",
                }),
            ),
        ],
    );
}

/// Reads, prunes, changes and writes the file under the write lock; nothing is written when the
/// change refuses or the file cannot be read.
fn change<T>(
    root: &Path,
    snapshot: Option<&Snapshot>,
    now: i64,
    f: impl FnOnce(&mut SpendFile) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let mut file = read(root)?;
    let before = file.clone();
    prune(&mut file, snapshot, now);
    let answer = f(&mut file)?;
    if file != before {
        write(root, &file)?;
    }
    Ok(answer)
}

fn new_question_id() -> String {
    format!("q_{}", &uuid::Uuid::new_v4().simple().to_string()[..16])
}

/// Sets (or with `usd: None` clears) the ceiling of a scope. Open questions of that scope close:
/// an answer is tied to the value it was asked about. From the CLI or MCP (`by`, C-Q1) only a
/// first ceiling of a scope or a value no higher than its current one is accepted: raising or
/// clearing is the window's (`RAISE_IN_WINDOW`), decided against the file under the write lock.
pub fn set_ceiling(
    root: &Path,
    snapshot: &Snapshot,
    scope: ChainScope,
    usd: Option<f64>,
    by: Surface,
    now: i64,
) -> Result<Option<Ceiling>, String> {
    let value = usd.map(cents).transpose()?;
    if !scope_valid(&scope) {
        return Err(match scope {
            ChainScope::Workflow { .. } => {
                "Not a workflow id: it looks like wf_ and 16 hex digits.".to_string()
            }
            _ => NO_GROUP.to_string(),
        });
    }
    if let (ChainScope::Project { pool }, Some(_)) = (&scope, value) {
        if !snapshot.projects.iter().any(|p| &p.pool == pool) {
            return Err(NO_GROUP.into());
        }
        if !group_has_own_key(snapshot, pool) {
            return Err(NO_OWN_KEY.into());
        }
    }
    let set = change(root, Some(snapshot), now, |file| {
        if by != Surface::Window {
            let current = file.ceilings.iter().find(|c| c.scope == scope);
            let loosens = match (current, value) {
                (None, _) => false,
                (Some(_), None) => true,
                (Some(c), Some(cents)) => cents > c.cents,
            };
            if loosens {
                return Err(RAISE_IN_WINDOW.into());
            }
        }
        file.ceilings.retain(|c| c.scope != scope);
        // An answer is tied to the value it was asked about: a new value closes the old question.
        file.questions
            .retain(|q| q.scope != scope || q.answer.is_some());
        let Some(cents) = value else {
            return Ok(None);
        };
        // The machine's one ceiling always fits; tasks and groups have slots of their own.
        let same_kind =
            |c: &&Ceiling| std::mem::discriminant(&c.scope) == std::mem::discriminant(&scope);
        let held = file.ceilings.iter().filter(same_kind).count();
        match scope {
            ChainScope::Workflow { .. } if held >= MAX_TASK_CEILINGS => {
                return Err(TASK_CEILINGS_FULL.into())
            }
            ChainScope::Project { .. } if held >= MAX_GROUP_CEILINGS => {
                return Err(GROUP_CEILINGS_FULL.into())
            }
            _ => {}
        }
        let ceiling = Ceiling {
            scope: scope.clone(),
            cents,
            set_at: now,
            doubled_from: None,
        };
        file.ceilings.push(ceiling.clone());
        Ok(Some(ceiling))
    })?;
    log(if set.is_some() { "set" } else { "cleared" }, &scope);
    Ok(set)
}

/// Opens the question for a reached ceiling, once per scope and value (C-5 calls it). Answers its
/// id. When every slot is taken, the oldest answered question makes room, then the oldest open
/// task question; a machine or group question then takes the oldest open question's slot, so it
/// always fits. `None` only for a task question while every slot holds an open machine or group
/// question. `_snapshot` is the caller's, often from the start of a hand-over pass: it never
/// drops a record of a group it does not hold (`prune` with `None`).
pub fn open_question(
    root: &Path,
    _snapshot: &Snapshot,
    scope: &ChainScope,
    cents: u32,
    spent: f64,
    now: i64,
) -> Result<Option<String>, String> {
    let spent = if amount_valid(spent) { spent } else { 0.0 };
    let opened = change(root, None, now, |file| {
        if let Some(q) = file
            .questions
            .iter()
            .find(|q| q.answer.is_none() && &q.scope == scope && q.cents == cents)
        {
            return Ok((Some(q.id.clone()), false));
        }
        if file.questions.len() >= MAX_QUESTIONS {
            let answered = file
                .questions
                .iter()
                .enumerate()
                .filter(|(_, q)| q.answer.is_some())
                .min_by_key(|(_, q)| q.answered_at)
                .map(|(i, _)| i);
            let oldest_open = |task_only: bool| {
                file.questions
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| {
                        q.answer.is_none()
                            && (!task_only || matches!(q.scope, ChainScope::Workflow { .. }))
                    })
                    .min_by_key(|(_, q)| q.reached_at)
                    .map(|(i, _)| i)
            };
            let room = answered.or_else(|| oldest_open(true)).or_else(|| {
                (!matches!(scope, ChainScope::Workflow { .. }))
                    .then(|| oldest_open(false))
                    .flatten()
            });
            match room {
                Some(i) => {
                    file.questions.remove(i);
                }
                None => return Ok((None, false)),
            }
        }
        let id = new_question_id();
        file.questions.push(Question {
            id: id.clone(),
            scope: scope.clone(),
            cents,
            spent,
            reached_at: now,
            answer: None,
            answered_at: None,
            answered_by: None,
        });
        Ok((Some(id), true))
    })?;
    if opened.1 {
        log("reached", scope);
    }
    Ok(opened.0)
}

/// Answers an open question: *keep* from any surface, *double* only from the window.
pub fn answer(
    root: &Path,
    snapshot: &Snapshot,
    id: &str,
    answer: Answer,
    by: Surface,
    now: i64,
) -> Result<Value, String> {
    if answer == Answer::Double && by != Surface::Window {
        return Err(DOUBLE_IN_WINDOW.into());
    }
    let (scope, ceiling) = change(root, Some(snapshot), now, |file| {
        let index = file
            .questions
            .iter()
            .position(|q| q.id == id && q.answer.is_none())
            .ok_or(NO_QUESTION)?;
        let (scope, asked) = (
            file.questions[index].scope.clone(),
            file.questions[index].cents,
        );
        // The question is about the ceiling as it was; one set since then closed it, and a
        // removed one takes it along.
        let current = file
            .ceilings
            .iter()
            .position(|c| c.scope == scope && c.cents == asked)
            .ok_or(NO_QUESTION)?;
        if answer == Answer::Double {
            let doubled = asked.checked_mul(2).filter(|d| *d <= MAX_CENTS);
            let doubled = doubled.ok_or(DOUBLE_TOO_HIGH)?;
            let c = &mut file.ceilings[current];
            c.cents = doubled;
            c.doubled_from = Some(asked);
            c.set_at = now;
        }
        let q = &mut file.questions[index];
        q.answer = Some(answer);
        q.answered_at = Some(now);
        q.answered_by = Some(by);
        Ok((scope, file.ceilings[current].clone()))
    })?;
    log(
        match answer {
            Answer::Keep => "kept",
            Answer::Double => "doubled",
        },
        &scope,
    );
    Ok(json!({"answer": answer, "ceiling": ceiling_view(snapshot, &ceiling)}))
}

fn ceiling_view(snapshot: &Snapshot, c: &Ceiling) -> Value {
    let inactive = match &c.scope {
        ChainScope::Project { pool } if !group_has_own_key(snapshot, pool) => Some("no_own_key"),
        _ => None,
    };
    json!({
        "scope": c.scope,
        "cents": c.cents,
        "usd": usd(c.cents),
        "set_at": c.set_at,
        "doubled_from": c.doubled_from.map(usd),
        "active": inactive.is_none(),
        "inactive_reason": inactive,
    })
}

/// The ceilings, open questions and last hand-over, for the window, CLI and MCP.
pub fn view(root: &Path, snapshot: &Snapshot, now: i64) -> Result<Value, String> {
    let mut file = read(root)?;
    prune(&mut file, Some(snapshot), now);
    let mut open: Vec<&Question> = file
        .questions
        .iter()
        .filter(|q| q.answer.is_none())
        .collect();
    open.sort_by_key(|q| q.reached_at);
    Ok(json!({
        "ceilings": file.ceilings.iter().map(|c| ceiling_view(snapshot, c)).collect::<Vec<_>>(),
        "questions": open.iter().map(|q| json!({
            "id": q.id,
            "scope": q.scope,
            "cents": q.cents,
            "usd": usd(q.cents),
            "double_usd": (q.cents.saturating_mul(2) <= MAX_CENTS).then(|| usd(q.cents * 2)),
            "spent": q.spent,
            "reached_at": q.reached_at,
        })).collect::<Vec<_>>(),
        "last_handover": file.last_handover,
        "limits": {
            "min_usd": usd(MIN_CENTS),
            "max_usd": usd(MAX_CENTS),
            "min_remaining_usd": MIN_REMAINING_USD,
        },
    }))
}

/// Records a task's baseline, the key's total usage at its first paid hand-over (C-5, C-Q2):
/// what its task ceiling counts from. A task that has one keeps it, so a later hand-over never
/// moves it forward. The gate writes it before the launch, so a restart between the decision and
/// the offer never loses it; an offer refused afterwards leaves a baseline that counts from a
/// little earlier, which only counts more. When every slot is taken, the oldest baseline of a task
/// with no ceiling makes room (at most 192 tasks hold a ceiling, so one always does). Answers
/// whether it was written. `usage` `None` (the total could not be read) records a marker: a task
/// ceiling set later counts the task's spend as unknown, never from zero. `_snapshot` drops
/// nothing, as for [`open_question`].
pub fn record_baseline(
    root: &Path,
    _snapshot: &Snapshot,
    workflow: &str,
    key: &KeySlot,
    usage: Option<f64>,
    now: i64,
) -> Result<bool, String> {
    if !workflow_id_valid(workflow)
        || !usage.is_none_or(amount_valid)
        || !short_text_valid(key.name())
    {
        return Ok(false);
    }
    change(root, None, now, |file| {
        if file.baselines.iter().any(|b| b.workflow == workflow) {
            return Ok(false);
        }
        if file.baselines.len() >= MAX_BASELINES {
            let ceilings = &file.ceilings;
            let oldest = file
                .baselines
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    !ceilings.iter().any(
                        |c| matches!(&c.scope, ChainScope::Workflow { id } if *id == b.workflow),
                    )
                })
                .min_by_key(|(_, b)| b.at)
                .map(|(i, _)| i);
            match oldest {
                Some(i) => {
                    file.baselines.remove(i);
                }
                None => return Ok(false),
            }
        }
        file.baselines.push(Baseline {
            workflow: workflow.to_owned(),
            key: key.name().to_owned(),
            usage,
            at: now,
        });
        Ok(true)
    })
}

/// Records what the last hand-over pass did (C-5), for the chains view and the window: the
/// outcome code and the steps passed over with their reasons, at most 8. A workflow id the file
/// does not accept is not recorded.
pub fn record_handover(
    root: &Path,
    _snapshot: &Snapshot,
    last: LastHandover,
    now: i64,
) -> Result<(), String> {
    let mut last = last;
    last.skipped.truncate(MAX_SKIPPED);
    let fits = workflow_id_valid(&last.workflow)
        && short_text_valid(&last.outcome)
        && last
            .skipped
            .iter()
            .all(|s| short_text_valid(&s.agent) && short_text_valid(&s.reason));
    if !fits {
        return Ok(());
    }
    change(root, None, now, |file| {
        file.last_handover = Some(last);
        Ok(())
    })
}

/// The decision for one paid step, in order: the key's own limit nearly spent, a ceiling reached,
/// a covering ceiling whose spend could not be read. Pure: the caller reads. A ceiling of a group
/// `snapshot` no longer holds covers nothing, as [`prune`] would drop it.
pub fn decide(file: &SpendFile, snapshot: &Snapshot, step: &PaidStep, reads: &Reads) -> Verdict {
    let key = reads.key.as_ref();
    if key
        .and_then(|k| k.limit_remaining)
        .is_some_and(|left| left < MIN_REMAINING_USD)
    {
        return Verdict::Skip {
            reason: "key_exhausted",
            reached: None,
        };
    }
    let covering: Vec<&Ceiling> = file
        .ceilings
        .iter()
        .filter(|c| match &c.scope {
            ChainScope::Machine => true,
            ChainScope::Project { pool } => {
                step.key == KeySlot::Group(pool.clone())
                    && snapshot.projects.iter().any(|p| &p.pool == pool)
            }
            ChainScope::Workflow { id } => step.workflow == Some(id.as_str()),
        })
        .collect();
    if covering.is_empty() {
        return if key.is_none() && !step.api_key_account {
            Verdict::RunUnread
        } else {
            Verdict::Run
        };
    }
    let unknown = Verdict::Skip {
        reason: "spend_unknown",
        reached: None,
    };
    // An API-key account's spend is not one Switchboard can read (C-Q3).
    if step.api_key_account {
        return unknown;
    }
    let spent = |c: &Ceiling| -> Option<f64> {
        match &c.scope {
            ChainScope::Machine => std::iter::once(key)
                .chain(reads.others.iter().map(Option::as_ref))
                .map(|k| k.and_then(|k| k.usage_daily).filter(|v| amount_valid(*v)))
                .sum(),
            ChainScope::Project { .. } => {
                key.and_then(|k| k.usage_daily).filter(|v| amount_valid(*v))
            }
            ChainScope::Workflow { id } => {
                let usage = key.and_then(|k| k.usage).filter(|v| amount_valid(*v))?;
                match file.baselines.iter().find(|b| &b.workflow == id) {
                    // No paid hand-over yet: nothing counted.
                    None => Some(0.0),
                    Some(b) if b.key != step.key.name() => None,
                    // Its first paid hand-over ran with the total unread: unknown, never zero.
                    Some(Baseline { usage: None, .. }) => None,
                    // A key whose total went down is another key: unknown, never zero.
                    Some(Baseline {
                        usage: Some(from), ..
                    }) => Some(usage - from).filter(|d| *d >= 0.0),
                }
            }
        }
    };
    let mut any_unknown = false;
    let mut reached: Option<(&Ceiling, f64)> = None;
    for c in covering {
        match spent(c) {
            None => any_unknown = true,
            Some(s) if s * 100.0 >= f64::from(c.cents) - 1e-9 => {
                if reached.is_none_or(|(r, _)| c.cents < r.cents) {
                    reached = Some((c, s));
                }
            }
            Some(_) => {}
        }
    }
    match reached {
        Some((c, s)) => Verdict::Skip {
            reason: "ceiling_reached",
            reached: Some((c.scope.clone(), c.cents, s)),
        },
        None if any_unknown => unknown,
        None => Verdict::Run,
    }
}

/// [`decide`] over the file on disk: an unreadable file hands nothing paid over, never "no
/// ceilings".
pub fn check(root: &Path, snapshot: &Snapshot, step: &PaidStep, reads: &Reads) -> Verdict {
    match read(root) {
        Ok(file) => decide(&file, snapshot, step, reads),
        Err(_) => Verdict::Skip {
            reason: "ceilings_unreadable",
            reached: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::agent_keys::AgentKey;
    use switchboard_core::Project;

    const T: i64 = 1_800_000_000;
    const WF: &str = "wf_0123456789abcdef";

    fn group(pool: &str) -> Project {
        Project {
            pool: pool.into(),
            name: pool.into(),
            folders: vec![format!("/work/{pool}")],
            created_at: 1,
            sources: vec![],
            organization: None,
        }
    }
    fn key(pool: Option<&str>) -> AgentKey {
        AgentKey {
            service: "openrouter".into(),
            id: format!("key-{}", pool.unwrap_or("machine")),
            saved_at: 1,
            pool: pool.map(str::to_owned),
            model: None,
        }
    }
    /// Groups alpha (its own key) and beta (on the machine key), and the machine key.
    fn snapshot() -> Snapshot {
        Snapshot {
            projects: vec![group("alpha"), group("beta")],
            agent_keys: vec![key(None), key(Some("alpha"))],
            ..Default::default()
        }
    }
    fn alpha() -> ChainScope {
        ChainScope::Project {
            pool: "alpha".into(),
        }
    }
    fn full(daily: f64, usage: f64) -> Option<KeyRead> {
        Some(KeyRead {
            limit_remaining: Some(9.0),
            usage: Some(usage),
            usage_daily: Some(daily),
        })
    }
    fn step(key: KeySlot) -> PaidStep<'static> {
        PaidStep {
            key,
            workflow: Some(WF),
            api_key_account: false,
        }
    }
    fn open(root: &Path) -> Vec<Question> {
        read(root)
            .unwrap()
            .questions
            .into_iter()
            .filter(|q| q.answer.is_none())
            .collect()
    }

    #[test]
    fn a_doubled_ceiling_is_tied_to_its_value_and_the_next_hit_asks_again() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Window, T).unwrap();
        let first = open_question(root.path(), &s, &alpha(), 500, 5.2, T + 10)
            .unwrap()
            .unwrap();
        // The same value reached again asks nothing new.
        assert_eq!(
            open_question(root.path(), &s, &alpha(), 500, 5.4, T + 20).unwrap(),
            Some(first.clone())
        );
        assert_eq!(open(root.path()).len(), 1);
        let answered = answer(
            root.path(),
            &s,
            &first,
            Answer::Double,
            Surface::Window,
            T + 30,
        )
        .unwrap();
        assert_eq!(answered["ceiling"]["cents"], 1000);
        let ceiling = read(root.path()).unwrap().ceilings;
        assert_eq!(ceiling.len(), 1);
        assert_eq!(
            (ceiling[0].cents, ceiling[0].doubled_from),
            (1000, Some(500))
        );
        assert!(open(root.path()).is_empty(), "the question closed");
        // The same question cannot double twice.
        assert_eq!(
            answer(
                root.path(),
                &s,
                &first,
                Answer::Double,
                Surface::Window,
                T + 31
            )
            .unwrap_err(),
            NO_QUESTION
        );
        // Reaching the new value asks again, a new question tied to $10.
        let second = open_question(root.path(), &s, &alpha(), 1000, 10.1, T + 40)
            .unwrap()
            .unwrap();
        assert_ne!(second, first);
        assert_eq!(open(root.path())[0].cents, 1000);
        // Keep closes it and leaves the value.
        answer(root.path(), &s, &second, Answer::Keep, Surface::Cli, T + 50).unwrap();
        assert_eq!(read(root.path()).unwrap().ceilings[0].cents, 1000);
        assert!(open(root.path()).is_empty());
        // Setting the ceiling by hand closes a question asked about the old value.
        let third = open_question(root.path(), &s, &alpha(), 1000, 10.5, T + 60)
            .unwrap()
            .unwrap();
        set_ceiling(root.path(), &s, alpha(), Some(7.0), Surface::Window, T + 70).unwrap();
        assert!(open(root.path()).is_empty());
        assert_eq!(
            answer(
                root.path(),
                &s,
                &third,
                Answer::Double,
                Surface::Window,
                T + 80
            )
            .unwrap_err(),
            NO_QUESTION,
            "a yes about $10 never doubles $7"
        );
    }

    #[test]
    fn a_group_without_its_own_key_cannot_take_a_daily_ceiling() {
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        let beta = ChainScope::Project {
            pool: "beta".into(),
        };
        assert_eq!(
            set_ceiling(root.path(), &s, beta, Some(5.0), Surface::Window, T).unwrap_err(),
            NO_OWN_KEY
        );
        assert_eq!(
            set_ceiling(
                root.path(),
                &s,
                ChainScope::Project {
                    pool: "gone".into()
                },
                Some(5.0),
                Surface::Window,
                T
            )
            .unwrap_err(),
            NO_GROUP
        );
        assert!(read(root.path()).unwrap().ceilings.is_empty());
        // With its own key it takes one; when the key goes, the ceiling stays, shown inactive.
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Window, T).unwrap();
        s.agent_keys.retain(|k| k.pool.is_none());
        let shown = view(root.path(), &s, T).unwrap();
        assert_eq!(shown["ceilings"][0]["active"], false);
        assert_eq!(shown["ceilings"][0]["inactive_reason"], "no_own_key");
        assert_eq!(
            read(root.path()).unwrap().ceilings.len(),
            1,
            "never deleted"
        );
        // An inactive ceiling covers nothing: the step now spends the machine key.
        let file = read(root.path()).unwrap();
        assert_eq!(
            decide(
                &file,
                &snapshot(),
                &step(KeySlot::Machine),
                &Reads {
                    key: full(50.0, 50.0),
                    others: vec![]
                }
            ),
            Verdict::Run
        );
        // It can still be cleared.
        assert_eq!(
            set_ceiling(root.path(), &s, alpha(), None, Surface::Window, T).unwrap(),
            None
        );
        assert!(read(root.path()).unwrap().ceilings.is_empty());
    }

    #[test]
    fn double_is_refused_from_mcp_and_cli() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(5.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let id = open_question(root.path(), &s, &ChainScope::Machine, 500, 6.0, T)
            .unwrap()
            .unwrap();
        for by in [Surface::Cli, Surface::Mcp] {
            assert_eq!(
                answer(root.path(), &s, &id, Answer::Double, by, T + 1).unwrap_err(),
                DOUBLE_IN_WINDOW
            );
        }
        assert_eq!(read(root.path()).unwrap().ceilings[0].cents, 500);
        assert_eq!(open(root.path()).len(), 1, "still open for the window");
        // Doubling past the estate's total is refused and leaves the question open.
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(600.0),
            Surface::Window,
            T + 2,
        )
        .unwrap();
        let high = open_question(root.path(), &s, &ChainScope::Machine, 60_000, 600.0, T + 3)
            .unwrap()
            .unwrap();
        assert_eq!(
            answer(
                root.path(),
                &s,
                &high,
                Answer::Double,
                Surface::Window,
                T + 4
            )
            .unwrap_err(),
            DOUBLE_TOO_HIGH
        );
        assert_eq!(open(root.path()).len(), 1);
        answer(root.path(), &s, &high, Answer::Keep, Surface::Mcp, T + 5).unwrap();
        assert!(open(root.path()).is_empty());
    }

    #[test]
    fn an_unreadable_spend_file_hands_nothing_over_never_no_ceiling() {
        let root = tempfile::tempdir().unwrap();
        let reads = || Reads {
            key: full(0.0, 0.0),
            others: vec![],
        };
        // No file: no ceilings, and a step on a full key runs.
        assert_eq!(
            check(root.path(), &snapshot(), &step(KeySlot::Machine), &reads()),
            Verdict::Run
        );
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert_eq!(read(root.path()).unwrap_err(), UNREADABLE);
        assert_eq!(view(root.path(), &snapshot(), T).unwrap_err(), UNREADABLE);
        assert_eq!(
            check(root.path(), &snapshot(), &step(KeySlot::Machine), &reads()),
            Verdict::Skip {
                reason: "ceilings_unreadable",
                reached: None
            }
        );
        // A write over it is refused too: the operator's values are not overwritten blind.
        assert_eq!(
            set_ceiling(
                root.path(),
                &snapshot(),
                ChainScope::Machine,
                Some(1.0),
                Surface::Window,
                T
            )
            .unwrap_err(),
            UNREADABLE
        );
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), b"{not json");
        // Not a plain file, or too large: unreadable as well.
        std::fs::remove_file(root.path().join(FILE)).unwrap();
        std::fs::create_dir(root.path().join(FILE)).unwrap();
        assert_eq!(read(root.path()).unwrap_err(), UNREADABLE);
        std::fs::remove_dir(root.path().join(FILE)).unwrap();
        std::fs::write(root.path().join(FILE), vec![b' '; (MAX_BYTES + 1) as usize]).unwrap();
        assert_eq!(read(root.path()).unwrap_err(), UNREADABLE);
    }

    #[test]
    fn a_hand_edited_spend_file_that_breaks_a_rule_is_unreadable_never_empty() {
        let root = tempfile::tempdir().unwrap();
        let ceiling =
            |scope: &str, cents: &str| format!(r#"{{"scope":{scope},"cents":{cents},"set_at":1}}"#);
        let machine = r#"{"kind":"machine"}"#;
        let bad = [
            // Out of range or not a whole number of cents.
            format!(r#"{{"ceilings":[{}]}}"#, ceiling(machine, "0")),
            format!(r#"{{"ceilings":[{}]}}"#, ceiling(machine, "100001")),
            format!(r#"{{"ceilings":[{}]}}"#, ceiling(machine, "-5")),
            format!(r#"{{"ceilings":[{}]}}"#, ceiling(machine, "5.5")),
            format!(r#"{{"ceilings":[{}]}}"#, ceiling(machine, "1e400")),
            // Two ceilings for one scope.
            format!(
                r#"{{"ceilings":[{},{}]}}"#,
                ceiling(machine, "100"),
                ceiling(machine, "200")
            ),
            // A task that is not a workflow id; a group with no name.
            format!(
                r#"{{"ceilings":[{}]}}"#,
                ceiling(r#"{"kind":"workflow","id":"wf_x"}"#, "100")
            ),
            format!(
                r#"{{"ceilings":[{}]}}"#,
                ceiling(r#"{"kind":"project","pool":""}"#, "100")
            ),
            // A scope of an unknown kind.
            format!(
                r#"{{"ceilings":[{}]}}"#,
                ceiling(r#"{"kind":"galaxy"}"#, "100")
            ),
            // Negative spend, an answer with no time, an id of the wrong shape.
            r#"{"questions":[{"id":"q_0123456789abcdef","scope":{"kind":"machine"},"cents":100,"spent":-1,"reached_at":1}]}"#.into(),
            r#"{"questions":[{"id":"q_0123456789abcdef","scope":{"kind":"machine"},"cents":100,"spent":1,"reached_at":1,"answer":"keep"}]}"#.into(),
            r#"{"questions":[{"id":"../../x","scope":{"kind":"machine"},"cents":100,"spent":1,"reached_at":1}]}"#.into(),
            r#"{"questions":[{"id":"q_0123456789abcdef","scope":{"kind":"machine"},"cents":100,"spent":1,"reached_at":1,"answer":"triple","answered_at":2}]}"#.into(),
            // A baseline with negative usage or an unknown task.
            r#"{"baselines":[{"workflow":"wf_0123456789abcdef","key":"machine","usage":-3,"at":1}]}"#.into(),
            r#"{"baselines":[{"workflow":"nope","key":"machine","usage":3,"at":1}]}"#.into(),
            // A newer format this build does not know.
            r#"{"version":2}"#.into(),
            // Not an object at all.
            "[]".into(),
            "null".into(),
        ];
        for text in bad {
            std::fs::write(root.path().join(FILE), &text).unwrap();
            assert_eq!(read(root.path()).unwrap_err(), UNREADABLE, "{text}");
        }
        // Too many ceilings.
        let many: Vec<String> = (0..=MAX_CEILINGS)
            .map(|i| {
                ceiling(
                    &format!(r#"{{"kind":"workflow","id":"wf_{i:016x}"}}"#),
                    "100",
                )
            })
            .collect();
        std::fs::write(
            root.path().join(FILE),
            format!(r#"{{"ceilings":[{}]}}"#, many.join(",")),
        )
        .unwrap();
        assert_eq!(read(root.path()).unwrap_err(), UNREADABLE);
        // A field a newer build added is ignored; the rest reads.
        std::fs::write(
            root.path().join(FILE),
            format!(
                r#"{{"version":1,"later":true,"ceilings":[{}]}}"#,
                ceiling(machine, "250")
            ),
        )
        .unwrap();
        assert_eq!(read(root.path()).unwrap().ceilings[0].cents, 250);
    }

    #[test]
    fn a_ceiling_of_zero_negative_nan_or_out_of_range_is_refused_and_nothing_is_saved() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        for usd in [
            0.0,
            -5.0,
            -0.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.004,
            1000.01,
            1e12,
        ] {
            assert_eq!(cents(usd).unwrap_err(), OUT_OF_RANGE, "{usd}");
            assert_eq!(
                set_ceiling(
                    root.path(),
                    &s,
                    ChainScope::Machine,
                    Some(usd),
                    Surface::Window,
                    T
                )
                .unwrap_err(),
                OUT_OF_RANGE,
                "{usd}"
            );
        }
        assert!(!root.path().join(FILE).exists(), "nothing was written");
        assert_eq!(cents(0.01), Ok(1));
        assert_eq!(cents(1000.0), Ok(100_000));
        assert_eq!(cents(2.5), Ok(250));
        assert_eq!(cents(0.1 + 0.2), Ok(30), "float noise rounds to the cent");
        // A second ceiling for the same scope replaces the first.
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(3.0),
            Surface::Window,
            T,
        )
        .unwrap();
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(4.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let c = read(root.path()).unwrap().ceilings;
        assert_eq!((c.len(), c[0].cents), (1, 400));
        // A task ceiling needs a workflow id.
        assert!(set_ceiling(
            root.path(),
            &s,
            ChainScope::Workflow { id: "wf_x".into() },
            Some(1.0),
            Surface::Window,
            T
        )
        .is_err());
    }

    #[test]
    fn a_clock_set_back_drops_nothing_and_answers_still_work() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(5.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let kept = open_question(root.path(), &s, &ChainScope::Machine, 500, 5.0, T)
            .unwrap()
            .unwrap();
        answer(root.path(), &s, &kept, Answer::Keep, Surface::Window, T).unwrap();
        let task = ChainScope::Workflow { id: WF.into() };
        set_ceiling(root.path(), &s, task.clone(), Some(2.0), Surface::Window, T).unwrap();
        let waiting = open_question(root.path(), &s, &task, 200, 2.5, T)
            .unwrap()
            .unwrap();
        let mut file = read(root.path()).unwrap();
        file.baselines.push(Baseline {
            workflow: WF.into(),
            key: "machine".into(),
            usage: Some(1.0),
            at: T,
        });
        write(root.path(), &file).unwrap();
        // The clock goes back a year: nothing is older than it was, so nothing goes.
        let earlier = T - 365 * DAY;
        let mut pruned = read(root.path()).unwrap();
        prune(&mut pruned, Some(&s), earlier);
        assert_eq!(pruned, read(root.path()).unwrap());
        // A question asked "in the future" is still answered, and the ceiling it doubles is the
        // one it was asked about.
        answer(
            root.path(),
            &s,
            &waiting,
            Answer::Double,
            Surface::Window,
            earlier,
        )
        .unwrap();
        let file = read(root.path()).unwrap();
        let doubled = file.ceilings.iter().find(|c| c.scope == task).unwrap();
        assert_eq!(doubled.cents, 400);
        // Time going forward again past 30 days drops the answered questions; the baseline of a
        // task that still has a ceiling stays (C-5), so the task is never counted from zero again.
        let mut later = file.clone();
        prune(&mut later, Some(&s), T + 31 * DAY);
        assert!(later.questions.is_empty());
        assert_eq!(later.baselines.len(), 1);
        assert_eq!(later.ceilings.len(), 2, "ceilings never age out");
        let mut cleared = later.clone();
        cleared.ceilings.retain(|c| c.scope != task);
        prune(&mut cleared, Some(&s), T + 31 * DAY);
        assert!(
            cleared.baselines.is_empty(),
            "without its ceiling it ages out"
        );
        // An open daily question stays answerable through the next UTC day, then goes.
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(6.0),
            Surface::Window,
            T,
        )
        .unwrap();
        let daily = open_question(root.path(), &s, &ChainScope::Machine, 600, 6.0, T)
            .unwrap()
            .unwrap();
        let mut f = read(root.path()).unwrap();
        prune(&mut f, Some(&s), T + DAY);
        assert!(f.questions.iter().any(|q| q.id == daily));
        prune(&mut f, Some(&s), T + 3 * DAY);
        assert!(!f.questions.iter().any(|q| q.id == daily));
    }

    #[test]
    fn a_removed_group_takes_its_ceiling_questions_and_baselines_with_it() {
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Window, T).unwrap();
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(9.0),
            Surface::Window,
            T,
        )
        .unwrap();
        open_question(root.path(), &s, &alpha(), 500, 5.0, T).unwrap();
        let mut file = read(root.path()).unwrap();
        file.baselines.push(Baseline {
            workflow: WF.into(),
            key: "alpha".into(),
            usage: Some(1.0),
            at: T,
        });
        write(root.path(), &file).unwrap();
        s.projects.retain(|p| p.pool != "alpha");
        s.agent_keys.retain(|k| k.pool.is_none());
        let mut f = read(root.path()).unwrap();
        prune(&mut f, Some(&s), T);
        assert_eq!(f.ceilings.len(), 1);
        assert_eq!(f.ceilings[0].scope, ChainScope::Machine);
        assert!(f.questions.is_empty());
        assert!(f.baselines.is_empty());
        // The view prunes in memory only; the next write prunes on disk.
        let shown = view(root.path(), &s, T).unwrap();
        assert_eq!(shown["ceilings"].as_array().unwrap().len(), 1);
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(8.0),
            Surface::Window,
            T,
        )
        .unwrap();
        assert_eq!(read(root.path()).unwrap().ceilings.len(), 1);
    }

    #[test]
    fn every_spend_file_this_build_writes_fits_a_backup() {
        // The backup carries spend.json only up to its own limit (`backup::setting_limit`): a
        // file this build may write and a backup would drop is a ceiling lost at a reinstall.
        assert_eq!(switchboard_core::backup::MAX_SPEND_SETTING, MAX_BYTES);
    }

    #[test]
    fn a_ceiling_of_a_removed_group_covers_no_step() {
        // Prune drops it at the next write; until then the decision never counts it either.
        let mut file = SpendFile::default();
        file.ceilings.push(Ceiling {
            scope: alpha(),
            cents: 100,
            set_at: T,
            doubled_from: None,
        });
        let reads = Reads {
            key: full(5.0, 5.0),
            others: vec![],
        };
        let on_alpha = step(KeySlot::Group("alpha".into()));
        assert!(matches!(
            decide(&file, &snapshot(), &on_alpha, &reads),
            Verdict::Skip {
                reason: "ceiling_reached",
                ..
            }
        ));
        let mut removed = snapshot();
        removed.projects.retain(|p| p.pool != "alpha");
        assert_eq!(decide(&file, &removed, &on_alpha, &reads), Verdict::Run);
    }

    #[test]
    fn an_unreadable_spend_read_while_a_ceiling_covers_a_step_is_a_denial() {
        let mut file = SpendFile::default();
        let unknown = Verdict::Skip {
            reason: "spend_unknown",
            reached: None,
        };
        // No ceiling: a read that failed lets the step run (the provider enforces the key's own
        // limit), and says so.
        let failed = Reads {
            key: None,
            others: vec![],
        };
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &failed),
            Verdict::RunUnread
        );
        // A machine ceiling covers every paid step: the same failed read is a denial.
        file.ceilings.push(Ceiling {
            scope: ChainScope::Machine,
            cents: 500,
            set_at: T,
            doubled_from: None,
        });
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &failed),
            unknown
        );
        // The step's key reads, but another saved key the machine ceiling adds up does not.
        let other_failed = Reads {
            key: full(1.0, 1.0),
            others: vec![None],
        };
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &other_failed),
            unknown
        );
        // OpenRouter answered without the day's usage.
        let no_daily = Reads {
            key: Some(KeyRead {
                limit_remaining: Some(9.0),
                usage: Some(1.0),
                usage_daily: None,
            }),
            others: vec![],
        };
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &no_daily),
            unknown
        );
        // An API-key Claude Code or Codex account: its spend is never readable (C-Q3).
        let api = PaidStep {
            key: KeySlot::Machine,
            workflow: None,
            api_key_account: true,
        };
        let readable = Reads {
            key: full(1.0, 1.0),
            others: vec![],
        };
        assert_eq!(decide(&file, &snapshot(), &api, &readable), unknown);
        assert_eq!(
            decide(&SpendFile::default(), &snapshot(), &api, &readable),
            Verdict::Run,
            "no ceiling, no gate"
        );
        // A task ceiling whose key's usage went down (another key now): unknown, not zero.
        let mut task = SpendFile::default();
        task.ceilings.push(Ceiling {
            scope: ChainScope::Workflow { id: WF.into() },
            cents: 200,
            set_at: T,
            doubled_from: None,
        });
        task.baselines.push(Baseline {
            workflow: WF.into(),
            key: "machine".into(),
            usage: Some(50.0),
            at: T,
        });
        assert_eq!(
            decide(
                &task,
                &snapshot(),
                &step(KeySlot::Machine),
                &Reads {
                    key: full(0.0, 10.0),
                    others: vec![]
                }
            ),
            unknown
        );
        // The task's baseline was taken on another key: unknown.
        assert_eq!(
            decide(
                &task,
                &snapshot(),
                &step(KeySlot::Group("alpha".into())),
                &Reads {
                    key: full(0.0, 60.0),
                    others: vec![]
                }
            ),
            unknown
        );
        // Counted from the baseline: $1 of $2 runs, $2.50 is reached.
        assert_eq!(
            decide(
                &task,
                &snapshot(),
                &step(KeySlot::Machine),
                &Reads {
                    key: full(0.0, 51.0),
                    others: vec![]
                }
            ),
            Verdict::Run
        );
        assert_eq!(
            decide(
                &task,
                &snapshot(),
                &step(KeySlot::Machine),
                &Reads {
                    key: full(0.0, 52.5),
                    others: vec![]
                }
            ),
            Verdict::Skip {
                reason: "ceiling_reached",
                reached: Some((ChainScope::Workflow { id: WF.into() }, 200, 2.5))
            }
        );
    }

    #[test]
    fn the_key_nearly_spent_comes_first_and_the_strictest_reached_ceiling_is_named() {
        let mut file = SpendFile::default();
        let exhausted = Reads {
            key: Some(KeyRead {
                limit_remaining: Some(0.49),
                usage: Some(1.0),
                usage_daily: Some(1.0),
            }),
            others: vec![],
        };
        // With or without a ceiling.
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &exhausted),
            Verdict::Skip {
                reason: "key_exhausted",
                reached: None
            }
        );
        let at_floor = Reads {
            key: Some(KeyRead {
                limit_remaining: Some(MIN_REMAINING_USD),
                usage: Some(1.0),
                usage_daily: Some(1.0),
            }),
            others: vec![],
        };
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &at_floor),
            Verdict::Run
        );
        // A key without a limit of its own has nothing to exhaust.
        let unlimited = Reads {
            key: Some(KeyRead {
                limit_remaining: None,
                usage: Some(1.0),
                usage_daily: Some(1.0),
            }),
            others: vec![],
        };
        assert_eq!(
            decide(&file, &snapshot(), &step(KeySlot::Machine), &unlimited),
            Verdict::Run
        );
        // Machine $8 over every key; group alpha $3 on its own key.
        file.ceilings.push(Ceiling {
            scope: ChainScope::Machine,
            cents: 800,
            set_at: T,
            doubled_from: None,
        });
        file.ceilings.push(Ceiling {
            scope: alpha(),
            cents: 300,
            set_at: T,
            doubled_from: None,
        });
        let on_alpha = step(KeySlot::Group("alpha".into()));
        // alpha spent $3.10 today, the machine key $2: alpha's ceiling is reached.
        let reads = Reads {
            key: full(3.10, 40.0),
            others: vec![full(2.0, 2.0)],
        };
        assert_eq!(
            decide(&file, &snapshot(), &on_alpha, &reads),
            Verdict::Skip {
                reason: "ceiling_reached",
                reached: Some((alpha(), 300, 3.10))
            }
        );
        // The machine key's own step is not covered by alpha's ceiling: $2 + $3.10 of $8 runs.
        let on_machine = step(KeySlot::Machine);
        let machine_reads = Reads {
            key: full(2.0, 2.0),
            others: vec![full(3.10, 40.0)],
        };
        assert_eq!(
            decide(&file, &snapshot(), &on_machine, &machine_reads),
            Verdict::Run
        );
        // Both reached: the stricter ceiling ($3) is named.
        let both = Reads {
            key: full(6.0, 40.0),
            others: vec![full(2.5, 2.5)],
        };
        assert_eq!(
            decide(&file, &snapshot(), &on_alpha, &both),
            Verdict::Skip {
                reason: "ceiling_reached",
                reached: Some((alpha(), 300, 6.0))
            }
        );
        // A reached ceiling is named before another covering ceiling whose read failed.
        let partly = Reads {
            key: full(3.5, 40.0),
            others: vec![None],
        };
        assert_eq!(
            decide(&file, &snapshot(), &on_alpha, &partly),
            Verdict::Skip {
                reason: "ceiling_reached",
                reached: Some((alpha(), 300, 3.5))
            }
        );
    }

    fn wf(i: usize) -> ChainScope {
        ChainScope::Workflow {
            id: format!("wf_{i:016x}"),
        }
    }

    /// A valid spend.json of exactly `len` bytes: a machine ceiling, padded inside the object so
    /// a backup's trim keeps every byte.
    fn exact(len: usize) -> Vec<u8> {
        let mut file = SpendFile::default();
        file.ceilings.push(Ceiling {
            scope: ChainScope::Machine,
            cents: 500,
            set_at: T,
            doubled_from: None,
        });
        let body = serde_json::to_vec(&file).unwrap();
        let mut out = b"{".to_vec();
        out.resize(len - body.len() + 1, b' ');
        out.extend_from_slice(&body[1..]);
        assert_eq!(out.len(), len);
        out
    }

    struct Keys(Mutex<Option<[u8; 32]>>);
    impl switchboard_core::backup::BackupKey for Keys {
        fn load(&self) -> Result<Option<[u8; 32]>, String> {
            Ok(*self.0.lock().unwrap())
        }
        fn create(&self, key: &[u8; 32]) -> Result<bool, String> {
            let mut k = self.0.lock().unwrap();
            if k.is_some() {
                return Ok(false);
            }
            *k = Some(*key);
            Ok(true)
        }
    }

    #[test]
    fn a_spend_file_of_exactly_the_limit_comes_back_from_a_backup_readable() {
        // Break 1 (C fix cycle 2): a file of exactly MAX_BYTES was carried by the backup, but the
        // restore added a line end and the read then refused it as unreadable.
        use std::sync::Arc;
        use switchboard_core::{AuthKind, Credential, MemoryVault, Provider, Store};
        let temp = tempfile::tempdir().unwrap();
        let backups = temp.path().join("backups");
        let one = Store::open(temp.path().join("one"), Arc::new(MemoryVault::default())).unwrap();
        one.add(
            "Fixture".into(),
            Provider::Claude,
            AuthKind::ApiKey,
            "default".into(),
            Credential::parse(Provider::Claude, AuthKind::ApiKey, "fixture-only").unwrap(),
        )
        .unwrap();
        let bytes = exact(MAX_BYTES as usize);
        write_bytes(one.root(), &bytes).unwrap();
        assert_eq!(read(one.root()).unwrap().ceilings[0].cents, 500);
        let keys = Keys(Mutex::new(None));
        let info = switchboard_core::backup::write(&one, &backups, &keys, T)
            .unwrap()
            .unwrap();
        let two = Store::open(temp.path().join("two"), Arc::new(MemoryVault::default())).unwrap();
        let restored =
            switchboard_core::backup::restore(&two, &backups, &info.file, &keys).unwrap();
        assert_eq!(restored.settings, 1);
        assert_eq!(
            std::fs::read(two.root().join(FILE)).unwrap(),
            bytes,
            "byte for byte"
        );
        assert_eq!(read(two.root()).unwrap().ceilings[0].cents, 500);
    }

    /// The largest file the count caps and the field rules allow.
    fn largest() -> SpendFile {
        let pool = |i: usize| format!("{i:0>64}");
        let long = |c: char| std::iter::repeat_n(c, 64).collect::<String>();
        SpendFile {
            version: VERSION,
            ceilings: (0..MAX_CEILINGS)
                .map(|i| Ceiling {
                    scope: ChainScope::Project { pool: pool(i) },
                    cents: MAX_CENTS,
                    set_at: i64::MIN,
                    doubled_from: Some(MAX_CENTS),
                })
                .collect(),
            questions: (0..MAX_QUESTIONS)
                .map(|i| Question {
                    id: format!("q_{i:016x}"),
                    scope: ChainScope::Project { pool: pool(i) },
                    cents: MAX_CENTS,
                    spent: f64::MAX,
                    reached_at: i64::MIN,
                    answer: Some(Answer::Double),
                    answered_at: Some(i64::MIN),
                    answered_by: Some(Surface::Window),
                })
                .collect(),
            baselines: (0..MAX_BASELINES)
                .map(|i| Baseline {
                    workflow: format!("wf_{i:016x}"),
                    key: pool(i),
                    usage: Some(f64::MAX),
                    at: i64::MIN,
                })
                .collect(),
            last_handover: Some(LastHandover {
                at: i64::MIN,
                workflow: WF.into(),
                outcome: long('o'),
                skipped: (0..MAX_SKIPPED)
                    .map(|_| Skipped {
                        agent: long('a'),
                        reason: long('r'),
                    })
                    .collect(),
            }),
        }
    }

    #[test]
    fn the_largest_file_the_caps_allow_fits_under_the_limit() {
        // Break 1: 256 ceilings, 64 questions and 256 baselines with 64-character pools did not
        // fit 64 KiB, so a write near the caps was refused as if the file were unreadable.
        let file = largest();
        assert!(validate(&file));
        let size = serde_json::to_vec(&file).unwrap().len() as u64;
        assert!(size < MAX_BYTES, "{size} bytes, limit {MAX_BYTES}");
        let root = tempfile::tempdir().unwrap();
        write(root.path(), &file).unwrap();
        assert_eq!(read(root.path()).unwrap(), file);
    }

    #[test]
    fn a_provider_read_that_is_negative_nan_or_huge_is_never_counted_as_spend() {
        // Risk 3: decide filters each usage read; a negative or NaN read is unknown (a denial),
        // a huge one is spend that reaches the ceiling. Never a step that runs.
        let unknown = Verdict::Skip {
            reason: "spend_unknown",
            reached: None,
        };
        let reached = |v: &Verdict| {
            matches!(
                v,
                Verdict::Skip {
                    reason: "ceiling_reached",
                    ..
                }
            )
        };
        let daily = |v: f64| {
            Some(KeyRead {
                limit_remaining: Some(9.0),
                usage: Some(1.0),
                usage_daily: Some(v),
            })
        };
        let mut machine = SpendFile::default();
        machine.ceilings.push(Ceiling {
            scope: ChainScope::Machine,
            cents: 500,
            set_at: T,
            doubled_from: None,
        });
        let mut group = SpendFile::default();
        group.ceilings.push(Ceiling {
            scope: alpha(),
            cents: 500,
            set_at: T,
            doubled_from: None,
        });
        let s = snapshot();
        let on_machine = step(KeySlot::Machine);
        let on_alpha = step(KeySlot::Group("alpha".into()));
        for (value, bad) in [(-1.0, true), (f64::NAN, true), (1e308, false)] {
            // On the step's own key, machine and group ceilings alike.
            for (file, on) in [(&machine, &on_machine), (&group, &on_alpha)] {
                let v = decide(
                    file,
                    &s,
                    on,
                    &Reads {
                        key: daily(value),
                        others: vec![],
                    },
                );
                if bad {
                    assert_eq!(v, unknown, "{value} on the step's key");
                } else {
                    assert!(reached(&v), "{value} on the step's key: {v:?}");
                }
            }
            // On another saved key, which the machine ceiling adds up.
            let v = decide(
                &machine,
                &s,
                &on_machine,
                &Reads {
                    key: daily(0.0),
                    others: vec![daily(value)],
                },
            );
            if bad {
                assert_eq!(v, unknown, "{value} on another key");
            } else {
                assert!(reached(&v), "{value} on another key: {v:?}");
            }
        }
    }

    #[test]
    fn two_surfaces_setting_one_ceiling_at_once_never_loosen_it() {
        // Risk 4: the window sets $10 while the CLI sets $20 as a first ceiling. Whichever runs
        // first, the result is $10: the CLI's $20 is a raise once the window's $10 is saved.
        // Without the write lock both read an empty file and the CLI's $20 can win.
        let s = snapshot();
        for i in 0..300 {
            let root = tempfile::tempdir().unwrap();
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|t| {
                for (usd, by) in [(10.0, Surface::Window), (20.0, Surface::Cli)] {
                    let (root, s, barrier) = (root.path(), &s, &barrier);
                    t.spawn(move || {
                        barrier.wait();
                        let _ = set_ceiling(root, s, ChainScope::Machine, Some(usd), by, T);
                    });
                }
            });
            let file = read(root.path()).unwrap();
            assert_eq!(file.ceilings.len(), 1, "iteration {i}");
            assert_eq!(file.ceilings[0].cents, 1000, "iteration {i}");
        }
    }

    #[test]
    fn four_scopes_set_at_once_are_all_kept() {
        // Risk 4: one read-modify-write at a time; without the lock a write drops another's.
        let s = snapshot();
        for i in 0..300 {
            let root = tempfile::tempdir().unwrap();
            let barrier = std::sync::Barrier::new(4);
            let scopes = [ChainScope::Machine, alpha(), wf(1), wf(2)];
            std::thread::scope(|t| {
                for scope in scopes.clone() {
                    let (root, s, barrier) = (root.path(), &s, &barrier);
                    t.spawn(move || {
                        barrier.wait();
                        set_ceiling(root, s, scope, Some(3.0), Surface::Window, T).unwrap();
                    });
                }
            });
            let file = read(root.path()).unwrap();
            for scope in &scopes {
                assert!(
                    file.ceilings.iter().any(|c| &c.scope == scope),
                    "iteration {i}: {scope:?} lost"
                );
            }
        }
    }

    #[test]
    fn a_machine_question_always_fits_and_open_task_questions_age_out() {
        // Risk 5: 64 open task questions left no room, so a machine ceiling reached asked
        // nothing. The oldest open task question makes room; a task question is kept 7 days.
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        for i in 0..MAX_QUESTIONS {
            open_question(root.path(), &s, &wf(i), 100, 1.0, T + i as i64)
                .unwrap()
                .unwrap();
        }
        assert_eq!(open(root.path()).len(), MAX_QUESTIONS);
        let machine = open_question(root.path(), &s, &ChainScope::Machine, 500, 5.0, T + 100)
            .unwrap()
            .expect("a machine question always fits");
        let held = open(root.path());
        assert_eq!(held.len(), MAX_QUESTIONS);
        assert!(held.iter().any(|q| q.id == machine));
        assert!(
            !held.iter().any(|q| q.scope == wf(0)),
            "the oldest task question made room"
        );
        // A group question fits the same way.
        assert!(open_question(root.path(), &s, &alpha(), 500, 5.0, T + 101)
            .unwrap()
            .is_some());
        // Eight days on, the open task questions are gone; the next write drops them.
        let later = T + 8 * DAY;
        open_question(root.path(), &s, &ChainScope::Machine, 700, 7.0, later).unwrap();
        assert!(
            open(root.path())
                .iter()
                .all(|q| !matches!(q.scope, ChainScope::Workflow { .. })),
            "open task questions older than 7 days age out"
        );
    }

    #[test]
    fn task_ceilings_leave_room_for_the_machine_and_the_groups() {
        // Risk 5: 256 task ceilings refused a first machine ceiling. Tasks take at most 192.
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        for i in 0..192 {
            set_ceiling(root.path(), &s, wf(i), Some(1.0), Surface::Cli, T).unwrap();
        }
        assert_eq!(
            set_ceiling(root.path(), &s, wf(192), Some(1.0), Surface::Cli, T).unwrap_err(),
            TASK_CEILINGS_FULL
        );
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(5.0),
            Surface::Cli,
            T,
        )
        .unwrap();
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Cli, T).unwrap();
        assert_eq!(read(root.path()).unwrap().ceilings.len(), 194);
        // Groups take at most 63, so the machine's one always fits beside 192 tasks.
        let mut many = snapshot();
        for i in 0..MAX_GROUP_CEILINGS {
            let pool = format!("g{i}");
            many.projects.push(group(&pool));
            many.agent_keys.push(key(Some(&pool)));
        }
        let other = tempfile::tempdir().unwrap();
        for i in 0..MAX_GROUP_CEILINGS {
            let scope = ChainScope::Project {
                pool: format!("g{i}"),
            };
            set_ceiling(other.path(), &many, scope, Some(1.0), Surface::Cli, T).unwrap();
        }
        assert_eq!(
            set_ceiling(other.path(), &many, alpha(), Some(1.0), Surface::Cli, T).unwrap_err(),
            GROUP_CEILINGS_FULL
        );
        set_ceiling(
            other.path(),
            &many,
            ChainScope::Machine,
            Some(1.0),
            Surface::Cli,
            T,
        )
        .unwrap();
        assert_eq!(MAX_TASK_CEILINGS + MAX_GROUP_CEILINGS + 1, MAX_CEILINGS);
    }

    #[test]
    fn a_write_past_the_limit_is_refused_with_its_own_sentence_and_nothing_written() {
        // Break 1: the size refusal is not "unreadable"; the file on disk stays as it was.
        let root = tempfile::tempdir().unwrap();
        write_bytes(root.path(), &exact(MAX_BYTES as usize)).unwrap();
        let before = std::fs::read(root.path().join(FILE)).unwrap();
        assert_eq!(
            write_bytes(root.path(), &exact(MAX_BYTES as usize + 1)).unwrap_err(),
            TOO_LARGE
        );
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), before);
        assert_ne!(TOO_LARGE, UNREADABLE);
    }

    #[test]
    fn deleting_a_group_takes_its_ceiling_questions_and_baselines_and_keeps_the_rest() {
        // Risk 2 (C-Q7): only the window deletes a group that holds a ceiling; a group without
        // one is deleted from anywhere; what spend.json held for it goes with it.
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Window, T).unwrap();
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Machine,
            Some(9.0),
            Surface::Window,
            T,
        )
        .unwrap();
        open_question(root.path(), &s, &alpha(), 500, 5.0, T).unwrap();
        let mut file = read(root.path()).unwrap();
        for key in ["alpha", "machine"] {
            file.baselines.push(Baseline {
                workflow: WF.into(),
                key: key.into(),
                usage: Some(1.0),
                at: T,
            });
        }
        write(root.path(), &file).unwrap();
        let before = std::fs::read(root.path().join(FILE)).unwrap();
        let mut removed = false;
        for by in [Surface::Cli, Surface::Mcp] {
            assert_eq!(
                remove_group(root.path(), "alpha", by, || {
                    removed = true;
                    Ok(())
                })
                .unwrap_err(),
                GROUP_HOLDS_CEILING
            );
        }
        assert!(!removed, "the group is never touched");
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), before);
        // A group with nothing in spend.json is deleted from the CLI, the file unchanged.
        assert_eq!(
            remove_group(root.path(), "beta", Surface::Cli, || Ok(7)).unwrap(),
            (7, false)
        );
        // The window deletes alpha; its ceiling, question and baseline go, the machine's stay.
        assert_eq!(
            remove_group(root.path(), "alpha", Surface::Window, || Ok(())).unwrap(),
            ((), true)
        );
        let after = read(root.path()).unwrap();
        assert_eq!(after.ceilings.len(), 1);
        assert_eq!(after.ceilings[0].scope, ChainScope::Machine);
        assert!(after.questions.is_empty());
        assert_eq!(after.baselines.len(), 1);
        assert_eq!(after.baselines[0].key, "machine");
        // A removal that fails changes nothing.
        set_ceiling(root.path(), &s, alpha(), Some(5.0), Surface::Window, T).unwrap();
        assert_eq!(
            remove_group(root.path(), "alpha", Surface::Window, || Err::<(), _>(
                "Project not found.".to_string()
            ))
            .unwrap_err(),
            "Project not found."
        );
        assert_eq!(read(root.path()).unwrap().ceilings.len(), 2);
        // An unreadable file refuses every surface.
        std::fs::write(root.path().join(FILE), b"{").unwrap();
        for by in [Surface::Window, Surface::Cli] {
            assert_eq!(
                remove_group(root.path(), "beta", by, || Ok(())).unwrap_err(),
                UNREADABLE
            );
        }
    }

    #[test]
    fn a_tasks_first_baseline_stays_and_a_full_list_makes_room_without_losing_a_capped_task() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        let id = |i: usize| format!("wf_{i:016x}");
        assert!(record_baseline(root.path(), &s, WF, &KeySlot::Machine, Some(10.0), T).unwrap());
        assert!(
            !record_baseline(root.path(), &s, WF, &KeySlot::Machine, Some(12.0), T + 1).unwrap()
        );
        assert_eq!(
            read(root.path()).unwrap().baselines[0].usage,
            Some(10.0),
            "the first one stays"
        );
        // A negative read or a bad id records nothing.
        assert!(
            !record_baseline(root.path(), &s, "wf_x", &KeySlot::Machine, Some(1.0), T).unwrap()
        );
        assert!(
            !record_baseline(root.path(), &s, &id(1), &KeySlot::Machine, Some(-1.0), T).unwrap()
        );
        // Full: the oldest baseline of a task with no ceiling makes room, never a capped one's.
        set_ceiling(
            root.path(),
            &s,
            ChainScope::Workflow { id: WF.into() },
            Some(1.0),
            Surface::Cli,
            T,
        )
        .unwrap();
        for i in 1..MAX_BASELINES {
            assert!(record_baseline(
                root.path(),
                &s,
                &id(i),
                &KeySlot::Machine,
                Some(1.0),
                T + i as i64
            )
            .unwrap());
        }
        assert!(record_baseline(
            root.path(),
            &s,
            &id(9999),
            &KeySlot::Machine,
            Some(1.0),
            T + 9999
        )
        .unwrap());
        let file = read(root.path()).unwrap();
        assert_eq!(file.baselines.len(), MAX_BASELINES);
        assert!(
            file.baselines.iter().any(|b| b.workflow == WF),
            "the capped task keeps it"
        );
        assert!(
            !file.baselines.iter().any(|b| b.workflow == id(1)),
            "the oldest uncapped went"
        );
    }
}
