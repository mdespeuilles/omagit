#!/usr/bin/env bash
# A repository built to be photographed.
#
#   scripts/showcase.sh
#   scripts/showcase.sh --at ~/work/omagit-showcase
#
# `fixture.sh` is for driving the app by hand: small, French, every working-copy
# state at once, a merge waiting to conflict. This one has a different job. It is
# what the screenshots are taken of, and a screenshot gets looked at closely by
# somebody who has never seen the app. So it differs on four points, each chosen:
#
#   * **English**, because the screenshots go in the README and the release page.
#   * **A graph whose lanes cross.** Four branches are alive at once and their
#     commits alternate in time. That alternation is the whole trick: a script
#     that writes all of branch A and then all of branch B draws two lanes end to
#     end, never side by side. Here one clock advances on every commit whichever
#     branch it is on, so the walk interleaves them and the gutter has something
#     to draw.
#   * **Rust, TOML, JSON and Markdown**, the four the diff highlights
#     (ARCHITECTURE §2.18). A diff in a language it does not colour photographs
#     the wrong thing.
#   * **Two authors and thirty-odd commits**, enough to fill a window and page as
#     it scrolls rather than stopping three rows down.
#
# Everything is deterministic apart from the dates, which are relative to the day
# it runs — the newest commit is about three hours old, so the history never says
# "last spring". It reads none of the developer's own Git configuration.
set -euo pipefail

root="${HOME:-/tmp}/omagit-showcase"

while [ $# -gt 0 ]; do
  case "$1" in
    --at) root="${2:?--at needs a directory}"; shift 2 ;;
    -h|--help) sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

repo="$root/orbit"
remote="$root/orbit.git"
teammate="$root/orbit-teammate"
marker="$root/.omagit-showcase"

# Rebuilt from nothing, and only ever over a directory this script wrote itself.
if [ -e "$root" ]; then
  if [ ! -e "$marker" ]; then
    echo "$root exists and was not made by this script — nothing is deleted." >&2
    exit 1
  fi
  rm -rf "$root"
fi
mkdir -p "$root"
: >"$marker"

# ── The clock ───────────────────────────────────────────────────────────────
#
# Uneven gaps, cycling. Even ones read as a machine; these read as evenings and
# weekends. The start is computed backwards from the number of commits below, so
# the newest one always lands about three hours ago however the history is edited.

gaps=(3 5 11 26 7 19 2 33)   # hours between one commit and the next
commits=32                   # every commit made below; checked at the end

