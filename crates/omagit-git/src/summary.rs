//! What a repository looks like at a glance.
//!
//! The Repositories screen draws two things from every repository it lists: one
//! line in the sidebar (`main ↑8 ↓2`, `merge · 2 conflits`) and a detail card
//! with the whole picture. Both come from here, in one read, because they are
//! read together and a screen that fired six background jobs per row would
//! spend its life cancelling them.
//!
//! Everything in it is a composition of the reads the other modules already do.
//! It lives in this crate rather than in the app for the reason SPEC §3 rule 5
//! gives: deciding that "a repository is dirty when the working copy has
//! entries the index does not" is Git knowledge, and Git knowledge does not
//! belong in a component.

use crate::{
    Cancel, Commit, Head, HistoryQuery, Operation, Remote, Repository, Result, Status,
    StatusOptions, Tracking, Walk, assert_off_render_thread,
};

/// The window the activity sparkline covers (DESIGN §4: 90 days, one bar per
/// three days, so 30 bars).
pub const ACTIVITY_DAYS: i64 = 90;
pub const ACTIVITY_BUCKET_DAYS: i64 = 3;
pub const ACTIVITY_BUCKETS: usize = (ACTIVITY_DAYS / ACTIVITY_BUCKET_DAYS) as usize;

/// Everything the Repositories screen shows about one repository.
#[derive(Clone, Debug)]
pub struct Summary {
    pub head: Head,
    /// A half-finished merge, rebase or cherry-pick. The row says so instead of
    /// the branch, because "on main" is misleading while a merge is stuck.
    pub operation: Option<Operation>,
    /// The upstream of the branch `HEAD` is on, if it has one.
    pub tracking: Option<Tracking>,
    pub counts: Counts,
    /// The tip commit, for the "Last Commit" line. `None` in a repository with
    /// no commits.
    pub last_commit: Option<Commit>,
    pub stashes: usize,
    pub remotes: Vec<Remote>,
    pub activity: Activity,
    /// Who a commit made here would be attributed to, and whether that comes
    /// from this repository or from the global configuration.
    pub committer: Option<Identity>,
}

impl Summary {
    /// Read everything, in the order that fails soonest: a repository that has
    /// moved should not cost a directory walk before saying so.
    pub fn load(repo: &Repository, cancel: &Cancel) -> Result<Self> {
        assert_off_render_thread();
        let head = repo.head()?;
        let operation = repo.operation();

        let status = Status::load(repo, StatusOptions::default(), cancel)?;
        let counts = Counts::of(&status);

        // Deliberately not `Refs::load`: that counts every branch against its
        // upstream, and a summary shows exactly one line.
        let tracking = crate::refs::head_tracking(repo, cancel)?;
        let remotes = crate::refs::remotes(&repo.gix());

        let last_commit = match head.commit() {
            Some(id) => Some(crate::history::commit(repo, *id)?),
            None => None,
        };

        Ok(Self {
            head,
            operation,
            tracking,
            counts,
            last_commit,
            stashes: stash_count(repo),
            remotes,
            activity: Activity::load(repo, cancel)?,
            committer: identity(repo),
        })
    }
}

/// The working copy in numbers — what the sidebar row and the card's Status
/// block both need.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub modified: usize,
    pub added: usize,
    pub deleted: usize,
    pub renamed: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

impl Counts {
    fn of(status: &Status) -> Self {
        use crate::{StageChange, WorktreeChange};
        let mut counts = Self::default();
        for entry in &status.entries {
            // A conflict is counted once and as nothing else: it is the state
            // that has to be resolved before any of the others matter.
            if entry.conflict.is_some() {
                counts.conflicted += 1;
                continue;
            }
            match (&entry.staged, &entry.unstaged) {
                (_, Some(WorktreeChange::Untracked)) => counts.untracked += 1,
                (_, Some(WorktreeChange::Ignored)) => {}
                (Some(StageChange::Added), _) => counts.added += 1,
                (Some(StageChange::Deleted), _) | (_, Some(WorktreeChange::Deleted)) => {
                    counts.deleted += 1;
                }
                (Some(StageChange::Renamed { .. }), _)
                | (_, Some(WorktreeChange::Renamed { .. })) => counts.renamed += 1,
                _ => counts.modified += 1,
            }
        }
        counts
    }

    /// Everything except untracked files: what "the working copy has changes"
    /// means for the square status pip of DESIGN §4.
    pub fn tracked_changes(&self) -> usize {
        self.modified + self.added + self.deleted + self.renamed + self.conflicted
    }

