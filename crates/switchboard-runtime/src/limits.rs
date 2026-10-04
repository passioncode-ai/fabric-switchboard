//! Provider limit errors as rotation evidence (0.5). A seat's individual spend limit or a
//! per-account rate limit answers 429 while the quota endpoint still reports spare capacity;
//! Claude Swap cannot see it. Two sources: the proxy's own `request rate_limited` events for
//! managed sessions, and the API-error markers Claude Code writes into its session transcripts
//! for ordinary sessions. Deserialization keeps only flagged API-error markers and timestamp
//! metadata for session attribution; conversation content is never materialized, kept or logged.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};
use switchboard_core::{ExternalIdentity, Snapshot};

/// An error older than this says nothing about the account now.
pub(crate) const WINDOW_SECONDS: i64 = 900;
/// Heuristic attribution grace: errors shortly after a switch may belong to the previous
/// account's session. Actual running-session adoption timing remains unverified (SB-06).
pub(crate) const SWITCH_GRACE_SECONDS: i64 = 60;
/// A limit with no reported reset holds for this long, then the account may be tried again.
const DEFAULT_HOLD_SECONDS: i64 = 900;
const MAX_HOLD_SECONDS: i64 = 7 * 86_400;
const MAX_FILES: usize = 64;
const MANAGED_REPEAT_SECONDS: i64 = 300;
const MANAGED_ECHO_SECONDS: i64 = 10;
const TAIL_BYTES: u64 = 256 * 1024;

/// What a limit says about the account (SB-41). `Quota` when Claude Code's marker names one of the
/// subscription windows in `quotaLimits.rateLimitType`; `Unknown` otherwise — a managed 429, a
/// marker without that field, or one naming anything else can be a rate, quota or spend limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LimitKind {
    Quota,
    Unknown,
}
/// How the account was tied to the limit. `Attributed`: the proxy served the request with that
/// account. `Inferred`: a transcript marker charged by timing and session binding — a heuristic
/// that cannot authorise anything beyond holding the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Confidence {
    Attributed,
    Inferred,
}

/// One account's limit evidence (SB-41, SES-B). `resets_at` is the provider's own time only;
/// `until` is when the account may be tried again — the reset, or the fallback hold. No provider
/// text, prompt or credential is kept. Scope (account, organisation, group) is reported as
/// unknown: no field available today states it, and a shared budget must not be ruled out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limit {
    pub event_id: String,
    pub source: String,
    /// The Claude Code session (transcript) it came from; none for a managed request.
    pub session: Option<String>,
    pub observed_at: i64,
    pub kind: LimitKind,
    /// Claude Code's own `quotaLimits.rateLimitType`, allowlisted (`five_hour`, `seven_day`…).
    #[serde(default)]
    pub limit_type: Option<String>,
    pub resets_at: Option<i64>,
    pub until: i64,
    pub confidence: Confidence,
}

/// Session → the accounts its limits were charged to, and when it was last seen failing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    accounts: Vec<String>,
    seen: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceFile {
    version: u32,
    limited: HashMap<String, Limit>,
    sessions: HashMap<String, Binding>,
}

pub(crate) const EVIDENCE_FILE: &str = "limit-evidence.json";
const MAX_ENTRIES: usize = 256;
/// A session's binding outlives its last marker by this long; transcripts older than the scan
/// window are not read, so nothing can consult it later.
const BINDING_SECONDS: i64 = 86_400;

#[derive(Default)]
pub(crate) struct LimitState {
    limited: Mutex<HashMap<String, Limit>>,
    sessions: Mutex<HashMap<String, Binding>>,
    /// The ordinary Claude Code identity last seen, and since when errors count against it.
    current: Mutex<Option<(String, i64)>>,
    /// Where evidence is kept across restarts (SES-06); every owner sets it, a bare state
    /// (tests) has none.
    path: Mutex<Option<PathBuf>>,
}

/// A marker as found: its session, when that session began, when it was written, the hold it
/// implies and whether the provider reported the reset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Marker {
    pub session: String,
    pub began: Option<i64>,
    pub at: i64,
    pub hold: i64,
    pub reported: bool,
    /// The provider's reset, as reported, even beyond the seven-day hold bound.
    pub reset: Option<i64>,
    pub limit_type: Option<String>,
}

