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
  build: { target: "safari15", sourcemap: true },
});
