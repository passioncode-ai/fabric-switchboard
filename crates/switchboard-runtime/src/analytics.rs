//! Anonymous product analytics (operator request 2026-10-05; docs/ANALYTICS.md). The desktop
//! app reports how many people install and use Switchboard and how many accounts they connect,
//! to the self-hosted Aptabase at `analytics.sshlg.me`, so one person using several PassionCode
//! apps can be counted once across them.
//!
//! What is sent: event names, counts and kinds (provider, credential type, how an account was
//! added, what caused a switch), the app version and OS, and one random installation id shared
//! by the PassionCode apps on this machine. Never: account ids, labels, e-mails, organisation
//! ids, pool names, paths, tokens, provider responses or errors.
//!
//! Only release builds carry an App Key (`SWITCHBOARD_ANALYTICS_APP_KEY`, set by the release
//! workflow), so source builds, forks and tests send nothing. Turning analytics off in About
//! writes `"analytics": false` into the shared file, which every PassionCode app honours.
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use switchboard_core::Snapshot;

pub const HOST: &str = "https://analytics.sshlg.me";
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
const ENVIRONMENT: &str = if cfg!(debug_assertions) || has_prerelease(env!("CARGO_PKG_VERSION")) {
    "sandbox"
} else {
    "production"
};
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

/// The person's switch, shared by every PassionCode app on this machine.
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

/// This app's bookkeeping: whether the install was reported, the last day reported active, and
/// the accounts already counted (ids stay on this machine).
#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    installed: bool,
    #[serde(default)]
    active_day: Option<i64>,
    #[serde(default)]
    known: Option<BTreeMap<String, (String, String)>>,
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
    state_path: PathBuf,
    state: Mutex<State>,
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
        let state = std::fs::read(&state_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            key: key.to_owned(),
            host: host.trim_end_matches('/').to_owned(),
            shared,
            state_path,
            state: Mutex::new(state),
            session: Mutex::new((String::new(), 0)),
            queue: Mutex::new(VecDeque::new()),
            retry_at: AtomicI64::new(0),
            flushing: AtomicBool::new(false),
            next_seq: std::sync::atomic::AtomicU64::new(0),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// The installation id when analytics is on; `None` when the person turned it off or the
    /// shared file cannot be trusted (fail closed).
    fn installation_id(&self) -> Option<(String, bool)> {
        match installation(&self.shared) {
            Ok((i, created)) if i.analytics => Some((i.id, created)),
            _ => None,
        }
    }

    /// Whether events are sent now, for the About panel.
    pub fn status(&self) -> Value {
        let shared = installation(&self.shared).ok().map(|(i, _)| i);
        json!({
            "available": true,
            "enabled": shared.as_ref().is_some_and(|i| i.analytics),
        })
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<Value, String> {
        set_shared_enabled(&self.shared, enabled)
            .map_err(|_| "Could not save the analytics choice.".to_string())?;
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
        let Some((id, _)) = self.installation_id() else {
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
                "isDebug": cfg!(debug_assertions),
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

    fn save(&self, state: &State) {
        if let Ok(body) = serde_json::to_vec(state) {
            let _ = switchboard_core::private_fs::private_write(&self.state_path, &body);
        }
    }

    /// At start: the install once per app, then a start with how it was launched.
    pub fn started(&self, launch: &str, snapshot: &Snapshot) {
        let Some((_, machine_first)) = self.installation_id() else {
            return;
        };
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state.installed {
            let mut props = counts(snapshot);
            props.insert("first_passioncode_app".into(), json!(machine_first));
            if self.track("app_installed", props) {
                state.installed = true;
            }
        }
        if state.known.is_none() {
            // Accounts present before analytics existed are not "added" now.
            state.known = Some(known(snapshot));
        }
        self.save(&state);
        drop(state);
        let mut props = Map::new();
        props.insert("launch".into(), json!(launch));
        self.track("app_started", props);
    }

    /// Every monitor pass: once a UTC day an active event with the counts, and any account added
    /// or removed by a path no operation reported.
    pub fn tick(&self, snapshot: &Snapshot, time: i64) {
        let day = time.div_euclid(86_400);
        let due = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.active_day != Some(day)
        };
        if due && self.track("app_active", counts(snapshot)) {
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
        if self.installation_id().is_none() {
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

    #[tokio::test]
    async fn events_carry_counts_and_the_installation_id_never_an_identity() {
        let server = Server::default();
        let url = serve(server.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let shared = folder.path().join("PassionCode");
        let client = Analytics::new("A-SH-0000000000", &url, shared.clone(), folder.path());
        let one = snapshot(vec![account("acct-1", Provider::Claude, AuthKind::OAuth)]);
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
        assert_eq!(sent[0]["props"]["first_passioncode_app"], true);
        assert_eq!(sent[1]["props"]["launch"], "background");
        assert_eq!(sent[2]["props"]["accounts"], 1);
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
}
