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
    write_launch_script,
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
pub const KIMI_SIGNED_OUT: &str = "This account is signed out. Sign in to it again.";
pub const KIMI_TOKEN_STALE: &str =
    "Kimi renews this sign-in the next time it runs. Launch the account once, then refresh.";
pub const KIMI_REFUSED: &str = "Kimi refused this sign-in. Sign in to the account again.";
pub const KIMI_UNREACHABLE: &str = "Kimi did not answer. Try again in a minute.";

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

async fn get(base: &str, path: &str, token: &Token) -> Result<Value, &'static str> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent(concat!("FabricSwitchboard/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| KIMI_UNREACHABLE)?;
    let response = client
        .get(format!("{base}{path}"))
        .bearer_auth(&token.access)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| KIMI_UNREACHABLE)?;
    match response.status().as_u16() {
        200 => response.json().await.map_err(|_| KIMI_UNREACHABLE),
        401 | 403 => Err(KIMI_REFUSED),
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
    let me = get(base, "/me", &token).await;
    let usages = get(base, "/usages", &token).await;
    let (nickname, tier) = me.as_ref().map(profile).unwrap_or((None, None));
    match usages {
        Ok(u) => json!({
            "signed_in": true, "nickname": nickname, "tier": tier,
            "windows": windows(&u), "checked_at": now, "error": null,
        }),
        Err(e) => {
            json!({"signed_in": e != KIMI_REFUSED, "nickname": nickname, "tier": tier, "error": e})
        }
    }
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
    let mut list = Vec::new();
    for account in store.snapshot()?.kimi_accounts {
        let status = home_status(
            &home(root, &account.id)?,
            &account.region,
            &base(&account.region),
            now,
        )
        .await;
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
    let env = BTreeMap::from([(
        "KIMI_CODE_HOME".to_string(),
        home.to_string_lossy().into_owned(),
    )]);
    // The session script lives beside the home, not in it: Kimi owns everything inside.
    let sessions = homes(root).join("sessions");
    private_dir(&sessions)?;
    let session = sessions.join(id);
    private_dir(&session)?;
    let path = write_launch_script(
        &session,
        &script(&session, program, &[], &env, false, &folder),
    )?;
    start(&path)?;
    Ok(json!({"launched": true, "account": account.label}))
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
    Ok(json!({"started": true}))
}

/// Forgets an account and deletes its home (and with it the only copy of its login).
pub fn remove(root: &Path, store: &Store, id: &str) -> Result<bool, String> {
    let home = home(root, id)?;
    let removed = store.remove_kimi_account(id)?;
    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(homes(root).join("sessions").join(id));
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, routing::get as route, Json, Router};
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
    async fn stand_in() -> String {
        let usages = json!({"usages": {
            "limit_5h": {"used_ratio": 0.25, "reset_time": "2027-01-15T08:00:00Z"},
            "limit_7d": {"used_ratio": "0.5", "reset_time": "2027-01-20T00:00:00Z"},
            "limit_month_total": {"used_ratio": 1.4},
        }});
        let me = json!({"nickname": "kimi\u{7} fan", "user_level_name": "Allegretto",
            "email": "person@example.com", "phone": {"number": "555"}});
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
            "555",
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
        assert!(launch_with(
            &root,
            &store,
            &Uuid::new_v4().to_string(),
            folder.path(),
            program,
            started
        )
        .is_err());
    }
}
