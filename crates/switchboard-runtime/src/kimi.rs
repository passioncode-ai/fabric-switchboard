//! Kimi Code subscription accounts (SB-81, packet XA-02 A-5..A-7).
//!
//! Each account is one `KIMI_CODE_HOME` Switchboard owns: `<data>/kimi/<id>/`. The official
//! `kimi login` signs it in (device code, in Terminal) and the official `kimi` refreshes it; a
//! credential never leaves that folder — Kimi's refresh tokens rotate, so a copy would break at the
//! first refresh. Switchboard reads the access token in-process only to ask Kimi what the account
//! may still use (`GET /usages`) and who it is (`GET /me`) while the token is valid, as the
//! official client does; it never refreshes it, never hands it to another agent or the proxy, and
//! never writes the ordinary `~/.kimi-code` (shown, not switched — A-6).
use crate::launch::{
    agent_folder, ensure_idle, find_program, private_dir, private_write, script,
    write_launch_script, Reservation, SESSION_RECORD,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use switchboard_core::{kimi_accounts::KimiAccount, Store};
use uuid::Uuid;

/// A loopback stand-in for Kimi in tests; only `http://127.0.0.1:<port>` is accepted, so the
/// environment can never send a token to another host.
pub const KIMI_BASE_ENV: &str = "SWITCHBOARD_KIMI_BASE";
const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_CREDENTIAL: u64 = 64 * 1024;

pub const KIMI_NOT_INSTALLED: &str =
    "Kimi Code is not installed. Install it from kimi.com/code, then try again.";
pub const KIMI_LOGIN_PENDING: &str = "Finish the Kimi Code sign-in in Terminal first.";
pub const KIMI_LOGIN_EMPTY: &str =
    "The Kimi Code sign-in did not save a login. Start it again and complete it in the browser.";
pub const KIMI_LOGIN_UNKNOWN: &str = "This Kimi Code sign-in is no longer waiting. Start it again.";
/// The same sentence Claude and Codex sign-ins give at their cap (`launch::MAX_LOGINS`).
pub const LOGINS_WAITING: &str = "Finish an existing sign-in before starting another.";
pub const KIMI_SIGNED_OUT: &str = "This account is signed out. Sign in to it again.";
pub const KIMI_TOKEN_STALE: &str =
    "Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.";
pub const KIMI_REFUSED: &str = "Kimi refused this sign-in. Sign in to the account again.";
pub const KIMI_UNREACHABLE: &str = "Kimi did not answer. Try again in a minute.";
/// SB-100: what Kimi said, not one sentence for every answer.
pub const KIMI_NO_USAGE_SERVICE: &str = "Kimi offers no usage figures for this account.";
pub const KIMI_BUSY: &str = "Kimi is limiting requests right now. Switchboard asks again shortly.";
pub const KIMI_BAD_ANSWER: &str =
    "Kimi answered with something Switchboard cannot read. Try again later.";
pub const KIMI_NO_WINDOWS: &str = "Kimi returned no usage figures for this account.";
/// A `kimi` session Switchboard started still runs in this account's folder (SB-99).
pub const KIMI_SESSION_RUNNING: &str =
    "A Kimi Code session still runs on this account. Close it in Terminal first.";

fn pick_base(configured: Option<String>, region: &str) -> String {
    configured
        .filter(|b| b.starts_with("http://127.0.0.1:") && !b.contains('@'))
        .unwrap_or_else(|| {
            if region == "global" {
                "https://api.kimi.ai/coding/v1".into()
            } else {
                "https://api.kimi.com/coding/v1".into()
            }
        })
}
fn base(region: &str) -> String {
    pick_base(std::env::var(KIMI_BASE_ENV).ok(), region)
}

fn homes(root: &Path) -> PathBuf {
    root.join("kimi")
}
/// The account's own `KIMI_CODE_HOME`.
pub fn home(root: &Path, id: &str) -> Result<PathBuf, String> {
    if Uuid::parse_str(id).map(|u| u.to_string()) != Ok(id.to_owned()) {
        return Err(switchboard_core::kimi_accounts::NO_KIMI_ACCOUNT.into());
    }
    Ok(homes(root).join(id))
}

/// The person's ordinary Kimi Code home: `KIMI_CODE_HOME` when set, else `~/.kimi-code`.
pub fn ordinary_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("KIMI_CODE_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(home));
    }
    let user = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(PathBuf::from(user).join(".kimi-code"))
}

/// What a home's saved login says, read in-process and never returned: the access token while it
/// is valid. `Err` names why there is none.
struct Token {
    access: String,
}
fn regular_read(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() > MAX_CREDENTIAL {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_CREDENTIAL + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_CREDENTIAL).then_some(bytes)
}
/// What a home's own `config.toml` says about the managed Kimi Code login, as `kimi` (2.1) reads
/// it: `[providers."managed:kimi-code".oauth]` names the credential slot (`key`) and the sign-in
/// host (`oauth_host`). Kimi writes both at `kimi login`; they outrank the `region` marker, which
/// an earlier login can leave behind (SB-84: a home moved to kimi.ai still read mainland's slot).
#[derive(Default)]
struct Configured {
    /// `credentials/<name>.json` of the configured slot.
    slot: Option<PathBuf>,
    region: Option<&'static str>,
}
fn configured(home: &Path) -> Configured {
    let Some(text) = regular_read(&home.join("config.toml")) else {
        return Configured::default();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&String::from_utf8_lossy(&text)) else {
        return Configured::default();
    };
    let oauth = &value
        .get("providers")
        .and_then(|p| p.get("managed:kimi-code"))
        .and_then(|p| p.get("oauth"));
    let key = oauth.and_then(|o| o.get("key")).and_then(|k| k.as_str());
    let host = oauth
        .and_then(|o| o.get("oauth_host"))
        .and_then(|h| h.as_str())
        .map(|h| h.trim().trim_end_matches('/'));
    Configured {
        slot: key
            .and_then(storage_name)
            .map(|name| home.join("credentials").join(format!("{name}.json"))),
        region: match host {
            Some("https://auth.kimi.ai") => Some("global"),
            Some("https://auth.kimi.com") => Some("mainland-cn"),
            _ if key == Some("oauth/kimi-code") => Some("mainland-cn"),
            _ => None,
        },
    }
}
/// The file name Kimi stores a slot under (`resolveKimiTokenStorageName`): `oauth/<name>` or a
/// bare name; anything that could leave `credentials/` is refused.
fn storage_name(key: &str) -> Option<&str> {
    let name = key.strip_prefix("oauth/").unwrap_or(key);
    (!name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && name.chars().all(|c| !c.is_control()))
    .then_some(name)
}
/// The login file a home uses: the slot its `config.toml` names; without one, the managed file of
/// the region — `credentials/kimi-code.json` for mainland, the `kimi-code-env-<hash>.json` Kimi
/// derives for the global host (the newest, if several).
fn credential_file(home: &Path, region: &str) -> Option<PathBuf> {
    if let Some(slot) = configured(home).slot {
        return slot.is_file().then_some(slot);
    }
    let dir = home.join("credentials");
    if region != "global" {
        let path = dir.join("kimi-code.json");
        return path.is_file().then_some(path);
    }
    fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("kimi-code-env-") && name.ends_with(".json")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}
/// The region a home signed in to, in Kimi's order: the configured sign-in host, then the
/// `region` marker, then which managed file exists (mainland's, else global).
fn home_region(home: &Path) -> &'static str {
    if let Some(region) = configured(home).region {
        return region;
    }
    match regular_read(&home.join("region"))
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .as_deref()
    {
        Some("global") => return "global",
        Some("mainland-cn") => return "mainland-cn",
        _ => {}
    }
    if home.join("credentials/kimi-code.json").is_file() {
        "mainland-cn"
    } else {
        "global"
    }
}
fn token(home: &Path, region: &str, now: i64) -> Result<Token, &'static str> {
    let path = credential_file(home, region).ok_or(KIMI_SIGNED_OUT)?;
    let value: Value = regular_read(&path)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(KIMI_SIGNED_OUT)?;
    let access = value["access_token"].as_str().unwrap_or_default();
    // A revoked login is kept as a tombstone with an empty token and `expires_at: 0`.
    if access.is_empty() {
        return Err(KIMI_SIGNED_OUT);
    }
    let expires = value["expires_at"].as_f64().unwrap_or(0.0) as i64;
    if expires <= now + 60 {
        return Err(KIMI_TOKEN_STALE);
    }
    Ok(Token {
        access: access.to_owned(),
    })
}

