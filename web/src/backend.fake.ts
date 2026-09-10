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
  /// Unmerged, as `git status` names the kind: `both modified`, `deleted by
  /// us`. What makes the row a conflict rather than a change.
  conflict?: string;
  /// How many conflicts the file's markers hold, for the dialog. One unless a
  /// test is about answering several.
  regions?: number;
};

import { isFiltered } from "./ipc";
import type {
  BranchRow,
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
  FileRow,
  Choice,
  Conflicted,
  Refs,
  RemoteBranchRow,
  Sides,
  RepoSummary,
  StashRow,
  StatusRow,
  TagRow,
  Tracking,
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
  /// Which of `Head`'s three shapes the window is told about. A test that is
  /// about SPEC §13's edge cases — a repository with no commit, a detached
  /// checkout — sets this rather than dressing the label to look like one.
  headKind: "branch" | "detached" | "unborn" = "branch";
  commits: { message: string; amend: boolean }[] = [];
  /// Set to make the next write fail, the way `git apply` does on a patch
  /// whose context has moved.
  failNextWrite: string | null = null;
  /// Which platform the window thinks it is on. Linux unless a test is about
  /// what only macOS has — the native menu bar (SPEC §9).
  os: "linux" | "macos" = "linux";
  /// The bindings the user has changed (SPEC §11). Empty unless a test is about
  /// a window that starts with a reassignment already stored.
  keymap: Record<string, string> = {};

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
  /// The branches, in the shape the sidebar reads them.
  branches: BranchRow[] = [
    {
      name: "main",
      commit: { full: "m".repeat(40), short: "mmmmmmm" },
      head: true,
      tracking: null,
      merged: true,
      age: 0,
    },
  ];
  tags: TagRow[] = [];
  remoteBranches: RemoteBranchRow[] = [];
  /// Held open so a test can watch the overlay while an operation runs.
  holdNetwork: Promise<void> | null = null;
  /// The same, for a write: held open so a test can see the window while one is
  /// in flight — which is when the filesystem watcher must stay out of the way.
  holdWrite: Promise<void> | null = null;
  /// What `open_in_editor` says it launched. Not always what is configured —
  /// a terminal editor is handed to the desktop's opener instead.
  editorSays = "shared.txt ouvert dans code";
  /// Set to make a conflicted file unreadable as one: markers that do not pair
  /// up, or a binary file.
  unreadableConflict: string | null = null;
  /// Set to make the next network call fail, the way an unreachable host does.
  failNetwork: string | null = null;
  /// What `git` said. Real operations answer with their stderr.
  networkSays = "Everything up-to-date";
  /// Whether anything configures how a diverged branch reconciles. False is
  /// the fresh-repository case, where `git pull` refuses to guess.
  reconcileConfigured = true;

  /// What the Preferences screen reads. A machine with no Omarchy, which is
  /// the case the screen has to say out loud rather than hide.
  preferences: import("./ipc").Preferences = {
    source: "automatic",
    theme: "",
    mode: "dark",
    density: "comfortable",
    scale: 1.15,
    resolved: "Tokyo Night",
    omarchy: false,
    system_appearance: true,
    catalogue: [
      { name: "Tokyo Night", mode: "dark" },
      { name: "Rosé Pine Dawn", mode: "light" },
    ],
    editor: "open — aucun éditeur configuré",
    credential_helper: "osxkeychain",
    git: "2.50.1",
  };

  /// What `check_remote` says when the dialog asks. `null` means it answers.
  unreachable: string | null = null;
  cancelled = 0;
  /// What the current branch tracks, which is what decides where a push goes
  /// and whether it has to set an upstream.
  tracking: Tracking | null = null;
  /// A half-finished merge or rebase, the way `Repository::operation` reports
  /// one.
  operation: string | null = null;
  /// What the two sides of a conflict are called while one is running.
  sides: Sides = { ours: "main", theirs: "feature", replayed: false };
  /// Set to make the next merge or rebase stop on a conflict.
  failIntegrate: string | null = null;
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
  /// The shelf, newest first — and what each entry took off the working copy,
  /// so that applying one puts the files back. The fake has to *change*: a
  /// shelf whose answers never moved would agree with any bug about which
  /// entry a command was aimed at.
  stashes: { row: StashRow; held: Fixture[] }[] = [];
  /// Numbers the fake's stash commits, so two entries never share an id.
  private stashed = 0;

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
      case "preferences":
        return { ...this.preferences };
      case "set_theme": {
        const source = args["source"] as string;
        this.preferences = {
          ...this.preferences,
          source: source.startsWith("embedded") ? "embedded" : source,
          mode: source === "embedded-light" ? "light" : "dark",
          theme: (args["name"] as string) ?? "",
          resolved: (args["name"] as string) || this.preferences.resolved,
        } as typeof this.preferences;
        return "--bg: #111; --text: #eee;";
      }
      case "set_density":
        this.preferences = {
          ...this.preferences,
          density: args["density"] as "compact" | "comfortable",
        };
        return "--bg: #000; --text: #fff; --row-height: 26px;";
      case "set_scale":
        this.preferences = { ...this.preferences, scale: args["scale"] as number };
        return "--bg: #000; --text: #fff;";
      case "platform": {
        const mac = this.os === "macos";
        return {
          name: this.os,
          modifier: mac ? "command" : "control",
          modifier_label: mac ? "⌘" : "Ctrl",
          reserve: { leading: mac ? 78 : 0, trailing: 0 },
          caption: mac
            ? { minimize: false, maximize: false, close: false, side: "trailing" }
            : { minimize: false, maximize: false, close: true, side: "trailing" },
          credential_helper: mac ? "osxkeychain" : "store",
          home: mac ? "/Users/dev" : "/home/dev",
        } satisfies PlatformFacts;
      }
      case "set_menu":
        return undefined;
      case "close_repository":
        return undefined;
      case "keymap":
        return { ...this.keymap };
      case "set_binding": {
        const id = args["id"] as string;
        const binding = args["binding"] as string | null;
        if (binding) this.keymap[id] = binding;
        else delete this.keymap[id];
        return undefined;
      }
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
          conflict: file.conflict ?? null,
          code: file.conflict ? "UU" : `${file.staged ? "M" : " "}${file.unstaged ? "M" : " "}`,
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
      case "pull_reconcile_configured":
        return this.reconcileConfigured;
      case "fetch":
      case "pull":
      case "push": {
        if (this.holdNetwork) await this.holdNetwork;
        const failure = this.failNetwork;
        this.failNetwork = null;
        if (failure) throw new Error(failure);
        return this.networkSays;
      }
      case "clone_directory": {
        // The backend's rule, kept in step by `network.rs`'s own tests. Here it
        // only has to be *a* rule, so the dialog has a name to show.
        const last = (args["url"] as string)
          .trim()
          .replace(/\/+$/, "")
          .split(/[/:]/)
          .filter(Boolean)
          .pop();
        return last ? last.replace(/\.git$/, "") : null;
      }
      case "check_remote": {
        if (this.unreachable) throw new Error(this.unreachable);
        return undefined;
      }
      case "clone_repository": {
        const request = args["request"] as {
          url: string;
          parent: string;
          name: string;
          group: number | null;
        };
        if (this.holdNetwork) await this.holdNetwork;
        const failure = this.failNetwork;
        this.failNetwork = null;
        if (failure) throw new Error(failure);
        const path = `${request.parent}/${request.name}`;
        this.library.push({
          group: request.group ?? 0,
          index: this.library.length,
          group_name: "Récents",
          path,
          name: request.name,
          description: "",
          last_opened: null,
          missing: false,
        });
        return this.summary(path);
      }
      case "merge":
      case "rebase":
        if (this.failIntegrate) {
          const failure = this.failIntegrate;
          this.failIntegrate = null;
          this.operation = command === "merge" ? "merge" : "rebase";
          throw new Error(failure);
        }
        return "Merge made by the 'ort' strategy.";
      case "conflict_file": {
        if (this.unreadableConflict) throw new Error(this.unreadableConflict);
        const file = this.find(args["file"] as string);
        if (!file) throw new Error(`${args["file"]} n'est plus dans le statut`);
        return this.markers(file);
      }
      case "resolve_hunks": {
        this.guard();
        const file = this.find(args["file"] as string);
        if (!file) throw new Error(`${args["file"]} n'est plus dans le statut`);
        const choices = args["choices"] as Choice[];
        const regions = file.conflict ? (file.regions ?? 1) : 0;
        // The real one refuses answers that no longer match the file.
        if (choices.length !== regions) {
          throw new Error(`the file now holds ${regions} conflicts, not the ${choices.length}`);
        }
        delete file.conflict;
        file.staged = "modified";
        file.unstaged = null;
        return undefined;
      }
      case "open_in_editor":
        return this.editorSays;
      case "conflict_sides":
        // Only ever asked while something is running, which is what the real
        // one answers too.
        return this.operation ? this.sides : null;
      case "resolve_conflict": {
        this.guard();
        const file = this.find(args["file"] as string);
        if (!file?.conflict) throw new Error(`un conflit sur ${args["file"]}`);
        // Resolved is staged: `git checkout --ours` leaves the path unmerged,
        // and the real one always follows it with `git add`.
        delete file.conflict;
        file.staged = "modified";
        file.unstaged = null;
        return undefined;
      }
      case "continue_operation": {
        this.guard();
        if (!this.operation) throw new Error("aucune opération n'est en cours");
        if (this.files.some((file) => file.conflict)) {
          throw new Error("error: you need to resolve your current index first");
        }
        this.operation = null;
        return "Merge made by the 'ort' strategy.";
      }
      case "abort_operation":
        if (!this.operation) throw new Error("aucune opération n'est en cours");
        this.operation = null;
        return undefined;
      case "cancel_operation":
        this.cancelled += 1;
        return "Fetch";
      case "refs":
        return {
          branches: this.branches.map((row) => ({ ...row })),
          remote_branches: this.remoteBranches.map((row) => ({ ...row })),
          tags: this.tags.map((row) => ({ ...row })),
          remotes: [],
        } satisfies Refs;
      case "checkout": {
        const name = args["name"] as string;
        if (!this.branches.some((row) => row.name === name)) {
          throw new Error(`la branche ${name} n'existe pas`);
        }
        for (const row of this.branches) row.head = row.name === name;
        this.head = name;
        return undefined;
      }
      case "create_branch": {
        const name = args["name"] as string;
        this.branches.push({
          name,
          commit: { full: "n".repeat(40), short: "nnnnnnn" },
          head: false,
          tracking: null,
          merged: true,
          age: 0,
        });
        if (args["switch"]) {
          for (const row of this.branches) row.head = row.name === name;
          this.head = name;
        }
        return undefined;
      }
      case "delete_branch": {
        const name = args["name"] as string;
        const row = this.branches.find((entry) => entry.name === name);
        if (!row) throw new Error(`la branche ${name} n'existe pas`);
        if (!row.merged && !args["force"]) {
          throw new Error(`la branche ${name} n'est pas entièrement fusionnée`);
        }
        this.branches = this.branches.filter((entry) => entry.name !== name);
        return undefined;
      }
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
          branch: "",
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
      case "stashes":
        // The index is a position in the list, recomputed on every read — the
        // way the reflog numbers them, and the reason no write may be given one.
        return this.stashes.map((entry, index) => ({
          ...entry.row,
          index,
        })) satisfies StashRow[];
      case "stash_files":
        return this.shelf(args["id"] as string).held.map((file) => ({
          path: file.path,
          change: "modified",
          added: 1,
          removed: 0,
          reason: null,
        })) satisfies FileRow[];
      case "stash_file_diff": {
        const held = this.shelf(args["id"] as string).held.find(
          (file) => file.path === args["file"],
        );
        return held ? this.rowsFor(held, false) : null;
      }
      case "stash_push": {
        this.guard();
        if (this.files.length === 0) return "No local changes to save";
        this.stashed += 1;
        const id = `s${this.stashed}`.padEnd(40, "0");
        const message = ((args["message"] as string) ?? "").trim();
        this.stashes.unshift({
          row: {
            index: 0,
            id: { full: id, short: id.slice(0, 7) },
            branch: this.head,
            message: message === "" ? `WIP on ${this.head}: seed` : message,
            when: 1_767_225_600,
            untracked: args["untracked"] as boolean,
          },
          held: this.files,
        });
        this.files = [];
        return `Saved working directory and index state On ${this.head}`;
      }
      case "stash_restore": {
        this.guard();
        const at = this.at(args["id"] as string);
        const entry = this.stashes[at]!;
        this.files = [
          ...this.files,
          ...entry.held.filter((file) => this.find(file.path) === undefined),
        ];
        // `keep` applies and leaves it; otherwise it is popped off.
        if (!(args["keep"] as boolean)) this.stashes.splice(at, 1);
        return "Already up to date.";
      }
      case "stash_drop": {
        this.guard();
        const at = this.at(args["id"] as string);
        const [gone] = this.stashes.splice(at, 1);
        return `Dropped stash@{${at}} (${gone!.row.id.full})`;
      }
      case "log":
        return undefined;
      case "stage":
        if (this.holdWrite) await this.holdWrite;
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
      head_kind: this.headKind,
      operation: this.operation,
      tracking: this.tracking,
      counts: {
        modified: this.files.filter((file) => !file.conflict).length,
        added: 0,
        deleted: 0,
        renamed: 0,
        untracked: 0,
        conflicted: this.files.filter((file) => file.conflict).length,
      },
      last_commit: null,
      stashes: this.stashes.length,
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

  /// A conflicted file's markers: one agreed line, then one conflict per
  /// region. Enough for the dialog to count them and answer them in order.
  private markers(file: Fixture): Conflicted {
    const regions = file.conflict ? (file.regions ?? 1) : 0;
    const segments: Conflicted["segments"] = [{ kind: "agreed", start: 1, lines: ["contexte"] }];
    for (let index = 0; index < regions; index += 1) {
      segments.push({
        kind: "conflict",
        index,
        start: 2 + index * 4,
        ours_label: "HEAD",
        theirs_label: "feature",
        ours: [`ours ${index}`],
        theirs: [`theirs ${index}`],
        base: null,
      });
    }
    return { segments, regions };
  }

  /// One entry of the shelf, by the commit it is addressed by.
  ///
  /// Refused by name when it is gone, the way the backend does: an id that no
  /// longer resolves is a list the front end has not re-read yet.
  private shelf(id: string): { row: StashRow; held: Fixture[] } {
    return this.stashes[this.at(id)]!;
  }

  private at(id: string): number {
    const at = this.stashes.findIndex((entry) => entry.row.id.full === id);
    if (at < 0) throw new Error(`the stash ${id} not found in this repository`);
    return at;
  }

  private diff(path: string, staged: boolean): Diff | null {
    const file = this.find(path);
    if (!file) return null;
    if (staged ? file.staged === null : file.unstaged === null) return null;
    return this.rowsFor(file, staged);
  }

  /// The rows of one fixture's diff. Split out from `diff` because a stashed
  /// file has left the working copy and is no longer in `files`, but its
  /// preview is drawn by the same viewer.
  private rowsFor(file: Fixture, staged: boolean): Diff {
    const path = file.path;
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
