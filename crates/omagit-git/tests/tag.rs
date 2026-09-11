//! Tag operations, against real repositories.
//!
//! Every assertion asks `git` what the repository looks like afterwards rather
//! than trusting the command that ran. The distinction that matters most here
//! is the one between the two kinds of tag, because it is invisible from a
//! name: a lightweight tag is a ref, an annotated one is an object with a
//! tagger and a message, and only the second is what a release is expected to
//! be. `cat-file -t` is what tells them apart, so that is what is asked.

mod support;

use omagit_git::cli::Git;
use omagit_git::ops::tag;
use omagit_git::refs::Refs;
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// A repository with two commits, so a tag can be put somewhere that is not
/// `HEAD`.
fn repository() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    repo.commit_file("more.txt", "two\n", "second");
    repo
}

fn kind(repo: &TestRepo, name: &str) -> String {
    repo.git(&["cat-file", "-t", name])
}

fn tags(repo: &TestRepo) -> Vec<String> {
    repo.git(&["tag", "--list"])
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn a_tag_with_no_message_is_a_ref_and_nothing_else() {
    let repo = repository();
    tag::create(&git(), &repo.open(), "v0.1.0", None, None, false, &never()).expect("created");

    assert_eq!(tags(&repo), vec!["v0.1.0"]);
    // A lightweight tag resolves straight to the commit: there is no tag object
    // between the name and it.
    assert_eq!(kind(&repo, "v0.1.0"), "commit");
    assert_eq!(
        repo.git(&["rev-parse", "v0.1.0"]),
        repo.git(&["rev-parse", "HEAD"]),
        "at HEAD, which is where a tag with no target goes"
    );
}

#[test]
fn a_tag_with_a_message_is_an_object_carrying_it() {
    let repo = repository();
    let message = "Première version publiée.\n\nAvec un corps, et une ligne vide avant lui.";
    tag::create(
        &git(),
        &repo.open(),
        "v1.0.0",
        None,
        Some(message),
        false,
        &never(),
    )
    .expect("created");

    assert_eq!(kind(&repo, "v1.0.0"), "tag", "an object of its own");
    let said = repo.git(&["tag", "--list", "--format=%(contents)", "v1.0.0"]);
    assert!(
        said.contains("Première version publiée."),
        "the message survives, accents included: {said}"
    );
    assert!(
        said.contains("une ligne vide avant lui"),
        "and so does the body, which is why it goes on stdin: {said}"
    );
    assert!(
        !repo
            .git(&["tag", "--list", "--format=%(taggername)", "v1.0.0"])
            .is_empty(),
        "an annotated tag has a tagger; that is what makes it annotated"
    );
}

#[test]
fn a_tag_can_be_put_on_a_commit_that_is_not_head() {
    let repo = repository();
    let first = repo.git(&["rev-parse", "HEAD~1"]);
    tag::create(
        &git(),
        &repo.open(),
        "v0.0.1",
        Some("HEAD~1"),
        None,
        false,
        &never(),
    )
    .expect("created");

    assert_eq!(repo.git(&["rev-parse", "v0.0.1"]), first);
}

#[test]
fn a_name_already_taken_is_refused_until_it_is_forced() {
    let repo = repository();
    let git = git();
    tag::create(&git, &repo.open(), "v1", None, None, false, &never()).expect("created");
    let was = repo.git(&["rev-parse", "v1"]);

    // Refused, and it is `git`'s own refusal that reaches the user (SPEC §3
    // rule 3): it names the tag, which is better than anything composed here.
    let refused = tag::create(
        &git,
        &repo.open(),
        "v1",
        Some("HEAD~1"),
        None,
        false,
        &never(),
    );
    assert!(refused.is_err(), "a tag is not silently moved");
    assert_eq!(repo.git(&["rev-parse", "v1"]), was, "and it did not move");

    tag::create(
        &git,
        &repo.open(),
        "v1",
        Some("HEAD~1"),
        None,
        true,
        &never(),
    )
    .expect("forced");
    assert_eq!(
        repo.git(&["rev-parse", "v1"]),
        repo.git(&["rev-parse", "HEAD~1"])
    );
}

#[test]
fn deleting_takes_the_tag_and_leaves_the_commit() {
    let repo = repository();
    let git = git();
    let head = repo.git(&["rev-parse", "HEAD"]);
    tag::create(
        &git,
        &repo.open(),
        "v1",
        None,
        Some("shipped"),
        false,
        &never(),
    )
    .expect("created");

    tag::delete(&git, &repo.open(), "v1", &never()).expect("deleted");

    assert!(tags(&repo).is_empty());
    assert_eq!(
        repo.git(&["rev-parse", "HEAD"]),
        head,
        "the commit the tag named is untouched"
    );
}

#[test]
fn the_refs_the_sidebar_reads_carry_the_annotation() {
    // The read side already peeled annotated tags; nothing had ever *made* one
    // for it to peel. This is the two halves meeting.
    let repo = repository();
    let git = git();
    tag::create(&git, &repo.open(), "light", None, None, false, &never()).expect("created");
    tag::create(
        &git,
        &repo.open(),
        "heavy",
        None,
        Some("with a message"),
        false,
        &never(),
    )
    .expect("created");

    let refs = Refs::load(&repo.open(), &never()).expect("refs");
    let names: Vec<&str> = refs.tags.iter().map(|one| one.name.as_str()).collect();
    assert!(
        names.contains(&"light") && names.contains(&"heavy"),
        "{names:?}"
    );

    let light = refs
        .tags
        .iter()
        .find(|one| one.name == "light")
        .expect("there");
    let heavy = refs
        .tags
        .iter()
        .find(|one| one.name == "heavy")
        .expect("there");
    assert!(light.annotation.is_none(), "a ref has nothing to annotate");
    let annotation = heavy.annotation.as_ref().expect("an object of its own");
    assert!(annotation.message.contains("with a message"));
    // Both name the same commit: peeling is what makes the two comparable.
    assert_eq!(light.commit, heavy.commit);
}

#[test]
fn a_history_can_be_scoped_to_a_tag_of_either_kind() {
    // A tag row in the sidebar answers a click now. Before this, the click
    // asked for a *branch* by that name and was told "the branch v1.0.0 not
    // found in this repository" — true, unhelpful, and about the wrong kind of
    // thing. The annotated case is the one that would still have been wrong
    // after a naive fix: its ref points at the tag object, not at a commit.
    let repo = repository();
    let git = git();
    let first = repo.git(&["rev-parse", "HEAD~1"]);
    tag::create(
        &git,
        &repo.open(),
        "light",
        Some("HEAD~1"),
        None,
        false,
        &never(),
    )
    .expect("created");
    tag::create(
        &git,
        &repo.open(),
        "heavy",
        Some("HEAD~1"),
        Some("a message"),
        false,
        &never(),
    )
    .expect("created");

    let opened = repo.open();
    let light = omagit_git::refs::tip_of(&opened, "light").expect("a lightweight tag resolves");
    let heavy = omagit_git::refs::tip_of(&opened, "heavy").expect("an annotated one too");

    assert_eq!(light.to_string(), first);
    assert_eq!(
        heavy.to_string(),
        first,
        "peeled to the commit, not the object"
    );
    // And a branch still wins over a tag of the same name, which is the order
    // `git` itself resolves in.
    assert!(omagit_git::refs::tip_of(&opened, "nothing").is_err());
}

#[test]
fn asking_the_remote_names_the_tags_it_has_and_no_others() {
    // Nothing in the repository can answer this: a tag fetched from a remote
    // sits in `refs/tags/` exactly where a local one does.
    let remote = tempfile::tempdir().expect("a temporary directory");
    std::process::Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(remote.path())
        .output()
        .expect("git is installed");

    let repo = repository();
    repo.git(&[
        "remote",
        "add",
        "origin",
        &remote.path().display().to_string(),
    ]);
    repo.git(&["push", "--set-upstream", "origin", "main"]);

    let git = git();
    let opened = repo.open();
    assert!(
        tag::on_remote(&git, &opened, "origin", &never())
            .expect("it answers")
            .is_empty(),
        "nothing published yet"
    );

    // One of each kind, and only the annotated one is published — the peeled
    // line it answers with names the same tag and must not be counted twice.
    tag::create(&git, &opened, "light", None, None, false, &never()).expect("created");
    tag::create(
        &git,
        &opened,
        "heavy",
        None,
        Some("a message"),
        false,
        &never(),
    )
    .expect("created");
    tag::push(&git, &opened, "origin", "heavy", &never()).expect("pushed");

    let there = tag::on_remote(&git, &opened, "origin", &never()).expect("it answers");
    assert_eq!(there, vec!["heavy"], "once, and without the local one");
}

#[test]
fn publishing_puts_one_tag_on_the_remote_and_unpublishing_takes_it_back() {
    let remote = tempfile::tempdir().expect("a temporary directory");
    std::process::Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(remote.path())
        .output()
        .expect("git is installed");

    let repo = repository();
    repo.git(&[
        "remote",
        "add",
        "origin",
        &remote.path().display().to_string(),
    ]);
    repo.git(&["push", "--set-upstream", "origin", "main"]);

    let git = git();
    tag::create(
        &git,
        &repo.open(),
        "v1",
        None,
        Some("shipped"),
        false,
        &never(),
    )
    .expect("created");
    // A second one, to hold the rule that says one tag is pushed and not all of
    // them: `--tags` would publish whatever happens to be lying around.
    tag::create(&git, &repo.open(), "scratch", None, None, false, &never()).expect("created");

    tag::push(&git, &repo.open(), "origin", "v1", &never()).expect("pushed");

    let published = std::process::Command::new("git")
        .args(["tag", "--list"])
        .current_dir(remote.path())
        .output()
        .expect("git runs");
    let published = String::from_utf8_lossy(&published.stdout);
    assert!(
        published.contains("v1"),
        "the one that was named: {published}"
    );
    assert!(
        !published.contains("scratch"),
        "and only that one: {published}"
    );

    tag::unpublish(&git, &repo.open(), "origin", "v1", &never()).expect("unpublished");

    let after = std::process::Command::new("git")
        .args(["tag", "--list"])
        .current_dir(remote.path())
        .output()
        .expect("git runs");
    assert!(String::from_utf8_lossy(&after.stdout).trim().is_empty());
    assert_eq!(
        tags(&repo),
        vec!["scratch", "v1"],
        "and the local ones are untouched: the two are different decisions"
    );
}
