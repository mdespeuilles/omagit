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
import { count as plural, preference, t, useLanguage, worded } from "./i18n";
import type { Row as PaletteRow } from "./palette";
import type { Action as KeymapAction } from "./keymap";
import {
  api,
  isFiltered,
  type Changed,
  type CommitDetail,
  type Comparison,
  type DiffRow,
  type FileRow,
  type HistoryQuery,
  type HistoryRow,
  type JournalRow,
  type LibraryGroup,
  type LibraryRow,
  type PlatformFacts,
  type Preferences,
  type Progress,
  type Refs,
  type Choice,
  type Conflicted,
  type RepoSummary,
  type Sides,
  type StashRow,
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

/// What a pane draws: the answer it has, or the one it is in the middle of
/// replacing.
///
/// The `previous` in `loading` was declared with the type and never used, so
/// every re-read emptied its pane and filled it again. On a screen that
/// re-reads itself — the watcher settles after every write, every save, every
/// commit made in a terminal — that is a window that flickers, which is what
/// this is here to stop. A pane is blank only when it has never had an answer.
export function shown<T>(state: Async<T>): T | null {
  if (state.status === "ready") return state.value;
  if (state.status === "loading") return state.previous ?? null;
  return null;
}

/// The `loading` to move to when something is already on screen: it carries
/// what it is replacing, so the pane keeps drawing until the answer lands.
export function again<T>(state: Async<T>): Async<T> {
  const previous = shown(state);
  return previous === null ? { status: "loading" } : { status: "loading", previous };
}

export type Screen = "repositories" | "working-copy" | "history" | "stashes" | "settings";

/// A write that did not happen: what was asked, and what came back.
export type Failure = { what: string; said: string };

type DiffValue = {
  path: string;
  header: string;
  rows: DiffRow[];
  reason: string | null;
};

/// A question the user has to answer before something irreversible happens
/// (SPEC §3 rule 7). The action itself is *not* in here: a closure inside
/// reactive state is a thing that cannot be inspected, compared or logged.
/// A question the user has to answer before something happens (SPEC §3 rule 7).
///
/// Usually two ways out — do it, or do not. `alternative` adds a second way of
/// doing it, for the one question that is not "are you sure": a pull on a
/// diverged branch with nothing configured has to choose between merging and
/// rebasing, and neither is the dangerous one.
type Question = { title: string; detail: string; verb: string; alternative?: string };

type State = {
  platform: PlatformFacts | null;
  /// Why writing is impossible, when it is. Raised before anything is typed
  /// rather than after (SPEC §8).
  gitUnusable: string | null;
  repositories: LibraryRow[];
  /// The folders the rows are filed under, in the order they are drawn. Held
  /// beside the rows rather than derived from them: a folder with nothing in
  /// it is one the user has just made, and deriving would make it vanish.
  groups: LibraryGroup[];
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
  /// The last write that failed, kept until one succeeds. Two fields, not one
  /// sentence: the band above the status bar sets the action apart from what
  /// `git` said about it, and a caller that had joined them would have to be
  /// unpicked to draw either.
  writeError: Failure | null;
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
  /// The clone dialog, while it is up. `null` when it is not.
  clone: CloneForm | null;

  /// The shelf, read when the screen is first looked at.
  stashes: Async<StashRow[]>;
  /// Which entry the preview is about, by commit — never by index. `git`
  /// addresses a stash by its position in a log, and that position moves the
  /// moment one is dropped; the commit is what does not.
  stash: string | null;
  /// What that entry holds, one row per file.
  stashFiles: Async<FileRow[]>;
  /// Which of those files the diff pane shows. Its own field rather than
  /// `commitFile`: a stash is not a commit here, and two screens sharing one
  /// selection is how a pane ends up drawing the other one's file.
  stashFile: string | null;
  /// The "Remiser" form, while it is open. `null` when it is not.
  stashing: StashForm | null;

  /// Which two versions a conflicted file has, while an operation is stopped on
  /// one. `null` when nothing is running — and read from the repository, never
  /// assumed from the branch on screen.
  sides: Sides | null;
  /// The conflict dialog, while it is up.
  resolving: Resolving | null;
  /// The command palette, while it is up. `at` is the row ⏎ would run, counted
  /// across the groups in the order they are drawn.
  palette: { query: string; at: number } | null;

  /// Which of DESIGN §5's three zones the keyboard is in — sidebar, centre
  /// column, detail panel — with the same meaning on every screen.
  ///
  /// In the state rather than in the DOM, like every other selection here: the
  /// lists are virtualised, so the row the keyboard is on is routinely *not*
  /// rendered, and `document.activeElement` cannot be the record of where the
  /// keyboard is when the element under it comes and goes with the scroll.
  zone: Zone;
  /// Where the keyboard is inside the branch tree. Its own cursor because a
  /// branch has no "selected" of its own: moving through them must not re-walk
  /// a history per keystroke, so `⏎` is what asks for one.
  branchCursor: string | null;
  /// Whether the `?` sheet is up.
  shortcuts: boolean;
  /// What the Preferences screen is showing, once it has been opened.
  preferences: Async<Preferences>;
  /// The bindings the user has changed, by action id (SPEC §11). Overrides
  /// only: the table of defaults is `keymap.ts`, and `binding()` there is what
  /// puts the two together.
  keymap: Record<string, string>;
  /// The repositories the window has open, oldest first, and `open` says which
  /// one is on screen. The name travels with the path: a repository taken out
  /// of the list while its tab is up would otherwise lose the only thing the
  /// tab shows.
  tabs: { path: string; name: string }[];
  /// Whether something is being dragged over the window right now. Board 06
  /// draws no drop target, so the window says it another way: the empty state
  /// and the list edge answer the pointer rather than staying silent.
  dragging: boolean;
  /// What narrows the repository list. Board 06 draws the box; it was disabled
  /// and labelled M9 until now, and `/` needs somewhere to land.
  libraryFilter: string;
  /// The folder whose name is being typed, or `null`. A folder made from the
  /// button opens straight into it: a folder called "New folder" that has to
  /// be renamed in a second gesture is a folder most people leave so named.
  renamingGroup: number | null;
  /// The repository being dragged, by path, and where it would land. Board 06
  /// draws the insertion as a 2px accent line between two rows — which needs
  /// to be *between*, so the target is a group and a position in it rather
  /// than a row.
  draggedRepository: string | null;
  dropAt: { group: number; index: number } | null;
};

/// The three columns, numbered as DESIGN §5 numbers them.
export type Zone = 1 | 2 | 3;

/// What the conflict dialog is holding.
export type Resolving = {
  file: string;
  /// The markers as they were read. A failure here is the file being
  /// unreadable as a conflict — binary, or markers that do not pair up — and
  /// the dialog says so rather than drawing nothing.
  body: Async<Conflicted>;
  /// One answer per conflict, in file order; `null` until one is made. The
  /// primary button waits for all of them, because a file written with half its
  /// conflicts answered would still have markers in it and would be staged
  /// anyway.
  choices: (Choice | null)[];
  /// Which conflict the header counts — board 07's "conflit 1 / 2".
  at: number;
};

/// What the Remiser form is holding.
export type StashForm = {
  message: string;
  /// Take the files git has never seen along too. Off by default: it removes
  /// files from the disk that are in no index and no commit, and the entry it
  /// puts them in is the only copy.
  untracked: boolean;
};

/// What the clone dialog is holding, board 07's fields one for one.
export type CloneForm = {
  url: string;
  /// The folder the clone is created *in*. The destination the dialog shows is
  /// this joined with `name`.
  parent: string;
  /// Derived from the URL by the backend, using `git`'s own rule — but
  /// editable, because a second clone of the same repository needs a different
  /// folder and the URL is not going to change.
  name: string;
  /// True once someone has typed in the name field. The URL stops filling it in
  /// after that: overwriting what was typed is the worst thing a helpful
  /// default can do.
  renamed: boolean;
  shallow: boolean;
  submodules: boolean;
  group: number | null;
  /// What `git ls-remote` said about the URL, if it has been asked yet.
  probe: "idle" | "checking" | "reachable" | { error: string };
};

const state = reactive<State>({
  platform: null,
  gitUnusable: null,
  repositories: [],
  groups: [],
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
  query: {
    all: false,
    firstParent: false,
    branch: "",
    author: "",
    text: "",
    path: "",
    since: 0,
    until: 0,
  },
  showFilters: false,
  commit: idle(),
  commitFile: null,
  compare: idle(),
  compareFrom: null,
  library: {},
  card: null,
  running: null,
  stopping: false,
  refs: idle(),
  collapsed: {},
  panes: {},
  clone: null,

  stashes: idle(),
  stash: null,
  stashFiles: idle(),
  stashFile: null,
  stashing: null,
  sides: null,
  resolving: null,
  palette: null,
  shortcuts: false,
  preferences: idle(),
  keymap: {},
  zone: 1,
  branchCursor: null,
  libraryFilter: "",
  renamingGroup: null,
  draggedRepository: null,
  dropAt: null,
  dragging: false,
  tabs: [],
});

export const app = readonly(state);

// ── Reading ─────────────────────────────────────────────────────────────────

export async function boot(): Promise<void> {
  const [theme, platform, gitUnusable, repositories, keymap, language] = await Promise.all([
    api.theme(),
    api.platform(),
    api.gitStatus(),
    api.repositories(),
    // With everything else, not lazily: a window that drew its shortcut hints
    // from the defaults and corrected them a moment later would be a window
    // that lies about its own keyboard for a frame.
    api.keymap(),
    // With everything else, and before the first frame: a window that drew in
    // English and switched to French a tick later would be a window that
    // flickers in a language you did not ask for.
    api.language(),
  ]);
  useLanguage(language);
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
  state.repositories = repositories.rows;
  state.groups = repositories.groups;
  state.keymap = keymap;

  // The list first, then a summary per row in the background: the screen draws
  // immediately and fills in, rather than waiting on a status walk per
  // repository.
  //
  // The summaries directly rather than through `readLibrary`, which would ask
  // for the arrangement a second time — it arrived above, with everything
  // else — and would not begin the status walks until that answer came back.
  void Promise.all(state.repositories.map((row) => readSummary(row)));

  // The window opens on Repositories, always. SPEC §12 measures cold start
  // "jusqu'à l'écran Repositories" and board 06 is drawn "pas de dépôt ouvert":
  // choosing which repository to work in is the first thing the product asks,
  // not something it decides for you. Opening the most recent one on your
  // behalf also means the first thing you see is a status walk you did not ask
  // for, on whichever repository happened to be first.
  state.screen = "repositories";
}

/// Re-read how the list is arranged — the folders, and which one each row is
/// in — and nothing Git knows.
///
/// Separate from [`readLibrary`] because the summaries are a status walk per
/// repository: renaming a folder would set every row back to "…" for as long
/// as that takes, to answer a question nobody asked.
export async function readArrangement(): Promise<void> {
  const { groups, rows } = await api.repositories();
  state.repositories = rows;
  state.groups = groups;
  if (state.card && !rows.some((row) => row.path === state.card)) state.card = null;
  state.card ??= rows[0]?.path ?? null;
}

/// The arrangement, and a fresh summary for each row that is still on disk.
export async function readLibrary(): Promise<void> {
  await readArrangement();
  await Promise.all(state.repositories.map((row) => readSummary(row)));
}

async function readSummary(row: LibraryRow): Promise<void> {
  if (row.missing) {
    // Not an error to be reported: DESIGN §4 says the row stays and says
    // "introuvable", because a repository on an unmounted disk comes back.
    state.library[row.path] = { status: "failed", error: t("library.missing") };
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
  const chosen = await open({ directory: true, multiple: false, title: t("library.picker") });
  if (typeof chosen !== "string") return;
  await addPath(chosen);
}

/// Add repositories by path — what the folder picker ends up calling, and what
/// a folder dropped on the window calls too.
///
/// M3 took a drop and the port to Tauri did not carry it over, which left one
/// door into the library: the platform's open panel. That is not a small gap —
/// the Finder's panel hides `/var/folders/…`, so a repository built under
/// `$TMPDIR` could not be added at all (§5, sixteenth defect).
///
/// One at a time and in order, because each one is a `git` open: the first
/// folder that is not a repository says so, and the rest still arrive.
export async function addRepositories(paths: string[]): Promise<void> {
  for (const path of paths) await addPath(path);
}

async function addPath(path: string): Promise<void> {
  state.addError = null;
  try {
    // Opening it is what says whether it is a repository, so a folder that is
    // not one is refused here rather than added and struck through.
    await api.addRepository(path);
    await readLibrary();
    state.card = path;
  } catch (error) {
    state.addError = message(error);
  }
}

/// Something is over the window, or no longer is.
export function dropping(over: boolean): void {
  state.dragging = over;
}

/// Take a repository out of the list. Never off the disk.
export function forgetRepository(row: LibraryRow): void {
  ask(
    {
      title: t("ask.forget.title", { name: row.name }),
      detail: t("ask.forget.detail"),
      verb: t("ask.forget.verb"),
    },
    () => {
      void (async () => {
        await api.forgetRepository(row.path);
        // Its tab goes with it: a tab for a repository the window no longer
        // lists is a way back to something you just said you were done with.
        closeTab(row.path);
        if (state.open === row.path) state.open = null;
        await readLibrary();
      })();
    },
  );
}

// ── The folders (SPEC §11) ──────────────────────────────────────────────────
//
// The model has carried a group per repository since M3 and the list has drawn
// them since; what was missing was every way to make one. Each of these writes
// `repositories.toml` and then re-reads the arrangement rather than patching
// the local copy: the indices a row names are positions in the stored list, and
// a front end that guessed at them after a move would draw rows under folders
// they are not in until the next read.

/// Make a folder, and open its name for typing.
export async function createGroup(): Promise<void> {
  const at = await api.createGroup(t("library.newGroup"));
  await readArrangement();
  state.renamingGroup = at;
}

/// Begin, or abandon, typing a folder's name.
export function renameGroup(group: number | null): void {
  state.renamingGroup = group;
}

/// Settle it. A blank name is refused by the backend and leaves the old one,
/// so `Esc` and an emptied box do the same thing, which is what makes the box
/// safe to open on a folder that already has a name.
export async function setGroupName(group: number, name: string): Promise<void> {
  state.renamingGroup = null;
  if (name.trim() === "") return;
  await api.renameGroup(group, name);
  await readArrangement();
}

/// Fold a folder, or unfold it. Written out: someone who files forty
/// repositories into eight folders did it to keep seven of them shut.
export async function toggleGroup(group: number): Promise<void> {
  const held = state.groups[group];
  if (!held) return;
  // Locally first, so the chevron turns under the finger rather than after a
  // round trip through a file.
  held.collapsed = !held.collapsed;
  await api.collapseGroup(group, held.collapsed);
}

/// Take a folder away. What was in it is not.
///
/// The question is asked only when there is something to say — an empty folder
/// loses nothing, and a confirmation that answers no question is one people
/// learn to click through. When it holds repositories, the sentence's job is
/// to say they are kept, because "Delete" on a folder holding eight rows reads
/// as though it takes them.
export function removeGroup(group: number): void {
  const folder = state.groups[group];
  if (!folder || state.groups.length < 2) return;
  const inside = state.repositories.filter((row) => row.group === group).length;
  const go = () => {
    void (async () => {
      await api.removeGroup(group);
      if (state.renamingGroup === group) state.renamingGroup = null;
      await readArrangement();
    })();
  };
  if (inside === 0) {
    go();
    return;
  }
  // Where they go: the folder above, or the one below when this is the first.
  const host = state.groups[group === 0 ? 1 : group - 1];
  ask(
    {
      title: t("ask.removeGroup.title", { name: worded(folder.name) }),
      detail: t("ask.removeGroup.detail", {
        n: plural("library.repositories", inside),
        into: worded(host?.name ?? ""),
      }),
      verb: t("ask.removeGroup.verb"),
    },
    go,
  );
}

/// Reorder the folders.
export async function moveGroup(from: number, to: number): Promise<void> {
  if (from === to) return;
  await api.moveGroup(from, to);
  await readArrangement();
}

/// A drag begins, moves, or ends nowhere.
export function dragRepository(path: string | null): void {
  state.draggedRepository = path;
  if (path === null) state.dropAt = null;
}

export function dropTarget(at: { group: number; index: number } | null): void {
  state.dropAt = at;
}

/// Put one repository in one folder, at the end of it.
///
/// What the card's select calls, and the only way to file a repository that
/// does not need a pointer. The end rather than a position: choosing a folder
/// is one decision, and asking for a rank inside it in the same gesture would
/// be two.
export async function fileRepository(path: string, group: number): Promise<void> {
  const rows = state.repositories.filter((row) => row.group === group).length;
  await api.moveRepository(path, group, rows);
  await readArrangement();
}

/// File the dragged repository where the insertion line is.
export async function dropRepository(): Promise<void> {
  const path = state.draggedRepository;
  const at = state.dropAt;
  state.draggedRepository = null;
  state.dropAt = null;
  if (!path || !at) return;
  await api.moveRepository(path, at.group, at.index);
  await readArrangement();
}

export async function openRepository(path: string): Promise<void> {
  state.open = path;
  state.screen = "working-copy";
  // The tab is what makes the second repository reachable without going back
  // through the Dépôts screen. Added here rather than at the call sites, so
  // every way in — a click, a drop, the palette, `⏎` on a row — leaves one.
  if (!state.tabs.some((tab) => tab.path === path)) {
    state.tabs.push({ path, name: nameOf(path) });
  }
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
  state.stashes = idle();
  state.stash = null;
  state.stashFiles = idle();
  state.stashFile = null;
  state.stashing = null;
  state.sides = null;
  state.resolving = null;
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
    void readSides(summary.operation);
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

/// What a repository is called: the library's name for it, or the last segment
/// of its path when the library has never heard of it.
function nameOf(path: string): string {
  const row = state.repositories.find((entry) => entry.path === path);
  return row?.name ?? path.split("/").filter(Boolean).pop() ?? path;
}

/// Close a tab. The repository stays on the disk and in the list; what closes
/// is the window's hold on it.
///
/// Closing the one on screen moves to its left-hand neighbour — the tab you
/// were on before it, more often than not — and closing the last one goes back
/// to the list, which is the only honest place to be with no repository open.
export function closeTab(path: string): void {
  const at = state.tabs.findIndex((tab) => tab.path === path);
  if (at < 0) return;
  state.tabs.splice(at, 1);
  // Caught, not `void`ed: an unhandled rejection reaches `main.ts`, which draws
  // it over the whole window — and a backend that could not let go of a handle
  // is not a reason to lose the window you were working in.
  api
    .closeRepository(path)
    .catch((error) => api.log("warn", `close_repository: ${message(error)}`));
  if (state.open !== path) return;

  const next = state.tabs[at - 1] ?? state.tabs[at];
  if (next) {
    void openRepository(next.path);
    return;
  }
  state.open = null;
  state.summary = null;
  showScreen("repositories");
}

export async function selectFile(file: string, staged: boolean): Promise<void> {
  const path = state.open;
  if (!path) return;

  const moved = state.selected?.path !== file || state.selected.staged !== staged;
  if (moved) state.picked.clear();
  state.selected = { path: file, staged };
  // Another file's diff is not worth keeping on screen, but *this* file's is:
  // re-reading it after a write is the same pane saying the same thing, and
  // blanking it in between is the flicker.
  state.diff = moved ? { status: "loading" } : again(state.diff);

  const started = performance.now();
  try {
    const diff = await api.fileDiff(path, file, staged);
    // The selection may have moved while this was in flight. Dropping a stale
    // answer is the same rule the Rust side calls a generation check.
    if (state.selected?.path !== file || state.selected.staged !== staged) return;

    if (!diff) {
      state.diff = {
        status: "failed",
        error: t("diff.goneFromStatus"),
      };
      return;
    }
    const fetched = performance.now();
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural("diff.hunks", diff.hunks)}`,
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
  // Preferences belong to the application rather than to a repository, like
  // the Repositories screen itself: they are the one thing you may need before
  // you have opened anything — the theme is unreadable, or `git` is missing.
  if (screen !== "repositories" && screen !== "settings" && !state.open) {
    state.screen = "repositories";
    return;
  }
  if (screen === "settings" && state.preferences.status === "idle") void readPreferences();
  state.screen = screen;
  // History is read when it is first looked at rather than when a repository
  // opens: a walk of a hundred thousand commits is not what someone who wanted
  // to stage a file asked for.
  if (screen === "history" && state.history.status === "idle") void loadHistory();
  // Same rule as History: read when it is first looked at. A shelf is cheap to
  // read, but the reflog walk plus one object per entry is still work nobody
  // who came to stage a file asked for.
  else if (screen === "stashes" && state.stashes.status === "idle") void readStashes();
  // And the other half of it: the screen that opens takes the diff pane back.
  // It is one pane for three screens, so whichever was last to write into it
  // wins otherwise — a stash's diff under the Working Copy's file list.
  else void reopenDiff(screen);
}

/// Put the shared diff pane back on the file the screen being opened is about.
async function reopenDiff(screen: Screen): Promise<void> {
  if (screen === "history") {
    if (state.commitFile) await selectCommitFile(state.commitFile);
    return;
  }
  if (screen === "stashes") {
    if (state.stashFile) await selectStashFile(state.stashFile);
    return;
  }
  if (screen !== "working-copy") return;

  const rows = state.status.status === "ready" ? state.status.value : [];
  const held = state.selected;
  const row = held ? rows.find((entry) => entry.path === held.path) : rows[0];
  if (!row) {
    state.selected = null;
    state.diff = idle();
    return;
  }
  await selectFile(row.path, sideOf(row, held?.staged ?? row.staged !== null));
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
  // Choosing a commit ends a comparison. The pane cannot answer "what did this
  // change" and "what is between these two" at once, and it was trying: the
  // detail kept the comparison, two rows kept their A and B marks, and the diff
  // below showed a file belonging to neither — the commit that had just been
  // clicked. A plain click is the ordinary way back out, and it has to work
  // without finding the button that says so.
  state.compare = idle();
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
      state.diff = { status: "failed", error: t("diff.goneFromCommit") };
      return;
    }
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural("diff.hunks", diff.hunks)}`,
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
      state.diff = { status: "failed", error: t("diff.goneFromComparison") };
      return;
    }
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural("diff.hunks", diff.hunks)}`,
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
    a.branch === b.branch &&
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

/// Notice that the repository changed underneath us (SPEC §10).
///
/// A client that only refreshes when clicked is wrong every time you touch a
/// terminal — commit from the shell, switch a branch, run a formatter — and
/// that is what this window was: `omagit-git`'s watcher has been written since
/// M2, debounced and targeted, and nothing had ever called it.
///
/// Three rules on this side, and all three are about *not* re-reading:
///
/// * **only the repository on screen.** Every open tab is watched, because the
///   handle is what carries the watch, and a change in another one is not
///   something this screen can show;
/// * **not during our own write.** `settle()` re-reads at the end of one
///   anyway, and the events a `git commit` produces would otherwise land in the
///   middle of it;
/// * **status or refs, not everything.** The backend says which, and a branch
///   moving does not cost a status walk.
export async function watchRepository(): Promise<void> {
  await listen<Changed>("changed", (event) => {
    const { path, status, refs } = event.payload;
    if (path !== state.open || state.busy) return;
    if (status) void settle();
    else if (refs) void readRefs();
  });
}

/// Follow the system palette for as long as the window is open.
///
/// SPEC §6.1 makes the Omarchy palette a *live* source, not a value read once:
/// switch the system theme and every window that follows it turns with it. The
/// backend decides whether anything actually changed — with a theme chosen by
/// hand in preferences, nothing arrives here at all.
export async function watchTheme(): Promise<void> {
  await listen<string>("theme", (event) => {
    document.documentElement.setAttribute("style", event.payload);
    // The tokens carry the density and the scale, so the row heights the lists
    // measured against the old ones are stale the moment they change.
    measure();
  });
}

/// Run one network operation, with the overlay up for its duration.
async function overNetwork(what: string, run: () => Promise<string>): Promise<void> {
  if (state.running) return;
  state.running = { what, phase: "…", percent: null };
  state.stopping = false;
  state.notes = null;
  try {
    const said = await run();
    // Into the same place every other success goes. It had a field of its own,
    // `networkSaid`, which nothing ever rendered: a fetch, a pull and a push
    // all finished in silence, which on the operations that take the longest is
    // the worst place for it. One channel for what went right, one band for
    // what did not.
    state.notes = [t("said.done", { what }), said.trim() || t("said.nothing")].join("\n");
  } catch (error) {
    state.writeError = { what, said: message(error) };
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

/// Pull, asking how to reconcile only when nothing already says.
///
/// Since 2.27 `git pull` refuses on a diverged branch when neither
/// `pull.rebase`, `pull.ff` nor the branch's own setting answers, and it prints
/// twelve lines of hints whose every suggestion is a `git config` command —
/// none of which can be run from this window (there is no preferences screen
/// until M9). So the question is asked here, before the pull, and the answer
/// travels with that one pull: nothing is written to the configuration, because
/// what to do *every* time is a decision this app has no business taking for
/// somebody (§2.33).
export function pullRemote(): void {
  const path = state.open;
  if (!path) return;
  const tracking = state.summary?.tracking;
  const diverged = !!tracking && tracking.ahead > 0 && tracking.behind > 0;
  if (!diverged) {
    void overNetwork("Pull", () => api.pull(path));
    return;
  }

  void (async () => {
    let configured = true;
    try {
      configured = await api.pullReconcileConfigured(path);
    } catch (error) {
      // Unreadable configuration is not a reason to refuse to pull: let `git`
      // answer for itself, as it did before this question existed.
      void api.log("warn", `pull_reconcile_configured: ${message(error)}`);
    }
    if (state.open !== path) return;
    if (configured) {
      void overNetwork("Pull", () => api.pull(path));
      return;
    }

    ask(
      {
        title: t("ask.reconcile.title"),
        detail: t("ask.reconcile.detail", {
          branch: state.summary?.head ?? t("ask.thisBranch"),
          ahead: plural("ask.commits", tracking.ahead),
          behind: tracking.behind,
          upstream: tracking.upstream,
        }),
        verb: t("ask.reconcile.verb"),
        alternative: t("ask.reconcile.alternative"),
      },
      () => void overNetwork("Pull", () => api.pull(path, "merge")),
      () => void overNetwork("Pull", () => api.pull(path, "rebase")),
    );
  })();
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
      title: t("ask.force.title", { branch }),
      detail: t("ask.force.detail"),
      verb: t("ask.force.verb"),
    },
    run,
  );
}

// ── Cloning (M7) ────────────────────────────────────────────────────────────
//
// The one operation with no repository to start from, and the only one that
// needs a form. Board 07 draws it: URL, destination, group, two options, and a
// line saying whether the remote answered.

/// Where a clone goes when nobody has said otherwise.
///
/// `~/src`, which is board 06's own example. It does not have to exist — the
/// backend creates it — so proposing it for someone who has never had one is a
/// suggestion rather than a broken default.
function defaultParent(): string {
  const home = state.platform?.home ?? "";
  return home ? `${home}/src` : "";
}

/// Timers for the two things the URL field sets off. Module-level rather than
/// on the state: they are not something anything renders.
let namingTimer: ReturnType<typeof setTimeout> | undefined;
let probeTimer: ReturnType<typeof setTimeout> | undefined;

export function openClone(): void {
  if (state.clone) return;
  state.clone = {
    url: "",
    parent: defaultParent(),
    name: "",
    renamed: false,
    shallow: false,
    submodules: false,
    group: null,
    probe: "idle",
  };
  state.addError = null;
}

export function closeClone(): void {
  clearTimeout(namingTimer);
  clearTimeout(probeTimer);
  state.clone = null;
}

/// The URL changed: fill the folder in, and ask whether the remote is there.
///
/// Two timers rather than one because they answer at different costs. The name
/// is a string rule and can be asked for as fast as someone types; the probe
/// starts a `git` process that talks to a server, so it waits until the typing
/// has stopped.
export function setCloneUrl(url: string): void {
  const form = state.clone;
  if (!form) return;
  form.url = url;
  form.probe = "idle";
  clearTimeout(namingTimer);
  clearTimeout(probeTimer);

  namingTimer = setTimeout(() => {
    void (async () => {
      // Derived by the backend, using `git`'s own rule, rather than by a second
      // regular expression here that would be subtly different from it.
      const name = await api.cloneDirectory(url);
      const current = state.clone;
      if (!current || current.url !== url || current.renamed) return;
      current.name = name ?? "";
    })();
  }, 120);

  // Nothing to ask about half a URL. `git` would refuse it instantly, and a red
  // line under a field somebody is still typing into is noise.
  if (!looksLikeUrl(url)) return;
  probeTimer = setTimeout(() => {
    void (async () => {
      const current = state.clone;
      if (!current || current.url !== url) return;
      current.probe = "checking";
      try {
        await api.checkRemote(url);
        if (state.clone?.url === url) state.clone.probe = "reachable";
      } catch (error) {
        if (state.clone?.url === url) state.clone.probe = { error: message(error) };
      }
    })();
  }, 700);
}

/// Enough of a URL to be worth asking a server about.
///
/// Deliberately loose: it is a gate on wasting a network round trip, not a
/// validator. `git` accepts more forms than this recognises, and the ones it
/// misses simply get no probe — the clone still runs.
function looksLikeUrl(url: string): boolean {
  const trimmed = url.trim();
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) || /^[^/\s]+@[^/\s]+:.+/.test(trimmed);
}

export function setCloneName(name: string): void {
  if (!state.clone) return;
  state.clone.name = name;
  // From here the URL stops filling it in. Overwriting what someone typed is
  // the worst thing a helpful default can do.
  state.clone.renamed = true;
}

export function setCloneParent(parent: string): void {
  if (state.clone) state.clone.parent = parent;
}

export function setCloneGroup(group: number | null): void {
  if (state.clone) state.clone.group = group;
}

export function setCloneOption(option: "shallow" | "submodules", on: boolean): void {
  if (state.clone) state.clone[option] = on;
}

/// Choose the folder to clone into, through the platform's own picker.
export async function browseCloneParent(): Promise<void> {
  const chosen = await open({ directory: true, multiple: false, title: t("picker.cloneInto") });
  if (typeof chosen === "string") setCloneParent(chosen);
}

/// Whether the form has enough in it to run, and why not when it has not.
export function cloneBlocker(form: CloneForm): string | null {
  if (!form.url.trim()) return t("clone.needUrl");
  if (!form.parent.trim()) return t("clone.needParent");
  if (!form.name.trim()) return t("clone.needName");
  // A name with a separator in it would put the clone somewhere other than
  // where the destination line says.
  if (/[/\\]/.test(form.name.trim())) return t("clone.slashInName");
  return null;
}

/// Run the clone, then open what landed.
///
/// The dialog closes first. It has nothing left to show — the progress overlay
/// takes over — and leaving it up over a four-minute operation would be a form
/// nobody can use blocking a screen they can.
export function startClone(): void {
  const form = state.clone;
  if (!form || cloneBlocker(form)) return;
  const request = {
    url: form.url.trim(),
    parent: form.parent.trim(),
    name: form.name.trim(),
    shallow: form.shallow,
    submodules: form.submodules,
    group: form.group,
  };
  closeClone();

  void (async () => {
    if (state.running) return;
    state.running = { what: "Clonage", phase: "…", percent: null };
    state.stopping = false;
    state.notes = null;
    let landed: string | null = null;
    try {
      const summary = await api.cloneRepository(request);
      landed = summary.path;
      state.notes = t("said.cloned", { path: summary.path });
    } catch (error) {
      state.addError = message(error);
    } finally {
      state.running = null;
      state.stopping = false;
    }
    await readLibrary();
    // Straight into it. A clone that finished and left you looking at the same
    // list you started from has made you find it yourself.
    if (landed) await openRepository(landed);
  })();
}

/// Stop what is running. `git` stops when it next looks, which is not instant.
export function stopNetwork(): void {
  if (!state.running) return;
  state.stopping = true;
  void api.cancelOperation();
}

export function dismissAddError(): void {
  state.addError = null;
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
      title: t("ask.merge.title", { branch, into }),
      detail: t("ask.merge.detail"),
      verb: t("ask.merge.verb"),
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
      title: t("ask.rebase.title", { branch, onto }),
      detail: t("ask.rebase.detail"),
      verb: t("ask.rebase.verb"),
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
      title: t("ask.abort.title", { operation }),
      detail: t("ask.abort.detail"),
      verb: t("ask.abort.verb"),
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
  // The tree stays up while it is re-read. `settle()` calls this after every
  // write and after every change the watcher notices, and a branch column that
  // empties itself each time is a sidebar that jumps for a living.
  state.refs = again(state.refs);
  try {
    const refs = await api.refs(path);
    if (state.open !== path) return;
    state.refs = { status: "ready", value: refs };
  } catch (error) {
    state.refs = { status: "failed", error: message(error) };
  }
}

/// Show one branch's history, which is what clicking its row means.
///
/// Tower does this and it is the obvious reading of a click on a branch: a row
/// that did nothing at all — the checkout was on the *double* click and nothing
/// else answered — is a control that looks broken. Checking out stays where it
/// was: switching branches rewrites the working tree, and that is not what a
/// single click should do.
export function showBranchHistory(name: string): void {
  if (!state.open) return;
  state.screen = "history";
  void setQuery({ branch: name, all: false });
}

/// Back to the branch `HEAD` is on.
export function showHeadHistory(): void {
  void setQuery({ branch: "" });
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
  void write(t("do.createBranch", { name: trimmed }), () =>
    api.createBranch(path, trimmed, start.trim(), andSwitch),
  );
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
      title: t("ask.deleteBranch.title", { name: row.name }),
      detail: force ? t("ask.deleteBranch.unmerged") : t("ask.deleteBranch.merged"),
      verb: t("ask.deleteBranch.verb"),
    },
    () =>
      void write(t("ask.deleteBranch.verb") + ` ${row.name}`, () =>
        api.deleteBranch(path, row.name, force),
      ),
  );
}

// ── Conflicts (M8) ──────────────────────────────────────────────────────────

/// Name the two sides, while an operation is stopped on a conflict.
///
/// Asked for only when one is running: with nothing half-finished there are no
/// sides, and a call per settled write would be a read for an answer that is
/// always null.
async function readSides(operation: string | null): Promise<void> {
  const path = state.open;
  if (!path) return;
  if (!operation) {
    state.sides = null;
    return;
  }
  try {
    const sides = await api.conflictSides(path);
    if (state.open === path) state.sides = sides;
  } catch (error) {
    // The buttons fall back to the words `git` uses; not knowing which branch
    // is which is not a reason to hide the way out of a conflict.
    void api.log("warn", `conflict_sides: ${message(error)}`);
  }
}

/// Keep one side of a conflicted file whole.
///
/// Not confirmed, and the reason is worth writing down because it will be
/// revisited: both versions are in commits, so what this overwrites is the file
/// with its markers — nothing that is not somewhere else. What it *would*
/// overwrite is a resolution made by hand in the file, and nothing here can
/// tell yet whether one was: the dialog that can is the next slice.
export function resolveConflict(row: StatusRow, side: "ours" | "theirs"): void {
  const path = state.open;
  if (!path) return;
  const named = side === "ours" ? state.sides?.ours : state.sides?.theirs;
  void write(t("do.resolveSide", { file: row.path, side: named ?? side }), () =>
    api.resolveConflict(path, row.path, side),
  );
}

/// Carry on with the half-finished operation.
///
/// The way *forward*, next to the way out. `git` refuses while a path is still
/// unmerged and names it, so the button being disabled is a courtesy rather
/// than the check — the index is the truth, and this side holds a copy.
export function continueOperation(): void {
  const path = state.open;
  const operation = state.summary?.operation;
  if (!path || !operation) return;
  void write(`Poursuivre ${operation}`, async () => {
    state.notes = await api.continueOperation(path);
  });
}

/// Open board 07's dialog on one conflicted file.
export async function openConflict(file: string): Promise<void> {
  const path = state.open;
  if (!path) return;
  state.resolving = { file, body: { status: "loading" }, choices: [], at: 0 };
  try {
    const body = await api.conflictFile(path, file);
    if (state.resolving?.file !== file) return;
    state.resolving = {
      file,
      body: { status: "ready", value: body },
      choices: Array.from({ length: body.regions }, () => null),
      at: 0,
    };
  } catch (error) {
    if (state.resolving?.file === file) {
      state.resolving = {
        file,
        body: { status: "failed", error: message(error) },
        choices: [],
        at: 0,
      };
    }
  }
}

export function closeConflict(): void {
  state.resolving = null;
}

/// Answer one conflict, and move to the next one nobody has answered.
///
/// Moving is the point: a file with four conflicts is four decisions, and a
/// dialog that stayed on the one just settled would make the reader find the
/// next one themselves every time.
export function chooseSide(index: number, choice: Choice): void {
  const resolving = state.resolving;
  if (!resolving || index >= resolving.choices.length) return;
  resolving.choices[index] = choice;
  const next = resolving.choices.findIndex((answer) => answer === null);
  resolving.at = next < 0 ? index : next;
}

/// Board 07's `n`: the next conflict, wrapping round.
export function nextConflict(): void {
  const resolving = state.resolving;
  if (!resolving || resolving.choices.length === 0) return;
  resolving.at = (resolving.at + 1) % resolving.choices.length;
}

/// Whether every conflict in the file has an answer.
export function conflictSettled(): boolean {
  const resolving = state.resolving;
  return !!resolving && resolving.choices.every((answer) => answer !== null);
}

/// Write the answers into the file and stage it.
export function applyResolution(): void {
  const path = state.open;
  const resolving = state.resolving;
  if (!path || !resolving || !conflictSettled()) return;
  const { file, choices } = resolving;
  state.resolving = null;
  void write(t("do.resolve", { file }), () => api.resolveHunks(path, file, choices as Choice[]));
}

/// Hand the file to the editor and stand aside.
///
/// The dialog closes, because what it is showing is about to stop being true:
/// the file is now open somewhere else, and a panel still drawing the markers
/// as they were would be the second opinion nobody asked for.
export function openInEditor(): void {
  const path = state.open;
  const file = state.resolving?.file;
  if (!path || !file) return;
  state.resolving = null;
  void write(`Ouvrir ${file}`, async () => {
    // What was launched, which is not always what was configured: a terminal
    // editor started from a window with no terminal is a process nobody sees.
    state.notes = worded(await api.openInEditor(path, file));
  });
}

/// How many conflicted paths are left, which is what the way forward waits on.
export function conflictCount(): number {
  return state.status.status === "ready"
    ? state.status.value.filter((row) => row.conflict !== null).length
    : (state.summary?.counts.conflicted ?? 0);
}

// ── The shelf (M8) ──────────────────────────────────────────────────────────
//
// Every write here is addressed by commit, never by the index the row shows.
// `stash@{0}` is a position in a reflog, and dropping one renumbers everything
// below it: an index held on this side is a copy of a numbering that has
// already moved. The backend resolves the commit to the index it has *now*,
// under the same lock as the command.

export async function readStashes(): Promise<void> {
  const path = state.open;
  if (!path) return;
  // Same rule as the branch tree: `settle()` re-reads the shelf once it has
  // been looked at, and the list it is replacing is still the truth until the
  // new one arrives.
  state.stashes = again(state.stashes);
  try {
    const shelf = await api.stashes(path);
    if (state.open !== path) return;
    state.stashes = { status: "ready", value: shelf };

    // Stay on the entry being read when the list moves underneath, and fall to
    // the newest when it is gone — popped, dropped, or never there.
    const held = shelf.find((row) => row.id.full === state.stash);
    const first = shelf[0];
    if (!held && !first) {
      state.stash = null;
      state.stashFiles = idle();
      state.stashFile = null;
      state.diff = idle();
      return;
    }
    if (!held && first) await selectStash(first.id.full);
    else if (held && state.stashFiles.status === "idle") await selectStash(held.id.full);
  } catch (error) {
    state.stashes = { status: "failed", error: message(error) };
  }
}

/// What one entry holds. Its files, then the first of them.
export async function selectStash(id: string): Promise<void> {
  const path = state.open;
  if (!path) return;
  state.stash = id;
  state.stashFiles = { status: "loading" };
  state.stashFile = null;
  state.diff = idle();

  try {
    const files = await api.stashFiles(path, id);
    // The selection may have moved while this was in flight.
    if (state.open !== path || state.stash !== id) return;
    state.stashFiles = { status: "ready", value: files };
    const first = files[0];
    if (first) await selectStashFile(first.path);
  } catch (error) {
    if (state.stash === id) state.stashFiles = { status: "failed", error: message(error) };
  }
}

export async function selectStashFile(file: string): Promise<void> {
  const path = state.open;
  const id = state.stash;
  if (!path || !id) return;

  state.stashFile = file;
  state.diff = { status: "loading" };
  try {
    const diff = await api.stashFileDiff(path, id, file);
    if (state.stash !== id || state.stashFile !== file) return;
    if (!diff) {
      state.diff = { status: "failed", error: t("diff.goneFromStash") };
      return;
    }
    state.diff = {
      status: "ready",
      value: {
        path: diff.path,
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${plural("diff.hunks", diff.hunks)}`,
        rows: diff.rows ?? [],
        reason: diff.reason,
      },
    };
  } catch (error) {
    if (state.stashFile === file) state.diff = { status: "failed", error: message(error) };
  }
}

