//! `omagit-git-cli` — the debug front end for the Git core (SPEC §14, M2).
//!
//! Every read `omagit-git` can do, reachable without a window. It exists for
//! three reasons:
//!
//! * to validate the core by hand against repositories nobody thought to write
//!   a fixture for — a decade-old monorepo, a shallow clone, a work tree on a
//!   network mount;
//! * to measure it, because SPEC §12's budgets are only real if they can be
//!   reproduced (`omagit-git-cli bench`);
//! * and because a Git core that can be driven from a command line is a Git
//!   core that has no UI dependency. If this binary stops compiling, rule 5 of
//!   SPEC §3 has been broken somewhere.
//!
//! The output is meant to be read by a person and diffed against `git`'s own,
//! so it echoes Git's vocabulary — two-letter status codes, `@@` headers, short
//! hashes — rather than inventing one.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use omagit_git::diff::{DiffContent, FileChange, FileDiff};
use omagit_git::status::short_code;
use omagit_git::{
    Cancel, Diff, DiffOptions, HistoryQuery, LineKind, ObjectId, Refs, Repository, Status,
    StatusOptions, Walk, cli,
};

#[derive(Parser)]
#[command(
    name = "omagit-git-cli",
    about = "Drive omagit's Git core from a terminal",
    version
)]
struct Cli {
    /// The repository to read. Searched upwards, like `git` itself.
    #[arg(long, short = 'C', global = true, default_value = ".")]
    repo: PathBuf,

    /// Print how long each read took.
    #[arg(long, global = true)]
    timing: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Where the repository is, where HEAD points, and what `git` is installed.
    Info,
    /// The working copy, in `git status --short` codes.
    Status(StatusArgs),
    /// Branches with ahead/behind, remotes and tags.
    Refs,
    /// The commit history, newest first.
    Log(LogArgs),
    /// One commit: its message, and the diff against its first parent.
    Show(ShowArgs),
    /// A diff of the working copy, or between two commits.
    Diff(DiffArgs),
    /// Time the reads SPEC §12 sets budgets for.
    Bench(BenchArgs),
}

#[derive(Args)]
struct StatusArgs {
    /// Include ignored files.
    #[arg(long)]
    ignored: bool,
    /// Leave out untracked files.
    #[arg(long)]
    no_untracked: bool,
    /// Do not detect renames.
    #[arg(long)]
    no_renames: bool,
}

#[derive(Args)]
struct LogArgs {
    /// How many commits to print.
    #[arg(long, short = 'n', default_value_t = 20)]
    limit: usize,
    /// Read in pages of this size, to exercise the resumable walk.
    #[arg(long, default_value_t = 100)]
    page: usize,
    /// Start from every branch and tag, not just HEAD.
    #[arg(long)]
    all: bool,
    /// Follow only the first parent of each merge.
    #[arg(long)]
    first_parent: bool,
}

#[derive(Args)]
struct ShowArgs {
    /// A commit, as a full hash. `HEAD` when omitted.
    revision: Option<String>,
}

#[derive(Args)]
struct DiffArgs {
    /// What is staged for the next commit.
    #[arg(long, conflicts_with_all = ["unstaged", "from"])]
    staged: bool,
    /// What is changed but not staged. The default.
    #[arg(long)]
    unstaged: bool,
    /// Compare this commit with `--to` (or with the working copy's HEAD).
    #[arg(long)]
    from: Option<String>,
    /// The other side of the comparison.
    #[arg(long, requires = "from")]
    to: Option<String>,
    /// Lines of context around each change.
    #[arg(long, default_value_t = 3)]
    context: u32,
    /// Do not compute the intra-line refinement.
    #[arg(long)]
    no_refine: bool,
}

#[derive(Args)]
struct BenchArgs {
    /// How many commits to walk when timing the history.
    #[arg(long, default_value_t = 1000)]
    commits: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cancel = Cancel::new();
    // Canonicalised so the paths printed back are the ones on disk, not the
    // `.` the shell happened to pass.
    let root = std::fs::canonicalize(&cli.repo)
        .with_context(|| format!("{} does not exist", cli.repo.display()))?;
    let repo =
        Repository::discover(&root).with_context(|| format!("opening {}", root.display()))?;
    let timing = cli.timing;

