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
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { measure } from "./metrics";
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
  type Progress,
  type Refs,
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
  /// Why the last folder that was picked could not be added. Kept until the
  /// next attempt: a dialog that closed on an error would take the message with
  /// it.
  addError: string | null;
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
  /// Whether the filter row is unfolded. Folded by default: it is five fields
  /// answering a question most readers are not asking, and it was taking two
  /// rows of a column they wanted for commits.
  showFilters: boolean;
  /// The commit the detail pane is about.
  commit: Async<CommitDetail>;
  /// Which file of that commit the diff pane shows.
  commitFile: string | null;
  /// One summary per repository in the library, filled in after the rows
  /// arrive. Keyed by path, because that is what a row is addressed by.
  ///
  /// Separate from the rows because reading one is Git work: a status walk per
  /// repository the user has ever added, before the screen has drawn anything,
  /// is what asking for them together would cost.
  library: Record<string, Async<RepoSummary>>;
  /// Which card the Repositories screen is showing.
  card: string | null;
  /// What the network is doing, while it is doing it. `null` when nothing is.
  running: Progress | null;
  /// Whether a cancellation has been asked for and not yet taken effect. The
  /// overlay says so rather than looking as though the button did nothing:
  /// `git` stops when it next checks, which is not instant.
  stopping: boolean;
  /// What the last network operation said. `git` reports what it did on
  /// stderr — "Everything up-to-date", the branches it created — and that is
  /// worth keeping until the next one.
  networkSaid: string | null;
  /// Every reference, for the sidebar's tree.
  ///
  /// Read in the background when a repository opens rather than with the
  /// status: `Refs::load` counts every branch against its upstream, which is
  /// the one expensive read on that path — `Summary` avoids it for the same
  /// reason.
  refs: Async<Refs>;
  /// Which branch prefixes are folded away, by name.
  collapsed: Record<string, boolean>;
  /// How wide each resizable column has been dragged, in unscaled pixels.
  /// Absent means the stylesheet's own width.
  panes: Record<string, number>;
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
  addError: null,
  journal: [],
  showJournal: false,

  history: idle(),
  historyDone: false,
  historyLoading: false,
  query: { all: false, firstParent: false, author: "", text: "", path: "", since: 0, until: 0 },
  showFilters: false,
  commit: idle(),
  commitFile: null,
  compare: idle(),
  compareFrom: null,
  library: {},
  card: null,
  running: null,
  stopping: false,
  networkSaid: null,
  refs: idle(),
  collapsed: {},
  panes: {},
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
  // The metrics have to be read *after* the tokens are on the element and
  // before any list decides how many rows fit.
  measure();
  state.platform = platform;
  // Widths the user dragged last time. Asked for with everything else rather
  // than lazily: a column that started at its default and jumped once the
  // answer arrived would be worse than one that never moved.
  // A width the user dragged is a convenience; failing to read one is not a
  // reason for the window not to open.
  void api
    .panes()
    .then((panes) => {
      state.panes = panes;
    })
    .catch((error) => api.log("warn", `largeurs de colonnes illisibles : ${message(error)}`));
  state.gitUnusable = gitUnusable;
  state.repositories = repositories;

  // The list first, then a summary per row in the background: the screen draws
  // immediately and fills in, rather than waiting on a status walk per
  // repository.
  void readLibrary();

  // The window opens on Repositories, always. SPEC §12 measures cold start
  // "jusqu'à l'écran Repositories" and board 06 is drawn "pas de dépôt ouvert":
  // choosing which repository to work in is the first thing the product asks,
  // not something it decides for you. Opening the most recent one on your
  // behalf also means the first thing you see is a status walk you did not ask
  // for, on whichever repository happened to be first.
  state.screen = "repositories";
}

/// Re-read the library, and a summary for each row that is still on disk.
export async function readLibrary(): Promise<void> {
  const rows = await api.repositories();
  state.repositories = rows;
  if (state.card && !rows.some((row) => row.path === state.card)) state.card = null;
  state.card ??= rows[0]?.path ?? null;

  await Promise.all(rows.map((row) => readSummary(row)));
}

async function readSummary(row: LibraryRow): Promise<void> {
  if (row.missing) {
    // Not an error to be reported: DESIGN §4 says the row stays and says
    // "introuvable", because a repository on an unmounted disk comes back.
    state.library[row.path] = { status: "failed", error: "introuvable sur le disque" };
    return;
  }
  state.library[row.path] = { status: "loading" };
  try {
    state.library[row.path] = { status: "ready", value: await api.summary(row.path) };
  } catch (error) {
    state.library[row.path] = { status: "failed", error: message(error) };
  }
}

export function showCard(path: string): void {
  state.card = path;
}

