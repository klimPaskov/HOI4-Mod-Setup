import { defineConfig, devices } from "@playwright/test";

// The built web edition in dist-web/, served at its GitHub Pages path, in
// the three browser engines it supports.
export default defineConfig({
  testDir: "./tests/web",
  timeout: 240_000,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  use: {
    baseURL: "http://127.0.0.1:4173/HOI4-Mod-Setup/app/",
    viewport: { width: 1280, height: 900 },
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 900 } } },
    { name: "firefox", use: { ...devices["Desktop Firefox"], viewport: { width: 1280, height: 900 } } },
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 900 } } },
  ],
  webServer: {
    command: "node node_modules/vite/bin/vite.js preview --config vite.web.config.ts --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173/HOI4-Mod-Setup/app/",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
