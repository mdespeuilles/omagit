//! Fetch, pull and push.
//!
//! The `git` binary, and here the reason is not only SPEC §8's list. It is
//! **authentication**: `libsecret` on Linux and `osxkeychain` on macOS are
//! reached through `git`'s credential helper protocol, and a client that
//! reimplemented it would be a client that asks for a password the system
//! already knows.
//!
//! ## Nothing here ever waits for a person
//!
//! `GIT_TERMINAL_PROMPT=0` is set for every invocation ([`crate::cli`]), so a
//! repository whose credentials no helper can supply **fails** rather than
//! blocking on an invisible prompt. That is the whole difference between an
//! operation a user can cancel and an application that has hung: there is no
//! terminal behind this window, and a `git` that decided to ask a question
//! would never be answered.
//!
//! `SSH_ASKPASS` is deliberately not set either. An `ssh` that wants a
//! passphrase and has no agent fails with a message naming the key, which is
//! actionable; pointing it at a helper that cannot draw a window would hang the
//! same way.

use std::time::Duration;

use crate::cli::{Git, Progress};
use crate::{Cancel, Repository, Result};

/// Network operations get their own deadline.
///
/// The default is for reads and is far too short here: cloning a large
/// repository over a slow link is legitimately minutes. It is still bounded —
/// a `git` that has stopped making progress has to end eventually, and
/// cancellation is the user's answer for everything shorter than this.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// What a push is allowed to do to the remote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PushForce {
    /// Refuse anything that is not a fast-forward.
    #[default]
    Never,
    /// `--force-with-lease`: overwrite, but only if the remote is still where
    /// we last saw it.
    ///
    /// The only force this offers. Plain `--force` overwrites whatever is
    /// there, including a colleague's commit pushed thirty seconds ago, and it
    /// cannot tell that case from the rebase you meant to publish.
    /// `--force-with-lease` refuses exactly that case, and the refusal is the
    /// point.
    WithLease,
}

/// Bring refs down from a remote without touching the working tree.
///
/// `--prune`, because a branch deleted on the remote that lingers as a
/// remote-tracking ref is a lie the sidebar would keep repeating.
pub fn fetch(
    git: &Git,
    repo: &Repository,
    remote: Option<&str>,
    watch: Option<Progress>,
    cancel: &Cancel,
) -> Result<String> {
    let mut invocation = super::at(git, repo)?
        .args(["fetch", "--prune", "--progress"])
        .timeout(NETWORK_TIMEOUT);
    if let Some(remote) = remote {
        invocation = invocation.arg(remote);
    } else {
        invocation = invocation.arg("--all");
    }
    if let Some(watch) = watch {
        invocation = invocation.watching(watch);
    }
    // `git fetch` says what it did on stderr, and says it there on success too.
    invocation.run(cancel).map(|output| output.stderr)
}

/// Fetch and integrate, the way the repository is configured to.
///
/// No strategy is chosen here. `pull.rebase`, `pull.ff` and a branch's own
/// `branch.<name>.rebase` are the user's settings, and a client that passed
/// `--rebase` or `--no-rebase` of its own would quietly override a decision
/// someone made for the repository. What it does pass is `--progress`, and
/// nothing else.
pub fn pull(
    git: &Git,
    repo: &Repository,
    watch: Option<Progress>,
    cancel: &Cancel,
) -> Result<String> {
    let mut invocation = super::at(git, repo)?
        .args(["pull", "--progress"])
        // It can rewrite the working tree and can leave a conflict behind.
        .destructive()
        .timeout(NETWORK_TIMEOUT);
    if let Some(watch) = watch {
        invocation = invocation.watching(watch);
    }
    invocation.run(cancel).map(|output| output.stderr)
}