now=$(date +%s)
span=0
for (( i = 0; i < commits; i++ )); do
  span=$(( span + gaps[i % ${#gaps[@]}] * 3600 ))
done
clock=$(( now - span - 3 * 3600 ))
tick=0

advance() {
  clock=$(( clock + gaps[tick % ${#gaps[@]}] * 3600 ))
  tick=$(( tick + 1 ))
}

# ── Git, with an identity of its own ────────────────────────────────────────

git_at() {
  local where="$1" name="$2" email="$3"; shift 3
  git -C "$where" \
    -c user.name="$name" \
    -c user.email="$email" \
    -c commit.gpgsign=false \
    -c tag.gpgsign=false \
    -c init.defaultBranch=main \
    -c core.editor=true \
    "$@"
}
git_repo() { git_at "$repo" "Ada Okonkwo" "ada@orbit.test" "$@"; }

author_name() {
  case "$1" in
    ada) echo "Ada Okonkwo" ;;
    nils) echo "Nils Bergström" ;;
    *) echo "unknown author: $1" >&2; exit 2 ;;
  esac
}
author_email() {
  case "$1" in
    ada) echo "ada@orbit.test" ;;
    nils) echo "nils@orbit.test" ;;
  esac
}

# commit "<subject>" [ada|nils] — at the next tick.
#
# The clock is global and never rewinds, so a commit made on a side branch lands
# *between* two commits of main. That is what puts the lanes beside each other
# instead of one after the other.
commit() {
  local who="${2:-ada}"
  advance
  GIT_AUTHOR_NAME="$(author_name "$who")" \
  GIT_AUTHOR_EMAIL="$(author_email "$who")" \
  GIT_AUTHOR_DATE="$clock +0000" \
  GIT_COMMITTER_DATE="$clock +0000" \
    git_repo commit --quiet --message "$1"
}

merge() {
  advance
  GIT_AUTHOR_DATE="$clock +0000" GIT_COMMITTER_DATE="$clock +0000" \
    git_repo merge --quiet --no-ff --no-edit --message "$2" "$1"
}

tag_at() {
  GIT_COMMITTER_DATE="$clock +0000" \
    git_repo tag --annotate "$1" --message "$2"
}

on() { git_repo switch --quiet "$@"; }

write() {
  mkdir -p "$(dirname "$repo/$1")"
  cat >"$repo/$1"
}

echo "── the trunk ──"
git init --quiet --initial-branch=main "$repo"

write .gitignore <<'EOF'
target/
*.log
.env
EOF
write Cargo.toml <<'EOF'
[package]
name = "orbit"
version = "0.1.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }
EOF
write README.md <<'EOF'
# orbit

A small service that keeps a satellite's telemetry and answers questions about
it. It exists to be read, not to be run.
EOF
write src/main.rs <<'EOF'
fn main() {
    println!("orbit");
}
EOF
git_repo add --all
commit "First commit"

write src/config.rs <<'EOF'
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub port: u16,
    pub database: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8080,
            database: "orbit.db".to_owned(),
        }
    }
}
EOF
write config/default.json <<'EOF'
{
  "port": 8080,
  "database": "orbit.db",
  "log": "info"
}
EOF
git_repo add --all
commit "Read the configuration from a file rather than the environment"

write src/router.rs <<'EOF'
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

pub const ROUTES: &[Route] = &[
    Route { method: "GET", path: "/health" },
    Route { method: "GET", path: "/telemetry" },
];

pub fn find(method: &str, path: &str) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|route| route.method == method && route.path == path)
}
EOF
git_repo add --all
commit "A router that answers two paths" nils

write src/store.rs <<'EOF'
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Store {
    readings: BTreeMap<i64, f64>,
}

impl Store {
    pub fn record(&mut self, at: i64, value: f64) {
        self.readings.insert(at, value);
    }

    pub fn since(&self, at: i64) -> Vec<(i64, f64)> {
        self.readings
            .range(at..)
            .map(|(when, value)| (*when, *value))
            .collect()
    }
}
EOF
git_repo add --all
commit "Keep the readings in memory, ordered by time"

tag_at v0.1.0 "First tag. Two routes and a map in memory."

# ── A branch abandoned early ────────────────────────────────────────────────
#
# Left here, right after v0.1.0, so its last commit is genuinely the oldest tip
# in the repository. No date trickery: it simply stopped.

echo "── the abandoned branch ──"
on --create old/protobuf-transport
write proto/telemetry.proto <<'EOF'
syntax = "proto3";

package orbit;

message Reading {
  int64 at = 1;
  double value = 2;
}

message Batch {
  repeated Reading readings = 1;
}
EOF
git_repo add --all
commit "A schema for the readings, if we ever go binary" nils

write docs/protobuf.md <<'EOF'
# Protobuf transport

Smaller on the wire and harder to debug. Parked until somebody measures what the
text format is actually costing us.
EOF
git_repo add --all
commit "Park the binary transport until somebody measures" nils

# ── Two branches, growing at the same time ──────────────────────────────────
#
# Every block below is one commit, and the branch changes between almost all of
# them. That is deliberate; see the note at the top.

echo "── the branches ──"

on --create feature/streaming main
write src/stream.rs <<'EOF'
pub struct Frame {
    pub at: i64,
    pub value: f64,
}

pub fn encode(frame: &Frame) -> String {
    format!("{}:{}", frame.at, frame.value)
}
EOF
git_repo add --all
commit "Frames, and a wire format small enough to read"

