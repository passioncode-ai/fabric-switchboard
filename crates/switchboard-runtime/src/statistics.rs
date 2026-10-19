//! SB-145: what the proxy relayed, kept for 90 days in `statistics.json` (0600) and answered
//! as tables by project, account, coding agent and day. Counts only: requests and their
//! outcome, and the tokens the provider reported for them; never a prompt, answer, path or
//! header value. The proxy hands its counts over (`ProxyHandle::take_statistics`); they reach
//! the disk at most every quarter of an hour and at stop.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use switchboard_core::Snapshot;
use switchboard_proxy::stats::{self, Tallies, Tally};

pub(crate) const FILE: &str = "statistics.json";
/// Days kept; an older day is dropped at the next save.
pub(crate) const KEEP_DAYS: i64 = 90;
const MAX_FILE: u64 = 8 * 1024 * 1024;
/// Rows kept at most: 90 days of a busy machine fit many times over.
const MAX_ROWS: usize = 50_000;
const FLUSH_SECONDS: i64 = 15 * 60;

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    version: u32,
    tallies: Tallies,
}

#[derive(Default)]
pub(crate) struct Statistics {
    inner: Mutex<State>,
}
#[derive(Default)]
struct State {
    tallies: Tallies,
    dirty: bool,
    saved_at: i64,
}