/// One client for every Kimi request (SB-100): connections are reused across accounts.
fn client() -> Result<reqwest::Client, &'static str> {
    // SB-103: a client that failed to build is tried again next time, never kept as a failure.
    static CLIENT: std::sync::Mutex<Option<reqwest::Client>> = std::sync::Mutex::new(None);
    let mut kept = CLIENT.lock().map_err(|_| KIMI_UNREACHABLE)?;
    if let Some(client) = kept.as_ref() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent(concat!("FabricSwitchboard/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| KIMI_UNREACHABLE)?;
    *kept = Some(client.clone());
    Ok(client)
}
async fn get(base: &str, path: &str, token: &Token) -> Result<Value, &'static str> {
    let response = client()?
        .get(format!("{base}{path}"))
        .bearer_auth(&token.access)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| KIMI_UNREACHABLE)?;
    match response.status().as_u16() {
        200 => response.json().await.map_err(|_| KIMI_BAD_ANSWER),
        401 | 403 => Err(KIMI_REFUSED),
        404 => Err(KIMI_NO_USAGE_SERVICE),
        429 => Err(KIMI_BUSY),
        _ => Err(KIMI_UNREACHABLE),
    }
}

/// `{nickname, tier}` from `GET /me`; nothing else of the profile is kept or shown.
fn profile(me: &Value) -> (Option<String>, Option<String>) {
    let text = |v: &Value| {
        v.as_str()
            .and_then(switchboard_core::kimi_accounts::profile_text)
    };
    (text(&me["nickname"]), text(&me["user_level_name"]))
}

fn reset_seconds(text: &str) -> Option<i64> {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
}
/// The plan windows of `GET /usages` as `{name, used_percent, resets_at}`: the 5-hour window, the
/// 7-day quota of legacy plans and the monthly totals. A window the answer lacks is left out.
pub fn windows(usages: &Value) -> Vec<Value> {
    let u = &usages["usages"];
    [
        ("limit_5h", "5h"),
        ("limit_7d", "7d"),
        ("limit_month_total", "month"),
        ("limit_month_code", "month_code"),
    ]
    .iter()
    .filter_map(|(field, name)| {
        let w = &u[*field];
        let ratio = match &w["used_ratio"] {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse::<f64>().ok(),
            _ => None,
        }?;
        if !ratio.is_finite() {
            return None;
        }
        Some(json!({
            "name": name,
            "used_percent": (ratio.clamp(0.0, 1.0) * 1000.0).round() / 10.0,
            "resets_at": w["reset_time"].as_str().and_then(reset_seconds),
        }))
    })
    .collect()
}

/// The status of one home: profile and plan windows, or the reason there are none.
async fn home_status(home: &Path, region: &str, base: &str, now: i64) -> Value {
    let token = match token(home, region, now) {
        Ok(t) => t,
        Err(state) => return json!({"signed_in": state != KIMI_SIGNED_OUT, "error": state}),
    };
    let (me, usages) = tokio::join!(get(base, "/me", &token), get(base, "/usages", &token));
    let (nickname, tier) = me.as_ref().map(profile).unwrap_or((None, None));
    match usages {
        Ok(u) => {
            let windows = windows(&u);
            let error = windows.is_empty().then_some(KIMI_NO_WINDOWS);
            json!({
                "signed_in": true, "nickname": nickname, "tier": tier,
                "windows": windows, "checked_at": now, "error": error,
            })
        }
        Err(e) => {
            json!({"signed_in": e != KIMI_REFUSED, "nickname": nickname, "tier": tier, "error": e})
        }
    }
}

/// How often the owner asks Kimi about one saved account in the background, and how many
/// accounts one monitor pass asks at most (SB-99, operator decision 2026-10-12): only while a
/// fallback chain is set, so a Kimi executor at its limit is handed on and a limited Kimi
/// account is never a chain's next step.
pub const WATCH_SECONDS: i64 = 600;
pub const WATCH_PER_PASS: usize = 4;

/// When a Kimi account is limited, from its plan windows — the window's rule (`kimiLimitUntil`):
/// a window at 100 % whose reset has not passed limits the account until the latest such reset;
/// a full window with no reset time limits it until the next background check.
pub fn limited_until(windows: &[Value], now: i64) -> Option<i64> {
    windows
        .iter()
        .filter(|w| w["used_percent"].as_f64().is_some_and(|p| p >= 100.0))
        .map(|w| w["resets_at"].as_i64())
        .filter(|reset| reset.is_none_or(|r| r > now))
        .map(|reset| reset.unwrap_or(now + WATCH_SECONDS))
        .max()
}

/// The background view of saved Kimi accounts (SB-99): when each is next asked, which are
/// limited and until when, and each one's highest window use. In memory only.
#[derive(Default)]
pub(crate) struct KimiWatch {
    inner: std::sync::Mutex<WatchInner>,
}
#[derive(Default)]
struct WatchInner {
    next: std::collections::HashMap<String, i64>,
    limited: std::collections::HashMap<String, i64>,
    used: std::collections::HashMap<String, f64>,
}
impl KimiWatch {
    /// The accounts due now, oldest-asked first, at most `WATCH_PER_PASS`; each is marked asked.
    pub(crate) fn due(&self, ids: &[String], now: i64) -> Vec<String> {
        let Ok(mut inner) = self.inner.lock() else {
            return Vec::new();
        };
        inner.next.retain(|id, _| ids.contains(id));
        inner.limited.retain(|id, _| ids.contains(id));
        inner.used.retain(|id, _| ids.contains(id));
        let mut due: Vec<(i64, String)> = ids
            .iter()
            .map(|id| (inner.next.get(id).copied().unwrap_or(0), id.clone()))
            .filter(|(next, _)| *next <= now)
            .collect();
        due.sort();
        due.truncate(WATCH_PER_PASS);
        for (_, id) in &due {
            inner.next.insert(id.clone(), now + WATCH_SECONDS);
        }
        due.into_iter().map(|(_, id)| id).collect()
    }
    /// Records one answer: its windows, or nothing when Kimi gave none (the last view stays).
    pub(crate) fn record(&self, id: &str, windows: &[Value], now: i64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        match limited_until(windows, now) {
            Some(until) => inner.limited.insert(id.to_owned(), until),
            None => inner.limited.remove(id),
        };
        let used = windows
            .iter()
            .filter_map(|w| w["used_percent"].as_f64())
            .fold(None, |max: Option<f64>, p| {
                Some(max.map_or(p, |m| m.max(p)))
            });
        if let Some(used) = used {
            inner.used.insert(id.to_owned(), used);
        }
    }
    /// The accounts limited now.
    pub(crate) fn limited(&self, now: i64) -> std::collections::HashSet<String> {
        self.inner.lock().map_or_else(
            |_| Default::default(),
            |inner| {
                inner
                    .limited
                    .iter()
                    .filter(|(_, until)| **until > now)
                    .map(|(id, _)| id.clone())
                    .collect()
            },
        )
    }
    /// The highest window use last seen; an account never seen sorts last.
    pub(crate) fn used(&self, id: &str) -> f64 {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.used.get(id).copied())
            .unwrap_or(1000.0)
    }
}

