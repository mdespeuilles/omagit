#!/usr/bin/env bash
# A repository to drive omagit against by hand.
#
# The test suites already prove that omagit agrees with `git`. What they cannot
# do is show what the screens look like when a repository has shape: a history
# with a merge in it, a branch nobody has touched since the spring, a working
# copy holding six kinds of change at once, two stashes, and a merge that is
# going to conflict the moment somebody asks for it. Building that by hand takes
# twenty minutes and comes out different every time, which is what a fixture is
# for.
#
#   scripts/fixture.sh              # build it, leave the merge unstarted
#   scripts/fixture.sh --conflict   # and stop half-way through it
#   scripts/fixture.sh --at ~/work/omagit-fixture
#
# It lands in `~/omagit-fixture` because that is where a file picker can reach
# it — `$TMPDIR` on macOS is a `/var/folders/…` path the Finder hides. `--at`
# puts it anywhere else.
#
# Everything is deterministic: fixed identities, dates relative to the day it is
# run, and a "remote" that is a bare repository on the disk beside it. Nothing
# here touches the network, and nothing reads the developer's own Git
# configuration — a fixture that inherited `commit.gpgsign` or
# `init.defaultBranch` would look different on every machine.
#
# ## Why there are two repositories and not one
#
# `git merge` refuses to start when *anything* is staged — any path where the
# index differs from `HEAD`, related to the merge or not. Measured, not assumed:
# with the same change left unstaged it merges and conflicts as expected. So one
# repository cannot both show a working copy with something in the index and be
# ready to conflict on demand.
#
#   atelier            every working-copy state at once, two stashes, a history
#                      with shape, and a divergence from origin.
#   atelier-collegue   a clean tree on the same branches: this is where you
#                      press "Fusionner feature/theme-runtime" and land in the
#                      dialog, and where a rebase has a chance of running.
set -euo pipefail

# In the home directory, not under `$TMPDIR`, and that is about the file picker
# rather than about tidiness: on macOS `$TMPDIR` is `/var/folders/_j/kz8…/T`,
# which the Finder's open panel hides and offers no way to reach — the fixture
# was unreachable from the one button that adds a repository to omagit. `~` is
# the first place every picker on both platforms opens into.
root="${HOME:-/tmp}/omagit-fixture"
conflict=no

while [ $# -gt 0 ]; do
  case "$1" in
    --at) root="${2:?--at needs a directory}"; shift 2 ;;
    --conflict) conflict=yes; shift ;;
    -h|--help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "argument inconnu : $1" >&2; exit 2 ;;
  esac
done

repo="$root/atelier"
remote="$root/atelier.git"
collaborator="$root/atelier-collegue"
marker="$root/.omagit-fixture"

# Rebuilt from scratch every time: a fixture that accumulated state across runs
# would stop being the thing its own description says it is. Only ever a
# directory this script wrote itself — the marker says so, and without it
# nothing is deleted.
if [ -e "$root" ]; then
  if [ ! -e "$marker" ]; then
    echo "$root existe et n'a pas été créé par ce script — rien n'est effacé." >&2
    exit 1
  fi
  rm -rf "$root"
fi
mkdir -p "$root"
: >"$marker"

# The history ends a fortnight ago and runs six hours per commit, so it reads as
# recent whenever this is run: a fixture whose newest commit is dated last
# spring makes every relative date on the screen say the same thing.
now=$(date +%s)
clock=$(( now - 20 * 86400 ))
step=21600

git_at() {
  local where="$1"; shift
  git -C "$where" \
    -c user.name="Camille Dupré" \
    -c user.email="camille@omagit.test" \
    -c commit.gpgsign=false \
    -c tag.gpgsign=false \
    -c init.defaultBranch=main \
    -c core.editor=true \
    "$@"
}

git_repo() { git_at "$repo" "$@"; }

# One commit, at the next step on the fixture's clock.
commit() {
  clock=$((clock + step))
  GIT_AUTHOR_DATE="$clock +0100" GIT_COMMITTER_DATE="$clock +0100" \
    git_repo commit --quiet --message "$1"
}