export function openStashForm(): void {
  state.stashing = { message: "", untracked: false };
}

export function closeStashForm(): void {
  state.stashing = null;
}

export function setStashMessage(text: string): void {
  if (state.stashing) state.stashing.message = text;
}

export function setStashUntracked(on: boolean): void {
  if (state.stashing) state.stashing.untracked = on;
}

/// Put the working copy on the shelf.
///
/// Not confirmed: what it takes off the working tree it stores, and the entry
/// is the first row of the list by the time the form closes. `git`'s own answer
/// is kept, because the one worth reading is the one it gives when there was
/// nothing to stash.
export function stashChanges(): void {
  const path = state.open;
  const form = state.stashing;
  if (!path || !form) return;
  const { message: text, untracked } = form;
  state.stashing = null;
  void write("Remiser", async () => {
    const said = await api.stashPush(path, text, untracked);
    // The one case where `git`'s own line is the whole answer: "No local
    // changes to save" is what happened, and no sentence of ours improves it.
    state.notes = said.includes("No local changes")
      ? said
      : [t("said.stashed"), said.trim()].filter(Boolean).join("\n");
  });
}

/// Bring one back: `keep` applies it and leaves it on the shelf, otherwise it
/// is popped off.
///
/// Neither is confirmed, and that is a decision rather than an omission. What
/// a pop takes off the shelf is in the working tree by the time it does, `git`
/// keeps the entry when the apply conflicts, and a confirmation on everything
/// is a confirmation on nothing — the one below has to mean something.
export function restoreStash(row: StashRow, keep: boolean): void {
  const path = state.open;
  if (!path) return;
  const where = address(row);
  void write(keep ? `Appliquer ${where}` : `Appliquer et retirer ${where}`, async () => {
    const said = await api.stashRestore(path, row.id.full, keep);
    // What `git stash apply` prints is a status — "On branch main" — which
    // answers a question nobody asked and looks, on a screen where the shelf
    // does not visibly move, exactly like a button that did nothing. The
    // outcome is said here in the app's own words; `git`'s own text follows on
    // the second line, and the exact command line is in the journal.
    state.notes = [
      keep ? t("said.applied", { stash: where }) : t("said.popped", { stash: where }),
      said.trim(),
    ]
      .filter(Boolean)
      .join("\n");
  });
}

