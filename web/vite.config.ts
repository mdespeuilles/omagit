import vue from "@vitejs/plugin-vue";
import { defineConfig, type Plugin } from "vite";

/// Vite marks its module script and stylesheet `crossorigin`, which asks for a
/// CORS-mode fetch. Tauri serves them from `tauri://localhost`, a protocol with
/// no CORS headers to satisfy it, so the module never runs and the window stays
/// blank without saying why.
function stripCrossorigin(): Plugin {
  return {
    name: "omagit-strip-crossorigin",
    enforce: "post",
    transformIndexHtml: (html) => html.replaceAll(" crossorigin", ""),
  };
}

export default defineConfig({
  plugins: [vue(), stripCrossorigin()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  // Every Tauri module the front end imports, named rather than discovered.
  //
  // Vite pre-bundles a dependency the first time it sees an import of it. A
  // module imported by a file written *after* the dev server started is found
  // late, and until the server has re-optimised, the import 404s — which in a
  // web view is a module that never runs and a window that shows nothing. It
  // cost a start-up failure the day `drop.ts` added the first import of
  // `@tauri-apps/api/webview`. Listing them makes a cold start prepare all of
  // them, and doubles as the record of what the front end asks of Tauri.
  optimizeDeps: {
    include: [
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
      "@tauri-apps/api/webview",
      "@tauri-apps/plugin-dialog",
    ],
  },
  build: { target: "safari15", sourcemap: true },
});
