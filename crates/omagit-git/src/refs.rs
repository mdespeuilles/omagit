//! Branches, remotes and tags — what the sidebar of DESIGN §4 lists.
//!
//! Everything is read in one pass and returned as owned data. The alternative,
//! handing back lazy iterators tied to the repository, would put the reads back
//! on whichever thread walks them — and that thread is the render thread
//! (SPEC §3 rule 2).

use crate::history::Signature;
use crate::repo::short_ref_name;
use crate::{Cancel, GitError, ObjectId, Repository, Result, assert_off_render_thread};

/// Every reference in the repository, grouped the way the sidebar groups them.
#[derive(Clone, Debug, Default)]
pub struct Refs {
    pub branches: Vec<Branch>,
    pub remote_branches: Vec<RemoteBranch>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
}

/// A local branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// `feature/login` — without `refs/heads/`. Grouping on `/` is the
    /// sidebar's job, not this crate's.
    pub name: String,
    pub commit: ObjectId,
    /// True for the branch `HEAD` is on.
    pub is_head: bool,
    /// Its upstream, when it has one that exists.
    pub tracking: Option<Tracking>,
}

/// A branch's relationship to its upstream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracking {
    /// `origin/main`.
    pub upstream: String,
    /// Commits on the branch that the upstream does not have.
    pub ahead: usize,
    /// Commits on the upstream that the branch does not have.
    pub behind: usize,
    /// The upstream is configured but no longer exists — the remote branch was
    /// deleted, or was never fetched. The sidebar says "gone" rather than
    /// showing 0/0, which would read as "up to date".
    pub gone: bool,
}

/// A branch on a remote, as last fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteBranch {
    /// `origin`.
    pub remote: String,
    /// `main`, without the remote's name.
    pub name: String,
    pub commit: ObjectId,
}

/// A configured remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    /// The fetch URL, as configured. `None` when the remote has no URL, which
    /// happens with a fetch-only or push-only configuration.
    pub url: Option<String>,
}

/// A tag, light or annotated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// The commit the tag ends up at, after peeling an annotated tag.
    pub commit: ObjectId,
    /// Present only for an annotated tag: its own object, message and tagger.
    pub annotation: Option<Annotation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    pub id: ObjectId,
    pub message: String,
    pub tagger: Option<Signature>,
}

/// The commit a ref points at, for a history scoped to it.
///
/// Local branches first, then remote-tracking ones, then tags. `main` therefore
/// means the branch you are on rather than `origin/main` when both exist —
/// the same order `git` resolves a name in, and the reason a history scoped to
/// "main" shows your commits and not the ones you have not pulled yet.
///
/// **Tags are here because the sidebar draws them and they answer a click.**
/// They did not used to: a tag row was inert, and the moment one became
/// clickable it asked this function for `v1.0.0` and was told "the branch
/// v1.0.0 not found in this repository" — true, unhelpful, and about the wrong
/// kind of thing. `peel_to_id` rather than `target().try_id()`, because an
/// annotated tag's ref points at the *tag object*: unpeeled, the history would
/// be asked to walk from something that is not a commit.
///
/// Deliberately still not `rev_parse`: what scopes a history here is a ref, not
/// an arbitrary revision, and accepting `HEAD~3` would be a second feature
/// nobody asked for with its own error cases.
pub fn tip_of(repo: &Repository, name: &str) -> Result<ObjectId> {
    assert_off_render_thread();
    let gix = repo.gix();
    for full in [
        format!("refs/heads/{name}"),
        format!("refs/remotes/{name}"),
        format!("refs/tags/{name}"),
    ] {
        let found = gix
            .try_find_reference(full.as_str())
            .map_err(|error| GitError::backend("looking a ref up", error))?;
        if let Some(mut reference) = found
            && let Ok(peeled) = reference.peel_to_id()
        {
            return Ok(peeled.detach());
        }
    }
    Err(GitError::NotFound(format!("the ref {name}")))
}

