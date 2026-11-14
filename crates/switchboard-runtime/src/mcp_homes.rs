//! SB-358, SB-359: MCP sign-ins across a person's Claude homes.
//!
//! Claude Code keeps an MCP server's OAuth sign-in in its credential item (`mcpOAuth`, keyed
//! `<server name>|<16 hex of sha256({type,url,headers})>`), one item per config folder: the
//! ordinary home's `Claude Code-credentials` and `Claude Code-credentials-<8 hex>` for each home
//! Switchboard made (on Linux and Windows the folder's `.credentials.json`). A server signed in
//! in one home reads «Needs authentication» in every other.
//!
//! Measured on the operator's MacBook on 2026-11-13 (design note
//! `docs/research/mcp-sign-ins-2026-11-14.md`): an entry with only `accessToken`, `expiresAt`,
//! `clientId` and its discovery state, without `refreshToken` or `clientSecret`, is used by
//! Claude Code (`claude mcp list`: «Needs authentication» before, «Connected» after), and the
//! entries Switchboard's homes held were discovery stubs with an empty token.
//!
//! So a launch carries a sign-in as a MIRROR: the access token and what names its client, never
//! the refresh token or the client secret. Only one home ever renews a grant, so no provider
//! sees one refresh token used twice. A home's own sign-in (an entry with a refresh token) is
//! never overwritten; a stub, an absent entry or an older mirror is. The newest unexpired source
//! wins. No token enters a log, an answer or a file other than the target home's own item.
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The fields a mirror copies from a source sign-in; `refreshToken` and `clientSecret` never.
const MIRROR_FIELDS: [&str; 9] = [
    "serverName",
    "serverUrl",
    "accessToken",
    "expiresAt",
    "scope",
    "issuer",
    "clientId",
    "redirectUri",
    "discoveryState",
];
/// A source within this many milliseconds of its expiry is not carried.
const MARGIN_MS: i64 = 5 * 60 * 1000;

/// One Claude home: the ordinary one or one Switchboard made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Home {
    /// `ordinary`, `account` (`homes/<id>`), `group` (`runtimes/claude-<pool>`) or `shell`
    /// (`shell/claude-<pool>`).
    pub kind: &'static str,
    /// The account id or the group's pool; none for the ordinary home.
    pub id: Option<String>,
    pub folder: PathBuf,
    /// The Keychain service of its credential item.
    pub service: String,
}

/// What reading a home's credential item found.
pub(crate) enum Read {
    Absent,
    /// The item exists but was not read: a read would ask for access, the keychain is locked, or
    /// it is not JSON.
    Unknown,
    Item(Value),
}

/// Where the credential items live; native on a real machine, in memory in tests.
pub(crate) trait Items {
    fn read(&self, home: &Home) -> Read;
    /// A stamp that changes when the item is written again (Keychain's creation and modification
    /// times, a file's modification time); None when it cannot be told, which means read it.
    fn stamp(&self, home: &Home) -> Option<String>;
    fn write(&self, home: &Home, item: &Value) -> Result<(), String>;
}

/// One server's entry in one home, without its secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub key: String,
    pub name: String,
    pub url: String,
    /// It holds a token: an access token or a refresh token.
    pub signed_in: bool,
    /// It holds a refresh token: the home's own sign-in.
    pub own: bool,
    /// Milliseconds since the epoch, as Claude Code writes it.
    pub expires_at: Option<i64>,
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// The entries of one credential item, sorted by key.
pub(crate) fn entries(item: &Value) -> Vec<Entry> {
    let Some(map) = item.get("mcpOAuth").and_then(Value::as_object) else {
        return vec![];
    };
    map.iter()
        .filter(|(_, v)| v.is_object())
        .map(|(key, v)| Entry {
            key: key.clone(),
            name: text(v, "serverName")
                .map(str::to_owned)
                .unwrap_or_else(|| key.split('|').next().unwrap_or_default().to_owned()),
            url: text(v, "serverUrl").unwrap_or_default().to_owned(),
            signed_in: text(v, "accessToken").is_some() || text(v, "refreshToken").is_some(),
            own: text(v, "refreshToken").is_some(),
            expires_at: v.get("expiresAt").and_then(Value::as_i64),
        })
        .collect()
}

