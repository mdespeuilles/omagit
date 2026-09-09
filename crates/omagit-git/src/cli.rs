//! Running the `git` binary, under the five rules of SPEC §8.
//!
//! `gix` covers the read path (see `docs/notes/gitoxide-capabilities.md`); this
//! is the other half of the hybrid, and at M2 it carries exactly one job —
//! finding out whether a usable `git` is installed, which the app asks at
//! start-up so the answer is a clear message rather than a failure at the first
//! commit. Writes and the network arrive at M5 and M7 and go through this same
//! runner, which is why it already implements the whole rule set:
//!
//! 1. **Machine formats.** `--porcelain` / `-z` / `--no-color` wherever one
//!    exists. Callers choose the flags; [`Invocation`] only guarantees the
//!    environment does not colour the output behind their back.
//! 2. **Controlled environment.** `GIT_TERMINAL_PROMPT=0` (never block on a
//!    hidden password prompt), `GIT_OPTIONAL_LOCKS=0` (a read must not write
//!    `index.lock` under a repository the user is also using elsewhere), and the
//!    `GIT_DIR`-family variables removed so an inherited one cannot point the
//!    command at a different repository than the one asked for.
//! 3. **Cancellable.** The child leads its own process group and a cancelled or
//!    timed-out command gets `SIGTERM` on the *group*: `git` delegates to
//!    `git-remote-https`, `ssh` and hooks, and signalling only the leader leaves
//!    those running.
//! 4. **Timed out.** Every invocation has a deadline.
//! 5. **`stderr` verbatim.** Git's own diagnostics are better than anything
//!    that could be written here, so they travel to the user unedited.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::journal::Journal;
use crate::{Cancel, GitError, MINIMUM_GIT_VERSION, Result, assert_off_render_thread};

/// How long an invocation may run before it is killed.
///
/// Generous for a read — a cold `git` on a network mount is slow — and short
/// enough that a wedged child cannot hang a background task forever. Network
/// commands will override it when they arrive (M7).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The start-up probe's own deadline. Much shorter than the default: `git
/// --version` does no work, so anything slow here is a `git` that will never
/// answer — a dead network mount, a wedged wrapper script — and the user is
/// waiting on a window.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Between `SIGTERM` and `SIGKILL`. Long enough for `git` to unlink its lock
/// files, short enough not to be felt.
const GRACE: Duration = Duration::from_millis(250);

/// How often the wait loop looks at the clock and the cancellation flag. Short
/// enough that a cancelled command dies while the click is still felt, long
/// enough not to spin a core.
const POLL: Duration = Duration::from_millis(10);

/// The installed `git`, once found and vetted.
#[derive(Clone, Debug)]
pub struct Git {
    program: PathBuf,
    version: Version,
    /// Where every invocation records itself. Carried here rather than passed
    /// at each call site so that a command cannot be run without being written
    /// down (SPEC §11, §15 risk 5).
    journal: Journal,
}

impl Git {
    /// Find `git` in `PATH` and check it is new enough (SPEC §8).
    pub fn detect() -> Result<Self> {
        Self::detect_at(Path::new("git"))
    }

    /// Same, for a `git` at a known location — used by tests, and by the
    /// preference that will let a user point at their own build (M9).
    ///
    /// Runs through the same [`execute`] as everything else, so the very first
    /// `git` omagit ever runs already has a deadline and a scrubbed
    /// environment. It is not a formality: an inherited `GIT_TERMINAL_PROMPT`
    /// or a `git` on a dead network mount can hang a start-up, and a start-up
    /// that hangs looks like an app that will not launch.
    pub fn detect_at(program: &Path) -> Result<Self> {
        let output = execute(
            Spawn {
                program,
                args: &[OsString::from("--version")],
                work_dir: None,
                timeout: PROBE_TIMEOUT,
                command_line: &format!("{} --version", program.display()),
                input: None,
                watch: None,
            },
            &Cancel::new(),
        )?;

        let text = output.text();
        let version = Version::parse(&text).ok_or_else(|| GitError::CommandFailed {
            command: format!("{} --version", program.display()),
            stderr: format!("unrecognised version line: {}", text.trim()),
        })?;
        let minimum = Version::parse_number(MINIMUM_GIT_VERSION).unwrap_or(Version {
            major: 2,
            minor: 35,
            patch: 0,
        });
        if version < minimum {
            return Err(GitError::GitTooOld {
                found: version.to_string(),
                minimum: MINIMUM_GIT_VERSION.to_owned(),
            });
        }
        tracing::debug!(git = %program.display(), version = %version, "git detected");
        Ok(Self {
            program: program.to_owned(),
            version,
            journal: Journal::new(),
        })
    }