impl Refs {
    /// Read every reference.
    ///
    /// Ahead/behind is computed here rather than on demand because the sidebar
    /// shows it on every row at once; computing it lazily would mean one
    /// background job per visible branch.
    pub fn load(repo: &Repository, cancel: &Cancel) -> Result<Self> {
        assert_off_render_thread();
        let gix = repo.gix();
        let head_branch = match repo.head()? {
            crate::Head::Branch { branch, .. } | crate::Head::Unborn { branch } => Some(branch),
            crate::Head::Detached { .. } => None,
        };

        let platform = gix
            .references()
            .map_err(|error| GitError::backend("listing references", error))?;

        let mut branches = Vec::new();
        for reference in platform
            .local_branches()
            .map_err(|error| GitError::backend("listing branches", error))?
            .filter_map(std::result::Result::ok)
        {
            cancel.check()?;
            let name = short_ref_name(reference.name().as_bstr());
            let Some(commit) = reference.target().try_id().map(ToOwned::to_owned) else {
                // A symbolic local branch is legal but vanishingly rare, and it
                // has no commit of its own to show.
                continue;
            };
            let tracking = tracking_of(&gix, &reference, &commit, cancel)?;
            branches.push(Branch {
                is_head: head_branch.as_deref() == Some(name.as_str()),
                name,
                commit,
                tracking,
            });
        }

        let mut remote_branches = Vec::new();
        for reference in platform
            .remote_branches()
            .map_err(|error| GitError::backend("listing remote branches", error))?
            .filter_map(std::result::Result::ok)
        {
            cancel.check()?;
            let full = short_ref_name(reference.name().as_bstr());
            // `origin/main` → remote `origin`, branch `main`. A remote name
            // cannot contain a slash, so the first one splits it.
            let Some((remote, name)) = full.split_once('/') else {
                continue;
            };
            // `origin/HEAD` is a symbolic pointer to the remote's default
            // branch, not a branch anyone can check out.
            if name == "HEAD" {
                continue;
            }
            let Some(commit) = reference.target().try_id().map(ToOwned::to_owned) else {
                continue;
            };
            remote_branches.push(RemoteBranch {
                remote: remote.to_owned(),
                name: name.to_owned(),
                commit,
            });
        }

        let mut tags = Vec::new();
        for mut reference in platform
            .tags()
            .map_err(|error| GitError::backend("listing tags", error))?
            .filter_map(std::result::Result::ok)
        {
            cancel.check()?;
            let name = short_ref_name(reference.name().as_bstr());
            let pointee = reference.target().try_id().map(ToOwned::to_owned);
            // Peeling follows an annotated tag through to its commit; a
            // lightweight tag already points at one and peels to itself.
            let Ok(peeled) = reference.peel_to_id() else {
                continue;
            };
            let commit = peeled.detach();
            let annotation = pointee
                .filter(|id| *id != commit)
                .and_then(|id| annotation_of(&gix, id));
            tags.push(Tag {
                name,
                commit,
                annotation,
            });
        }

        let mut remotes = remotes(&gix);

        // Sorted here so every consumer — the sidebar, the CLI, the tests —
        // sees the same order. Reference iteration order is the packed-refs
        // file's, which changes when Git repacks.
        branches.sort_by(|a, b| a.name.cmp(&b.name));
        remote_branches.sort_by(|a, b| (&a.remote, &a.name).cmp(&(&b.remote, &b.name)));
        remotes.sort_by(|a, b| a.name.cmp(&b.name));
        tags.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self {
            branches,
            remote_branches,
            remotes,
            tags,
        })
    }

    pub fn head_branch(&self) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.is_head)
    }
}

/// The configured remotes, without touching a single commit.
///
/// Split out of [`Refs::load`] because the repository card needs the remotes
/// and nothing else about the references.
pub fn remotes(gix: &gix::Repository) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = gix
        .remote_names()
        .iter()
        .map(|name| {
            let name = name.to_string();
            let url = gix
                .try_find_remote(name.as_str())
                .and_then(std::result::Result::ok)
                .and_then(|remote| {
                    remote
                        .url(gix::remote::Direction::Fetch)
                        .map(ToString::to_string)
                });
            Remote { name, url }
        })
        .collect();
    remotes.sort_by(|a, b| a.name.cmp(&b.name));
    remotes
}

