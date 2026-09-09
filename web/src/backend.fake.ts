// A repository that behaves enough like one to catch a state bug.
//
// The point of the fake is that it *changes*: staging a hunk leaves the file on
// both sides of the index, staging the whole file moves it to one side, and
// discarding an untracked file removes it. `state.ts` re-reads after every
// write, so a fake whose answers never move would agree with any bug at all.
//
// It is not a Git model. Anything about how Git really behaves is tested in
// `omagit-git`, against the real binary. This is about what the screen does
// with the answers.
//
// Every answer is typed against `ipc.ts` rather than returned as `unknown`.
// That is not tidiness: an untyped fake drifts from the wire silently, and this
// one did — the summary kept its old flat shape after the Repositories screen
// widened it, and five unrelated tests failed on a null dereference instead of
// one saying the fake was out of date.

export type Fixture = {
  path: string;
  /// The two sides of the index, as the status names them, or `null` for a
  /// side with nothing on it.
  staged: string | null;
  unstaged: string | null;
  /// How many hunks the unstaged diff has, so a partial write has something to
  /// leave behind.
  hunks: number;
  /// The text of the changed line, and the byte ranges inside it that differ.
  /// Set when a test is about the text rather than about the index — the
  /// refinements are Rust's byte offsets, which is the whole subtlety.
  line?: { text: string; refined: [number, number][] };
};

import { isFiltered } from "./ipc";
import type {
  CommitDetail,
  Comparison,
  Diff,
  DiffRow,
  HistoryQuery,
  JournalRow,
  LibraryRow,
  Made as Committed,
  Page,
  PlatformFacts,
  RepoSummary,
  StatusRow,
} from "./ipc";

export type Call = { command: string; args: Record<string, unknown> };

/// One commit in the fake's history. Enough for the list to draw a row and for
/// the walk to be paged, which is what the state machine is about.
export type Made = {
  id: string;
  summary: string;
  parents: string[];
  lane?: number;
};

export class Repository {
  files: Fixture[];
  /// Every invocation, in order — what a test asserts a write did *not* make.
  calls: Call[] = [];
  committer: string | null = "Test <test@omagit.test>";
  template: string | null = null;
  head = "main";
  commits: { message: string; amend: boolean }[] = [];
  /// Set to make the next write fail, the way `git apply` does on a patch
  /// whose context has moved.
  failNextWrite: string | null = null;

  /// The history, newest first. Named for `git log` rather than "commits",
  /// which is already the list of commits the commit box has *made*.
  log: Made[] = [];
  /// What the library holds. One row in one group unless a test says otherwise.
  library: LibraryRow[] = [
    {
      group: 0,
      index: 0,
      group_name: "Récents",
      path: "/repo",
      name: "repo",
      description: "",
      last_opened: null,
      missing: false,
    },
  ];
  /// A folder the picker would hand back that is not a repository.
  notARepository: string | null = null;
  /// The column widths the settings file remembers.
  panes: Record<string, number> = {};
  /// How many rows a page holds. Small in tests, so paging is exercised by
  /// three commits rather than by fifteen hundred.
  page = 3;
  /// What `history_more` continues from. The whole point of the parked walk is
  /// that this is *not* reset by a second call.
  private cursor = 0;
  /// Commits the front end must never see twice, whatever the paging does.
  handedOut: string[] = [];
  /// When set, `history_more` waits on it before answering. That is the only
  /// way to make a page arrive *after* the restart that overtook it, which is
  /// the race the stale-page guard exists for — without it the restart happens
  /// last and hides the bug by replacing what the stale page appended.
  holdHistoryMore: Promise<void> | null = null;
  /// Whether the last `history` call asked for a filter — which is what decides
  /// whether the rows carry a graph, as it does on the real backend.
  private filtering = false;
  /// Holds one repository's summary open, by path.
  ///
  /// By path and not for all of them, because the repository being *opened*
  /// legitimately waits for its own summary — that is what fills the status
  /// bar. What must not wait is the rest of the list.
  holdSummary: { path: string; until: Promise<void> } | null = null;

  constructor(files: Fixture[]) {
    this.files = files;
  }

  find(path: string): Fixture | undefined {
    return this.files.find((file) => file.path === path);
  }

