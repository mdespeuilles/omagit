//! The four states everything read from a repository is in.
//!
//! SPEC §10 names them and requires the UI to render all four: `Idle`,
//! `Loading`, `Ready` and `Failed`. No spinner over the whole window, no blank
//! screen while something loads, and no error swallowed into an empty list —
//! each case gets drawn as itself.
//!
//! `Ready` carries the generation it was produced by, which is how a slow
//! result that arrives after a newer one is dropped instead of overwriting it.
//! Without that, clicking through four repositories quickly leaves the card
//! showing whichever read happened to finish last.

use omagit_git::GitError;

/// Monotonic per store, not per key: comparing two generations from different
/// keys is meaningless, but a single counter makes them cheap to mint and
/// impossible to reuse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(pub u64);

impl Generation {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Not `Clone`: `GitError` is not, deliberately — it carries a boxed source
/// chain — and a state that could be cloned would invite components to keep
/// copies of it, which SPEC §10 forbids.
#[derive(Debug, Default)]
pub enum AsyncState<T> {
    /// Never asked for.
    #[default]
    Idle,
    /// Asked for, nothing back yet. Carries what was there before, so a refresh
    /// redraws the old value dimmed rather than blanking the panel — the
    /// difference between "still working" and "gone".
    Loading(Option<T>),
    Ready(T, Generation),
    Failed(GitError),
}

impl<T> AsyncState<T> {
    /// The value, if there is one — including the stale one under a refresh.
    pub fn value(&self) -> Option<&T> {
        match self {
            AsyncState::Ready(value, _) => Some(value),
            AsyncState::Loading(previous) => previous.as_ref(),
            AsyncState::Idle | AsyncState::Failed(_) => None,
        }
    }

    pub fn error(&self) -> Option<&GitError> {
        match self {
            AsyncState::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, AsyncState::Loading(_))
    }

    /// True while there is nothing to draw yet — the state a placeholder row is
    /// for, as opposed to a refresh over an existing value.
    pub fn is_blank(&self) -> bool {
        matches!(self, AsyncState::Idle | AsyncState::Loading(None))
    }

    /// Move to `Loading`, keeping whatever was already there.
    pub fn begin(&mut self) {
        let previous = match std::mem::replace(self, AsyncState::Idle) {
            AsyncState::Ready(value, _) => Some(value),
            AsyncState::Loading(previous) => previous,
            AsyncState::Idle | AsyncState::Failed(_) => None,
        };
        *self = AsyncState::Loading(previous);
    }

    /// Land a result, unless a newer request has already been made.
    ///
    /// Returns whether it was accepted, which is what tells the caller there is
    /// something new to draw.
    pub fn finish(
        &mut self,
        result: Result<T, GitError>,
        generation: Generation,
        current: Generation,
    ) -> bool {
        if generation < current {
            return false;
        }
        *self = match result {
            Ok(value) => AsyncState::Ready(value, generation),
            // A cancelled read is not a failure to show: the user asked for
            // something else, and something else is already loading.
            Err(error) if error.is_cancelled() => return false,
            Err(error) => AsyncState::Failed(error),
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_keeps_the_value_it_is_replacing() {
        let mut state = AsyncState::Ready(7, Generation(1));
        state.begin();
        assert!(state.is_loading());
        assert!(!state.is_blank(), "there is still something to draw");
        assert_eq!(state.value(), Some(&7));
    }

    #[test]
    fn a_first_load_has_nothing_to_show() {
        let mut state = AsyncState::<u8>::Idle;
        assert!(state.is_blank());
        state.begin();
        assert!(state.is_blank(), "and still nothing while it loads");
        assert_eq!(state.value(), None);
    }

    #[test]
    fn a_stale_result_is_dropped() {
        let mut state = AsyncState::Idle;
        assert!(state.finish(Ok(2), Generation(2), Generation(2)));
        assert_eq!(state.value(), Some(&2));

        // The slow read for generation 1 lands after the fast one for 2.
        assert!(!state.finish(Ok(1), Generation(1), Generation(2)));
        assert_eq!(state.value(), Some(&2), "the newer answer stands");
    }

    #[test]
    fn a_cancellation_is_not_a_failure() {
        let mut state = AsyncState::Ready(1, Generation(1));
        state.begin();
        assert!(!state.finish(Err(GitError::Cancelled), Generation(2), Generation(2)));
        assert!(
            state.error().is_none(),
            "the user moved on; that is not something to report"
        );
        assert_eq!(state.value(), Some(&1));
    }

    #[test]
    fn a_failure_replaces_the_value() {
        let mut state = AsyncState::Ready(1, Generation(1));
        assert!(state.finish(Err(GitError::GitNotFound), Generation(2), Generation(2)));
        assert!(state.error().is_some());
        assert_eq!(
            state.value(),
            None,
            "showing a stale value beside an error would say the error was harmless"
        );
    }
}
