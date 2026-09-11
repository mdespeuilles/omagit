// The backend's surface, typed once.
//
// Hand-written rather than generated: the shapes are few, they change with the
// commands that produce them, and a mismatch here is a compile error rather
// than a runtime surprise. If this grows past a screenful, generate it.

import { invoke } from "@tauri-apps/api/core";

export type Oid = { full: string; short: string };

export type StatusRow = {
  path: string;
  staged: string | null;
  unstaged: string | null;
  conflict: string | null;
  code: string;
};

export type DiffRow =
  | { kind: "header"; hunk: number; text: string }
  | { kind: "fold"; lines: number }
  | {
      kind: "line";
      hunk: number;
      index: number;
      side: "context" | "added" | "removed";
      old: number | null;
      new: number | null;
      text: string;
      refined: [number, number][];
      no_newline: boolean;
    };

export type Diff = {
  path: string;
  change: string;
  rows: DiffRow[] | null;
  reason: string | null;
  added: number;
  removed: number;
  hunks: number;
};

export type Tracking = { upstream: string; ahead: number; behind: number; gone: boolean };

export type Counts = {
  modified: number;
  added: number;
  deleted: number;
  renamed: number;
  untracked: number;
  conflicted: number;
};

export type LastCommit = { id: Oid; summary: string; author: string; when: number };

export type RemoteRow = { name: string; url: string | null };

export type Identity = {
  name: string;
  email: string;
  initials: string;
  /// From the global or system configuration rather than from this repository.
  inherited: boolean;
};

/// What changed under a repository, as the backend reports it (SPEC §10). The
/// two booleans are invalidations, not events: the window has no business
/// knowing that `.git/index.lock` exists.
export type Changed = { path: string; status: boolean; refs: boolean };

/// What "Open in the editor" will launch — a fact, not a sentence: the window
/// is what words it.
export type Editor = {
  program: string;
  configured: string | null;
  instead: "nothing-configured" | "lives-in-a-terminal" | null;
};

export type RepoSummary = {
  path: string;
  name: string;
  head: string;
  /// Which of the three shapes `head` is: on a branch, on a commit with no
  /// branch to move, or on a branch that does not exist yet.
  head_kind: "branch" | "detached" | "unborn";
  /// A half-finished merge, rebase or cherry-pick. Shown *instead* of the
  /// branch: "on main" is misleading while a merge is stuck.
  operation: string | null;
  tracking: Tracking | null;
  counts: Counts;
  last_commit: LastCommit | null;
  stashes: number;
  remotes: RemoteRow[];
  activity: number[];
  commits: number;
  committer: Identity | null;
};

/// One reference drawn beside a commit. Four kinds because DESIGN §5 draws
/// them four ways, and because "which branch am I on" is the question the
/// screen is most often asked.
export type Label = { name: string; kind: "head" | "branch" | "remote" | "tag" };

export type HistoryRow = {
  id: Oid;
  summary: string;
  author: string;
  /// Seconds since the epoch. The backend has no locale; see `format.ts`.
  when: number;
  merge: boolean;
  labels: Label[];
  lane: number;
  passing: number[];
  incoming: number[];
  outgoing: number[];
  width: number;
  /// Whether the five fields above mean anything. False under a filter, where
  /// the rows are a search result rather than a history.
  graph: boolean;
};

export type Page = { rows: HistoryRow[]; done: boolean };

export type HistoryQuery = {
  all: boolean;
  firstParent: boolean;
  /// One branch's history rather than HEAD's — what clicking a branch in the
  /// sidebar asks for. Empty is HEAD, and `all` wins over it.
  branch: string;
  /// SPEC §11's filters. Empty means "not filtering" — a box that has been
  /// typed into and cleared again must not go on narrowing anything.
  author: string;
  text: string;
  path: string;
  /// Seconds since the epoch, inclusive; zero is unset.
  since: number;
  until: number;
};

/// Whether a query narrows anything, which is also what decides whether the
/// rows carry a graph.
export function isFiltered(query: HistoryQuery): boolean {
  return (
    query.author.trim() !== "" ||
    query.text.trim() !== "" ||
    query.path.trim() !== "" ||
    query.since > 0 ||
    query.until > 0
  );
}

export type Who = { name: string; email: string; when: number; offset: number };

export type FileRow = {
  path: string;
  change: string;
  added: number;
  removed: number;
  reason: string | null;
};

