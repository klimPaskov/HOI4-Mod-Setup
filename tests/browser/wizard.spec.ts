import { test, expect } from "@playwright/test";
import { DOCUMENTATION_SCENARIOS } from "../../src/documentation-scenarios";
import { resolve } from "node:path";

const screenshotNames: Record<string, string> = {
  welcome: "01-welcome.jpg", description: "02-description.png", identity: "03-identity.png",
  components: "04-components.png", workflows: "05-integrations.png", mcp: "06-mcp-credentials.png",
  git: "07-git.png", ready: "08-ready.png", "dry-run": "09-dry-run.png",
  provider: "10-provider-selection.png", existing: "11-existing-project.png", maintenance: "12-maintenance.png",
  recovery: "14-recovery.png", environments: "15-coding-environments.png",
  "image-generation": "16-image-generation.png",
  "live-testing": "17-live-testing.png",
};

// Browser coverage uses synthetic, development-only state. Native commands,
// filesystem mutation and provider authentication are covered separately.
for (const scenario of DOCUMENTATION_SCENARIOS) {
  test(`${scenario} renders without runtime errors or horizontal overflow`, async ({ page }, testInfo) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`/?screenshot=${scenario}`, { waitUntil: "domcontentloaded" });
    await expect(page.locator("#screen-title")).toBeVisible();
    await expect(page.locator("#screen-title")).not.toBeEmpty();
    await page.screenshot({ path: testInfo.outputPath(`${scenario}.png`), fullPage: true });
    if (process.env.HOI4_UPDATE_SCREENSHOTS === "1") {
      await page.screenshot({ path: resolve("docs/screenshots", screenshotNames[scenario]), fullPage: false });
    }
    for (const width of [1280, 640]) {
      await page.setViewportSize({ width, height: 960 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await expect(page.locator("#screen-title")).toBeVisible();
    }
    expect(errors).toEqual([]);
  });
}

