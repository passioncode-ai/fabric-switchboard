//! SB-561 (W11.2, spec S1; builds on SB-463): one continuation module. Every path that continues
//! a Claude Code session as itself (`claude --resume <id>`) builds its script here, from the
//! launch record it continues: a person's Continue in the window, MCP or CLI and the boot resume
//! (`resumable::continue_as`), the duty officer's restart (`duty`) and a restart wave
//! (`restarts`). The recovery claim (a host resumes the stop itself) and the automatic fallback (a
//! new session on another agent, built by `launch`) are gated here instead (`claim_refusal`,
//! `workflow_held`), so no path starts a session another one is continuing.
//!
//! What a resume carries, and from where (DEC-0034):
//! - the account: the session's home names it (`homes/<account>` isolated, `runtimes/claude-<pool>`
//!   managed). It is checked through the store before anything starts (`check`): an account no
//!   longer saved, disabled, used on another machine, given to an agent, turned off (DEC-0027, the
//!   hook `turned_off`) or a delegated seat whose lease ended refuses the resume with that reason,
//!   as does a group switched off. An isolated session stopped on its limit (a waiting stop)
//!   continues through its group's managed route (the proxy picks the group's next account);
//!   without one the resume is refused. An isolated home gets the account's current sign-in, which
//!   the background renewal keeps fresh, in place of the one its launch wrote.
//! - its permission mode: the live process's own flags when it is about to be restarted, else the
//!   launch record's `permissions` (written by every launch since this module), else, for a record
//!   from before it, the person's setting as before; a session no launch record names (the
//!   person's own) resumes with no permission flags, so its own settings decide. Never wider.
//! - its environment: the home's exports (its route, its labels), and the launch's own values
//!   (`OBSERVATORY_WORKFLOW_ID`, `OBSERVATORY_HANDOFF_ID`, `SWITCHBOARD_CONTEXT`) from its record,
//!   never another launch's that rewrote the home since.
//! - its home marker: a resume in a Switchboard home with no launch of its own writes the home's
//!   `.session-pid`, so a launch, a remove or a lease end never takes the home under it
//!   (`launch::ensure_idle` also reads the home's live session files).
#![cfg_attr(not(unix), allow(dead_code))]
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use switchboard_core::{Snapshot, Store};

/// The launch's own variables, kept in its record (`launch_env`) and given back to its resume.
/// Everything else a resume exports is the home's (its route and its labels).
pub const PER_LAUNCH_ENV: &[&str] = &[
    "OBSERVATORY_WORKFLOW_ID",
    "OBSERVATORY_HANDOFF_ID",
    "SWITCHBOARD_CONTEXT",
];
/// Lines of a home's `launch.command` a resume never copies: its own launch id, config folder and
/// PATH are written by the resume itself.
const OWN_LINES: &[&str] = &["SWITCHBOARD_LAUNCH_ID", "CLAUDE_CONFIG_DIR", "PATH"];

/// Permission modes a launch record keeps (`permissions`).
pub const BYPASS: &str = "bypass";
/// No permission flags: Claude Code's own settings decide.
pub const DEFAULT: &str = "default";
/// SB-405: prompts on, explicitly (`--permission-mode manual`).
pub const ASK: &str = "ask";

pub const ACCOUNT_GONE: &str =
    "The account that session ran on is no longer saved in Switchboard, so it is not continued on it.";
pub const ACCOUNT_DISABLED: &str = "The account that session ran on is disabled in Switchboard, so it is not continued on it. Enable the account, then continue the session.";
pub const ACCOUNT_ELSEWHERE: &str = "The account that session ran on is in use on another machine, so the session is not continued here.";
pub const ACCOUNT_ASSIGNED: &str = "The account that session ran on is given to an agent, so Switchboard does not continue the session on it. Take the account back, then continue the session.";
pub const ACCOUNT_OFF: &str = "The account that session ran on is turned off in Switchboard, so the session is not continued on it. Turn the account on, then continue the session.";
pub const SEAT_ENDED: &str = "The account that session ran on was delegated to this computer and its lease ran out, so the session is not continued on it.";
pub const GROUP_OFF: &str = "The group that session ran in is switched off, so the session is not continued. Turn the group on, then continue the session.";
pub const NO_ROUTE: &str = "The group that session ran in has no account selected, so the session is not continued. Select one of its accounts, then continue the session.";
pub const AT_LIMIT: &str = "The account that session ran on is at its limit, and its group has no managed route to continue on. Continue the session when the account has room again.";
pub const ACCOUNTS_UNREAD: &str = "Switchboard could not read its accounts, so the session's account was not checked and the session was not continued. Try again in a moment.";
pub const HOME_BUSY: &str = "Another session runs in that account's home now, so this one is not continued there. Close the other session first.";
pub const NOT_PREPARED: &str = "The session's home could not be prepared with the account's current sign-in, so the session was not continued.";

// ------------------------------------------------------------------------------------------
// The store, for the paths that run without one at hand (the one-minute session watch).

static STORES: Mutex<Vec<(PathBuf, Weak<Store>)>> = Mutex::new(Vec::new());

/// Registers the store of the data folder `root`, so a continuation started from the session
/// watch's own thread checks its account. A runtime registers its store when it opens; a
/// registration whose store was dropped is forgotten.
pub fn attach(root: &Path, store: &Arc<Store>) {
    let mut stores = STORES.lock().unwrap_or_else(|p| p.into_inner());
    stores.retain(|(r, s)| r != root && s.strong_count() > 0);
    stores.push((root.to_owned(), Arc::downgrade(store)));
}

