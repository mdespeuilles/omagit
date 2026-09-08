//! omagit's Git core.
//!
//! No UI dependency (SPEC §3 rule 5). The hybrid backend of SPEC §8 — `gix` for
//! reads, the `git` binary for writes and the network — lands at M2. M0 ships
//! the two things every later milestone builds on: the error type that carries
//! Git failures to the UI as a state, and the render-thread guard.

pub mod error;
pub mod thread_guard;

pub use error::{GitError, Result};
pub use thread_guard::{assert_off_render_thread, mark_render_thread, on_render_thread};

/// The oldest `git` omagit runs against (SPEC §8).
pub const MINIMUM_GIT_VERSION: &str = "2.35";