/// Throw one away without applying it.
///
/// The destructive one, and the one that looks least like it: the working tree
/// does not move, so nothing on screen changes except a row disappearing.
export function dropStash(row: StashRow): void {
  const path = state.open;
  if (!path) return;
  ask(
    {
      title: t("ask.dropStash.title", { address: address(row) }),
      detail: t("ask.dropStash.detail", { message: row.message }),
      verb: t("ask.dropStash.verb"),
    },
    () =>
      void write(t("ask.dropStash.verb") + ` ${address(row)}`, async () => {
        const said = await api.stashDrop(path, row.id.full);
        state.notes = [t("said.dropped", { stash: address(row) }), said.trim()]
          .filter(Boolean)
          .join("\n");
      }),
  );
}

/// `stash@{0}` — what `git` calls it, and what the journal will show.
export function address(row: { index: number }): string {
  return `stash@{${row.index}}`;
}

/// Re-read everything the open screen shows.
///
/// The watcher does this on its own when the repository moves; this is the key
/// for when you want to be sure — after a rebase run in a terminal, or a
/// repository rebuilt underneath the window. On the Repositories screen there
/// is no open repository to re-read, so it re-reads the list instead: the same
/// key, the same meaning, on whatever is on screen.
export async function refresh(): Promise<void> {
  if (state.screen === "repositories" || !state.open) {
    await readLibrary();
    return;
  }
  await settle();
  if (state.screen === "history") await loadHistory();
}