/// The store registered for `root`, while it is open.
pub(crate) fn store_for(root: &Path) -> Option<Arc<Store>> {
    let stores = STORES.lock().unwrap_or_else(|p| p.into_inner());
    stores
        .iter()
        .rev()
        .find(|(r, _)| r == root)
        .and_then(|(_, s)| s.upgrade())
}

// ------------------------------------------------------------------------------------------
// The session's home and the launch record it continues.

/// Whose sign-in a config folder runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Home {
    /// `homes/<account>`: the account's own sign-in, written in its `settings.json`.
    Isolated(String),
    /// `runtimes/claude-<pool>`: the group's managed route through the proxy.
    Managed(String),
    /// Any other folder (the person's own Claude folder, an editor's): not Switchboard's.
    Other,
}

/// The kind of home `config` is, under the data folder `root`.
pub fn home_of(root: &Path, config: &Path) -> Home {
    let relative: Option<PathBuf> = config
        .strip_prefix(root)
        .ok()
        .map(Path::to_path_buf)
        .or_else(|| {
            let (root, config) = (root.canonicalize().ok()?, config.canonicalize().ok()?);
            config.strip_prefix(&root).ok().map(Path::to_path_buf)
        });
    let Some(relative) = relative else {
        return Home::Other;
    };
    let parts: Vec<String> = relative
        .components()
        .map(|c| match c {
            std::path::Component::Normal(part) => part.to_string_lossy().into_owned(),
            _ => String::new(),
        })
        .collect();
    match parts.as_slice() {
        [homes, id] if homes == "homes" && crate::launches::valid_id(id) => {
            Home::Isolated(id.clone())
        }
        [runtimes, name] if runtimes == "runtimes" => match name.strip_prefix("claude-") {
            Some(pool) if !pool.is_empty() => Home::Managed(pool.to_owned()),
            _ => Home::Other,
        },
        _ => Home::Other,
    }
}

/// The config folder a launch record ran in: its home (`home_path`, `home_dir`) under `root`, or
/// the folder a resume's record names (`config_dir`).
fn config_of(root: &Path, record: &Value) -> Option<PathBuf> {
    record["home_path"]
        .as_str()
        .or(record["home_dir"].as_str())
        .map(|h| root.join(h))
        .or_else(|| record["config_dir"].as_str().map(PathBuf::from))
}

fn same_folder(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

/// How far back the launch record of a session is looked for: as long as records are kept.
const RECORD_WINDOW: i64 = 90 * 86_400;

/// The launch record a resume of `session` in `config` continues: the newest record naming the
/// session whose config folder is `config`, else the newest naming the session; none for a session
/// Switchboard never launched (the person's own). A dispatched board run is never one.
pub fn record_of(root: &Path, session: &str, config: &Path, now: i64) -> Option<Value> {
    let mut best: Option<((bool, i64), Value)> = None;
    for r in crate::launches::recent_records(root, now - RECORD_WINDOW) {
        if r["provider_session_id"] != session || r.get("dispatch").is_some_and(|d| !d.is_null()) {
            continue;
        }
        let here = config_of(root, &r).is_some_and(|c| same_folder(&c, config));
        let at = r["launched_at"].as_i64().unwrap_or(0);
        if best.as_ref().is_none_or(|(key, _)| (here, at) > *key) {
            best = Some(((here, at), r));
        }
    }
    best.map(|(_, r)| r)
}

// ------------------------------------------------------------------------------------------
// Permissions.

fn valid_mode(mode: &str) -> bool {
    !mode.is_empty() && mode.len() <= 32 && mode.bytes().all(|b| b.is_ascii_alphabetic())
}

/// The permission mode an agent's arguments name: a mode named with `--permission-mode` (Claude
/// Code), `bypass` when the agent's full-access arguments are all there (SB-515), else `default`.
pub fn mode_of_args<'a>(agent: &str, args: impl IntoIterator<Item = &'a str>) -> String {
    let args: Vec<&str> = args.into_iter().collect();
    let mut named = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--permission-mode" {
            named = args.get(i + 1).copied();
            i += 2;
            continue;
        }
        if let Some(mode) = args[i].strip_prefix("--permission-mode=") {
            named = Some(mode);
        }
        i += 1;
    }
    if let Some(mode) = named.filter(|m| valid_mode(m)) {
        return if mode == "manual" {
            ASK.to_owned()
        } else {
            mode.to_owned()
        };
    }
    let bypass = crate::agent_catalog::full_access(agent).is_some_and(|access| {
        !access.args.is_empty() && access.args.iter().all(|a| args.contains(&a.as_str()))
    });
    if bypass { BYPASS } else { DEFAULT }.to_owned()
}

/// The permission mode of a live Claude Code process, from its command line as `ps` reads it.
pub fn mode_of_command(command: &str) -> String {
    mode_of_args("claude-code", command.split_whitespace())
}

/// The arguments a resume takes for `mode`.
pub fn args_of_mode(mode: &str) -> Vec<String> {
    match mode {
        BYPASS => crate::agent_catalog::full_access("claude-code")
            .map(|a| a.args.clone())
            .unwrap_or_default(),
        DEFAULT => Vec::new(),
        ASK => vec!["--permission-mode".into(), "manual".into()],
        named if valid_mode(named) => vec!["--permission-mode".into(), named.to_owned()],
        _ => Vec::new(),
    }
}

/// The mode a resume runs with: the live process's own (`live`), else the record's, else for a
/// record from before this module the person's setting (what launched it then), else `default`.
pub fn mode_for(root: &Path, record: Option<&Value>, live: Option<&str>) -> String {
    if let Some(mode) = live.filter(|m| valid_mode(m)) {
        return mode.to_owned();
    }
    match record {
        Some(r) => match r["permissions"].as_str().filter(|m| valid_mode(m)) {
            Some(mode) => mode.to_owned(),
            None => mode_of_args(
                "claude-code",
                crate::session_accounts::resume_flags(root).into_iter(),
            ),
        },
        None => DEFAULT.to_owned(),
    }
}

