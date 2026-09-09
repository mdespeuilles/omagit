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

export type LibraryRow = { group: number; index: number; path: string; name: string };

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
  log: (level: "info" | "warn" | "error", message: string) =>
    invoke<void>("log", { level, message }),
};