export type CommitDetail = {
  id: Oid;
  parents: Oid[];
  author: Who;
  /// Only when it differs from the author — a rebase, a cherry-pick, a patch
  /// applied by someone else.
  committer: Who | null;
  summary: string;
  body: string;
  files: FileRow[];
};

export type Comparison = { from: Oid; to: Oid; files: FileRow[] };

export type BranchRow = {
  name: string;
  commit: Oid;
  head: boolean;
  tracking: Tracking | null;
  /// Every commit of this branch is on HEAD too, so deleting it removes a
  /// label and nothing else.
  merged: boolean;
  /// Seconds since its tip was authored.
  age: number;
};

export type RemoteBranchRow = { remote: string; name: string; commit: Oid };
export type TagRow = {
  name: string;
  commit: Oid;
  /// What the tagger wrote, or empty. The message, not a flag naming which of
  /// Git's two kinds of tag this is: the kind is Git's own vocabulary and
  /// offers no control — you write a message or you do not.
  message: string;
};

export type Refs = {
  branches: BranchRow[];
  remote_branches: RemoteBranchRow[];
  tags: TagRow[];
  remotes: RemoteRow[];
};

/// What the Preferences screen reads. The theme *source*, never the resolved
/// palette — a stored palette goes stale the moment the system theme changes.
export type Preferences = {
  source: "automatic" | "omarchy" | "system-appearance" | "embedded" | "user-override";
  theme: string;
  mode: "dark" | "light";
  density: "compact" | "comfortable";
  scale: number;
  /// Which theme is on screen now, whatever was asked for.
  resolved: string;
  /// What this machine can offer (SPEC §6.1). A source it cannot is drawn
  /// disabled with the reason, not hidden.
  omarchy: boolean;
  system_appearance: boolean;
  catalogue: { name: string; mode: "dark" | "light" }[];
  editor: Editor;
  credential_helper: string;
  git: string;
};

/// The two versions a conflicted file has, named.
///
/// The names matter more than the words: during a rebase `--ours` is the branch
/// being replayed *onto* and `--theirs` is your own work, so a button labelled
/// with the pronoun alone asks someone to choose between two meanings they
/// cannot see. `replayed` is what tells the interface to explain that.
export type Sides = { ours: string; theirs: string; replayed: boolean };

/// Which version of one conflict inside a file to keep. `both` keeps the two,
/// ours first — board 07's third button.
export type Choice = "ours" | "theirs" | "both";

/// One conflicted file, cut into what the two sides agree on and what they do
/// not.
///
/// Lines, not a diff: `git` has already written both versions into the file
/// between its markers, and resolving is choosing which of them to keep.
export type ConflictSegment =
  | { kind: "agreed"; start: number; lines: string[] }
  | {
      kind: "conflict";
      index: number;
      start: number;
      /// What git wrote after the markers — `HEAD`, a branch, a subject.
      ours_label: string;
      theirs_label: string;
      ours: string[];
      theirs: string[];
      /// Only under `merge.conflictStyle = diff3`: what both sides started
      /// from. Shown, never offered as a choice.
      base: string[] | null;
    };

export type Conflicted = { segments: ConflictSegment[]; regions: number };

/// One entry of the shelf.
///
/// `index` is `git`'s own address — `stash@{0}` is the most recent — and it is
/// here to be *shown*, never to be sent back: dropping one renumbers every
/// entry below it, so every write is addressed by `id` and resolved to an index
/// on the Rust side, under the lock.
export type StashRow = {
  index: number;
  id: Oid;
  /// Null for one made on a detached HEAD, where git writes `(no branch)` —
  /// a sentence rather than somewhere to switch to.
  branch: string | null;
  message: string;
  /// Seconds since the epoch, from the reflog: the time `git stash list` shows.
  when: number;
  /// Holds files that were on no index. They live in a third parent, which is
  /// why its preview is not a commit diff.
  untracked: boolean;
};

/// One line of `git`'s progress, as the overlay draws it.
export type Progress = { what: string; phase: string; percent: number | null };

/// One coding agent omagit knows how to ask for a commit message.
export type AgentCandidate = {
  id: string;
  /// A product name. Not translated.
  label: string;
  program: string;
  /// What it printed for `--version`, or `null` when it did not answer. That is
  /// the whole of "installed": being on the `PATH` is not enough — a broken
  /// wrapper is on it too.
  version: string | null;
};

