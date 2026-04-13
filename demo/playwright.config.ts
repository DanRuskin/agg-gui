import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  retries: 0,
  timeout: 30_000,

  use: {
    baseURL: "http://localhost:3001",
    // Headless Chromium for speed — no cross-browser needed.
    ...devices["Desktop Chrome"],
    headless: true,
  },

  // Single project: headless Chromium only.
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  // Start the dev server automatically if not already running.
  webServer: {
    command: "bun run dev",
    port: 3001,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
