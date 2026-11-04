//! Anonymous product analytics (operator request 2026-10-05; docs/ANALYTICS.md). The desktop
//! app reports how many people install and use Switchboard and how many accounts they connect,
//! to PassionCode's self-hosted Aptabase at `analytics.passioncode.ai` (SB-H01, passioncode-platform
//! PLAT-019; the personal `analytics.sshlg.me` until 0.6.41), so one person using several
//! PassionCode apps can be counted once across them.
//!
//! What is sent: event names, counts and kinds (provider, credential type, how an account was
//! added, what caused a switch), the app version and OS, and one random installation id shared
//! by the PassionCode apps on this machine. Never: account ids, labels, e-mails, organisation
//! ids, pool names, paths, tokens, provider responses or errors.
//!
//! Only release builds carry an App Key (`SWITCHBOARD_ANALYTICS_APP_KEY`, set by the release
//! workflow), so source builds, forks and tests send nothing.
//!
//! On by default, Switchboard's own switch (SB-246, operator decision 2026-11-04, replacing the
//! rule of issue #128): events are queued and sent from the first start, a background start
//! included; the window shows a first-run notice that says so, once. The switch lives in this
//! app's state file (`enabled`, absent = on) and turns Switchboard off only; the shared
//! `installation.json` supplies the installation id and is never written for the switch. An
//! install whose person answered off here under issue #128 (its `disclosed_at` with the shared
//! switch off) stays off.
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use switchboard_core::Snapshot;

pub const HOST: &str = "https://analytics.passioncode.ai";
/// Compiled into release builds only.
pub const APP_KEY: Option<&str> = option_env!("SWITCHBOARD_ANALYTICS_APP_KEY");
/// The installation file every PassionCode app shares: `<data dir>/PassionCode/installation.json`.
pub const SHARED_FOLDER: &str = "PassionCode";
pub const SHARED_FILE: &str = "installation.json";
/// This app's own bookkeeping in its data folder.
pub const STATE_FILE: &str = "analytics-state.json";
const SDK: &str = concat!("switchboard-analytics@", env!("CARGO_PKG_VERSION"));
/// `production` for a release (a version without a pre-release part, built in release mode);
/// `sandbox` for a debug build or a pre-release (`-rc.N` rehearsals, betas).
const ENVIRONMENT: &str = environment_for(cfg!(debug_assertions), env!("CARGO_PKG_VERSION"));
const fn environment_for(debug_build: bool, version: &str) -> &'static str {
    if debug_build || has_prerelease(version) {
        "sandbox"
    } else {
        "production"
    }
}
/// Aptabase's build mode follows the environment (sshlg-analytics client contract, "Build modes", 2026-10-09):
/// `isDebug` is true exactly when the event is not production, so a pre-release lands in the dashboard's Debug view.
fn is_debug_event(environment: &str) -> bool {
    environment != "production"
}
const fn has_prerelease(version: &str) -> bool {
    let bytes = version.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            return true;
        }
        i += 1;
    }
    false
}
/// The server refuses events older than a day; keep a margin.
const MAX_AGE_SECONDS: i64 = 23 * 3600;
/// The server's batch cap.
const BATCH: usize = 25;
/// Events kept while the server cannot be reached; the oldest go first.
const MAX_QUEUE: usize = 200;
/// A new session after this long without an event (the SDKs' rule).
const SESSION_IDLE_SECONDS: i64 = 3600;

fn now() -> i64 {
    crate::monitor::now()
}

/// The file the PassionCode apps share. Unknown fields written by another app are kept.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installation {
    pub version: u32,
    pub id: String,
    #[serde(default = "enabled_by_default")]
    pub analytics: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}
fn enabled_by_default() -> bool {
    true
}

/// `<data dir>/PassionCode`: `~/Library/Application Support/PassionCode` on macOS,
/// `%APPDATA%\PassionCode` on Windows.
pub fn shared_folder() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(SHARED_FOLDER))
}

