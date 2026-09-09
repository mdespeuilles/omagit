// The window's entry point.
//
// M6b, first slice: prove the stack end to end — the theme crosses, a
// repository opens, its status lists, and a file's diff renders virtualised.
// That last one is the whole reason this slice exists: it is the measurement
// the stack change was decided without (SPEC §4, amended), and it has to be
// made on Linux and WebKitGTK.

import { api, type DiffRow, type PlatformFacts, type StatusRow } from "./ipc";
import { VirtualList } from "./virtual-list";

const app = document.querySelector<HTMLElement>("#app")!;

async function start(): Promise<void> {
  // The tokens first: everything below is drawn with them, and a frame drawn
  // before they arrive would flash the browser's defaults.
  document.documentElement.setAttribute("style", await api.theme());

  const [facts, unusable, repositories] = await Promise.all([
    api.platform(),
    api.gitStatus(),
    api.repositories(),
  ]);

  app.replaceChildren(
    topbar(facts, unusable),
    shell(repositories.map((row) => row.path)),
  );

  if (repositories[0]) void openRepository(repositories[0].path);
}

function topbar(facts: PlatformFacts, unusable: string | null): HTMLElement {
  const bar = element("header", "topbar");
  // A reserve, not a padding (DESIGN-TOKENS §9): the macOS traffic lights are
  // drawn over these pixels by the system, and a spacer means nothing else
  // shifts when the platform changes.
  if (facts.reserve.leading > 0) {
    const reserve = element("span", "reserve");
    reserve.style.width = `${facts.reserve.leading}px`;
    bar.append(reserve);
  }
  bar.append(
    element("span", "topbar-name mono", "omagit"),
    element("span", "topbar-sep", "│"),
    element("span", "topbar-crumb", "Dépôts"),
  );
  if (unusable) {
    // SPEC §8: an app that only discovers `git` is missing when someone presses
    // Commit has already let them write the message.
    bar.append(element("span", "banner danger", `git indisponible — ${unusable}`));
  }
  return bar;
}

let files: VirtualList<StatusRow>;
let diffRows: VirtualList<DiffRow>;
let openPath = "";

function shell(paths: string[]): HTMLElement {
  const body = element("div", "shell");

  const sidebar = element("nav", "sidebar");
  for (const path of paths) {
    const row = element("button", "sidebar-row", path.split("/").pop() ?? path);
    row.addEventListener("click", () => void openRepository(path));
    sidebar.append(row);
  }

  files = new VirtualList<StatusRow>(
    (node, row) => {
      node.className = "vlist-row file-row";
      node.replaceChildren(
        element("span", "file-code mono", row.code),
        element("span", "file-path mono", row.path),
      );
      node.onclick = () => void showDiff(row);
    },
    { rowHeight: 24 },
  );

  diffRows = new VirtualList<DiffRow>(drawDiffRow, { rowHeight: 18 });

  const middle = element("div", "files");
  middle.append(element("div", "pane-head", "STATUS"), files.element);

  const right = element("div", "diff");
  right.append(element("div", "pane-head", "—"), diffRows.element);

  body.append(sidebar, middle, right);
  return body;
}

async function openRepository(path: string): Promise<void> {
  openPath = path;
  const [summary, status] = await Promise.all([api.summary(path), api.status(path)]);
  document.title = `omagit — ${summary.name}`;
  files.setItems(status);
  diffRows.setItems([]);
  // Open on a file rather than on an empty panel: the first row is what the
  // reader is going to click anyway, and an empty pane says nothing about the
  // repository.
  if (status[0]) void showDiff(status[0]);
}

async function showDiff(row: StatusRow): Promise<void> {
  const staged = row.staged !== null;
  const started = performance.now();
  const diff = await api.fileDiff(openPath, row.path, staged);
  const fetched = performance.now();

  const head = document.querySelector<HTMLElement>(".diff .pane-head")!;
  if (!diff) {
    head.textContent = "—";
    diffRows.setItems([]);
    return;
  }
  head.textContent = `${diff.path} · +${diff.added} −${diff.removed} · ${diff.hunks} blocs`;
  diffRows.setItems(diff.rows ?? []);

  // The number the port was decided without, now measured on every diff. It
  // goes to the Rust log rather than the webview console, because that is the
  // file anyone will actually read — and because the run that matters is on
  // Linux, over WebKitGTK.
  void api.log(
    "info",
    `diff ${diff.path}: ${diff.rows?.length ?? 0} lignes · IPC ${(fetched - started).toFixed(1)}ms` +
      ` · rendu ${(performance.now() - fetched).toFixed(1)}ms`,
  );
}

function drawDiffRow(node: HTMLElement, row: DiffRow): void {
  if (row.kind === "header") {
    node.className = "vlist-row diff-header mono";
    node.textContent = row.text;
    return;
  }
  if (row.kind === "fold") {
    node.className = "vlist-row diff-fold mono";
    node.textContent = `⌄ ${row.lines} lignes de contexte repliées`;
    return;
  }
  node.className = `vlist-row diff-line mono ${row.side}`;
  node.replaceChildren(
    element("span", "gutter", row.old?.toString() ?? ""),
    element("span", "gutter", row.new?.toString() ?? ""),
    element("span", "sign", row.side === "added" ? "+" : row.side === "removed" ? "−" : " "),
    element("span", "text", row.text),
  );
}

function element(tag: string, className: string, text?: string): HTMLElement {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// A front end that fails silently shows a blank window, which SPEC §10 forbids
// as plainly as it forbids a spinner over everything. Whatever goes wrong at
// start-up is drawn.
start().catch((error: unknown) => {
  const text = error instanceof Error ? `${error.message}\n\n${error.stack ?? ""}` : String(error);
  const pre = document.createElement("pre");
  pre.className = "fatal mono";
  pre.textContent = `omagit n'a pas pu démarrer\n\n${text}`;
  app.replaceChildren(pre);
});

window.addEventListener("error", (event) => {
  console.error("uncaught", event.error);
});
