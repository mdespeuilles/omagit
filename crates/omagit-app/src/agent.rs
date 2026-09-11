//! Asking a coding agent already installed on the machine to write a commit
//! message.
//!
//! SPEC §11 lists "génération de message de commit par IA" out of the MVP, and
//! this amends it. What changed is the shape, not the ambition: the feature was
//! imagined as an API key typed into Preferences, and the better answer —
//! Tower's — is that the machine very likely already has an agent on it, logged
//! in, paid for, and configured.
//!
//! That removes almost all of what made the first shape expensive. **There is
//! no secret to store**: no keychain, no `credentials.toml`, nothing in
//! `settings.toml` that must not be read. **There is no HTTP client**, no TLS
//! stack, no provider to keep up with. And it is the shape this app already
//! has: SPEC §8 says everything omagit writes, it writes by running `git` —
//! a command you could have typed, shown in the journal before it runs. This is
//! that again, with a different program.
//!
//! One property was not designed and turned out to matter most. The agent is
//! started **in the repository**, so it reads the project's own `CLAUDE.md` or
//! `AGENTS.md` before answering. Asked for a message about a three-line diff in
//! this repository, `claude -p` came back in French, in the repository's own
//! voice, and noted that nothing called the new function yet. An API key with a
//! diff and no context could not have done either.
//!
//! ## What each of these was verified to do
//!
//! Checked against the binaries installed on the machine this was written on,
//! which is the only claim that means anything (`docs/notes/` holds the same
//! discipline for every dependency):
//!
//! * **`claude` 2.1.267** — verified end to end. The prompt goes on *stdin*,
//!   never as an argument: `--disallowed-tools` is variadic and swallows every
//!   word that follows it, so a prompt passed as an argument is read as a list
//!   of tool names and the agent is asked nothing at all.
//! * **`codex`** — present on the `PATH` and broken: its wrapper could not find
//!   its own vendored binary. Its arguments here come from its documentation
//!   and have **not** been run. That is exactly what [`probe`] is for — an
//!   agent that cannot answer `--version` is not offered.
//! * **`gemini` 0.17.1** — answers `--version`. Given a prompt on stdin with no
//!   terminal it produced nothing and did not return; the deadline is what
//!   turns that into a message rather than a hang.
//!
//! Which is why the last entry is a command the user names themselves.

use std::path::{Path, PathBuf};
use std::time::Duration;

use omagit_git::journal::Journal;
use omagit_git::{Cancel, cli};

/// How long an agent may take. Generous — `claude -p` took 25 seconds on a
/// three-line diff, and a large staged change is a great deal more thinking —
/// and finite, because the alternative is a button that never comes back.
const DEADLINE: Duration = Duration::from_secs(180);

/// The tools an agent is refused.
///
/// It is asked to write a sentence, not to work. Everything it needs is on its
/// standard input, and an agent with `Bash` and `Edit` pointed at the
/// repository you are about to commit is a different proposition from a
/// sentence generator — one the user did not ask for by clicking "Generate".
///
/// Comma-separated as a single argument. Space-separated it is still accepted,
/// and still variadic, which is the trap above.
const DENIED: &str = "Bash,Edit,Write,Read,Glob,Grep,WebFetch,WebSearch,Task,NotebookEdit";

/// One agent omagit knows how to ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Known {
    /// Stored in `settings.toml`, and never shown.
    pub id: &'static str,
    /// What it is called on screen. A product name, so it is not translated.
    pub label: &'static str,
    /// Looked up on the `PATH`.
    pub program: &'static str,
    /// What makes it answer once, from stdin, with nothing else running.
    pub args: &'static [&'static str],
}

pub const KNOWN: &[Known] = &[
    Known {
        id: "claude",
        label: "Claude Code",
        program: "claude",
        args: &["-p", "--disallowed-tools", DENIED],
    },
    Known {
        id: "codex",
        label: "Codex",
        program: "codex",
        // `exec` is its non-interactive mode and `--sandbox read-only` is the
        // nearest thing it has to the deny list above. Unverified: see the
        // module's note.
        args: &["exec", "--sandbox", "read-only", "-"],
    },
    Known {
        id: "gemini",
        label: "Gemini CLI",
        program: "gemini",
        args: &["-o", "text"],
    },
];

/// What the Preferences screen draws: one row per agent, whether it answered,
/// and what it said when it did.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Candidate {
    pub id: String,
    pub label: String,
    pub program: String,
    /// The version line it printed, or `None` when it did not answer. That is
    /// the whole of "installed": being on the `PATH` is not enough.
    pub version: Option<String>,
}

/// Which agent to ask, and what to tell it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// One of [`KNOWN`], by id.
    Known(&'static Known),
    /// A command the user named. Anything that reads a prompt on stdin and
    /// prints an answer — another agent, a wrapper script, a model running on
    /// the machine.
    Custom(PathBuf),
}

