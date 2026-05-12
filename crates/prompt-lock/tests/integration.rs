use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use domain::{AgentId, PromptLock};
use prompt_lock::FsPromptLock;
use tempfile::tempdir;

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

#[test]
fn second_acquire_for_same_session_blocks_until_first_drops() {
    let dir = tempdir().unwrap();
    let lock = Arc::new(FsPromptLock::new(dir.path().to_path_buf()));
    let session = id("sess-1");

    let started = Arc::new(AtomicBool::new(false));
    let started_signal = Arc::clone(&started);
    let lock_for_t1 = Arc::clone(&lock);
    let session_for_t1 = session.clone();

    let t1 = thread::spawn(move || {
        let _g = lock_for_t1.acquire(&session_for_t1).unwrap();
        started_signal.store(true, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(50));
    });

    while !started.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(1));
    }

    let started_at = Instant::now();
    let _g2 = lock.acquire(&session).unwrap();
    let waited = started_at.elapsed();
    t1.join().unwrap();

    assert!(
        waited >= Duration::from_millis(20),
        "second acquire should have blocked, waited {waited:?}"
    );
}

#[test]
fn different_sessions_do_not_block_each_other() {
    let dir = tempdir().unwrap();
    let lock = Arc::new(FsPromptLock::new(dir.path().to_path_buf()));
    let session_a = id("sess-a");
    let session_b = id("sess-b");

    let lock_for_t1 = Arc::clone(&lock);
    let session_a_t1 = session_a.clone();
    let t1 = thread::spawn(move || {
        let _g = lock_for_t1.acquire(&session_a_t1).unwrap();
        thread::sleep(Duration::from_millis(50));
    });

    // Yield long enough that thread 1's acquire lands before we race for B.
    thread::sleep(Duration::from_millis(5));

    let started_at = Instant::now();
    let _g = lock.acquire(&session_b).unwrap();
    let waited = started_at.elapsed();

    assert!(
        waited < Duration::from_millis(20),
        "cross-session acquire blocked unexpectedly, waited {waited:?}"
    );
    t1.join().unwrap();
}