/// A column's width, or the stylesheet's own when it has not been dragged.
export function paneWidth(name: string, fallback: number): number {
  return state.panes[name] ?? fallback;
}

/// While the pointer moves.
export function resizePane(name: string, width: number): void {
  state.panes[name] = width;
}

/// When it stops. Separate because the settings file is rewritten on every
/// call, and a drag is a hundred of them.
export function settlePane(name: string, width: number): void {
  state.panes[name] = width;
  void api.setPane(name, width);
}

/// Add a folder to the library, through the platform's own folder picker.
///
/// The picker is the platform's rather than a path box: a path typed by hand is
/// a path that can be mistyped, and every desktop has a folder chooser people
/// already know.
export async function addRepository(): Promise<void> {
  const chosen = await open({ directory: true, multiple: false, title: "Ajouter un dépôt" });
  if (typeof chosen !== "string") return;

  state.addError = null;
  try {
    // Opening it is what says whether it is a repository, so a folder that is
    // not one is refused here rather than added and struck through.
    await api.addRepository(chosen);
    await readLibrary();
    state.card = chosen;
  } catch (error) {
    state.addError = message(error);
  }
}

/// Take a repository out of the list. Never off the disk.
export function forgetRepository(row: LibraryRow): void {
  ask(
    {
      title: `Retirer ${row.name} de la liste ?`,
      detail:
        "Le dépôt reste sur le disque : seule son entrée dans cette liste disparaît, et il peut être rajouté.",
      verb: "Retirer",
    },
    () => {
      void (async () => {
        await api.forgetRepository(row.path);
        if (state.open === row.path) state.open = null;
        await readLibrary();
      })();
    },
  );
}