/// Reads the shared installation, creating it when no PassionCode app has yet. Creation never
/// replaces a file another app wrote at the same moment: the new file is linked into place, which
/// fails if one is already there, and that one is read instead. `created` tells the caller this
/// machine had no PassionCode app before.
pub fn installation(folder: &Path) -> Result<(Installation, bool), String> {
    let path = folder.join(SHARED_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => return parse(&bytes).map(|i| (i, false)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Installation file unreadable.".into()),
    }
    std::fs::create_dir_all(folder).map_err(|_| "Installation folder unavailable.".to_string())?;
    let fresh = Installation {
        version: 1,
        id: uuid::Uuid::new_v4().to_string(),
        analytics: true,
        created_at: now(),
        other: Map::new(),
    };
    let temporary = folder.join(format!(".{SHARED_FILE}.{}", uuid::Uuid::new_v4()));
    let body = serde_json::to_vec_pretty(&fresh).map_err(|_| "Installation unavailable.")?;
    std::fs::write(&temporary, body).map_err(|_| "Installation folder unavailable.".to_string())?;
    let linked = std::fs::hard_link(&temporary, &path);
    let _ = std::fs::remove_file(&temporary);
    match linked {
        Ok(()) => Ok((fresh, true)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::read(&path)
            .map_err(|_| "Installation file unreadable.".to_string())
            .and_then(|bytes| parse(&bytes))
            .map(|i| (i, false)),
        Err(_) => Err("Installation folder unavailable.".into()),
    }
}

fn parse(bytes: &[u8]) -> Result<Installation, String> {
    let installation: Installation =
        serde_json::from_slice(bytes).map_err(|_| "Installation file unreadable.".to_string())?;
    // A file whose id is not a UUID is not ours to trust or to repair.
    uuid::Uuid::parse_str(&installation.id).map_err(|_| "Installation file unreadable.")?;
    Ok(installation)
}

/// Another PassionCode app's switch in the shared file. Switchboard never writes it (SB-246:
/// the switch is each product's own); tests use it to play the other app.
#[cfg(test)]
pub fn set_shared_enabled(folder: &Path, enabled: bool) -> Result<Installation, String> {
    let (mut installation, _) = installation(folder)?;
    installation.analytics = enabled;
    let path = folder.join(SHARED_FILE);
    let temporary = folder.join(format!(".{SHARED_FILE}.{}", uuid::Uuid::new_v4()));
    let body = serde_json::to_vec_pretty(&installation).map_err(|_| "Installation unavailable.")?;
    std::fs::write(&temporary, body)
        .and_then(|_| std::fs::rename(&temporary, &path))
        .map_err(|_| {
            let _ = std::fs::remove_file(&temporary);
            "Could not save the analytics choice.".to_string()
        })?;
    Ok(installation)
}

/// The rule this state was last written under: absent (0) in every state written before SB-246,
/// 2 since. Only a state from before reads the old fields (`disclosed_at`, a day without the
/// install flag) as history.
const RULE: u32 = 2;

/// This app's bookkeeping: whether the install was reported, the last day reported active, the
/// accounts already counted (ids stay on this machine), Switchboard's own switch (SB-246; absent
/// is on) and when the first-run notice was acknowledged (Unix seconds).
#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    rule: u32,
    #[serde(default)]
    installed: bool,
    /// The answer under the rule of 2026-10-08 (issue #128), kept as history: read once by the
    /// migration in `Analytics::new`, never written again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disclosed_at: Option<i64>,
    /// Switchboard's own switch (SB-246, operator 2026-11-04): absent is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    /// When the person acknowledged the first-run notice (SB-246); absent shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notice_at: Option<i64>,
    #[serde(default)]
    active_day: Option<i64>,
    #[serde(default)]
    known: Option<BTreeMap<String, (String, String)>>,
    /// This app created the machine's shared installation file at a start made while the switch
    /// was off: kept until `app_installed` reports it, so a later start says so truly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    machine_first: bool,
}
impl State {
    /// At load: a state from before SB-246 that was never answered under issue #128 and reported
    /// a day but not the install (0.6.81 and earlier) was an install all the same, so it is never
    /// reported a second time. A day in a state answered under issue #128 says nothing about the
    /// install: an answer of off, then on in one run, reported a day before the install.
    fn migrated(mut self) -> Self {
        if self.rule < RULE
            && self.disclosed_at.is_none()
            && self.active_day.is_some()
            && !self.installed
        {
            self.installed = true;
            self.machine_first = false;
        }
        self
    }
    fn on(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// Reads this app's state. An absent file is a fresh install. A file that cannot be read or
/// parsed may hold the person's "off" and may already have reported the install: it reads as
/// off for this run (fail closed, not written unless something else is saved) and never sends
/// `app_installed` a second time (SB-246).
fn read_state(path: &Path) -> State {
    let unreadable = || State {
        installed: true,
        enabled: Some(false),
        ..State::default()
    };
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<State>(&bytes)
            .map(State::migrated)
            .unwrap_or_else(|_| unreadable()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(_) => unreadable(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    timestamp: String,
    session_id: String,
    event_name: String,
    system_props: Value,
    props: Value,
    #[serde(skip)]
    at: i64,
    /// Removes exactly what was sent, whatever the queue did meanwhile.
    #[serde(skip)]
    seq: u64,
}

/// Sends events for one app. Built only with an App Key; every failure is swallowed after
/// logging its kind — analytics never fails or delays an operation.
pub struct Analytics {
    key: String,
    host: String,
    shared: PathBuf,
    /// The data folder: `spend.json` is read there once a day for `app_active` (C-Q6).
    root: PathBuf,
    state_path: PathBuf,
    state: Mutex<State>,
    /// Switchboard's own switch beside `state.enabled`, so it can be read while the state is
    /// locked.
    on: AtomicBool,
    session: Mutex<(String, i64)>,
    queue: Mutex<VecDeque<Event>>,
    retry_at: AtomicI64,
    flushing: AtomicBool,
    next_seq: std::sync::atomic::AtomicU64,
    http: reqwest::Client,
}

impl Analytics {
    pub fn new(key: &str, host: &str, shared: PathBuf, data_root: &Path) -> Self {
        let state_path = data_root.join(STATE_FILE);
        let mut state = read_state(&state_path);
        // SB-246 migration, once (coordinator's correction 2026-11-04, the ecosystem's reading:
        // sshlg-growth consent contract a48f41e): a person who answered here under issue #128
        // while the shared switch reads off said no, and stays off; every other install is on.
        let mut migrate = false;
        // Off for this run only, not written: the shared file could not be read, so the old
        // answer is decided at a later start (the state keeps its old rule until then).
        let mut deferred = false;
        if state.rule < RULE && state.enabled.is_none() && state.disclosed_at.is_some() {
            match installation(&shared) {
                Ok((i, _)) => {
                    state.enabled = Some(i.analytics);
                    migrate = true;
                }
                Err(_) => deferred = true,
            }
        }
        if !deferred {
            state.rule = RULE;
        }
        let on = state.on() && !deferred;
        let client = Self {
            key: key.to_owned(),
            host: host.trim_end_matches('/').to_owned(),
            shared,
            root: data_root.to_owned(),
            state_path,
            state: Mutex::new(state),
            on: AtomicBool::new(on),
            session: Mutex::new((String::new(), 0)),
            queue: Mutex::new(VecDeque::new()),
            retry_at: AtomicI64::new(0),
            flushing: AtomicBool::new(false),
            next_seq: std::sync::atomic::AtomicU64::new(0),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        };
        if migrate {
            let state = client.state.lock().unwrap_or_else(|p| p.into_inner());
            client.save(&state);
        }
        client
    }

    /// Whether Switchboard's own switch is on.
    fn on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    /// The installation id when events may leave now: Switchboard's switch is on and the shared
    /// file can be trusted (fail closed). The shared file's `analytics` field is another app's
    /// and is not read (SB-246).
    fn sending(&self) -> Option<(String, bool)> {
        if !self.on() {
            return None;
        }
        installation(&self.shared)
            .ok()
            .map(|(i, created)| (i.id, created))
    }

    /// For the window: `enabled` is Switchboard's own switch, `notice_pending` whether the
    /// first-run notice still waits to be acknowledged (SB-246).
    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        json!({
            "available": true,
            "enabled": self.on(),
            "notice_pending": state.notice_at.is_none(),
        })
    }

    /// The first-run notice was read: *OK*, or a choice made with the switch.
    pub fn acknowledge(&self) -> Value {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.notice_at.is_none() {
                state.notice_at = Some(now());
                self.save(&state);
            }
        }
        self.status()
    }

    /// Switchboard's own switch (SB-246): About's checkbox or the notice's *Turn off*. It never
    /// touches the shared file. Off drops what waits, and nothing more leaves; it also
    /// acknowledges the notice, which the choice answered.
    pub fn set_enabled(&self, enabled: bool) -> Result<Value, String> {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.enabled = Some(enabled);
            if state.notice_at.is_none() {
                state.notice_at = Some(now());
            }
            // A choice that cannot be written (`analytics_state_unwritable`) holds for this run.
            self.save(&state);
            self.on.store(enabled, Ordering::SeqCst);
        }
        if !enabled {
            // Nothing recorded before the switch leaves the machine afterwards.
            if let Ok(mut queue) = self.queue.lock() {
                queue.clear();
            }
        }
        Ok(self.status())
    }

    fn session(&self, at: i64) -> String {
        let mut session = self.session.lock().unwrap_or_else(|p| p.into_inner());
        if session.0.is_empty() || at - session.1 > SESSION_IDLE_SECONDS {
            // The SDKs' numeric form: seconds × 10^8 + eight random digits.
            let random = uuid::Uuid::new_v4().as_u128() % 100_000_000;
            session.0 = format!("{}", at as i128 * 100_000_000 + random as i128);
        }
        session.1 = at;
        session.0.clone()
    }

    /// Queues one event; returns whether it was queued (analytics on). The caller flushes.
    pub fn track(&self, name: &str, mut props: Map<String, Value>) -> bool {
        let Some((id, _)) = self.sending() else {
            return false;
        };
        let at = now();
        props.insert("install_id".into(), json!(id));
        // sshlg-growth counts installs by `iid` and keeps only `production` and `sandbox`
        // events (operator decision 2026-10-05, growth report 2026-10-05-analytics-platform-review
        // decision 3): the same id under growth's name, and where the build came from.
        props.insert("iid".into(), json!(id));
        props.insert("environment".into(), json!(ENVIRONMENT));
        let event = Event {
            timestamp: rfc3339(at),
            session_id: self.session(at),
            event_name: name.to_owned(),
            system_props: json!({
                "isDebug": is_debug_event(ENVIRONMENT),
                "osName": if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" },
                "appVersion": env!("CARGO_PKG_VERSION"),
                "sdkVersion": SDK,
            }),
            props: Value::Object(props),
            at,
            seq: self.next_seq.fetch_add(1, Ordering::SeqCst),
        };
        let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        queue.push_back(event);
        while queue.len() > MAX_QUEUE {
            queue.pop_front();
        }
        true
    }

    /// Sends what is queued, 25 at a time. Transport errors, 429 and 5xx keep the batch for a
    /// later try with backoff; 400 and 404 drop it (the server will never take it).
    pub async fn flush(&self) {
        if self.retry_at.load(Ordering::SeqCst) > now()
            || self.flushing.swap(true, Ordering::SeqCst)
        {
            return;
        }
        // Cleared on every exit, a panic or a dropped future included.
        struct Flushing<'a>(&'a AtomicBool);
        impl Drop for Flushing<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _flushing = Flushing(&self.flushing);
        let clear = || {
            if let Ok(mut queue) = self.queue.lock() {
                queue.clear();
            }
        };
        if !self.on() {
            // Turned off in Switchboard: nothing waiting may leave.
            clear();
            return;
        }
        if installation(&self.shared).is_err() {
            // The shared file cannot be read just now: keep what waits for a later pass.
            return;
        }
        loop {
            let batch: Vec<Event> = {
                let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
                let cutoff = now() - MAX_AGE_SECONDS;
                queue.retain(|e| e.at >= cutoff);
                queue.iter().take(BATCH).cloned().collect()
            };
            if batch.is_empty() {
                break;
            }
            let sent = self
                .http
                .post(format!("{}/api/v0/events", self.host))
                .header("App-Key", &self.key)
                .json(&batch)
                .send()
                .await;
            let outcome = match &sent {
                Ok(r) if r.status().is_success() => "sent",
                Ok(r) if r.status().as_u16() == 400 || r.status().as_u16() == 404 => "refused",
                Ok(_) | Err(_) => "deferred",
            };
            crate::oplog::event(
                "analytics_flush",
                &[
                    ("outcome", crate::oplog::Field::Code(outcome)),
                    ("events", crate::oplog::Field::Number(batch.len() as i64)),
                ],
            );
            if outcome == "deferred" {
                let previous = self.retry_at.load(Ordering::SeqCst);
                let wait = if previous > 0 { 600 } else { 60 };
                self.retry_at.store(now() + wait, Ordering::SeqCst);
                break;
            }
            self.retry_at.store(0, Ordering::SeqCst);
            let sent: std::collections::BTreeSet<u64> = batch.iter().map(|e| e.seq).collect();
            let mut queue = self.queue.lock().unwrap_or_else(|p| p.into_inner());
            queue.retain(|e| !sent.contains(&e.seq));
        }
    }

    pub fn pending(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// Writes this app's state; false (and one `analytics_state_unwritable` log line, a code
    /// only) when it could not. The cost is a repeated once-only event, the notice shown again
    /// or a choice kept for this run only, never an event sent after the person turned it off
    /// in this run.
    fn save(&self, state: &State) -> bool {
        let written = serde_json::to_vec(state).ok().is_some_and(|body| {
            switchboard_core::private_fs::private_write(&self.state_path, &body).is_ok()
        });
        if !written {
            crate::oplog::event(
                "analytics_state_unwritable",
                &[("file", crate::oplog::Field::Code("analytics-state"))],
            );
        }
        written
    }

    /// At start: the install once per app, then a start with how it was launched, a background
    /// start included (SB-246). While the switch is off nothing is queued; the accounts are
    /// still counted locally so turning it on later reports nothing retroactively.
    pub fn started(&self, launch: &str, snapshot: &Snapshot) {
        let machine_first = match installation(&self.shared) {
            Ok((_, created)) => created,
            Err(_) => return,
        };
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut changed = false;
        if state.known.is_none() {
            // Accounts present before analytics existed are not "added" later.
            state.known = Some(known(snapshot));
            changed = true;
        }
        if machine_first && !state.installed && !state.machine_first {
            // This start created the shared file; a later `app_installed` reports it truly.
            state.machine_first = true;
            changed = true;
        }
        if self.on() && !state.installed {
            let mut props = counts(snapshot);
            props.insert("first_passioncode_app".into(), json!(state.machine_first));
            if self.track("app_installed", props) {
                state.installed = true;
                state.machine_first = false;
                changed = true;
            }
        }
        if changed {
            self.save(&state);
        }
        drop(state);
        let mut props = Map::new();
        props.insert("launch".into(), json!(launch));
        self.track("app_started", props);
    }

    /// Every monitor pass: once a UTC day an active event with the counts, and any account added
    /// or removed by a path no operation reported. The ceilings file is read only on the pass
    /// that sends the day's event, so the idle budget is unchanged.
    pub fn tick(&self, snapshot: &Snapshot, time: i64) {
        let day = time.div_euclid(86_400);
        let due = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.active_day != Some(day)
        };
        if due && self.track("app_active", active_counts(snapshot, &self.root, time)) {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.active_day = Some(day);
            self.save(&state);
        }
        self.reconcile(snapshot, "sync");
    }

    /// Emits `account_added` / `account_removed` for the difference from the accounts already
    /// counted, attributing additions to `method`.
    pub fn reconcile(&self, snapshot: &Snapshot, method: &str) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let Some(before) = state.known.clone() else {
            return;
        };
        let after = known(snapshot);
        if before == after {
            return;
        }
        if self.sending().is_none() {
            // Off: keep counting locally so turning it on later reports nothing retroactively.
            state.known = Some(after);
            self.save(&state);
            return;
        }
        let total = after.len();
        for (id, (provider, kind)) in &after {
            if !before.contains_key(id) {
                let mut props = Map::new();
                props.insert("provider".into(), json!(provider));
                props.insert("kind".into(), json!(kind));
                props.insert("method".into(), json!(method));
                props.insert("accounts".into(), json!(total));
                self.track("account_added", props);
            }
        }
        for (id, (provider, kind)) in &before {
            if !after.contains_key(id) {
                let mut props = Map::new();
                props.insert("provider".into(), json!(provider));
                props.insert("kind".into(), json!(kind));
                props.insert("accounts".into(), json!(total));
                self.track("account_removed", props);
            }
        }
        state.known = Some(after);
        self.save(&state);
    }

    /// A project created, changed or removed: how many projects, folders and project accounts,
    /// never a name or a path.
    pub fn project(&self, change: &str, snapshot: &Snapshot) {
        let mut props = Map::new();
        props.insert("change".into(), json!(change));
        props.insert("projects".into(), json!(snapshot.projects.len()));
        props.insert(
            "folders".into(),
            json!(snapshot
                .projects
                .iter()
                .map(|p| p.folders.len())
                .sum::<usize>()),
        );
        props.insert(
            "project_accounts".into(),
            json!(snapshot
                .accounts
                .iter()
                .filter(|a| snapshot.project_of_pool(&a.pool).is_some())
                .count()),
        );
        self.track("project_changed", props);
    }
    /// A switch of the account in use or of a pool's managed route.
    pub fn switched(&self, provider: &str, target: &str, cause: &str) {
        let mut props = Map::new();
        props.insert("provider".into(), json!(provider));
        props.insert("target".into(), json!(target));
        props.insert("cause".into(), json!(cause));
        self.track("account_switched", props);
    }
}

