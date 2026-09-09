// The application's state, in one place.
//
// A plain reactive object rather than a store library: there is one window and
// one open repository, and a store would be an abstraction with one user. It
// moves to Pinia the day two parts of the app need to write the same state
// without knowing about each other — which is the same rule the Rust side
// follows about traits with one implementor.
//
// What this exists for: every UI bug this project has shipped was a state bug —
// a trail that disagreed with the screen, a sidebar that belonged to one screen
// and vanished on the next, a control that needed data nobody had asked for.
// State lives here, screens derive from it, and none of them can disagree.

import { reactive, readonly } from "vue";
import {
  api,
  isFiltered,
  type CommitDetail,
  type Comparison,
  type DiffRow,
  type HistoryQuery,
  type HistoryRow,
  type JournalRow,
  type LibraryRow,
  type PlatformFacts,
  type RepoSummary,
  type StatusRow,
} from "./ipc";

/// The four states of SPEC §10, rendered explicitly and never collapsed into a
/// spinner over everything or a blank panel.
export type Async<T> =
  | { status: "idle" }
  | { status: "loading"; previous?: T }
  | { status: "ready"; value: T }
  | { status: "failed"; error: string };

export const idle = <T>(): Async<T> => ({ status: "idle" });

export type Screen = "repositories" | "working-copy" | "history";

type DiffValue = {
  path: string;
  header: string;
  rows: DiffRow[];
  reason: string | null;
};

/// A question the user has to answer before something irreversible happens
/// (SPEC §3 rule 7). The action itself is *not* in here: a closure inside
/// reactive state is a thing that cannot be inspected, compared or logged.
type Question = { title: string; detail: string; verb: string };

type State = {
  platform: PlatformFacts | null;
  /// Why writing is impossible, when it is. Raised before anything is typed
  /// rather than after (SPEC §8).
  gitUnusable: string | null;
  repositories: LibraryRow[];
  screen: Screen;
  /// The repository the window is looking at, by path.
  open: string | null;
  summary: RepoSummary | null;
  status: Async<StatusRow[]>;
  /// Which file the diff panel is about, and on which side of the index.
  selected: { path: string; staged: boolean } | null;
  diff: Async<DiffValue>;
  /// Lines picked inside the open diff, as `hunk:index`. Cleared by every
  /// write: the coordinates index the diff that has just stopped being true.
  picked: Set<string>;

  /// The commit box.
  message: string;
  amend: boolean;
  signOff: boolean;
  noVerify: boolean;
  /// Who `git` would record. `null` means it has no identity — asked before
  /// the message is written rather than after (SPEC §11).
  committer: string | null;

  /// What is being written, while it is. One at a time, which is the front
  /// end's half of the rule the Rust side also enforces with a lock.
  busy: string | null;
  /// The last write that failed, kept until one succeeds: a write that failed
  /// is visible nowhere else, because the repository simply did not change.
  writeError: string | null;
  /// Whatever `git commit` said on the way — hook output, its own summary.
  notes: string | null;
  question: Question | null;
  journal: JournalRow[];
  showJournal: boolean;

  /// The history list. Paged: the backend parks the walk and hands out five
  /// hundred rows at a time, so this grows rather than being replaced.
  history: Async<HistoryRow[]>;
  /// True once the walk has reached the roots. What stops the list asking —
  /// not a short page, which a filter can also produce.
  historyDone: boolean;
  /// A page is in flight. Without it a fast scroll asks four times for the
  /// same page, and the rows arrive four times.
  historyLoading: boolean;
  query: HistoryQuery;
  /// The commit the detail pane is about.
  commit: Async<CommitDetail>;
  /// Which file of that commit the diff pane shows.
  commitFile: string | null;
  /// Two commits being compared, when someone has picked a second one.
  ///
  /// Its own field rather than a mode on `commit`, because it is a different
  /// question: a commit detail asks "what did this change", a comparison asks
  /// "what is between these two", and the second has no author, no message and
  /// no parent to show.
  compare: Async<Comparison>;
  /// The commit a comparison would start from, once one has been marked.
  compareFrom: string | null;
};