/// The divergence of the branch `HEAD` is on, and only that one.
///
/// [`Refs::load`] counts every branch against its upstream because the sidebar
/// of M7 shows every row at once. A repository *summary* shows one line, and on
/// a repository with fifty branches paying for fifty traversals to draw it
/// would be the difference between a list that appears and a list that arrives.
pub fn head_tracking(repo: &Repository, cancel: &Cancel) -> Result<Option<Tracking>> {
    assert_off_render_thread();
    let gix = repo.gix();
    let Some(reference) = gix
        .head_ref()
        .map_err(|error| GitError::backend("reading HEAD", error))?
    else {
        // Detached, or unborn: no branch, so nothing to be ahead of.
        return Ok(None);
    };
    let Some(commit) = reference.target().try_id().map(ToOwned::to_owned) else {
        return Ok(None);
    };
    tracking_of(&gix, &reference, &commit, cancel)
}

/// Resolve a branch's upstream and count the divergence.
fn tracking_of(
    gix: &gix::Repository,
    reference: &gix::Reference<'_>,
    local: &ObjectId,
    cancel: &Cancel,
) -> Result<Option<Tracking>> {
    let Some(Ok(upstream_ref)) = reference.remote_tracking_ref_name(gix::remote::Direction::Fetch)
    else {
        return Ok(None);
    };
    let upstream = short_ref_name(upstream_ref.as_ref().as_bstr());
    let Some(upstream_id) = gix
        .try_find_reference(upstream_ref.as_ref())
        .map_err(|error| GitError::backend("resolving the upstream branch", error))?
        .and_then(|mut reference| reference.peel_to_id().ok())
        .map(|id| id.detach())
    else {
        // Configured, but the remote branch is not here: deleted upstream, or
        // never fetched. Counting against nothing would say "0 ahead, 0 behind".
        return Ok(Some(Tracking {
            upstream,
            ahead: 0,
            behind: 0,
            gone: true,
        }));
    };

    Ok(Some(Tracking {
        upstream,
        ahead: count_exclusive(gix, *local, upstream_id, cancel)?,
        behind: count_exclusive(gix, upstream_id, *local, cancel)?,
        gone: false,
    }))
}

/// Both sides of `HEAD`'s divergence from its upstream, so a screen can name
/// the commits instead of only counting them: "3 to push" is a number, the
/// three summaries are the reason to look.
#[derive(Clone, Debug, Default)]
pub struct Divergence {
    /// `origin/main`.
    pub upstream: String,
    /// On the branch, not on the upstream — what a push would send. Newest
    /// first, at most the limit asked for.
    pub outgoing: Vec<crate::Commit>,
    /// On the upstream, not on the branch — what a pull would bring.
    pub incoming: Vec<crate::Commit>,
    /// The last commit both sides share, where the two lines of work parted.
    /// `None` when they share nothing.
    pub base: Option<ObjectId>,
}

/// `HEAD`'s divergence, commit by commit. `None` when `HEAD` is not on a branch
/// with an upstream that exists — the cases the tracking line already names.
pub fn head_divergence(
    repo: &Repository,
    limit: usize,
    cancel: &Cancel,
) -> Result<Option<Divergence>> {
    assert_off_render_thread();
    let gix = repo.gix();
    let Some(reference) = gix
        .head_ref()
        .map_err(|error| GitError::backend("reading HEAD", error))?
    else {
        return Ok(None);
    };
    let Some(local) = reference.target().try_id().map(ToOwned::to_owned) else {
        return Ok(None);
    };
    let Some(Ok(upstream_ref)) = reference.remote_tracking_ref_name(gix::remote::Direction::Fetch)
    else {
        return Ok(None);
    };
    let upstream = short_ref_name(upstream_ref.as_ref().as_bstr());
    let Some(remote) = gix
        .try_find_reference(upstream_ref.as_ref())
        .map_err(|error| GitError::backend("resolving the upstream branch", error))?
        .and_then(|mut reference| reference.peel_to_id().ok())
        .map(|id| id.detach())
    else {
        return Ok(None);
    };
    Ok(Some(Divergence {
        upstream,
        outgoing: exclusive_commits(repo, &gix, local, remote, limit, cancel)?,
        incoming: exclusive_commits(repo, &gix, remote, local, limit, cancel)?,
        base: gix.merge_base(local, remote).ok().map(|id| id.detach()),
    }))
}

