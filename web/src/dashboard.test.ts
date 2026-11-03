// The dashboard's sort (DESIGN v2): what needs a hand first, one reason per
// repository, and which ones have arrows too old to trust.

import { describe, expect, it } from "vitest";
import { isStale, needOf, triage, STALE_AFTER } from "./dashboard";
import type { LibraryRow, RepoSummary } from "./ipc";

const NOW = 2_000_000_000;

function summary(change: Partial<RepoSummary> = {}): RepoSummary {
  return {
    path: "/r",
    name: "r",
    head: "main",
    head_kind: "branch",
    operation: null,
    tracking: { upstream: "origin/main", ahead: 0, behind: 0, gone: false },
    counts: { modified: 0, added: 0, deleted: 0, renamed: 0, untracked: 0, conflicted: 0 },
    last_commit: null,
    stashes: 0,
    remotes: [{ name: "origin", url: "git@example.com:r.git" }],
    activity: [],
    commits: 1,
    committer: null,
    fetched_at: NOW - 60,
    ...change,
  };
}

const tracking = (ahead: number, behind: number) => ({
  upstream: "origin/main",
  ahead,
  behind,
  gone: false,
});

function row(path: string, last_opened = 0): LibraryRow {
  return {
    group: 0,
    index: 0,
    group_name: "",
    path,
    name: path,
    description: "",
    last_opened,
    missing: false,
  };
}

describe("why a repository needs a hand", () => {
  it("is nothing for one that is clean and up to date", () => {
    expect(needOf(summary())).toBeNull();
  });

  it("names the most urgent reason only", () => {
    const counts = { modified: 2, added: 0, deleted: 0, renamed: 0, untracked: 0, conflicted: 1 };
    expect(needOf(summary({ operation: "merge", counts, tracking: tracking(1, 1) }))).toBe(
      "operation",
    );
    expect(needOf(summary({ counts, tracking: tracking(1, 1) }))).toBe("conflicts");
    expect(needOf(summary({ tracking: tracking(1, 1) }))).toBe("diverged");
    expect(needOf(summary({ tracking: tracking(0, 3) }))).toBe("behind");
    expect(needOf(summary({ tracking: tracking(2, 0) }))).toBe("ahead");
  });

  it("calls a branch with a remote but no upstream unpublished, and one with no remote at all fine", () => {
    expect(needOf(summary({ tracking: null }))).toBe("local");
    expect(needOf(summary({ tracking: null, remotes: [] }))).toBeNull();
  });

  it("counts untracked files as uncommitted work", () => {
    const counts = { modified: 0, added: 0, deleted: 0, renamed: 0, untracked: 1, conflicted: 0 };
    expect(needOf(summary({ counts }))).toBe("changes");
  });
});

describe("arrows too old to trust", () => {
  it("after a day, or never fetched", () => {
    expect(isStale(summary({ fetched_at: NOW - STALE_AFTER + 1 }), NOW)).toBe(false);
    expect(isStale(summary({ fetched_at: NOW - STALE_AFTER - 1 }), NOW)).toBe(true);
    expect(isStale(summary({ fetched_at: null }), NOW)).toBe(true);
  });

  it("never for a repository with no remote: there is nothing to compare with", () => {
    expect(isStale(summary({ fetched_at: null, remotes: [] }), NOW)).toBe(false);
  });
});

describe("the library, split", () => {
  it("puts the urgent first, then the most recently opened, and leaves out what has not been read", () => {
    const rows = [
      row("/clean"),
      row("/ahead", 5),
      row("/merge"),
      row("/ahead-old", 1),
      row("/loading"),
    ];
    const ready = (value: RepoSummary) => ({ status: "ready", value });
    const split = triage(
      rows,
      {
        "/clean": ready(summary()),
        "/ahead": ready(summary({ tracking: tracking(1, 0) })),
        "/ahead-old": ready(summary({ tracking: tracking(1, 0), fetched_at: null })),
        "/merge": ready(summary({ operation: "merge" })),
        "/loading": { status: "loading" },
      },
      NOW,
    );
    expect(split.needs.map((entry) => entry.row.path)).toEqual(["/merge", "/ahead", "/ahead-old"]);
    expect(split.clean.map((entry) => entry.row.path)).toEqual(["/clean"]);
    expect(split.stale.map((entry) => entry.row.path)).toEqual(["/ahead-old"]);
  });
});
