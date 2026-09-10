//! Handing a conflicted file to the editor the user already configured.
//!
//! SPEC §11 asks for "ouverture dans l'éditeur configuré", and the configured
//! editor is `git`'s: `GIT_EDITOR`, then `core.editor`, then `VISUAL`, then
//! `EDITOR` — the order `git` itself resolves them in.
//!
//! ## The trap, and what is done about it
//!
//! Most people's `git` editor is a *terminal* editor, because the only thing
//! `git` ever opens it for is a commit message in a terminal. Spawning `vim`
//! from a window that has no terminal starts a process nobody can see, waiting
//! on a pipe: the button appears to do nothing, and there is now a `vim` hidden
//! in the process table holding the file open.
//!
//! So a terminal editor is not launched. The file goes to the desktop's own
//! opener instead — `xdg-open`, `open` — which hands it to whatever the machine
//! opens that kind of file with, and the answer says which of the two happened
//! and why. Guessing right and saying nothing would be worse than either.

use std::process::{Command, Stdio};

use omagit_git::cli::Git;
use omagit_git::{Cancel, GitError, RepoPath, Repository, Result};

/// What to run, and what to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    /// What `core.editor` held, when it held anything. Kept beside the reason
    /// because the sentence the window builds names it.
    pub configured: Option<String>,
    /// Set when the configured editor was *not* used, so the answer can say so
    /// rather than leaving someone waiting for a window that is not coming.
    pub instead: Option<Instead>,
}

/// Why the configured editor was not the one used.
///
/// A reason rather than a sentence: the window says it, in the language it is
/// in, and this side has no business holding French.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Instead {
    /// `core.editor` is empty, so the desktop's opener answers instead.
    NothingConfigured,
    /// It is an editor that lives in a terminal, and this window has none.
    LivesInATerminal,
}

/// Editors that live in a terminal, by the name they are invoked as.
///
/// A list, not a heuristic. Being wrong in one direction opens the file in the
/// desktop's default application — recoverable, visible; being wrong in the
/// other starts an invisible process holding a file open, which is the failure
/// this whole module exists to avoid. `emacs` is absent on purpose: it opens a
/// window unless it is told `-nw`, which the flag check below catches.
const TERMINAL: &[&str] = &[
    "vi",
    "vim",
    "nvim",
    "nano",
    "pico",
    "micro",
    "hx",
    "helix",
    "kak",
    "kakoune",
    "ed",
    "joe",
    "ne",
    "mcedit",
    "emacsclient",
];

/// Decide what to launch, given the configured editor and the platform's own
/// opener.
///
/// Pure, and separated from the spawning for that reason: the decision is the
/// part with rules in it, and a test that had to start a process to check them
/// would be a test nobody runs.
pub fn decide(configured: Option<&str>, opener: &str) -> Launch {
    let Some(editor) = configured
        .map(str::trim)
        .filter(|editor| !editor.is_empty())
    else {
        return Launch {
            program: opener.to_owned(),
            configured: None,
            instead: Some(Instead::NothingConfigured),
        };
    };

    if is_terminal(editor) {
        return Launch {
            program: opener.to_owned(),
            configured: Some(editor.to_owned()),
            instead: Some(Instead::LivesInATerminal),
        };
    }
    Launch {
        program: editor.to_owned(),
        configured: Some(editor.to_owned()),
        instead: None,
    }
}

/// Whether this command line runs in a terminal.
///
/// The first word is the program; `-nw` anywhere is Emacs being told to stay in
/// one. A path is reduced to its file name, so `/usr/bin/vim` is `vim`.
fn is_terminal(editor: &str) -> bool {
    let mut words = editor.split_whitespace();
    let program = words
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if TERMINAL.contains(&program) {
        return true;
    }
    editor.split_whitespace().any(|word| word == "-nw")
}