on main
write docs/decisions.md <<'EOF'
# Decisions

## The store is a map, not a database

Ordered by time and small enough to hold. A database would answer questions
nobody is asking yet.
EOF
git_repo add --all
commit "Write down why the store is a map"

on --create feature/back-pressure main
write src/limits.rs <<'EOF'
pub struct Limits {
    pub per_second: u32,
    pub burst: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { per_second: 50, burst: 100 }
    }
}
EOF
git_repo add --all
commit "A budget per second, and a burst above it" nils

on feature/streaming
write src/stream.rs <<'EOF'
pub struct Frame {
    pub at: i64,
    pub value: f64,
}

pub fn encode(frame: &Frame) -> String {
    format!("{}:{}", frame.at, frame.value)
}

pub fn decode(line: &str) -> Option<Frame> {
    let (at, value) = line.split_once(':')?;
    Some(Frame {
        at: at.parse().ok()?,
        value: value.parse().ok()?,
    })
}
EOF
git_repo add --all
commit "Decode what we encode, and refuse the rest"

on main
write src/router.rs <<'EOF'
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

pub const ROUTES: &[Route] = &[
    Route { method: "GET", path: "/health" },
    Route { method: "GET", path: "/telemetry" },
    Route { method: "POST", path: "/telemetry" },
];

pub fn find(method: &str, path: &str) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|route| route.method == method && route.path == path)
}

pub fn allowed(path: &str) -> Vec<&'static str> {
    ROUTES
        .iter()
        .filter(|route| route.path == path)
        .map(|route| route.method)
        .collect()
}
EOF
git_repo add --all
commit "Accept a reading as well as hand one back"

on feature/back-pressure
write src/limits.rs <<'EOF'
use std::time::{Duration, Instant};

pub struct Limits {
    pub per_second: u32,
    pub burst: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { per_second: 50, burst: 100 }
    }
}

pub struct Bucket {
    left: f64,
    filled: Instant,
}

impl Bucket {
    pub fn take(&mut self, limits: &Limits) -> bool {
        let since = self.filled.elapsed().as_secs_f64();
        self.left = (self.left + since * f64::from(limits.per_second))
            .min(f64::from(limits.burst));
        self.filled = Instant::now();
        if self.left < 1.0 {
            return false;
        }
        self.left -= 1.0;
        true
    }
}

pub const WINDOW: Duration = Duration::from_secs(1);
EOF
git_repo add --all
commit "A leaky bucket, refilled on read rather than on a timer" nils

on --create fix/clock-drift main
write src/clock.rs <<'EOF'
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}
EOF
git_repo add --all
commit "One clock, so two halves of the service cannot disagree"

on feature/streaming
write src/stream.rs <<'EOF'
pub struct Frame {
    pub at: i64,
    pub value: f64,
}

pub fn encode(frame: &Frame) -> String {
    format!("{}:{}", frame.at, frame.value)
}

pub fn decode(line: &str) -> Option<Frame> {
    let (at, value) = line.split_once(':')?;
    Some(Frame {
        at: at.parse().ok()?,
        value: value.parse().ok()?,
    })
}

pub fn batch(frames: &[Frame]) -> String {
    frames.iter().map(encode).collect::<Vec<_>>().join("\n")
}
EOF
write docs/wire.md <<'EOF'
# The wire format

One frame per line, `timestamp:value`. Nothing else, on purpose: a format you
can read with `cat` is a format you can debug on a bad day.
EOF
git_repo add --all
commit "Batch frames, and write the format down"

on main
merge feature/streaming "Merge branch 'feature/streaming'"

on fix/clock-drift
write src/clock.rs <<'EOF'
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

/// Monotonic, for measuring an interval. `now()` is allowed to go backwards.
pub fn ticks() -> u128 {
    std::time::Instant::now().elapsed().as_nanos()
}
EOF
git_repo add --all
commit "Separate the wall clock from the one that measures"

