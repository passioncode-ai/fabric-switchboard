//! SB-404 (operator, 2026-11-17: «Чтобы можно было добавить токены ChatGPT, OpenRouter, Anthropic,
//! либо они подтягивались в Observatory автоматически, и можно было бы их использовать для запуска
//! агентов сразу»): the provider keys Project Observatory holds, by name, and adding one to
//! Switchboard without its value ever passing through an argument, a file, a log or an answer.
//!
//! K1 reads the names: `vault.py list` prints each slot as `project/env/NAME (size, notes)`, never
//! a value. Only `ANTHROPIC_*`, `OPENAI_*` and `OPENROUTER_*` slots that hold a key are listed;
//! organization admin, management, provisioning and BYOK keys and archived `_PREVIOUS` copies are
//! not, since no agent runs on one.
//!
//! K2 adds one by name. `plan` (in the owner) checks the slot and says which saves it makes; the
//! caller then runs each save as Observatory's `use_secret.py run --vault-only <slot> -- /bin/sh -c
//! <script> <switchboard> <save command>`: the engine puts the value in the shell's environment,
//! the shell moves it into an unexported variable, removes it from its environment and writes it
//! to the save command's stdin, so the switchboard process that saves it reads it from a pipe as
//! when a person pipes a key. `record` then notes the slot each saved key came from in
//! `observatory-keys.json` (0600; slots and targets, never a value) so a rotation can be followed.
//!
//! Where a key goes (DEC-0024): an Anthropic key becomes a Claude API-key account, which lands
//! in the pay-per-token group (DEC-0023) and is metered there; with `--to agent` it is also, or
//! instead, a named Anthropic key for Hermes (unmetered, SB-390). An OpenRouter key becomes the
//! machine's or a group's OpenRouter key, under its ceiling (SB-72). An OpenAI key is listed but
//! not added yet: no ceiling counts one.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use switchboard_core::Store;

pub const FILE: &str = "observatory-keys.json";
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const PATH_TIMEOUT: Duration = Duration::from_secs(10);
const SAVE_TIMEOUT: Duration = Duration::from_secs(60);
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const MAX_RECORDS: usize = 256;

pub const NOT_INSTALLED: &str = "Project Observatory is not installed on this computer, so no key can come from it. Save a key by name with switchboard agents anthropic set or switchboard agents openrouter set instead.";
pub const UNREADABLE: &str =
    "Project Observatory did not list its keys. Check it with project-observatory full-path, then try again.";
pub const SLOT_INVALID: &str =
    "Name the key as Observatory does: project/env/NAME, with env local, stage or prod.";
pub const NOT_LISTED: &str = "Project Observatory holds no provider key under that name. switchboard keys observatory lists the ones it holds.";
pub const OPENAI_NOT_YET: &str = "An OpenAI key cannot be added yet: no spending ceiling counts one in Switchboard. Run Codex on a ChatGPT sign-in, or an agent on an OpenRouter key.";
pub const TARGET_INVALID: &str = "An Anthropic key goes to an account in the pay-per-token group, a named key for Hermes, or both; an OpenRouter key goes to the agents' key.";
pub const NAME_NEEDED: &str =
    "Give the key a name: 1 to 32 lowercase letters, digits and hyphens, such as appvillis.";
pub const NAME_TAKEN: &str = "An Anthropic key is already saved under that name. Choose another name, or replace the saved key.";
pub const OPENROUTER_TAKEN: &str =
    "An OpenRouter key is already saved there. Replace it, or choose a group for this one.";
pub const ALREADY_ACCOUNT: &str = "This key is already an account in the pay-per-token group. Remove that account first to add it again.";
pub const POOL_NOT_FOR_ANTHROPIC: &str =
    "A Claude API key always goes to the pay-per-token group; choose no group for it.";
pub const UNIX_ONLY: &str = "Adding a key from Observatory runs on macOS and Linux for now.";
pub const SAVE_FAILED: &str = "The key could not be saved from Observatory. Check the slot with project-observatory, then try again.";
pub const SAVE_TIMED_OUT: &str = "Saving the key from Observatory took longer than a minute and was stopped. Nothing was recorded; try again.";
pub const RECORDS_UNREADABLE: &str =
    "The record of keys added from Observatory (observatory-keys.json) could not be read. Remove it to start a new one.";
pub const RECORDS_UNSAVED: &str =
    "The key was saved, but where it came from could not be recorded.";

/// The provider a key is for, read from its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Anthropic,
    Openai,
    Openrouter,
}

