import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/browser",
  timeout: 30_000,
  workers: 2,
  use: {
    baseURL: "http://127.0.0.1:1421",
    viewport: { width: 1280, height: 960 },
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    launchOptions: process.env.HOI4_TEST_BROWSER_PATH
      ? { executablePath: process.env.HOI4_TEST_BROWSER_PATH }
      : {},
  },
  webServer: {
    command: "node scripts/start_browser_fixture.mjs",
    url: "http://127.0.0.1:1421",
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
