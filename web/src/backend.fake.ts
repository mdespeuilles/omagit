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
        };
      case "git_status":
        return null;
      case "repositories":
        return [{ group: 0, index: 0, path: "/repo", name: "repo" }];
      case "status":
        return this.files.map((file) => ({
          path: file.path,
          staged: file.staged,
          unstaged: file.unstaged,
          conflict: null,
          code: `${file.staged ? "M" : " "}${file.unstaged ? "M" : " "}`,
        }));
      case "summary":
        return {
          path: "/repo",
          name: "repo",
          head: this.head,
          modified: this.files.length,
          untracked: 0,
          conflicted: 0,
          stashes: 0,
          committer: this.committer,
        };
      case "file_diff":
        return this.diff(args["file"] as string, args["staged"] as boolean);
      case "committer":
        return this.committer;
      case "commit_template":
        return this.template;
      case "head_message":
        return "le message précédent";
      case "journal":
        return [];
      case "history":
        this.cursor = 0;
        this.handedOut = [];
        return this.nextPage();
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

  private nextPage(): unknown {
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
      })),
      done: this.cursor >= this.log.length,
    };
  }

  private detail(id: string): unknown {
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

  private diff(path: string, staged: boolean): unknown {
    const file = this.find(path);
    if (!file) return null;
    if (staged ? file.staged === null : file.unstaged === null) return null;

    const hunks = staged ? 1 : file.hunks;
    const rows: unknown[] = [];
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

  private commit(args: Record<string, unknown>): unknown {
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
