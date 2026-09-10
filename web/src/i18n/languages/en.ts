// English — the reference catalogue.
//
// Every other language is typed against this one, so a key added here without a
// translation is a compile error there, and a key removed here is a compile
// error at its call site. That is the whole reason the catalogues are
// TypeScript and not JSON.
//
// Conventions, so a translator has something to hold on to:
//
// * keys read `area.thing`, sorted by area, and the area is where the string is
//   *shown* — not where it is built;
// * `{name}` is filled in by the caller. What each one holds is said in the
//   comment above the key when it is not obvious from the sentence;
// * a string that counts has one entry per plural category — `.one`, `.other`,
//   and `.few` or `.many` in the languages that have them. `{n}` is the number.
//   Which entry applies is `Intl.PluralRules`', never ours;
// * Git's own vocabulary stays: `Fetch`, `Pull`, `Push`, `HEAD`, `stash`. A
//   translated `Push` is a button nobody recognises, and the words are the ones
//   the terminal uses.

export const name = "English";

export const strings = {
  // ── The topbar ──────────────────────────────────────────────────────────
  "topbar.back": "Back to the repositories",
  "topbar.repositories": "Repositories",
  "topbar.addRepository": "Add a local repository",
  "topbar.clone": "Clone…",
  "topbar.cloneTitle": "Clone a repository",
  "topbar.search": "Search",
  "topbar.palette": "Command palette",
  /// `{reason}` is what `git` said when it could not be run.
  "topbar.gitUnusable": "git unavailable — {reason}",
  "topbar.noUpstream": "This branch tracks no remote branch",
  "topbar.detached": "HEAD is detached: there is no branch to publish",
  "topbar.networkBusy": "a network operation is already running",

  // ── The tabs ────────────────────────────────────────────────────────────
  "tabs.close": "Close {name}",

  // ── The sidebar ─────────────────────────────────────────────────────────
  "sidebar.workspace": "Workspace",
  "sidebar.workingCopy": "Working Copy",
  "sidebar.history": "History",
  "sidebar.stashes": "Stashes",
  "sidebar.settings": "Settings",
  "sidebar.allRepositories": "All repositories",

  // ── The status bar ──────────────────────────────────────────────────────
  "statusbar.staged.one": "{n} staged",
  "statusbar.staged.other": "{n} staged",
  "statusbar.unstaged.one": "{n} unstaged",
  "statusbar.unstaged.other": "{n} unstaged",
  /// `{operation}` is `git`'s own word: merge, rebase, cherry-pick.
  "statusbar.running": "{operation} in progress",
  "statusbar.continue": "Continue",
  "statusbar.finish": "Finish {operation}",
  "statusbar.conflictsLeft.one": "{n} conflict left to settle",
  "statusbar.conflictsLeft.other": "{n} conflicts left to settle",
  "statusbar.abort": "Abort",
  "statusbar.conflicts.one": "{n} conflict",
  "statusbar.conflicts.other": "{n} conflicts",
  "statusbar.journal": "Journal",

  // ── The working copy ────────────────────────────────────────────────────
  "status.title": "Status",
  "status.stageAll": "Stage everything",
  "status.unstageAll": "Unstage everything",
  "status.reading": "Reading the status…",
  "status.clean": "Nothing changed",
  "status.stageFile": "Stage this file",
  "status.unstageFile": "Unstage this file",
  "status.resolved": "Mark this file resolved",
  "status.resolve": "Resolve…",
  "status.resolveTitle": "Settle it conflict by conflict",
  "status.discard": "Discard",
  "status.discardTitle": "Discard the unstaged changes",
  "status.keepOurs": "Keep ours",
  "status.keepTheirs": "Keep theirs",

  // ── The commit box ──────────────────────────────────────────────────────
  "commit.message": "Commit message",
  "commit.amendMessage": "Message of the amended commit",
  "commit.amend": "Amend",
  "commit.signOff": "Sign off",
  "commit.noVerify": "No verify",
  "commit.noVerifyTitle": "Skips the pre-commit and commit-msg hooks",
  "commit.noIdentity": "no identity configured",
  "commit.do.one": "Commit {n} file",
  "commit.do.other": "Commit {n} files",
  "commit.doAmend": "Amend the commit",
  "commit.blockedGit": "git unavailable — {reason}",
  "commit.blockedUnborn": "No commit to amend: this one will be the first",
  "commit.blockedIdentity":
    "No Git identity: git config --global user.name && git config --global user.email",
  "commit.blockedEmpty": "Nothing is staged",
  "commit.blockedMessage": "The message is empty",
  "commit.detached": "{head}: this commit will belong to no branch.",

  // ── The diff ────────────────────────────────────────────────────────────
  "diff.conflicted": "In conflict",
  "diff.unstaged": "Unstaged",
  "diff.staged": "Staged",
  "diff.none": "No file open",
  "diff.reading": "Reading the diff…",
  "diff.nothing": "Nothing to show for this file",
  "diff.stageHunk": "Stage the hunk",
  "diff.unstageHunk": "Unstage the hunk",
  "diff.discard": "Discard",
  "diff.folded.one": "{n} line of context folded",
  "diff.folded.other": "{n} lines of context folded",
  "diff.noNewline": "⏎̸ no newline at end of file",
  "diff.hunks.one": "{n} hunk",
  "diff.hunks.other": "{n} hunks",
  "diff.picked.one": "{n} line",
  "diff.picked.other": "{n} lines",
  "diff.stage": "Stage",
  "diff.unstage": "Unstage",
  "diff.clear": "Clear",

  // ── The journal and the bands ───────────────────────────────────────────
  "journal.title": "Journal",
  "journal.close": "Close",
  "journal.empty":
    "No command yet — every Git write is recorded here with the exact line that ran.",
  "progress.cancel": "Cancel",
  "progress.stopping": "stopping…",

  // ── The two sides of a conflict ─────────────────────────────────────────
  //
  // Named by their branch wherever `git` gives one. `ours` and `theirs` are
  // only honest during a merge: on a replay — a rebase, a cherry-pick — `ours`
  // is the side already in place and `theirs` is the work being replayed, which
  // is usually your own.
  "conflict.inPlace": "the version in place",
  "conflict.arriving": "the one arriving",
  "conflict.keep": "Keep the version from {side} ({pronoun})",
  "conflict.oursReplayed": "the side already in place, the one commits are replayed onto",
  "conflict.theirsReplayed": "the replayed side, the commits being put back",

  // ── History ─────────────────────────────────────────────────────────────
  "history.title": "History",
  "history.allBranches": "All branches",
  "history.trunk": "Trunk",
  "history.onBranch": "Back to the current branch's history",
  "history.idle": "History not loaded",
  "history.reading": "Reading the history…",
  "history.noMatch": "No commit matches.",
  "history.clearFilters": "Clear the filters",
  "history.unborn": "This repository has no commit yet — the first is made from the working copy.",
  "history.empty": "No commit",
  "history.more": "· more…",
  "history.filter": "Filter",
  "history.author": "Author",
  "history.message": "Message",
  "history.path": "Path",
  "history.since": "Since",
  "history.until": "Until",
  "history.clear": "Clear",
  "history.filtered":
    "Search result — no graph: neighbouring rows are not related, only the matching ones that follow.",

  // ── A commit, and a comparison ──────────────────────────────────────────
  "commitDetail.comparison": "Comparison",
  "commitDetail.title": "Commit",
  "commitDetail.readingComparison": "Reading the comparison…",
  "commitDetail.between.one": "{n} file between the two",
  "commitDetail.between.other": "{n} files between the two",
  "commitDetail.stopComparing": "Stop comparing",
  "commitDetail.files": "Files",
  "commitDetail.same": "Nothing differs between these two commits",
  "commitDetail.none": "No commit selected",
  "commitDetail.reading": "Reading the commit…",
  "commitDetail.committedBy": "committed by",
  "commitDetail.parent": "parent",
  "commitDetail.parents": "parents",
  "commitDetail.root": "none — root commit",
  "commitDetail.noFiles": "This commit changes no file",

  // ── The shelf ───────────────────────────────────────────────────────────
  "stash.title": "Stashes",
  "stash.push": "Stash…",
  "stash.message": "What it holds, in a few words",
  "stash.untracked": "Take the untracked files too",
  "stash.cancel": "Cancel",
  "stash.confirm": "Stash",
  "stash.reading": "Reading the stashes…",
  "stash.empty":
    'Nothing stashed. "Stash" puts the working copy aside and leaves the branch clean.',
  "stash.detached": "detached HEAD",
  "stash.plusUntracked": "+ untracked",
  "stash.apply": "Apply",
  "stash.applyTitle": "Put these changes back in the working copy. The stash stays on the shelf.",
  "stash.pop": "Apply and remove",
  "stash.popTitle":
    "Put these changes back in the working copy, then take the stash off the shelf.",
  "stash.drop": "Delete",
  "stash.dropTitle":
    "Throw the stash away without applying it. What it holds will only be reachable through the reflog.",
  "stash.one": "Stash",
  "stash.none": "No stash selected",
  "stash.from": "stashed from",
  "stash.holdsUntracked": "holds untracked files, taken with the stash",
  "stash.files": "Files",
  "stash.readingOne": "Reading the stash…",
  "stash.noFiles": "This stash changes no file",

  // ── The conflict dialog ─────────────────────────────────────────────────
  "conflict.title": "Settle a conflict",
  "conflict.of": "{at} of {total}",
  "conflict.next": "next conflict",
  "conflict.readingFile": "Reading the file…",
  "conflict.gone":
    'This file holds no conflict markers any more — it was settled elsewhere. "Mark resolved" is what closes it.',
  "conflict.ours": "Ours",
  "conflict.theirs": "Theirs",
  "conflict.both": "Both",
  "conflict.bothTitle": "Keep both sides, Ours then Theirs",
  "conflict.atLine": "line {line}",
  "conflict.base": "Base — the common ancestor",
  "conflict.openEditor": "Open in the editor",
  "conflict.cancel": "Cancel",
  "conflict.resolve": "Mark resolved and stage",
  "conflict.unanswered": "Every conflict is waiting for an answer",

  // ── The repositories screen ─────────────────────────────────────────────
  "library.recents": "Recent",
  "library.filter": "Filter the repositories",
  "library.noMatch": "No repository matches",
  "library.noMatchText": '{n} in the list, none holding "{needle}".',
  "library.repositories.one": "{n} repository",
  "library.repositories.other": "{n} repositories",
  "library.clearFilter": "Clear the filter",
  "library.empty": "No repository yet",
  "library.emptyText":
    "Add a folder that is already versioned, or drop one on the window. It stays on the disk: this list keeps only its path.",
  "library.forget": "Remove",
  "library.forgetTitle": "Remove from the list — the repository stays on the disk",
  "library.picker": "Add a repository",
  "library.missing": "not on the disk",
  "library.clean": "clean",
  "library.gone": "gone",
  "library.upToDate": "up to date",
  "library.changed.one": "{n} changed",
  "library.changed.other": "{n} changed",
  "library.untracked.one": "{n} untracked",
  "library.untracked.other": "{n} untracked",
  "library.conflicts.one": "{n} conflict",
  "library.conflicts.other": "{n} conflicts",

  // ── The repository card ─────────────────────────────────────────────────
  "card.none": "No repository selected",
  "card.forget": "Remove from the list",
  "card.open": "Open",
  "card.openHint": "double-click to open",
  "card.missing":
    "The folder is no longer where it was recorded. The entry stays: an unmounted disk comes back, and a list that tidied itself would lose what you arranged.",
  "card.repository": "Repository",
  "card.location": "Location",
  "card.lastOpened": "Last opened",
  "card.never": "never",
  "card.lastCommit": "Last commit",
  "card.description": "Description",
  "card.identity": "Committer identity",
  "card.inherited": "inherited from the global configuration",
  "card.workingCopy": "Working copy",
  "card.branch": "Current branch",
  "card.gone": "gone",
  "card.status": "Status",
  "card.stashes": "Stashes",
  "card.remotes": "Remotes",
  "card.noRemoteUrl": "no URL configured",
  "card.days": "90 days",
  "card.modified.one": "{n} modified",
  "card.modified.other": "{n} modified",
  "card.added.one": "{n} added",
  "card.added.other": "{n} added",
  "card.deleted.one": "{n} deleted",
  "card.deleted.other": "{n} deleted",
  "card.renamed.one": "{n} renamed",
  "card.renamed.other": "{n} renamed",

  // ── The clone dialog ────────────────────────────────────────────────────
  "clone.title": "Clone a repository",
  "clone.url": "URL",
  "clone.destination": "Destination",
  "clone.browse": "Browse…",
  "clone.group": "Group",
  "clone.default": "Default",
  "clone.shallow": "Shallow clone",
  "clone.submodules": "Include the submodules",
  "clone.probing": "… reaching the remote",
  "clone.reachable": "✓ host reachable · access granted",
  "clone.nothing": "nothing to clone",
  "clone.escape": "Esc to cancel",
  "clone.cancel": "Cancel",
  "clone.confirm": "Clone",

  // ── The palette and the sheet ───────────────────────────────────────────
  "bind.movement": "{key} is how you move around the window",
  "bind.taken": 'already taken by "{action}"',
  "window.minimize": "Minimise",
  "window.maximize": "Maximise",
  "window.close": "Close",
  "history.commits.one": "{n} commit",
  "history.commits.other": "{n} commits",
  "history.loaded": "{n} commits loaded",
  "palette.actions": "Actions",
  "palette.repositories": "Repositories",
  "palette.branches": "Branches",
  "palette.files": "Files",
  "palette.run": "run ⏎",
  "palette.open": "open ⏎",
  "palette.switch": "switch ⏎",
  "palette.placeholder": "An action, a repository, a branch, a file…",
  "palette.nothing": "Nothing by that name here.",
  "palette.close": "Esc",
  "sheet.title": "Shortcuts",
  "sheet.commands": "Commands",
  "sheet.movement": "Movement",
  "sheet.bare":
    "None of these fires while the caret is in a field: that is what makes a single letter usable as a shortcut.",

  // ── The questions (SPEC §3 rule 7) ──────────────────────────────────────
  //
  // Every one of these is asked before something that cannot be undone, so the
  // detail says what is lost and where it can still be found. The verb on the
  // button is the act, never "OK".
  "ask.forget.title": "Remove {name} from the list?",
  "ask.forget.detail":
    "The repository stays on the disk: only its entry in this list goes, and it can be added again.",
  "ask.forget.verb": "Remove",
  "ask.reconcile.title": "Merge or rebase?",
  "ask.reconcile.detail":
    "{branch} has {ahead} that {upstream} does not, and {upstream} has {behind} of its own. Nothing in the configuration says how to reconcile them, and git refuses to choose. Merging keeps both histories and adds a merge commit; rebasing replays your commits on top of theirs, which rewrites them. This choice holds for this pull only: nothing is stored.",
  "ask.reconcile.verb": "Merge",
  "ask.reconcile.alternative": "Rebase",
  "ask.force.title": "Force-publish {branch}?",
  "ask.force.detail":
    "With --force-with-lease: refused if the remote has moved since we last saw it. What is replaced is on no clone any more.",
  "ask.force.verb": "Force",
  "ask.merge.title": "Merge {branch} into {into}?",
  "ask.merge.detail":
    "The files in the working copy will change. If both branches touched the same lines, the merge will stop on a conflict and the repository will stay half-way — the way out is in the bottom bar.",
  "ask.merge.verb": "Merge",
  "ask.rebase.title": "Rebase {branch} onto {onto}?",
  "ask.rebase.detail":
    "The branch's commits are rewritten: the ones they replace will only be reachable through the reflog. A published branch that depends on them will diverge.",
  "ask.rebase.verb": "Rebase",
  "ask.abort.title": "Abort {operation}?",
  "ask.abort.detail":
    "The repository goes back to where the operation found it. What has been settled so far is lost: nobody else has it.",
  "ask.abort.verb": "Abort",
  "ask.deleteBranch.title": "Delete branch {name}?",
  "ask.deleteBranch.merged":
    "All its commits are already on the current branch. Only the label goes.",
  "ask.deleteBranch.unmerged":
    "Its commits are on no other branch: after this they will only be reachable through the reflog.",
  "ask.deleteBranch.verb": "Delete",
  "ask.dropStash.title": "Delete {address}?",
  "ask.dropStash.detail":
    '"{message}" will be thrown away without being applied. What it holds is in no commit, and will only be reachable through the reflog.',
  "ask.dropStash.verb": "Delete",
  "ask.discardFile.title": "Discard the changes to {file}?",
  "ask.restoreFile.title": "Restore {file}?",
  "ask.discardFile.untracked":
    "This file is not tracked: discarding it deletes it from the disk. Nothing will hold it back.",
  "ask.discardFile.deleted":
    "This file was deleted from the working copy. Restoring it puts it back as it is in the last commit — nothing is lost.",
  "ask.discardFile.modified":
    "The unstaged changes to this file will be lost. They are in no commit and in no reflog.",
  "ask.discardFile.verb": "Discard",
  "ask.restoreFile.verb": "Restore",
  "ask.discardHunk.title": "Discard hunk {n} of {file}?",
  "ask.discardLines.title": "Discard the picked lines of {file}?",
  "ask.discardLines.detail": "These lines will be taken out of the file. They are in no commit.",
  "ask.amend.title": "Replace the previous commit?",
  "ask.amend.detail":
    "The current commit will be replaced. It will stay reachable through the reflog only, and will disappear from a branch that was already pushed.",
  "ask.amend.verb": "Replace",
  "ask.thisBranch": "this branch",
  "ask.commits.one": "{n} commit",
  "ask.commits.other": "{n} commits",

  // ── What a write is called while it runs, and what it says after ────────
  "do.unstageFile": "Unstage {file}",
  "do.stageFile": "Stage {file}",
  "do.unstageAll": "Unstage everything",
  "do.stageAll": "Stage everything",
  "do.unstageHunk": "Unstage hunk {n}",
  "do.stageHunk": "Stage hunk {n}",
  "do.unstageLines": "Unstage {what}",
  "do.stageLines": "Stage {what}",
  "do.discardFile": "Discard {file}",
  "do.discardHunk": "Discard hunk {n}",
  "do.discardLines": "Discard {what}",
  "do.resolve": "Settle {file}",
  "do.resolveSide": "Settle {file} — {side}",
  "do.createBranch": "Create {name}",
  "do.binding": "Shortcut: {binding}",
  "do.bindingDefault": "Default shortcut",
  "do.theme": "Theme: {name}",
  "do.density": "Density: {density}",
  "do.scale": "Scale: {percent} %",
  "said.done": "{what} finished",
  "said.nothing": "nothing to do",
  "said.cloned": "Cloned into {path}",
  "said.stashed": "Stashed · the working copy is clean again",
  "said.applied": "{stash} applied · it stays on the shelf",
  "said.popped": "{stash} applied and taken off the shelf",
  "said.dropped": "{stash} deleted · what it held is only reachable through the reflog",
  "clone.needUrl": "A URL is needed",
  "clone.needParent": "A destination folder is needed",
  "clone.needName": "A folder name is needed",
  "picker.cloneInto": "Clone into",

  // ── What the actions are called ─────────────────────────────────────────
  //
  // One label per action, read by four things: the palette searches them, the
  // `?` sheet prints them, the macOS menu bar shows them, and the topbar puts
  // them in its tooltips.
  "action.repository.add": "Add a local repository",
  "action.repository.clone": "Clone a repository",
  "action.repository.all": "All repositories",
  "action.screen.workingCopy": "Go to the working copy",
  "action.screen.history": "Go to the history",
  "action.screen.stashes": "Go to the stashes",
  "action.network.fetch": "Fetch",
  "action.network.pull": "Pull",
  "action.network.push": "Push",
  "action.network.stop": "Stop the network operation",
  "action.workingCopy.refresh": "Re-read the repository",
  "action.stash.push": "Stash the changes",
  "action.palette.open": "Command palette",
  "action.help.shortcuts": "Keyboard shortcuts",
  "action.settings.open": "Settings",
  "action.journal.toggle": "Operations journal",

  // ── Moving about ────────────────────────────────────────────────────────
  "move.zones": "The sidebar, the centre column, the detail panel",
  "move.tab": "The next stop or the previous one: a whole list is one",
  "move.updown": "Down and up inside the zone",
  "move.ends": "The first row · the last",
  "move.enter": "What the row is for: open, switch, stage",
  "move.filter": "This screen's filter",
  "move.escape": "Up one level: a filter, then the sidebar",

  // ── The rest of Preferences ─────────────────────────────────────────────
  "settings.title": "Settings",
  "settings.reread": "Re-read",
  "settings.reading": "Reading the settings…",
  "settings.theme": "Theme",
  "settings.onScreen": "On screen right now: {theme}.",
  "settings.themeNote":
    "One choice here: follow something, or name a theme — naming a theme stops all following.",
  "settings.automatic": "Automatic",
  "settings.automaticDetail":
    "the first source that answers: Omarchy, then the system, otherwise the embedded theme",
  "settings.omarchy": "Follow Omarchy",
  "settings.omarchyYes": "the Quattro palette, followed live",
  "settings.omarchyNo": "no Omarchy on this machine",
  "settings.system": "Follow the system",
  "settings.systemYes": "light or dark, switching included",
  "settings.systemNo": "this platform reports no preference",
  "settings.embeddedDark": "Embedded — dark",
  "settings.embeddedDarkDetail": "the default dark theme, following nothing",
  "settings.embeddedLight": "Embedded — light",
  "settings.embeddedLightDetail": "the default light theme, following nothing",
  "settings.namedTheme": "Or a theme by name",
  "settings.light": "light",
  "settings.dark": "dark",
  "settings.density": "Density",
  "settings.densityNote":
    "Board 08: density changes neither the type size nor its weight — it changes line heights, margins and gaps. No target ever goes below 24×24 px, compact included.",
  "settings.comfortable": "Comfortable",
  "settings.comfortableDetail": "what the boards draw",
  "settings.compact": "Compact",
  "settings.compactDetail": "beside a dense terminal",
  "settings.scale": "Scale",
  "settings.scaleNote":
    "The whole interface, proportions kept. The type scale is a contract; how large it is drawn is a property of your screen.",
  "settings.default": "Default",
  "settings.keyboard": "Keyboard",
  "settings.keyboardNote":
    "One table, read by four things: the keyboard, the palette, the shortcut sheet and the menu bar. Changing a binding here changes all four.",
  "settings.seeShortcuts": "See every shortcut",
  "settings.pressIt": "Press…",
  "settings.pressPrompt": "Press the combination you want. Esc cancels.",
  "settings.resetBinding": "Put the original binding back",
  "settings.git": "Git",
  "settings.gitNote":
    "What the app reads from your configuration, and what it does with it. Nothing here is written: `git config` stays the only place these are decided.",
  "settings.binary": "Binary",
  "settings.editor": "Editor",
  "settings.credentials": "Credentials",

  // ── The native menu bar (macOS) ─────────────────────────────────────────
  //
  // The platform's own items, not omagit's — but they are still said in the
  // language the window is in, so they are sent to the backend with the
  // actions. The keys are the ones `menu.rs` looks for.
  "menu.about": "About omagit",
  "menu.services": "Services",
  "menu.hide": "Hide omagit",
  "menu.hideOthers": "Hide others",
  "menu.showAll": "Show all",
  "menu.quit": "Quit omagit",
  "menu.file": "File",
  "menu.closeWindow": "Close window",
  "menu.edit": "Edit",
  "menu.undo": "Undo",
  "menu.redo": "Redo",
  "menu.cut": "Cut",
  "menu.copy": "Copy",
  "menu.paste": "Paste",
  "menu.selectAll": "Select all",
  "menu.view": "View",
  "menu.fullscreen": "Full screen",
  "menu.repository": "Repository",
  "menu.window": "Window",
  "menu.minimize": "Minimise",
  "menu.zoom": "Zoom",
  "menu.help": "Help",

  // ── What the backend refuses, and what it opened ────────────────────────
  //
  // These reach the window as `omagit:<key>|<arg>` — its own words, told apart
  // from Git's, which are shown verbatim. `{0}`, `{1}` are the parts.
  "refuse.networkBusy": "{0} is already running",
  "refuse.noOperation": "no operation is in progress",
  "refuse.noSide": "{0} has no diff on that side",
  "refuse.themeSource": "unknown theme source: {0}",
  "refuse.density": "unknown density: {0}",
  "said.opened": "{0} opened in {1}",
  "said.openedByOpener": "{0} opened with {1} — no editor configured",
  "said.openedNotInTerminal":
    "{0} opened with {1} — {2} lives in a terminal, and this window has none",
  "conflict.theBase": "the base",
  "conflict.cherryPicked": "the cherry-picked commit",
  "conflict.reverted": "the reverted commit",
  "conflict.patched": "the applied patch",
  "settings.editorPlain": "{program}",
  "settings.editorNone": "{program} — no editor configured",
  "settings.editorTerminal": "{program} — {editor} lives in a terminal, and this window has none",

  // ── Dates ───────────────────────────────────────────────────────────────
  "date.today": "today {time}",
  "date.yesterday": "yesterday {time}",

  // ── Preferences ─────────────────────────────────────────────────────────
  "settings.language": "Language",
  "settings.languageNote":
    "The system's language unless you say otherwise. Adding a language is adding one file to `web/src/i18n/languages/`.",
  "settings.languageSystem": "Follow the system",

  // ── The branch tree ─────────────────────────────────────────────────────
  "branches.title": "Branches",
  "branches.tags": "Tags",
  "branches.remotes": "Remotes",
  "branches.empty": "No branch yet: the first commit will make one.",
  "branches.reading": "Reading the branches…",
  "branches.new": "New branch",
  "branches.name": "Branch name",
  "branches.create": "Create",
  "branches.merge": "Merge {branch} into the current branch",
  "branches.mergeShort": "Merge",
  "branches.rebase": "Replay the current branch on top of {branch}",
  "branches.rebaseShort": "Rebase",
  "branches.delete": "Delete this branch",
  "branches.deleteShort": "Delete",
  "branches.merged": "Merged",
  "branches.row": "{branch} — click: its history, double-click: switch to it",
  "branches.remoteRow": "{branch} — click: its history",
  "branches.months.one": "{n} month",
  "branches.months.other": "{n} months",
  "branches.years.one": "{n} year",
  "branches.years.other": "{n} years",
} as const;