export async function openRepository(path: string): Promise<void> {
  state.open = path;
  state.screen = "working-copy";
  void api.touchRepository(path);
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
  state.refs = idle();
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
    void readRefs();

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
  // Working Copy and History are views *of a repository*. With none open they
  // would draw a shell around nothing — a sidebar counting a working copy that
  // does not exist, a history that cannot load. Reachable only through the
  // sidebar, which is only drawn when one is open; the guard makes that a
  // property of the state rather than of the routing.
  if (screen !== "repositories" && !state.open) {
    state.screen = "repositories";
    return;
  }
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

/// Fold the filter row away, or bring it back.
///
/// Folding it never *clears* it: a filter that went on narrowing the list while
/// the fields that explain it were hidden would be a list that lies. So the
/// row stays open as long as something is filtered.
export function toggleFilters(): void {
  state.showFilters = !state.showFilters || isFiltered(state.query);
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

// ── The network (M7) ────────────────────────────────────────────────────────

/// Start listening for progress.
///
/// Once, at start-up. `git` writes progress to stderr as it works, and the
/// backend turns each line into an event — the answer to the command only comes
/// back at the end, which is exactly what the user is waiting to hear about.
export async function watchProgress(): Promise<void> {
  await listen<Progress>("progress", (event) => {
    state.running = event.payload;
  });
}

/// Run one network operation, with the overlay up for its duration.
async function overNetwork(what: string, run: () => Promise<string>): Promise<void> {
  if (state.running) return;
  state.running = { what, phase: "…", percent: null };
  state.stopping = false;
  state.networkSaid = null;
  try {
    const said = await run();
    state.networkSaid = said.trim() || `${what} : rien à faire`;
  } catch (error) {
    state.writeError = `${what} : ${message(error)}`;
  } finally {
    state.running = null;
    state.stopping = false;
  }
  // Refs move, and so does the divergence every branch row shows.
  await settle();
}

export function fetchRemote(remote = ""): void {
  const path = state.open;
  if (!path) return;
  void overNetwork("Fetch", () => api.fetch(path, remote));
}

export function pullRemote(): void {
  const path = state.open;
  if (!path) return;
  void overNetwork("Pull", () => api.pull(path));
}

/// Push the current branch to its upstream's remote, or to `origin`.
///
/// `force` is `--force-with-lease` and nothing weaker: plain `--force`
/// overwrites whatever is on the remote, including a colleague's commit pushed
/// thirty seconds ago, and cannot tell that from the rebase you meant to
/// publish.
export function pushBranch(force: boolean): void {
  const path = state.open;
  const summary = state.summary;
  if (!path || !summary) return;
  const branch = summary.operation ? null : summary.head;
  if (!branch) return;

  const tracking = summary.tracking;
  const remote = tracking?.upstream.split("/")[0] ?? "origin";
  const run = (): void =>
    void overNetwork("Push", () => api.push(path, remote, branch, force, !tracking));

  if (!force) {
    run();
    return;
  }
  ask(
    {
      title: `Forcer la publication de ${branch} ?`,
      detail:
        "Avec --force-with-lease : refusé si le distant a bougé depuis la dernière fois qu'on l'a vu. Ce qui est remplacé n'est plus sur aucun clone.",
      verb: "Forcer",
    },
    run,
  );
}

/// Stop what is running. `git` stops when it next looks, which is not instant.
export function stopNetwork(): void {
  if (!state.running) return;
  state.stopping = true;
  void api.cancelOperation();
}

export function dismissNetworkSaid(): void {
  state.networkSaid = null;
}

// ── Integrating one branch into another (M7) ────────────────────────────────

/// Merge `branch` into the current one.
///
/// Confirmed, because it rewrites the working tree and can stop half-way on a
/// conflict — a state that is neither before nor after, and one the reader
/// should have chosen to enter.
export function mergeBranch(branch: string, noFastForward = false): void {
  const path = state.open;
  const into = state.summary?.head;
  if (!path || !into) return;
  ask(
    {
      title: `Fusionner ${branch} dans ${into} ?`,
      detail:
        "Les fichiers de la copie de travail vont changer. Si les deux branches ont touché les mêmes lignes, la fusion s'arrêtera sur un conflit et le dépôt restera à mi-chemin — l'abandon est dans la barre du bas.",
      verb: "Fusionner",
    },
    () =>
      void write(`Fusionner ${branch}`, async () => {
        await api.merge(path, branch, noFastForward, false);
      }),
  );
}

/// Replay the current branch's commits on top of `onto`.
export function rebaseOnto(onto: string): void {
  const path = state.open;
  const branch = state.summary?.head;
  if (!path || !branch) return;
  ask(
    {
      title: `Rebaser ${branch} sur ${onto} ?`,
      detail:
        "Les commits de la branche sont réécrits : ceux qu'ils remplacent ne seront joignables que par le reflog. Si une branche publiée en dépend, elle divergera.",
      verb: "Rebaser",
    },
    () =>
      void write(`Rebaser sur ${onto}`, async () => {
        await api.rebase(path, onto);
      }),
  );
}

/// Put the repository back where the half-finished operation found it.
export function abortOperation(): void {
  const path = state.open;
  const operation = state.summary?.operation;
  if (!path || !operation) return;
  ask(
    {
      title: `Abandonner ${operation} ?`,
      detail:
        "Le dépôt revient où l'opération l'a trouvé. Ce qui a été résolu jusqu'ici est perdu : personne d'autre ne l'a.",
      verb: "Abandonner",
    },
    () =>
      void write(`Abandonner ${operation}`, async () => {
        await api.abortOperation(path);
      }),
  );
}

// ── Branches (M7) ───────────────────────────────────────────────────────────

export async function readRefs(): Promise<void> {
  const path = state.open;
  if (!path) return;
  state.refs = { status: "loading" };
  try {
    const refs = await api.refs(path);
    if (state.open !== path) return;
    state.refs = { status: "ready", value: refs };
  } catch (error) {
    state.refs = { status: "failed", error: message(error) };
  }
}

export function toggleBranchGroup(name: string): void {
  state.collapsed[name] = !state.collapsed[name];
}

export function isCollapsed(name: string): boolean {
  return state.collapsed[name] ?? false;
}

/// Switch to a branch. Everything on screen is about the old one.
export function checkoutBranch(name: string): void {
  const path = state.open;
  if (!path) return;
  void write(`Basculer sur ${name}`, () => api.checkout(path, name, false));
}

export function createBranch(name: string, start: string, andSwitch: boolean): void {
  const path = state.open;
  const trimmed = name.trim();
  if (!path || trimmed === "") return;
  void write(`Créer ${trimmed}`, () => api.createBranch(path, trimmed, start.trim(), andSwitch));
}

/// Delete a branch, asking the question its state deserves.
///
/// The two are genuinely different: removing a label costs nothing, and
/// throwing away commits that are on no other branch leaves them reachable
/// only through the reflog. A single wording for both would either frighten
/// people off the harmless one or wave them through the other.
export function deleteBranch(row: { name: string; merged: boolean }): void {
  const path = state.open;
  if (!path) return;
  const force = !row.merged;
  ask(
    {
      title: `Supprimer la branche ${row.name} ?`,
      detail: force
        ? "Ses commits ne sont sur aucune autre branche : après ça ils ne seront joignables que par le reflog."
        : "Tous ses commits sont déjà sur la branche courante. Seule l'étiquette disparaît.",
      verb: "Supprimer",
    },
    () => void write(`Supprimer ${row.name}`, () => api.deleteBranch(path, row.name, force)),
  );
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
    // A checkout, a branch created or deleted: the tree is what changed.
    void readRefs();

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
