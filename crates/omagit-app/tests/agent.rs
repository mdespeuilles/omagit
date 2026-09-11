//! What is sent to a coding agent when it is asked for a commit message.
//!
//! The agent itself is not run here — that would be a network call, a
//! subscription and thirty seconds per test. What *is* tested is the part that
//! decides what leaves the machine, which is the part a screenshot cannot check
//! and the part that matters most: a patch that quietly omits a file produces a
//! confident message about a change it never saw.

mod support;

use omagit_lib::agent;
use support::{TestRepo, never};

#[test]
fn the_patch_holds_what_is_staged_and_nothing_else() {
    let repo = TestRepo::new();
    repo.commit_file("kept.txt", "one\ntwo\nthree\n", "first");
    repo.write("kept.txt", "one\nTWO\nthree\n");
    repo.write("fresh.txt", "new file\n");
    repo.write("later.txt", "not staged at all\n");
    repo.git(&["add", "kept.txt", "fresh.txt"]);

    let (patch, truncated) = agent::staged_patch(&repo.open(), &never()).expect("a patch");

    assert!(!truncated, "three small files are not a large change");
    assert!(patch.contains("kept.txt"), "the modified file");
    assert!(
        patch.contains("+one\nTWO") || patch.contains("+TWO"),
        "its change"
    );
    assert!(patch.contains("fresh.txt"), "the added file");
    assert!(
        !patch.contains("later.txt"),
        "what is only in the working copy is not part of this commit"
    );
}

#[test]
fn an_unstaged_change_to_a_staged_file_stays_out_of_it() {
    // The case that makes "just run git diff" tempting and wrong to get wrong:
    // one file, changed twice, with only the first change added. A message
    // written from the second change describes a commit that is not being made.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    repo.write("a.txt", "one\nstaged\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "one\nstaged\nnot staged\n");

    let (patch, _) = agent::staged_patch(&repo.open(), &never()).expect("a patch");

    assert!(patch.contains("+staged"));
    assert!(
        !patch.contains("not staged"),
        "the index is what is committed"
    );
}

#[test]
fn a_repository_with_nothing_staged_has_nothing_to_send() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    repo.write("a.txt", "changed\n");

    let (patch, _) = agent::staged_patch(&repo.open(), &never()).expect("no patch is not an error");
    assert!(patch.trim().is_empty());
}

#[test]
fn a_staged_binary_file_is_named_rather_than_sent() {
    // "The icon changed" is part of what the commit is, even when its bytes are
    // not worth sending — and a patch that skipped it in silence would produce
    // a message that does not mention it at all.
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    std::fs::write(repo.path().join("logo.png"), [0u8, 1, 2, 0, 255, 0, 3]).expect("write");
    repo.git(&["add", "logo.png"]);

    let (patch, _) = agent::staged_patch(&repo.open(), &never()).expect("a patch");
    assert!(patch.contains("logo.png"), "named");
    assert!(patch.contains("binary"), "and said to be binary");
}

#[test]
fn a_change_larger_than_the_cap_is_cut_short_and_says_so() {
    let repo = TestRepo::new();
    repo.commit_file("seed.txt", "one\n", "first");
    // Well past the 120 KB cap, spread over files so the cut falls between two.
    for at in 0..40 {
        repo.write(
            &format!("big{at}.txt"),
            &"a line of perfectly ordinary text\n".repeat(200),
        );
    }
    repo.git(&["add", "."]);

    let (patch, truncated) = agent::staged_patch(&repo.open(), &never()).expect("a patch");

    assert!(truncated, "it did not fit");
    assert!(
        patch.len() < 200_000,
        "and it stopped rather than sending all of it"
    );

    // And the prompt says so, because a model that believes it has seen the
    // whole change writes a message claiming to describe all of it.
    let said = agent::prompt_truncated(&patch, "main", "");
    assert!(said.contains("cut short"));
    assert!(!agent::prompt(&patch, "main", "").contains("cut short"));
}