  async call(command: string, args: Record<string, unknown>): Promise<unknown> {
    this.calls.push({ command, args });
    switch (command) {
      case "theme":
        return "--bg: #000; --text: #fff;";
      case "platform":
        return {
          name: "linux",
          modifier: "control",
          modifier_label: "Ctrl",
          reserve: { leading: 0, trailing: 0 },
          credential_helper: "store",
        } satisfies PlatformFacts;
      case "git_status":
        return null;
      case "repositories":
        return this.library.map((row) => ({ ...row })) satisfies LibraryRow[];
      case "add_repository": {
        const path = args["path"] as string;
        if (path === this.notARepository) throw new Error(`${path} n'est pas un dépôt Git`);
        this.library.push({
          group: 0,
          index: this.library.length,
          group_name: "Récents",
          path,
          name: path.split("/").pop() ?? path,
          description: "",
          last_opened: null,
          missing: false,
        });
        return this.summary(path);
      }
      case "forget_repository":
        this.library = this.library.filter((row) => row.path !== args["path"]);
        return undefined;
      case "touch_repository": {
        const row = this.library.find((entry) => entry.path === args["path"]);
        if (row) row.last_opened = 1_767_225_600;
        return undefined;
      }
      case "status":
        return this.files.map((file) => ({
          path: file.path,
          staged: file.staged,
          unstaged: file.unstaged,
          conflict: null,
          code: `${file.staged ? "M" : " "}${file.unstaged ? "M" : " "}`,
        })) satisfies StatusRow[];
      case "summary": {
        const path = args["path"] as string;
        const answer = this.summary(path);
        if (this.holdSummary?.path === path) await this.holdSummary.until;
        return answer;
      }

      case "file_diff":
        return this.diff(args["file"] as string, args["staged"] as boolean);
      case "committer":
        return this.committer;
      case "commit_template":
        return this.template;
      case "head_message":
        return "le message précédent";
      case "journal":
        return [] satisfies JournalRow[];
      case "panes":
        return { ...this.panes };
      case "set_pane":
        this.panes[args["name"] as string] = args["width"] as number;
        return undefined;
      case "history": {
        this.cursor = 0;
        this.handedOut = [];
        const query = (args["query"] ?? {}) as Partial<HistoryQuery>;
        this.filtering = isFiltered({
          all: false,
          firstParent: false,
          author: "",
          text: "",
          path: "",
          since: 0,
          until: 0,
          ...query,
        });
        return this.nextPage();
      }
      case "history_more": {
        if (this.cursor === 0) throw new Error("aucun parcours d'historique en cours");
        const page = this.nextPage();
        if (this.holdHistoryMore) await this.holdHistoryMore;
        return page;
      }
      case "commit_detail":
        return this.detail(args["id"] as string);
      case "commit_file_diff":
        return this.diff("a.txt", false);
      case "compare":
        return {
          from: { full: args["from"] as string, short: (args["from"] as string).slice(0, 7) },
          to: { full: args["to"] as string, short: (args["to"] as string).slice(0, 7) },
          files: [{ path: "a.txt", change: "modified", added: 3, removed: 1, reason: null }],
        } satisfies Comparison;
      case "compare_file_diff":
        return this.diff("a.txt", false);
      case "log":
        return undefined;
      case "stage":
        return this.stage(args);
      case "stage_all":
        return this.stageAll(args["unstage"] as boolean);
      case "discard":
        return this.discard(args);
      case "commit":
        return this.commit(args);
      default:
        throw new Error(`la commande ${command} n'existe pas`);
    }
  }

  private summary(path: string): RepoSummary {
    const row = this.library.find((entry) => entry.path === path);
    if (row?.missing) throw new Error(`${path} est introuvable`);
    return {
      path,
      name: row?.name ?? "repo",
      head: this.head,
      operation: null,
      tracking: null,
      counts: {
        modified: this.files.length,
        added: 0,
        deleted: 0,
        renamed: 0,
        untracked: 0,
        conflicted: 0,
      },
      last_commit: null,
      stashes: 0,
      remotes: [],
      activity: [],
      commits: 0,
      committer: this.committer
        ? { name: "Test", email: "test@omagit.test", initials: "T", inherited: false }
        : null,
    };
  }

