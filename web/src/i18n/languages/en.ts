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