    match &cli.command {
        Command::Info => info(&repo)?,
        Command::Status(args) => {
            let (status, elapsed) = time(|| Status::load(&repo, args.options(), &cancel))?;
            print_status(&status);
            report("status", elapsed, timing);
        }
        Command::Refs => {
            let (refs, elapsed) = time(|| Refs::load(&repo, &cancel))?;
            print_refs(&refs);
            report("refs", elapsed, timing);
        }
        Command::Log(args) => {
            let elapsed = log(&repo, args, &cancel)?;
            report("log", elapsed, timing);
        }
        Command::Show(args) => {
            let id = revision(&repo, args.revision.as_deref())?;
            let commit = omagit_git::history::commit(&repo, id)?;
            println!("commit {}", commit.id);
            println!("Author: {} <{}>", commit.author.name, commit.author.email);
            println!("Date:   {}", format_time(commit.author.time));
            println!("\n    {}", commit.summary);
            for line in commit.body.lines() {
                println!("    {line}");
            }
            println!();
            let (diff, elapsed) =
                time(|| Diff::commit(&repo, id, DiffOptions::default(), &cancel))?;
            print_diff(&diff);
            report("diff", elapsed, timing);
        }
        Command::Diff(args) => {
            let elapsed = diff(&repo, args, &cancel)?;
            report("diff", elapsed, timing);
        }
        Command::Bench(args) => bench(&repo, args, &cancel)?,
    }
    Ok(())
}

impl StatusArgs {
    fn options(&self) -> StatusOptions {
        StatusOptions {
            include_ignored: self.ignored,
            include_untracked: !self.no_untracked,
            detect_renames: !self.no_renames,
        }
    }
}

fn info(repo: &Repository) -> Result<()> {
    println!("git dir:   {}", repo.git_dir().display());
    println!(
        "work tree: {}",
        repo.work_dir()
            .map_or_else(|| "(bare)".to_owned(), |path| path.display().to_string())
    );
    println!("HEAD:      {}", repo.head()?.label());
    if let Some(operation) = repo.operation() {
        println!("operation: {operation} in progress");
    }
    // The start-up check of SPEC §8, run here so its message can be seen.
    match cli::Git::detect() {
        Ok(git) => println!("git:       {} ({})", git.version(), git.program().display()),
        Err(error) => println!("git:       unusable — {error}"),
    }
    Ok(())
}

fn print_status(status: &Status) {
    if status.is_clean() {
        println!("(clean)");
        return;
    }
    for entry in &status.entries {
        let mut line = format!("{} {}", short_code(entry), entry.path);
        if let Some(omagit_git::StageChange::Renamed { from })
        | Some(omagit_git::StageChange::Copied { from }) = &entry.staged
        {
            line = format!("{line} <- {from}");
        }
        if let Some(omagit_git::WorktreeChange::Renamed { from }) = &entry.unstaged {
            line = format!("{line} <- {from}");
        }
        if let Some(conflict) = entry.conflict {
            line = format!("{line}  ({conflict})");
        }
        if entry.is_submodule {
            line = format!("{line}  (submodule)");
        }
        if entry.is_directory {
            line = format!("{line}/");
        }
        println!("{line}");
    }
    println!("\n{} entries", status.entries.len());
}

fn print_refs(refs: &Refs) {
    println!("branches");
    for branch in &refs.branches {
        let marker = if branch.is_head { "*" } else { " " };
        let tracking = match &branch.tracking {
            None => String::new(),
            Some(tracking) if tracking.gone => format!("  [{} gone]", tracking.upstream),
            Some(tracking) => format!(
                "  [{} +{} -{}]",
                tracking.upstream, tracking.ahead, tracking.behind
            ),
        };
        println!(
            "{marker} {} {}{tracking}",
            short(&branch.commit),
            branch.name
        );
    }

    if !refs.remote_branches.is_empty() {
        println!("\nremote branches");
        for branch in &refs.remote_branches {
            println!(
                "  {} {}/{}",
                short(&branch.commit),
                branch.remote,
                branch.name
            );
        }
    }

    if !refs.remotes.is_empty() {
        println!("\nremotes");
        for remote in &refs.remotes {
            println!("  {} {}", remote.name, remote.url.as_deref().unwrap_or("-"));
        }
    }

    if !refs.tags.is_empty() {
        println!("\ntags");
        for tag in &refs.tags {
            let kind = if tag.annotation.is_some() {
                "annotated"
            } else {
                "light"
            };
            println!("  {} {} ({kind})", short(&tag.commit), tag.name);
        }
    }
}