# The same, back-dated: what a branch nobody has touched since the spring looks
# like in the sidebar.
commit_at() {
  GIT_AUTHOR_DATE="$1 +0100" GIT_COMMITTER_DATE="$1 +0100" \
    git_repo commit --quiet --message "$2"
}

write() {
  mkdir -p "$(dirname "$repo/$1")"
  cat >"$repo/$1"
}

echo "── le dépôt ──"
git init --quiet --initial-branch=main "$repo"
git_repo config user.name "Camille Dupré"
git_repo config user.email "camille@omagit.test"
git_repo config commit.gpgsign false

write .gitignore <<'EOF'
target/
*.log
EOF
write README.md <<'EOF'
# atelier

Un dépôt d'essai pour omagit. Rien ici ne sert à autre chose qu'à être lu à
l'écran : l'historique a une fusion, la copie de travail a six états à la fois,
et une branche attend d'entrer en conflit.
EOF
write src/main.rs <<'EOF'
fn main() {
    println!("atelier");
}
EOF
git_repo add --all
commit "Première pierre"

write src/theme.rs <<'EOF'
pub struct Theme {
    pub name: String,
    pub mode: Mode,
}

pub enum Mode {
    Dark,
    Light,
}

pub fn resolve(name: &str) -> Theme {
    Theme {
        name: name.to_owned(),
        mode: Mode::Dark,
    }
}
EOF
git_repo add --all
commit "Le thème, dans sa version la plus bête"

write src/render.rs <<'EOF'
pub fn draw(rows: &[String]) {
    for row in rows {
        println!("{row}");
    }
}

pub fn measure(rows: &[String]) -> usize {
    rows.iter().map(String::len).max().unwrap_or(0)
}

pub fn clear() {
    print!("\x1b[2J");
}
EOF
git_repo add --all
commit "Un rendu qui tient dans un terminal"

write src/merge.rs <<'EOF'
pub enum Strategy {
    Recursive,
    Ours,
}

pub fn resolve_strategy(name: &str) -> Strategy {
    match name {
        "ours" => Strategy::Ours,
        _ => Strategy::Recursive,
    }
}

pub fn describe(strategy: &Strategy) -> &'static str {
    match strategy {
        Strategy::Recursive => "récursive",
        Strategy::Ours => "la nôtre",
    }
}
EOF
write LICENSE-old <<'EOF'
Licence provisoire, à remplacer.
EOF
git_repo add --all
commit "La stratégie de fusion, et une licence provisoire"

# ── A branch that was merged, so the sidebar has one to draw as merged ──────
git_repo switch --quiet --create fix/statusbar-height
write src/render.rs <<'EOF'
pub fn draw(rows: &[String]) {
    for row in rows {
        println!("{row}");
    }
}

pub fn measure(rows: &[String]) -> usize {
    rows.iter().map(String::len).max().unwrap_or(0)
}

pub fn clear() {
    print!("\x1b[2J");
}

pub fn statusbar_height() -> u32 {
    26
}
EOF
git_repo add --all
commit "La barre du bas fait 26"

git_repo switch --quiet main
clock=$((clock + step))
GIT_AUTHOR_DATE="$clock +0100" GIT_COMMITTER_DATE="$clock +0100" \
  git_repo merge --quiet --no-ff --no-edit fix/statusbar-height

# ── A branch nobody has touched since the spring ────────────────────────────
git_repo switch --quiet --create old/gpui-spike
write src/spike.rs <<'EOF'
// Un essai abandonné. Gardé pour la mémoire, pas pour le code.
pub fn spike() {}
EOF
git_repo add --all
commit_at "$(( now - 210 * 86400 ))" "Un essai qu'on n'a pas gardé"

# ── A branch that rebases cleanly: nothing it touches is touched on main ────
git_repo switch --quiet main
git_repo switch --quiet --create feature/graph-lanes
write src/graph.rs <<'EOF'
pub struct Lane(pub usize);