fn known(snapshot: &Snapshot) -> BTreeMap<String, (String, String)> {
    snapshot
        .accounts
        .iter()
        .map(|a| {
            (
                a.id.clone(),
                (a.provider.as_str().to_owned(), kind(a.kind).to_owned()),
            )
        })
        .collect()
}

fn kind(kind: switchboard_core::AuthKind) -> &'static str {
    match kind {
        switchboard_core::AuthKind::OAuth => "oauth",
        switchboard_core::AuthKind::ApiKey => "api_key",
        switchboard_core::AuthKind::SetupToken => "setup_token",
    }
}

/// Counts only: no identifier, label or pool name.
fn counts(snapshot: &Snapshot) -> Map<String, Value> {
    let accounts = &snapshot.accounts;
    let by =
        |f: &dyn Fn(&switchboard_core::Account) -> bool| accounts.iter().filter(|a| f(a)).count();
    let pools: std::collections::BTreeSet<_> = accounts
        .iter()
        .map(|a| (a.provider.as_str(), a.pool.as_str()))
        .collect();
    let mut props = Map::new();
    props.insert("accounts".into(), json!(accounts.len()));
    props.insert("enabled".into(), json!(by(&|a| a.enabled)));
    props.insert(
        "claude".into(),
        json!(by(&|a| a.provider == switchboard_core::Provider::Claude)),
    );
    props.insert(
        "codex".into(),
        json!(by(&|a| a.provider == switchboard_core::Provider::Codex)),
    );
    props.insert(
        "oauth".into(),
        json!(by(&|a| a.kind == switchboard_core::AuthKind::OAuth)),
    );
    props.insert(
        "api_key".into(),
        json!(by(&|a| a.kind == switchboard_core::AuthKind::ApiKey)),
    );
    props.insert(
        "setup_token".into(),
        json!(by(&|a| a.kind == switchboard_core::AuthKind::SetupToken)),
    );
    props.insert("pools".into(), json!(pools.len()));
    props.insert(
        "rotation_on".into(),
        json!(snapshot.policies.iter().filter(|p| p.enabled).count()),
    );
    props.insert("project_rules".into(), json!(snapshot.rules.len()));
    props.insert("projects".into(), json!(snapshot.projects.len()));
    props
}