fn live(expires_at: Option<i64>, now_ms: i64) -> bool {
    expires_at.is_none_or(|t| t > now_ms + MARGIN_MS)
}

/// The sign-ins `sources` can lend, by server URL: each a home's own sign-in with an unexpired
/// access token, the newest expiry winning (no expiry ranks above any), with the key it was
/// filed under. By URL, not by key: the key hashes the server's configuration, so a plugin update
/// files the next sign-in under another key (measured 2026-11-14: Figma's token under one key,
/// the current configuration's stub under another, in every home).
pub(crate) fn lendable(sources: &[Value], now_ms: i64) -> BTreeMap<String, (String, Value)> {
    let mut best: BTreeMap<String, (i64, String, Value)> = BTreeMap::new();
    for item in sources {
        let Some(map) = item.get("mcpOAuth").and_then(Value::as_object) else {
            continue;
        };
        for (key, v) in map {
            let expires = v.get("expiresAt").and_then(Value::as_i64);
            let Some(url) = text(v, "serverUrl") else {
                continue;
            };
            if text(v, "accessToken").is_none()
                || text(v, "refreshToken").is_none()
                || !live(expires, now_ms)
            {
                continue;
            }
            let rank = expires.unwrap_or(i64::MAX);
            if best.get(url).is_none_or(|(r, _, _)| rank > *r) {
                let mirror: Map<String, Value> = MIRROR_FIELDS
                    .iter()
                    .filter_map(|f| v.get(*f).map(|x| ((*f).to_owned(), x.clone())))
                    .collect();
                best.insert(url.to_owned(), (rank, key.clone(), Value::Object(mirror)));
            }
        }
    }
    best.into_iter()
        .map(|(url, (_, k, v))| (url, (k, v)))
        .collect()
}

/// Whether `existing`, a target's entry, gives way to `mirror`: never a home's own sign-in (it
/// holds a refresh token); a stub, an expired entry or an older mirror does.
fn gives_way(existing: &Value, mirror: &Value, now_ms: i64) -> bool {
    if text(existing, "refreshToken").is_some() {
        return false;
    }
    if text(existing, "accessToken").is_none() {
        return true;
    }
    let had = existing.get("expiresAt").and_then(Value::as_i64);
    let new = mirror.get("expiresAt").and_then(Value::as_i64);
    !live(had, now_ms) || new.unwrap_or(i64::MAX) > had.unwrap_or(i64::MAX)
}

/// Puts each of `lendable`'s sign-ins into `target` where the target has none of its own for that
/// server URL: into each of the target's entries for the URL that gives way (under the target's
/// own key and server name, which are what its configuration looks up), or under the source's key
/// when the target has no entry for the URL at all. Every other key of the item is kept. Returns
/// how many entries were written.
pub(crate) fn merge(
    target: &mut Value,
    lendable: &BTreeMap<String, (String, Value)>,
    now_ms: i64,
) -> usize {
    if lendable.is_empty() {
        return 0;
    }
    if !target.is_object() {
        *target = json!({});
    }
    let object = target.as_object_mut().expect("an object");
    let map = object.entry("mcpOAuth").or_insert_with(|| json!({}));
    let Some(map) = map.as_object_mut() else {
        return 0;
    };
    let mut carried = 0;
    for (url, (source_key, mirror)) in lendable {
        let mine: Vec<String> = map
            .iter()
            .filter(|(_, v)| text(v, "serverUrl") == Some(url.as_str()))
            .map(|(k, _)| k.clone())
            .collect();
        if mine.is_empty() {
            map.insert(source_key.clone(), mirror.clone());
            carried += 1;
            continue;
        }
        for key in mine {
            if !gives_way(&map[&key], mirror, now_ms) {
                continue;
            }
            let mut entry = mirror.clone();
            if let Some(name) = map[&key].get("serverName").cloned() {
                entry["serverName"] = name;
            }
            map.insert(key, entry);
            carried += 1;
        }
    }
    carried
}

/// The Keychain service of the credential item of the Claude home at `folder`.
fn service_of(folder: &Path) -> String {
    crate::external::service(&folder.to_string_lossy())
}

/// Per server URL: the homes signed in, and the server's names.
type ByUrl = BTreeMap<String, (Vec<String>, Vec<String>)>;
/// Runs a target's read and write under its credential locks.
pub(crate) type Locked<'a> =
    &'a dyn Fn(&dyn Fn() -> Result<usize, String>) -> Result<usize, String>;