impl Chosen {
    /// Resolve what `settings.toml` holds. `None` for a name this version does
    /// not know, which is what a settings file from a later release looks like.
    pub fn parse(command: &str) -> Option<Self> {
        let command = command.trim();
        if command.is_empty() {
            return None;
        }
        if let Some(known) = KNOWN.iter().find(|known| known.id == command) {
            return Some(Self::Known(known));
        }
        // A path, or a program name to be found on the `PATH`. Anything with a
        // separator in it is the user pointing at a file; anything else is a
        // name — and either way it is theirs, not one of ours.
        Some(Self::Custom(PathBuf::from(command)))
    }

    fn program(&self) -> &Path {
        match self {
            Self::Known(known) => Path::new(known.program),
            Self::Custom(path) => path,
        }
    }

    fn args(&self) -> &[&str] {
        match self {
            Self::Known(known) => known.args,
            // Nothing. A command the user named is a command the user has
            // already spelled the way they want it; guessing a flag for it
            // would be guessing which agent it is.
            Self::Custom(_) => &[],
        }
    }
}

/// Ask every agent this version knows whether it is there.
///
/// Sequential and not parallel: each has a five-second ceiling, there are three
/// of them, and a thread per probe would be machinery for a screen that opens
/// once in a while.
pub fn candidates() -> Vec<Candidate> {
    KNOWN
        .iter()
        .map(|known| Candidate {
            id: known.id.to_owned(),
            label: known.label.to_owned(),
            program: known.program.to_owned(),
            version: cli::probe(Path::new(known.program)).ok(),
        })
        .collect()
}

/// Everything that is staged, as one patch.
///
/// Assembled from `gix`'s own diff and the patch serialiser that staging by
/// hunk already depends on, rather than by running `git diff --staged`. SPEC §8
/// draws the line at reads — "gix still does every read; these do every write"
/// — and a patch is a serialisation of a read, not a second way of taking one.
/// It also costs nothing: `patch::build` is a pure function that is already
/// tested against the case that matters, and the alternative was a subprocess.
///
/// Capped, and it says when it capped. A staged change can be megabytes —
/// a lockfile, a vendored tree — and the whole of it is neither affordable nor
/// useful: what a commit message is about is legible in the first pages.
pub fn staged_patch(
    repo: &omagit_git::Repository,
    cancel: &Cancel,
) -> Result<(String, bool), String> {
    use omagit_git::diff::{DiffOptions, staged_file};
    use omagit_git::patch::{Direction, Selection};
    use omagit_git::status::{Status, StatusOptions};

    let status = Status::load(repo, StatusOptions::default(), cancel).map_err(|e| e.to_string())?;
    let mut patch = String::new();
    let mut truncated = false;

    for entry in &status.entries {
        if entry.staged.is_none() || entry.conflict.is_some() {
            continue;
        }
        let Ok(Some(diff)) = staged_file(repo, entry, DiffOptions::default()) else {
            continue;
        };
        let Ok(bytes) = omagit_git::patch::build(&diff, &Selection::File, Direction::Forward)
        else {
            // A binary file, or one whose diff is not text. Named rather than
            // skipped in silence: "the icon changed" is part of what the commit
            // is, even when its bytes are not worth sending.
            patch.push_str("--- binary: ");
            patch.push_str(&entry.path.display_lossy());
            patch.push('\n');
            continue;
        };
        if patch.len() + bytes.len() > CAP {
            truncated = true;
            break;
        }
        patch.push_str(&String::from_utf8_lossy(&bytes));
    }
    Ok((patch, truncated))
}

/// How much of the staged patch is sent. Large enough for any change somebody
/// would write one message about, small enough that a generated lockfile does
/// not become the whole prompt.
const CAP: usize = 120_000;

/// What the agent is asked, in the order it reads it.
///
/// The instruction first and the diff last, because the diff is the long part
/// and an instruction under a thousand lines of patch is an instruction that
/// gets skimmed.
///
/// Deliberately short on style. The agent is standing in the repository and has
/// read whatever the project tells contributors; a paragraph here about
/// imperative mood and 50 columns would be this app talking over the project's
/// own conventions. `guidelines` is where somebody who wants that says so.
pub fn prompt(diff: &str, branch: &str, guidelines: &str) -> String {
    let mut text = String::from(
        "Write a Git commit message for the staged changes below.\n\n\
         Answer with the message and nothing else: no preamble, no explanation \
         of what you did, no surrounding quotes or code fence. A subject line \
         on its own, then a blank line, then the body. Follow the conventions \
         of this repository — its language included — over any habit of your \
         own.\n",
    );
    if !guidelines.trim().is_empty() {
        text.push_str("\nThe author asks specifically for this:\n");
        text.push_str(guidelines.trim());
        text.push('\n');
    }
    text.push_str("\nThe branch is ");
    text.push_str(branch);
    text.push_str(".\n\n--- git diff --staged ---\n");
    text.push_str(diff);
    text
}