/// `app_active`'s props: the counts above, plus how many fallback chains and spending ceilings
/// are set and how many ceiling questions wait (decision C-Q6, 0.7.0). Counts only: never a
/// scope, a group id, a workflow id, a question id, an agent, a key name or an amount.
fn active_counts(snapshot: &Snapshot, root: &Path, now: i64) -> Map<String, Value> {
    let mut props = counts(snapshot);
    props.extend(chains_and_ceilings(snapshot, crate::spend::read(root), now));
    props
}

/// `chains` always; `ceilings` and `ceiling_questions` only when `spend.json` could be read
/// (an unreadable file is unknown, never zero), after the same pruning the window applies.
fn chains_and_ceilings(
    snapshot: &Snapshot,
    spend: Result<crate::spend::SpendFile, String>,
    now: i64,
) -> Map<String, Value> {
    let mut props = Map::new();
    props.insert("chains".into(), json!(snapshot.chains.len()));
    if let Ok(mut file) = spend {
        crate::spend::prune(&mut file, Some(snapshot), now);
        props.insert("ceilings".into(), json!(file.ceilings.len()));
        let open = file.questions.iter().filter(|q| q.answer.is_none()).count();
        props.insert("ceiling_questions".into(), json!(open));
    }
    props
}

/// `YYYY-MM-DDTHH:MM:SS.000Z` for Unix seconds (civil-from-days, proleptic Gregorian).
fn rfc3339(seconds: i64) -> String {
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
    use std::sync::Arc;
    use switchboard_core::{Account, AuthKind, Provider};

    #[test]
    fn release_builds_send_to_passioncodes_own_host() {
        // SB-H01: PassionCode's apps moved off the personal instance (passioncode-platform PLAT-019).
        assert_eq!(HOST, "https://analytics.passioncode.ai");
    }

    #[derive(Clone, Default)]
    struct Server {
        bodies: Arc<Mutex<Vec<Value>>>,
        keys: Arc<Mutex<Vec<String>>>,
        status: Arc<Mutex<u16>>,
    }
    async fn serve(server: Server) -> String {
        let app = Router::new()
            .route(
                "/api/v0/events",
                post(
                    |State(s): State<Server>,
                     headers: axum::http::HeaderMap,
                     Json(body): Json<Value>| async move {
                        s.keys
                            .lock()
                            .unwrap()
                            .push(headers["app-key"].to_str().unwrap().to_owned());
                        s.bodies.lock().unwrap().push(body);
                        let code = *s.status.lock().unwrap();
                        (
                            StatusCode::from_u16(if code == 0 { 200 } else { code }).unwrap(),
                            "{}",
                        )
                    },
                ),
            )
            .with_state(server);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }
    fn account(id: &str, provider: Provider, kind: AuthKind) -> Account {
        Account {
            id: id.into(),
            label: format!("Label of {id}"),
            provider,
            kind,
            pool: "secret-pool-name".into(),
            enabled: true,
            created_at: 1,
            identity: Some("person@example.invalid".into()),
            usage: None,
            external_identity: None,
            usage_health: None,
        }
    }
    fn snapshot(accounts: Vec<Account>) -> Snapshot {
        Snapshot {
            policies: vec![],
            accounts,
            routes: Default::default(),
            events: vec![],
            rules: vec![],
            projects: vec![],
            chains: vec![],
            agent_keys: vec![],
            kimi_accounts: vec![],
            ..Default::default()
        }
    }
    /// Planted identifiers for C-Q6: a group id, a workflow id, a question id, an account, a key
    /// name and an amount (4242 cents), none of which may leave the machine.
    const PLANTED_GROUP: &str = "planted-group-id";
    const PLANTED_WORKFLOW: &str = "wf_0123456789abcdef";
    const PLANTED_QUESTION: &str = "q_0123456789abcdef";
    fn planted_chains() -> Vec<switchboard_core::Chain> {
        use switchboard_core::{Chain, ChainScope, Executor};
        let executors = vec![Executor {
            agent: "claude-code".into(),
            account_id: Some("acct-chain-planted".into()),
            key: Some("planted/prod/PLANTED_KEY_NAME".into()),
        }];
        [
            ChainScope::Machine,
            ChainScope::Project {
                pool: PLANTED_GROUP.into(),
            },
            ChainScope::Workflow {
                id: PLANTED_WORKFLOW.into(),
            },
        ]
        .into_iter()
        .map(|scope| Chain {
            scope,
            executors: executors.clone(),
            updated_at: 1,
        })
        .collect()
    }
    /// Two ceilings (the machine's, a task's) and three questions: one open, one answered and
    /// one a day old past its day, which the window no longer shows.
    fn planted_spend(now: i64) -> crate::spend::SpendFile {
        use crate::spend::{Answer, Ceiling, Question, SpendFile, Surface};
        use switchboard_core::ChainScope;
        let task = ChainScope::Workflow {
            id: PLANTED_WORKFLOW.into(),
        };
        let question = |id: &str, scope: ChainScope, reached_at: i64, answered: bool| Question {
            id: id.into(),
            scope,
            cents: 4242,
            spent: 42.42,
            reached_at,
            answer: answered.then_some(Answer::Keep),
            answered_at: answered.then_some(reached_at),
            answered_by: answered.then_some(Surface::Window),
        };
        SpendFile {
            ceilings: vec![
                Ceiling {
                    scope: ChainScope::Machine,
                    cents: 4242,
                    set_at: now,
                    doubled_from: None,
                },
                Ceiling {
                    scope: task.clone(),
                    cents: 4242,
                    set_at: now,
                    doubled_from: None,
                },
            ],
            questions: vec![
                question(PLANTED_QUESTION, task.clone(), now, false),
                question("q_00000000000000aa", task, now, true),
                question(
                    "q_00000000000000bb",
                    ChainScope::Machine,
                    now - 3 * 86_400,
                    false,
                ),
            ],
            ..Default::default()
        }
    }
    fn events(server: &Server) -> Vec<Value> {
        server
            .bodies
            .lock()
            .unwrap()
            .iter()
            .flat_map(|b| b.as_array().unwrap().clone())
            .collect()
    }
    fn names(server: &Server) -> Vec<String> {
        events(server)
            .iter()
            .map(|e| e["eventName"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_installation_is_shared_created_once_and_never_overwritten() {
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let (first, created) = installation(&shared).unwrap();
        assert!(created && first.analytics);
        let (again, created) = installation(&shared).unwrap();
        assert!(!created);
        assert_eq!(again.id, first.id);
        // Another app's fields survive this app's switch.
        let mut other = again.clone();
        other.other.insert("inbox_seen".into(), json!(true));
        std::fs::write(
            shared.join(SHARED_FILE),
            serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();
        let off = set_shared_enabled(&shared, false).unwrap();
        assert!(!off.analytics);
        let (read, _) = installation(&shared).unwrap();
        assert_eq!(
            (read.id, read.other["inbox_seen"].clone()),
            (first.id, json!(true))
        );
        // A file that is not ours is never replaced, and analytics stays off with it.
        std::fs::write(shared.join(SHARED_FILE), b"{not json").unwrap();
        assert!(installation(&shared).is_err());
        assert_eq!(
            std::fs::read(shared.join(SHARED_FILE)).unwrap(),
            b"{not json"
        );
    }

    #[test]
    fn chains_and_ceilings_are_counted_and_an_unreadable_file_is_never_zero() {
        let now = 86_400 * 20_000 + 3_600;
        let mut planted = snapshot(vec![]);
        planted.chains = planted_chains();
        let props = chains_and_ceilings(&planted, Ok(planted_spend(now)), now);
        assert_eq!(
            Value::Object(props),
            json!({"chains": 3, "ceilings": 2, "ceiling_questions": 1}),
            "an answered question and one past its day are not open"
        );
        // Nothing set: zeros, which are known.
        let none = chains_and_ceilings(&snapshot(vec![]), Ok(Default::default()), now);
        assert_eq!(
            Value::Object(none),
            json!({"chains": 0, "ceilings": 0, "ceiling_questions": 0})
        );
        // spend.json unreadable: the ceilings are unknown, so they are left out, never 0.
        let unknown = chains_and_ceilings(&planted, Err("unreadable".into()), now);
        assert_eq!(Value::Object(unknown), json!({"chains": 3}));
        // Through the file in the data folder, as tick reads it; an absent file is no ceiling.
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(active_counts(&planted, folder.path(), now)["ceilings"], 0);
        let path = folder.path().join(crate::spend::FILE);
        switchboard_core::private_fs::private_write(&path, b"{not json").unwrap();
        let props = active_counts(&planted, folder.path(), now);
        assert!(props.get("ceilings").is_none() && props.get("ceiling_questions").is_none());
        assert_eq!(props["chains"], 3);
        assert_eq!(props["accounts"], 0, "the earlier counts stay");
    }

    #[tokio::test]
    async fn events_carry_counts_and_the_installation_id_never_an_identity() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("A-SH-0000000000", &url, shared.clone(), folder.path());
        let mut one = snapshot(vec![account("acct-1", Provider::Claude, AuthKind::OAuth)]);
        // C-Q6: chains and ceilings whose scopes, ids, key name and amount are planted.
        one.chains = planted_chains();
        switchboard_core::private_fs::private_write(
            &folder.path().join(crate::spend::FILE),
            &serde_json::to_vec(&planted_spend(now())).unwrap(),
        )
        .unwrap();
        // SB-246: sent from a background start, with no answer asked for.
        client.started("background", &one);
        client.tick(&one, now());
        // A new account by a sign-in, and one removed.
        let two = snapshot(vec![account("acct-2", Provider::Codex, AuthKind::ApiKey)]);
        client.reconcile(&two, "sign_in");
        client.switched("codex", "managed", "manual");
        client.flush().await;
        assert_eq!(
            names(&server),
            [
                "app_installed",
                "app_started",
                "app_active",
                "account_added",
                "account_removed",
                "account_switched"
            ]
        );
        assert_eq!(server.keys.lock().unwrap()[0], "A-SH-0000000000");
        let all = serde_json::to_string(&events(&server)).unwrap();
        for private in [
            "acct-1",
            "acct-2",
            "Label of",
            "person@example.invalid",
            "secret-pool-name",
            PLANTED_GROUP,
            PLANTED_WORKFLOW,
            "0123456789abcdef",
            PLANTED_QUESTION,
            "acct-chain-planted",
            "PLANTED_KEY_NAME",
            "claude-code",
            // An amount as a JSON number (the session id is a string of digits).
            ":4242",
            ":42.42",
        ] {
            assert!(!all.contains(private), "{private} left the machine");
        }
        let id = installation(&shared).unwrap().0.id;
        let sent = events(&server);
        assert!(sent.iter().all(|e| e["props"]["install_id"] == id));
        // growth's names for the same facts, on every event.
        assert!(sent.iter().all(|e| e["props"]["iid"] == id));
        assert!(sent
            .iter()
            .all(|e| e["props"]["environment"] == ENVIRONMENT));
        assert!(sent
            .iter()
            .all(|e| e["systemProps"]["isDebug"] == (e["props"]["environment"] != "production")));
        assert_eq!(sent[0]["props"]["first_passioncode_app"], true);
        assert_eq!(sent[1]["props"]["launch"], "background");
        assert_eq!(sent[2]["props"]["accounts"], 1);
        // C-Q6: counts only, and only in app_active.
        assert_eq!(sent[2]["props"]["chains"], 3);
        assert_eq!(sent[2]["props"]["ceilings"], 2);
        assert_eq!(sent[2]["props"]["ceiling_questions"], 1);
        assert!(
            sent[0]["props"].get("chains").is_none(),
            "app_installed unchanged"
        );
        assert_eq!(sent[3]["props"]["method"], "sign_in");
        assert_eq!(sent[3]["props"]["provider"], "codex");
        assert_eq!(sent[3]["props"]["kind"], "api_key");
        assert_eq!(sent[4]["props"]["provider"], "claude");
        let event = &sent[0];
        assert!(event["timestamp"].as_str().unwrap().ends_with(".000Z"));
        assert!(event["sessionId"].as_str().unwrap().parse::<u128>().is_ok());
        assert!(event["systemProps"]["sdkVersion"]
            .as_str()
            .unwrap()
            .starts_with("switchboard-analytics@"));
        assert_eq!(client.pending(), 0);
    }

    /// Everything this app can report, in one go: a start, a day, an account added and
    /// removed, a switch, a project change and a bare event.
    fn everything(client: &Analytics, launch: &str) {
        let one = snapshot(vec![account("acct-1", Provider::Claude, AuthKind::OAuth)]);
        client.started(launch, &one);
        client.tick(&one, now());
        let two = snapshot(vec![account("acct-2", Provider::Codex, AuthKind::ApiKey)]);
        client.reconcile(&two, "sign_in");
        client.switched("codex", "managed", "manual");
        client.project("created", &two);
        client.track("x", Map::new());
    }
    fn state_file(folder: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(folder.join(STATE_FILE)).unwrap()).unwrap()
    }

    fn shared_bytes(shared: &Path) -> Vec<u8> {
        std::fs::read(shared.join(SHARED_FILE)).unwrap()
    }

    #[tokio::test]
    async fn on_by_default_a_background_start_sends_from_the_first_start() {
        // SB-246 (operator 2026-11-04): on by default, nothing held for an answer.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        let status = client.status();
        assert_eq!(
            (&status["enabled"], &status["notice_pending"]),
            (&json!(true), &json!(true))
        );
        everything(&client, "background");
        client.flush().await;
        assert_eq!(
            names(&server),
            [
                "app_installed",
                "app_started",
                "app_active",
                "account_added",
                "account_removed",
                "account_switched",
                "project_changed",
                "x"
            ]
        );
        assert_eq!(events(&server)[1]["props"]["launch"], "background");
        assert_eq!(events(&server)[0]["props"]["first_passioncode_app"], true);
        // The notice still waits: nothing was acknowledged by sending.
        assert_eq!(client.status()["notice_pending"], true);
        // A restart (the login item) sends its start, never a second install.
        let again = Analytics::new("k", &url, shared, folder.path());
        again.started("background", &snapshot(vec![]));
        again.flush().await;
        assert_eq!(
            names(&server)
                .iter()
                .filter(|n| *n == "app_installed")
                .count(),
            1
        );
        assert_eq!(names(&server).last().unwrap(), "app_started");
    }

    #[tokio::test]
    async fn the_notice_waits_until_acknowledged_and_ok_changes_nothing_else() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        client.started("ordinary", &snapshot(vec![]));
        let status = client.acknowledge();
        assert_eq!(
            (&status["enabled"], &status["notice_pending"]),
            (&json!(true), &json!(false))
        );
        assert!(state_file(folder.path())["notice_at"].is_i64());
        assert!(
            state_file(folder.path()).get("enabled").is_none(),
            "absent is on"
        );
        let restarted = Analytics::new("k", &url, shared, folder.path());
        assert_eq!(restarted.status()["notice_pending"], false, "shown once");
        // Turning off from the notice answers it too.
        let other = tempfile::tempdir().unwrap();
        let fresh = Analytics::new("k", &url, other.path().join("PassionCode"), other.path());
        let status = fresh.set_enabled(false).unwrap();
        assert_eq!(
            (&status["enabled"], &status["notice_pending"]),
            (&json!(false), &json!(false))
        );
    }

    #[tokio::test]
    async fn an_install_that_answered_under_the_old_rule_sees_the_notice_once() {
        // issue #128's answer (`disclosed_at`) is not the new notice's acknowledgement.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        installation(&shared).unwrap();
        std::fs::write(
            folder.path().join(STATE_FILE),
            br#"{"installed":true,"disclosed_at":1791165882,"active_day":20000,"known":{}}"#,
        )
        .unwrap();
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        let status = client.status();
        assert_eq!(
            (&status["enabled"], &status["notice_pending"]),
            (&json!(true), &json!(true))
        );
        client.started("ordinary", &snapshot(vec![]));
        client.flush().await;
        assert_eq!(names(&server), ["app_started"], "never a second install");
        client.acknowledge();
        let restarted = Analytics::new("k", &url, shared, folder.path());
        assert_eq!(restarted.status()["notice_pending"], false);
        let state = state_file(folder.path());
        assert_eq!(
            (&state["installed"], &state["active_day"], &state["rule"]),
            (&json!(true), &json!(20000), &json!(RULE))
        );
    }

    #[tokio::test]
    async fn an_install_whose_shared_file_says_off_still_sends_when_never_answered_here() {
        // SB-246: "для всех уже текущих тоже по дефолту включено"; another app's switch is its own.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        set_shared_enabled(&shared, false).unwrap();
        // Fresh, and from 0.6.81 or earlier (never answered here).
        for (name, body) in [
            ("fresh", None),
            (
                "old",
                Some(&br#"{"installed":true,"active_day":20000}"#[..]),
            ),
        ] {
            let data = folder.path().join(name);
            std::fs::create_dir_all(&data).unwrap();
            if let Some(body) = body {
                std::fs::write(data.join(STATE_FILE), body).unwrap();
            }
            let client = Analytics::new("k", &url, shared.clone(), &data);
            assert_eq!(client.status()["enabled"], true, "{name}");
            client.switched("claude", "native", "manual");
            client.flush().await;
        }
        assert_eq!(names(&server), ["account_switched", "account_switched"]);
        assert!(!installation(&shared).unwrap().0.analytics, "never written");
    }

    #[tokio::test]
    async fn an_answer_of_off_under_the_old_rule_stays_off_and_only_that() {
        // Coordinator's correction 2026-11-04 (sshlg-growth consent contract a48f41e): an
        // explicit "no" stays off. Answered here + shared off -> off; answered + shared on -> on.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let old = br#"{"installed":true,"disclosed_at":1791165882,"known":{}}"#;
        for (name, shared_on) in [("said-no", false), ("said-yes", true)] {
            let data = folder.path().join(name);
            let shared = data.join("PassionCode");
            std::fs::create_dir_all(&data).unwrap();
            set_shared_enabled(&shared, shared_on).unwrap();
            std::fs::write(data.join(STATE_FILE), old).unwrap();
            let before = shared_bytes(&shared);
            let client = Analytics::new("k", &url, shared.clone(), &data);
            assert_eq!(client.status()["enabled"], shared_on, "{name}");
            assert_eq!(
                state_file(&data)["enabled"],
                shared_on,
                "{name}: decided once"
            );
            client.started("ordinary", &snapshot(vec![]));
            client.flush().await;
            // Decided once: the shared switch flipping later does not change Switchboard's.
            set_shared_enabled(&shared, !shared_on).unwrap();
            let restarted = Analytics::new("k", &url, shared.clone(), &data);
            assert_eq!(restarted.status()["enabled"], shared_on, "{name}");
            set_shared_enabled(&shared, shared_on).unwrap();
            assert_eq!(shared_bytes(&shared).len(), before.len());
        }
        assert_eq!(names(&server), ["app_started"], "only the one who said yes");
    }

    #[tokio::test]
    async fn an_old_answer_is_decided_at_a_later_start_when_the_shared_file_cannot_be_read() {
        // Review of SB-246: off for that run only. Writing `rule: 2` with `enabled: false` then
        // would keep a person who had said yes off for good.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let data = tempfile::tempdir().unwrap();
        let shared = data.path().join("PassionCode");
        let old = br#"{"installed":true,"disclosed_at":1791165882}"#;
        std::fs::write(data.path().join(STATE_FILE), old).unwrap();
        // The shared file cannot be read: a folder in its place.
        std::fs::create_dir_all(shared.join(SHARED_FILE)).unwrap();
        let client = Analytics::new("k", &url, shared.clone(), data.path());
        assert_eq!(client.status()["enabled"], false, "fail closed for the run");
        everything(&client, "ordinary");
        client.flush().await;
        assert!(server.bodies.lock().unwrap().is_empty());
        // The person reads the notice in that run: the state is written, the old answer not.
        client.acknowledge();
        let kept = state_file(data.path());
        assert!(kept["notice_at"].is_i64(), "{kept}");
        assert!(kept.get("enabled").is_none(), "{kept}");
        assert_ne!(kept["rule"], RULE, "still the old rule: {kept}");
        // Readable again and on: the old answer is decided now, once.
        std::fs::remove_dir(shared.join(SHARED_FILE)).unwrap();
        set_shared_enabled(&shared, true).unwrap();
        let restarted = Analytics::new("k", &url, shared.clone(), data.path());
        assert_eq!(restarted.status()["enabled"], true);
        assert_eq!(state_file(data.path())["enabled"], true);
    }

    #[tokio::test]
    async fn turning_off_in_switchboard_sends_nothing_after_and_leaves_the_shared_file() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        client.started("ordinary", &snapshot(vec![]));
        let before = shared_bytes(&shared);
        assert!(client.pending() > 0);
        assert_eq!(client.set_enabled(false).unwrap()["enabled"], false);
        assert_eq!(client.pending(), 0, "what waits is dropped");
        everything(&client, "ordinary");
        client.flush().await;
        let restarted = Analytics::new("k", &url, shared.clone(), folder.path());
        everything(&restarted, "background");
        restarted.flush().await;
        assert!(server.bodies.lock().unwrap().is_empty());
        assert_eq!(
            shared_bytes(&shared),
            before,
            "the shared file is untouched"
        );
        assert_eq!(restarted.status()["enabled"], false);
        // On again later: the same installation id, never rotated, and the install once.
        restarted.set_enabled(true).unwrap();
        restarted.switched("claude", "native", "manual");
        restarted.flush().await;
        assert_eq!(names(&server), ["account_switched"]);
        assert_eq!(
            events(&server)[0]["props"]["iid"],
            json!(installation(&shared).unwrap().0.id)
        );
        assert_eq!(shared_bytes(&shared), before);
    }

    #[tokio::test]
    async fn an_off_start_reports_the_install_at_the_next_start_once() {
        // Off at the first start, then on in About: the day goes, and the next start reports the
        // install (with the first PassionCode app said truly), once.
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        std::fs::write(
            folder.path().join(STATE_FILE),
            br#"{"rule":2,"enabled":false}"#,
        )
        .unwrap();
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        let none = snapshot(vec![]);
        client.started("ordinary", &none);
        client.set_enabled(true).unwrap();
        client.tick(&none, now());
        client.flush().await;
        assert_eq!(names(&server), ["app_active"]);
        let restarted = Analytics::new("k", &url, shared.clone(), folder.path());
        restarted.started("ordinary", &none);
        restarted.flush().await;
        assert_eq!(
            names(&server),
            ["app_active", "app_installed", "app_started"]
        );
        assert_eq!(events(&server)[1]["props"]["first_passioncode_app"], true);
        assert!(state_file(folder.path()).get("machine_first").is_none());
        let again = Analytics::new("k", &url, shared, folder.path());
        again.started("ordinary", &none);
        again.flush().await;
        assert_eq!(
            names(&server)
                .iter()
                .filter(|n| *n == "app_installed")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn an_old_state_with_a_day_but_no_install_flag_is_not_a_new_install() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join(STATE_FILE),
            br#"{"installed":false,"active_day":20000}"#,
        )
        .unwrap();
        let client = Analytics::new("k", &url, folder.path().join("PassionCode"), folder.path());
        client.started("ordinary", &snapshot(vec![]));
        client.flush().await;
        assert_eq!(names(&server), ["app_started"]);
        assert_eq!(state_file(folder.path())["installed"], true);
    }

    #[tokio::test]
    async fn an_unreadable_state_is_off_for_the_run_and_never_reports_a_second_install() {
        // Fail closed: it may hold the person's "off".
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join(STATE_FILE), b"{not json").unwrap();
        let client = Analytics::new("k", &url, folder.path().join("PassionCode"), folder.path());
        assert_eq!(client.status()["enabled"], false);
        everything(&client, "ordinary");
        client.flush().await;
        assert!(server.bodies.lock().unwrap().is_empty());
        client.set_enabled(true).unwrap();
        client.started("ordinary", &snapshot(vec![]));
        client.flush().await;
        assert_eq!(names(&server), ["app_started"]);
    }

    #[tokio::test]
    async fn a_shared_file_unreadable_for_a_moment_keeps_what_waits() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        client.track("x", Map::new());
        let file = shared.join(SHARED_FILE);
        let body = std::fs::read(&file).unwrap();
        std::fs::write(&file, b"{being rewritten").unwrap();
        client.flush().await;
        assert_eq!(client.pending(), 1, "kept, not dropped");
        assert!(server.bodies.lock().unwrap().is_empty());
        std::fs::write(&file, body).unwrap();
        client.flush().await;
        assert_eq!(names(&server), ["x"]);
        // Another app's switch going off changes nothing here (SB-246).
        client.track("y", Map::new());
        set_shared_enabled(&shared, false).unwrap();
        client.flush().await;
        assert_eq!(names(&server), ["x", "y"]);
    }

    #[tokio::test]
    async fn a_state_that_cannot_be_written_is_reported_not_hidden() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        // A folder where the state file should be: unreadable and unwritable.
        std::fs::create_dir(folder.path().join(STATE_FILE)).unwrap();
        let client = Analytics::new("k", &url, folder.path().join("PassionCode"), folder.path());
        assert!(
            !client.save(&State::default()),
            "a failed write is reported"
        );
        assert_eq!(client.status()["enabled"], false, "unreadable: off");
        // The choice holds for this run; nothing reports a second install.
        client.set_enabled(true).unwrap();
        client.started("ordinary", &snapshot(vec![]));
        client.flush().await;
        assert_eq!(names(&server), ["app_started"]);
        client.set_enabled(false).unwrap();
        client.switched("claude", "native", "manual");
        client.flush().await;
        assert_eq!(names(&server), ["app_started"], "off holds for the run");
    }

    #[tokio::test]
    async fn the_switch_adds_no_field_to_the_shared_file_and_never_writes_it() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        client.started("ordinary", &snapshot(vec![]));
        let before = shared_bytes(&shared);
        for enabled in [false, true, false] {
            client.set_enabled(enabled).unwrap();
            client.acknowledge();
            assert_eq!(shared_bytes(&shared), before);
            assert_eq!(state_file(folder.path())["enabled"], enabled);
        }
    }

    /// Through the runtime, as the desktop app runs it: a background start sends its start and
    /// the day at once (SB-246).
    #[tokio::test]
    async fn through_the_runtime_a_background_start_sends_the_start_and_the_day() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let root = tempfile::tempdir().unwrap();
        let machine = tempfile::tempdir().unwrap();
        let runtime = crate::fixtures::runtime(
            root.path(),
            crate::fixtures::signed_out,
            crate::fixtures::activates,
        )
        .await;
        runtime.enable_analytics_with("k", &url, machine.path().join("PassionCode"), "background");
        assert_eq!(runtime.analytics_status()["notice_pending"], true);
        let mut pass = crate::monitor::Pass::default();
        pass.run(&runtime, false, now()).await;
        let client = runtime.analytics().unwrap();
        // The pass spawns its own flush; ours may find it running, so wait for the queue to empty.
        for _ in 0..200 {
            client.flush().await;
            if client.pending() == 0 && names(&server).len() >= 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            names(&server),
            ["app_installed", "app_started", "app_active"]
        );
        assert_eq!(events(&server)[1]["props"]["launch"], "background");
        assert_eq!(runtime.analytics_notice_seen()["notice_pending"], false);
        assert_eq!(runtime.set_analytics(false).unwrap()["enabled"], false);
        assert_eq!(client.pending(), 0);
    }

    #[test]
    fn the_environment_follows_the_build_and_the_version() {
        assert!(has_prerelease("0.6.2-rc.1"));
        assert!(has_prerelease("0.5.5-beta.1"));
        assert!(!has_prerelease("0.6.2"));
        // Tests are debug builds.
        assert_eq!(ENVIRONMENT, "sandbox");
    }

    #[tokio::test]
    async fn the_install_and_the_day_are_reported_once_and_existing_accounts_are_not_added() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let accounts = snapshot(vec![account("acct-1", Provider::Claude, AuthKind::OAuth)]);
        {
            let client = Analytics::new("k", &url, shared.clone(), folder.path());
            client.set_enabled(true).unwrap();
            client.started("ordinary", &accounts);
            client.tick(&accounts, 86_400 * 20_000 + 10);
            client.tick(&accounts, 86_400 * 20_000 + 500);
            client.flush().await;
        }
        // A restart on the same day: no second install, no second active, nothing "added".
        let client = Analytics::new("k", &url, shared, folder.path());
        client.started("ordinary", &accounts);
        client.tick(&accounts, 86_400 * 20_000 + 900);
        client.tick(&accounts, 86_400 * 20_001 + 1);
        client.flush().await;
        assert_eq!(
            names(&server),
            [
                "app_installed",
                "app_started",
                "app_active",
                "app_started",
                "app_active"
            ]
        );
    }

    #[tokio::test]
    async fn turning_it_off_stops_everything_and_drops_what_waits() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("k", &url, shared.clone(), folder.path());
        client.set_enabled(true).unwrap();
        let one = snapshot(vec![account("acct-1", Provider::Claude, AuthKind::OAuth)]);
        client.started("ordinary", &one);
        assert!(client.pending() > 0);
        assert_eq!(client.set_enabled(false).unwrap()["enabled"], false);
        assert_eq!(client.pending(), 0);
        let two = snapshot(vec![]);
        client.reconcile(&two, "manual");
        client.tick(&two, now());
        client.switched("claude", "native", "manual");
        client.flush().await;
        assert!(names(&server).is_empty());
        // Back on: an account removed while off is not reported afterwards.
        client.set_enabled(true).unwrap();
        client.reconcile(&two, "manual");
        client.flush().await;
        assert!(!names(&server).contains(&"account_removed".to_string()));
    }

    #[tokio::test]
    async fn a_busy_server_keeps_the_batch_a_refusing_one_drops_it_and_batches_hold_25() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let client = Analytics::new("k", &url, folder.path().join("PassionCode"), folder.path());
        client.set_enabled(true).unwrap();
        *server.status.lock().unwrap() = 429;
        for _ in 0..30 {
            client.track("x", Map::new());
        }
        client.flush().await;
        assert_eq!(client.pending(), 30, "429 keeps everything");
        // The backoff holds the next try.
        client.flush().await;
        assert_eq!(server.bodies.lock().unwrap().len(), 1);
        client.retry_at.store(0, Ordering::SeqCst);
        *server.status.lock().unwrap() = 400;
        client.flush().await;
        assert_eq!(client.pending(), 0, "400 is never retried");
        let sizes: Vec<usize> = server
            .bodies
            .lock()
            .unwrap()
            .iter()
            .map(|b| b.as_array().unwrap().len())
            .collect();
        assert_eq!(sizes, [25, 25, 5]);
    }

    #[tokio::test]
    async fn only_what_was_sent_leaves_the_queue() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let client = Analytics::new("k", &url, folder.path().join("PassionCode"), folder.path());
        client.set_enabled(true).unwrap();
        for _ in 0..3 {
            client.track("first", Map::new());
        }
        // The front of the queue changes while the batch is out: the oldest event is trimmed.
        let sending = client.flush();
        tokio::pin!(sending);
        tokio::select! {
            biased;
            _ = &mut sending => {}
            _ = async {
                client.queue.lock().unwrap().pop_front();
                client.track("second", Map::new());
                std::future::pending::<()>().await
            } => {}
        }
        let left: Vec<String> = client
            .queue
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.event_name.clone())
            .collect();
        assert!(left.is_empty(), "{left:?}");
        // The event tracked during the send went out in the next batch; nothing was lost.
        let sent = names(&server);
        assert_eq!(
            sent.iter().filter(|n| *n == "second").count(),
            1,
            "{sent:?}"
        );
        assert!(!client.flushing.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn an_unreachable_server_costs_nothing_and_old_events_are_dropped() {
        let folder = tempfile::tempdir().unwrap();
        let client = Analytics::new(
            "k",
            "http://127.0.0.1:9",
            folder.path().join("PassionCode"),
            folder.path(),
        );
        client.set_enabled(true).unwrap();
        client.track("x", Map::new());
        client.flush().await;
        assert_eq!(client.pending(), 1);
        client.queue.lock().unwrap()[0].at = now() - MAX_AGE_SECONDS - 1;
        client.retry_at.store(0, Ordering::SeqCst);
        client.flush().await;
        assert_eq!(client.pending(), 0);
    }

    #[test]
    fn timestamps_are_rfc3339_utc() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_791_165_882), "2026-10-05T02:04:42.000Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn a_release_build_of_a_pre_release_is_debug_in_aptabase() {
        for (debug_build, version, debug) in [
            (false, "0.9.0", false),
            (false, "0.9.0-rc.1", true),
            (false, "0.9.0-beta.2", true),
            (true, "0.9.0", true),
        ] {
            assert_eq!(
                is_debug_event(environment_for(debug_build, version)),
                debug,
                "{version} debug_build={debug_build}"
            );
        }
    }
}