// ── The command palette (M9) ────────────────────────────────────────────────
//
// Board 07 calls it "le point d'entrée principal de l'app". The rows it can
// reach are therefore not a decoration: what is not in here has to be found by
// knowing where it lives.

export function openPalette(): void {
  state.palette = { query: "", at: 0 };
}

export function closePalette(): void {
  state.palette = null;
}

export function setPaletteQuery(query: string): void {
  if (!state.palette) return;
  state.palette.query = query;
  // Back to the top: the row under the cursor was chosen against a list that no
  // longer exists, and ⏎ has to stay pressable without looking.
  state.palette.at = 0;
}

/// Move by one, wrapping. The list is short and the wrap is what makes ↑ from
/// the first row reach the last without a second key.
export function movePalette(by: number, total: number): void {
  if (!state.palette || total === 0) return;
  state.palette.at = (state.palette.at + by + total) % total;
}

/// Run one row. What that means depends on what the row is, which is why the
/// row says so on its right — "basculer ⏎" is not "ouvrir ⏎".
export function runPaletteRow(row: PaletteRow, actions: KeymapAction[]): void {
  if (!row.enabled) return;
  closePalette();
  switch (row.kind) {
    case "action":
      actions.find((action) => action.id === row.key)?.run();
      return;
    case "repository":
      void openRepository(row.key);
      return;
    case "branch":
      checkoutBranch(row.key);
      return;
    case "file":
      showScreen("working-copy");
      void selectFile(row.key, false);
      return;
  }
}