/// The Claude homes on this machine: the ordinary one first (when known), then those Switchboard
/// made under `root`, by kind and id. `claude_accounts`, when given, keeps only the account homes
/// of those ids (a Codex account's home holds no Claude item).
pub(crate) fn homes(
    root: &Path,
    ordinary: Option<(PathBuf, String)>,
    claude_accounts: Option<&[String]>,
) -> Vec<Home> {
    let mut out = vec![];
    if let Some((folder, service)) = ordinary {
        out.push(Home {
            kind: "ordinary",
            id: None,
            folder,
            service,
        });
    }
    let mut add = |kind: &'static str, dir: PathBuf, keep: &dyn Fn(&str) -> Option<String>| {
        let Ok(read) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut found: Vec<(String, PathBuf)> = read
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                keep(&name).map(|id| (id, e.path()))
            })
            .collect();
        found.sort();
        for (id, folder) in found {
            let service = service_of(&folder);
            out.push(Home {
                kind,
                id: Some(id),
                folder,
                service,
            });
        }
    };
    add("account", root.join("homes"), &|name| {
        claude_accounts
            .is_none_or(|ids| ids.iter().any(|i| i == name))
            .then(|| name.to_owned())
    });
    add("group", root.join("runtimes"), &|name| {
        name.strip_prefix("claude-").map(str::to_owned)
    });
    add("shell", root.join(crate::shell::map::DIR), &|name| {
        name.strip_prefix("claude-").map(str::to_owned)
    });
    out
}

fn label(home: &Home) -> String {
    match &home.id {
        Some(id) => format!("{}:{id}", home.kind),
        None => home.kind.to_owned(),
    }
}

/// SB-359: which home is signed in to which MCP server. Names, URLs, booleans and expiry times
/// only, never a token. `findings`: `partial`, a server (by URL) signed in in some homes and not
/// in others whose item was read; `same_url`, two names for one URL in one home.
pub(crate) fn report(homes: &[Home], items: &dyn Items) -> Value {
    let mut rows = vec![];
    // url -> (homes signed in, names)
    let mut by_url: ByUrl = BTreeMap::new();
    let mut read_homes = vec![];
    let mut findings = vec![];
    for home in homes {
        let (state, list) = match items.read(home) {
            Read::Absent => ("absent", vec![]),
            Read::Unknown => ("unknown", vec![]),
            Read::Item(item) => ("present", entries(&item)),
        };
        if state != "unknown" {
            read_homes.push(label(home));
        }
        let mut names_by_url: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for e in &list {
            if e.url.is_empty() {
                continue;
            }
            let names = names_by_url.entry(&e.url).or_default();
            if !names.contains(&e.name.as_str()) {
                names.push(&e.name);
            }
            let row = by_url.entry(e.url.clone()).or_default();
            if !row.1.contains(&e.name) {
                row.1.push(e.name.clone());
            }
            if e.signed_in && !row.0.contains(&label(home)) {
                row.0.push(label(home));
            }
        }
        // A sign-in filed under one key and a stub for the same URL under another: the server's
        // configuration changed since, and Claude Code looks the sign-in up by the new key, so this
        // home reads «Needs authentication» for it.
        for url in names_by_url.keys() {
            let of_url = || list.iter().filter(|e| e.url == *url);
            if of_url().any(|e| e.own) && of_url().any(|e| !e.signed_in) {
                findings.push(json!({"kind": "stale_key", "home": label(home), "url": url}));
            }
        }
        for (url, names) in names_by_url {
            if names.len() > 1 {
                findings.push(
                    json!({"kind": "same_url", "home": label(home), "url": url, "names": names}),
                );
            }
        }
        rows.push(json!({
            "kind": home.kind,
            "id": home.id,
            "folder": home.folder,
            "item": state,
            "servers": list.iter().map(|e| json!({
                "name": e.name,
                "url": e.url,
                "signed_in": e.signed_in,
                "own": e.own,
                "expires_at": e.expires_at,
            })).collect::<Vec<_>>(),
        }));
    }
    let mut servers = vec![];
    for (url, (signed, names)) in &mut by_url {
        let missing: Vec<&String> = read_homes.iter().filter(|h| !signed.contains(h)).collect();
        if !signed.is_empty() && !missing.is_empty() {
            findings.push(json!({
                "kind": "partial",
                "url": url,
                "names": names,
                "signed_in": signed,
                "not_signed_in": missing.len(),
            }));
        }
        servers.push(json!({
            "url": url,
            "names": names,
            "signed_in_homes": signed.len(),
        }));
    }
    json!({
        "homes": rows,
        "servers": servers,
        "findings": findings,
        "read": read_homes.len(),
        "unknown": homes.len() - read_homes.len(),
    })
}