pub fn lanes(parents: &[usize]) -> Vec<Lane> {
    parents.iter().map(|index| Lane(*index)).collect()
}
EOF
git_repo add --all
commit "Les lanes du graphe, en topologie seulement"

# ── Everything the working copy will be dirty in, committed first ───────────
#
# This is the ordering the header is about. `git merge` refuses when a file that
# differs between the merge base and `HEAD` has local changes, so every one of
# these has to be settled *before* the conflicting branch is cut.
git_repo switch --quiet main
write docs/journal.md <<'EOF'
# Journal

- On garde le graphe en topologie pure.
- La barre du bas fait 26, pas 24.
EOF
write docs/notes-anciennes.md <<'EOF'
Des notes qu'on garde, sous un autre nom.
EOF
write src/vieux.rs <<'EOF'
// Un fichier qui va disparaître de la copie de travail.
pub fn vieux() {}
EOF
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR omagit binaire' >"$repo/assets-logo.png"
git_repo add --all
commit "Un journal, des notes, un fichier de trop et un binaire"

# ── The branch that will conflict, cut here ─────────────────────────────────
#
# Two files that conflict in three places each — an enum, a match arm and a
# string, which is board 07's own mock almost line for line — so the dialog has
# a "conflit 1 / 3" to count and `n` something to move through. Plus
# `LICENSE-old`, deleted on one side and changed on the other: the kind where
# keeping "ours" means keeping the deletion, because there is no version of ours
# to restore.
git_repo switch --quiet --create feature/theme-runtime
write src/theme.rs <<'EOF'
pub struct Theme {
    pub name: String,
    pub mode: Mode,
    pub tint: f32,
}

pub enum Mode {
    Dark,
    Light,
    Auto,
}

pub fn resolve(name: &str) -> Theme {
    Theme {
        name: name.to_owned(),
        mode: Mode::Auto,
        tint: 0.12,
    }
}
EOF
write src/merge.rs <<'EOF'
pub enum Strategy {
    Recursive,
    Ours,
    Theirs,
}

pub fn resolve_strategy(name: &str) -> Strategy {
    match name {
        "ours" => Strategy::Ours,
        "theirs" => Strategy::Theirs,
        _ => Strategy::Recursive,
    }
}

pub fn describe(strategy: &Strategy) -> &'static str {
    match strategy {
        Strategy::Recursive => "récursive v2",
        Strategy::Ours => "la nôtre",
        Strategy::Theirs => "la leur",
    }
}
EOF
write LICENSE-old <<'EOF'
Licence provisoire, à remplacer.
Ajout côté branche : on la garde encore un peu.
EOF
git_repo add --all
commit "Le thème suit le système, et trois stratégies"

# ── main changes the same three files ───────────────────────────────────────
git_repo switch --quiet main
write src/theme.rs <<'EOF'
pub struct Theme {
    pub name: String,
    pub mode: Mode,
    pub contrast: f32,
}

pub enum Mode {
    Dark,
    Light,
    HighContrast,
}

pub fn resolve(name: &str) -> Theme {
    Theme {
        name: name.to_owned(),
        mode: Mode::HighContrast,
        contrast: 4.5,
    }
}
EOF
write src/merge.rs <<'EOF'
pub enum Strategy {
    Recursive,
    Ours,
    Octopus,
}

pub fn resolve_strategy(name: &str) -> Strategy {
    match name {
        "ours" => Strategy::Ours,
        "octopus" => Strategy::Octopus,
        _ => Strategy::Recursive,
    }
}

pub fn describe(strategy: &Strategy) -> &'static str {
    match strategy {
        Strategy::Recursive => "récursive",
        Strategy::Ours => "la nôtre",
        Strategy::Octopus => "la pieuvre",
    }
}
EOF
git_repo add --all
commit "Le contraste corrigé, et une pieuvre"

git_repo rm --quiet LICENSE-old
write LICENSE <<'EOF'
MIT, cette fois pour de bon.
EOF
git_repo add --all
commit "La vraie licence, et l'ancienne qui s'en va"