export type Agents = {
  candidates: AgentCandidate[];
  /// The chosen id or command. Empty means the feature is off.
  command: string;
  guidelines: string;
};

/// One folder of the repository list, in the order the user arranged them.
///
/// No count: the header shows how many rows are *visible*, and the filter is
/// applied on this side.
export type LibraryGroup = {
  /// Data, and renameable. The default group's is an `omagit:` key — run it
  /// through `worded()` before drawing it.
  name: string;
  collapsed: boolean;
};

/// The folders and the rows, from one read: a row names its group by index, so
/// two reads a moment apart could put a row under the wrong folder.
export type LibraryView = {
  groups: LibraryGroup[];
  rows: LibraryRow[];
};

export type LibraryRow = {
  group: number;
  index: number;
  group_name: string;
  path: string;
  name: string;
  description: string;
  last_opened: number | null;
  /// The directory is gone. DESIGN §4: the row stays, struck through, and is
  /// never silently removed — a repository on an unmounted disk comes back.
  missing: boolean;
};

export type JournalRow = {
  command: string;
  destructive: boolean;
  outcome: "running" | "ok" | "failed";
  stderr: string;
};

/// The window buttons the app draws itself, and the edge they sit on.
///
/// All three false on macOS, where the system draws its own over the topbar.
export type Caption = {
  minimize: boolean;
  maximize: boolean;
  close: boolean;
  side: "leading" | "trailing";
};

export type PlatformFacts = {
  name: string;
  modifier: "control" | "command";
  modifier_label: string;
  reserve: { leading: number; trailing: number };
  caption: Caption;
  credential_helper: string;
  /// Empty when the environment does not say.
  home: string;
};

/// What part of a file an operation acts on.
///
/// `file` needs no diff, which is what lets a checkbox work on a row nobody has
/// opened. The other two speak in the coordinates the diff rows carry: a hunk
/// index, and a line's index inside its hunk.
export type Target =
  | { kind: "file" }
  | { kind: "hunks"; hunks: number[] }
  | { kind: "lines"; lines: [number, number][] };

export type Made = { id: Oid; notes: string };

/// Everything the clone dialog collected.
///
/// `parent` is the folder the clone is created *in*: `git clone <url> <dir>`
/// creates `<dir>`, so the destination the dialog shows is these two joined.
export type CloneRequest = {
  url: string;
  parent: string;
  name: string;
  shallow: boolean;
  submodules: boolean;
  /// Which group of the library it joins, or the default one.
  group: number | null;
};

/// One item of the native menu bar (SPEC §9), as `menu.ts` arranges it from the
/// keymap table.
export type MenuEntry = {
  id: string;
  label: string;
  binding: string;
  menu: string;
  enabled: boolean;
};