on main
write config/default.json <<'EOF'
{
  "port": 8080,
  "database": "orbit.db",
  "log": "info",
  "stream": {
    "batch": 64,
    "flush_ms": 200
  }
}
EOF
git_repo add --all
commit "Configure the stream, now that there is one"

tag_at v0.2.0 "Streaming, end to end."

on feature/back-pressure
write src/limits.rs <<'EOF'
use std::time::{Duration, Instant};

pub struct Limits {
    pub per_second: u32,
    pub burst: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { per_second: 50, burst: 100 }
    }
}

pub struct Bucket {
    left: f64,
    filled: Instant,
}

impl Bucket {
    pub fn new(limits: &Limits) -> Self {
        Self {
            left: f64::from(limits.burst),
            filled: Instant::now(),
        }
    }

    pub fn take(&mut self, limits: &Limits) -> bool {
        let since = self.filled.elapsed().as_secs_f64();
        self.left = (self.left + since * f64::from(limits.per_second))
            .min(f64::from(limits.burst));
        self.filled = Instant::now();
        if self.left < 1.0 {
            return false;
        }
        self.left -= 1.0;
        true
    }

    /// What to put in `Retry-After`, in whole seconds, never zero.
    pub fn retry_after(&self, limits: &Limits) -> u64 {
        let missing = (1.0 - self.left).max(0.0);
        (missing / f64::from(limits.per_second)).ceil().max(1.0) as u64
    }
}

pub const WINDOW: Duration = Duration::from_secs(1);
EOF
git_repo add --all
commit "Tell the caller when to come back instead of only refusing" nils

on main
merge fix/clock-drift "Merge branch 'fix/clock-drift'"

on --create experiment/zero-copy main
write src/stream.rs <<'EOF'
pub struct Frame<'a> {
    pub at: i64,
    pub raw: &'a str,
}

pub fn parse(line: &str) -> Option<Frame<'_>> {
    let (at, raw) = line.split_once(':')?;
    Some(Frame {
        at: at.parse().ok()?,
        raw,
    })
}
EOF
git_repo add --all
commit "Borrow the line instead of copying out of it"

on main
write src/main.rs <<'EOF'
mod clock;
mod config;
mod router;
mod store;
mod stream;

fn main() {
    let config = config::Config::default();
    println!("orbit on :{}", config.port);
}
EOF
git_repo add --all
commit "Wire the modules together in main"

on feature/back-pressure
write docs/limits.md <<'EOF'
# Limits

Fifty a second, a hundred in a burst. Both are guesses, and both live in
`config/default.json` so a guess can be corrected without a release.
EOF
git_repo add --all
commit "Write the limits down, and say they are guesses" nils

on main
merge feature/back-pressure "Merge branch 'feature/back-pressure'"

# ── A long tail on main, so the history pages ───────────────────────────────

write src/store.rs <<'EOF'
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Store {
    readings: BTreeMap<i64, f64>,
    kept: usize,
}

impl Store {
    pub fn with_capacity(kept: usize) -> Self {
        Self { readings: BTreeMap::new(), kept }
    }

    pub fn record(&mut self, at: i64, value: f64) {
        self.readings.insert(at, value);
        while self.kept > 0 && self.readings.len() > self.kept {
            let oldest = *self.readings.keys().next().expect("not empty");
            self.readings.remove(&oldest);
        }
    }

    pub fn since(&self, at: i64) -> Vec<(i64, f64)> {
        self.readings
            .range(at..)
            .map(|(when, value)| (*when, *value))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.readings.len()
    }
}
EOF
git_repo add --all
commit "Drop the oldest reading once the store is full"

write tests/store.rs <<'EOF'
use orbit::store::Store;

#[test]
fn the_oldest_reading_goes_first() {
    let mut store = Store::with_capacity(2);
    store.record(1, 1.0);
    store.record(2, 2.0);
    store.record(3, 3.0);
    assert_eq!(store.since(0), vec![(2, 2.0), (3, 3.0)]);
}
EOF
git_repo add --all
commit "A test for the part that throws data away"

