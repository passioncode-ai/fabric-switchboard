//! SB-401: the pay-per-token group's daily ceiling, as the proxy checks it before every request
//! the group's keys serve: the ceiling from `spend.json` (read again only when the file changed)
//! and the day's list-price spend the proxy counted itself (`Spend`, priced by `prices`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// The pay-per-token group's spend today, written after every priced answer of its keys (0600):
/// `{version: 1, day, pools: {pool: micro-dollars}}`.
pub const SPEND_FILE: &str = "tokens-spend.json";
/// Seconds between two tries of a failed spend file write.
const RETRY: i64 = 5;
/// What `statistics.json` may weigh at most (as the owner reads it).
const STATISTICS_LIMIT: u64 = 8 * 1024 * 1024;

/// What each pool's answers cost this UTC day, in micro-dollars, counted by the proxy for its
/// own check. Unlike the statistics tallies, which the owner takes away every background pass,
/// it is never drained. The proxy loads it before it listens (`Spend::load`): the larger of
/// `tokens-spend.json` (written after each answer of a pay-per-token key) and the day's cost in
/// `statistics.json`, so a restart forgets nothing.
#[derive(Default)]
pub struct Spend {
    days: Mutex<Days>,
    /// One write of `tokens-spend.json` at a time, the newest only (its sequence number).
    writer: Mutex<u64>,
    /// Writes handed to a blocking thread and not finished yet.
    pending: AtomicUsize,
}
#[derive(Default)]
struct Days {
    /// The last write of `tokens-spend.json` failed: the day's spend is not on disk, so the
    /// pay-per-token group serves no request until a write succeeds (retried at most every
    /// `RETRY` seconds, at the check).
    write_failed: bool,
    retried_at: i64,
    root: Option<PathBuf>,
    /// The newest state of the file, by sequence number.
    seq: u64,
    /// The UTC day whose spend could not be read at start (a file that exists and cannot be
    /// read): the pay-per-token group serves no request that day.
    unknown: Option<String>,
    /// Pools whose day is written to `tokens-spend.json`.
    written: std::collections::BTreeSet<String>,
    /// Pool → (UTC day, micro-dollars spent that day).
    pools: HashMap<String, (String, u64)>,
    /// Pool → the UTC day its ceiling was last found reached.
    noticed: HashMap<String, String>,
    reached: Vec<Reached>,
}
/// A pool's ceiling found reached, once per pool and UTC day, for the owner to ask the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reached {
    pub pool: String,
    pub cents: u32,
    pub micros: u64,
}
impl Spend {
    /// The day's spend as the files under `root` hold it, read before the proxy listens. A file
    /// that is missing adds nothing; one that exists and cannot be read (too large, not JSON)
    /// leaves the day unknown unless the other file holds today's figures, and an unknown day
    /// refuses the pay-per-token group (`unknown_today`).
    pub fn load(root: &Path, now: i64) -> Self {
        let today = crate::stats::day_of(now);
        let spend = Spend::default();
        let read = |name: &str, limit: u64| -> Result<Option<serde_json::Value>, ()> {
            let path = root.join(name);
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(()),
                Ok(_) => {}
            }
            let bytes = switchboard_core::private_fs::read_private(&path, limit).map_err(|_| ())?;
            serde_json::from_slice(&bytes).map(Some).map_err(|_| ())
        };
        let mut written: HashMap<String, u64> = HashMap::new();
        let file = read(SPEND_FILE, 64 * 1024);
        let file_today = matches!(&file, Ok(Some(v)) if v["day"] == today.as_str());
        if let Ok(Some(v)) = &file {
            if file_today {
                if let Some(pools) = v["pools"].as_object() {
                    for (pool, micros) in pools {
                        if let Some(m) = micros.as_u64() {
                            written.insert(pool.clone(), m);
                        }
                    }
                }
            }
        }
        let statistics = read("statistics.json", STATISTICS_LIMIT);
        let mut counted: HashMap<String, u64> = HashMap::new();
        if let Ok(Some(v)) = &statistics {
            if let Some(tallies) = v["tallies"].as_object() {
                for (key, t) in tallies {
                    let Some(p) = crate::stats::parts(key) else {
                        continue;
                    };
                    if p.day != today {
                        continue;
                    }
                    if let Some(m) = t["cost_micros"].as_u64() {
                        let cell = counted.entry(p.pool.to_owned()).or_default();
                        *cell = cell.saturating_add(m);
                    }
                }
            }
        }
        {
            let mut days = spend.days();
            for (pool, micros) in written.iter().chain(counted.iter()) {
                let entry = days
                    .pools
                    .entry(pool.clone())
                    .or_insert_with(|| (today.clone(), 0));
                entry.1 = entry.1.max(*micros);
            }
            days.written = written.keys().cloned().collect();
            if file.is_err() || (statistics.is_err() && !file_today) {
                days.unknown = Some(today);
            }
        }
        spend
    }
    /// Whether today's spend could not be read at start.
    pub fn unknown_today(&self, now: i64) -> bool {
        let (failed, retry) = {
            let mut days = self.days();
            if days.unknown.as_deref() == Some(crate::stats::day_of(now).as_str()) {
                return true;
            }
            let retry = days.write_failed && now - days.retried_at >= RETRY;
            if retry {
                days.retried_at = now;
            }
            (days.write_failed, retry)
        };
        if retry {
            // The check itself tries the write again; it serves only once one succeeded.
            self.write_newest();
            return self.days().write_failed;
        }
        failed
    }
    /// The day's spend of every pay-per-token pool as the file holds it, with its sequence.
    fn body(days: &Days, day: &str) -> serde_json::Value {
        let pools: serde_json::Map<String, serde_json::Value> = days
            .written
            .iter()
            .filter_map(|p| {
                days.pools
                    .get(p)
                    .filter(|(d, _)| d == day)
                    .map(|(_, m)| (p.clone(), serde_json::json!(m)))
            })
            .collect();
        serde_json::json!({"version": 1, "day": day, "pools": pools})
    }
    /// Writes the newest state, unless a newer one was already written; a failure marks the day.
    fn write_newest(&self) {
        let mut written = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        let (seq, root, body) = {
            let days = self.days();
            let day = days
                .pools
                .values()
                .map(|(d, _)| d.clone())
                .max()
                .unwrap_or_default();
            (days.seq, days.root.clone(), Self::body(&days, &day))
        };
        let Some(root) = root else {
            return;
        };
        if seq <= *written && !self.days().write_failed {
            return;
        }
        let ok = switchboard_core::private_fs::private_write(
            &root.join(SPEND_FILE),
            body.to_string().as_bytes(),
        )
        .is_ok();
        if ok {
            *written = seq;
        }
        self.days().write_failed = !ok;
    }
    /// Writes still running on a blocking thread.
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }
    /// Adds one answer's cost to `pool`'s day and, with `root` (a pay-per-token key's answer),
    /// writes the day of every pay-per-token pool to `<root>/tokens-spend.json`. The write runs off
    /// the counter's lock, on a blocking thread when a runtime runs, so a ceiling check never
    /// waits on the disk; an older state never lands after a newer one.
    pub fn add_and_write(self: &Arc<Self>, pool: &str, now: i64, micros: u64, root: Option<&Path>) {
        let day = crate::stats::day_of(now);
        {
            let mut days = self.days();
            Self::add_to(&mut days, pool, &day, micros);
            let Some(root) = root else {
                return;
            };
            days.written.insert(pool.to_owned());
            days.root = Some(root.to_owned());
            days.seq = days.seq.saturating_add(1);
        }
        let spend = Arc::clone(self);
        self.pending.fetch_add(1, Ordering::SeqCst);
        let job = move || {
            spend.write_newest();
            spend.pending.fetch_sub(1, Ordering::SeqCst);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(job);
            }
            Err(_) => job(),
        }
    }
    fn add_to(days: &mut Days, pool: &str, day: &str, micros: u64) {
        let entry = days
            .pools
            .entry(pool.to_owned())
            .or_insert_with(|| (day.to_owned(), 0));
        if entry.0 != day {
            *entry = (day.to_owned(), 0);
        }
        entry.1 = entry.1.saturating_add(micros);
    }
    /// The counts survive a panic elsewhere: a poisoned lock is taken over, never read as zero.
    fn days(&self) -> MutexGuard<'_, Days> {
        self.days.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Adds one answer's cost to `pool`'s day.
    pub fn add(&self, pool: &str, now: i64, micros: u64) {
        let day = crate::stats::day_of(now);
        Self::add_to(&mut self.days(), pool, &day, micros);
    }
    /// What `pool`'s answers cost so far this UTC day.
    pub fn today(&self, pool: &str, now: i64) -> u64 {
        let day = crate::stats::day_of(now);
        self.days()
            .pools
            .get(pool)
            .filter(|(d, _)| *d == day)
            .map_or(0, |(_, micros)| *micros)
    }
    /// Notes `pool`'s ceiling reached; the owner hears of it once per pool and UTC day.
    pub fn note_reached(&self, pool: &str, now: i64, cents: u32, micros: u64) {
        let day = crate::stats::day_of(now);
        let mut days = self.days();
        if days.noticed.get(pool) == Some(&day) {
            return;
        }
        days.noticed.insert(pool.to_owned(), day);
        if days.reached.len() < 64 {
            days.reached.push(Reached {
                pool: pool.to_owned(),
                cents,
                micros,
            });
        }
    }
    /// The ceilings found reached since the last call.
    pub fn take_reached(&self) -> Vec<Reached> {
        std::mem::take(&mut self.days().reached)
    }
}