impl LimitState {
    /// Evidence and session bindings kept at `path`, those still meaningful at `now` loaded.
    pub(crate) fn kept_in(path: PathBuf, now: i64) -> Self {
        let state = Self::default();
        if let Ok(bytes) = switchboard_core::private_fs::read_private(&path, 512 * 1024) {
            if let Ok(file) = serde_json::from_slice::<EvidenceFile>(&bytes) {
                if file.version == 1 {
                    if let Ok(mut limited) = state.limited.lock() {
                        let mut kept: Vec<_> = file
                            .limited
                            .into_iter()
                            .filter(|(_, l)| sane(l, now))
                            .collect();
                        // The latest holds win a table over its bound.
                        kept.sort_by_key(|(_, l)| std::cmp::Reverse(l.until));
                        kept.truncate(MAX_ENTRIES);
                        *limited = kept.into_iter().collect();
                    }
                    if let Ok(mut sessions) = state.sessions.lock() {
                        *sessions = file
                            .sessions
                            .into_iter()
                            .filter(|(_, b)| b.seen > now - BINDING_SECONDS && b.seen <= now + 60)
                            .collect();
                    }
                }
            }
        }
        if let Ok(mut kept) = state.path.lock() {
            *kept = Some(path);
        }
        state
    }
    fn persist(&self) {
        let Some(path) = self.path.lock().ok().and_then(|p| p.clone()) else {
            return;
        };
        let (Ok(limited), Ok(sessions)) = (self.limited.lock(), self.sessions.lock()) else {
            return;
        };
        let file = EvidenceFile {
            version: 1,
            limited: limited.clone(),
            sessions: sessions.clone(),
        };
        drop((limited, sessions));
        if let Ok(bytes) = serde_json::to_vec(&file) {
            let _ = switchboard_core::private_fs::private_write(&path, &bytes);
        }
    }
    pub(crate) fn limited_ids(&self, now: i64) -> HashSet<String> {
        self.limited
            .lock()
            .map(|l| {
                l.iter()
                    .filter(|(_, limit)| limit.until > now)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(crate) fn report(&self, now: i64) -> Vec<Value> {
        let Ok(limited) = self.limited.lock() else {
            return vec![];
        };
        let mut rows: Vec<_> = limited
            .iter()
            .filter(|(_, l)| l.until > now)
            .map(|(id, l)| {
                serde_json::json!({"account_id": id, "until": l.until, "source": l.source,
                    "kind": l.kind, "limit_type": l.limit_type, "resets_at": l.resets_at, "confidence": l.confidence,
                    "scope": "unknown", "observed_at": l.observed_at, "event_id": l.event_id})
            })
            .collect();
        rows.sort_by(|a, b| a["account_id"].as_str().cmp(&b["account_id"].as_str()));
        rows
    }
    /// Keeps the later hold per account. True if anything changed.
    fn mark(&self, id: &str, limit: Limit) -> bool {
        let Ok(mut limited) = self.limited.lock() else {
            return false;
        };
        if limited.get(id).is_some_and(|old| old.until >= limit.until) {
            return false;
        }
        // Full (accounts removed while held): the hold ending soonest makes room. A new limit is
        // never dropped — an exhausted account must not look free.
        if limited.len() >= MAX_ENTRIES && !limited.contains_key(id) {
            if let Some(soonest) = limited
                .iter()
                .min_by_key(|(_, l)| l.until)
                .map(|(k, _)| k.clone())
            {
                limited.remove(&soonest);
            }
        }
        limited.insert(id.to_owned(), limit);
        true
    }
    fn bind(&self, session: &str, accounts: &[String], seen: i64) -> bool {
        let Ok(mut sessions) = self.sessions.lock() else {
            return false;
        };
        if sessions.len() >= MAX_ENTRIES && !sessions.contains_key(session) {
            if let Some(stalest) = sessions
                .iter()
                .min_by_key(|(_, b)| b.seen)
                .map(|(k, _)| k.clone())
            {
                sessions.remove(&stalest);
            }
        }
        match sessions.get_mut(session) {
            None => {
                sessions.insert(
                    session.to_owned(),
                    Binding {
                        accounts: accounts.to_vec(),
                        seen,
                    },
                );
                true
            }
            Some(binding) => {
                // A session that adopted another sign-in now charges that account.
                let moved = binding.accounts != accounts;
                if moved {
                    binding.accounts = accounts.to_vec();
                }
                let newer = binding.seen < seen;
                binding.seen = binding.seen.max(seen);
                moved || newer
            }
        }
    }
    fn bound(&self, session: &str) -> Option<Vec<String>> {
        self.sessions
            .lock()
            .ok()?
            .get(session)
            .map(|b| b.accounts.clone())
    }
    /// Forgets holds of accounts the store no longer has, and drops them from session bindings.
    pub(crate) fn forget_missing<'a>(&self, accounts: impl IntoIterator<Item = &'a str>) {
        let present: HashSet<&str> = accounts.into_iter().collect();
        let mut changed = false;
        if let Ok(mut limited) = self.limited.lock() {
            let before = limited.len();
            limited.retain(|id, _| present.contains(id.as_str()));
            changed |= limited.len() != before;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            for binding in sessions.values_mut() {
                let before = binding.accounts.len();
                binding.accounts.retain(|id| present.contains(id.as_str()));
                changed |= binding.accounts.len() != before;
            }
            sessions.retain(|_, b| !b.accounts.is_empty());
        }
        if changed {
            self.persist();
        }
    }
    /// A switch Switchboard made: the new account starts with a clean slate after the grace.
    pub(crate) fn switched(&self, identity: Option<&ExternalIdentity>, now: i64) {
        if let (Some(id), Ok(mut current)) = (
            identity.and_then(|i| i.account_id.clone()),
            self.current.lock(),
        ) {
            *current = Some((id, now + SWITCH_GRACE_SECONDS));
        }
    }
    /// Errors count against `identity` from the returned time on. A newly seen identity starts
    /// a grace period. At startup the journal says when Switchboard last switched to this
    /// account; without such an entry the whole window counts.
    fn since(&self, identity: &str, ids: &[String], snapshot: &Snapshot, now: i64) -> i64 {
        let Ok(mut current) = self.current.lock() else {
            return now;
        };
        let since = match current.as_ref() {
            Some((id, since)) if id == identity => return *since,
            Some(_) => now + SWITCH_GRACE_SECONDS,
            None => snapshot
                .events
                .iter()
                .filter(|e| {
                    e.account_id.as_ref().is_some_and(|id| ids.contains(id))
                        && ((e.action == "activation" && e.detail == "completed")
                            || (e.action == "rotation" && e.detail == "switched"))
                })
                .map(|e| e.at + SWITCH_GRACE_SECONDS)
                .max()
                .unwrap_or(now - WINDOW_SECONDS),
        };
        *current = Some((identity.to_owned(), since));
        since
    }

    /// One pass: drop expired limits, then add what the proxy and Claude Code report.
    pub(crate) fn refresh(
        &self,
        snapshot: &Snapshot,
        native: Option<(&ExternalIdentity, Vec<String>)>,
        transcripts: &[PathBuf],
        now: i64,
    ) {
        let mut changed = false;
        if let Ok(mut limited) = self.limited.lock() {
            let before = limited.len();
            limited.retain(|_, l| l.until > now);
            changed |= limited.len() != before;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            let before = sessions.len();
            sessions.retain(|_, b| b.seen > now - BINDING_SECONDS);
            changed |= sessions.len() != before;
        }
        // A single 429 can be a short burst; two within five minutes is a limit.
        let managed: Vec<_> = snapshot
            .events
            .iter()
            .filter(|e| {
                e.action == "request" && e.detail == "rate_limited" && now - e.at <= WINDOW_SECONDS
            })
            .collect();
        for (index, event) in managed.iter().enumerate() {
            let Some(id) = &event.account_id else {
                continue;
            };
            let repeated = managed.iter().enumerate().any(|(other, e)| {
                other != index
                    && e.account_id.as_ref() == Some(id)
                    && (e.at - event.at).abs() <= MANAGED_REPEAT_SECONDS
            });
            if repeated {
                changed |= self.mark(
                    id,
                    Limit {
                        event_id: format!("managed:{id}:{}", event.at),
                        source: "managed".into(),
                        session: None,
                        observed_at: event.at,
                        kind: LimitKind::Unknown,
                        limit_type: None,
                        resets_at: None,
                        until: event.at + DEFAULT_HOLD_SECONDS,
                        confidence: Confidence::Attributed,
                    },
                );
            }
        }
        if let Some((identity, ids)) = native {
            if let Some(account) = identity.account_id.as_deref() {
                changed |= self.native(snapshot, account, &ids, &managed, transcripts, now);
            }
        }
        if changed {
            self.persist();
        }
    }

    /// Charges Claude Code's own markers. A session's marker that continues a limit already held
    /// on the accounts the session was charged to (same reset, or no reset while that hold
    /// lasts) stays there. Otherwise: a session that began before the switch and reports another
    /// account's reset belongs to that account (sessions of one account share its reset; distinct
    /// accounts only sometimes do); one that began after the switch — or reports a new reset —
    /// is the current account's, equal reset or not (SES-04). Whether a running session adopts a
    /// switch is not assumed (SB-06): a new limit after it holds the account in use.
    fn native(
        &self,
        snapshot: &Snapshot,
        account: &str,
        ids: &[String],
        managed: &[&switchboard_core::Event],
        transcripts: &[PathBuf],
        now: i64,
    ) -> bool {
        let since = self
            .since(account, ids, snapshot, now)
            .max(now - WINDOW_SECONDS);
        let foreign: Vec<(String, i64)> = self
            .limited
            .lock()
            .map(|l| {
                l.iter()
                    .filter(|(id, limit)| !ids.contains(id) && limit.source == "claude_code")
                    .map(|(id, limit)| (id.clone(), limit.until))
                    .collect()
            })
            .unwrap_or_default();
        let mut changed = false;
        for found in markers(transcripts, since, now) {
            // A session pointed at the proxy writes its 429 to these transcripts too.
            if managed
                .iter()
                .any(|e| (e.at - found.at).abs() <= MANAGED_ECHO_SECONDS)
            {
                continue;
            }
            let old = found.began.is_none_or(|b| b < since - SWITCH_GRACE_SECONDS);
            // A bound session's marker belongs to its accounts only while it continues a limit
            // already held there: the same reported reset, or — without a reset — a hold still
            // in force. A new reset is a new limit, possibly on a sign-in the session adopted
            // since (SB-06 is open), and is attributed like any marker.
            let continued = self.bound(&found.session).filter(|accounts| {
                let Ok(limited) = self.limited.lock() else {
                    return false;
                };
                accounts.iter().any(|id| {
                    limited.get(id).is_some_and(|held| {
                        if found.reported {
                            held.until == found.hold
                        } else {
                            held.until > now
                        }
                    })
                })
            });
            let charged: Vec<String> = match continued {
                Some(accounts) => accounts,
                None => {
                    let owners: Vec<String> = foreign
                        .iter()
                        .filter(|(_, until)| found.reported && *until == found.hold)
                        .map(|(id, _)| id.clone())
                        .collect();
                    if old && !owners.is_empty() {
                        owners
                    } else if old && !found.reported {
                        // Nothing in the marker tells the previous account from this one.
                        continue;
                    } else {
                        ids.to_vec()
                    }
                }
            };
            changed |= self.bind(&found.session, &charged, found.at);
            for id in &charged {
                changed |= self.mark(
                    id,
                    Limit {
                        event_id: format!("native:{}:{}", found.session, found.at),
                        source: "claude_code".into(),
                        session: Some(found.session.clone()),
                        observed_at: found.at,
                        kind: if found.limit_type.as_deref().is_some_and(quota_window) {
                            LimitKind::Quota
                        } else {
                            LimitKind::Unknown
                        },
                        limit_type: found.limit_type.clone(),
                        resets_at: found.reset,
                        until: found.hold,
                        confidence: Confidence::Inferred,
                    },
                );
            }
        }
        changed
    }
}

/// Evidence read back from disk that can still apply and is internally consistent.
fn sane(limit: &Limit, now: i64) -> bool {
    limit.until > now
        && limit.until <= now.saturating_add(MAX_HOLD_SECONDS)
        && limit.observed_at <= now + 60
        && limit.resets_at.is_none_or(|r| r >= limit.until)
        && limit
            .limit_type
            .as_deref()
            .is_none_or(|t| limit_type(t).as_deref() == Some(t))
        && limit.event_id.len() <= 160
        && limit.session.as_ref().is_none_or(|s| s.len() <= 64)
        && matches!(limit.source.as_str(), "managed" | "claude_code")
}

/// A transcript's session id: its file stem when it is a plain id, else a short digest — never
/// a path.
fn session_id(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !stem.is_empty()
        && stem.len() <= 64
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        stem.to_owned()
    } else {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()))[..32].to_owned()
    }
}

