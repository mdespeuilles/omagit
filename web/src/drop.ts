// A folder dropped on the window is a repository added (§5, sixteenth defect).
//
// M3 accepted a drop and the port to Tauri did not carry it over, which left
// the library with exactly one door: the platform's open panel. That is not a
// small gap — the macOS panel hides `/var/folders/…`, so a repository built
// under `$TMPDIR` could not be added at all, and the fixture script had to move
// to `~` because of it.
//
// The webview's own event, not the DOM's: a browser drop hands over file
// *contents*, and what is wanted here is the path on disk. Tauri answers with
// the paths, which is the whole reason this is not `@dragover`/`@drop` on the
// shell.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { addRepositories, dropping, showScreen } from "./state";

/// Listen for as long as the window is open.
///
/// Returns nothing to unsubscribe with: there is one window, and it ends with
/// the process.
export async function watchDrops(): Promise<void> {
  await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "over") return dropping(true);
    if (event.payload.type === "leave") return dropping(false);
    if (event.payload.type !== "drop") return;
    dropping(false);
    const paths = event.payload.paths;
    if (paths.length === 0) return;
    // The list is where they land, so that is where the window goes: a drop
    // that added a row on a screen showing something else would look like a
    // drop that did nothing.
    showScreen("repositories");
    void addRepositories(paths);
  });
}
