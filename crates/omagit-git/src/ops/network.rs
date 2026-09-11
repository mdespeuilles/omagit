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

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::{Git, Progress};
use crate::{Cancel, GitError, Repository, Result};

/// Network operations get their own deadline.
///
/// The default is for reads and is far too short here: cloning a large
/// repository over a slow link is legitimately minutes. It is still bounded —
/// a `git` that has stopped making progress has to end eventually, and
/// cancellation is the user's answer for everything shorter than this.
pub(crate) const NETWORK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Asking a remote whether it is there gets a much shorter one.
///
/// It runs while somebody watches a dialog. A probe that takes thirty minutes
/// to say "unreachable" has answered nothing.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

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
/// How a pull should reconcile a divergence.
///
/// Only ever passed when *nothing* configures it. `pull.rebase`, `pull.ff` and
/// a branch's own `branch.<name>.rebase` are the user's decisions, and a client
/// that sent a flag of its own would quietly override one somebody made for the
/// repository (§2.33). What this covers is the case where there is no decision
/// to override and `git` refuses to guess: since 2.27 it stops on a diverged
/// branch with a wall of hints, and the only way to answer them is a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconcile {
    /// A merge commit, `git`'s own historical default.
    Merge,
    /// Replay our commits on top of theirs, rewriting them.
    Rebase,
}

impl Reconcile {
    fn flag(self) -> &'static str {
        match self {
            Reconcile::Merge => "--no-rebase",
            Reconcile::Rebase => "--rebase",
        }
    }
}

/// Whether anything already says how this branch reconciles a divergence.
///
/// Three places can, in the order `git` reads them, and any one of them is an
/// answer: the branch's own setting, then `pull.rebase`, then `pull.ff`. Asked
/// rather than deduced from a failure message, because that message is
/// translated on a machine whose `git` speaks the user's language, and matching
/// English against it would work everywhere it was written and nowhere else.
pub fn reconcile_configured(
    git: &Git,
    repo: &Repository,
    branch: &str,
    cancel: &Cancel,
) -> Result<bool> {
    for key in [
        format!("branch.{branch}.rebase"),
        "pull.rebase".to_owned(),
        "pull.ff".to_owned(),
    ] {
        let answered = match super::at(git, repo)?
            .args(["config", "--get", &key])
            .run(cancel)
        {
            Ok(output) => !output.text().trim().is_empty(),
            // `git config --get` exits 1 for a key nobody set, which is the
            // ordinary case here rather than a failure.
            Err(crate::GitError::CommandFailed { .. }) => false,
            Err(error) => return Err(error),
        };
        if answered {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn pull(
    git: &Git,
    repo: &Repository,
    reconcile: Option<Reconcile>,
    watch: Option<Progress>,
    cancel: &Cancel,
) -> Result<String> {
    let mut invocation = super::at(git, repo)?
        .args(["pull", "--progress"])
        // It can rewrite the working tree and can leave a conflict behind.
        .destructive()
        .timeout(NETWORK_TIMEOUT);
    if let Some(reconcile) = reconcile {
        invocation = invocation.arg(reconcile.flag());
    }
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

/// What a clone brings down beyond the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloneOptions {
    /// `--depth 1`: the tip of each branch and none of the history behind it.
    ///
    /// Fast, and a real trade: the log has one commit in it, `blame` cannot see
    /// past it, and a push from a shallow clone is refused by some servers.
    /// Offered because for a theme repository somebody wants to *read* it is
    /// the difference between four seconds and four minutes, and the caller is
    /// the one who knows which case this is.
    ///
    /// `git` ignores it when the URL is a local path — it clones those by
    /// copying object files, a transport with no notion of depth — and says so
    /// on stderr, which reaches the progress stream. Not routed around by
    /// rewriting the URL to `file://`: that would be a different clone from the
    /// one that was asked for, no longer sharing objects with the original, and
    /// deciding that quietly is worse than a warning that says what happened.
    pub shallow: bool,
    /// `--recurse-submodules`: clone the submodules too, rather than leaving
    /// empty directories where they should be.
    pub submodules: bool,
}

/// Copy a remote repository into `parent`, as a directory called `name`.
///
/// The one operation here with no repository to start from, so it runs in the
/// parent directory rather than through [`super::at`] — and that is why it
/// takes `parent` and `name` apart rather than one path: `git clone <url>
/// <dir>` creates `<dir>`, and running *inside* a directory that does not exist
/// yet is not a thing.
///
/// Returns where it landed. `git` refuses a destination that exists and is not
/// empty, which is the check this does not repeat: the race between asking and
/// cloning is real, and `git`'s answer is the one that is true at the moment it
/// matters.
///
/// Not marked destructive. It writes a great deal, but everything it writes is
/// in a directory that did not exist a moment ago; there is nothing of anyone's
/// to lose.
pub fn clone(
    git: &Git,
    url: &str,
    parent: &Path,
    name: &str,
    options: &CloneOptions,
    watch: Option<Progress>,
    cancel: &Cancel,
) -> Result<PathBuf> {
    // The parent has to exist for `git` to have somewhere to run. A picker
    // hands back a directory that does, but a path remembered from a previous
    // session can name one that has since gone.
    std::fs::create_dir_all(parent).map_err(|failed| {
        GitError::backend(
            "creating the destination folder",
            NoSuchParent {
                parent: parent.to_owned(),
                failed,
            },
        )
    })?;

    let mut invocation = git
        .at(parent)
        .args(["clone", "--progress"])
        .timeout(NETWORK_TIMEOUT);
    if options.shallow {
        invocation = invocation.args(["--depth", "1"]);
    }
    if options.submodules {
        invocation = invocation.arg("--recurse-submodules");
    }
    if let Some(watch) = watch {
        invocation = invocation.watching(watch);
    }
    // `--` before the URL: a URL is a string somebody pasted, and one starting
    // with a dash would otherwise be read as an option.
    invocation
        .arg("--")
        .arg(url)
        .arg(name)
        .run(cancel)
        .map(|_| parent.join(name))
}

/// The destination's parent could not be made.
///
/// Named rather than passed through, because `std::io::Error` does not carry
/// the path it failed on: "permission denied" without saying *where* is a
/// message that sends someone looking.
#[derive(Debug)]
struct NoSuchParent {
    parent: PathBuf,
    failed: std::io::Error,
}

impl std::fmt::Display for NoSuchParent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.parent.display(), self.failed)
    }
}