  private nextPage(): Page {
    const slice = this.log.slice(this.cursor, this.cursor + this.page);
    this.cursor += slice.length;
    this.handedOut.push(...slice.map((commit) => commit.id));
    return {
      rows: slice.map((commit, at) => ({
        id: { full: commit.id, short: commit.id.slice(0, 7) },
        summary: commit.summary,
        author: "Test Author",
        when: 1_767_225_600 + at,
        merge: commit.parents.length > 1,
        labels: [],
        lane: commit.lane ?? 0,
        passing: [],
        incoming: [],
        outgoing: commit.parents.length > 0 ? [commit.lane ?? 0] : [],
        width: 1,
        graph: !this.filtering,
      })),
      done: this.cursor >= this.log.length,
    };
  }

  private detail(id: string): CommitDetail {
    const commit = this.log.find((made) => made.id === id);
    if (!commit) throw new Error(`${id} n'existe pas`);
    return {
      id: { full: commit.id, short: commit.id.slice(0, 7) },
      parents: commit.parents.map((parent) => ({
        full: parent,
        short: parent.slice(0, 7),
      })),
      author: {
        name: "Test Author",
        email: "author@omagit.test",
        when: 1_767_225_600,
        offset: 0,
      },
      committer: null,
      summary: commit.summary,
      body: "",
      files: [{ path: "a.txt", change: "modified", added: 1, removed: 0, reason: null }],
    };
  }

  private diff(path: string, staged: boolean): Diff | null {
    const file = this.find(path);
    if (!file) return null;
    if (staged ? file.staged === null : file.unstaged === null) return null;

    const hunks = staged ? 1 : file.hunks;
    const rows: DiffRow[] = [];
    for (let hunk = 0; hunk < hunks; hunk += 1) {
      rows.push({ kind: "header", hunk, text: `@@ hunk ${hunk} @@` });
      for (let index = 0; index < 3; index += 1) {
        rows.push({
          kind: "line",
          hunk,
          index,
          side: index === 1 ? "added" : "context",
          old: index,
          new: index,
          text: index === 1 && file.line ? file.line.text : `ligne ${hunk}.${index}`,
          refined: index === 1 && file.line ? file.line.refined : [],
          no_newline: false,
        });
      }
    }
    return {
      path,
      change: "modified",
      rows,
      reason: null,
      added: hunks,
      removed: 0,
      hunks,
    };
  }

  private guard(): void {
    const failure = this.failNextWrite;
    this.failNextWrite = null;
    if (failure) throw new Error(failure);
  }

  private stage(args: Record<string, unknown>): void {
    this.guard();
    const file = this.find(args["file"] as string);
    if (!file) throw new Error("ce fichier n'est plus dans le statut");
    const whole = (args["target"] as { kind: string }).kind === "file";

    if (args["unstage"]) {
      file.unstaged ??= "modified";
      if (whole) file.staged = null;
      return;
    }
    file.staged ??= "modified";
    // A whole file leaves nothing behind; part of one leaves the rest.
    if (whole) file.unstaged = null;
  }

  private stageAll(unstage: boolean): void {
    this.guard();
    for (const file of this.files) {
      if (unstage) {
        file.unstaged ??= "modified";
        file.staged = null;
      } else {
        file.staged ??= "modified";
        file.unstaged = null;
      }
    }
  }

  private discard(args: Record<string, unknown>): void {
    this.guard();
    const path = args["file"] as string;
    const file = this.find(path);
    if (!file) throw new Error("ce fichier n'est plus dans le statut");
    const whole = (args["target"] as { kind: string }).kind === "file";

    if (whole) {
      file.unstaged = null;
      // Nothing on either side means the file has left the working copy.
      if (file.staged === null) this.files = this.files.filter((other) => other !== file);
    }
  }

  private commit(args: Record<string, unknown>): Committed {
    this.guard();
    if (!this.committer) throw new Error("aucune identité");
    this.commits.push({
      message: args["message"] as string,
      amend: args["amend"] as boolean,
    });
    for (const file of this.files) file.staged = null;
    this.files = this.files.filter((file) => file.unstaged !== null);
    return { id: { full: "a".repeat(40), short: "aaaaaaa" }, notes: "" };
  }
}