    /// Record every invocation into `journal` instead of this `Git`'s own.
    /// The app calls this once, so the interface and the commands share one.
    pub fn with_journal(mut self, journal: Journal) -> Self {
        self.journal = journal;
        self
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Start building an invocation inside `work_dir`.
    pub fn at(&self, work_dir: impl Into<PathBuf>) -> Invocation<'_> {
        Invocation {
            git: self,
            work_dir: work_dir.into(),
            args: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            input: None,
            destructive: false,
            watch: None,
        }
    }
}

/// A `git` command line, before it runs.
///
/// Kept as a value rather than executed on the spot so it can be logged exactly
/// as it will run — SPEC §15 requires that of every destructive command, and the
/// operations journal of SPEC §11 shows the same string to the user.
/// No `Debug`: it carries a closure, and what a reader wants from an invocation
/// is [`Invocation::command_line`], which is the thing the journal records.
#[derive(Clone)]
pub struct Invocation<'a> {
    git: &'a Git,
    work_dir: PathBuf,
    args: Vec<OsString>,
    timeout: Duration,
    input: Option<Vec<u8>>,
    destructive: bool,
    watch: Option<Progress>,
}

impl<'a> Invocation<'a> {
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    /// Feed `input` to the command on standard input.
    ///
    /// Bytes, and it matters: this carries patches and commit messages, neither
    /// of which anybody promised was UTF-8, and both of which would be mangled
    /// by going through an argument.
    pub fn input(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// Mark this command as one that can lose work, so the journal says so
    /// before it runs (SPEC §15 risk 5).
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Watch `stderr` as it is written, for a progress overlay.
    ///
    /// Only useful with `--progress`: `git` writes progress when it thinks it
    /// has a terminal, and it does not have one here.
    pub fn watching(mut self, watch: Progress) -> Self {
        self.watch = Some(watch);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The command line as it will run, for logs and for the operations
    /// journal. Not shell-quoted: it is meant to be read, not pasted.
    pub fn command_line(&self) -> String {
        let mut line = String::from("git");
        for arg in &self.args {
            line.push(' ');
            line.push_str(&arg.to_string_lossy());
        }
        line
    }

    /// Run to completion, or until `cancel` fires or the deadline passes.
    pub fn run(&self, cancel: &Cancel) -> Result<Output> {
        let command_line = self.command_line();
        // Opened before the process exists, so a command that never returns
        // still leaves a record of having been started.
        let record = self
            .git
            .journal
            .begin(&command_line, &self.work_dir, self.destructive);
        let started = Instant::now();

        let result = execute(
            Spawn {
                program: &self.git.program,
                args: &self.args,
                work_dir: Some(&self.work_dir),
                timeout: self.timeout,
                command_line: &command_line,
                input: self.input.as_deref(),
                watch: self.watch.clone(),
            },
            cancel,
        );

        let elapsed = started.elapsed();
        match &result {
            Ok(output) => record.succeeded(elapsed, &output.stderr),
            Err(error) => record.failed(elapsed, &error.to_string()),
        }
        result
    }
}

/// Spawn `git`, drain it, and enforce the deadline and the cancellation.
///
/// The one place a `git` process is created, so rules 2 to 5 of SPEC §8 cannot
/// be forgotten at a call site.
struct Spawn<'a> {
    program: &'a Path,
    args: &'a [OsString],
    work_dir: Option<&'a Path>,
    timeout: Duration,
    command_line: &'a str,
    input: Option<&'a [u8]>,
    watch: Option<Progress>,
}

fn execute(spawn: Spawn<'_>, cancel: &Cancel) -> Result<Output> {
    let Spawn {
        program,
        args,
        work_dir,
        timeout,
        command_line,
        input,
        watch,
    } = spawn;
    assert_off_render_thread();
    tracing::debug!(
        command = %command_line,
        dir = %work_dir.unwrap_or(Path::new(".")).display(),
        "running git"
    );

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(work_dir) = work_dir {
        command.current_dir(work_dir);
    }
    scrub_environment(&mut command);
    #[cfg(unix)]
    {
        // Its own process group, so the kill below reaches the helpers `git`
        // spawns and not just `git` itself.
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => GitError::GitNotFound,
        _ => GitError::Io {
            path: program.to_owned(),
            source,
        },
    })?;