/// Name parts that mark a key no agent runs on: an organization's admin or management key, a
/// provisioning key, a key given to a provider as BYOK, or an archived copy after a rotation.
const NOT_AN_AGENT_KEY: &[&str] = &[
    "ADMIN",
    "MANAGEMENT",
    "MGMT",
    "BYOK",
    "PROVISIONING",
    "PREVIOUS",
];

/// The kind of an agent's provider key, or `None` for any other slot (an admin or management key,
/// a URL, a model name).
pub fn classify(name: &str) -> Option<Kind> {
    if !name_valid(name) {
        return None;
    }
    let kind = if name.starts_with("ANTHROPIC_") {
        Kind::Anthropic
    } else if name.starts_with("OPENAI_") {
        Kind::Openai
    } else if name.starts_with("OPENROUTER_") {
        Kind::Openrouter
    } else {
        return None;
    };
    let parts: Vec<&str> = name.split('_').collect();
    if parts.iter().any(|p| NOT_AN_AGENT_KEY.contains(p)) {
        return None;
    }
    // A key or a token; not a base URL, a model or an organization id.
    parts
        .iter()
        .any(|p| matches!(*p, "KEY" | "TOKEN"))
        .then_some(kind)
}

/// A vault slot's NAME: an environment variable, upper case.
fn name_valid(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_uppercase()
        && b.iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
}
fn project_valid(project: &str) -> bool {
    let b = project.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}
fn env_valid(env: &str) -> bool {
    matches!(env, "local" | "stage" | "prod")
}

/// One slot, `project/env/NAME`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub project: String,
    pub env: String,
    pub name: String,
}
impl Slot {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.trim().split('/');
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(p), Some(e), Some(n), None)
                if project_valid(p) && env_valid(e) && name_valid(n) =>
            {
                Ok(Self {
                    project: p.into(),
                    env: e.into(),
                    name: n.into(),
                })
            }
            _ => Err(SLOT_INVALID.into()),
        }
    }
    pub fn text(&self) -> String {
        format!("{}/{}/{}", self.project, self.env, self.name)
    }
}

/// One provider key as `vault.py list` names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub slot: Slot,
    pub kind: Kind,
    /// When the minting door recorded an expiry (`expires 2027-11-13`).
    pub expires: Option<String>,
    /// The last rotation's day (`rotated 2026-11-14×1`).
    pub rotated: Option<String>,
}

/// The provider keys in `vault.py list`'s text. A line that is not a slot, or a slot that holds
/// no agent's provider key, is left out; a line marked "not a credential" too.
pub fn parse_list(text: &str) -> Vec<Listed> {
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((slot, rest)) = line.split_once("  (") else {
            continue;
        };
        let Ok(slot) = Slot::parse(slot) else {
            continue;
        };
        let Some(kind) = classify(&slot.name) else {
            continue;
        };
        let notes = rest.strip_suffix(')').unwrap_or(rest);
        if notes.contains("not a credential") {
            continue;
        }
        let mut expires = None;
        let mut rotated = None;
        for note in notes.split(", ") {
            let day = |prefix: &str| {
                note.strip_prefix(prefix)
                    .map(|d| d.chars().take(10).collect::<String>())
                    .filter(|d| d.len() == 10 && d.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
            };
            if let Some(d) = day("expires ") {
                expires = Some(d);
            }
            if let Some(d) = day("rotated ") {
                rotated = Some(d);
            }
        }
        keys.push(Listed {
            slot,
            kind,
            expires,
            rotated,
        });
    }
    keys
}

/// The engine folder and the Python that runs its tools.
#[derive(Clone, Debug)]
pub struct Engine {
    pub dir: PathBuf,
    pub python: PathBuf,
}

/// Observatory's engine on this computer: `project-observatory full-path`, run with the Python of
/// the virtual environment it lives in.
pub fn engine() -> Result<Engine, String> {
    if !cfg!(unix) {
        return Err(UNIX_ONLY.into());
    }
    let bin = crate::continuation::observatory().map_err(|_| NOT_INSTALLED.to_string())?;
    let mut command = Command::new(&bin);
    command.arg("full-path");
    let output = bounded(command, PATH_TIMEOUT).map_err(|_| NOT_INSTALLED.to_string())?;
    if !output.success {
        return Err(NOT_INSTALLED.into());
    }
    let dir = PathBuf::from(output.stdout.trim());
    if !dir.is_absolute() || !dir.join("tools").join("vault.py").is_file() {
        return Err(NOT_INSTALLED.into());
    }
    let python = python_for(&bin, &dir).ok_or(NOT_INSTALLED)?;
    Ok(Engine { dir, python })
}