/// `spend.json` as last read, kept while its path and stamp stay the same.
#[derive(Default)]
pub struct Ceilings {
    kept: Mutex<Option<(PathBuf, Stamp, serde_json::Value)>>,
}
type Stamp = (std::time::SystemTime, u64, u64);
fn stamp(meta: &std::fs::Metadata) -> Option<Stamp> {
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(meta);
    #[cfg(not(unix))]
    let inode = 0;
    Some((meta.modified().ok()?, meta.len(), inode))
}
impl Ceilings {
    /// The daily ceiling of `pool` in cents, from `<root>/spend.json`; none when it has none or
    /// the file cannot be read (then the group serves no request).
    pub fn cents(&self, root: &Path, pool: &str) -> Option<u32> {
        let path = root.join("spend.json");
        let meta = std::fs::metadata(&path).ok()?;
        let stamp = stamp(&meta)?;
        let mut kept = self.kept.lock().unwrap_or_else(|p| p.into_inner());
        let fresh = kept
            .as_ref()
            .is_some_and(|(p, s, _)| *p == path && *s == stamp);
        if !fresh {
            *kept = None;
            let bytes = switchboard_core::private_fs::read_private(&path, 128 * 1024).ok()?;
            let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            *kept = Some((path, stamp, value));
        }
        let file = &kept.as_ref()?.2;
        file["ceilings"].as_array()?.iter().find_map(|c| {
            (c["scope"]["kind"] == "project" && c["scope"]["pool"] == pool)
                .then(|| c["cents"].as_u64().and_then(|n| u32::try_from(n).ok()))
                .flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_794_880_000;

    #[test]
    fn the_days_spend_is_the_pools_own_and_todays_only() {
        let spend = Spend::default();
        spend.add("tokens", NOW - 86_400, 9_000);
        spend.add("tokens", NOW, 700);
        spend.add("tokens", NOW, 300);
        spend.add("apps", NOW, 5_000);
        assert_eq!(spend.today("tokens", NOW), 1_000);
        assert_eq!(
            spend.today("tokens", NOW + 86_400),
            0,
            "a new day starts at zero"
        );
        spend.add("tokens", NOW, u64::MAX);
        assert_eq!(spend.today("tokens", NOW), u64::MAX, "saturates");
    }

    #[test]
    fn the_day_is_loaded_from_the_spend_file_and_the_statistics_the_larger_winning() {
        let root = tempfile::tempdir().unwrap();
        let today = crate::stats::day_of(NOW);
        // Nothing on disk: nothing spent, nothing unknown.
        let empty = Arc::new(Spend::load(root.path(), NOW));
        assert_eq!(
            (empty.today("tokens", NOW), empty.unknown_today(NOW)),
            (0, false)
        );
        // An answer of a pay-per-token key writes the file; a restart reads it.
        empty.add_and_write("tokens", NOW, 700, Some(root.path()));
        empty.add_and_write("tokens", NOW, 300, Some(root.path()));
        empty.add("apps", NOW, 5);
        let again = Spend::load(root.path(), NOW);
        assert_eq!(again.today("tokens", NOW), 1_000);
        assert_eq!(
            again.today("apps", NOW),
            0,
            "only pay-per-token pools are written"
        );
        // statistics.json holding more for the day wins; an older day counts nothing.
        let key = crate::stats::key(&today, "a", "tokens", "claude-code", "/w");
        let old = crate::stats::key(
            &crate::stats::day_of(NOW - 86_400),
            "a",
            "tokens",
            "c",
            "/w",
        );
        switchboard_core::private_fs::private_write(
            &root.path().join("statistics.json"),
            serde_json::json!({"version": 1, "tallies": {key: {"cost_micros": 4_000}, old: {"cost_micros": 9_000}}})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(Spend::load(root.path(), NOW).today("tokens", NOW), 4_000);
        // A file that exists and cannot be read leaves the day unknown.
        std::fs::write(root.path().join(SPEND_FILE), b"{not json").unwrap();
        let broken = Spend::load(root.path(), NOW);
        assert!(broken.unknown_today(NOW));
        assert!(
            !broken.unknown_today(NOW + 86_400),
            "the next day starts clean"
        );
        // A statistics file past its bound, with no spend file of today: unknown too.
        std::fs::remove_file(root.path().join(SPEND_FILE)).unwrap();
        switchboard_core::private_fs::private_write(&root.path().join("statistics.json"), b"[1,")
            .unwrap();
        assert!(Spend::load(root.path(), NOW).unknown_today(NOW));
    }

    #[test]
    fn a_spend_file_that_cannot_be_written_refuses_until_a_write_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let spend = Arc::new(Spend::load(root.path(), NOW));
        // A folder where the file should be: the write fails.
        std::fs::create_dir(root.path().join(SPEND_FILE)).unwrap();
        spend.add_and_write("tokens", NOW, 10, Some(root.path()));
        assert!(spend.unknown_today(NOW));
        // The next try is a retry interval later; it succeeds once the way is clear.
        std::fs::remove_dir(root.path().join(SPEND_FILE)).unwrap();
        assert!(
            spend.unknown_today(NOW + 1),
            "not retried before its interval"
        );
        assert!(!spend.unknown_today(NOW + RETRY + 1));
        assert_eq!(Spend::load(root.path(), NOW).today("tokens", NOW), 10);
    }

    #[test]
    fn a_reached_ceiling_is_handed_over_once_a_day() {
        let spend = Spend::default();
        spend.note_reached("tokens", NOW, 500, 5_000_000);
        spend.note_reached("tokens", NOW + 10, 500, 5_100_000);
        assert_eq!(
            spend.take_reached(),
            vec![Reached {
                pool: "tokens".into(),
                cents: 500,
                micros: 5_000_000
            }]
        );
        assert!(spend.take_reached().is_empty());
        spend.note_reached("tokens", NOW + 86_400, 500, 1);
        assert_eq!(spend.take_reached().len(), 1, "the next day asks again");
    }

    #[test]
    fn a_ceiling_is_read_from_spend_json_and_none_means_none() {
        let ceilings = Ceilings::default();
        let root = tempfile::tempdir().unwrap();
        assert_eq!(ceilings.cents(root.path(), "tokens"), None);
        switchboard_core::private_fs::private_write(
            &root.path().join("spend.json"),
            br#"{"version":1,"ceilings":[{"scope":{"kind":"machine"},"cents":900,"set_at":1},{"scope":{"kind":"project","pool":"tokens"},"cents":2000,"set_at":1}]}"#,
        )
        .unwrap();
        assert_eq!(ceilings.cents(root.path(), "tokens"), Some(2000));
        assert_eq!(ceilings.cents(root.path(), "apps"), None);
        // Another root is another file, whatever was read before.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(ceilings.cents(other.path(), "tokens"), None);
        // An unreadable file is no ceiling.
        std::fs::write(root.path().join("spend.json"), b"{not json").unwrap();
        assert_eq!(ceilings.cents(root.path(), "tokens"), None);
    }
}
