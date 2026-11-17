//! SB-401: the pay-per-token group's daily ceiling, as the proxy checks it before every request
//! of the group: the ceiling from `spend.json` (read again only when the file changed) and the
//! day's list-price spend from the proxy's own tallies (`stats`, `prices`).
use std::path::Path;
use std::sync::Mutex;

/// The group's daily ceiling in cents, from `<root>/spend.json`; none when it has none or the
/// file cannot be read (then the group serves no request).
pub fn ceiling_cents(root: &Path, pool: &str) -> Option<u32> {
    type Kept = (std::time::SystemTime, u64, serde_json::Value);
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let path = root.join("spend.json");
    let meta = std::fs::metadata(&path).ok()?;
    let stamp = (meta.modified().ok()?, meta.len());
    let mut kept = KEPT.lock().ok()?;
    let fresh = kept
        .as_ref()
        .is_some_and(|(at, len, _)| (*at, *len) == stamp);
    if !fresh {
        let bytes = switchboard_core::private_fs::read_private(&path, 128 * 1024).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        *kept = Some((stamp.0, stamp.1, value));
    }
    let file = &kept.as_ref()?.2;
    file["ceilings"].as_array()?.iter().find_map(|c| {
        (c["scope"]["kind"] == "project" && c["scope"]["pool"] == pool)
            .then(|| c["cents"].as_u64().and_then(|n| u32::try_from(n).ok()))
            .flatten()
    })
}

/// What the group's answers cost so far this UTC day, in micro-dollars.
pub fn spent_today(stats: &crate::stats::Stats, pool: &str, now: i64) -> u64 {
    let day = crate::stats::day_of(now);
    stats.lock().map_or(0, |all| {
        all.iter()
            .filter_map(|(key, t)| {
                let p = crate::stats::parts(key)?;
                (p.day == day && p.pool == pool).then_some(t.cost_micros)
            })
            .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_days_spend_is_the_groups_own_and_todays_only() {
        let stats: crate::stats::Stats = std::sync::Arc::default();
        let now = 1_794_880_000;
        let today = crate::stats::day_of(now);
        let yesterday = crate::stats::day_of(now - 86_400);
        {
            let mut all = stats.lock().unwrap();
            for (day, pool, micros) in [
                (&today, "tokens", 700),
                (&today, "tokens", 300),
                (&today, "apps", 5_000),
                (&yesterday, "tokens", 9_000),
            ] {
                all.entry(crate::stats::key(
                    day,
                    "a",
                    pool,
                    "claude-code",
                    &format!("/w/{micros}"),
                ))
                .or_default()
                .cost_micros += micros;
            }
        }
        assert_eq!(spent_today(&stats, "tokens", now), 1_000);
    }

    #[test]
    fn a_ceiling_is_read_from_spend_json_and_none_means_none() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(ceiling_cents(root.path(), "tokens"), None);
        switchboard_core::private_fs::private_write(
            &root.path().join("spend.json"),
            br#"{"version":1,"ceilings":[{"scope":{"kind":"machine"},"cents":900,"set_at":1},{"scope":{"kind":"project","pool":"tokens"},"cents":2000,"set_at":1}]}"#,
        )
        .unwrap();
        assert_eq!(ceiling_cents(root.path(), "tokens"), Some(2000));
        assert_eq!(ceiling_cents(root.path(), "apps"), None);
    }
}