    // Written from its own thread, for the same reason both output pipes are
    // drained from theirs: a patch larger than a pipe buffer deadlocks a
    // process that writes it while nobody reads `git`'s output.
    let feeding = input.map(|input| {
        let mut stdin = child.stdin.take();
        let input = input.to_vec();
        std::thread::spawn(move || {
            if let Some(stdin) = stdin.as_mut() {
                use std::io::Write as _;
                let _ = stdin.write_all(&input);
            }
            // Dropped here, closing the pipe: `git apply -` waits for EOF.
            drop(stdin);
        })
    });

    // Drain both pipes from their own threads. A command that writes more than
    // a pipe buffer to `stderr` while nobody reads it blocks forever, and `git`
    // on a large repository writes plenty.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(|pipe| match watch {
        Some(watch) => drain_watching(pipe, watch),
        None => drain(pipe),
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(source) => {
                return Err(GitError::Io {
                    path: program.to_owned(),
                    source,
                });
            }
        }
        let outcome = if cancel.is_cancelled() {
            Some(GitError::Cancelled)
        } else if started.elapsed() >= timeout {
            Some(GitError::Timeout {
                command: command_line.to_owned(),
                seconds: timeout.as_secs_f32(),
            })
        } else {
            None
        };
        if let Some(error) = outcome {
            terminate(&mut child);
            return Err(error);
        }
        std::thread::sleep(POLL);
    };

    if let Some(feeding) = feeding {
        let _ = feeding.join();
    }
    let stdout = stdout.map(join).unwrap_or_default();
    let stderr = stderr.map(join).unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr).trim_end().to_owned();

    if status.success() {
        Ok(Output { stdout, stderr })
    } else {
        // SPEC §3 rule 3 wants Git's own words, and Git does not always put
        // them on `stderr`: a merge that conflicts explains itself on `stdout`
        // — "CONFLICT (content): Merge conflict in shared.txt" — and reporting
        // only `stderr` would hand the user an error with nothing in it.
        let said = if stderr.is_empty() {
            String::from_utf8_lossy(&stdout).trim().to_owned()
        } else {
            stderr
        };
        Err(GitError::CommandFailed {
            command: command_line.to_owned(),
            stderr: said,
        })
    }
}

/// Told each line `git` writes to `stderr`, as it is written.
///
/// `git` reports progress there and nowhere else, and only when it believes it
/// is talking to a terminal — hence `--progress` at every call site that wants
/// this.
pub type Progress = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// What a successful invocation produced.
///
/// `stderr` is kept even on success: `git` reports progress and warnings there,
/// and the operations journal shows them.
#[derive(Clone, Debug)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    /// The output as text, lossily. Only for commands whose output is known to
    /// be text; anything that can carry a path stays bytes.
    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }
}

/// The `git` version, compared as a triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a whole `git version …` line.
    ///
    /// Apple ships `git version 2.39.5 (Apple Git-154)` and Windows Git adds
    /// `.windows.1`; both parse, because only the leading numeric components of
    /// the third field are read and the rest is ignored.
    pub fn parse(line: &str) -> Option<Self> {
        let number = line.split_whitespace().nth(2)?;
        Self::parse_number(number)
    }

    /// Parse a bare `2.35`, `2.35.1` or `2.39.5.windows.1`.
    pub fn parse_number(number: &str) -> Option<Self> {
        let mut parts = number
            .split('.')
            .map(|part| part.parse::<u32>().ok())
            .take_while(Option::is_some)
            .flatten();
        Some(Self {
            major: parts.next()?,
            minor: parts.next().unwrap_or(0),
            patch: parts.next().unwrap_or(0),
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Rule 2 of SPEC §8, in one place.
fn scrub_environment(command: &mut Command) {
    command
        // Never stop for a prompt nobody can answer: a credential helper that
        // needs a terminal must fail, so the UI can ask instead (M7).
        .env("GIT_TERMINAL_PROMPT", "0")
        // A read must not take the index lock. Without this, running omagit
        // while a terminal `git` holds the lock turns a status into an error.
        .env("GIT_OPTIONAL_LOCKS", "0")
        // Colour is for humans; every format read here is machine-readable.
        .env("GIT_PAGER", "cat")
        .env("NO_COLOR", "1")
        // The other prompt nobody can answer. `git merge` and `git pull` open
        // an editor for the merge message, and `git rebase` opens one for its
        // todo list. `git` skips both when stdin is not a terminal, and stdin
        // here is `/dev/null` — but that is a behaviour to rely on rather than
        // a guarantee, and the failure mode if it ever changed is a `vi` on a
        // pipe nobody can see, waiting forever.
        //
        // `true` is the shell builtin: it exits 0 immediately and leaves the
        // file untouched, which is exactly "keep the message git prepared".
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true");
    // An inherited `GIT_DIR` (from a hook, or from a terminal the app was
    // launched from) silently redirects the command at another repository.
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
    ] {
        command.env_remove(variable);
    }
}

/// `SIGTERM` the child's whole process group, then `SIGKILL` what survives.
fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // Negative pid means "the process group", which is why the child was
        // given one of its own.
        let group = -(child.id() as i32);
        // SAFETY: `kill` on a pid we own; the child is not reaped until the
        // `wait` below, so the pid cannot have been recycled.
        unsafe { libc::kill(group, libc::SIGTERM) };
        let deadline = Instant::now() + GRACE;
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // SAFETY: as above.
        unsafe { libc::kill(group, libc::SIGKILL) };
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

fn drain<R: std::io::Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = std::io::Read::read_to_end(&mut pipe, &mut buffer);
        buffer
    })
}

