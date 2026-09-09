// The front-end test harness.
//
// SPEC §13's amendment is blunt about why this exists: every interface bug this
// project shipped went through the hole where there was no way to drive the UI
// without a person looking at it. The GPUI build had `TestAppContext`; the web
// build has this.
//
// Kept apart from `vite.config.ts` so the app's build config stays about the
// app.
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    include: ["src/**/*.test.ts"],
  },
});