// ── Preferences (M9) ────────────────────────────────────────────────────────
//
// The screen that replaces editing `settings.toml` by hand. Every change is
// applied to the window in the same tick it is saved: the answer to each setter
// is the stylesheet it renders to, because a preference you cannot see the
// effect of is one nobody trusts — and because the alternative, re-asking for
// the theme afterwards, is two round trips where the second can fail on its own.

/// The `?` sheet: every binding, read from the table that answers them.
export function toggleShortcuts(): void {
  state.shortcuts = !state.shortcuts;
}

export function closeShortcuts(): void {
  state.shortcuts = false;
}

/// Choose the interface language, or `null` to follow the system's.
///
/// The catalogues are the front end's, so this is the whole of it: set what is
/// in force, and store the choice. Nothing is re-read — `t()` is reactive, so
/// every string on screen changes in the same tick.
export function chooseLanguage(tag: string | null): void {
  useLanguage(tag);
  void api.setLanguage(tag).catch((error) => api.log("warn", `set_language: ${message(error)}`));
}

/// What was chosen, `null` while following the system.
export function languagePreference(): string | null {
  return preference.value;
}

/// Give an action a different binding, or `null` to put back the table's own.
///
/// What may be bound is decided in `keymap.ts` — it has the table — and the
/// caller is expected to have asked it. This writes.
export function setBinding(id: string, chosen: string | null): void {
  void write(chosen ? t("do.binding", { binding: chosen }) : t("do.bindingDefault"), async () => {
    await api.setBinding(id, chosen);
    // The map, not a reassignment of the object: `state.keymap` is reactive and
    // everything printing a binding reads through it.
    if (chosen) state.keymap[id] = chosen;
    else delete state.keymap[id];
  });
}