fn log(repo: &Repository, args: &LogArgs, cancel: &Cancel) -> Result<Duration> {
    let mut query = if args.all {
        HistoryQuery::all()
    } else {
        HistoryQuery::head()
    };
    if args.first_parent {
        query = query.first_parent();
    }

    let started = Instant::now();
    let mut walk = Walk::new(repo, query, cancel)?;
    let mut printed = 0;
    while printed < args.limit {
        let page = walk.next_page(args.page.min(args.limit - printed), cancel)?;
        if page.is_empty() {
            break;
        }
        for commit in page {
            let parents = if commit.is_merge() { " (merge)" } else { "" };
            println!(
                "{} {} <{}>{parents}  {}",
                short(&commit.id),
                commit.author.name,
                commit.author.email,
                commit.summary
            );
            printed += 1;
        }
    }
    let elapsed = started.elapsed();
    println!(
        "\n{printed} commits{}",
        if walk.is_done() { "" } else { ", more to come" }
    );
    Ok(elapsed)
}

fn diff(repo: &Repository, args: &DiffArgs, cancel: &Cancel) -> Result<Duration> {
    let options = DiffOptions {
        context_lines: args.context,
        refine: !args.no_refine,
        ..DiffOptions::default()
    };

    let started = Instant::now();
    let diff = if let Some(from) = &args.from {
        let from = revision(repo, Some(from))?;
        let to = match &args.to {
            Some(to) => revision(repo, Some(to))?,
            None => revision(repo, None)?,
        };
        Diff::between(repo, Some(from), Some(to), options, cancel)?
    } else {
        let status = Status::load(repo, StatusOptions::default(), cancel)?;
        if args.staged {
            Diff::staged(repo, &status, options, cancel)?
        } else {
            Diff::unstaged(repo, &status, options, cancel)?
        }
    };
    let elapsed = started.elapsed();
    print_diff(&diff);
    Ok(elapsed)
}

fn print_diff(diff: &Diff) {
    if diff.is_empty() {
        println!("(no differences)");
        return;
    }
    for file in &diff.files {
        println!("{}", header_of(file));
        match &file.content {
            DiffContent::Text {
                hunks,
                added,
                removed,
            } => {
                for hunk in hunks {
                    println!("{}", hunk.header());
                    for line in &hunk.lines {
                        let sign = match line.kind {
                            LineKind::Context => ' ',
                            LineKind::Added => '+',
                            LineKind::Removed => '-',
                        };
                        println!("{sign}{}", line.text_lossy());
                        if !line.refinements.is_empty() {
                            println!(" {}", underline(line));
                        }
                        if line.no_newline_at_eof {
                            println!("\\ No newline at end of file");
                        }
                    }
                }
                println!("({added} added, {removed} removed)");
            }
            DiffContent::Binary {
                old_bytes,
                new_bytes,
            } => println!("binary: {old_bytes} bytes -> {new_bytes} bytes"),
            DiffContent::Oversized { bytes } => {
                println!("too large to diff: {bytes} bytes");
            }
            DiffContent::Submodule { old, new } => println!(
                "submodule: {} -> {}",
                old.map(|id| short(&id)).unwrap_or_else(|| "-".into()),
                new.map(|id| short(&id)).unwrap_or_else(|| "-".into()),
            ),
            DiffContent::Empty => println!("(no content change)"),
        }
        println!();
    }
}

fn header_of(file: &FileDiff) -> String {
    match &file.change {
        FileChange::Added => format!("+++ {} (added)", file.path),
        FileChange::Deleted => format!("--- {} (deleted)", file.path),
        FileChange::Modified => format!("--- {}", file.path),
        FileChange::ModeChanged => format!("--- {} (mode changed)", file.path),
        FileChange::Renamed { from } => format!("--- {} -> {} (renamed)", from, file.path),
        FileChange::Copied { from } => format!("--- {} -> {} (copied)", from, file.path),
    }
}

/// A caret row under the refined spans, so the intra-line highlighting can be
/// checked in a terminal that has no colour to show it with.
fn underline(line: &omagit_git::Line) -> String {
    let mut marks = vec![b' '; line.text.len()];
    for span in &line.refinements {
        for mark in marks.iter_mut().take(span.end).skip(span.start) {
            *mark = b'^';
        }
    }
    String::from_utf8_lossy(&marks).trim_end().to_owned()
}

