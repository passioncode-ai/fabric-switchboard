//! SB-580: one answer of the Sessions list for the callers that ask at once. Fabric, Fabric
//! Dashboards, the window and agents ask the owner for `sessions --rows` every few seconds; under
//! load each read took seconds, and ten read side by side took 8 to 9.7 s each where one alone
//! took 0.2 s (measured on 2026-11-19, load average 120), past Fabric's 10 s deadline, which
//! ends the call with SIGTERM. Now a read runs once at a time: a caller that arrives while one
//! runs waits for it and takes its answer, and an answer stays good for `FRESH` unless an act on
//! a session (`changed`) made it old.

use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// How long an answer serves the callers that come after it.
const FRESH: Duration = Duration::from_secs(3);

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static KEPT: std::sync::Mutex<Option<(Instant, PathBuf, u64, Value)>> = std::sync::Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// An act changed the sessions (ended, closed, continued, restarted): the next read reads anew.
pub(crate) fn changed() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn kept(root: &Path) -> Option<Value> {
    let kept = KEPT.lock().ok()?;
    let (at, folder, generation, value) = kept.as_ref()?;
    (folder == root && *generation == GENERATION.load(Ordering::SeqCst) && at.elapsed() < FRESH)
        .then(|| value.clone())
}

/// The kept answer when it is good, else `work`'s, read by one caller at a time.
pub(crate) async fn shared(
    root: &Path,
    work: impl std::future::Future<Output = Result<Value, String>>,
) -> Result<Value, String> {
    if let Some(value) = kept(root) {
        return Ok(value);
    }
    let _turn = TURN.lock().await;
    // The read this caller waited for answers it too.
    if let Some(value) = kept(root) {
        return Ok(value);
    }
    let generation = GENERATION.load(Ordering::SeqCst);
    let value = work.await?;
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some((Instant::now(), root.to_owned(), generation, value.clone()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicU32, Arc};

    // One test for the statics: the kept answer is shared by every test in this process.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn callers_at_once_share_one_read_and_an_act_makes_the_next_read_anew() {
        let root = PathBuf::from("/rows-share-test");
        let reads = Arc::new(AtomicU32::new(0));
        let ask = |reads: Arc<AtomicU32>| {
            let root = root.clone();
            tokio::spawn(async move {
                shared(&root, async move {
                    reads.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(serde_json::json!({"rows": []}))
                })
                .await
            })
        };
        let callers: Vec<_> = (0..10).map(|_| ask(reads.clone())).collect();
        for caller in callers {
            assert_eq!(
                caller.await.unwrap().unwrap(),
                serde_json::json!({"rows": []})
            );
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1, "ten callers, one read");
        // Within FRESH the kept answer serves the next caller.
        ask(reads.clone()).await.unwrap().unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        // After an act it does not.
        changed();
        ask(reads.clone()).await.unwrap().unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        // A failed read is not kept: the next caller reads.
        changed();
        let failed = shared(&root, async { Err::<Value, _>("no".to_owned()) }).await;
        assert!(failed.is_err());
        ask(reads.clone()).await.unwrap().unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 3);
    }
}