export async function readPreferences(): Promise<void> {
  state.preferences = { status: "loading" };
  try {
    state.preferences = { status: "ready", value: await api.preferences() };
  } catch (error) {
    state.preferences = { status: "failed", error: message(error) };
  }
}

/// Apply a stylesheet the backend has just rendered.
///
/// `measure()` goes with it, always: the tokens carry the row heights, and a
/// list that kept measuring against the old ones would draw the wrong number of
/// rows into its viewport — which is how a density change used to leave a gap
/// at the bottom of every virtualised list.
function wear(sheet: string): void {
  document.documentElement.setAttribute("style", sheet);
  measure();
}

export function chooseTheme(source: string, name = ""): void {
  void write(t("do.theme", { name: name || source }), async () => {
    wear(await api.setTheme(source, name));
    await readPreferences();
  });
}

export function chooseDensity(density: "compact" | "comfortable"): void {
  void write(t("do.density", { density }), async () => {
    wear(await api.setDensity(density));
    await readPreferences();
  });
}

export function chooseScale(scale: number): void {
  void write(t("do.scale", { percent: Math.round(scale * 100) }), async () => {
    wear(await api.setScale(scale));
    await readPreferences();
  });
}

// ── Moving about (M9, DESIGN §5 and board 09) ───────────────────────────────
//
// One zone is one tab stop; `1` `2` `3` mean the sidebar, the centre column and
// the detail panel on every screen; movement inside a zone is `j`/`k` or the
// arrows; `Esc` goes up one level. What follows is that vocabulary, expressed
// against the state rather than against the DOM — the lists are virtualised, so
// the row the keyboard is on is often not rendered at all, and focus cannot be
// where the browser thinks it is.

