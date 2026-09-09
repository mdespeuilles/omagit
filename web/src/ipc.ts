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

export type RepoSummary = {
  path: string;
  name: string;
  head: string;
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

export type PlatformFacts = {
  name: string;
  modifier: "control" | "command";
  modifier_label: string;
  reserve: { leading: number; trailing: number };
  credential_helper: string;
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

export const api = {
  platform: () => invoke<PlatformFacts>("platform"),
  theme: () => invoke<string>("theme"),
  gitStatus: () => invoke<string | null>("git_status"),
  repositories: () => invoke<LibraryRow[]>("repositories"),
  addRepository: (path: string) => invoke<RepoSummary>("add_repository", { path }),
  forgetRepository: (path: string) => invoke<void>("forget_repository", { path }),
  touchRepository: (path: string) => invoke<void>("touch_repository", { path }),
  summary: (path: string) => invoke<RepoSummary>("summary", { path }),
  status: (path: string) => invoke<StatusRow[]>("status", { path }),
  fileDiff: (path: string, file: string, staged: boolean) =>
    invoke<Diff | null>("file_diff", { path, file, staged }),
  journal: () => invoke<JournalRow[]>("journal"),

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
