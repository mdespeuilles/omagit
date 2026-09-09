//! Stopped on a conflict: which side is which, taking one, and getting out the
//! front.

mod support;

use omagit_git::cli::Git;
use omagit_git::conflict::{self, Side};
use omagit_git::ops::{conflict as resolve, integrate};
use omagit_git::repo::Operation;
use omagit_git::status::{Conflict, Status, StatusOptions};
use support::{TestRepo, never};

fn git() -> Git {
    Git::detect().expect("git is installed; these tests need it")
}

/// `main` and `feature`, both changing the same line of the same file.
fn conflicting() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit_file("shared.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("shared.txt", "theirs\n", "their version");
    repo.checkout("main");
    repo.commit_file("shared.txt", "ours\n", "our version");
    repo
}

/// Stop a merge of `feature` on that conflict.
fn stopped_merge() -> TestRepo {
    let repo = conflicting();
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("the same line");
    repo
}

fn status(repo: &TestRepo) -> Status {
    Status::load(&repo.open(), StatusOptions::default(), &never()).expect("a status")
}

fn conflicts(repo: &TestRepo) -> Vec<String> {
    status(repo)
        .conflicts()
        .map(|entry| entry.path.display_lossy().into_owned())
        .collect()
}

fn contents(repo: &TestRepo, path: &str) -> String {
    String::from_utf8_lossy(&repo.read(path)).into_owned()
}

#[test]
fn a_merge_keeps_the_words_ours_and_theirs_meaning_what_they_say() {
    let repo = stopped_merge();

    let sides = conflict::sides(&repo.open())
        .expect("read")
        .expect("a merge is running");

    assert_eq!(sides.ours, "main");
    assert_eq!(sides.theirs, "feature");
    assert!(!sides.replayed);
}

#[test]
fn a_rebase_calls_the_branch_you_are_standing_on_theirs() {
    // The reason this module exists. During a rebase `--ours` is the branch
    // being replayed *onto* and `--theirs` is your own work — two buttons
    // labelled with the pronouns alone would be asking someone to choose
    // between two meanings they cannot see.
    let repo = conflicting();
    repo.checkout("feature");
    integrate::rebase(&git(), &repo.open(), "main", &never()).expect_err("the same line");

    let sides = conflict::sides(&repo.open())
        .expect("read")
        .expect("a rebase is running");

    assert_eq!(sides.ours, "main", "the side being replayed onto");
    assert_eq!(sides.theirs, "feature", "your own commits");
    assert!(sides.replayed, "and the interface has to say so");
}

#[test]
fn a_repository_in_the_middle_of_nothing_has_no_sides() {
    let repo = conflicting();
    assert_eq!(conflict::sides(&repo.open()).expect("read"), None);
}

#[test]
fn keeping_ours_leaves_our_version_resolved() {
    let repo = stopped_merge();

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "ours\n");
    assert!(
        conflicts(&repo).is_empty(),
        "and it is out of the index's unmerged state, not merely restored"
    );
}

#[test]
fn keeping_theirs_leaves_their_version_resolved() {
    let repo = stopped_merge();

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "theirs\n");
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn keeping_our_side_of_a_file_we_deleted_keeps_the_deletion() {
    // `DU`: our side has no version at all, and `git checkout --ours` says so
    // — "does not have our version", which is true and useless. Keeping ours
    // here means keeping the deletion.
    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("gone.txt", "theirs\n", "they changed it");
    repo.checkout("main");
    repo.remove("gone.txt");
    repo.add_all();
    repo.commit_staged("we deleted it");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("delete against modify");

    let kind = status(&repo)
        .conflicts()
        .find_map(|entry| entry.conflict)
        .expect("a conflict");
    assert_eq!(kind, Conflict::DeletedByUs);

    resolve::take(
        &git(),
        &repo.open(),
        &"gone.txt".into(),
        Side::Ours,
        kind,
        &never(),
    )
    .expect("resolved");

    assert!(!repo.path().join("gone.txt").exists());
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn keeping_their_side_of_a_file_they_deleted_removes_it() {
    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "original\n", "seed");
    repo.branch("feature");
    repo.remove("gone.txt");
    repo.add_all();
    repo.commit_staged("they deleted it");
    repo.checkout("main");
    repo.commit_file("gone.txt", "ours\n", "we changed it");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("modify against delete");

    let kind = status(&repo)
        .conflicts()
        .find_map(|entry| entry.conflict)
        .expect("a conflict");
    assert_eq!(kind, Conflict::DeletedByThem);

    resolve::take(
        &git(),
        &repo.open(),
        &"gone.txt".into(),
        Side::Theirs,
        kind,
        &never(),
    )
    .expect("resolved");

    assert!(!repo.path().join("gone.txt").exists());
    assert!(conflicts(&repo).is_empty());
}

