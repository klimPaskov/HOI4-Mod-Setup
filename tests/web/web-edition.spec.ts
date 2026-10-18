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
  await fakeAssistant(page);
  await page.goto("./");
  (page as Page & { hosts?: Set<string> }).hosts = hosts;
  return errors;
}

const CORS = { "access-control-allow-origin": "*", "access-control-allow-headers": "*", "access-control-allow-methods": "POST" };

/** Answer the setup assistant like Anthropic's Messages API: one valid
 * structured analysis for the exact input the core sent, which the
 * WebAssembly validator then checks as it would a real reply. */
async function fakeAssistant(page: Page): Promise<void> {
  await page.route("https://api.anthropic.com/**", async (route) => {
    if (route.request().method() === "OPTIONS") {
      await route.fulfill({ status: 204, headers: CORS });
      return;
    }
    const body = JSON.parse(route.request().postData() ?? "{}") as { messages: Array<{ content: string }> };
    const prompt = body.messages[0].content;
    const inputSha256 = /input_sha256=([0-9a-f]{64})/.exec(prompt)![1];
    const input = JSON.parse(prompt.slice(prompt.indexOf("\ninput=") + "\ninput=".length)) as { mode: string; constraints: { requested_mod_name?: string; component_registry: { component_ids: string[] } } };
    const existing = input.mode === "existing_project_semantics";
    const name = input.constraints.requested_mod_name ?? (existing ? "Event Chain Mod" : "Iron Dawn");
    const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
    const proposal = (key: string, value: unknown) => ({ key, value, confidence: 0.9, reason: "Fits the brief.", evidence_refs: [] });
    const analysis = {
      schema_version: "1.0.0",
      analysis_id: "web-fixture",
      mode: input.mode,
      input_sha256: inputSha256,
      project_summary: existing ? "An event chain mod with a crisis storyline." : "A Balkan alternate-history mod driven by events.",
      proposals: [
        proposal("display_name", name),
        proposal("project_id", id),
        proposal("script_prefix", "ecm"),
        proposal("primary_namespace", "ecm"),
        proposal("project_description", existing ? "A crisis told through linked events." : "Balkan nations rise in 1936."),
        proposal("descriptor_tags", ["Events"]),
        proposal("folder_profile", ["events", "common", "localisation/english"]),
        proposal("agents_profile", "default"),
        proposal("localisation_convention", "english"),
        proposal("documentation_convention", "markdown"),
      ],
      component_recommendations: input.constraints.component_registry.component_ids.includes("workflow.super_events")
        ? [{ component_id: "workflow.super_events", recommendation: "recommended", reason: "The mod tells its story through events." }]
        : [],
      warnings: [],
    };
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      headers: CORS,
      body: JSON.stringify({ type: "message", role: "assistant", stop_reason: "end_turn", content: [{ type: "text", text: JSON.stringify(analysis) }] }),
    });
  });
}

/** Choose the assistant, then describe a new mod and let it fill the rest. */
async function newModDetails(page: Page): Promise<void> {
  await page.getByRole("button", { name: /New mod/ }).click();
  await page.getByLabel("API key").fill("sk-ant-test");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByLabel("Mod name").fill("Iron Dawn");
  await page.getByLabel("What the mod is about").fill("Balkan nations rise in 1936 through events and decisions.");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Review the details" })).toBeVisible({ timeout: 60_000 });
}

test("the page loads under its Content Security Policy", async ({ page }) => {
  const errors = await prepare(page);
  await expect(page.getByRole("heading", { name: "Prepare a mod in your browser" })).toBeVisible();
  const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute("content");
  expect(csp).toContain("default-src 'none'");
  expect(errors).toEqual([]);
});

test("the assistant fills a new mod and it reaches the component choice with the WebAssembly core", async ({ page }) => {
  const errors = await prepare(page);
  await newModDetails(page);
  await expect(page.getByLabel(/Mod ID/)).toHaveValue("iron_dawn");
  await expect(page.getByLabel(/Event namespace/)).toHaveValue("ecm");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Choose components" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByLabel("Primary")).not.toHaveValue("");
  expect(errors).toEqual([]);
});

test("a new mod downloads verified files and produces a ZIP", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "The full download runs once; the other engines cover the core above.");
  const errors = await prepare(page);
  await newModDetails(page);
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Choose components" })).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Continue" }).click();
  // Format 6 sources offer image, portrait, and live-testing integrations.
  await expect(page.getByRole("heading", { name: "Choose integrations" })).toBeVisible();
  await page.getByLabel("Main image provider").selectOption("none");
  await page.getByRole("button", { name: "Download and review" }).click({ timeout: 60_000 });
  await expect(page.getByRole("heading", { name: "Review the changes" })).toBeVisible({ timeout: 180_000 });
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download ZIP" }).click();
  const file = await (await download).path();
  const zip = readFileSync(file);
  expect(zip.subarray(0, 4).toString("binary")).toBe("PK\u0003\u0004");
  for (const name of ["iron_dawn.mod", "iron_dawn/descriptor.mod", "iron_dawn/thumbnail.png", "iron_dawn/.hoi4-mod-setup/install.lock.json", "iron_dawn/.hoi4-mod-setup/state.json", "iron_dawn/AGENTS.md"]) {
    expect(zip.includes(Buffer.from(name)), name).toBe(true);
  }
  await expect(page.getByRole("heading", { name: "Your ZIP is ready" })).toBeVisible();
  // The checks the desktop's Ready screen shows, computed by the core.
  await expect(page.getByRole("heading", { name: "Checks" })).toBeVisible();
  await expect(page.getByText("descriptor.mod names the mod for the launcher.")).toBeVisible();
  const hosts = (page as Page & { hosts?: Set<string> }).hosts!;
  const external = [...hosts].filter((host) => !host.startsWith("127.0.0.1"));
  expect(new Set(external)).toEqual(new Set(["api.github.com", "raw.githubusercontent.com", "api.anthropic.com"]));
  expect(errors).toEqual([]);
});

test("an existing mod folder is scanned in the browser", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "Folder upload with webkitdirectory is a Chromium-only Playwright feature.");
  const errors = await prepare(page);
  await page.locator("input[type=file]").first().setInputFiles(GOLDEN_MOD);
  await expect(page.getByRole("heading", { name: "event-chain-mod" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByText("ecm_crisis").first()).toBeVisible();
  await page.getByLabel("API key").fill("sk-ant-test");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Review the details" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByLabel(/Mod name/)).toHaveValue("Event Chain Mod");
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
  await expect(newMod).toHaveAttribute("aria-pressed", "true");
  await page.getByLabel("API key").fill("sk-ant-test");
  await page.getByRole("button", { name: "Continue" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Describe the mod" })).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.type("Iron Dawn");
  await expect(page.getByLabel("Mod name")).toHaveValue("Iron Dawn");
  expect(errors).toEqual([]);
});