export const api = {
  platform: () => invoke<PlatformFacts>("platform"),
  setMenu: (entries: MenuEntry[], labels: Record<string, string>) =>
    invoke<void>("set_menu", { entries, labels }),
  theme: () => invoke<string>("theme"),

  // Preferences (M9). Each setter answers with the stylesheet its change
  // renders to, so the window applies it in the same tick it is set: a
  // preference nobody can see the effect of is one nobody trusts.
  preferences: () => invoke<Preferences>("preferences"),
  setTheme: (source: string, name: string) => invoke<string>("set_theme", { source, name }),
  setDensity: (density: string) => invoke<string>("set_density", { density }),
  setScale: (scale: number) => invoke<string>("set_scale", { scale }),
  panes: () => invoke<Record<string, number>>("panes"),
  keymap: () => invoke<Record<string, string>>("keymap"),
  /// `null` while the interface follows the system's language.
  language: () => invoke<string | null>("language"),
  setLanguage: (language: string | null) => invoke<void>("set_language", { language }),

  // The commit-message agent (SPEC §11, amended). No key, no endpoint: what is
  // stored is the name of a program already on the machine, and asking it is
  // running it — the same shape as everything omagit does with `git`.
  //
  // `agents` starts a process per candidate, so it is asked when the block is
  // looked at and not with the rest of the preferences.
  // Tags (SPEC §11). A message is what makes one annotated rather than a bare
  // ref, so there is no separate flag to disagree with it.
  createTag: (path: string, name: string, at: string, message: string, force: boolean) =>
    invoke<void>("create_tag", { path, name, at, message, force }),
  deleteTag: (path: string, name: string) => invoke<void>("delete_tag", { path, name }),
  pushTag: (path: string, remote: string, name: string, remove: boolean) =>
    invoke<string>("push_tag", { path, remote, name, remove }),
  /// Which tags the remote has. Nothing in the repository can answer this: a
  /// tag fetched from a remote sits where a local one does.
  remoteTags: (path: string, remote: string) => invoke<string[]>("remote_tags", { path, remote }),

  agents: () => invoke<Agents>("agents"),
  /// Only which one is chosen, which costs a string rather than three
  /// processes. What start-up asks.
  agentChosen: () => invoke<string>("agent_chosen"),
  setAgent: (command: string | null) => invoke<void>("set_agent", { command }),
  setAgentGuidelines: (guidelines: string) => invoke<void>("set_agent_guidelines", { guidelines }),
  draftMessage: (path: string) => invoke<string>("draft_message", { path }),
  /// `null` puts the action back on the table's own binding.
  setBinding: (id: string, binding: string | null) => invoke<void>("set_binding", { id, binding }),
  setPane: (name: string, width: number) => invoke<void>("set_pane", { name, width }),
  gitStatus: () => invoke<string | null>("git_status"),
  repositories: () => invoke<LibraryView>("repositories"),
  addRepository: (path: string) => invoke<RepoSummary>("add_repository", { path }),
  forgetRepository: (path: string) => invoke<void>("forget_repository", { path }),
  touchRepository: (path: string) => invoke<void>("touch_repository", { path }),

  // The folders (SPEC §11). Every one of them writes `repositories.toml` and
  // answers nothing: the window re-reads the arrangement afterwards, which is
  // one round trip rather than a view returned from six places that could each
  // shape it differently.
  createGroup: (name: string) => invoke<number>("create_group", { name }),
  renameGroup: (group: number, name: string) => invoke<void>("rename_group", { group, name }),
  removeGroup: (group: number) => invoke<void>("remove_group", { group }),
  collapseGroup: (group: number, collapsed: boolean) =>
    invoke<void>("collapse_group", { group, collapsed }),
  moveGroup: (from: number, to: number) => invoke<void>("move_group", { from, to }),
  /// The row's own path, which is the canonicalised one the list was sent.
  moveRepository: (path: string, group: number, index: number) =>
    invoke<void>("move_repository", { path, group, index }),
  closeRepository: (path: string) => invoke<void>("close_repository", { path }),
  summary: (path: string) => invoke<RepoSummary>("summary", { path }),
  status: (path: string) => invoke<StatusRow[]>("status", { path }),
  fileDiff: (path: string, file: string, staged: boolean) =>
    invoke<Diff | null>("file_diff", { path, file, staged }),
  journal: () => invoke<JournalRow[]>("journal"),

  // The network (M7). Each can take minutes and each can be stopped; the
  // progress arrives as `progress` events rather than in the answer, which
  // only comes back at the end.
  fetch: (path: string, remote: string) => invoke<string>("fetch", { path, remote }),
  pull: (path: string, reconcile?: "merge" | "rebase") =>
    invoke<string>("pull", { path, reconcile }),
  /// Whether anything already says how a diverged branch reconciles. Asked
  /// before pulling, because `git`'s refusal can only be answered from a
  /// terminal — and because its wording is translated on some machines.
  pullReconcileConfigured: (path: string) => invoke<boolean>("pull_reconcile_configured", { path }),
  push: (path: string, remote: string, branch: string, force: boolean, setUpstream: boolean) =>
    invoke<string>("push", { path, remote, branch, force, setUpstream }),
  cancelOperation: () => invoke<string | null>("cancel_operation"),

  // Cloning (M7). The one operation with no repository to start from, so it
  // takes a URL and a place to put it rather than a path that already exists.
  cloneDirectory: (url: string) => invoke<string | null>("clone_directory", { url }),
  checkRemote: (url: string) => invoke<void>("check_remote", { url }),
  cloneRepository: (request: CloneRequest) => invoke<RepoSummary>("clone_repository", { request }),

  // Integrating one branch into another (M7). Both can stop half-way on a
  // conflict; `abortOperation` is the way out, and it reads what is running
  // from the repository rather than being told.
  merge: (path: string, branch: string, noFastForward: boolean, squash: boolean) =>
    invoke<string>("merge", { path, branch, noFastForward, squash }),
  rebase: (path: string, onto: string) => invoke<string>("rebase", { path, onto }),
  abortOperation: (path: string) => invoke<void>("abort_operation", { path }),

  // Branches (M7).
  refs: (path: string) => invoke<Refs>("refs", { path }),
  checkout: (path: string, name: string, detach: boolean) =>
    invoke<void>("checkout", { path, name, detach }),
  createBranch: (path: string, name: string, start: string, switch_: boolean) =>
    invoke<void>("create_branch", { path, name, start, switch: switch_ }),
  deleteBranch: (path: string, name: string, force: boolean) =>
    invoke<void>("delete_branch", { path, name, force }),

  // Conflicts (M8). The side is sent by name — the backend reads what *kind*
  // of conflict it is from the status, because half of them have no version on
  // one of the two sides and keeping that side means `git rm`.
  conflictSides: (path: string) => invoke<Sides | null>("conflict_sides", { path }),
  resolveConflict: (path: string, file: string, side: "ours" | "theirs") =>
    invoke<void>("resolve_conflict", { path, file, side }),
  continueOperation: (path: string) => invoke<string>("continue_operation", { path }),
  conflictFile: (path: string, file: string) => invoke<Conflicted>("conflict_file", { path, file }),
  resolveHunks: (path: string, file: string, choices: Choice[]) =>
    invoke<void>("resolve_hunks", { path, file, choices }),
  openInEditor: (path: string, file: string) => invoke<string>("open_in_editor", { path, file }),

  // The shelf (M8). Reads are addressed by index because that is what the row
  // shows; writes by commit, because the numbering moves under them.
  stashes: (path: string) => invoke<StashRow[]>("stashes", { path }),
  stashFiles: (path: string, id: string) => invoke<FileRow[]>("stash_files", { path, id }),
  stashFileDiff: (path: string, id: string, file: string) =>
    invoke<Diff | null>("stash_file_diff", { path, id, file }),
  stashPush: (path: string, message: string, untracked: boolean) =>
    invoke<string>("stash_push", { path, message, untracked }),
  stashRestore: (path: string, id: string, keep: boolean) =>
    invoke<string>("stash_restore", { path, id, keep }),
  stashDrop: (path: string, id: string) => invoke<string>("stash_drop", { path, id }),

  // History. `history` always restarts the walk; `historyMore` continues the
  // one already parked on the backend, and refuses if there is none — a "load
  // more" answered from a fresh walk would hand back rows the list already has.
  history: (path: string, query: HistoryQuery) => invoke<Page>("history", { path, query }),
  historyMore: (path: string) => invoke<Page>("history_more", { path }),
  commitDetail: (path: string, id: string) => invoke<CommitDetail>("commit_detail", { path, id }),
  commitFileDiff: (path: string, id: string, file: string) =>
    invoke<Diff | null>("commit_file_diff", { path, id, file }),
  compare: (path: string, from: string, to: string) =>
    invoke<Comparison>("compare", { path, from, to }),
  compareFileDiff: (path: string, from: string, to: string, file: string) =>
    invoke<Diff | null>("compare_file_diff", { path, from, to, file }),

  // The writes. Each takes the repository's lock on the Rust side for its whole
  // duration, so two of these can never run at once on one repository.
  //
  // Argument names are lowerCamelCase because that is what Tauri looks for: the
  // `#[tauri::command]` macro converts each Rust parameter name to camelCase
  // and reads that key (`ArgumentCase::Camel`, its default). So `noVerify`
  // here reaches `no_verify` there, and sending `no_verify` would not.
  stage: (path: string, file: string, target: Target, unstage: boolean) =>
    invoke<void>("stage", { path, file, target, unstage }),
  stageAll: (path: string, unstage: boolean) => invoke<void>("stage_all", { path, unstage }),
  discard: (path: string, file: string, target: Target) =>
    invoke<void>("discard", { path, file, target }),
  committer: (path: string) => invoke<string | null>("committer", { path }),
  commitTemplate: (path: string) => invoke<string | null>("commit_template", { path }),
  headMessage: (path: string) => invoke<string | null>("head_message", { path }),
  commit: (path: string, message: string, amend: boolean, signOff: boolean, noVerify: boolean) =>
    invoke<Made>("commit", { path, message, amend, signOff, noVerify }),
  log: (level: "info" | "warn" | "error", message: string) =>
    invoke<void>("log", { level, message }),
};
