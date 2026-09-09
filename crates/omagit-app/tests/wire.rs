//! Every command the window asks for is one the backend answers.
//!
//! This is the one link neither suite reaches. The front end's tests run
//! against a fake that answers any name; `edits.rs`'s tests call Rust functions
//! directly. A command that is written but never passed to
//! `tauri::generate_handler!` compiles, type-checks, and fails at runtime with
//! "command not found" the first time somebody presses the button.
//!
//! It reads the two files as text on purpose. There is no shared declaration to
//! check against — that is exactly the problem — so the check has to be that
//! the two lists agree.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate is two levels under the workspace root")
        .to_owned()
}

/// The names `ipc.ts` passes to `invoke`.
fn invoked() -> BTreeSet<String> {
    let source = std::fs::read_to_string(root().join("web/src/ipc.ts")).expect("ipc.ts is there");
    let mut names = BTreeSet::new();
    let mut rest = source.as_str();
    while let Some(at) = rest.find("invoke<") {
        rest = &rest[at + "invoke<".len()..];
        // Past the type argument to the first string literal, which is the name.
        let Some(open) = rest.find('"') else { break };
        let Some(close) = rest[open + 1..].find('"') else {
            break;
        };
        names.insert(rest[open + 1..open + 1 + close].to_owned());
        rest = &rest[open + 1 + close..];
    }
    assert!(
        names.len() > 10,
        "the scan found only {names:?}, which means it stopped matching rather than \
         that the front end shrank"
    );
    names
}

/// The names passed to `tauri::generate_handler!`.
fn registered() -> BTreeSet<String> {
    let source =
        std::fs::read_to_string(root().join("crates/omagit-app/src/lib.rs")).expect("lib.rs");
    let start = source
        .find("generate_handler![")
        .expect("the handler list is in lib.rs");
    let list = &source[start..];
    let end = list.find(']').expect("the handler list is closed");

    list[..end]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("commands::"))
        .map(|name| name.trim_end_matches(',').to_owned())
        .collect()
}

#[test]
fn every_command_the_window_calls_is_registered() {
    let missing: Vec<_> = invoked().difference(&registered()).cloned().collect();
    assert!(
        missing.is_empty(),
        "the front end calls {missing:?}, which `generate_handler!` does not list — \
         they would fail at runtime with \"command not found\""
    );
}

#[test]
fn every_registered_command_is_one_the_window_calls() {
    // The other direction is not an error, but it is worth seeing: a command
    // nobody calls is either a screen that is not ported yet or dead code, and
    // the two look identical until someone says which.
    let unused: Vec<_> = registered().difference(&invoked()).cloned().collect();
    assert!(
        unused.is_empty(),
        "the backend registers {unused:?}, which nothing calls — port the screen \
         that wants them, or delete them"
    );
}
