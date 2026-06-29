import { defineConfig } from "vitest/config";

// Vitest owns the node WS e2e under tests/. Playwright owns the browser specs
// under playwright/ (see playwright.config.ts) — keep them from picking up each
// other's files.
export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
  },
});