    pub fn is_clean(&self) -> bool {
        self.tracked_changes() == 0 && self.untracked == 0
    }
}

/// Commits per three-day bucket over the last ninety days, oldest bucket first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub buckets: Vec<u32>,
    /// The total over the window — the "312 commits" beside the sparkline.
    pub total: u32,
}

impl Activity {
    fn load(repo: &Repository, cancel: &Cancel) -> Result<Self> {
        let now = unix_now();
        let cutoff = now - ACTIVITY_DAYS * 86_400;
        let mut buckets = vec![0u32; ACTIVITY_BUCKETS];
        let mut total = 0;

        let mut walk = Walk::new(repo, HistoryQuery::head(), cancel)?;
        'walk: loop {
            let page = walk.next_page(256, cancel)?;
            if page.is_empty() {
                break;
            }
            for commit in page {
                let seconds = commit.committer.time.seconds;
                // The walk is newest-first, so the first commit older than the
                // window means every remaining one is too.
                if seconds < cutoff {
                    break 'walk;
                }
                // Bucket 0 is the oldest three days of the window, so the
                // sparkline reads left to right like every other timeline.
                let age_days = (now - seconds).max(0) / 86_400;
                let from_end = (age_days / ACTIVITY_BUCKET_DAYS) as usize;
                if from_end < ACTIVITY_BUCKETS {
                    buckets[ACTIVITY_BUCKETS - 1 - from_end] += 1;
                    total += 1;
                }
            }
        }
        Ok(Self { buckets, total })
    }

    /// The tallest bucket, which is what the sparkline scales against. Never
    /// zero, so a caller can divide by it.
    pub fn peak(&self) -> u32 {
        self.buckets.iter().copied().max().unwrap_or(0).max(1)
    }
}

/// The committer a commit made here would carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
    /// True when the value comes from the global or system configuration rather
    /// than from this repository — the card says "hérité du global", because a
    /// per-repository identity is a deliberate act and worth distinguishing.
    pub inherited: bool,
}

impl Identity {
    /// `Élodie Laurent` → `EL`. First letters of the first two words, which is
    /// what the card's avatar shows.
    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    }
}

fn identity(repo: &Repository) -> Option<Identity> {
    let gix = repo.gix();
    let snapshot = gix.config_snapshot();
    let name = snapshot.string("user.name")?.to_string();
    let email = snapshot.string("user.email")?.to_string();
    // Asked twice: once for the effective value, once restricted to the files
    // that belong to this repository. Absent from the second means inherited.
    let local = snapshot
        .plumbing()
        .string_filter("user.name", |metadata| {
            matches!(
                metadata.source,
                gix::config::Source::Local | gix::config::Source::Worktree
            )
        })
        .is_some();
    Some(Identity {
        name,
        email,
        inherited: !local,
    })
}

/// How many entries `refs/stash` holds.
///
/// A stash is a reflog entry, not a reference per stash, so counting them means
/// reading that log. Failure is not an error: a repository that has never
/// stashed has no log at all, which is the same answer as zero.
fn stash_count(repo: &Repository) -> usize {
    let gix = repo.gix();
    let Ok(Some(reference)) = gix.try_find_reference("refs/stash") else {
        return 0;
    };
    let mut platform = reference.log_iter();
    match platform.all() {
        Ok(Some(entries)) => entries.filter(|entry| entry.is_ok()).count(),
        _ => 0,
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        // Before 1970 the clock is wrong in a way no client can fix; the window
        // simply comes out empty.
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_are_the_first_two_words() {
        let identity = |name: &str| Identity {
            name: name.to_owned(),
            email: "e@example.test".into(),
            inherited: false,
        };
        assert_eq!(identity("Élodie Laurent").initials(), "ÉL");
        assert_eq!(identity("marek kowal").initials(), "MK");
        assert_eq!(identity("Cher").initials(), "C");
        assert_eq!(
            identity("Ada Lovelace Byron").initials(),
            "AL",
            "two letters, never three"
        );
        assert_eq!(identity("").initials(), "");
    }

    #[test]
    fn a_peak_is_never_zero() {
        // The sparkline divides by it.
        assert_eq!(Activity::default().peak(), 1);
        assert_eq!(
            Activity {
                buckets: vec![0, 3, 1],
                total: 4
            }
            .peak(),
            3
        );
    }
}