/// One background pass (SB-99): asks Kimi's `/usages` — never `/me`, never a refresh — for the
/// saved accounts that are due, only while a fallback chain is set. Quiet otherwise: no file is
/// read and nothing is sent.
pub(crate) async fn watch(root: &Path, store: &Store, watch: &KimiWatch, now: i64) {
    watch_with(root, store, watch, &|region| base(region), now).await
}
pub(crate) async fn watch_with(
    root: &Path,
    store: &Store,
    watch: &KimiWatch,
    base: &(dyn Fn(&str) -> String + Sync),
    now: i64,
) {
    let Ok(snapshot) = store.snapshot() else {
        return;
    };
    if snapshot.chains.is_empty() || snapshot.kimi_accounts.is_empty() {
        return;
    }
    let ids: Vec<String> = snapshot
        .kimi_accounts
        .iter()
        .map(|a| a.id.clone())
        .collect();
    let mut asked = tokio::task::JoinSet::new();
    for id in watch.due(&ids, now) {
        let Some(account) = snapshot.kimi_accounts.iter().find(|a| a.id == id) else {
            continue;
        };
        let Ok(folder) = home(root, &id) else {
            continue;
        };
        let region = if credential_file(&folder, home_region(&folder)).is_some() {
            home_region(&folder).to_owned()
        } else {
            account.region.clone()
        };
        let base = base(&region);
        asked.spawn(async move {
            let Ok(token) = token(&folder, &region, now) else {
                return (id, None);
            };
            (
                id,
                get(&base, "/usages", &token)
                    .await
                    .ok()
                    .map(|u| windows(&u)),
            )
        });
    }
    while let Some(done) = asked.join_next().await {
        if let Ok((id, Some(windows))) = done {
            watch.record(&id, &windows, now);
        }
    }
}

/// Whether the account's own home holds a sign-in (no file is decrypted: Kimi keeps it plain).
pub fn signed_in(root: &Path, store: &Store, id: &str) -> bool {
    let (Ok(account), Ok(folder)) = (store.kimi_account(id), home(root, id)) else {
        return false;
    };
    credential_file(&folder, home_region(&folder)).is_some()
        || credential_file(&folder, &account.region).is_some()
}

/// Every saved account with its plan and usage, and the ordinary `kimi`'s sign-in.
pub async fn accounts(root: &Path, store: &Store, now: i64) -> Result<Value, String> {
    accounts_with(root, store, ordinary_home(), &|region| base(region), now).await
}
async fn accounts_with(
    root: &Path,
    store: &Store,
    ordinary: Option<PathBuf>,
    base: &(dyn Fn(&str) -> String + Sync),
    now: i64,
) -> Result<Value, String> {
    // SB-100: every account is asked at once — five unreachable accounts no longer take 80 s —
    // and each with the region its own folder names (a login inside a session may switch it).
    let accounts = store.snapshot()?.kimi_accounts;
    let mut asked = tokio::task::JoinSet::new();
    for (index, account) in accounts.iter().enumerate() {
        let folder = home(root, &account.id)?;
        let region = if credential_file(&folder, home_region(&folder)).is_some() {
            home_region(&folder).to_owned()
        } else {
            account.region.clone()
        };
        let base = base(&region);
        asked.spawn(async move { (index, home_status(&folder, &region, &base, now).await) });
    }
    // SB-103: one failed task leaves an error on its own row; the others are still read.
    let mut statuses = vec![json!({"signed_in": true, "error": KIMI_UNREACHABLE}); accounts.len()];
    while let Some(done) = asked.join_next().await {
        if let Ok((index, status)) = done {
            statuses[index] = status;
        }
    }
    let mut list = Vec::new();
    for (account, status) in accounts.into_iter().zip(statuses) {
        // A refreshed profile is kept, so the list names the account while it is signed out.
        let (nickname, tier) = (
            status["nickname"].as_str().map(str::to_owned),
            status["tier"].as_str().map(str::to_owned),
        );
        let account = if (nickname.is_some() && nickname != account.nickname)
            || (tier.is_some() && tier != account.tier)
        {
            store
                .update_kimi_account(&account.id, None, nickname, tier)
                .unwrap_or(account)
        } else {
            account
        };
        list.push(json!({"account": account, "status": status}));
    }
    let current = match ordinary.filter(|h| h.is_dir()) {
        None => Value::Null,
        Some(home) => {
            let region = home_region(&home);
            let mut status = home_status(&home, region, &base(region), now).await;
            status["region"] = json!(region);
            status
        }
    };
    Ok(json!({"accounts": list, "current": current, "installed": find_program("kimi").is_some()}))
}

/// A sign-in in progress: its home exists with `.kimi-login` and no account yet.
fn login_marker(home: &Path) -> PathBuf {
    home.join(".kimi-login")
}

/// Starts `kimi login` in Terminal for a new home. The returned id becomes the account's id.
pub fn begin_login(
    root: &Path,
    label: &str,
    region: &str,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let program = find_program("kimi").ok_or(KIMI_NOT_INSTALLED)?;
    begin_login_with(root, label, region, &program, start)
}
fn begin_login_with(
    root: &Path,
    label: &str,
    region: &str,
    program: &Path,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    if !switchboard_core::kimi_accounts::REGIONS.contains(&region) {
        return Err(switchboard_core::kimi_accounts::REGION_INVALID.into());
    }
    if label.len() > 80 || label.chars().any(char::is_control) {
        return Err("Enter a label of up to 80 characters.".into());
    }
    // SB-99: at most as many waiting Kimi sign-ins as Claude and Codex ones.
    if waiting_logins(root) >= crate::launch::MAX_LOGINS {
        return Err(LOGINS_WAITING.into());
    }
    private_dir(&homes(root))?;
    let id = Uuid::new_v4().to_string();
    let home = home(root, &id)?;
    private_dir(&home)?;
    private_write(
        &login_marker(&home),
        json!({"label": label.trim(), "region": region})
            .to_string()
            .as_bytes(),
        false,
    )?;
    let env = BTreeMap::from([(
        "KIMI_CODE_HOME".to_string(),
        home.to_string_lossy().into_owned(),
    )]);
    let path = write_launch_script(
        &home,
        &script(
            &home,
            program,
            &["login", "--region", region],
            &env,
            true,
            &home,
        ),
    )?;
    if let Err(error) = start(&path) {
        let _ = fs::remove_dir_all(&home);
        return Err(error);
    }
    Ok(json!({"login_id": id, "state": "pending"}))
}

/// The unsaved sign-in homes under `<data>/kimi/`: UUID-named folders with a sign-in marker. A
/// saved account's marker is gone once it is saved, so an account is never one of them.
fn login_homes(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(homes(root)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let id = e.file_name().to_str()?.to_owned();
            (Uuid::parse_str(&id).map(|u| u.to_string()).ok()? == id).then_some(id)
        })
        .filter_map(|id| {
            let home = home(root, &id).ok()?;
            login_marker(&home).is_file().then_some((id, home))
        })
        .collect()
}

/// Sign-ins still waiting: open in Terminal, or completed there and not yet saved.
fn waiting_logins(root: &Path) -> usize {
    login_homes(root)
        .into_iter()
        .filter(|(id, _)| login_state(root, id) != "ended")
        .count()
}