const state = reactive<State>({
  platform: null,
  gitUnusable: null,
  repositories: [],
  screen: "repositories",
  open: null,
  summary: null,
  status: idle(),
  selected: null,
  diff: idle(),
  picked: new Set(),

  message: "",
  amend: false,
  signOff: false,
  noVerify: false,
  committer: null,

  busy: null,
  writeError: null,
  notes: null,
  question: null,
  journal: [],
  showJournal: false,

  history: idle(),
  historyDone: false,
  historyLoading: false,
  query: { all: false, firstParent: false, author: "", text: "", path: "", since: 0, until: 0 },
  commit: idle(),
  commitFile: null,
  compare: idle(),
  compareFrom: null,
});

export const app = readonly(state);

// ── Reading ─────────────────────────────────────────────────────────────────

export async function boot(): Promise<void> {
  const [theme, platform, gitUnusable, repositories] = await Promise.all([
    api.theme(),
    api.platform(),
    api.gitStatus(),
    api.repositories(),
  ]);
  // Before anything is drawn: a frame rendered without the tokens shows the
  // browser's defaults, and every rule in `style.css` reads one of them.
  document.documentElement.setAttribute("style", theme);
  state.platform = platform;
  state.gitUnusable = gitUnusable;
  state.repositories = repositories;

  const first = repositories[0];
  if (first) await openRepository(first.path);
}

export async function openRepository(path: string): Promise<void> {
  state.open = path;
  state.screen = "working-copy";
  state.selected = null;
  state.diff = idle();
  state.picked.clear();
  state.status = { status: "loading" };
  state.history = idle();
  state.historyDone = false;
  state.commit = idle();
  state.commitFile = null;
  state.compare = idle();
  state.compareFrom = null;
  // The box belongs to the repository, not to the window.
  state.message = "";
  state.amend = false;
  state.notes = null;
  state.writeError = null;

  try {
    const [rows, summary, committer, template] = await Promise.all([
      api.status(path),
      api.summary(path),
      api.committer(path),
      api.commitTemplate(path),
    ]);
    if (state.open !== path) return;

    state.status = { status: "ready", value: rows };
    state.summary = summary;
    state.committer = committer;
    if (template) state.message = template;
    void refreshJournal();

    // Open on a file rather than on an empty panel: the first row is what the
    // reader is going to click anyway.
    const first = rows[0];
    if (first) await selectFile(first.path, first.staged !== null);
  } catch (error) {
    state.status = { status: "failed", error: message(error) };
  }
}

