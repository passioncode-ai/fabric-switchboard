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
//! This module stores, validates and decides; the hand-over gate that calls [`check`] is C-5.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;
use switchboard_core::{workflow_id_valid, ChainScope, Snapshot};

pub const FILE: &str = "spend.json";
/// The backup carries this file whole (`backup::setting_limit`): 256 ceilings, 64 questions and
/// 256 baselines fit with room.
pub const MAX_BYTES: u64 = 64 * 1024;
const VERSION: u32 = 1;
/// A ceiling is whole cents from $0.01 to $1,000, the estate's monthly total (authority L0).
pub const MIN_CENTS: u32 = 1;
pub const MAX_CENTS: u32 = 100_000;
/// A paid step is not handed work on a key with less than this left of its own limit: a session
/// started on an almost empty key dies at the provider after the offer, which stops the chain.
pub const MIN_REMAINING_USD: f64 = 0.50;
pub const MAX_CEILINGS: usize = 256;
pub const MAX_QUESTIONS: usize = 64;
pub const MAX_BASELINES: usize = 256;
const MAX_SKIPPED: usize = 8;
const DAY: i64 = 86_400;
/// Answered questions and task baselines are kept this long.
const KEEP: i64 = 30 * DAY;

pub const UNREADABLE: &str = "Spending ceilings (spend.json) could not be read, so no paid agent is handed work until they are. Restore it from a backup, or delete it to start over with no ceilings.";
pub const OUT_OF_RANGE: &str = "Enter an amount from $0.01 to $1,000.";
pub const NO_OWN_KEY: &str = "Give this group its own key under API keys first.";
pub const NO_GROUP: &str = "That group id names no group.";
pub const DOUBLE_IN_WINDOW: &str = "Doubling a ceiling is answered in the Switchboard window.";
pub const DOUBLE_TOO_HIGH: &str =
    "Doubling would pass $1,000, the most a ceiling may be. Set a ceiling of up to $1,000 instead.";
/// A request that names the window but did not come from the window's own process (C-Q1).
pub const WINDOW_ONLY: &str = "Only the Switchboard window answers as the window. Keep a ceiling from here, or double it in the window.";
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
    /// The key's total usage then, in dollars.
    pub usage: f64,
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
/// file unreadable, so a ceiling is never silently lost or loosened.
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
            workflow_id_valid(&b.workflow) && short_text_valid(&b.key) && amount_valid(b.usage)
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
    if bytes.len() as u64 > MAX_BYTES {
        return Err(UNREADABLE.into());
    }
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
}

/// Whether a group has its own OpenRouter key (SB-144): only such a group takes a daily ceiling.
pub fn group_has_own_key(snapshot: &Snapshot, pool: &str) -> bool {
    snapshot
        .agent_keys
        .iter()
        .any(|k| k.service == "openrouter" && k.pool.as_deref() == Some(pool))
}

