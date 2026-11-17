//! SB-401: the pay-per-token group's daily ceiling, as the proxy checks it before every request
//! the group's keys serve: the ceiling from `spend.json` (read again only when the file changed)
//! and the day's list-price spend the proxy counted itself (`Spend`, priced by `prices`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// What each pool's answers cost this UTC day, in micro-dollars, counted by the proxy for its
/// own check. Unlike the statistics tallies, which the owner takes away every background pass,
/// it is never drained; the owner seeds it at start with what `statistics.json` already holds
/// for the day, so a restart forgets nothing.
#[derive(Default)]
pub struct Spend {
    days: Mutex<Days>,
}
#[derive(Default)]
struct Days {
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
    /// The counts survive a panic elsewhere: a poisoned lock is taken over, never read as zero.
    fn days(&self) -> MutexGuard<'_, Days> {
        self.days.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Adds one answer's cost to `pool`'s day.
    pub fn add(&self, pool: &str, now: i64, micros: u64) {
        let day = crate::stats::day_of(now);
        let mut days = self.days();
        let entry = days
            .pools
            .entry(pool.to_owned())
            .or_insert_with(|| (day.clone(), 0));
        if entry.0 != day {
            *entry = (day, 0);
        }
        entry.1 = entry.1.saturating_add(micros);
    }
    /// Adds what the statistics on disk hold for `pool` on `day` (the owner's start). A seed of
    /// an older day than the one counted is ignored.
    pub fn seed(&self, pool: &str, day: &str, micros: u64) {
        let mut days = self.days();
        let entry = days
            .pools
            .entry(pool.to_owned())
            .or_insert_with(|| (day.to_owned(), 0));
        if entry.0.as_str() < day {
            *entry = (day.to_owned(), 0);
        }
        if entry.0 == day {
            entry.1 = entry.1.saturating_add(micros);
        }
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
    fn a_seed_adds_the_days_spend_on_disk_and_ignores_an_older_day() {
        let spend = Spend::default();
        let today = crate::stats::day_of(NOW);
        let yesterday = crate::stats::day_of(NOW - 86_400);
        spend.add("tokens", NOW, 500);
        spend.seed("tokens", &today, 1_500);
        spend.seed("tokens", &yesterday, 9_000);
        assert_eq!(spend.today("tokens", NOW), 2_000);
        // A seed of today on a pool counted yesterday replaces yesterday's figure.
        spend.add("apps", NOW - 86_400, 7);
        spend.seed("apps", &today, 3);
        assert_eq!(spend.today("apps", NOW), 3);
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
