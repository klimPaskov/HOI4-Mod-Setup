import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// GitHub's REST API is answered locally so the run does not spend the shared
// anonymous rate limit; it resolves to the commit the checked-in candidate
// manifest fixture is pinned to. Source files still come from
// raw.githubusercontent.com and pass the core's SHA-256 check.
const COMMIT = "0b124ad9d3121fda5eb3cc5ff365f74a28db0f8f";
const GOLDEN_MOD = fileURLToPath(new URL("../../crates/hoi4-setup-core/tests/golden/event-chain-mod", import.meta.url));

async function prepare(page: Page): Promise<string[]> {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  const hosts = new Set<string>();
  page.on("request", (request) => hosts.add(new URL(request.url()).host));
  await page.route("https://api.github.com/**", async (route) => {
    const url = route.request().url();
    const body = url.endsWith("/Agentic-HOI4-Modding") ? { default_branch: "main" } : { sha: COMMIT };
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body), headers: { "access-control-allow-origin": "*" } });
  });
  await page.goto("./");
  (page as Page & { hosts?: Set<string> }).hosts = hosts;
  return errors;
}

test("the page loads under its Content Security Policy", async ({ page }) => {
  const errors = await prepare(page);
  await expect(page.getByRole("heading", { name: "Prepare a mod in your browser" })).toBeVisible();
  const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute("content");
  expect(csp).toContain("default-src 'none'");
  expect(errors).toEqual([]);
});

test("a new mod reaches the component choice with the WebAssembly core", async ({ page }) => {
  const errors = await prepare(page);
  await page.getByRole("button", { name: /New mod/ }).click();
  await page.getByLabel("Mod name").fill("Iron Dawn");
  await expect(page.getByLabel("Mod ID")).toHaveValue("iron_dawn");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Choose components" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByLabel("Primary")).not.toHaveValue("");
  expect(errors).toEqual([]);
});

test("a new mod downloads verified files and produces a ZIP", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "The full download runs once; the other engines cover the core above.");
  const errors = await prepare(page);
  await page.getByRole("button", { name: /New mod/ }).click();
  await page.getByLabel("Mod name").fill("Iron Dawn");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Download and review" }).click({ timeout: 60_000 });
  await expect(page.getByRole("heading", { name: "Review the changes" })).toBeVisible({ timeout: 180_000 });
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download ZIP" }).click();
  const file = await (await download).path();
  const zip = readFileSync(file);
  expect(zip.subarray(0, 4).toString("binary")).toBe("PK\u0003\u0004");
  for (const name of ["iron_dawn.mod", "iron_dawn/descriptor.mod", "iron_dawn/thumbnail.png", "iron_dawn/.hoi4-mod-setup/install.lock.json", "iron_dawn/AGENTS.md"]) {
    expect(zip.includes(Buffer.from(name)), name).toBe(true);
  }
  await expect(page.getByRole("heading", { name: "Your ZIP is ready" })).toBeVisible();
  const hosts = (page as Page & { hosts?: Set<string> }).hosts!;
  const external = [...hosts].filter((host) => !host.startsWith("127.0.0.1"));
  expect(new Set(external)).toEqual(new Set(["api.github.com", "raw.githubusercontent.com"]));
  expect(errors).toEqual([]);
});

test("an existing mod folder is scanned in the browser", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "Folder upload with webkitdirectory is a Chromium-only Playwright feature.");
  const errors = await prepare(page);
  await page.locator("input[type=file]").first().setInputFiles(GOLDEN_MOD);
  await expect(page.getByRole("heading", { name: "event-chain-mod" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByText("ecm_crisis").first()).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByLabel("Mod name")).toHaveValue("Event Chain Mod");
  expect(errors).toEqual([]);
});

test("the setup can start from the keyboard and moves focus to each new step", async ({ page, browserName }) => {
  test.skip(browserName === "webkit", "WebKit skips buttons when tabbing unless the platform setting is on.");
  const errors = await prepare(page);
  const newMod = page.getByRole("button", { name: /New mod/ });
  for (let presses = 0; presses < 12 && !(await newMod.evaluate((element) => element === document.activeElement)); presses += 1) {
    await page.keyboard.press("Tab");
  }
  await expect(newMod).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Describe the mod" })).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.type("Iron Dawn");
  await expect(page.getByLabel("Mod name")).toHaveValue("Iron Dawn");
  expect(errors).toEqual([]);
});