test("setup can run without a setup assistant, clearly labelled", async ({ page }) => {
  await page.goto("/?screenshot=welcome");
  await page.getByLabel("AI provider", { exact: true }).selectOption("none");
  await expect(page.getByRole("note", { name: "Setup without an assistant" })).toContainText("Nothing is sent to an AI provider.");
  await expect(page.getByLabel("Model", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Continue", exact: true })).toBeEnabled();
});

test("existing-project management remains available while signed out", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Manage an existing project", exact: true }).click();
  await expect(page.getByLabel("Project folder", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Browse project folder", exact: true })).toBeEnabled();
});

test("Claude offers the verified catalog with Haiku 5.5 as the default", async ({ page }) => {
  await page.goto("/?screenshot=welcome");
  await expect(page.getByRole("combobox", { name: "Model", exact: true })).toHaveValue("claude-haiku-5-5");
  await expect(page.getByRole("combobox", { name: "Model", exact: true }).locator("option")).toHaveCount(14);
  await page.getByRole("combobox", { name: "Model", exact: true }).selectOption("claude-sonnet-5-5");
  await expect(page.getByRole("combobox", { name: "Reasoning effort" })).toHaveValue("medium");
});

for (const [id, label] of [["gemini_account", "Gemini"], ["grok_account", "Grok"]]) {
  test(`${label} account choice exposes sign-in without an API-key field`, async ({ page }) => {
    await page.goto("/?screenshot=welcome");
    await page.getByLabel("AI provider").selectOption(id);
    await expect(page.getByRole("group", { name: `${label} account access` })).toBeVisible();
    await expect(page.getByRole("button", { name: `Sign in to ${label}`, exact: true })).toBeVisible();
    await expect(page.locator('input[type="password"]')).toHaveCount(0);
    await page.setViewportSize({ width: 640, height: 960 });
    await expect(page.getByRole("button", { name: `Sign in to ${label}`, exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  });
}

test("each coding environment can be primary without becoming an additional choice", async ({ page }) => {
  await page.goto("/?screenshot=environments");
  const radios = page.getByRole("radio");
  await expect(radios).toHaveCount(5);
  for (let index = 0; index < 5; index += 1) {
    await radios.nth(index).check();
    await expect(radios.nth(index)).toBeChecked();
    await expect(page.getByRole("checkbox")).toHaveCount(4);
  }
  await radios.first().focus();
  await page.keyboard.press("ArrowRight");
  await expect(radios.nth(1)).toBeChecked();
});

test("image-generation setup displays provider costs and lets the user add another route", async ({ page }) => {
  await page.goto("/?screenshot=image-generation");
  await expect(page.getByRole("heading", { name: "Image generation" })).toBeVisible();
  await expect(page.getByText(/No separate image API bill/)).toBeVisible();
  const subscription = page.getByRole("radio", { name: /ChatGPT subscription \(GPTImage\)/ });
  const openAi = page.getByRole("radio", { name: /OpenAI Images API/ });
  await expect(subscription).toBeChecked();
  await expect(page.locator(".image-provider-card").nth(1).locator("input[type=checkbox]")).toBeChecked();
  await expect(page.getByText("Transparent images")).toBeVisible();
  await openAi.check();
  await expect(openAi).toBeChecked();
  await expect(subscription).not.toBeChecked();
  await page.getByLabel("Add as an optional provider", { exact: true }).first().uncheck();
  await expect(page.getByLabel("Add as an optional provider", { exact: true }).first()).not.toBeChecked();
});

test("maintenance repair sends the changed image provider in its reviewed plan request", async ({ page }) => {
  await page.addInitScript(() => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    (window as unknown as Record<string, unknown>).__HOI4_TEST_COMMANDS__ = calls;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: Record<string, unknown>) => {
        calls.push({ command, args });
        if (command === "scan_project") return { scan_id: "maintenance-scan", project_root: args.root, findings: [], conflicts: [], partial: false, cancelled: false, limits_hit: [], completed_at: "2026-10-14T00:00:00Z" };
        if (command === "build_maintenance_plan") throw "captured by browser test";
        return null;
      },
    };
  });
  await page.goto("/?screenshot=maintenance");
  const openAi = page.getByRole("radio", { name: /OpenAI Images API/ });
  await openAi.check();
  await expect(openAi).toBeChecked();

  await page.getByRole("button", { name: "Repair installation" }).click();
  await expect(page.getByRole("alert")).toContainText("captured by browser test");

  const call = await page.evaluate(() =>
    (window as unknown as { __HOI4_TEST_COMMANDS__: Array<{ command: string; args: Record<string, unknown> }> })
      .__HOI4_TEST_COMMANDS__
      .find((entry) => entry.command === "build_maintenance_plan"),
  );
  expect(call?.args.mode).toBe("repair");
  const order = await page.evaluate(() => (window as unknown as { __HOI4_TEST_COMMANDS__: Array<{ command: string }> }).__HOI4_TEST_COMMANDS__.map((entry) => entry.command));
  expect(order.indexOf("scan_project")).toBeLessThan(order.indexOf("build_maintenance_plan"));
  expect(call?.args.imageGeneration).toMatchObject({
    primary_provider: "openai_api",
    additional_providers: [],
  });
});

test("pre-apply recovery presents details and safe choices", async ({ page }) => {
  await page.goto("/?screenshot=recovery");
  await expect(page.locator("#screen-title")).toBeVisible();
  await page.getByText("Details", { exact: true }).click();
  await expect(page.getByText("Launcher descriptor path did not match the selected project root.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Undo", exact: true })).toHaveCount(0);
});


test("live testing follows workflows and disabling it advances past it", async ({ page }) => {
  await page.goto("/?screenshot=workflows");
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await expect(page.locator("#screen-title")).toHaveText("Live testing");
  await page.getByRole("radio", { name: /^None/ }).check();
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await expect(page.locator("#screen-title")).not.toHaveText("Live testing");
});

test("the integrations review is skipped in both directions when nothing needs input", async ({ page }) => {
  await page.goto("/?screenshot=git");
  await expect(page.locator("#screen-title")).toHaveText("Choose Git setup");
  await page.getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.locator("#screen-title")).toHaveText("Optional workflows");
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await expect(page.locator("#screen-title")).toHaveText("Choose Git setup");
});

for (const platform of ["windows", "macos", "linux"] as const) {
test(`maintenance sends the changed live testing provider on ${platform}`, async ({ page }) => {
  await page.addInitScript((selectedPlatform) => Object.defineProperty(navigator, "userAgent", { get: () => selectedPlatform === "macos" ? "Macintosh Mac OS X" : selectedPlatform === "linux" ? "X11; Linux x86_64" : "Windows NT 10.0" }), platform);
  await page.addInitScript(() => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    (window as unknown as Record<string, unknown>).__HOI4_TEST_COMMANDS__ = calls;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = { invoke: async (command: string, args: Record<string, unknown>) => { calls.push({ command, args }); if (command === "scan_project") return { scan_id: "maintenance-scan", project_root: args.root, findings: [], conflicts: [], partial: false, cancelled: false, limits_hit: [], completed_at: "2026-10-14T00:00:00Z" }; if (command === "build_maintenance_plan") throw "captured live testing request"; return null; } };
  });
  await page.goto("/?screenshot=maintenance");
  const isMac = await page.evaluate(() => /macintosh|mac os x/i.test(navigator.userAgent));
  const liveProviders = page.getByRole("radiogroup", { name: "Live testing provider" });
  if (isMac || platform === "linux") {
    await liveProviders.getByRole("radio", { name: /^None/ }).click();
    await expect(liveProviders).toHaveCount(0);
  } else {
    await liveProviders.getByRole("radio", { name: /HOI4 Live Tools \(experimental\)/ }).check();
  }
  await page.getByRole("button", { name: "Repair installation" }).click();
  await expect(page.getByRole("alert")).toContainText("captured live testing request");
  const call = await page.evaluate(() => (window as unknown as { __HOI4_TEST_COMMANDS__: Array<{ command: string; args: Record<string, unknown> }> }).__HOI4_TEST_COMMANDS__.find((entry) => entry.command === "build_maintenance_plan"));
  expect(call?.args.liveTesting).toEqual({ provider: isMac || platform === "linux" ? "none" : "hoi4_live_tools", linux_ready: false, wsl_distribution: "", ssh_host: "" });
});
}