fn bench(repo: &Repository, args: &BenchArgs, cancel: &Cancel) -> Result<()> {
    println!("{}", repo.git_dir().display());

    // Twice: the first run pays for the index and the filesystem cache, the
    // second is what a user feels on a repository they already have open.
    let (status, cold) = time(|| Status::load(repo, StatusOptions::default(), cancel))?;
    let (_, warm) = time(|| Status::load(repo, StatusOptions::default(), cancel))?;
    println!(
        "status         {:>8.1?} cold  {:>8.1?} warm   ({} entries, budget 300ms)",
        cold,
        warm,
        status.entries.len()
    );

    let (count, elapsed) = time(|| -> omagit_git::Result<usize> {
        let mut walk = Walk::new(repo, HistoryQuery::all(), cancel)?;
        let mut seen = 0;
        while seen < args.commits {
            let page = walk.next_page(100.min(args.commits - seen), cancel)?;
            if page.is_empty() {
                break;
            }
            seen += page.len();
        }
        Ok(seen)
    })?;
    println!(
        "history        {:>8.1?}        ({count} commits, budget 250ms for 1000)",
        elapsed
    );

    let (refs, elapsed) = time(|| Refs::load(repo, cancel))?;
    println!(
        "refs           {:>8.1?}        ({} branches, {} tags)",
        elapsed,
        refs.branches.len(),
        refs.tags.len()
    );

    if let Some(head) = repo.head()?.commit().copied() {
        let (diff, elapsed) = time(|| Diff::commit(repo, head, DiffOptions::default(), cancel))?;
        println!(
            "diff of HEAD   {:>8.1?}        ({} files)",
            elapsed,
            diff.files.len()
        );
    }
    Ok(())
}

fn revision(repo: &Repository, revision: Option<&str>) -> Result<ObjectId> {
    match revision {
        None | Some("HEAD") => repo
            .head()?
            .commit()
            .copied()
            .context("this repository has no commits yet"),
        Some(text) => text
            .parse()
            .with_context(|| format!("{text} is not a full object hash")),
    }
}

/// A commit time as the author's own clock showed it.
///
/// Written out here rather than pulled from a date crate: the CLI needs one
/// format, and `omagit-git` deliberately hands back seconds and an offset
/// because formatting belongs to whoever displays it — with a locale, in the
/// app's case, and without one here.
fn format_time(time: omagit_git::history::Time) -> String {
    let local = time.seconds + i64::from(time.offset_seconds);
    let days = local.div_euclid(86_400);
    let seconds = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hours, minutes) = (seconds / 3600, (seconds % 3600) / 60);
    let sign = if time.offset_seconds < 0 { '-' } else { '+' };
    let offset = time.offset_seconds.abs();
    format!(
        "{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{:02} {sign}{:02}{:02}",
        seconds % 60,
        offset / 3600,
        (offset % 3600) / 60
    )
}

/// Days since the epoch to a civil date — Howard Hinnant's `civil_from_days`,
/// which is exact for every year a Git commit can carry.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

fn short(id: &ObjectId) -> String {
    id.to_string()[..8].to_owned()
}

fn time<T, E>(operation: impl FnOnce() -> std::result::Result<T, E>) -> Result<(T, Duration)>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let started = Instant::now();
    let value = operation()?;
    Ok((value, started.elapsed()))
}

fn report(what: &str, elapsed: Duration, enabled: bool) {
    if enabled {
        eprintln!("{what}: {elapsed:.1?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagit_git::history::Time;

    #[test]
    fn formats_a_commit_time_in_the_authors_own_offset() {
        // 2026-09-08T13:19:33Z, written by someone two hours east of UTC.
        let time = Time {
            seconds: 1_788_873_573,
            offset_seconds: 7200,
        };
        assert_eq!(format_time(time), "2026-09-08 15:19:33 +0200");

        let utc = Time {
            seconds: 0,
            offset_seconds: 0,
        };
        assert_eq!(format_time(utc), "1970-01-01 00:00:00 +0000");

        // A leap day, and a negative offset that crosses back over midnight.
        let leap = Time {
            seconds: 1_709_164_800,
            offset_seconds: -18_000,
        };
        assert_eq!(format_time(leap), "2024-02-28 19:00:00 -0500");
    }
}
