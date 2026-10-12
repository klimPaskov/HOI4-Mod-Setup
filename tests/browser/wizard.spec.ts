import { test, expect } from "@playwright/test";
import { DOCUMENTATION_SCENARIOS } from "../../src/documentation-fixtures";
import { resolve } from "node:path";

const screenshotNames: Record<string, string> = {
  welcome: "01-welcome.jpg", description: "02-description.png", identity: "03-identity.png",
  components: "04-components.png", workflows: "05-integrations.png", mcp: "06-mcp-credentials.png",
  git: "07-git.png", ready: "08-ready.png", "dry-run": "09-dry-run.png",
  provider: "10-provider-selection.png", existing: "11-existing-project.png", maintenance: "12-maintenance.png",
  "chat-sources": "13-chat-sources.png", recovery: "14-recovery.png", environments: "15-coding-environments.png",
  "image-generation": "16-image-generation.png",
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

test("existing-project management remains available while signed out", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Manage an existing project", exact: true }).click();
  await expect(page.getByLabel("Project folder", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Browse project folder", exact: true })).toBeEnabled();
});

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
