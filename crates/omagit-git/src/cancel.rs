//! One cancellation token, shared by everything that can take a while.
//!
//! A status on 50 000 files, a history walk and a `git` subprocess are all
//! things the user can walk away from — switching repository mid-scan has to
//! stop the scan, not wait for it. They cancel through the same handle for a
//! practical reason: `gix` wants an `Arc<AtomicBool>` it can poll from its own
//! worker threads, and that is exactly what a subprocess killer needs too, so
//! there is no second mechanism to keep in sync.
//!
//! Cancelling is one-way. A token that has fired is spent; the next operation
//! gets a fresh one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask whatever holds this token to stop as soon as it can.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// The shared flag, in the shape `gix` takes for its `should_interrupt`
    /// parameters.
    pub fn as_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0)
    }

    /// `Err(GitError::Cancelled)` once fired, so a long loop can `?` on it.
    pub fn check(&self) -> crate::Result<()> {
        if self.is_cancelled() {
            Err(crate::GitError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_for_every_holder() {
        let token = Cancel::new();
        let copy = token.clone();
        let flag = token.as_flag();

        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());

        copy.cancel();

        assert!(token.is_cancelled(), "a clone cancels the original");
        assert!(flag.load(Ordering::Acquire), "and the flag gix polls");
        assert!(matches!(token.check(), Err(crate::GitError::Cancelled)));
    }
}