// ------------------------------------------------------------------------------------------
// The environment.

/// What a launch record keeps of the launch's environment: its own values only (never a token).
pub fn launch_env(env: &std::collections::BTreeMap<String, String>) -> Value {
    let mut kept = serde_json::Map::new();
    for name in PER_LAUNCH_ENV {
        if let Some(value) = env.get(*name) {
            kept.insert((*name).to_owned(), value.clone().into());
        }
    }
    Value::Object(kept)
}

/// A home's `export` lines, each with its variable's name.
fn home_exports(config: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(config.join("launch.command"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let name = line.strip_prefix("export ")?.split('=').next()?;
            Some((name.to_owned(), line.to_owned()))
        })
        .collect()
}

/// The export lines of a resume: the home's (minus what the resume writes itself and what
/// belongs to one launch), then the launch's own values from `record` (`env`). A record from
/// before this module keeps the home's context, and its workflow only when the home's is the
/// one the record continued; a session no record names keeps the home's context only.
fn exports(config: &Path, record: Option<&Value>) -> Vec<String> {
    let quote = fabric_sessions_core::cli::quoted;
    let home = home_exports(config);
    let mut out: Vec<String> = home
        .iter()
        .filter(|(n, _)| !OWN_LINES.contains(&n.as_str()) && !PER_LAUNCH_ENV.contains(&n.as_str()))
        .map(|(_, l)| l.clone())
        .collect();
    match record {
        Some(r) if r["env"].is_object() => {
            for name in PER_LAUNCH_ENV {
                if let Some(value) = r["env"][*name].as_str() {
                    out.push(format!("export {name}={}", quote(value)));
                }
            }
        }
        _ => {
            let workflow = record
                .and_then(|r| r["continues"].as_str())
                .map(|w| format!("export OBSERVATORY_WORKFLOW_ID={}", quote(w)));
            let same_workflow = workflow.as_ref().is_some_and(|w| {
                home.iter()
                    .any(|(n, l)| n == "OBSERVATORY_WORKFLOW_ID" && l == w)
            });
            for (name, line) in &home {
                let keep = match name.as_str() {
                    "SWITCHBOARD_CONTEXT" => true,
                    "OBSERVATORY_WORKFLOW_ID" | "OBSERVATORY_HANDOFF_ID" => same_workflow,
                    _ => false,
                };
                if keep {
                    out.push(line.clone());
                }
            }
        }
    }
    out
}

/// The word `fabric_sessions_core::cli::quoted` wrote: `'...'` with `'\\''` for a quote.
fn unquoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    let word = inner.replace("'\\''", "'");
    (fabric_sessions_core::cli::quoted(&word) == value).then_some(word)
}

/// The per-launch values `lines` export, for the record of a resume that is a launch of its own.
fn env_of(lines: &[String]) -> Value {
    let mut kept = serde_json::Map::new();
    for line in lines {
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once('=') else {
            continue;
        };
        if PER_LAUNCH_ENV.contains(&name) {
            if let Some(word) = unquoted(value) {
                kept.insert(name.to_owned(), word.into());
            }
        }
    }
    Value::Object(kept)
}

// ------------------------------------------------------------------------------------------
// The script.

/// What a resume script runs.
pub struct Script<'a> {
    pub config: &'a Path,
    pub cwd: &'a str,
    pub session: &'a str,
    pub program: &'a Path,
    pub permissions: &'a [String],
    pub exports: &'a [String],
    /// The route the session's requests go through, when not its own home's (`--settings=`).
    pub route: Option<String>,
    pub prompt: Option<&'a str>,
    /// The launch this resume reports to, exported after the line that clears it.
    pub launch_id: Option<&'a str>,
    /// Write the home's `.session-pid` before the session starts.
    pub marker: bool,
}

/// The resume script: everything a launch must not inherit cleared, the launch it reports to,
/// the home's and the launch's exports, its config folder, PATH, then `claude <permissions>
/// [--settings <route>] --resume <id> [<prompt>]` in its folder. Ended by a signal it exits 0, so
/// a Terminal window closes behind it.
pub fn script(s: &Script) -> String {
    let quote = fabric_sessions_core::cli::quoted;
    let mut out = format!(
        "#!{}\nset +x\nunset {}\n",
        crate::launch::SCRIPT_SHELL,
        crate::launch::CONFLICTS.join(" ")
    );
    if let Some(id) = s.launch_id {
        out.push_str(&format!("export SWITCHBOARD_LAUNCH_ID={}\n", quote(id)));
    }
    let person = crate::person::claude_dir();
    let own_config = person.as_deref() != Some(s.config);
    // SB-452: a managed home's own route, which outranks the folder's local settings.
    let route = s.route.clone().or_else(|| {
        (own_config && s.config.join(crate::launch::ROUTE_FILE).is_file())
            .then(|| crate::launch::route_arg(s.config))
    });
    if own_config {
        for line in s.exports {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!(
            "export CLAUDE_CONFIG_DIR={}\n",
            quote(&s.config.to_string_lossy())
        ));
    }
    // A Terminal window runs the script with the PATH it was started with; the program's folder
    // goes first and the person's login PATH fills what that PATH lacks (SB-455), so the session's
    // hooks find `node` (operator 2026-11-15: every hook failed with "node: command not found").
    out.push_str(&crate::launch::path_export(s.program));
    if s.marker {
        out.push_str(&format!(
            "(umask 077; printf '%s' \"$$\" > {})\n",
            quote(&s.config.join(".session-pid").to_string_lossy())
        ));
    }
    out.push_str(&format!(
        "cd {} || exit 1\n{}",
        quote(s.cwd),
        quote(&s.program.to_string_lossy())
    ));
    for arg in s
        .permissions
        .iter()
        .map(String::as_str)
        .chain(route.as_deref())
    {
        out.push(' ');
        out.push_str(&quote(arg));
    }
    out.push_str(&format!(" --resume {}", quote(s.session)));
    if let Some(prompt) = s.prompt {
        out.push(' ');
        out.push_str(&quote(prompt));
    }
    out.push_str("\nstatus=$?\n[ \"$status\" -ge 128 ] && exit 0\nexit \"$status\"\n");
    out
}