/// The Python beside the engine's own executable (its virtual environment), else the one whose
/// environment holds the engine folder, else `python3` on the PATH.
fn python_for(bin: &Path, dir: &Path) -> Option<PathBuf> {
    let beside = bin
        .canonicalize()
        .ok()
        .and_then(|b| b.parent().map(|p| p.join("python3")))
        .filter(|p| p.is_file());
    let venv = || {
        dir.ancestors()
            .find(|a| a.join("pyvenv.cfg").is_file())
            .map(|a| a.join("bin").join("python3"))
            .filter(|p| p.is_file())
    };
    beside
        .or_else(venv)
        .or_else(|| crate::launch::find_program("python3"))
}

struct Output {
    success: bool,
    stdout: String,
    stderr: String,
}

/// One bounded call: both pipes read on their own threads and capped; at the deadline the child
/// and its process group are killed and reaped (LC-02). `Err(true)` is a timeout.
fn bounded(mut command: Command, timeout: Duration) -> Result<Output, bool> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|_| false)?;
    let pipe = |reader: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(reader) = reader {
                let _ = reader
                    .take((OUTPUT_LIMIT + 1) as u64)
                    .read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let out = pipe(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = pipe(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                #[cfg(unix)]
                if let Ok(group) = libc::pid_t::try_from(child.id()) {
                    // SAFETY: killpg only sends a signal to the group this call created.
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
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    let status = status.ok_or(true)?;
    if stdout.len() > OUTPUT_LIMIT || stderr.len() > OUTPUT_LIMIT {
        return Err(false);
    }
    Ok(Output {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// The provider keys Observatory holds now, by name.
pub fn read(engine: &Engine) -> Result<Vec<Listed>, String> {
    let mut command = Command::new(&engine.python);
    command
        .arg(engine.dir.join("tools").join("vault.py"))
        .arg("list");
    let output = bounded(command, LIST_TIMEOUT).map_err(|_| UNREADABLE.to_string())?;
    if !output.success {
        return Err(UNREADABLE.into());
    }
    Ok(parse_list(&output.stdout))
}

/// Where one saved key went: an account, a named Anthropic key or an OpenRouter key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    /// A Claude API-key account (its home is the pay-per-token group).
    Account { account_id: String },
    /// A named Anthropic key for Hermes (`anthropic-keys.json`).
    Anthropic { name: String },
    /// The machine's OpenRouter key (`pool: null`) or a group's.
    Openrouter {
        #[serde(default)]
        pool: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    slot: String,
    target: Target,
    added_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Records {
    keys: Vec<Record>,
}

fn records(root: &Path) -> Result<Records, String> {
    match std::fs::read(root.join(FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| RECORDS_UNREADABLE.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Records::default()),
        Err(_) => Err(RECORDS_UNREADABLE.into()),
    }
}

/// Whether a target still exists: the account, the named key or the OpenRouter key.
fn present(root: &Path, snapshot: &switchboard_core::Snapshot, target: &Target) -> bool {
    match target {
        Target::Account { account_id } => snapshot.accounts.iter().any(|a| &a.id == account_id),
        Target::Anthropic { name } => crate::anthropic_keys::find(root, name).is_ok(),
        Target::Openrouter { pool } => snapshot
            .agent_keys
            .iter()
            .any(|k| k.service == "openrouter" && &k.pool == pool),
    }
}

/// Notes that `slot` was saved as `targets` now. A target saved again from another slot moves to
/// that slot; the record keeps the newest 256 and drops the ones whose target is gone.
pub fn record(
    root: &Path,
    store: &Store,
    slot: &str,
    targets: &[Target],
    now: i64,
) -> Result<Value, String> {
    let slot = Slot::parse(slot)?.text();
    let snapshot = store.snapshot()?;
    let mut records = records(root)?;
    records.keys.retain(|r| !targets.contains(&r.target));
    records.keys.extend(targets.iter().map(|target| Record {
        slot: slot.clone(),
        target: target.clone(),
        added_at: now,
    }));
    records.keys.retain(|r| present(root, &snapshot, &r.target));
    if records.keys.len() > MAX_RECORDS {
        let extra = records.keys.len() - MAX_RECORDS;
        records.keys.drain(..extra);
    }
    let bytes = serde_json::to_vec_pretty(&records).map_err(|_| RECORDS_UNSAVED)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| RECORDS_UNSAVED.to_string())?;
    crate::oplog::event(
        "observatory_key",
        &[("outcome", crate::oplog::Field::Code("added"))],
    );
    Ok(json!({"slot": slot, "targets": targets}))
}

/// What each target is called for a person: the account's label and group, the key's name.
fn target_view(snapshot: &switchboard_core::Snapshot, target: &Target) -> Value {
    match target {
        Target::Account { account_id } => {
            let account = snapshot.accounts.iter().find(|a| &a.id == account_id);
            json!({
                "kind": "account",
                "account_id": account_id,
                "label": account.map(|a| a.label.clone()),
                "pool": account.map(|a| a.pool.clone()),
                "metered": true,
            })
        }
        Target::Anthropic { name } => json!({"kind": "anthropic", "name": name, "metered": false}),
        Target::Openrouter { pool } => json!({"kind": "openrouter", "pool": pool, "metered": true}),
    }
}

/// K1: the provider keys Observatory holds, by name, each with where Switchboard has saved it.
/// Observatory missing or unreadable is an answer (`state`), never an error.
pub fn list(root: &Path, store: &Store) -> Result<Value, String> {
    list_from(root, store, engine().and_then(|e| read(&e)))
}

fn list_from(
    root: &Path,
    store: &Store,
    listed: Result<Vec<Listed>, String>,
) -> Result<Value, String> {
    let snapshot = store.snapshot()?;
    let (state, keys, why) = match listed {
        Ok(keys) => ("ok", keys, None),
        Err(e) if e == NOT_INSTALLED || e == UNIX_ONLY => ("absent", Vec::new(), Some(e)),
        Err(e) => ("unreadable", Vec::new(), Some(e)),
    };
    let (saved, degraded) = match records(root) {
        Ok(r) => (r.keys, Vec::<Value>::new()),
        Err(e) => (Vec::new(), vec![json!({"source": FILE, "reason": e})]),
    };
    let keys: Vec<Value> = keys
        .iter()
        .map(|k| {
            let slot = k.slot.text();
            let added: Vec<Value> = saved
                .iter()
                .filter(|r| r.slot == slot && present(root, &snapshot, &r.target))
                .map(|r| target_view(&snapshot, &r.target))
                .collect();
            json!({
                "slot": slot,
                "project": k.slot.project,
                "env": k.slot.env,
                "name": k.slot.name,
                "kind": k.kind,
                "expires": k.expires,
                "rotated": k.rotated,
                "can_add": k.kind != Kind::Openai,
                "why_not": (k.kind == Kind::Openai).then_some(OPENAI_NOT_YET),
                "added": added,
            })
        })
        .collect();
    Ok(json!({
        "state": state,
        "why": why,
        "keys": keys,
        "degraded": degraded,
    }))
}

/// K2: what adding a key asks for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddRequest {
    pub slot: String,
    /// `account`, `agent` or `both`; default `account` for an Anthropic key, `agent` otherwise.
    #[serde(default)]
    pub to: Option<String>,
    /// The account's label, or the named Anthropic key's name.
    #[serde(default, rename = "as")]
    pub name: Option<String>,
    /// An OpenRouter key's group; none: this computer's key.
    #[serde(default)]
    pub pool: Option<String>,
    /// The default model of a named key.
    #[serde(default)]
    pub model: Option<String>,
    /// Replace a named Anthropic key or an OpenRouter key already saved there.
    #[serde(default)]
    pub replace: bool,
}

/// A name for a named key from the project's folder name (`appvillis-bot` from `Appvillis_Bot`).
fn derived_name(project: &str) -> Option<String> {
    let mut name = String::new();
    for c in project.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            name.push(c);
        } else if !name.ends_with('-') && !name.is_empty() {
            name.push('-');
        }
        if name.len() >= 32 {
            break;
        }
    }
    let name = name.trim_matches('-').to_owned();
    crate::anthropic_keys::name_valid(&name).then_some(name)
}

/// K2: checks a request against what Observatory holds and what Switchboard has saved, and says
/// which saves it makes: each step is a switchboard command that reads the value on stdin.
pub fn plan(root: &Path, store: &Store, request: &AddRequest) -> Result<Value, String> {
    let engine = engine()?;
    let listed = read(&engine)?;
    plan_from(root, store, request, &engine, &listed)
}

fn plan_from(
    root: &Path,
    store: &Store,
    request: &AddRequest,
    engine: &Engine,
    listed: &[Listed],
) -> Result<Value, String> {
    let slot = Slot::parse(&request.slot)?;
    let key = listed.iter().find(|k| k.slot == slot).ok_or(NOT_LISTED)?;
    let snapshot = store.snapshot()?;
    let saved = records(root)?;
    let mut steps = Vec::new();
    match key.kind {
        Kind::Openai => return Err(OPENAI_NOT_YET.into()),
        Kind::Anthropic => {
            let (account, agent) = match request.to.as_deref().unwrap_or("account") {
                "account" => (true, false),
                "agent" => (false, true),
                "both" => (true, true),
                _ => return Err(TARGET_INVALID.into()),
            };
            if request.pool.is_some() {
                return Err(POOL_NOT_FOR_ANTHROPIC.into());
            }
            if let Some(model) = &request.model {
                if !crate::anthropic_keys::model_valid(model) {
                    return Err(crate::anthropic_keys::MODEL_INVALID.into());
                }
            }
            if account {
                if saved.keys.iter().any(|r| {
                    r.slot == slot.text()
                        && matches!(r.target, Target::Account { .. })
                        && present(root, &snapshot, &r.target)
                }) {
                    return Err(ALREADY_ACCOUNT.into());
                }
                let label = match &request.name {
                    Some(name) if !name.trim().is_empty() && name.len() <= 80 => {
                        name.trim().to_owned()
                    }
                    Some(_) => return Err(NAME_NEEDED.into()),
                    None => format!("{} ({})", slot.project, slot.env),
                };
                steps.push(json!({
                    "target": "account",
                    "args": ["accounts", "add", "--provider", "claude", "--kind", "api-key",
                             "--label", label, "--pool", "default", "--secret-stdin"],
                }));
            }
            if agent {
                let name = match &request.name {
                    Some(name) => name.clone(),
                    None => derived_name(&slot.project).ok_or(NAME_NEEDED)?,
                };
                if !crate::anthropic_keys::name_valid(&name) {
                    return Err(NAME_NEEDED.into());
                }
                if !request.replace && crate::anthropic_keys::find(root, &name).is_ok() {
                    return Err(NAME_TAKEN.into());
                }
                let mut args = vec![
                    "agents".to_string(),
                    "anthropic".into(),
                    "set".into(),
                    name.clone(),
                    "--key-stdin".into(),
                ];
                if let Some(model) = &request.model {
                    args.extend(["--model".into(), model.clone()]);
                }
                steps.push(json!({"target": "anthropic", "name": name, "args": args}));
            }
        }
        Kind::Openrouter => {
            if !matches!(request.to.as_deref(), None | Some("agent")) {
                return Err(TARGET_INVALID.into());
            }
            if let Some(model) = &request.model {
                if !switchboard_core::agent_keys::model_valid(model) {
                    return Err(switchboard_core::agent_keys::MODEL_INVALID.into());
                }
            }
            if let Some(pool) = &request.pool {
                if !snapshot.projects.iter().any(|p| &p.pool == pool) {
                    return Err(switchboard_core::agent_keys::KEY_POOL_INVALID.into());
                }
            }
            let taken = snapshot
                .agent_keys
                .iter()
                .any(|k| k.service == "openrouter" && k.pool == request.pool);
            if taken && !request.replace {
                return Err(OPENROUTER_TAKEN.into());
            }
            let mut args = vec![
                "agents".to_string(),
                "openrouter".into(),
                "set".into(),
                "--key-stdin".into(),
            ];
            if let Some(pool) = &request.pool {
                args.extend(["--pool".into(), pool.clone()]);
            }
            if let Some(model) = &request.model {
                args.extend(["--model".into(), model.clone()]);
            }
            steps.push(json!({"target": "openrouter", "pool": request.pool, "args": args}));
        }
    }
    Ok(json!({
        "slot": slot.text(),
        "kind": key.kind,
        "engine": engine.dir,
        "python": engine.python,
        "steps": steps,
    }))
}

/// The shell between Observatory and the save: the value moves from the variable the engine set
/// into an unexported one, leaves the environment, and goes to the save command's stdin. `$0` is
/// the switchboard program, `$@` its arguments. `name` is a checked slot NAME (upper-case
/// letters, digits and `_`), so it cannot change the script.
fn pipe_script(name: &str) -> String {
    format!("k=\"${{{name}}}\"; unset {name}; printf '%s' \"$k\" | \"$0\" \"$@\"")
}

/// One save of a planned add: Observatory's `use_secret.py run --vault-only` gives the slot's value
/// to the shell, which pipes it into `cli --data-dir root --json <args>`. The answer is that
/// command's own JSON answer; its value never passes through this process.
pub fn save_step(
    engine: &Engine,
    slot: &Slot,
    cli: &Path,
    root: &Path,
    args: &[String],
) -> Result<Value, String> {
    if !cfg!(unix) {
        return Err(UNIX_ONLY.into());
    }
    let mut command = Command::new(&engine.python);
    command
        .arg(engine.dir.join("tools").join("use_secret.py"))
        .args([
            "run",
            "--vault-only",
            "--env",
            &slot.env,
            &slot.project,
            &slot.name,
        ])
        .arg("--")
        .args(["/bin/sh", "-c", &pipe_script(&slot.name)])
        .arg(cli)
        .arg("--data-dir")
        .arg(root)
        .arg("--json")
        .args(args)
        .env("OBSERVATORY_VAULT_ONLY", "1");
    let output = match bounded(command, SAVE_TIMEOUT) {
        Ok(output) => output,
        Err(true) => return Err(SAVE_TIMED_OUT.into()),
        Err(false) => return Err(SAVE_FAILED.into()),
    };
    let answer = output
        .stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .or_else(|| serde_json::from_str::<Value>(output.stdout.trim()).ok());
    if output.success {
        return answer
            .map(|a| a.get("data").cloned().unwrap_or(a))
            .ok_or_else(|| SAVE_FAILED.to_string());
    }
    // The save's own refusal (a key of the wrong kind, a name taken), when it gave one; never
    // text that could carry a value.
    let refusal = output
        .stderr
        .lines()
        .rev()
        .find_map(|l| {
            serde_json::from_str::<Value>(l.trim())
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
        })
        .filter(|e| !e.contains("sk-") && e.len() <= 400);
    Err(refusal.unwrap_or_else(|| SAVE_FAILED.into()))
}

/// The target a step's answer saved.
fn saved_target(step: &Value, answer: &Value) -> Option<Target> {
    match step["target"].as_str()? {
        "account" => Some(Target::Account {
            account_id: answer["id"].as_str()?.to_owned(),
        }),
        "anthropic" => Some(Target::Anthropic {
            name: step["name"].as_str()?.to_owned(),
        }),
        "openrouter" => Some(Target::Openrouter {
            pool: step["pool"].as_str().map(str::to_owned),
        }),
        _ => None,
    }
}

/// K2 from the CLI, the MCP server or the window: plan in the owner, run each save, then record
/// where the key came from. `cli` is the switchboard program the saves run (it reads stdin). A save
/// that fails stops the rest; what was saved before it is still recorded.
pub async fn add(root: &Path, cli: &Path, request: AddRequest) -> Result<Value, String> {
    let plan = crate::call(root, crate::Operation::ObservatoryKeyPlan { request }).await?;
    let engine = Engine {
        dir: plan["engine"]
            .as_str()
            .map(PathBuf::from)
            .ok_or(SAVE_FAILED)?,
        python: plan["python"]
            .as_str()
            .map(PathBuf::from)
            .ok_or(SAVE_FAILED)?,
    };
    let slot = Slot::parse(plan["slot"].as_str().unwrap_or_default())?;
    let steps = plan["steps"].as_array().cloned().unwrap_or_default();
    let mut targets = Vec::new();
    let mut saved = Vec::new();
    let mut failure = None;
    for step in &steps {
        let args: Vec<String> = step["args"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let (engine, slot, cli, root) = (
            engine.clone(),
            slot.clone(),
            cli.to_owned(),
            root.to_owned(),
        );
        let answer =
            tokio::task::spawn_blocking(move || save_step(&engine, &slot, &cli, &root, &args))
                .await
                .map_err(|_| SAVE_FAILED.to_string())?;
        match answer {
            Ok(answer) => {
                if let Some(target) = saved_target(step, &answer) {
                    targets.push(target);
                }
                saved.push(json!({"target": step["target"], "answer": answer}));
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    if !targets.is_empty() {
        crate::call(
            root,
            crate::Operation::ObservatoryKeyRecord {
                slot: slot.text(),
                targets: targets.clone(),
            },
        )
        .await?;
    }
    match failure {
        Some(error) if saved.is_empty() => Err(error),
        failure => Ok(json!({
            "slot": slot.text(),
            "kind": plan["kind"],
            "saved": saved,
            "failed": failure,
            "next": next_steps(&plan["kind"], &targets),
        })),
    }
}

/// What the person does next: the pay-per-token group starts off, and a launch names the key.
fn next_steps(kind: &Value, targets: &[Target]) -> Vec<String> {
    let mut next = Vec::new();
    if targets.iter().any(|t| matches!(t, Target::Account { .. })) {
        next.push("The key is in the pay-per-token group, which starts off: set its daily ceiling, turn it on and add the folders it serves (switchboard project edit). Then switchboard agents launch claude-code or hermes --pool <its id> --model <model>.".into());
    }
    if let Some(Target::Anthropic { name }) = targets
        .iter()
        .find(|t| matches!(t, Target::Anthropic { .. }))
    {
        next.push(format!("Hermes starts on it with switchboard agents launch hermes --anthropic {name}; no ceiling counts that launch."));
    }
    if kind == "openrouter" {
        next.push("Agents start on it with switchboard agents launch <agent> --openrouter, inside its ceiling.".into());
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const LIST: &str = "  0xDEV/prod/OPENROUTER_API_KEY  (73B, expires 2027-11-13)
  alpha-web/prod/ANTHROPIC_API_KEY  (108B, rotated 2026-11-14×1)
  alpha-web/prod/ANTHROPIC_ADMIN_KEY  (110B)
  alpha-web/stage/OPENAI_API_KEY  (164B)
  beta/prod/OPENAI_BYOK_KEY  (164B)
  beta/prod/OPENROUTER_MANAGEMENT_KEY  (73B)
  beta/prod/OPENROUTER_PROVISIONING_KEY  (73B)
  beta/prod/OPENROUTER_API_KEY_PREVIOUS  (73B)
  beta/local/OPENROUTER_KEY  (73B, header for https://openrouter.ai)
  beta/prod/OPENAI_BASE_URL  (30B)
  beta/prod/DATABASE_URL  (145B)
  beta/prod/OPENROUTER_FLAG_KEY  (1B, not a credential: it does not expire)
  nothing matches
";

    fn store(root: &Path) -> Store {
        Store::open(
            root.into(),
            Arc::new(switchboard_core::MemoryVault::default()),
        )
        .unwrap()
    }
    fn engine() -> Engine {
        Engine {
            dir: "/engine".into(),
            python: "/engine/python3".into(),
        }
    }

    #[test]
    fn only_an_agents_provider_keys_are_listed() {
        let keys = parse_list(LIST);
        let slots: Vec<String> = keys.iter().map(|k| k.slot.text()).collect();
        assert_eq!(
            slots,
            [
                "0xDEV/prod/OPENROUTER_API_KEY",
                "alpha-web/prod/ANTHROPIC_API_KEY",
                "alpha-web/stage/OPENAI_API_KEY",
                "beta/local/OPENROUTER_KEY",
            ]
        );
        assert_eq!(keys[0].kind, Kind::Openrouter);
        assert_eq!(keys[0].expires.as_deref(), Some("2027-11-13"));
        assert_eq!(keys[1].kind, Kind::Anthropic);
        assert_eq!(keys[1].rotated.as_deref(), Some("2026-11-14"));
        assert_eq!(keys[2].kind, Kind::Openai);
        assert_eq!(classify("ANTHROPIC_MGMT_KEY"), None);
        assert_eq!(classify("anthropic_api_key"), None);
    }

    #[test]
    fn slots_are_checked() {
        assert!(Slot::parse("alpha-web/prod/ANTHROPIC_API_KEY").is_ok());
        for bad in [
            "alpha-web/dev/ANTHROPIC_API_KEY",
            "alpha web/prod/X_KEY",
            "../prod/X_KEY",
            "a/prod/x_key",
            "a/prod/X_KEY/more",
            "a/prod",
            "a/prod/X;rm",
        ] {
            assert_eq!(Slot::parse(bad).unwrap_err(), SLOT_INVALID, "{bad}");
        }
        assert_eq!(
            pipe_script("ANTHROPIC_API_KEY"),
            "k=\"${ANTHROPIC_API_KEY}\"; unset ANTHROPIC_API_KEY; printf '%s' \"$k\" | \"$0\" \"$@\""
        );
    }

    #[test]
    fn a_missing_engine_is_an_answer_and_records_join_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let absent = list_from(dir.path(), &store, Err(NOT_INSTALLED.into())).unwrap();
        assert_eq!(absent["state"], "absent");
        assert_eq!(absent["keys"], json!([]));
        let broken = list_from(dir.path(), &store, Err(UNREADABLE.into())).unwrap();
        assert_eq!(broken["state"], "unreadable");
        // A named key recorded as added from a slot shows beside it; one removed since does not.
        crate::anthropic_keys::set(dir.path(), &store, "alpha", "sk-ant-api03-fixture", None, 1)
            .unwrap();
        record(
            dir.path(),
            &store,
            "alpha-web/prod/ANTHROPIC_API_KEY",
            &[Target::Anthropic {
                name: "alpha".into(),
            }],
            5,
        )
        .unwrap();
        let listed = list_from(dir.path(), &store, Ok(parse_list(LIST))).unwrap();
        let key = &listed["keys"][1];
        assert_eq!(key["slot"], "alpha-web/prod/ANTHROPIC_API_KEY");
        assert_eq!(key["added"][0]["kind"], "anthropic");
        assert_eq!(key["added"][0]["name"], "alpha");
        assert_eq!(listed["keys"][2]["can_add"], false);
        assert!(!listed.to_string().contains("sk-ant"));
        crate::anthropic_keys::remove(dir.path(), &store, "alpha").unwrap();
        let listed = list_from(dir.path(), &store, Ok(parse_list(LIST))).unwrap();
        assert_eq!(listed["keys"][1]["added"], json!([]));
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!text.contains("sk-ant"));
        std::fs::write(dir.path().join(FILE), b"{oops").unwrap();
        let listed = list_from(dir.path(), &store, Ok(parse_list(LIST))).unwrap();
        assert_eq!(listed["degraded"][0]["source"], FILE);
    }

    #[test]
    fn a_plan_names_the_saves_and_refuses_what_it_should() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store(root);
        let listed = parse_list(LIST);
        let plan = |r: AddRequest| plan_from(root, &store, &r, &engine(), &listed);
        let anthropic = |to: Option<&str>| AddRequest {
            slot: "alpha-web/prod/ANTHROPIC_API_KEY".into(),
            to: to.map(str::to_owned),
            ..Default::default()
        };
        // By default an Anthropic key becomes an account, labelled by its project and env.
        let steps = plan(anthropic(None)).unwrap()["steps"].clone();
        assert_eq!(steps.as_array().unwrap().len(), 1);
        assert_eq!(steps[0]["target"], "account");
        assert_eq!(steps[0]["args"][7], "alpha-web (prod)");
        assert_eq!(steps[0]["args"][10], "--secret-stdin");
        // Both: an account and a named key, named from the project.
        let both = plan(anthropic(Some("both"))).unwrap();
        assert_eq!(both["steps"][1]["target"], "anthropic");
        assert_eq!(both["steps"][1]["name"], "alpha-web");
        assert_eq!(
            plan(anthropic(Some("everywhere"))).unwrap_err(),
            TARGET_INVALID
        );
        let mut pooled = anthropic(None);
        pooled.pool = Some("team".into());
        assert_eq!(plan(pooled).unwrap_err(), POOL_NOT_FOR_ANTHROPIC);
        // A name already saved is refused unless replaced.
        crate::anthropic_keys::set(root, &store, "alpha-web", "sk-ant-api03-old", None, 1).unwrap();
        assert_eq!(plan(anthropic(Some("agent"))).unwrap_err(), NAME_TAKEN);
        let mut again = anthropic(Some("agent"));
        again.replace = true;
        assert!(plan(again).is_ok());
        // An OpenAI key is listed but not added; a slot Observatory does not hold is refused.
        let openai = AddRequest {
            slot: "alpha-web/stage/OPENAI_API_KEY".into(),
            ..Default::default()
        };
        assert_eq!(plan(openai).unwrap_err(), OPENAI_NOT_YET);
        let admin = AddRequest {
            slot: "alpha-web/prod/ANTHROPIC_ADMIN_KEY".into(),
            ..Default::default()
        };
        assert_eq!(plan(admin).unwrap_err(), NOT_LISTED);
        // An OpenRouter key goes to this computer's key, never over one already saved.
        let openrouter = AddRequest {
            slot: "0xDEV/prod/OPENROUTER_API_KEY".into(),
            model: Some("moonshotai/kimi-k2".into()),
            ..Default::default()
        };
        let planned = plan(openrouter.clone()).unwrap();
        assert_eq!(
            planned["steps"][0]["args"],
            json!([
                "agents",
                "openrouter",
                "set",
                "--key-stdin",
                "--model",
                "moonshotai/kimi-k2"
            ])
        );
        store
            .set_agent_key("openrouter", "sk-or-v1-fixture", None)
            .unwrap();
        assert_eq!(plan(openrouter.clone()).unwrap_err(), OPENROUTER_TAKEN);
        let mut to_group = openrouter;
        to_group.pool = Some("nowhere".into());
        assert_eq!(
            plan(to_group).unwrap_err(),
            switchboard_core::agent_keys::KEY_POOL_INVALID
        );
    }

    #[test]
    fn a_saved_target_comes_from_the_steps_answer() {
        assert_eq!(
            saved_target(&json!({"target": "account"}), &json!({"id": "a1"})),
            Some(Target::Account {
                account_id: "a1".into()
            })
        );
        assert_eq!(
            saved_target(&json!({"target": "openrouter", "pool": null}), &json!({})),
            Some(Target::Openrouter { pool: None })
        );
        assert_eq!(
            derived_name("Appvillis_Bot").as_deref(),
            Some("appvillis-bot")
        );
        assert_eq!(derived_name("0xDEV").as_deref(), Some("0xdev"));
        assert_eq!(derived_name("___"), None);
    }
}
