//! Board SB-23: a `/usr/bin/security` call waits up to five seconds. Run on an async worker it
//! would hold that worker — and every task queued behind it — for the whole wait. `run` tells
//! the runtime the call blocks, so the worker's other tasks move to another thread.

/// Runs `work`, which blocks, without stalling the async tasks sharing this thread. Outside a
/// multi-threaded runtime (a plain thread, a single-threaded test runtime) it simply runs.
pub(crate) fn run<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(work)
        }
        _ => work(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
        time::Duration,
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn a_blocking_call_does_not_stall_the_tasks_beside_it() {
        let ticks = Arc::new(AtomicU32::new(0));
        let counter = ticks.clone();
        let ticker = tokio::spawn(async move {
            loop {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        tokio::task::yield_now().await;
        let before = ticks.load(Ordering::SeqCst);
        // The one worker runs a call that blocks for 300 ms, like a `security` wait.
        tokio::spawn(async { super::run(|| std::thread::sleep(Duration::from_millis(300))) })
            .await
            .unwrap();
        let during = ticks.load(Ordering::SeqCst) - before;
        ticker.abort();
        assert!(during >= 10, "the ticker kept running: {during} ticks");
    }

    #[tokio::test]
    async fn on_a_single_threaded_runtime_it_just_runs() {
        assert_eq!(super::run(|| 7), 7);
    }

    #[test]
    fn outside_a_runtime_it_just_runs() {
        assert_eq!(super::run(|| 7), 7);
    }
}