impl Statistics {
    /// What a previous run kept; an unreadable file starts empty and is replaced at the next
    /// save (the counts are a convenience, not a ledger).
    pub(crate) fn load(root: &Path) -> Self {
        let tallies = switchboard_core::private_fs::read_private(&root.join(FILE), MAX_FILE)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .map(|s| s.tallies)
            .unwrap_or_default()
            .into_iter()
            .filter(|(k, _)| stats::parts(k).is_some())
            .collect();
        Self {
            inner: Mutex::new(State {
                tallies,
                dirty: false,
                saved_at: 0,
            }),
        }
    }
    /// Adds the proxy's new counts.
    pub(crate) fn absorb(&self, new: Tallies) {
        if new.is_empty() {
            return;
        }
        if let Ok(mut s) = self.inner.lock() {
            for (k, t) in new {
                s.tallies.entry(k).or_default().add(&t);
            }
            s.dirty = true;
        }
    }
    /// Writes when something changed and the last write is older than a quarter of an hour, or
    /// at once with `force` (the stop).
    pub(crate) fn save(&self, root: &Path, now: i64, force: bool) {
        let Ok(mut s) = self.inner.lock() else {
            return;
        };
        if !s.dirty || (!force && now - s.saved_at < FLUSH_SECONDS) {
            return;
        }
        let oldest = stats::day_of(now - (KEEP_DAYS - 1) * 86_400);
        s.tallies
            .retain(|k, _| stats::parts(k).is_some_and(|p| p.0 >= oldest.as_str()));
        while s.tallies.len() > MAX_ROWS {
            // Oldest day first: keys start with the day.
            let Some(first) = s.tallies.keys().next().cloned() else {
                break;
            };
            s.tallies.remove(&first);
        }
        let saved = Saved {
            version: 1,
            tallies: s.tallies.clone(),
        };
        let written = serde_json::to_vec(&saved)
            .map_err(|_| String::new())
            .and_then(|bytes| {
                switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
            });
        crate::oplog::event(
            "statistics_saved",
            &[
                (
                    "outcome",
                    crate::oplog::Field::Code(if written.is_ok() { "written" } else { "failed" }),
                ),
                ("rows", crate::oplog::Field::Number(s.tallies.len() as i64)),
            ],
        );
        if written.is_ok() {
            s.dirty = false;
            s.saved_at = now;
        }
    }
    /// The last `days` days (1–90) as tables, names resolved from the snapshot.
    pub(crate) fn report(&self, snapshot: &Snapshot, days: i64, now: i64) -> Value {
        let days = days.clamp(1, KEEP_DAYS);
        let from = stats::day_of(now - (days - 1) * 86_400);
        let to = stats::day_of(now);
        let tallies = self
            .inner
            .lock()
            .map(|s| s.tallies.clone())
            .unwrap_or_default();
        let since = tallies
            .keys()
            .next()
            .and_then(|k| stats::parts(k))
            .map(|p| p.0.to_owned());
        let mut total = Tally::default();
        let mut by_project: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_account: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_agent: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_day: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_route: BTreeMap<(String, String, String), Tally> = BTreeMap::new();
        for (key, t) in &tallies {
            let Some((day, account, pool, client)) = stats::parts(key) else {
                continue;
            };
            if day < from.as_str() || day > to.as_str() {
                continue;
            }
            total.add(t);
            by_project.entry(pool.into()).or_default().add(t);
            by_account.entry(account.into()).or_default().add(t);
            by_agent.entry(client.into()).or_default().add(t);
            by_day.entry(day.into()).or_default().add(t);
            by_route
                .entry((pool.into(), account.into(), client.into()))
                .or_default()
                .add(t);
        }
        let project_name = |pool: &str| {
            snapshot
                .projects
                .iter()
                .find(|p| p.pool == pool)
                .map(|p| p.name.clone())
        };
        let account = |id: &str| snapshot.accounts.iter().find(|a| a.id == id);
        let row = |mut fields: serde_json::Map<String, Value>, t: &Tally| {
            if let Value::Object(counts) = json!(t) {
                fields.extend(counts);
            }
            Value::Object(fields)
        };
        let busiest = |mut rows: Vec<(Value, u64)>| {
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            rows.into_iter().map(|r| r.0).collect::<Vec<_>>()
        };
        let weight =
            |t: &Tally| t.input_tokens + t.output_tokens + t.cache_read_tokens + t.requests;
        let account_fields = |id: &str| {
            let a = account(id);
            let mut m = serde_json::Map::new();
            m.insert("account_id".into(), json!(id));
            m.insert("label".into(), json!(a.map(|a| a.label.clone())));
            m.insert("provider".into(), json!(a.map(|a| a.provider)));
            m.insert("kind".into(), json!(a.map(|a| a.kind)));
            m
        };
        let projects = busiest(
            by_project
                .iter()
                .map(|(pool, t)| {
                    let mut m = serde_json::Map::new();
                    m.insert("pool".into(), json!(pool));
                    m.insert("name".into(), json!(project_name(pool)));
                    (row(m, t), weight(t))
                })
                .collect(),
        );
        let accounts = busiest(
            by_account
                .iter()
                .map(|(id, t)| (row(account_fields(id), t), weight(t)))
                .collect(),
        );
        let agents = busiest(
            by_agent
                .iter()
                .map(|(client, t)| {
                    let mut m = serde_json::Map::new();
                    m.insert("agent".into(), json!(client));
                    (row(m, t), weight(t))
                })
                .collect(),
        );
        let routes = busiest(
            by_route
                .iter()
                .map(|((pool, id, client), t)| {
                    let mut m = account_fields(id);
                    m.insert("pool".into(), json!(pool));
                    m.insert("name".into(), json!(project_name(pool)));
                    m.insert("agent".into(), json!(client));
                    (row(m, t), weight(t))
                })
                .collect(),
        );
        let days_rows: Vec<Value> = by_day
            .iter()
            .rev()
            .map(|(day, t)| {
                let mut m = serde_json::Map::new();
                m.insert("day".into(), json!(day));
                row(m, t)
            })
            .collect();
        json!({
            "days": days,
            "from": from,
            "to": to,
            "since": since,
            "total": total,
            "projects": projects,
            "accounts": accounts,
            "agents": agents,
            "routes": routes,
            "by_day": days_rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(requests: u64, input: u64) -> Tally {
        Tally {
            requests,
            ok: requests,
            input_tokens: input,
            ..Tally::default()
        }
    }

    #[test]
    fn counts_survive_a_restart_and_old_days_are_dropped() {
        let root = tempfile::tempdir().unwrap();
        let now = 1_792_400_000; // 2026-10-19
        let today = stats::day_of(now);
        let old = stats::day_of(now - 100 * 86_400);
        let s = Statistics::load(root.path());
        s.absorb(Tallies::from([
            (stats::key(&today, "acc", "pool", "codex"), tally(2, 10)),
            (stats::key(&old, "acc", "pool", "codex"), tally(9, 99)),
        ]));
        s.absorb(Tallies::from([(
            stats::key(&today, "acc", "pool", "codex"),
            tally(1, 5),
        )]));
        // Not yet: nothing written before a quarter of an hour, unless forced.
        s.save(root.path(), now, false);
        s.save(root.path(), now, true);
        let mode = std::fs::metadata(root.path().join(FILE))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let again = Statistics::load(root.path());
        let kept = again.inner.lock().unwrap().tallies.clone();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.values().next().unwrap(), &tally(3, 15));
        // A damaged file starts empty rather than failing the owner.
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert!(Statistics::load(root.path())
            .inner
            .lock()
            .unwrap()
            .tallies
            .is_empty());
    }

    #[test]
    fn the_report_groups_by_project_account_agent_and_day_inside_the_range() {
        let now = 1_792_400_000;
        let today = stats::day_of(now);
        let week_ago = stats::day_of(now - 8 * 86_400);
        let s = Statistics::default();
        s.absorb(Tallies::from([
            (stats::key(&today, "a1", "proj", "codex"), tally(3, 300)),
            (
                stats::key(&today, "a1", "proj", "claude-code"),
                tally(1, 10),
            ),
            (
                stats::key(&today, "gone", "default", "claude-code"),
                tally(2, 20),
            ),
            (stats::key(&week_ago, "a1", "proj", "codex"), tally(5, 500)),
        ]));
        let snapshot = Snapshot::default();
        let week = s.report(&snapshot, 7, now);
        assert_eq!(week["total"]["requests"], 6);
        assert_eq!(week["projects"][0]["pool"], "proj");
        assert_eq!(week["projects"][0]["input_tokens"], 310);
        assert_eq!(week["agents"][0]["agent"], "codex");
        // An account that is gone keeps its counts, without a label.
        assert!(week["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["account_id"] == "gone" && a["label"].is_null()));
        assert_eq!(week["routes"].as_array().unwrap().len(), 3);
        assert_eq!(week["by_day"].as_array().unwrap().len(), 1);
        let month = s.report(&snapshot, 30, now);
        assert_eq!(month["total"]["requests"], 11);
        assert_eq!(month["by_day"][0]["day"], today);
        assert_eq!(month["since"], week_ago);
    }
}
