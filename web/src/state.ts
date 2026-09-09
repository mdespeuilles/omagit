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
import { api, type DiffRow, type LibraryRow, type PlatformFacts, type StatusRow } from "./ipc";

/// The four states of SPEC §10, rendered explicitly and never collapsed into a
/// spinner over everything or a blank panel.
export type Async<T> =
  | { status: "idle" }
  | { status: "loading"; previous?: T }
  | { status: "ready"; value: T }
  | { status: "failed"; error: string };

export const idle = <T>(): Async<T> => ({ status: "idle" });

export type Screen = "repositories" | "working-copy" | "history";

type State = {
  platform: PlatformFacts | null;
  /// Why writing is impossible, when it is. Raised before anything is typed
  /// rather than after (SPEC §8).
  gitUnusable: string | null;
  repositories: LibraryRow[];
  screen: Screen;
  /// The repository the window is looking at, by path.
  open: string | null;
  status: Async<StatusRow[]>;
  /// Which file the diff panel is about, and on which side of the index.
  selected: { path: string; staged: boolean } | null;
  diff: Async<{ header: string; rows: DiffRow[] }>;
};

const state = reactive<State>({
  platform: null,
  gitUnusable: null,
  repositories: [],
  screen: "repositories",
  open: null,
  status: idle(),
  selected: null,
  diff: idle(),
});

export const app = readonly(state);

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
  state.status = { status: "loading" };

  try {
    const rows = await api.status(path);
    state.status = { status: "ready", value: rows };
    // Open on a file rather than on an empty panel: the first row is what the
    // reader is going to click anyway.
    const first = rows[0];
    if (first) await selectFile(first);
  } catch (error) {
    state.status = { status: "failed", error: message(error) };
  }
}

export async function selectFile(row: StatusRow): Promise<void> {
  const path = state.open;
  if (!path) return;

  const staged = row.staged !== null;
  state.selected = { path: row.path, staged };
  state.diff = { status: "loading" };

  const started = performance.now();
  try {
    const diff = await api.fileDiff(path, row.path, staged);
    // The selection may have moved while this was in flight. Dropping a stale
    // answer is the same rule the Rust side calls a generation check.
    if (state.selected?.path !== row.path) return;

    if (!diff) {
      state.diff = { status: "failed", error: "ce fichier n'est plus dans le statut" };
      return;
    }
    const fetched = performance.now();
    state.diff = {
      status: "ready",
      value: {
        header: `${diff.path} · +${diff.added} −${diff.removed} · ${diff.hunks} blocs`,
        rows: diff.rows ?? [],
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
    if (state.selected?.path === row.path) {
      state.diff = { status: "failed", error: message(error) };
    }
  }
}

export function showScreen(screen: Screen): void {
  state.screen = screen;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
