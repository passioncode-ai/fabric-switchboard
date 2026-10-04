//! Provider limit errors as rotation evidence (0.5). A seat's individual spend limit or a
//! per-account rate limit answers 429 while the quota endpoint still reports spare capacity;
//! Claude Swap cannot see it. Two sources: the proxy's own `request rate_limited` events for
//! managed sessions, and the API-error markers Claude Code writes into its session transcripts
//! for ordinary sessions. Deserialization keeps only flagged API-error markers and timestamp
//! metadata for session attribution; conversation content is never materialized, kept or logged.
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Limit {
    pub until: i64,
    pub source: &'static str,
}

#[derive(Default)]
pub(crate) struct LimitState {
    limited: Mutex<HashMap<String, Limit>>,
    /// The ordinary Claude Code identity last seen, and since when errors count against it.
    current: Mutex<Option<(String, i64)>>,
}

impl LimitState {
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
            .map(|(id, l)| serde_json::json!({"account_id": id, "until": l.until, "source": l.source}))
            .collect();
        rows.sort_by(|a, b| a["account_id"].as_str().cmp(&b["account_id"].as_str()));
        rows
    }
    fn mark(&self, id: &str, limit: Limit) {
        if let Ok(mut limited) = self.limited.lock() {
            let keep = limited.get(id).is_some_and(|old| old.until >= limit.until);
            if !keep {
                limited.insert(id.to_owned(), limit);
            }
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
        if let Ok(mut limited) = self.limited.lock() {
            limited.retain(|_, l| l.until > now);
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
                self.mark(
                    id,
                    Limit {
                        until: event.at + DEFAULT_HOLD_SECONDS,
                        source: "managed",
                    },
                );
            }
        }
        let Some((identity, ids)) = native else {
            return;
        };
        let Some(account) = identity.account_id.as_deref() else {
            return;
        };
        let since = self
            .since(account, &ids, snapshot, now)
            .max(now - WINDOW_SECONDS);
        // Holds already charged to other accounts: a marker with the same reset comes from a
        // session still running on that account, not from this one.
        let foreign: Vec<i64> = self
            .limited
            .lock()
            .map(|l| {
                l.iter()
                    .filter(|(id, limit)| !ids.contains(id) && limit.source == "claude_code")
                    .map(|(_, limit)| limit.until)
                    .collect()
            })
            .unwrap_or_default();
        let until = markers(transcripts, since, now)
            .into_iter()
            // A session pointed at the proxy writes its 429 to these transcripts too.
            .filter(|(at, _)| {
                managed
                    .iter()
                    .all(|e| (e.at - at).abs() > MANAGED_ECHO_SECONDS)
            })
            .filter(|(_, hold)| !foreign.contains(hold))
            .map(|(_, hold)| hold)
            .max();
        if let Some(until) = until {
            for id in ids {
                self.mark(
                    &id,
                    Limit {
                        until,
                        source: "claude_code",
                    },
                );
            }
        }
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

/// The latest hold implied by a rate-limit marker at or after `since`, or None.
#[cfg(test)]
pub(crate) fn latest_limit(files: &[PathBuf], since: i64, now: i64) -> Option<i64> {
    markers(files, since, now)
        .into_iter()
        .map(|(_, hold)| hold)
        .max()
}
/// Every rate-limit marker at or after `since`: when it was written, and until when it holds.
/// A marker without a reported reset, in a session that began before `since`, is skipped: that
/// session may still run on the previous account, and nothing in the marker can tell the two
/// apart (report §P2-8). One with a reset is told apart by the foreign-reset filter instead.
pub(crate) fn markers(files: &[PathBuf], since: i64, now: i64) -> Vec<(i64, i64)> {
    let mut found = Vec::new();
    for path in files {
        let Some(tail) = tail(path) else {
            continue;
        };
        let began = session_start(path);
        for line in tail.split(|b| *b == b'\n') {
            // Cheap byte checks first: a line that is not an API error is never parsed.
            if !contains(line, b"\"isApiErrorMessage\":true") || !contains(line, b"rate_limit") {
                continue;
            }
            if let Some((at, hold, reported)) = marker(line, since, now) {
                if !reported && began.is_some_and(|b| b < since - SWITCH_GRACE_SECONDS) {
                    continue;
                }
                found.push((at, hold));
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
fn marker(line: &[u8], since: i64, now: i64) -> Option<(i64, i64, bool)> {
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
        .filter(|t| *t > now);
    Some((
        at,
        reset
            .unwrap_or(at + DEFAULT_HOLD_SECONDS)
            .min(now + MAX_HOLD_SECONDS),
        reset.is_some(),
    ))
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
        // An old session still on A keeps failing after the grace, with A's reset.
        let late = transcript(temp.path(), &[error_line(NOW + 120, Some(NOW + 3600))]);
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
}
