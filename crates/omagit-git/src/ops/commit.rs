//! Committing: the message, who it is from, and the three flags.
//!
//! The message goes to `git` on standard input rather than in an argument.
//! A commit message is arbitrary bytes with newlines in it — that is the whole
//! point of a body — and an argument list is the wrong shape for it on every
//! platform.

use std::path::PathBuf;
use std::time::Duration;

use crate::cli::Git;
use crate::{Cancel, GitError, ObjectId, Repository, Result};

/// A commit can run hooks, and a `pre-commit` hook can be slow. The default
/// deadline is for reads.
const COMMIT_TIMEOUT: Duration = Duration::from_secs(120);

/// How a commit is to be made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitOptions {
    /// Replace the previous commit rather than adding one.
    ///
    /// Destructive: the commit it replaces is reachable only through the
    /// reflog afterwards, so the journal records it as such.
    pub amend: bool,
    /// Append `Signed-off-by:` for the committer.
    pub sign_off: bool,
    /// Skip `pre-commit` and `commit-msg`.
    ///
    /// Explicit, never a default: SPEC §8 says a client that ignores hooks
    /// breaks team workflows, so this is something a user asks for once, in
    /// view, and not a setting that quietly stays on.
    pub no_verify: bool,
}

/// What a commit produced.
#[derive(Clone, Debug)]
pub struct CommitOutcome {
    pub id: ObjectId,
    /// Whatever `git` said on the way — hook output, `git commit`'s summary.
    /// Shown rather than swallowed: a `pre-commit` hook that rewrote a file
    /// says so here.
    pub notes: String,
}

/// Who `git` would record as the author of a commit made now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// The committer identity `git` would use, or `None` when it has none.
///
/// SPEC §11 asks for a warning when the identity is missing, and the warning has
/// to come *before* the commit: `git commit` with no identity fails with a wall
/// of text about `git config --global`, and an app that lets a user write a
/// message and then shows them that has wasted their work.
pub fn committer_identity(git: &Git, repo: &Repository, cancel: &Cancel) -> Result<Option<Author>> {
    // `git var` rather than `git config --get user.email`, because the question
    // is who `git` *would* record, not what the config file says. They differ:
    // with no `user.email`, git derives one from the account and the host and
    // commits with a warning, which `git config --get` reports as no identity
    // at all. An empty name or address is the state git itself refuses to
    // commit with, so that is what "none" means here.
    let ident = match super::at(git, repo)?
        .args(["var", "GIT_COMMITTER_IDENT"])
        .run(cancel)
    {
        Ok(output) => output.text().trim().to_owned(),
        Err(GitError::CommandFailed { .. }) => return Ok(None),
        Err(error) => return Err(error),
    };

    Ok(parse_ident(&ident))
}

/// `Name <address> 1700000000 +0200`, as `git var` writes it.
fn parse_ident(line: &str) -> Option<Author> {
    let open = line.rfind(" <")?;
    let close = line[open..].find('>')? + open;
    let name = line[..open].trim();
    let email = line[open + 2..close].trim();
    if name.is_empty() || email.is_empty() {
        return None;
    }
    Some(Author {
        name: name.to_owned(),
        email: email.to_owned(),
    })
}

/// The contents of `commit.template`, if the repository configures one.
///
/// Returned rather than applied: the caller decides whether an editor already
/// holds a message worth keeping (SPEC §11).
pub fn template(git: &Git, repo: &Repository, cancel: &Cancel) -> Result<Option<String>> {
    let path = match super::at(git, repo)?
        .args(["config", "--get", "commit.template"])
        .run(cancel)
    {
        Ok(output) => output.text().trim().to_owned(),
        Err(GitError::CommandFailed { .. }) => return Ok(None),
        Err(error) => return Err(error),
    };
    if path.is_empty() {
        return Ok(None);
    }

    // `~` is `git`'s own spelling, and it expands it itself; nothing else does.
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|| PathBuf::from(&path)),
        None => {
            let candidate = PathBuf::from(&path);
            if candidate.is_absolute() {
                candidate
            } else {
                super::work_dir(repo)?.join(candidate)
            }
        }
    };

    match std::fs::read_to_string(&expanded) {
        Ok(text) => Ok(Some(text)),
        // A configured template that is not there is worth saying, but not
        // worth refusing to commit over.
        Err(error) => {
            tracing::warn!(path = %expanded.display(), %error, "commit.template is unreadable");
            Ok(None)
        }
    }
}

/// Make the commit.
pub fn commit(
    git: &Git,
    repo: &Repository,
    message: &str,
    options: &CommitOptions,
    cancel: &Cancel,
) -> Result<CommitOutcome> {
    // Refused here rather than by `git`, whose message for it is about
    // configuring a global and says nothing about the work now at risk.
    if committer_identity(git, repo, cancel)?.is_none() {
        return Err(GitError::backend("committing", MissingIdentity));
    }

    let mut invocation = super::at(git, repo)?
        .args(["commit", "--file", "-", "--cleanup=strip"])
        .timeout(COMMIT_TIMEOUT);
    if options.amend {
        // Amending replaces a commit; the one it replaces survives only in the
        // reflog.
        invocation = invocation.arg("--amend").destructive();
    }
    if options.sign_off {
        invocation = invocation.arg("--signoff");
    }
    if options.no_verify {
        invocation = invocation.arg("--no-verify");
    }

    let output = invocation.input(message.as_bytes().to_vec()).run(cancel)?;

    // Asked rather than parsed out of `git commit`'s summary, whose format is
    // for people and changes between versions.
    let id = super::at(git, repo)?
        .args(["rev-parse", "HEAD"])
        .run(cancel)?
        .text()
        .trim()
        .parse::<ObjectId>()
        .map_err(|error| GitError::backend("reading the new commit", error))?;

    let notes = [output.text().trim(), output.stderr.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(CommitOutcome { id, notes })
}

#[derive(Debug)]
struct MissingIdentity;

impl std::fmt::Display for MissingIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no committer identity is configured (user.name and user.email)")
    }
}

impl std::error::Error for MissingIdentity {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ident_line_splits_into_a_name_and_an_address() {
        let parsed = parse_ident("Ada Lovelace <ada@example.org> 1700000000 +0200")
            .expect("a complete ident");
        assert_eq!(parsed.name, "Ada Lovelace");
        assert_eq!(parsed.email, "ada@example.org");
    }

    #[test]
    fn a_name_with_an_angle_bracket_still_splits_at_the_address() {
        // `rfind` rather than `find`: the address is the last bracketed part.
        let parsed = parse_ident("A <weird> Name <real@example.org> 1700000000 +0200")
            .expect("a complete ident");
        assert_eq!(parsed.name, "A <weird> Name");
        assert_eq!(parsed.email, "real@example.org");
    }

    #[test]
    fn an_empty_half_is_no_identity() {
        // What `git var` prints when `user.email` is set to nothing, and the
        // state `git commit` refuses with "empty ident".
        assert_eq!(parse_ident("Ada Lovelace <> 1700000000 +0200"), None);
        assert_eq!(parse_ident(" <ada@example.org> 1700000000 +0200"), None);
        assert_eq!(parse_ident(""), None);
    }
}