/// Transcript files of ordinary Claude Code sessions changed within the window, newest first.
pub(crate) fn transcript_files(now: i64) -> Vec<PathBuf> {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")));
    let Some(root) = root else {
        return vec![];
    };
    recent_files(&root.join("projects"), now)
}
fn recent_files(projects: &Path, now: i64) -> Vec<PathBuf> {
    let mut found: Vec<(i64, PathBuf)> = Vec::new();
    let Ok(dirs) = fs::read_dir(projects) else {
        return vec![];
    };
    for dir in dirs.flatten() {
        let Ok(files) = fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if meta.is_file() && now - modified <= WINDOW_SECONDS {
                found.push((modified, path));
            }
        }
    }
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().take(MAX_FILES).map(|(_, p)| p).collect()
}

/// The latest hold implied by a rate-limit marker at or after `since`, or None — with the
/// old-session rule for markers without a reset.
#[cfg(test)]
pub(crate) fn latest_limit(files: &[PathBuf], since: i64, now: i64) -> Option<i64> {
    markers(files, since, now)
        .into_iter()
        .filter(|m| m.reported || m.began.is_none_or(|b| b >= since - SWITCH_GRACE_SECONDS))
        .map(|m| m.hold)
        .max()
}
/// Every rate-limit marker at or after `since`, with its session. Attribution — which account
/// a marker is charged to — is `LimitState::native`'s.
pub(crate) fn markers(files: &[PathBuf], since: i64, now: i64) -> Vec<Marker> {
    let mut found = Vec::new();
    for path in files {
        let Some(tail) = tail(path) else {
            continue;
        };
        let session = session_id(path);
        let began = session_start(path);
        for line in tail.split(|b| *b == b'\n') {
            // Cheap byte checks first: a line that is not an API error is never parsed.
            if !contains(line, b"\"isApiErrorMessage\":true") || !contains(line, b"rate_limit") {
                continue;
            }
            if let Some((at, reset, limit_type)) = marker(line, since, now) {
                // Bounded from the marker's own time, so the hold is the same on every pass.
                let hold = reset
                    .unwrap_or(at + DEFAULT_HOLD_SECONDS)
                    .min(at.saturating_add(MAX_HOLD_SECONDS));
                found.push(Marker {
                    session: session.clone(),
                    began,
                    at,
                    hold,
                    reported: reset.is_some(),
                    reset,
                    limit_type,
                });
            }
        }
    }
    found
}
/// The time of the first timestamped entry of a session transcript (its first 64 KiB).
fn session_start(path: &Path) -> Option<i64> {
    // Unknown fields use serde's IgnoredAny visitor rather than allocating a Value tree.
    // An ordinary prompt is opaque; only its timestamp participates in attribution.
    #[derive(serde::Deserialize)]
    struct Metadata {
        timestamp: String,
    }
    let mut head = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    head.split(|b| *b == b'\n').find_map(|line| {
        let metadata: Metadata = serde_json::from_slice(line).ok()?;
        time::OffsetDateTime::parse(
            &metadata.timestamp,
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
        .map(|t| t.unix_timestamp())
    })
}
/// A marker's time, the provider's reported reset (if still ahead) and its allowlisted limit
/// type. Only flagged API-error lines reach this.
fn marker(line: &[u8], since: i64, now: i64) -> Option<(i64, Option<i64>, Option<String>)> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let limited = value.get("error").and_then(Value::as_str) == Some("rate_limit")
        || value.get("apiErrorStatus").and_then(Value::as_i64) == Some(429);
    if !limited {
        return None;
    }
    let at = time::OffsetDateTime::parse(
        value.get("timestamp")?.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?
    .unix_timestamp();
    if at < since || at > now + 60 {
        return None;
    }
    let reset = value
        .pointer("/quotaLimits/resetsAt")
        .and_then(Value::as_i64)
        .filter(|t| *t > now && *t <= 253_402_300_799);
    let limit_type = value
        .pointer("/quotaLimits/rateLimitType")
        .and_then(Value::as_str)
        .and_then(limit_type);
    Some((at, reset, limit_type))
}
/// A limit type kept only when it is a short `[a-z0-9_]` name; anything else is dropped.
fn limit_type(raw: &str) -> Option<String> {
    (!raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
    .then(|| raw.to_owned())
}
/// Claude's subscription windows: a limit naming one is a quota limit.
fn quota_window(limit_type: &str) -> bool {
    limit_type == "five_hour" || limit_type == "seven_day" || limit_type.starts_with("seven_day_")
}
fn tail(path: &Path) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    if start > 0 {
        // Drop the partial first line.
        let cut = bytes.iter().position(|b| *b == b'\n')? + 1;
        bytes.drain(..cut);
    }
    Some(bytes)
}
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::Event;

    fn rfc(t: i64) -> String {
        time::OffsetDateTime::from_unix_timestamp(t)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    }
    fn error_line(at: i64, reset: Option<i64>) -> String {
        let mut v = serde_json::json!({"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","apiErrorStatus":429,"timestamp":rfc(at),"message":{"content":[{"type":"text","text":"You've hit your individual spend limit"}]}});
        if let Some(r) = reset {
            v["quotaLimits"] =
                serde_json::json!({"status":"rejected","resetsAt":r,"rateLimitType":"five_hour"});
        }
        serde_json::to_string(&v).unwrap()
    }
    fn transcript(dir: &Path, lines: &[String]) -> PathBuf {
        let project = dir.join("projects").join("-synthetic-project");
        fs::create_dir_all(&project).unwrap();
        let path = project.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
        fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }
    fn identity(id: &str) -> ExternalIdentity {
        ExternalIdentity {
            account_id: Some(id.into()),
            organization_id: None,
            email: None,
        }
    }
    const NOW: i64 = 2_000_000_000;

    #[test]
    fn a_spend_limit_marker_holds_until_its_reset() {
        let temp = tempfile::tempdir().unwrap();
        let ordinary = r#"{"type":"user","message":{"content":"rate_limit is a word in a prompt, not an error"}}"#;
        let path = transcript(
            temp.path(),
            &[ordinary.into(), error_line(NOW - 120, Some(NOW + 3600))],
        );
        assert_eq!(
            latest_limit(std::slice::from_ref(&path), NOW - 900, NOW),
            Some(NOW + 3600)
        );
        // An error before the account became current is not evidence about it.
        assert_eq!(latest_limit(&[path], NOW - 60, NOW), None);
    }
    #[test]
    fn ordinary_lines_mentioning_limits_are_never_markers() {
        let temp = tempfile::tempdir().unwrap();
        let path = transcript(
            temp.path(),
            &[
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"\"isApiErrorMessage\":true rate_limit"}]},"timestamp":"2033-05-18T03:31:00Z"}"#.into(),
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"server_error","apiErrorStatus":529,"timestamp":"2033-05-18T03:31:00Z"}"#.into(),
            ],
        );
        assert_eq!(latest_limit(&[path], NOW - 900, NOW), None);
    }
    #[test]
    fn a_marker_without_reset_holds_fifteen_minutes() {
        let temp = tempfile::tempdir().unwrap();
        let path = transcript(temp.path(), &[error_line(NOW - 30, None)]);
        assert_eq!(
            latest_limit(&[path], NOW - 900, NOW),
            Some(NOW - 30 + DEFAULT_HOLD_SECONDS)
        );
    }
    #[test]
    fn errors_count_against_the_current_identity_only_after_the_switch_grace() {
        let temp = tempfile::tempdir().unwrap();
        let path = transcript(temp.path(), &[error_line(NOW - 120, Some(NOW + 600))]);
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        // Startup: the error two minutes ago counts against the account in use.
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            std::slice::from_ref(&path),
            NOW,
        );
        assert_eq!(state.limited_ids(NOW), HashSet::from(["id-a".to_string()]));
        // Switchboard switches to b: the same old error must not stop b.
        state.switched(Some(&identity("b")), NOW);
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[path],
            NOW + 5,
        );
        assert!(!state.limited_ids(NOW + 5).contains("id-b"));
        // a stays limited until its reset, then frees itself.
        assert!(state.limited_ids(NOW + 5).contains("id-a"));
        assert!(state.limited_ids(NOW + 601).is_empty());
    }
    #[test]
    fn an_identity_changed_outside_switchboard_also_gets_the_grace() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            &[],
            NOW,
        );
        // `claude /login` as b; its old transcript error predates the change.
        let path = transcript(temp.path(), &[error_line(NOW + 30, Some(NOW + 600))]);
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[path],
            NOW + 40,
        );
        assert!(state.limited_ids(NOW + 40).is_empty());
    }
    #[test]
    fn managed_rate_limit_events_mark_their_account() {
        let state = LimitState::default();
        let snapshot = Snapshot {
            events: vec![
                Event {
                    at: NOW - 60,
                    action: "request".into(),
                    account_id: Some("id-m".into()),
                    detail: "rate_limited".into(),
                },
                Event {
                    at: NOW - 30,
                    action: "request".into(),
                    account_id: Some("id-m".into()),
                    detail: "rate_limited".into(),
                },
                Event {
                    at: NOW - 40,
                    action: "request".into(),
                    account_id: Some("id-burst".into()),
                    detail: "rate_limited".into(),
                },
                Event {
                    at: NOW - 2000,
                    action: "request".into(),
                    account_id: Some("id-old".into()),
                    detail: "rate_limited".into(),
                },
                Event {
                    at: NOW - 10,
                    action: "request".into(),
                    account_id: Some("id-ok".into()),
                    detail: "success".into(),
                },
            ],
            ..Snapshot::default()
        };
        state.refresh(&snapshot, None, &[], NOW);
        assert_eq!(state.limited_ids(NOW), HashSet::from(["id-m".to_string()]));
        assert_eq!(state.report(NOW)[0]["source"], "managed");
    }
    fn event(at: i64, action: &str, id: &str, detail: &str) -> Event {
        Event {
            at,
            action: action.into(),
            account_id: Some(id.into()),
            detail: detail.into(),
        }
    }
    #[test]
    fn after_a_restart_the_journal_dates_the_last_switch() {
        let temp = tempfile::tempdir().unwrap();
        // A was limited and Switchboard switched to B 120 s ago; the app restarts now.
        let path = transcript(temp.path(), &[error_line(NOW - 300, Some(NOW + 3600))]);
        let snapshot = Snapshot {
            events: vec![event(NOW - 120, "activation", "id-b", "completed")],
            ..Snapshot::default()
        };
        let state = LimitState::default();
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[path],
            NOW,
        );
        assert!(
            state.limited_ids(NOW).is_empty(),
            "A's marker is not charged to B"
        );
    }
    #[test]
    fn a_marker_with_another_accounts_reset_is_not_charged_to_the_new_one() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        let first = transcript(temp.path(), &[error_line(NOW - 100, Some(NOW + 3600))]);
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            &[first],
            NOW,
        );
        state.switched(Some(&identity("b")), NOW);
        // An old session still on A — opened before the switch, never charged before — keeps
        // failing after the grace, with A's reset.
        let opened = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW - 3600));
        let late = transcript(
            temp.path(),
            &[opened, error_line(NOW + 120, Some(NOW + 3600))],
        );
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            std::slice::from_ref(&late),
            NOW + 130,
        );
        assert!(!state.limited_ids(NOW + 130).contains("id-b"));
        // A new limit on B, with its own reset, does count.
        let own = transcript(temp.path(), &[error_line(NOW + 140, Some(NOW + 7200))]);
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[late, own],
            NOW + 150,
        );
        assert!(state.limited_ids(NOW + 150).contains("id-b"));
    }
    #[test]
    fn a_proxy_session_429_is_not_charged_to_the_native_account() {
        let temp = tempfile::tempdir().unwrap();
        let path = transcript(temp.path(), &[error_line(NOW - 60, None)]);
        let snapshot = Snapshot {
            events: vec![event(NOW - 58, "request", "id-m", "rate_limited")],
            ..Snapshot::default()
        };
        let state = LimitState::default();
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            &[path],
            NOW,
        );
        assert!(!state.limited_ids(NOW).contains("id-a"));
    }
    #[test]
    fn an_old_sessions_marker_without_a_reset_is_not_charged_to_the_new_account() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            &[],
            NOW,
        );
        state.switched(Some(&identity("b")), NOW);
        let opened = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW - 3600));
        // A session opened an hour before the switch keeps failing without a reset.
        let old = transcript(temp.path(), &[opened, error_line(NOW + 120, None)]);
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[old],
            NOW + 130,
        );
        assert!(state.limited_ids(NOW + 130).is_empty(), "no cascade onto b");
        // A session started after the switch is b's own evidence.
        let fresh_open = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW + 90));
        let new = transcript(temp.path(), &[fresh_open, error_line(NOW + 140, None)]);
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[new],
            NOW + 150,
        );
        assert!(state.limited_ids(NOW + 150).contains("id-b"));
    }
    #[test]
    fn opaque_conversation_content_cannot_hide_an_old_sessions_start() {
        let temp = tempfile::tempdir().unwrap();
        // This valid JSON number cannot be represented by serde_json::Value.
        // Session attribution needs only the timestamp, not conversation values.
        let opened = format!(
            r#"{{"type":"user","message":{{"content":{{"opaque":1e9999}}}},"timestamp":"{}"}}"#,
            rfc(NOW - 3600)
        );
        let path = transcript(temp.path(), &[opened, error_line(NOW - 10, None)]);
        assert_eq!(session_start(&path), Some(NOW - 3600));
        assert_eq!(latest_limit(&[path], NOW - 60, NOW), None);
    }
    #[test]
    fn only_recent_jsonl_files_are_considered_and_tails_are_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let fresh = transcript(temp.path(), &[error_line(NOW, None)]);
        fs::write(fresh.with_extension("txt"), "not a transcript").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let found = recent_files(&temp.path().join("projects"), now);
        assert_eq!(found, vec![fresh.clone()]);
        assert!(recent_files(&temp.path().join("projects"), now + 2 * WINDOW_SECONDS).is_empty());
        // A marker at the very end of a file larger than the tail is still read.
        let mut big = "x".repeat(400 * 1024);
        big.push('\n');
        big.push_str(&error_line(NOW - 5, Some(NOW + 100)));
        fs::write(&fresh, big).unwrap();
        assert_eq!(latest_limit(&[fresh], NOW - 900, NOW), Some(NOW + 100));
    }

    /// SES-04: two accounts can share a reset time. A session opened after the switch runs on
    /// the new account, so its limit is the new account's even when its reset equals the old
    /// one's; the old session, bound to the old account, keeps charging that account.
    #[test]
    fn an_equal_reset_on_a_new_session_is_the_new_accounts_limit() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        let opened_a = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW - 3600));
        let session_a = transcript(
            temp.path(),
            &[opened_a.clone(), error_line(NOW - 100, Some(NOW + 3600))],
        );
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            std::slice::from_ref(&session_a),
            NOW,
        );
        state.switched(Some(&identity("b")), NOW);
        // B's own new session hits a limit that happens to reset when A's does.
        let opened_b = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW + 90));
        let session_b = transcript(
            temp.path(),
            &[opened_b, error_line(NOW + 140, Some(NOW + 3600))],
        );
        // A's old session fails again too: it stays A's.
        fs::write(
            &session_a,
            [
                opened_a,
                error_line(NOW - 100, Some(NOW + 3600)),
                error_line(NOW + 145, Some(NOW + 3600)),
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            &[session_a.clone(), session_b.clone()],
            NOW + 150,
        );
        let limited = state.limited_ids(NOW + 150);
        assert!(limited.contains("id-b"), "B's own limit counts");
        assert!(limited.contains("id-a"));
        let report = state.report(NOW + 150);
        let b = report.iter().find(|r| r["account_id"] == "id-b").unwrap();
        assert_eq!(b["kind"], "quota");
        assert_eq!(b["resets_at"], NOW + 3600);
        assert_eq!(b["confidence"], "inferred");
        assert_eq!(b["scope"], "unknown");
        assert!(b["event_id"]
            .as_str()
            .unwrap()
            .contains(&session_id(&session_b)));
    }
    /// SES-06: evidence and session bindings survive a restart; a hold without a provider reset
    /// is a retry time, never reported as a reset.
    #[test]
    fn evidence_survives_a_restart_and_a_hold_is_not_a_reset() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(EVIDENCE_FILE);
        let state = LimitState::kept_in(file.clone(), NOW);
        let snapshot = Snapshot {
            events: vec![
                event(NOW - 60, "request", "id-m", "rate_limited"),
                event(NOW - 30, "request", "id-m", "rate_limited"),
            ],
            ..Snapshot::default()
        };
        state.refresh(&snapshot, None, &[], NOW);
        let restarted = LimitState::kept_in(file.clone(), NOW + 10);
        assert!(restarted.limited_ids(NOW + 10).contains("id-m"));
        let row = &restarted.report(NOW + 10)[0];
        assert_eq!(row["kind"], "unknown");
        assert!(
            row["resets_at"].is_null(),
            "a fallback hold is not a provider reset"
        );
        assert_eq!(row["until"], NOW - 30 + DEFAULT_HOLD_SECONDS);
        assert_eq!(row["confidence"], "attributed");
        // Past its hold it is not loaded again; nothing in the file is provider text.
        assert!(
            LimitState::kept_in(file.clone(), NOW + 2 * DEFAULT_HOLD_SECONDS)
                .limited_ids(NOW + 2 * DEFAULT_HOLD_SECONDS)
                .is_empty()
        );
        let text = fs::read_to_string(&file).unwrap();
        assert!(!text.contains("spend limit") && !text.contains("message"));
    }
    #[test]
    fn evidence_read_from_disk_is_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(EVIDENCE_FILE);
        let limit = |until: i64, source: &str| Limit {
            event_id: "managed:x:1".into(),
            source: source.into(),
            session: None,
            observed_at: NOW - 10,
            kind: LimitKind::Unknown,
            limit_type: None,
            resets_at: None,
            until,
            confidence: Confidence::Attributed,
        };
        let evidence = EvidenceFile {
            version: 1,
            limited: HashMap::from([
                ("ok".to_string(), limit(NOW + 100, "managed")),
                (
                    "forever".to_string(),
                    limit(NOW + 10 * MAX_HOLD_SECONDS, "managed"),
                ),
                ("odd".to_string(), limit(NOW + 100, "elsewhere")),
            ]),
            sessions: HashMap::new(),
        };
        switchboard_core::private_fs::private_write(&file, &serde_json::to_vec(&evidence).unwrap())
            .unwrap();
        let state = LimitState::kept_in(file, NOW);
        assert_eq!(state.limited_ids(NOW), HashSet::from(["ok".to_string()]));
    }

    #[test]
    fn a_full_table_makes_room_and_never_drops_a_new_limit() {
        let state = LimitState::default();
        let limit = |until: i64| Limit {
            event_id: format!("managed:x:{until}"),
            source: "managed".into(),
            session: None,
            observed_at: NOW,
            kind: LimitKind::Unknown,
            limit_type: None,
            resets_at: None,
            until,
            confidence: Confidence::Attributed,
        };
        for i in 0..MAX_ENTRIES as i64 {
            assert!(state.mark(&format!("id-{i}"), limit(NOW + 100 + i)));
        }
        assert!(state.mark("id-new", limit(NOW + 50)));
        let limited = state.limited_ids(NOW);
        assert!(limited.contains("id-new"));
        assert!(
            !limited.contains("id-0"),
            "the hold ending soonest made room"
        );
        assert_eq!(limited.len(), MAX_ENTRIES);
    }

    /// Review P1: a session bound to A that adopted B after the switch and then hits B's own
    /// limit (a new reset) holds B, so rotation can leave it; A keeps only its own reset.
    #[test]
    fn a_bound_session_that_hits_a_new_limit_holds_the_current_account() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let snapshot = Snapshot::default();
        let opened = format!(r#"{{"type":"user","timestamp":"{}"}}"#, rfc(NOW - 3600));
        let first = [opened.clone(), error_line(NOW - 100, Some(NOW + 3600))];
        let session = transcript(temp.path(), &first);
        state.refresh(
            &snapshot,
            Some((&identity("a"), vec!["id-a".into()])),
            std::slice::from_ref(&session),
            NOW,
        );
        state.switched(Some(&identity("b")), NOW);
        let later = [
            opened,
            error_line(NOW - 100, Some(NOW + 3600)),
            error_line(NOW + 200, Some(NOW + 9000)),
        ];
        fs::write(&session, later.join("\n") + "\n").unwrap();
        state.refresh(
            &snapshot,
            Some((&identity("b"), vec!["id-b".into()])),
            std::slice::from_ref(&session),
            NOW + 210,
        );
        let report = state.report(NOW + 210);
        let until = |id: &str| {
            report
                .iter()
                .find(|r| r["account_id"] == id)
                .map(|r| r["until"].clone())
        };
        assert_eq!(until("id-b"), Some(serde_json::json!(NOW + 9000)));
        assert_eq!(
            until("id-a"),
            Some(serde_json::json!(NOW + 3600)),
            "A keeps its own reset"
        );
    }
    /// Review P2: a reset beyond the seven-day bound is held from the marker's own time — the
    /// same on every pass, so the file is written once — and its reported time is kept as is.
    #[test]
    fn a_far_reset_is_written_once_and_reported_as_given() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(EVIDENCE_FILE);
        let state = LimitState::kept_in(file.clone(), NOW);
        let month = NOW + 30 * 86_400;
        let path = transcript(temp.path(), &[error_line(NOW - 30, Some(month))]);
        let mut contents = std::collections::HashSet::new();
        for pass in 0..20 {
            state.refresh(
                &Snapshot::default(),
                Some((&identity("a"), vec!["id-a".into()])),
                std::slice::from_ref(&path),
                NOW + pass * 30,
            );
            contents.insert(fs::read(&file).unwrap());
        }
        assert_eq!(contents.len(), 1, "one write for one marker");
        let row = &state.report(NOW)[0];
        assert_eq!(row["resets_at"], month, "the provider's time, uncapped");
        assert_eq!(row["until"], NOW - 30 + MAX_HOLD_SECONDS);
    }
    /// Review P2: a session's first binding is saved even when no hold changes with it.
    #[test]
    fn a_new_binding_alone_is_saved() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(EVIDENCE_FILE);
        let state = LimitState::kept_in(file.clone(), NOW);
        let one = transcript(temp.path(), &[error_line(NOW - 100, Some(NOW + 7200))]);
        state.refresh(
            &Snapshot::default(),
            Some((&identity("a"), vec!["id-a".into()])),
            std::slice::from_ref(&one),
            NOW,
        );
        // A second session on the same account with an earlier reset: A's hold does not change.
        let two = transcript(temp.path(), &[error_line(NOW - 50, Some(NOW + 3600))]);
        state.refresh(
            &Snapshot::default(),
            Some((&identity("a"), vec!["id-a".into()])),
            &[one, two.clone()],
            NOW + 1,
        );
        let restarted = LimitState::kept_in(file, NOW + 2);
        assert_eq!(
            restarted.bound(&session_id(&two)),
            Some(vec!["id-a".to_string()])
        );
    }
    #[test]
    fn a_removed_account_loses_its_holds_and_bindings() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::kept_in(temp.path().join(EVIDENCE_FILE), NOW);
        let path = transcript(temp.path(), &[error_line(NOW - 100, Some(NOW + 3600))]);
        state.refresh(
            &Snapshot::default(),
            Some((&identity("a"), vec!["id-a".into()])),
            std::slice::from_ref(&path),
            NOW,
        );
        state.forget_missing(["id-other"]);
        assert!(state.limited_ids(NOW).is_empty());
        assert_eq!(state.bound(&session_id(&path)), None);
    }
    /// A limit is a quota limit only when the marker names a subscription window.
    #[test]
    fn the_kind_comes_from_the_markers_own_limit_type() {
        let temp = tempfile::tempdir().unwrap();
        let state = LimitState::default();
        let mut spend: serde_json::Value =
            serde_json::from_str(&error_line(NOW - 40, Some(NOW + 600))).unwrap();
        spend["quotaLimits"]["rateLimitType"] = "Overage Limit!".into();
        let quota = transcript(temp.path(), &[error_line(NOW - 30, Some(NOW + 900))]);
        let other = transcript(temp.path(), &[spend.to_string()]);
        state.refresh(
            &Snapshot::default(),
            Some((&identity("a"), vec!["id-a".into()])),
            &[quota],
            NOW,
        );
        let row = &state.report(NOW)[0];
        assert_eq!(
            (row["kind"].as_str(), row["limit_type"].as_str()),
            (Some("quota"), Some("five_hour"))
        );
        let state = LimitState::default();
        state.refresh(
            &Snapshot::default(),
            Some((&identity("a"), vec!["id-a".into()])),
            &[other],
            NOW,
        );
        assert_eq!(
            state.report(NOW)[0]["kind"],
            "unknown",
            "an unrecognised type is not a quota"
        );
    }
}