/// SB-358: carries the newest sign-in of each MCP server from the other homes into `target`
/// (see the module comment). `sources` are read through `cache` by stamp, so a launch reads only
/// items written since the last one; `locked` runs the target's read and write under Claude
/// Code's credential locks. Returns how many entries were written; a target whose item would ask
/// for access is skipped.
pub(crate) fn carry(
    target: &Home,
    sources: &[Home],
    items: &dyn Items,
    cache: &Cache,
    now_ms: i64,
    locked: Locked,
) -> Result<usize, String> {
    let mut lent = vec![];
    for home in sources.iter().filter(|h| h.service != target.service) {
        if let Some(item) = cache.own_sign_ins(home, items) {
            lent.push(item);
        }
    }
    let lendable = lendable(&lent, now_ms);
    if lendable.is_empty() {
        return Ok(0);
    }
    locked(&|| {
        let mut item = match items.read(target) {
            Read::Absent => json!({}),
            Read::Item(item) => item,
            Read::Unknown => return Ok(0),
        };
        let carried = merge(&mut item, &lendable, now_ms);
        if carried > 0 {
            items.write(target, &item)?;
        }
        Ok(carried)
    })
}

/// The own sign-ins of each source item, kept by stamp: an item not written since is not read
/// again. Holds tokens in memory only, never written anywhere.
#[derive(Default)]
pub(crate) struct Cache {
    by_service: std::sync::Mutex<BTreeMap<String, (String, Value)>>,
}
impl Cache {
    fn own_sign_ins(&self, home: &Home, items: &dyn Items) -> Option<Value> {
        let stamp = items.stamp(home);
        if let Some(stamp) = &stamp {
            if let Some((seen, value)) = self.by_service.lock().ok()?.get(&home.service) {
                if seen == stamp {
                    return Some(value.clone());
                }
            }
        }
        let Read::Item(item) = items.read(home) else {
            return None;
        };
        // Only what can be lent is kept.
        let own: Map<String, Value> = item
            .get("mcpOAuth")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter(|(_, v)| text(v, "refreshToken").is_some())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let value = json!({ "mcpOAuth": own });
        if let Some(stamp) = stamp {
            if let Ok(mut cache) = self.by_service.lock() {
                cache.insert(home.service.clone(), (stamp, value.clone()));
            }
        }
        Some(value)
    }
}

/// The items on this machine: Keychain through `/usr/bin/security` on macOS, read only when the
/// item's access list trusts that tool (no dialog); the home's `.credentials.json` elsewhere.
pub(crate) struct Native;

#[cfg(target_os = "macos")]
const SECURITY: &str = "/usr/bin/security";

impl Items for Native {
    fn read(&self, home: &Home) -> Read {
        #[cfg(target_os = "macos")]
        {
            use switchboard_core::external_keychain::{probe_external, ItemProbe};
            let Ok(user) = crate::launch::login_user() else {
                return Read::Unknown;
            };
            match probe_external(&home.service, &user, Some(Path::new(SECURITY))) {
                ItemProbe::Absent => Read::Absent,
                ItemProbe::Present {
                    tool_trusted: true, ..
                } => match switchboard_core::security_cli::find(&home.service, &user) {
                    Ok(Some(bytes)) => serde_json::from_slice(&bytes)
                        .map(Read::Item)
                        .unwrap_or(Read::Unknown),
                    Ok(None) => Read::Absent,
                    Err(_) => Read::Unknown,
                },
                _ => Read::Unknown,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            match std::fs::read(home.folder.join(".credentials.json")) {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .map(Read::Item)
                    .unwrap_or(Read::Unknown),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Absent,
                Err(_) => Read::Unknown,
            }
        }
    }
    fn stamp(&self, home: &Home) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            use switchboard_core::external_keychain::{probe_external, ItemProbe};
            let user = crate::launch::login_user().ok()?;
            match probe_external(&home.service, &user, Some(Path::new(SECURITY))) {
                ItemProbe::Present {
                    stamp: (created, modified),
                    tool_trusted: true,
                } => Some(format!("{created}:{modified}")),
                _ => None,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let meta = std::fs::metadata(home.folder.join(".credentials.json")).ok()?;
            Some(format!("{:?}:{}", meta.modified().ok()?, meta.len()))
        }
    }
    fn write(&self, home: &Home, item: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(item).map_err(|_| "MCP sign-in could not be written.")?;
        #[cfg(target_os = "macos")]
        {
            let user = crate::launch::login_user()?;
            switchboard_core::security_cli::add(&home.service, &user, &bytes, true).map(|_| ())
        }
        #[cfg(not(target_os = "macos"))]
        {
            crate::launch::private_write(&home.folder.join(".credentials.json"), &bytes, false)
        }
    }
}