// ------------------------------------------------------------------------------------------
// The account check.

/// DEC-0027 (SB-418, PR #821): an account the person turned off. Until that lands, no account is
/// turned off; it adds its `off` mark here, the one place every resume reads.
fn turned_off(snapshot: &Snapshot, id: &str) -> bool {
    let _ = (snapshot, id);
    false
}

/// Why a session may not be continued on account `id`, as the store says now. Pure, for tests.
pub fn account_refusal(snapshot: &Snapshot, id: &str) -> Option<&'static str> {
    let Some(account) = snapshot.accounts.iter().find(|a| a.id == id) else {
        return Some(ACCOUNT_GONE);
    };
    if snapshot.elsewhere.contains_key(id) {
        return Some(ACCOUNT_ELSEWHERE);
    }
    if turned_off(snapshot, id) {
        return Some(ACCOUNT_OFF);
    }
    if snapshot
        .delegated
        .get(id)
        .is_some_and(|d| d.ended_at.is_some())
    {
        return Some(SEAT_ENDED);
    }
    if !account.enabled {
        return Some(ACCOUNT_DISABLED);
    }
    // SB-322: as at a launch, an account given to an agent serves only once the person used it
    // anyway, which made it its group's selection.
    if snapshot.is_assigned(id)
        && snapshot
            .routes
            .get(&format!("{}:{}", account.provider.as_str(), account.pool))
            .map(String::as_str)
            != Some(id)
    {
        return Some(ACCOUNT_ASSIGNED);
    }
    group_refusal(snapshot, &account.pool)
}

fn group_refusal(snapshot: &Snapshot, pool: &str) -> Option<&'static str> {
    snapshot
        .project_of_pool(pool)
        .is_some_and(|g| g.off)
        .then_some(GROUP_OFF)
}

/// What the check decided for one resume.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    /// The route the session continues through when not its home's own (a limit).
    pub route: Option<String>,
    /// The session had a waiting limit stop; it is settled once the resume starts.
    pub stop: bool,
}

/// Rewrites `home`'s `settings.json` so its session signs in with `token` (an isolated Claude
/// home's own credential), or, with `None`, with no credential of its own (it goes through a
/// route). The rest of the file is kept; a missing or unreadable one is built as a launch builds it.
fn set_home_token(home: &Path, name: &str, token: Option<&str>) -> Result<(), String> {
    let path = home.join("settings.json");
    let mut settings = switchboard_core::private_fs::read_private(&path, 256 * 1024)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| {
            crate::carry::claude_settings(crate::carry::claude_user_dir().as_deref(), None)
        });
    if !settings["env"].is_object() {
        settings["env"] = json!({});
    }
    let env = settings["env"].as_object_mut().ok_or(NOT_PREPARED)?;
    match token {
        Some(token) => {
            env.insert(name.to_owned(), token.into());
        }
        None => {
            env.remove(name);
        }
    }
    crate::launch::private_write(
        &path,
        serde_json::to_string(&settings)
            .map_err(|_| NOT_PREPARED)?
            .as_bytes(),
        false,
    )
    .map_err(|_| NOT_PREPARED.to_string())
}

/// Checks the account a resume of `session` in `config` would run on, and prepares its home.
/// `restarting`: the session's own process still holds the home and is stopped first (the duty
/// officer, a restart wave), so the home is not refused as busy.
pub fn check(
    root: &Path,
    config: &Path,
    session: &str,
    restarting: bool,
    now: i64,
) -> Result<Checked, String> {
    let home = home_of(root, config);
    if home == Home::Other {
        return Ok(Checked::default());
    }
    let store = store_for(root).ok_or(ACCOUNTS_UNREAD)?;
    let snapshot = store.snapshot().map_err(|_| ACCOUNTS_UNREAD)?;
    let stop = crate::recovery::live_stop_sessions(root, now).contains(session);
    // One session per Switchboard home, as at a launch (`launch::Reservation`): a resume never
    // starts beside another session there. The duty officer's and a wave's restart stop the
    // session's own process first.
    if !restarting && crate::launch::ensure_idle(config).is_err() {
        return Err(HOME_BUSY.into());
    }
    match home {
        Home::Managed(pool) => {
            if let Some(why) = group_refusal(&snapshot, &pool) {
                return Err(why.into());
            }
            // The proxy serves the group's selected account and moves a refused request to its
            // next one (DEC-0002), so a limit is the route's to handle.
            if store
                .selected_account(switchboard_core::Provider::Claude, &pool)
                .is_none()
            {
                return Err(NO_ROUTE.into());
            }
            Ok(Checked { route: None, stop })
        }
        Home::Isolated(id) => {
            if let Some(why) = account_refusal(&snapshot, &id) {
                return Err(why.into());
            }
            let account = snapshot
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or(ACCOUNT_GONE)?;
            let name = if account.kind == switchboard_core::AuthKind::ApiKey {
                "ANTHROPIC_API_KEY"
            } else {
                "CLAUDE_CODE_OAUTH_TOKEN"
            };
            if stop {
                // M1-45: the account is at its limit; the group's managed route takes the
                // session to its next account, or the session waits for room.
                let managed = root
                    .join("runtimes")
                    .join(format!("claude-{}", account.pool));
                if !managed.join(crate::launch::ROUTE_FILE).is_file()
                    || store
                        .selected_account(switchboard_core::Provider::Claude, &account.pool)
                        .is_none()
                {
                    return Err(AT_LIMIT.into());
                }
                set_home_token(config, name, None)?;
                return Ok(Checked {
                    route: Some(crate::launch::route_arg(&managed)),
                    stop,
                });
            }
            let credential = store.credential(&id)?;
            set_home_token(config, name, Some(&credential.access_token))?;
            Ok(Checked { route: None, stop })
        }
        Home::Other => Ok(Checked::default()),
    }
}

