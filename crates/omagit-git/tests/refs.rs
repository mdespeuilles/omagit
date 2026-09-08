//! Branches, remotes and tags, and the ahead/behind counts the sidebar shows.

mod support;

use omagit_git::{Refs, Repository};
use support::{TestRepo, never, scripted};

fn refs(fixture: &TestRepo) -> Refs {
    Refs::load(&fixture.open(), &never()).expect("references are readable")
}

/// A repository with a real upstream: a bare repository on disk, pushed to.
///
/// Local paths rather than a network remote, so the test needs no daemon and no
/// credentials — the ahead/behind arithmetic is the same either way.
fn with_upstream() -> (TestRepo, TestRepo) {
    let origin = TestRepo::bare();
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    fixture.commit_file("b.txt", "b\n", "second");
    fixture.git(&["remote", "add", "origin", &origin.path().to_string_lossy()]);
    fixture.git(&["push", "-u", "origin", "main"]);
    (fixture, origin)
}

#[test]
fn lists_local_branches_and_marks_the_one_head_is_on() {
    let fixture = scripted();
    fixture.git(&["branch", "another"]);

    let refs = refs(&fixture);
    let names: Vec<&str> = refs
        .branches
        .iter()
        .map(|branch| branch.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["another", "feature", "main"],
        "sorted, not packed-refs order"
    );

    let head = refs.head_branch().expect("HEAD is on a branch");
    assert_eq!(head.name, "main");
    assert_eq!(head.commit, fixture.head());
}

#[test]
fn a_detached_head_marks_no_branch() {
    let fixture = scripted();
    fixture.checkout(&fixture.head().to_string());

    let refs = refs(&fixture);
    assert!(refs.head_branch().is_none());
    assert!(!refs.branches.is_empty(), "the branches are still there");
}

#[test]
fn counts_ahead_and_behind_against_the_upstream() {
    let (fixture, _origin) = with_upstream();

    let tracking = refs(&fixture)
        .head_branch()
        .expect("on main")
        .tracking
        .clone()
        .expect("main tracks origin/main");
    assert_eq!(tracking.upstream, "origin/main");
    assert_eq!((tracking.ahead, tracking.behind), (0, 0));
    assert!(!tracking.gone);

    fixture.commit_file("c.txt", "c\n", "third");
    fixture.commit_file("d.txt", "d\n", "fourth");
    let ahead = refs(&fixture)
        .head_branch()
        .unwrap()
        .tracking
        .clone()
        .unwrap();
    assert_eq!((ahead.ahead, ahead.behind), (2, 0));

    // Rewind past the pushed tip: now the upstream is ahead of us instead.
    fixture.git(&["reset", "--hard", "HEAD~3"]);
    let behind = refs(&fixture)
        .head_branch()
        .unwrap()
        .tracking
        .clone()
        .unwrap();
    assert_eq!((behind.ahead, behind.behind), (0, 1));
}

#[test]
fn an_upstream_that_no_longer_exists_is_reported_as_gone() {
    let (fixture, _origin) = with_upstream();
    // The branch was deleted on the remote and the deletion fetched.
    fixture.git(&["update-ref", "-d", "refs/remotes/origin/main"]);

    let tracking = refs(&fixture)
        .head_branch()
        .unwrap()
        .tracking
        .clone()
        .expect("the configuration still names an upstream");
    assert!(
        tracking.gone,
        "0 ahead / 0 behind would read as 'up to date', which is the opposite of the truth"
    );
}

#[test]
fn a_branch_with_no_upstream_tracks_nothing() {
    let fixture = scripted();
    assert!(refs(&fixture).head_branch().unwrap().tracking.is_none());
}

#[test]
fn lists_remotes_and_their_branches() {
    let (fixture, origin) = with_upstream();
    let refs = refs(&fixture);

    assert_eq!(refs.remotes.len(), 1);
    assert_eq!(refs.remotes[0].name, "origin");
    assert_eq!(
        refs.remotes[0].url.as_deref(),
        Some(origin.path().to_string_lossy().as_ref())
    );

    assert_eq!(refs.remote_branches.len(), 1);
    assert_eq!(refs.remote_branches[0].remote, "origin");
    assert_eq!(
        refs.remote_branches[0].name, "main",
        "the remote's name is not part of the branch's"
    );
}

#[test]
fn reads_both_kinds_of_tag() {
    let fixture = TestRepo::new();
    let commit = fixture.commit_file("a.txt", "a\n", "first");
    fixture.git(&["tag", "v1-light"]);
    fixture.git(&["tag", "-a", "v1", "-m", "the first release"]);

    let refs = refs(&fixture);
    let names: Vec<&str> = refs.tags.iter().map(|tag| tag.name.as_str()).collect();
    assert_eq!(names, vec!["v1", "v1-light"]);

    let light = &refs.tags[1];
    assert_eq!(light.commit, commit);
    assert!(light.annotation.is_none());

    let annotated = &refs.tags[0];
    assert_eq!(
        annotated.commit, commit,
        "an annotated tag is peeled through to its commit"
    );
    let annotation = annotated.annotation.as_ref().expect("its own object");
    assert_eq!(annotation.message.trim(), "the first release");
    assert_eq!(
        annotation
            .tagger
            .as_ref()
            .map(|tagger| tagger.name.as_str()),
        Some("Test Committer")
    );
}

#[test]
fn keeps_the_slashes_in_a_branch_name() {
    let fixture = TestRepo::new();
    fixture.commit_file("a.txt", "a\n", "first");
    fixture.branch("feature/deeply/nested");

    let refs = refs(&fixture);
    assert!(
        refs.branches
            .iter()
            .any(|branch| branch.name == "feature/deeply/nested"),
        "grouping on `/` is the sidebar's job, so the name arrives whole"
    );
}

#[test]
fn an_empty_repository_has_no_references() {
    let fixture = TestRepo::new();
    let refs = refs(&fixture);
    assert!(
        refs.branches.is_empty(),
        "the unborn branch does not exist yet"
    );
    assert!(refs.tags.is_empty());
    assert!(refs.remotes.is_empty());
}

#[test]
fn a_reference_read_can_be_cancelled() {
    let fixture = scripted();
    let cancel = omagit_git::Cancel::new();
    cancel.cancel();

    let error = Refs::load(&fixture.open(), &cancel).expect_err("a cancelled read returns");
    assert!(error.is_cancelled(), "got {error:?}");
}

#[test]
fn a_bare_repository_still_lists_its_branches() {
    let (fixture, origin) = with_upstream();
    let _ = fixture;
    let refs = Refs::load(
        &Repository::open(origin.path()).expect("a repository"),
        &never(),
    )
    .expect("references are readable without a work tree");
    assert!(refs.branches.iter().any(|branch| branch.name == "main"));
}