#[test]
fn continuing_finishes_the_merge_the_conflict_stopped() {
    let repo = stopped_merge();
    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    integrate::resume(&git(), &repo.open(), Operation::Merge, &never()).expect("continued");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(
        repo.git(&["rev-list", "--count", "--merges", "HEAD"]),
        "1",
        "the merge commit the conflict was standing in the way of"
    );
}

#[test]
fn continuing_with_a_path_still_unmerged_is_refused_in_gits_own_words() {
    // The check is git's, not ours: the index is the truth, and anything read
    // here first would be a copy of it taken a moment earlier.
    let repo = stopped_merge();

    let refused =
        integrate::resume(&git(), &repo.open(), Operation::Merge, &never()).expect_err("unmerged");

    let said = refused.to_string();
    assert!(
        said.contains("shared.txt") || said.to_lowercase().contains("conflict"),
        "it names the file in the way: {said}"
    );
    assert_eq!(
        repo.open().operation(),
        Some(Operation::Merge),
        "and the operation is still where it was"
    );
}

#[test]
fn continuing_a_rebase_replays_what_was_left() {
    let repo = conflicting();
    repo.checkout("feature");
    integrate::rebase(&git(), &repo.open(), "main", &never()).expect_err("the same line");
    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    integrate::resume(
        &git(),
        &repo.open(),
        Operation::Rebase { interactive: false },
        &never(),
    )
    .expect("continued");

    assert_eq!(repo.open().operation(), None);
    assert_eq!(repo.git(&["rev-parse", "--abbrev-ref", "HEAD"]), "feature");
    assert_eq!(
        repo.git(&["log", "--format=%s", "-2"]),
        "their version\nour version",
        "our commit replayed on top of main's"
    );
}

#[test]
fn a_bisect_is_refused_by_name_rather_than_sent_a_flag_it_has_not_got() {
    let repo = conflicting();

    let error = integrate::resume(&git(), &repo.open(), Operation::Bisect, &never())
        .expect_err("bisect has no --continue");

    assert!(error.to_string().contains("bisect good"), "{error}");
}