write README.md <<'EOF'
# orbit

A small service that keeps a satellite's telemetry and answers questions about
it. It exists to be read, not to be run.

## Routes

| Method | Path         | What it does                       |
| ------ | ------------ | ---------------------------------- |
| GET    | `/health`    | says whether the process is alive  |
| GET    | `/telemetry` | the readings since a given instant |
| POST   | `/telemetry` | records one reading                |

## Limits

Fifty requests a second, a hundred in a burst. A refusal carries `Retry-After`.
EOF
git_repo add --all
commit "A README somebody could actually start from"

write src/router.rs <<'EOF'
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

pub const ROUTES: &[Route] = &[
    Route { method: "GET", path: "/health" },
    Route { method: "GET", path: "/telemetry" },
    Route { method: "POST", path: "/telemetry" },
    Route { method: "GET", path: "/metrics" },
];

pub fn find(method: &str, path: &str) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|route| route.method == method && route.path == path)
}

pub fn allowed(path: &str) -> Vec<&'static str> {
    ROUTES
        .iter()
        .filter(|route| route.path == path)
        .map(|route| route.method)
        .collect()
}
EOF
git_repo add --all
commit "Expose the counters the operators keep asking for" nils

write Cargo.toml <<'EOF'
[package]
name = "orbit"
version = "0.3.0"
edition = "2024"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
EOF
git_repo add --all
commit "0.3.0"

tag_at v0.3.0 "Metrics, limits, and a store that forgets."

write docs/decisions.md <<'EOF'
# Decisions

## The store is a map, not a database

Ordered by time and small enough to hold. A database would answer questions
nobody is asking yet.

## Limits are refilled on read

A timer would need a thread. Reading the clock at the moment somebody asks is
the same answer for none of the cost.

## The wire format is one frame per line

You can read it with `cat`, which is worth more than the bytes it costs.
EOF
git_repo add --all
commit "Three decisions, written down while they are still fresh"

# ── The remote ──────────────────────────────────────────────────────────────
#
# On disk, so none of this touches a network. v0.3.0 is deliberately left
# unpushed: the tag list then has one of each.

echo "── the remote ──"
git init --quiet --bare --initial-branch=main "$remote"
git_repo remote add origin "$remote"
git_repo push --quiet --set-upstream origin main
git_repo push --quiet --set-upstream origin feature/streaming feature/back-pressure experiment/zero-copy
git_repo push --quiet origin v0.1.0 v0.2.0

# Somebody else pushes one commit, so main ends up behind as well as ahead.
git clone --quiet "$remote" "$teammate"
mkdir -p "$teammate/docs"
cat >"$teammate/docs/runbook.md" <<'EOF'
# Runbook

## It stopped answering

Check `/health` first. If the process is up and the reading count is zero, the
ingest side is what broke, not the service.
EOF
git_at "$teammate" "Nils Bergström" "nils@orbit.test" add --all
GIT_AUTHOR_DATE="$(( now - 5 * 3600 )) +0000" \
GIT_COMMITTER_DATE="$(( now - 5 * 3600 )) +0000" \
  git_at "$teammate" "Nils Bergström" "nils@orbit.test" \
    commit --quiet --message "A runbook, written the morning after"
git_at "$teammate" "Nils Bergström" "nils@orbit.test" push --quiet origin main

# Three commits that exist only here, so the topbar has an ahead count.
on main
write src/health.rs <<'EOF'
pub struct Health {
    pub alive: bool,
    pub readings: usize,
}

pub fn check(readings: usize) -> Health {
    Health { alive: true, readings }
}
EOF
git_repo add --all
commit "A health check that counts rather than guesses"

write src/main.rs <<'EOF'
mod clock;
mod config;
mod health;
mod limits;
mod router;
mod store;
mod stream;

fn main() {
    let config = config::Config::default();
    let store = store::Store::with_capacity(10_000);
    println!("orbit on :{} holding {}", config.port, store.len());
}
EOF
git_repo add --all
commit "Start the store with a capacity instead of letting it grow"