/// Removes the Kimi sign-in homes an earlier owner left behind whose `kimi login` ended without
/// signing in (SB-99): never a saved account, never one still open in Terminal, never one that
/// completed (`switchboard kimi finish <id>` can still save it), never a folder younger than
/// `min_age` (a sign-in just beginning). Runs once, at an owner's start. Returns how many went.
pub fn sweep_logins(root: &Path, store: &Store) -> usize {
    sweep_logins_older(root, store, Duration::from_secs(60))
}
fn sweep_logins_older(root: &Path, store: &Store, min_age: Duration) -> usize {
    let mut removed = 0;
    for (id, home) in login_homes(root) {
        let young = fs::symlink_metadata(&home)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
            .is_none_or(|age| age < min_age);
        if young || store.kimi_account(&id).is_ok() || login_state(root, &id) != "ended" {
            continue;
        }
        if fs::remove_dir_all(&home).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// `pending` while Terminal runs `kimi login`, `complete` once it exited successfully, `ended`
/// when no such sign-in waits — none was started here, or its Terminal session closed without
/// signing in (SB-83).
pub fn login_state(root: &Path, id: &str) -> &'static str {
    let Ok(home) = home(root, id) else {
        return "ended";
    };
    if !login_marker(&home).is_file() {
        return "ended";
    }
    if home.join(".completed").is_file() {
        return "complete";
    }
    #[cfg(unix)]
    let session = home.join(".session-pid");
    #[cfg(windows)]
    let session = home.join(".session-process");
    if session.exists() && ensure_idle(&home).is_ok() {
        "ended"
    } else {
        "pending"
    }
}

/// Saves the signed-in home as an account, named by its label or Kimi's nickname.
pub async fn finish_login(root: &Path, store: &Store, id: &str, now: i64) -> Result<Value, String> {
    finish_login_at(root, store, id, &|region| base(region), now).await
}
async fn finish_login_at(
    root: &Path,
    store: &Store,
    id: &str,
    base: &(dyn Fn(&str) -> String + Sync),
    now: i64,
) -> Result<Value, String> {
    match login_state(root, id) {
        "ended" => return Err(KIMI_LOGIN_UNKNOWN.into()),
        "pending" => return Err(KIMI_LOGIN_PENDING.into()),
        _ => {}
    }
    let home = home(root, id)?;
    let marker: Value = regular_read(&login_marker(&home))
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(KIMI_LOGIN_UNKNOWN)?;
    let region = marker["region"]
        .as_str()
        .unwrap_or("mainland-cn")
        .to_owned();
    let token = token(&home, &region, now).map_err(|_| KIMI_LOGIN_EMPTY)?;
    let (nickname, tier) = match get(&base(&region), "/me", &token).await {
        Ok(me) => profile(&me),
        Err(_) => (None, None),
    };
    let label = marker["label"]
        .as_str()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| nickname.clone())
        .unwrap_or_else(|| "Kimi Code".into());
    let account = store.add_kimi_account(id, &label, &region, nickname, tier)?;
    for leftover in [".kimi-login", ".completed", ".session-pid"] {
        let _ = fs::remove_file(home.join(leftover));
    }
    Ok(json!({"account": account}))
}

/// Ends a sign-in that has not been saved: a `kimi login` still waiting in Terminal is ended the
/// way closing its window would (SB-83), then its home goes.
pub fn cancel_login(root: &Path, store: &Store, id: &str) -> Result<(), String> {
    if store.kimi_account(id).is_ok() {
        return Err("This sign-in is already saved as an account.".into());
    }
    let home = home(root, id)?;
    if !home.exists() {
        return Ok(());
    }
    if login_state(root, id) == "pending" {
        crate::launch::end_sign_in(&home)?;
    }
    fs::remove_dir_all(&home).map_err(|_| "The sign-in folder could not be removed.".into())
}

/// SB-201: a Kimi session's context, fixed for the session as Claude Code's and Codex's. A Kimi
/// account belongs to no organization and serves no group's pool, so it is personal, also in an
/// organization's folder (the more private wins; A-Q6: launched, not refused).
fn context(store: &Store, folder: &Path) -> Result<String, String> {
    Ok(store.snapshot()?.context_for(None, None, Some(folder)))
}