/// The same, reporting each line as it arrives.
///
/// `read_to_end` is the wrong shape for a network operation: it answers once,
/// at the end, and the end is exactly what the user is waiting to hear about.
/// This reads in chunks and cuts on **both** `\n` and `\r`, because `git`
/// overwrites its own progress line with a carriage return — a reader that
/// split on newlines alone would receive one enormous line at the end and
/// report nothing until then.
fn drain_watching<R: std::io::Read + Send + 'static>(
    mut pipe: R,
    watch: Progress,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut pending = Vec::new();
        loop {
            match std::io::Read::read(&mut pipe, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    buffer.extend_from_slice(&chunk[..read]);
                    pending.extend_from_slice(&chunk[..read]);
                    let mut start = 0;
                    for (at, byte) in pending.iter().enumerate() {
                        if *byte == b'\n' || *byte == b'\r' {
                            let line = String::from_utf8_lossy(&pending[start..at]);
                            let line = line.trim();
                            if !line.is_empty() {
                                watch(line);
                            }
                            start = at + 1;
                        }
                    }
                    pending.drain(..start);
                }
            }
        }
        // Whatever `git` left without a terminator — the last line of a
        // failure, most often, which is the one worth showing.
        let line = String::from_utf8_lossy(&pending);
        let line = line.trim();
        if !line.is_empty() {
            watch(line);
        }
        buffer
    })
}

fn join(handle: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_version_lines_git_actually_prints() {
        assert_eq!(
            Version::parse("git version 2.51.0"),
            Some(Version {
                major: 2,
                minor: 51,
                patch: 0
            })
        );
        assert_eq!(
            Version::parse("git version 2.39.5 (Apple Git-154)"),
            Some(Version {
                major: 2,
                minor: 39,
                patch: 5
            })
        );
        assert_eq!(
            Version::parse("git version 2.45.1.windows.1"),
            Some(Version {
                major: 2,
                minor: 45,
                patch: 1
            })
        );
        assert_eq!(Version::parse("not a version line"), None);
        assert_eq!(Version::parse(""), None);
    }

    #[test]
    fn orders_by_component_not_by_string() {
        // "2.9" sorts after "2.35" as text, and before it as a version.
        let old = Version::parse_number("2.9.0").expect("valid");
        let new = Version::parse_number("2.35.0").expect("valid");
        assert!(old < new);
    }

    #[test]
    fn the_environment_is_scrubbed_before_anything_runs() {
        // Rule 2 of SPEC §8, checked without spawning: `Command` can be asked
        // what it would change about the environment.
        let mut command = Command::new("git");
        scrub_environment(&mut command);
        let changes: Vec<_> = command
            .get_envs()
            .map(|(key, value)| (key.to_string_lossy().into_owned(), value.is_some()))
            .collect();

        for (name, expected) in [
            ("GIT_TERMINAL_PROMPT", true),
            ("GIT_OPTIONAL_LOCKS", true),
            // The other prompt nobody can answer: a `vi` on a pipe.
            ("GIT_EDITOR", true),
            ("GIT_SEQUENCE_EDITOR", true),
            // An inherited GIT_DIR would point the command at another
            // repository than the one asked for.
            ("GIT_DIR", false),
            ("GIT_WORK_TREE", false),
            ("GIT_INDEX_FILE", false),
        ] {
            let set = changes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, set)| *set);
            assert_eq!(
                set,
                Some(expected),
                "{name} should be {}",
                if expected { "set" } else { "removed" }
            );
        }
    }

    #[test]
    fn a_missing_git_says_so() {
        let error = Git::detect_at(Path::new("git-omagit-does-not-exist"))
            .expect_err("this program is not installed");
        assert!(matches!(error, GitError::GitNotFound), "got {error:?}");
    }
}
