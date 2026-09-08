//! Keeps Git work off the render thread.
//!
//! GPUI renders on a single thread. One blocking Git call on it and the app
//! freezes on a large repository — SPEC §15 names this the most likely failure
//! mode and asks for a debug assertion from M0 onwards.
//!
//! The app calls [`mark_render_thread`] once during start-up, on the thread that
//! owns the windows. Every entry point in this crate then opens with
//! [`assert_off_render_thread`], which panics in debug builds and costs one
//! relaxed atomic load in release.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::ThreadId;

/// `0` means "not registered yet". Thread ids are never zero once hashed with
/// the +1 offset below, so the sentinel is unambiguous.
static RENDER_THREAD: AtomicU64 = AtomicU64::new(0);

fn key(id: ThreadId) -> u64 {
    // `ThreadId` has no stable numeric accessor on stable Rust, but its `Debug`
    // is not something to parse either. Hashing it is stable within a process,
    // which is all this guard needs.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish() | 1
}

/// Record the calling thread as the render thread. Called once, from the app.
pub fn mark_render_thread() {
    RENDER_THREAD.store(key(std::thread::current().id()), Ordering::Relaxed);
}

/// True when the caller is the registered render thread.
pub fn on_render_thread() -> bool {
    let marked = RENDER_THREAD.load(Ordering::Relaxed);
    marked != 0 && marked == key(std::thread::current().id())
}

/// Panic in debug builds if Git work was started on the render thread.
///
/// Deliberately loud: a frozen window is far harder to diagnose after the fact
/// than a panic pointing at the call site that blocked it.
#[track_caller]
pub fn assert_off_render_thread() {
    debug_assert!(
        !on_render_thread(),
        "Git work must not run on the render thread — move it to the background \
         executor and return the result through a message (SPEC §3 rule 2)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test, not three: the registry is a process-wide static, so separate
    // `#[test]` functions would race each other through it.
    #[test]
    fn guards_the_render_thread_and_only_it() {
        // Nothing registered yet: Git work is allowed anywhere.
        assert!(!on_render_thread());
        assert_off_render_thread();

        mark_render_thread();
        assert!(
            on_render_thread(),
            "the thread that registered is the render thread"
        );

        std::thread::spawn(|| {
            assert!(!on_render_thread(), "a background thread never is");
            assert_off_render_thread();
        })
        .join()
        .expect("background thread panicked");

        // And on the render thread the guard fires, in debug builds.
        if cfg!(debug_assertions) {
            let hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let caught = std::panic::catch_unwind(assert_off_render_thread);
            std::panic::set_hook(hook);
            assert!(caught.is_err(), "the guard must panic on the render thread");
        }
    }
}