/// Starts the official `kimi` on the account's home in a folder (Terminal).
pub fn launch(
    root: &Path,
    store: &Store,
    id: &str,
    working_directory: &Path,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let program = find_program("kimi").ok_or(KIMI_NOT_INSTALLED)?;
    launch_with(root, store, id, working_directory, &program, start)
}
fn launch_with(
    root: &Path,
    store: &Store,
    id: &str,
    working_directory: &Path,
    program: &Path,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let account: KimiAccount = store.kimi_account(id)?;
    let folder = agent_folder(root, working_directory)?;
    let home = home(root, id)?;
    if credential_file(&home, &account.region).is_none() {
        return Err(KIMI_SIGNED_OUT.into());
    }
    let context = context(store, &folder)?;
    let env = BTreeMap::from([
        (
            "KIMI_CODE_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        ),
        ("SWITCHBOARD_CONTEXT".to_string(), context.clone()),
    ]);
    // The session script lives beside the home, not in it: Kimi owns everything inside.
    let sessions = homes(root).join("sessions");
    private_dir(&sessions)?;
    let session = sessions.join(id);
    private_dir(&session)?;
    // SB-99: one `kimi` per account folder — a second one would share its rotating login.
    let mut reservation = Reservation::new(&session).map_err(|_| KIMI_SESSION_RUNNING)?;
    let path = write_launch_script(
        &session,
        &script(&session, program, &[], &env, false, &folder),
    )?;
    let record = json!({
        "account_id": id, "provider": "kimi", "pool": "default", "context": context,
        "mode": "isolated", "working_directory": folder, "in_place": false,
        "launched_at": switchboard_proxy::now(), "continues": null,
    });
    private_write(
        &session.join(SESSION_RECORD),
        record.to_string().as_bytes(),
        false,
    )?;
    start(&path)?;
    reservation.committed = true;
    Ok(json!({"launched": true, "account": account.label}))
}

pub const KIMI_MCP_UNREADABLE: &str = "This account's Kimi MCP settings (mcp.json) cannot be read, so Switchboard did not change them. Fix or remove that file, then try again.";

/// Adds Project Observatory to the account home's own `mcp.json` (`mcpServers.observatory`),
/// keeping every other server: `kimi` 2.1.1 takes no MCP file on its command line, and a
/// continuation's session must reach Observatory (SB-99). A file that is not a JSON object is
/// never overwritten.
/// The home's `mcp.json` as an object (empty when absent); refused when it is not a regular file
/// or not a JSON object — checked before an offer too, so a broken file never costs one.
fn read_mcp(home: &Path) -> Result<Value, String> {
    let path = home.join("mcp.json");
    Ok(match fs::symlink_metadata(&path) {
        Err(_) => json!({}),
        Ok(meta) if !meta.file_type().is_file() => return Err(KIMI_MCP_UNREADABLE.into()),
        Ok(_) => regular_read(&path)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(Value::is_object)
            .ok_or(KIMI_MCP_UNREADABLE)?,
    })
}
pub(crate) fn add_observatory(
    home: &Path,
    server: &crate::continuation::McpServer,
) -> Result<(), String> {
    let path = home.join("mcp.json");
    let mut config = read_mcp(home)?;
    let servers = config
        .as_object_mut()
        .ok_or(KIMI_MCP_UNREADABLE)?
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    servers.as_object_mut().ok_or(KIMI_MCP_UNREADABLE)?.insert(
        "observatory".into(),
        json!({"command": server.command, "args": server.args}),
    );
    private_write(
        &path,
        serde_json::to_vec_pretty(&config)
            .map_err(|_| KIMI_MCP_UNREADABLE)?
            .as_slice(),
        false,
    )
}

/// What refuses a continuation on a Kimi account before anything is offered: no `kimi`, a
/// signed-out home, an `mcp.json` the launch could not extend, or a `kimi` already running in it.
/// Everything the launch would refuse for the account is checked here, so a refusal moves a
/// chain on instead of spending an offer that then lapses.
pub fn continuation_preflight(root: &Path, store: &Store, id: &str) -> Result<(), String> {
    continuation_preflight_with(root, store, id, find_program("kimi").as_deref())
}
/// `continuation_preflight` with the `kimi` found (or not), so tests need no installed Kimi.
fn continuation_preflight_with(
    root: &Path,
    store: &Store,
    id: &str,
    program: Option<&Path>,
) -> Result<(), String> {
    store.kimi_account(id)?;
    program.ok_or(KIMI_NOT_INSTALLED)?;
    if !signed_in(root, store, id) {
        return Err(KIMI_SIGNED_OUT.into());
    }
    read_mcp(&home(root, id)?)?;
    let session = homes(root).join("sessions").join(id);
    if crate::launch::reservation_open(&session)
        || matches!(crate::launch::session_running(&session), Ok(true))
    {
        return Err(KIMI_SESSION_RUNNING.into());
    }
    Ok(())
}

/// Continues an Observatory workflow on a saved Kimi account (SB-99, operator decision
/// 2026-10-12): Terminal runs the first turn as `kimi -p <prompt>` — it accepts the handoff and
/// works, and Kimi 2.1.1 asks for no approval in that mode (measured) — then `kimi -c` resumes the
/// same session interactively in the same folder.
pub fn launch_continuation(
    root: &Path,
    store: &Store,
    id: &str,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let program = find_program("kimi").ok_or(KIMI_NOT_INSTALLED)?;
    launch_continuation_with(
        root,
        store,
        id,
        working_directory,
        continuation,
        &program,
        start,
    )
}
#[cfg_attr(windows, allow(unused_variables))]
fn launch_continuation_with(
    root: &Path,
    store: &Store,
    id: &str,
    working_directory: &Path,
    continuation: &crate::continuation::Continuation,
    program: &Path,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    #[cfg(windows)]
    {
        Err(crate::continuation::UNIX_ONLY.into())
    }
    #[cfg(not(windows))]
    {
        let account: KimiAccount = store.kimi_account(id)?;
        let folder = agent_folder(root, working_directory)?;
        let home = home(root, id)?;
        if !signed_in(root, store, id) {
            return Err(KIMI_SIGNED_OUT.into());
        }
        let sessions = homes(root).join("sessions");
        private_dir(&sessions)?;
        let session = sessions.join(id);
        private_dir(&session)?;
        let mut reservation = Reservation::new(&session).map_err(|_| KIMI_SESSION_RUNNING)?;
        add_observatory(&home, &continuation.observatory)?;
        let context = context(store, &folder)?;
        let env = BTreeMap::from([
            (
                "KIMI_CODE_HOME".to_string(),
                home.to_string_lossy().into_owned(),
            ),
            ("SWITCHBOARD_CONTEXT".to_string(), context.clone()),
        ]);
        let first = crate::continuation::prompt(continuation);
        let path = write_launch_script(
            &session,
            &crate::launch::script_then(
                &session,
                program,
                &["-p", first.as_str()],
                &["-c"],
                &env,
                &folder,
            ),
        )?;
        let record = json!({
            "account_id": id, "provider": "kimi", "pool": "default", "context": context,
            "mode": "isolated", "working_directory": folder, "in_place": false,
            "launched_at": switchboard_proxy::now(), "continues": continuation.workflow_id,
        });
        private_write(
            &session.join(SESSION_RECORD),
            record.to_string().as_bytes(),
            false,
        )?;
        start(&path)?;
        reservation.committed = true;
        Ok(json!({"launched": true, "account": account.label, "script": path}))
    }
}

/// Re-signs an account in its own home (`kimi login` again), for a login Kimi refused.
pub fn sign_in_again(
    root: &Path,
    store: &Store,
    id: &str,
    start: fn(&Path) -> Result<(), String>,
) -> Result<Value, String> {
    let account = store.kimi_account(id)?;
    let program = find_program("kimi").ok_or(KIMI_NOT_INSTALLED)?;
    let home = home(root, id)?;
    let env = BTreeMap::from([(
        "KIMI_CODE_HOME".to_string(),
        home.to_string_lossy().into_owned(),
    )]);
    let sessions = homes(root).join("sessions");
    private_dir(&sessions)?;
    let session = sessions.join(id);
    private_dir(&session)?;
    // SB-99: never `kimi login` beside a running `kimi` on the same folder.
    let mut reservation = Reservation::new(&session).map_err(|_| KIMI_SESSION_RUNNING)?;
    // SB-103: a sign-in is not a session; the last launch's record must not describe it.
    let _ = fs::remove_file(session.join(SESSION_RECORD));
    let path = write_launch_script(
        &session,
        &script(
            &session,
            &program,
            &["login", "--region", &account.region],
            &env,
            true,
            &session,
        ),
    )?;
    start(&path)?;
    reservation.committed = true;
    Ok(json!({"started": true}))
}

/// Forgets an account and deletes its home (and with it the only copy of its login).
pub fn remove(root: &Path, store: &Store, id: &str) -> Result<bool, String> {
    let home = home(root, id)?;
    // SB-99: a running `kimi` would write its login back into a folder no account names.
    let session = homes(root).join("sessions").join(id);
    if session.is_dir() && ensure_idle(&session).is_err() {
        return Err(KIMI_SESSION_RUNNING.into());
    }
    let removed = store.remove_kimi_account(id)?;
    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(homes(root).join("sessions").join(id));
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, response::IntoResponse, routing::get as route, Json, Router};
    use std::sync::Arc;
    use switchboard_core::MemoryVault;

    const NOW: i64 = 1_800_000_000;

    fn started(_: &Path) -> Result<(), String> {
        Ok(())
    }
    fn fails(_: &Path) -> Result<(), String> {
        Err("Terminal could not open.".into())
    }
    fn signed_in(home: &Path, file: &str, token: &str, expires_at: i64) {
        let dir = home.join("credentials");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(file),
            json!({"access_token": token, "refresh_token": "synthetic-refresh", "expires_at": expires_at}).to_string(),
        )
        .unwrap();
    }
    #[tokio::test]
    async fn each_kimi_answer_reads_as_what_it_is() {
        // SB-100: 404, 429, an unreadable 200 and a 200 with no figures each say so, instead of
        // one "Kimi did not answer" for all of them.
        let app = Router::new()
            .route("/me", route(|| async { Json(json!({"nickname": "n"})) }))
            .route(
                "/usages",
                route(|h: HeaderMap| async move {
                    let token = h
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    match token {
                        "Bearer gone" => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
                        "Bearer busy" => {
                            (axum::http::StatusCode::TOO_MANY_REQUESTS, "").into_response()
                        }
                        "Bearer garbled" => (axum::http::StatusCode::OK, "<html>").into_response(),
                        _ => Json(json!({"usages": {}})).into_response(),
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        for (token, expected) in [
            ("gone", KIMI_NO_USAGE_SERVICE),
            ("busy", KIMI_BUSY),
            ("garbled", KIMI_BAD_ANSWER),
            ("empty", KIMI_NO_WINDOWS),
        ] {
            let home = tempfile::tempdir().unwrap();
            signed_in(home.path(), "kimi-code.json", token, NOW + 3600);
            let status = home_status(home.path(), "mainland-cn", &base, NOW).await;
            assert_eq!(status["error"], expected, "{token}");
            assert_eq!(status["signed_in"], true, "{token}: still signed in");
        }
    }
    async fn stand_in() -> String {
        let usages = json!({"usages": {
            "limit_5h": {"used_ratio": 0.25, "reset_time": "2027-01-15T08:00:00Z"},
            "limit_7d": {"used_ratio": "0.5", "reset_time": "2027-01-20T00:00:00Z"},
            "limit_month_total": {"used_ratio": 1.4},
        }});
        let me = json!({"nickname": "kimi\u{7} fan", "user_level_name": "Allegretto",
            "email": "person@example.com", "phone": {"number": "+1-555-0100-7319"}});
        let auth = |headers: &HeaderMap| {
            headers.get("authorization").and_then(|v| v.to_str().ok())
                == Some("Bearer synthetic-access")
        };
        let app = Router::new()
            .route(
                "/usages",
                route(move |h: HeaderMap| {
                    let u = usages.clone();
                    async move {
                        if auth(&h) {
                            (axum::http::StatusCode::OK, Json(u))
                        } else {
                            (axum::http::StatusCode::UNAUTHORIZED, Json(json!({})))
                        }
                    }
                }),
            )
            .route(
                "/me",
                route(move |h: HeaderMap| {
                    let m = me.clone();
                    async move {
                        if auth(&h) {
                            (axum::http::StatusCode::OK, Json(m))
                        } else {
                            (axum::http::StatusCode::UNAUTHORIZED, Json(json!({})))
                        }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }
    fn store(root: &Path) -> Store {
        Store::open(root.join("store"), Arc::new(MemoryVault::default())).unwrap()
    }

    #[test]
    fn the_base_is_pinned_and_only_a_loopback_stand_in_overrides_it() {
        assert_eq!(
            pick_base(None, "mainland-cn"),
            "https://api.kimi.com/coding/v1"
        );
        assert_eq!(pick_base(None, "global"), "https://api.kimi.ai/coding/v1");
        assert_eq!(
            pick_base(Some("https://evil.example".into()), "global"),
            "https://api.kimi.ai/coding/v1"
        );
        assert_eq!(
            pick_base(Some("http://127.0.0.1:9@evil".into()), "global"),
            "https://api.kimi.ai/coding/v1"
        );
        assert_eq!(
            pick_base(Some("http://127.0.0.1:9".into()), "global"),
            "http://127.0.0.1:9"
        );
    }

    #[test]
    fn plan_windows_read_ratios_as_numbers_or_strings_and_clamp_them() {
        let w = windows(&json!({"usages": {
            "limit_5h": {"used_ratio": 0.256, "reset_time": "2027-01-15T08:00:00Z"},
            "limit_7d": {"used_ratio": "0.5"},
            "limit_month_total": {"used_ratio": 7},
            "limit_month_code": {"used_ratio": "n/a"},
        }}));
        assert_eq!(w.len(), 3);
        assert_eq!(w[0]["name"], "5h");
        assert_eq!(w[0]["used_percent"], 25.6);
        assert_eq!(w[0]["resets_at"], 1_800_000_000);
        assert_eq!(w[1]["used_percent"], 50.0);
        assert_eq!(w[1]["resets_at"], Value::Null);
        assert_eq!(w[2]["used_percent"], 100.0);
        assert!(windows(&json!({})).is_empty());
    }

    #[test]
    fn a_token_is_read_only_while_valid_and_a_tombstone_is_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert_eq!(token(home, "mainland-cn", NOW).err(), Some(KIMI_SIGNED_OUT));
        signed_in(home, "kimi-code.json", "", 0);
        assert_eq!(token(home, "mainland-cn", NOW).err(), Some(KIMI_SIGNED_OUT));
        signed_in(home, "kimi-code.json", "synthetic-access", NOW + 30);
        assert_eq!(
            token(home, "mainland-cn", NOW).err(),
            Some(KIMI_TOKEN_STALE)
        );
        signed_in(home, "kimi-code.json", "synthetic-access", NOW + 3600);
        assert_eq!(
            token(home, "mainland-cn", NOW).unwrap().access,
            "synthetic-access"
        );
        // The global host's file carries a derived name.
        assert!(token(home, "global", NOW).is_err());
        signed_in(
            home,
            "kimi-code-env-0123456789abcdef.json",
            "synthetic-access",
            NOW + 3600,
        );
        assert!(token(home, "global", NOW).is_ok());
        assert_eq!(home_region(home), "mainland-cn");
    }

    #[test]
    fn the_slot_kimi_is_configured_with_outranks_a_stale_marker_and_a_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        // Signed in to kimi.com once (marker, then signed out: a tombstone), now to kimi.ai.
        fs::write(home.join("region"), "mainland-cn\n").unwrap();
        signed_in(home, "kimi-code.json", "", 0);
        signed_in(
            home,
            "kimi-code-env-0e4f99c69cc27850.json",
            "synthetic-access",
            NOW + 3600,
        );
        // Another environment's slot, newer, that Kimi is not configured with.
        signed_in(home, "kimi-code-env-ffffffffffffffff.json", "", 0);
        assert_eq!(
            home_region(home),
            "mainland-cn",
            "without config the marker decides"
        );
        fs::write(
            home.join("config.toml"),
            "[providers.\"managed:kimi-code\"]\nbase_url = \"https://api.kimi.ai/coding/v1\"\n\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/kimi-code-env-0e4f99c69cc27850\"\noauth_host = \"https://auth.kimi.ai\"\n",
        )
        .unwrap();
        assert_eq!(home_region(home), "global");
        assert_eq!(
            token(home, home_region(home), NOW).unwrap().access,
            "synthetic-access"
        );
        // A configured slot whose file is gone is signed out, never another slot's login.
        fs::remove_file(home.join("credentials/kimi-code-env-0e4f99c69cc27850.json")).unwrap();
        assert_eq!(token(home, "global", NOW).err(), Some(KIMI_SIGNED_OUT));
        // A key that would leave credentials/ is ignored.
        assert_eq!(storage_name("oauth/../x"), None);
        assert_eq!(storage_name("oauth/.hidden"), None);
        assert_eq!(storage_name("oauth/kimi-code"), Some("kimi-code"));
    }

    #[test]
    fn a_sign_in_runs_kimi_login_in_its_own_home_and_a_failed_terminal_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let program = Path::new("/opt/Kimi Tools/kimi");
        assert!(begin_login_with(root, "Work", "moon", program, started).is_err());
        assert!(begin_login_with(root, "Work", "global", program, fails).is_err());
        assert_eq!(
            fs::read_dir(homes(root)).unwrap().count(),
            0,
            "a failed start left a home"
        );
        let begun = begin_login_with(root, "Work", "global", program, started).unwrap();
        let id = begun["login_id"].as_str().unwrap();
        let home = home(root, id).unwrap();
        assert_eq!(login_state(root, id), "pending");
        let script = fs::read_to_string(home.join(if cfg!(windows) {
            "launch.ps1"
        } else {
            "launch.command"
        }))
        .unwrap();
        assert!(script.contains("KIMI_CODE_HOME"));
        assert!(script.contains("login") && script.contains("global"));
        assert!(script.contains(".completed"));
        assert_eq!(login_state(root, "not-an-id"), "ended");
    }

    #[test]
    fn waiting_sign_ins_are_capped_and_a_start_sweeps_only_the_ended_ones() {
        // SB-99: four sign-ins may wait at once; a fifth is refused until one ends or is saved.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store(root);
        let program = Path::new("/opt/kimi");
        let ids: Vec<String> = (0..crate::launch::MAX_LOGINS)
            .map(|_| {
                begin_login_with(root, "", "global", program, started).unwrap()["login_id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            begin_login_with(root, "", "global", program, started).unwrap_err(),
            LOGINS_WAITING
        );
        let homes: Vec<PathBuf> = ids.iter().map(|id| home(root, id).unwrap()).collect();
        let ended = |home: &Path| {
            let _ = fs::remove_file(home.join(".launch-pending"));
            #[cfg(unix)]
            private_write(&home.join(".session-pid"), b"999999", false).unwrap();
            #[cfg(windows)]
            private_write(
                &home.join(".session-process"),
                br#"{"pid":999999,"created":1}"#,
                false,
            )
            .unwrap();
        };
        // [0] ended without signing in; [1] completed, not yet saved; [2] still open in Terminal;
        // [3] ended, and saved as an account meanwhile (its marker kept, as a crash would leave it).
        ended(&homes[0]);
        ended(&homes[1]);
        private_write(&homes[1].join(".completed"), b"complete", false).unwrap();
        ended(&homes[3]);
        store
            .add_kimi_account(&ids[3], "Saved", "global", None, None)
            .unwrap();
        assert_eq!(login_state(root, &ids[2]), "pending");
        // A start right after the sign-ins began leaves every one: they may be just beginning.
        assert_eq!(sweep_logins(root, &store), 0);
        assert_eq!(sweep_logins_older(root, &store, Duration::ZERO), 1);
        assert!(!homes[0].exists(), "an ended sign-in goes");
        assert!(homes[1].exists(), "a completed one can still be saved");
        assert!(homes[2].exists(), "an open one is kept");
        assert!(homes[3].exists(), "a saved account is never swept");
        // One slot is free again.
        assert!(begin_login_with(root, "", "global", program, started).is_ok());
    }

    #[test]
    fn a_sign_in_whose_terminal_closed_reads_ended_and_cancels_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store(root);
        let begun = begin_login_with(root, "", "global", Path::new("/opt/kimi"), started).unwrap();
        let id = begun["login_id"].as_str().unwrap().to_owned();
        let home = home(root, &id).unwrap();
        // The script ran (its marker is written) and the session is gone, without `.completed`.
        let _ = fs::remove_file(home.join(".launch-pending"));
        #[cfg(unix)]
        private_write(&home.join(".session-pid"), b"999999", false).unwrap();
        #[cfg(windows)]
        private_write(
            &home.join(".session-process"),
            br#"{"pid":999999,"created":1}"#,
            false,
        )
        .unwrap();
        assert_eq!(login_state(root, &id), "ended");
        cancel_login(root, &store, &id).unwrap();
        assert!(!home.exists());
    }

    #[tokio::test]
    async fn finishing_saves_the_account_by_nickname_and_never_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store(root);
        let base = stand_in().await;
        let base_of = |_: &str| base.clone();
        let begun =
            begin_login_with(root, "", "mainland-cn", Path::new("/bin/true"), started).unwrap();
        let id = begun["login_id"].as_str().unwrap().to_owned();
        let home = home(root, &id).unwrap();
        assert_eq!(
            finish_login_at(root, &store, &id, &base_of, NOW)
                .await
                .unwrap_err(),
            KIMI_LOGIN_PENDING
        );
        fs::write(home.join(".completed"), "complete").unwrap();
        assert_eq!(
            finish_login_at(root, &store, &id, &base_of, NOW)
                .await
                .unwrap_err(),
            KIMI_LOGIN_EMPTY
        );
        signed_in(&home, "kimi-code.json", "synthetic-access", NOW + 3600);
        let saved = finish_login_at(root, &store, &id, &base_of, NOW)
            .await
            .unwrap();
        assert_eq!(saved["account"]["label"], "kimi fan");
        assert_eq!(saved["account"]["tier"], "Allegretto");
        assert!(!login_marker(&home).exists() && !home.join(".completed").exists());
        assert_eq!(login_state(root, &id), "ended");
        // The store names the account and tier; no token, e-mail or phone reaches it.
        let stored = fs::read_to_string(root.join("store/accounts.json")).unwrap_or_default();
        let all = format!(
            "{stored}{}",
            serde_json::to_string(&store.snapshot().unwrap()).unwrap()
        );
        for secret in [
            "synthetic-access",
            "synthetic-refresh",
            "person@example.com",
            // The whole number: a bare "555" also turns up in random account ids and temporary
            // paths (Windows ARM nightly, 2026-10-14), which is no leak.
            "555-0100-7319",
        ] {
            assert!(!all.contains(secret), "{secret} reached the store");
        }
        // The list asks Kimi for the plan windows; the ordinary home is shown, not written.
        let ordinary = tempfile::tempdir().unwrap();
        signed_in(
            ordinary.path(),
            "kimi-code.json",
            "synthetic-access",
            NOW + 3600,
        );
        let before = fs::read(ordinary.path().join("credentials/kimi-code.json")).unwrap();
        let list = accounts_with(
            root,
            &store,
            Some(ordinary.path().to_owned()),
            &base_of,
            NOW,
        )
        .await
        .unwrap();
        let status = &list["accounts"][0]["status"];
        assert_eq!(status["windows"].as_array().unwrap().len(), 3);
        assert_eq!(status["error"], Value::Null);
        assert_eq!(list["current"]["nickname"], "kimi fan");
        assert_eq!(list["current"]["region"], "mainland-cn");
        assert_eq!(
            fs::read(ordinary.path().join("credentials/kimi-code.json")).unwrap(),
            before
        );
        assert!(!list.to_string().contains("synthetic-access"));
        // A refused token is named as refused; the account stays saved.
        signed_in(&home, "kimi-code.json", "revoked-elsewhere", NOW + 3600);
        let list = accounts_with(root, &store, None, &base_of, NOW)
            .await
            .unwrap();
        assert_eq!(list["accounts"][0]["status"]["error"], KIMI_REFUSED);
        assert_eq!(list["current"], Value::Null);
        // Removing forgets the account and deletes the only copy of its login.
        assert!(remove(root, &store, &id).unwrap());
        assert!(!home.exists());
        assert!(store.snapshot().unwrap().kimi_accounts.is_empty());
    }

    #[test]
    fn a_full_window_limits_until_its_latest_reset_as_the_window_shows_it() {
        // SB-99: the same rule as `kimiLimitUntil` in the window.
        let w = |used: f64, reset: Option<i64>| json!({"used_percent": used, "resets_at": reset});
        assert_eq!(limited_until(&[w(99.9, Some(NOW + 60))], NOW), None);
        assert_eq!(
            limited_until(&[w(100.0, Some(NOW + 60)), w(100.0, Some(NOW + 600))], NOW),
            Some(NOW + 600)
        );
        assert_eq!(
            limited_until(&[w(100.0, Some(NOW - 1))], NOW),
            None,
            "reset passed"
        );
        assert_eq!(
            limited_until(&[w(100.0, None)], NOW),
            Some(NOW + WATCH_SECONDS),
            "no reset time: until the next look"
        );
    }

    #[test]
    fn the_background_view_asks_each_account_every_ten_minutes_four_at_a_time() {
        // SB-99 on a fake clock: six accounts, a pass every 30 s for an hour.
        let watch = KimiWatch::default();
        let ids: Vec<String> = (0..6).map(|n| format!("k{n}")).collect();
        let mut asked = std::collections::HashMap::<String, Vec<i64>>::new();
        for pass in 0..120 {
            let now = NOW + pass * 30;
            let due = watch.due(&ids, now);
            assert!(due.len() <= WATCH_PER_PASS);
            for id in due {
                asked.entry(id).or_default().push(now);
            }
        }
        for id in &ids {
            let times = &asked[id];
            assert!(
                times.windows(2).all(|t| t[1] - t[0] >= WATCH_SECONDS),
                "{id}: {times:?}"
            );
            assert!((5..=7).contains(&times.len()), "{id}: {times:?}");
        }
        // A full window marks the account limited until its reset; an answer below 100 % clears it.
        let full = [json!({"used_percent": 100.0, "resets_at": NOW + 900})];
        watch.record("k0", &full, NOW);
        assert!(watch.limited(NOW).contains("k0"));
        assert!(!watch.limited(NOW + 900).contains("k0"));
        assert_eq!(watch.used("k0"), 100.0);
        watch.record(
            "k0",
            &[json!({"used_percent": 12.5, "resets_at": null})],
            NOW,
        );
        assert!(watch.limited(NOW).is_empty());
        assert_eq!(watch.used("never-seen"), 1000.0);
        // A removed account is forgotten.
        watch.due(&["k1".to_string()], NOW);
        assert_eq!(watch.used("k0"), 1000.0);
    }

    #[tokio::test]
    async fn the_background_view_runs_only_while_a_chain_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let store = store(root);
        let id = Uuid::new_v4().to_string();
        store
            .add_kimi_account(&id, "Work", "global", None, None)
            .unwrap();
        let watch = KimiWatch::default();
        // No chain: nothing is due-marked, so nothing was asked.
        watch_with(root, &store, &watch, &|_| "http://127.0.0.1:9".into(), NOW).await;
        assert_eq!(watch.due(std::slice::from_ref(&id), NOW), vec![id.clone()]);
    }

    #[test]
    fn observatory_joins_the_accounts_mcp_servers_and_an_unreadable_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let server = crate::continuation::McpServer {
            command: "/usr/bin/python3".into(),
            args: vec!["/opt/observatory/mcp/server.py".into()],
        };
        add_observatory(home, &server).unwrap();
        let read =
            || serde_json::from_slice::<Value>(&fs::read(home.join("mcp.json")).unwrap()).unwrap();
        assert_eq!(
            read()["mcpServers"]["observatory"]["command"],
            "/usr/bin/python3"
        );
        fs::write(
            home.join("mcp.json"),
            r#"{"mcpServers":{"context7":{"url":"https://example.invalid"}},"other":1}"#,
        )
        .unwrap();
        add_observatory(home, &server).unwrap();
        let config = read();
        assert_eq!(
            config["mcpServers"]["context7"]["url"],
            "https://example.invalid"
        );
        assert_eq!(config["other"], 1);
        assert!(config["mcpServers"]["observatory"].is_object());
        fs::write(home.join("mcp.json"), "not json").unwrap();
        assert_eq!(
            add_observatory(home, &server).unwrap_err(),
            KIMI_MCP_UNREADABLE
        );
        assert_eq!(
            fs::read_to_string(home.join("mcp.json")).unwrap(),
            "not json"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_continuation_runs_its_first_turn_then_resumes_the_session_interactively() {
        // SB-99 (operator decision 2026-10-12): `kimi -p <prompt>`, then `kimi -c`, one Terminal.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        fs::create_dir(&root).unwrap();
        let store = store(&root);
        let folder = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        store
            .add_kimi_account(&id, "Work", "global", None, None)
            .unwrap();
        let home = home(&root, &id).unwrap();
        fs::create_dir_all(&home).unwrap();
        let resumed = crate::continuation::Continuation {
            workflow_id: "wf_0123456789abcdef".into(),
            handoff_id: "hf_0123456789abcdef".into(),
            observatory: crate::continuation::McpServer {
                command: "/usr/bin/python3".into(),
                args: vec!["/opt/observatory/mcp/server.py".into()],
            },
            previous_provider: Some("claude".into()),
        };
        let program = Path::new("/opt/kimi");
        // Nothing is offered without a `kimi` to run it (found by review, SB-107).
        assert_eq!(
            continuation_preflight_with(&root, &store, &id, None).unwrap_err(),
            KIMI_NOT_INSTALLED
        );
        assert_eq!(
            continuation_preflight_with(&root, &store, &id, Some(program)).unwrap_err(),
            KIMI_SIGNED_OUT
        );
        signed_in(&home, "kimi-code.json", "synthetic-access", NOW);
        continuation_preflight_with(&root, &store, &id, Some(program)).unwrap();
        // An `mcp.json` the launch could not extend refuses before the offer too.
        fs::write(home.join("mcp.json"), b"{not json").unwrap();
        assert_eq!(
            continuation_preflight_with(&root, &store, &id, Some(program)).unwrap_err(),
            KIMI_MCP_UNREADABLE
        );
        fs::remove_file(home.join("mcp.json")).unwrap();
        let launched = launch_continuation_with(
            &root,
            &store,
            &id,
            folder.path(),
            &resumed,
            program,
            started,
        )
        .unwrap();
        let session = homes(&root).join("sessions").join(&id);
        // The answer names the script, for a continuation in place (SB-108).
        assert_eq!(launched["script"], json!(session.join("launch.command")));
        let script = fs::read_to_string(session.join("launch.command")).unwrap();
        let first = script
            .find("'/opt/kimi' '-p' 'Continue Observatory workflow wf_0123456789abcdef")
            .unwrap();
        let then = script.find("\nexec '/opt/kimi' '-c'").unwrap();
        assert!(first < then, "{script}");
        // `-c` only after a first turn that succeeded: a failed one resumes nothing.
        assert!(script[..first].ends_with("if ! "), "{script}");
        assert!(script[first..then].contains("exit 1\nfi"), "{script}");
        // The shell Terminal runs it with accepts it.
        let shell = script.lines().next().unwrap().trim_start_matches("#!");
        let checked = std::process::Command::new(shell)
            .arg("-n")
            .arg(session.join("launch.command"))
            .status()
            .unwrap();
        assert!(checked.success(), "{script}");
        assert!(!script.contains("synthetic-access"));
        assert!(home.join("mcp.json").is_file());
        let record: Value =
            serde_json::from_slice(&fs::read(session.join(SESSION_RECORD)).unwrap()).unwrap();
        assert_eq!(record["continues"], "wf_0123456789abcdef");
        // SB-201: a Kimi account belongs to no organization: its context is personal.
        assert_eq!(record["context"], "personal");
        assert!(script.contains("export SWITCHBOARD_CONTEXT='personal'"));
        assert_eq!(
            continuation_preflight_with(&root, &store, &id, Some(program)).unwrap_err(),
            KIMI_SESSION_RUNNING,
            "one kimi per account folder"
        );
    }

    #[test]
    fn a_launch_needs_a_signed_in_home_and_writes_its_script_outside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        fs::create_dir(&root).unwrap();
        let store = store(&root);
        let folder = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        store
            .add_kimi_account(&id, "Work", "mainland-cn", None, None)
            .unwrap();
        let home = home(&root, &id).unwrap();
        fs::create_dir_all(&home).unwrap();
        let program = Path::new("/bin/true");
        assert_eq!(
            launch_with(&root, &store, &id, folder.path(), program, started).unwrap_err(),
            KIMI_SIGNED_OUT
        );
        signed_in(&home, "kimi-code.json", "synthetic-access", NOW);
        assert!(launch_with(&root, &store, &id, Path::new("relative"), program, started).is_err());
        launch_with(&root, &store, &id, folder.path(), program, started).unwrap();
        let session = homes(&root).join("sessions").join(&id);
        let script = fs::read_to_string(session.join(if cfg!(windows) {
            "launch.ps1"
        } else {
            "launch.command"
        }))
        .unwrap();
        assert!(script.contains(home.to_string_lossy().as_ref()));
        assert!(!script.contains("synthetic-access"));
        assert!(!home.join("launch.command").exists());
        // SB-201: the session's context is recorded and exported, as Claude Code's and Codex's.
        let context = |session: &Path| {
            let record: Value =
                serde_json::from_slice(&fs::read(session.join(SESSION_RECORD)).unwrap()).unwrap();
            record["context"].clone()
        };
        assert_eq!(context(&session), "personal");
        assert!(script.contains("SWITCHBOARD_CONTEXT"));
        #[cfg(not(windows))]
        assert!(script.contains("export SWITCHBOARD_CONTEXT='personal'"));
        assert!(launch_with(
            &root,
            &store,
            &Uuid::new_v4().to_string(),
            folder.path(),
            program,
            started
        )
        .is_err());

        // SB-99: the launched session holds its folder — no second `kimi`, no `kimi login`
        // beside it, no removal under it — and the session list shows it.
        assert_eq!(
            launch_with(&root, &store, &id, folder.path(), program, started).unwrap_err(),
            KIMI_SESSION_RUNNING
        );
        assert_eq!(
            remove(&root, &store, &id).unwrap_err(),
            KIMI_SESSION_RUNNING
        );
        assert!(home.exists(), "the login stays while its session runs");
        let listed = crate::launch::sessions(&root, false);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["provider"], "kimi");
        assert_eq!(listed[0]["state"], "starting");
        assert!(
            !fs::read_to_string(session.join(crate::launch::SESSION_RECORD))
                .unwrap()
                .contains("synthetic-access")
        );

        // The session ended (its script ran and exited): the folder is free again.
        fs::remove_file(session.join(".launch-pending")).unwrap();
        // A-Q6: in an organization's folder a Kimi session is launched and reads personal (the
        // more private wins), never the folder's organization.
        let acme = store.add_organization("Acme", &[]).unwrap();
        store
            .save_project_in(
                None,
                "Acme",
                &[folder.path().canonicalize().unwrap()],
                &[],
                Some(Some(&acme.id)),
            )
            .unwrap();
        launch_with(&root, &store, &id, folder.path(), program, started).unwrap();
        assert_eq!(context(&session), "personal");
        fs::remove_file(session.join(".launch-pending")).unwrap();
        assert!(remove(&root, &store, &id).unwrap());
        assert!(!home.exists());
    }
}