/// One branch on the map, measured against the trunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapBranch {
    pub name: String,
    pub head: bool,
    /// When its tip was authored, in Unix seconds.
    pub tip_time: i64,
    /// When the commit it shares with the trunk was authored — where it
    /// "left" main. `None` for a branch with no history in common.
    pub base_time: Option<i64>,
    /// Commits it has that the trunk does not.
    pub ahead: usize,
    /// Commits the trunk has that it does not.
    pub behind: usize,
    /// Its own commits — the `ahead` ones — newest first, at most
    /// [`MAP_COMMITS`], for the dots along its line.
    pub commits: Vec<MapCommit>,
}

/// One commit on the map: when it was authored, and whether it is a merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapCommit {
    pub time: i64,
    pub merge: bool,
}

/// How many commits a line on the map carries at most. The dots are there to
/// show rhythm — a burst, a pause — and past a few hundred they are a solid
/// line anyway; the counts beside each branch stay exact.
pub const MAP_COMMITS: usize = 300;

/// Every local branch against the trunk (DESIGN v2's branch map).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchMap {
    pub trunk: String,
    pub trunk_time: i64,
    pub branches: Vec<MapBranch>,
    /// Every tag that points at a commit, and when that commit was authored,
    /// for the marks along the trunk.
    pub tags: Vec<(String, i64)>,
    /// The trunk's own line, first parents from its tip, newest first, at
    /// most [`MAP_COMMITS`].
    pub trunk_commits: Vec<MapCommit>,
}

/// The names a trunk goes by, in the order one is chosen when several exist.
const TRUNKS: [&str; 4] = ["main", "master", "trunk", "develop"];

/// Every local branch measured against the trunk: where it left it, how far
/// ahead and behind it is. `None` when no branch is called like a trunk — a
/// map with no main line has nothing to draw the others against.
pub fn branch_map(repo: &Repository, cancel: &Cancel) -> Result<Option<BranchMap>> {
    assert_off_render_thread();
    let gix = repo.gix();
    let head_branch = match repo.head()? {
        crate::Head::Branch { branch, .. } => Some(branch),
        _ => None,
    };
    let platform = gix
        .references()
        .map_err(|error| GitError::backend("listing references", error))?;
    let mut tips = Vec::new();
    for reference in platform
        .local_branches()
        .map_err(|error| GitError::backend("listing branches", error))?
        .filter_map(std::result::Result::ok)
    {
        cancel.check()?;
        if let Some(commit) = reference.target().try_id().map(ToOwned::to_owned) {
            tips.push((short_ref_name(reference.name().as_bstr()), commit));
        }
    }
    let Some((trunk, trunk_tip)) = TRUNKS
        .iter()
        .find_map(|name| tips.iter().find(|(branch, _)| branch == name).cloned())
    else {
        return Ok(None);
    };
    let time =
        |id: ObjectId| -> Result<i64> { Ok(crate::history::commit(repo, id)?.author.time.seconds) };

    let mut branches = Vec::new();
    for (name, tip) in tips.into_iter().filter(|(name, _)| *name != trunk) {
        cancel.check()?;
        let base = gix.merge_base(tip, trunk_tip).ok().map(|id| id.detach());
        branches.push(MapBranch {
            commits: map_commits(repo, &gix, tip, Some(trunk_tip), false, cancel)?,
            head: head_branch.as_deref() == Some(name.as_str()),
            tip_time: time(tip)?,
            base_time: base.map(time).transpose()?,
            ahead: count_exclusive(&gix, tip, trunk_tip, cancel)?,
            behind: count_exclusive(&gix, trunk_tip, tip, cancel)?,
            name,
        });
    }
    let mut tags = Vec::new();
    for mut reference in platform
        .tags()
        .map_err(|error| GitError::backend("listing tags", error))?
        .filter_map(std::result::Result::ok)
    {
        cancel.check()?;
        let name = short_ref_name(reference.name().as_bstr());
        // A tag of a tree or a blob has no time to be placed at; it is left off.
        if let Ok(id) = reference.peel_to_id()
            && let Ok(at) = time(id.detach())
        {
            tags.push((name, at));
        }
    }
    Ok(Some(BranchMap {
        trunk_time: time(trunk_tip)?,
        trunk_commits: map_commits(repo, &gix, trunk_tip, None, true, cancel)?,
        trunk,
        branches,
        tags,
    }))
}

