//! Tags: creating them, deleting them, and publishing them.
//!
//! SPEC §11 asks for "tags légers et annotés" in the same breath as branches,
//! and until now they were only *read*: `refs.rs` peels them, the sidebar draws
//! them, and nothing could make one.
//!
//! The two kinds are not two names for the same thing, which is why the choice
//! reaches the interface rather than being decided here. A **lightweight** tag
//! is a ref and nothing else — a name pointing at a commit, with no author, no
//! date and no message. An **annotated** tag is an object of its own, with a
//! tagger, a time and a message, and it is the one `git describe` prefers and
//! the one a release is expected to be.
//!
//! ## What is not refused here
//!
//! Tagging a commit that already carries a tag, deleting a tag that was
//! published, moving one. `git` answers each of those itself, and its answer
//! names what is in the way. SPEC §3 rule 3 wants Git's own words shown.

use crate::cli::Git;
use crate::{Cancel, Repository, Result};

use super::network::NETWORK_TIMEOUT;

/// Create a tag at `at`, or at `HEAD` when it is `None`.
///
/// `message` decides the kind: `Some` makes an annotated tag, `None` a
/// lightweight one. That is `git`'s own rule — `-m` implies `-a` — and saying
/// it with the presence of the message rather than a separate flag means the
/// two cannot disagree.
///
/// The message goes on **standard input**, not in an argument. A tag message is
/// a paragraph somebody typed, and a paragraph in an argument is a paragraph
/// that a quoting mistake truncates; `--file -` is what `git` offers for
/// exactly this, and it is the same door the commit message already goes
/// through.
pub fn create(
    git: &Git,
    repo: &Repository,
    name: &str,
    at: Option<&str>,
    message: Option<&str>,
    force: bool,
    cancel: &Cancel,
) -> Result<()> {
    let mut invocation = super::at(git, repo)?.arg("tag");
    if let Some(message) = message {
        invocation = invocation
            .args(["--annotate", "--file", "-"])
            .input(message);
    }
    if force {
        // Moving a tag that exists. Destructive in SPEC §15 rule 5's sense: the
        // old target is not written anywhere a tag can be recovered from, and
        // anybody who already fetched it keeps the old one.
        invocation = invocation.arg("--force").destructive();
    }
    invocation = invocation.arg(name);
    if let Some(at) = at {
        invocation = invocation.arg(at);
    }
    invocation.run(cancel).map(drop)
}

/// Delete a tag from this repository.
///
/// Destructive, and more so than deleting a branch: a branch leaves its tip in
/// the reflog and a tag leaves nothing at all. The caller confirms first.
///
/// It does not touch any remote. A tag that was pushed is still on the remote
/// and comes back with the next fetch — which is the truth worth telling in the
/// confirmation, because "delete" reading as "unpublish" is how somebody
/// deletes the same tag three times.
pub fn delete(git: &Git, repo: &Repository, name: &str, cancel: &Cancel) -> Result<()> {
    super::at(git, repo)?
        .args(["tag", "--delete"])
        .arg(name)
        .destructive()
        .run(cancel)
        .map(drop)
}

/// Publish one tag to `remote`.
///
/// One tag and never `--tags`: pushing every tag at once publishes whatever
/// happens to be local, including the ones somebody made while trying something
/// out. Naming it is the whole point of the gesture.
pub fn push(
    git: &Git,
    repo: &Repository,
    remote: &str,
    name: &str,
    cancel: &Cancel,
) -> Result<String> {
    let output = super::at(git, repo)?
        .args(["push", "--progress"])
        .timeout(NETWORK_TIMEOUT)
        .arg(remote)
        .arg(format!("refs/tags/{name}"))
        .run(cancel)?;
    Ok(output.stderr)
}

/// Remove a tag from `remote`, leaving the local one alone.
///
/// The other half of publishing, and it has to be said separately because the
/// two are genuinely different acts: a tag deleted here and a tag deleted there
/// are two decisions, and a client that did both from one button would delete
/// other people's copy for somebody who only wanted their own tidied up.
pub fn unpublish(
    git: &Git,
    repo: &Repository,
    remote: &str,
    name: &str,
    cancel: &Cancel,
) -> Result<String> {
    let output = super::at(git, repo)?
        .args(["push", "--progress"])
        .timeout(NETWORK_TIMEOUT)
        .arg(remote)
        .arg(format!(":refs/tags/{name}"))
        .destructive()
        .run(cancel)?;
    Ok(output.stderr)
}