/// Where a push is going and what it is allowed to do there.
///
/// A struct rather than four arguments, the way [`super::CommitOptions`] is
/// one: `push(git, repo, "origin", "main", Never, true, …)` is a line where
/// swapping two of the middle values compiles and does something else.
#[derive(Clone, Debug)]
pub struct Push<'a> {
    pub remote: &'a str,
    pub branch: &'a str,
    pub force: PushForce,
    /// `--set-upstream`: pushing a new branch and then finding it tracks
    /// nothing is a second step nobody wants, and `git` itself suggests exactly
    /// this command in that case.
    pub set_upstream: bool,
}

/// Send commits to a remote.
pub fn push(
    git: &Git,
    repo: &Repository,
    to: &Push<'_>,
    watch: Option<Progress>,
    cancel: &Cancel,
) -> Result<String> {
    let Push {
        remote,
        branch,
        force,
        set_upstream,
    } = *to;
    let mut invocation = super::at(git, repo)?
        .args(["push", "--progress"])
        .timeout(NETWORK_TIMEOUT);
    if force == PushForce::WithLease {
        // Rewrites published history: what it replaces is on the remote's
        // reflog and on nobody's clone.
        invocation = invocation.arg("--force-with-lease").destructive();
    }
    if set_upstream {
        invocation = invocation.arg("--set-upstream");
    }
    if let Some(watch) = watch {
        invocation = invocation.watching(watch);
    }
    invocation
        .arg(remote)
        .arg(branch)
        .run(cancel)
        .map(|output| output.stderr)
}

/// What one line of `git`'s progress means.
///
/// Parsed here rather than in the interface because the shape is Git's, not the
/// screen's: `Receiving objects:  42% (420/1000), 1.2 MiB | 500 KiB/s`.
/// A line that does not match keeps its whole text as the phase — `git` writes
/// plenty that is worth showing and is not a percentage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub phase: String,
    /// `None` for a line with no percentage, which is most of them.
    pub percent: Option<u8>,
}

impl Step {
    pub fn parse(line: &str) -> Self {
        let line = line.trim();
        // `remote: Counting objects: 45% (450/1000)` — the prefix is the
        // server talking, and it is worth keeping.
        let Some((phase, rest)) = line.rsplit_once(':') else {
            return Self {
                phase: line.to_owned(),
                percent: None,
            };
        };
        let percent = rest
            .trim()
            .split('%')
            .next()
            .and_then(|number| number.trim().parse::<u8>().ok())
            .filter(|_| rest.contains('%'));
        match percent {
            Some(percent) => Self {
                phase: phase.trim().to_owned(),
                percent: Some(percent.min(100)),
            },
            None => Self {
                phase: line.to_owned(),
                percent: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_progress_git_actually_writes() {
        let step = Step::parse("Receiving objects:  42% (420/1000), 1.2 MiB | 500.00 KiB/s");
        assert_eq!(step.phase, "Receiving objects");
        assert_eq!(step.percent, Some(42));

        // The server's own progress keeps its prefix: "remote:" is who is
        // counting, and dropping it would make two phases look like one.
        let remote = Step::parse("remote: Counting objects:  45% (450/1000)");
        assert_eq!(remote.phase, "remote: Counting objects");
        assert_eq!(remote.percent, Some(45));
    }

    #[test]
    fn a_line_that_is_not_progress_keeps_all_of_itself() {
        // `git` writes plenty here that is worth showing and is not a
        // percentage — and losing it would leave the overlay blank during the
        // part of a fetch that has no percentage at all.
        for line in [
            "From github.com:owner/repo",
            " * [new branch]      main       -> origin/main",
            "Everything up-to-date",
        ] {
            let step = Step::parse(line);
            assert_eq!(step.phase, line.trim());
            assert_eq!(step.percent, None);
        }
    }

    #[test]
    fn a_finished_phase_is_a_hundred_and_not_more() {
        assert_eq!(
            Step::parse("Resolving deltas: 100% (250/250), done.").percent,
            Some(100)
        );
    }
}