/// One zone's list, as the keyboard sees it: how long, where the cursor is, how
/// to move it, and what `⏎` does.
type Walkable = {
  length: number;
  at: number;
  select: (at: number) => void;
  activate?: () => void;
};

/// The list the keyboard is walking, or `null` where a zone has none — the diff
/// panel on the Working Copy, which has its own line picking, and the card on
/// the Repositories screen, which is a reading surface rather than a list.
function walkable(): Walkable | null {
  const zone = state.zone;
  if (state.screen === "repositories") {
    if (zone !== 1) return null;
    const rows = visibleRepositories();
    return {
      length: rows.length,
      at: Math.max(
        0,
        rows.findIndex((row) => row.path === state.card),
      ),
      select: (at) => showCard(rows[at]!.path),
      activate: () => {
        const row = rows.find((entry) => entry.path === state.card);
        if (row && !row.missing) void openRepository(row.path);
      },
    };
  }

  if (zone === 1) {
    const branches = state.refs.status === "ready" ? state.refs.value.branches : [];
    if (branches.length === 0) return null;
    const at = Math.max(
      0,
      branches.findIndex((row) => row.name === state.branchCursor),
    );
    return {
      length: branches.length,
      at,
      // Moving does not walk a history: that is what `⏎` is for. A list whose
      // every keystroke started a walk of a hundred thousand commits would be
      // the filter box's old mistake in another place.
      select: (to) => {
        state.branchCursor = branches[to]!.name;
      },
      activate: () => {
        if (state.branchCursor) showBranchHistory(state.branchCursor);
      },
    };
  }

  if (state.screen === "working-copy") {
    if (zone !== 2) return null;
    const rows = state.status.status === "ready" ? state.status.value : [];
    if (rows.length === 0) return null;
    return {
      length: rows.length,
      at: Math.max(
        0,
        rows.findIndex((row) => row.path === state.selected?.path),
      ),
      select: (to) => void selectFile(rows[to]!.path, rows[to]!.staged !== null),
      activate: () => {
        const row = rows.find((entry) => entry.path === state.selected?.path);
        if (row) stageFile(row, row.staged !== null && row.unstaged === null);
      },
    };
  }

  if (state.screen === "history") {
    const rows = state.history.status === "ready" ? state.history.value : [];
    if (zone === 2) {
      if (rows.length === 0) return null;
      const open = state.commit.status === "ready" ? state.commit.value.id.full : null;
      return {
        length: rows.length,
        at: Math.max(
          0,
          rows.findIndex((row) => row.id.full === open),
        ),
        select: (to) => void selectCommit(rows[to]!.id.full),
      };
    }
    const files = state.commit.status === "ready" ? state.commit.value.files : [];
    if (files.length === 0) return null;
    return {
      length: files.length,
      at: Math.max(
        0,
        files.findIndex((file) => file.path === state.commitFile),
      ),
      select: (to) => void selectCommitFile(files[to]!.path),
    };
  }

  // The shelf.
  const shelf = state.stashes.status === "ready" ? state.stashes.value : [];
  if (zone === 2) {
    if (shelf.length === 0) return null;
    return {
      length: shelf.length,
      at: Math.max(
        0,
        shelf.findIndex((row) => row.id.full === state.stash),
      ),
      select: (to) => void selectStash(shelf[to]!.id.full),
    };
  }
  const files = state.stashFiles.status === "ready" ? state.stashFiles.value : [];
  if (files.length === 0) return null;
  return {
    length: files.length,
    at: Math.max(
      0,
      files.findIndex((file) => file.path === state.stashFile),
    ),
    select: (to) => void selectStashFile(files[to]!.path),
  };
}

/// The repositories the list is drawing, filter included.
export function visibleRepositories(): LibraryRow[] {
  const wanted = state.libraryFilter.trim().toLowerCase();
  if (wanted === "") return state.repositories;
  return state.repositories.filter(
    (row) =>
      row.name.toLowerCase().includes(wanted) ||
      row.path.toLowerCase().includes(wanted) ||
      row.description.toLowerCase().includes(wanted),
  );
}

export function setLibraryFilter(text: string): void {
  state.libraryFilter = text;
  // The card has to follow the list: one showing a repository the filter has
  // hidden is a panel about something nobody can see.
  const rows = visibleRepositories();
  if (!rows.some((row) => row.path === state.card)) state.card = rows[0]?.path ?? null;
}

