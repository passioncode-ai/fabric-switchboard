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
    /// SB-156: each project key's name when it was counted, so a project that left every group
    /// (or whose checkout was deleted) keeps the name it had.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    names: BTreeMap<String, String>,
}

#[derive(Default)]
pub(crate) struct Statistics {
    inner: Mutex<State>,
}
#[derive(Default)]
struct State {
    tallies: Tallies,
    names: BTreeMap<String, String>,
    dirty: bool,
    saved_at: i64,
}

impl Statistics {
    /// What a previous run kept; an unreadable file starts empty and is replaced at the next
    /// save (the counts are a convenience, not a ledger).
    pub(crate) fn load(root: &Path) -> Self {
        let saved = switchboard_core::private_fs::read_private(&root.join(FILE), MAX_FILE)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .unwrap_or_default();
        let tallies = saved
            .tallies
            .into_iter()
            .filter(|(k, _)| stats::parts(k).is_some())
            .collect();
        Self {
            inner: Mutex::new(State {
                tallies,
                names: saved.names,
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
    /// SB-156: adds the proxy's new counts with each folder replaced by its project's key (the
    /// repository's identity, or the folder outside Git) as `index` sees it now, and remembers
    /// the project's name. A count then stays with its project after the checkout it came from
    /// is removed (an agent's worktree), instead of becoming a project of its own.
    pub(crate) fn absorb_resolved(&self, new: Tallies, index: &ProjectIndex) {
        if new.is_empty() {
            return;
        }
        let mut resolved = Tallies::new();
        let mut names = BTreeMap::new();
        for (key, t) in new {
            let Some(p) = stats::parts(&key) else {
                continue;
            };
            let project = index.project_of(p.folder);
            if let Some(name) = index.named(&project) {
                names.insert(project.clone(), name);
            }
            resolved
                .entry(stats::key(p.day, p.account, p.pool, p.client, &project))
                .or_default()
                .add(&t);
        }
        if let Ok(mut s) = self.inner.lock() {
            s.names.extend(names);
        }
        self.absorb(resolved);
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
            .retain(|k, _| stats::parts(k).is_some_and(|p| p.day >= oldest.as_str()));
        while s.tallies.len() > MAX_ROWS {
            // Oldest day first: keys start with the day.
            let Some(first) = s.tallies.keys().next().cloned() else {
                break;
            };
            s.tallies.remove(&first);
        }
        let keys: std::collections::BTreeSet<String> = s
            .tallies
            .keys()
            .filter_map(|k| stats::parts(k).map(|p| p.folder.to_owned()))
            .collect();
        s.names.retain(|k, _| keys.contains(k));
        let saved = Saved {
            version: 1,
            tallies: s.tallies.clone(),
            names: s.names.clone(),
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
    /// The last `days` days (1–90) as tables, names resolved from the snapshot and folders
    /// resolved to their projects by `index` (SB-147).
    pub(crate) fn report(
        &self,
        snapshot: &Snapshot,
        index: &ProjectIndex,
        days: i64,
        now: i64,
    ) -> Value {
        let days = days.clamp(1, KEEP_DAYS);
        let from = stats::day_of(now - (days - 1) * 86_400);
        let to = stats::day_of(now);
        let (tallies, saved_names) = self
            .inner
            .lock()
            .map(|s| (s.tallies.clone(), s.names.clone()))
            .unwrap_or_default();
        let name_of = |key: &str| {
            index
                .named(key)
                .or_else(|| saved_names.get(key).cloned())
                .or_else(|| index.name_of(key))
        };
        let since = tallies
            .keys()
            .filter_map(|k| stats::parts(k).map(|p| p.day.to_owned()))
            .min();
        let mut total = Tally::default();
        let mut by_group: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_project: BTreeMap<String, (Tally, std::collections::BTreeSet<String>)> =
            BTreeMap::new();
        let mut by_account: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_agent: BTreeMap<String, Tally> = BTreeMap::new();
        let mut by_day: BTreeMap<String, (Tally, BTreeMap<String, Tally>)> = BTreeMap::new();
        let mut by_route: BTreeMap<(String, String, String, String), Tally> = BTreeMap::new();
        for (key, t) in &tallies {
            let Some(p) = stats::parts(key) else {
                continue;
            };
            if p.day < from.as_str() || p.day > to.as_str() {
                continue;
            }
            let project = index.project_of(p.folder);
            total.add(t);
            by_group.entry(p.pool.into()).or_default().add(t);
            let entry = by_project.entry(project.clone()).or_default();
            entry.0.add(t);
            entry.1.insert(p.pool.into());
            by_account.entry(p.account.into()).or_default().add(t);
            by_agent.entry(p.client.into()).or_default().add(t);
            let day = by_day.entry(p.day.into()).or_default();
            day.0.add(t);
            day.1.entry(p.pool.into()).or_default().add(t);
            by_route
                .entry((p.pool.into(), project, p.account.into(), p.client.into()))
                .or_default()
                .add(t);
        }
        let group_name = |pool: &str| {
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
            rows.sort_by_key(|r| std::cmp::Reverse(r.1));
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
        let groups = busiest(
            by_group
                .iter()
                .map(|(pool, t)| {
                    let mut m = serde_json::Map::new();
                    m.insert("pool".into(), json!(pool));
                    m.insert("name".into(), json!(group_name(pool)));
                    (row(m, t), weight(t))
                })
                .collect(),
        );
        let projects = busiest(
            by_project
                .iter()
                .map(|(key, (t, pools))| {
                    let mut m = serde_json::Map::new();
                    m.insert("project".into(), json!(key));
                    m.insert("name".into(), json!(name_of(key)));
                    m.insert(
                        "groups".into(),
                        json!(pools
                            .iter()
                            .map(|p| json!({"pool": p, "name": group_name(p)}))
                            .collect::<Vec<_>>()),
                    );
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
                .map(|((pool, project, id, client), t)| {
                    let mut m = account_fields(id);
                    m.insert("pool".into(), json!(pool));
                    m.insert("name".into(), json!(group_name(pool)));
                    m.insert("project".into(), json!(project));
                    m.insert("project_name".into(), json!(name_of(project)));
                    m.insert("agent".into(), json!(client));
                    (row(m, t), weight(t))
                })
                .collect(),
        );
        let tokens = |t: &Tally| {
            t.input_tokens + t.output_tokens + t.cache_read_tokens + t.cache_write_tokens
        };
        let days_rows: Vec<Value> = by_day
            .iter()
            .rev()
            .map(|(day, (t, pools))| {
                let mut m = serde_json::Map::new();
                m.insert("day".into(), json!(day));
                m.insert(
                    "groups".into(),
                    json!(pools
                        .iter()
                        .map(|(pool, t)| json!({"pool": pool, "requests": t.requests, "tokens": tokens(t)}))
                        .collect::<Vec<_>>()),
                );
                row(m, t)
            })
            .collect();
        json!({
            "days": days,
            "from": from,
            "to": to,
            "since": since,
            "total": total,
            "groups": groups,
            "projects": projects,
            "accounts": accounts,
            "agents": agents,
            "routes": routes,
            "by_day": days_rows,
        })
    }
}

/// SB-147: which project a group folder belongs to. A project is a repository with all its
/// checkouts (the main one, its worktrees, its clones of one `origin`), or a folder outside Git
/// on its own (DEC-0089's AT-6, AT-7 and AT-15); its name is the repository's or the folder's.
#[derive(Default, Clone)]
pub(crate) struct ProjectIndex {
    by_folder: std::collections::HashMap<String, String>,
    /// SB-155: each project's name as in the person's folders (`roots::Root::name`), made
    /// unique: two projects that share one get their owner in brackets.
    names: std::collections::HashMap<String, String>,
}
impl ProjectIndex {
    /// From the groups' folders as Git sees them (`roots::tree` per group).
    pub(crate) fn build(snapshot: &Snapshot) -> Self {
        let mut index = Self::default();
        for group in &snapshot.projects {
            for root in crate::roots::tree(&group.folders) {
                let key = root.remote.clone().unwrap_or_else(|| root.folder.clone());
                index.names.entry(key.clone()).or_insert(root.name.clone());
                for folder in std::iter::once(root.folder.clone())
                    .chain(root.derived.iter().map(|d| d.folder.clone()))
                {
                    index.by_folder.insert(folder, key.clone());
                }
            }
        }
        index.disambiguate();
        index
    }
    /// SB-155: two projects with one name (`ssheleg/research-agent` and `prowl-ai/research-agent`)
    /// read as `research-agent (ssheleg)` and `research-agent (prowl-ai)`; a folder outside Git
    /// takes its parent folder's name.
    fn disambiguate(&mut self) {
        let mut count: std::collections::HashMap<String, usize> = Default::default();
        for name in self.names.values() {
            *count.entry(name.clone()).or_default() += 1;
        }
        for (key, name) in self.names.iter_mut() {
            if count.get(name).copied().unwrap_or(0) > 1 {
                let owner = key
                    .trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .nth(1)
                    .unwrap_or_default();
                if !owner.is_empty() {
                    *name = format!("{name} ({owner})");
                }
            }
        }
    }
    /// The project key of a group folder: its repository's identity, or the folder itself;
    /// empty for requests whose folder is not known.
    pub(crate) fn project_of(&self, folder: &str) -> String {
        if folder.is_empty() {
            return String::new();
        }
        self.by_folder
            .get(folder)
            .cloned()
            .unwrap_or_else(|| folder.to_owned())
    }
    /// SB-156: the name of a project a group holds now, if any.
    pub(crate) fn named(&self, key: &str) -> Option<String> {
        self.names.get(key).cloned()
    }
    /// A project's name (SB-155: as in the person's folders); its key's last segment for a key
    /// no group holds now; none for requests whose folder is not known.
    pub(crate) fn name_of(&self, key: &str) -> Option<String> {
        if let Some(name) = self.names.get(key) {
            return Some(name.clone());
        }
        (!key.is_empty()).then(|| {
            key.trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(key)
                .to_owned()
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
            (stats::key(&today, "acc", "pool", "codex", ""), tally(2, 10)),
            (stats::key(&old, "acc", "pool", "codex", ""), tally(9, 99)),
        ]));
        s.absorb(Tallies::from([(
            stats::key(&today, "acc", "pool", "codex", ""),
            tally(1, 5),
        )]));
        // Not yet: nothing written before a quarter of an hour, unless forced.
        s.save(root.path(), now, false);
        s.save(root.path(), now, true);
        #[cfg(unix)]
        {
            let mode = std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions();
            assert_eq!(
                std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
                0o600
            );
        }
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
    fn the_report_groups_by_group_project_account_agent_and_day_inside_the_range() {
        let now = 1_792_400_000;
        let today = stats::day_of(now);
        let week_ago = stats::day_of(now - 8 * 86_400);
        let s = Statistics::default();
        s.absorb(Tallies::from([
            (
                stats::key(&today, "a1", "proj", "codex", "/w/app"),
                tally(3, 300),
            ),
            (
                stats::key(&today, "a1", "proj", "claude-code", "/w/app-clone"),
                tally(1, 10),
            ),
            (
                stats::key(&today, "gone", "default", "claude-code", ""),
                tally(2, 20),
            ),
            (
                stats::key(&week_ago, "a1", "proj", "codex", "/w/app"),
                tally(5, 500),
            ),
        ]));
        // Two checkouts of one repository are one project (DEC-0089 AT-7).
        let index = ProjectIndex {
            by_folder: [
                ("/w/app".to_string(), "github.com/o/app".to_string()),
                ("/w/app-clone".to_string(), "github.com/o/app".to_string()),
            ]
            .into(),
            names: Default::default(),
        };
        let snapshot = Snapshot::default();
        let week = s.report(&snapshot, &index, 7, now);
        assert_eq!(week["total"]["requests"], 6);
        assert_eq!(week["groups"][0]["pool"], "proj");
        assert_eq!(week["groups"][0]["input_tokens"], 310);
        assert_eq!(week["projects"][0]["name"], "app");
        assert_eq!(week["projects"][0]["requests"], 4);
        assert_eq!(week["projects"][0]["groups"][0]["pool"], "proj");
        // Requests whose folder is not known are one row without a name.
        assert!(week["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["project"] == "" && p["name"].is_null() && p["requests"] == 2));
        assert_eq!(week["agents"][0]["agent"], "codex");
        assert!(week["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["account_id"] == "gone" && a["label"].is_null()));
        assert_eq!(week["routes"].as_array().unwrap().len(), 3);
        assert_eq!(week["by_day"].as_array().unwrap().len(), 1);
        assert_eq!(week["by_day"][0]["groups"].as_array().unwrap().len(), 2);
        let month = s.report(&snapshot, &index, 30, now);
        assert_eq!(month["total"]["requests"], 11);
        assert_eq!(month["by_day"][0]["day"], today);
        assert_eq!(month["since"], week_ago);
        // SB-156: counts are kept under their project, so a checkout removed later (an agent's
        // worktree) never becomes a project of its own, and the project keeps its name.
        let kept = Statistics::default();
        let mut live = ProjectIndex {
            by_folder: [(
                "/w/_worktrees/app-x".to_string(),
                "github.com/o/app".to_string(),
            )]
            .into(),
            names: [("github.com/o/app".to_string(), "app".to_string())].into(),
        };
        kept.absorb_resolved(
            Tallies::from([(
                stats::key(&today, "a1", "proj", "codex", "/w/_worktrees/app-x"),
                tally(2, 2),
            )]),
            &live,
        );
        live = ProjectIndex::default(); // the worktree and the group are gone
        let after = kept.report(&snapshot, &live, 7, now);
        assert_eq!(after["projects"][0]["project"], "github.com/o/app");
        assert_eq!(after["projects"][0]["name"], "app");
        // A 0.6.52 key (no folder) still reads.
        // SB-155: names come from the folders, and two projects with one name are told apart.
        let mut named = ProjectIndex {
            by_folder: Default::default(),
            names: [
                (
                    "github.com/ssheleg/research-agent".to_string(),
                    "research-agent".to_string(),
                ),
                (
                    "github.com/prowl-ai/research-agent".to_string(),
                    "research-agent".to_string(),
                ),
                (
                    "github.com/o/dashboard".to_string(),
                    "observatory-open-source".to_string(),
                ),
            ]
            .into(),
        };
        named.disambiguate();
        assert_eq!(
            named
                .name_of("github.com/ssheleg/research-agent")
                .as_deref(),
            Some("research-agent (ssheleg)")
        );
        assert_eq!(
            named
                .name_of("github.com/prowl-ai/research-agent")
                .as_deref(),
            Some("research-agent (prowl-ai)")
        );
        assert_eq!(
            named.name_of("github.com/o/dashboard").as_deref(),
            Some("observatory-open-source"),
            "the folder's name, not the remote's"
        );
        assert_eq!(named.name_of("github.com/x/gone").as_deref(), Some("gone"));
        let old = Statistics::default();
        old.absorb(Tallies::from([(
            format!("{today}\ta1\tproj\tcodex"),
            tally(1, 1),
        )]));
        assert_eq!(
            old.report(&snapshot, &index, 7, now)["total"]["requests"],
            1
        );
    }
}