/// The editor `git` would use, or `None` when nothing configures one.
///
/// Not `git var GIT_EDITOR`, which always answers: with nothing set it returns
/// `git`'s built-in default, and "the default" is exactly the case this has to
/// tell apart. The four sources are read in `git`'s own order.
pub fn configured(git: &Git, repo: &Repository, cancel: &Cancel) -> Option<String> {
    if let Some(editor) = from_env("GIT_EDITOR") {
        return Some(editor);
    }
    let from_config = git
        .at(repo.work_dir()?)
        .args(["config", "--get", "core.editor"])
        .run(cancel)
        .ok()
        .map(|output| output.text().trim().to_owned())
        .filter(|editor| !editor.is_empty());
    from_config
        .or_else(|| from_env("VISUAL"))
        .or_else(|| from_env("EDITOR"))
}

fn from_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Open `path` and leave it open.
///
/// Detached on purpose: an editor is a program the user works in for minutes,
/// so nothing here waits for it, gives it a deadline or reads what it prints.
/// That is the opposite of every other subprocess in this application, and the
/// reason it does not go through `cli::Invocation` — those rules exist for
/// `git`, which always answers.
///
/// Through `sh -c` because a configured editor is a *command line*: `code
/// --wait` and `subl -n` are ordinary answers, and splitting them by hand would
/// get quoting wrong in a way that only shows up on somebody else's machine.
pub fn open(
    git: &Git,
    repo: &Repository,
    path: &RepoPath,
    opener: &str,
    cancel: &Cancel,
) -> Result<String> {
    let work_dir = repo
        .work_dir()
        .ok_or_else(|| GitError::NotFound("a work tree".to_owned()))?;
    let full = path
        .to_absolute(work_dir)
        .ok_or_else(|| GitError::NotFound(path.display_lossy().into_owned()))?;

    let launch = decide(configured(git, repo, cancel).as_deref(), opener);
    let mut command = Command::new("sh");
    command
        .arg("-c")
        // `"$@"` and not `$1`: the same form `git` uses, and the one that keeps
        // a path with a space in it one argument.
        .arg(format!("{} \"$@\"", launch.program))
        .arg("omagit")
        .arg(&full)
        .current_dir(work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command.spawn().map_err(|source| GitError::Io {
        path: full.clone(),
        source,
    })?;
    tracing::info!(program = %launch.program, file = %full.display(), "opened in an editor");

    // A key and its two parts, not a sentence: the window says it (§2.60).
    Ok(match launch.instead {
        None => crate::dto::worded("said.opened", &[&path.display_lossy(), &launch.program]),
        Some(Instead::NothingConfigured) => crate::dto::worded(
            "said.openedByOpener",
            &[&path.display_lossy(), &launch.program],
        ),
        Some(Instead::LivesInATerminal) => crate::dto::worded(
            "said.openedNotInTerminal",
            &[
                &path.display_lossy(),
                &launch.program,
                launch.configured.as_deref().unwrap_or_default(),
            ],
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_graphical_editor_is_launched_as_it_is_configured() {
        let launch = decide(Some("code --wait"), "xdg-open");
        assert_eq!(launch.program, "code --wait");
        assert_eq!(launch.instead, None);
    }

    #[test]
    fn a_terminal_editor_is_not_started_where_there_is_no_terminal() {
        // The failure this avoids is invisible: a `vim` waiting on a pipe,
        // holding the file, with a button that looks broken.
        for editor in ["vim", "/usr/bin/nvim", "nano", "emacs -nw"] {
            let launch = decide(Some(editor), "xdg-open");
            assert_eq!(launch.program, "xdg-open", "{editor}");
            assert!(launch.instead.is_some(), "{editor} — and it says why");
        }
    }

    #[test]
    fn emacs_opens_a_window_unless_it_is_told_not_to() {
        assert_eq!(decide(Some("emacs"), "open").program, "emacs");
        assert_eq!(decide(Some("emacs -nw"), "open").program, "open");
    }

    #[test]
    fn nothing_configured_falls_to_the_desktop() {
        for configured in [None, Some(""), Some("   ")] {
            assert_eq!(decide(configured, "open").program, "open");
        }
    }
}