/// The same, saying that the patch was cut short.
///
/// Said rather than hidden: a model that believes it has seen the whole change
/// writes a message claiming to describe all of it.
pub fn prompt_truncated(diff: &str, branch: &str, guidelines: &str) -> String {
    let mut text = prompt(diff, branch, guidelines);
    text.push_str(
        "\n--- the patch was cut short here: the staged change is larger than \
         what is sent. Write about what you can see, and do not claim to \
         describe the whole change. ---\n",
    );
    text
}

/// Run it, and give back what it said.
///
/// The journal is passed on purpose. A command this app starts on the user's
/// machine is a command the user is entitled to see, and "which program did it
/// just run, with what" is a fair question to be able to answer after the fact.
pub fn ask(
    chosen: &Chosen,
    journal: &Journal,
    work_dir: &Path,
    prompt: &str,
    cancel: &Cancel,
) -> Result<String, String> {
    let program = chosen.program();
    let output = cli::program(program, journal, work_dir)
        .args(chosen.args())
        .input(prompt.as_bytes())
        .timeout(DEADLINE)
        .run(cancel)
        .map_err(|error| error.to_string())?;

    let said = tidy(&output.text());
    if said.is_empty() {
        return Err(format!(
            "{} answered nothing",
            program
                .file_name()
                .unwrap_or(program.as_os_str())
                .to_string_lossy()
        ));
    }
    Ok(said)
}

/// What an agent hands back, made into a commit message.
///
/// Two habits to undo, both seen rather than imagined. A model told to answer
/// with the message and nothing else still wraps it in a code fence often
/// enough to matter — and a fence pasted into a commit message is three
/// backticks in the subject line. And trailing blank lines are free here and
/// noise in `git log`.
fn tidy(said: &str) -> String {
    let mut lines: Vec<&str> = said.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.remove(0);
        if lines.last().is_some_and(|line| line.trim() == "```") {
            lines.pop();
        }
    }
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_settings_file_holds() {
        assert_eq!(
            Chosen::parse("claude"),
            Some(Chosen::Known(&KNOWN[0])),
            "a known id"
        );
        assert_eq!(
            Chosen::parse("  /usr/local/bin/mine.sh  "),
            Some(Chosen::Custom(PathBuf::from("/usr/local/bin/mine.sh"))),
            "trimmed, and a path is the user's own"
        );
        assert_eq!(Chosen::parse(""), None, "nothing chosen");
        assert_eq!(Chosen::parse("   "), None);
    }

    #[test]
    fn a_name_this_version_does_not_know_is_a_command_and_not_an_error() {
        // What a settings file written by a later release looks like. Refusing
        // it would turn "omagit knows one more agent than you do" into a
        // feature that is simply off with no way to find out why; treated as a
        // command, it either runs or says it could not.
        assert_eq!(
            Chosen::parse("aider"),
            Some(Chosen::Custom(PathBuf::from("aider")))
        );
    }

    #[test]
    fn the_prompt_puts_the_instruction_before_the_diff() {
        let text = prompt("diff --git a/a b/a\n", "main", "");
        let instruction = text.find("Write a Git commit message").expect("said");
        let diff = text.find("diff --git").expect("carried");
        assert!(
            instruction < diff,
            "an instruction under a patch is skimmed"
        );
        assert!(text.contains("The branch is main."));
        assert!(!text.contains("The author asks"), "nothing was asked");
    }

    #[test]
    fn guidelines_are_carried_when_there_are_any() {
        let text = prompt("", "main", "  Toujours en français.  ");
        assert!(text.contains("The author asks specifically for this:"));
        assert!(
            text.contains("Toujours en français."),
            "trimmed, not dropped"
        );
    }

    #[test]
    fn a_fenced_answer_is_unwrapped() {
        // Told to answer with the message and nothing else, a model wraps it in
        // a fence often enough to matter — and a fence in a commit message is
        // three backticks in the subject line.
        assert_eq!(
            tidy("```\nUn sujet\n\nUn corps.\n```"),
            "Un sujet\n\nUn corps."
        );
        assert_eq!(tidy("```text\nUn sujet\n```"), "Un sujet");
        // An opening fence with nothing closing it: the first line still goes,
        // because it is still not part of the message.
        assert_eq!(tidy("```\nUn sujet"), "Un sujet");
    }

    #[test]
    fn a_message_that_is_only_a_message_is_left_alone() {
        assert_eq!(tidy("Un sujet\n\nUn corps.\n\n\n"), "Un sujet\n\nUn corps.");
        // A fence *inside* the body is the author's, and stays.
        assert_eq!(
            tidy("Un sujet\n\nComme ceci :\n\n```sh\ngit log\n```"),
            "Un sujet\n\nComme ceci :\n\n```sh\ngit log\n```"
        );
    }
}
