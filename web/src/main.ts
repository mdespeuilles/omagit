// The window's entry point.

import { createApp } from "vue";
import App from "./App.vue";
import { api } from "./ipc";
import { installMenu } from "./menu";
import { boot, watchProgress, watchTheme } from "./state";

const app = createApp(App);

// A front end that fails silently shows a blank window, which SPEC §10 forbids
// as plainly as it forbids a spinner over everything. Whatever goes wrong is
// drawn, and told to the Rust log — the file anyone will actually read, rather
// than a devtools console nobody has open.
app.config.errorHandler = (error) => report(error);
window.addEventListener("unhandledrejection", (event) => report(event.reason));

function report(error: unknown): void {
  const text = error instanceof Error ? `${error.message}\n${error.stack ?? ""}` : String(error);
  void api.log("error", text);
  const pre = document.createElement("pre");
  pre.className = "fatal mono";
  pre.textContent = text;
  document.body.append(pre);
}

app.mount("#app");

// The menu bar needs the platform, which `boot` is what asks for — so unlike
// the two watchers below, this one waits for it (SPEC §9). Its failure is
// logged rather than drawn: a window without a menu bar is a window missing a
// menu bar, not a window that failed to open, and `report` paints over the app.
boot()
  .then(() =>
    installMenu().catch((error) =>
      api.log("warn", `barre de menus indisponible : ${String(error)}`),
    ),
  )
  .catch(report);
// Independent of `boot`: progress belongs to the window's lifetime, not to a
// repository's, and a failure to subscribe must not stop the app from opening.
watchProgress().catch(report);
// Same reasoning, and the same independence: the system palette can change
// while the window is open, and SPEC §6.1's second source is a live one.
watchTheme().catch(report);