/// The ordinary home and its item's service: `CLAUDE_CONFIG_DIR` when set (as Claude Code names
/// it), else the person's Claude folder under the fixed service.
pub(crate) fn ordinary() -> Option<(PathBuf, String)> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(dir);
        let service = service_of(&dir);
        return Some((dir, service));
    }
    crate::carry::claude_user_dir().map(|dir| (dir, "Claude Code-credentials".to_owned()))
}

/// SB-359: the report on this machine's homes (`switchboard mcp-homes`). Reads every item whose
/// access list trusts `/usr/bin/security`; one that would ask reads `unknown`. Never on a timer.
pub fn report_now(root: &Path, claude_accounts: &[String]) -> Value {
    report(&homes(root, ordinary(), Some(claude_accounts)), &Native)
}

static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();

/// SB-358 at a launch: carries the other homes' sign-ins into `folder`, a Claude home Switchboard
/// made under `root`. Logs counts only. A failure leaves the home as it was and never stops the
/// launch.
pub(crate) fn carry_into(root: &Path, folder: &Path) {
    let all = homes(root, ordinary(), None);
    let service = service_of(folder);
    let target = Home {
        kind: "launch",
        id: None,
        folder: folder.to_owned(),
        service,
    };
    let now_ms = switchboard_proxy::now().saturating_mul(1000);
    // Keychain reads and Claude Code's locks block: off the async worker (SB-23).
    let result = crate::blocking::run(|| {
        carry(
            &target,
            &all,
            &Native,
            CACHE.get_or_init(Cache::default),
            now_ms,
            &|write| crate::external::with_credential_locks(folder, write),
        )
    });
    match result {
        Ok(0) => {}
        Ok(carried) => crate::oplog::event(
            "mcp_sign_ins",
            &[("carried", crate::oplog::Field::Number(carried as i64))],
        ),
        Err(_) => crate::oplog::event(
            "mcp_sign_ins",
            &[("error", crate::oplog::Field::Code("write"))],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory {
        items: RefCell<HashMap<String, Option<Value>>>,
        reads: RefCell<usize>,
        writes: RefCell<usize>,
    }
    impl Items for Memory {
        fn read(&self, home: &Home) -> Read {
            *self.reads.borrow_mut() += 1;
            match self.items.borrow().get(&home.service) {
                None => Read::Absent,
                Some(None) => Read::Unknown,
                Some(Some(v)) => Read::Item(v.clone()),
            }
        }
        fn stamp(&self, home: &Home) -> Option<String> {
            self.items
                .borrow()
                .get(&home.service)
                .map(|v| format!("{v:?}"))
        }
        fn write(&self, home: &Home, item: &Value) -> Result<(), String> {
            *self.writes.borrow_mut() += 1;
            self.items
                .borrow_mut()
                .insert(home.service.clone(), Some(item.clone()));
            Ok(())
        }
    }
    fn home(service: &str) -> Home {
        Home {
            kind: "account",
            id: Some(service.into()),
            folder: PathBuf::from(format!("/synthetic/{service}")),
            service: service.into(),
        }
    }
    const NOW: i64 = 1_800_000_000_000;
    const KEY: &str = "plugin:figma:figma|0123456789abcdef";
    fn own(expires: i64) -> Value {
        json!({"serverName":"plugin:figma:figma","serverUrl":"https://mcp.example/mcp",
            "accessToken":"synthetic-access","refreshToken":"synthetic-refresh-never-carried",
            "clientId":"synthetic-client","clientSecret":"synthetic-secret-never-carried",
            "expiresAt":expires,"scope":"read","discoveryState":{"oauthMetadataFound":true}})
    }
    fn stub() -> Value {
        json!({"serverName":"plugin:figma:figma","serverUrl":"https://mcp.example/mcp",
            "accessToken":"","discoveryState":{"oauthMetadataFound":true}})
    }
    fn locked(write: &dyn Fn() -> Result<usize, String>) -> Result<usize, String> {
        write()
    }

    #[test]
    fn a_mirror_carries_the_access_token_and_never_the_refresh_token_or_secret() {
        let lend = lendable(&[json!({"mcpOAuth": {KEY: own(NOW + 3_600_000)}})], NOW);
        let mut target =
            json!({"claudeAiOauth": {"accessToken": "kept"}, "mcpOAuth": {KEY: stub()}});
        assert_eq!(merge(&mut target, &lend, NOW), 1);
        let entry = &target["mcpOAuth"][KEY];
        assert_eq!(entry["accessToken"], "synthetic-access");
        assert_eq!(entry["clientId"], "synthetic-client");
        assert!(entry.get("refreshToken").is_none());
        assert!(entry.get("clientSecret").is_none());
        assert_eq!(
            target["claudeAiOauth"]["accessToken"], "kept",
            "other keys kept"
        );
        assert!(!target.to_string().contains("never-carried"));
    }

    #[test]
    fn a_homes_own_sign_in_is_never_overwritten_and_the_newest_source_wins() {
        let older = json!({"mcpOAuth": {KEY: own(NOW + 3_600_000)}});
        let newer = json!({"mcpOAuth": {KEY: own(NOW + 7_200_000)}});
        let lend = lendable(&[older, newer], NOW);
        assert_eq!(
            lend["https://mcp.example/mcp"].1["expiresAt"],
            NOW + 7_200_000
        );
        let mut target = json!({"mcpOAuth": {KEY: own(NOW + 60_000_000)}});
        let before = target.clone();
        assert_eq!(merge(&mut target, &lend, NOW), 0);
        assert_eq!(target, before);
        // An older mirror is replaced by a newer one; the same one is not written again.
        let mut mirror = json!({});
        assert_eq!(merge(&mut mirror, &lend, NOW), 1);
        assert_eq!(merge(&mut mirror, &lend, NOW), 0);
    }

    /// The measured case: the sign-in was filed under the configuration's old key, the current
    /// configuration's stub sits under another key with the same URL; the mirror goes under the
    /// stub's key and name, which is what the target's Claude Code looks up.
    #[test]
    fn a_mirror_fills_the_targets_own_key_for_the_same_url() {
        let lend = lendable(&[json!({"mcpOAuth": {KEY: own(NOW + 3_600_000)}})], NOW);
        let current = "plugin:marketing:figma|aaaaaaaaaaaaaaaa";
        let mut target = json!({"mcpOAuth": {current: {
            "serverName": "plugin:marketing:figma", "serverUrl": "https://mcp.example/mcp", "accessToken": ""
        }}});
        assert_eq!(merge(&mut target, &lend, NOW), 1);
        let map = target["mcpOAuth"].as_object().unwrap();
        assert_eq!(map.len(), 1, "no second entry under the source's key");
        assert_eq!(map[current]["accessToken"], "synthetic-access");
        assert_eq!(map[current]["serverName"], "plugin:marketing:figma");
        // Another server's URL is left alone.
        let mut other = json!({"mcpOAuth": {"x|1": {"serverUrl": "https://other.example/mcp", "accessToken": ""}}});
        assert_eq!(merge(&mut other, &lend, NOW), 1);
        assert_eq!(other["mcpOAuth"]["x|1"]["accessToken"], "");
        assert_eq!(other["mcpOAuth"][KEY]["accessToken"], "synthetic-access");
    }

    #[test]
    fn an_expired_or_tokenless_source_lends_nothing() {
        let lend = lendable(
            &[
                json!({"mcpOAuth": {KEY: own(NOW + 60_000)}}),
                json!({"mcpOAuth": {KEY: stub()}}),
                json!({"mcpOAuth": "not an object"}),
            ],
            NOW,
        );
        assert!(lend.is_empty());
    }

    #[test]
    fn a_launch_carries_into_a_stub_and_skips_a_home_that_would_ask() {
        let items = Memory::default();
        items.items.borrow_mut().insert(
            "ordinary".into(),
            Some(json!({"mcpOAuth": {KEY: own(NOW + 3_600_000)}})),
        );
        items
            .items
            .borrow_mut()
            .insert("a".into(), Some(json!({"mcpOAuth": {KEY: stub()}})));
        items.items.borrow_mut().insert("asks".into(), None);
        let all = [home("ordinary"), home("a"), home("asks"), home("new")];
        let cache = Cache::default();
        assert_eq!(carry(&all[1], &all, &items, &cache, NOW, &locked), Ok(1));
        assert_eq!(carry(&all[2], &all, &items, &cache, NOW, &locked), Ok(0));
        assert_eq!(carry(&all[3], &all, &items, &cache, NOW, &locked), Ok(1));
        assert!(items.items.borrow()["asks"].is_none(), "never written");
        // The source is read once; later launches use its stamp.
        let reads = *items.reads.borrow();
        assert_eq!(carry(&all[1], &all, &items, &cache, NOW, &locked), Ok(0));
        // Only the targets are read again (and "a" is now a source whose stamp changed).
        assert!(*items.reads.borrow() - reads <= 3);
        // A mirror is not lent onwards: only own sign-ins are.
        let a = match items.read(&all[1]) {
            Read::Item(v) => v,
            _ => unreachable!(),
        };
        assert!(lendable(&[a], NOW).is_empty());
    }

    #[test]
    fn the_report_names_partial_sign_ins_and_two_names_for_one_url_without_tokens() {
        let items = Memory::default();
        items.items.borrow_mut().insert(
            "ordinary".into(),
            Some(json!({"mcpOAuth": {
                KEY: own(NOW + 3_600_000),
                "figma|fedcba9876543210": {"serverName":"figma","serverUrl":"https://mcp.example/mcp","accessToken":""}
            }})),
        );
        items
            .items
            .borrow_mut()
            .insert("a".into(), Some(json!({"mcpOAuth": {KEY: stub()}})));
        items.items.borrow_mut().insert("asks".into(), None);
        let all = [home("ordinary"), home("a"), home("asks"), home("b")];
        let report = report(&all, &items);
        let text = report.to_string();
        assert!(!text.contains("synthetic-access") && !text.contains("synthetic-refresh"));
        assert_eq!(report["read"], 3);
        assert_eq!(report["unknown"], 1);
        let kinds: Vec<&str> = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["stale_key", "same_url", "partial"]);
        let partial = &report["findings"][2];
        assert_eq!(partial["signed_in"], json!(["account:ordinary"]));
        assert_eq!(partial["not_signed_in"], 2);
        assert_eq!(report["homes"][1]["servers"][0]["signed_in"], false);
        assert_eq!(report["homes"][2]["item"], "unknown");
        assert_eq!(report["homes"][3]["item"], "absent");
    }

    #[test]
    fn homes_lists_the_ordinary_one_then_switchboards_claude_homes() {
        let root = tempfile::tempdir().unwrap();
        for (parent, dir) in [
            ("homes", "acc-1"),
            ("homes", "acc-2"),
            ("runtimes", "claude-agents"),
            ("runtimes", "codex-default"),
            ("shell", "claude-org"),
            ("shell", "codex-org"),
        ] {
            std::fs::create_dir_all(root.path().join(parent).join(dir)).unwrap();
        }
        let found = homes(
            root.path(),
            Some((
                PathBuf::from("/synthetic/.claude"),
                "Claude Code-credentials".into(),
            )),
            Some(&["acc-2".to_owned()]),
        );
        let labels: Vec<String> = found.iter().map(label).collect();
        assert_eq!(
            labels,
            ["ordinary", "account:acc-2", "group:agents", "shell:org"]
        );
        assert_eq!(
            found[1].service,
            // The path a launch names the home by (`root.join("homes").join(id)`), on every platform.
            service_of(&root.path().join("homes").join("acc-2"))
        );
    }
}
