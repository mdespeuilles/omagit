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

export type RepoSummary = {
  path: string;
  name: string;
  head: string;
  modified: number;
  untracked: number;
  conflicted: number;
  stashes: number;
  committer: string | null;
};

export type LibraryRow = {
  group: number;
  index: number;
  path: string;
  name: string;
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
  summary: (path: string) => invoke<RepoSummary>("summary", { path }),
  status: (path: string) => invoke<StatusRow[]>("status", { path }),
  fileDiff: (path: string, file: string, staged: boolean) =>
    invoke<Diff | null>("file_diff", { path, file, staged }),
  journal: () => invoke<JournalRow[]>("journal"),

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