// ------------------------------------------------------------------------------------------
// The one builder.

/// The session a resume continues.
pub struct Target<'a> {
    pub session: &'a str,
    pub config: &'a Path,
    pub cwd: &'a str,
    /// The permission mode its live process runs with, read before that process is stopped.
    pub live: Option<&'a str>,
    /// Its own process still holds its home and is stopped before the resume starts.
    pub restarting: bool,
}

/// A resume ready to start.
#[derive(Clone, Debug)]
pub struct Resume {
    pub script: String,
    /// The permission mode it runs with, for the record of a resume that is a launch of its own.
    pub permissions: String,
    /// The launch's own values it exports, for that record.
    pub env: Value,
    /// It continues past a waiting limit stop, settled once it starts (`started`).
    pub stop: bool,
}

/// Builds the resume of `t` with `program`, going on with `prompt`, reporting to `launch_id` when
/// it is a launch of its own: the one place every path continues a session from (S1.1).
pub fn build(
    root: &Path,
    t: &Target,
    program: &Path,
    prompt: Option<&str>,
    launch_id: Option<&str>,
    now: i64,
) -> Result<Resume, String> {
    let record = record_of(root, t.session, t.config, now);
    let checked = check(root, t.config, t.session, t.restarting, now)?;
    let permissions = mode_for(root, record.as_ref(), t.live);
    let lines = exports(t.config, record.as_ref());
    let marker = launch_id.is_none() && home_of(root, t.config) != Home::Other;
    let args = args_of_mode(&permissions);
    let text = script(&Script {
        config: t.config,
        cwd: t.cwd,
        session: t.session,
        program,
        permissions: &args,
        exports: &lines,
        route: checked.route,
        prompt,
        launch_id,
        marker,
    });
    Ok(Resume {
        script: text,
        permissions,
        env: env_of(&lines),
        stop: checked.stop,
    })
}

/// After a resume started: the waiting limit stop it continued past is settled, so a host never
/// resumes the session a second time.
pub fn started(root: &Path, resume: &Resume, session: &str, now: i64) {
    if resume.stop {
        let _ = crate::recovery::continued(root, session, now);
    }
}

// ------------------------------------------------------------------------------------------
// The paths that continue no session themselves.

/// M1-21: why a host may not claim the stop of `session` (stopped at `stopped_at`): it is being
/// continued now, or a launch continued it after the stop.
pub fn claim_refusal(
    root: &Path,
    session: &str,
    stopped_at: i64,
    now: i64,
) -> Option<&'static str> {
    if crate::resumable::being_continued(root, session, now) {
        return Some(crate::recovery::BEING_CONTINUED);
    }
    let sessions: std::collections::HashSet<&str> = [session].into();
    let launched = crate::launches::by_session(root, &sessions, stopped_at - 86_400);
    launched
        .get(session)
        .is_some_and(|(launch, _)| crate::launches::continuation(root, launch, stopped_at).0)
        .then_some(crate::recovery::CONTINUED)
}

/// M1-22: why the automatic fallback leaves `workflow` alone now: one of its sessions is being
/// continued, or a host claimed its stop and resumes it. A waiting stop does not hold it; the
/// hand-over settles it (`handed_over`).
pub fn workflow_held(root: &Path, workflow: &str, now: i64) -> Option<&'static str> {
    let sessions = workflow_sessions(root, workflow, now);
    if sessions.is_empty() {
        return None;
    }
    if sessions
        .iter()
        .any(|s| crate::resumable::being_continued(root, s, now))
    {
        return Some("being_continued");
    }
    let claimed = crate::recovery::read(root).ok().is_some_and(|rows| {
        rows.iter()
            .any(|r| r.state == crate::recovery::State::Claimed && sessions.contains(&r.session))
    });
    claimed.then_some("stop_claimed")
}

/// After the automatic fallback handed `workflow` on: the waiting stops of its sessions are
/// settled, so a host never resumes beside the hand-over.
pub fn handed_over(root: &Path, workflow: &str, now: i64) {
    for session in workflow_sessions(root, workflow, now) {
        let _ = crate::recovery::continued(root, &session, now);
    }
}

/// The sessions the launches continuing `workflow` ran in the last day.
fn workflow_sessions(root: &Path, workflow: &str, now: i64) -> std::collections::HashSet<String> {
    crate::launches::recent_records(root, now - 86_400)
        .iter()
        .filter(|r| r["continues"] == workflow)
        .filter_map(|r| r["provider_session_id"].as_str().map(str::to_owned))
        .collect()
}