git_repo tag v0.0.9
GIT_AUTHOR_DATE="$clock +0100" GIT_COMMITTER_DATE="$clock +0100" \
  git_repo tag --annotate v0.1.0 --message "Première version montrable"

# ── The remote, and a colleague who pushed while you were away ──────────────
echo "── le distant ──"
git init --quiet --bare --initial-branch=main "$remote"
git_repo remote add origin "$remote"
git_repo push --quiet --set-upstream origin main
git_repo push --quiet origin feature/theme-runtime feature/graph-lanes

git clone --quiet "$remote" "$collaborator"
git_at "$collaborator" config user.name "Ines Roy"
git_at "$collaborator" config user.email "ines@omagit.test"
cat >"$collaborator/docs/décisions.md" <<'EOF'
# Décisions

- Les remises sont adressées par commit, jamais par leur numéro.
EOF
git_at "$collaborator" add --all
GIT_AUTHOR_DATE="$((clock + step)) +0100" GIT_COMMITTER_DATE="$((clock + step)) +0100" \
  git_at "$collaborator" commit --quiet --message "Un fichier de décisions, avec un nom accentué"
git_at "$collaborator" push --quiet origin main

# The same branches, locally, so the sidebar there has rows to press rather than
# only remote-tracking ones.
git_at "$collaborator" branch --quiet feature/theme-runtime origin/feature/theme-runtime
git_at "$collaborator" branch --quiet feature/graph-lanes origin/feature/graph-lanes

# Fetched once here, so the divergence is on screen before anybody presses
# anything. Fetch still has something to say afterwards — it is how the app
# notices a *second* push — but the interesting state is there at once.
git_repo fetch --quiet origin

# Two commits of your own on top, in files the working copy leaves alone: main
# is 2 ahead and 1 behind at the same time.
write src/settings.rs <<'EOF'
pub struct Settings {
    pub density: Density,
}

pub enum Density {
    Compact,
    Comfortable,
}
EOF
git_repo add --all
commit "Les réglages, et deux densités"

write src/palette.rs <<'EOF'
pub const LANES: usize = 8;

pub fn lane_colour(index: usize) -> usize {
    index % LANES
}
EOF
git_repo add --all
commit "Huit couleurs de lane, et pas une de plus"

# ── Two stashes ─────────────────────────────────────────────────────────────
echo "── les remises ──"
write src/render.rs <<'EOF'
pub fn draw(rows: &[String]) {
    for row in rows {
        println!("  {row}");
    }
}

pub fn measure(rows: &[String]) -> usize {
    rows.iter().map(String::len).max().unwrap_or(0)
}

pub fn clear() {
    print!("\x1b[2J");
}

pub fn statusbar_height() -> u32 {
    26
}
EOF
git_repo stash push --quiet --message "l'indentation du rendu, à revoir"

write src/settings.rs <<'EOF'
pub struct Settings {
    pub density: Density,
    pub theme: String,
}

pub enum Density {
    Compact,
    Comfortable,
}
EOF
write brouillon.md <<'EOF'
Des notes qui ne sont dans aucun commit. Cette remise-là les emporte.
EOF
git_repo stash push --quiet --include-untracked --message "les réglages, avec un brouillon non suivi"

# ── The working copy, holding six things at once ────────────────────────────
echo "── la copie de travail ──"

# Staged, and only staged.
write docs/journal.md <<'EOF'
# Journal

- On garde le graphe en topologie pure.
- La barre du bas fait 26, pas 24.
- Le panneau de diff appartient à l'écran ouvert.
EOF
git_repo add docs/journal.md

# Unstaged, in three separate hunks — enough to stage one and leave the others.
write src/render.rs <<'EOF'
pub fn draw(rows: &[String], indent: usize) {
    for row in rows {
        println!("{}{row}", " ".repeat(indent));
    }
}

pub fn measure(rows: &[String]) -> usize {
    rows.iter().map(String::len).max().unwrap_or(0)
}

