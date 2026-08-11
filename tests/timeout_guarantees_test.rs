use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::{interval, timeout};

/// Simulated HephaestusInterceptor that spawns a future and returns a JoinHandle.
/// This mimics the real interceptor which offloads work to a background Tokio task.
struct HephaestusInterceptor;

impl HephaestusInterceptor {
    /// Spawns a future on the Tokio runtime and returns its JoinHandle.
    /// The call returns immediately, guaranteeing non‑blocking behaviour for the caller.
    fn spawn<F>(&self, fut: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        tokio::spawn(fut)
    }
}

#[tokio::test]
async fn test_timeout_guarantees() {
    // ---- 1. Define a hanging operation (simulated infinite loop) ----
    // The loop yields control to the Tokio scheduler on each iteration to avoid
    // consuming 100 % CPU while still representing a task that never completes.
    async fn hanging_operation() {
        loop {
            tokio::task::yield_now().await;
        }
    }

    // ---- 2. Instantiate the mock interceptor and spawn the hanging task ----
    let interceptor = HephaestusInterceptor;
    let join_handle = interceptor.spawn(hanging_operation());

    // ---- 3. Verify that the main loop stays non‑blocking ----
    // We run a lightweight background worker that increments a counter every 50 ms.
    // If the main loop were blocked, this worker would make little or no progress.
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    let _worker = tokio::spawn(async move {
        let mut tick = interval(Duration::from_millis(50));
        loop {
            tick.tick().await;
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    // ---- 4. Apply a strict 2‑second timeout to the hanging operation ----
    let result = timeout(Duration::from_secs(2), join_handle).await;

    // ---- 5. Assertions ----
    // 5a. The background worker must have progressed, proving the main loop
    //     remained free to do other work.
    let count = counter.load(Ordering::Relaxed);
    assert!(
        count > 0,
        "The main loop should have been able to perform other work while waiting"
    );

    // 5b. The hanging operation must have been aborted by the timeout.
    // We expect an Err variant, which contains the elapsed time (timeout occurred).
    result.expect_err("Expected the operation to timeout after exactly 2 seconds");
}