write config/default.json <<'EOF'
{
  "port": 8080,
  "database": "orbit.db",
  "log": "info",
  "stream": {
    "batch": 64,
    "flush_ms": 200
  },
  "limits": {
    "per_second": 50,
    "burst": 100
  }
}
EOF
git_repo add --all
commit "Move the limits into the configuration file"

git_repo fetch --quiet origin

# ── Two stashes ─────────────────────────────────────────────────────────────

echo "── the stashes ──"
write src/router.rs <<'EOF'
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub authenticated: bool,
}
EOF
git_repo stash push --quiet --message "routes carrying whether they need a token"

write src/metrics.rs <<'EOF'
pub struct Counters {
    pub requests: u64,
    pub refused: u64,
}
EOF
write notes.txt <<'EOF'
Committed nowhere. This stash takes it along.
EOF
git_repo stash push --quiet --include-untracked --message "counters, and an untracked scratch file"

# ── The working copy, with one of every kind ────────────────────────────────

echo "── the working copy ──"

# Staged only.
write docs/wire.md <<'EOF'
# The wire format

One frame per line, `timestamp:value`. Nothing else, on purpose: a format you
can read with `cat` is a format you can debug on a bad day.

## Batching

Up to `stream.batch` frames go out together, or after `stream.flush_ms`,
whichever comes first.
EOF
git_repo add docs/wire.md

# A rename, staged, which the status has its own letter for.
git_repo mv docs/limits.md docs/limits-and-budgets.md

# Unstaged, in three separate places, so the hunks can be staged one at a time.
write src/store.rs <<'EOF'
use std::collections::BTreeMap;

/// Readings, oldest first, with a ceiling.
#[derive(Default)]
pub struct Store {
    readings: BTreeMap<i64, f64>,
    kept: usize,
}

impl Store {
    pub fn with_capacity(kept: usize) -> Self {
        Self { readings: BTreeMap::new(), kept }
    }

    pub fn record(&mut self, at: i64, value: f64) {
        self.readings.insert(at, value);
        while self.kept > 0 && self.readings.len() > self.kept {
            let oldest = *self.readings.keys().next().expect("not empty");
            self.readings.remove(&oldest);
        }
    }

    pub fn since(&self, at: i64) -> Vec<(i64, f64)> {
        self.readings
            .range(at..)
            .map(|(when, value)| (*when, *value))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.readings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }
}
EOF

# Staged, then changed again, which the status shows on both sides at once.
write src/config.rs <<'EOF'
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub port: u16,
    pub database: String,
    pub log: String,
}
EOF
git_repo add src/config.rs
write src/config.rs <<'EOF'
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub port: u16,
    pub database: String,
    pub log: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8080,
            database: "orbit.db".to_owned(),
            log: "info".to_owned(),
        }
    }
}
EOF

# Deleted, and untracked.
rm "$repo/src/clock.rs"
write scratch.md <<'EOF'
An untracked file, so the status has one of those too.
EOF

# ── Did the clock land where it was told? ───────────────────────────────────

made=$(git_repo rev-list --count --branches)
if [ "$made" != "$commits" ]; then
  echo >&2
  echo "  note: commits= says $commits, the script made $made." >&2
  echo "  The newest commit is $(( (now - $(git_repo log -1 --format=%at main)) / 3600 ))h old" >&2
  echo "  instead of 3h. Set commits=$made at the top." >&2
fi

echo
echo "── ready ──"
echo
git_repo -c color.ui=always log --graph --oneline --all --decorate -14 | sed 's/^/  /'
echo
echo "  $made commits · $(git_repo branch --list | wc -l | tr -d ' ') branches · $(git_repo tag --list | wc -l | tr -d ' ') tags · 2 stashes"
echo "  main is $(git_repo rev-list --count origin/main..main) ahead and $(git_repo rev-list --count main..origin/main) behind origin"
echo
echo "Add it in omagit with « Add a local repository », pointed at:"
echo
echo "  $repo"