/// The commits from `tip` — down first parents only for the trunk, or all of
/// them not reachable from `hidden` for a branch — newest first, capped.
fn map_commits(
    repo: &Repository,
    gix: &gix::Repository,
    tip: ObjectId,
    hidden: Option<ObjectId>,
    first_parent: bool,
    cancel: &Cancel,
) -> Result<Vec<MapCommit>> {
    if hidden == Some(tip) {
        return Ok(Vec::new());
    }
    let mut platform = gix.rev_walk(Some(tip)).with_hidden(hidden);
    if first_parent {
        platform = platform.first_parent_only();
    }
    let walk = platform
        .all()
        .map_err(|error| GitError::backend("drawing the branch map", error))?;
    let mut commits = Vec::new();
    for info in walk.take(MAP_COMMITS) {
        cancel.check()?;
        let info = info.map_err(|error| GitError::backend("drawing the branch map", error))?;
        commits.push(MapCommit {
            time: crate::history::commit(repo, info.id)?.author.time.seconds,
            merge: info.parent_ids.len() > 1,
        });
    }
    commits.sort_by_key(|commit| std::cmp::Reverse(commit.time));
    Ok(commits)
}

/// The commits on `branch` that the trunk `onto` does not have, newest first.
pub fn branch_commits(
    repo: &Repository,
    branch: &str,
    onto: &str,
    limit: usize,
    cancel: &Cancel,
) -> Result<Vec<crate::Commit>> {
    assert_off_render_thread();
    let gix = repo.gix();
    let tip = tip_of(repo, branch)?;
    let hidden = tip_of(repo, onto)?;
    exclusive_commits(repo, &gix, tip, hidden, limit, cancel)
}

/// The commits reachable from `tip` and not from `hidden`, newest first.
fn exclusive_commits(
    repo: &Repository,
    gix: &gix::Repository,
    tip: ObjectId,
    hidden: ObjectId,
    limit: usize,
    cancel: &Cancel,
) -> Result<Vec<crate::Commit>> {
    if tip == hidden || limit == 0 {
        return Ok(Vec::new());
    }
    let walk = gix
        .rev_walk(Some(tip))
        .with_hidden(Some(hidden))
        .all()
        .map_err(|error| GitError::backend("listing a divergence", error))?;
    let mut commits = Vec::new();
    for info in walk {
        cancel.check()?;
        let info = info.map_err(|error| GitError::backend("listing a divergence", error))?;
        commits.push(crate::history::commit(repo, info.id)?);
    }
    // The walk's order is the graph's, not the clock's; a reader expects the
    // newest at the top. Sorted on all of them before cutting, so the limit
    // keeps the newest rather than whichever the walk met first.
    commits.sort_by_key(|commit| std::cmp::Reverse(commit.author.time.seconds));
    commits.truncate(limit);
    Ok(commits)
}

/// How many commits are reachable from `tip` but not from `hidden`.
///
/// `git rev-list --count tip ^hidden`, through `gix`'s hidden-tip traversal.
/// Note that hiding is not free: `gix` has to paint the hidden side before it
/// can be sure a commit is wanted, so two long-diverged branches cost a walk of
/// both. That is why the count is done once per branch when the sidebar loads,
/// and not per keystroke.
fn count_exclusive(
    gix: &gix::Repository,
    tip: ObjectId,
    hidden: ObjectId,
    cancel: &Cancel,
) -> Result<usize> {
    if tip == hidden {
        return Ok(0);
    }
    let walk = gix
        .rev_walk(Some(tip))
        .with_hidden(Some(hidden))
        .all()
        .map_err(|error| GitError::backend("counting ahead/behind", error))?;
    let mut count = 0;
    for commit in walk {
        cancel.check()?;
        commit.map_err(|error| GitError::backend("counting ahead/behind", error))?;
        count += 1;
    }
    Ok(count)
}

fn annotation_of(gix: &gix::Repository, id: ObjectId) -> Option<Annotation> {
    let object = gix.find_object(id).ok()?;
    let tag = object.try_into_tag().ok()?;
    let reference = tag.decode().ok()?;
    Some(Annotation {
        id,
        message: reference.message.to_string(),
        tagger: reference.tagger().ok().flatten().map(Signature::from_ref),
    })
}