/// Drops what removed groups and time leave behind. A clock set back drops nothing: an age is
/// never negative.
pub(crate) fn prune(file: &mut SpendFile, snapshot: &Snapshot, now: i64) {
    let group_exists = |pool: &str| snapshot.projects.iter().any(|p| p.pool == pool);
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
                // A daily ceiling's question no longer applies after the next UTC day.
                None => {
                    matches!(q.scope, ChainScope::Workflow { .. })
                        || day(now) <= day(q.reached_at) + 1
                }
            }
    });
    file.baselines
        .retain(|b| (b.key == "machine" || group_exists(&b.key)) && !older_than(b.at, KEEP));
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
    snapshot: &Snapshot,
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
/// an answer is tied to the value it was asked about.
pub fn set_ceiling(
    root: &Path,
    snapshot: &Snapshot,
    scope: ChainScope,
    usd: Option<f64>,
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
    let set = change(root, snapshot, now, |file| {
        file.ceilings.retain(|c| c.scope != scope);
        // An answer is tied to the value it was asked about: a new value closes the old question.
        file.questions
            .retain(|q| q.scope != scope || q.answer.is_some());
        let Some(cents) = value else {
            return Ok(None);
        };
        if file.ceilings.len() >= MAX_CEILINGS {
            return Err("At most 256 spending ceilings. Clear one first.".into());
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
/// id; `None` when every slot holds an open question.
pub fn open_question(
    root: &Path,
    snapshot: &Snapshot,
    scope: &ChainScope,
    cents: u32,
    spent: f64,
    now: i64,
) -> Result<Option<String>, String> {
    let spent = if amount_valid(spent) { spent } else { 0.0 };
    let opened = change(root, snapshot, now, |file| {
        if let Some(q) = file
            .questions
            .iter()
            .find(|q| q.answer.is_none() && &q.scope == scope && q.cents == cents)
        {
            return Ok((Some(q.id.clone()), false));
        }
        if file.questions.len() >= MAX_QUESTIONS {
            // Room is made from answered questions, oldest first; open ones are never dropped.
            match file
                .questions
                .iter()
                .enumerate()
                .filter(|(_, q)| q.answer.is_some())
                .min_by_key(|(_, q)| q.answered_at)
                .map(|(i, _)| i)
            {
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
    let (scope, ceiling) = change(root, snapshot, now, |file| {
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
    prune(&mut file, snapshot, now);
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

/// The decision for one paid step, in order: the key's own limit nearly spent, a ceiling reached,
/// a covering ceiling whose spend could not be read. Pure: the caller reads.
pub fn decide(file: &SpendFile, step: &PaidStep, reads: &Reads) -> Verdict {
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
            ChainScope::Project { pool } => step.key == KeySlot::Group(pool.clone()),
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
                    // A key whose total went down is another key: unknown, never zero.
                    Some(b) => Some(usage - b.usage).filter(|d| *d >= 0.0),
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
pub fn check(root: &Path, step: &PaidStep, reads: &Reads) -> Verdict {
    match read(root) {
        Ok(file) => decide(&file, step, reads),
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
        set_ceiling(root.path(), &s, alpha(), Some(5.0), T).unwrap();
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
        set_ceiling(root.path(), &s, alpha(), Some(7.0), T + 70).unwrap();
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
            set_ceiling(root.path(), &s, beta, Some(5.0), T).unwrap_err(),
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
                T
            )
            .unwrap_err(),
            NO_GROUP
        );
        assert!(read(root.path()).unwrap().ceilings.is_empty());
        // With its own key it takes one; when the key goes, the ceiling stays, shown inactive.
        set_ceiling(root.path(), &s, alpha(), Some(5.0), T).unwrap();
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
            set_ceiling(root.path(), &s, alpha(), None, T).unwrap(),
            None
        );
        assert!(read(root.path()).unwrap().ceilings.is_empty());
    }

    #[test]
    fn double_is_refused_from_mcp_and_cli() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(5.0), T).unwrap();
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
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(600.0), T + 2).unwrap();
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
            check(root.path(), &step(KeySlot::Machine), &reads()),
            Verdict::Run
        );
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert_eq!(read(root.path()).unwrap_err(), UNREADABLE);
        assert_eq!(view(root.path(), &snapshot(), T).unwrap_err(), UNREADABLE);
        assert_eq!(
            check(root.path(), &step(KeySlot::Machine), &reads()),
            Verdict::Skip {
                reason: "ceilings_unreadable",
                reached: None
            }
        );
        // A write over it is refused too: the operator's values are not overwritten blind.
        assert_eq!(
            set_ceiling(root.path(), &snapshot(), ChainScope::Machine, Some(1.0), T).unwrap_err(),
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
                set_ceiling(root.path(), &s, ChainScope::Machine, Some(usd), T).unwrap_err(),
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
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(3.0), T).unwrap();
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(4.0), T).unwrap();
        let c = read(root.path()).unwrap().ceilings;
        assert_eq!((c.len(), c[0].cents), (1, 400));
        // A task ceiling needs a workflow id.
        assert!(set_ceiling(
            root.path(),
            &s,
            ChainScope::Workflow { id: "wf_x".into() },
            Some(1.0),
            T
        )
        .is_err());
    }

    #[test]
    fn a_clock_set_back_drops_nothing_and_answers_still_work() {
        let root = tempfile::tempdir().unwrap();
        let s = snapshot();
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(5.0), T).unwrap();
        let kept = open_question(root.path(), &s, &ChainScope::Machine, 500, 5.0, T)
            .unwrap()
            .unwrap();
        answer(root.path(), &s, &kept, Answer::Keep, Surface::Window, T).unwrap();
        let task = ChainScope::Workflow { id: WF.into() };
        set_ceiling(root.path(), &s, task.clone(), Some(2.0), T).unwrap();
        let waiting = open_question(root.path(), &s, &task, 200, 2.5, T)
            .unwrap()
            .unwrap();
        let mut file = read(root.path()).unwrap();
        file.baselines.push(Baseline {
            workflow: WF.into(),
            key: "machine".into(),
            usage: 1.0,
            at: T,
        });
        write(root.path(), &file).unwrap();
        // The clock goes back a year: nothing is older than it was, so nothing goes.
        let earlier = T - 365 * DAY;
        let mut pruned = read(root.path()).unwrap();
        prune(&mut pruned, &s, earlier);
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
        // Time going forward again past 30 days drops the answered questions and the baseline.
        let mut later = file.clone();
        prune(&mut later, &s, T + 31 * DAY);
        assert!(later.questions.is_empty());
        assert!(later.baselines.is_empty());
        assert_eq!(later.ceilings.len(), 2, "ceilings never age out");
        // An open daily question stays answerable through the next UTC day, then goes.
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(6.0), T).unwrap();
        let daily = open_question(root.path(), &s, &ChainScope::Machine, 600, 6.0, T)
            .unwrap()
            .unwrap();
        let mut f = read(root.path()).unwrap();
        prune(&mut f, &s, T + DAY);
        assert!(f.questions.iter().any(|q| q.id == daily));
        prune(&mut f, &s, T + 3 * DAY);
        assert!(!f.questions.iter().any(|q| q.id == daily));
    }

    #[test]
    fn a_removed_group_takes_its_ceiling_questions_and_baselines_with_it() {
        let root = tempfile::tempdir().unwrap();
        let mut s = snapshot();
        set_ceiling(root.path(), &s, alpha(), Some(5.0), T).unwrap();
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(9.0), T).unwrap();
        open_question(root.path(), &s, &alpha(), 500, 5.0, T).unwrap();
        let mut file = read(root.path()).unwrap();
        file.baselines.push(Baseline {
            workflow: WF.into(),
            key: "alpha".into(),
            usage: 1.0,
            at: T,
        });
        write(root.path(), &file).unwrap();
        s.projects.retain(|p| p.pool != "alpha");
        s.agent_keys.retain(|k| k.pool.is_none());
        let mut f = read(root.path()).unwrap();
        prune(&mut f, &s, T);
        assert_eq!(f.ceilings.len(), 1);
        assert_eq!(f.ceilings[0].scope, ChainScope::Machine);
        assert!(f.questions.is_empty());
        assert!(f.baselines.is_empty());
        // The view prunes in memory only; the next write prunes on disk.
        let shown = view(root.path(), &s, T).unwrap();
        assert_eq!(shown["ceilings"].as_array().unwrap().len(), 1);
        set_ceiling(root.path(), &s, ChainScope::Machine, Some(8.0), T).unwrap();
        assert_eq!(read(root.path()).unwrap().ceilings.len(), 1);
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
            decide(&file, &step(KeySlot::Machine), &failed),
            Verdict::RunUnread
        );
        // A machine ceiling covers every paid step: the same failed read is a denial.
        file.ceilings.push(Ceiling {
            scope: ChainScope::Machine,
            cents: 500,
            set_at: T,
            doubled_from: None,
        });
        assert_eq!(decide(&file, &step(KeySlot::Machine), &failed), unknown);
        // The step's key reads, but another saved key the machine ceiling adds up does not.
        let other_failed = Reads {
            key: full(1.0, 1.0),
            others: vec![None],
        };
        assert_eq!(
            decide(&file, &step(KeySlot::Machine), &other_failed),
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
        assert_eq!(decide(&file, &step(KeySlot::Machine), &no_daily), unknown);
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
        assert_eq!(decide(&file, &api, &readable), unknown);
        assert_eq!(
            decide(&SpendFile::default(), &api, &readable),
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
            usage: 50.0,
            at: T,
        });
        assert_eq!(
            decide(
                &task,
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
            decide(&file, &step(KeySlot::Machine), &exhausted),
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
            decide(&file, &step(KeySlot::Machine), &at_floor),
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
            decide(&file, &step(KeySlot::Machine), &unlimited),
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
            decide(&file, &on_alpha, &reads),
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
        assert_eq!(decide(&file, &on_machine, &machine_reads), Verdict::Run);
        // Both reached: the stricter ceiling ($3) is named.
        let both = Reads {
            key: full(6.0, 40.0),
            others: vec![full(2.5, 2.5)],
        };
        assert_eq!(
            decide(&file, &on_alpha, &both),
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
            decide(&file, &on_alpha, &partly),
            Verdict::Skip {
                reason: "ceiling_reached",
                reached: Some((alpha(), 300, 3.5))
            }
        );
    }
}
