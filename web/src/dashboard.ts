// What the Repositories screen puts first (DESIGN v2, board "Dépôts").
//
// Two hundred repositories is too many to read row by row for the three that
// need something. So the library is sorted into what needs a hand — a merge
// half done, a branch that diverged, work not pushed — and the rest, which is
// only counted. And the counts are only as fresh as each repository's last
// fetch, so the ones nobody fetched lately are named too.

import type { LibraryRow, RepoSummary } from "./ipc";

/// Why a repository is on the "to deal with" list, most urgent first.
export type Need =
  "operation" | "conflicts" | "diverged" | "behind" | "ahead" | "local" | "changes";

const ORDER: Need[] = ["operation", "conflicts", "diverged", "behind", "ahead", "local", "changes"];

/// How old a fetch may be before the arrows are said to be out of date.
export const STALE_AFTER = 24 * 60 * 60;

export type Triaged = { row: LibraryRow; summary: RepoSummary; need: Need };

/// The one reason a repository needs attention, or `null` for one that does
/// not. One reason, the most urgent: a list that says four things per row is
/// back to being read row by row.
export function needOf(summary: RepoSummary): Need | null {
  if (summary.operation) return "operation";
  if (summary.counts.conflicted > 0) return "conflicts";
  const tracking = summary.tracking;
  if (tracking && !tracking.gone) {
    if (tracking.ahead > 0 && tracking.behind > 0) return "diverged";
    if (tracking.behind > 0) return "behind";
    if (tracking.ahead > 0) return "ahead";
  } else if (summary.head_kind === "branch" && summary.remotes.length > 0) {
    return "local";
  }
  const { modified, added, deleted, renamed, untracked } = summary.counts;
  if (modified + added + deleted + renamed + untracked > 0) return "changes";
  return null;
}

/// Whether a repository's arrows may be out of date. Never for one with no
/// remote: there is nothing to compare it with, so nothing to go stale.
export function isStale(summary: RepoSummary, now: number): boolean {
  if (summary.remotes.length === 0) return false;
  return summary.fetched_at === null || now - summary.fetched_at > STALE_AFTER;
}

/// The library, split. Rows whose summary has not arrived yet are in neither
/// list: counting them as clean would be a guess.
export function triage(
  rows: readonly LibraryRow[],
  // Loose on purpose: the window's copy is deeply readonly, and nothing here
  // writes to it.
  summaries: Readonly<Record<string, { readonly status: string; readonly value?: unknown }>>,
  now: number,
): { needs: Triaged[]; clean: Triaged[]; stale: Triaged[] } {
  const needs: Triaged[] = [];
  const clean: Triaged[] = [];
  const stale: Triaged[] = [];
  for (const row of rows) {
    const held = summaries[row.path];
    if (held?.status !== "ready" || !held.value) continue;
    const summary = held.value as RepoSummary;
    const need = needOf(summary);
    const entry = { row, summary, need: need ?? ("changes" as Need) };
    (need ? needs : clean).push(entry);
    if (isStale(summary, now)) stale.push(entry);
  }
  needs.sort(
    (a, b) =>
      ORDER.indexOf(a.need) - ORDER.indexOf(b.need) ||
      (b.row.last_opened ?? 0) - (a.row.last_opened ?? 0),
  );
  return { needs, clean, stale };
}
