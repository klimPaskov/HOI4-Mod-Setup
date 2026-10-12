import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const args = process.argv.slice(2);
const updateDocumentation = args.includes("--update-documentation");
const result = spawnSync(process.execPath, [require.resolve("@playwright/test/cli"), "test", ...args.filter((arg) => arg !== "--update-documentation")], {
  cwd: new URL("..", import.meta.url),
  stdio: "inherit",
  windowsHide: true,
  env: { ...process.env, ...(updateDocumentation ? { HOI4_UPDATE_SCREENSHOTS: "1" } : {}) },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