export async function selectFile(file: string, staged: boolean): Promise<void> {
  const path = state.open;
  if (!path) return;

  const moved = state.selected?.path !== file || state.selected.staged !== staged;
  if (moved) state.picked.clear();
  state.selected = { path: file, staged };
  state.diff = { status: "loading" };

  const started = performance.now();
  try {
    const diff = await api.fileDiff(path, file, staged);
    // The selection may have moved while this was in flight. Dropping a stale
    // answer is the same rule the Rust side calls a generation check.
    if (state.selected?.path !== file || state.selected.staged !== staged) return;

    if (!diff) {
      state.diff = {
        status: "failed",
        error: "ce fichier n'est plus dans le statut",
      };
      return;
    }
    const fetched = performance.now();
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural(diff.hunks, "bloc")}`,
        rows: diff.rows ?? [],
        reason: diff.reason,
      },
    };
    // The number the port was decided without, measured on every diff. It goes
    // to the Rust log because that is the file anyone will read — and because
    // the run that matters is on Linux, over WebKitGTK.
    void api.log(
      "info",
      `diff ${diff.path}: ${diff.rows?.length ?? 0} lignes · IPC ${(fetched - started).toFixed(1)}ms`,
    );
  } catch (error) {
    if (state.selected?.path === file) {
      state.diff = { status: "failed", error: message(error) };
    }
  }
}

/// Switch screens. The shell — topbar, sidebar, status bar — does not move:
/// it belongs to `App.vue`, not to a screen, which is the fix for the GPUI bug
/// where opening History left a window with no way out of it.
export function showScreen(screen: Screen): void {
  state.screen = screen;
  // History is read when it is first looked at rather than when a repository
  // opens: a walk of a hundred thousand commits is not what someone who wanted
  // to stage a file asked for.
  if (screen === "history" && state.history.status === "idle") void loadHistory();
}

export async function loadHistory(): Promise<void> {
  const path = state.open;
  if (!path) return;
  const query = { ...state.query };
  state.history = { status: "loading" };
  state.historyDone = false;
  state.commit = idle();
  state.commitFile = null;
  // A comparison names two commits, and the walk that produced them is being
  // replaced. Keeping it would leave the pane pointing at rows nobody can see.
  state.compare = idle();
  state.compareFrom = null;

  const started = performance.now();
  try {
    const page = await api.history(path, query);
    if (state.open !== path || !sameQuery(query, state.query)) return;
    state.history = { status: "ready", value: page.rows };
    state.historyDone = page.done;
    void api.log(
      "info",
      `history: ${page.rows.length} lignes · IPC ${(performance.now() - started).toFixed(1)}ms`,
    );
    const first = page.rows[0];
    if (first) await selectCommit(first.id.full);
  } catch (error) {
    state.history = { status: "failed", error: message(error) };
  }
}

/// The next page, asked for when the list nears its end.
export async function moreHistory(): Promise<void> {
  const path = state.open;
  if (!path || state.historyLoading || state.historyDone) return;
  if (state.history.status !== "ready") return;
  const query = { ...state.query };

  state.historyLoading = true;
  try {
    const page = await api.historyMore(path);
    // A query that changed while this was in flight means these rows belong to
    // a history nobody is looking at any more.
    if (state.open !== path || !sameQuery(query, state.query)) return;
    if (state.history.status !== "ready") return;
    state.history = { status: "ready", value: [...state.history.value, ...page.rows] };
    state.historyDone = page.done;
  } catch (error) {
    state.history = { status: "failed", error: message(error) };
  } finally {
    state.historyLoading = false;
  }
}

/// Change what the walk covers. Always restarts it: a different query is a
/// different history, not more of this one.
export async function setQuery(query: Partial<HistoryQuery>): Promise<void> {
  state.query = { ...state.query, ...query };
  await loadHistory();
}

/// Clear every filter, leaving the two view switches alone.
///
/// They are different things: `all` and `firstParent` say which history to
/// look at, the rest say what to look for in it. A "clear" that also reset the
/// branch scope would undo a choice nobody asked to undo.
export async function clearFilters(): Promise<void> {
  await setQuery({ author: "", text: "", path: "", since: 0, until: 0 });
}

export function isFilteringHistory(): boolean {
  return isFiltered(state.query);
}

export async function selectCommit(id: string): Promise<void> {
  const path = state.open;
  if (!path) return;
  state.commit = { status: "loading" };
  state.commitFile = null;
  state.diff = idle();

  try {
    const detail = await api.commitDetail(path, id);
    // The selection may have moved while this was in flight.
    if (state.open !== path || state.commit.status !== "loading") return;
    state.commit = { status: "ready", value: detail };
    const first = detail.files[0];
    if (first) await selectCommitFile(first.path);
  } catch (error) {
    state.commit = { status: "failed", error: message(error) };
  }
}

export async function selectCommitFile(file: string): Promise<void> {
  const path = state.open;
  const id = state.commit.status === "ready" ? state.commit.value.id.full : null;
  if (!path || !id) return;

  state.commitFile = file;
  state.diff = { status: "loading" };
  const started = performance.now();
  try {
    const diff = await api.commitFileDiff(path, id, file);
    if (state.commitFile !== file) return;
    if (!diff) {
      state.diff = { status: "failed", error: "ce fichier n'est pas dans ce commit" };
      return;
    }
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural(diff.hunks, "bloc")}`,
        rows: diff.rows ?? [],
        reason: diff.reason,
      },
    };
    void api.log(
      "info",
      `diff ${diff.path}@${id.slice(0, 7)}: ${diff.rows?.length ?? 0} lignes · IPC ${(
        performance.now() - started
      ).toFixed(1)}ms`,
    );
  } catch (error) {
    if (state.commitFile === file) state.diff = { status: "failed", error: message(error) };
  }
}