/// M1-23: a limit stop kept while no owner ran is stale once its session went on: a launch that
/// names it took a turn after it, or started after it.
pub fn spool_stale(root: &Path, session: &str, stopped_at: i64, now: i64) -> bool {
    crate::launches::recent_records(root, stopped_at.min(now) - 60)
        .iter()
        .filter(|r| r["provider_session_id"] == session)
        .any(|r| {
            r["last_turn_at"].as_i64().is_some_and(|t| t > stopped_at)
                || r["launched_at"].as_i64().is_some_and(|t| t > stopped_at)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{AuthKind, Credential, MemoryVault, Provider};

    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        store: Arc<Store>,
    }
    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        crate::launch::private_dir(&root).unwrap();
        let store = Arc::new(Store::open(root.clone(), Arc::new(MemoryVault::default())).unwrap());
        attach(&root, &store);
        Fixture {
            _temp: temp,
            root,
            store,
        }
    }
    impl Fixture {
        fn add(&self, pool: &str, token: &str) -> String {
            let material = format!(
                "{{\"claudeAiOauth\":{{\"accessToken\":\"{token}\",\"refreshToken\":\"r-{token}\"}}}}"
            );
            let credential = Credential::parse(Provider::Claude, AuthKind::OAuth, &material)
                .unwrap_or_else(|e| panic!("synthetic credential: {e}"));
            self.store
                .add(
                    "Synthetic".into(),
                    Provider::Claude,
                    AuthKind::OAuth,
                    pool.into(),
                    credential,
                )
                .unwrap()
                .id
        }
        fn home(&self, id: &str) -> PathBuf {
            let home = self.root.join("homes").join(id);
            crate::launch::private_dir(&self.root.join("homes")).unwrap();
            crate::launch::private_dir(&home).unwrap();
            home
        }
    }

    fn launched(root: &Path, session: &str, home: &str, extra: Value) -> String {
        let id = crate::launches::new_id();
        let mut record = json!({"provider": "claude", "provider_session_id": session,
            "working_directory": "/w/a", "launched_at": 100, "home_dir": home});
        if let (Some(r), Some(e)) = (record.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                r.insert(k.clone(), v.clone());
            }
        }
        crate::launches::record_resume(root, &id, record, &crate::launches::LaunchedBy::default())
            .unwrap();
        id
    }

    #[test]
    fn a_home_is_read_from_its_folder() {
        let root = Path::new("/d");
        assert_eq!(
            home_of(
                root,
                Path::new("/d/homes/0b5f3c1e-aaaa-4bbb-8ccc-dddddddddddd")
            ),
            Home::Isolated("0b5f3c1e-aaaa-4bbb-8ccc-dddddddddddd".into())
        );
        assert_eq!(
            home_of(root, Path::new("/d/runtimes/claude-team_a")),
            Home::Managed("team_a".into())
        );
        assert_eq!(home_of(root, Path::new("/d/runtimes/codex-x")), Home::Other);
        assert_eq!(home_of(root, Path::new("/h/.claude")), Home::Other);
        assert_eq!(home_of(root, Path::new("/d/homes/../x")), Home::Other);
    }

    #[test]
    fn the_permission_mode_is_read_from_the_arguments_and_given_back() {
        assert_eq!(
            mode_of_args("claude-code", ["--dangerously-skip-permissions", "x"]),
            BYPASS
        );
        assert_eq!(
            mode_of_args("claude-code", ["--permission-mode", "plan"]),
            "plan"
        );
        assert_eq!(
            mode_of_args("claude-code", ["--permission-mode=acceptEdits"]),
            "acceptEdits"
        );
        assert_eq!(
            mode_of_args("claude-code", ["--permission-mode", "manual"]),
            ASK
        );
        assert_eq!(mode_of_args("claude-code", ["--resume", "s"]), DEFAULT);
        assert_eq!(
            mode_of_command("/usr/local/bin/node /x/cli.js --permission-mode plan --resume s"),
            "plan"
        );
        assert_eq!(args_of_mode(BYPASS), ["--dangerously-skip-permissions"]);
        assert!(args_of_mode(DEFAULT).is_empty());
        assert_eq!(args_of_mode(ASK), ["--permission-mode", "manual"]);
        assert_eq!(args_of_mode("plan"), ["--permission-mode", "plan"]);
        assert!(args_of_mode("rm -rf").is_empty(), "never a word of its own");
    }

    #[test]
    fn a_resume_never_widens_the_permissions_its_launch_ran_with() {
        // M1-06: before, every resume took the person's global setting (bypass by default).
        let root = tempfile::tempdir().unwrap();
        let ask = json!({"permissions": ASK});
        assert_eq!(mode_for(root.path(), Some(&ask), None), ASK);
        let plan = json!({"permissions": "plan"});
        assert_eq!(mode_for(root.path(), Some(&plan), None), "plan");
        // The live process's own flags win: they are what it runs with now.
        assert_eq!(mode_for(root.path(), Some(&ask), Some("plan")), "plan");
        // A session Switchboard never launched resumes with its own settings.
        assert_eq!(mode_for(root.path(), None, None), DEFAULT);
        // A record from before this module: the person's setting, as it launched then.
        assert_eq!(mode_for(root.path(), Some(&json!({})), None), BYPASS);
        crate::agent_settings::set_permissions(root.path(), crate::agent_settings::ASK).unwrap();
        assert_eq!(mode_for(root.path(), Some(&json!({})), None), DEFAULT);
    }

    #[test]
    fn a_launch_keeps_only_its_own_values_never_a_token() {
        let env = std::collections::BTreeMap::from([
            ("OBSERVATORY_WORKFLOW_ID".to_owned(), "wf_1".to_owned()),
            ("SWITCHBOARD_CONTEXT".to_owned(), "personal".to_owned()),
            ("ANTHROPIC_AUTH_TOKEN".to_owned(), "secret".to_owned()),
            ("CLAUDE_CONFIG_DIR".to_owned(), "/d/homes/x".to_owned()),
        ]);
        assert_eq!(
            launch_env(&env),
            json!({"OBSERVATORY_WORKFLOW_ID": "wf_1", "SWITCHBOARD_CONTEXT": "personal"})
        );
    }

    #[test]
    fn a_resume_exports_its_own_launchs_values_never_another_launchs() {
        // M1-07: the home's launch.command is the newest launch's; the record keeps its own.
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("launch.command"),
            "#!/bin/zsh\nexport ANTHROPIC_BASE_URL='http://127.0.0.1:9/x'\n\
             export OBSERVATORY_WORKFLOW_ID='wf_other'\nexport OBSERVATORY_HANDOFF_ID='h_other'\n\
             export SWITCHBOARD_CONTEXT='org:a'\nexport SWITCHBOARD_LAUNCH_ID='L9'\n\
             export CLAUDE_CONFIG_DIR='/x'\nexec claude\n",
        )
        .unwrap();
        let own = json!({"env": {"OBSERVATORY_WORKFLOW_ID": "wf_mine", "SWITCHBOARD_CONTEXT": "personal"}});
        let lines = exports(home.path(), Some(&own));
        assert!(lines.contains(&"export ANTHROPIC_BASE_URL='http://127.0.0.1:9/x'".to_owned()));
        assert!(lines.contains(&"export OBSERVATORY_WORKFLOW_ID='wf_mine'".to_owned()));
        assert!(lines.contains(&"export SWITCHBOARD_CONTEXT='personal'".to_owned()));
        assert!(!lines
            .iter()
            .any(|l| l.contains("wf_other") || l.contains("h_other")));
        assert!(!lines
            .iter()
            .any(|l| l.contains("SWITCHBOARD_LAUNCH_ID") || l.contains("CLAUDE_CONFIG_DIR")));
        assert_eq!(
            env_of(&lines),
            json!({"OBSERVATORY_WORKFLOW_ID": "wf_mine", "SWITCHBOARD_CONTEXT": "personal"})
        );
        // A record from before this module keeps the home's workflow only when it is its own.
        let older = json!({"continues": "wf_other"});
        assert!(exports(home.path(), Some(&older))
            .contains(&"export OBSERVATORY_HANDOFF_ID='h_other'".to_owned()));
        let elsewhere = json!({"continues": "wf_mine"});
        assert!(!exports(home.path(), Some(&elsewhere))
            .iter()
            .any(|l| l.contains("OBSERVATORY_")));
        // No record: the home's context only.
        let none = exports(home.path(), None);
        assert!(none.contains(&"export SWITCHBOARD_CONTEXT='org:a'".to_owned()));
        assert!(!none.iter().any(|l| l.contains("OBSERVATORY_")));
    }

    #[test]
    fn the_record_continued_is_the_newest_that_ran_in_the_sessions_folder() {
        let root = tempfile::tempdir().unwrap();
        let session = "55555555-5555-4555-8555-555555555555";
        let here = launched(root.path(), session, "homes/a", json!({"launched_at": 100}));
        let _there = launched(root.path(), session, "homes/b", json!({"launched_at": 200}));
        let r = record_of(root.path(), session, &root.path().join("homes/a"), 300).unwrap();
        assert_eq!(r["launch_id"], json!(here));
        let r = record_of(root.path(), session, Path::new("/elsewhere"), 300).unwrap();
        assert_eq!(r["launched_at"], json!(200), "else the newest");
        assert!(record_of(root.path(), "S-unknown", Path::new("/x"), 300).is_none());
    }

    #[test]
    fn an_account_that_may_not_serve_refuses_its_resume_with_why() {
        let f = fixture();
        let id = f.add("default", "tok-a");
        let snapshot = f.store.snapshot().unwrap();
        assert_eq!(account_refusal(&snapshot, &id), None);
        assert_eq!(account_refusal(&snapshot, "gone"), Some(ACCOUNT_GONE));
        f.store.update(&id, "Synthetic".into(), false).unwrap();
        assert_eq!(
            account_refusal(&f.store.snapshot().unwrap(), &id),
            Some(ACCOUNT_DISABLED)
        );
        f.store.update(&id, "Synthetic".into(), true).unwrap();
        f.store.mark_elsewhere(&id, true, 10).unwrap();
        assert_eq!(
            account_refusal(&f.store.snapshot().unwrap(), &id),
            Some(ACCOUNT_ELSEWHERE)
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_isolated_resume_gets_the_accounts_current_sign_in_and_its_home_marker() {
        // M1-05, M1-04: before, the home's old token was reused and no marker was written.
        let f = fixture();
        let id = f.add("default", "tok-now");
        let home = f.home(&id);
        std::fs::write(
            home.join("settings.json"),
            r#"{"theme":"dark","env":{"CLAUDE_CODE_OAUTH_TOKEN":"tok-old"}}"#,
        )
        .unwrap();
        let session = "66666666-6666-4666-8666-666666666666";
        launched(
            &f.root,
            session,
            &format!("homes/{id}"),
            json!({"permissions": "plan", "env": {"SWITCHBOARD_CONTEXT": "personal"}}),
        );
        let t = Target {
            session,
            config: &home,
            cwd: "/w/a",
            live: None,
            restarting: false,
        };
        let r = build(&f.root, &t, Path::new("/bin/claude"), None, None, 300).unwrap();
        let settings: Value =
            serde_json::from_slice(&std::fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert_eq!(settings["env"]["CLAUDE_CODE_OAUTH_TOKEN"], json!("tok-now"));
        assert_eq!(settings["theme"], json!("dark"), "the rest is kept");
        assert!(!r.script.contains("tok-"), "no token in the script");
        assert!(r.script.contains("'--permission-mode' 'plan' --resume"));
        assert!(r.script.contains(".session-pid"), "{}", r.script);
        assert_eq!(r.permissions, "plan");
        // A resume that is a launch of its own (hosted) writes no marker: its host holds it.
        let hosted = build(&f.root, &t, Path::new("/bin/claude"), None, Some("L1"), 300).unwrap();
        assert!(!hosted.script.contains(".session-pid"));
        assert!(hosted.script.contains("export SWITCHBOARD_LAUNCH_ID='L1'"));
        // Disabled: refused, and the home is left as it was.
        f.store.update(&id, "Synthetic".into(), false).unwrap();
        assert_eq!(
            build(&f.root, &t, Path::new("/bin/claude"), None, None, 300).unwrap_err(),
            ACCOUNT_DISABLED
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_switchboard_home_is_never_resumed_without_its_account_checked() {
        let f = fixture();
        let other = tempfile::tempdir().unwrap();
        let root = other.path().join("data");
        std::fs::create_dir_all(root.join("homes/x")).unwrap();
        // No store registered for this data folder.
        assert_eq!(
            check(&root, &root.join("homes/x"), "S1", false, 1).unwrap_err(),
            ACCOUNTS_UNREAD
        );
        // A folder that is not Switchboard's is not its to check.
        assert_eq!(
            check(&f.root, Path::new("/h/.claude"), "S1", false, 1),
            Ok(Checked::default())
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_isolated_session_at_its_limit_continues_on_its_groups_route_or_waits() {
        // M1-45: before, Continue resumed on the exhausted account and left the stop waiting.
        let f = fixture();
        let id = f.add("default", "tok-a");
        let home = f.home(&id);
        std::fs::write(
            home.join("settings.json"),
            r#"{"env":{"CLAUDE_CODE_OAUTH_TOKEN":"tok-old"}}"#,
        )
        .unwrap();
        let session = "77777777-7777-4777-8777-777777777777";
        let snapshot = f.store.snapshot().unwrap();
        crate::recovery::register_stop(
            &f.root,
            &snapshot,
            session,
            &id,
            &std::env::temp_dir(),
            crate::recovery::StopKind::Limit,
            1000,
        )
        .unwrap();
        assert_eq!(
            check(&f.root, &home, session, false, 1000).unwrap_err(),
            AT_LIMIT,
            "no managed route yet"
        );
        let managed = f.root.join("runtimes/claude-default");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::write(managed.join(crate::launch::ROUTE_FILE), "{}").unwrap();
        f.store
            .select(Provider::Claude, "default", &id)
            .unwrap_or_else(|e| panic!("select: {e}"));
        let checked = check(&f.root, &home, session, false, 1000).unwrap();
        assert_eq!(checked.route, Some(crate::launch::route_arg(&managed)));
        assert!(checked.stop);
        let settings: Value =
            serde_json::from_slice(&std::fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert!(settings["env"].get("CLAUDE_CODE_OAUTH_TOKEN").is_none());
        let r = Resume {
            script: String::new(),
            permissions: DEFAULT.into(),
            env: json!({}),
            stop: true,
        };
        started(&f.root, &r, session, 1001);
        assert!(!crate::recovery::live_stop_sessions(&f.root, 1001).contains(session));
    }

    #[cfg(unix)]
    #[test]
    fn a_resume_never_starts_beside_another_session_in_its_home() {
        // SB-561 (S1.5): one session per Switchboard home, as at a launch.
        let f = fixture();
        let id = f.add("default", "tok-a");
        let home = f.home(&id);
        // This test process stands for a session running there (its marker names it).
        std::fs::write(home.join(".session-pid"), std::process::id().to_string()).unwrap();
        assert_eq!(
            check(&f.root, &home, "S1", false, 1).unwrap_err(),
            HOME_BUSY
        );
        assert!(
            check(&f.root, &home, "S1", true, 1).is_ok(),
            "a restart stops the home's own session first"
        );
        std::fs::remove_file(home.join(".session-pid")).unwrap();
        // A live Claude Code session file in the home holds it too (a hosted resume's).
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::write(
            home.join("sessions")
                .join(format!("{}.json", std::process::id())),
            "{}",
        )
        .unwrap();
        assert_eq!(
            check(&f.root, &home, "S1", false, 1).unwrap_err(),
            HOME_BUSY
        );
    }

    #[test]
    fn a_stop_kept_while_no_owner_ran_is_stale_once_its_session_went_on() {
        let root = tempfile::tempdir().unwrap();
        let session = "88888888-8888-4888-8888-888888888888";
        assert!(!spool_stale(root.path(), session, 500, 600));
        let id = launched(root.path(), session, "homes/a", json!({"launched_at": 100}));
        assert!(!spool_stale(root.path(), session, 500, 600));
        crate::launches::record_resume(
            root.path(),
            &id,
            json!({"provider": "claude", "provider_session_id": session, "launched_at": 100,
                "last_turn_at": 550}),
            &crate::launches::LaunchedBy::default(),
        )
        .unwrap();
        assert!(spool_stale(root.path(), session, 500, 600));
    }

    #[test]
    fn a_workflow_being_continued_is_left_to_that_continuation() {
        let root = tempfile::tempdir().unwrap();
        let session = "99999999-9999-4999-8999-999999999999";
        assert_eq!(workflow_held(root.path(), "wf_a", 1000), None);
        launched(
            root.path(),
            session,
            "homes/a",
            json!({"continues": "wf_a", "launched_at": 800}),
        );
        assert_eq!(workflow_held(root.path(), "wf_a", 1000), None);
        crate::resumable::mark_resuming(root.path(), session, 990);
        assert_eq!(
            workflow_held(root.path(), "wf_a", 1000),
            Some("being_continued")
        );
        assert_eq!(workflow_held(root.path(), "wf_b", 1000), None);
    }
}