#[test]
fn taking_a_side_is_recorded_as_destructive_and_continuing_a_replay_too() {
    let repo = stopped_merge();
    let journal = omagit_git::journal::Journal::new();
    let git = git().with_journal(journal.clone());

    resolve::take(
        &git,
        &repo.open(),
        &"shared.txt".into(),
        Side::Ours,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");
    integrate::resume(&git, &repo.open(), Operation::Merge, &never()).expect("continued");

    let entries = journal.entries();
    assert!(
        entries
            .iter()
            .any(|entry| entry.destructive && entry.command.contains("checkout --ours")),
        "it overwrites the file in the work tree, hand resolution included: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| !entry.destructive && entry.command.contains("merge --continue")),
        "a merge --continue commits what is already in the index: {entries:?}"
    );
}

#[test]
fn a_stash_that_conflicts_on_the_way_back_has_no_operation_and_still_has_sides_to_take() {
    // The case the two M8 slices meet in. A `git stash pop` that conflicts
    // leaves unmerged paths behind *without* starting an operation: there is no
    // `MERGE_HEAD`, so nothing to name the sides after — and the interface falls
    // back to the words. What still works is the resolution, because the index
    // has the stages whether or not anything is half-finished.
    let repo = TestRepo::new();
    repo.commit_file("shared.txt", "original\n", "seed");
    repo.write("shared.txt", "stashed\n");
    omagit_git::ops::stash::push(&git(), &repo.open(), "shelved", false, &never())
        .expect("stashed");
    // Committed, not merely written: `git stash pop` refuses outright when the
    // change in the way is uncommitted, and refusing is not conflicting.
    repo.commit_file("shared.txt", "committed since\n", "the line moved under it");

    omagit_git::ops::stash::pop(&git(), &repo.open(), 0, &never()).expect_err("the same line");

    assert_eq!(
        conflict::sides(&repo.open()).expect("read"),
        None,
        "no operation is running: a stash pop is not one"
    );
    assert_eq!(conflicts(&repo), ["shared.txt"]);

    resolve::take(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        Side::Theirs,
        Conflict::BothModified,
        &never(),
    )
    .expect("resolved");

    assert_eq!(contents(&repo, "shared.txt"), "stashed\n");
    assert!(conflicts(&repo).is_empty());
}

// ── The markers, and choosing between them ──────────────────────────────────

use omagit_git::conflict::{Choice, Segment};

/// The file `git` leaves behind for the merge in `stopped_merge`.
fn markers(repo: &TestRepo) -> String {
    contents(repo, "shared.txt")
}

#[test]
fn the_two_sides_are_read_out_of_the_file_git_wrote() {
    let repo = stopped_merge();

    let file = conflict::read(&repo.open(), &"shared.txt".into()).expect("markers");

    assert_eq!(file.regions, 1);
    let Some(Segment::Conflict(region)) = file.segments.first() else {
        panic!("the whole file is one conflict: {:?}", file.segments)
    };
    assert_eq!(region.ours, ["ours"]);
    assert_eq!(region.theirs, ["theirs"]);
    assert_eq!(region.index, 0);
    assert_eq!(region.start, 1);
    // The labels are git's own: `HEAD` on our side, the branch on theirs.
    assert_eq!(region.ours_label, "HEAD");
    assert_eq!(region.theirs_label, "feature");
    assert_eq!(region.base, None, "the default style keeps no base");
}

#[test]
fn agreed_text_keeps_its_place_in_the_file() {
    let repo = TestRepo::new();
    repo.commit_file("poem.txt", "one\ntwo\nthree\nfour\n", "seed");
    repo.branch("feature");
    repo.write("poem.txt", "one\ntwo\nTHEIRS\nfour\n");
    repo.add_all();
    repo.commit_staged("their line");
    repo.checkout("main");
    repo.write("poem.txt", "one\ntwo\nOURS\nfour\n");
    repo.add_all();
    repo.commit_staged("our line");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("the same line");

    let file = conflict::read(&repo.open(), &"poem.txt".into()).expect("markers");

    let shapes: Vec<&str> = file
        .segments
        .iter()
        .map(|segment| match segment {
            Segment::Agreed { .. } => "agreed",
            Segment::Conflict(_) => "conflict",
        })
        .collect();
    assert_eq!(shapes, ["agreed", "conflict", "agreed"]);
    let Some(Segment::Agreed { start, lines }) = file.segments.first() else {
        panic!("the two lines above it")
    };
    assert_eq!(
        (*start, lines.as_slice()),
        (1, ["one".to_owned(), "two".to_owned()].as_slice())
    );
}

#[test]
fn a_diff3_file_keeps_its_base_apart_from_the_two_sides() {
    // With `merge.conflictStyle = diff3` git writes a third section. Parsed
    // even though no button offers it: a resolution that ignored it would leave
    // the ancestor's lines in the file.
    let repo = stopped_merge();
    repo.git(&["config", "merge.conflictStyle", "diff3"]);
    repo.git(&["checkout", "--merge", "--", "shared.txt"]);

    let file = conflict::read(&repo.open(), &"shared.txt".into()).expect("markers");

    let Some(Segment::Conflict(region)) = file.segments.first() else {
        panic!("one conflict")
    };
    assert_eq!(
        region.base.as_deref(),
        Some(["original".to_owned()].as_slice())
    );
    assert_eq!(region.ours, ["ours"]);
    assert_eq!(region.theirs, ["theirs"]);
}

#[test]
fn choosing_a_side_writes_the_file_without_its_markers() {
    let repo = stopped_merge();
    assert!(markers(&repo).contains("<<<<<<<"));

    omagit_git::ops::conflict::resolve(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        &[Choice::Theirs],
        &never(),
    )
    .expect("resolved");

    assert_eq!(markers(&repo), "theirs\n");
    assert!(
        conflicts(&repo).is_empty(),
        "and staged, because a rewritten file is still unmerged until it is added"
    );
}

#[test]
fn both_keeps_the_two_sides_ours_first() {
    let repo = stopped_merge();

    omagit_git::ops::conflict::resolve(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        &[Choice::Both],
        &never(),
    )
    .expect("resolved");

    assert_eq!(markers(&repo), "ours\ntheirs\n");
}

#[test]
fn answers_for_a_file_that_has_moved_underneath_are_refused() {
    // The dialog read three conflicts; something resolved one by hand while it
    // was open. Writing the old answers would undo that silently.
    let repo = stopped_merge();

    let error = omagit_git::ops::conflict::resolve(
        &git(),
        &repo.open(),
        &"shared.txt".into(),
        &[Choice::Ours, Choice::Ours],
        &never(),
    )
    .expect_err("two answers, one conflict");

    assert!(
        error.to_string().contains("1 conflicts, not the 2"),
        "{error}"
    );
    assert!(
        markers(&repo).contains("<<<<<<<"),
        "and the file is untouched"
    );
}

#[test]
fn markers_that_do_not_pair_up_are_refused_rather_than_guessed_at() {
    let repo = stopped_merge();
    // Half a conflict — an editor left open, a bad hand resolution.
    repo.write("shared.txt", "<<<<<<< HEAD\nours\n");

    let error = conflict::read(&repo.open(), &"shared.txt".into()).expect_err("half a marker");

    assert!(error.to_string().contains("do not pair up"), "{error}");
}

#[test]
fn a_file_resolved_by_hand_has_no_conflicts_left_and_that_is_not_an_error() {
    let repo = stopped_merge();
    repo.write("shared.txt", "settled by hand\n");

    let file = conflict::read(&repo.open(), &"shared.txt".into()).expect("no markers");

    assert_eq!(file.regions, 0);
    assert_eq!(
        file.segments,
        vec![Segment::Agreed {
            start: 1,
            lines: vec!["settled by hand".to_owned()]
        }]
    );
}

#[test]
fn a_resolution_gives_back_the_line_endings_it_was_given() {
    // Windows line endings, and a file with no final newline. Both survive
    // because everything outside the markers is copied byte for byte rather
    // than reassembled from the lines the parser read.
    let repo = TestRepo::new();
    repo.git(&["config", "core.autocrlf", "false"]);
    repo.write_bytes("crlf.txt", b"one\r\ntwo\r\nthree");
    repo.add_all();
    repo.commit_staged("seed");
    repo.branch("feature");
    repo.write_bytes("crlf.txt", b"one\r\nTHEIRS\r\nthree");
    repo.add_all();
    repo.commit_staged("their line");
    repo.checkout("main");
    repo.write_bytes("crlf.txt", b"one\r\nOURS\r\nthree");
    repo.add_all();
    repo.commit_staged("our line");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("the same line");

    omagit_git::ops::conflict::resolve(
        &git(),
        &repo.open(),
        &"crlf.txt".into(),
        &[Choice::Ours],
        &never(),
    )
    .expect("resolved");

    assert_eq!(repo.read("crlf.txt"), b"one\r\nOURS\r\nthree");
}

#[test]
fn a_conflicted_file_has_a_diff_of_its_own() {
    // Neither side of the index can answer for an unmerged path: there is no
    // stage 0. Both `staged_file` and `unstaged_file` returned nothing, and the
    // Working Copy drew "ce fichier n'est plus dans le statut" over a file that
    // was right there in the list.
    use omagit_git::diff::{DiffOptions, conflicted_file, staged_file, unstaged_file};

    let repo = stopped_merge();
    let status = status(&repo);
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path.display_lossy() == "shared.txt")
        .expect("the conflicted file");

    assert!(
        staged_file(&repo.open(), entry, DiffOptions::default())
            .expect("read")
            .is_none()
            && unstaged_file(&repo.open(), entry, DiffOptions::default())
                .expect("read")
                .is_none(),
        "the two ordinary sides have nothing to say about an unmerged path"
    );

    let diff = conflicted_file(&repo.open(), entry, DiffOptions::default())
        .expect("read")
        .expect("a conflicted file has a diff");

    // Ours, against the file git has written the markers into: the reader sees
    // what the merge did to their own version.
    let text = match &diff.content {
        omagit_git::diff::DiffContent::Text { hunks, .. } => hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .map(|line| line.text_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("the fixture is text, not {other:?}"),
    };
    assert!(
        text.contains("<<<<<<<"),
        "the markers are the point: {text}"
    );
    assert!(
        text.contains("theirs"),
        "and the other side's lines: {text}"
    );
}

#[test]
fn a_file_we_deleted_and_they_changed_reads_as_theirs_arriving() {
    // No stage 2 at all, so the comparison is against nothing — which is what
    // "we do not have this file" looks like as a diff.
    use omagit_git::diff::{DiffOptions, conflicted_file};

    let repo = TestRepo::new();
    repo.commit_file("gone.txt", "original\n", "seed");
    repo.branch("feature");
    repo.commit_file("gone.txt", "theirs\n", "they changed it");
    repo.checkout("main");
    repo.remove("gone.txt");
    repo.add_all();
    repo.commit_staged("we deleted it");
    integrate::merge(
        &git(),
        &repo.open(),
        "feature",
        &Default::default(),
        &never(),
    )
    .expect_err("delete against modify");

    let status = status(&repo);
    let entry = status.conflicts().next().expect("a conflict");
    let diff = conflicted_file(&repo.open(), entry, DiffOptions::default())
        .expect("read")
        .expect("a diff");

    assert_eq!(diff.change, omagit_git::diff::FileChange::Added);
}