// ── Comparing two commits (SPEC §11) ────────────────────────────────────────

/// Mark where a comparison starts, or drop the mark.
///
/// Marking is separate from comparing because the second commit is chosen by
/// scrolling, and a comparison that ran on every row the pointer touched would
/// read a diff per row.
export function markCompareFrom(id: string | null): void {
  state.compareFrom = id;
  if (id === null) state.compare = idle();
}

/// Compare the marked commit with `id`.
export async function compareWith(id: string): Promise<void> {
  const path = state.open;
  const from = state.compareFrom;
  if (!path || !from || from === id) return;

  state.compare = { status: "loading" };
  state.commitFile = null;
  state.diff = idle();
  try {
    const comparison = await api.compare(path, from, id);
    if (state.open !== path || state.compare.status !== "loading") return;
    state.compare = { status: "ready", value: comparison };
    const first = comparison.files[0];
    if (first) await selectCompareFile(first.path);
  } catch (error) {
    state.compare = { status: "failed", error: message(error) };
  }
}

export function stopComparing(): void {
  state.compare = idle();
  state.compareFrom = null;
  const open = state.commit.status === "ready" ? state.commit.value : null;
  if (open) void selectCommit(open.id.full);
}

export async function selectCompareFile(file: string): Promise<void> {
  const path = state.open;
  const ends = state.compare.status === "ready" ? state.compare.value : null;
  if (!path || !ends) return;

  state.commitFile = file;
  state.diff = { status: "loading" };
  try {
    const diff = await api.compareFileDiff(path, ends.from.full, ends.to.full, file);
    if (state.commitFile !== file) return;
    if (!diff) {
      state.diff = { status: "failed", error: "ce fichier n'est pas dans cette comparaison" };
      return;
    }
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural(diff.hunks, "bloc")}`,
        rows: diff.rows ?? [],
        reason: diff.reason,
      },
    };
  } catch (error) {
    if (state.commitFile === file) state.diff = { status: "failed", error: message(error) };
  }
}

function sameQuery(a: HistoryQuery, b: HistoryQuery): boolean {
  return (
    a.all === b.all &&
    a.firstParent === b.firstParent &&
    a.author === b.author &&
    a.text === b.text &&
    a.path === b.path &&
    a.since === b.since &&
    a.until === b.until
  );
}

export function toggleJournal(): void {
  state.showJournal = !state.showJournal;
  if (state.showJournal) void refreshJournal();
}

async function refreshJournal(): Promise<void> {
  state.journal = await api.journal();
}

// ── Writing ─────────────────────────────────────────────────────────────────

/// Run one write, then put the screen back in agreement with the repository.
///
/// Everything a write can change is re-read afterwards, including the diff of
/// the open file: staging one hunk means the file the reader is looking at has
/// two fewer lines on this side and two more on the other, and a pane that kept
/// drawing the old one would be lying about what is now in the index.
async function write(label: string, run: () => Promise<void>): Promise<void> {
  if (state.busy) return;
  state.busy = label;
  state.writeError = null;
  try {
    await run();
  } catch (error) {
    state.writeError = `${label} : ${message(error)}`;
  } finally {
    state.busy = null;
  }
  await settle();
}

async function settle(): Promise<void> {
  const path = state.open;
  if (!path) return;
  // The picked lines index a diff that has just stopped being true.
  state.picked.clear();

  try {
    const [rows, summary] = await Promise.all([api.status(path), api.summary(path)]);
    if (state.open !== path) return;
    state.status = { status: "ready", value: rows };
    state.summary = summary;
    void refreshJournal();

    const was = state.selected;
    if (!was) return;
    const row = rows.find((entry) => entry.path === was.path);
    if (!row) {
      // The file has left the working copy — staged whole, or discarded.
      state.selected = null;
      state.diff = idle();
      return;
    }
    await selectFile(row.path, sideOf(row, was.staged));
  } catch (error) {
    state.status = { status: "failed", error: message(error) };
  }
}

/// Which side of the index to show for a row, preferring the one being read.
///
/// Staging a hunk usually leaves more on the unstaged side, and the point of
/// staging one hunk is to look at what is left — so the pane stays where it
/// was whenever that side still has something.
function sideOf(row: StatusRow, prefer: boolean): boolean {
  if (prefer && row.staged !== null) return true;
  if (!prefer && row.unstaged !== null) return false;
  return row.staged !== null;
}

export function stageFile(row: StatusRow, unstage: boolean): void {
  const path = state.open;
  if (!path) return;
  void write(unstage ? `Désindexer ${row.path}` : `Indexer ${row.path}`, () =>
    api.stage(path, row.path, { kind: "file" }, unstage),
  );
}

export function stageEverything(unstage: boolean): void {
  const path = state.open;
  if (!path) return;
  void write(unstage ? "Tout désindexer" : "Tout indexer", () => api.stageAll(path, unstage));
}

export function stageHunk(hunk: number, unstage: boolean): void {
  const path = state.open;
  const file = state.selected?.path;
  if (!path || !file) return;
  void write(unstage ? `Désindexer le bloc ${hunk + 1}` : `Indexer le bloc ${hunk + 1}`, () =>
    api.stage(path, file, { kind: "hunks", hunks: [hunk] }, unstage),
  );
}

export function stagePicked(unstage: boolean): void {
  const path = state.open;
  const file = state.selected?.path;
  const lines = pickedLines();
  if (!path || !file || lines.length === 0) return;
  const what = plural(lines.length, "ligne");
  void write(unstage ? `Désindexer ${what}` : `Indexer ${what}`, () =>
    api.stage(path, file, { kind: "lines", lines }, unstage),
  );
}

// ── The destructive ones, which ask first ───────────────────────────────────
//
// SPEC §3 rule 7: what a discard removes was never committed and is not in the
// reflog, so it is the one thing in this file that cannot be undone. The
// pending action is held here rather than in the reactive state, so nothing can
// render it, serialise it or compare it.

let pending: (() => void) | null = null;

function ask(question: Question, action: () => void): void {
  state.question = question;
  pending = action;
}

export function answer(yes: boolean): void {
  const action = pending;
  pending = null;
  state.question = null;
  if (yes) action?.();
}

export function discardFile(row: StatusRow): void {
  const path = state.open;
  if (!path) return;
  ask(
    {
      title: `Rejeter les modifications de ${row.path} ?`,
      detail:
        row.unstaged === "untracked"
          ? "Ce fichier n'est pas suivi : le rejeter le supprime du disque. Rien ne le retiendra."
          : "Les modifications non indexées de ce fichier seront perdues. Elles ne sont dans aucun commit ni dans le reflog.",
      verb: "Rejeter",
    },
    () => void write(`Rejeter ${row.path}`, () => api.discard(path, row.path, { kind: "file" })),
  );
}

export function discardHunk(hunk: number): void {
  const path = state.open;
  const file = state.selected?.path;
  if (!path || !file) return;
  ask(
    {
      title: `Rejeter le bloc ${hunk + 1} de ${file} ?`,
      detail: "Ces lignes seront retirées du fichier. Elles ne sont dans aucun commit.",
      verb: "Rejeter",
    },
    () =>
      void write(`Rejeter le bloc ${hunk + 1}`, () =>
        api.discard(path, file, { kind: "hunks", hunks: [hunk] }),
      ),
  );
}

export function discardPicked(): void {
  const path = state.open;
  const file = state.selected?.path;
  const lines = pickedLines();
  if (!path || !file || lines.length === 0) return;
  const what = plural(lines.length, "ligne");
  ask(
    {
      title: `Rejeter ${what} de ${file} ?`,
      detail: "Ces lignes seront retirées du fichier. Elles ne sont dans aucun commit.",
      verb: "Rejeter",
    },
    () => void write(`Rejeter ${what}`, () => api.discard(path, file, { kind: "lines", lines })),
  );
}

// ── The commit box ──────────────────────────────────────────────────────────

export function setMessage(text: string): void {
  state.message = text;
}

export function setSignOff(on: boolean): void {
  state.signOff = on;
}

export function setNoVerify(on: boolean): void {
  state.noVerify = on;
}

/// Turning on amend brings in the message it is about to replace.
///
/// Without it the box would open empty and a slip of a checkbox would cost the
/// message of the commit being replaced — which is the loss rule 7 is about,
/// arriving through a control nobody thinks of as destructive.
export async function setAmend(on: boolean): Promise<void> {
  state.amend = on;
  if (!on || state.message.trim() !== "" || !state.open) return;
  try {
    const previous = await api.headMessage(state.open);
    if (state.amend && state.message.trim() === "" && previous) state.message = previous;
  } catch (error) {
    void api.log("warn", `message du commit précédent illisible : ${message(error)}`);
  }
}

export function commit(): void {
  const path = state.open;
  if (!path || !canCommit()) return;
  const { message: text, amend, signOff, noVerify } = state;

  const run = (): void => {
    void write(amend ? "Corriger le commit" : "Commiter", async () => {
      const made = await api.commit(path, text, amend, signOff, noVerify);
      state.message = "";
      state.amend = false;
      // Hook output, shown rather than swallowed: a `pre-commit` hook that
      // rewrote a file says so here and nowhere else.
      state.notes = [`${made.id.short} · ${firstLine(text)}`, made.notes.trim()]
        .filter(Boolean)
        .join("\n");
    });
  };

  if (amend) {
    ask(
      {
        title: "Remplacer le commit précédent ?",
        detail:
          "Le commit actuel sera remplacé. Il ne restera accessible que par le reflog, et disparaîtra d'une branche déjà poussée.",
        verb: "Corriger",
      },
      run,
    );
  } else {
    run();
  }
}

export function canCommit(): boolean {
  if (state.gitUnusable || state.busy || !state.committer) return false;
  if (state.message.trim() === "") return false;
  return state.amend || stagedCount() > 0;
}

export function stagedCount(): number {
  return state.status.status === "ready"
    ? state.status.value.filter((row) => row.staged !== null).length
    : 0;
}

export function unstagedCount(): number {
  return state.status.status === "ready"
    ? state.status.value.filter((row) => row.unstaged !== null).length
    : 0;
}

export function dismissNotes(): void {
  state.notes = null;
}

export function dismissWriteError(): void {
  state.writeError = null;
}

// ── Picking lines ───────────────────────────────────────────────────────────

export const lineKey = (hunk: number, index: number): string => `${hunk}:${index}`;

/// Toggle one line, or extend from the last one when `extend` is set.
///
/// A range only ever runs inside one hunk: a selection that spanned two would
/// have to mean something about the context between them, and it does not.
export function pickLine(hunk: number, index: number, extend: boolean): void {
  if (extend && anchor && anchor.hunk === hunk) {
    const [from, to] = anchor.index <= index ? [anchor.index, index] : [index, anchor.index];
    for (let at = from; at <= to; at += 1) state.picked.add(lineKey(hunk, at));
    return;
  }
  const key = lineKey(hunk, index);
  if (state.picked.has(key)) state.picked.delete(key);
  else state.picked.add(key);
  anchor = { hunk, index };
}

let anchor: { hunk: number; index: number } | null = null;

export function clearPicked(): void {
  state.picked.clear();
  anchor = null;
}

function pickedLines(): [number, number][] {
  return [...state.picked]
    .map((key) => key.split(":").map(Number) as [number, number])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

// ── Odds and ends ───────────────────────────────────────────────────────────

export function plural(count: number, word: string): string {
  return `${count} ${word}${count > 1 ? "s" : ""}`;
}

function firstLine(text: string): string {
  return text.split("\n", 1)[0] ?? "";
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