pub fn clear() {
    print!("\x1b[2J\x1b[H");
}

pub fn statusbar_height() -> u32 {
    26
}

pub fn topbar_height() -> u32 {
    48
}
EOF

# Staged *and* unstaged at once: the half-in checkbox.
write src/main.rs <<'EOF'
mod render;

fn main() {
    println!("atelier");
}
EOF
git_repo add src/main.rs
write src/main.rs <<'EOF'
mod render;
mod settings;

fn main() {
    println!("atelier");
}
EOF

# A rename, staged. A deletion, unstaged.
git_repo mv docs/notes-anciennes.md docs/notes.md
rm "$repo/src/vieux.rs"

# Untracked, ignored, and binary — three the file list draws differently.
write bloc-notes.txt <<'EOF'
Rien de suivi ici.
EOF
write build.log <<'EOF'
ignoré par .gitignore
EOF
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR omagit binaire modifié' >"$repo/assets-logo.png"

# ── Optionally, the merge already stopped ───────────────────────────────────
#
# In the clone, because that is the one with a clean index. Checked rather than
# hoped for: a fixture that quietly merged cleanly, or was refused before
# starting, would be a fixture whose description is wrong.
if [ "$conflict" = yes ]; then
  echo "── la fusion, arrêtée en plein milieu ──"
  if git_at "$collaborator" merge --no-edit feature/theme-runtime >/dev/null 2>&1; then
    echo "la fusion est passée sans conflit — la fixture ne fait pas ce qu'elle dit" >&2
    exit 1
  fi
  if [ ! -e "$collaborator/.git/MERGE_HEAD" ]; then
    echo "la fusion a été refusée avant même de commencer :" >&2
    git_at "$collaborator" merge --no-edit feature/theme-runtime >&2 || true
    exit 1
  fi
fi

cat <<EOF

Le dépôt est là :

  $repo             la copie de travail, les remises, l'historique
  $collaborator    l'index propre : c'est là qu'on fusionne et qu'on rebase
  $remote         le « distant », sur le disque, sans réseau

Ce qu'il y a dedans :

  History      une quinzaine de commits, une fusion, deux tags dont un annoté,
               cinq branches — une déjà fusionnée, une que personne n'a touchée
               depuis sept mois.
  Topbar       main est 2 commits en avance et 1 en retard sur origin/main, et
               c'est déjà fetché : la divergence est à l'écran avant qu'on
               touche à quoi que ce soit. Pull et Push ont l'un et l'autre du
               travail.
  Working Copy un fichier indexé, un non indexé en trois hunks, un à moitié dans
               l'index, un renommage, une suppression, un non suivi, un ignoré,
               et un binaire.
  Stashes      deux remises, dont une qui emporte un fichier non suivi.
  Conflits     dans atelier-collegue : « Fusionner feature/theme-runtime »
               s'arrête sur trois fichiers. src/merge.rs et src/theme.rs en
               tiennent trois chacun — le dialogue compte « conflit 1 / 3 », et
               \`n\` a de quoi avancer — et LICENSE-old est supprimé d'un côté,
               modifié de l'autre : là, « garder le nôtre » veut dire garder la
               suppression.

Pourquoi deux dépôts : \`git merge\` refuse de démarrer dès que quelque chose est
indexé, quel que soit le fichier — donc le dépôt qui montre un index à moitié
rempli ne peut pas être celui qui fusionne. Dans atelier, « Fusionner » répond
donc par le refus de \`git\`, mot pour mot, ce qui vaut aussi d'être vu une fois.
\`git rebase\` va plus loin : il veut une copie de travail propre, donc pour
l'essayer dans atelier il faut remiser d'abord — une façon d'essayer les deux.

Aucun éditeur n'est configuré, donc « Ouvrir dans l'éditeur » passe par l'ouvreur
du bureau ; \`git -C "$collaborator" config core.editor code\` pour essayer
l'autre chemin.

Pour le refaire à zéro : scripts/fixture.sh$([ "$conflict" = yes ] && echo " --conflict")
EOF