/// Go to one of the three zones. A zone with nothing in it still takes the
/// keyboard: `3` on the Working Copy means "the diff", and the diff is there
/// even when no list is.
///
/// The *native* focus follows, in `App.vue`, which is the one place allowed to
/// touch the DOM for it: `1` `2` `3` are a shortcut through the tab order, so
/// the next `Tab` has to continue from where they landed rather than from
/// wherever focus was left.
export function goToZone(zone: Zone): void {
  state.zone = zone;
}

/// `j` `k`, and the arrows. Stops at the ends rather than wrapping: a list of
/// commits that jumped from the oldest back to the newest would lose the place
/// a reader was holding.
export function moveInZone(by: number): boolean {
  const list = walkable();
  if (!list) return false;
  const to = Math.min(Math.max(list.at + by, 0), list.length - 1);
  if (to !== list.at) list.select(to);
  return true;
}

/// The ends, for `g` and `G`.
export function jumpInZone(to: "start" | "end"): boolean {
  const list = walkable();
  if (!list) return false;
  list.select(to === "start" ? 0 : list.length - 1);
  return true;
}

/// `⏎` on whatever the keyboard is holding. Not every zone has an answer — a
/// commit is already open by the time it is selected — and a key that does
/// nothing there is better than one that invents something.
export function activateInZone(): boolean {
  const list = walkable();
  if (!list?.activate) return false;
  list.activate();
  return true;
}

/// `Esc`, one level at a time (DESIGN §5).
///
/// A filter first, because it is the thing most likely to be hiding what
/// somebody is looking for; then back to the sidebar. It never leaves the
/// repository: "up one level" is not "out".
export function escapeLevel(): boolean {
  if (state.screen === "repositories" && state.libraryFilter !== "") {
    setLibraryFilter("");
    return true;
  }
  if (state.screen === "history" && isFiltered(state.query)) {
    void clearFilters();
    return true;
  }
  if (state.zone !== 1) {
    state.zone = 1;
    return true;
  }
  return false;
}

/// Whether the keyboard is in this zone — what decides which selected row wears
/// the focus ring.
///
/// DESIGN §1 keeps the two apart: selected is a surface change and survives the
/// keyboard leaving; focus is a ring, and only one zone has it.
export function zoneActive(zone: Zone): boolean {
  return state.zone === zone;
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
    state.writeError = { what: label, said: message(error) };
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
    void readSides(summary.operation);
    void refreshJournal();
    // A checkout, a branch created or deleted: the tree is what changed.
    void readRefs();
    // Only once it has been looked at: a write on the Working Copy screen
    // cannot change the shelf, and reading it on every stage would be a reflog
    // walk per checkbox. Popping and dropping do change it, and they happen on
    // a screen that has read it.
    if (state.stashes.status !== "idle") void readStashes();

    const was = state.selected;

    // One diff pane is shared by three screens, and this is the Working Copy's
    // half of that. A write started from the sidebar — a stash applied, a
    // branch merged — settles the same way wherever the reader is standing, and
    // re-reading the working copy's file here would replace the stash's diff
    // they are looking at with a file from another screen.
    if (state.screen !== "working-copy") {
      // The selection is still dropped when the file has left the status, so
      // coming back does not open on a file that is gone.
      if (was && !rows.some((entry) => entry.path === was.path)) state.selected = null;
      return;
    }

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
  void write(
    unstage ? t("do.unstageFile", { file: row.path }) : t("do.stageFile", { file: row.path }),
    () => api.stage(path, row.path, { kind: "file" }, unstage),
  );
}

export function stageEverything(unstage: boolean): void {
  const path = state.open;
  if (!path) return;
  void write(unstage ? t("do.unstageAll") : t("do.stageAll"), () => api.stageAll(path, unstage));
}

export function stageHunk(hunk: number, unstage: boolean): void {
  const path = state.open;
  const file = state.selected?.path;
  if (!path || !file) return;
  void write(
    unstage ? t("do.unstageHunk", { n: hunk + 1 }) : t("do.stageHunk", { n: hunk + 1 }),
    () => api.stage(path, file, { kind: "hunks", hunks: [hunk] }, unstage),
  );
}

export function stagePicked(unstage: boolean): void {
  const path = state.open;
  const file = state.selected?.path;
  const lines = pickedLines();
  if (!path || !file || lines.length === 0) return;
  const what = plural("diff.picked", lines.length);
  void write(unstage ? t("do.unstageLines", { what }) : t("do.stageLines", { what }), () =>
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
let pendingAlternative: (() => void) | null = null;

function ask(question: Question, action: () => void, alternative?: () => void): void {
  state.question = question;
  pending = action;
  pendingAlternative = alternative ?? null;
}

export function answer(yes: boolean): void {
  const action = pending;
  pending = null;
  pendingAlternative = null;
  state.question = null;
  if (yes) action?.();
}

/// The second way of doing it, when the question offers one.
export function answerAlternative(): void {
  const action = pendingAlternative;
  pending = null;
  pendingAlternative = null;
  state.question = null;
  action?.();
}

/// What rejecting this row would actually do.
function discardWarning(row: StatusRow): string {
  if (row.unstaged === "untracked") {
    return t("ask.discardFile.untracked");
  }
  if (row.unstaged === "deleted") {
    return t("ask.discardFile.deleted");
  }
  return t("ask.discardFile.modified");
}

export function discardFile(row: StatusRow): void {
  const path = state.open;
  if (!path) return;
  ask(
    {
      title:
        row.unstaged === "deleted"
          ? t("ask.restoreFile.title", { file: row.path })
          : t("ask.discardFile.title", { file: row.path }),
      // Three different acts wear the same button, and the question has to say
      // which one it is. Rejecting an edit loses work; rejecting a deletion
      // *gives a file back*, and telling someone their work is about to be
      // lost when nothing is at stake is how a confirmation stops being read.
      detail: discardWarning(row),
      verb: row.unstaged === "deleted" ? t("ask.restoreFile.verb") : t("ask.discardFile.verb"),
    },
    () =>
      void write(t("do.discardFile", { file: row.path }), () =>
        api.discard(path, row.path, { kind: "file" }),
      ),
  );
}

export function discardHunk(hunk: number): void {
  const path = state.open;
  const file = state.selected?.path;
  if (!path || !file) return;
  ask(
    {
      title: t("ask.discardHunk.title", { n: hunk + 1, file }),
      detail: t("ask.discardLines.detail"),
      verb: t("ask.discardFile.verb"),
    },
    () =>
      void write(t("do.discardHunk", { n: hunk + 1 }), () =>
        api.discard(path, file, { kind: "hunks", hunks: [hunk] }),
      ),
  );
}

export function discardPicked(): void {
  const path = state.open;
  const file = state.selected?.path;
  const lines = pickedLines();
  if (!path || !file || lines.length === 0) return;
  const what = plural("diff.picked", lines.length);
  ask(
    {
      title: t("ask.discardLines.title", { file }),
      detail: t("ask.discardLines.detail"),
      verb: t("ask.discardFile.verb"),
    },
    () =>
      void write(t("do.discardLines", { what }), () =>
        api.discard(path, file, { kind: "lines", lines }),
      ),
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
    void api.log("warn", `head_message: ${message(error)}`);
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
        title: t("ask.amend.title"),
        detail: t("ask.amend.detail"),
        verb: t("ask.amend.verb"),
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

/// How many files the working copy has anything to say about.
///
/// One per row, not one per side. The sidebar was adding the two sides
/// together, so a file that is half in the index — staged, then edited again —
/// counted twice: "Working Copy 8" over a list of seven. The two sides belong
/// in the status bar, which is about the index; the sidebar's number answers
/// "how many files need me", and that is a count of files.
export function changedCount(): number {
  return state.status.status === "ready" ? state.status.value.length : 0;
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

function firstLine(text: string): string {
  return text.split("\n", 1)[0] ?? "";
}

function message(error: unknown): string {
  return worded(error instanceof Error ? error.message : String(error));
}