test("Linux Clausewitz is native and needs no remote environment fields", async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "userAgent", { get: () => "Mozilla/5.0 (X11; Linux x86_64)" }));
  await page.goto("/?screenshot=live-testing");
  await expect(page.getByRole("radio", { name: /^Clausewitz MCP/ })).toBeChecked();
  await expect(page.getByRole("radio", { name: /HOI4 Live Tools/ })).toBeDisabled();
  await expect(page.getByText(/Clausewitz MCP runs directly on Linux/)).toBeVisible();
  await page.screenshot({ path: "artifacts/linux-live-testing.png" });
  await expect(page.getByRole("textbox", { name: /SSH host|WSL distribution/ })).toHaveCount(0);
  await page.getByRole("checkbox", { name: /I confirm a compatible Linux game/ }).check();
  await expect(page.getByRole("button", { name: "Next", exact: true })).toBeEnabled();
});

test("review captures can show coding environments with no client preselected", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.goto("/?screenshot=environments&coding-environments=none");
  await expect(page.getByRole("radio")).toHaveCount(5);
  await expect(page.locator('input[type="radio"]:checked')).toHaveCount(0);
  await expect(page.locator('input[type="checkbox"]:checked')).toHaveCount(0);
});


test("macOS Clausewitz readiness requires an SSH alias and disables Live Tools", async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "userAgent", { get: () => "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)" }));
  await page.goto("/?screenshot=live-testing");
  await expect(page.getByRole("radio", { name: /HOI4 Live Tools/ })).toBeDisabled();
  const ready = page.getByRole("checkbox", { name: /I confirm a compatible Linux game/ });
  await ready.focus();
  await page.keyboard.press("Space");
  await expect(ready).toBeChecked();
  await expect(page.getByRole("button", { name: "Next", exact: true })).toBeDisabled();
  await expect(page.getByRole("alert")).toContainText("a ready SSH route requires its alias");
  await page.getByLabel("SSH host alias").fill("user@host");
  await expect(page.getByRole("button", { name: "Next", exact: true })).toBeDisabled();
  await page.getByLabel("SSH host alias").fill("linux-game");
  await expect(page.getByRole("button", { name: "Next", exact: true })).toBeEnabled();
});

test("Windows Clausewitz confirmation is keyboard accessible and can be deferred", async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "userAgent", { get: () => "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" }));
  await page.goto("/?screenshot=live-testing");
  await expect(page.getByRole("radio", { name: /HOI4 Live Tools/ })).toBeEnabled();
  const ready = page.getByRole("checkbox", { name: /I confirm a compatible Linux game/ });
  await expect(ready).not.toBeChecked();
  await expect(page.getByText("Configure later: setup records a to-do and does not register the server.")).toBeVisible();
  await ready.focus();
  await page.keyboard.press("Space");
  await expect(ready).toBeChecked();
  await page.getByLabel("WSL distribution (optional)").fill("Ubuntu");
  await expect(page.getByRole("button", { name: "Next", exact: true })).toBeEnabled();
});