impl std::error::Error for NoSuchParent {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.failed)
    }
}

/// Ask a remote whether it is there and whether we are allowed in.
///
/// `git ls-remote` is the cheapest question that exercises the whole path: DNS,
/// the transport, and the credential helper. It is asked *before* the clone
/// because the alternative is finding out from a four-minute operation that
/// failed on its first second — and because with `GIT_TERMINAL_PROMPT=0` a
/// repository whose credentials no helper can supply fails rather than
/// prompting, which makes this a real test of authentication and not just of
/// the network.
///
/// Its own timeout, much shorter than a clone's: this runs while somebody
/// watches a dialog, and a probe that takes thirty minutes to say "unreachable"
/// has answered nothing.
pub fn reachable(git: &Git, url: &str, cancel: &Cancel) -> Result<()> {
    git.at(std::env::temp_dir())
        .args(["ls-remote", "--heads"])
        .timeout(PROBE_TIMEOUT)
        .arg("--")
        .arg(url)
        .run(cancel)
        .map(drop)
}

/// The directory `git clone` would create for this URL, by `git`'s own rule.
///
/// The last path segment, without a trailing `.git` and without a trailing
/// slash. Derived rather than asked for, because a form that made someone type
/// the name of the thing they just pasted a URL to is a form asking a question
/// it can answer.
pub fn directory_for(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed
        .rsplit(['/', ':'])
        .find(|segment| !segment.is_empty())?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_owned())
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
    fn the_directory_is_the_one_git_would_choose() {
        for (url, expected) in [
            ("https://github.com/owner/repo.git", "repo"),
            ("https://github.com/owner/repo", "repo"),
            ("git@github.com:owner/repo.git", "repo"),
            ("ssh://git@host:22/owner/repo.git", "repo"),
            ("/srv/git/repo.git", "repo"),
            // A trailing slash is something a browser adds, not something the
            // person meant.
            ("https://github.com/owner/repo/", "repo"),
        ] {
            assert_eq!(directory_for(url).as_deref(), Some(expected), "for {url}");
        }
    }

    #[test]
    fn a_url_with_no_name_in_it_yields_none_rather_than_a_guess() {
        for url in ["", "   ", "/", "///"] {
            assert_eq!(directory_for(url), None, "for {url:?}");
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
