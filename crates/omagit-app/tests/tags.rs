//! What the sidebar is sent about a tag.
//!
//! `omagit-git` proves that an annotated tag's message is read back off the
//! object. What is proved here is the step after it — that the message survives
//! the crossing into the window — because that is the half a front-end test
//! cannot see: its fake backend answers whatever the fake was told to answer.

mod support;

use omagit_git::cli::Git;
use omagit_git::ops::tag;
use omagit_git::refs::Refs;
use omagit_lib::dto;
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

fn rows(repo: &TestRepo) -> Vec<dto::TagRow> {
    let opened = repo.open();
    let refs = Refs::load(&opened, &never()).expect("refs");
    dto::refs(&opened, &refs, &Default::default(), 0).tags
}

#[test]
fn the_message_reaches_the_window_and_the_kind_does_not() {
    // The row used to carry a boolean saying which of Git's two kinds of tag
    // this is — the word "annotated". It carries the message instead: the kind
    // offers the reader no choice and is Git's own vocabulary, while the
    // message is the thing somebody wrote.
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");
    let git = git();

    tag::create(
        &git,
        &repo.open(),
        "v1.0.0",
        None,
        Some("Première version montrable"),
        false,
        &never(),
    )
    .expect("created");
    tag::create(&git, &repo.open(), "xx", None, None, false, &never()).expect("created");

    let tags = rows(&repo);
    let written = tags
        .iter()
        .find(|row| row.name == "v1.0.0")
        .expect("listed");
    let bare = tags.iter().find(|row| row.name == "xx").expect("listed");

    assert_eq!(
        written.message, "Première version montrable",
        "the message a person typed, accents and all"
    );
    assert_eq!(bare.message, "", "nothing was written, so there is nothing");
}

#[test]
fn a_message_of_several_lines_arrives_whole() {
    // The sidebar shows the first line and the tooltip shows all of it, so the
    // row cannot be handed a subject alone.
    let repo = TestRepo::new();
    repo.commit_file("README.md", "one\n", "seed");

    tag::create(
        &git(),
        &repo.open(),
        "v2.0.0",
        None,
        Some("Un sujet\n\nEt un corps qui explique."),
        false,
        &never(),
    )
    .expect("created");

    let tags = rows(&repo);
    let row = tags
        .iter()
        .find(|one| one.name == "v2.0.0")
        .expect("listed");
    assert!(row.message.starts_with("Un sujet"));
    assert!(
        row.message.contains("Et un corps qui explique."),
        "the whole of it: {}",
        row.message
    );
}
